//! Forward-only schema migrations (`03 Â§4/Â§12`).
//!
//! The authoritative DDL lives here. Migrations are an append-only list of
//! `(version, sql)` pairs applied in order; the current version is tracked in the
//! `schema_version` table (created by the bootstrap in [`crate`], not by a
//! migration). To evolve the schema, **append** a new `(n, "...")` entry â€” never
//! edit a shipped one (no schema drift).

/// The highest migration version this build knows how to reach.
pub const LATEST_SCHEMA_VERSION: i32 = 12;

/// Vector dimensionality for every embedding lane (`03 Â§3/Â§4`,
/// [`traits::EmbeddingProvider::dim`]).
pub const EMBEDDING_DIM: usize = 768;

/// `frame_text.filter_version` written by PR2's interim passthrough populator
/// (`07` #51): `0` marks "no attention filter applied â€” `content_text` is a raw copy".
/// PR3's classifier writes [`FILTER_VERSION`] and is bumpable to recompute the chrome
/// catalog (`03 Â§3b`).
pub const UNFILTERED_FILTER_VERSION: i32 = 0;

/// `frame_text.filter_version` written by PR3's attention filter. Bumping this is the
/// "recompute the chrome catalog" lever (`03 Â§3b`): on startup
/// [`SqliteStore::backfill_filter_version`] re-cleans every frame below the active
/// version against the now-warm chrome catalog (`textfilter::reconcile`), so the live
/// classifier's cold-start window no longer leaves app/nav chrome permanently in
/// `content_text` (the 2026-06-26 PR3 audit blocker;
/// `docs/AUDIT_0.2.0_PR3_2026-06-26.md`). v2 turns on that backfill (v1 only wiped the
/// catalog and left old frames dirty).
pub const FILTER_VERSION: i32 = 2;

/// Ordered, forward-only migrations. Each is applied in its own transaction when
/// the DB's tracked version is below it.
pub const MIGRATIONS: &[(i32, &str)] = &[
    (1, MIGRATION_V1),
    (2, MIGRATION_V2),
    (3, MIGRATION_V3),
    (4, MIGRATION_V4),
    (5, MIGRATION_V5),
    (6, MIGRATION_V6),
    (7, MIGRATION_V7),
    (8, MIGRATION_V8),
    (9, MIGRATION_V9),
    (10, MIGRATION_V10),
    (11, MIGRATION_V11),
    (12, MIGRATION_V12),
];

/// v1 â€” the full data spine (`03 Â§4`, transcribed verbatim, plus the FTS5 and
/// vector-sync triggers the spec describes in prose).
const MIGRATION_V1: &str = r#"
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
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_frames_captured_at ON frames(captured_at);

-- OCR text (one row per frame) + FTS5 mirror
CREATE TABLE ocr_text (
  frame_id        INTEGER PRIMARY KEY REFERENCES frames(id) ON DELETE CASCADE,
  text            TEXT NOT NULL,
  mean_confidence REAL NOT NULL,
  engine          TEXT NOT NULL
);
CREATE VIRTUAL TABLE ocr_text_fts USING fts5(text, content='ocr_text', content_rowid='frame_id',
                                             tokenize='porter');
-- external-content sync triggers (standard FTS5 pattern)
CREATE TRIGGER ocr_text_ai AFTER INSERT ON ocr_text BEGIN
  INSERT INTO ocr_text_fts(rowid, text) VALUES (new.frame_id, new.text);
END;
CREATE TRIGGER ocr_text_ad AFTER DELETE ON ocr_text BEGIN
  INSERT INTO ocr_text_fts(ocr_text_fts, rowid, text) VALUES('delete', old.frame_id, old.text);
END;
CREATE TRIGGER ocr_text_au AFTER UPDATE ON ocr_text BEGIN
  INSERT INTO ocr_text_fts(ocr_text_fts, rowid, text) VALUES('delete', old.frame_id, old.text);
  INSERT INTO ocr_text_fts(rowid, text) VALUES (new.frame_id, new.text);
