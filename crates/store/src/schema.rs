//! The v14 baseline schema (`03 §4`, 0.4.5 D2).
//!
//! There is no migration chain. [`BASELINE_SQL`] is the complete DDL: a fresh
//! database is created from it and `schema_version` is written as
//! [`LATEST_SCHEMA_VERSION`] in the same transaction by [`create_baseline`]. A
//! schema change edits this script and bumps the version. Any database whose
//! `schema_version` is not [`LATEST_SCHEMA_VERSION`] refuses to open; it is
//! rebuilt with the dev-only `ssv2c-rebuild` tool (`03 §4`, 0.4.5 D2/D3).
//!
//! The v1 to v13 migration SQL lives in git history and `CHANGELOG-ARCHIVE.md`.
//! It is deliberately absent here: the v14 schema is v13 plus the eleven 0.4.5
//! tables, carried as one script instead of a replay.

use std::sync::LazyLock;

use rusqlite::Connection;

/// The schema version this build creates and requires (`03 §4`).
pub const LATEST_SCHEMA_VERSION: i32 = 14;

/// Vector dimensionality for every embedding lane (`03 §3/§4`,
/// [`traits::EmbeddingProvider::dim`]).
pub const EMBEDDING_DIM: usize = 768;

/// `frame_text.filter_version` written by PR2's interim passthrough populator
/// (`07` #51): `0` marks "no attention filter applied, `content_text` is a raw copy".
/// PR3's classifier writes [`FILTER_VERSION`] and is bumpable to recompute the chrome
/// catalog (`03 §3b`).
pub const UNFILTERED_FILTER_VERSION: i32 = 0;

/// `frame_text.filter_version` written by PR3's attention filter. Bumping this is the
/// "recompute the chrome catalog" lever (`03 §3b`): on startup
/// [`crate::SqliteStore::backfill_filter_version`] re-cleans every frame below the active
/// version against the now-warm chrome catalog (`textfilter::reconcile`), so the live
/// classifier's cold-start window no longer leaves app/nav chrome permanently in
/// `content_text` (the 2026-06-26 PR3 audit blocker;
/// `docs/AUDIT_0.2.0_PR3_2026-06-26.md`). v2 turns on that backfill (v1 only wiped the
/// catalog and left old frames dirty).
pub const FILTER_VERSION: i32 = 2;

/// The complete v14 DDL (`03 §4`).
///
/// Every `CREATE` here is as-built v13 plus the eleven 0.4.5 tables. Four objects
/// have no counterpart in `03 §4` (a recorded documentation gap): the report
/// tables `report_runs`, `report_deliveries`, `report_files` (their exact DDL is
/// owned by the store module, `03 §8c`) and the raw-text FTS mirror
/// `frame_text_raw_fts` (`03 §4` describes it only in prose). They are carried
/// unchanged because the arc adds tables, it does not reshape the old ones.
///
/// `07` #51 "clean DB" history is folded in: `ocr_text` / `ocr_text_fts` (dropped
/// in v3) and the image-embedding lane (dropped in v9) are simply absent.
///
/// The two `vec0` virtual tables and the three FTS5 mirrors require their
/// extensions on the connection. [`create_baseline`] must be handed a connection
/// opened after `crate::register_vec_extension`; the store's open path guarantees
/// that.
pub const BASELINE_SQL: &str = r#"
-- ============================================================================
-- Core spine
-- ============================================================================

-- schema_version: one row, written with the baseline in the same transaction
CREATE TABLE schema_version (version INTEGER NOT NULL);

