//! Rebuild test fence (`docs/kickoffs/P3.md`, "Rebuild tool test fence").
//!
//! Every database in this file is a **synthetic v13 fixture created here**, in a
//! temp directory the test owns and removes. Nothing reads a real profile,
//! `%APPDATA%`, an environment variable pointing at a profile, or any path outside
//! the repository: the v13 DDL and every value below are written by this test.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rebuild::RebuildOptions;
use rusqlite::{params, Connection};
use traits::{FRAME_ID_HIGH_WATER_KEY, HISTORY_ID_KEY};

/// The reserved identity the fixture carries and the rebuild must copy verbatim.
const HISTORY_ID: &str = "11111111-2222-4333-8444-555555555555";
/// The reserved allocator high-water the fixture carries (never recomputed). It is
/// deliberately **above** the fixture's highest frame id (7), as on a real profile whose
/// newest frames were deleted: an implementation that recomputed the allocator from
/// `MAX(frames.id)` would write 7 and fail, so the test can tell copy from recompute.
const HIGH_WATER: &str = "42";

/// The nine settings-v2 `deleted` rows (`03 §8`, 0.4.5 D6); all are seeded so the
/// test proves the full set is dropped, not just one example.
const DELETED_KEYS: &[&str] = &[
    "api.network_notice_state",
    "capture.event_on_click",
    "capture.event_on_clipboard",
    "capture.event_on_scroll_stop",
    "capture.event_on_typing_pause",
    "capture.event_typing_pause_ms",
    "capture.uia_run_on_interactive",
    "enrich.image_embeddings",
    "storage.jpeg_quality",
];

/// The eleven 0.4.5 tables: absent from a v13 source, so they must be empty.
const NEW_TABLES: &[&str] = &[
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

struct Fixture {
    _dir: tempfile::TempDir,
    source: PathBuf,
}

impl Fixture {
    fn path(&self) -> &Path {
        self._dir.path()
    }
}

fn fixture() -> Result<Fixture> {
    // The fence allows a temp directory the test creates and removes, and forbids
    // opening any path outside the repository: `tempfile` owns creation/removal,
    // rooted in the repository's ignored `target/` tree instead of the system temp.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/p3-rebuild-tests");
    fs::create_dir_all(&root).context("create the repository-local fixture root")?;
    let dir = tempfile::Builder::new()
        .prefix("fixture-")
        .tempdir_in(&root)
        .context("create the fixture temp directory")?;
    let source = dir.path().join("screensearch.db");
    // vec0 registration is process-wide; the fixture's own `embedding_vectors`
    // virtual table needs it exactly like a real v13 profile does.
    rebuild::register_vec_extension();

    let conn = Connection::open(&source).context("create the synthetic v13 fixture")?;
    conn.execute_batch(V13_FIXTURE_SQL)
        .context("write the synthetic v13 fixture schema and rows")?;
    // A real 768-float vector: the metadata row alone would not prove the vec0 copy.
    let vector: Vec<u8> = (0..768u32)
        .flat_map(|index| (index as f32 / 768.0).to_le_bytes())
        .collect();
    conn.execute(
        "INSERT INTO embedding_vectors(embedding_id, embedding) VALUES (42, ?1)",
        params![vector],
    )
    .context("insert the fixture vector")?;
    drop(conn);

    // Two of the three referenced images exist; `frames/0007.webp` is deliberately
    // absent so the report has something real to say without dropping the row.
    fs::create_dir_all(dir.path().join("frames"))?;
    fs::write(dir.path().join("frames/0005.webp"), b"fixture-image-5")?;
    fs::write(dir.path().join("frames/0006.webp"), b"fixture-image-6")?;

    Ok(Fixture { _dir: dir, source })
}

fn table_counts(path: &Path) -> Result<BTreeMap<String, i64>> {
    Ok(rebuild::table_counts(path)?.into_iter().collect())
}

fn count(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok())
}