END;

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

-- image embeddings (optional visual recall): metadata + sqlite-vec index
CREATE TABLE image_embeddings (
  id        INTEGER PRIMARY KEY,
  frame_id  INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  model     TEXT NOT NULL, dim INTEGER NOT NULL,
  UNIQUE(frame_id)
);
CREATE VIRTUAL TABLE image_embedding_vectors USING vec0(
  image_embedding_id INTEGER PRIMARY KEY,   -- == image_embeddings.id
  embedding          FLOAT[768] distance_metric=cosine
);
CREATE TRIGGER image_embeddings_ad AFTER DELETE ON image_embeddings BEGIN
  DELETE FROM image_embedding_vectors WHERE image_embedding_id = old.id;
END;

-- durable job queue (the heart of enrich-deferred) â€” see 03 Â§5
CREATE TABLE jobs (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL,               -- 'embed_text' | 'embed_image' | 'vision_tag'
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

-- tagging, settings
CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL);
CREATE TABLE frame_tags (frame_id INTEGER REFERENCES frames(id) ON DELETE CASCADE,
                         tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
                         PRIMARY KEY(frame_id, tag_id));
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

/// v2 â€” index supporting the vision scheduler's pending-job dedup. The
/// `untagged_frame_ids` query runs a correlated `NOT EXISTS` over `jobs` keyed by
/// `frame_id` (+ `kind`/`state`) for every candidate frame; without this the subquery
/// scans the whole `jobs` table per candidate, which degrades as completed/dead rows
/// accumulate (no purge yet). Index-only, no data change.
const MIGRATION_V2: &str = r#"
CREATE INDEX IF NOT EXISTS idx_jobs_frame_kind_state ON jobs(frame_id, kind, state);
"#;

/// v3 â€” 0.2.x text signal (`03 Â§3b`/`Â§4`, `docs/0.2.0.md` PR2). Splits the single
/// unfiltered OCR string into a preserved `raw_text` layer and a filtered
/// `content_text` layer, adds per-word `text_spans` (normalized geometry + role) and
/// the `chrome_text_catalog` PR3's static-chrome suppression will drive.
///
/// Clean-DB assumption (`07` #51): `ocr_text` is empty in every install reaching v3,
/// so it is dropped â€” `frame_text.raw_text` becomes the single raw store
/// (`03 Â§4` "legacy ocr_text â€¦ not required going forward"). The `DROP â€¦ IF EXISTS`
/// form keeps the migration idempotent. `include_chrome=true` searches raw via a
/// dedicated raw FTS5 mirror (`frame_text_raw_fts`); roles aren't populated until PR3,
/// so a role-filtered spans FTS would be meaningless now (`05`).
///
/// FTS5 + trigger style mirrors v1's `ocr_text_fts` exactly: external-content over
/// `frame_text` keyed on `frame_id`, kept in sync by AFTER INSERT/DELETE/UPDATE
/// triggers (the `'delete'` command form purges the old row).
const MIGRATION_V3: &str = r#"
-- clean-DB: retire the legacy single-string OCR store (frame_text replaces it)
DROP TRIGGER IF EXISTS ocr_text_au;
DROP TRIGGER IF EXISTS ocr_text_ad;
DROP TRIGGER IF EXISTS ocr_text_ai;
DROP TABLE   IF EXISTS ocr_text_fts;
DROP TABLE   IF EXISTS ocr_text;

-- frame_text: preserved raw text + filtered default-retrieval text, one row per frame
CREATE TABLE frame_text (
  frame_id            INTEGER PRIMARY KEY REFERENCES frames(id) ON DELETE CASCADE,
  raw_text            TEXT    NOT NULL,          -- full unfiltered OCR/UIA text (preserved)
  content_text        TEXT    NOT NULL,          -- filtered text (NOT vision); default retrieval input
  primary_source      TEXT    NOT NULL CHECK (primary_source IN ('ocr','uia')),
  filter_version      INTEGER NOT NULL,          -- bump to recompute the chrome catalog
  suppressed_count    INTEGER NOT NULL,          -- spans dropped from content_text (suppression-rate metric)
  target_window_title TEXT,                      -- foreground window title (metadata, nullable)
  target_app_hint     TEXT,                      -- foreground app hint (metadata, nullable)
  created_at          INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);

