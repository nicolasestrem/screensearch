//! Metric math, generator ground truth, the unlabeled entity check and the forward-compatible
//! citation reader - all on synthetic fixtures, with expectations computed from the fixture the
//! test builds (never a hardcoded answer key).

mod common;

use harness::eval::client::{AskStreamReader, FailureClass, HealthInfo, TurnFailure};
use harness::eval::facts::Db;
use harness::eval::gen::{generate_pairs, GenOpts, Generator, Window};
use harness::eval::metrics::{compute, known_fact, normalize, TurnRun};
use harness::eval::report::{self, EntityCheck, ReportInput, RunMeta};
use rusqlite::Connection;
use serde_json::json;

const NOW: i64 = 1_800_000_000_000;

fn generate(path: &std::path::Path, seed: u64, pairs: usize) -> (Db, harness::eval::GenOutcome) {
    let db = Db::open(path).unwrap();
    let outcome = generate_pairs(
        &db,
        &GenOpts {
            seed,
            pairs,
            max_frames: None,
            now_ms: NOW,
        },
    )
    .unwrap();
    (db, outcome)
}

#[test]
fn each_generator_derives_its_ground_truth_from_the_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let (db, outcome) = generate(&path, 1, 300);
    let pairs = &outcome.pairs;

    // --- last-seen: the newest frames carrying the file path.
    let last_seen = pairs
        .iter()
        .find(|p| {
            p.generator == Generator::LastSeen
                && p.expected_strings[0].ends_with("notes00.md")
                && p.expected_strings[0].starts_with("C:\\")
        })
        .expect("last-seen pair for case00's file path");
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT id FROM frames
             WHERE instr(lower(COALESCE(window_title, '')), 'notes00.md') > 0
                OR instr(lower(COALESCE(browser_url, '')), 'notes00.md') > 0
                OR instr(lower(COALESCE((SELECT content_text FROM frame_text ft
                                          WHERE ft.frame_id = frames.id), '')), 'notes00.md') > 0
             ORDER BY captured_at DESC, id DESC LIMIT 10",
        )
        .unwrap();
    let expected_frames: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(last_seen.expected_frames, expected_frames);
    assert_eq!(last_seen.expected_frames[0], expected_frames[0]);
    assert_eq!(last_seen.expected_sessions, vec![1]);
    assert_eq!(
        last_seen.window, None,
        "last-seen is a whole-history question"
    );
    assert_eq!(
        last_seen.expected_local_date.as_deref(),
        Some("2026-10-01"),
        "the local date of the newest matching frame"
    );
    assert_eq!(
        last_seen.question,
        "When did I last have `C:\\work\\case00\\notes00.md` open?"
    );

    // --- involvement: sessions owning frames whose title/URL carries the repository.
    let involvement = pairs
        .iter()
        .find(|p| {
            p.generator == Generator::Involvement
                && p.expected_strings[0] == "github.com/acme/proj00"
        })
        .expect("involvement pair for the repository");
    assert_eq!(involvement.expected_sessions, vec![1]);
    assert_eq!(involvement.expected_frames.len(), 1);
    assert_eq!(
        involvement.question,
        "Which sessions involved `github.com/acme/proj00`?"
    );

    // --- sequence: the session with the greatest started_at below session 5 is session 4.
    let sequence = pairs
        .iter()
        .find(|p| p.generator == Generator::Sequence && p.question.contains("session 5 "))
        .expect("sequence pair targeting session 5");
    assert_eq!(sequence.expected_sessions, vec![4]);
    let z_start: i64 = db
        .conn()
        .query_row("SELECT started_at FROM sessions WHERE id = 5", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        sequence.window,
        Some(Window {
            start: db.history_bounds().unwrap().unwrap().0,
            end: z_start + 1
        })
    );

    // --- window recall: the 2026-10-02 morning holds sessions 4, 5 and 6.
    let recall = pairs
        .iter()
        .find(|p| p.generator == Generator::WindowRecall && p.question.contains("2026-10-02"))
        .expect("window-recall pair for 2026-10-02");
    assert_eq!(recall.expected_sessions, vec![4, 5, 6]);
    let (start, end) = db.local_hour_window("2026-10-02", 6, 12).unwrap();
    assert_eq!(recall.window, Some(Window { start, end }));
    assert_eq!(recall.question, "What did I work on 2026-10-02 morning?");
    assert!(!recall.expected_strings.is_empty());

    // --- seen-again: exactly the FTS match set for the sampled line.
    let seen_again = pairs
        .iter()
        .find(|p| p.generator == Generator::SeenAgain && p.expected_strings[0].contains("run 00"))
        .expect("seen-again pair for case00's line");
    let phrase = format!("\"{}\"", seen_again.expected_strings[0]);
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT f.id FROM frame_text_fts
             JOIN frames f ON f.id = frame_text_fts.rowid
             WHERE frame_text_fts MATCH ?1
             ORDER BY f.captured_at DESC, f.id DESC LIMIT 10",
        )
        .unwrap();
    let expected: Vec<i64> = stmt
        .query_map([phrase], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(seen_again.expected_frames, expected);
    assert_eq!(seen_again.expected_frames.len(), 2);
    assert!(seen_again
        .question
        .starts_with("Where else did this text appear: \""));
}

