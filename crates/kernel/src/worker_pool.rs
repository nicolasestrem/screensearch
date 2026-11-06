//! The bounded enrichment worker pool — the consumer half of *enrich-deferred*
//! (`03 §5`). Each worker independently `claim_jobs`→runs the provider→
//! `complete_job`/`fail_job`, so the durable queue P2 fills is finally drained into
//! vectors and deferred vision analyses. Provider attachment is slot-driven:
//! embedding jobs are claimed only when an embedder is attached and the matching
//! setting is enabled, while `vision_tag` jobs are claimed once the sidecar-backed
//! vision provider is attached.
//!
//! Shutdown mirrors the capture loop: one `watch` channel stops every worker after
//! its in-flight job. A periodic sweep requeues jobs a dead worker left `running`
//! (`03 §6`, `07` gap #6). There is no durable per-job lease, so the sweep avoids
//! requeueing while this process still has live in-flight jobs; startup recovery
//! still requeues any `running` jobs before workers are spawned.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use traits::{
    AnswerProvider, ChunkSource, EmbeddingProvider, Job, JobCompleted, JobKind, Store,
    VisionProvider,
};

use crate::events::KernelEvent;

/// Shared, runtime-settable embedding provider slot (`None` until the embedder loads
/// off the launch thread). Behind an `Arc<RwLock>` so inference can start the worker
/// pool before embeddings are available, and the pool can pick up embeddings later
/// without a restart.
pub(crate) type EmbedderSlot = Arc<RwLock<Option<Arc<dyn EmbeddingProvider>>>>;

/// Shared, runtime-settable vision provider slot (`None` until the inference sidecar
/// is attached). Behind an `Arc<RwLock>` so the kernel can attach the provider after
/// the pool is already running without restarting it (`03 §6`).
pub(crate) type VisionSlot = Arc<RwLock<Option<Arc<dyn VisionProvider>>>>;

/// Shared, runtime-settable answer provider slot (`None` until inference attaches). The
/// optional work-graph normalization pass reads it live; a normalize job that runs before
/// the sidecar is up degrades to a logged no-op (`03 §7h.2`).
pub(crate) type AnswerSlot = Arc<RwLock<Option<Arc<dyn AnswerProvider>>>>;

/// Idle poll floor — how long a worker waits after an empty claim before retrying.
const IDLE_MIN: Duration = Duration::from_millis(250);
/// Idle poll ceiling — the backoff caps here so an empty queue costs ~one claim/2 s.
const IDLE_MAX: Duration = Duration::from_secs(2);
/// How often the sweep checks for stale `running` jobs.
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// A `running` job older than this is presumed abandoned (worker died) and requeued.
/// Must exceed the longest plausible single-job runtime (embed ≪ 1 s).
const VISIBILITY_TIMEOUT_MS: i64 = 5 * 60 * 1000;
/// Retry backoff base; the nth failure waits `BASE · 2^attempts`, capped.
const BACKOFF_BASE_MS: i64 = 1_000;
const BACKOFF_CAP_MS: i64 = 60_000;

/// Current unix time in milliseconds (the queue's clock unit, `03 §4`).
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Exponential backoff (ms) for the next retry given the job's prior attempt count.
fn backoff_ms(attempts: i64) -> i64 {
    let shift = attempts.clamp(0, 16) as u32;
    BACKOFF_BASE_MS
        .saturating_mul(1_i64 << shift)
        .min(BACKOFF_CAP_MS)
}

