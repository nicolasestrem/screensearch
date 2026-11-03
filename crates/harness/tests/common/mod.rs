//! Synthetic v14-shaped databases for the P4 eval tests.
//!
//! **Fixture only.** Every test builds its database in a temp directory; nothing here reads,
//! copies or references a real database, profile, model or token. The DDL mirrors the columns the
//! eval runner queries in `03 §4` (frames / sessions / frame_text + FTS / settings / entities),
//! not the whole baseline: a test proves the generators respond to data, it does not re-test the
//! store's schema.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

pub const DDL: &str = r#"
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
  created_at    INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
  capture_trigger TEXT,
  session_id    INTEGER REFERENCES sessions(id) ON DELETE SET NULL
);
CREATE INDEX idx_frames_captured_at ON frames(captured_at);
CREATE INDEX idx_frames_session ON frames(session_id);

CREATE TABLE sessions (
  id            INTEGER PRIMARY KEY,
  started_at    INTEGER NOT NULL,
  ended_at      INTEGER,
  kind          TEXT    NOT NULL,
  tool          TEXT,
  host          TEXT,
  context_key   TEXT    NOT NULL,
  title         TEXT,
  summary       TEXT,
  summary_model TEXT,
  confidence    REAL    NOT NULL,
  frozen        INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
  updated_at    INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
CREATE INDEX idx_sessions_time ON sessions(started_at, ended_at);

CREATE TABLE frame_text (
  frame_id            INTEGER PRIMARY KEY REFERENCES frames(id) ON DELETE CASCADE,
  raw_text            TEXT    NOT NULL,
  content_text        TEXT    NOT NULL,
  primary_source      TEXT    NOT NULL,
  filter_version      INTEGER NOT NULL,
  suppressed_count    INTEGER NOT NULL,
  target_window_title TEXT,
  target_app_hint     TEXT,
  created_at          INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
CREATE VIRTUAL TABLE frame_text_fts USING fts5(content_text, content='frame_text',
                                               content_rowid='frame_id', tokenize='porter');
CREATE TRIGGER frame_text_ai AFTER INSERT ON frame_text BEGIN
  INSERT INTO frame_text_fts(rowid, content_text) VALUES (new.frame_id, new.content_text);
END;
CREATE TRIGGER frame_text_ad AFTER DELETE ON frame_text BEGIN
  INSERT INTO frame_text_fts(frame_text_fts, rowid, content_text)
    VALUES ('delete', old.frame_id, old.content_text);
END;
CREATE TRIGGER frame_text_au AFTER UPDATE ON frame_text BEGIN
  INSERT INTO frame_text_fts(frame_text_fts, rowid, content_text)
    VALUES ('delete', old.frame_id, old.content_text);
  INSERT INTO frame_text_fts(rowid, content_text) VALUES (new.frame_id, new.content_text);
END;

CREATE TABLE ocr_text (
  frame_id        INTEGER PRIMARY KEY REFERENCES frames(id) ON DELETE CASCADE,
  text            TEXT NOT NULL,
  mean_confidence REAL NOT NULL,
  engine          TEXT NOT NULL
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE schema_version (version INTEGER NOT NULL);
INSERT INTO schema_version (version) VALUES (14);

CREATE TABLE entities (
  id             INTEGER PRIMARY KEY,
  kind           TEXT NOT NULL,
  canonical_name TEXT NOT NULL,
  normalized_key TEXT NOT NULL,
  source         TEXT NOT NULL,
  confidence     REAL NOT NULL,
  first_seen_at  INTEGER NOT NULL,
  last_seen_at   INTEGER NOT NULL,
  created_at     INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
  updated_at     INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
CREATE TABLE entity_aliases (
  id         INTEGER PRIMARY KEY,
  entity_id  INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  alias      TEXT NOT NULL,
  source     TEXT NOT NULL,
  confidence REAL NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
"#;

/// A writable fixture builder. Times are given as **local wall-clock strings**
/// (`"2026-10-01 07:00:00"`) and resolved through SQLite's timezone math, so every assertion that
/// depends on local dates or the morning window holds on any machine timezone.
pub struct Builder {
    conn: Connection,
    path: PathBuf,
    next_frame: i64,
    next_entity: i64,
}

impl Builder {
    pub fn new(dir: &Path) -> Self {
        let path = dir.join("screensearch.db");
        let conn = Connection::open(&path).expect("open fixture db");
        conn.execute_batch(DDL).expect("create fixture schema");
        Self {
            conn,
            path,
            next_frame: 1,
            next_entity: 1,
        }
    }

    pub fn at(&self, local: &str) -> i64 {
        self.conn
            .query_row("SELECT unixepoch(?1, 'utc') * 1000", [local], |r| {
                r.get::<_, i64>(0)
            })
            .expect("resolve local time")
    }

    pub fn session(&mut self, id: i64, start_local: &str, duration_min: i64, title: Option<&str>) {
        let start = self.at(start_local);
        self.conn
            .execute(
                "INSERT INTO sessions
                 (id, started_at, ended_at, kind, tool, host, context_key, title, confidence)
                 VALUES (?1, ?2, ?3, 'focus', NULL, 'desktop', 'focus:test', ?4, 0.8)",
                params![id, start, start + duration_min * 60_000, title],
            )
            .expect("insert session");
    }

    pub fn frame(
        &mut self,
        at_local: &str,
        session: Option<i64>,
        title: &str,
        url: &str,
        content: &str,
    ) -> i64 {
        let id = self.next_frame;
        self.next_frame += 1;
        let at = self.at(at_local);
        self.conn
            .execute(
                "INSERT INTO frames
                 (id, captured_at, monitor_index, width, height, image_path, content_hash,
                  app_hint, window_title, browser_url, session_id)
                 VALUES (?1, ?2, 0, 1920, 1080, 'f.webp', 'hash', 'TestApp', ?3, ?4, ?5)",
                params![id, at, title, url, session],
            )
            .expect("insert frame");
        self.conn
            .execute(
                "INSERT INTO frame_text
                 (frame_id, raw_text, content_text, primary_source, filter_version, suppressed_count)
                 VALUES (?1, ?2, ?2, 'ocr', 1, 0)",
                params![id, content],
            )
            .expect("insert frame text");
        id
    }

    pub fn setting(&mut self, key: &str, value: &str) {
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .unwrap();
    }

    /// One heuristic entity row; `normalized_key` is the lowercased canonical name.
    pub fn entity(&mut self, kind: &str, canonical: &str) {
        let id = self.next_entity;
        self.next_entity += 1;
        self.conn
            .execute(
                "INSERT INTO entities
                 (id, kind, canonical_name, normalized_key, source, confidence,
                  first_seen_at, last_seen_at)
                 VALUES (?1, ?2, ?3, ?4, 'heuristic', 0.9, 0, 0)",
                params![id, kind, canonical, canonical.to_lowercase()],
            )
            .unwrap();
    }

    pub fn alias(&mut self, entity_id: i64, alias: &str) {
        self.conn
            .execute(
                "INSERT INTO entity_aliases (entity_id, alias, source, confidence)
                 VALUES (?1, ?2, 'heuristic', 0.9)",
                params![entity_id, alias],
            )
            .unwrap();
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_parts(self) -> (PathBuf, Connection) {
        (self.path, self.conn)
    }

    pub fn finish(self) -> PathBuf {
        self.path
    }
}

/// 12 sessions over four local days, three per morning, each carrying a distinct file, document,
/// domain, repository and an FTS-distinctive line. Enough units for every generator, so the
/// determinism and data-following tests have real choices to make.
///
/// Per session `i` (0..12):
/// - frame 0: title `C:\work\case{i}\notes{i}.md - VS Code`, content line
///   `checklist item seven for run {i} is complete`
/// - frame 1: title `github.com/acme/proj{i} - Browser`, url `https://site{i}.example.com/page`,
///   the same content line (so it matches two frames)
/// - frame 2: title `report{i}.pdf - Acrobat`, unrelated content
pub fn rich(dir: &Path) -> PathBuf {
    let mut b = Builder::new(dir);
    fill_rich(&mut b);
    b.finish()
}

/// [`rich`] plus an `api.token` row, for the end-to-end CLI test.
pub fn rich_with_token(dir: &Path) -> PathBuf {
    let mut b = Builder::new(dir);
    fill_rich(&mut b);
    b.setting("api.token", "test-token");
    b.finish()
}

fn fill_rich(b: &mut Builder) {
    for i in 0..12i64 {
        let day = format!("2026-10-{:02}", 1 + i / 3);
        let hour = 7 + (i % 3);
        let start = format!("{day} {hour:02}:00:00");
        b.session(i + 1, &start, 40, Some(&format!("case {i:02}")));

        let minutes = |m: i64| {
            let total = hour * 60 + m;
            format!("{day} {:02}:{:02}:00", total / 60, total % 60)
        };
        let phrase = format!("checklist item seven for run {i:02} is complete");
        b.frame(
            &minutes(0),
            Some(i + 1),
            &format!("C:\\work\\case{i:02}\\notes{i:02}.md - VS Code"),
            "",
            &format!("{phrase}\nsecond line for case {i:02}"),
        );
        b.frame(
            &minutes(1),
            Some(i + 1),
            &format!("github.com/acme/proj{i:02} - Browser"),
            &format!("https://site{i:02}.example.com/page"),
            &phrase,
        );
        b.frame(
            &minutes(2),
            Some(i + 1),
            &format!("report{i:02}.pdf - Acrobat"),
            "",
            &format!("appendix for case {i:02}"),
        );
    }
}

/// A minimal, hand-controlable database: two sessions, a handful of frames, one distinctive line.
pub fn tiny(dir: &Path) -> PathBuf {
    let mut b = Builder::new(dir);
    b.session(1, "2026-05-04 09:00:00", 30, Some("alpha"));
    b.session(2, "2026-05-04 10:00:00", 30, Some("beta"));
    b.frame(
        "2026-05-04 09:02:00",
        Some(1),
        "C:\\work\\alpha\\notes.md - VS Code",
        "",
        "the quarterly migration checklist item seven is done\nsecond line",
    );
    b.frame(
        "2026-05-04 09:10:00",
        Some(1),
        "notes.md - VS Code",
        "",
        "the quarterly migration checklist item seven is done",
    );
    b.frame(
        "2026-05-04 10:05:00",
        Some(2),
        "report.pdf - Acrobat",
        "",
        "unrelated appendix text for the second block",
    );
    b.finish()
}