-- default search FTS mirrors content_text (porter), external-content over frame_text
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

-- raw FTS mirror (porter) â€” searched only when include_chrome=true (03 Â§3b/Â§4)
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
  PRIMARY KEY (frame_id, span_index)
);

-- chrome_text_catalog: signature counter that drives static-chrome suppression (PR3)
CREATE TABLE chrome_text_catalog (
  signature       TEXT PRIMARY KEY,              -- app_hint + normalized_text + region_bucket
  app_hint        TEXT,
  region_bucket   TEXT,
  normalized_text TEXT    NOT NULL,
  seen_count      INTEGER NOT NULL,
  first_seen_at   INTEGER NOT NULL,
  last_seen_at    INTEGER NOT NULL,
  suppressed      INTEGER NOT NULL DEFAULT 0 CHECK (suppressed IN (0,1))  -- 0/1; marked chrome after a configurable threshold (Â§8)
);
"#;

/// v4 â€” PR3 attention filter (`03 Â§3b`, `docs/0.2.0.md` PR3). Carries the OCR
/// `line_index` onto each span so the classifier groups words into lines exactly
/// (the engine already computed line boundaries; PR2 flattened spans to words and
/// dropped them). Clean-DB: `text_spans` is empty at v4, so the `DEFAULT 0` only
/// shapes the column, not existing rows. Index-light: the `(frame_id, span_index)`
/// PK already covers the per-frame read PR3's filter does.
const MIGRATION_V4: &str = r#"
ALTER TABLE text_spans ADD COLUMN line_index INTEGER NOT NULL DEFAULT 0;
"#;

/// v5 â€” record **why** a frame was captured (0.2.1 event-driven capture,
/// `docs/0.2.0.md`, `07` #47/#57). Nullable: frames captured before this column
/// existed read back as `NULL` ("unknown/legacy"); new captures always write a token
/// from [`traits::CaptureTrigger::as_db_str`] (`timer` in the 0.2.0 timer/idle path).
/// The `CHECK` mirrors the other closed-set TEXT columns (`primary_source`, `role`,
/// `suppress_reason`): an invalid token from a future bug fails loudly at write time
/// instead of silently degrading to `NULL` via `from_db_str` and losing provenance.
/// SQLite enforces the constraint on new inserts/updates only, so existing `NULL`
/// rows added by this `ADD COLUMN` need no data migration.
const MIGRATION_V5: &str = r#"
ALTER TABLE frames ADD COLUMN capture_trigger TEXT
  CHECK (capture_trigger IS NULL
         OR capture_trigger IN ('timer','idle','foreground_change','clipboard_change','typing_pause','manual'));
"#;