/// What to do with a job after running its provider. Shared with the work-graph job
/// modules (`03 §7h.7`) and the insights generator (`03 §7j.3`, which re-exports it), so
/// every producer lane names the same three outcomes the pool maps onto the queue. `Debug`
/// is part of the contract: a caller's assertion on a pass has to be able to print it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Done (or nothing to do) → `complete_job`.
    Complete { changed_data: bool },
    /// Unrecoverable (malformed job, missing file) → dead-letter now, no retry.
    DeadLetter(String),
    /// Recoverable (embed/upsert/read error) → `fail_job` with backoff (the store
    /// dead-letters once attempts are exhausted).
    Retry(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompletedJobInfo {
    job_id: i64,
    kind: JobKind,
    frame_id: i64,
}

/// Everything a worker needs, shared (via `Arc`) across the pool.
pub(crate) struct Shared {
    pub store: Arc<dyn Store>,
    pub embedder: EmbedderSlot,
    /// The vision provider, attached after the sidecar comes up (`None` until then).
    pub vision: VisionSlot,
    pub enable_embed_text: bool,
    /// Job ids currently being processed by this worker pool. The periodic sweep
    /// uses this as a process-local guard so long-running provider calls can still
    /// record their final retry/dead-letter outcome instead of being requeued out
    /// from under themselves.
    pub active_jobs: Arc<Mutex<HashSet<i64>>>,
    /// App-data root; `frames.image_path` is stored relative to it (`frames/…`).
    pub data_dir: PathBuf,
    pub events: broadcast::Sender<KernelEvent>,
    pub concurrency: usize,
    /// Current enrichment-throttle level, shared live with the kernel governor (`03 §5`):
    /// `0` claims every enabled kind; `>= 1` pauses `vision_tag`; `2` also caps concurrent
    /// `embed_text` to `embed_text_floor`. Stays `0` when the throttle is off, so behavior
    /// is unchanged. Read on every claim — no pool restart on a change.
    pub throttle_level: Arc<AtomicU8>,
    /// Minimum concurrent `embed_text` jobs at level 2 (clamped `>= 1`), so text indexing
    /// never fully stalls under sustained pressure. Shared live with the governor, which
    /// rewrites it from settings each tick — so a floor edit hot-applies seamlessly, the
    /// same way the thresholds/dwells do, with no pool restart (`03 §5`).
    pub embed_text_floor: Arc<AtomicUsize>,
    /// Live count of in-flight `embed_text` jobs across the pool — the level-2 floor gate.
    /// Fresh per pool (an in-flight count is meaningless across a restart).
    pub active_embed_text: Arc<AtomicUsize>,
    /// Shared gate for the four work-graph job kinds (`03 §7h.7`): set by the kernel when
    /// the graph scheduler starts, so the graph lane exists only while a producer does. The
    /// kinds are model-free (extraction, linking, clustering, backfill) except the optional
    /// normalization pass, so the lane works with no inference provider attached.
    pub graph_enabled: Arc<AtomicBool>,
    /// Shared ordering gate for the four work-graph job kinds (`03 §7h.6`, `06` #43): `true`
    /// only once the sessions historical pass has reached its target. Pending graph jobs left by
    /// a previous process must not be claimed before that, or the first launch after an upgrade
    /// could extract historical frames before their sessions exist (the P1 review finding).
    /// The producer sets it every tick; jobs already claimed are still bounded by the job body's
    /// own writes and retry safely.
    pub graph_ready: Arc<AtomicBool>,
    /// The answer provider, read live by the optional `graph_normalize` job (`03 §7h.2`).
    pub answer: AnswerSlot,
    /// Kernel-local cluster success marker, written by a successful `graph_cluster` job and
    /// read by the producer loop so a dead-lettered cluster job is retried instead of
    /// suppressing the completion line (`crate::graph_cluster::ClusterProgress`).
    pub cluster_progress: Arc<crate::graph_cluster::ClusterProgress>,
}

/// Handle to the running pool: a stop signal and the spawned task joins.
pub(crate) struct WorkerPool {
    stop: watch::Sender<bool>,
    joins: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    /// Spawns `concurrency` worker tasks plus one stale-job sweep task, all stopped
    /// by a single `watch` channel.
    pub(crate) fn spawn(shared: Shared) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        let shared = Arc::new(shared);
        let mut joins = Vec::with_capacity(shared.concurrency + 1);
        for _ in 0..shared.concurrency.max(1) {
            joins.push(tokio::spawn(worker_loop(shared.clone(), stop_rx.clone())));
        }
        joins.push(tokio::spawn(sweep_loop(shared.clone(), stop_rx.clone())));
        Self {
            stop: stop_tx,
            joins,
        }
    }

    /// Signals every worker to stop and waits for in-flight jobs to finish. Takes the
    /// joins out via `mem::take` (rather than moving out of `self`) because
    /// [`WorkerPool`] implements [`Drop`].
    pub(crate) async fn shutdown(mut self) {
        let _ = self.stop.send(true);
        for join in std::mem::take(&mut self.joins) {
            let _ = join.await;
        }
    }
}

