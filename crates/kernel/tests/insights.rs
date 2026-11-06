//! Integration coverage for the 0.4.5 P8 insights generator (`03 §7j.1`, 0.4.5 D10).
//!
//! Every case drives the real [`kernel::insights::run_insight_generate`] against a real
//! in-memory `SqliteStore` seeded through the store's own write paths (sessions, a thread
//! plan, heuristic entity hits, an open mark and two session vectors), so the assertions state
//! what the seeded history supports rather than what a fake returned. Expectations are computed
//! from the seed values: the fixtures below are the only literals.
//!
//! Covered here (the pure per-rule cases live in the module's own unit tests):
//!
//! - one pass produces all five kinds, each row with dense evidence and the expected payload;
//! - a second pass **replaces** rows instead of duplicating them (`03 §7j.5`);
//! - deleting a referenced row makes the sweep flag the insight stale, and the next pass
//!   regenerates without it (`07` C10);
//! - a store that cannot write insights yields `Outcome::Retry` and leaves the table byte for
//!   byte unchanged — the generator never half-applies a pass.

use std::sync::Arc;

use async_trait::async_trait;
use kernel::insights::{
    run_insight_generate, Outcome, LOOKBACK_MS, NEIGHBOUR_MIN_SIMILARITY, RESUMPTION_GAP_MS,
};
use store::{SqliteStore, EMBEDDING_DIM};
use traits::{
    Embedding, EntityKind, EvidenceKind, FrameEnrichmentInput, GraphHit, Insight, InsightFilter,
    InsightGenerator, InsightKind, InsightPayload, Job, JobStats, NewFrame, NewInsight, NewJob,
    NewSession, OcrResult, OpenLoopSource, SearchHit, SearchQuery, SessionHost, SessionKind, Store,
    ThreadDraft, ThreadMemberDraft, ThreadPlan, VisionAnalysis,
};

const DAY: i64 = 86_400_000;
/// A whole minute, so a pass window ending here is exact and every expectation is arithmetic
/// over the fixture instead of a rounded timestamp.
const NOW: i64 = 1_800_000_000_000;

/// The seeded history one case works against.
struct Seed {
    s1: i64,
    s2: i64,
    s3: i64,
    s4: i64,
    s5: i64,
    thread_a: i64,
    thread_b: i64,
    thread_c: i64,
    mark_id: i64,
    mark_frame: i64,
    entity_id: i64,
    /// `sessions.started_at` / `ended_at` of every seeded session, so expectations never
    /// restate a timestamp the fixture already holds.
    spans: Vec<(i64, i64, Option<i64>)>,
}

impl Seed {
    fn span(&self, session_id: i64) -> (i64, Option<i64>) {
        let (_, started_at, ended_at) = self
            .spans
            .iter()
            .copied()
            .find(|(id, _, _)| *id == session_id)
            .expect("the session is part of the fixture");
        (started_at, ended_at)
    }
}

async fn insert_frame(store: &SqliteStore, at: i64) -> i64 {
    store
        .insert_frame(NewFrame {
            captured_at: at,
            monitor_index: 0,
            width: 1920,
            height: 1080,
            image_path: format!("frames/{at}.webp"),
            content_hash: format!("hash-{at}"),
            app_hint: Some("codex".to_string()),
            window_title: Some("Codex".to_string()),
            browser_url: None,
            capture_trigger: None,
        })
        .await
        .expect("insert frame")
}

async fn insert_session(store: &SqliteStore, started_at: i64, ended_at: Option<i64>) -> i64 {
    store
        .insert_session(NewSession {
            started_at,
            ended_at,
            kind: SessionKind::Ai,
            tool: Some("codex".to_string()),
            host: Some(SessionHost::Desktop),
            context_key: "ai:codex".to_string(),
            confidence: 0.9,
            frozen: false,
        })
        .await
        .expect("insert session")
}