#[test]
fn the_entity_check_measures_recovery_and_invention_without_labels() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let (db, outcome) = generate(&path, 1, 300);
    assert!(
        !harness::eval::entity_check(&db, &outcome).unwrap().surface,
        "an empty graph has no surface to score"
    );

    // A three-row graph: two names that do appear in the text, one that does not.
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO entities
         (id, kind, canonical_name, normalized_key, source, confidence, first_seen_at, last_seen_at)
         VALUES (1, 'file', 'C:\\work\\case00\\notes00.md', 'c:/work/case00/notes00.md',
                 'heuristic', 0.9, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entities
         (id, kind, canonical_name, normalized_key, source, confidence, first_seen_at, last_seen_at)
         VALUES (2, 'domain', 'site00.example.com', 'site00.example.com', 'heuristic', 0.9, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entities
         (id, kind, canonical_name, normalized_key, source, confidence, first_seen_at, last_seen_at)
         VALUES (3, 'document', 'Mystery Report', 'mystery report', 'heuristic', 0.9, 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entity_aliases (entity_id, alias, source, confidence)
         VALUES (2, 'Site Zero', 'llm', 0.5)",
        [],
    )
    .unwrap();
    drop(conn);

    let check = harness::eval::entity_check(&db, &outcome).unwrap();
    assert!(check.surface);
    assert_eq!(check.entity_rows, 3);
    assert_eq!(check.alias_rows, 1);
    assert_eq!(check.reference_entities, outcome.reference_entities.len());
    assert_eq!(
        check.recovered, 2,
        "the file path and the domain are recoverable"
    );
    assert_eq!(check.invention_checked, 3);
    assert_eq!(
        check.invented, 1,
        "`Mystery Report` appears nowhere in the text"
    );
}

#[test]
fn metrics_over_fixture_derived_pairs_match_their_definitions() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let (db, outcome) = generate(&path, 3, 20);
    let pairs = &outcome.pairs;
    let mut runs = Vec::new();
    for (i, pair) in pairs.iter().enumerate() {
        if i % 2 == 0 {
            // A perfect turn: cite every expected id (all of which the generator derived from the
            // fixture) and answer with the first expected string.
            let frames = db.cited_frames(&pair.expected_frames).unwrap();
            let sessions = db.cited_sessions(&pair.expected_sessions).unwrap();
            let answer = pair
                .expected_strings
                .first()
                .cloned()
                .or_else(|| pair.expected_local_date.clone())
                .unwrap_or_default();
            assert!(
                known_fact(pair, &answer),
                "the fixture answer carries the fact"
            );
            runs.push(TurnRun::success(
                pair,
                answer,
                frames,
                sessions,
                10 + i as u64,
                json!({"type": "done"}),
            ));
        } else {
            runs.push(TurnRun::failed(
                pair,
                TurnFailure {
                    class: FailureClass::Timeout,
                    detail: "synthetic".into(),
                    terminal: None,
                },
            ));
        }
    }

    let m = compute(pairs, &runs);
    let perfect = pairs.iter().enumerate().filter(|(i, _)| i % 2 == 0).count();
    assert_eq!(m.overall.pairs, pairs.len());
    assert_eq!(m.overall.turns_ok, perfect);
    assert_eq!(
        m.overall.failures.get(&FailureClass::Timeout),
        Some(&(pairs.len() - perfect))
    );
    // Every perfect turn scores 1.0; every failed turn scores 0.0.
    let mean = perfect as f64 / pairs.len() as f64;
    assert!((m.overall.evidence_recall10().unwrap() - mean).abs() < 1e-9);
    // The generator's own ground truth is always inside its window: a perfect citation is valid.
    assert_eq!(m.overall.citation_validity(), Some(1.0));
    assert!(m.invalid.is_empty());
    assert_eq!(m.overall.known_hits, perfect);
    assert!((m.overall.known_fact_rate().unwrap() - mean).abs() < 1e-9);
    assert!(m.overall.p50.is_some() && m.overall.p95.is_some());
    // The per-generator rows partition the pair set.
    let per_generator: usize = m.per_generator.iter().map(|g| g.aggregate.pairs).sum();
    assert_eq!(per_generator, pairs.len());
    assert_eq!(m.per_generator.len(), Generator::ALL.len());
}