#[test]
fn rebuild_carries_history_and_repopulates_indexes() -> Result<()> {
    let fixture = fixture()?;
    let source = fixture.source.clone();
    let dest = fixture.path().join("screensearch-v14.db");

    let bytes_before = fs::read(&source)?;
    let mtime_before = fs::metadata(&source)?.modified()?;
    assert!(!dest.exists(), "the destination must not exist beforehand");

    let report = rebuild::rebuild(&RebuildOptions {
        source: source.clone(),
        dest: dest.clone(),
        sample_frames: Some(20),
    })?;

    // The source is read-only: bytes and modification time are unchanged.
    assert_eq!(fs::read(&source)?, bytes_before);
    assert_eq!(
        fs::metadata(&source)?.modified()?,
        mtime_before,
        "opening the source read-only must not touch its mtime"
    );
    // The destination was created fresh, not upgraded in place.
    assert!(dest.is_file());
    assert_eq!(report.source_version, 13);
    assert_eq!(report.dest_version, 14);
    assert_eq!(report.integrity_check, ["ok"]);
    assert_eq!(report.foreign_key_violations, 0);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    // Per-table counts: carried tables match the source; queue/derived tables are empty.
    let source_counts = table_counts(&source)?;
    let dest_counts = table_counts(&dest)?;
    for copy in &report.carried {
        assert_eq!(copy.source_rows, copy.dest_rows, "{}", copy.table);
        assert_eq!(source_counts[&copy.table], copy.dest_rows, "{}", copy.table);
        assert_eq!(dest_counts[&copy.table], copy.dest_rows, "{}", copy.table);
    }
    assert_eq!(dest_counts["frames"], 3);
    assert_eq!(dest_counts["frame_text"], 3);
    assert_eq!(dest_counts["text_spans"], 1);
    assert_eq!(dest_counts["vision_analysis"], 1);
    assert_eq!(dest_counts["embeddings"], 1);
    assert_eq!(dest_counts["sessions"], 1);
    assert_eq!(dest_counts["session_artifacts"], 1);
    assert_eq!(dest_counts["marks"], 1);
    assert_eq!(dest_counts["tags"], 1);
    assert_eq!(dest_counts["frame_tags"], 1);
    assert_eq!(dest_counts["report_runs"], 1);
    assert_eq!(dest_counts["report_deliveries"], 1);
    assert_eq!(dest_counts["report_files"], 1);
    assert_eq!(dest_counts["jobs"], 0, "jobs is a transient queue");
    assert_eq!(
        dest_counts["chrome_text_catalog"], 0,
        "the chrome catalog is derived"
    );

    let conn = Connection::open(&dest)?;
    assert_eq!(
        count(&conn, "SELECT version FROM schema_version")?,
        14,
        "the baseline writes schema_version = 14"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM pragma_integrity_check WHERE integrity_check <> 'ok'"
        )?,
        0
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM pragma_foreign_key_check")?,
        0
    );
    for table in NEW_TABLES {
        assert_eq!(
            count(&conn, &format!("SELECT COUNT(*) FROM {table}"))?,
            0,
            "{table}"
        );
    }

    // FTS5 is rebuilt from the copied rows, both mirrors.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM frame_text_fts WHERE frame_text_fts MATCH 'zebraword'"
        )?,
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM frame_text_raw_fts WHERE frame_text_raw_fts MATCH 'rawsecret'"
        )?,
        1
    );

    // vec0 receives the vector belonging to the copied embeddings row.
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM embedding_vectors")?, 1);
    let vector: Vec<u8> = conn.query_row(
        "SELECT embedding FROM embedding_vectors WHERE embedding_id = 42",
        [],
        |row| row.get(0),
    )?;
    let expected: Vec<u8> = (0..768u32)
        .flat_map(|index| (index as f32 / 768.0).to_le_bytes())
        .collect();
    assert_eq!(vector, expected);

    // Reserved metadata is copied, never recomputed.
    assert_eq!(setting(&conn, HISTORY_ID_KEY)?.as_deref(), Some(HISTORY_ID));
    assert_eq!(
        setting(&conn, FRAME_ID_HIGH_WATER_KEY)?.as_deref(),
        Some(HIGH_WATER)
    );

    // Settings: the table's live keys plus the out-of-table settings survive; the
    // nine deleted rows and any other unknown key are gone.
    assert_eq!(
        setting(&conn, "capture.interval_ms")?.as_deref(),
        Some("1234")
    );
    assert_eq!(
        setting(&conn, "capture.diff_threshold")?.as_deref(),
        Some("0.006")
    );
    assert_eq!(
        setting(&conn, "app.tray_toast_done")?.as_deref(),
        Some("true")
    );
    assert_eq!(
        setting(&conn, "api.read_token")?.as_deref(),
        Some("fixture-read-token")
    );
    assert_eq!(
        setting(&conn, "memory.telegram.chat_id")?.as_deref(),
        Some("424242")
    );
    for key in DELETED_KEYS {
        assert_eq!(setting(&conn, key)?, None, "{key} must be dropped");
    }
    assert_eq!(setting(&conn, "obsolete.unknown_key")?, None);
    // Internal markers guard carried state (the session backfill checkpoint above all:
    // without it the app re-segments the carried history), so they survive verbatim.
    assert_eq!(
        setting(&conn, "sessions.backfill_done_until")?.as_deref(),
        Some("2100")
    );
    for key in [
        "overlay.hotkey_migrated",
        "sessions.gap_close_secs_migrated",
        "maintenance.self_capture_purged",
        "maintenance.purged_spans_merged",
    ] {
        assert_eq!(
            setting(&conn, key)?.as_deref(),
            Some("1"),
            "{key} must survive"
        );
    }
    // The catalog watermark resets with `chrome_text_catalog`, which is not carried.
    assert_eq!(setting(&conn, "text.catalog_filter_version")?, None);
    assert_eq!(report.settings_copied, 13);
    assert_eq!(report.settings_dropped.len(), 11);

    // A missing image is reported, never a reason to drop the row or its path.
    assert_eq!(report.missing_images, ["frames/0007.webp"]);
    assert_eq!(
        conn.query_row("SELECT image_path FROM frames WHERE id = 7", [], |row| {
            row.get::<_, String>(0)
        })?,
        "frames/0007.webp"
    );

    // The sampled ids come from the rebuilt file and all exist there.
    assert_eq!(report.sample_frames.len(), 3);
    for id in &report.sample_frames {
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM frames WHERE id = ?1",
                params![id],
                |row| { row.get::<_, i64>(0) }
            )?,
            1,
            "sampled frame {id} must exist in the rebuilt file"
        );
    }
    drop(conn);

    // The app's own open path accepts the rebuilt file (version + every baseline
    // table + the carried history metadata), which is what "boots on v14" reduces
    // to at the store boundary.
    let store = store::SqliteStore::open_path(&dest)
        .context("the store must accept the rebuilt v14 file")?;
    assert_eq!(store.schema_version()?, 14);
    Ok(())
}