impl Drop for WorkerPool {
    /// Best-effort safety net: if the handle is dropped without a graceful
    /// `shutdown` (an early return or a panic in the owner), still signal stop so the
    /// detached `tokio::spawn` tasks exit their loops promptly instead of draining the
    /// whole queue first. (`drop` can't `.await` the joins, but the signal alone ends
    /// the loops.)
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

struct ActiveJobGuard {
    active_jobs: Arc<Mutex<HashSet<i64>>>,
    id: i64,
}

impl ActiveJobGuard {
    fn new(active_jobs: Arc<Mutex<HashSet<i64>>>, id: i64) -> Self {
        active_jobs
            .lock()
            .expect("active job set lock poisoned")
            .insert(id);
        Self { active_jobs, id }
    }
}

impl Drop for ActiveJobGuard {
    fn drop(&mut self) {
        self.active_jobs
            .lock()
            .expect("active job set lock poisoned")
            .remove(&self.id);
    }
}

fn active_job_count(active_jobs: &Mutex<HashSet<i64>>) -> usize {
    active_jobs
        .lock()
        .expect("active job set lock poisoned")
        .len()
}

/// Tracks an in-flight `embed_text` job in the shared count for the level-2 floor gate.
/// Always held while an `embed_text` job runs (regardless of level) so the count is
/// already accurate the moment the throttle steps up to level 2.
struct EmbedTextGuard {
    count: Arc<AtomicUsize>,
}

impl EmbedTextGuard {
    fn new(count: Arc<AtomicUsize>) -> Self {
        // Relaxed is sufficient: the floor is a soft cap (`worker_loop` reads this count
        // with `Relaxed` and an accepted one-job race), so a stronger ordering here would
        // be a superfluous full fence the reader can't observe anyway.
        count.fetch_add(1, Ordering::Relaxed);
        Self { count }
    }
}

impl Drop for EmbedTextGuard {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// The claimable job kinds, narrowed by both provider readiness (the original slot-gating)
/// and the throttle: at `level >= 1` `vision_tag` is dropped; at level 2 `embed_text` is
/// dropped too when the in-flight count has reached the floor (`allow_embed_text` is
/// precomputed by the caller so the level read + count read are a single snapshot).
///
/// The four work-graph kinds are additive (`03 §7h.7`): claimed only once the graph
/// scheduler has started and paused by the same `level >= 1` throttle that pauses vision.
/// `session_embed` is equally additive (`03 §7i.2`, 0.4.5 D11): producer-driven (enqueued by
/// the Ask v2 pipeline on demand and by its idle scheduler), claimed only while the embedder
/// is attached and paused by the same throttle. `insight_generate` is additive the same way
/// (`03 §7j.3`, 0.4.5 P8/D10) but needs **no** provider at all — a pass is pure store reads
/// and arithmetic — so it stays claimable at level 0 whichever slots are attached.
/// `embed_text` and `vision_tag` keep their exact previous conditions.
fn claim_kinds(shared: &Shared, level: u8, allow_embed_text: bool) -> ([JobKind; 8], usize) {
    let embedder_ready = shared
        .embedder
        .read()
        .expect("embedder slot lock")
        .is_some();
    let vision_ready = shared.vision.read().expect("vision slot lock").is_some();
    let mut kinds = [JobKind::EmbedText; 8];
    let mut len = 0;
    if embedder_ready && shared.enable_embed_text && allow_embed_text {
        kinds[len] = JobKind::EmbedText;
        len += 1;
    }
    // vision_tag (sidecar) is paused once throttling engages (level >= 1).
    if vision_ready && level == 0 {
        kinds[len] = JobKind::VisionTag;
        len += 1;
    }
    // The work graph is producer-driven and degrades to "the graph is behind" (`03 §7h.7`):
    // no claim at throttle level >= 1, no claim until the scheduler starts, and no claim while
    // the sessions historical pass has not reached its target (`06` #43) so a job left by a
    // previous process cannot extract historical frames before their sessions exist.
    if shared.graph_enabled.load(Ordering::Relaxed)
        && shared.graph_ready.load(Ordering::Relaxed)
        && level == 0
    {
        for kind in [
            JobKind::GraphExtract,
            JobKind::GraphNormalize,
            JobKind::GraphCluster,
            JobKind::GraphBackfill,
        ] {
            kinds[len] = kind;
            len += 1;
        }
    }
    // Ask v2 session vectors (`03 §7i.2`, 0.4.5 D11): producer-driven like the graph, so a
    // claim needs a queued job; the embedder is the only provider it requires. Paused by the
    // same throttle that pauses vision, and never enqueued on the capture hot path.
    if embedder_ready && level == 0 {
        kinds[len] = JobKind::SessionEmbed;
        len += 1;
    }
    // Insights (`03 §7j.3`, 0.4.5 P8/D10): producer-driven and model-free — the pass reads the
    // store and computes, so no provider gate applies; the same `level >= 1` throttle pauses
    // it like every other background lane.
    if level == 0 {
        kinds[len] = JobKind::InsightGenerate;
        len += 1;
    }
    (kinds, len)
}

/// One worker: claim a job, process it, emit progress; back off when the queue is
/// empty; stop promptly when signalled.
async fn worker_loop(shared: Arc<Shared>, mut stop: watch::Receiver<bool>) {
    let mut backoff = IDLE_MIN;
    loop {
        if *stop.borrow() {
            break;
        }
        // Snapshot the throttle level + embed_text in-flight count once, so the claim set
        // reflects a single consistent view (the floor is a soft cap; a rare race just
        // lets one extra embed_text run, self-correcting next iteration).
        let level = shared.throttle_level.load(Ordering::Relaxed);
        let allow_embed_text = level < 2
            || shared.active_embed_text.load(Ordering::Relaxed)
                < shared.embed_text_floor.load(Ordering::Relaxed);
        let (kinds_arr, len) = claim_kinds(&shared, level, allow_embed_text);
        let kinds = &kinds_arr[..len];
        let claimed = if kinds.is_empty() {
            None
        } else {
            match shared.store.claim_jobs(kinds, 1, now_ms()).await {
                Ok(mut jobs) => jobs.pop(),
                Err(e) => {
                    tracing::warn!(error = %e, "worker: claim failed");
                    None
                }
            }
        };
        match claimed {
            Some(job) => {
                let _active = ActiveJobGuard::new(shared.active_jobs.clone(), job.id);
                // Track in-flight embed_text for the level-2 concurrency floor.
                let _embed_text = (job.kind == JobKind::EmbedText)
                    .then(|| EmbedTextGuard::new(shared.active_embed_text.clone()));
                // Snapshot providers out of the shared slots before the await (the std
                // RwLock guards must not cross it). Claim kinds are also slot-gated,
                // so missing providers here are defensive against races.
                let embedder = shared.embedder.read().expect("embedder slot lock").clone();
                let vision = shared.vision.read().expect("vision slot lock").clone();
                let answer = shared.answer.read().expect("answer slot lock").clone();
                let completed = match process_job_with_providers(
                    &shared.store,
                    embedder.as_ref(),
                    vision.as_ref(),
                    answer.as_ref(),
                    Some(&shared.cluster_progress),
                    &shared.data_dir,
                    job,
                )
                .await
                {
                    Ok(completed) => completed,
                    Err(e) => {
                        tracing::warn!(error = %e, "worker: finalizing job failed");
                        None
                    }
                };
                emit_progress(&shared, completed).await;
                backoff = IDLE_MIN;
            }
            None => {
                tokio::select! {
                    biased;
                    _ = stop.changed() => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(IDLE_MAX);
            }
        }
    }
}

/// Runs one already-claimed job to a terminal state: executes the provider, then
/// `complete_job` / `fail_job` (with backoff) / dead-letter. Exposed so tests can
/// drive a single job deterministically without the polling loop. Graph jobs that need
/// the optional answer model see no provider through this seam; `graph_extract`,
/// `graph_cluster` and `graph_backfill` are model-free.
pub async fn process_job(
    store: &Arc<dyn Store>,
    embedder: &Arc<dyn EmbeddingProvider>,
    vision: Option<&Arc<dyn VisionProvider>>,
    data_dir: &Path,
    job: Job,
) -> Result<()> {
    process_job_with_providers(store, Some(embedder), vision, None, None, data_dir, job)
        .await
        .map(|_| ())
}

async fn process_job_with_providers(
    store: &Arc<dyn Store>,
    embedder: Option<&Arc<dyn EmbeddingProvider>>,
    vision: Option<&Arc<dyn VisionProvider>>,
    answer: Option<&Arc<dyn AnswerProvider>>,
    cluster_progress: Option<&crate::graph_cluster::ClusterProgress>,
    data_dir: &Path,
    job: Job,
) -> Result<Option<CompletedJobInfo>> {
    let job_id = job.id;
    let kind = job.kind;
    let frame_id = job.frame_id;
    let outcome = match job.kind {
        JobKind::EmbedText => match embedder {
            Some(embedder) => embed_text_outcome(store, embedder, job.frame_id).await,
            None => Outcome::Retry("embedding provider not attached".to_string()),
        },
        JobKind::VisionTag => vision_tag_outcome(store, vision, data_dir, job.frame_id).await,
        JobKind::GraphExtract => crate::graph_extract::run(store, job.frame_id).await,
        JobKind::GraphBackfill => crate::graph_backfill::run(store).await,
        JobKind::GraphCluster => crate::graph_cluster::run(store, now_ms(), cluster_progress).await,
        JobKind::GraphNormalize => crate::graph_normalize::run(store, answer).await,
        // Ask v2 session vectors (`03 §7i.2`, 0.4.5 D11): embedder-only, bounded batch. A
        // job claimed while the slot is empty is defensive (the claim kinds gate on it) and
        // retries rather than failing the batch permanently.
        JobKind::SessionEmbed => match embedder {
            Some(embedder) => crate::session_vectors::run_session_embed(store, embedder).await,
            None => Outcome::Retry("embedding provider not attached".to_string()),
        },
        // Insights (`03 §7j.3`, 0.4.5 P8/D10): model-free, so the arm never looks at a
        // provider slot — the pass derives every row from the store's own history.
        JobKind::InsightGenerate => crate::insights::run_insight_generate(store, now_ms()).await,
    };
    match outcome {
        Outcome::Complete { changed_data } => {
            store.complete_job(job.id).await?;
            Ok(completion_info(job_id, kind, frame_id, changed_data))
        }
        Outcome::DeadLetter(err) => {
            tracing::warn!(job = job.id, error = %err, "job dead-lettered");
            store.fail_job(job.id, &err, None).await?;
            Ok(None)
        }
        Outcome::Retry(err) => {
            let retry_at = now_ms() + backoff_ms(job.attempts);
            tracing::debug!(job = job.id, error = %err, retry_at, "job retry scheduled");
            store.fail_job(job.id, &err, Some(retry_at)).await?;
            Ok(None)
        }
    }
}

fn completion_info(
    job_id: i64,
    kind: JobKind,
    frame_id: Option<i64>,
    changed_data: bool,
) -> Option<CompletedJobInfo> {
    if changed_data {
        frame_id.map(|frame_id| CompletedJobInfo {
            job_id,
            kind,
            frame_id,
        })
    } else {
        None
    }
}

/// Periodic stale-`running` recovery (`03 §6`, `07` gap #6).
async fn sweep_loop(shared: Arc<Shared>, mut stop: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => break,
            _ = tokio::time::sleep(SWEEP_INTERVAL) => {
                let active = active_job_count(&shared.active_jobs);
                if active > 0 {
                    tracing::debug!(active, "sweep: skipped while worker jobs are active");
                    continue;
                }
                match shared.store.reset_stale_running_jobs(VISIBILITY_TIMEOUT_MS).await {
                    Ok(n) if n > 0 => tracing::warn!(requeued = n, "sweep: requeued stale running jobs"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "sweep: failed"),
                }
            }
        }
    }
}

async fn emit_progress(shared: &Shared, completed: Option<CompletedJobInfo>) {
    if let Ok(stats) = shared.store.job_stats().await {
        let _ = shared.events.send(KernelEvent::JobProgress(stats.clone()));
        if let Some(c) = completed {
            let _ = shared.events.send(KernelEvent::JobCompleted(JobCompleted {
                job_id: c.job_id,
                kind: c.kind,
                frame_id: c.frame_id,
                stats,
            }));
        }
    }
}

/// Embed a frame's OCR text into one chunk (`chunk_index = 0`, `03 §5`). A purged
/// frame or empty OCR text is a no-op success — embedding nothing is meaningless,
/// not a failure.
async fn embed_text_outcome(
    store: &Arc<dyn Store>,
    embedder: &Arc<dyn EmbeddingProvider>,
    frame_id: Option<i64>,
) -> Outcome {
    let Some(frame_id) = frame_id else {
        return Outcome::DeadLetter("embed_text job has no frame_id".to_string());
    };
    let input = match store.get_enrichment_input(frame_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return Outcome::Complete {
                changed_data: false,
            }
        } // frame purged between enqueue and run
        Err(e) => return Outcome::Retry(format!("read frame {frame_id}: {e}")),
    };
    let text = input.ocr_text.unwrap_or_default();
    if text.trim().is_empty() {
        return Outcome::Complete {
            changed_data: false,
        }; // no OCR text to embed
    }
    let emb = match embedder.embed_texts(std::slice::from_ref(&text)).await {
        Ok(mut v) => match v.pop() {
            Some(e) => e,
            None => return Outcome::Retry("text embedder returned no vector".to_string()),
        },
        Err(e) => return Outcome::Retry(format!("embed text: {e}")),
    };
    match store
        .upsert_text_embedding(
            frame_id,
            0,
            &text,
            ChunkSource::Ocr,
            &emb,
            embedder.text_model_name(),
        )
        .await
    {
        Ok(()) => Outcome::Complete { changed_data: true },
        Err(e) => Outcome::Retry(format!("upsert text embedding: {e}")),
    }
}

