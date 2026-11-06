//! Store coverage for the 0.4.5 P8 insight engine's seam (`03 §7j`, `03 §7c` P8 block).
//!
//! Every fixture is seeded through the public store API (`insert_frame`, `insert_session`,
//! `set_session_title_summary`, `upsert_session_vector`, `insert_mark`,
//! `graph_apply_hits`, `graph_apply_thread_plan`), and every expectation is derived from what
//! those calls returned: ids come back from the inserts, windows and figures come from the
//! same constants the fixture was built with. No expectation is a literal copied from the
//! implementation.
//!
//! The fixture opens the store on a temp **file**, not `:memory:`, and keeps a second
//! connection for raw assertions: the atomicity requirements are proved by row counts
//! (`SELECT COUNT(*) FROM insights`) and by manual row edits, and an in-memory store's single
//! connection is private to the store. `tests/ask.rs` uses the same second-connection pattern
//! for the same reason; the store behaviour under test is identical.

use std::collections::HashMap;

use rusqlite::Connection;
use store::{SqliteStore, EMBEDDING_DIM};
use traits::{
    Embedding, EntityFilter, EntityKind, EntitySessionInput, EntitySummary, EvidenceKind, GraphHit,
    InsightEvidence, InsightFilter, InsightGenerator, InsightKind, InsightPayload, JobKind,
    NewFrame, NewInsight, NewJob, NewSession, OpenLoopPayload, OpenLoopSource, OpenMarkInput,
    RecurrencePayload, SessionHost, SessionKind, ThreadActivityInput, ThreadActivityPayload,
    ThreadDraft, ThreadFilter, ThreadMemberDraft, ThreadOverrideDraft, ThreadOverrideKind,
    ThreadOverridePayload, ThreadPlan, ThreadSessionInput, ThreadSummary,
};

/// The confidence every seeded thread member carries.
const MEMBER_CONFIDENCE: f64 = 0.8;

// ---- Fixture ---------------------------------------------------------------------------

struct Fixture {
    path: std::path::PathBuf,
    store: SqliteStore,
    /// Dropped last: the temp directory outlives the store and its file.
    _temp: tempfile::TempDir,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join(format!("insights-{name}.sqlite"));
        let store = SqliteStore::open_path(&path).expect("open store");
        Self {
            path,
            store,
            _temp: temp,
        }
    }

    /// A second connection so a test can assert raw row state.
    fn raw(&self) -> Connection {
        Connection::open(&self.path).expect("open second connection")
    }
}

/// Row count of `table` on a raw connection.
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .expect("count rows")
}

// ---- Seeding ---------------------------------------------------------------------------

fn frame_at(captured_at: i64) -> NewFrame {
    NewFrame {
        captured_at,
        monitor_index: 0,
        width: 1920,
        height: 1080,
        image_path: format!("frames/{captured_at}.jpg"),
        content_hash: format!("hash-{captured_at}"),
        app_hint: Some("Firefox".to_string()),
        window_title: Some("Inbox".to_string()),
        browser_url: Some("https://mail.example.com".to_string()),
        capture_trigger: None,
    }
}

async fn frame(store: &SqliteStore, captured_at: i64) -> i64 {
    store
        .insert_frame(frame_at(captured_at))
        .await
        .expect("insert frame")
}

async fn session(store: &SqliteStore, started_at: i64, ended_at: Option<i64>) -> i64 {
    store
        .insert_session(NewSession {
            started_at,
            ended_at,
            kind: SessionKind::Ai,
            tool: Some("claude-code".to_string()),
            host: Some(SessionHost::Terminal),
            context_key: "ai:claude-code".to_string(),
            confidence: 0.9,
            frozen: false,
        })
        .await
        .expect("insert session")
}

/// A session with cached title/summary text (the shape the embedder consumes).
async fn titled_session(
    store: &SqliteStore,
    started_at: i64,
    ended_at: Option<i64>,
    title: &str,
    summary: &str,
) -> i64 {
    let id = session(store, started_at, ended_at).await;
    let updated = store
        .set_session_title_summary(id, title, summary, "test-model")
        .await
        .expect("set session title/summary");
    assert!(updated, "the session was just inserted");
    id
}

/// A one-hot unit vector: identical indices are cosine-identical (distance 0), distinct
/// indices are orthogonal (distance 1), which makes KNN ordering deterministic.
fn one_hot(hot: usize) -> Embedding {
    let mut vector = vec![0.0_f32; EMBEDDING_DIM];
    vector[hot] = 1.0;
    Embedding(vector)
}

/// Writes the session's vector for its current title/summary text.
async fn seed_vector(store: &SqliteStore, session_id: i64, title: &str, summary: &str, hot: usize) {
    let text = format!("{title}\n{summary}");
    let written = store
        .upsert_session_vector(session_id, &text, &one_hot(hot))
        .await
        .expect("upsert session vector");
    assert!(written, "a titled session's vector write must land");
}

/// One entity hit into a session's graph links (aliases included). The dedup key is the
/// fixture's own stand-in for the extractor's normalization — lowercase with spaces turned
/// into hyphens — so each seeded entity keeps one stable identity across hits.
async fn hit(
    store: &SqliteStore,
    session_id: i64,
    captured_at: i64,
    kind: EntityKind,
    canonical_name: &str,
    confidence: f64,
    aliases: &[&str],
) {
    let normalized_key = canonical_name.to_lowercase().replace(' ', "-");
    store
        .graph_apply_hits(
            Some(session_id),
            0,
            captured_at,
            &[GraphHit {
                kind,
                canonical_name: canonical_name.to_string(),
                normalized_key,
                confidence,
                aliases: aliases.iter().map(|alias| alias.to_string()).collect(),
            }],
        )
        .await
        .expect("apply graph hits");
}

struct ThreadSeed {
    cluster_key: &'static str,
    title: &'static str,
    first_seen_at: i64,
    last_seen_at: i64,
    members: Vec<i64>,
}

/// Persists every seed as one thread plan (the clusterer's write path).
async fn seed_threads(store: &SqliteStore, seeds: &[ThreadSeed]) {
    let plan = ThreadPlan {
        threads: seeds
            .iter()
            .map(|seed| ThreadDraft {
                cluster_key: seed.cluster_key.to_string(),
                title: seed.title.to_string(),
                first_seen_at: seed.first_seen_at,
                last_seen_at: seed.last_seen_at,
                sessions: seed
                    .members
                    .iter()
                    .map(|session_id| ThreadMemberDraft {
                        session_id: *session_id,
                        confidence: MEMBER_CONFIDENCE,
                    })
                    .collect(),
            })
            .collect(),
        delete_thread_ids: Vec::new(),
        override_updates: Vec::new(),
    };
    store
        .graph_apply_thread_plan(&plan)
        .await
        .expect("apply thread plan");
}

/// The persisted id of a seeded thread (the plan upserts by `cluster_key`).
async fn stored_thread_id(store: &SqliteStore, cluster_key: &str) -> i64 {
    store
        .graph_stored_threads()
        .await
        .expect("read stored threads")
        .into_iter()
        .find(|thread| thread.cluster_key == cluster_key)
        .expect("seeded thread exists")
        .id
}

// ---- Insight fixtures ------------------------------------------------------------------

fn recurrence_payload(entity_id: i64) -> InsightPayload {
    InsightPayload::Recurrence(RecurrencePayload {
        entity_id,
        entity_kind: EntityKind::Repo.as_str().to_string(),
        entity_name: format!("entity-{entity_id}"),
        first_session_id: 1,
        first_seen_at: 1_000,
        second_session_id: 2,
        second_seen_at: 2_000,
        gap_ms: 1_000,
        occurrence_count: 2,
    })
}

fn open_loop_payload(mark_id: i64) -> InsightPayload {
    InsightPayload::OpenLoop(OpenLoopPayload {
        source: OpenLoopSource::UnresolvedMark,
        mark_id: Some(mark_id),
        frame_id: Some(1),
        note: None,
        thread_id: None,
        title: None,
        opened_at: 1_000,
        idle_ms: 500,
    })
}

fn thread_activity_payload(thread_id: i64, title: &str) -> InsightPayload {
    InsightPayload::ThreadActivity(ThreadActivityPayload {
        thread_id,
        title: title.to_string(),
        session_count: 2,
        active_ms: 1_000,
        first_touched_at: 1_000,
        last_touched_at: 4_000,
    })
}

/// Dense ordinals `0..n` over the given references, in list order.
fn evidence(items: &[(EvidenceKind, i64)]) -> Vec<InsightEvidence> {
    items
        .iter()
        .enumerate()
        .map(|(position, (kind, ref_id))| InsightEvidence {
            ordinal: u32::try_from(position).expect("ordinal fits a u32"),
            kind: *kind,
            ref_id: *ref_id,
        })
        .collect()
}

fn new_insight(
    window: (i64, i64),
    payload: InsightPayload,
    evidence: Vec<InsightEvidence>,
) -> NewInsight {
    NewInsight {
        window_start: window.0,
        window_end: window.1,
        payload,
        generator: InsightGenerator::Heuristic,
        evidence,
    }
}

/// Every insight of every kind over the whole history.
fn all_insights() -> InsightFilter {
    InsightFilter {
        from_ms: None,
        to_ms: None,
        kind: None,
        limit: 1_000,
    }
}

// ---- Writing evidenced insights --------------------------------------------------------

/// An insight without evidence is a defect (`03 §7j.2`): the call fails and no row is written.
#[tokio::test]
async fn insights_insert_rejects_empty_evidence() {
    let fixture = Fixture::new("insert-empty-evidence");
    let insight = new_insight((1_000, 2_000), recurrence_payload(1), Vec::new());

    let error = fixture
        .store
        .insert_insight(insight)
        .await
        .expect_err("an insight without evidence must be refused");
    assert!(
        error.to_string().contains("at least one evidence row"),
        "{error}"
    );

    let raw = fixture.raw();
    assert_eq!(count(&raw, "insights"), 0, "no insight row was written");
    assert_eq!(
        count(&raw, "insight_evidence"),
        0,
        "no evidence row was written"
    );
}