#[test]
fn rebuild_refuses_an_existing_destination_and_leaves_it_alone() -> Result<()> {
    let fixture = fixture()?;
    let dest = fixture.path().join("occupied.db");
    fs::write(&dest, b"not a database")?;

    let error = rebuild::rebuild(&RebuildOptions {
        source: fixture.source.clone(),
        dest: dest.clone(),
        sample_frames: None,
    })
    .expect_err("an existing destination must be refused");

    assert!(format!("{error:#}").contains("already exists"), "{error:#}");
    assert_eq!(fs::read(&dest)?, b"not a database");
    Ok(())
}

#[test]
fn counts_report_skips_index_shadows() -> Result<()> {
    let fixture = fixture()?;
    let counts = rebuild::table_counts(&fixture.source)?;
    let names: Vec<&str> = counts.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"frames"));
    assert!(names.contains(&"settings"));
    assert!(
        !names
            .iter()
            .any(|name| name.starts_with("embedding_vectors")),
        "vec0 shadow tables must not be reported: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name.contains("_fts_")),
        "FTS5 shadow tables must not be reported: {names:?}"
    );
    let frames = counts
        .iter()
        .find(|(name, _)| name == "frames")
        .map(|(_, rows)| *rows);
    assert_eq!(frames, Some(3));
    Ok(())
}

#[test]
fn settings_policy_matches_the_spec_table() {
    let policy = rebuild::SettingsPolicy::load();
    assert_eq!(policy.ui_count(), 25);
    assert_eq!(policy.deleted_count(), 9);
    for key in DELETED_KEYS {
        assert!(!policy.carries(key));
    }
    assert!(policy.carries("capture.interval_ms"));
    assert!(policy.carries("api.history_id"));
    assert!(policy.carries("api.frame_id_high_water"));
}