/// Run deferred vision tagging on a frame's stored image (lossless WebP since 0.3.0;
/// legacy frames may still be JPEG) via the sidecar provider (`03 §5`). The image lives
/// at `data_dir / image_path`: a transient IO error (AV, search indexer, or backup
/// briefly holding the file on Windows) retries, a genuinely missing file is
/// dead-lettered (it won't reappear). A purged frame is a no-op success.
/// A sidecar/analyze error is retryable (the sidecar may be restarting).
async fn vision_tag_outcome(
    store: &Arc<dyn Store>,
    vision: Option<&Arc<dyn VisionProvider>>,
    data_dir: &Path,
    frame_id: Option<i64>,
) -> Outcome {
    let Some(frame_id) = frame_id else {
        return Outcome::DeadLetter("vision_tag job has no frame_id".to_string());
    };
    let Some(vision) = vision else {
        // A vision_tag job exists but no provider is attached yet — retry; the
        // backlog drains once the sidecar comes up (`03 §6`).
        return Outcome::Retry("vision provider not attached".to_string());
    };
    let input = match store.get_enrichment_input(frame_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return Outcome::Complete {
                changed_data: false,
            }
        } // frame purged between enqueue and run
        Err(e) => return Outcome::Retry(format!("read frame {frame_id}: {e}")),
    };
    let abs = data_dir.join(&input.image_path);
    let decode_started = std::time::Instant::now();
    let image = match load_rgba(abs.clone()).await {
        Ok(img) => img,
        Err(e) => {
            return if abs.exists() {
                Outcome::Retry(format!("load image {}: {e}", abs.display()))
            } else {
                Outcome::DeadLetter(format!("image missing {}: {e}", abs.display()))
            };
        }
    };
    let decode_ms = decode_started.elapsed().as_millis() as u64;
    // Privacy-safe VLM request provenance (gap #44): frame id + the relative capture
    // path only — never image bytes/data-URL, OCR `content_text`, app/window title, or
    // the model reply (`init_tracing`'s "no screen content at info level" contract).
    // Emitted at `info` so it reaches `screensearch.log` (the default `EnvFilter` is
    // `info`; a `debug!` here would be silently dropped — the original audit gap).
    tracing::info!(
        frame_id,
        image_path = %input.image_path,
        "vision_tag: sending frame image to VLM"
    );
    let analyze_started = std::time::Instant::now();
    let analysis = match vision.analyze(&image).await {
        Ok(a) => a,
        // `{e:#}` prints the whole anyhow chain so the real sidecar cause (e.g.
        // `exceed_context_size_error`) reaches `jobs.last_error` instead of collapsing to the
        // top `.context("vision completion")`.
        Err(e) => return Outcome::Retry(format!("vision analyze frame {frame_id}: {e:#}")),
    };
    let analyze_ms = analyze_started.elapsed().as_millis() as u64;
    // Per-job stored-image decode vs sidecar analyze wall time, plus the decoded source
    // dimensions. Privacy-safe: path + dimensions + durations only — no screen content.
    tracing::info!(
        frame_id,
        decode_ms,
        analyze_ms,
        total_ms = decode_ms + analyze_ms,
        src_width = image.width(),
        src_height = image.height(),
        path = %input.image_path,
        "vision_tag timing"
    );
    match store.insert_vision(frame_id, analysis).await {
        Ok(()) => Outcome::Complete { changed_data: true },
        Err(e) => Outcome::Retry(format!("insert vision {frame_id}: {e}")),
    }
}

