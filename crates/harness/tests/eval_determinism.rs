//! Determinism and data-following for the P4 generators (`docs/kickoffs/P4.md` acceptance:
//! "the generators are deterministic for a given DB and seed (test)" and "the pairs follow the
//! data").

mod common;

use harness::eval::facts::Db;
use harness::eval::gen::{generate_pairs, GenOpts, Generator, Pair};
use rusqlite::{params, Connection};

/// Fixed "now" for generation. The fixture's sessions are all closed, so `now_ms` cannot affect
/// these pairs; a constant keeps the test independent of the wall clock.
const NOW: i64 = 1_800_000_000_000;

fn gen(db: &Db, seed: u64, pairs: usize) -> Vec<Pair> {
    generate_pairs(
        db,
        &GenOpts {
            seed,
            pairs,
            max_frames: None,
            now_ms: NOW,
        },
    )
    .expect("generate pairs")
    .pairs
}

fn jsonl(pairs: &[Pair]) -> String {
    pairs
        .iter()
        .map(|p| serde_json::to_string(p).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn same_database_and_seed_produce_identical_pairs() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let a = gen(&Db::open(&path).unwrap(), 42, 10);
    let b = gen(&Db::open(&path).unwrap(), 42, 10);
    assert!(!a.is_empty(), "the fixture must generate pairs");
    assert_eq!(
        jsonl(&a),
        jsonl(&b),
        "same (db, seed) => byte-identical pairs"
    );
}

#[test]
fn different_seeds_produce_different_pairs() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let db = Db::open(&path).unwrap();
    let a = gen(&db, 42, 10);
    let b = gen(&db, 43, 10);
    assert_eq!(a.len(), b.len(), "sample size is the request, not the seed");
    assert_ne!(
        jsonl(&a),
        jsonl(&b),
        "a seeded sample must actually vary with the seed"
    );
}

/// Inserting a later frame that carries the same reference moves the last-seen ground truth.
#[test]
fn inserting_a_frame_changes_the_last_seen_pair() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let db = Db::open(&path).unwrap();
    let before = gen(&db, 42, 300);
    let target = before
        .iter()
        .find(|p| {
            p.generator == Generator::LastSeen
                && p.expected_strings[0].starts_with("C:\\")
                && p.expected_strings[0].contains("case00")
        })
        .expect("a last-seen pair for case00's file path");
    let title = target.expected_strings[0].clone();

    let conn = Connection::open(&path).unwrap();
    let later: i64 = conn
        .query_row(
            "SELECT unixepoch('2026-10-08 15:00:00', 'utc') * 1000",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO frames
         (id, captured_at, monitor_index, width, height, image_path, content_hash,
          window_title, session_id)
         VALUES (900, ?1, 0, 10, 10, 'f.webp', 'hash', ?2, NULL)",
        params![later, title],
    )
    .unwrap();
    drop(conn);

    let after = gen(&db, 42, 300);
    let moved = after
        .iter()
        .find(|p| p.id == target.id)
        .expect("the pair id is stable under this edit");
    assert_eq!(
        moved.expected_frames[0], 900,
        "expected frame follows the data"
    );
    assert_eq!(
        moved.expected_local_date.as_deref(),
        Some("2026-10-08"),
        "the expected local date follows the data"
    );
}

/// Deleting one of the two frames that carry a distinctive line removes the seen-again pair.
#[test]
fn deleting_a_frame_changes_the_seen_again_set() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let db = Db::open(&path).unwrap();
    let before = gen(&db, 42, 300);
    let phrase = "checklist item seven for run 00 is complete";
    let pair = before
        .iter()
        .find(|p| p.generator == Generator::SeenAgain && p.expected_strings[0] == phrase)
        .expect("the fixture's distinctive line matches two frames");
    assert_eq!(pair.expected_frames.len(), 2);
    let victim = pair.expected_frames[1];

    let conn = Connection::open(&path).unwrap();
    // The app enables foreign keys; with them on, deleting the frame cascades to frame_text and
    // therefore fires the FTS delete trigger.
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    conn.execute("DELETE FROM frames WHERE id = ?1", [victim])
        .unwrap();
    let remaining: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM frame_text WHERE frame_id = ?1",
            [victim],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0, "cascade removed the text row");
    drop(conn);

    let after = gen(&db, 42, 300);
    assert!(
        !after
            .iter()
            .any(|p| p.generator == Generator::SeenAgain && p.expected_strings[0] == phrase),
        "one match is not a seen-again pair"
    );
}

/// Retitling the only frame that carries a repository removes its involvement pair.
#[test]
fn retitling_a_frame_changes_the_involvement_set() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let db = Db::open(&path).unwrap();
    let before = gen(&db, 42, 300);
    let repo = "github.com/acme/proj00";
    let pair = before
        .iter()
        .find(|p| p.generator == Generator::Involvement && p.expected_strings[0] == repo)
        .expect("an involvement pair for the repository");
    assert_eq!(
        pair.expected_sessions,
        vec![1],
        "the repo is in exactly one session"
    );

    let conn = Connection::open(&path).unwrap();
    let carrier: i64 = conn
        .query_row(
            "SELECT id FROM frames
             WHERE instr(COALESCE(window_title, ''), 'proj00') > 0
                OR instr(COALESCE(browser_url, ''), 'proj00') > 0
             LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE frames SET window_title = 'untitled', browser_url = '' WHERE id = ?1",
        [carrier],
    )
    .unwrap();
    drop(conn);

    let after = gen(&db, 42, 300);
    assert!(
        !after
            .iter()
            .any(|p| p.generator == Generator::Involvement && p.expected_strings[0] == repo),
        "the repository left the captured text, so its pair is gone"
    );
    assert!(before
        .iter()
        .any(|p| p.generator == Generator::Involvement && p.expected_strings[0] == repo));
}

/// `--pairs` changes only how many pairs are emitted, never how one is generated.
#[test]
fn pair_count_never_changes_a_pairs_content() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let db = Db::open(&path).unwrap();
    let small = gen(&db, 7, 5);
    let large = gen(&db, 7, 300);
    assert_eq!(small.len(), 5);
    for pair in &small {
        let same = large
            .iter()
            .find(|p| p.generator == pair.generator && p.question == pair.question)
            .unwrap_or_else(|| panic!("{} missing from the larger run", pair.question));
        // Ids are assignment order, so compare everything that defines the question and its
        // ground truth instead.
        assert_eq!(same.expected_frames, pair.expected_frames);
        assert_eq!(same.expected_sessions, pair.expected_sessions);
        assert_eq!(same.expected_strings, pair.expected_strings);
        assert_eq!(same.window, pair.window);
        assert_eq!(same.expected_local_date, pair.expected_local_date);
    }
}