/// v6 â€” widen `frames.capture_trigger`'s CHECK to admit the `click` / `scroll_stop`
/// tokens of the event-driven remainder (`docs/0.2.0.md`, `07` #47). SQLite can't alter a
/// column's CHECK in place, so this rebuilds `frames` via the documented recipe
/// (create-new â†’ copy â†’ drop-old â†’ rename â†’ recreate index). `frames` is a **parent**
/// table with many `ON DELETE CASCADE` children, so the rebuild is only safe with foreign
/// keys disabled â€” otherwise `DROP TABLE frames` fires an implicit cascade DELETE that
/// wipes every child row. The migration runner ([`crate::bootstrap_and_migrate`]) disables
/// foreign keys around the whole migration phase and runs `foreign_key_check` afterward
/// (PRAGMA foreign_keys is a no-op inside the per-migration transaction). The `frames_new`
/// copy lists every column **explicitly** (not `SELECT *`) so it stays correct regardless of
/// any future column reordering; only the CHECK token set changes here. The `frames_new`
/// column list and order still match v1 + v5. Index `idx_frames_captured_at` is dropped with the old table
/// and recreated. No other object references `frames` by name except the children's FK
/// clauses, which rebind to the renamed table.
const MIGRATION_V6: &str = r#"
CREATE TABLE frames_new (
  id            INTEGER PRIMARY KEY,
  captured_at   INTEGER NOT NULL,
  monitor_index INTEGER NOT NULL,
  width         INTEGER NOT NULL,
  height        INTEGER NOT NULL,
  image_path    TEXT    NOT NULL,
  content_hash  TEXT    NOT NULL,
  app_hint      TEXT, window_title TEXT, browser_url TEXT,
  activity_type TEXT,
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  capture_trigger TEXT
    CHECK (capture_trigger IS NULL
           OR capture_trigger IN ('timer','idle','foreground_change','clipboard_change',
                                  'typing_pause','click','scroll_stop','manual'))
);
INSERT INTO frames_new
  (id, captured_at, monitor_index, width, height, image_path, content_hash,
   app_hint, window_title, browser_url, activity_type, created_at, capture_trigger)
SELECT
   id, captured_at, monitor_index, width, height, image_path, content_hash,
   app_hint, window_title, browser_url, activity_type, created_at, capture_trigger
FROM frames;
DROP INDEX IF EXISTS idx_frames_captured_at;
DROP TABLE frames;
ALTER TABLE frames_new RENAME TO frames;
CREATE INDEX idx_frames_captured_at ON frames(captured_at);
"#;

/// v7 â€” degrade-to-text retention (`storage.retention_days`). Adds `frames.image_purged`:
/// `0` while the screenshot is on disk, `1` once the retention sweep has deleted the image
/// file but kept the row + `frame_text` + `text_spans` + embeddings as durable proof. A
/// plain additive `ADD COLUMN` (no table rebuild): existing rows default to `0` ("image
/// present"), and the `CHECK` mirrors the other closed-set columns so a future bug writing
/// a bad value fails loudly. The retention sweep lists candidates via
/// `frames_with_image_older_than` (`image_purged = 0`) so each frame degrades exactly once.
const MIGRATION_V7: &str = r#"
ALTER TABLE frames ADD COLUMN image_purged INTEGER NOT NULL DEFAULT 0
  CHECK (image_purged IN (0,1));
"#;

/// v8 â€” index the degrade-to-text retention sweep. `frames_with_image_older_than` filters
/// `captured_at < cutoff AND image_purged = 0 ORDER BY captured_at`. Under degrade-to-text,
/// purged rows stay in `frames` forever, so against the plain `idx_frames_captured_at` the
/// hourly sweep would scan and skip an ever-growing prefix of already-purged history to find
/// the current candidates (or prove there are none) â€” cost grows with all expired history. A
/// **partial** index over `captured_at` restricted to `image_purged = 0` contains only the
/// candidate rows (and shrinks as frames degrade), so the sweep is O(matched), independent of
/// how much purged history has accumulated. SQLite uses it whenever the query's `WHERE`
/// includes the literal `image_purged = 0` predicate (it does). Index-only, no data change.
const MIGRATION_V8: &str = r#"
CREATE INDEX IF NOT EXISTS idx_frames_image_retention
  ON frames(captured_at) WHERE image_purged = 0;
"#;

