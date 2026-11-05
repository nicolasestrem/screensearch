//! The `graph_backfill` job body: the resumable historical pass (`03 §7h.6`, 0.4.5 P5).
//!
//! The queue is the checkpoint (`03 §7h.6`, 0.4.5 D2 keeps the schema fixed and D6 keeps the
//! settings surface closed): a `graph_extract` job carries the real `frames.id`, and the
//! cursor is `MAX(jobs.frame_id)` over that kind. One backfill job enqueues a bounded batch
//! of the frames above the cursor; the scheduler refills the next batch when the pending
//! extract count falls below a low-water mark. When no frame remains above the cursor the
//! pass is complete, and the job simply completes: it never logs the completion evidence
//! line. That line belongs to the `graph_cluster` job body, which logs it after applying the
//! thread plan over a drained graph (`crate::graph_cluster`), so the line's
//! `threads`/`thread_sessions` counts are necessarily post-cluster. No additional state is
//! stored.
//!
//! Interrupting the app leaves pending and failed rows intact; a re-run continues from them,
//! and every write is an upsert against the table `UNIQUE` constraints, so a replayed chunk
//! cannot duplicate a row.
//!
//! [`run_to_completion`] drives the identical pieces synchronously (no worker pool) so the
//! acceptance evidence can populate a fixture database and print the five counts twice. The
//! production job body and the driver share [`enqueue_next_batch`],
//! [`crate::graph_extract::extract_frame`] and the cluster pass; there is no second
//! extraction path. The driver logs exactly one line after its own cluster pass, using the
//! same helper and literal as the production cluster job.

use std::sync::Arc;

use traits::{GraphBackfillStatus, GraphTableCounts, JobKind, NewJob, Store};

use crate::worker_pool::Outcome;

/// Frames one `graph_backfill` job enqueues per pass. A kernel constant, never a setting
/// (0.4.5 D6, `03 §8`).
pub const BATCH: u32 = 200;
/// Pending `graph_extract` count below which the scheduler refills the next batch. A kernel
/// constant, never a setting (0.4.5 D6, `03 §8`).
pub const LOW_WATER: u64 = 32;
/// Extract jobs one synchronous drain claims at a time (`run_to_completion` only).
const DRAIN_CLAIM_BATCH: u32 = 64;
/// Attempts a graph job may take before the queue dead-letters it (`03 §5`).
const GRAPH_MAX_ATTEMPTS: i64 = 3;

/// Runs one claimed `graph_backfill` job (`03 §7h.6`). Failures are retryable with the
/// queue's normal backoff; the job itself never blocks capture and never logs the
/// completion evidence line.
pub(crate) async fn run(store: &Arc<dyn Store>) -> Outcome {
    match enqueue_next_batch(store.as_ref(), BATCH, LOW_WATER).await {
        Ok(_) => Outcome::Complete {
            changed_data: false,
        },
        Err(error) => Outcome::Retry(format!("graph backfill: {error}")),
    }
}

/// The result of one bounded backfill step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Batch {
    /// This many `graph_extract` jobs were enqueued by this step.
    Enqueued(u32),
    /// No frame remains above the cursor: the pass is complete.
    Complete,
}

/// Enqueues the next bounded batch of still-unqueued frames. The cursor comes from the queue
/// itself, so there is no separate checkpoint to corrupt (`03 §7h.6`). A drained state is a
/// plain completion here: the backfill job never emits the completion evidence line.
pub(crate) async fn enqueue_next_batch(
    store: &dyn Store,
    batch_size: u32,
    low_water: u64,
) -> anyhow::Result<Batch> {
    let status = store.graph_backfill_status().await?;
    if !has_frames_above_cursor(&status) {
        tracing::debug!(
            cursor = status.cursor,
            max_frame_id = status.max_frame_id,
            "graph backfill: nothing above the cursor"
        );
        return Ok(Batch::Complete);
    }
    if status.pending_extract >= low_water {
        tracing::debug!(
            pending_extract = status.pending_extract,
            low_water,
            "graph backfill: yielded to the pending extract backlog"
        );
        return Ok(Batch::Enqueued(0));
    }
    let frame_ids = store
        .graph_frame_ids_after(status.cursor, batch_size)
        .await?;
    if frame_ids.is_empty() {
        // The completion target can be stale when frames were purged between the status
        // read and this query: treat it as complete rather than spinning on an empty batch.
        tracing::debug!(
            cursor = status.cursor,
            max_frame_id = status.max_frame_id,
            "graph backfill: no frame row above the cursor"
        );
        return Ok(Batch::Complete);
    }
    let mut enqueued = 0u32;
    for frame_id in frame_ids {
        store
            .enqueue_job(NewJob {
                kind: JobKind::GraphExtract,
                frame_id: Some(frame_id),
                priority: 0,
                max_attempts: GRAPH_MAX_ATTEMPTS,
                not_before: 0,
            })
            .await?;
        enqueued += 1;
    }
    tracing::debug!(
        enqueued,
        cursor = status.cursor,
        "graph backfill: enqueued graph_extract jobs"
    );
    Ok(Batch::Enqueued(enqueued))
}

