# 03 — Master Production Spec

> **Question this file answers:** *"Exactly how should this be built?"* — the engineering truth.
> If something matters in production, it belongs here. Builds on `01_PROJECT_CONTEXT.md` and
> `02_STRATEGIC_PLAN.md`. The agent treats this file as authoritative for *how*; when it is silent
> or self-contradictory, **stop and ask** (see `04`).

---

## 1. System overview

A single desktop process (Tauri 2) hosts a Rust **kernel** that owns a typed event bus and a
registry of **trait-bounded modules**. The only out-of-process component is the **llama.cpp
inference sidecar**, bound to the app via a Windows Job Object. Heavy work is **deferred** into a
durable SQLite **job queue** and run by workers on user-controlled triggers.

```
Tauri WebView2 UI ──(commands/events, ts-rs)── Kernel
                                                 ├── event bus (typed)
   always-on:  CaptureSource → OcrProvider → Store      (cheap)
   deferred:   Store.JobQueue → Workers:
                  • EmbeddingProvider (fastembed, in-proc)
                  • VisionProvider  ─┐
                  • AnswerProvider  ─┴─→ ModelSupervisor → llama.cpp sidecar (Job-Object-bound)
   query:      Store.hybrid_search (FTS5 + vec KNN → RRF) → AnswerProvider → stream to UI
```

## 2. Workspace layout (Cargo + Tauri)

```
screensearch-v2c/
├── src-tauri/                 # Tauri app shell + command handlers + main() (composition root)
│   └── Cargo.toml
├── crates/
│   ├── kernel/                # orchestrator, event bus, ModelSupervisor, worker pool, resume heuristic
│   ├── traits/                # the module contracts + shared domain types (no impls)
│   ├── store/                 # Store/JobQueue impl on SQLite + sqlite-vec + FTS5
│   ├── capture/               # CaptureSource (WGC) + diff gate + privacy gates + event triggers
│   ├── ocr/                   # OcrProvider (WinRT Media.Ocr, STA worker)
│   ├── uia/                   # OcrProvider via UI Automation (target-window text, COM MTA; OCR fallback — 0.2.x)
│   ├── embeddings/            # EmbeddingProvider (fastembed, text-only since 0.3.0 PR4)
│   ├── inference/             # VisionProvider + AnswerProvider (sidecar HTTP client) + supervisor
│   ├── textfilter/            # attention-first span classifier → filtered content_text (0.2.x, §3b)
│   ├── sysmon/                # PressureProbe (CPU GetSystemTimes + GPU PDH) for the enrichment throttle
│   ├── doctor/                # environment smoke-check: WebView2 / Vulkan / llama-server
│   ├── api/                   # opt-in local HTTP API (axum, 127.0.0.1-only, bearer token) + export (0.3.0, §7c)
│   └── mcp/                   # → screensearch-mcp.exe: stdio MCP wrapper over the HTTP API (0.3.0, D13)
├── ui/                        # React 18 + TS + Vite ("Command Deck")
├── specs/
├── Cargo.toml                 # workspace
└── README.md  LICENSE  .gitignore
```

**Dependency rule:** `kernel` and module crates depend on `traits` (contracts), never on each
other's concrete impls. `src-tauri` wires concrete impls into the kernel at startup (composition
root). This is the modularity guarantee.

## 3. Module contracts (`traits` crate)

Signatures are normative (names/shapes may be refined in impl, but the boundaries are fixed).
All fallible async; `Result<T>` = `anyhow::Result<T>` (or a crate error enum).

```rust
pub struct CapturedFrame { pub monitor_index: u32, pub width: u32, pub height: u32,
                           pub captured_at: i64 /*unix ms*/, pub pixels: Arc<RgbaImage>,
                           pub content_hash: String }

#[async_trait] pub trait CaptureSource: Send + Sync {
    fn monitors(&self) -> Vec<MonitorInfo>;
    /// Yields the next *changed* frame (diff-gated) or None on shutdown.
    async fn next_frame(&mut self) -> Result<Option<CapturedFrame>>;
}
// 0.3.0: `capture_now` (§7b) is NOT a method on this trait. It is a per-request flag handed to the
// **capture worker** that drives `next_frame()`, telling it to emit exactly one frame *past* the diff
// gate — a demanded frame (mark-this-moment) must never be dropped as "unchanged" (D8). The diff gate
// lives in the worker, not in `CaptureSource`.

pub struct OcrResult { pub text: String, pub mean_confidence: f32, pub engine: String,
                       pub spans: Vec<TextSpan> }   // 0.2.x: per-line/word geometry — see §3b
#[async_trait] pub trait OcrProvider: Send + Sync {
    async fn recognize(&self, frame: &CapturedFrame) -> Result<OcrResult>;
}

pub struct Embedding(pub Vec<f32>); // len == dim()
#[async_trait] pub trait EmbeddingProvider: Send + Sync {
    fn dim(&self) -> usize;                 // 768
    /// NOTE: quantized text model cannot batch — impl embeds one input at a time.
    async fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Embedding>>;
    // 0.3.0 (PR4) removed `embed_image` — the image-embedding lane is gone (§4/§5).
}

pub struct VisionAnalysis { pub description: String, pub activity_type: Option<String>,
                            pub app_hint: Option<String>, pub confidence: f32, pub model: String }
#[async_trait] pub trait VisionProvider: Send + Sync {
    async fn analyze(&self, image: &RgbaImage) -> Result<VisionAnalysis>;
}

pub struct RetrievedChunk { pub frame_id: i64, pub text: String, pub score: f32, pub captured_at: i64 }
pub struct AnswerOpts { pub thinking: bool, pub max_tokens: u32 }
#[async_trait] pub trait AnswerProvider: Send + Sync {
    /// Streams answer deltas over the channel; returns when complete.
    async fn answer(&self, query: &str, context: &[RetrievedChunk], opts: AnswerOpts,
                    tx: tokio::sync::mpsc::Sender<AnswerDelta>) -> Result<()>;
}

#[async_trait] pub trait Store: Send + Sync {
    // frames + ocr
    async fn insert_frame(&self, f: NewFrame) -> Result<i64>;
    async fn insert_ocr(&self, frame_id: i64, ocr: OcrResult) -> Result<()>;
    async fn insert_vision(&self, frame_id: i64, v: VisionAnalysis) -> Result<()>;
    // embeddings
    async fn upsert_text_embedding(&self, frame_id: i64, chunk_index: i32, chunk_text: &str,
                                   source: ChunkSource, emb: &Embedding, model: &str) -> Result<()>;
    // 0.3.0 (PR4) removed `upsert_image_embedding` — the image-embedding lane is gone (§4/§5).
    // marks (§4, §7b): insert_mark / list_marks / resolve_mark — see §7 command table.
    // retrieval
    async fn hybrid_search(&self, q: &SearchQuery) -> Result<Vec<SearchHit>>;
    // job queue (see §5)
    async fn enqueue_job(&self, job: NewJob) -> Result<i64>;
    async fn claim_jobs(&self, kinds: &[JobKind], limit: u32, now: i64) -> Result<Vec<Job>>;
    async fn complete_job(&self, id: i64) -> Result<()>;
    async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> Result<()>;
    async fn job_stats(&self) -> Result<JobStats>;
    // settings
    async fn get_setting(&self, key: &str) -> Result<Option<String>>;
    async fn set_setting(&self, key: &str, value: &str) -> Result<()>;
}
```

## 3b. Text signal (0.2.x): raw vs content text, windows, spans, roles

*Added by the 0.2.x arc (`02 §5b`, `docs/0.2.0.md`). v1.0 stored one unfiltered OCR string per
frame; 0.2.x splits that into a preserved raw layer and a filtered default-retrieval layer.*

**Raw vs content text.**
- **`raw_text`** — the full, unfiltered OCR/UIA text of the frame. **Always preserved.**
- **`content_text`** — **filtered OCR/UIA text** (explicitly: *not* vision descriptions — those stay
  in `vision_analysis`). This is the **default** input for search, Ask, embeddings, and reports. It
  keeps `content` (+ useful `unknown`) inside the target window and excludes
  `system`/`background`/`chrome`.
- **Default search stays hybrid (FTS + vector) over `content_text`; the FTS fallback is never
  removed.** Raw / app-chrome text is searchable only when the caller opts in via
  `SearchQuery.include_chrome = true`. So **raw text is preserved but is *not* the default retrieval
  input.**

**Active / target window semantics.** The foreground (target) window's rectangle and title define
the user's focus. Text inside that rect is eligible for `content`; visible text from other windows
is `background`. The foreground window **title** is carried as frame metadata (`target_window_title`
/ `target_app_hint`), **not** injected into `content_text` as repeated body text.

**Text roles** (per span; stored in `text_spans`, `§4`):
- `system` — taskbar, desktop icons, tray, clock, Start/search bar.
- `background` — visible text outside the target/foreground window.
- `chrome` — menus, tabs, sidebars, toolbars, status bars, repeated app labels.
- `content` — document/editor/browser/terminal/chat/report body text.
- `unknown` — kept only when not obviously static noise.

**Static chrome suppression.** A span's signature is `app_hint + normalized_text + region_bucket`
(catalogued in `chrome_text_catalog`, `§4`). After repeated appearances a signature is marked chrome
and dropped from `content_text`. Constraints:
- **Thresholds are Settings-configurable, not hardcoded** (`§8`) — the project already hit the
  hardcoded-constant anti-pattern with the vision batch size.
- Never suppress a long, information-rich line solely because it repeats; never suppress text inside
  `content`/editor roles solely because it repeats.
- `filter_version` (on `frame_text`) is bumpable to recompute the whole catalog.
- **False suppression is the top risk (silent data loss).** Expose a per-app **suppression-rate
  metric** so over-suppression is observable; `include_chrome` + preserved `raw_text` always recover
  anything wrongly suppressed.

**New types** (`traits` crate; exported via `ts-rs`, regenerated in PR2 — not in PR1):