/// One call writes the insight and all of its evidence; the read shape round-trips them with
/// dense ordinals.
#[tokio::test]
async fn insights_insert_writes_insight_and_evidence_atomically() {
    let fixture = Fixture::new("insert-atomic");
    let first = session(&fixture.store, 1_000, Some(2_000)).await;
    let second = session(&fixture.store, 3_000, Some(4_000)).await;
    seed_threads(
        &fixture.store,
        &[ThreadSeed {
            cluster_key: "budget",
            title: "Budget review",
            first_seen_at: 1_000,
            last_seen_at: 4_000,
            members: vec![first, second],
        }],
    )
    .await;
    let thread = stored_thread_id(&fixture.store, "budget").await;

    let payload = thread_activity_payload(thread, "Budget review");
    let references = evidence(&[
        (EvidenceKind::Thread, thread),
        (EvidenceKind::Session, first),
        (EvidenceKind::Session, second),
    ]);
    let id = fixture
        .store
        .insert_insight(new_insight(
            (1_000, 5_000),
            payload.clone(),
            references.clone(),
        ))
        .await
        .expect("insert insight");
    assert!(id > 0);

    let raw = fixture.raw();
    assert_eq!(count(&raw, "insights"), 1);
    assert_eq!(count(&raw, "insight_evidence"), references.len() as i64);

    let listed = fixture
        .store
        .list_insights(all_insights())
        .await
        .expect("list insights");
    assert_eq!(listed.len(), 1);
    let insight = &listed[0];
    assert_eq!(insight.id, id);
    assert_eq!(insight.kind, InsightKind::ThreadActivity);
    assert_eq!(insight.payload, payload);
    assert_eq!(insight.generator, InsightGenerator::Heuristic);
    assert!(!insight.stale);
    assert!(
        insight.generated_at > 0,
        "generated_at defaults to the DB clock"
    );
    assert_eq!(insight.window_start, 1_000);
    assert_eq!(insight.window_end, 5_000);
    assert_eq!(
        insight.evidence, references,
        "ordinals stay dense and ordered"
    );
}

/// Ordinals that are not exactly `0..n` in order are refused before anything is written.
#[tokio::test]
async fn insights_insert_rejects_non_dense_ordinals() {
    let fixture = Fixture::new("insert-ordinals");
    let session_ref = EvidenceKind::Session;

    let cases: Vec<(&str, Vec<InsightEvidence>)> = vec![
        (
            "a gap at 1",
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: session_ref,
                    ref_id: 1,
                },
                InsightEvidence {
                    ordinal: 2,
                    kind: session_ref,
                    ref_id: 2,
                },
            ],
        ),
        (
            "a repeated ordinal",
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: session_ref,
                    ref_id: 1,
                },
                InsightEvidence {
                    ordinal: 0,
                    kind: session_ref,
                    ref_id: 2,
                },
            ],
        ),
        (
            "a list that starts at 1",
            vec![InsightEvidence {
                ordinal: 1,
                kind: session_ref,
                ref_id: 1,
            }],
        ),
    ];

    for (label, references) in cases {
        let error = fixture
            .store
            .insert_insight(new_insight(
                (1_000, 2_000),
                recurrence_payload(3),
                references,
            ))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("dense and ascending from 0"),
            "{label}: {error}"
        );
    }

    let raw = fixture.raw();
    assert_eq!(count(&raw, "insights"), 0);
    assert_eq!(count(&raw, "insight_evidence"), 0);
}

/// Replace semantics (`03 §7j.5`): only rows of that kind whose window overlaps are dropped,
/// half-open boundaries at both edges keep their neighbours, and the evidence of a dropped
/// row cascades away.
#[tokio::test]
async fn insights_replace_replaces_overlapping_rows_only() {
    let fixture = Fixture::new("replace-overlap");
    let replaced = fixture
        .store
        .insert_insight(new_insight(
            (1_000, 2_000),
            recurrence_payload(1),
            evidence(&[(EvidenceKind::Entity, 1)]),
        ))
        .await
        .expect("insert replaced row");
    let touching_before = fixture
        .store
        .insert_insight(new_insight(
            (0, 900),
            recurrence_payload(2),
            evidence(&[(EvidenceKind::Entity, 2)]),
        ))
        .await
        .expect("insert row touching the window start");
    let touching_after = fixture
        .store
        .insert_insight(new_insight(
            (2_100, 3_000),
            recurrence_payload(3),
            evidence(&[(EvidenceKind::Entity, 3)]),
        ))
        .await
        .expect("insert row touching the window end");
    let far_away = fixture
        .store
        .insert_insight(new_insight(
            (5_000, 6_000),
            recurrence_payload(4),
            evidence(&[(EvidenceKind::Entity, 4)]),
        ))
        .await
        .expect("insert far row");
    let other_kind = fixture
        .store
        .insert_insight(new_insight(
            (1_000, 2_000),
            open_loop_payload(5),
            evidence(&[(EvidenceKind::Mark, 5)]),
        ))
        .await
        .expect("insert other-kind row");
    assert_eq!(count(&fixture.raw(), "insights"), 5);

    let replacement = new_insight(
        (900, 2_100),
        recurrence_payload(6),
        evidence(&[(EvidenceKind::Entity, 6), (EvidenceKind::Session, 7)]),
    );
    let inserted = fixture
        .store
        .replace_insights(
            InsightKind::Recurrence,
            900,
            2_100,
            vec![replacement.clone()],
        )
        .await
        .expect("replace insights");
    assert_eq!(inserted.len(), 1, "one replacement row");

    let raw = fixture.raw();
    assert_eq!(
        count(&raw, "insights"),
        5,
        "one replaced row was exchanged for one new row"
    );
    assert_eq!(
        count(&raw, "insight_evidence"),
        6,
        "the replaced row's evidence cascaded away, the new row carries two"
    );
    let orphaned_evidence: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM insight_evidence WHERE insight_id = ?1",
            [replaced],
            |row| row.get(0),
        )
        .expect("count replaced evidence");
    assert_eq!(orphaned_evidence, 0);

    let listed = fixture
        .store
        .list_insights(all_insights())
        .await
        .expect("list insights");
    let ids: Vec<i64> = listed.iter().map(|insight| insight.id).collect();
    assert_eq!(ids.len(), 5);
    for kept in [touching_before, touching_after, far_away, other_kind] {
        assert!(ids.contains(&kept), "row {kept} must survive the replace");
    }
    assert!(ids.contains(&inserted[0]));
    assert!(!ids.contains(&replaced), "the overlapping row is gone");
    let replacement_row = listed
        .iter()
        .find(|insight| insight.id == inserted[0])
        .expect("replacement is listed");
    assert_eq!(replacement_row.payload, replacement.payload);
}

/// A batch that fails validation deletes nothing and inserts nothing — the whole call is a
/// no-op, not a partial replacement.
#[tokio::test]
async fn insights_replace_rolls_back_on_a_bad_batch() {
    let fixture = Fixture::new("replace-rollback");
    let original = fixture
        .store
        .insert_insight(new_insight(
            (1_000, 2_000),
            recurrence_payload(1),
            evidence(&[(EvidenceKind::Entity, 1), (EvidenceKind::Session, 2)]),
        ))
        .await
        .expect("insert original");
    let raw = fixture.raw();
    let insights_before = count(&raw, "insights");
    let evidence_before = count(&raw, "insight_evidence");
    assert_eq!(insights_before, 1);
    assert_eq!(evidence_before, 2);

    let valid = new_insight(
        (1_000, 2_000),
        recurrence_payload(3),
        evidence(&[(EvidenceKind::Entity, 3)]),
    );

    // (a) one entry without evidence.
    let error = fixture
        .store
        .replace_insights(
            InsightKind::Recurrence,
            1_000,
            2_000,
            vec![
                valid.clone(),
                new_insight((1_000, 2_000), recurrence_payload(4), Vec::new()),
            ],
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("at least one evidence row"),
        "{error}"
    );

    // (b) one entry whose ordinals are not dense.
    let error = fixture
        .store
        .replace_insights(
            InsightKind::Recurrence,
            1_000,
            2_000,
            vec![
                valid.clone(),
                new_insight(
                    (1_000, 2_000),
                    recurrence_payload(5),
                    vec![InsightEvidence {
                        ordinal: 1,
                        kind: EvidenceKind::Entity,
                        ref_id: 5,
                    }],
                ),
            ],
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("dense and ascending from 0"),
        "{error}"
    );

    // (c) one entry stating a different kind than the replaced one.
    let error = fixture
        .store
        .replace_insights(
            InsightKind::Recurrence,
            1_000,
            2_000,
            vec![
                valid,
                new_insight(
                    (1_000, 2_000),
                    open_loop_payload(6),
                    evidence(&[(EvidenceKind::Mark, 6)]),
                ),
            ],
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("not the replaced kind"),
        "{error}"
    );

    let raw = fixture.raw();
    assert_eq!(
        count(&raw, "insights"),
        insights_before,
        "no row was deleted or inserted"
    );
    assert_eq!(count(&raw, "insight_evidence"), evidence_before);
    let listed = fixture
        .store
        .list_insights(all_insights())
        .await
        .expect("list insights");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, original, "the original row is untouched");
}

// ---- Reading insights ------------------------------------------------------------------