/// A minimal v13 profile: only the tables the carry set reads, with v13-shaped
/// columns. `text_spans.line_index` and `vision_analysis.created_at` are omitted on
/// purpose — the destination defaults them via the column intersection, which is
/// the generic behaviour the tool must have (no v13 DDL is restated in the code).
const V13_FIXTURE_SQL: &str = r#"
CREATE TABLE schema_version (version INTEGER NOT NULL);
INSERT INTO schema_version VALUES (13);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO settings(key, value) VALUES
  ('api.history_id', '11111111-2222-4333-8444-555555555555'),
  ('api.frame_id_high_water', '42'),
  ('api.token', 'fixture-api-token'),
  ('api.read_token', 'fixture-read-token'),
  ('capture.interval_ms', '1234'),
  ('capture.diff_threshold', '0.006'),
  ('app.tray_toast_done', 'true'),
  ('memory.telegram.chat_id', '424242'),
  ('api.network_notice_state', 'dismissed'),
  ('capture.event_on_click', 'true'),
  ('capture.event_on_clipboard', 'true'),
  ('capture.event_on_scroll_stop', 'true'),
  ('capture.event_on_typing_pause', 'true'),
  ('capture.event_typing_pause_ms', '500'),
  ('capture.uia_run_on_interactive', 'true'),
  ('enrich.image_embeddings', 'true'),
  ('storage.jpeg_quality', '82'),
  ('sessions.backfill_done_until', '2100'),
  ('overlay.hotkey_migrated', '1'),
  ('sessions.gap_close_secs_migrated', '1'),
  ('maintenance.self_capture_purged', '1'),
  ('maintenance.purged_spans_merged', '1'),
  ('text.catalog_filter_version', '3'),
  ('obsolete.unknown_key', 'not in the table');

