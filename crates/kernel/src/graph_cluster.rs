//! The `graph_cluster` job body: one deterministic thread-planning run plus the single
//! completion evidence line (`03 §7h.4/.5/.6`, 0.4.5 P5).
//!
//! The job reads the complete cluster input (sessions plus strong session-entity links), the
//! persisted threads and the override rows, runs the pure `graph` planner and persists the
//! resulting plan in one store transaction. Clustering is deterministic and idempotent
//! (`03 §7h.4`): running it twice over the same data produces the same threads, the same
//! `cluster_key` values and the same titles, and a membership change rebuilds rows rather
//! than editing them.
//!
//! Overrides are applied here too, after clustering, always winning (`03 §7h.5`); P5 never
//! inserts one itself, and the current planner emits **no** override-row rewrites: merge
//! durability is retention of the emptied source row plus replay order (`06` #37 revised), so
//! `ThreadPlan.override_updates` stays empty even though the store contract can apply one.
//!
//! This job also owns the `03 §7h.6` completion evidence line: after a successful plan apply,
//! and only when the graph is drained (`cursor >= max_frame_id` and no pending extraction),
//! it logs the one greppable line carrying the five table counts. Owning the line here makes
//! the counts necessarily post-cluster, so a restart's second pass reports identical numbers.
//! The backfill job never logs it, and neither does a refill batch.

use std::sync::{Arc, Mutex};

use traits::{GraphTableCounts, Store};

use crate::worker_pool::Outcome;

/// The literal marker of the completion evidence line (`03 §7h.6` acceptance evidence).
pub(crate) const COMPLETION_MARKER: &str = "graph backfill complete";

/// Kernel-local memory of the last extraction state a successful cluster pass covered.
///
/// Not persisted state (`03 §7h.6`: the queue is the checkpoint): it lives in the kernel
/// alongside the producer and is shared with the worker pool so the scheduler can tell a
/// successful pass from a dead-lettered one. Without it, a cluster job that dead-letters
/// would leave `done_extract` unchanged and the scheduler would never retry, permanently
/// suppressing the completion line.
///
/// The generation guards the reverse race: the reconcile pass can invalidate the marker while a
/// cluster job is already planning. The job snapshots the generation before it plans and records
/// only if the generation is unchanged, so a stale plan cannot erase the invalidation and leave
/// repaired links unplanned until extraction moves again. A mutex keeps the generation check and
/// the marker write atomic (no lost-update window between two atomics).
pub(crate) struct ClusterProgress {
    state: Mutex<ClusterState>,
}

#[derive(Debug, Clone, Copy)]
struct ClusterState {
    done_extract: u64,
    generation: u64,
}