/// Seeds history that supports every one of the five kinds:
///
/// * thread A — two sessions a week apart (a resumption), last touched 12 days ago (abandoned);
/// * thread B — one session, last touched 24 days ago (abandoned);
/// * thread C — one session still open;
/// * one unresolved mark;
/// * entity `acme/widgets` seen in thread A's two sessions (a recurrence);
/// * session 5 with a vector identical to thread A's first session (a semantic neighbour on
///   another day).
async fn seed() -> (Arc<dyn Store>, SqliteStore, Seed) {
    let sqlite = SqliteStore::open_in_memory().expect("open in-memory store");

    let s1_started = NOW - 26 * DAY;
    let s1_ended = NOW - 20 * DAY;
    let s2_started = NOW - 13 * DAY;
    let s2_ended = NOW - 12 * DAY;
    let s3_started = NOW - 25 * DAY;
    let s3_ended = NOW - 24 * DAY;
    let s4_started = NOW - DAY;
    let s5_started = NOW - 5 * DAY;
    let s5_ended = NOW - 4 * DAY;

    let s1 = insert_session(&sqlite, s1_started, Some(s1_ended)).await;
    let s2 = insert_session(&sqlite, s2_started, Some(s2_ended)).await;
    let s3 = insert_session(&sqlite, s3_started, Some(s3_ended)).await;
    let s4 = insert_session(&sqlite, s4_started, None).await;
    let s5 = insert_session(&sqlite, s5_started, Some(s5_ended)).await;

    // Titles/summaries are what the resumption payload's `last_doing` and the session-vector
    // text are built from, so they are written before the vectors.
    sqlite
        .set_session_title_summary(s1, "Draft the schema", "Sketching tables", "test-model")
        .await
        .expect("title s1");
    sqlite
        .set_session_title_summary(
            s2,
            "Finish the schema",
            "Applied the baseline",
            "test-model",
        )
        .await
        .expect("title s2");
    sqlite
        .set_session_title_summary(s3, "Abandoned spike", "Half-done probe", "test-model")
        .await
        .expect("title s3");
    sqlite
        .set_session_title_summary(s4, "Live session", "Still running", "test-model")
        .await
        .expect("title s4");
    sqlite
        .set_session_title_summary(s5, "Vector twin", "Near-identical work", "test-model")
        .await
        .expect("title s5");

    // Identical vectors: cosine similarity 1.0, so the edge clears NEIGHBOUR_MIN_SIMILARITY by
    // any margin and the assertion stays about the rule, not about float noise.
    let embedding = Embedding(vec![1.0_f32; EMBEDDING_DIM]);
    for (session_id, text) in [
        (s1, "Draft the schema\nSketching tables"),
        (s5, "Vector twin\nNear-identical work"),
    ] {
        let written = sqlite
            .upsert_session_vector(session_id, text, &embedding)
            .await
            .expect("upsert session vector");
        assert!(written, "the seeded session vector is accepted");
    }

    sqlite
        .graph_apply_thread_plan(&ThreadPlan {
            threads: vec![
                ThreadDraft {
                    cluster_key: "cluster-a".to_string(),
                    title: "Schema work".to_string(),
                    first_seen_at: s1_started,
                    last_seen_at: s2_ended,
                    sessions: vec![
                        ThreadMemberDraft {
                            session_id: s1,
                            confidence: 0.9,
                        },
                        ThreadMemberDraft {
                            session_id: s2,
                            confidence: 0.85,
                        },
                    ],
                },
                ThreadDraft {
                    cluster_key: "cluster-b".to_string(),
                    title: "Abandoned spike".to_string(),
                    first_seen_at: s3_started,
                    last_seen_at: s3_ended,
                    sessions: vec![ThreadMemberDraft {
                        session_id: s3,
                        confidence: 0.9,
                    }],
                },
                ThreadDraft {
                    cluster_key: "cluster-c".to_string(),
                    title: "Live work".to_string(),
                    first_seen_at: s4_started,
                    last_seen_at: s4_started,
                    sessions: vec![ThreadMemberDraft {
                        session_id: s4,
                        confidence: 0.9,
                    }],
                },
            ],
            ..ThreadPlan::default()
        })
        .await
        .expect("apply thread plan");
    let stored = sqlite.graph_stored_threads().await.expect("stored threads");
    let thread_id = |key: &str| {
        stored
            .iter()
            .find(|thread| thread.cluster_key == key)
            .map(|thread| thread.id)
            .expect("the planned thread is stored")
    };
    let (thread_a, thread_b, thread_c) = (
        thread_id("cluster-a"),
        thread_id("cluster-b"),
        thread_id("cluster-c"),
    );

    let hit = GraphHit {
        kind: EntityKind::Repo,
        canonical_name: "acme/widgets".to_string(),
        normalized_key: "acme/widgets".to_string(),
        confidence: 0.9,
        aliases: Vec::new(),
    };
    for (session_id, at) in [(s1, s1_started), (s2, s2_started)] {
        sqlite
            .graph_apply_hits(Some(session_id), 0, at, std::slice::from_ref(&hit))
            .await
            .expect("apply entity hit");
    }
    let entity_id = sqlite
        .graph_entity_catalog(50)
        .await
        .expect("entity catalog")
        .into_iter()
        .find(|entity| entity.normalized_key == hit.normalized_key)
        .expect("the seeded entity exists")
        .id;

    let mark_frame = insert_frame(&sqlite, NOW - 2 * DAY).await;
    let mark_id = sqlite
        .insert_mark(
            mark_frame,
            NOW - 2 * DAY,
            Some("why did the gate fail?".to_string()),
        )
        .await
        .expect("insert mark");

    let seed = Seed {
        s1,
        s2,
        s3,
        s4,
        s5,
        thread_a,
        thread_b,
        thread_c,
        mark_id,
        mark_frame,
        entity_id,
        spans: vec![
            (s1, s1_started, Some(s1_ended)),
            (s2, s2_started, Some(s2_ended)),
            (s3, s3_started, Some(s3_ended)),
            (s4, s4_started, None),
            (s5, s5_started, Some(s5_ended)),
        ],
    };
    let store: Arc<dyn Store> = Arc::new(sqlite.clone());
    (store, sqlite, seed)
}