/// v9 â€” 0.3.0 PR4: remove the image-embedding lane (`03 Â§4` "0.3.0 migrations",
/// `docs/0.3.0.md` PR4, D5). The lane was dark-launched and flag-off
/// (`enrich.image_embeddings`, default false), so this destroys **derived,
/// re-derivable vectors only** â€” frames, stored images, text, and text embeddings are
/// untouched. Dropping `image_embeddings` takes its `image_embeddings_ad` vec0-sync
/// trigger with it (a `DROP TABLE` fires no `AFTER DELETE` triggers), and dropping the
/// `image_embedding_vectors` vec0 virtual table also removes its shadow tables (sqlite-vec
/// is loaded on every connection before open â€” [`crate::register_vec_extension`]).
/// `embed_image` jobs are deleted in **any** state: the kind no longer exists and
/// [`traits::JobKind`] can no longer parse it, so a leftover row would fail
/// `kind_from_token` at claim/list time. Deliberately **no** `jobs.kind` CHECK and **no**
/// jobs-table rebuild (`07` #82 â€” D5 is drop-tables + delete-jobs only). The V1 image DDL
/// stays frozen in place (append-only migrations, `MIGRATION_V1` is never edited); a fresh
/// DB creates the tables in V1 and drops them here, ending at the same schema as an upgrade.
const MIGRATION_V9: &str = r#"
DROP TABLE image_embedding_vectors;
DROP TABLE image_embeddings;
DELETE FROM jobs WHERE kind = 'embed_image';
"#;

/// v10 â€” 0.3.0 PR6: mark-this-moment (`03 Â§4` "0.3.0 migrations", `docs/0.3.0.md` PR6).
/// One row per user-flagged moment, with an optional intention note. Plain additive DDL
/// (no table rebuild): `marks.frame_id` CASCADEs with the frame like every per-frame
/// child (`frame_text`, `text_spans`, `embeddings`, `jobs`), so a hard-deleted frame
/// takes its marks with it; a retention-*degraded* frame keeps its row, so a mark
/// survives image expiry â€” the mark keeps the text reconstruction reachable, no retention
/// pinning (`03 Â§4`, D10). `idx_marks_open` serves `list_marks`' canonical order â€” the
/// unresolved group first (a partial-style `resolved_at IS NULL` head sorts first because
/// SQL NULLs sort low), newest-first within each group by `created_at DESC` (`03 Â§7`).
const MIGRATION_V10: &str = r#"
CREATE TABLE marks (
  id          INTEGER PRIMARY KEY,
  frame_id    INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  created_at  INTEGER NOT NULL,
  note        TEXT,
  resolved_at INTEGER
);
CREATE INDEX idx_marks_open ON marks(resolved_at, created_at DESC);
"#;