```rust
pub enum TextSource { Ocr, Uia }                 // primary_source / span source
pub enum TextRole { Content, Chrome, Background, System, Unknown }
pub enum SuppressReason { StaticChrome, SystemUi, BackgroundWindow }

// One OCR/UIA span with normalized [0,1] geometry; carried on OcrResult.spans (§3).
// WinRT exposes no per-word confidence, so OcrResult keeps the CONFIDENCE_UNKNOWN sentinel.
// suppress_reason is Option — None maps to the nullable `text_spans.suppress_reason` column
// (a searchable, non-suppressed span); no redundant in-enum None variant (§4 DDL).
pub struct TextSpan { pub text: String, pub normalized_text: String,
                      pub source: TextSource, pub role: TextRole,
                      pub x: f32, pub y: f32, pub w: f32, pub h: f32,   // normalized [0,1]
                      pub is_searchable: bool, pub suppress_reason: Option<SuppressReason> }

// FrameDetail (returned by `get_frame`, §7) gains: raw_text, content_text,
//   text_source: TextSource, suppressed_text_count: u32.
// SearchQuery gains: include_chrome: bool (default false).
```

This contract is implemented across **PR2** (schema + span geometry + types) and **PR3** (the
classifier that fills roles and `content_text`). **Interim:** PR2 lands before PR3, so PR2 fills
`content_text` as a **passthrough copy of `raw_text`**; PR3's filter applies from its deploy onward
with **no backfill** (clean-DB assumption — see `07`).

## 4. Data model (SQLite, WAL) — authoritative DDL

Single file `screensearch.db`. Migrations are forward-only, tracked in `schema_version`.

```sql
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
  -- 0.2.1 event-capture label (`07` #47). 0.3.0 (PR2) keeps this **widened CHECK** unchanged so legacy
  -- tokens stay readable (D2 — no schema change); new frames only ever emit 'timer'/'idle'/
  -- 'foreground_change' after the trigger trim, but 'clipboard_change'/'typing_pause'/'click'/
  -- 'scroll_stop' remain valid for old rows (the Moment "Captured via" row still renders them).
  capture_trigger TEXT
    CHECK (capture_trigger IS NULL
           OR capture_trigger IN ('timer','idle','foreground_change','clipboard_change',
                                  'typing_pause','click','scroll_stop','manual'))
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
-- triggers keep FTS in sync (insert/delete/update) — standard external-content pattern

-- 0.2.x text signal (schema_version 2 → 3; clean DB, forward-only migration authored in PR2).
-- frame_text: preserved raw text + filtered default-retrieval text, one row per frame.
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
-- triggers keep frame_text_fts in sync — same external-content pattern as ocr_text_fts.
-- include_chrome=true also searches raw_text (a raw FTS or a role-filtered text_spans FTS, chosen
-- in PR2). With a clean DB, frame_text.raw_text is the single raw store — the legacy ocr_text table
-- is not required going forward.

-- text_spans: per-frame OCR/UIA spans with normalized geometry + classified role.
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

-- chrome_text_catalog: signature counter that drives static-chrome suppression (PR3).
CREATE TABLE chrome_text_catalog (
  signature       TEXT PRIMARY KEY,              -- app_hint + normalized_text + region_bucket
  app_hint        TEXT,
  region_bucket   TEXT,
  normalized_text TEXT    NOT NULL,
  seen_count      INTEGER NOT NULL,
  first_seen_at   INTEGER NOT NULL,
  last_seen_at    INTEGER NOT NULL,
  suppressed      INTEGER NOT NULL DEFAULT 0 CHECK (suppressed IN (0,1))  -- 0/1; marked chrome after a configurable threshold (§8)
);
-- Interim (PR2 lands before PR3): insert_ocr fills content_text = raw_text (NOT NULL passthrough);
-- frames captured in the PR2→PR3 window are not backfilled (clean-DB assumption — see 07).

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
  content_hash TEXT NOT NULL                -- skip re-embed if unchanged
);
CREATE INDEX idx_embeddings_frame ON embeddings(frame_id);
CREATE VIRTUAL TABLE embedding_vectors USING vec0(
  embedding_id INTEGER PRIMARY KEY,         -- == embeddings.id
  embedding    FLOAT[768] distance_metric=cosine
);

-- image embeddings — REMOVED in 0.3.0 (PR4). The `image_embeddings` + `image_embedding_vectors`
-- tables and their `AFTER DELETE` trigger are DROPped by the PR4 migration (see "0.3.0 migrations"
-- below); text embeddings + vision tags cover semantic reach. A fresh 0.3.0+ DB never creates them
-- (`02 §5c`, `MODEL_REGISTRY §3`).

-- 0.3.0 marks (mark-this-moment; §7b): user-flagged frames + optional intention note. One row per
-- mark; CASCADEs with the frame like every per-frame table (authored by PR6 — see "0.3.0 migrations").
CREATE TABLE marks (
  id          INTEGER PRIMARY KEY,
  frame_id    INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
  created_at  INTEGER NOT NULL,
  note        TEXT,                          -- optional one-line intention (nullable)
  resolved_at INTEGER                        -- NULL = unresolved; set on resolve/dismiss
);
CREATE INDEX idx_marks_open ON marks(resolved_at, created_at DESC);  -- list_marks order: unresolved first (resolved_at NULLs sort first), newest-first within each group (§7/§7b)

-- 0.4.0 sessions (added by the PR3 migration, schema 10 → 11; contract §7e). Derived-but-persisted:
-- a wiped `sessions` table is fully recomputable from frames (D1). Segmentation is pure heuristic —
-- zero model calls (D3); titles/summaries are lazily generated + cached on the row.
CREATE TABLE sessions (
  id            INTEGER PRIMARY KEY,
  started_at    INTEGER NOT NULL,                 -- unix ms (first frame in the run)
  ended_at      INTEGER,                          -- NULL = open (still accreting)
  kind          TEXT NOT NULL CHECK (kind IN ('focus','meeting','ai','other')),
  tool          TEXT CHECK (tool IS NULL OR kind = 'ai'),  -- taxonomy id ('claude-code','codex',…); NULL unless kind='ai' (D7)
  host          TEXT CHECK (host IN ('terminal','desktop','browser','ide')),
  context_key   TEXT NOT NULL,                    -- closed task-level grammar (06 #27/#28): 'ai:<tool-id>' |
                                                  --   'meeting:<taxonomy-id>' | 'focus:<dominant-app-stem>'
                                                  --   (bare 'focus' when no stem qualifies); the §7b key generalized (§7e)
  title         TEXT,                             -- lazily generated + cached (D3); NULL until first asked
  summary       TEXT,                             -- lazily generated + cached (D3); NULL until first asked
  summary_model TEXT,                             -- model id that produced `summary` (provenance)
  confidence    REAL NOT NULL,                    -- segmenter confidence; surfaced honestly in the UI (D11)
  frozen        INTEGER NOT NULL DEFAULT 0 CHECK (frozen IN (0,1)),  -- D2 freeze rule: 1 = id + boundaries immutable
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_sessions_time ON sessions(started_at, ended_at);

-- frames link to their session (nullable; ON DELETE SET NULL — frames SURVIVE session deletion, D1).
-- Assigned by PR4's segmenter (incremental + a one-shot historical pass), never by the migration.
ALTER TABLE frames ADD COLUMN session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL;
CREATE INDEX idx_frames_session ON frames(session_id);

-- the artifact extension point (D8). 'exchange' (user-vs-agent turns) is the only kind USED this arc;
-- 'transcript' is RESERVED for the audio arc (D14 — made concrete now, not built); 'note' reserved for
-- future annotation. Roles live here, not on spans — the §3b TextRole (chrome/content) axis is untouched.
CREATE TABLE session_artifacts (
  id         INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('exchange','transcript','note')),
  role       TEXT CHECK ((kind = 'exchange' AND role IS NOT NULL AND role IN ('user','agent')) OR (kind IN ('transcript','note') AND role IS NULL)),  -- exchanges REQUIRE a non-null role, reserved kinds forbid one (D8; §7e "roles never invented"). The IS NOT NULL guard is load-bearing: NULL IN (…) is NULL, and SQLite passes a CHECK that evaluates to NULL.
  frame_id   INTEGER REFERENCES frames(id) ON DELETE SET NULL,
  content    TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE INDEX idx_artifacts_session ON session_artifacts(session_id, created_at);
-- index the `frame_id` FK: SQLite does not auto-index FKs, and `frame_id` is ON DELETE SET NULL,
-- so a frame retention delete would otherwise full-scan session_artifacts to null matching rows.
CREATE INDEX idx_artifacts_frame ON session_artifacts(frame_id);

-- durable job queue (the heart of enrich-deferred) — see §5
CREATE TABLE jobs (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL,               -- 'embed_text' | 'vision_tag' (0.3.0 PR4 removed 'embed_image'). No CHECK — matches schema.rs; a value CHECK is optional hardening tracked in 07 #82.
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

-- tagging, settings, schema version
CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL);
CREATE TABLE frame_tags (frame_id INTEGER REFERENCES frames(id) ON DELETE CASCADE,
                         tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
                         PRIMARY KEY(frame_id, tag_id));
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE schema_version (version INTEGER NOT NULL);
```

**Vector sync:** on `upsert_text_embedding`, insert into `embeddings` then
`embedding_vectors(embedding_id, embedding)` with the new rowid; on frame delete, the cascade
removes `embeddings`, and an `AFTER DELETE` trigger (or app-side txn) removes the matching
`embedding_vectors` rows. *(0.3.0 removed the image-embedding lane, so there is no second vec0 sync.)*

**0.3.0 migrations** (forward-only; each bumps `schema_version` by **exactly one** — D15 — and must
confirm the next integer against `crates/store/src/schema.rs::LATEST_SCHEMA_VERSION` rather than a
hardcoded number, and ship a populated-DB migration test, the 0.2.1 `frames`-rebuild test as the
pattern):
- **PR4 — image-lane drop:** `DROP TABLE image_embedding_vectors; DROP TABLE image_embeddings;` (the
  `AFTER DELETE` trigger goes with its table) and `DELETE FROM jobs WHERE kind = 'embed_image';` (in
  any state). This destroys **derived, re-derivable** vectors only — frames, stored images, text, and
  text embeddings are untouched. Acceptance: fresh-DB and migrated-DB schemas agree, and hybrid search
  is unchanged on the 10k-frame fixture (the image arm was flag-off, so parity is expected and shown).
