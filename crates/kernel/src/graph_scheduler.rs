//! Idle/on-demand producer loop for the work graph (`03 §7h.6/.7`, 0.4.5 P5).
//!
//! The scheduler is a producer plus bounded reconciliation work: it never extracts links for
//! a live capture frame, never clusters inline, and never touches the capture path. It
//! enqueues durable graph jobs on a timer, runs a bounded session-ownership reconcile batch
//! once extraction is drained, and every failure is logged and swallowed, so a graph problem
//! can only ever mean "the graph is behind" (`03 §7h.7`).
//!
//! No new settings key exists for it (0.4.5 D6, `03 §8`): batch sizes and intervals are
//! kernel constants, and the optional normalization pass is gated on an attached answer
//! provider plus a platform idle source, never on a toggle.
//!
//! Each tick, in order:
//! 1. drop out at enrichment throttle level >= 1 (debug log "graph is behind", degrade only);
//! 2. wait while the sessions historical backfill has not reached its target (debug log
//!    "graph waits for the sessions history pass"): the graph must not extract frames before
//!    their sessions exist, which is the root cause of the P5 review finding;
//! 3. read the queue-derived backfill status;
//! 4. if extraction is NOT drained, refill the resumable backfill while frames above the
//!    cursor exist and the pending extract count is under the low-water mark;
//! 5. if extraction IS drained, run one bounded session-ownership reconcile step
//!    ([`crate::graph_reconcile::reconcile_tick`]) before the cluster check, then enqueue one
//!    cluster pass when the last successful plan no longer covers the extraction state;
//! 6. enqueue one normalization pass per idle episode when an answer provider is attached.
//!
//! The cluster job owns the completion evidence line (`03 §7h.6`) and records its success in
//! kernel-local memory ([`ClusterProgress`]), so a dead-lettered cluster job leaves the
//! marker stale and the scheduler retries it instead of permanently suppressing the line. A
//! reconcile rebuild that changed links invalidates that marker, so repaired links are
//! re-planned even when extraction itself has not moved.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use traits::{JobKind, NewJob, Store};

use crate::graph_backfill::LOW_WATER;
use crate::graph_cluster::ClusterProgress;
use crate::graph_reconcile::{self, DirtySessions};
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
/// [`ClusterProgress`], not duplicated here; the startup sweep is this process's one-time
/// backfill of the ownership reconcile (`crate::graph_reconcile`).
#[derive(Debug, Default)]
pub(crate) struct TickState {
    /// Whether the current idle episode has already enqueued its normalization pass.
    normalize_ran_this_idle_episode: bool,
    /// One-time startup sweep of unfrozen sessions, advanced one bounded batch per drained
    /// tick once the dirty set is empty.
    startup_sweep: graph_reconcile::StartupSweep,
    /// Worker-pool claim gate for graph jobs (`06` #43): set only while the sessions
    /// historical pass is complete. `None` in unit tests, which exercise the scheduler logic
    /// without a worker pool.
    graph_ready: Option<Arc<AtomicBool>>,
}

impl TickState {
    fn with_graph_ready(graph_ready: Arc<AtomicBool>) -> Self {
        Self {
            graph_ready: Some(graph_ready),
            ..Self::default()
        }
    }

    fn set_graph_ready(&self, ready: bool) {
        if let Some(gate) = &self.graph_ready {
            gate.store(ready, Ordering::Relaxed);
        }
    }
}

/// The two live worker-pool claim gates for graph jobs (`03 §7h.7`, `06` #43): `enabled` when
/// the producer exists, `ready` only after the sessions historical pass is complete.
#[derive(Clone)]
pub(crate) struct GraphClaimGates {
    pub(crate) enabled: Arc<AtomicBool>,
    pub(crate) ready: Arc<AtomicBool>,
}