/// `true` while a frame id above the queue-derived cursor still exists.
fn has_frames_above_cursor(status: &GraphBackfillStatus) -> bool {
    match status.max_frame_id {
        Some(max_frame_id) => status.cursor.is_none_or(|cursor| cursor < max_frame_id),
        None => false,
    }
}

/// `true` when the graph is drained (`03 §7h.6`): the queue cursor reached
/// `MAX(frames.id)` and no `graph_extract` job is pending or running. Both the scheduler and
/// the cluster job's completion-line gate use this one predicate.
pub(crate) fn is_drained(status: &GraphBackfillStatus) -> bool {
    !has_frames_above_cursor(status) && status.pending_extract == 0
}

/// Runs the production graph pipeline to completion over `store`, for the acceptance
/// evidence (a fixture database) and for tests, without the kernel worker pool.
///
/// The driver reuses the production producer ([`enqueue_next_batch`]), the production
/// extraction body ([`crate::graph_extract::extract_frame`]) behind the queue's real
/// `claim_jobs` / `complete_job` cycle, and the production clustering pass
/// ([`crate::graph_cluster::run_pass`]). On start it requeues stale `running` jobs, the same
/// startup sweep the worker pool performs, so an interrupted earlier run resumes. After the
/// cluster pass it logs exactly one completion line with the post-cluster counts and returns
/// those counts. A second call over the same database enqueues nothing, re-runs only the
/// idempotent cluster pass and logs the same line with identical values.
pub async fn run_to_completion(store: &Arc<dyn Store>) -> anyhow::Result<GraphTableCounts> {
    let store: &dyn Store = store.as_ref();
    // With no worker pool live, any `running` row is stale (`03 §6`): requeue it so an
    // interrupted earlier run resumes from the queue instead of stalling.
    store.reset_stale_running_jobs(0).await?;
    loop {
        let status = store.graph_backfill_status().await?;
        if is_drained(&status) {
            break;
        }
        let mut enqueued = 0u32;
        if has_frames_above_cursor(&status) && status.pending_extract < LOW_WATER {
            if let Batch::Enqueued(count) = enqueue_next_batch(store, BATCH, LOW_WATER).await? {
                enqueued = count;
            }
        }
        let processed = drain_extract_jobs(store).await?;
        if processed == 0 && enqueued == 0 {
            anyhow::bail!(
                "graph backfill stalled with {} pending graph_extract jobs",
                status.pending_extract
            );
        }
    }
    crate::graph_cluster::run_pass(store, crate::worker_pool::now_ms()).await?;
    crate::graph_cluster::log_completion(store).await
}

/// Claims and processes the pending `graph_extract` jobs until the claim comes back empty.
/// This is the same body the worker pool dispatches, run inline.
async fn drain_extract_jobs(store: &dyn Store) -> anyhow::Result<usize> {
    let mut processed = 0usize;
    loop {
        let jobs = store
            .claim_jobs(
                &[JobKind::GraphExtract],
                DRAIN_CLAIM_BATCH,
                crate::worker_pool::now_ms(),
            )
            .await?;
        if jobs.is_empty() {
            return Ok(processed);
        }
        for job in jobs {
            let Some(frame_id) = job.frame_id else {
                store
                    .fail_job(job.id, "graph_extract job has no frame_id", None)
                    .await?;
                processed += 1;
                continue;
            };
            crate::graph_extract::extract_frame(store, frame_id).await?;
            store.complete_job(job.id).await?;
            processed += 1;
        }
    }
}