impl ClusterProgress {
    /// Sentinel for "no cluster pass has succeeded in this process yet".
    pub(crate) const NEVER: u64 = u64::MAX;

    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(ClusterState {
                done_extract: Self::NEVER,
                generation: 0,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ClusterState> {
        self.state.lock().expect("cluster progress lock")
    }

    /// Records that a plan covering this `done_extract` value was applied successfully. The job
    /// body uses [`Self::record_generation`]; this direct form exists for tests that simulate a
    /// settled pass.
    #[cfg(test)]
    pub(crate) fn record(&self, done_extract: u64) {
        self.lock().done_extract = done_extract;
    }

    /// The generation a cluster job must pass back to [`Self::record_generation`].
    pub(crate) fn generation(&self) -> u64 {
        self.lock().generation
    }

    /// Records success only when no invalidation happened since `generation` was snapshotted.
    /// Returns `false` when the pass is stale: the marker stays invalidated so the next drained
    /// tick enqueues another pass.
    pub(crate) fn record_generation(&self, done_extract: u64, generation: u64) -> bool {
        let mut state = self.lock();
        if state.generation != generation {
            return false;
        }
        state.done_extract = done_extract;
        true
    }

    /// Forces the next drained scheduler tick to enqueue a cluster pass: the generation advances
    /// and the marker returns to [`Self::NEVER`]. A cluster job that snapshotted the previous
    /// generation cannot record over this.
    pub(crate) fn invalidate(&self) {
        let mut state = self.lock();
        state.generation = state.generation.wrapping_add(1);
        state.done_extract = Self::NEVER;
    }

    /// `true` when the last successful plan does not cover `done_extract`. Invalidating the
    /// marker makes this true, so repaired links are re-planned even when extraction itself
    /// has not moved.
    pub(crate) fn needs_cluster(&self, done_extract: u64) -> bool {
        self.last_clustered_done_extract() != done_extract
    }

    /// The `done_extract` value the last successful plan covered, or [`Self::NEVER`].
    pub(crate) fn last_clustered_done_extract(&self) -> u64 {
        self.lock().done_extract
    }
}

/// Runs one claimed `graph_cluster` job. The pass is model-free and safe to retry: an I/O or
/// plan failure retries; a successful pass is idempotent by construction. `progress` is the
/// kernel-local success marker; it is `None` only through the public `process_job` test seam.
pub(crate) async fn run(
    store: &Arc<dyn Store>,
    now_ms: i64,
    progress: Option<&ClusterProgress>,
) -> Outcome {
    match run_job(store.as_ref(), now_ms, progress).await {
        Ok(()) => Outcome::Complete {
            changed_data: false,
        },
        Err(error) => Outcome::Retry(format!("graph cluster: {error}")),
    }
}

/// One clustering run: snapshot, plan, persist, log the completion line when the graph is
/// drained, then record success (`03 §7h.4/.5/.6`).
async fn run_job(
    store: &dyn Store,
    now_ms: i64,
    progress: Option<&ClusterProgress>,
) -> anyhow::Result<()> {
    // Snapshot the reconcile generation before planning: an invalidation that happens while this
    // pass runs must not be erased by the record below, or repaired links would stay unplanned
    // until extraction moves again.
    let generation = progress.map(ClusterProgress::generation);
    // Snapshot before planning: the recorded progress must never claim more extraction state
    // than the plan actually covered, or a newer `done_extract` would suppress a needed
    // re-cluster. Extraction can only add links after this point, so the plan sees at least
    // this state; the worst case is one redundant idempotent pass, never a stale graph.
    let status = store.graph_backfill_status().await?;
    run_pass(store, now_ms).await?;
    // The evidence line is only truthful for a completed graph: drained status means every
    // frame above the queue cursor has been extracted, so the counts are final. It is logged
    // before success is recorded, so a failure to log leaves the marker stale and the
    // scheduler retries this pass instead of permanently losing the line.
    if crate::graph_backfill::is_drained(&status) {
        log_completion(store).await?;
    }
    if let (Some(progress), Some(generation)) = (progress, generation) {
        if !progress.record_generation(status.done_extract, generation) {
            tracing::debug!(
                "graph cluster: links were repaired during the pass; keeping progress invalidated"
            );
        }
    }
    Ok(())
}

/// One clustering run without the completion-line decision: read, plan, persist
/// (`03 §7h.4/.5`). Shared by the job body and the evidence driver
/// ([`crate::graph_backfill::run_to_completion`], which logs its own post-cluster line).
pub(crate) async fn run_pass(store: &dyn Store, now_ms: i64) -> anyhow::Result<()> {
    let input = store.graph_cluster_input(now_ms).await?;
    let stored = store.graph_stored_threads().await?;
    let overrides = store.graph_thread_overrides().await?;
    let plan = graph::clustering::plan_threads(&input, &stored, &overrides)?;
    store.graph_apply_thread_plan(&plan).await?;
    tracing::debug!(
        threads = plan.threads.len(),
        deleted = plan.delete_thread_ids.len(),
        override_updates = plan.override_updates.len(),
        "graph cluster: pass applied"
    );
    Ok(())
}

/// The single-line acceptance evidence (`03 §7h.6`): the literal marker plus all five counts
/// as `name=value`, so the completion log can be grepped unambiguously.
pub(crate) fn completion_message(counts: &GraphTableCounts) -> String {
    format!(
        "{COMPLETION_MARKER} entities={} entity_aliases={} session_entities={} threads={} \
         thread_sessions={}",
        counts.entities,
        counts.entity_aliases,
        counts.session_entities,
        counts.threads,
        counts.thread_sessions
    )
}

/// Reads the five table counts and logs the one completion line (`03 §7h.6`). Callers must
/// have applied the cluster pass first so `threads`/`thread_sessions` are final.
pub(crate) async fn log_completion(store: &dyn Store) -> anyhow::Result<GraphTableCounts> {
    let counts = store.graph_table_counts().await?;
    tracing::info!("{}", completion_message(&counts));
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::AtomicU8;
    use std::sync::{Arc, RwLock};

    use store::SqliteStore;
    use traits::{JobKind, NewFrame, NewSession, OcrResult, SessionKind, Store};

    use super::*;
    use crate::graph_backfill::test_support::new_capture;
    use crate::graph_backfill::{enqueue_next_batch, Batch};
    use crate::graph_scheduler::{tick, TickState};
    use crate::worker_pool::AnswerSlot;

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
                window_title: Some("Work graph".to_string()),
                browser_url: Some(browser_url.to_string()),
                capture_trigger: None,
            })
            .await
            .expect("insert frame");
        store
            .insert_ocr(
                frame_id,
                OcrResult {
                    text: "working on the work graph".to_string(),
                    mean_confidence: 0.9,
                    engine: "test".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .expect("insert ocr");
        frame_id
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

    /// Seeds two linked sessions (same host) and drains their extraction through the
    /// production queue path, leaving a drained graph that warrants one thread.
    async fn seed_drained_thread_graph(store: &Arc<dyn Store>) -> (i64, i64) {
        // The graph producer waits for the sessions historical pass; this fixture has its
        // sessions, so it marks that gate complete the same way the sessions scheduler does.
        store
            .set_setting(
                crate::sessions_scheduler::BACKFILL_MARKER,
                r#"{"cursor_ms":1,"target_ms":1}"#,
            )
            .await
            .expect("write sessions history marker");
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
        // Ownership is the graph's link source (P5 review fix), so the fixture assigns the
        // frames the same way the sessions scheduler would.
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
        assert_eq!(
            enqueue_next_batch(store.as_ref(), 10, u64::MAX)
                .await
                .expect("backfill batch"),
            Batch::Enqueued(2)
        );
        loop {
            let jobs = store
                .claim_jobs(&[JobKind::GraphExtract], 16, crate::worker_pool::now_ms())
                .await
                .expect("claim extract jobs");
            if jobs.is_empty() {
                break;
            }
            for job in jobs {
                let outcome = crate::graph_extract::run(store, job.frame_id).await;
                assert!(matches!(
                    outcome,
                    crate::worker_pool::Outcome::Complete { .. }
                ));
                store
                    .complete_job(job.id)
                    .await
                    .expect("complete extract job");
            }
        }
        assert!(
            store
                .graph_backfill_status()
                .await
                .expect("status")
                .pending_extract
                == 0
        );
        (first_session, second_session)
    }

    /// Runs one drained producer tick and the cluster job it enqueued, capturing the
    /// completion line the job emits.
    async fn run_drained_cluster_pass(
        store: &Arc<dyn Store>,
        progress: &Arc<ClusterProgress>,
        label: &str,
    ) -> Vec<String> {
        let idle: crate::graph_scheduler::IdleSlot = Arc::new(RwLock::new(None));
        let answer: AnswerSlot = Arc::new(RwLock::new(None));
        let dirty = crate::graph_reconcile::new_dirty_sessions();
        let mut state = TickState::default();
        tick(
            store.as_ref(),
            &AtomicU8::new(0),
            &idle,
            &answer,
            progress,
            &dirty,
            &mut state,
        )
        .await;
        let jobs = store
            .claim_jobs(&[JobKind::GraphCluster], 1, crate::worker_pool::now_ms())
            .await
            .expect("claim cluster job");
        assert_eq!(jobs.len(), 1, "{label}: one cluster job is pending");
        let capture = new_capture();
        {
            let _guard = crate::graph_backfill::test_support::capture_scope(&capture);
            let outcome = run(store, crate::worker_pool::now_ms(), Some(progress)).await;
            assert!(
                matches!(outcome, Outcome::Complete { .. }),
                "{label}: cluster job completes"
            );
            store
                .complete_job(jobs[0].id)
                .await
                .expect("complete cluster job");
        }
        capture.lines()
    }

    #[tokio::test]
    async fn a_cluster_pass_plans_the_shared_entity_thread_and_is_idempotent() {
        let store = test_store().await;
        let first_session = insert_session(store.as_ref(), 0, Some(1_000_000)).await;
        let second_session = insert_session(store.as_ref(), 2_000_000, Some(3_000_000)).await;
        for (captured_at, session_index) in [(100_000_i64, 0), (2_100_000, 1)] {
            let frame_id = insert_frame(
                store.as_ref(),
                captured_at,
                "https://github.com/acme/widgets/pull/42",
            )
            .await;
            let session = if session_index == 0 {
                first_session
            } else {
                second_session
            };
            // Linking is ownership-only (P5 review ruling): the frame must be assigned to
            // its session before extraction, or it would carry entities without a link.
            assert_eq!(
                store
                    .assign_frames_session(&[frame_id], Some(session))
                    .await
                    .expect("assign frame"),
                1
            );
            let input = store
                .graph_frame_input(frame_id)
                .await
                .expect("frame input")
                .expect("frame row");
            assert_eq!(
                input.session.as_ref().map(|value| value.session_id),
                Some(session),
                "the frame's owning session is its link target"
            );
            crate::graph_extract::extract_frame(store.as_ref(), frame_id)
                .await
                .expect("extract frame");
        }

        assert!(matches!(
            run(&store, 10_000_000, None).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        let first_threads = store.graph_stored_threads().await.expect("threads");
        let first_counts = store.graph_table_counts().await.expect("counts");
        assert_eq!(
            first_threads.len(),
            1,
            "two linked sessions form one thread"
        );

        // Expectations derive from the fixture through the documented rule: a seed entity
        // is one that strong (>= 0.7) links share between at least two sessions, the
        // cluster key is its sorted id list, and the title is the dominant seed's
        // canonical name (equal weights here, so the lowest entity id wins).
        let cluster_input = store
            .graph_cluster_input(10_000_000)
            .await
            .expect("cluster input");
        let mut sessions_per_entity: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
        for link in &cluster_input.links {
            if link.confidence >= graph::clustering::SEED_CONFIDENCE_MIN {
                sessions_per_entity
                    .entry(link.entity_id)
                    .or_default()
                    .insert(link.session_id);
            }
        }
        let seed_ids: Vec<i64> = sessions_per_entity
            .iter()
            .filter(|(_, sessions)| sessions.len() >= 2)
            .map(|(entity_id, _)| *entity_id)
            .collect();
        assert!(
            !seed_ids.is_empty(),
            "the two fixture frames share strong entities"
        );
        let expected_key = format!(
            "seed:{}",
            seed_ids
                .iter()
                .map(i64::to_string)
                .collect::<Vec<String>>()
                .join(",")
        );
        let catalog = store
            .graph_entity_catalog(100)
            .await
            .expect("entity catalog");
        let dominant = catalog
            .iter()
            .find(|entity| entity.id == seed_ids[0])
            .expect("the dominant seed entity is in the catalog");
        assert_eq!(first_threads[0].cluster_key, expected_key);
        assert_eq!(first_threads[0].title, dominant.canonical_name);
        let members: BTreeSet<i64> = first_threads[0]
            .sessions
            .iter()
            .map(|member| member.session_id)
            .collect();
        assert_eq!(
            members,
            BTreeSet::from([first_session, second_session]),
            "both linked sessions are thread members"
        );

        // Second run over the same data: identical rows, no duplicates.
        assert!(matches!(
            run(&store, 10_000_000, None).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(
            store.graph_stored_threads().await.expect("threads"),
            first_threads,
            "clustering is deterministic and idempotent"
        );
        assert_eq!(
            store.graph_table_counts().await.expect("counts"),
            first_counts
        );
    }

    #[tokio::test]
    async fn sessions_without_a_shared_strong_entity_form_no_thread() {
        let store = test_store().await;
        let first_session = insert_session(store.as_ref(), 0, Some(1_000_000)).await;
        let second_session = insert_session(store.as_ref(), 2_000_000, Some(3_000_000)).await;
        for (captured_at, url, session) in [
            (100_000_i64, "https://site-one.test/work", first_session),
            (2_100_000, "https://site-two.test/work", second_session),
        ] {
            let frame_id = insert_frame(store.as_ref(), captured_at, url).await;
            // Ownership-only linking: assign the frame so its link exists; the two sessions
            // still share no strong entity, which is what this test is about.
            assert_eq!(
                store
                    .assign_frames_session(&[frame_id], Some(session))
                    .await
                    .expect("assign frame"),
                1
            );
            crate::graph_extract::extract_frame(store.as_ref(), frame_id)
                .await
                .expect("extract frame");
        }
        assert!(
            !store
                .graph_cluster_input(10_000_000)
                .await
                .expect("cluster input")
                .links
                .is_empty(),
            "the fixture sessions do have links, they just share no strong entity"
        );
        run_pass(store.as_ref(), 10_000_000)
            .await
            .expect("cluster pass");
        assert!(
            store
                .graph_stored_threads()
                .await
                .expect("threads")
                .is_empty(),
            "unshared entities cannot seed a thread"
        );
        assert_eq!(store.graph_table_counts().await.expect("counts").threads, 0);
    }

    #[tokio::test]
    async fn the_completion_line_is_logged_after_the_cluster_pass_and_a_restart_pass_is_identical()
    {
        let store = test_store().await;
        seed_drained_thread_graph(&store).await;
        let counts_before_cluster = store.graph_table_counts().await.expect("counts");
        assert_eq!(
            counts_before_cluster.threads, 0,
            "no thread exists before the cluster pass"
        );

        // First drained pass: the producer enqueues the cluster job, and the job logs the
        // line only after applying the plan, so the line's thread counts are post-cluster.
        let progress = Arc::new(ClusterProgress::new());
        let first_lines = run_drained_cluster_pass(&store, &progress, "first pass").await;
        let counts = store.graph_table_counts().await.expect("counts");
        assert!(
            counts.threads >= 1 && counts.thread_sessions >= 1,
            "the fixture warrants one thread with two members"
        );
        assert_eq!(
            first_lines,
            vec![completion_message(&counts)],
            "exactly one line, carrying post-cluster counts"
        );

        // A second drained pass over the same DB with fresh kernel-local memory (the state a
        // restart begins with) replays the idempotent cluster pass and prints the same line.
        let restarted = Arc::new(ClusterProgress::new());
        let second_lines = run_drained_cluster_pass(&store, &restarted, "restart pass").await;
        assert_eq!(
            second_lines, first_lines,
            "a restart's second pass prints identical counts"
        );
        assert_eq!(
            store.graph_table_counts().await.expect("counts"),
            counts,
            "the second pass changes no table count"
        );
    }

    #[test]
    fn completion_message_carries_the_marker_and_every_count_as_name_value() {
        let counts = GraphTableCounts {
            entities: 11,
            entity_aliases: 22,
            session_entities: 33,
            threads: 44,
            thread_sessions: 55,
        };
        let line = completion_message(&counts);
        assert!(line.starts_with(COMPLETION_MARKER));
        for (name, value) in [
            ("entities", counts.entities),
            ("entity_aliases", counts.entity_aliases),
            ("session_entities", counts.session_entities),
            ("threads", counts.threads),
            ("thread_sessions", counts.thread_sessions),
        ] {
            assert!(
                line.contains(&format!("{name}={value}")),
                "missing {name}={value} in {line}"
            );
        }
    }

    #[test]
    fn a_never_clustered_progress_is_distinct_from_zero_extraction() {
        let progress = ClusterProgress::new();
        assert_eq!(
            progress.last_clustered_done_extract(),
            ClusterProgress::NEVER
        );
        assert_ne!(progress.last_clustered_done_extract(), 0);
        progress.record(0);
        assert_eq!(progress.last_clustered_done_extract(), 0);
    }

    #[test]
    fn a_stale_pass_cannot_erase_a_concurrent_invalidation() {
        let progress = ClusterProgress::new();
        let generation = progress.generation();
        // A pass that started before the repair finishes normally.
        assert!(progress.record_generation(7, generation));
        assert_eq!(progress.last_clustered_done_extract(), 7);

        // A repair invalidates while another pass is running: that pass must not record over it,
        // otherwise the repaired links would never be re-planned on an otherwise idle graph.
        progress.invalidate();
        assert!(!progress.record_generation(7, generation));
        assert_eq!(
            progress.last_clustered_done_extract(),
            ClusterProgress::NEVER
        );
        assert!(progress.needs_cluster(7));

        // The pass that snapshotted the new generation records normally.
        let next_generation = progress.generation();
        assert!(progress.record_generation(7, next_generation));
        assert_eq!(progress.last_clustered_done_extract(), 7);
        assert!(!progress.needs_cluster(7));
    }
}