/// v11 â€” 0.4.0 PR3: sessions (`03 Â§4` "0.4.0 migration", `docs/0.4.0.md` Â§3 PR3 â€” the arc's
/// **only** schema change, D4). **Structure only, no backfill**: plain `CREATE TABLE` +
/// `ALTER TABLE ADD COLUMN` + `CREATE INDEX`, no table rebuild â€” fast on a large DB, and it
/// creates **zero** session rows (PR4's segmenter assigns `frames.session_id` over history as a
/// resumable background job, never the migration). Sessions are derived-but-persisted (D1): a
/// wiped `sessions` table is fully recomputable from frames, so `frames.session_id` is
/// `ON DELETE SET NULL` (a frame outlives its session) while `session_artifacts` CASCADE with
/// their session. `session_artifacts` is the artifact extension point (D8): 'exchange' is the
/// only kind used this arc; 'transcript' (the audio arc, D14) and 'note' are reserved â€” made
/// concrete by the schema, not built. The compound `role` CHECK requires a non-NULL
/// 'user'/'agent' role on exchanges and forbids one on reserved kinds; the `role IS NOT NULL`
/// guard is load-bearing â€” `NULL IN (â€¦)` is NULL, and SQLite passes a CHECK that evaluates to
/// NULL (the same reason the bare `host` CHECK admits NULL by design). Additive (D10): nothing
/// existing is touched beyond the new nullable `frames.session_id` column.
const MIGRATION_V11: &str = r#"
-- 0.4.0 sessions (added by the PR3 migration, schema 10 â†’ 11; contract Â§7e). Derived-but-persisted:
-- a wiped `sessions` table is fully recomputable from frames (D1). Segmentation is pure heuristic â€”
-- zero model calls (D3); titles/summaries are lazily generated + cached on the row.
CREATE TABLE sessions (
  id            INTEGER PRIMARY KEY,
  started_at    INTEGER NOT NULL,                 -- unix ms (first frame in the run)
  ended_at      INTEGER,                          -- NULL = open (still accreting)
  kind          TEXT NOT NULL CHECK (kind IN ('focus','meeting','ai','other')),
  tool          TEXT CHECK (tool IS NULL OR kind = 'ai'),  -- taxonomy id ('claude-code','codex',â€¦); NULL unless kind='ai' (D7)
  host          TEXT CHECK (host IN ('terminal','desktop','browser','ide')),
  context_key   TEXT NOT NULL,                    -- closed task-level grammar (06 #27/#28): 'ai:<tool-id>' |
                                                  --   'meeting:<taxonomy-id>' | 'focus:<dominant-app-stem>'
                                                  --   (bare 'focus' when no stem qualifies); the Â§7b key generalized (Â§7e)
  title         TEXT,                             -- lazily generated + cached (D3); NULL until first asked
  summary       TEXT,                             -- lazily generated + cached (D3); NULL until first asked
  summary_model TEXT,                             -- model id that produced `summary` (provenance)
  confidence    REAL NOT NULL,                    -- segmenter confidence; surfaced honestly in the UI (D11)
  frozen        INTEGER NOT NULL DEFAULT 0 CHECK (frozen IN (0,1)),  -- D2 freeze rule: 1 = id + boundaries immutable
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_sessions_time ON sessions(started_at, ended_at);

-- frames link to their session (nullable; ON DELETE SET NULL â€” frames SURVIVE session deletion, D1).
-- Assigned by PR4's segmenter (incremental + a one-shot historical pass), never by the migration.
ALTER TABLE frames ADD COLUMN session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL;
CREATE INDEX idx_frames_session ON frames(session_id);

-- the artifact extension point (D8). 'exchange' (user-vs-agent turns) is the only kind USED this arc;
-- 'transcript' is RESERVED for the audio arc (D14 â€” made concrete now, not built); 'note' reserved for
-- future annotation. Roles live here, not on spans â€” the Â§3b TextRole (chrome/content) axis is untouched.
CREATE TABLE session_artifacts (
  id         INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('exchange','transcript','note')),
  role       TEXT CHECK ((kind = 'exchange' AND role IS NOT NULL AND role IN ('user','agent')) OR (kind IN ('transcript','note') AND role IS NULL)),  -- exchanges REQUIRE a non-null role, reserved kinds forbid one (D8; Â§7e "roles never invented"). The IS NOT NULL guard is load-bearing: NULL IN (â€¦) is NULL, and SQLite passes a CHECK that evaluates to NULL.
  frame_id   INTEGER REFERENCES frames(id) ON DELETE SET NULL,
  content    TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_artifacts_session ON session_artifacts(session_id, created_at);
-- index the `frame_id` FK: SQLite does not auto-index FKs, and `frame_id` is ON DELETE SET NULL,
-- so a frame retention delete would otherwise full-scan session_artifacts to null matching rows.
CREATE INDEX idx_artifacts_frame ON session_artifacts(frame_id);
"#;

/// Durable report snapshots deliberately retain evidence IDs without evidence FKs.
const MIGRATION_V12: &str = r#"
CREATE TABLE report_runs (
 run_id TEXT PRIMARY KEY NOT NULL,
 schedule_id TEXT,
 revision_of TEXT,
 window_start INTEGER NOT NULL,
 window_end INTEGER NOT NULL CHECK(window_end > window_start),
 phase TEXT NOT NULL CHECK(phase IN ('queued','running','blocked','finished')),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 payload TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_report_schedule_window ON report_runs(schedule_id, window_start, window_end)
 WHERE schedule_id IS NOT NULL AND revision_of IS NULL;
CREATE INDEX idx_report_runs_created ON report_runs(created_at DESC);
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