-- settings: opaque key/value pairs (`03 §8`; the typed map lives in traits)
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- sessions (0.4.0 PR3, `§7e`). Derived-but-persisted: a wiped `sessions` table is
-- fully recomputable from frames (D1). Segmentation is pure heuristic (zero model
-- calls, D3); titles/summaries are lazily generated and cached on the row.
CREATE TABLE sessions (
  id            INTEGER PRIMARY KEY,
  started_at    INTEGER NOT NULL,                 -- unix ms (first frame in the run)
  ended_at      INTEGER,                          -- NULL = open (still accreting)
  kind          TEXT NOT NULL CHECK (kind IN ('focus','meeting','ai','other')),
  tool          TEXT CHECK (tool IS NULL OR kind = 'ai'),  -- taxonomy id; NULL unless kind='ai' (D7)
  host          TEXT CHECK (host IN ('terminal','desktop','browser','ide')),
  context_key   TEXT NOT NULL,                    -- closed task-level grammar (06 #27/#28):
                                                  --   'ai:<tool-id>' | 'meeting:<taxonomy-id>' |
                                                  --   'focus:<dominant-app-stem>' (bare 'focus' when
                                                  --   no stem qualifies); the §7b key generalized (§7e)
  title         TEXT,                             -- lazily generated and cached (D3)
  summary       TEXT,                             -- lazily generated and cached (D3)
  summary_model TEXT,                             -- model id that produced `summary`
  confidence    REAL NOT NULL,                    -- segmenter confidence, surfaced honestly (D11)
  frozen        INTEGER NOT NULL DEFAULT 0 CHECK (frozen IN (0,1)),  -- D2 freeze rule
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_sessions_time ON sessions(started_at, ended_at);

-- frames: one row per stored (changed) capture
CREATE TABLE frames (
  id            INTEGER PRIMARY KEY,
  captured_at   INTEGER NOT NULL,          -- unix ms
  monitor_index INTEGER NOT NULL,
  width         INTEGER NOT NULL,
  height        INTEGER NOT NULL,
  image_path    TEXT    NOT NULL,          -- relative path to JPEG on disk
  content_hash  TEXT    NOT NULL,
  app_hint      TEXT, window_title TEXT, browser_url TEXT,  -- context (nullable)
  activity_type TEXT,                       -- filled by vision (nullable)
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  -- Event-capture label (0.2.1, `07` #47). The widened token set keeps legacy rows
  -- readable (0.3.0 D2, no schema change); new frames emit 'timer'/'idle'/
  -- 'foreground_change' only.
  capture_trigger TEXT
    CHECK (capture_trigger IS NULL
           OR capture_trigger IN ('timer','idle','foreground_change','clipboard_change',
                                  'typing_pause','click','scroll_stop','manual')),
  -- Degrade-to-text retention (0.2.x, `storage.retention_days`): 0 while the image is
  -- on disk, 1 once the sweep deleted the file but kept the row, text and embeddings.
  image_purged  INTEGER NOT NULL DEFAULT 0 CHECK (image_purged IN (0,1)),
  -- Assigned by the segmenter, never by a migration. Frames SURVIVE session deletion.
  session_id    INTEGER REFERENCES sessions(id) ON DELETE SET NULL
);
CREATE INDEX idx_frames_captured_at ON frames(captured_at);
-- Partial index for the retention sweep: contains only still-imaged rows, so the
-- hourly candidate scan is O(matched), independent of purged history.
CREATE INDEX idx_frames_image_retention ON frames(captured_at) WHERE image_purged = 0;
CREATE INDEX idx_frames_session ON frames(session_id);

-- frame_text: preserved raw text + filtered default-retrieval text, one row per frame
CREATE TABLE frame_text (
  frame_id            INTEGER PRIMARY KEY REFERENCES frames(id) ON DELETE CASCADE,
  raw_text            TEXT    NOT NULL,          -- full unfiltered OCR/UIA text (preserved)
  content_text        TEXT    NOT NULL,          -- filtered text (NOT vision); default retrieval input
  primary_source      TEXT    NOT NULL CHECK (primary_source IN ('ocr','uia')),
  filter_version      INTEGER NOT NULL,          -- bump to recompute the chrome catalog
  suppressed_count    INTEGER NOT NULL,          -- spans dropped from content_text
  target_window_title TEXT,                      -- foreground window title (metadata, nullable)
  target_app_hint     TEXT,                      -- foreground app hint (metadata, nullable)
  created_at          INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);

-- Default search FTS mirrors content_text (porter), external-content over frame_text.
CREATE VIRTUAL TABLE frame_text_fts USING fts5(content_text, content='frame_text',
                                               content_rowid='frame_id', tokenize='porter');
CREATE TRIGGER frame_text_ai AFTER INSERT ON frame_text BEGIN
  INSERT INTO frame_text_fts(rowid, content_text) VALUES (new.frame_id, new.content_text);
END;
CREATE TRIGGER frame_text_ad AFTER DELETE ON frame_text BEGIN
  INSERT INTO frame_text_fts(frame_text_fts, rowid, content_text) VALUES('delete', old.frame_id, old.content_text);
END;
CREATE TRIGGER frame_text_au AFTER UPDATE ON frame_text BEGIN
  INSERT INTO frame_text_fts(frame_text_fts, rowid, content_text) VALUES('delete', old.frame_id, old.content_text);
  INSERT INTO frame_text_fts(rowid, content_text) VALUES (new.frame_id, new.content_text);
END;

-- Raw FTS mirror (porter), searched only when include_chrome=true (`03 §3b`).
CREATE VIRTUAL TABLE frame_text_raw_fts USING fts5(raw_text, content='frame_text',
                                                   content_rowid='frame_id', tokenize='porter');
CREATE TRIGGER frame_text_raw_ai AFTER INSERT ON frame_text BEGIN
  INSERT INTO frame_text_raw_fts(rowid, raw_text) VALUES (new.frame_id, new.raw_text);
END;
CREATE TRIGGER frame_text_raw_ad AFTER DELETE ON frame_text BEGIN
  INSERT INTO frame_text_raw_fts(frame_text_raw_fts, rowid, raw_text) VALUES('delete', old.frame_id, old.raw_text);
END;
CREATE TRIGGER frame_text_raw_au AFTER UPDATE ON frame_text BEGIN
  INSERT INTO frame_text_raw_fts(frame_text_raw_fts, rowid, raw_text) VALUES('delete', old.frame_id, old.raw_text);
  INSERT INTO frame_text_raw_fts(rowid, raw_text) VALUES (new.frame_id, new.raw_text);
END;

-- text_spans: per-frame OCR/UIA spans with normalized geometry + classified role
CREATE TABLE text_spans (
  frame_id        INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  span_index      INTEGER NOT NULL,
  text            TEXT    NOT NULL,
  normalized_text TEXT    NOT NULL,
  source          TEXT    NOT NULL CHECK (source IN ('ocr','uia')),
  role            TEXT    NOT NULL CHECK (role IN ('content','chrome','background','system','unknown')),
  x REAL NOT NULL, y REAL NOT NULL, w REAL NOT NULL, h REAL NOT NULL,  -- normalized [0,1] bbox
  is_searchable   INTEGER NOT NULL CHECK (is_searchable IN (0,1)),
  suppress_reason TEXT CHECK (suppress_reason IS NULL
                              OR suppress_reason IN ('static_chrome','system_ui','background_window')),
  line_index      INTEGER NOT NULL DEFAULT 0,    -- OCR line grouping (0.2.x attention filter)
  PRIMARY KEY (frame_id, span_index)
);

-- chrome_text_catalog: signature counter that drives static-chrome suppression
CREATE TABLE chrome_text_catalog (
  signature       TEXT PRIMARY KEY,              -- app_hint + normalized_text + region_bucket
  app_hint        TEXT,
  region_bucket   TEXT,
  normalized_text TEXT    NOT NULL,
  seen_count      INTEGER NOT NULL,
  first_seen_at   INTEGER NOT NULL,
  last_seen_at    INTEGER NOT NULL,
  suppressed      INTEGER NOT NULL DEFAULT 0 CHECK (suppressed IN (0,1))
);

-- vision analysis (deferred, optional, one row per analyzed frame)
CREATE TABLE vision_analysis (
  frame_id     INTEGER PRIMARY KEY REFERENCES frames(id) ON DELETE CASCADE,
  description  TEXT NOT NULL, activity_type TEXT, app_hint TEXT,
  confidence   REAL NOT NULL, model TEXT NOT NULL,
  created_at   INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);

-- text embeddings: metadata + sqlite-vec index
CREATE TABLE embeddings (
  id           INTEGER PRIMARY KEY,
  frame_id     INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  chunk_index  INTEGER NOT NULL,
  chunk_text   TEXT NOT NULL,
  source       TEXT NOT NULL,               -- 'ocr' | 'vision_description'
  model        TEXT NOT NULL, dim INTEGER NOT NULL,
  content_hash TEXT NOT NULL,               -- skip re-embed if unchanged
  UNIQUE(frame_id, chunk_index)
);
CREATE INDEX idx_embeddings_frame ON embeddings(frame_id);
CREATE VIRTUAL TABLE embedding_vectors USING vec0(
  embedding_id INTEGER PRIMARY KEY,         -- == embeddings.id
  embedding    FLOAT[768] distance_metric=cosine
);
-- keep the vec0 shadow in sync when an embeddings row is removed (incl. via the
-- frames ON DELETE CASCADE; recursive_triggers is enabled per-connection)
CREATE TRIGGER embeddings_ad AFTER DELETE ON embeddings BEGIN
  DELETE FROM embedding_vectors WHERE embedding_id = old.id;
END;

-- durable job queue (the heart of enrich-deferred), see `§5`
CREATE TABLE jobs (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL,               -- 'embed_text' | 'vision_tag'; no CHECK (07 #82)
  frame_id     INTEGER REFERENCES frames(id) ON DELETE CASCADE,
  state        TEXT NOT NULL DEFAULT 'pending', -- pending|running|done|failed|dead
  priority     INTEGER NOT NULL DEFAULT 0,  -- higher first
  attempts     INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 3,
  not_before   INTEGER NOT NULL DEFAULT 0,  -- unix ms (scheduling + backoff)
  last_error   TEXT,
  created_at   INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at   INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_jobs_ready ON jobs(state, not_before, priority DESC, id);
-- the vision scheduler's per-frame pending dedup (`NOT EXISTS` over jobs)
CREATE INDEX idx_jobs_frame_kind_state ON jobs(frame_id, kind, state);

-- ============================================================================
-- 0.4.5 work graph + Ask v2 + insights (`03 §4`, §7h/§7i/§7j)
-- Created empty: P5, P6 and P8 are the first writers. P3 adds no query module.
-- ============================================================================

-- Work graph (0.4.5 D8): entities extracted from captured text, linked to sessions.
CREATE TABLE entities (
  id             INTEGER PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('repo','file','domain','document','ref','app')),
  canonical_name TEXT NOT NULL,
  normalized_key TEXT NOT NULL,          -- lowercase, trimmed, path/URL-normalized; the dedup identity
  source         TEXT NOT NULL CHECK (source IN ('heuristic','llm')),
  confidence     REAL NOT NULL,          -- 0..1; the extraction rule's confidence, never invented
  first_seen_at  INTEGER NOT NULL,       -- unix ms
  last_seen_at   INTEGER NOT NULL,
  created_at     INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at     INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  UNIQUE(kind, normalized_key)
);
CREATE INDEX idx_entities_last_seen ON entities(last_seen_at DESC);
CREATE INDEX idx_entities_kind ON entities(kind, last_seen_at DESC);

CREATE TABLE entity_aliases (
  id         INTEGER PRIMARY KEY,
  entity_id  INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  alias      TEXT NOT NULL,
  source     TEXT NOT NULL CHECK (source IN ('heuristic','llm','user')),
  confidence REAL NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  UNIQUE(entity_id, alias)
);
CREATE INDEX idx_entity_aliases_alias ON entity_aliases(alias);

-- session <-> entity link. A session touches an entity at most once; hit_count is the
-- evidence weight (distinct frames).
CREATE TABLE session_entities (
  session_id    INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  entity_id     INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  hit_count     INTEGER NOT NULL DEFAULT 1,
  confidence    REAL NOT NULL,
  first_seen_at INTEGER NOT NULL,
  last_seen_at  INTEGER NOT NULL,
  PRIMARY KEY(session_id, entity_id)
);
CREATE INDEX idx_session_entities_entity ON session_entities(entity_id, last_seen_at DESC);

-- Threads: clusters of sessions sharing strong entities within a decaying window (`§7h`).
CREATE TABLE threads (
  id            INTEGER PRIMARY KEY,
  title         TEXT NOT NULL,
  cluster_key   TEXT NOT NULL UNIQUE,    -- deterministic clusterer identity; rebuilt, not edited
  first_seen_at INTEGER NOT NULL,
  last_seen_at  INTEGER NOT NULL,
  session_count INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_threads_last_seen ON threads(last_seen_at DESC);

-- A session belongs to at most ONE thread (UNIQUE session_id); split/merge rewrite rows.
CREATE TABLE thread_sessions (
  thread_id  INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  session_id INTEGER NOT NULL UNIQUE REFERENCES sessions(id) ON DELETE CASCADE,
  confidence REAL NOT NULL,
  PRIMARY KEY(thread_id, session_id)
);
CREATE INDEX idx_thread_sessions_thread ON thread_sessions(thread_id);

-- User overrides always beat the clusterer (`§7h`). payload is typed JSON per kind.
CREATE TABLE thread_overrides (
  id               INTEGER PRIMARY KEY,
  kind             TEXT NOT NULL CHECK (kind IN ('rename','merge','split')),
  thread_id        INTEGER REFERENCES threads(id) ON DELETE CASCADE,
  target_thread_id INTEGER REFERENCES threads(id) ON DELETE CASCADE,
  payload          TEXT NOT NULL,        -- rename: {"title":...}; merge: {"session_ids":[...]}
                                         -- split: {"thread_id":...,"session_ids":[...]}
  created_at       INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  CHECK ((kind = 'rename' AND thread_id IS NOT NULL AND target_thread_id IS NULL)
      OR (kind = 'merge'  AND thread_id IS NOT NULL AND target_thread_id IS NOT NULL)
      OR (kind = 'split'  AND thread_id IS NOT NULL))
);

-- Session vectors (`§7i`): session title+summary embeddings, same model + dim as text.
CREATE VIRTUAL TABLE session_vectors USING vec0(
  session_id INTEGER PRIMARY KEY,        -- == sessions.id
  embedding  FLOAT[768] distance_metric=cosine
);

-- Insights (`§7j`): the typed payload is the truth; the model never owns a row.
CREATE TABLE insights (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL CHECK (kind IN ('thread_activity','open_loop','recurrence','semantic_neighbour','resumption')),
  window_start INTEGER NOT NULL,         -- unix ms
  window_end   INTEGER NOT NULL,
  payload      TEXT NOT NULL,            -- typed JSON, rendered without the model
  generator    TEXT NOT NULL CHECK (generator IN ('heuristic','llm_phrased')),
  generated_at INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  stale        INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0,1))
);
CREATE INDEX idx_insights_kind ON insights(kind, generated_at DESC);
CREATE INDEX idx_insights_window ON insights(window_start, window_end);

-- An insight without evidence is a defect (`§7j`). SQLite cannot defer a cross-table
-- rule, so the store API enforces it (insight + evidence in one transaction; an empty
-- evidence list is an error).
CREATE TABLE insight_evidence (
  insight_id INTEGER NOT NULL REFERENCES insights(id) ON DELETE CASCADE,
  ordinal    INTEGER NOT NULL,
  kind       TEXT NOT NULL CHECK (kind IN ('frame','session','thread','entity','mark','report')),
  ref_id     INTEGER NOT NULL,
  PRIMARY KEY(insight_id, ordinal)
);

-- Ask v2 conversations (`§7i`).
CREATE TABLE ask_threads (
  id         INTEGER PRIMARY KEY,
  title      TEXT,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE TABLE ask_turns (
  id                INTEGER PRIMARY KEY,
  thread_id         INTEGER NOT NULL REFERENCES ask_threads(id) ON DELETE CASCADE,
  ordinal           INTEGER NOT NULL,
  question          TEXT NOT NULL,
  answer            TEXT,                -- NULL while streaming / on failure
  window_start      INTEGER,
  window_end        INTEGER,
  resolved_entities TEXT,                -- JSON array of entity ids resolved for this turn
  created_at        INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  UNIQUE(thread_id, ordinal)
);

-- ============================================================================
-- Tagging, marks, artifacts, reports
-- ============================================================================

CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL);
CREATE TABLE frame_tags (frame_id INTEGER REFERENCES frames(id) ON DELETE CASCADE,
                         tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
                         PRIMARY KEY(frame_id, tag_id));

-- marks (0.3.0 PR6, `§7b`): user-flagged frames + optional intention note. CASCADEs with
-- the frame; a retention-degraded frame keeps its row, so a mark survives image expiry.
CREATE TABLE marks (
  id          INTEGER PRIMARY KEY,
  frame_id    INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  created_at  INTEGER NOT NULL,
  note        TEXT,                          -- optional one-line intention (nullable)
  resolved_at INTEGER                        -- NULL = unresolved; set on resolve/dismiss
);
-- list_marks order: unresolved first (NULLs sort low), newest-first within each group.
CREATE INDEX idx_marks_open ON marks(resolved_at, created_at DESC);

-- the artifact extension point (0.4.0 D8). 'exchange' is the only kind used; 'transcript'
-- and 'note' are reserved. The role CHECK is load-bearing: NULL IN (...) is NULL, and
-- SQLite passes a CHECK that evaluates to NULL, so the IS NOT NULL guard is explicit.
CREATE TABLE session_artifacts (
  id         INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('exchange','transcript','note')),
  role       TEXT CHECK ((kind = 'exchange' AND role IS NOT NULL AND role IN ('user','agent')) OR (kind IN ('transcript','note') AND role IS NULL)),
  frame_id   INTEGER REFERENCES frames(id) ON DELETE SET NULL,
  content    TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_artifacts_session ON session_artifacts(session_id, created_at);
-- index the frame_id FK: SQLite does not auto-index FKs and frame_id is ON DELETE SET
-- NULL, so a frame retention delete would otherwise full-scan this table.
CREATE INDEX idx_artifacts_frame ON session_artifacts(frame_id);

-- Durable report snapshots (0.4.0 PR2; exact DDL owned by this module, `03 §8c`).
-- Evidence ids are retained without evidence FKs by design.
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
 kind TEXT NOT NULL DEFAULT 'daily'
 CHECK(kind IN ('daily','weekly_synthesis'))
);
CREATE UNIQUE INDEX idx_report_schedule_window ON report_runs(schedule_id, window_start, window_end)
 WHERE schedule_id IS NOT NULL AND revision_of IS NULL;
CREATE INDEX idx_report_runs_created ON report_runs(created_at DESC);
CREATE INDEX idx_report_runs_kind_created ON report_runs(kind, created_at DESC);

CREATE TABLE report_deliveries (
 id TEXT PRIMARY KEY NOT NULL,
 run_id TEXT NOT NULL REFERENCES report_runs(run_id) ON DELETE CASCADE,
 state TEXT NOT NULL,
 next_attempt_at INTEGER,
 payload TEXT NOT NULL
);
CREATE INDEX idx_report_deliveries_run ON report_deliveries(run_id);

CREATE TABLE report_files (
 path TEXT PRIMARY KEY NOT NULL,
 payload TEXT NOT NULL
);
"#;

/// Creates the v14 database: executes [`BASELINE_SQL`] and writes
/// `schema_version = LATEST_SCHEMA_VERSION` in one transaction.
///
/// The connection must already have the `sqlite-vec` extension registered (the
/// store's open path calls `crate::register_vec_extension` before opening it);
/// `session_vectors` and `embedding_vectors` cannot be created without it.
pub fn create_baseline(conn: &Connection) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(BASELINE_SQL)?;
    tx.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [LATEST_SCHEMA_VERSION],
    )?;
    tx.commit()
}

/// Every table [`BASELINE_SQL`] declares, parsed once from the script itself: no
/// hand-maintained list can drift from the DDL. The open path requires all of them in
/// an existing database, so a truncated or partially restored file is refused instead
/// of failing later with a misleading query error (`03 §4`, 0.4.5 D2).
pub(crate) static BASELINE_TABLE_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut names = Vec::new();
    for prefix in ["CREATE TABLE ", "CREATE VIRTUAL TABLE "] {
        let mut rest = BASELINE_SQL;
        while let Some(pos) = rest.find(prefix) {
            let after = &rest[pos + prefix.len()..];
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            debug_assert!(
                !name.is_empty(),
                "baseline has a table statement with no parsable name"
            );
            rest = &after[name.len()..];
            names.push(name);
        }
    }
    names
});