/// Starts the producer loop and enables graph job claiming in the worker pool
/// (`03 §7h.7`). The kernel calls this when the session engine attaches; it needs no
/// inference lane, because extraction, linking, clustering and backfill are model-free.
pub(crate) fn spawn(
    store: Arc<dyn Store>,
    gates: GraphClaimGates,
    throttle_level: Arc<AtomicU8>,
    idle: IdleSlot,
    answer: AnswerSlot,
    cluster_progress: Arc<ClusterProgress>,
    dirty: DirtySessions,
) -> SchedulerHandle {
    gates.enabled.store(true, Ordering::Relaxed);
    let (stop_tx, stop_rx) = watch::channel(false);
    let join = tokio::spawn(scheduler_loop(
        LoopContext {
            store,
            throttle_level,
            idle,
            answer,
            cluster_progress,
            dirty,
        },
        gates.ready,
        stop_rx,
    ));
    SchedulerHandle {
        stop: stop_tx,
        join: Some(join),
    }
}

/// The live handles one producer loop uses; grouped so the loop signature stays small.
struct LoopContext {
    store: Arc<dyn Store>,
    throttle_level: Arc<AtomicU8>,
    idle: IdleSlot,
    answer: AnswerSlot,
    cluster_progress: Arc<ClusterProgress>,
    dirty: DirtySessions,
}

async fn scheduler_loop(
    context: LoopContext,
    graph_ready: Arc<AtomicBool>,
    mut stop: watch::Receiver<bool>,
) {
    let mut state = TickState::with_graph_ready(graph_ready);
    loop {
        tick(
            context.store.as_ref(),
            &context.throttle_level,
            &context.idle,
            &context.answer,
            &context.cluster_progress,
            &context.dirty,
            &mut state,
        )
        .await;
        if wait_or_stop(&mut stop, TICK).await {
            break;
        }
    }
}