- **PR6 — marks:** creates the `marks` table above. **Retention (D10):** a marked frame follows
  **normal** retention — its image still expires like any other frame (the mark keeps the text
  reconstruction reachable); an unresolved mark never pins a frame or blocks retention. No retention
  pinning in 0.3.0.

**0.4.0 migration** (the arc's **only** schema change — D4; same forward-only mechanics: bumps
`schema_version` by **exactly one** — 10 → 11 — with the next integer confirmed against
`crates/store/src/schema.rs::LATEST_SCHEMA_VERSION` rather than a hardcoded number, and a
populated-DB migration test, the 0.2.1 `frames`-rebuild test as the pattern; if any **other** 0.4.0
PR appears to need schema, that PR is wrong — stop and report):
- **PR3 — sessions:** creates the `sessions`, `session_artifacts` tables + indexes above and adds
  the `frames.session_id` column + `idx_frames_session` (all shown in the DDL block). **Structure
  only, no backfill** — the migration is fast and creates no session rows; pre-existing frames get
  `session_id` assigned by PR4's segmenter running over history as a normal resumable background job
  (`§7e`), not by the migration. **Additive (D10):** every frame-level feature (search, Ask, reports,
  marks, overlay, where-was-i) is demonstrably unchanged on a populated schema-10 → 11 fixture;
  a wiped `sessions` table is fully recomputable from frames (D1). Acceptance: fresh-DB and
  migrated-DB schemas agree. **Backup gate (D5, release-blocker-class):** before this branch's build
  is ever pointed at the live DB, a dated copy of `screensearch.db` exists outside the app data dir —
  recorded in `07`'s manual-steps section (the live DB is simultaneously the only production install
  and the arc's PR2 ground-truth dataset). *PR3 may refine the DDL's column details if PR2's evidence
  demands (e.g. more structure in `context_key`) — any such refinement is recorded in `06`/`08` and
  re-normalized into this section by PR3.*

## 5. Job queue & worker model (the core change)

**Producers**
- After `insert_ocr` succeeds → enqueue `embed_text` (priority normal).
  *(0.3.0 PR4 removed the `embed_image` producer with the image-embedding lane — §4.)*
- `vision_tag` is **never auto-enqueued per frame.** It is enqueued only by:
  1. **On-demand** — a UI command for a frame or a time range.
  2. **Timer** — a scheduler enqueues up to *N* untagged frames every *interval*.
  3. **Idle** — when the OS reports user-idle ≥ threshold.

**Workers**
- A bounded worker pool (`kernel`) loops: `claim_jobs(kinds, batch, now)` →
  `UPDATE … SET state='running'` (atomic claim) → run provider → `complete_job` or
  `fail_job(err, retry_at)`.
- **Claim atomicity:** single `UPDATE … WHERE id IN (SELECT … state='pending' AND not_before<=now
  ORDER BY priority DESC, id LIMIT n) RETURNING *` under WAL.
- **Retry/backoff:** on failure `attempts++`; if `attempts < max_attempts` set
  `not_before = now + backoff(attempts)`, else `state='dead'` (dead-letter; surfaced in
  diagnostics, never silently dropped).
- **Resource control:** worker concurrency, enabled job kinds, and the vision trigger mode are all
  settings (§8). Embedding workers may run on a background trigger; vision workers honor the
  on-demand/timer/idle mode strictly.
- **Smart enrichment throttle (0.2.1, opt-in, default OFF).** When `throttle.enabled` is on, the
  pool reacts to *sustained* CPU/GPU pressure with graded backpressure: at level ≥ 1 (High) the
  claim-kind gate drops `vision_tag` from the claimable set (heavy enrichment
  pauses; 0.3.0 PR4 removed `embed_image`); at level 2 (Sustained) an in-flight gate additionally floors concurrent `embed_text` to
  `throttle.embed_text_floor` (≥1). **Capture / OCR / storage never throttle** — they are
  structurally outside the worker pool. Pressure is read through a `PressureProbe` seam injected by
  the composition root (the kernel forbids `unsafe`, mirroring the `IdleSource` / `BackfillControl`
  seam); the Windows-native impl lives in the `sysmon` crate (CPU via `GetSystemTimes`, GPU via PDH
  `\GPU Engine(*)\Utilization Percentage` English counters, summed across engines — vendor-neutral).
  When GPU counters are absent the probe latches a truthful **"GPU not monitored"** state and the
  throttle runs CPU-only. A pure `ThrottleMachine` (clock-injected, no Win32) applies hysteresis
  (`*_exit_pct` < `*_enter_pct`) and dwell timers, stepping one level per dwell; a kernel governor
  loop samples the probe each `throttle.sample_interval_ms`, publishes the level into a shared
  `AtomicU8` the pool reads (no pool restart on level change), and broadcasts `throttle_changed`
  (§7). All thresholds are settings, never hardcoded (§8).

## 6. Inference sidecar — protocol & lifecycle (hard requirements)

**Process:** one `llama-server` child, OpenAI-compatible HTTP on `127.0.0.1:<ephemeral>`.
**Model-agnostic, tiered:** the `ModelSupervisor` is given a `ModelSpec { lane, tier, gguf_path,
mmproj_path?, ngl }` and starts the server for it. Switching lane/tier that needs a different model
stops and restarts with the new GGUF (vision needs `--mmproj`; answer does not).

**Lifecycle (MUST):**
1. **Job Object binding.** On supervisor init, create a Windows **Job Object** with
   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Every spawned `llama-server` is assigned to it at spawn,
   before unsuspend. → If the app dies for *any* reason (crash, kill, power-loss-after-resume,
   clean exit), the OS terminates the child. **No orphaned inference, ever.**
2. **Startup reap.** On launch, detect and kill any stray `llama-server` from a prior run that
   this app owns (tracked via a pidfile + a unique command-line sentinel arg); never kill an
   unrelated process.
3. **Lazy spawn + idle evict.** Spawn on first request needing a model; stop after
   `sidecar.idle_ttl_secs` of no requests (frees GPU/RAM — the footprint control).
4. **Health + restart.** Poll `/health`; on hang/crash, restart and requeue the in-flight job.
5. **mmproj same-family invariant.** Never pair a vision model with a mismatched projector.

**Crate option:** `win32job`, or `windows`-rs `CreateJobObject`/`AssignProcessToJobObject`/
`SetInformationJobObject`.

## 7. UI ↔ Core contract (Tauri commands + events, `ts-rs`)

All request/response types are Rust structs exported to TS via `ts-rs` (no hand-written
duplicates). **Commands** (UI → core):