/// The list order is `window_start DESC, kind ASC, id DESC`, the window filter is the
/// half-open overlap predicate, and evidence comes back in `ordinal` order even when the rows
/// were written out of order.
#[tokio::test]
async fn insights_list_orders_by_window_kind_and_id() {
    let fixture = Fixture::new("list-order");
    let insert = |payload: InsightPayload, window: (i64, i64), reference: (EvidenceKind, i64)| {
        let store = fixture.store.clone();
        async move {
            store
                .insert_insight(new_insight(window, payload, evidence(&[reference])))
                .await
                .expect("insert insight")
        }
    };

    // Same window, both recurrences: the later id must come first within the same kind.
    let earlier_recurrence = insert(
        recurrence_payload(1),
        (1_000, 2_000),
        (EvidenceKind::Entity, 1),
    )
    .await;
    let open_loop = insert(
        open_loop_payload(2),
        (1_000, 2_000),
        (EvidenceKind::Mark, 2),
    )
    .await;
    let later_recurrence = insert(
        recurrence_payload(3),
        (1_000, 2_000),
        (EvidenceKind::Entity, 3),
    )
    .await;
    let middle = insert(
        recurrence_payload(4),
        (2_000, 3_000),
        (EvidenceKind::Entity, 4),
    )
    .await;
    let newest = insert(
        thread_activity_payload(5, "Newest thread"),
        (3_000, 4_000),
        (EvidenceKind::Thread, 5),
    )
    .await;
    // Ends exactly at the filter's start: `window_end > from` is false, so it is excluded.
    let before_window = insert(
        thread_activity_payload(6, "Before the window"),
        (0, 1_000),
        (EvidenceKind::Thread, 6),
    )
    .await;

    let listed = fixture
        .store
        .list_insights(InsightFilter {
            from_ms: Some(1_500),
            to_ms: Some(3_500),
            kind: None,
            limit: 1_000,
        })
        .await
        .expect("list insights in window");
    let ids: Vec<i64> = listed.iter().map(|insight| insight.id).collect();
    assert_eq!(
        ids,
        vec![
            newest,
            middle,
            open_loop,
            later_recurrence,
            earlier_recurrence
        ],
        "window_start DESC, kind ASC (open_loop before recurrence), id DESC"
    );
    assert!(!ids.contains(&before_window));

    // The kind filter is an exact token, not a prefix.
    let recurrences = fixture
        .store
        .list_insights(InsightFilter {
            from_ms: Some(1_500),
            to_ms: Some(3_500),
            kind: Some(InsightKind::Recurrence),
            limit: 1_000,
        })
        .await
        .expect("list recurrences");
    let recurrence_ids: Vec<i64> = recurrences.iter().map(|insight| insight.id).collect();
    assert_eq!(
        recurrence_ids,
        vec![middle, later_recurrence, earlier_recurrence]
    );

    // Evidence written 2, 0, 1 by hand must still be read 0, 1, 2 — SQLite has no obligation
    // to return rows in insertion order, so the read must order by `ordinal`.
    let manual_payload = serde_json::to_string(&recurrence_payload(9)).expect("payload json");
    let raw = fixture.raw();
    raw.execute(
        "INSERT INTO insights (kind, window_start, window_end, payload, generator)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["recurrence", 5_000, 5_100, manual_payload, "heuristic"],
    )
    .expect("insert manual insight");
    let manual_id = raw.last_insert_rowid();
    for ordinal in [2_i64, 0, 1] {
        raw.execute(
            "INSERT INTO insight_evidence (insight_id, ordinal, kind, ref_id)
             VALUES (?1, ?2, 'session', ?3)",
            rusqlite::params![manual_id, ordinal, 100 + ordinal],
        )
        .expect("insert manual evidence");
    }

    let manual = fixture
        .store
        .list_insights(InsightFilter {
            from_ms: Some(4_900),
            to_ms: Some(5_200),
            kind: None,
            limit: 1_000,
        })
        .await
        .expect("list the manual insight");
    assert_eq!(manual.len(), 1);
    let ordinals: Vec<u32> = manual[0].evidence.iter().map(|item| item.ordinal).collect();
    assert_eq!(ordinals, vec![0, 1, 2]);
    let refs: Vec<i64> = manual[0].evidence.iter().map(|item| item.ref_id).collect();
    assert_eq!(refs, vec![100, 101, 102]);
}

/// The evidence invariant's sweep is empty on a healthy database and names the row whose
/// evidence was removed.
#[tokio::test]
async fn insights_unevidenced_ids_find_orphaned_rows() {
    let fixture = Fixture::new("unevidenced");
    assert!(
        fixture
            .store
            .unevidenced_insight_ids()
            .await
            .expect("sweep an empty database")
            .is_empty(),
        "an empty database has no unevidenced insight"
    );

    let insight = fixture
        .store
        .insert_insight(new_insight(
            (1_000, 2_000),
            recurrence_payload(1),
            evidence(&[(EvidenceKind::Entity, 1)]),
        ))
        .await
        .expect("insert insight");
    assert!(fixture
        .store
        .unevidenced_insight_ids()
        .await
        .expect("sweep a healthy database")
        .is_empty());

    fixture
        .raw()
        .execute(
            "DELETE FROM insight_evidence WHERE insight_id = ?1",
            [insight],
        )
        .expect("strip the evidence");
    assert_eq!(
        fixture
            .store
            .unevidenced_insight_ids()
            .await
            .expect("sweep the damaged database"),
        vec![insight]
    );
}

/// Only an insight with a dangling reference turns stale, only the 0→1 transition counts, and
/// the flagged row is still listed.
#[tokio::test]
async fn insights_stale_marks_only_rows_with_a_dangling_ref() {
    let fixture = Fixture::new("stale");
    let kept = session(&fixture.store, 1_000, Some(2_000)).await;
    let doomed = session(&fixture.store, 3_000, Some(4_000)).await;

    let dangling = fixture
        .store
        .insert_insight(new_insight(
            (1_000, 5_000),
            recurrence_payload(1),
            evidence(&[
                (EvidenceKind::Session, kept),
                (EvidenceKind::Session, doomed),
            ]),
        ))
        .await
        .expect("insert dangling-able insight");
    let healthy = fixture
        .store
        .insert_insight(new_insight(
            (1_000, 5_000),
            recurrence_payload(2),
            evidence(&[(EvidenceKind::Session, kept)]),
        ))
        .await
        .expect("insert healthy insight");

    assert_eq!(
        fixture
            .store
            .mark_insights_stale_where_evidence_missing()
            .await
            .expect("sweep with every reference present"),
        0
    );

    assert!(fixture
        .store
        .delete_unfrozen_session(doomed)
        .await
        .expect("delete the referenced session"));
    assert_eq!(
        fixture
            .store
            .mark_insights_stale_where_evidence_missing()
            .await
            .expect("sweep after the session disappeared"),
        1,
        "exactly the insight with the dangling reference changes"
    );
    assert_eq!(
        fixture
            .store
            .mark_insights_stale_where_evidence_missing()
            .await
            .expect("second sweep"),
        0,
        "a second sweep reports no new mark"
    );

    let listed = fixture
        .store
        .list_insights(all_insights())
        .await
        .expect("list insights");
    let by_id = |id: i64| {
        listed
            .iter()
            .find(|insight| insight.id == id)
            .expect("insight is listed")
    };
    assert!(by_id(dangling).stale, "the flagged row is still served");
    assert!(!by_id(healthy).stale);
}

// ---- Threads and entities --------------------------------------------------------------

/// Thread overlap uses `first_seen_at < to AND last_seen_at >= from`, each bound independent,
/// and the order is `last_seen_at DESC, id DESC`.
#[tokio::test]
async fn insights_threads_in_range_overlap_and_order() {
    let fixture = Fixture::new("threads-range");
    let first = session(&fixture.store, 1_000, Some(2_000)).await;
    let second = session(&fixture.store, 3_000, Some(4_000)).await;
    let third = session(&fixture.store, 6_000, Some(7_000)).await;
    let fourth = session(&fixture.store, 8_000, Some(9_000)).await;
    seed_threads(
        &fixture.store,
        &[
            ThreadSeed {
                cluster_key: "alpha",
                title: "Alpha thread",
                first_seen_at: 1_000,
                last_seen_at: 2_000,
                members: vec![first],
            },
            ThreadSeed {
                cluster_key: "beta",
                title: "Beta thread",
                first_seen_at: 3_000,
                last_seen_at: 4_000,
                members: vec![second],
            },
            ThreadSeed {
                cluster_key: "gamma",
                title: "Gamma thread",
                first_seen_at: 6_000,
                last_seen_at: 9_000,
                members: vec![third, fourth],
            },
        ],
    )
    .await;

    let titles = |threads: &[ThreadSummary]| -> Vec<String> {
        threads.iter().map(|thread| thread.title.clone()).collect()
    };

    let window = fixture
        .store
        .threads_in_range(ThreadFilter {
            from_ms: Some(2_500),
            to_ms: Some(7_000),
            limit: 1_000,
        })
        .await
        .expect("threads in window");
    assert_eq!(
        titles(&window),
        vec!["Gamma thread".to_string(), "Beta thread".to_string()],
        "alpha ended before the window opens"
    );
    let gamma = &window[0];
    assert_eq!(gamma.first_seen_at, 6_000);
    assert_eq!(gamma.last_seen_at, 9_000);
    assert_eq!(gamma.session_count, 2);

    // A lower bound alone keeps only threads last seen at or after it: alpha's window closed
    // before 2_500, so the single-bound form must exclude it just like the two-bound form.
    let from_only = fixture
        .store
        .threads_in_range(ThreadFilter {
            from_ms: Some(2_500),
            to_ms: None,
            limit: 1_000,
        })
        .await
        .expect("threads after a lower bound");
    assert_eq!(
        titles(&from_only),
        vec!["Gamma thread".to_string(), "Beta thread".to_string()],
        "alpha was last seen before the lower bound"
    );

    // The boundary is inclusive on `last_seen_at`: a bound equal to it keeps the thread.
    let boundary = fixture
        .store
        .threads_in_range(ThreadFilter {
            from_ms: Some(2_000),
            to_ms: None,
            limit: 1_000,
        })
        .await
        .expect("threads from the exact last-seen instant");
    assert_eq!(
        titles(&boundary),
        vec![
            "Gamma thread".to_string(),
            "Beta thread".to_string(),
            "Alpha thread".to_string()
        ]
    );

    let to_only = fixture
        .store
        .threads_in_range(ThreadFilter {
            from_ms: None,
            to_ms: Some(3_500),
            limit: 1_000,
        })
        .await
        .expect("threads before an upper bound");
    assert_eq!(
        titles(&to_only),
        vec!["Beta thread".to_string(), "Alpha thread".to_string()],
        "gamma starts after the upper bound"
    );
}