CREATE TABLE sessions (
  id            INTEGER PRIMARY KEY,
  started_at    INTEGER NOT NULL,
  ended_at      INTEGER,
  kind          TEXT NOT NULL CHECK (kind IN ('focus','meeting','ai','other')),
  tool          TEXT CHECK (tool IS NULL OR kind = 'ai'),
  host          TEXT CHECK (host IN ('terminal','desktop','browser','ide')),
  context_key   TEXT NOT NULL,
  title         TEXT,
  summary       TEXT,
  summary_model TEXT,
  confidence    REAL NOT NULL,
  frozen        INTEGER NOT NULL DEFAULT 0 CHECK (frozen IN (0,1)),
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
INSERT INTO sessions(id, started_at, ended_at, kind, tool, host, context_key, title,
                     summary, summary_model, confidence, frozen, created_at, updated_at)
VALUES (3, 1000, 2000, 'focus', NULL, 'desktop', 'focus:editor', 'Fixture session',
        'A summary about zebraword', NULL, 0.91, 0, 1000, 2000);

CREATE TABLE frames (
  id            INTEGER PRIMARY KEY,
  captured_at   INTEGER NOT NULL,
  monitor_index INTEGER NOT NULL,
  width         INTEGER NOT NULL,
  height        INTEGER NOT NULL,
  image_path    TEXT    NOT NULL,
  content_hash  TEXT    NOT NULL,
  app_hint      TEXT,
  window_title  TEXT,
  browser_url   TEXT,
  activity_type TEXT,
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  capture_trigger TEXT,
  image_purged  INTEGER NOT NULL DEFAULT 0 CHECK (image_purged IN (0,1)),
  session_id    INTEGER REFERENCES sessions(id) ON DELETE SET NULL
);
INSERT INTO frames(id, captured_at, monitor_index, width, height, image_path, content_hash,
                   app_hint, window_title, browser_url, activity_type, created_at,
                   capture_trigger, image_purged, session_id)
VALUES
  (5, 1000, 0, 800, 600, 'frames/0005.webp', 'hash-5', 'editor', 'Editor',
   NULL, NULL, 1000, 'timer', 0, 3),
  (6, 1500, 0, 800, 600, 'frames/0006.webp', 'hash-6', 'browser', 'Docs',
   'https://example.test/zebraword', NULL, 1500, 'timer', 0, 3),
  (7, 2100, 1, 800, 600, 'frames/0007.webp', 'hash-7', NULL, NULL,
   NULL, NULL, 2100, NULL, 0, NULL);

CREATE TABLE frame_text (
  frame_id            INTEGER PRIMARY KEY,
  raw_text            TEXT    NOT NULL,
  content_text        TEXT    NOT NULL,
  primary_source      TEXT    NOT NULL CHECK (primary_source IN ('ocr','uia')),
  filter_version      INTEGER NOT NULL,
  suppressed_count    INTEGER NOT NULL,
  target_window_title TEXT,
  target_app_hint     TEXT,
  created_at          INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
INSERT INTO frame_text(frame_id, raw_text, content_text, primary_source, filter_version,
                       suppressed_count, target_window_title, target_app_hint, created_at)
VALUES
  (5, 'zebraword fastapi notes rawsecret', 'zebraword fastapi notes', 'ocr', 2, 1, 'Editor', 'editor', 1000),
  (6, 'ordinary docs text', 'ordinary docs text', 'uia', 2, 0, 'Docs', 'browser', 1500),
  (7, 'third frame text', 'third frame text', 'ocr', 2, 0, NULL, NULL, 2100);

-- v13 shape without `line_index`; the destination default fills it.
CREATE TABLE text_spans (
  frame_id        INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  span_index      INTEGER NOT NULL,
  text            TEXT    NOT NULL,
  normalized_text TEXT    NOT NULL,
  source          TEXT    NOT NULL,
  role            TEXT    NOT NULL,
  x REAL NOT NULL, y REAL NOT NULL, w REAL NOT NULL, h REAL NOT NULL,
  is_searchable   INTEGER NOT NULL,
  suppress_reason TEXT,
  PRIMARY KEY (frame_id, span_index)
);
INSERT INTO text_spans(frame_id, span_index, text, normalized_text, source, role,
                       x, y, w, h, is_searchable, suppress_reason)
VALUES (5, 0, 'zebraword', 'zebraword', 'ocr', 'content', 0.1, 0.1, 0.2, 0.05, 1, NULL);

-- v13 shape without `created_at`; the destination default fills it.
CREATE TABLE vision_analysis (
  frame_id     INTEGER PRIMARY KEY REFERENCES frames(id) ON DELETE CASCADE,
  description  TEXT NOT NULL,
  activity_type TEXT,
  app_hint     TEXT,
  confidence   REAL NOT NULL,
  model        TEXT NOT NULL
);
INSERT INTO vision_analysis(frame_id, description, activity_type, app_hint, confidence, model)
VALUES (6, 'A fixture editor', 'other', 'editor', 0.5, 'fixture-vision');

CREATE TABLE embeddings (
  id           INTEGER PRIMARY KEY,
  frame_id     INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  chunk_index  INTEGER NOT NULL,
  chunk_text   TEXT NOT NULL,
  source       TEXT NOT NULL,
  model        TEXT NOT NULL,
  dim          INTEGER NOT NULL,
  content_hash TEXT NOT NULL,
  UNIQUE(frame_id, chunk_index)
);
INSERT INTO embeddings(id, frame_id, chunk_index, chunk_text, source, model, dim, content_hash)
VALUES (42, 5, 0, 'zebraword fastapi notes', 'ocr', 'fixture-embed', 768, 'hash-42');

CREATE VIRTUAL TABLE embedding_vectors USING vec0(
  embedding_id INTEGER PRIMARY KEY,
  embedding    FLOAT[768] distance_metric=cosine
);

CREATE TABLE marks (
  id          INTEGER PRIMARY KEY,
  frame_id    INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  created_at  INTEGER NOT NULL,
  note        TEXT,
  resolved_at INTEGER
);
INSERT INTO marks(id, frame_id, created_at, note, resolved_at)
VALUES (11, 5, 1500, 'fixture mark', NULL);

CREATE TABLE session_artifacts (
  id         INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('exchange','transcript','note')),
  role       TEXT,
  frame_id   INTEGER REFERENCES frames(id) ON DELETE SET NULL,
  content    TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
INSERT INTO session_artifacts(id, session_id, kind, role, frame_id, content, created_at)
VALUES (21, 3, 'exchange', 'user', 5, 'fixture exchange', 1500);

CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL);
INSERT INTO tags(id, name) VALUES (31, 'fixture-tag');
CREATE TABLE frame_tags (
  frame_id INTEGER REFERENCES frames(id) ON DELETE CASCADE,
  tag_id   INTEGER REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY(frame_id, tag_id)
);
INSERT INTO frame_tags(frame_id, tag_id) VALUES (5, 31);

CREATE TABLE report_runs (
  run_id TEXT PRIMARY KEY NOT NULL,
  schedule_id TEXT,
  revision_of TEXT,
  window_start INTEGER NOT NULL,
  window_end INTEGER NOT NULL CHECK(window_end > window_start),
  phase TEXT NOT NULL CHECK(phase IN ('queued','running','blocked','finished')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  payload TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'daily' CHECK(kind IN ('daily','weekly_synthesis'))
);
INSERT INTO report_runs(run_id, schedule_id, revision_of, window_start, window_end,
                        phase, created_at, updated_at, payload, kind)
VALUES ('run-fixture', NULL, NULL, 1000, 2000, 'finished', 2000, 2000,
        '{"fixture":true}', 'daily');

CREATE TABLE report_deliveries (
  id TEXT PRIMARY KEY NOT NULL,
  run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
  state TEXT NOT NULL,
  next_attempt_at INTEGER,
  payload TEXT NOT NULL
);
INSERT INTO report_deliveries(id, run_id, state, next_attempt_at, payload)
VALUES ('delivery-fixture', 'run-fixture', 'delivered', NULL, '{"fixture":true}');

CREATE TABLE report_files (path TEXT PRIMARY KEY NOT NULL, payload TEXT NOT NULL);
INSERT INTO report_files(path, payload) VALUES ('notes/fixture.md', '{"bytes":1}');

CREATE TABLE jobs (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL,
  frame_id     INTEGER REFERENCES frames(id) ON DELETE CASCADE,
  state        TEXT NOT NULL DEFAULT 'pending',
  priority     INTEGER NOT NULL DEFAULT 0,
  attempts     INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 3,
  not_before   INTEGER NOT NULL DEFAULT 0,
  last_error   TEXT,
  created_at   INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at   INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
INSERT INTO jobs(id, kind, frame_id, state) VALUES (51, 'embed_text', 5, 'pending');
INSERT INTO jobs(id, kind, frame_id, state) VALUES (52, 'vision_tag', 6, 'done');

CREATE TABLE chrome_text_catalog (
  signature       TEXT PRIMARY KEY,
  app_hint        TEXT,
  region_bucket   TEXT,
  normalized_text TEXT    NOT NULL,
  seen_count      INTEGER NOT NULL,
  first_seen_at   INTEGER NOT NULL,
  last_seen_at    INTEGER NOT NULL,
  suppressed      INTEGER NOT NULL DEFAULT 0 CHECK (suppressed IN (0,1))
);
INSERT INTO chrome_text_catalog(signature, app_hint, region_bucket, normalized_text,
                                seen_count, first_seen_at, last_seen_at, suppressed)
VALUES ('fixture-signature', 'editor', 'top', 'menu', 12, 1000, 2000, 1);
"#;

#[test]
fn cli_reports_counts_and_rebuilds_the_fixture() -> Result<()> {
    let fixture = fixture()?;
    let dest = fixture.path().join("cli-v14.db");
    let binary = env!("CARGO_BIN_EXE_ssv2c-rebuild");

    let counts_before = std::process::Command::new(binary)
        .arg("--counts")
        .arg(&fixture.source)
        .output()?;
    assert!(counts_before.status.success(), "{counts_before:?}");
    let stdout = String::from_utf8(counts_before.stdout)?;
    assert!(stdout.contains("frames"), "{stdout}");
    assert!(stdout.contains("settings"), "{stdout}");

    let rebuilt = std::process::Command::new(binary)
        .arg("--source")
        .arg(&fixture.source)
        .arg("--dest")
        .arg(&dest)
        .args(["--sample-frames", "2"])
        .output()?;
    assert!(
        rebuilt.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&rebuilt.stdout),
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    let stdout = String::from_utf8(rebuilt.stdout)?;
    for needle in [
        "rebuild OK",
        "schema_version 14",
        "integrity_check: ok",
        "foreign_key_check: clean",
        "sample frames (2 requested",
        "images: 1 referenced file(s) missing",
    ] {
        assert!(stdout.contains(needle), "missing `{needle}` in:\n{stdout}");
    }
    assert!(dest.is_file());

    let counts_after = std::process::Command::new(binary)
        .arg("--counts")
        .arg(&dest)
        .output()?;
    assert!(counts_after.status.success(), "{counts_after:?}");
    let stdout = String::from_utf8(counts_after.stdout)?;
    assert!(stdout.contains("frame_text"), "{stdout}");
    assert!(stdout.contains("jobs"), "{stdout}");
    Ok(())
}