#[test]
fn a_stubbed_terminal_citations_event_overrides_per_frame_citations() {
    let mut reader = AskStreamReader::default();
    let today = [
        r#"{"type":"token","text":"It was in "}"#,
        r#"{"type":"token","text":"VS Code."}"#,
        r#"{"type":"citation","frame_id":5}"#,
        r#"{"type":"citation","frame_id":6}"#,
        r#"{"type":"done"}"#,
    ];
    for line in today {
        reader.feed(line);
    }
    assert_eq!(reader.citation_frames(), vec![5, 6]);
    assert!(reader.citation_sessions().is_empty());

    // The future shape: one terminal event with both arrays; the per-frame events are ignored.
    let mut reader = AskStreamReader::default();
    for line in [
        r#"{"type":"citation","frame_id":5}"#,
        r#"{"type":"citations","sessions":[1,2],"frames":[7,8],"stage_counts":{"threads":3},"tokens":900}"#,
        r#"{"type":"done","stage_counts":{"threads":3}}"#,
    ] {
        reader.feed(line);
    }
    assert_eq!(reader.citation_frames(), vec![7, 8]);
    assert_eq!(reader.citation_sessions(), vec![1, 2]);
    assert_eq!(reader.terminal().unwrap()["type"], "done");
}

#[test]
fn the_report_prints_five_metrics_an_empty_gate_a_column_and_the_entity_na() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    // The fixture cannot fill 300 pairs, so the report must say so.
    let (db, outcome) = generate(&path, 5, 300);
    let metrics = compute(&outcome.pairs, &[]);
    let entity: EntityCheck = harness::eval::entity_check(&db, &outcome).unwrap();
    assert!(!entity.surface);

    let run_id = "test-run";
    let started = "2026-11-03 10:00:00 +01:00";
    let db_path = path.display().to_string();
    let health = HealthInfo {
        version: "test".into(),
        uptime_secs: 1,
        capturing: true,
    };
    let text = report::render(&ReportInput {
        meta: RunMeta {
            run_id,
            started_local: started,
            seed: 5,
            db_path: &db_path,
            api_base: "http://127.0.0.1:43210",
            health: Some(&health),
            schema_version: Some(14),
            history_local: None,
            now_ms: NOW,
            pairs_requested: 300,
            max_frames: None,
            duration_secs: 0,
            aborted: None,
        },
        generated: &outcome,
        metrics: &metrics,
        runs: &[],
        entity: &entity,
        command_line: "harness eval --db <db> --api <api>",
    });

    for needle in [
        "## Pair generation",
        "## Metrics",
        "| overall |",
        "Evidence recall@10",
        "Retrieval recall@10 (diagnostic)",
        "Citation validity",
        "Known fact present",
        "Latency p50 (ms)",
        "Latency p95 (ms)",
        "Gate A threshold",
        "## Failures",
        "## Invalid citations",
        "n/a (no entity surface)",
        "## Metric definitions (the comparator Gate A freezes)",
        "## Reproduce",
    ] {
        assert!(text.contains(needle), "report is missing {needle:?}");
    }
    // One row per generator plus the overall row.
    for generator in Generator::ALL {
        assert!(
            text.contains(&format!("(`{}`)", generator.tag())),
            "report is missing the {} row",
            generator.tag()
        );
    }
    // The shortfall is stated, not hidden.
    assert!(text.contains("Pair shortfall"));
}

#[test]
fn normalization_makes_the_known_fact_check_case_and_space_insensitive() {
    assert_eq!(normalize("  Alpha\n\tNotes  "), "alpha notes");
    assert!(normalize("I saw ALPHA   notes today").contains(&normalize("Alpha Notes")));
}
