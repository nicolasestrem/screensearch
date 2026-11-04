//! Baseline schema tests (`03 §4`, 0.4.5 D2).
//!
//! There is no migration chain any more, so these replace the populated-DB migration
//! tests: a fresh store must materialise the whole v14 baseline, and any database that
//! is not at v14 must refuse to open without being touched.

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use store::{SqliteStore, BASELINE_SQL, LATEST_SCHEMA_VERSION};

/// The eleven 0.4.5 tables, created empty by the baseline (`03 §4`).
const NEW_TABLES: [&str; 11] = [
    "entities",
    "entity_aliases",
    "session_entities",
    "threads",
    "thread_sessions",
    "thread_overrides",
    "session_vectors",
    "insights",
    "insight_evidence",
    "ask_threads",
    "ask_turns",
];

/// Parses names created by one `CREATE ... TABLE <name>` prefix out of the baseline
/// script itself, so the expected set can never drift from the DDL.
fn declared_tables_with_prefix(sql: &str, prefix: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = sql;
    while let Some(pos) = rest.find(prefix) {
        let after = &rest[pos + prefix.len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(
            !name.is_empty(),
            "baseline has a {prefix:?} with no parsable name near {:?}",
            &after[..after.len().min(40)]
        );
        rest = &after[name.len()..];
        names.push(name);
    }
    names
}

/// Every table the baseline declares: plain tables plus virtual tables.
fn tables_declared_by(sql: &str) -> Vec<String> {
    let mut names = declared_tables_with_prefix(sql, "CREATE TABLE ");
    names.extend(declared_tables_with_prefix(sql, "CREATE VIRTUAL TABLE "));
    names.sort();
    names
}

/// The virtual tables the baseline declares. Their FTS5 / `vec0` shadow tables are an
/// implementation detail of the extension, not part of the schema contract.
fn virtual_tables_declared_by(sql: &str) -> Vec<String> {
    let mut names = declared_tables_with_prefix(sql, "CREATE VIRTUAL TABLE ");
    names.sort();
    names
}

/// The physical table set of a database, excluding the shadow tables that FTS5 and
/// `vec0` create for their virtual tables.
fn physical_tables(conn: &Connection, virtual_tables: &[String]) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM pragma_table_list
             WHERE schema = 'main' AND type IN ('table','virtual') AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .expect("prepare pragma_table_list");
    let mut names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .expect("query table list")
        .map(|r| r.expect("table name"))
        .collect();
    names.retain(|name| {
        !virtual_tables
            .iter()
            .any(|vtab| name.starts_with(&format!("{vtab}_")))
    });
    names.sort();
    names
}

fn integrity_check(conn: &Connection) -> String {
    conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity_check")
}

fn foreign_key_violations(conn: &Connection) -> i64 {
    let mut stmt = conn
        .prepare("PRAGMA foreign_key_check")
        .expect("prepare foreign_key_check");
    let mut rows = stmt.query([]).expect("run foreign_key_check");
    let mut count = 0;
    while rows.next().expect("step foreign_key_check").is_some() {
        count += 1;
    }
    count
}

fn table_row_count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count rows")
}

/// A fresh store is the whole v14 baseline: version 14, exactly the tables the script
/// declares, empty 0.4.5 tables, clean integrity and no foreign-key violations.
#[test]
fn fresh_store_is_the_v14_baseline() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("screensearch.db");

    let store = SqliteStore::open_path(&path).expect("create the baseline");
    assert_eq!(
        store.schema_version().expect("read schema_version"),
        LATEST_SCHEMA_VERSION
    );
    assert_eq!(LATEST_SCHEMA_VERSION, 14, "the baseline version is 14");
    drop(store);

    // The store is closed, so a plain read-only connection sees the file as written.
    let conn = Connection::open(&path).expect("re-open the baseline file");

    let declared = tables_declared_by(BASELINE_SQL);
    let virtual_tables = virtual_tables_declared_by(BASELINE_SQL);
    let actual = physical_tables(&conn, &virtual_tables);
    assert_eq!(
        actual, declared,
        "the database tables must be exactly the tables the baseline script declares"
    );

    for table in NEW_TABLES {
        assert!(
            declared.iter().any(|name| name == table),
            "the baseline must declare `{table}` (`03 §4`)"
        );
        assert_eq!(
            table_row_count(&conn, table),
            0,
            "`{table}` must be created empty: no writer exists in P3"
        );
    }

    assert_eq!(integrity_check(&conn), "ok", "PRAGMA integrity_check");
    assert_eq!(
        foreign_key_violations(&conn),
        0,
        "PRAGMA foreign_key_check must be empty"
    );
}