/// `thread_detail` returns the thread, its sessions in `started_at` order (not insert order),
/// and the entities its member sessions touch, with per-thread counts and maximum confidence;
/// a missing id is `None`.
#[tokio::test]
async fn insights_thread_detail_returns_sessions_and_entities() {
    let fixture = Fixture::new("thread-detail");
    // Inserted newest-first so the session order cannot come from insertion order.
    let later = titled_session(
        &fixture.store,
        3_000,
        None,
        "Invoice export",
        "implemented the CSV export",
    )
    .await;
    let earlier = titled_session(
        &fixture.store,
        1_000,
        Some(2_000),
        "Invoice model",
        "rewrote the invoice model",
    )
    .await;
    seed_threads(
        &fixture.store,
        &[ThreadSeed {
            cluster_key: "invoices",
            title: "Invoicing",
            first_seen_at: 1_000,
            last_seen_at: 3_000,
            members: vec![later, earlier],
        }],
    )
    .await;
    let thread = stored_thread_id(&fixture.store, "invoices").await;

    hit(
        &fixture.store,
        earlier,
        1_500,
        EntityKind::Repo,
        "screensearch",
        0.9,
        &["ScreenSearch"],
    )
    .await;
    hit(
        &fixture.store,
        later,
        3_500,
        EntityKind::Repo,
        "screensearch",
        0.6,
        &[],
    )
    .await;
    hit(
        &fixture.store,
        earlier,
        1_600,
        EntityKind::App,
        "Visual Studio Code",
        0.7,
        &[],
    )
    .await;

    let detail = fixture
        .store
        .thread_detail(thread)
        .await
        .expect("read thread detail")
        .expect("the thread exists");
    assert_eq!(detail.thread.id, thread);
    assert_eq!(detail.thread.title, "Invoicing");
    assert_eq!(detail.thread.first_seen_at, 1_000);
    assert_eq!(detail.thread.last_seen_at, 3_000);
    assert_eq!(detail.thread.session_count, 2);

    let session_ids: Vec<i64> = detail.sessions.iter().map(|row| row.session_id).collect();
    assert_eq!(
        session_ids,
        vec![earlier, later],
        "sessions are ordered by started_at ascending"
    );
    assert_eq!(detail.sessions[0].started_at, 1_000);
    assert_eq!(detail.sessions[0].ended_at, Some(2_000));
    assert_eq!(detail.sessions[0].kind, "ai");
    assert_eq!(detail.sessions[0].title.as_deref(), Some("Invoice model"));
    assert_eq!(detail.sessions[0].confidence, MEMBER_CONFIDENCE);
    assert_eq!(
        detail.sessions[1].ended_at, None,
        "an open session stays open"
    );

    let entity_names: Vec<&str> = detail
        .entities
        .iter()
        .map(|entity| entity.canonical_name.as_str())
        .collect();
    assert_eq!(
        entity_names,
        vec!["screensearch", "Visual Studio Code"],
        "ordered by confidence descending"
    );
    assert_eq!(detail.entities[0].kind, "repo");
    assert_eq!(detail.entities[0].session_count, 2);
    assert_eq!(
        detail.entities[0].confidence, 0.9,
        "max over member sessions"
    );
    assert_eq!(detail.entities[1].kind, "app");
    assert_eq!(detail.entities[1].session_count, 1);
    assert_eq!(detail.entities[1].confidence, 0.7);

    assert_eq!(
        fixture
            .store
            .thread_detail(thread + 1_000)
            .await
            .expect("read a missing thread"),
        None
    );
}

/// Entity filtering: exact kind token, case-insensitive substring over the canonical name or
/// any alias (literal — `%`/`_` are not wildcards), `last_seen_at DESC` order, and the
/// all-history link count.
#[tokio::test]
async fn insights_entities_filtered_by_kind_and_alias_substring() {
    let fixture = Fixture::new("entities-filter");
    let first = session(&fixture.store, 1_000, Some(2_000)).await;
    let second = session(&fixture.store, 3_000, Some(4_000)).await;

    hit(
        &fixture.store,
        first,
        1_000,
        EntityKind::Repo,
        "screensearch",
        0.9,
        &["ScreenSearch", "ss-core"],
    )
    .await;
    hit(
        &fixture.store,
        second,
        2_000,
        EntityKind::Repo,
        "screensearch",
        0.8,
        &[],
    )
    .await;
    hit(
        &fixture.store,
        second,
        3_000,
        EntityKind::Domain,
        "github.com",
        0.9,
        &[],
    )
    .await;
    hit(
        &fixture.store,
        first,
        4_000,
        EntityKind::Repo,
        "project-x",
        0.7,
        &["zz-alias"],
    )
    .await;

    let names = |entities: &[EntitySummary]| -> Vec<String> {
        entities
            .iter()
            .map(|entity| entity.canonical_name.clone())
            .collect()
    };

    let repos = fixture
        .store
        .entities_filtered(EntityFilter {
            kind: Some(EntityKind::Repo.as_str().to_string()),
            query: None,
            limit: 1_000,
        })
        .await
        .expect("filter by kind");
    assert_eq!(
        names(&repos),
        vec!["project-x".to_string(), "screensearch".to_string()],
        "last_seen_at DESC, and the domain is filtered out"
    );
    let screensearch = repos
        .iter()
        .find(|entity| entity.canonical_name == "screensearch")
        .expect("the repo is listed");
    assert_eq!(
        screensearch.aliases,
        vec!["ScreenSearch".to_string(), "ss-core".to_string()],
        "aliases in insertion order"
    );
    assert_eq!(
        screensearch.session_count, 2,
        "all-history link count across sessions"
    );
    assert_eq!(screensearch.last_seen_at, 2_000, "the later hit widened it");

    // Lowercase, uppercase and mixed case all match the same ASCII-folded name.
    for query in ["screen", "SCREEN", "Screen"] {
        let matched = fixture
            .store
            .entities_filtered(EntityFilter {
                kind: None,
                query: Some(query.to_string()),
                limit: 1_000,
            })
            .await
            .expect("filter by name substring");
        assert_eq!(
            names(&matched),
            vec!["screensearch".to_string()],
            "query {query:?} matches the canonical name case-insensitively"
        );
    }

    // An alias-only substring: the canonical name does not contain it.
    let alias_only = fixture
        .store
        .entities_filtered(EntityFilter {
            kind: None,
            query: Some("zz-al".to_string()),
            limit: 1_000,
        })
        .await
        .expect("filter by alias substring");
    assert_eq!(names(&alias_only), vec!["project-x".to_string()]);
    assert_eq!(alias_only[0].session_count, 1);

    // `%` is a literal character, not a wildcard: nothing is called `%`.
    let literal_percent = fixture
        .store
        .entities_filtered(EntityFilter {
            kind: None,
            query: Some("%".to_string()),
            limit: 1_000,
        })
        .await
        .expect("filter by a percent sign");
    assert!(literal_percent.is_empty(), "`%` is not a glob");

    // `_` is the other `LIKE` metacharacter and is likewise literal: no name contains one,
    // so a single `_` matches nothing rather than "any single character".
    let literal_underscore = fixture
        .store
        .entities_filtered(EntityFilter {
            kind: None,
            query: Some("_".to_string()),
            limit: 1_000,
        })
        .await
        .expect("filter by an underscore");
    assert!(literal_underscore.is_empty(), "`_` is not a glob");

    let domains = fixture
        .store
        .entities_filtered(EntityFilter {
            kind: Some(EntityKind::Domain.as_str().to_string()),
            query: Some("git".to_string()),
            limit: 1_000,
        })
        .await
        .expect("filter by kind and substring");
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].canonical_name, "github.com");
    assert_eq!(domains[0].kind, "domain");
}

// ---- Generator inputs ------------------------------------------------------------------

/// One row per thread × member session overlapping the window, an open session included via
/// `window_end`, and a session that ended before the window excluded.
#[tokio::test]
async fn insights_thread_activity_input_rows() {
    let fixture = Fixture::new("thread-activity-input");
    let ended_before = session(&fixture.store, 0, Some(500)).await;
    let closed = session(&fixture.store, 1_000, Some(2_000)).await;
    let open = session(&fixture.store, 1_500, None).await;
    let other = session(&fixture.store, 2_500, Some(2_600)).await;
    seed_threads(
        &fixture.store,
        &[
            ThreadSeed {
                cluster_key: "alpha",
                title: "Alpha",
                first_seen_at: 0,
                last_seen_at: 2_000,
                members: vec![ended_before, closed, open],
            },
            ThreadSeed {
                cluster_key: "beta",
                title: "Beta",
                first_seen_at: 2_500,
                last_seen_at: 2_600,
                members: vec![other],
            },
        ],
    )
    .await;
    let alpha = stored_thread_id(&fixture.store, "alpha").await;
    let beta = stored_thread_id(&fixture.store, "beta").await;

    let rows = fixture
        .store
        .insight_thread_activity(1_200, 3_000)
        .await
        .expect("thread activity input");
    assert_eq!(
        rows,
        vec![
            ThreadActivityInput {
                thread_id: alpha,
                title: "Alpha".to_string(),
                session_id: closed,
                started_at: 1_000,
                ended_at: Some(2_000),
            },
            ThreadActivityInput {
                thread_id: alpha,
                title: "Alpha".to_string(),
                session_id: open,
                started_at: 1_500,
                ended_at: None,
            },
            ThreadActivityInput {
                thread_id: beta,
                title: "Beta".to_string(),
                session_id: other,
                started_at: 2_500,
                ended_at: Some(2_600),
            },
        ],
        "ordered thread_id then started_at; the session that ended before the window is absent"
    );
}

/// Only marks still open and created before the window's end are read, ordered by creation.
#[tokio::test]
async fn insights_open_marks_input_rows() {
    let fixture = Fixture::new("open-marks-input");
    let first_frame = frame(&fixture.store, 500).await;
    let second_frame = frame(&fixture.store, 1_500).await;
    let third_frame = frame(&fixture.store, 4_000).await;

    let open_with_note = fixture
        .store
        .insert_mark(
            first_frame,
            1_000,
            Some("send the revised invoice".to_string()),
        )
        .await
        .expect("insert mark with a note");
    let resolved = fixture
        .store
        .insert_mark(second_frame, 1_200, Some("already handled".to_string()))
        .await
        .expect("insert resolvable mark");
    fixture
        .store
        .resolve_mark(resolved, 1_300)
        .await
        .expect("resolve the mark");
    let open_without_note = fixture
        .store
        .insert_mark(second_frame, 1_500, None)
        .await
        .expect("insert note-less mark");
    let after_window = fixture
        .store
        .insert_mark(third_frame, 5_000, Some("later".to_string()))
        .await
        .expect("insert mark after the window");

    let rows = fixture
        .store
        .insight_open_marks(3_000)
        .await
        .expect("open marks input");
    assert_eq!(
        rows,
        vec![
            OpenMarkInput {
                mark_id: open_with_note,
                frame_id: first_frame,
                created_at: 1_000,
                note: Some("send the revised invoice".to_string()),
            },
            OpenMarkInput {
                mark_id: open_without_note,
                frame_id: second_frame,
                created_at: 1_500,
                note: None,
            },
        ],
        "the resolved mark and the mark created after window_end are absent"
    );
    assert!(!rows.iter().any(|row| row.mark_id == after_window));
}