async fn list_all(store: &dyn Store) -> Vec<Insight> {
    store
        .list_insights(InsightFilter {
            from_ms: None,
            to_ms: None,
            kind: None,
            limit: 1000,
        })
        .await
        .expect("list insights")
}

fn of_kind(rows: &[Insight], kind: InsightKind) -> Vec<&Insight> {
    rows.iter().filter(|row| row.kind == kind).collect()
}

fn evidence_refs(row: &Insight) -> Vec<(EvidenceKind, i64)> {
    row.evidence
        .iter()
        .map(|evidence| (evidence.kind, evidence.ref_id))
        .collect()
}

/// Asserts the invariants every generated row shares: the pass window, the heuristic
/// generator, dense evidence ordinals and a non-empty evidence list (`03 §7j.2`).
fn assert_pass_invariants(rows: &[Insight]) {
    for row in rows {
        assert_eq!(row.window_start, NOW - LOOKBACK_MS, "the pass window start");
        assert_eq!(row.window_end, NOW, "the pass window end");
        assert_eq!(row.generator, InsightGenerator::Heuristic);
        assert!(
            !row.evidence.is_empty(),
            "an insight without evidence is a defect (03 §7j.2)"
        );
        let ordinals: Vec<u32> = row
            .evidence
            .iter()
            .map(|evidence| evidence.ordinal)
            .collect();
        assert_eq!(
            ordinals,
            (0..row.evidence.len() as u32).collect::<Vec<_>>(),
            "evidence ordinals are 0-based and dense"
        );
        assert_eq!(
            row.payload.kind(),
            row.kind,
            "the payload tag and the row kind agree"
        );
    }
}

