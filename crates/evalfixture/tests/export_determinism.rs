//! Acceptance test for the fixture JSON export (`ssv2c-evalfixture export`).
//!
//! Requirement 2.3: exporting the same fixture database twice must produce byte-identical
//! files. Every fixture here is generated with `embeddings: false` into a `tempfile`
//! directory, so the test never loads or downloads a model and never touches a real profile.
//! `gen::generate` and `export::run` each own their Tokio runtime, so they run from a plain
//! `#[test]` function.

use evalfixture::export;
use evalfixture::gen::{default_start, generate, GenOptions};
use evalfixture::DEFAULT_SEED;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn exporting_the_same_fixture_twice_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = generate(&GenOptions {
        out_dir: dir.path().join("fixture"),
        seed: DEFAULT_SEED,
        start: default_start(),
        weeks: 2,
        frames_per_week: 60,
        embeddings: false,
        embed_frames: None,
        persona: None,
    })
    .unwrap();

    let db = manifest.db.display().to_string();
    let first = dir.path().join("first.json");
    let second = dir.path().join("second.json");
    let first_path = first.display().to_string();
    let second_path = second.display().to_string();
    export::run(&args(&["--db", &db, "--out", &first_path])).unwrap();
    export::run(&args(&["--db", &db, "--out", &second_path])).unwrap();

    assert!(first.is_file(), "{} was not written", first_path);
    assert!(second.is_file(), "{} was not written", second_path);
    let first_bytes = std::fs::read(&first).unwrap();
    let second_bytes = std::fs::read(&second).unwrap();
    assert_eq!(
        first_bytes, second_bytes,
        "the same database must export to identical bytes"
    );
    assert_eq!(
        first_bytes.last(),
        Some(&b'\n'),
        "the export is UTF-8 JSON with a trailing newline"
    );

    let document: serde_json::Value =
        serde_json::from_slice(&first_bytes).expect("the export is a JSON document");
    assert_eq!(
        document["persona"]["id"], "northwind-tech",
        "a default fixture reports the legacy persona identity"
    );
    for key in ["projects", "sessions", "days"] {
        let rows = document[key]
            .as_array()
            .unwrap_or_else(|| panic!("`{key}` is a JSON array"));
        assert!(!rows.is_empty(), "`{key}` is not empty");
    }
}