/// Entity/session links for the sessions overlapping the window, ordered `entity_id` then
/// `started_at`, with the entity's stored display data.
#[tokio::test]
async fn insights_entity_sessions_input_rows() {
    let fixture = Fixture::new("entity-sessions-input");
    let outside = session(&fixture.store, 0, Some(500)).await;
    let first = session(&fixture.store, 1_000, Some(2_000)).await;
    let second = session(&fixture.store, 2_500, Some(3_500)).await;

    hit(
        &fixture.store,
        outside,
        100,
        EntityKind::Repo,
        "alpha",
        0.5,
        &[],
    )
    .await;
    hit(
        &fixture.store,
        first,
        1_200,
        EntityKind::Repo,
        "alpha",
        0.6,
        &[],
    )
    .await;
    hit(
        &fixture.store,
        second,
        2_600,
        EntityKind::Repo,
        "alpha",
        0.7,
        &[],
    )
    .await;
    hit(
        &fixture.store,
        second,
        2_700,
        EntityKind::App,
        "beta",
        0.8,
        &[],
    )
    .await;

    let entity_id = |conn: &Connection, name: &str| -> i64 {
        conn.query_row(
            "SELECT id FROM entities WHERE canonical_name = ?1",
            [name],
            |row| row.get(0),
        )
        .expect("read the seeded entity")
    };
    let raw = fixture.raw();
    let alpha = entity_id(&raw, "alpha");
    let beta = entity_id(&raw, "beta");
    assert!(alpha < beta, "alpha was created first");

    let rows = fixture
        .store
        .insight_entity_sessions(900, 4_000)
        .await
        .expect("entity sessions input");
    assert_eq!(
        rows,
        vec![
            EntitySessionInput {
                entity_id: alpha,
                entity_kind: "repo".to_string(),
                canonical_name: "alpha".to_string(),
                session_id: first,
                session_started_at: 1_000,
                last_seen_at: 2_600,
            },
            EntitySessionInput {
                entity_id: alpha,
                entity_kind: "repo".to_string(),
                canonical_name: "alpha".to_string(),
                session_id: second,
                session_started_at: 2_500,
                last_seen_at: 2_600,
            },
            EntitySessionInput {
                entity_id: beta,
                entity_kind: "app".to_string(),
                canonical_name: "beta".to_string(),
                session_id: second,
                session_started_at: 2_500,
                last_seen_at: 2_700,
            },
        ],
        "ordered entity_id then session started_at; the out-of-window session is absent"
    );
}

/// The KNN read: `k` neighbours per in-window session that has a vector, self excluded,
/// nearest first, with both sessions' display data; `k = 0` reads nothing.
#[tokio::test]
async fn insights_session_neighbours_input_rows() {
    let fixture = Fixture::new("session-neighbours-input");
    let alpha = titled_session(
        &fixture.store,
        1_000,
        Some(2_000),
        "Postgres indexing",
        "vacuum",
    )
    .await;
    let beta = titled_session(
        &fixture.store,
        2_000,
        Some(3_000),
        "Postgres vacuum",
        "autovacuum",
    )
    .await;
    let gamma = titled_session(
        &fixture.store,
        3_000,
        Some(4_000),
        "Invoice model",
        "billing",
    )
    .await;
    // No vector: never a subject.
    let unembedded =
        titled_session(&fixture.store, 1_500, Some(2_500), "Untracked", "no vector").await;

    seed_vector(&fixture.store, alpha, "Postgres indexing", "vacuum", 0).await;
    seed_vector(&fixture.store, beta, "Postgres vacuum", "autovacuum", 0).await;
    seed_vector(&fixture.store, gamma, "Invoice model", "billing", 1).await;

    assert!(fixture
        .store
        .insight_session_neighbours(500, 4_500, 0)
        .await
        .expect("zero neighbours reads nothing")
        .is_empty());

    let rows = fixture
        .store
        .insight_session_neighbours(500, 4_500, 2)
        .await
        .expect("neighbour input");
    let pairs: Vec<(i64, i64)> = rows
        .iter()
        .map(|row| (row.session_id, row.neighbour_session_id))
        .collect();
    assert_eq!(
        pairs,
        vec![
            (alpha, beta),
            (alpha, gamma),
            (beta, alpha),
            (beta, gamma),
            (gamma, alpha),
            (gamma, beta),
        ],
        "one row per (subject, neighbour), subject ascending, nearest first"
    );
    assert!(
        !pairs.iter().any(|(subject, neighbour)| subject == neighbour
            || *subject == unembedded
            || *neighbour == unembedded),
        "self and unembedded sessions never appear"
    );

    // Derived from the same values the fixture seeded, so a mismatch means the read lost or
    // invented display data.
    let seeded: HashMap<i64, (Option<String>, i64)> = [
        (alpha, ("Postgres indexing", 1_000)),
        (beta, ("Postgres vacuum", 2_000)),
        (gamma, ("Invoice model", 3_000)),
    ]
    .into_iter()
    .map(|(id, (title, started_at))| (id, (Some(title.to_string()), started_at)))
    .collect();
    for row in &rows {
        let subject = seeded
            .get(&row.session_id)
            .expect("the subject is a seeded session");
        assert_eq!(&row.session_title, &subject.0);
        assert_eq!(
            row.session_started_at, subject.1,
            "the subject's own timestamp is carried"
        );
        let neighbour = seeded
            .get(&row.neighbour_session_id)
            .expect("the neighbour is a seeded session");
        assert_eq!(&row.neighbour_title, &neighbour.0);
        assert_eq!(row.neighbour_started_at, neighbour.1);
        let involves_gamma = row.session_id == gamma || row.neighbour_session_id == gamma;
        let expected = if involves_gamma { 1.0 } else { 0.0 };
        assert!(
            (row.distance - expected).abs() < 1e-6,
            "one-hot distance for {row:?} should be {expected}"
        );
    }

    // `k = 1` keeps exactly one neighbour per subject. alpha and beta are cosine-identical,
    // so their nearest is unambiguous; gamma's two candidates are exactly equidistant, and
    // which tied row vec0 hands back inside `k` is unspecified — so only "one of them" is a
    // contract, not a particular one.
    let nearest = fixture
        .store
        .insight_session_neighbours(500, 4_500, 1)
        .await
        .expect("single neighbour input");
    assert_eq!(
        nearest.len(),
        3,
        "one neighbour per in-window embedded session"
    );
    let neighbour_of = |subject: i64| -> i64 {
        nearest
            .iter()
            .find(|row| row.session_id == subject)
            .expect("every subject has a neighbour")
            .neighbour_session_id
    };
    assert_eq!(neighbour_of(alpha), beta);
    assert_eq!(neighbour_of(beta), alpha);
    let gamma_neighbour = neighbour_of(gamma);
    assert!(
        gamma_neighbour == alpha || gamma_neighbour == beta,
        "gamma's nearest is one of its equidistant candidates"
    );

    // A session outside the window is never a subject, even with a vector: add one whose
    // interval closes before the window opens and re-read.
    let outside = titled_session(
        &fixture.store,
        0,
        Some(400),
        "Old work",
        "before the window",
    )
    .await;
    seed_vector(&fixture.store, outside, "Old work", "before the window", 2).await;
    let after = fixture
        .store
        .insight_session_neighbours(500, 4_500, 2)
        .await
        .expect("neighbour input after an out-of-window vector");
    assert!(
        after.iter().all(|row| row.session_id != outside),
        "an out-of-window session is not a subject"
    );
}

/// Every member session of every thread overlapping the window, including a member older than
/// the window, ordered `thread_id` then `started_at`.
#[tokio::test]
async fn insights_thread_sessions_input_rows() {
    let fixture = Fixture::new("thread-sessions-input");
    let old = titled_session(
        &fixture.store,
        1_000,
        Some(2_000),
        "Kickoff",
        "scope agreed",
    )
    .await;
    let resumed = titled_session(
        &fixture.store,
        8_000,
        None,
        "Kickoff resumed",
        "implemented export",
    )
    .await;
    let unrelated =
        titled_session(&fixture.store, 20_000, Some(21_000), "Other", "elsewhere").await;
    seed_threads(
        &fixture.store,
        &[
            ThreadSeed {
                cluster_key: "kickoff",
                title: "Kickoff",
                first_seen_at: 1_000,
                last_seen_at: 8_000,
                members: vec![resumed, old],
            },
            ThreadSeed {
                cluster_key: "unrelated",
                title: "Unrelated",
                first_seen_at: 20_000,
                last_seen_at: 21_000,
                members: vec![unrelated],
            },
        ],
    )
    .await;
    let kickoff = stored_thread_id(&fixture.store, "kickoff").await;

    let rows = fixture
        .store
        .insight_thread_sessions(7_000, 9_000)
        .await
        .expect("thread sessions input");
    assert_eq!(
        rows,
        vec![
            ThreadSessionInput {
                thread_id: kickoff,
                title: "Kickoff".to_string(),
                session_id: old,
                started_at: 1_000,
                ended_at: Some(2_000),
                session_title: Some("Kickoff".to_string()),
                summary: Some("scope agreed".to_string()),
            },
            ThreadSessionInput {
                thread_id: kickoff,
                title: "Kickoff".to_string(),
                session_id: resumed,
                started_at: 8_000,
                ended_at: None,
                session_title: Some("Kickoff resumed".to_string()),
                summary: Some("implemented export".to_string()),
            },
        ],
        "the older member is carried for the gap, the non-overlapping thread is absent"
    );
}