#[tokio::test]
async fn insights_pass_generates_every_kind_with_evidence() {
    let (store, _sqlite, seed) = seed().await;
    let outcome = run_insight_generate(&store, NOW).await;
    assert!(
        matches!(outcome, Outcome::Complete { changed_data: true }),
        "a pass over seeded history writes rows"
    );

    let rows = list_all(store.as_ref()).await;
    assert_pass_invariants(&rows);

    // — thread_activity: one row per seeded thread, over its own member sessions ————
    let thread_activity = of_kind(&rows, InsightKind::ThreadActivity);
    assert_eq!(
        thread_activity.len(),
        3,
        "every seeded thread has an activity statement"
    );
    let activity_of = |thread_id: i64| {
        thread_activity
            .iter()
            .find(|row| matches!(&row.payload, InsightPayload::ThreadActivity(payload) if payload.thread_id == thread_id))
            .unwrap_or_else(|| panic!("thread {thread_id} has a thread_activity row"))
    };
    let InsightPayload::ThreadActivity(thread_a_payload) = &activity_of(seed.thread_a).payload
    else {
        unreachable!("filtered above")
    };
    let (s1_start, s1_end) = seed.span(seed.s1);
    let (s2_start, s2_end) = seed.span(seed.s2);
    assert_eq!(thread_a_payload.title, "Schema work");
    assert_eq!(thread_a_payload.session_count, 2);
    assert_eq!(
        thread_a_payload.active_ms,
        (s1_end.expect("closed") - s1_start) + (s2_end.expect("closed") - s2_start)
    );
    assert_eq!(thread_a_payload.first_touched_at, s1_start);
    assert_eq!(thread_a_payload.last_touched_at, s2_end.expect("closed"));
    assert_eq!(
        evidence_refs(activity_of(seed.thread_a)),
        vec![
            (EvidenceKind::Thread, seed.thread_a),
            (EvidenceKind::Session, seed.s1),
            (EvidenceKind::Session, seed.s2),
        ],
        "the thread is ordinal 0 and its sessions follow in started_at order"
    );

    // An open session is active up to the window end.
    let InsightPayload::ThreadActivity(thread_c_payload) = &activity_of(seed.thread_c).payload
    else {
        unreachable!("filtered above")
    };
    let (s4_start, s4_end) = seed.span(seed.s4);
    assert_eq!(s4_end, None, "the fixture's live session is open");
    assert_eq!(thread_c_payload.session_count, 1);
    assert_eq!(thread_c_payload.active_ms, NOW - s4_start);
    assert_eq!(thread_c_payload.last_touched_at, NOW);

    // — open_loop: the unresolved mark plus the threads dropped mid-way ————
    let open_loops = of_kind(&rows, InsightKind::OpenLoop);
    let abandoned_before = NOW - RESUMPTION_GAP_MS;
    let expected_abandoned = [seed.thread_a, seed.thread_b]
        .into_iter()
        .filter(|thread_id| {
            let last_end = match *thread_id {
                id if id == seed.thread_a => seed.span(seed.s2).1,
                _ => seed.span(seed.s3).1,
            };
            last_end.is_some_and(|end| end <= abandoned_before)
        })
        .count();
    assert_eq!(
        open_loops.len(),
        1 + expected_abandoned,
        "the mark plus every thread whose last session ended at least RESUMPTION_GAP_MS ago"
    );
    let mark_loop = open_loops
        .iter()
        .find(|row| matches!(&row.payload, InsightPayload::OpenLoop(payload) if payload.source == OpenLoopSource::UnresolvedMark))
        .expect("the unresolved mark is an open loop");
    let InsightPayload::OpenLoop(mark_payload) = &mark_loop.payload else {
        unreachable!("filtered above")
    };
    assert_eq!(mark_payload.mark_id, Some(seed.mark_id));
    assert_eq!(mark_payload.frame_id, Some(seed.mark_frame));
    assert_eq!(
        mark_payload.note.as_deref(),
        Some("why did the gate fail?"),
        "the note is carried verbatim"
    );
    assert_eq!(mark_payload.thread_id, None);
    assert_eq!(mark_payload.opened_at, NOW - 2 * DAY);
    assert_eq!(mark_payload.idle_ms, 2 * DAY);
    assert_eq!(
        evidence_refs(mark_loop),
        vec![
            (EvidenceKind::Mark, seed.mark_id),
            (EvidenceKind::Frame, seed.mark_frame),
        ]
    );

    let abandoned = open_loops
        .iter()
        .find(|row| matches!(&row.payload, InsightPayload::OpenLoop(payload) if payload.thread_id == Some(seed.thread_b)))
        .expect("a thread untouched for 24 days is abandoned");
    let InsightPayload::OpenLoop(abandoned_payload) = &abandoned.payload else {
        unreachable!("filtered above")
    };
    let (_, s3_end) = seed.span(seed.s3);
    let s3_end = s3_end.expect("closed");
    assert_eq!(abandoned_payload.source, OpenLoopSource::AbandonedThread);
    assert_eq!(abandoned_payload.title.as_deref(), Some("Abandoned spike"));
    assert_eq!(abandoned_payload.mark_id, None);
    assert_eq!(abandoned_payload.opened_at, s3_end);
    assert_eq!(abandoned_payload.idle_ms, NOW - s3_end);
    assert_eq!(
        evidence_refs(abandoned),
        vec![
            (EvidenceKind::Thread, seed.thread_b),
            (EvidenceKind::Session, seed.s3),
        ]
    );
    // The live thread is never a loop.
    assert!(
        !open_loops.iter().any(|row| matches!(
            &row.payload,
            InsightPayload::OpenLoop(payload) if payload.thread_id == Some(seed.thread_c)
        )),
        "a thread with an open session is still in play"
    );

    // — recurrence: the entity seen in both of thread A's sessions ————
    let recurrence = of_kind(&rows, InsightKind::Recurrence);
    assert_eq!(recurrence.len(), 1, "only one entity was seen twice");
    let InsightPayload::Recurrence(recurrence_payload) = &recurrence[0].payload else {
        panic!("expected a recurrence payload");
    };
    assert_eq!(recurrence_payload.entity_id, seed.entity_id);
    assert_eq!(recurrence_payload.entity_kind, EntityKind::Repo.as_str());
    assert_eq!(recurrence_payload.entity_name, "acme/widgets");
    assert_eq!(recurrence_payload.first_session_id, seed.s1);
    assert_eq!(recurrence_payload.first_seen_at, s1_start);
    assert_eq!(recurrence_payload.second_session_id, seed.s2);
    assert_eq!(recurrence_payload.second_seen_at, s2_start);
    assert_eq!(recurrence_payload.gap_ms, s2_start - s1_start);
    assert!(recurrence_payload.gap_ms >= kernel::insights::RECURRENCE_GAP_MS);
    assert_eq!(recurrence_payload.occurrence_count, 2);
    assert_eq!(
        evidence_refs(recurrence[0]),
        vec![
            (EvidenceKind::Entity, seed.entity_id),
            (EvidenceKind::Session, seed.s1),
            (EvidenceKind::Session, seed.s2),
        ]
    );

    // — semantic_neighbour: the two sessions with identical vectors ————
    let neighbours = of_kind(&rows, InsightKind::SemanticNeighbour);
    let mut neighbour_subjects: Vec<i64> = neighbours
        .iter()
        .map(|row| match &row.payload {
            InsightPayload::SemanticNeighbour(payload) => payload.session_id,
            other => panic!("unexpected payload {other:?}"),
        })
        .collect();
    neighbour_subjects.sort_unstable();
    let mut expected_subjects = vec![seed.s1, seed.s5];
    expected_subjects.sort_unstable();
    assert_eq!(
        neighbour_subjects, expected_subjects,
        "both sessions with a vector are described"
    );
    let s1_neighbours = neighbours
        .iter()
        .find(|row| matches!(&row.payload, InsightPayload::SemanticNeighbour(payload) if payload.session_id == seed.s1))
        .expect("session 1 is described");
    let InsightPayload::SemanticNeighbour(s1_payload) = &s1_neighbours.payload else {
        unreachable!("filtered above")
    };
    assert_eq!(
        s1_payload.session_title.as_deref(),
        Some("Draft the schema")
    );
    assert_eq!(s1_payload.session_started_at, s1_start);
    assert_eq!(s1_payload.neighbours.len(), 1);
    assert_eq!(s1_payload.neighbours[0].session_id, seed.s5);
    assert_eq!(s1_payload.neighbours[0].started_at, seed.span(seed.s5).0);
    assert!(
        s1_payload.neighbours[0].similarity >= NEIGHBOUR_MIN_SIMILARITY,
        "a kept neighbour clears the similarity floor: {}",
        s1_payload.neighbours[0].similarity
    );
    assert!(
        (s1_payload.neighbours[0].similarity - 1.0).abs() < 1e-3,
        "identical stored vectors are ~1.0 similar"
    );
    assert_eq!(
        evidence_refs(s1_neighbours),
        vec![
            (EvidenceKind::Session, seed.s1),
            (EvidenceKind::Session, seed.s5),
        ],
        "the subject is ordinal 0 and each listed neighbour follows"
    );

    // — resumption: thread A picked up a week after its first session ended ————
    let resumptions = of_kind(&rows, InsightKind::Resumption);
    assert_eq!(resumptions.len(), 1, "only thread A resumes");
    let InsightPayload::Resumption(resumption) = &resumptions[0].payload else {
        panic!("expected a resumption payload");
    };
    assert_eq!(resumption.thread_id, seed.thread_a);
    assert_eq!(resumption.title, "Schema work");
    assert_eq!(resumption.previous_session_id, seed.s1);
    assert_eq!(resumption.previous_ended_at, s1_end.expect("closed"));
    assert_eq!(resumption.resumed_session_id, seed.s2);
    assert_eq!(resumption.resumed_at, s2_start);
    assert_eq!(resumption.gap_ms, s2_start - s1_end.expect("closed"));
    assert!(resumption.gap_ms >= RESUMPTION_GAP_MS);
    assert_eq!(
        resumption.last_doing.as_deref(),
        Some("Sketching tables"),
        "the previous session's summary is preferred over its title"
    );
    assert_eq!(
        evidence_refs(resumptions[0]),
        vec![
            (EvidenceKind::Thread, seed.thread_a),
            (EvidenceKind::Session, seed.s1),
            (EvidenceKind::Session, seed.s2),
        ]
    );

    // — the evidence invariant the pass sweeps for ————
    assert!(
        store
            .unevidenced_insight_ids()
            .await
            .expect("sweep unevidenced insights")
            .is_empty(),
        "no insight is stored without evidence"
    );
    assert!(
        rows.iter().all(|row| !row.stale),
        "a fresh pass leaves nothing stale"
    );
}