/// Decode a stored capture (lossless WebP since 0.3.0; legacy JPEG/PNG frames still
/// decode) from disk into RGBA on the blocking pool (CPU + file IO).
async fn load_rgba(path: PathBuf) -> Result<image::RgbaImage> {
    tokio::task::spawn_blocking(move || -> Result<image::RgbaImage> {
        Ok(image::open(&path)?.to_rgba8())
    })
    .await
    .map_err(|e| anyhow!("image decode task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use traits::{Embedding, NewFrame, NewJob, OcrResult, VisionAnalysis};

    fn test_shared(store: Arc<dyn Store>) -> Shared {
        let (events, _receiver) = broadcast::channel(8);
        Shared {
            store,
            embedder: Arc::new(RwLock::new(None)),
            vision: Arc::new(RwLock::new(None)),
            enable_embed_text: true,
            active_jobs: Arc::new(Mutex::new(HashSet::new())),
            data_dir: PathBuf::new(),
            events,
            concurrency: 1,
            throttle_level: Arc::new(AtomicU8::new(0)),
            embed_text_floor: Arc::new(AtomicUsize::new(1)),
            active_embed_text: Arc::new(AtomicUsize::new(0)),
            graph_enabled: Arc::new(AtomicBool::new(false)),
            graph_ready: Arc::new(AtomicBool::new(true)),
            answer: Arc::new(RwLock::new(None)),
            cluster_progress: Arc::new(crate::graph_cluster::ClusterProgress::new()),
        }
    }

    fn test_store() -> Arc<dyn Store> {
        Arc::new(store::SqliteStore::open_in_memory().expect("in-memory store"))
    }

    struct FixedEmbedder;

    #[async_trait::async_trait]
    impl EmbeddingProvider for FixedEmbedder {
        fn dim(&self) -> usize {
            2
        }

        async fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Embedding>> {
            Ok(inputs
                .iter()
                .map(|_| Embedding(vec![0.0_f32, 0.0]))
                .collect())
        }

        fn text_model_name(&self) -> &str {
            "test-embedder"
        }
    }

    struct UnusedVision;

    #[async_trait::async_trait]
    impl VisionProvider for UnusedVision {
        async fn analyze(&self, _image: &image::RgbaImage) -> Result<VisionAnalysis> {
            anyhow::bail!("the vision provider is not exercised by these tests")
        }
    }

    #[test]
    fn graph_kinds_are_claimed_only_when_enabled_and_not_throttled() {
        let shared = test_shared(test_store());
        // Insights (`03 §7j.3`) need no provider, so they are the one kind claimable on a
        // bare pool — the graph lane is still gated on its producer.
        let (kinds, len) = claim_kinds(&shared, 0, true);
        assert_eq!(&kinds[..len], &[JobKind::InsightGenerate]);

        shared.graph_enabled.store(true, Ordering::Relaxed);
        let (kinds, len) = claim_kinds(&shared, 0, true);
        assert_eq!(
            &kinds[..len],
            &[
                JobKind::GraphExtract,
                JobKind::GraphNormalize,
                JobKind::GraphCluster,
                JobKind::GraphBackfill,
                JobKind::InsightGenerate,
            ],
            "the four graph kinds are claimable with no inference provider attached"
        );

        shared.graph_ready.store(false, Ordering::Relaxed);
        let (kinds, len) = claim_kinds(&shared, 0, true);
        assert_eq!(
            &kinds[..len],
            &[JobKind::InsightGenerate],
            "stale graph jobs wait for the sessions historical pass (06 #43)"
        );
        shared.graph_ready.store(true, Ordering::Relaxed);

        let (_, len) = claim_kinds(&shared, 1, true);
        assert_eq!(len, 0, "the enrichment throttle pauses the graph lane");
    }

    #[test]
    fn the_graph_lane_does_not_change_the_embed_and_vision_claim_rules() {
        let shared = test_shared(test_store());
        *shared.embedder.write().expect("embedder slot lock") = Some(Arc::new(FixedEmbedder));
        *shared.vision.write().expect("vision slot lock") = Some(Arc::new(UnusedVision));
        shared.graph_enabled.store(true, Ordering::Relaxed);

        let (kinds, len) = claim_kinds(&shared, 0, true);
        assert_eq!(
            &kinds[..len],
            &[
                JobKind::EmbedText,
                JobKind::VisionTag,
                JobKind::GraphExtract,
                JobKind::GraphNormalize,
                JobKind::GraphCluster,
                JobKind::GraphBackfill,
                // Ask v2's session vectors need only the embedder (0.4.5 D11).
                JobKind::SessionEmbed,
                // Insights are model-free, so they follow regardless of the slots (P8).
                JobKind::InsightGenerate,
            ]
        );

        // Level 1 drops vision and session vectors but keeps embed_text, exactly as before
        // the graph lane.
        let (kinds, len) = claim_kinds(&shared, 1, true);
        assert_eq!(&kinds[..len], &[JobKind::EmbedText]);
        // Level 2 with the floor reached also drops embed_text.
        let (_, len) = claim_kinds(&shared, 2, false);
        assert_eq!(len, 0);
    }

    #[tokio::test]
    async fn process_job_dispatches_a_graph_extract_job() {
        let store = test_store();
        let frame_id = store
            .insert_frame(NewFrame {
                captured_at: 1_000,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: "frames/1.webp".to_string(),
                content_hash: "hash-1".to_string(),
                app_hint: Some("firefox".to_string()),
                window_title: None,
                browser_url: Some("https://github.com/acme/widgets".to_string()),
                capture_trigger: None,
            })
            .await
            .expect("insert frame");
        store
            .insert_ocr(
                frame_id,
                OcrResult {
                    text: "work graph".to_string(),
                    mean_confidence: 0.9,
                    engine: "test".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .expect("insert ocr");
        store
            .enqueue_job(NewJob {
                kind: JobKind::GraphExtract,
                frame_id: Some(frame_id),
                priority: 0,
                max_attempts: 3,
                not_before: 0,
            })
            .await
            .expect("enqueue graph job");
        let mut jobs = store
            .claim_jobs(&[JobKind::GraphExtract], 1, now_ms())
            .await
            .expect("claim graph job");
        let job = jobs.pop().expect("one claimed graph job");
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(FixedEmbedder);
        process_job(&store, &embedder, None, Path::new("."), job)
            .await
            .expect("process graph job");
        assert_eq!(
            store.job_stats().await.expect("job stats").done,
            1,
            "the graph job completed through the public test seam"
        );
        let catalog = store
            .graph_entity_catalog(10)
            .await
            .expect("entity catalog");
        assert!(
            catalog
                .iter()
                .any(|entity| entity.canonical_name == "github.com"),
            "the graph extract body persisted the frame's domain entity"
        );
    }

    #[test]
    fn active_job_guard_tracks_in_flight_job_until_drop() {
        let active_jobs = Arc::new(Mutex::new(HashSet::new()));
        assert_eq!(active_job_count(&active_jobs), 0);

        {
            let _guard = ActiveJobGuard::new(active_jobs.clone(), 42);
            assert_eq!(active_job_count(&active_jobs), 1);
        }

        assert_eq!(active_job_count(&active_jobs), 0);
    }

    #[test]
    fn completion_info_only_for_changed_frame_jobs() {
        assert_eq!(
            completion_info(1, JobKind::EmbedText, Some(7), true),
            Some(CompletedJobInfo {
                job_id: 1,
                kind: JobKind::EmbedText,
                frame_id: 7,
            })
        );
        assert_eq!(completion_info(1, JobKind::EmbedText, Some(7), false), None);
        assert_eq!(completion_info(1, JobKind::EmbedText, None, true), None);
    }
}