/// One scheduler iteration. Producer plus bounded reconcile work, infallible by contract:
/// every store or enqueue error is logged and skipped (`03 §7h.7` degradation rule).
pub(crate) async fn tick(
    store: &dyn Store,
    throttle_level: &AtomicU8,
    idle: &IdleSlot,
    answer: &AnswerSlot,
    cluster_progress: &ClusterProgress,
    dirty: &DirtySessions,
    state: &mut TickState,
) {
    let level = throttle_level.load(Ordering::Relaxed);
    if level >= 1 {
        tracing::debug!(level, "graph is behind: enrichment throttle active");
        return;
    }
    // The sessions historical pass owns frozen-history rows. Extracting frames before their
    // sessions exist is exactly the P5 review finding, so the graph waits for that marker and
    // keeps the worker pool from claiming graph jobs (including jobs left by a previous
    // process) until then (`06` #43).
    if !crate::sessions_scheduler::history_backfill_complete(store).await {
        state.set_graph_ready(false);
        tracing::debug!("graph waits for the sessions history pass");
        return;
    }
    state.set_graph_ready(true);
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
    if drained {
        // Repair missing or stale session-ownership links before asking for a re-cluster.
        // A changed rebuild invalidates `cluster_progress`, so the check below re-enqueues.
        // The drained snapshot bounds the rebuild: frames captured after this status read have
        // no extraction job yet and must be left to their normal extraction (PR #159 Codex P2).
        graph_reconcile::reconcile_tick(
            store,
            dirty,
            cluster_progress,
            &mut state.startup_sweep,
            status.max_frame_id,
        )
        .await;
        if status.pending_cluster == 0 && cluster_progress.needs_cluster(status.done_extract) {
            // The cluster job logs the completion line after applying the plan; recording
            // its success in `cluster_progress` is what stops this branch from re-enqueueing
            // on a successful pass. A dead-lettered job leaves the marker unchanged, so the
            // next tick retries instead of suppressing the line forever.
            enqueue(store, JobKind::GraphCluster).await;
        }
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
async fn wait_or_stop(stop: &mut watch::Receiver<bool>, duration: Duration) -> bool {
    tokio::select! {
        biased;
        _ = stop.changed() => true,
        _ = tokio::time::sleep(duration) => *stop.borrow(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use std::sync::{Arc, RwLock};

    use store::SqliteStore;
    use traits::{AnswerProvider, JobKind, NewFrame, NewSession, SessionKind, Store};

    use super::*;
    use crate::graph_backfill::{enqueue_next_batch, Batch};
    use crate::graph_normalize::test_support::FakeAnswerProvider;
    use crate::graph_reconcile::new_dirty_sessions;
    use crate::worker_pool::AnswerSlot;

    async fn test_store() -> Arc<dyn Store> {
        Arc::new(SqliteStore::open_in_memory().expect("in-memory store"))
    }

    /// Marks the sessions historical pass complete, which the graph tick requires before it
    /// does any work. The marker is the production `sessions.backfill_done_until` setting.
    async fn complete_sessions_history(store: &dyn Store) {
        store
            .set_setting(
                crate::sessions_scheduler::BACKFILL_MARKER,
                r#"{"cursor_ms":1000,"target_ms":1000}"#,
            )
            .await
            .expect("write sessions history marker");
    }

    fn no_progress() -> Arc<ClusterProgress> {
        Arc::new(ClusterProgress::new())
    }

    fn no_dirty() -> DirtySessions {
        new_dirty_sessions()
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

    async fn insert_frame(store: &dyn Store, captured_at: i64) -> i64 {
        store
            .insert_frame(NewFrame {
                captured_at,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: format!("frames/{captured_at}.webp"),
                content_hash: format!("hash-single-{captured_at}"),
                app_hint: Some("firefox".to_string()),
                window_title: None,
                browser_url: Some(format!("https://site-{captured_at}.test/page")),
                capture_trigger: None,
            })
            .await
            .expect("insert frame")
    }

    async fn insert_session(store: &dyn Store, started_at: i64, ended_at: Option<i64>) -> i64 {
        store
            .insert_session(NewSession {
                started_at,
                ended_at,
                kind: SessionKind::Focus,
                tool: None,
                host: None,
                context_key: format!("ctx:{started_at}"),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .expect("insert session")
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

    async fn pending(store: &dyn Store, kind: KindCounts) -> u64 {
        let status = store.graph_backfill_status().await.expect("status");
        match kind {
            KindCounts::Extract => status.pending_extract,
            KindCounts::Backfill => status.pending_backfill,
            KindCounts::Cluster => status.pending_cluster,
            KindCounts::Normalize => status.pending_normalize,
        }
    }

    enum KindCounts {
        Extract,
        Backfill,
        Cluster,
        Normalize,
    }

    #[tokio::test]
    async fn spawn_enables_graph_job_claiming() {
        let store = test_store().await;
        let enabled = Arc::new(AtomicBool::new(false));
        let handle = spawn(
            store,
            GraphClaimGates {
                enabled: enabled.clone(),
                ready: Arc::new(AtomicBool::new(false)),
            },
            Arc::new(AtomicU8::new(0)),
            no_idle(),
            no_answer(),
            no_progress(),
            no_dirty(),
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
        complete_sessions_history(store.as_ref()).await;
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &AtomicU8::new(1),
            &no_idle(),
            &no_answer(),
            &no_progress(),
            &no_dirty(),
            &mut state,
        )
        .await;
        assert_eq!(pending(store.as_ref(), KindCounts::Extract).await, 0);
        assert_eq!(pending(store.as_ref(), KindCounts::Backfill).await, 0);
        assert_eq!(pending(store.as_ref(), KindCounts::Cluster).await, 0);
        assert_eq!(pending(store.as_ref(), KindCounts::Normalize).await, 0);
    }

    #[tokio::test]
    async fn the_sessions_history_gate_blocks_and_then_allows_graph_work() {
        let store = test_store().await;
        insert_frames(store.as_ref(), 2).await;
        let throttle = AtomicU8::new(0);
        let mut state = TickState::default();
        // Missing marker: the sessions historical pass has not run yet, so the graph must
        // not extract anything (the P5 review finding's root cause).
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &no_progress(),
            &no_dirty(),
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Backfill).await,
            0,
            "a missing sessions-history marker blocks graph work"
        );

        // An unparsable marker is not complete either.
        store
            .set_setting(
                crate::sessions_scheduler::BACKFILL_MARKER,
                "not a checkpoint",
            )
            .await
            .expect("write bad marker");
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &no_progress(),
            &no_dirty(),
            &mut state,
        )
        .await;
        assert_eq!(pending(store.as_ref(), KindCounts::Backfill).await, 0);

        // A completed marker lets the backfill producer run.
        complete_sessions_history(store.as_ref()).await;
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &no_progress(),
            &no_dirty(),
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Backfill).await,
            1,
            "the completed history gate allows the graph backfill"
        );
    }

    #[tokio::test]
    async fn clustering_is_requested_once_extraction_progress_moves() {
        let store = test_store().await;
        insert_frames(store.as_ref(), 2).await;
        complete_sessions_history(store.as_ref()).await;
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
        let dirty = no_dirty();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Cluster).await,
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
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(pending(store.as_ref(), KindCounts::Cluster).await, 1);
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
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Cluster).await,
            0,
            "a successful pass covering this extraction state requests no further cluster pass"
        );
    }

    #[tokio::test]
    async fn a_changed_reconcile_invalidates_cluster_progress_and_requeues_cluster() {
        let store = test_store().await;
        complete_sessions_history(store.as_ref()).await;
        // The session ends before the third frame so that frame is outside the interval
        // fallback and starts with no link at all.
        let session_id = insert_session(store.as_ref(), 0, Some(2_500)).await;
        let frame_a = insert_frame(store.as_ref(), 1_000).await;
        let frame_b = insert_frame(store.as_ref(), 2_000).await;
        let frame_c = insert_frame(store.as_ref(), 3_000).await;
        store
            .assign_frames_session(&[frame_a, frame_b], Some(session_id))
            .await
            .expect("assign initial frames");
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, u64::MAX)
                .await
                .expect("backfill batch"),
            Batch::Enqueued(3)
        );
        drain_extract_jobs(&store).await;

        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let dirty = no_dirty();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        let jobs = claim_all(store.as_ref(), JobKind::GraphCluster).await;
        assert_eq!(jobs.len(), 1);
        assert!(matches!(
            crate::graph_cluster::run(&store, crate::worker_pool::now_ms(), Some(&progress)).await,
            crate::worker_pool::Outcome::Complete { .. }
        ));
        store.complete_job(jobs[0].id).await.expect("complete job");
        assert_eq!(progress.last_clustered_done_extract(), 3);

        // An ownership change: the third frame now belongs to the session, so the reconcile
        // rebuild is a real change and must invalidate the cluster progress.
        store
            .assign_frames_session(&[frame_c], Some(session_id))
            .await
            .expect("assign extra frame");
        crate::graph_reconcile::mark_dirty(&dirty, [session_id]);
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            progress.last_clustered_done_extract(),
            ClusterProgress::NEVER,
            "a changed rebuild invalidates the cluster progress"
        );
        assert_eq!(
            pending(store.as_ref(), KindCounts::Cluster).await,
            1,
            "repaired links request a cluster pass even though extraction did not move"
        );
        assert!(
            dirty.lock().expect("dirty lock").is_empty(),
            "the reconciled session leaves the dirty set"
        );
    }

    #[tokio::test]
    async fn a_noop_reconcile_does_not_requeue_cluster() {
        let store = test_store().await;
        complete_sessions_history(store.as_ref()).await;
        let session_id = insert_session(store.as_ref(), 0, Some(2_500)).await;
        let frame_a = insert_frame(store.as_ref(), 1_000).await;
        let frame_b = insert_frame(store.as_ref(), 2_000).await;
        store
            .assign_frames_session(&[frame_a, frame_b], Some(session_id))
            .await
            .expect("assign frames");
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, u64::MAX)
                .await
                .expect("backfill batch"),
            Batch::Enqueued(2)
        );
        drain_extract_jobs(&store).await;

        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let dirty = no_dirty();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        let jobs = claim_all(store.as_ref(), JobKind::GraphCluster).await;
        assert_eq!(jobs.len(), 1);
        assert!(matches!(
            crate::graph_cluster::run(&store, crate::worker_pool::now_ms(), Some(&progress)).await,
            crate::worker_pool::Outcome::Complete { .. }
        ));
        store.complete_job(jobs[0].id).await.expect("complete job");

        // The same session is marked dirty again without any ownership change: the rebuild
        // is exact, so no cluster pass is requested.
        crate::graph_reconcile::mark_dirty(&dirty, [session_id]);
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Cluster).await,
            0,
            "an unchanged reconcile does not re-enqueue cluster"
        );
    }

    #[tokio::test]
    async fn a_dead_lettered_cluster_job_does_not_suppress_the_completion_line() {
        let store = test_store().await;
        insert_frames(store.as_ref(), 2).await;
        complete_sessions_history(store.as_ref()).await;
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, u64::MAX)
                .await
                .expect("backfill batch"),
            Batch::Enqueued(2)
        );
        drain_extract_jobs(&store).await;

        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let dirty = no_dirty();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
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
        assert_eq!(pending(store.as_ref(), KindCounts::Cluster).await, 0);
        // No successful pass recorded this extraction state, so the next tick retries.
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Cluster).await,
            1,
            "a dead cluster job is retried rather than suppressing the line forever"
        );
    }

    #[tokio::test]
    async fn normalization_is_enqueued_once_per_idle_episode() {
        let store = test_store().await;
        complete_sessions_history(store.as_ref()).await;
        let throttle = AtomicU8::new(0);
        let idle = idle_idle_source(10 * NORMALIZE_IDLE_MS);
        let answer = fake_answer();
        let progress = no_progress();
        let dirty = no_dirty();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &idle,
            &answer,
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Normalize).await,
            1,
            "an attached model plus idle time requests one normalization pass"
        );
        tick(
            store.as_ref(),
            &throttle,
            &idle,
            &answer,
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Normalize).await,
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
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Normalize).await,
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
            &dirty,
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
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Normalize).await,
            1,
            "a new idle episode requests one more pass"
        );
    }

    #[tokio::test]
    async fn normalization_needs_both_a_model_and_an_idle_source() {
        let store = test_store().await;
        complete_sessions_history(store.as_ref()).await;
        let throttle = AtomicU8::new(0);
        let progress = no_progress();
        let dirty = no_dirty();
        let mut state = TickState::default();
        // Model attached, no idle source.
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &fake_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(pending(store.as_ref(), KindCounts::Normalize).await, 0);
        // Idle source present, no model.
        tick(
            store.as_ref(),
            &throttle,
            &idle_idle_source(u64::MAX),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(pending(store.as_ref(), KindCounts::Normalize).await, 0);
    }

    #[tokio::test]
    async fn the_refill_yields_at_the_low_water_constant_and_resumes_below_it() {
        let store = test_store().await;
        complete_sessions_history(store.as_ref()).await;
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
        let dirty = no_dirty();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &no_idle(),
            &no_answer(),
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Backfill).await,
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
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Backfill).await,
            1,
            "below the low-water mark the scheduler refills the backfill"
        );
    }

    #[tokio::test]
    async fn the_normalization_gate_uses_the_idle_dwell_constant() {
        let store = test_store().await;
        complete_sessions_history(store.as_ref()).await;
        let throttle = AtomicU8::new(0);
        let answer = fake_answer();
        let progress = no_progress();
        let dirty = no_dirty();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &throttle,
            &idle_idle_source(NORMALIZE_IDLE_MS - 1),
            &answer,
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Normalize).await,
            0,
            "an idle dwell below the constant is not an idle period"
        );
        tick(
            store.as_ref(),
            &throttle,
            &idle_idle_source(NORMALIZE_IDLE_MS),
            &answer,
            &progress,
            &dirty,
            &mut state,
        )
        .await;
        assert_eq!(
            pending(store.as_ref(), KindCounts::Normalize).await,
            1,
            "the idle constant itself opens the idle period"
        );
    }
}