/// A reader must not be told a claim is fresh once the row it cites has gone (`03 §7j.2`):
/// staleness is a property of the data at read time, not of whichever sweep ran last.
///
/// The generation sweep only runs on an idle/level-0 pass, and evidence rows carry no foreign
/// key to the table they name, so a session (or a purged frame, or a recomputed thread) can
/// disappear long before the next pass. Reading the stored flag alone would report
/// `stale: false` for a statement whose evidence no longer exists.
#[tokio::test]
async fn insights_read_computes_staleness_from_surviving_evidence() {
    let fixture = Fixture::new("stale-at-read");
    let kept = session(&fixture.store, 1_000, Some(2_000)).await;
    let doomed = session(&fixture.store, 3_000, Some(4_000)).await;

    let insight = fixture
        .store
        .insert_insight(new_insight(
            (500, 5_000),
            recurrence_payload(1),
            evidence(&[
                (EvidenceKind::Session, kept),
                (EvidenceKind::Session, doomed),
            ]),
        ))
        .await
        .expect("insert evidenced insight");

    let before = fixture
        .store
        .list_insights(all_insights())
        .await
        .expect("read before the deletion");
    assert_eq!(before.len(), 1);
    assert!(!before[0].stale, "every citation still resolves");

    assert!(
        fixture
            .store
            .delete_unfrozen_session(doomed)
            .await
            .expect("delete the cited session"),
        "the cited session existed"
    );

    let after = fixture
        .store
        .list_insights(all_insights())
        .await
        .expect("read after the deletion");
    assert_eq!(
        after.len(),
        1,
        "the row is still returned, flagged rather than hidden"
    );
    assert!(
        after[0].stale,
        "a claim citing a deleted session is stale even though no sweep has run yet"
    );

    // The read is not a write: the persisted flag is untouched, and the sweep agrees with the
    // reader about which rows are unverifiable.
    let raw = fixture.raw();
    let stored: i64 = raw
        .query_row("SELECT stale FROM insights WHERE id=?1", [insight], |row| {
            row.get(0)
        })
        .expect("read the stored flag");
    assert_eq!(stored, 0, "a GET must not rewrite the stored flag");
    assert_eq!(
        fixture
            .store
            .mark_insights_stale_where_evidence_missing()
            .await
            .expect("sweep"),
        1,
        "the sweep marks exactly the row the reader already called stale"
    );
}

// ---- P9 thread figures, graph reads and insight reads ----------------------------------

/// P9 grant 4: every thread read fills `active_ms` (the sum over member sessions of
/// `max(0, COALESCE(ended_at, now_ms) − started_at)`) and `confidence`
/// (`MAX(thread_sessions.confidence)`), and a thread with no member rows reads `0`/`0.0`.
///
/// The open session's contribution is the database clock at read time, so the expectation is
/// the closed session's exact span plus a bound taken from two reads of that same clock; the
/// clock-skewed session proves `max(0, …)` instead of a negative duration.
#[tokio::test]
async fn insights_thread_reads_fill_active_ms_and_confidence() {
    let fixture = Fixture::new("thread-active-ms");
    let store = &fixture.store;
    let raw = fixture.raw();
    let clock = |conn: &Connection| -> i64 {
        conn.query_row("SELECT unixepoch()*1000", [], |row| row.get(0))
            .expect("read the database clock")
    };
    let now_before = clock(&raw);

    // A closed session contributes exactly `ended_at - started_at`.
    let closed = session(store, 10_000, Some(14_000)).await;
    // An open session contributes up to the database clock.
    let open_started_at = now_before - 3_000;
    let open = session(store, open_started_at, None).await;
    // A session whose clock runs ahead contributes nothing at all.
    let skewed = session(store, now_before + 60_000, None).await;

    let with_members = "thread-with-members";
    let without_members = "thread-without-members";
    store
        .graph_apply_thread_plan(&ThreadPlan {
            threads: vec![
                ThreadDraft {
                    cluster_key: with_members.to_string(),
                    title: "Deploys".to_string(),
                    first_seen_at: 10_000,
                    last_seen_at: 14_000,
                    sessions: vec![
                        ThreadMemberDraft {
                            session_id: closed,
                            confidence: 0.4,
                        },
                        ThreadMemberDraft {
                            session_id: open,
                            confidence: 0.9,
                        },
                        ThreadMemberDraft {
                            session_id: skewed,
                            confidence: 0.7,
                        },
                    ],
                },
                // An emptied merge source: a real row with no members.
                ThreadDraft {
                    cluster_key: without_members.to_string(),
                    title: "Emptied".to_string(),
                    first_seen_at: 10_000,
                    last_seen_at: 20_000,
                    sessions: Vec::new(),
                },
            ],
            delete_thread_ids: Vec::new(),
            override_updates: Vec::new(),
        })
        .await
        .expect("apply the thread plan");
    let with_members = stored_thread_id(store, with_members).await;
    let without_members = stored_thread_id(store, without_members).await;

    let listed = store
        .threads_in_range(ThreadFilter {
            from_ms: None,
            to_ms: None,
            limit: 100,
        })
        .await
        .expect("read the thread list");
    let now_after = clock(&raw);

    let closed_span = 14_000 - 10_000;
    let lower = closed_span + (now_before - open_started_at);
    let upper = closed_span + (now_after - open_started_at);
    let busy = listed
        .iter()
        .find(|thread| thread.id == with_members)
        .expect("the thread with members is listed");
    assert!(
        (lower..=upper).contains(&busy.active_ms),
        "active_ms {} must lie in [{lower}, {upper}]: the closed span plus the open session \
         counted to the database clock, and nothing for the clock-skewed one",
        busy.active_ms
    );
    assert_eq!(
        busy.confidence, 0.9,
        "confidence is the maximum member confidence, not the last one written"
    );
    assert_eq!(busy.session_count, 3, "the persisted count is unchanged");

    let empty = listed
        .iter()
        .find(|thread| thread.id == without_members)
        .expect("the emptied thread is listed");
    assert_eq!(
        empty.active_ms, 0,
        "a thread with no members spends no time"
    );
    assert_eq!(empty.confidence, 0.0, "and has no confidence to report");

    // The detail read uses the same aggregate: it cannot report different figures.
    let detail = store
        .thread_detail(with_members)
        .await
        .expect("read the thread detail")
        .expect("the thread exists");
    assert_eq!(detail.thread.confidence, busy.confidence);
    assert!(
        (lower..=upper).contains(&detail.thread.active_ms),
        "the detail's active_ms {} must lie in [{lower}, {upper}]",
        detail.thread.active_ms
    );
    let empty_detail = store
        .thread_detail(without_members)
        .await
        .expect("read the emptied thread detail")
        .expect("the thread exists");
    assert_eq!(empty_detail.thread.active_ms, 0);
    assert_eq!(empty_detail.thread.confidence, 0.0);
}

/// Enqueues one graph job that is ready now.
async fn enqueue_graph_job(store: &SqliteStore, kind: JobKind) -> i64 {
    store
        .enqueue_job(NewJob {
            kind,
            frame_id: None,
            priority: 0,
            max_attempts: 3,
            not_before: 0,
        })
        .await
        .expect("enqueue a graph job")
}

/// `graph_progress` counts every session, the distinct clustered ones, and reads the backlog
/// from the same grouped jobs query as `graph_backfill_status` — including `graph_normalize`,
/// which `running` covers but no counter reports.
#[tokio::test]
async fn graph_progress_counts_sessions_and_the_queue_backlog() {
    let fixture = Fixture::new("graph-progress");
    let store = &fixture.store;

    let empty = store.graph_progress().await.expect("read progress");
    assert_eq!((empty.sessions_total, empty.sessions_clustered), (0, 0));
    assert!(!empty.running, "an idle queue is not running");
    assert_eq!(
        (
            empty.pending_extract,
            empty.pending_backfill,
            empty.pending_cluster
        ),
        (0, 0, 0)
    );

    let first = session(store, 1_000, Some(2_000)).await;
    let second = session(store, 3_000, Some(4_000)).await;
    let unclustered = session(store, 5_000, Some(6_000)).await;
    let sessions = [first, second, unclustered];
    seed_threads(
        store,
        &[
            ThreadSeed {
                cluster_key: "progress-one",
                title: "One",
                first_seen_at: 1_000,
                last_seen_at: 2_000,
                members: vec![first],
            },
            ThreadSeed {
                cluster_key: "progress-two",
                title: "Two",
                first_seen_at: 3_000,
                last_seen_at: 4_000,
                members: vec![second],
            },
        ],
    )
    .await;

    let progress = store.graph_progress().await.expect("read progress");
    assert_eq!(
        progress.sessions_total,
        sessions.len() as u64,
        "every session row counts, clustered or not"
    );
    assert_eq!(
        progress.sessions_clustered, 2,
        "one session per thread is clustered; the third is not"
    );
    assert!(!progress.running);

    // A pending normalize job is graph work even though it has no counter of its own.
    let normalize = enqueue_graph_job(store, JobKind::GraphNormalize).await;
    let progress = store.graph_progress().await.expect("read progress");
    assert!(
        progress.running,
        "a pending graph_normalize job means running"
    );
    assert_eq!(
        (
            progress.pending_extract,
            progress.pending_backfill,
            progress.pending_cluster
        ),
        (0, 0, 0),
        "normalize is not one of the three reported counters"
    );

    // Extract, backfill and cluster each land in their own counter.
    let extract = enqueue_graph_job(store, JobKind::GraphExtract).await;
    enqueue_graph_job(store, JobKind::GraphBackfill).await;
    enqueue_graph_job(store, JobKind::GraphCluster).await;
    let progress = store.graph_progress().await.expect("read progress");
    assert_eq!(
        (
            progress.pending_extract,
            progress.pending_backfill,
            progress.pending_cluster
        ),
        (1, 1, 1)
    );

    // A claimed (running) job is still backlog: `pending_*` and `running` cover both states.
    let running = store
        .claim_jobs(&[JobKind::GraphCluster], 1, 0)
        .await
        .expect("claim the cluster job");
    assert_eq!(running.len(), 1);
    let running_id = running[0].id;
    let progress = store.graph_progress().await.expect("read progress");
    assert_eq!(
        progress.pending_cluster, 1,
        "a running job still counts as backlog"
    );
    assert!(progress.running);

    // Draining the queue clears the flag: `running` is the backlog, not retained state.
    let graph_kinds = [
        JobKind::GraphExtract,
        JobKind::GraphBackfill,
        JobKind::GraphCluster,
        JobKind::GraphNormalize,
    ];
    loop {
        let claimed = store
            .claim_jobs(&graph_kinds, 1, 0)
            .await
            .expect("claim a graph job");
        let Some(job) = claimed.into_iter().next() else {
            break;
        };
        store.complete_job(job.id).await.expect("complete the job");
    }
    // The claimed job is not claimable again, so it is completed by id.
    store
        .complete_job(running_id)
        .await
        .expect("complete the running job");
    let progress = store.graph_progress().await.expect("read progress");
    assert!(!progress.running, "a drained queue is idle again");
    assert_eq!(
        (
            progress.pending_extract,
            progress.pending_backfill,
            progress.pending_cluster
        ),
        (0, 0, 0)
    );
    // The variables exist so the enqueues above cannot be optimised away by a reader.
    let _ = (normalize, extract);
}

