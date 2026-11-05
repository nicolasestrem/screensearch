//! Idle/on-demand producer loop for the work graph (`03 §7h.6/.7`, 0.4.5 P5).
//!
//! The scheduler is a producer only: it never extracts, links or clusters inline and never
//! touches the capture path. It enqueues durable graph jobs on a timer, and every failure is
//! logged and swallowed, so a graph problem can only ever mean "the graph is behind"
//! (`03 §7h.7`).
//!
//! No new settings key exists for it (0.4.5 D6, `03 §8`): batch sizes and intervals are
//! kernel constants, and the optional normalization pass is gated on an attached answer
//! provider plus a platform idle source, never on a toggle.
//!
//! Each tick:
//! - drops out at enrichment throttle level >= 1 (debug log "graph is behind", degrade only);
//! - refills the resumable backfill while frames above the queue cursor exist and the
//!   pending extract count is under the low-water mark;
//! - once extraction is drained, enqueues one cluster pass whenever the extraction progress
//!   moved since the last successful pass. Success is recorded by the cluster job itself in
//!   kernel-local memory ([`ClusterProgress`]), so a dead-lettered cluster job leaves the
//!   marker stale and the scheduler retries it instead of permanently suppressing the
//!   completion line. The cluster job also owns that line (`03 §7h.6`); the scheduler never
//!   asks the backfill job to log it;
//! - enqueues one normalization pass per idle episode when an answer provider is attached.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use traits::{JobKind, NewJob, Store};

use crate::graph_backfill::LOW_WATER;
use crate::graph_cluster::ClusterProgress;
use crate::worker_pool::AnswerSlot;
use crate::IdleSource;

/// How often the producer loop re-reads the queue-derived backfill status.
pub(crate) const TICK: Duration = Duration::from_secs(30);
/// Idle dwell that counts as "the machine is idle" for the optional normalization pass.
/// A kernel constant, not a settings key (0.4.5 D6).
pub(crate) const NORMALIZE_IDLE_MS: u64 = 300_000;
/// Attempts a graph job may take before the queue dead-letters it (`03 §5`).
const GRAPH_MAX_ATTEMPTS: i64 = 3;

/// The platform idle source slot, filled when inference attaches and read live per tick.
pub(crate) type IdleSlot = Arc<RwLock<Option<IdleSource>>>;

/// Handle to the running producer task; stop it via [`SchedulerHandle::stop`]. The `Drop`
/// safety net mirrors [`crate::vision_scheduler`], so an early owner return still ends the
/// loop.
pub(crate) struct SchedulerHandle {
    stop: watch::Sender<bool>,
    join: Option<JoinHandle<()>>,
}

