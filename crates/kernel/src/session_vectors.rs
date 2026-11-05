//! Ask v2 session-vector maintenance (`03 §7i.2`, 0.4.5 D11): the `session_embed` job and
//! its producer.
//!
//! `session_vectors` is the vec0 table holding one embedding per session — the same model
//! and dimension as text embeddings — built from the session's concatenated non-NULL
//! `title`/`summary` (`03 §4`, `§7i.2`). The store owns that table and decides which
//! sessions still need a row; this module only drains the backlog in bounded batches.
//!
//! Nothing here runs on the capture hot path: the job is enqueued by the idle producer
//! below (one `frame_id: None` job per tick while work is pending) and on demand from
//! [`crate::ask::Kernel::ask_turn`], and the worker pool claims it only while the embedder
//! is attached and the enrichment throttle is at level 0. A failure degrades to "the session
//! vectors are behind", never to a capture stall.
//!
//! Replay is idempotent through the store's upsert: candidates are the sessions *without* a
//! row, and the write re-checks the session's current title/summary, so a batch that runs
//! twice writes nothing the second time and a concurrent summary edit cannot leave a stale
//! vector behind.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use traits::{EmbeddingProvider, JobKind, NewJob, Store};

use crate::worker_pool::Outcome;

/// Sessions embedded per job (`03 §7i.2`): a bounded batch, so one job can never monopolize
/// a worker or the embedding model.
pub(crate) const BATCH: u32 = 8;
/// How often the idle producer re-checks the backlog. A kernel constant, not a settings key
/// (0.4.5 D6).
pub(crate) const TICK: Duration = Duration::from_secs(30);
/// Attempts a `session_embed` job may take before the queue dead-letters it (`03 §5`).
const MAX_ATTEMPTS: i64 = 3;

/// Runs one bounded `session_embed` batch: read the next sessions without a vector, embed
/// their text, and upsert each row. Exposed to the worker pool's dispatch arm; every store
/// or provider failure is a `Retry` (the queue's backoff and dead-letter bound it), and an
/// empty backlog is a no-op success.
pub(crate) async fn run_session_embed(
    store: &Arc<dyn Store>,
    embedder: &Arc<dyn EmbeddingProvider>,
) -> Outcome {
    let candidates = match store.session_embed_candidates(BATCH).await {
        Ok(candidates) => candidates,
        Err(error) => return Outcome::Retry(format!("read session embed candidates: {error}")),
    };
    if candidates.is_empty() {
        return Outcome::Complete {
            changed_data: false,
        };
    }
    let texts: Vec<String> = candidates.iter().map(|c| c.text.clone()).collect();
    let vectors = match embedder.embed_texts(&texts).await {
        Ok(vectors) => vectors,
        Err(error) => return Outcome::Retry(format!("embed session text: {error}")),
    };
    if vectors.len() != candidates.len() {
        return Outcome::Retry(format!(
            "session embedder returned {} vectors for {} inputs",
            vectors.len(),
            candidates.len()
        ));
    }
    let mut written = 0usize;
    for (candidate, emb) in candidates.iter().zip(vectors.iter()) {
        match store
            .upsert_session_vector(candidate.session_id, &candidate.text, emb)
            .await
        {
            Ok(true) => written += 1,
            // The session was deleted, or its title/summary changed under us: the store
            // skipped the write on purpose and the next batch picks the fresh text up.
            Ok(false) => {}
            Err(error) => {
                return Outcome::Retry(format!(
                    "upsert session vector {}: {error}",
                    candidate.session_id
                ))
            }
        }
    }
    tracing::debug!(
        candidates = candidates.len(),
        written,
        "session embed batch complete"
    );
    Outcome::Complete {
        changed_data: written > 0,
    }
}

/// Handle to the running producer loop; stopping mirrors [`crate::graph_scheduler`]. The
/// `Drop` safety net ends the loop even if its owner returns early.
pub(crate) struct SchedulerHandle {
    stop: watch::Sender<bool>,
    join: Option<JoinHandle<()>>,
}

impl SchedulerHandle {
    pub(crate) async fn stop(mut self) {
        let _ = self.stop.send(true);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

/// Starts the idle/on-demand producer (`03 §7i.2`, D11). The embedder is the only provider
/// it needs, so the kernel calls this from `attach_embedder`, not from inference attach.
pub(crate) fn spawn(store: Arc<dyn Store>, throttle_level: Arc<AtomicU8>) -> SchedulerHandle {
    let (stop_tx, stop_rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let mut stop = stop_rx;
        // Idle by construction: wait a full tick before the first check, so starting the
        // producer never races a just-started app (or a test) with an immediate job.
        loop {
            tokio::select! {
                biased;
                _ = stop.changed() => break,
                _ = tokio::time::sleep(TICK) => {}
            }
            if *stop.borrow() {
                break;
            }
            tick(store.as_ref(), &throttle_level).await;
        }
    });
    SchedulerHandle {
        stop: stop_tx,
        join: Some(join),
    }
}

/// One producer iteration: paused by the enrichment throttle, one queued job at a time,
/// and only while the backlog is non-empty. Infallible by contract — every store error is
/// logged and the next tick retries.
pub(crate) async fn tick(store: &dyn Store, throttle_level: &AtomicU8) {
    let level = throttle_level.load(Ordering::Relaxed);
    if level >= 1 {
        tracing::debug!(
            level,
            "session vectors are behind: enrichment throttle active"
        );
        return;
    }
    if !backlog_ready(store).await {
        return;
    }
    enqueue(store).await;
}

/// On-demand producer used by [`crate::ask::Kernel::ask_turn`]: the same guard as the idle
/// tick, and never a failure of the turn it piggybacks on.
pub(crate) async fn enqueue_on_demand(store: &dyn Store) {
    if !backlog_ready(store).await {
        return;
    }
    enqueue(store).await;
}

/// Whether a bounded backfill job may be enqueued now: work pending and no `session_embed`
/// job already queued (so a second tick cannot stack a redundant run).
async fn backlog_ready(store: &dyn Store) -> bool {
    match store.session_embed_pending().await {
        Ok(0) => return false,
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error = %error, "session embed: pending count read failed");
            return false;
        }
    }
    match store.pending_jobs_of_kind(JobKind::SessionEmbed).await {
        Ok(0) => true,
        Ok(_) => false,
        Err(error) => {
            tracing::warn!(error = %error, "session embed: queued job count read failed");
            false
        }
    }
}

/// Enqueues one bounded `session_embed` job (`frame_id: None`: sessions are not frames).
async fn enqueue(store: &dyn Store) -> bool {
    match store
        .enqueue_job(NewJob {
            kind: JobKind::SessionEmbed,
            frame_id: None,
            priority: 0,
            max_attempts: MAX_ATTEMPTS,
            not_before: 0,
        })
        .await
    {
        Ok(job_id) => {
            tracing::debug!(job_id, "session embed job enqueued");
            true
        }
        Err(error) => {
            tracing::warn!(error = %error, "session embed enqueue failed");
            false
        }
    }
}