| Command | In → Out |
|---|---|
| `search` | `SearchQuery` → `SearchHit[]` |
| `ask` | `AskRequest` → `()` (answer streamed via `answer_delta` events) |
| `get_timeline` | `TimeRange` → `TimelineBucket[]` |
| `get_frame` | `frame_id` → `FrameDetail` |
| `generate_report` | `ReportRequest` → `ReportResponse` (0.2.x; daily/weekly/custom over `content_text`, cites frames — `§8b`) |
| `enqueue_vision` | `frame_id \| TimeRange` → `enqueued_count` |
| `get_job_stats` | `()` → `JobStats` |
| `get_throttle_status` | `()` → `ThrottleStatus` (0.2.1; smart enrichment throttle — `§5`) |
| `get_settings` / `set_settings` | `()` / `Settings` |
| `set_model_tier` | `{lane, tier}` → `()` |
| `capture_control` | `{start\|stop}` → `()` |
| `get_readiness` | `()` → `Readiness` (capture, db, embed model, sidecar) |
| `where_was_i` | `()` → `Option<ResumeContext>` (0.3.0; last sustained context — `§7b`) |
| `add_mark` | `{ frame_id? \| capture_now, note? }` → `MarkId` (0.3.0; `capture_now` bypasses the diff gate — `§7b`/D8) |
| `list_marks` | `()` → `Mark[]` (0.3.0; **all** marks, unresolved first then newest-first within each group — `§7b`; the Intentions strip renders the unresolved head) |
| `resolve_mark` | `mark_id` → `()` (0.3.0; resolve = done, dismiss = resolve-no-action — `§7b`) |
| `set_mark_note` | `{ mark_id, note }` → `()` (0.3.0; attaches the optional toast note **after the fact** — the mark row is inserted at hotkey-press time and the note arrives seconds later from the confirmation toast — `§7b`) |
| `export_data` | `ExportRequest` → `ExportResult` (0.3.0; Settings "Export…"; same code path as `GET /v1/export`, works with the API off — `§7c`/D12) |
| `set_api_config` | `{ enabled, port? }` → `ApiStatus` (0.3.0; enable/disable + port; bind failure is loud + guided-change — `§7c`) |
| `get_api_status` | `()` → `ApiStatus` (0.3.0; enabled, bound port, token-present — `§7c`) |
| `regenerate_api_token` | `()` → `ApiStatus` (0.3.0; new bearer token — `§7c`) |
| `list_sessions` | `SessionQuery` → `Session[]` (0.4.0 PR5; filters kind/tool/`TimeRange`/limit; also the Timeline session-bands data source — `§7e`, `UI_REFERENCE §3`) |
| `get_session` | `session_id` → `SessionDetail` (0.4.0 PR5; span + tool/host + exchanges; `include_summary` triggers lazy generation — `§7e`) |
| `session_recap` | `session_id` → report (0.4.0 PR5; the existing **§8b report engine** scoped to the session's time range + frames — no new summarization machinery, D3) |

*(0.4.0 command names + shapes are **naming PROPOSALS** — PR5 owns the final call, the 0.3.2 `app.*`
precedent. `session_recap` may instead land as a session scope on the existing `generate_report`
command; implementer's call, recorded in `08`. `FrameDetail` (`get_frame`) gains an optional session
reference — from `frames.session_id` — so Moment can render its "part of session" line; also a PR5
proposal.)*

**Events** (core → UI): `capture_tick`, `job_progress`, `answer_delta`, `sidecar_status`,
`readiness_changed`, `toast`, `throttle_changed` (0.2.1; payload `ThrottleStatus`, broadcast each
governor tick while `throttle.enabled` — `§5`), and `sessions_changed` (0.4.0 PR5; typed payload
`null`, from Rust unit `()`). After a sessions scheduler iteration has at least one successful awaited
store pass, the kernel emits `sessions_changed`; mounted session list/detail queries refetch from the
store. It is a pull-based cache-invalidation signal only: **no notification, toast,
nudge, badge, or score** is produced (D10/D11).

**`ThrottleStatus` shape** (0.2.1, smart enrichment throttle): `{ enabled: bool, level: u8 (0=Normal
/ 1=High / 2=Sustained), sample: Option<PressureSample>, gpu_monitored: bool }`, where
`PressureSample = { cpu_pct: f32, gpu_pct: Option<f32>, gpu_monitored: bool, sampled_at: i64 }`.
`gpu_monitored=false` (and `gpu_pct=None`) is the truthful "GPU not monitored" state — the throttle
then runs CPU-only.

**`Readiness` shape** (defined 2026-06-21, was silent — see `07` gap #3): each of
`capture | db | embed_model | sidecar` is a `ComponentReadiness { status, detail? }`, where
`status ∈ { unknown, disabled, initializing, ready, unavailable, error }` and `detail` is an
optional human-readable explanation.

## 7b. Flow recall mechanics (0.3.0): where-was-i, marks, capture_now

Core-side contracts behind the Flow overlay (PR5) and the where-was-i / mark-this-moment workflow
(PR6). Exposed to the UI as the `where_was_i` / `add_mark` / `list_marks` / `resolve_mark` commands
(§7); the overlay and the local API (§7c) reuse them — **no new retrieval code is invented**.

**Where-was-i (context resume) — D9.** A pure, unit-tested heuristic (a store query + a small pure
function; no new crate). The **anchor** is the **current context** — but because where-was-i is almost
always invoked *from the overlay* (which holds the OS foreground while it is up, with its input focused
on show), "current" must mean the **last non-ScreenSearch foreground context**: the app/domain active
immediately *before* ScreenSearch or its overlay took focus, derived core-side from recent frames —
ScreenSearch and its overlay never count as the anchor. The **last sustained context** = the most
recent run of frames, ending *before* that anchor context began, in which the same **context key**
persisted for at least `resume.min_dwell_secs` (default **120**, a setting like every threshold, §8).
Context key = `app_hint`, refined by browser domain (from `browser_url`) when present. **Transient
excursions are absorbed:** a run of one context key is *not* split by a brief switch away (a 2-second
alt-tab, a notification, a frame whose `app_hint` fails to resolve) — an interruption breaks the run
only if the interrupting context key is **itself sustained** (persists ≥ `resume.min_dwell_secs`);
shorter excursions fold into the surrounding run. This reuses the one dwell threshold rather than
adding a second knob (subtraction thesis, `02 §5c`). Excluded from candidacy: the **anchor context**,
**ScreenSearch itself**, and any app on `privacy.excluded_apps`. Returns the run's **representative
(last) frame** + app, window title, URL, and span start/end (a `ResumeContext`). Surfaced in the
overlay's empty state (PR5) and as a Deck "Jump back" card (PR6); `Enter`/click opens the Moment, from
which the existing frame context gets the user back. **0.4.0:** `§7e` generalizes this same context
key into persisted *sessions*; where-was-i itself is unchanged (additive — D10-of-0.4.0).

**Mark-this-moment (intention capture) — D8/D10.** The mark hotkey (`marks.hotkey`, default
`Ctrl+Alt+M`; §8) issues **`capture_now`**: a request into the *existing* capture worker that
**bypasses the diff gate for that one frame** (D8 — a demanded frame must never be dropped as
"unchanged"; it is serialized like any capture cycle, a per-request flag, **not** a mode), inserts the
frame, then inserts a **mark** (§4). **Multi-monitor is deterministic:** a capture cycle yields one
frame per monitor, so `capture_now` marks the frame on the **monitor holding the foreground window** —
the one whose `target_rect` resolves (`crates/capture`), i.e. the screen the user is actually on —
falling back to the primary monitor if none resolves. One `capture_now` → one mark, never the
ambiguous "first queued monitor". The mark row is inserted **at press time** (durable immediately); the
optional note arrives seconds later from the confirmation toast and is attached by **`set_mark_note`**
(§7) — so a crash during the toast never loses the mark. A brief, quiet overlay toast confirms
("Marked ✓ — note?") **without stealing focus** (the shell shows the overlay non-focusable; the user
keeps typing — clicking the note field focuses it); ignoring it costs nothing. A `capture_now` whose
privacy gate denies (locked screen / a ScreenSearch window focused / an excluded app) or whose worker is
off inserts **nothing** and surfaces the reason honestly in the toast ("Capture is off — mark not
saved"), never a silent no-op. The Deck **Intentions** strip lists unresolved marks
newest-first (thumbnail/reconstruction, note, age); resolve = done, dismiss = resolve-with-no-action.
**No badge counts anywhere** (D14 — pull-based, never nagging). Marked frames follow normal retention
(§4 — images expire; the mark keeps the reconstruction reachable; no retention pinning).

**Overlay window (PR5) is capture-safe — D7.** The overlay is a second Tauri window,
**hidden-not-destroyed** (show/hide, so summon latency is window-show latency, not a webview boot),
always-on-top, frameless, transparent, skip-taskbar. Because it is the app's own window, the existing
**self-exclude capture gate must cover it** — the overlay must never appear in its own capture history;
the privacy-gate tests assert the second window is never captured (§8 privacy prose). Hotkey
registration failure (conflict with another app) is a **visible Settings warning + toast**, never a
silent no-op (D6). Exclusive-fullscreen apps may suppress the overlay (accepted; documented) — the
overlay never steals focus without the hotkey.

## 7c. Local HTTP API + export (0.3.0, opt-in — a separate external surface, NOT a Tauri IPC surface)

A new crate `crates/api` (axum; behind a trait, wired in the composition root, **not constructed at
all** unless enabled) exposes the same core queries over HTTP for local scripts/agents. It reuses
`hybrid_search`, the ask pipeline, the where-was-i heuristic (§7b), and marks — no new retrieval code.

**Posture (D11).** Default **OFF** (`api.enabled` = false). When enabled it binds **`127.0.0.1` only,
hard-coded** (not a setting — a `0.0.0.0` bind must be impossible by construction: shown in code review,
asserted in a test). Every request requires a **bearer token**, generated on first enable, stored in
settings, shown/copyable/regenerable in Settings; a request without or with a wrong token gets **401**.
Default port **43210** (`api.port`, configurable). **Port-bind failure = loud + guided change:** if the
port is already in use on enable, the API does **not** start, a visible Settings warning + toast fire,
and the Settings API panel offers an inline "port in use — pick another" retry (mirrors the D6 hotkey
pattern; never a silent no-op — resolved gap, `07`). Threat model stated plainly in the spec, the
Settings UI, and docs: *any local process holding the token can read your entire screen history —
enabling this is an explicit trust decision.*

**Endpoints (v1):**
- `GET /v1/health` — version, uptime, capture state.
- `GET /v1/search?q=&from=&to=&limit=&include_chrome=` — hybrid search, content-text default, same
  semantics as the UI.
- `POST /v1/ask` — grounded answer, **SSE stream**, cited frame ids. Reuses the ask pipeline; the API
  layer adapts the pipeline's `answer_delta` stream (§7) to SSE (the sidecar client already speaks SSE).
  **Client disconnect cancels inference:** a half-read `/v1/ask` must never leave the sidecar
  generating into a closed socket. This is **not free today** — `AnswerProvider::answer`
  (`crates/inference/src/answer.rs`) is driven by the *sidecar* stream (`prx.recv()`) and discards
  downstream `tx.send` errors, so merely dropping the SSE receiver keeps the sidecar generating to
  `Done`. PR7 must add the cancellation path: detect the closed downstream (send failure /
  `tx.is_closed()`) — or cancel a task/token the API layer owns — then **stop consuming and abort the
  sidecar `stream_task`** so GPU/CPU is actually freed.
- `GET /v1/frames/{id}` — metadata + text; `?image=1` returns the stored image (WebP; the §4
  `image_path` comment predates the native-WebP switch, `07` #73).
- `GET /v1/context/where-was-i` — the §7b heuristic.
- `GET /v1/marks` (same order as `list_marks`: all marks, unresolved first then newest-first — §7) ·
  `POST /v1/marks` (body: `frame_id` **or** `"now"` → `capture_now`) · `POST /v1/marks/{id}/resolve` —
  the **only write surface** in v1 (D11; write scopes beyond marks are deferred, `07`).
- `GET /v1/export?from=&to=&format=json` — frames + content text (+ marks), **no images** in v1 (D12).
  **Serialized as a stream** (frames written incrementally to the response body — memory stays flat)
  and bounded by the optional `from`/`to` window, so exporting months of history never buffers the
  whole result set or risks OOM on the local box. A Settings **"Export…"** button calls the *same* code
  path internally (streaming to a file), so export works even with the API disabled.
- **0.4.0 (PR6, D12 — read-only, no new write scopes):**
  - `GET /v1/sessions?kind=&tool=&from=&to=&limit=` — sessions that **overlap** the requested
    window, not only sessions that start inside it: predicate is `started_at < to AND
    COALESCE(ended_at, now) > from` when both bounds are present, with the corresponding half-open
    single-bound forms (`COALESCE(ended_at, now) > from` for `from` only; `started_at < to` for `to`
    only). Open sessions (`ended_at` null) use request-time `now` for this overlap test, so a long or
    still-active session that began before `from` remains visible in Timeline/MCP range queries. The
    list surface behind `list_sessions` (§7) and the MCP `list_sessions` tool. `limit` defaults to
    **1000** and is clamped to **1..=1000** (mirrors the in-app `list_sessions` IPC surface;
    constants `SESSIONS_LIMIT_DEFAULT`/`SESSIONS_LIMIT_MAX` in `crates/api/src/sessions.rs`). List
    rows serve `summary`/`summary_model` as **null** (title is always included); the summary is
    revealed only by the detail endpoint's `include_summary=1`.
  - `GET /v1/sessions/{id}?include_summary=` — session detail + its `exchange` artifacts.
    `include_summary=1` returns the **cached** summary if one exists, else `null` — it **never
    triggers generation** over this surface. D12 makes the API/MCP strictly read-only: a GET must not
    start inference or write `sessions.summary`/`summary_model`. Lazy generation is an **in-app IPC
    action** (`session_recap` / the app requesting a summary, §7/§7e); the API only ever reflects
    already-cached state. Without `include_summary=1` (and on every list row) `summary`/`summary_model`
    are served as **null**; `title` is always included. For an **open** session the cached summary (if
    any) is served with an `open: true`/non-final marker, never regenerated on read. (If a future need for API-triggered
    generation appears, it is an explicit `POST` command — a new write scope, out of this arc, `07`.)
  - `POST /v1/ask` gains an optional **`session_id`** scope — when set, retrieval is restricted to that
    session's frames and the answer cites **only** in-session frames (the arc's strategic payoff:
    *"what did I do in my last Claude Code session?"*). Absent `session_id`, behavior is unchanged (D10).

Docs: a hand-written `docs/API.md` (OpenAPI-lite; v1 is small, no codegen), authored by PR7; PR6
extends it with the sessions endpoints.

**MCP server (PR8, D13).** A separate workspace **binary** crate `crates/mcp` → `screensearch-mcp.exe`,
shipped in the NSIS installer — a thin **stdio** wrapper over this HTTP API, with **no store access and
no app coupling**: it is purely an HTTP client of `127.0.0.1:<port>` with the bearer token from
args/env (`SCREENSEARCH_API_URL` / `SCREENSEARCH_API_TOKEN`). Tools: `search_screen_history`,
`ask_screen_history`, `get_moment`, `where_was_i`, `list_marks`, `add_mark`. If the API is off, every
tool returns a clear "enable the API in ScreenSearch Settings" error. Docs: `docs/MCP.md` (copy-paste
client config for Claude Desktop / Claude Code + the same threat-model paragraph), authored by PR8.
**0.4.0 (PR6, D12):** the wrapper gains `list_sessions`, `get_session`, and `ask_session` (over the
`§7c` sessions endpoints) — still read-only, still a stdio HTTP client with no store access (0.3.0
D13 holds); `docs/MCP.md`'s tool table + threat-model boundary grow with them.

## 7d. App lifecycle (0.3.2): tray, close-to-tray, single instance

The tray is the app's **passive lifecycle surface** (issues #56/#57; `docs/0.3.2.md` D3/D4).
Pull-based by construction: the icon *displays* state; nothing notifies, nudges, counts, or pushes
(`02 §7`, `07` #97). Anything push-shaped is out of contract for this arc.

**Tray icon (D3).** Present while the app runs. Glyph/tint encodes live capture state —
**capturing / paused / error** — fed by the same state that drives the StatusRail (no separate
poller). This passive display is the *entire* "app running reminder" feature of #56.

**Tray menu (D3).** Exactly: **Open ScreenSearch** · **Pause/Resume capture** · **Load/Unload
answer model** · **Start/Stop vision tagging** · **Check for updates** (`§11b`) · **Quit**. The same
load/unload + start/stop-vision quick actions also join the in-app quick menu (the NavRail footer
surface, `07` #99), completing #57. Menu items act through the existing commands/IPC — no new
side-channel into the kernel.

**Close-to-tray (D3).** `app.close_to_tray` (default **true**, `§8`): closing the main window hides
it to the tray; capture continues. A **one-time** toast (existing `Toast` primitive + `toast`
z-layer, `UI_REFERENCE §2`/`§3`) explains this **the first time the window is reopened from the
tray** (Open / tray-icon click / a second launch) — a toast fired at hide-time, when the window is
vanishing, would never be seen. Informational, never repeated (persisted via the `app.tray_toast_done`
marker, `§8`), never a nag. With the setting off, window close quits (clean shutdown, below).

**Run at startup (D3).** `app.run_at_startup` (default **false**, `§8`) registers/unregisters
launch-at-login — an explicit user choice, never a silent default.

**Single instance (codifying shipped behavior).** `tauri-plugin-single-instance`
(`src-tauri/src/lib.rs`): a second launch spawns no second app; the callback calls `window.show()` →
`unminimize()` → `set_focus()`, so a tray-hidden window is **restored** — this is the contract
interaction between close-to-tray and single instance (second launch behaves as "Open").

**Quit is a clean shutdown** (from the tray menu, or window close with close-to-tray off): capture
stopped, and the sidecar terminated via the **Job-Object lifecycle (`§6`)** — quit must never orphan
`llama-server`.

**No new chords.** 0.3.2 registers no new global hotkeys; the tray is pointer/menu-keyboard only.
The `overlay.hotkey`/`marks.hotkey` cross-chord conflict check (gap `07` #100) is a **Settings-side
inline warning** (PR5, `UI_REFERENCE §3`), not a registration-layer change.

## 7e. Sessions (0.4.0): segmentation, taxonomy, recognition, exchanges, lazy intelligence

The sessions arc (P8, `docs/0.4.0.md`) reframes recall from the **frame** to the **session** — a run
of frames sharing a context, given a stable identity, made visible in the UI (`UI_REFERENCE §3`/`§4`)
and queryable over the API/MCP (`§7c`). Everything here is **additive (D10)**: it adds derived
objects on top of the shipped app and changes **no** frame-level behavior (capture, search, Ask,
reports, marks, overlay, where-was-i). The schema lives in `§4` (`sessions`, `session_artifacts`,
`frames.session_id`); this section is the behavioral contract behind it. **Build order:** the
segmenter is validated against the maintainer's real DB in **PR2** *before* the schema freezes in
**PR3**; PR4 productionizes what PR2 validated; PR5 (UI) ∥ PR6 (API/MCP). PR4 merges **only** if it
meets the thresholds PR2 recorded on the real-DB harness (**D9** — a miss is a stop-and-report, not
a tune-until-green).

**Derivation principle (D1).** Sessions are **derived-but-persisted**; frames stay the source of
truth. Deleting or regenerating session rows never touches frames, text, embeddings, or marks; a
wiped `sessions` table is fully recomputable from frames. `frames.session_id` is `ON DELETE SET NULL`
(a frame outlives its session).

**Segmentation.** The production core lives in `crates/sessions`: a pure, unit-tested function over
ordered frame metadata, exposed to `kernel` behind the `SessionSegmenter` provider trait so the
module-dependency rule (`§2`) remains intact. The `kernel` scheduler runs an **incremental pass** over
the unfrozen tail plus a **one-shot, resumable, throttle-aware historical pass** over pre-0.4.0
frames. Its raw checkpoint is `sessions.backfill_done_until` (not part of typed `Settings`); one
approximately six-hour chunk runs per tick and extends to the next global `merge_gap` before it is
committed, so chunk boundaries cannot split a session. Any pass failure logs and degrades to no
sessions; capture/OCR never waits on it (D10). A scheduler iteration emits the typed-null
`sessions_changed` event only after a successful awaited store pass, so mounted session queries can
refetch committed rows; failures do not fabricate a change event, and the signal has no user-facing
notification/toast semantics (`§7`).
- **Two key levels ([a]).** The `§7b`-generalized key — `app_hint` ⊕ browser domain ⊕ taxonomy tool
  id — remains the internal **micro-run** key; where-was-i/`resume.rs` is untouched. The persisted
  task-level grammar is exactly `ai:<tool-id>` | `meeting:<taxonomy-id>` |
  `focus:<dominant-app-stem>` (bare `focus` when no stem qualifies). The browser-domain term remains
  dormant while production persists `browser_url: None` (`07` #109).
- **Concurrent tracks ([b]).** The shipped model has one `ai:<tool>` track per taxonomy id, one
  `meeting:<id>` track per meeting identity, and **one** residual `focus` track (not one per stem).
  Each frame has one owner; sessions on the same track never overlap, while different tracks may
  overlap. A track closes only on **its own** inactivity `>= merge_gap` or at end of input — a foreign
  recognized identity opens its own track and never closes the incumbent
  (`SustainedForeignIdentity` is absent). `sessions.gap_close_secs` remains the pass-1 micro close
  rule; `sessions.min_len_secs` remains the emitted-session floor.
- **Absorption + meetings ([c]).** An unrecognized run `<= absorb_max` is assigned to the
  last-touched open AI track; a leading run ramps into an opening AI track; a longer run becomes
  focus material. Meetings never absorb, but meeting bands are not walk barriers: AI tracks may span
  them, and overlapping meeting identities emit overlapping sessions. Frame ownership remains
  exclusive even where session time spans overlap.
- **Accretion, floors, and freeze ([d]–[f]).** Incremental reconciliation follows anchor continuity
  and reuses an overlapping row with the same persisted context key, preserving stable unfrozen ids.
  An AI track emits only when its **own recognized presence** reaches `IDENTITY_QUALIFY_MS`; AI and
  meetings are density-exempt. Focus requires `focus_min_len` plus the `focus_min_density` gate
  (`0` disables it). Anchorless focus confidence is always below anchored AI/meeting confidence.
  `kind`, `tool`, and `context_key` freeze at the same instant as boundaries.
- **Frozen shipped parameters.** `merge_gap = 2_700_000 ms`; `absorb_max = 1_800_000 ms`;
  `meeting_gap = 480_000 ms`; `focus_min_len = 600_000 ms` (a named constant after the 11-day
  sweep reclassified it FLAT); `focus_min_density = 30 fph`; `IDENTITY_QUALIFY_MS = 120_000 ms`;
  micro defaults `gap_close = 120_000 ms` and `min_len = 120_000 ms`; freeze lookback
  **W = 21_600_000 ms (6 h)**. Only `gap_close` and `min_len` are settings (`§8`); every other
  value is a named correctness constant, never a hidden knob. The retunes are grounded in the
  11-day sweep and 25-day stability sample (usage review 2026-08-01 §6.7–§6.8).
- **Freeze rule (D2).** A closed session, once older than the **freeze lookback window**, has its
  `id` and boundaries **frozen** (immutable; `sessions.frozen = 1`). Resegmentation only ever touches
  **unfrozen** (open/recent) sessions. This is what makes a session `id` safe to hand to MCP
  consumers — history keeps its identities; heuristic improvements apply going forward only. (A
  deliberate "resegment history" feature would be its own explicitly-versioned arc item.)
  - **The lookback window is the named constant W = 6 h (21 600 000 ms).** The 25-day
    stability sample measured zero boundary drift at 6 h, 12 h, 24 h, and 48 h, making 6 h the
    smallest evidenced stable window (usage review 2026-08-01 §6.8). It is deliberately **not**
    a user setting (ID stability is a correctness property, not a tuning knob).

**Taxonomy (D6/D7).** Recognition is driven by a **versioned data file in the repo** (a `version`
field; proposed TOML), compiled/shipped with the app (parsed at startup) — **not** schema and **not**
settings. Adding a tool = editing the file; a user-editable override file is **deferred** (`07` #107).
Each entry carries: tool id, `kind`, `host`, `app_hint` patterns, window-title patterns, and browser
domains. **Two recognition dimensions:** tool identity × host (`terminal` / `desktop` / `browser` /
`ide`). **Tuned v3 set (D7, nine entries):** tools `claude-code`, `codex`, `claude-desktop`,
`browser-ai` (Claude / ChatGPT / Gemini in a browser); meetings Zoom,
Teams, Meet, Webex, Discord (app hints + window-title patterns). **`browser_url` is dormant in
production capture today** — `capture_loop.rs` persists `browser_url: None` (needs UIA; the same
dormancy `§7b`/`resume.rs` already document and implement-per-contract). So **browser-AI recognition
keys on the metadata that IS stored — `app_hint` + window-title patterns** (browsers put the page
title, e.g. "Claude", "ChatGPT", "Gemini …", in the window title); the claude.ai / chatgpt.com /
gemini.google.com **domain match is a refinement that activates if/when `browser_url` capture lands**
(deferred — `07` #109), never the sole signal. **The seed patterns are tuned in PR2
against real captures, not guessed** — this section fixes the file *format* and seed *set*; PR4 ships
the tuned file. `vscode`/`cursor` were dropped by the PR2 evidence. Claude Code is terminal-stem AND
(`"claude"` in title OR first-glyph U+2733/U+2800–U+28FF); Codex is a desktop app and accepts the
post-rename `chatgpt` process stem except an explicit `"ChatGPT Classic"` title exclusion (`07`
#117). Recognition emits `kind` / `tool` / `host` onto the session (`tool` NULL unless
`kind='ai'`, per the `§4` CHECKs). **No model calls anywhere in segmentation or recognition (D3).**

**Exchange extraction (D8, best-effort).** For recognized **AI** sessions, tool-specific markers over
`content_text` (**never** raw full-screen text — the 0.2.x content-vs-raw rule, `§3b`, holds) extract
user-vs-agent turns into `session_artifacts` rows (`kind='exchange'`, `role ∈ {user, agent}`).
Explicitly best-effort: when markers don't match, the session simply has **no** exchanges — roles are
**never invented**. This is a *separate axis* from `§3b`'s `TextRole` (chrome/content): the span
schema (`§4 text_spans`) and `TextRole` enum are **untouched**. `'transcript'` and `'note'` artifact
kinds are reserved (D14 / future) — made concrete by the schema, not built this arc.

**Lazy intelligence (D3).** Session creation is pure heuristic — zero model calls. Intelligence is
generated **on demand**:
- **Title / summary** generate on first request **from the in-app IPC path** and **cache** on the row
  (`sessions.title` / `summary`, with `summary_model` recording provenance); subsequent reads are
  served from the cache. The read-only API/MCP surface (`§7c`, D12) **never** triggers generation — it
  returns the cached value or `null`; a GET must not start inference or write the row.
- **Recap** is **not** new machinery and is **not** row-cached: it **is** the existing coverage-first
  **report engine (`§8b`)** scoped to the session's time range and frames, generated per request like
  any report (with the same honest footer reports already carry). Context stays pinned at **8192** —
  the recap uses the shipped map-reduce, not a bigger window.

**Non-shaming (D11).** Session surfaces are truthful, on-demand, and neutral: **no** session
notifications, **no** "you spent N hours in X" nudges, **no** streaks or scores. `confidence` is
surfaced honestly (a low-confidence session says so); an open session is shown as still-running with
non-final boundaries. **Degradation (D10).** Sessions never gate capture; a segmenter failure degrades
to "no sessions", never to lost frames.

## 8. Configuration / settings (keys in `settings`)

`capture.interval_ms` (3000) · `capture.monitors` ([]=all) · `capture.diff_threshold` (0.006) ·
*(0.3.2 PR5 retires `storage.jpeg_quality` — provably inert, its own UI hint said "has no effect
today"; the persisted key is tolerated + ignored on load per the unknown-key rule below — D8, no
migration)* · `storage.max_width` (1280) · `storage.retention_days` (0=keep) ·
`enrich.embed_text` (true) · *(0.3.0 PR4 removed `enrich.image_embeddings`)* ·
`enrich.vision_timer_enabled` (false) · `enrich.vision_timer_interval_ms` (3600000) ·
`enrich.vision_idle_enabled` (false) · `enrich.vision_idle_secs` (300) ·
`enrich.vision_batch_size` (20, clamped 1–500 — max still-untagged frames a timer/idle tick enqueues) ·
`enrich.worker_concurrency` (2) ·
`models.vision_tier` (`default`) · `models.answer_tier` (`default`; each ∈ {`default`,`quality`} —
0.3.0 retired `beta`: a persisted `beta` selection **maps to `quality` on load**, logged once + the
mapping persisted, and any Beta GGUF already on disk is left alone with no cleanup logic — D3/D4) ·
`answer.thinking` (true) · `sidecar.idle_ttl_secs` (180) · `sidecar.ngl` (99) ·
`sidecar.ctx_size` (0=auto → per-lane default vision 4096 / answer 8192, else clamped 512–32768 —
the dominant VRAM lever) · `sidecar.kv_cache_type` (`q8_0`; one of `f16`/`q8_0`/`q4_0`, quantized
only when flash attention is active) · `sidecar.flash_attn` (`auto`; one of `auto`/`on`/`off`) ·
`sidecar.recycle_enabled` (true — recycle the sidecar process when committed host RAM crosses the
ceiling; mitigates the upstream llama.cpp multimodal memory leak, `07` #72) ·
`sidecar.recycle_rss_mb` (0=auto → ceiling derived from total installed RAM, else clamped
8192–131072 — the committed-RAM ceiling in MiB at which the sidecar is recycled; the 8 GiB floor
matches the auto-ceiling minimum so an explicit value clears the vision model's ~6.8 GB warmup
baseline and can't trigger a recycle loop; 0 is recommended for most users. Both keys are resolved
once at supervisor construction like `sidecar.idle_ttl_secs`, so changes apply on app restart) ·
`privacy.excluded_apps` (["1Password","KeePass","Bitwarden"]) · `privacy.pause_on_lock` (true).

**0.2.x text-signal keys** (defaults provisional — finalized/tuned in PR2/PR3; thresholds are
settings, never hardcoded, per `§3b` and the guardrail in `04 §4`):
`text.include_chrome_default` (false — default search uses `content_text`) ·
`text.chrome_suppress_min_seen` (12 — appearances before a signature is marked chrome) ·
`text.chrome_protect_min_chars` (48 — lines longer than this are never suppressed for repeating) ·
`text.chrome_region_buckets` (8 — grid resolution for `region_bucket`) ·
`retrieval.default_top_k` (8 — replaces the hardcoded `ASK_TOP_K`; per-request override allowed) ·
`reports.daily_top_k` (40) · `reports.weekly_top_k` (200) ·
`reports.map_reduce_min_frames` (20 — frame count above which a range uses map-reduce, `§8b`; set
at the worst-case single-pass fit so frames are batched, not dropped, before the 8192 answer context
overflows: ~400 tok/frame × 20 ≈ 8192).

**0.2.1 smart-enrichment-throttle keys** (opt-in CPU/GPU backpressure for the worker pool, `§5`;
thresholds are settings, never hardcoded — same guardrail as the text-signal keys above and `04 §4`):
`throttle.enabled` (false — master switch; throttle is opt-in and OFF by default) ·
`throttle.cpu_enter_pct` (85.0, clamp 1..=100 — busy% at which CPU pressure begins) ·
`throttle.cpu_exit_pct` (65.0, clamp 0..=enter−1 — hysteresis; must sit below `cpu_enter_pct`) ·
`throttle.gpu_enter_pct` (90.0, clamp 1..=100 — ignored when GPU is unmonitored, `§5`) ·
`throttle.gpu_exit_pct` (70.0, clamp 0..=enter−1 — hysteresis; must sit below `gpu_enter_pct`) ·
`throttle.enter_after_ms` (5000, clamp 500..=120000 — sustained-enter dwell before stepping up a level) ·
`throttle.exit_after_ms` (8000, clamp 500..=300000 — recovered-exit dwell before stepping down a level) ·
`throttle.sample_interval_ms` (1000, clamp 250..=10000 — governor probe/sample cadence) ·
`throttle.embed_text_floor` (1, clamp 1..=16 — min concurrent `embed_text` workers at level 2).

**Event-driven-capture keys** (opt-in `CaptureTrigger` source, `§5`/`07` #47; thresholds are settings,
never hardcoded — same guardrail as above). **0.3.0 (PR2) trimmed the six triggers to foreground +
idle**: `capture.event_driven_enabled` (false — master switch; off = timer cadence, every frame tagged
`Timer`) · `capture.event_on_foreground` (true) · `capture.event_on_idle` (false) ·
`capture.event_debounce_ms` (500, clamp 100..=10000) · `capture.event_min_interval_ms` (1000, clamp
250..=60000 — the rate ceiling) · `capture.event_idle_threshold_ms` (5000, clamp 1000..=60000) ·
`capture.event_fallback_interval_ms` (30000, clamp 1000..=3600000 — a static screen is still sampled at
least this often, tagged `Timer`). **Removed in 0.3.0 (PR2):** `capture.event_on_clipboard`,
`capture.event_on_typing_pause`, `capture.event_on_click`, `capture.event_on_scroll_stop`,
`capture.event_typing_pause_ms` — click / scroll-stop were the **only** consumers of the global
`WH_MOUSE_LL` mouse hook (deleted with them), the clipboard listener was a privacy-optics landmine, and
typing-pause was redundant with idle (D1). **Settings load tolerates + drops unknown keys** (log once,
no error), so a config persisted with any retired key still loads. **No schema change** (D2):
`frames.capture_trigger` keeps its widened CHECK, so legacy `clipboard`/`typing_pause`/`click`/
`scroll_stop` tokens stay readable (the Moment "Captured via" row still renders them); new frames simply
never emit them again.

**0.2.1 UIA text-source keys** (target-window accessibility text with OCR fallback, `07` #48/#71;
thresholds are settings, never hardcoded): `capture.uia_text_enabled` (true — default ON, OCR carries
any failure/timeout/thin-yield; hot-applies per frame) · `capture.uia_latency_budget_ms` (150, clamp
20..=2000 — soft per-walk budget; a 2× hard timeout guards a wedged worker) · `capture.uia_min_text_chars`
(16, clamp 0..=10000 — below this the read is a thin yield → OCR). UIA-hang-fix keys (`07` #71, all
baked into the provider at startup → applied on app restart):
*(0.3.2 PR5 retires `capture.uia_run_on_interactive` — inert-in-practice since the 0.3.0 PR2 trigger
trim removed its only firing triggers, `07` #83; the persisted key is tolerated + ignored on load —
D8, no migration)* · `capture.uia_view_control_only` (true — default
ON; control view collapses a Chromium page's per-text-run node explosion; off = raw view, far heavier) ·
`capture.uia_max_nodes` (4000, clamp 100..=20000 — hard cap on nodes visited per walk; replaces the
former hardcoded constant) · `capture.uia_max_textpattern_calls` (64, clamp 1..=4096 — max live
`TextPattern` visible-range reads per walk, the one uncacheable cross-process cost) ·
`capture.uia_suppress_during_input_ms` (500, clamp 0..=10000 — `Timer` frames captured within this
many ms of the last keyboard/mouse input skip UIA → OCR, closing the freeze gap the trigger gate
leaves in default timer-only capture; `0` disables — the suppress window now always applies, since
the former `uia_run_on_interactive` bypass retired with the knob).

**0.3.0 flow-recall + API keys** (all hotkeys/thresholds/ports are settings, never hardcoded — same
guardrail as above; `§7b`/`§7c`):
`overlay.hotkey` (`Ctrl+Alt+Z` — global summon; a failed registration is a **visible Settings
warning + toast**, never silent — D6) · `overlay.max_results` (8, clamped `1..=50` — top-N results in the overlay) ·
`resume.min_dwell_secs` (120 — min sustained-context dwell for where-was-i, `§7b`; D9) ·
`marks.hotkey` (`Ctrl+Alt+M` — mark-this-moment; same loud registration-failure handling as
`overlay.hotkey` — D6) ·
`api.enabled` (false — master switch; the API crate is **not constructed** unless on — D11) ·
`api.port` (43210, clamp 1024..=65535 — the bind is `127.0.0.1` only, **hard-coded, not a setting**; a
bind failure is **loud + guided-change**, `§7c`) ·
`api.token` (generated on first enable; the bearer token, regenerable in Settings — never blank while
enabled).

**0.3.2 lifecycle keys** (`§7d`; the only new settings surface this arc — new keys exist only where a
PR names them, `docs/0.3.2.md`). PR3 shipped these two names as proposed; they are **contract** now (D7).
`app.close_to_tray` (true — closing the main window hides to tray, capture continues; one-time
explanatory toast; off = window close quits cleanly, `§7d`) ·
`app.run_at_startup` (false — registers/unregisters launch-at-login; an explicit user choice, never
silently enabled — D3).
There is also an **internal marker** `app.tray_toast_done` (not part of the typed `Settings` struct;
same pattern as `api.token`, `§7c`): set to `true` the first time the one-time close-to-tray toast is
shown so it never repeats. It is written directly to the `settings` table and read at tray init; it
never rides through `get_settings`/`set_settings` or the ts-rs bindings.

**0.4.0 sessions keys** (`§7e`; the arc's **only** new settings surface — new keys exist only where a
PR names them, `docs/0.4.0.md`). The two names, defaults, and clamp ranges are final; PR5 owns only
their Settings-UI placement:
`sessions.min_len_secs` (120 — minimum session length; mirrors `resume.min_dwell_secs`, `§7e`;
clamp `30..=3600`) ·
`sessions.gap_close_secs` (120 — pass-1 micro gap close, retuned by the 11-day sweep;
usage review 2026-08-01 §6.7; clamp `60..=3600`).
Proposed Settings home: a new **Advanced** expander ("Sessions") under the 0.3.2 two-tier IA
(`UI_REFERENCE §3`) — PR5's call, not settled here.

**Dead-setting removal mechanics (0.3.2, D8):** a key may be retired in this arc only if **provably
inert** (the two annotated above). Retirement = UI removal + load tolerance for the orphaned key (no
error, no migration, no write-back requirement); if the config layer would error on an unknown key,
keep the field deserialized-but-unused rather than build migration machinery.

Capture honors `privacy.excluded_apps` (skip frame if foreground app matches) and
`privacy.pause_on_lock`; the **self-exclude capture gate also covers the 0.3.0 overlay window** (D7 —
the app's own overlay must never appear in its own capture history; asserted by the privacy-gate tests,
`§7b`). OCR runs on the **full-res** frame before resize/storage.

## 8b. Recall reports (0.2.x)

`generate_report(ReportRequest) -> ReportResponse` (`§7`) summarizes work/content over a time range
from **`frame_text.content_text`** (never raw full-screen text) and **cites the frames** it used.
There is **no saved-report table** — scheduled/saved reports are deferred (`07`).

- **Ranges.** Daily = local "today"; Weekly = trailing 7 local days; Custom = a selected range +
  optional user prompt.
- **Context strategy (tiered, weak-hardware-safe, no `ctx_size` bump):**
  - *Daily / small range* → **single pass** over content text (filtered text is ~150–400 tok/frame,
    so only ~20 frames fit the pinned 8192 answer context in the worst case — hence the
    `reports.map_reduce_min_frames` default, `§8`).
  - *Weekly / large range* → **map-reduce**: batch-summarize frames to fit 8192, then summarize the
    summaries (triggered above `reports.map_reduce_min_frames`, `§8`, so frames are batched rather
    than dropped). VRAM-flat; costs more sidecar calls.
  - Retrieval depth and reply budget are **per-request** (`ReportRequest` + settings `§8`), removing
    the hardcoded `ASK_TOP_K`. The existing answer budgeter (`crates/inference`) already packs frames
    best-first, drops overflow, and cites only what the model actually read.
  - `sidecar.ctx_size` stays the existing power-user / hardware knob — **not** bumped by default.
- **Honest framing:** "retrieve up to N, summarize what fits" — not a guarantee that all N frames
  are read. Empty / no-evidence ranges produce honest output (no fabrication), consistent with the
  Ask no-evidence behavior.

## 9. Logging & observability
`tracing` + daily-rotating file (`tracing-appender`) and console. Job-queue depth, sidecar state
transitions, and model load/evict are logged at info. No screen content or OCR text at info level
(privacy). Diagnostics surface dead-letter jobs.

## 10. Testing requirements
- **Unit:** each module against `traits` with fakes; `Store` against in-memory SQLite (`:memory:`).
- **Job queue:** state-machine tests (claim atomicity, retry/backoff, dead-letter, concurrent
  claim with N workers).
- **Retrieval:** FTS5 + vec KNN + RRF fusion correctness on a seeded fixture.
- **Sidecar lifecycle:** spawn → kill parent → assert child terminated (the no-orphan guarantee);
  startup reap; idle evict.
- **Windows-gated** (`#[cfg(windows)]`, may be `#[ignore]` in CI without GPU): WGC capture, WinRT
  OCR, real llama-server smoke.
- **Integration:** capture(stub frames) → OCR → store → embed job → search returns the frame →
  ask streams an answer.

## 11. CI/CD
The workspace MSRV is **Rust 1.88**; local rustup, CI, and release builds pin
**1.88.0**. The committed `image`, `time`, and `serde_with` versions already require
1.88. The `quinn-proto` security update (PR #112, GHSA-4w2j-m93h-cj5j) requires at
least 1.85 and is retained at 0.11.16. CI and release builds reject affected
`quinn-proto` lockfile entries with `scripts/check-quinn-security.mjs`; Cargo CI
and MCP staging use `--locked` so verification cannot silently resolve another graph.

GitHub Actions on `windows-latest`: `cargo fmt --check`, `cargo clippy --workspace -D warnings`,
`cargo build`, `cargo test` (GPU/WinRT tests `#[ignore]`d), `ui` `npm ci && npm run build`, and a
`tauri build` artifact job. Release workflow (later): **NSIS** installer (shipped v0.1.0; code-signing pending). Inno/MSI/portable ZIP dropped — `07` #26.

## 11b. Auto-update (0.3.2): signed manifest, passive UX

Issue #69 (`07` #96 — hard-sequenced **before** 0.4.0 ships). D1/D2 of `docs/0.3.2.md`, normalized as
contract:

- **Plugin + key (D2):** `tauri-plugin-updater` (Tauri 2) with a **minisign** keypair; the public key
  baked into `tauri.conf.json`; endpoint = this repo's GitHub Releases `latest.json` URL. The release
  pipeline signs the installer artifact with the updater key and publishes `latest.json` (version,
  notes URL, signature, per-platform URLs) as a release asset.
- **The signature is load-bearing:** a tampered, unsigned, or wrong-key manifest/artifact is
  **rejected** — no download installs. Tested as a negative path (PR2 acceptance, `docs/0.3.2.md` §3).
- **Passive UX (D1 — 0.3.1's D6 principle applied to the updater):** check on launch + a manual
  "Check for updates" (tray menu `§7d` + the Settings App section, `UI_REFERENCE §3`). When an update
  exists: a **quiet persistent indicator** (NavRail presence indicator — never a count — plus an
  App-section line, e.g. "v0.3.3 available — restart to update"); download in the background;
  **install only on user-initiated restart**. No modal, no nag, no auto-restart, ever. No update →
  **zero UI presence**.
- **Network posture:** the check is an outbound HTTPS GET against GitHub Releases — the same class as
  model downloads (`01 §5`, `04 §4`); no local data leaves the machine.
- **Key custody (D2 — RELEASE BLOCKER):** private key = CI secret + an offline
  maintainer-controlled backup, recorded in `07` manual steps **before** the first signed release is
  tagged. Key loss strands every installed copy on manual downloads again; a leak forces key rotation
  via a manually-shipped release.
- **Genesis + scope note:** `v0.3.2` itself is still a manual download and its release notes must say
  so; auto-update delivers releases *to* 0.3.2+ installs from then on (0.4.0 is the first). The
  minisign updater signature is **not Authenticode** — the Windows code-signing gap (`§11`, `07`
  manual steps) stays open and is explicitly not this feature.
- **First-auto-delivery gate (D16 — 0.4.0):** because 0.4.0 is the first release existing installs
  receive automatically **and** it carries the arc's one migration, `v0.4.0` is tagged **only** after
  the live path — a real 0.3.2/0.3.3 install → auto-update check → download → user-initiated restart →
  first boot runs the **10 → 11** migration → sessions appear over pre-existing history — passes
  end-to-end on a real machine, with the D5 backup (`07` manual steps) verified present first. Full
  item in `§13c` (PR7).

## 12. Failure modes & rollback
- **Migrations** forward-only via `schema_version`; each ships an idempotent up-script.
- **Job failure** → retry/backoff → dead-letter (visible), never silent loss.
- **Sidecar crash** → restart + requeue; repeated failure on a tier → surface a toast, fall back to
  Default tier.
- **Corrupt/oversized frame** → mark + skip; capture continues.
- **DB busy** → WAL + bounded retry.
- **Model tier** — both surviving tiers (Default/Quality) are vanilla-arch + Apache; the 0.3.0 Beta
  retirement removed the only hybrid-arch / non-Apache incompatibility risk (`02 §5c`).

## 13. Definition of done (v1.0)
1. Always-on capture → OCR → store works across multiple monitors; honors privacy settings.
2. Deferred embeddings populate text vectors via the job queue. *(0.3.0 removed the optional
   image-vector lane — §4.)*
3. Vision tagging runs **only** on-demand/timer/idle per setting — never real-time.
4. Hybrid search (FTS5 + vec → RRF) returns correct frames < ~200 ms on a realistic DB.
5. `ask` streams a grounded, *thinking* answer with citations to frames.
6. Model tiers (vision + answer: Default/Quality) selectable in settings and take effect via
   sidecar reload. *(0.3.0 retired Beta — `§8`, `02 §5c`.)*
7. **No orphaned `llama-server` after a forced app crash** — verified by test and manually.
8. `cargo clippy -D warnings` clean; all non-ignored tests green.
9. **NSIS installer builds successfully** (shipped v0.1.0; Inno/MSI/portable ZIP dropped — `07` #26); code-signing pending.

## 13b. Definition of done (0.3.0 arc)
Each item is demonstrated with verbatim command output or a described-and-performed live check
(`04 §6`); the per-PR acceptance detail lives in `docs/0.3.0.md`.
1. **Trigger trim (PR2).** `WH_MOUSE_LL`, `AddClipboardFormatListener`, and `typing_pause` appear
   nowhere in the tree (except CHANGELOG/specs history + the `capture_trigger` CHECK); event mode
   live-fires on alt-tab and on idle; a legacy `capture_trigger='click'` frame still renders in Moment;
   settings load drops the retired keys without error.
2. **Beta retired (PR3).** `Nemotron` / `Qwen3.5-9B` appear only in history docs; a settings file
   persisted with a `beta` tier loads cleanly as `quality`; both remaining tiers of both lanes still
   download/resolve per `MODEL_REGISTRY §4`.
3. **Image-lane removal (PR4).** `nomic` / `image_embedding` / `EmbedImage` appear nowhere outside
   history; fresh-DB and migrated-DB schemas agree; hybrid search is unchanged on the 10k-frame fixture
   (image arm was flag-off — parity shown). Forward-only schema bump +1 with a populated-DB migration test.
4. **Flow overlay (PR5).** Hotkey works while an unrelated app is fullscreen-focused; overlay visible
   **< 150 ms** (warm) and first results within the **< 200 ms** search budget, measured + shown; the
   overlay never appears in its own capture history after a live capture session (D7); a colliding
   hotkey shows the visible warning (D6).
5. **Where-was-i + marks (PR6).** where-was-i returns the correct run on a scripted fixture (unit) and
   live (work app → browser detour → hotkey → the work app's context offered); a mark from a fullscreen
   third-party app lands a frame captured at press time even on a static screen (diff-gate bypass shown
   — D8); the Intentions strip is live-verified. Forward-only schema bump +1 with a migration test.
6. **Local API + export (PR7).** API off by default (fresh profile: nothing listens); with it on, a
   request without / with a wrong token gets 401; a `0.0.0.0` bind is impossible by construction
   (asserted in a test); a port-bind conflict surfaces **loud + guided-change**; every endpoint is
   exercised against a fixture DB; Settings "Export…" produces a valid JSON file with the API disabled.
7. **MCP server (PR8).** `screensearch-mcp.exe` speaks the MCP handshake + tool listing over stdio; each
   tool round-trips against a live app with the API on; the API-off error path is clean; the NSIS
   installer includes the binary.
8. **Audit + release (PR9).** Every acceptance line above is verified end-to-end on a real Windows
   desktop (`docs/TESTING.md` 0.3.0 sections); `05`–`08` swept current; **v0.3.0** tagged and archived
   per `04 §7`; release notes lead with the **removals** + a one-line rationale each.
9. Full verification suite green for every PR: `cargo fmt --check` · `cargo clippy --workspace
   --all-targets -D warnings` · `cargo build --workspace` · `cargo test --workspace` · `ui`
   `npm run lint && npm run build` · `git diff --exit-code -- ui/src/bindings` clean.

## 13c. Definition of done (0.4.0 arc)
Each item is demonstrated with verbatim command output or a described-and-performed live check
(`04 §6`); the per-PR acceptance detail lives in `docs/0.4.0.md`.
1. **Specs contract (PR1).** No code changes (`git diff --name-only main` shows only `.md`); every
   decision D1–D16 appears in the specs verbatim or normalized into contract language; a fresh agent
   session can implement any of PR2–PR6 from the specs alone without reopening `docs/0.4.0.md`.
2. **Ground truth + harness (PR2).** The candidate segmenter runs end-to-end over a real 5–10-day
   export; boundary precision/recall + tool-recognition accuracy scored against the maintainer's hand
   labels; chosen parameters + **proposed D9 thresholds** recorded in `05`/`06`. The labeled dataset is
   kept **local-only** (git-ignored — it is personal screen history). **Hard stop condition:** if no
   reasonable parameterization reaches credible boundary agreement, stop and report — the arc redesigns
   *before* PR3 freezes a schema.
3. **Sessions schema + migration (PR3).** The 10 → 11 migration is forward-only, bumps `schema_version`
   by exactly one confirmed against `schema.rs`, and passes a **populated-DB migration test** on a
   schema-10 fixture with real-shaped data; fresh-DB and migrated-DB schemas agree; every frame-level
   feature (search, Ask, reports, marks, overlay, where-was-i) is demonstrably unchanged on the fixture
   (D10); the **D5 backup gate** step is documented in `07`. Structure only — no backfill in the
   migration. Forward-only schema bump +1 with a populated-DB migration test.
4. **Segmentation engine + recognition (PR4 — the binding D9 gate).** The shipped segmenter meets the
   PR2-recorded thresholds when re-run through the harness (a miss is stop-and-report, not
   tune-until-green); recognition is demonstrated live on a real **Claude Code** session, a **Codex**
   session, a **browser-AI** session, and a **meeting-titled** window; the historical pass completes on
   the live DB without disturbing capture; exchange extraction is shown on a real AI session
   (qualitative, maintainer-judged); where-was-i and all frame-level features are unchanged.
5. **Sessions in the UI (PR5).** The drill-in round-trips live (band → drill-in → Moment → back); the
   Recap cites real frames; zero shell-layout-contract violations (one scroll context per route, no
   nested scrollbars — `UI_REFERENCE §8`); no new NavRail route (D13); `npm run lint`/`build` clean.
6. **API + MCP session exposure (PR6).** Every new endpoint is exercised against a fixture DB; the
   API-off and bad-token paths are clean; each MCP tool (`list_sessions`/`get_session`/`ask_session`)
   round-trips against a live app; `ask_session` answers cite **only** frames from the named session;
   `docs/API.md` + `docs/MCP.md` updated.
7. **Audit + release (PR7).** Every acceptance line above is verified end-to-end on a real Windows
   desktop (`docs/TESTING.md` 0.4.0 sections); D1–D16 are confirmed landed; `05`–`08` swept current and
   `07` carries every deferral from `docs/0.4.0.md` §2. **The D16 first-auto-delivery gate passes**
   (0.3.2/0.3.3 install → auto-update → restart → 10 → 11 migration on first boot → sessions over
   pre-existing history) with the D5 backup verified present beforehand; **v0.4.0** tagged and archived
   per `04 §7`; release notes lead with **sessions** + the **#88** close, and repeat the
   unsigned-installer (no Authenticode) caveat.
8. Full verification suite green for every PR: `cargo fmt --check` · `cargo clippy --workspace
   --all-targets -D warnings` · `cargo build --workspace` · `cargo test --workspace` · `ui`
   `npm run lint && npm run build` · `git diff --exit-code -- ui/src/bindings` clean.

---

*Next layer:* `04_CLAUDE_CODE_BUILD_PROMPT.md` — how the agent operates against `00`–`03`
(reading order, source-of-truth, build order, guardrails, stop-at-ambiguity).