/// Creates a synthetic database holding only a `schema_version` row, duplicating what an
/// older or newer build would leave behind. No product data, no repository path.
fn synthetic_version_file(path: &Path, version: i32) {
    let conn = Connection::open(path).expect("create synthetic db");
    conn.execute_batch("CREATE TABLE schema_version (version INTEGER NOT NULL);")
        .expect("schema_version table");
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [version],
    )
    .expect("version row");
}

/// Any database that is not v14 refuses to open, names the version found and
/// `ssv2c-rebuild`, and is left byte for byte untouched (`03 §4`, 0.4.5 D2).
#[test]
fn refuses_any_database_that_is_not_v14() {
    let dir = tempfile::tempdir().expect("temp dir");

    // A v13 file (the release this arc starts from) and a v15 file (a newer build).
    for version in [13_i32, 15_i32] {
        let path = dir.path().join(format!("screensearch-v{version}.db"));
        synthetic_version_file(&path, version);
        let bytes_before = fs::read(&path).expect("read before");
        let mtime_before = fs::metadata(&path)
            .expect("stat before")
            .modified()
            .expect("mtime before");

        let err = match SqliteStore::open_path(&path) {
            Ok(_) => panic!("a schema_version {version} database must not open"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("ssv2c-rebuild"),
            "the refusal must name the rebuild tool, got {err:?}"
        );
        assert!(
            err.contains(&version.to_string()),
            "the refusal must name the version found ({version}), got {err:?}"
        );
        assert!(
            err.contains(&LATEST_SCHEMA_VERSION.to_string()),
            "the refusal must name the required version, got {err:?}"
        );

        assert_eq!(
            fs::read(&path).expect("read after"),
            bytes_before,
            "a refused v{version} database must not be modified"
        );
        assert_eq!(
            fs::metadata(&path)
                .expect("stat after")
                .modified()
                .expect("mtime after"),
            mtime_before,
            "a refused v{version} database must not be touched"
        );
    }
}

/// A zero-byte `screensearch.db` holds no data, so it is initialised rather than refused.
/// This is also what a first run that failed mid-way leaves behind (the baseline is one
/// transaction, so the failure rolls back to an empty file). Refusing it would brick the
/// app permanently: `ssv2c-rebuild` only reads v13 sources and cannot repair it either.
#[test]
fn initialises_a_zero_byte_file_instead_of_refusing_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("screensearch.db");
    fs::write(&path, b"").expect("create an empty file");

    let store = SqliteStore::open_path(&path).expect("an empty file is initialised");
    assert_eq!(
        store.schema_version().expect("version"),
        store::LATEST_SCHEMA_VERSION
    );
    drop(store);
    // And it reopens normally as a v14 store afterwards.
    SqliteStore::open_path(&path).expect("the initialised file reopens");
}

/// A file that holds schema objects but no readable version is someone's partial or
/// foreign database: it is refused with the rebuild instruction and left untouched,
/// never "initialised" over. Contrast with the zero-byte case above, which has no
/// objects and so holds nothing to protect.
#[test]
fn refuses_a_database_with_objects_but_no_version_row() {
    let dir = tempfile::tempdir().expect("temp dir");
    let no_row = dir.path().join("no-row.db");
    Connection::open(&no_row)
        .expect("create db")
        .execute_batch("CREATE TABLE schema_version (version INTEGER NOT NULL);")
        .expect("schema_version table");
    let before = fs::read(&no_row).expect("read before");
    let err = match SqliteStore::open_path(&no_row) {
        Ok(_) => panic!("a database without a version row must not open"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("ssv2c-rebuild"), "got {err:?}");
    assert!(err.contains("unknown"), "got {err:?}");
    assert_eq!(
        fs::read(&no_row).expect("read after"),
        before,
        "a refused database must stay byte-identical"
    );
}

/// A file that is not SQLite at all is refused, never overwritten with a baseline.
#[test]
fn refuses_a_non_sqlite_file_and_leaves_it_untouched() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("screensearch.db");
    let garbage = b"this is not a database, it is a text file someone renamed".repeat(80);
    fs::write(&path, &garbage).expect("write a non-sqlite file");
    let err = match SqliteStore::open_path(&path) {
        Ok(_) => panic!("a non-SQLite file must not be adopted as a store"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("ssv2c-rebuild"), "got {err:?}");
    assert_eq!(
        fs::read(&path).expect("read after"),
        garbage,
        "left untouched"
    );
}

/// Reopening a v14 store succeeds: the refusal path must not reject a healthy file.
#[test]
fn reopening_a_v14_store_succeeds() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("screensearch.db");

    {
        let store = SqliteStore::open_path(&path).expect("create the baseline");
        assert_eq!(
            store.schema_version().expect("read schema_version"),
            LATEST_SCHEMA_VERSION
        );
    }

    let store = SqliteStore::open_path(&path).expect("reopen the v14 store");
    assert_eq!(
        store.schema_version().expect("read schema_version"),
        LATEST_SCHEMA_VERSION
    );
    drop(store);

    let conn = Connection::open(&path).expect("re-open the file");
    assert_eq!(integrity_check(&conn), "ok", "PRAGMA integrity_check");
}