/// `threads_for_entity` lists each thread once, ordered `last_seen_at DESC`, bounded by
/// `limit` — the Work list's `?entity=` filter.
#[tokio::test]
async fn graph_threads_for_entity_lists_threads_touching_it() {
    let fixture = Fixture::new("threads-for-entity");
    let store = &fixture.store;
    // Three sessions in three threads; two of them touch the shared entity.
    let early = session(store, 1_000, Some(2_000)).await;
    let late = session(store, 3_000, Some(4_000)).await;
    let unrelated = session(store, 5_000, Some(6_000)).await;
    hit(
        store,
        early,
        1_000,
        EntityKind::Repo,
        "screensearch",
        0.9,
        &["ss"],
    )
    .await;
    hit(
        store,
        late,
        3_000,
        EntityKind::Repo,
        "screensearch",
        0.8,
        &[],
    )
    .await;
    hit(store, unrelated, 5_000, EntityKind::App, "codex", 0.7, &[]).await;
    let entity_id = |name: &str| {
        let name = name.to_string();
        async move {
            store
                .entities_filtered(EntityFilter {
                    kind: None,
                    query: Some(name),
                    limit: 10,
                })
                .await
                .expect("find the seeded entity")
                .first()
                .expect("the entity exists")
                .id
        }
    };
    let repo = entity_id("screensearch").await;
    let app = entity_id("codex").await;

    seed_threads(
        store,
        &[
            ThreadSeed {
                cluster_key: "entity-early",
                title: "Early",
                first_seen_at: 1_000,
                last_seen_at: 2_000,
                members: vec![early],
            },
            ThreadSeed {
                cluster_key: "entity-late",
                title: "Late",
                first_seen_at: 3_000,
                last_seen_at: 4_000,
                members: vec![late],
            },
            ThreadSeed {
                cluster_key: "entity-unrelated",
                title: "Unrelated",
                first_seen_at: 5_000,
                last_seen_at: 6_000,
                members: vec![unrelated],
            },
        ],
    )
    .await;
    let early_thread = stored_thread_id(store, "entity-early").await;
    let late_thread = stored_thread_id(store, "entity-late").await;
    let unrelated_thread = stored_thread_id(store, "entity-unrelated").await;

    let threads = store
        .threads_for_entity(repo, 10)
        .await
        .expect("read the threads of the entity");
    assert_eq!(
        threads.iter().map(|thread| thread.id).collect::<Vec<_>>(),
        vec![late_thread, early_thread],
        "newest `last_seen_at` first, and each thread once"
    );
    assert!(
        threads
            .iter()
            .all(|thread| thread.session_count == 1 && thread.confidence == MEMBER_CONFIDENCE),
        "the rows carry the same figures as the thread list"
    );

    let narrowed = store
        .threads_for_entity(repo, 1)
        .await
        .expect("read one thread");
    assert_eq!(
        narrowed.iter().map(|thread| thread.id).collect::<Vec<_>>(),
        vec![late_thread],
        "the limit cuts the tail of the order"
    );

    let app_threads = store
        .threads_for_entity(app, 10)
        .await
        .expect("read the app's threads");
    assert_eq!(
        app_threads
            .iter()
            .map(|thread| thread.id)
            .collect::<Vec<_>>(),
        vec![unrelated_thread]
    );
}

/// `connected_threads` orders by shared-entity count descending, then `last_seen_at`
/// descending, and carries the shared entity ids as the evidence of the connection.
#[tokio::test]
async fn graph_connected_threads_order_by_shared_count_then_recency() {
    let fixture = Fixture::new("connected-threads");
    let store = &fixture.store;
    let anchor = session(store, 1_000, Some(2_000)).await;
    let few_newer = session(store, 3_000, Some(4_000)).await;
    let few_older = session(store, 5_000, Some(6_000)).await;
    let many_oldest = session(store, 7_000, Some(8_000)).await;
    let nothing_shared = session(store, 9_000, Some(10_000)).await;

    // The anchor touches four subjects.
    for (index, name) in ["deploy", "invoice", "roadmap", "notes"].iter().enumerate() {
        hit(store, anchor, 1_000, EntityKind::Repo, name, 0.9, &[]).await;
        // `index` only keeps the four calls in a readable order.
        assert!(index < 4);
    }
    // Two threads share two subjects each (differing only by recency) …
    for name in ["deploy", "invoice"] {
        hit(store, few_newer, 3_000, EntityKind::Repo, name, 0.9, &[]).await;
        hit(store, few_older, 5_000, EntityKind::Repo, name, 0.9, &[]).await;
    }
    // … and one shares three, though it is the oldest thread of them all.
    for name in ["deploy", "invoice", "notes"] {
        hit(store, many_oldest, 7_000, EntityKind::Repo, name, 0.9, &[]).await;
    }
    // This one shares nothing with the anchor.
    hit(
        store,
        nothing_shared,
        9_000,
        EntityKind::App,
        "codex",
        0.9,
        &[],
    )
    .await;

    let mut names: HashMap<String, i64> = HashMap::new();
    for entity in store
        .entities_filtered(EntityFilter {
            kind: None,
            query: None,
            limit: 100,
        })
        .await
        .expect("list the seeded entities")
    {
        names.insert(entity.canonical_name, entity.id);
    }
    let id_of = |name: &str| *names.get(name).expect("the entity was seeded");

    seed_threads(
        store,
        &[
            ThreadSeed {
                cluster_key: "anchor",
                title: "Anchor",
                first_seen_at: 1_000,
                last_seen_at: 2_000,
                members: vec![anchor],
            },
            ThreadSeed {
                cluster_key: "few-newer",
                title: "Few newer",
                first_seen_at: 3_000,
                last_seen_at: 30_000,
                members: vec![few_newer],
            },
            ThreadSeed {
                cluster_key: "few-older",
                title: "Few older",
                first_seen_at: 5_000,
                last_seen_at: 20_000,
                members: vec![few_older],
            },
            ThreadSeed {
                cluster_key: "many-oldest",
                title: "Many oldest",
                first_seen_at: 7_000,
                last_seen_at: 10_000,
                members: vec![many_oldest],
            },
            ThreadSeed {
                cluster_key: "nothing-shared",
                title: "Nothing shared",
                first_seen_at: 9_000,
                last_seen_at: 40_000,
                members: vec![nothing_shared],
            },
        ],
    )
    .await;
    let anchor_thread = stored_thread_id(store, "anchor").await;
    let few_newer_thread = stored_thread_id(store, "few-newer").await;
    let few_older_thread = stored_thread_id(store, "few-older").await;
    let many_oldest_thread = stored_thread_id(store, "many-oldest").await;

    let connected = store
        .connected_threads(anchor_thread, 10)
        .await
        .expect("read the connections");
    assert_eq!(
        connected
            .iter()
            .map(|row| row.thread.id)
            .collect::<Vec<_>>(),
        vec![many_oldest_thread, few_newer_thread, few_older_thread],
        "three shared subjects win over recency; the tie then breaks on `last_seen_at`"
    );
    assert_eq!(
        connected[0].shared_entity_ids,
        vec![id_of("notes"), id_of("deploy"), id_of("invoice")]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        "the shared ids are the evidence, ascending"
    );
    assert_eq!(
        connected[1].shared_entity_ids, connected[2].shared_entity_ids,
        "the two two-subject threads name the same subjects"
    );
    assert_eq!(connected[1].shared_entity_ids.len(), 2);

    let narrowed = store
        .connected_threads(anchor_thread, 1)
        .await
        .expect("read one connection");
    assert_eq!(
        narrowed.iter().map(|row| row.thread.id).collect::<Vec<_>>(),
        vec![many_oldest_thread],
        "the limit applies to the ranked order"
    );

    // A thread nothing shares with has no connections, and an unknown id has none either.
    assert!(store
        .connected_threads(few_older_thread, 10)
        .await
        .expect("read the isolated thread's connections")
        .iter()
        .all(|row| row.thread.id != few_older_thread));
    assert!(store
        .connected_threads(999_999, 10)
        .await
        .expect("read an unknown thread")
        .is_empty());
}

