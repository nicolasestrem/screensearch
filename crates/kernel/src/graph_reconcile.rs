//! Bounded session-ownership reconciliation for the work graph (P5 review fix, `03 §7h.3`).
//!
//! The original P5 flow linked a frame's evidence when the frame was extracted and then
//! treated that as final. Session ownership moves afterwards: the sessions scheduler inserts,
//! updates, reassigns and deletes session rows on every pass, and the database cascade
//! (`session_entities.session_id REFERENCES sessions(id) ON DELETE CASCADE`) can remove a
//! session's links while the frames survive (`frames.session_id ... ON DELETE SET NULL`).
//! The automated review of PR #157 found the resulting holes: a link missing because the
//! first extraction ran before the frame's session existed, or stale because the frame moved
//! to a different session.
//!
//! This module repairs those links exactly: [`rebuild_session`] re-extracts every frame the
//! session currently owns and [`Store::graph_replace_session_links`] replaces that session's
//! link rows with the exact aggregate, so a rebuild can restore, move or remove links and can
//! never inflate `hit_count` (the aggregate is absolute, not incremental). The graph
//! scheduler drives that work in bounded batches through [`reconcile_tick`], never on the
//! capture path, and only once extraction is drained so new links are not fought over.
//!
//! Linking is **ownership-only** (`03 §7e`, the final P5 review ruling): a frame's evidence
//! belongs to the session named by `frames.session_id`, and the `[started_at, ended_at)`
//! interval rule is never a linking fallback. A rebuild therefore re-extracts the session's
//! OWNED frames exactly; an unowned frame carries entities only and is never folded into an
//! interval-matching session.
//!
//! Dirty signals are process-local memory (no new table, column or setting, 0.4.5 D2/D6):
//! the sessions scheduler marks the sessions it just mutated, and after the dirty set is
//! empty a startup sweep rebuilds every unfrozen session, so the mutable tail of a graph
//! populated by the pre-fix build heals on the first launch of the fixed one. Frozen history
//! from a pre-fix build has no per-frame evidence ledger, so a partial gap there is not
//! detectable; only a full re-extraction repairs it (`specs/07_KNOWN_GAPS.md`, P5 review-fix
//! section). New historical rows cannot develop the gap: the sessions-history gate keeps
//! extraction from running before the one-shot historical sessions pass finishes. A live
//! session can still close with a missed link if the app shuts down before a drained idle
//! moment repairs it and the session has since frozen (the dirty set is process-local and the
//! sweep deliberately does not walk frozen history); that residual is recorded in `07`.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use traits::{EntityKind, GraphFrameInput, GraphSessionInput, GraphSessionLinkAggregate, Store};

use crate::graph_cluster::ClusterProgress;

/// Session ids whose graph links need an exact rebuild. Ordered so a pass is deterministic.
pub(crate) type DirtySessions = Arc<Mutex<BTreeSet<i64>>>;

/// Sessions one drained scheduler tick rebuilds before yielding. A kernel constant, never a
/// setting (0.4.5 D6, `03 §8`).
pub(crate) const RECONCILE_BATCH: usize = 16;

/// Upper bound on the dirty set. When it overflows the set is cleared, the sentinel requests a
/// fresh startup-sweep snapshot, and a debug line is logged: an unbounded in-memory set is never
/// allowed to grow with capture volume.
pub(crate) const SESSION_DIRTY_CAP: usize = 4096;

/// Sentinel inserted into the dirty set when it overflows. It tells the next reconcile tick to
/// re-snapshot the startup sweep (there is no periodic sweep; this is the recovery path for the
/// process-local marks lost in the overflow, within the unfrozen sweep scope).
const RESWEEP_SENTINEL: i64 = 0;

/// One dirty-set handle shared between the sessions scheduler (writer) and the graph
/// scheduler (drainer).
pub(crate) fn new_dirty_sessions() -> DirtySessions {
    Arc::new(Mutex::new(BTreeSet::new()))
}

