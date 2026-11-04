//! Regression tests for four ground-truth defects found in review of the P4 harness. Each
//! fixture is built here, in a temporary directory, so the expected answer follows from the
//! data the test wrote: nothing is hand-labelled (0.4.5 D17).

mod common;

use harness::eval::facts::Db;
use harness::eval::gen::{generate_pairs, ExpectedId, GenOpts, Generator, Pair};

const NOW: i64 = 1_800_000_000_000;

fn pairs_of(path: &std::path::Path, seed: u64, pairs: usize, generator: Generator) -> Vec<Pair> {
    generate_pairs(
        &Db::open(path).unwrap(),
        &GenOpts {
            seed,
            pairs,
            max_frames: None,
            now_ms: NOW,
        },
    )
    .expect("generate pairs")
    .pairs
    .into_iter()
    .filter(|p| p.generator == generator)
    .collect()
}

/// `sequence` answers "before session Z" with the greatest `started_at` strictly below Z.
/// When two sessions start at the same instant there is no unique answer, so neither of
/// the tied sessions may be the target of a pair whose "before" is the other one.
#[test]
fn sequence_never_names_a_session_that_started_at_the_same_instant() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = common::Builder::new(tmp.path());
    b.session(1, "2026-10-01 07:00:00", 30, Some("one"));
    b.session(2, "2026-10-01 09:00:00", 30, Some("two"));
    b.session(3, "2026-10-01 09:00:00", 30, Some("three")); // same start as 2
    b.session(4, "2026-10-01 11:00:00", 30, Some("four"));
    for (s, at, repo) in [
        (1, "2026-10-01 07:05:00", "alpha"),
        (2, "2026-10-01 09:05:00", "bravo"),
        (3, "2026-10-01 09:06:00", "charlie"),
        (4, "2026-10-01 11:05:00", "delta"),
    ] {
        b.frame(
            at,
            Some(s),
            &format!("C:\\src\\{repo}\\main.rs - VS Code"),
            "",
            "notes",
        );
    }
    let path = b.finish();
    let pairs = pairs_of(&path, 7, 300, Generator::Sequence);
    assert!(!pairs.is_empty(), "the fixture must yield sequence pairs");
    for p in &pairs {
        let target: i64 = p
            .question
            .split_whitespace()
            .find_map(|w| w.parse().ok())
            .expect("question names its session");
        // Session 3 ties with 2, so "before 3" is ambiguous; and "before 4" would be 2-or-3.
        assert_ne!(target, 3, "an ambiguous 'before' must be skipped: {p:?}");
        assert_ne!(
            target, 4,
            "a tie below the target must be skipped too: {p:?}"
        );
        assert_ne!(
            p.expected_sessions,
            vec![3],
            "a tied session is never the answer"
        );
    }
}

/// `window_recall` must not require, as evidence, a frame that citation validity rejects
/// as "outside the pair window". A session straddling 06:00 contributes only its
/// in-window frames.
#[test]
fn window_recall_expects_only_frames_inside_its_window() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = common::Builder::new(tmp.path());
    b.session(1, "2026-10-01 05:30:00", 90, Some("early")); // 05:30 to 07:00
    let before = b.frame(
        "2026-10-01 05:31:00",
        Some(1),
        "C:\\src\\repo\\a.rs - VS Code",
        "",
        "notes",
    );
    let also_before = b.frame(
        "2026-10-01 05:45:00",
        Some(1),
        "C:\\src\\repo\\a.rs - VS Code",
        "",
        "notes",
    );
    let inside = b.frame(
        "2026-10-01 06:30:00",
        Some(1),
        "C:\\src\\repo\\a.rs - VS Code",
        "",
        "notes",
    );
    let path = b.finish();
    let pairs = pairs_of(&path, 7, 300, Generator::WindowRecall);
    assert_eq!(pairs.len(), 1, "one day, one window-recall pair");
    let p = &pairs[0];
    let w = p.window.as_ref().expect("window recall has a window");
    assert!(
        p.expected_frames.contains(&inside),
        "the in-window frame is evidence"
    );
    for id in [before, also_before] {
        assert!(
            !p.expected_frames.contains(&id),
            "frame {id} is outside {w:?}"
        );
    }
}

/// Recall reads the first 10 expected ids. With ten expected frames the sessions must
/// still be inside that slice, or the session-through-frame match rule can never fire.
#[test]
fn expected_sessions_are_inside_the_recall_slice_when_frames_hit_the_cap() {
    let p = Pair {
        id: "p".into(),
        generator: Generator::Involvement,
        question: "q".into(),
        expected_frames: (100..110).collect(),
        expected_sessions: vec![1, 2, 3],
        expected_strings: vec!["repo".into()],
        window: None,
        expected_local_date: None,
    };
    let slice: Vec<ExpectedId> = p.expected_order().into_iter().take(10).collect();
    for s in [1, 2, 3] {
        assert!(
            slice.contains(&ExpectedId::Session(s)),
            "session {s} must be scorable: {slice:?}"
        );
    }
    assert_eq!(
        p.expected_order().len(),
        13,
        "nothing is lost, only reordered"
    );
}

/// `--pairs` may change how many pairs are kept, never which pairs exist: a small request
/// must be a prefix of a large one for the same seed.
///
/// The defect this pins only shows on a database where the seen-again probe size
/// changes *which* frames are sampled: with the probe tied to `--pairs`, a large request
/// on a short-of-pairs database (the redistribution retry raises the generator's cap to
/// the full request) drew `4 x pairs` frames instead of the fixed 400, a different random
/// sample. So the fixture needs far more frames than 400, each carrying its own
/// distinct line (seen exactly twice so every line qualifies), and the large request must
/// be big enough that `4 x pairs` stays inside the frame count.
#[test]
fn a_small_pairs_request_is_a_prefix_of_a_large_one_for_seen_again() {
    let tmp = tempfile::tempdir().unwrap();
    let mut b = common::Builder::new(tmp.path());
    b.session(1, "2026-10-01 00:00:00", 24 * 60, Some("day"));
    // 2400 frames; frames 2k and 2k+1 share line k, so 1200 distinct lines, each seen twice.
    for i in 0..2400 {
        let second = i * 30; // one frame every 30 s across the day
        let line = format!(
            "ticket ABC-{:04} rollout checklist step {:04}",
            i / 2,
            i / 2
        );
        b.frame(
            &format!(
                "2026-10-01 {:02}:{:02}:{:02}",
                second / 3600,
                (second / 60) % 60,
                second % 60
            ),
            Some(1),
            "Tracker - Chrome",
            "https://tracker.example.test/board",
            &line,
        );
    }
    let path = b.finish();
    let small = pairs_of(&path, 11, 10, Generator::SeenAgain);
    // 500 requested: the other four generators are nearly empty on this fixture, so the
    // retry raises seen-again's cap to 500, which the old code turned into a 2000-frame probe.
    let large = pairs_of(&path, 11, 500, Generator::SeenAgain);
    assert!(!small.is_empty(), "the fixture must yield seen-again pairs");
    let q = |v: &[Pair]| v.iter().map(|p| p.question.clone()).collect::<Vec<_>>();
    let (qs, ql) = (q(&small), q(&large));
    assert!(
        ql.len() >= qs.len() && ql[..qs.len()] == qs[..],
        "--pairs changed which seen-again pairs exist:\nsmall={qs:#?}\nlarge prefix={:#?}",
        &ql[..qs.len().min(ql.len())]
    );
}
