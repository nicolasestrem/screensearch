//! The read-only guarantee (`docs/kickoffs/P4.md` acceptance: "the harness opens the DB read-only
//! (test asserts it)").
//!
//! The runner's only database entry point is [`harness::eval::Db::open`], which wraps the crate's
//! `export::open_readonly` (`SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only` + a busy timeout for the
//! live WAL). This test proves the effect structurally and observably: the pragma is on, a write
//! fails with SQLite's read-only error, `CREATE TABLE` fails too, reads still work, and the file's
//! bytes are unchanged afterwards.

mod common;

use harness::eval::Db;
use rusqlite::Connection;

#[test]
fn the_runner_opens_read_only_and_an_attempted_write_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let before = std::fs::read(&path).expect("read fixture bytes");

    let db = Db::open(&path).expect("open read-only");
    let query_only: i64 = db
        .conn()
        .query_row("PRAGMA query_only", [], |r| r.get(0))
        .expect("read query_only");
    assert_eq!(query_only, 1, "PRAGMA query_only must be set");

    let insert = db
        .conn()
        .execute("INSERT INTO settings (key, value) VALUES ('x', 'y')", [])
        .expect_err("a write through the runner's handle must fail");
    let message = insert.to_string().to_lowercase();
    assert!(
        message.contains("readonly") || message.contains("read-only"),
        "expected a read-only error, got: {insert}"
    );

    let create = db
        .conn()
        .execute_batch("CREATE TABLE sneaky (id INTEGER)")
        .expect_err("creating a table through the runner's handle must fail");
    let message = create.to_string().to_lowercase();
    assert!(
        message.contains("readonly") || message.contains("read-only"),
        "expected a read-only error, got: {create}"
    );

    // Reads keep working: 12 sessions x 3 frames.
    assert_eq!(db.count_frames().unwrap(), 36);
    assert_eq!(db.count_sessions().unwrap(), 12);
    assert_eq!(db.schema_version().unwrap(), Some(14));

    drop(db);
    let after = std::fs::read(&path).expect("read fixture bytes again");
    assert_eq!(before, after, "the database file must be byte-identical");
}

#[test]
fn a_write_connection_can_still_change_the_same_file() {
    // The negative control: the fixture really is writable, so the failure above is the runner's
    // read-only handle rather than an immutable file.
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('probe', 'ok')",
        [],
    )
    .expect("the fixture file is writable");
    drop(conn);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.setting("probe").unwrap().as_deref(), Some("ok"));
}

#[test]
fn the_api_token_is_read_from_settings_and_a_missing_row_explains_the_fix() {
    let tmp = tempfile::tempdir().unwrap();
    let path = common::rich(tmp.path());
    let db = Db::open(&path).unwrap();
    let error = db
        .api_token()
        .expect_err("no api.token row in this fixture");
    let message = error.to_string();
    assert!(
        message.contains("Settings > Integrations"),
        "the message must name the fix: {message}"
    );
    assert!(
        !message.contains("test-token"),
        "the message must never echo a token"
    );

    let tmp2 = tempfile::tempdir().unwrap();
    let path = common::rich_with_token(tmp2.path());
    let db = Db::open(&path).unwrap();
    assert_eq!(db.api_token().unwrap(), "test-token");
}