/// Marks `ids` as needing a link rebuild. Non-positive ids are ignored (they cannot be real
/// sessions). At [`SESSION_DIRTY_CAP`] the set is cleared, [`RESWEEP_SENTINEL`] is inserted and
/// the overflow is logged at debug: the next reconcile tick re-snapshots the startup sweep, so
/// the lost marks are covered for unfrozen sessions.
pub(crate) fn mark_dirty(dirty: &DirtySessions, ids: impl IntoIterator<Item = i64>) {
    let mut set = dirty.lock().expect("dirty sessions lock");
    for id in ids {
        if id <= 0 {
            continue;
        }
        if set.len() >= SESSION_DIRTY_CAP {
            tracing::debug!(
                cap = SESSION_DIRTY_CAP,
                "graph reconcile: dirty session set overflowed; requesting a fresh sweep"
            );
            set.clear();
            set.insert(RESWEEP_SENTINEL);
        }
        set.insert(id);
    }
}

/// One session's exact recomputed entity links, replacing whatever the store holds for it.
///
/// The aggregate is the absolute link state (`hit_count`, confidence, seen window), not an
/// increment, which is exactly what [`Store::graph_replace_session_links`] needs to be
/// idempotent: rebuilding twice writes the same rows and reports `false` the second time.
///
/// Linking is ownership-only (`03 §7e`): this re-extracts the session's OWNED frames exactly
/// (`Store::session_frames_meta`) and applies no interval fallback, so an unowned frame can
/// never leak its evidence into an interval-matching session.
pub(crate) async fn rebuild_session(store: &dyn Store, session_id: i64) -> anyhow::Result<bool> {
    let Some(session) = store.get_session(session_id).await? else {
        // Deleted after it was marked dirty: its links cascaded with the row and its frames
        // are being claimed by whichever session absorbs them. Nothing to rebuild.
        return Ok(false);
    };
    let frames = store.session_frames_meta(session_id).await?;
    let frame_ids: Vec<i64> = frames.iter().map(|frame| frame.id).collect();
    let contents = store.content_texts_for_frames(&frame_ids).await?;
    let content_by_frame: HashMap<i64, String> = contents
        .into_iter()
        .map(|content| (content.frame_id, content.content_text))
        .collect();
    // The owning session is the extraction context: the reconcile pass is not re-deriving
    // ownership, it is rebuilding the links for the ownership that already exists.
    let session_input = GraphSessionInput {
        session_id,
        started_at: session.started_at,
        ended_at: session.ended_at,
        tool: session.tool.clone(),
        host: session.host,
    };
    let mut aggregates: BTreeMap<(EntityKind, String), LinkAggregate> = BTreeMap::new();
    for frame in &frames {
        let input = GraphFrameInput {
            frame_id: frame.id,
            captured_at: frame.captured_at,
            app_hint: frame.app_hint.clone(),
            window_title: frame.window_title.clone(),
            browser_url: frame.browser_url.clone(),
            content_text: content_by_frame.get(&frame.id).cloned(),
            session: Some(session_input.clone()),
        };
        let recognition =
            sessions::recognize_tool(frame.app_hint.as_deref(), frame.window_title.as_deref());
        let hits = graph::extract(&graph::ExtractionInput {
            frame: &input,
            recognition: recognition.as_ref(),
        });
        for hit in hits {
            let aggregate = aggregates
                .entry((hit.kind, hit.normalized_key.clone()))
                .or_insert_with(|| LinkAggregate {
                    kind: hit.kind,
                    canonical_name: hit.canonical_name.clone(),
                    normalized_key: hit.normalized_key.clone(),
                    confidence: hit.confidence,
                    contributing_frames: BTreeSet::new(),
                    first_seen_at: frame.captured_at,
                    last_seen_at: frame.captured_at,
                    aliases: Vec::new(),
                });
            // The first display form wins for the canonical name; later frames only widen
            // confidence, the seen window, the frame count and the alias set.
            aggregate.confidence = aggregate.confidence.max(hit.confidence);
            aggregate.contributing_frames.insert(frame.id);
            aggregate.first_seen_at = aggregate.first_seen_at.min(frame.captured_at);
            aggregate.last_seen_at = aggregate.last_seen_at.max(frame.captured_at);
            for alias in hit.aliases {
                if !aggregate
                    .aliases
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(&alias))
                {
                    aggregate.aliases.push(alias);
                }
            }
        }
    }
    let links: Vec<GraphSessionLinkAggregate> = aggregates
        .into_values()
        .map(|aggregate| GraphSessionLinkAggregate {
            kind: aggregate.kind,
            canonical_name: aggregate.canonical_name,
            normalized_key: aggregate.normalized_key,
            confidence: aggregate.confidence,
            hit_count: aggregate.contributing_frames.len() as u32,
            first_seen_at: aggregate.first_seen_at,
            last_seen_at: aggregate.last_seen_at,
            aliases: aggregate.aliases,
        })
        .collect();
    // A session with no frames replaces its links with an empty set, clearing stale rows.
    store.graph_replace_session_links(session_id, &links).await
}