impl SchedulerHandle {
    /// Signals the loop to stop and waits for it to finish.
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

/// Kernel-local scheduler memory. Nothing here is persisted: the queue is the only
/// checkpoint (`03 §7h.6`). Cluster success is shared with the job through
/// [`ClusterProgress`], not duplicated here.
#[derive(Debug, Default)]
pub(crate) struct TickState {
    /// Whether the current idle episode has already enqueued its normalization pass.
    normalize_ran_this_idle_episode: bool,
}

/// Starts the producer loop and enables graph job claiming in the worker pool
/// (`03 §7h.7`). The kernel calls this when the session engine attaches; it needs no
/// inference lane, because extraction, linking, clustering and backfill are model-free.
pub(crate) fn spawn(
    store: Arc<dyn Store>,
    graph_enabled: Arc<AtomicBool>,
    throttle_level: Arc<AtomicU8>,
    idle: IdleSlot,
    answer: AnswerSlot,
    cluster_progress: Arc<ClusterProgress>,
) -> SchedulerHandle {
    graph_enabled.store(true, Ordering::Relaxed);
    let (stop_tx, stop_rx) = watch::channel(false);
    let join = tokio::spawn(scheduler_loop(
        store,
        throttle_level,
        idle,
        answer,
        cluster_progress,
        stop_rx,
    ));
    SchedulerHandle {
        stop: stop_tx,
        join: Some(join),
    }
}

async fn scheduler_loop(
    store: Arc<dyn Store>,
    throttle_level: Arc<AtomicU8>,
    idle: IdleSlot,
    answer: AnswerSlot,
    cluster_progress: Arc<ClusterProgress>,
    mut stop: watch::Receiver<bool>,
) {
    let mut state = TickState::default();
    loop {
        tick(
            store.as_ref(),
            &throttle_level,
            &idle,
            &answer,
            &cluster_progress,
            &mut state,
        )
        .await;
        if wait_or_stop(&mut stop, TICK).await {
            break;
        }
    }
}

/// One scheduler iteration. Producer-only and infallible by contract: every store or enqueue
/// error is logged and skipped (`03 §7h.7` degradation rule).
pub(crate) async fn tick(
    store: &dyn Store,
    throttle_level: &AtomicU8,
    idle: &IdleSlot,
    answer: &AnswerSlot,
    cluster_progress: &ClusterProgress,
    state: &mut TickState,
) {
    let level = throttle_level.load(Ordering::Relaxed);
    if level >= 1 {
        tracing::debug!(level, "graph is behind: enrichment throttle active");
        return;
    }
    let status = match store.graph_backfill_status().await {
        Ok(status) => status,
        Err(error) => {
            tracing::warn!(error = %error, "graph scheduler: backfill status read failed");
            return;
        }
    };
    let frames_remain = match status.max_frame_id {
        Some(max_frame_id) => status.cursor.is_none_or(|cursor| cursor < max_frame_id),
        None => false,
    };
    let drained = !frames_remain && status.pending_extract == 0;
    if frames_remain && status.pending_extract < LOW_WATER && status.pending_backfill == 0 {
        enqueue(store, JobKind::GraphBackfill).await;
    }
    if drained
        && status.pending_cluster == 0
        && status.done_extract != cluster_progress.last_clustered_done_extract()
    {
        // The cluster job logs the completion line after applying the plan; recording its
        // success in `cluster_progress` is what stops this branch from re-enqueueing on a
        // successful pass. A dead-lettered job leaves the marker unchanged, so the next tick
        // retries instead of suppressing the line forever.
        enqueue(store, JobKind::GraphCluster).await;
    }
    // The optional normalization pass: one per idle episode, only with a model attached.
    let idle_ms = {
        let slot = idle.read().expect("graph idle slot lock");
        slot.as_ref().and_then(|source| source())
    };
    let machine_idle = idle_ms.is_some_and(|ms| ms >= NORMALIZE_IDLE_MS);
    if !machine_idle {
        state.normalize_ran_this_idle_episode = false;
        return;
    }
    let answer_attached = answer.read().expect("answer slot lock").is_some();
    if answer_attached
        && status.pending_normalize == 0
        && !state.normalize_ran_this_idle_episode
        && enqueue(store, JobKind::GraphNormalize).await
    {
        state.normalize_ran_this_idle_episode = true;
    }
}

/// Enqueues one graph job. Returns `true` on success; an enqueue failure is logged and the
/// loop simply retries on its next tick.
async fn enqueue(store: &dyn Store, kind: JobKind) -> bool {
    match store
        .enqueue_job(NewJob {
            kind,
            frame_id: None,
            priority: 0,
            max_attempts: GRAPH_MAX_ATTEMPTS,
            not_before: 0,
        })
        .await
    {
        Ok(job_id) => {
            tracing::debug!(job_id, kind = ?kind, "graph scheduler enqueued a graph job");
            true
        }
        Err(error) => {
            tracing::warn!(kind = ?kind, error = %error, "graph scheduler enqueue failed");
            false
        }
    }
}

/// Sleeps `dur`, returning early with `true` if a stop was signalled.
async fn wait_or_stop(stop: &mut watch::Receiver<bool>, dur: Duration) -> bool {
    tokio::select! {
        biased;
        _ = stop.changed() => true,
        _ = tokio::time::sleep(dur) => *stop.borrow(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use std::sync::{Arc, RwLock};

    use store::SqliteStore;
    use traits::{AnswerProvider, JobKind, NewFrame, Store};

    use super::*;
    use crate::graph_backfill::{enqueue_next_batch, Batch};
    use crate::graph_normalize::test_support::FakeAnswerProvider;
    use crate::worker_pool::AnswerSlot;

    async fn test_store() -> Arc<dyn Store> {
        Arc::new(SqliteStore::open_in_memory().expect("in-memory store"))
    }

    fn no_progress() -> Arc<ClusterProgress> {
        Arc::new(ClusterProgress::new())
    }

    fn no_idle() -> IdleSlot {
        Arc::new(RwLock::new(None))
    }

    fn idle_idle_source(ms: u64) -> IdleSlot {
        let source: IdleSource = Arc::new(move || Some(ms));
        Arc::new(RwLock::new(Some(source)))
    }

    fn no_answer() -> AnswerSlot {
        Arc::new(RwLock::new(None))
    }

    fn fake_answer() -> AnswerSlot {
        let provider: Arc<dyn AnswerProvider> =
            Arc::new(FakeAnswerProvider::replying(r#"{"proposals":[]}"#));
        Arc::new(RwLock::new(Some(provider)))
    }

    async fn insert_frames(store: &dyn Store, count: usize) -> Vec<i64> {
        let mut ids = Vec::with_capacity(count);
        for index in 0..count {
            let captured_at = (index as i64 + 1) * 1_000;
            ids.push(
                store
                    .insert_frame(NewFrame {
                        captured_at,
                        monitor_index: 0,
                        width: 1920,
                        height: 1080,
                        image_path: format!("frames/{captured_at}.webp"),
                        content_hash: format!("hash-{index}"),
                        app_hint: Some("firefox".to_string()),
                        window_title: None,
                        browser_url: Some(format!("https://site-{index}.test/page")),
                        capture_trigger: None,
                    })
                    .await
                    .expect("insert frame"),
            );
        }
        ids
    }

    /// Drains every pending extract job through the production body and completion calls.
    async fn drain_extract_jobs(store: &Arc<dyn Store>) {
        loop {
            let jobs = store
                .claim_jobs(&[JobKind::GraphExtract], 16, crate::worker_pool::now_ms())
                .await
                .expect("claim extract jobs");
            if jobs.is_empty() {
                return;
            }
            for job in jobs {
                let outcome = crate::graph_extract::run(store, job.frame_id).await;
                assert!(matches!(
                    outcome,
                    crate::worker_pool::Outcome::Complete { .. }
                ));
                store.complete_job(job.id).await.expect("complete job");
            }
        }
    }

    async fn claim_all(store: &dyn Store, kind: JobKind) -> Vec<traits::Job> {
        store
            .claim_jobs(&[kind], 16, crate::worker_pool::now_ms())
            .await
            .expect("claim")
    }

    async fn complete_all(store: &dyn Store, kind: JobKind) {
        for job in claim_all(store, kind).await {
            store.complete_job(job.id).await.expect("complete job");
        }
    }

    #[tokio::test]
    async fn spawn_enables_graph_job_claiming() {
        let store = test_store().await;
        let enabled = Arc::new(AtomicBool::new(false));
        let handle = spawn(
            store,
            enabled.clone(),
            Arc::new(AtomicU8::new(0)),
            no_idle(),
            no_answer(),
            no_progress(),
        );
        assert!(
            enabled.load(Ordering::Relaxed),
            "starting the scheduler enables the graph claim gate"
        );
        handle.stop().await;
    }

    #[tokio::test]
    async fn a_throttled_tick_enqueues_nothing() {
        let store = test_store().await;
        insert_frames(store.as_ref(), 2).await;
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &AtomicU8::new(1),
            &no_idle(),
            &no_answer(),
            &no_progress(),
            &mut state,
        )
        .await;
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(status.pending_extract, 0);
        assert_eq!(status.pending_backfill, 0);
        assert_eq!(status.pending_cluster, 0);
        assert_eq!(status.pending_normalize, 0);
    }

    #[tokio::test]
    async fn clustering_is_requested_once_extraction_progress_moves() {
        let store = test_store().await;
        insert_frames(store.as_ref(), 2).await;
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, u64::MAX)
                .await
                .expect("backfill batch"),
            Batch::Enqueued(2)
        );
        drain_extract_jobs(&store).await;
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(status.pending_extract, 0);
        assert_eq!(status.done_extract, 2);

        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_cluster,
            1,
            "a moved extraction count requests one cluster pass"
        );
        // No duplicate while the cluster job is still pending / the count has not moved.
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_cluster,
            1
        );
        // A successful pass records the extraction state it covered.
        let jobs = claim_all(store.as_ref(), JobKind::GraphCluster).await;
        assert_eq!(jobs.len(), 1);
        let outcome =
            crate::graph_cluster::run(&store, crate::worker_pool::now_ms(), Some(&progress)).await;
        assert!(matches!(
            outcome,
            crate::worker_pool::Outcome::Complete { .. }
        ));
        store.complete_job(jobs[0].id).await.expect("complete job");
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_cluster,
            0,
            "a successful pass covering this extraction state requests no further cluster pass"
        );
    }

    #[tokio::test]
    async fn a_dead_lettered_cluster_job_does_not_suppress_the_completion_line() {
        let store = test_store().await;
        insert_frames(store.as_ref(), 2).await;
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, u64::MAX)
                .await
                .expect("backfill batch"),
            Batch::Enqueued(2)
        );
        drain_extract_jobs(&store).await;

        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        let jobs = claim_all(store.as_ref(), JobKind::GraphCluster).await;
        assert_eq!(jobs.len(), 1);
        // An unrecoverable plan failure dead-letters the job (no retry time requested).
        store
            .fail_job(jobs[0].id, "simulated plan failure", None)
            .await
            .expect("dead-letter the cluster job");
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_cluster,
            0
        );
        // No successful pass recorded this extraction state, so the next tick retries.
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_cluster,
            1,
            "a dead cluster job is retried rather than suppressing the line forever"
        );
    }

    #[tokio::test]
    async fn normalization_is_enqueued_once_per_idle_episode() {
        let store = test_store().await;
        let throttle = AtomicU8::new(0);
        let idle = idle_idle_source(10 * NORMALIZE_IDLE_MS);
        let answer = fake_answer();
        let progress = no_progress();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &idle,
            &answer,
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            1,
            "an attached model plus idle time requests one normalization pass"
        );
        tick(
            store.as_ref(),
            &throttle,
            &idle,
            &answer,
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            1,
            "the same idle episode does not request a second pass"
        );
        complete_all(store.as_ref(), JobKind::GraphNormalize).await;
        tick(
            store.as_ref(),
            &throttle,
            &idle,
            &answer,
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            0,
            "a completed pass is not re-requested within the same idle episode"
        );

        // The user comes back: the next idle episode may run one pass again.
        let not_idle: IdleSource = Arc::new(|| Some(0_u64));
        *idle.write().expect("idle slot lock") = Some(not_idle);
        tick(
            store.as_ref(),
            &throttle,
            &idle,
            &answer,
            &progress,
            &mut state,
        )
        .await;
        let idle_again: IdleSource = Arc::new(|| Some(u64::MAX));
        *idle.write().expect("idle slot lock") = Some(idle_again);
        tick(
            store.as_ref(),
            &throttle,
            &idle,
            &answer,
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            1,
            "a new idle episode requests one more pass"
        );
    }

    #[tokio::test]
    async fn normalization_needs_both_a_model_and_an_idle_source() {
        let store = test_store().await;
        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let mut state = TickState::default();
        // Model attached, no idle source.
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &fake_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            0
        );
        // Idle source present, no model.
        tick(
            store.as_ref(),
            &throttle,
            &idle_idle_source(u64::MAX),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            0
        );
    }

    #[tokio::test]
    async fn the_refill_yields_at_the_low_water_constant_and_resumes_below_it() {
        let store = test_store().await;
        let frames = insert_frames(store.as_ref(), LOW_WATER as usize + 2).await;
        assert!(frames.len() > LOW_WATER as usize);
        // Seed exactly the low-water mark of pending extract jobs, with the cursor still
        // below the last frame, so the pass has work above it but must not refill yet.
        for frame_id in frames.iter().take(LOW_WATER as usize) {
            store
                .enqueue_job(NewJob {
                    kind: JobKind::GraphExtract,
                    frame_id: Some(*frame_id),
                    priority: 0,
                    max_attempts: GRAPH_MAX_ATTEMPTS,
                    not_before: 0,
                })
                .await
                .expect("seed extract job");
        }
        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_backfill,
            0,
            "at the low-water mark the scheduler does not refill"
        );
        // Drain one job: the pending count is now below the constant, so a refill is due.
        let jobs = claim_all(store.as_ref(), JobKind::GraphExtract).await;
        let job = jobs.into_iter().next().expect("one pending extract job");
        let outcome = crate::graph_extract::run(&store, job.frame_id).await;
        assert!(matches!(
            outcome,
            crate::worker_pool::Outcome::Complete { .. }
        ));
        store.complete_job(job.id).await.expect("complete job");
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_backfill,
            1,
            "below the low-water mark the scheduler refills the backfill"
        );
    }

    #[tokio::test]
    async fn the_normalization_gate_uses_the_idle_dwell_constant() {
        let store = test_store().await;
        let throttle = AtomicU8::new(0);
        let answer = fake_answer();
        let progress = no_progress();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &idle_idle_source(NORMALIZE_IDLE_MS - 1),
            &answer,
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            0,
            "an idle dwell below the constant is not an idle period"
        );
        tick(
            store.as_ref(),
            &throttle,
            &idle_idle_source(NORMALIZE_IDLE_MS),
            &answer,
            &progress,
            &mut state,
        )
        .await;
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_normalize,
            1,
            "the idle constant itself opens the idle period"
        );
    }
}