/// `thread_merge_target` is the latest `merge` override naming the thread as its source, and
/// `None` for a thread that was never merged away — a rename in between changes nothing.
#[tokio::test]
async fn graph_thread_merge_target_reads_the_latest_merge_override() {
    let fixture = Fixture::new("merge-target");
    let store = &fixture.store;
    let source_session = session(&fixture.store, 1_000, Some(2_000)).await;
    let survivor_session = session(&fixture.store, 3_000, Some(4_000)).await;
    let later_session = session(&fixture.store, 5_000, Some(6_000)).await;
    seed_threads(
        store,
        &[
            ThreadSeed {
                cluster_key: "merge-source",
                title: "Source",
                first_seen_at: 1_000,
                last_seen_at: 2_000,
                members: vec![source_session],
            },
            ThreadSeed {
                cluster_key: "merge-survivor",
                title: "Survivor",
                first_seen_at: 3_000,
                last_seen_at: 4_000,
                members: vec![survivor_session],
            },
            ThreadSeed {
                cluster_key: "merge-later",
                title: "Later",
                first_seen_at: 5_000,
                last_seen_at: 6_000,
                members: vec![later_session],
            },
        ],
    )
    .await;
    let source = stored_thread_id(store, "merge-source").await;
    let survivor = stored_thread_id(store, "merge-survivor").await;
    let later = stored_thread_id(store, "merge-later").await;

    assert_eq!(
        store
            .thread_merge_target(source)
            .await
            .expect("read target"),
        None,
        "a thread that was never merged away has no survivor"
    );

    store
        .graph_insert_thread_override(
            &ThreadOverrideDraft {
                kind: ThreadOverrideKind::Merge,
                thread_id: Some(source),
                target_thread_id: Some(survivor),
                payload: ThreadOverridePayload::Merge {
                    session_ids: vec![source_session],
                },
            },
            1_000,
        )
        .await
        .expect("insert the merge override");
    // An unrelated rename on the same thread must not become the answer.
    store
        .graph_insert_thread_override(
            &ThreadOverrideDraft {
                kind: ThreadOverrideKind::Rename,
                thread_id: Some(source),
                target_thread_id: None,
                payload: ThreadOverridePayload::Rename {
                    title: "Renamed".to_string(),
                },
            },
            2_000,
        )
        .await
        .expect("insert the rename override");

    assert_eq!(
        store
            .thread_merge_target(source)
            .await
            .expect("read target"),
        Some(survivor)
    );
    assert_eq!(
        store
            .thread_merge_target(survivor)
            .await
            .expect("read the survivor's target"),
        None,
        "the survivor was not merged away"
    );

    // A later merge of the same source to another thread is now the latest row.
    store
        .graph_insert_thread_override(
            &ThreadOverrideDraft {
                kind: ThreadOverrideKind::Merge,
                thread_id: Some(source),
                target_thread_id: Some(later),
                payload: ThreadOverridePayload::Merge {
                    session_ids: vec![source_session],
                },
            },
            3_000,
        )
        .await
        .expect("insert the later merge override");
    assert_eq!(
        store
            .thread_merge_target(source)
            .await
            .expect("read target"),
        Some(later),
        "the latest merge row wins on `(created_at, id)`"
    );
}

/// `insights_for_evidence` matches any of the named `(kind, ref_id)` pairs, orders like
/// `list_insights`, honours the kind filter, and returns exactly what `list_insights` would —
/// including the read-time `stale` flag once the cited row is gone.
#[tokio::test]
async fn insights_for_evidence_matches_refs_and_shares_the_stale_rule() {
    let fixture = Fixture::new("insights-for-evidence");
    let store = &fixture.store;
    let cited_session = session(store, 1_000, Some(2_000)).await;
    let kept_session = session(store, 3_000, Some(4_000)).await;
    hit(
        store,
        cited_session,
        1_000,
        EntityKind::Repo,
        "screensearch",
        0.9,
        &[],
    )
    .await;
    let entity_id = store
        .entities_filtered(EntityFilter {
            kind: None,
            query: Some("screensearch".to_string()),
            limit: 10,
        })
        .await
        .expect("find the entity")
        .first()
        .expect("the entity exists")
        .id;
    seed_threads(
        store,
        &[ThreadSeed {
            cluster_key: "evidence-thread",
            title: "Evidence",
            first_seen_at: 1_000,
            last_seen_at: 4_000,
            members: vec![cited_session, kept_session],
        }],
    )
    .await;
    let thread_id = stored_thread_id(store, "evidence-thread").await;

    // Three kinds, all in one window, each evidenced by a different ref.
    let open_loop = store
        .insert_insight(new_insight(
            (1_000, 5_000),
            open_loop_payload(1),
            evidence(&[(EvidenceKind::Session, cited_session)]),
        ))
        .await
        .expect("insert the open loop");
    let recurrence = store
        .insert_insight(new_insight(
            (1_000, 5_000),
            recurrence_payload(entity_id),
            evidence(&[(EvidenceKind::Entity, entity_id)]),
        ))
        .await
        .expect("insert the recurrence");
    let activity = store
        .insert_insight(new_insight(
            (1_000, 5_000),
            thread_activity_payload(thread_id, "Evidence"),
            evidence(&[(EvidenceKind::Thread, thread_id)]),
        ))
        .await
        .expect("insert the thread activity");

    // One ref matches one insight; the order is the list order (window DESC, kind ASC).
    let by_session = store
        .insights_for_evidence(None, &[(EvidenceKind::Session, cited_session)])
        .await
        .expect("read by session evidence");
    assert_eq!(
        by_session
            .iter()
            .map(|insight| insight.id)
            .collect::<Vec<_>>(),
        vec![open_loop]
    );

    // Several refs union their matches, in the same order `list_insights` uses.
    let two_refs = store
        .insights_for_evidence(
            None,
            &[
                (EvidenceKind::Session, cited_session),
                (EvidenceKind::Thread, thread_id),
            ],
        )
        .await
        .expect("read by two refs");
    assert_eq!(
        two_refs
            .iter()
            .map(|insight| insight.id)
            .collect::<Vec<_>>(),
        vec![open_loop, activity],
        "`open_loop` sorts before `thread_activity` on kind"
    );

    // The kind filter narrows the same set.
    let only_activity = store
        .insights_for_evidence(
            Some(InsightKind::ThreadActivity),
            &[
                (EvidenceKind::Session, cited_session),
                (EvidenceKind::Thread, thread_id),
            ],
        )
        .await
        .expect("read with a kind filter");
    assert_eq!(
        only_activity
            .iter()
            .map(|insight| insight.id)
            .collect::<Vec<_>>(),
        vec![activity]
    );

    // Nothing to match, and a ref nothing cites, are both empty — never the whole table.
    assert!(store
        .insights_for_evidence(None, &[])
        .await
        .expect("read with no refs")
        .is_empty());
    assert!(store
        .insights_for_evidence(None, &[(EvidenceKind::Session, 999_999)])
        .await
        .expect("read with an unmatched ref")
        .is_empty());

    // Deleting a cited row makes the read-time `stale` rule fire, exactly as `list_insights`
    // reports it (both go through the same select).
    assert!(store
        .delete_unfrozen_session(cited_session)
        .await
        .expect("delete the cited session"));
    let stale = store
        .insights_for_evidence(None, &[(EvidenceKind::Session, cited_session)])
        .await
        .expect("read the stale insight");
    assert_eq!(stale.len(), 1, "the evidence row survives its target");
    assert_eq!(stale[0].id, open_loop);
    assert!(stale[0].stale, "a dangling citation reads stale");
    let listed = store
        .list_insights(all_insights())
        .await
        .expect("list every insight");
    let same = listed
        .iter()
        .find(|insight| insight.id == open_loop)
        .expect("the insight is listed");
    assert_eq!(
        (&stale[0].stale, &stale[0].evidence),
        (&same.stale, &same.evidence),
        "the two reads agree on the freshness rule and the evidence"
    );
    let _ = recurrence;
}

/// `insight_window_status` counts the queued insight passes and reports the newest
/// `generated_at`, `None` before anything was generated.
#[tokio::test]
async fn insight_window_status_counts_pending_generation() {
    let fixture = Fixture::new("insight-status");
    let store = &fixture.store;

    let idle = store
        .insight_window_status()
        .await
        .expect("read the idle status");
    assert_eq!(idle.pending_jobs, 0);
    assert_eq!(idle.last_generated_at, None);

    let insight_id = store
        .insert_insight(new_insight(
            (1_000, 2_000),
            recurrence_payload(1),
            evidence(&[(EvidenceKind::Session, 1)]),
        ))
        .await
        .expect("insert an insight");
    let generated_at = store
        .list_insights(all_insights())
        .await
        .expect("list the insight")
        .into_iter()
        .find(|insight| insight.id == insight_id)
        .expect("the insight is listed")
        .generated_at;
    let status = store
        .insight_window_status()
        .await
        .expect("read the status");
    assert_eq!(
        status.last_generated_at,
        Some(generated_at),
        "the newest generated row is reported"
    );
    assert_eq!(status.pending_jobs, 0);

    for _ in 0..2 {
        enqueue_graph_job(store, JobKind::InsightGenerate).await;
    }
    // A different job kind is not part of the insight window's status.
    enqueue_graph_job(store, JobKind::GraphCluster).await;
    let status = store
        .insight_window_status()
        .await
        .expect("read the status");
    assert_eq!(
        status.pending_jobs, 2,
        "only `insight_generate` jobs are counted"
    );
}

/// `entities_by_ids` returns the ids that exist, in `last_seen_at DESC` order with aliases
/// decoded, and silently skips ids that are gone.
#[tokio::test]
async fn entities_by_ids_returns_existing_rows_and_skips_missing() {
    let fixture = Fixture::new("entities-by-ids");
    let store = &fixture.store;
    let anchor = session(store, 1_000, Some(2_000)).await;
    for (captured_at, name, alias) in [
        (1_000, "deploy", "ship"),
        (2_000, "invoice", "bill"),
        (3_000, "roadmap", "plan"),
    ] {
        hit(
            store,
            anchor,
            captured_at,
            EntityKind::Repo,
            name,
            0.9,
            &[alias],
        )
        .await;
    }

    let listed = store
        .entities_filtered(EntityFilter {
            kind: None,
            query: None,
            limit: 100,
        })
        .await
        .expect("list the seeded entities");
    let by_name: HashMap<&str, &EntitySummary> = listed
        .iter()
        .map(|entity| (entity.canonical_name.as_str(), entity))
        .collect();

    let fetched = store
        .entities_by_ids(&[
            by_name["deploy"].id,
            by_name["roadmap"].id,
            by_name["invoice"].id,
        ])
        .await
        .expect("read the entities");
    assert_eq!(
        fetched
            .iter()
            .map(|entity| entity.canonical_name.as_str())
            .collect::<Vec<_>>(),
        vec!["roadmap", "invoice", "deploy"],
        "newest `last_seen_at` first, whatever order the ids arrived in"
    );
    for entity in &fetched {
        let expected = by_name[entity.canonical_name.as_str()];
        assert_eq!(entity.aliases, expected.aliases, "aliases are decoded");
        assert_eq!(entity.session_count, expected.session_count);
        assert_eq!(entity.first_seen_at, expected.first_seen_at);
    }

    let partial = store
        .entities_by_ids(&[by_name["deploy"].id, 999_999])
        .await
        .expect("read a partly missing list");
    assert_eq!(partial.len(), 1, "a missing id is absent, not fabricated");
    assert_eq!(partial[0].canonical_name, "deploy");

    assert!(store
        .entities_by_ids(&[])
        .await
        .expect("read no ids")
        .is_empty());
}