/// Accumulator for one `(kind, normalized_key)` across a session's frames.
struct LinkAggregate {
    kind: EntityKind,
    canonical_name: String,
    normalized_key: String,
    confidence: f64,
    contributing_frames: BTreeSet<i64>,
    first_seen_at: i64,
    last_seen_at: i64,
    aliases: Vec<String>,
}

/// Startup sweep state, owned by the graph scheduler's [`crate::graph_scheduler::TickState`].
/// It snapshots unfrozen sessions once; an overflow of the dirty set requests a fresh snapshot
/// (there is no periodic sweep).
#[derive(Debug, Default)]
pub(crate) struct StartupSweep {
    initialized: bool,
    pending: VecDeque<i64>,
}

/// Runs one bounded reconciliation step. The graph scheduler calls this only while graph
/// extraction is drained, so it never rebuilds links that a pending `graph_extract` job will
/// write anyway.
///
/// Each stage gets up to [`RECONCILE_BATCH`] sessions per drained tick: the dirty batch runs
/// first, then the startup sweep advances even if the dirty set remains non-empty, so a busy
/// set (or one permanently failing session) cannot starve the sweep. A [`RESWEEP_SENTINEL`] in
/// the dirty batch forces a fresh sweep snapshot. Every rebuild that reports a change
/// invalidates the cluster progress so the next drained tick re-plans threads against the
/// repaired links.
pub(crate) async fn reconcile_tick(
    store: &dyn Store,
    dirty: &DirtySessions,
    cluster_progress: &ClusterProgress,
    sweep: &mut StartupSweep,
) {
    let (batch, resweep) = take_dirty_batch(dirty);
    if resweep {
        sweep.initialized = false;
    }
    if !batch.is_empty() {
        drain_dirty_sessions(store, dirty, cluster_progress, batch).await;
    }
    drain_startup_sweep(store, cluster_progress, sweep).await;
}

/// Takes the next bounded batch out of the dirty set, reporting whether the overflow sentinel
/// was among it. The batch is removed before rebuilding so a session marked dirty again during
/// the rebuild re-enters the set and is not lost.
fn take_dirty_batch(dirty: &DirtySessions) -> (Vec<i64>, bool) {
    let mut set = dirty.lock().expect("dirty sessions lock");
    let mut batch: Vec<i64> = set.iter().take(RECONCILE_BATCH).copied().collect();
    let resweep = match batch
        .iter()
        .position(|session_id| *session_id == RESWEEP_SENTINEL)
    {
        Some(position) => {
            batch.remove(position);
            set.remove(&RESWEEP_SENTINEL);
            true
        }
        None => false,
    };
    for session_id in &batch {
        set.remove(session_id);
    }
    (batch, resweep)
}

/// Rebuilds a taken dirty batch. Failed rebuilds are re-added so they retry on a later tick.
async fn drain_dirty_sessions(
    store: &dyn Store,
    dirty: &DirtySessions,
    cluster_progress: &ClusterProgress,
    batch: Vec<i64>,
) {
    let mut failed = Vec::new();
    for session_id in batch {
        match rebuild_session(store, session_id).await {
            Ok(changed) => {
                if changed {
                    cluster_progress.invalidate();
                }
            }
            Err(error) => {
                // Do not log the session id: opaque ids are CodeQL-cleartext-logging class
                // sensitive data, and the scheduler only needs to know the batch retries.
                tracing::warn!(
                    error = %error,
                    "graph reconcile: session rebuild failed; retrying later"
                );
                failed.push(session_id);
            }
        }
    }
    mark_dirty(dirty, failed);
}