#[tokio::test]
async fn insights_second_pass_replaces_instead_of_duplicating() {
    let (store, _sqlite, _seed) = seed().await;
    assert!(matches!(
        run_insight_generate(&store, NOW).await,
        Outcome::Complete { .. }
    ));
    let first = list_all(store.as_ref()).await;
    let counts = |rows: &[Insight]| {
        [
            InsightKind::ThreadActivity,
            InsightKind::OpenLoop,
            InsightKind::Recurrence,
            InsightKind::SemanticNeighbour,
            InsightKind::Resumption,
        ]
        .map(|kind| of_kind(rows, kind).len())
    };
    let first_counts = counts(&first);
    assert!(
        first_counts.iter().all(|count| *count > 0),
        "the fixture supports every kind: {first_counts:?}"
    );

    assert!(matches!(
        run_insight_generate(&store, NOW).await,
        Outcome::Complete { .. }
    ));
    let second = list_all(store.as_ref()).await;
    assert_eq!(
        counts(&second),
        first_counts,
        "a second pass of the same window replaces each kind instead of stacking rows"
    );
    assert_eq!(
        second.len(),
        first.len(),
        "replace semantics keep the row count stable (03 §7j.5)"
    );
    assert_pass_invariants(&second);
    assert!(store
        .unevidenced_insight_ids()
        .await
        .expect("sweep")
        .is_empty());
}