/// Test-only tracing capture for the completion evidence line. The line is emitted through
/// `tracing`, so a test-owned subscriber is the honest way to assert its exact text and
/// ordering without the app's log sink.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use crate::graph_cluster::COMPLETION_MARKER;

    /// Records every `graph backfill complete ...` event emitted inside the capture scope.
    #[derive(Clone)]
    pub(crate) struct CompletionCapture {
        lines: Arc<Mutex<Vec<String>>>,
        next_span: Arc<AtomicU64>,
    }

    impl CompletionCapture {
        pub(crate) fn new() -> Self {
            Self {
                lines: Arc::new(Mutex::new(Vec::new())),
                next_span: Arc::new(AtomicU64::new(1)),
            }
        }

        pub(crate) fn lines(&self) -> Vec<String> {
            self.lines.lock().expect("completion capture lock").clone()
        }
    }

    /// A fresh capture handle. Use [`capture_scope`] to install it as the thread-local
    /// tracing subscriber for a block that may span awaits.
    pub(crate) fn new_capture() -> Arc<CompletionCapture> {
        Arc::new(CompletionCapture::new())
    }

    /// Installs `capture` as the thread-local subscriber until the returned guard drops.
    /// Kernel tests run on the current-thread runtime, so the guard can span awaits.
    pub(crate) fn capture_scope(
        capture: &Arc<CompletionCapture>,
    ) -> tracing::subscriber::DefaultGuard {
        tracing::subscriber::set_default(capture.as_ref().clone())
    }

    struct MessageVisitor {
        message: Option<String>,
    }

    impl Visit for MessageVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.message = Some(format!("{value:?}").trim_matches('"').to_string());
            }
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "message" {
                self.message = Some(value.to_string());
            }
        }
    }

    impl Subscriber for CompletionCapture {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            Id::from_u64(self.next_span.fetch_add(1, Ordering::Relaxed))
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut visitor = MessageVisitor { message: None };
            event.record(&mut visitor);
            if let Some(message) = visitor.message {
                if message.starts_with(COMPLETION_MARKER) {
                    self.lines
                        .lock()
                        .expect("completion capture lock")
                        .push(message);
                }
            }
        }

        fn enter(&self, _span: &Id) {}

        fn exit(&self, _span: &Id) {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use store::SqliteStore;
    use traits::{
        GraphTableCounts, JobKind, NewFrame, NewJob, NewSession, OcrResult, SessionKind, Store,
    };

    use super::test_support::{capture_scope, new_capture};
    use super::*;
    use crate::worker_pool::Outcome;

    async fn test_store() -> Arc<dyn Store> {
        Arc::new(SqliteStore::open_in_memory().expect("in-memory store"))
    }

    async fn insert_frame(store: &dyn Store, captured_at: i64, browser_url: &str) -> i64 {
        let frame_id = store
            .insert_frame(NewFrame {
                captured_at,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: format!("frames/{captured_at}.webp"),
                content_hash: format!("hash-{captured_at}"),
                app_hint: Some("firefox".to_string()),
                window_title: None,
                browser_url: Some(browser_url.to_string()),
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
        frame_id
    }

    async fn insert_frames(store: &dyn Store, count: usize) -> Vec<i64> {
        let mut ids = Vec::with_capacity(count);
        for index in 0..count {
            let captured_at = (index as i64 + 1) * 1_000;
            ids.push(
                insert_frame(
                    store,
                    captured_at,
                    &format!("https://site-{index}.test/page"),
                )
                .await,
            );
        }
        ids
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

    /// Claim the pending extract jobs and run the production job body on each.
    async fn drain_with_job_body(store: &Arc<dyn Store>) {
        loop {
            let jobs = store
                .claim_jobs(&[JobKind::GraphExtract], 16, crate::worker_pool::now_ms())
                .await
                .expect("claim extract jobs");
            if jobs.is_empty() {
                return;
            }
            for job in jobs {
                assert!(matches!(
                    crate::graph_extract::run(store, job.frame_id).await,
                    Outcome::Complete { .. }
                ));
                store.complete_job(job.id).await.expect("complete job");
            }
        }
    }

    async fn pending_extract(store: &dyn Store) -> u64 {
        store
            .graph_backfill_status()
            .await
            .expect("backfill status")
            .pending_extract
    }

    #[tokio::test]
    async fn an_interrupted_batch_keeps_its_pending_jobs_and_resumes_from_the_cursor() {
        let store = test_store().await;
        let frames = insert_frames(store.as_ref(), 5).await;

        // A large low-water mark disables the yield so each call advances exactly one batch.
        let first = enqueue_next_batch(store.as_ref(), 2, u64::MAX)
            .await
            .expect("first batch");
        assert_eq!(first, Batch::Enqueued(2));
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(status.cursor, Some(frames[1]));
        assert_eq!(status.pending_extract, 2);

        // "Interrupt" here: the pending rows are untouched. The next pass continues after
        // the cursor instead of restarting, so no frame is enqueued twice.
        let second = enqueue_next_batch(store.as_ref(), 2, u64::MAX)
            .await
            .expect("second batch");
        assert_eq!(second, Batch::Enqueued(2));
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(status.cursor, Some(frames[3]));
        assert_eq!(status.pending_extract, 4);

        let third = enqueue_next_batch(store.as_ref(), 2, u64::MAX)
            .await
            .expect("third batch");
        assert_eq!(third, Batch::Enqueued(1));
        assert_eq!(pending_extract(store.as_ref()).await, 5);

        // Nothing above the cursor: the pass completes and enqueues nothing.
        let fourth = enqueue_next_batch(store.as_ref(), 2, u64::MAX)
            .await
            .expect("completion step");
        assert_eq!(fourth, Batch::Complete);
        assert_eq!(pending_extract(store.as_ref()).await, 5);

        drain_with_job_body(&store).await;
        assert_eq!(pending_extract(store.as_ref()).await, 0);

        let before = store.graph_table_counts().await.expect("counts");
        assert!(
            before.entities >= 5,
            "every fixture frame produced a domain"
        );

        // The drained pass, and a second one, must leave every table count unchanged.
        assert!(matches!(
            run(&store).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        let after = store.graph_table_counts().await.expect("counts");
        assert_eq!(before, after);
        assert!(matches!(
            run(&store).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(
            store.graph_table_counts().await.expect("counts"),
            before,
            "a replayed drained pass cannot duplicate rows"
        );
    }

    #[tokio::test]
    async fn a_batch_enqueues_real_frame_ids_in_oldest_first_order() {
        let store = test_store().await;
        let frames = insert_frames(store.as_ref(), 3).await;
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, u64::MAX)
                .await
                .expect("batch"),
            Batch::Enqueued(3)
        );
        let jobs = store
            .claim_jobs(&[JobKind::GraphExtract], 10, crate::worker_pool::now_ms())
            .await
            .expect("claim");
        let mut enqueued_frames: Vec<i64> = jobs.iter().filter_map(|job| job.frame_id).collect();
        enqueued_frames.sort_unstable();
        assert_eq!(enqueued_frames, frames);
    }

    #[tokio::test]
    async fn the_job_body_enqueues_exactly_one_bounded_batch_and_then_yields() {
        let store = test_store().await;
        let frames = insert_frames(store.as_ref(), BATCH as usize + 3).await;
        assert!(matches!(
            run(&store).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(
            status.pending_extract,
            u64::from(BATCH),
            "one job body enqueues at most BATCH frames"
        );
        assert_eq!(status.cursor, Some(frames[BATCH as usize - 1]));
        // The next body yields while the pending backlog is at or above the low-water mark.
        assert!(matches!(
            run(&store).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_extract,
            u64::from(BATCH),
            "the body enqueues nothing while the extract backlog is not below low-water"
        );
    }

    #[tokio::test]
    async fn a_refill_yields_while_the_extract_backlog_is_at_the_low_water_mark() {
        let store = test_store().await;
        let frames = insert_frames(store.as_ref(), 3).await;
        for frame_id in frames.iter().take(2).copied() {
            store
                .enqueue_job(NewJob {
                    kind: JobKind::GraphExtract,
                    frame_id: Some(frame_id),
                    priority: 0,
                    max_attempts: GRAPH_MAX_ATTEMPTS,
                    not_before: 0,
                })
                .await
                .expect("seed job");
        }
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(status.pending_extract, 2);
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, 2)
                .await
                .expect("yielded batch"),
            Batch::Enqueued(0)
        );
        assert_eq!(pending_extract(store.as_ref()).await, 2);
        assert_eq!(
            store.graph_backfill_status().await.expect("status").cursor,
            status.cursor
        );
    }

    #[tokio::test]
    async fn an_empty_history_completes_with_zero_counts() {
        let store = test_store().await;
        assert!(matches!(
            run(&store).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(
            store.graph_table_counts().await.expect("counts"),
            GraphTableCounts::default()
        );
        assert_eq!(pending_extract(store.as_ref()).await, 0);
    }

    #[tokio::test]
    async fn a_backfill_job_never_logs_the_completion_line() {
        let store = test_store().await;
        let capture = new_capture();
        {
            let _guard = capture_scope(&capture);
            // Empty history: drained, and the backfill body must stay silent.
            assert!(matches!(
                run(&store).await,
                Outcome::Complete {
                    changed_data: false
                }
            ));
            // A refill body with frames above the cursor must also stay silent.
            insert_frames(store.as_ref(), 2).await;
            assert!(matches!(
                run(&store).await,
                Outcome::Complete {
                    changed_data: false
                }
            ));
            // A drained body after the extraction drained must still stay silent.
            drain_with_job_body(&store).await;
            assert!(matches!(
                run(&store).await,
                Outcome::Complete {
                    changed_data: false
                }
            ));
        }
        assert!(
            capture.lines().is_empty(),
            "only the graph cluster job logs the completion line"
        );
    }

    #[tokio::test]
    async fn run_to_completion_logs_one_post_cluster_line_and_the_second_pass_repeats_it() {
        let store = test_store().await;
        // Two sessions linked by a shared host: the cluster pass must create a thread, so a
        // pre-cluster line would visibly lack the thread counts. Linking is ownership-only,
        // so each frame is assigned to its session before the backfill extracts it.
        let first_session = insert_session(store.as_ref(), 0, Some(1_000_000)).await;
        let second_session = insert_session(store.as_ref(), 2_000_000, Some(3_000_000)).await;
        let first_frame = insert_frame(
            store.as_ref(),
            100_000,
            "https://github.com/acme/widgets/pull/42",
        )
        .await;
        let second_frame = insert_frame(
            store.as_ref(),
            2_100_000,
            "https://github.com/acme/widgets/pull/42",
        )
        .await;
        assert_eq!(
            store
                .assign_frames_session(&[first_frame], Some(first_session))
                .await
                .expect("assign first frame"),
            1
        );
        assert_eq!(
            store
                .assign_frames_session(&[second_frame], Some(second_session))
                .await
                .expect("assign second frame"),
            1
        );

        let first_capture = new_capture();
        let first = {
            let _guard = capture_scope(&first_capture);
            run_to_completion(&store).await.expect("first pass")
        };
        let first_counts = store.graph_table_counts().await.expect("counts");
        assert_eq!(first, first_counts);
        assert!(
            first_counts.threads >= 1 && first_counts.thread_sessions >= 1,
            "the fixture warrants one thread with two members"
        );
        assert_eq!(
            first_capture.lines(),
            vec![crate::graph_cluster::completion_message(&first_counts)],
            "exactly one line, carrying post-cluster counts"
        );

        let second_capture = new_capture();
        let second = {
            let _guard = capture_scope(&second_capture);
            run_to_completion(&store).await.expect("second pass")
        };
        assert_eq!(second, first, "a second pass changes no table count");
        assert_eq!(
            second_capture.lines(),
            first_capture.lines(),
            "a second pass logs the identical line"
        );
    }

    #[tokio::test]
    async fn run_to_completion_processes_every_frame_and_is_idempotent() {
        let store = test_store().await;
        insert_frames(store.as_ref(), 5).await;

        let first = run_to_completion(&store).await.expect("first pass");
        assert!(first.entities >= 5, "each fixture frame produced a domain");
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(status.pending_extract, 0);
        assert_eq!(status.cursor, status.max_frame_id);

        let second = run_to_completion(&store).await.expect("second pass");
        assert_eq!(first, second, "a second pass changes no table count");
        let status = store.graph_backfill_status().await.expect("status");
        assert_eq!(
            status.pending_extract, 0,
            "the second pass enqueues no extraction work"
        );
    }

    #[tokio::test]
    async fn run_to_completion_resumes_after_an_interrupted_first_attempt() {
        let fresh_store = test_store().await;
        insert_frames(fresh_store.as_ref(), 5).await;
        let fresh = run_to_completion(&fresh_store).await.expect("fresh pass");

        let resumed_store = test_store().await;
        insert_frames(resumed_store.as_ref(), 5).await;
        // Simulate a run that was interrupted after one bounded batch was committed.
        assert_eq!(
            enqueue_next_batch(resumed_store.as_ref(), 2, u64::MAX)
                .await
                .expect("interrupted batch"),
            Batch::Enqueued(2)
        );
        let resumed = run_to_completion(&resumed_store)
            .await
            .expect("resumed pass");
        assert_eq!(
            fresh, resumed,
            "a resumed pass reaches the same five counts as a fresh pass"
        );
    }
}