/// Rebuilds up to [`RECONCILE_BATCH`] sessions of the startup sweep, snapshotting the
/// unfrozen sessions (newest `updated_at` first) on the first pass. A failed rebuild is
/// moved to the back of the queue and retried on a later tick.
async fn drain_startup_sweep(
    store: &dyn Store,
    cluster_progress: &ClusterProgress,
    sweep: &mut StartupSweep,
) {
    if !sweep.initialized {
        match store.unfrozen_sessions().await {
            Ok(mut sessions) => {
                // The sweep order is deterministic: newest update first, then highest id.
                sessions.sort_by(|left, right| {
                    right
                        .updated_at
                        .cmp(&left.updated_at)
                        .then_with(|| right.id.cmp(&left.id))
                });
                sweep.pending = sessions.into_iter().map(|session| session.id).collect();
                sweep.initialized = true;
                tracing::debug!(
                    pending = sweep.pending.len(),
                    "graph reconcile: startup sweep snapshotted unfrozen sessions"
                );
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "graph reconcile: startup sweep snapshot failed; retrying later"
                );
                return;
            }
        }
    }
    let mut rebuilt = 0usize;
    for _ in 0..RECONCILE_BATCH {
        let Some(session_id) = sweep.pending.pop_front() else {
            break;
        };
        match rebuild_session(store, session_id).await {
            Ok(changed) => {
                rebuilt += 1;
                if changed {
                    cluster_progress.invalidate();
                }
            }
            Err(error) => {
                // No session id in the log (CodeQL cleartext-logging class); the queue keeps it.
                tracing::warn!(
                    error = %error,
                    "graph reconcile: startup sweep rebuild failed; retrying later"
                );
                sweep.pending.push_back(session_id);
            }
        }
    }
    if rebuilt > 0 && sweep.pending.is_empty() {
        tracing::debug!("graph reconcile: startup sweep complete");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use store::SqliteStore;
    use traits::{NewFrame, NewSession, OcrResult, SessionKind, Store};

    use super::*;

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
                    text: "work graph notes".to_string(),
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

    async fn links_for(store: &dyn Store, session_id: i64) -> Vec<traits::GraphClusterLink> {
        store
            .graph_cluster_input(10_000_000)
            .await
            .expect("cluster input")
            .links
            .into_iter()
            .filter(|link| link.session_id == session_id)
            .collect()
    }

    #[tokio::test]
    async fn a_late_session_assignment_restores_the_missing_link() {
        let store = test_store().await;
        let frame_id = insert_frame(store.as_ref(), 1_000, "https://github.com/acme/widgets").await;
        // The review finding: the frame is extracted while it has no session, so entities
        // exist but no link does.
        crate::graph_extract::extract_frame(store.as_ref(), frame_id)
            .await
            .expect("initial extraction");
        assert!(
            store
                .graph_cluster_input(10_000_000)
                .await
                .expect("cluster input")
                .links
                .is_empty(),
            "no session exists yet, so no link can exist"
        );

        let session_id = insert_session(store.as_ref(), 900, Some(1_100)).await;
        assert_eq!(
            store
                .assign_frames_session(&[frame_id], Some(session_id))
                .await
                .expect("assign frame"),
            1
        );

        // A reconcile pass rebuilds the session's exact links from its owned frames.
        let dirty = new_dirty_sessions();
        mark_dirty(&dirty, [session_id]);
        let progress = ClusterProgress::new();
        let mut sweep = StartupSweep::default();
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;

        let links = links_for(store.as_ref(), session_id).await;
        let domain = links
            .iter()
            .find(|link| link.canonical_name == "github.com")
            .expect("the shared domain link is restored");
        assert!(
            (domain.confidence - graph::extraction::confidence::URL_DOMAIN_SCHEME).abs() < 1e-9,
            "the link carries the rule's own confidence"
        );
        assert_eq!(
            progress.last_clustered_done_extract(),
            ClusterProgress::NEVER,
            "a changed rebuild invalidates the cluster progress"
        );

        // Rebuilding the same session again is an exact no-op: the aggregate (including
        // hit_count) matches the persisted link state, so the store reports no change.
        assert!(
            !rebuild_session(store.as_ref(), session_id)
                .await
                .expect("second rebuild"),
            "an unchanged rebuild reports no change and cannot inflate hit_count"
        );
    }

    #[tokio::test]
    async fn a_deleted_and_recreated_session_restores_its_links_without_inflation() {
        let store = test_store().await;
        let old_session = insert_session(store.as_ref(), 900, Some(2_100)).await;
        let frame_a = insert_frame(store.as_ref(), 1_000, "https://github.com/acme/widgets").await;
        let frame_b = insert_frame(store.as_ref(), 2_000, "https://github.com/acme/widgets").await;
        store
            .assign_frames_session(&[frame_a, frame_b], Some(old_session))
            .await
            .expect("assign frames");
        crate::graph_extract::extract_frame(store.as_ref(), frame_a)
            .await
            .expect("extract frame a");
        crate::graph_extract::extract_frame(store.as_ref(), frame_b)
            .await
            .expect("extract frame b");
        let before = links_for(store.as_ref(), old_session).await;
        assert!(!before.is_empty(), "the fixture session has links");

        // Another session keeps the id allocator from immediately reusing the deleted row's
        // id, so the fixture really gets a new session id.
        insert_session(store.as_ref(), 5_000, Some(6_000)).await;
        assert!(store
            .delete_unfrozen_session(old_session)
            .await
            .expect("delete old session"));
        let new_session = insert_session(store.as_ref(), 900, Some(2_100)).await;
        assert_ne!(new_session, old_session, "re-creation allocates a new id");
        // The FK cascade removed the old links and SET NULL left the frames unowned; the
        // re-created session re-claims them.
        assert_eq!(
            store
                .assign_frames_session(&[frame_a, frame_b], Some(new_session))
                .await
                .expect("reassign frames"),
            2
        );

        let dirty = new_dirty_sessions();
        mark_dirty(&dirty, [old_session, new_session]);
        let progress = ClusterProgress::new();
        let mut sweep = StartupSweep::default();
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;

        let restored = links_for(store.as_ref(), new_session).await;
        assert_eq!(
            restored.len(),
            before.len(),
            "the re-created session gets the same exact links back"
        );
        assert!(restored
            .iter()
            .any(|link| link.canonical_name == "github.com"));
        // The aggregate is absolute: two contributing frames and identical rebuilt state.
        assert!(
            !rebuild_session(store.as_ref(), new_session)
                .await
                .expect("second rebuild"),
            "the restored link is exact and cannot be inflated by replay"
        );
        assert!(
            links_for(store.as_ref(), old_session).await.is_empty(),
            "the deleted session has no links"
        );
    }

    #[tokio::test]
    async fn a_frame_moved_between_sessions_removes_the_stale_link() {
        let store = test_store().await;
        let session_a = insert_session(store.as_ref(), 0, Some(1_000)).await;
        let session_b = insert_session(store.as_ref(), 2_000, Some(3_000)).await;
        let moved_frame =
            insert_frame(store.as_ref(), 100, "https://github.com/acme/widgets").await;
        let stays_with_a = insert_frame(store.as_ref(), 200, "https://example.test/docs").await;
        store
            .assign_frames_session(&[moved_frame, stays_with_a], Some(session_a))
            .await
            .expect("assign to A");
        crate::graph_extract::extract_frame(store.as_ref(), moved_frame)
            .await
            .expect("extract moved frame");
        crate::graph_extract::extract_frame(store.as_ref(), stays_with_a)
            .await
            .expect("extract staying frame");
        assert_eq!(
            links_for(store.as_ref(), session_a).await.len(),
            3,
            "two frames on shared hosts contribute their entities to session A"
        );

        // The user's ownership change moves one frame to session B.
        assert_eq!(
            store
                .assign_frames_session(&[moved_frame], Some(session_b))
                .await
                .expect("move the frame"),
            1
        );
        let dirty = new_dirty_sessions();
        mark_dirty(&dirty, [session_a, session_b]);
        let progress = ClusterProgress::new();
        let mut sweep = StartupSweep::default();
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;

        let a_links = links_for(store.as_ref(), session_a).await;
        assert!(
            a_links
                .iter()
                .all(|link| link.canonical_name != "github.com"),
            "the stale link left session A"
        );
        assert!(a_links
            .iter()
            .any(|link| link.canonical_name == "example.test"));
        let b_links = links_for(store.as_ref(), session_b).await;
        assert_eq!(b_links.len(), 2, "the moved frame links to session B");
        assert!(b_links
            .iter()
            .any(|link| link.canonical_name == "github.com"));
    }

    #[tokio::test]
    async fn a_session_with_no_frames_clears_its_stale_links() {
        let store = test_store().await;
        let session_id = insert_session(store.as_ref(), 0, Some(1_000)).await;
        let frame_id = insert_frame(store.as_ref(), 100, "https://github.com/acme/widgets").await;
        store
            .assign_frames_session(&[frame_id], Some(session_id))
            .await
            .expect("assign frame");
        crate::graph_extract::extract_frame(store.as_ref(), frame_id)
            .await
            .expect("extract frame");
        assert!(!links_for(store.as_ref(), session_id).await.is_empty());

        assert_eq!(
            store
                .assign_frames_session(&[frame_id], None)
                .await
                .expect("unassign frame"),
            1
        );
        assert!(
            rebuild_session(store.as_ref(), session_id)
                .await
                .expect("rebuild empty session"),
            "removing the last link is a change"
        );
        assert!(
            links_for(store.as_ref(), session_id).await.is_empty(),
            "a frame-less session clears its stale link rows"
        );
    }

    #[tokio::test]
    async fn rebuilding_a_deleted_session_is_a_noop() {
        let store = test_store().await;
        assert!(!rebuild_session(store.as_ref(), 424_242)
            .await
            .expect("rebuild missing session"));
    }

    #[tokio::test]
    async fn an_unchanged_reconcile_does_not_invalidate_the_cluster_progress() {
        let store = test_store().await;
        let frame_id = insert_frame(store.as_ref(), 100, "https://github.com/acme/widgets").await;
        // Extraction ran before the session existed: the review finding's missed link.
        crate::graph_extract::extract_frame(store.as_ref(), frame_id)
            .await
            .expect("extract frame");
        let session_id = insert_session(store.as_ref(), 0, Some(1_000)).await;
        store
            .assign_frames_session(&[frame_id], Some(session_id))
            .await
            .expect("assign frame");
        assert!(
            rebuild_session(store.as_ref(), session_id)
                .await
                .expect("first rebuild"),
            "the first rebuild restores the missed links"
        );

        let progress = ClusterProgress::new();
        progress.record(7);
        let dirty = new_dirty_sessions();
        mark_dirty(&dirty, [session_id]);
        let mut sweep = StartupSweep::default();
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;
        assert_eq!(
            progress.last_clustered_done_extract(),
            7,
            "a no-op rebuild leaves the cluster progress intact"
        );
        assert!(
            dirty.lock().expect("dirty lock").is_empty(),
            "the reconciled session leaves the dirty set"
        );
    }

    #[tokio::test]
    async fn the_startup_sweep_rebuilds_unfrozen_sessions_once() {
        let store = test_store().await;
        let session_id = insert_session(store.as_ref(), 0, Some(1_000)).await;
        let frame_id = insert_frame(store.as_ref(), 100, "https://github.com/acme/widgets").await;
        store
            .assign_frames_session(&[frame_id], Some(session_id))
            .await
            .expect("assign frame");
        crate::graph_extract::extract_frame(store.as_ref(), frame_id)
            .await
            .expect("extract frame");
        let expected_links = links_for(store.as_ref(), session_id).await.len();
        assert!(expected_links > 0);
        // The graph is populated by an older build: the entity exists but the link does not.
        store
            .graph_replace_session_links(session_id, &[])
            .await
            .expect("simulate the old missing link");
        assert!(links_for(store.as_ref(), session_id).await.is_empty());

        let progress = ClusterProgress::new();
        let dirty = new_dirty_sessions();
        let mut sweep = StartupSweep::default();
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;
        assert_eq!(
            links_for(store.as_ref(), session_id).await.len(),
            expected_links,
            "the startup sweep heals a link the old build missed"
        );
        // A second drained tick finds the sweep exhausted and changes nothing.
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;
        assert!(
            !rebuild_session(store.as_ref(), session_id)
                .await
                .expect("rebuild after sweep"),
            "the swept session is exact"
        );
    }

    #[test]
    fn the_dirty_set_is_bounded_and_ignores_invalid_ids() {
        let dirty = new_dirty_sessions();
        mark_dirty(&dirty, [-1, 0, 3]);
        assert_eq!(
            dirty
                .lock()
                .expect("dirty lock")
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![3],
            "non-positive ids are ignored"
        );

        mark_dirty(&dirty, 1..=SESSION_DIRTY_CAP as i64);
        assert_eq!(
            dirty.lock().expect("dirty lock").len(),
            SESSION_DIRTY_CAP,
            "the set fills to the cap"
        );
        mark_dirty(&dirty, [SESSION_DIRTY_CAP as i64 + 1]);
        let (batch, resweep) = take_dirty_batch(&dirty);
        assert!(
            resweep,
            "an overflow requests a fresh startup-sweep snapshot"
        );
        assert_eq!(
            batch,
            vec![SESSION_DIRTY_CAP as i64 + 1],
            "the sweep request is not a session to rebuild"
        );
        assert!(
            dirty.lock().expect("dirty lock").is_empty(),
            "the overflow cleared the older marks and the sentinel"
        );
    }

    #[tokio::test]
    async fn a_dirty_overflow_takes_a_fresh_sweep_snapshot() {
        let store = test_store().await;
        let session_id = insert_session(store.as_ref(), 0, Some(1_000)).await;
        let frame_id = insert_frame(store.as_ref(), 100, "https://github.com/acme/widgets").await;
        store
            .assign_frames_session(&[frame_id], Some(session_id))
            .await
            .expect("assign frame");
        crate::graph_extract::extract_frame(store.as_ref(), frame_id)
            .await
            .expect("extract frame");
        let expected_links = links_for(store.as_ref(), session_id).await.len();
        let progress = ClusterProgress::new();
        let dirty = new_dirty_sessions();
        let mut sweep = StartupSweep::default();
        // First tick snapshots and rebuilds the session; the sweep is then spent.
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;
        assert_eq!(
            links_for(store.as_ref(), session_id).await.len(),
            expected_links
        );

        // Simulate a missing link created after the one-time sweep, then lose the dirty marks
        // to an overflow: the sentinel must make the next tick re-snapshot and heal it again.
        store
            .graph_replace_session_links(session_id, &[])
            .await
            .expect("drop the link");
        assert!(links_for(store.as_ref(), session_id).await.is_empty());
        mark_dirty(&dirty, 1..=SESSION_DIRTY_CAP as i64);
        mark_dirty(&dirty, [SESSION_DIRTY_CAP as i64 + 1]);
        reconcile_tick(store.as_ref(), &dirty, &progress, &mut sweep).await;
        assert_eq!(
            links_for(store.as_ref(), session_id).await.len(),
            expected_links,
            "the overflow request re-runs the sweep over the unfrozen tail"
        );
        assert!(
            dirty.lock().expect("dirty lock").is_empty(),
            "the overflowed marks are gone once the fresh sweep covers them"
        );
    }

    #[tokio::test]
    async fn the_reconcile_does_not_change_entities_or_link_other_sessions() {
        let store = test_store().await;
        // Both frames are extracted before any session exists (entities only, no links).
        let frame_a = insert_frame(store.as_ref(), 100, "https://site-a.test/page").await;
        let frame_b = insert_frame(store.as_ref(), 2_100, "https://site-b.test/page").await;
        crate::graph_extract::extract_frame(store.as_ref(), frame_a)
            .await
            .expect("extract a");
        crate::graph_extract::extract_frame(store.as_ref(), frame_b)
            .await
            .expect("extract b");
        let entities_before = store.graph_table_counts().await.expect("counts").entities;
        let session_a = insert_session(store.as_ref(), 0, Some(1_000)).await;
        let session_b = insert_session(store.as_ref(), 2_000, Some(3_000)).await;
        store
            .assign_frames_session(&[frame_a], Some(session_a))
            .await
            .expect("assign a");
        store
            .assign_frames_session(&[frame_b], Some(session_b))
            .await
            .expect("assign b");

        assert!(rebuild_session(store.as_ref(), session_a)
            .await
            .expect("rebuild a"));
        let counts = store.graph_table_counts().await.expect("counts");
        assert_eq!(
            counts.entities, entities_before,
            "a rebuild reuses the existing entity rows"
        );
        let a_links = links_for(store.as_ref(), session_a).await;
        assert!(!a_links.is_empty());
        assert_eq!(
            counts.session_entities as usize,
            a_links.len(),
            "only session A's links were written"
        );
        assert!(
            links_for(store.as_ref(), session_b).await.is_empty(),
            "rebuilding A does not touch B's links"
        );
    }
}