#[tokio::test]
async fn insights_stale_rows_are_flagged_then_replaced_on_the_next_pass() {
    let (store, sqlite, seed) = seed().await;
    assert!(matches!(
        run_insight_generate(&store, NOW).await,
        Outcome::Complete { .. }
    ));
    let before = list_all(store.as_ref()).await;
    let referencing_s5 = before
        .iter()
        .filter(|row| {
            row.evidence.iter().any(|evidence| {
                evidence.kind == EvidenceKind::Session && evidence.ref_id == seed.s5
            })
        })
        .count();
    assert!(
        referencing_s5 > 0,
        "the fixture's neighbour rows reference session 5"
    );

    // Deleting the session removes the row its evidence pointed at — the case `07` C10 is
    // about: the statement is no longer verifiable from stored data.
    assert!(
        sqlite
            .delete_unfrozen_session(seed.s5)
            .await
            .expect("delete session"),
        "the seeded session is unfrozen and deletable"
    );
    let flagged = store
        .mark_insights_stale_where_evidence_missing()
        .await
        .expect("mark stale insights");
    assert_eq!(
        flagged as usize, referencing_s5,
        "every row referencing the deleted session is flagged, and only those"
    );

    let flagged_rows = list_all(store.as_ref()).await;
    for row in &flagged_rows {
        let references_s5 = row
            .evidence
            .iter()
            .any(|evidence| evidence.kind == EvidenceKind::Session && evidence.ref_id == seed.s5);
        assert_eq!(
            row.stale, references_s5,
            "staleness follows the missing evidence, nothing else"
        );
    }

    // The next pass replaces the unverifiable rows: the subject and its neighbour are gone.
    assert!(matches!(
        run_insight_generate(&store, NOW).await,
        Outcome::Complete { .. }
    ));
    let after = list_all(store.as_ref()).await;
    assert!(
        after.iter().all(|row| !row.stale),
        "the regenerated pass clears every stale row"
    );
    assert!(
        !after
            .iter()
            .any(|row| row.evidence.iter().any(
                |evidence| evidence.kind == EvidenceKind::Session && evidence.ref_id == seed.s5
            )),
        "no surviving row references the deleted session"
    );
    assert!(
        of_kind(&after, InsightKind::SemanticNeighbour).is_empty(),
        "with one vector left there is no neighbour statement"
    );
    assert_pass_invariants(&after);
}