/// A database that claims to be v14 but is missing baseline tables is refused: the
/// version row alone does not make a usable store (`03 §4`, 0.4.5 D2).
#[test]
fn refuses_a_v14_file_missing_baseline_tables() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("screensearch.db");
    synthetic_version_file(&path, LATEST_SCHEMA_VERSION);
    let bytes_before = fs::read(&path).expect("read before");

    let err = match SqliteStore::open_path(&path) {
        Ok(_) => panic!("a v14 file without the baseline tables must not open"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("missing"),
        "the refusal must say a table is missing, got {err:?}"
    );
    assert!(
        err.contains(&LATEST_SCHEMA_VERSION.to_string()),
        "the refusal must name the version found, got {err:?}"
    );
    assert!(
        err.contains("ssv2c-rebuild"),
        "the refusal must name the rebuild tool, got {err:?}"
    );
    assert_eq!(
        fs::read(&path).expect("read after"),
        bytes_before,
        "a refused file must not be modified"
    );
}

/// The baseline's closed-set CHECK constraints are really enforced, including the two
/// rewritten tables (`frames`, whose CHECKs were rebuilt by hand, and the new 0.4.5
/// tables). A silently dropped constraint would be a schema change.
#[test]
fn baseline_check_constraints_are_enforced() {
    // Registers sqlite-vec before the raw connections below are opened.
    drop(SqliteStore::open_in_memory().expect("register vec"));
    let conn = Connection::open_in_memory().expect("in-memory db");
    store::create_baseline(&conn).expect("baseline");

    // frames.capture_trigger: the widened 0.2.1 token set, and nothing else.
    conn.execute(
        "INSERT INTO frames (id, captured_at, monitor_index, width, height, image_path,
                             content_hash, capture_trigger)
         VALUES (1, 1000, 0, 10, 10, 'a.jpg', 'h', 'scroll_stop')",
        [],
    )
    .expect("a legacy event token is still valid");
    assert!(
        conn.execute(
            "INSERT INTO frames (id, captured_at, monitor_index, width, height, image_path,
                                 content_hash, capture_trigger)
             VALUES (2, 2000, 0, 10, 10, 'b.jpg', 'h', 'bogus')",
            [],
        )
        .is_err(),
        "capture_trigger must reject a token outside the closed set"
    );

    // entities.kind: people and "topic" are out (0.4.5 D8).
    assert!(
        conn.execute(
            "INSERT INTO entities (kind, canonical_name, normalized_key, source, confidence,
                                   first_seen_at, last_seen_at)
             VALUES ('topic', 'x', 'x', 'heuristic', 1.0, 0, 0)",
            [],
        )
        .is_err(),
        "entities.kind must reject a kind outside the closed set"
    );

    // insights.kind: the five 0.4.5 types only.
    assert!(
        conn.execute(
            "INSERT INTO insights (kind, window_start, window_end, payload, generator)
             VALUES ('mood', 0, 1, '{}', 'heuristic')",
            [],
        )
        .is_err(),
        "insights.kind must reject a kind outside the closed set"
    );

    // thread_overrides: a rename carries no target thread.
    conn.execute(
        "INSERT INTO threads (id, title, cluster_key, first_seen_at, last_seen_at)
         VALUES (1, 't', 'k', 0, 0)",
        [],
    )
    .expect("thread row");
    assert!(
        conn.execute(
            "INSERT INTO thread_overrides (kind, thread_id, target_thread_id, payload)
             VALUES ('rename', 1, 1, '{}')",
            [],
        )
        .is_err(),
        "a rename override must not name a target thread"
    );
}