/// A `Store` decorator that forwards everything to a real in-memory store but refuses
/// `replace_insights` — the generator's only write path. The real store never fails that
/// call, so this is the only way to drive the pass's failure policy without stubbing success.
struct FailReplaceInsights {
    inner: SqliteStore,
}

#[async_trait]
impl Store for FailReplaceInsights {
    async fn replace_insights(
        &self,
        _kind: InsightKind,
        _window_start: i64,
        _window_end: i64,
        _batch: Vec<NewInsight>,
    ) -> traits::Result<Vec<i64>> {
        Err(anyhow::anyhow!("injected replace_insights failure"))
    }

    // The reads the pass performs before its first write, forwarded unchanged.
    async fn insight_thread_activity(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> traits::Result<Vec<traits::ThreadActivityInput>> {
        self.inner
            .insight_thread_activity(window_start, window_end)
            .await
    }
    async fn insight_open_marks(
        &self,
        window_end: i64,
    ) -> traits::Result<Vec<traits::OpenMarkInput>> {
        self.inner.insight_open_marks(window_end).await
    }
    async fn insight_entity_sessions(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> traits::Result<Vec<traits::EntitySessionInput>> {
        self.inner
            .insight_entity_sessions(window_start, window_end)
            .await
    }
    async fn insight_session_neighbours(
        &self,
        window_start: i64,
        window_end: i64,
        k: u32,
    ) -> traits::Result<Vec<traits::SessionNeighbourInput>> {
        self.inner
            .insight_session_neighbours(window_start, window_end, k)
            .await
    }
    async fn insight_thread_sessions(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> traits::Result<Vec<traits::ThreadSessionInput>> {
        self.inner
            .insight_thread_sessions(window_start, window_end)
            .await
    }
    async fn mark_insights_stale_where_evidence_missing(&self) -> traits::Result<u64> {
        self.inner
            .mark_insights_stale_where_evidence_missing()
            .await
    }
    async fn unevidenced_insight_ids(&self) -> traits::Result<Vec<i64>> {
        self.inner.unevidenced_insight_ids().await
    }
    async fn list_insights(&self, filter: InsightFilter) -> traits::Result<Vec<Insight>> {
        self.inner.list_insights(filter).await
    }

    // The required trait surface (everything else has a default body).
    async fn insert_frame(&self, frame: NewFrame) -> traits::Result<i64> {
        self.inner.insert_frame(frame).await
    }
    async fn insert_ocr(&self, frame_id: i64, ocr: OcrResult) -> traits::Result<()> {
        self.inner.insert_ocr(frame_id, ocr).await
    }
    async fn insert_vision(&self, frame_id: i64, vision: VisionAnalysis) -> traits::Result<()> {
        self.inner.insert_vision(frame_id, vision).await
    }
    async fn upsert_text_embedding(
        &self,
        frame_id: i64,
        chunk_index: i32,
        chunk_text: &str,
        source: traits::ChunkSource,
        emb: &Embedding,
        model: &str,
    ) -> traits::Result<()> {
        self.inner
            .upsert_text_embedding(frame_id, chunk_index, chunk_text, source, emb, model)
            .await
    }
    async fn hybrid_search(&self, query: &SearchQuery) -> traits::Result<Vec<SearchHit>> {
        self.inner.hybrid_search(query).await
    }
    async fn get_enrichment_input(
        &self,
        frame_id: i64,
    ) -> traits::Result<Option<FrameEnrichmentInput>> {
        self.inner.get_enrichment_input(frame_id).await
    }
    async fn enqueue_job(&self, job: NewJob) -> traits::Result<i64> {
        self.inner.enqueue_job(job).await
    }
    async fn claim_jobs(
        &self,
        kinds: &[traits::JobKind],
        limit: u32,
        now: i64,
    ) -> traits::Result<Vec<Job>> {
        self.inner.claim_jobs(kinds, limit, now).await
    }
    async fn complete_job(&self, id: i64) -> traits::Result<()> {
        self.inner.complete_job(id).await
    }
    async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> traits::Result<()> {
        self.inner.fail_job(id, err, retry_at).await
    }
    async fn job_stats(&self) -> traits::Result<JobStats> {
        self.inner.job_stats().await
    }
    async fn reset_stale_running_jobs(&self, older_than_ms: i64) -> traits::Result<u64> {
        self.inner.reset_stale_running_jobs(older_than_ms).await
    }
    async fn get_setting(&self, key: &str) -> traits::Result<Option<String>> {
        self.inner.get_setting(key).await
    }
    async fn set_setting(&self, key: &str, value: &str) -> traits::Result<()> {
        self.inner.set_setting(key, value).await
    }
    async fn delete_settings(&self, keys: &[&str]) -> traits::Result<Vec<String>> {
        self.inner.delete_settings(keys).await
    }
}

#[tokio::test]
async fn insights_forced_replace_failure_retries_and_leaves_rows_untouched() {
    let (store, sqlite, _seed) = seed().await;
    assert!(matches!(
        run_insight_generate(&store, NOW).await,
        Outcome::Complete { .. }
    ));
    let before = list_all(store.as_ref()).await;
    assert!(!before.is_empty(), "the fixture produced rows to preserve");

    let failing: Arc<dyn Store> = Arc::new(FailReplaceInsights {
        inner: sqlite.clone(),
    });
    let outcome = run_insight_generate(&failing, NOW).await;
    assert!(
        matches!(outcome, Outcome::Retry(_)),
        "a store write error is retryable, not a dead letter: {outcome:?}"
    );

    let after = list_all(store.as_ref()).await;
    let ids = |rows: &[Insight]| {
        let mut ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        ids.sort_unstable();
        ids
    };
    assert_eq!(
        ids(&after),
        ids(&before),
        "the failed pass wrote nothing and deleted nothing"
    );
    assert_eq!(
        after.len(),
        before.len(),
        "the insights table is unchanged by a failed pass"
    );
}

/// An OCR provider that refuses to run: the insights pass is model-free, so a test that
/// drives the kernel seam must never reach text recognition.
struct UnusedOcr;

#[async_trait]
impl traits::OcrProvider for UnusedOcr {
    async fn recognize(&self, _frame: &traits::CapturedFrame) -> traits::Result<OcrResult> {
        anyhow::bail!("OCR is not exercised by insights generation")
    }
}

#[tokio::test]
async fn insights_on_demand_request_through_the_kernel_queues_one_pass() {
    let (store, _sqlite, _seed) = seed().await;
    let temp = tempfile::tempdir().expect("temp dir");
    let factory: kernel::CaptureFactory =
        Arc::new(|_, _| panic!("capture is not part of insights generation"));
    let kernel = kernel::Kernel::new(
        store.clone(),
        Arc::new(UnusedOcr) as Arc<dyn traits::OcrProvider>,
        factory,
        temp.path().join("frames"),
        traits::Readiness::default(),
    );

    let pending = |store: Arc<dyn Store>| async move {
        store
            .pending_jobs_of_kind(traits::JobKind::InsightGenerate)
            .await
            .expect("pending pass count")
    };

    kernel.request_insights_generation().await;
    assert_eq!(
        pending(store.clone()).await,
        1,
        "an explicit request queues exactly one pass"
    );
    kernel.request_insights_generation().await;
    assert_eq!(
        pending(store.clone()).await,
        1,
        "a second request never stacks a redundant pass"
    );
    kernel.stop_insight_scheduler().await;
}
