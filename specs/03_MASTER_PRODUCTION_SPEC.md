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
│   ├── api/                   # opt-in HTTP API (axum, configurable IPv4/IPv6, default loopback, bearer token) + export (0.3.0, §7c)
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

Single file `screensearch.db`. **Baseline rule (0.4.5 D2):** this section holds the complete DDL for
the current `schema_version = 15`; the app creates a fresh DB from it and ships **no migration
chain**. A schema change edits this baseline and bumps `schema_version`; there are no populated-DB
migration tests. A DB whose `schema_version` is not 15 refuses to open with a message naming the
rebuild tool (`crates/rebuild`'s `ssv2c-rebuild`); that dev-only tool reads the old v13 DB **read-only**
and writes a fresh v15 file; keeping the source as `*.v13.bak` is a maintainer step after the new
file is confirmed to open — a clean start instead of a rebuild is the maintainer's Gate A choice,
never automatic (0.4.5 D3). The shipped app never migrates history; the 0.2.x–0.4.2 forward-only
chain is history, not the rule (0.4.5 D2).

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

-- 0.2.x text signal (schema_version 2 → 3; clean DB, forward-only migration authored in PR2 — history
-- only: from 0.4.5 the baseline rule above governs and no migration chain exists, 0.4.5 D2).
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
  kind         TEXT NOT NULL,               -- 'embed_text' | 'vision_tag' | 'graph_extract' | 'graph_normalize' | 'graph_cluster' | 'graph_backfill' | 'session_embed' | 'insight_generate' (0.3.0 PR4 removed 'embed_image'; the eight values are exactly the `JobKind` variants, crates/traits/src/jobs.rs). No CHECK — matches schema.rs; a value CHECK is optional hardening tracked in 07 #82.
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

-- 0.4.5 work graph (0.4.5 D8): entities extracted from captured text, linked to sessions (§7h).
CREATE TABLE entities (
  id            INTEGER PRIMARY KEY,
  kind          TEXT NOT NULL CHECK (kind IN ('repo','file','domain','document','ref','app')),
  canonical_name TEXT NOT NULL,
  normalized_key TEXT NOT NULL,          -- lowercase, trimmed, path/URL-normalized; the dedup identity
  source        TEXT NOT NULL CHECK (source IN ('heuristic','llm')),
  confidence    REAL NOT NULL,           -- 0..1; the extraction rule's confidence, never invented
  first_seen_at INTEGER NOT NULL,        -- unix ms
  last_seen_at  INTEGER NOT NULL,
  created_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at    INTEGER NOT NULL DEFAULT (unixepoch()*1000),
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

-- session ↔ entity link. A session touches an entity at most once; hit_count is the evidence weight.
CREATE TABLE session_entities (
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  entity_id  INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  hit_count  INTEGER NOT NULL DEFAULT 1,
  confidence REAL NOT NULL,
  first_seen_at INTEGER NOT NULL,
  last_seen_at  INTEGER NOT NULL,
  PRIMARY KEY(session_id, entity_id)
);
CREATE INDEX idx_session_entities_entity ON session_entities(entity_id, last_seen_at DESC);

-- 0.4.5 threads: clusters of sessions sharing strong entities within a decaying window (§7h).
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

-- A session belongs to at most ONE thread (UNIQUE session_id); split/merge rewrite these rows.
CREATE TABLE thread_sessions (
  thread_id  INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  session_id INTEGER NOT NULL UNIQUE REFERENCES sessions(id) ON DELETE CASCADE,
  confidence REAL NOT NULL,
  PRIMARY KEY(thread_id, session_id)
);
CREATE INDEX idx_thread_sessions_thread ON thread_sessions(thread_id);

-- User overrides always beat the clusterer (§7h). payload is typed JSON per kind, validated by traits.
CREATE TABLE thread_overrides (
  id               INTEGER PRIMARY KEY,
  kind             TEXT NOT NULL CHECK (kind IN ('rename','merge','split')),
  thread_id        INTEGER REFERENCES threads(id) ON DELETE CASCADE,
  target_thread_id INTEGER REFERENCES threads(id) ON DELETE CASCADE,
  payload          TEXT NOT NULL,        -- rename: {"title":…} · merge/split: {"session_ids":[…]} / {"thread_id":…,"session_ids":[…]}
  created_at       INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  CHECK ((kind = 'rename' AND thread_id IS NOT NULL AND target_thread_id IS NULL)
      OR (kind = 'merge'  AND thread_id IS NOT NULL AND target_thread_id IS NOT NULL)
      OR (kind = 'split'  AND thread_id IS NOT NULL))
);

-- 0.4.5 session vectors (§7i): session title+summary embeddings, same model + dim as text embeddings.
CREATE VIRTUAL TABLE session_vectors USING vec0(
  session_id INTEGER PRIMARY KEY,        -- == sessions.id
  embedding  FLOAT[768] distance_metric=cosine
);

-- 0.4.5 insights (§7j): typed payload is the truth; the model never owns a row.
CREATE TABLE insights (
  id             INTEGER PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('thread_activity','open_loop','recurrence','semantic_neighbour','resumption')),
  window_start   INTEGER NOT NULL,       -- unix ms
  window_end     INTEGER NOT NULL,
  payload        TEXT NOT NULL,          -- typed JSON, rendered without the model
  generator      TEXT NOT NULL CHECK (generator IN ('heuristic','llm_phrased')),
  generated_at   INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  stale          INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0,1))
);
CREATE INDEX idx_insights_kind ON insights(kind, generated_at DESC);
CREATE INDEX idx_insights_window ON insights(window_start, window_end);

-- An insight without evidence is a defect (§7j). SQLite has no deferred constraints across tables, so the
-- rule is enforced by the store API (insight + evidence written in one transaction; zero evidence → Err) + a test.
CREATE TABLE insight_evidence (
  insight_id INTEGER NOT NULL REFERENCES insights(id) ON DELETE CASCADE,
  ordinal    INTEGER NOT NULL,
  kind       TEXT NOT NULL CHECK (kind IN ('frame','session','thread','entity','mark','report')),
  ref_id     INTEGER NOT NULL,
  PRIMARY KEY(insight_id, ordinal)
);

-- 0.4.5 Ask v2 conversations (§7i).
CREATE TABLE ask_threads (
  id         INTEGER PRIMARY KEY,
  title      TEXT,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch()*1000)
);
CREATE TABLE ask_turns (
  id             INTEGER PRIMARY KEY,
  thread_id      INTEGER NOT NULL REFERENCES ask_threads(id) ON DELETE CASCADE,
  ordinal        INTEGER NOT NULL,
  question       TEXT NOT NULL,
  answer         TEXT,                   -- NULL while streaming / on failure
  window_start   INTEGER,
  window_end     INTEGER,
  resolved_entities TEXT,                -- JSON array of entity ids resolved for this turn
  scope_thread_id INTEGER,               -- work-graph thread the turn was scoped to (no FK)
  citations      TEXT,                   -- JSON {"sessions":[…],"frames":[…]}; NULL until it finishes
  created_at     INTEGER NOT NULL DEFAULT (unixepoch()*1000),
  UNIQUE(thread_id, ordinal)
);

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

**Historical migrations (0.2.x–0.4.2; context only).** Under the baseline rule above these steps
exist only in git history and in `CHANGELOG-ARCHIVE.md`; the v15 baseline is the whole story.
Retained so the provenance of `frames.session_id`, `marks`, `report_runs.kind` and the removed image
lane stays readable.

**0.3.0 migrations, as shipped** (forward-only; each bumped `schema_version` by **exactly one** —
D15 — confirming the next integer against `crates/store/src/schema.rs::LATEST_SCHEMA_VERSION` rather
than a hardcoded number, and shipping a populated-DB migration test, the 0.2.1 `frames`-rebuild test
as the pattern). Superseded by the baseline rule: no such chain or test exists from 0.4.5 on
(0.4.5 D2).
- **PR4 — image-lane drop:** `DROP TABLE image_embedding_vectors; DROP TABLE image_embeddings;` (the
  `AFTER DELETE` trigger goes with its table) and `DELETE FROM jobs WHERE kind = 'embed_image';` (in
  any state). This destroys **derived, re-derivable** vectors only — frames, stored images, text, and
  text embeddings are untouched. Acceptance: fresh-DB and migrated-DB schemas agree, and hybrid search
  is unchanged on the 10k-frame fixture (the image arm was flag-off, so parity is expected and shown).
- **PR6 — marks:** creates the `marks` table above. **Retention (D10):** a marked frame follows
  **normal** retention — its image still expires like any other frame (the mark keeps the text
  reconstruction reachable); an unresolved mark never pins a frame or blocks retention. No retention
  pinning in 0.3.0.

**0.4.0 migration, as shipped** (the arc's **only** schema change — D4; same forward-only mechanics:
bumped `schema_version` by **exactly one** — 10 → 11 — with the next integer confirmed against
`crates/store/src/schema.rs::LATEST_SCHEMA_VERSION` rather than a hardcoded number, and a
populated-DB migration test, the 0.2.1 `frames`-rebuild test as the pattern; if any **other** 0.4.0
PR had appeared to need schema, that PR was wrong — a stop-and-report, not a schema edit). Superseded
by the baseline rule (0.4.5 D2): v14 is created whole, and this step survives only as history.
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
  pool reacts to *sustained* CPU/GPU/memory pressure with graded backpressure: at level ≥ 1 (High) the
  claim-kind gate drops `vision_tag` from the claimable set (heavy enrichment
  pauses; 0.3.0 PR4 removed `embed_image`); at level 2 (Sustained) an in-flight gate additionally floors concurrent `embed_text` to
  `throttle.embed_text_floor` (≥1), **and the resident inference model is released** (see below).
  **Capture / OCR / storage never throttle** — they are
  structurally outside the worker pool. Pressure is read through a `PressureProbe` seam injected by
  the composition root (the kernel forbids `unsafe`, mirroring the `IdleSource` / `BackfillControl`
  seam); the Windows-native impl lives in the `sysmon` crate (CPU via `GetSystemTimes`, GPU via PDH
  `\GPU Engine(*)\Utilization Percentage` English counters, summed across engines — vendor-neutral;
  physical memory via `GlobalMemoryStatusEx`, system commit charge via `GetPerformanceInfo`
  (`CommitTotal`/`CommitLimit` scaled by `PageSize`)).
  Those GPU counters are vendor-neutral but not universal: on the maintainer's NVIDIA box the
  bundled sidecar's Vulkan compute is filed under `engtype_3d` alone (25.5 % peak; 37.6 % summed
  across engines) while `nvidia-smi` reads 96 %, so the GPU arm stays below `gpu_enter_pct` for that
  workload. `06` #23 records the measurement and three options; nothing is implemented. For a
  GPU-heavy workload that leaves host memory and CPU quiet, no arm fires — that is the limit `07`
  records, and the memory arm does not pretend to be per-process accounting.
  When GPU counters are absent the probe latches a truthful **"GPU not monitored"** state and the
  throttle runs on the remaining arms; the memory arm needs two syscalls per sample and both must
  succeed, otherwise that sample reports `mem_monitored = false` (never a fabricated 0 %); the
  reading is per sample, not latched.
  **The memory arm exists because CPU/GPU pressure only sees a *busy* machine.** A machine can be out
  of RAM while ScreenSearch is idle — a large editor, another recorder, several browsers, a scheduled
  job — and that is the failure the user notices (the app cannot allocate). Both *physical usage*
  (`1 − available/total`) and *system commit usage* (`committed/limit`) are watched: commit counts
  pages a process has committed, not pages that are resident, so it moves for a process that has
  committed far more than it is actively using — before physical RAM runs out. (Reserving address
  space charges nothing and is not what this arm watches.) Both are expressed as **usage**
  percentages so they share the CPU/GPU convention (`*_exit_pct` < `*_enter_pct`), and the byte
  counts (`avail_phys_mb`, `commit_charge_mb`, `commit_limit_mb`) travel
  in the sample so the UI can show "8.3 GiB available" (available, never "free") and a commit
  percentage rather than a bare usage number. Either reading at its enter threshold raises the
  level; **both** must fall below their exit thresholds before an exit dwell can lower it, because
  they describe the same exhausted resource.
  **Level-2 relief:** pausing `vision_tag` frees nothing — the 6–8 GiB is held by the sidecar, not by
  the workers. So on the *transition into* level 2 (edge-triggered, never per tick) the governor calls
  a `PressureRelief` hook (`dyn` over `traits`, implemented by the `ModelSupervisor`) that unloads the
  resident `llama-server` exactly like the manual Unload control: drain the request gate, kill the
  child, emit `Evicted`. **Relief outranks the backfill's keep-warm** — keep-warm exists so a drain is
  not cold-started between batches, which is not a promise to hold VRAM while the machine is out of
  memory — but **never an explicit user preload**: a model pinned by the user's own load stays
  resident and relief logs the skip. Holding level 2 does not retry; a new escalation (a full exit
  dwell and a fresh enter dwell) is the only way to ask again, so a long starvation window cannot
  become a kill/reload loop.
  A pure `ThrottleMachine` (clock-injected, no Win32) applies hysteresis
  (`*_exit_pct` < `*_enter_pct`) and dwell timers, stepping one level per dwell; a kernel governor
  loop samples the probe each `throttle.sample_interval_ms`, publishes the level into a shared
  `AtomicU8` the pool reads (no pool restart on level change), and broadcasts `throttle_changed`
  (§7). All thresholds are settings, never hardcoded (§8).
- **The in-process text embedder is a lazy holder with an idle TTL.** `LazyEmbedder` allocates
  nothing until the first `embed_texts` (single-flight, so `N` concurrent callers start exactly one
  ONNX session) or an eager `load_now` at startup, so a first search after readiness reported
  `Ready` does not pay a cold start. `dim()` and `text_model_name()` are metadata only and never
  load the model. An embed holds the session for its whole run and stamps last-use as it finishes,
  even when it fails. A sweep every 30 s evicts only when no embed still holds the session — the
  slot is taken under its write lock and the session's strong count must be 1, the slot itself — so
  neither the sweep nor an explicit unload can drop a session out from under a running embed (both
  return `false`), and a second session cannot load alongside it. The load gate spans teardown, so
  a reload cannot overlap it, and the load itself holds that gate rather than the caller waiting for
  it: cancelling an embed that has not been handed a session yet cannot release the flight, so
  `M::load`, the publication of the loaded session into the slot and the last-use stamp all complete
  under the held gate, and the next embed reuses that published session instead of starting a second
  load beside the first. Once `enrich.embed_idle_ttl_secs` has passed with no embed (`0`
  disables the sweep), the sweep releases the session; the next embed reloads. Session teardown runs
  off the async worker thread. This bounds the embedder's resident footprint when the app is idle;
  it changes no capture, OCR or worker contract.

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
   `sidecar.idle_ttl_secs` of no requests (frees GPU/RAM — the footprint control). While the idle
   backfill is draining, eviction is suppressed (keep-warm) — **except under level-2 memory relief**
   (§5), which releases the model regardless: relief outranks keep-warm, because a keep-warm drain
   must not hold the machine's memory hostage. An explicit user preload (`pinned`) is the one thing
   that outranks relief; it is reported as a skipped release, never a silent unload.
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
| `set_api_config` | `{ enabled, port?, bind_address? }` → `ApiStatus` (enable/disable + IP literal + port; bind failure is loud + guided-change — `§7c`) |
| `get_api_status` | `()` → `ApiStatus` (0.3.0; enabled, bound port, token-present — `§7c`) |
| `regenerate_api_token` | `()` → `ApiStatus` (0.3.0; new bearer token — `§7c`) |
| `list_sessions` | `SessionQuery` → `Session[]` (0.4.0 PR5; filters kind/tool/`TimeRange`/limit; also the Timeline session-bands data source — `§7e`, `UI_REFERENCE §3`) |
| `get_session` | `session_id` → `SessionDetail` (0.4.0 PR5; span + tool/host + exchanges; `include_summary` triggers lazy generation — `§7e`) |
| `session_recap` | `session_id` → report (0.4.0 PR5; the existing **§8b report engine** scoped to the session's time range + frames — no new summarization machinery, D3) |
| `get_setup_status` | `()` → `SetupStatus` (2026-10-20; current-session setup/acquisition snapshot — `§7g`) |

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
nudge, badge, or score** is produced (D10/D11). `model_download` (payload `ModelDownloadStatus`,
per-lane GGUF progress) and `setup_changed` (2026-10-20; payload the whole `SetupStatus`
snapshot, `§7g`) drive the download/setup progress UI.

**`ThrottleStatus` shape** (0.2.1, smart enrichment throttle; memory fields added 2026-11-09):
`{ enabled: bool, level: u8 (0=Normal / 1=High / 2=Sustained), sample: Option<PressureSample>,
gpu_monitored: bool, mem_monitored: bool }`, where `PressureSample = { cpu_pct: f32,
gpu_pct: Option<f32>, gpu_monitored: bool, mem_used_pct: f32, commit_pct: f32, avail_phys_mb: u64,
commit_charge_mb: u64, commit_limit_mb: u64, mem_monitored: bool, sampled_at: i64 }`.
`gpu_monitored=false` (and `gpu_pct=None`) is the truthful "GPU not monitored" state — the throttle
then runs on the remaining arms. `mem_monitored=false` is the same honest state for the memory arm:
the percentages read `0.0` and the UI says "not monitored" rather than printing 0 %. Both flags
mirror the probe so the UI can say so before the first sample exists.

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

## 7c. HTTP API + export (0.3.0 origin; Gate A network amendment 2026-10-15)

A new crate `crates/api` (axum; behind a trait, wired in the composition root, **not constructed at
all** unless enabled) exposes the same core queries over HTTP for scripts/agents on reachable hosts. It reuses
`hybrid_search`, the ask pipeline, the where-was-i heuristic (§7b), and marks — no new retrieval code.

**Posture (configurable-bind correction; supersedes the earlier Gate A wildcard default).**
Default **OFF** (`api.enabled=false`). `api.bind_address` accepts any IPv4 or IPv6 address
literal, including loopback, a specific interface, or a wildcard (`0.0.0.0` / `::`). Missing
values default to **`127.0.0.1`**, including existing enabled profiles; an upgrade must never
broaden access automatically. Reject hostnames, URLs, ports and invalid literals visibly.
Address and port changes persist and apply immediately; preserve enabled intent and surface
bind failures with guided correction. Clients use an actual reachable address, never a wildcard
listen address. IPv6 URLs bracket the address, for example `http://[::1]:43210`.
Every request, including health and unknown routes, requires a bearer token. Tokens are generated
on first enable and remain revealable, copyable and regenerable in Settings. Missing/wrong tokens
return 401; authenticated unknown routes return 404. Default port is 43210. Rotation takes effect
without restart. Preserve disable/re-enable behavior and Windows exclusive socket ownership.
Settings reports the configured address and actual listening/error state honestly.

**Connection bounds (PR #122 review amendment, maintainer-approved).** Every enabled listener,
including loopback, admits at most **64 active connections**, before authentication. Accept and
immediately close excess sockets rather than queueing application tasks. Require each request's
headers within **30 seconds**, using an installed runtime timer; idle keep-alive connections are
subject to the same next-header deadline. This is not a response/body deadline: long Ask SSE and
export responses must remain usable. These are fixed implementation constants, not new settings.
Shutdown stops accepting, drains for the existing bounded grace period, then cancels every owned
connection task; no connection may survive server shutdown. Verify raw-socket cap/recovery,
incomplete-header expiry, streaming beyond the header deadline, and shutdown on Windows.

The API uses plain HTTP without transport encryption. With a non-loopback bind, any token holder
on a reachable host can read the entire screen history, and network observers may intercept tokens
and responses. Network binding is an explicit user configuration decision. No wildcard CORS,
cookie authentication or token-bearing URLs. No upgrade notice or acknowledgement is necessary:
profiles lacking the new key retain loopback exposure; the earlier automatic-broadening notice
contract is superseded and its internal marker is no longer used.

**Network boundary.** This is not public hosting. No automatic firewall changes, router port
forwarding or inference exposure. Document a user-managed Windows inbound-allow step and explain
that 200/401 is an HTTP result while a timeout can indicate routing/firewall failure. A wildcard
bind itself does not guarantee LAN-only reachability. The approved standalone remote-wrapper
amendment permits the stdio HTTP-client wrapper on other OSes and explicit remote HTTP/HTTPS
destinations only. Default URL remains loopback; preserve `.no_proxy()`, validate destinations and
refuse every redirect. The app/capture/inference remain Windows-only, inference loopback-only;
no network-hosted MCP server, listener or CORS change. This approves scope, not shipped behavior. Scheduled reports, vault
writes, report endpoints and Telegram are outside this increment.

**Acceptance for the network increment.** Assert actual bound sockets for IPv4/IPv6 loopback,
specific-interface and wildcard configurations; connect to usable addresses in API/MCP fixtures.
Prove missing-key enabled upgrades remain on `127.0.0.1`, invalid addresses fail visibly, disabled
means no listener, authentication covers all routes, address changes apply immediately, and token rotation takes
effect without restart, bind errors remain visible and status reports the listen address honestly.
On Windows claim the API port with `SO_EXCLUSIVEADDRUSE` before bind: a wildcard listener
must reject an existing loopback/interface-specific listener as well as another wildcard
listener. Otherwise local MCP could connect to a different process on the configured port.
Run the native Windows build and app; from a separate machine verify `/v1/health` returns 200
with the actual version for a valid token and 401 for missing/wrong tokens. Verify local MCP,
disable/enable, app restart and Ask stream disconnect, and confirm the inference sidecar remains
loopback-only. Record any manual test-machine firewall action separately; missing remote evidence
is a blocked acceptance gate, never a substituted loopback success.

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
- **0.4.5 (P8, 0.4.5 D12 — read-only, no new write scopes):** four additive reads over the work graph
  and insights (`§7h`/`§7j`), behind the same genuine read-credential authorization as `§7f` and with
  the same window/`limit` conventions as `GET /v1/sessions`:
  - `GET /v1/threads` — threads overlapping the requested window, in an explicitly stated order.
  - `GET /v1/threads/{id}` — one thread with its sessions and its member entities.
  - `GET /v1/entities` — entities, filterable by kind and by name/alias substring.
  - `GET /v1/insights` — insights whose window overlaps the request, each with its evidence ids.
  All four are GETs: none generates, regenerates or writes anything, and a read never starts
  inference — the rule the 0.4.0 session reads already follow. `docs/API.md` grows with them in the
  same phase (P8, 0.4.5 D12).

Docs: a hand-written `docs/API.md` (OpenAPI-lite; v1 is small, no codegen), authored by PR7; PR6
extends it with the sessions endpoints; P8 extends it with the 0.4.5 threads/entities/insights reads
(0.4.5 D12).

**MCP server (PR8, D13).** A separate workspace **binary** crate `crates/mcp` → `screensearch-mcp.exe`,
shipped in the NSIS installer — a thin **stdio** wrapper over this HTTP API, with **no store access and
no app coupling**: it is purely an HTTP client, default `127.0.0.1:<port>`, with explicit
remote destination support approved under `§7f` (not assumed implemented). The bearer token comes from
args/env (`SCREENSEARCH_API_URL` / `SCREENSEARCH_API_TOKEN`). Tools: `search_screen_history`,
`ask_screen_history`, `get_moment`, `where_was_i`, `list_marks`, `add_mark`. If the API is off, every
tool returns a clear "enable the API in ScreenSearch Settings" error. Docs: `docs/MCP.md` (copy-paste
client config for Claude Desktop / Claude Code + the same threat-model paragraph), authored by PR8.
**0.4.0 (PR6, D12):** the wrapper gains `list_sessions`, `get_session`, and `ask_session` (over the
`§7c` sessions endpoints) — still read-only, still a stdio HTTP client with no store access (0.3.0
D13 holds); `docs/MCP.md`'s tool table + threat-model boundary grow with them.
**0.4.5 (P8, 0.4.5 D12):** the wrapper gains `list_threads`, `get_thread`, `find_connections` and
`list_insights` over the `§7c` 0.4.5 read routes — still read-only, still a stdio HTTP client with no
store access; `docs/MCP.md` grows with them in the same phase.

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

## 7f. External-agent memory API — approved normative contract (2026-10-16)

**Authority and implementation state.** Nico approved execution of the external-agent memory
API plan, including its recommendations. This section freezes the approved contract; it is not
an assertion that these endpoints, permission controls or portable artifacts exist at the
planning base `cd9f5fecfa35970962be0067f76105553b24bfba` (0.4.1, schema 12). The attachment's
DRAFT/awaiting-GO labels are superseded by that direct instruction. Do not request redundant
blanket approval. Independent review of the remote-wrapper boundary precedes its implementation;
no merge, release, publication, updater action or live-profile write is authorized.

**Scope precedence.** Saved-output HTTP/MCP reads are promoted from the earlier deferral in
`§8c`; remote generation, retry, scheduling, delivery, credential-management HTTP APIs and
filesystem repair remain excluded. `§7c` is unchanged for existing listener defaults, socket
ownership, authentication of unknown routes, 64-connection cap, 30-second header deadline,
shutdown and sidecar isolation. This section adds read contracts, not a listener rewrite.
The ordinary local workflow with API disabled remains compatible. Retrieval/report defaults
stay filtered; only evidence enumeration exposes raw and filtered layers separately. No query,
answer model, scheduler, output folder or Telegram is a prerequisite for evidence/report reads.
DTOs and Store seams belong in `traits`; SQL belongs in `store`; HTTP adapts through the host/
composition root. MCP remains an HTTP-only client with no Store dependency. No schema migration
is approved: the initial implementation uses existing settings and report rows. If invariant
protection or measured query plans require a metadata table/index, stop for a separately approved
forward-only migration with populated-DB tests; do not silently rebuild frames or change schema 12.

### 7f.1 Chronological evidence pages

`GET /v1/frames?from=<UTC-ms>&to=<UTC-ms>&app=<literal>&window_title=<literal>&limit=<n>&cursor=<opaque>`

- Initial request requires finite `from` and `to`, `from < to`, half-open `[from,to)`. No query term, model or report dependency. Client examples convert a calendar day in an explicitly chosen timezone to UTC bounds, including DST.
- Order is **`captured_at ASC, id ASC`**, not ID-only append order. App is case-insensitive exact match using the existing SQLite NOCASE convention; title is literal substring using the same documented ASCII case-fold behavior. `%` and `_` are literal characters, not caller-supplied SQL wildcards. Resolve app/title from target-window metadata when present, falling back to frame metadata; never inferred vision labels. Null metadata does not match a supplied filter. Filters run before limit.
- `limit` defaults 100, valid 1–1000; invalid bounds/limits/filters/cursors return JSON 400, not an empty success. App and title filters are each capped at 256 UTF-8 bytes before normalization (400 above the bound). Use a canonical compact encoding; add a worst-case escaping test proving every accepted filter combination produces a cursor within the 4096-byte encoded wire limit. Cursor input length is capped at the same limit. Continuations provide the cursor and optional limit only; filters/time are frozen in the cursor. Mixing cursor with filters is 400.
- Response: `items`, `next_cursor` (explicit null at exhaustion), `history_id`, `window`, `snapshot_max_frame_id`, `observed_at`, and `consistency: "bounded_membership_live_content"`. Cursor v1 carries history identity, normalized filters/window, fixed maximum frame ID, and last `(captured_at,id)`; validate all fields. A cursor is not a credential. A mismatched history identity returns 409 `history_changed`; malformed/version-unsupported cursors return 400.
- Select max frame ID and the first page in one database read transaction. Later pages filter `id <= snapshot_max_frame_id` and seek strictly after the last tuple. With allocator-active writers only, new/backdated inserts cannot extend the active traversal. Old/direct writers can silently violate this bound; unsupported writes and state rewinds require checkpoint invalidation and reconciliation, not an assumed server-side detection guarantee. Equal timestamps and out-of-order capture timestamps remain deterministic. Cursor replay is retryable but not a byte-identical snapshot of mutable text.
- Each item returns stable `frame_id`, timestamp, monitor, stored app/title/URL, full `raw_text`, full `content_text`, text source/filter version when present, stored vision text when present, session reference when present, and `image_purged`. No text snippet truncation or dropped textless frames; missing text is null plus `text_state: "not_available"`, not proof of empty OCR or permanent failure. Stored-but-empty text is distinguished. Images use the existing detail image route, not inline page blobs; filesystem paths are not part of the new DTO.
- Retention or deletion can remove members between pages: this is not a frozen DB snapshot and not a full replication feed. A page may shrink; the keyset still progresses. Missing detail remains 404 and purged image retains its existing error. Return complete text or an explicit error, never silently truncate to fit MCP context. Clients can lower page size.
- New frames and later OCR/filter/vision updates require a new bounded traversal or detail re-read. Document a retryable reference consumer: persist page cursor only after processing, upsert by `(history_id,frame_id)` while comparing the recorded `captured_at`; a changed timestamp is an identity conflict, never a silent overwrite. Preserve the old fact, invalidate the traversal and reconcile the window. This detects some reuse, not same-timestamp reuse. Re-read textless IDs, and periodically re-scan the selected window to reconcile late enrichment. Do not promise eventual completeness after one traversal or claim exhaustion means capture completeness.

**Identity prerequisite:** initialize a distinct `api.history_id` once during app/store setup, not on GET and not by repurposing resettable `memory.source_id`. Preserve across moves/restores; independently active clones are not auto-detectable. Use a transactional persisted high-water allocator for all production frame inserts (`api.frame_id_high_water` in existing settings), initialized at the existing maximum. Allocate explicitly above both persisted high-water and current maximum; never reduce on deletion. Treat both identity and allocator settings keys as reserved internal metadata: user settings reset/import/export and retired-key cleanup must neither copy them into another history nor remove/lower them in place. Initialize missing keys only for first upgrade/new history; an established history with missing allocator metadata must fail feed readiness, not silently recreate a lower counter. The P0 inventory below covers every settings write/delete/reset/import path before accepting this storage choice. If that invariant cannot be enforced with current settings storage, stop and propose a dedicated metadata table with a forward migration rather than weakening identity guarantees. This preserves existing IDs and prevents future reuse without table reconstruction. Previously deleted/reused IDs cannot be repaired retroactively. Persist the observed allocator high-water alongside client checkpoints and expose the current allocator high-water in coverage. A decrease is a detectable rewind signal: invalidate the cursor and re-scan the selected window. This signal is sufficient but not necessary: a restored DB can catch up before observation, so invisible rewinds remain possible. An operator restoring a backup must invalidate client checkpoints explicitly even without a decrease. No cross-restore global exactly-once promise. Old binaries do not maintain the allocator: returning to them invalidates the new feed's continuity guarantee.

### 7f.2 Bounded coverage/status, not activity estimates

`GET /v1/coverage?from=<UTC-ms>&to=<UTC-ms>` requires the same valid half-open bounds.

Return request window; observation time; current capture-enabled state (explicitly intent, not proof of health); current persisted allocator high-water; global oldest/newest stored frame and newest timestamp; in-window stored-frame and text-available counts; image-retention days with `0=keep`; image-only retention semantics; saved-report retention policy; and capture-observation availability.

Minimal known-interruption support is a **process-local bounded record**, not a new journal: record capture start/stop and worker errors already observed by the controller, with reason enum and observed timestamp. Keep up to 1000 transitions; return at most 100 intersecting intervals, `truncated`, observation start and unavailable-before boundary. An open pause has null end. On restart, previous-process intervals are explicitly unavailable; do not infer sleep, lock, privacy exclusion or crash durations from missing frame timestamps. A lost/overflowed observation is unknown, never silently complete. No OCR, titles or paths in transition records. Retention settings are policies, not proof of successful purge at a specific time.

Empty window means **no recorded frames in this query**, not “nothing happened.” Even enabled capture plus no frames cannot prove inactivity: diff gating, errors and privacy gates exist. No work-duration/activity scoring, no fabricated percentage of a person's day. Storage failures are errors, not NO_DATA. Keep `/health` compatible and cheap; coverage is a separate read.

### 7f.3 Genuine read-only credential

Add one independently generated/revocable/rotatable **read-only token** alongside the existing token; do not build a user-account or arbitrary-scope administration subsystem. Existing token and clients retain existing access; upgraded profiles have no read token until the user creates one. Creating/rotating it does not enable API or change bind address.

Define internal permission classes now: `history.read`, `marks.write`, `capture.request`, `answer.generate`; reserve `reports.generate` as unsupported. Read token grants only `history.read`; legacy token retains only currently supported classes, never future report-generation permission by default.

- Read allowlist: existing health/search/frame/image/context/marks/session/export reads plus new frame/coverage/report reads. Explicitly classify methods/routes, not “all GETs are safe.”
- Marks on an existing frame and resolve require `marks.write`; `now:true` additionally requires `capture.request`, checked before entering capture. Ask including session Ask requires `answer.generate`. Read credentials get JSON 403 before any side effect.
- Missing/invalid token remains 401 including unknown routes; valid token on unknown route remains 404. Unsupported future routes are not implicitly granted. Token rotation/revocation affects subsequent requests without restart; active streams keep existing behavior, documented rather than falsely claimed cancelled.
- Local Settings can create/reveal/copy/rotate/revoke read token independently, with loading/empty/error/partial/populated states. No tokens in general settings/export/report DTOs/logs. Keep existing token storage mechanism; no credential-store migration.
- Serialize read-token creation/rotation/revocation with the existing API configuration lock;
  persist successfully before swapping live credentials. A persistence failure preserves the
  prior live credential and listener state. Legacy-token rotation leaves the read token unchanged.
- MCP gains explicit `--read-only` mode for memory-agent configs: lists/accepts only read tools. This is ergonomics, **not authorization**; API independently enforces token rights even with normal MCP mode or raw HTTP. Clear 403 guidance, not an API-off error.

### 7f.4 Supported remote MCP client

Extend the existing Rust stdio wrapper, not a new server. Default remains `http://127.0.0.1:43210`; supplying canonical `--api-url` (with existing `--url` retained as a backward-compatible alias) or `SCREENSEARCH_API_URL` explicitly selects the remote destination. Both flag spellings have the same precedence: CLI > environment > default. No second redundant remote-enable flag.

Validate with a URL parser in the shared client construction path as well as CLI parsing: allow HTTP/HTTPS, hostnames and concrete IPv4/IPv6, optional explicit port; reject credentials, query, fragment, wildcard destinations, missing host and non-root path. Strip only a root trailing slash. Document newly rejected URL shapes as compatibility tightening. Preserve `.no_proxy()` for all destinations in this increment; no environment-proxy leakage or proxy-support expansion. HTTPS certificate checks stay enabled.

Set redirect policy to **none**, for same-origin and cross-origin redirects and every client request path (JSON, image and SSE). Return an explicit redirect error without visiting Location or forwarding token. No listener, CORS or inference-sidecar changes.

Add MCP read tools `list_frames`, `get_capture_coverage`, then `list_reports`/`get_report` when their HTTP routes land. Preserve structured page objects/cursors/nulls; never auto-drain history or silently shorten text. Existing tools remain compatible outside `--read-only`.

Supported first client artifacts: Windows x86_64, Linux x86_64, macOS arm64. Publish standalone wrapper archives/checksums through the existing release process **only under separate release authorization**; preserve the Windows NSIS bundled executable and its staging contract. Rust 1.95.0 build instructions offer source builds, but untested architectures are not labeled supported. Do not stub Windows app crates to compile the wrapper. Add package-scoped native client verification, not a cross-platform workspace build.

### 7f.5 Saved reports, not remote generation

`GET /v1/reports?from=&to=&phase=&outcome=&limit=&cursor=` lists runs overlapping `[from,to)` (`window_start < to AND window_end > from`), newest `created_at DESC,run_id DESC`. Initial time bounds required; defaults/limits/errors match frame listing. Cursor freezes window/filter/order/high-water tuple. Phase/outcome are current observations; filtered pagination is not a status-update feed and may change during execution. Clients re-list a window to discover transitions.

`GET /v1/reports/{run_id}` returns a public projection, not serialized `ReportRun` internals:
- run/source/revision IDs; selected dates when present; UTC window/timezone/local date; created/updated times; phase/outcome; allowlisted error code;
- saved Markdown/model and coverage counts/flags; full summarized frame IDs, cited IDs and generation-time session IDs (distinct concepts, no 200-item citation cap substituted for full evidence);
- null result only when no saved result exists; truthful NO_DATA result when stored. **Resolved (§7f.10, Nico GO):** an immutable saved result is exposed immediately whenever it exists, including while delivery is queued or retrying — the delivery phase never hides already saved evidence;
- generation-time provenance preserved even if current frames/sessions are gone. Existing frame/session endpoints resolve references and may return 404. Do not invent availability from old IDs or replace stored session IDs with current segmentation.

List rows omit Markdown/full evidence arrays; detail supplies the complete saved result. No config snapshot, destination snapshot, folder/recovery path, Telegram chat/receipt, credential or arbitrary error message. Missing/pruned run returns 404, never reconstructs from a folder or regenerates. Apply the 90-day terminal-run retention and pending-delivery exceptions in `§8c`; exported notes can outlive API run records. This does not retroactively persist old unsaved Recall reports.

Reads work with schedules off and no loaded answer model; they cannot claim/queue/retry a run, write an index, send a message or repair missing content. No new report SQL schema is needed for read projection; add SQL-side window/keyset filtering, not fetch-100-then-filter.


### 7f.6 Typed wire definitions and validation

The following JSON field names and nullability are normative for the new surfaces. Rust
implementations may factor structs internally but must export equivalent types from `traits`.
UTC timestamps, frame/session IDs and allocator values are signed `i64` JSON integers, never
floating-point timestamps; limits are `u32` with the stricter HTTP range above. UUID identities
are opaque strings. Counts use nonnegative integer types large enough for the stored history.
The F3 increment adds a `kind` field to `ReportSummary`/`ReportDetail`; that addition is
normative in `§8e`.
All optional response fields are present with JSON null; arrays are present, including empty
arrays. Responses must not replace missing values with empty strings or fabricated timestamps.

- `EvidenceWindow { from: i64, to: i64 }`: required, finite integral UTC milliseconds, `from < to`.
- `FramePageRequest`: initial `{ from, to, app?: String, window_title?: String, limit?: u32 }`
  or continuation `{ cursor: String, limit?: u32 }`, mutually exclusive. Reject malformed integer
  syntax, overflow, nonpositive/out-of-range limits, duplicate/ambiguous filter values and invalid
  cursor fields with JSON 400. A supplied filter is a literal string; no SQL-pattern interpretation
  or Unicode casefold beyond SQLite ASCII NOCASE. Preserve literal whitespace; normalization must
  not change the requested matching semantics. The byte bound applies before normalization.
- `FrameEvidence { frame_id: i64, captured_at: i64, monitor_index: u32,
  app_hint: Option<String>, window_title: Option<String>, browser_url: Option<String>,
  raw_text: Option<String>, content_text: Option<String>, text_source: Option<TextSource>,
  filter_version: Option<i32>, text_state: FrameTextState,
  vision_description: Option<String>, session_id: Option<i64>, image_purged: bool }`.
  `FrameTextState` serializes `available|not_available`: available includes stored empty strings;
  not_available means no stored text record, so raw/content/source/version are null. Available
  empty OCR is not missing OCR. App/title use target metadata when nonnull and stored frame
  fallback otherwise, consistently for filtering and returned labels. Browser URL is only the
  stored value. Session is the observed stored reference, not an inference call. Vision is the
  complete stored description or null. No image paths or image bytes occur here.
- `FramePage { items: Vec<FrameEvidence>, next_cursor: Option<String>, history_id: String,
  window: EvidenceWindow, snapshot_max_frame_id: i64, observed_at: i64,
  consistency: "bounded_membership_live_content" }`. Empty database maximum is represented as 0.
  Hydrate rows and text in the same page read, not remote detail calls or load-all filtering.
  A terminal empty page has null next_cursor. A short page alone is not a capture-completeness
  claim. Cursor v1 must validate window/filter/identity/high-water/seek tuple relationships,
  numeric ranges and known version, not just decode successfully. The cursor must not permit
  a seek outside the frozen window or above its maximum. Validate maximum input size before
  decoding; canonical compact encoding must satisfy worst-case filter escaping within 4096 bytes.
- `CaptureCoverage` contains `window: EvidenceWindow`, `observed_at: i64`,
  `capture_enabled: bool` (controller intent only), `frame_id_high_water: i64`,
  `oldest_frame: Option<FrameReference>`, `newest_frame: Option<FrameReference>`,
  `newest_captured_at: Option<i64>`, `frames_stored` and `frames_text_available` in the requested
  window, `image_retention_days: u32`, `retention_scope: "images_only"`,
  `saved_report_retention` and `capture_observations`. A `FrameReference` is `{frame_id, captured_at}`;
  global oldest/newest use chronological `(captured_at,id)` order, not insertion order. Global
  references/timestamp are null when the database is empty. Text-available count includes empty
  stored text, excludes absent text, and cannot exceed stored-frame count. `saved_report_retention`
  states `terminal_days: 90` plus the queued/running/blocked and active-delivery exemptions below;
  it is policy, not a per-run expiry guarantee. Store read failures return an error, never zeros.
- `CaptureObservations { observation_started_at: i64, unavailable_before: i64,
  intervals: Vec<CaptureInterruption>, truncated: bool }`; an interruption is
  `{ start: i64, end: Option<i64>, reason: CaptureInterruptionReason }`.
  Reasons are observed controller stop (`capture_stopped`) or observed worker error (`worker_error`),
  never inferred sleep/lock/exclusion/crash. Capture start closes a known pause. Controller
  start/stop/error observations carry observed timestamps internally; retain at most 1000 such
  transitions. Return at most 100 intervals intersecting the requested half-open window in
  chronological order. Preserve actual observed endpoints (do not relabel query bounds as
  observations); null end denotes an ongoing known interruption. At process startup,
  observation_started_at and unavailable_before begin at process observation start; overflow
  advances unavailable_before. Truncated is true if the requested interval includes unavailable
  observations or if intersecting intervals exceed the response cap. Earlier process history,
  missing observations and overflow are unknown; a current enabled intent does not erase them.
- `ReportPageRequest`: initial `{from, to, phase?, outcome?, limit?}` or `{cursor, limit?}`.
  Phase values are `queued|running|blocked|finished`; outcomes `OK|PARTIAL|NO_DATA|BLOCKED|FAILED`.
  Unknown enum values are JSON 400, not ignored. No inference from absent/null outcomes. The same
  required bounds, default 100, valid 1–1000, mutually exclusive continuation and 4096-byte cursor
  cap apply. SQL filters overlap/status before limit and seeks descending `(created_at,run_id)`.
  The initial maximum ordering tuple and normalized filters/window stay frozen in cursor v1;
  later pages admit no tuple above that maximum and seek strictly below the last tuple. This
  bounds tuples, not a frozen membership snapshot or replication log. Equal-ms lower UUIDs,
  backdated inserts and clock rollback can admit newly created rows below the frozen high tuple;
  status changes alter filtered membership. Re-list the selected window for transitions.
- `ReportSummary { run_id: String, source_id: String, revision_of: Option<String>,
  requested_range: Option<MemoryReportRange>, window_start: i64, window_end: i64,
  timezone: String, local_date: String, created_at: i64, updated_at: i64,
  phase: ReportRunPhase, outcome: Option<ReportOutcome>, error_code: Option<String> }`.
  Error codes are only the §8c allowlist; never expose arbitrary stored error messages or codes.
  Unknown/unallowlisted errors project as null, not private diagnostic text.
- `ReportPage { items: Vec<ReportSummary>, next_cursor: Option<String> }` with explicit terminal
  null. `ReportDetail` contains all ReportSummary fields plus `result: Option<SavedReport>`.
  The §8c SavedReport fields are the complete public saved-result shape: markdown, model,
  frame_ids, session_ids, cited_frame_ids, frames_sampled, frames_summarized, periods_total,
  periods_covered, passes, truncated. Preserve immutable saved values, full distinct summarized
  IDs and generation-time session provenance. **Resolved visibility rule (§7f.10, Nico GO):**
  `result` is `Some` for any valid persisted `SavedReport` regardless of the run's current
  delivery/run phase — expose immutable saved output immediately whenever it exists, including
  while delivery is queued or retrying; the delivery phase never hides already saved evidence.
  The prior unfinished-result-null rule no longer nulls a present saved result: a run with no
  saved result has null result (any phase), not generated replacement prose; stored NO_DATA
  remains its factual saved result; a malformed persisted saved payload is a read error, never
  null. No delivery, path, configuration, Telegram or credential fields enter either projection.

Report retention is **90 days from last terminal update**, not created_at. Never prune
queued/running/blocked runs, runs with pending/sending/unknown/conflict delivery, or failed
delivery with nonnull next_attempt_at. Failed delivery without a pending retry is terminal;
reviewed uncertainty can expire. Exported notes and ownership records outlive run pruning;
missing/pruned API rows remain 404. These reads do not create persistence for old unsaved reports.

New read-route HTTP failures use existing JSON envelope conventions with fixed safe messages:
invalid query/cursor/enum/limit is 400; history identity mismatch is 409 `history_changed`;
missing/pruned detail is 404; readiness/store/JSON-decode failures are explicit non-success
responses, never empty data/NO_DATA. Return 503 for unavailable feed readiness, 500 for storage/
decode failure. Do not serialize/log private anyhow error strings, stored payloads, paths or
credentials on these new read adapters. Saved uppercase error_code is distinct from HTTP errors.
Report reads serve stored error/provenance only, never the in-app current-evidence overlay.

### 7f.7 Explicit route/tool/permission matrix

Inventory checked against `crates/api/src/lib.rs::build_router` and
`crates/mcp/src/tools.rs` at the planning base. Rows tagged new are approved contracts, not
routes/tools observed implemented. L = existing legacy token; R = independently created read
token. `history.read` is granted to both. No class automatically expands to future routes.

| Method + route | MCP tool | Required classes | L | R |
|---|---|---|---|---|
| GET /v1/health | none | history.read | allow | allow |
| GET /v1/search | search_screen_history | history.read | allow | allow |
| GET /v1/frames/{id}, including ?image=1 | get_moment | history.read | allow | allow |
| GET /v1/context/where-was-i | where_was_i | history.read | allow | allow |
| GET /v1/marks | list_marks | history.read | allow | allow |
| GET /v1/sessions | list_sessions | history.read | allow | allow |
| GET /v1/sessions/{id}, including include_summary=1 | get_session | history.read; cached summary only | allow | allow |
| GET /v1/export | none | history.read | allow | allow |
| POST /v1/marks, existing frame | add_mark | marks.write | allow | 403 |
| POST /v1/marks, now:true / existing capture-now shape | add_mark | marks.write AND capture.request | allow | 403 |
| POST /v1/marks/{id}/resolve | none | marks.write | allow | 403 |
| POST /v1/ask, with or without session_id (SSE) | ask_screen_history / ask_session | answer.generate | allow | 403 |
| GET /v1/frames (new) | list_frames (new) | history.read | allow | allow |
| GET /v1/coverage (new) | get_capture_coverage (new) | history.read | allow | allow |
| GET /v1/reports (new) | list_reports (new, P3) | history.read | allow | allow |
| GET /v1/reports/{run_id} (new) | get_report (new, P3) | history.read | allow | allow |
| GET /v1/threads (new) | list_threads (new, P8) | history.read | allow | allow |
| GET /v1/threads/{id} (new) | get_thread (new, P8) | history.read | allow | allow |
| GET /v1/entities (new) | find_connections (new, P8) | history.read | allow | allow |
| GET /v1/insights (new) | list_insights (new, P8) | history.read | allow | allow |
| Unknown route | unknown tool rejected locally | authenticate first; no implicit grant | 404 | 404 |
| Any unsupported future mutation/generation route | not registered | default-deny; reports.generate reserved unsupported | no implicit grant | no implicit grant |

Missing/invalid credentials return JSON 401 even on unknown routes. Valid read credentials on
all registered GET routes and their implicit HEAD variants require `history.read`; HEAD runs
the same authorized handler before body removal, including image reads and search embeddings.
The existing POST-only routes do not gain HEAD support. Valid read credentials on
recognized writes/Ask return JSON 403 before capture/provider/store mutation; body flags cannot
smuggle capture into a read. Keep existing method behavior for unregistered methods; never
route them to a permitted GET handler. Image and SSE paths use the same authentication policy.
MCP --read-only registers and dispatches only the read tools in the matrix; it rejects add_mark,
ask_screen_history and ask_session without entering the client. Normal mode preserves legacy
registry behavior while the HTTP server independently enforces credentials. Unknown tools never
fall through to arbitrary HTTP. Revocation/rotation affects subsequent requests, not a promise
to cancel an already authorized in-flight stream. Settings is the only credential-management UI;
there is no network credential administration endpoint or general scope editor.

The 0.4.5 rows (`list_threads`, `get_thread`, `find_connections`, `list_insights` over the `§7c`
reads) join `history.read` at the same level as every other read — no new class, no implicit grant.
P8 updates `docs/API.md` and `docs/MCP.md` in the same phase, and 0.4.5 D12 permits those wire shapes
to change while the maintainer's own consumers are updated beside them.

### 7f.8 P0 storage/identity inventory and implementation obligations

The settings-KV choice is accepted subject to enforcing the following protections in P1;
this inventory is source inspection, **not proof that protection already exists**:

| Path at planning base | Observed responsibility | Required P1 protection |
|---|---|---|
| kernel/src/capture_loop.rs → Store::insert_frame → store/src/lib.rs → store/src/records.rs::insert_frame | Production timer/event/demanded capture shares the Store insertion seam; SQL currently omits explicit id | Allocate and persist explicit id + high-water atomically in that seam; concurrent inserts serialize, deleted tails never reuse |
| store/src/schema.rs migration frame-copy SQL | Forward migration copies old IDs, not a new capture writer | Preserve IDs; no table rebuild or allocator rewind in this arc |
| harness/src/export.rs and fixture/test SQL | Isolated test/harness inserts, not production capture | Label direct writes unsupported for continuity; production must not acquire a bypass |
| store/src/settings.rs::set_setting / set_settings_batch / delete_settings | Generic unguarded KV upsert/delete seams | Reject generic overwrite/delete of reserved identity/allocator keys; dedicated transactional setup/allocation owns them |
| kernel/src/settings.rs::save_settings / load_settings / retired-key purge | Fixed typed settings list; retired-key deletion is explicit | Exclude reserved keys from public settings/export/import/reset and retire cleanup; reset-to-default typed values cannot lower/remove them |
| kernel/src/settings.rs model-tier/hotkey/session-setting compatibility writes | Narrow named-key migrations, not a reset-all API | Keep named updates disjoint from reserved keys |
| src-tauri/src/local_api.rs config and bearer writes; src-tauri/src/lib.rs set_model_tier; tray.rs toast marker | Named app/API settings writers | Read-token lifecycle cannot touch history identity, allocator, enabled intent or bind address |
| store/src/records.rs filter-version watermark | Direct SQL named settings write | Keep disjoint from reserved keys |
| store/src/memory.rs ensure_report_source_id / reset_report_source_id; src-tauri/src/memory.rs settings/schedule state | Resettable report identity and report KV state | Never reuse/reset api.history_id; report identity reset leaves frame continuity untouched |
| kernel/src/sessions_scheduler.rs backfill marker | Named resumable-session KV state | Keep disjoint from reserved keys |

At this base, source search found no general settings-import or reset-all handler; that absence
is not permission for a later one to copy or clear reserved metadata. `get_settings` returns a
typed public projection; existing /v1/export emits frame content/marks, not settings. Dedicated
history setup must initialize both keys transactionally before serving the feed or accepting
new production inserts, once for a new/first-upgraded history, never as a GET side effect.
If established history identity exists without valid allocator metadata, block feed readiness
instead of recreating from current max. Ordinary settings saves, memory-source reset, retired
key cleanup and API toggles must preserve both reserved keys. Allocation must reject exhausted/
invalid counters explicitly, preserve existing IDs and commit frame + high-water together.
The generic write/delete APIs must be made incapable of removing/lowering them, including batch
calls. A malicious/manual DB edit, old binary, or restore is outside allocator-only continuity;
client reconciliation is mandatory, not an undetectable server guarantee. If settings storage
cannot enforce these invariants, stop for a metadata-table migration proposal before proceeding.

### 7f.9 Acceptance gates and rollback (approved, not yet demonstrated)

P1 must drain and resume more than 1000 nonprivate fixture frames with equal/out-of-order
capture timestamps, with the answer provider unavailable and schedules disabled. Verify exact
bounded membership absent deliberate deletion/rewind/nonallocator writes; preserve full raw and
filtered text, absent versus empty text, late-text re-read, backdated inserts, deletion progress,
terminal cursor null, literal wildcard/Unicode filtering, byte bounds and worst-case cursor
encoding. Verify transactional allocator restart/concurrency/deleted-tail/reset protections,
missing metadata blocked readiness, history mismatch 409, invalid cursor 400, timestamp identity
conflict and decreasing-high-water checkpoint invalidation. The runnable reference consumer
must actually process pages from an isolated API fixture, commit cursor only after work, retain
earlier conflicting facts and perform bounded reconciliation, not consume canned responses.

Permission tests cover every route/tool/method class, image and SSE, independent token lifecycle,
unknown-route 401/404, read denial 403, zero capture/provider/mutation calls on denial and
future-route default deny. Coverage tests cover empty windows, source errors, process restart,
overflow and response caps without inferred inactivity. Read-token UI must be exercised natively,
including save failure, all states, keyboard operation, independent revoke and no API enable/rebind.

P2 verifies 301/302/303/307/308 for JSON/image/SSE, same/cross-origin, with zero destination requests
and no forwarded token; proxy environment ignored; certificate validation retained; invalid URL
shapes rejected in both CLI and shared client. Verify handshake/tool schema, complete page/null
objects, no auto-drain, full large single-frame payload or explicit failure, and 403 guidance.
Native package build/test/archive-unpack/version/handshake/authenticated-read evidence is required
for Windows x86_64, Linux x86_64 and macOS arm64 before calling those artifacts verified. Also
exercise a real non-Windows wrapper → separate Windows app path with read success, invalid-token
401, direct forbidden mutation 403 and continued pagination; loopback fixtures cannot replace
this acceptance. Publishing still needs separate release authorization.

P3 verifies SQL-side overlap/status/keyset filters, ties/revisions, mutable statuses, precise
null/NO_DATA outputs, internal-field exclusion and unchanged stored Markdown/evidence/session
provenance after source deletion/resegmentation. Read a persisted fixture run with model unloaded
and schedules off, asserting no generator/scheduler/file writer/Telegram calls or state mutation.
P4 exercises coverage → page/resume → report → current source detail and ordinary local/API-off
workflows, existing schedules, legacy clients and Windows staging/bundling. No Telegram send or
new report generation is required for this arc's acceptance when a safe saved fixture suffices.

Retain existing bearer/rotation, cached-session-read, export paging, image purge, connection cap/
header timeout/shutdown and loopback-upgrade regressions. Profile increasing nonprivate histories,
late narrow windows and sparse filters; record SQL query plan, latency, peak memory. Use bounded
hydration/keyset seeks, not load-all, OFFSET or per-item remote calls. Establish performance budget
from measurement before optimization; no guessed latency promise. Missing native/remote evidence
is an explicit blocked gate. Stop on unresolved contradictions, snippets replacing full text,
read-triggered inference, mutation by read token, or unenforceable stable identity.

Rollback disables API/revokes only the read token first and preserves local history/reports,
notes and unrelated credentials. Wrapper rollback is independent. Snapshot isolated verification
DB before identity/setup changes; no live-profile downgrade or schema-version edit. Old writers
or backup restores invalidate checkpoints and require bounded reconciliation even if observed
high-water has already caught up; preserve metadata for forward repair. No global exactly-once
or cross-restore uniqueness guarantee, release tag, merge or updater action is implied.

### 7f.10 Resolved P3 saved-output visibility decision and evidence handoff

**Resolved (Nico, GO).** The report inventory found queued/running delivery retries with immutable
saved results in `src-tauri/src/memory.rs` (planning-base lines 613–617 and 786–788), where a run
is re-queued for delivery while `run.result` is preserved. The approved plan's unfinished-result-null
wording conflicted with preserving saved output in those states; Nico resolved the conflict in favor
of mirroring actual saved-result presence rather than phase-based suppression:

- Expose immutable saved output immediately whenever it exists, including while delivery is queued
  or retrying. `ReportDetail.result` is `Some` for any valid persisted `SavedReport` regardless of
  the run's current delivery/run phase; the delivery phase must not hide already saved evidence.
- The prior unfinished-result-null rule no longer nulls a present saved result. Null remains only
  for a run with no saved result. Stored NO_DATA remains its factual saved result; a malformed
  persisted saved payload is a read error, never null.
- Only the allowlisted saved-result projection is exposed. Generation, delivery, retry and their
  side effects remain excluded; no internal config/destination/path/Telegram/credential fields
  enter the projection, and stable provenance and immutable saved Markdown are preserved.

This decision unblocks P3 (P1/P2 were always authorized and are unaffected). Record the resolved
decision in `06_PATCH_PLAN.md`/`07_KNOWN_GAPS.md` by their owning agent; this contract worker does
not own those files. Wire projection field names/nullability, explicit enum validation, unknown
error-code→null and observed chronological coverage definitions above are normalization decisions
to record in `07`, not runtime evidence.

Independent P0 evidence artifacts (profile scratch): `memory-api-reports-inventory.md` and
`memory-api-store-inventory.md`. The latter pins producer/settings anchors and confirms existing
KV storage is feasible without migration only after generic single/batch/delete reserved-key
rejection plus dedicated atomic initialization/allocation. Every production producer shares the
same insertion primitive. Identity-present/allocator-missing blocks feed readiness; never
recreate a lower allocator. These prerequisites are required code, not already implemented.

## 7g. Setup visibility: startup acquisition progress and errors (approved 2026-10-20)

Nico approved this contract on 2026-10-20 ("Plan is approved"), after the 2026-10-19 field
failure where an exhausted anonymous GitHub API quota left inference unavailable with nothing
but a log line. It is additive: `model_download`, `get_readiness`, `readiness_changed` and the
retry/fallback/integrity behavior of `crates/inference/src/download.rs` are unchanged. No schema
migration, no new setting, no new network destination, no new automatic download.

**Operations.** Four independent setup operations, keyed by `SetupOperation`:
`engine` (llama-server acquisition, `ensure_binary`), `embeddings` (fastembed search model
initialization, `init_embeddings`), `vision_model` and `answer_model` (GGUF lane downloads,
mirrored from `model_download`). A change to one operation never overwrites another.

**Phases** (`SetupPhase`): `checking` · `downloading` · `verifying` · `installing` · `preparing`
· `done` · `failed`. Engine: `checking` (local install / release lookup) → `downloading` →
`verifying` (digest, only when a digest is known) → `installing` (extract + publish) → `done`.
Embeddings: `preparing` → `done`, always indeterminate (fastembed exposes no byte counter, so no
percentage is shown). Model lanes: `downloading` → `done` / `failed`. `done` means the operation
completed — never that a model is loaded or a capability is ready; readiness stays authoritative
for that.

**`SetupStep`** = `{ operation, phase, attempt: u32 (1-based, engine download attempt; 1
elsewhere), downloaded_bytes: Option<u64>, total_bytes: Option<u64>, error: Option<String> }`.
Bytes are real received bytes of the current attempt only (a retry restarts at 0; no
cross-attempt or cross-phase aggregate). `total_bytes` is `None` unless the server reported a
length; the UI shows a percentage only when both are known. `error` is set only on `failed` and
is diagnostic text with every URL query string removed (signed CDN parameters never reach the
UI). **`SetupStatus`** = `{ revision: u64, steps: SetupStep[] }`: the current-session state of
every operation that has reported, in `SetupOperation` order. `revision` increases by one on
every change.

**Transport.** The kernel owns the authoritative `SetupStatus`. Every change is applied to it
first and then broadcast as the `setup_changed` event carrying the **whole** snapshot; the
`get_setup_status` command returns the same snapshot. The UI subscribes before fetching and
keeps whichever snapshot has the higher `revision`, so a late mount, a webview reload or a
lagged/lost event reconciles to the current state and an older snapshot never overwrites a
newer one. Engine progress events are rate-limited at the source (at most ~5/s, plus every
phase change). State is per-session memory: nothing is persisted, and a previous session's
activity is never shown as live.

**Failure persistence and recovery copy.** A `failed` step stays until that same operation
reports a new phase (a new attempt), never merely because Settings was visited or another
operation succeeded. Engine acquisition still runs once per launch (`07` #94): its failure copy
says to resolve the cause and restart ScreenSearch; no retry button is offered for it. Model-lane
failures point at the existing action that re-runs that lane's download — "Load answer model" for
the answer lane, "Start vision tagging" for the vision lane (PR #146 review); no new vision retry or automatic Ask replay
is introduced. Offline is never inferred from a generic network error.

**UI** (`UI_REFERENCE §4`/`§5`): a compact StatusRail setup indicator, present only while any
operation is active or failed, opening the Settings · Models setup details; no modal, no toast
for progress, no badge count. Acceptance: Rust unit tests for ordering/revision, independent
operations, failure persistence, URL redaction and engine phase/byte reporting (known and
unknown length, retry reset, digest failure, extraction failure); UI tests for the indicator and
details states; the full CI chain; and a native run of a source-built app on Windows showing
real engine progress, a failure and reload reconciliation, recorded in `05`.

## 7h. Work graph contract (0.4.5)

The work-graph half of the 0.4.5 arc (`02 §5k`; `docs/0.4.5.md`): ScreenSearch stops only recording
work and starts linking it. Everything here is **derived-but-persisted** in the `§7e` sense — the
graph tables are recomputable from frames, text and sessions, and deleting them loses no evidence.
The schema is in `§4`; this section is the behavioural contract behind it. Implemented by **P5**
(`crates/graph` — pure logic over `traits` — plus store queries and kernel jobs); the tables ship
empty in P3's baseline and nothing writes them before P5.

### 7h.1 Entity kinds (0.4.5 D8)

`entities.kind` is a closed set of six: **`repo`**, **`file`**, **`domain`**, **`document`**,
**`ref`** (an issue/PR/ticket reference such as `#123` or `ABC-123`), **`app`**. People are **out of
v1**. A "topic" is not a kind: there is no `topic` value, and a topic-shaped name is stored as an
`app`-or-`document` entity whose display name is carried by `entity_aliases` — it exists only after
normalization, never as a pre-normalization heuristic row. A seventh kind is a schema change, i.e. a
baseline edit plus a `schema_version` bump (0.4.5 D2), not a code-only extension.

### 7h.2 Extraction sources and confidence

Heuristic extraction — deterministic, no model — reads: window titles, browser URLs (domain plus
path shape), filesystem paths, repository names, terminal cwd and prompts, IDE titles, document
titles, and issue/PR references. It consumes `crates/sessions` tool recognition as **input only**
(read-only; the sessions read side is not modified here). Every hit carries `source='heuristic'` and
the confidence of the rule that produced it — the rule's own value, never invented or smoothed.

The optional LLM normalization pass reads session summaries and proposes canonical names and aliases
(`screensearch-v2c` = `ScreenSearch` = `screensearch.app`). It writes `entity_aliases` rows with
`source='llm'` and its own confidence; it never rewrites a heuristic entity's `normalized_key`. **An
LLM pass is additive: heuristic entities never depend on the model**, and the graph is complete and
useful with the model lane disabled.

Dedup identity is `UNIQUE(kind, normalized_key)` where `normalized_key` is lowercase, trimmed and
path/URL-normalized; aliases are unique per `(entity_id, alias)`.

### 7h.3 Linking rule

A hit links to the session whose interval contains it: the hit's timestamp falls inside
`[started_at, ended_at)` (an open session, `ended_at` NULL, is open-ended). The link is one
`session_entities` row per `(session_id, entity_id)` — a session touches an entity at most once.
`hit_count` counts **distinct frames** contributing to the link; the link's `confidence` is the
**maximum** of the contributing hits' confidences, so a session never renders more certain than its
best evidence.

> **Amendment (P5 review fix, `06` #41 revised).** Production linking is **ownership-only**: a hit
> links to `frames.session_id` when that owner row exists, and an unowned frame contributes
> entities but no link. This section's interval rule survives as the documented
> `Store::graph_session_at` primitive and its tests; it is not a linking fallback. `frames.session_id`
> is authoritative (`03 §7e`).


### 7h.4 Thread clustering rule

Sessions cluster into a `threads` row when they share **at least one** entity with
`confidence ≥ 0.7`, and the gap between them — measured between `last_seen_at` values — falls inside
a **decaying window**: full weight at 7 days, decaying **linearly to zero at 30 days**. Beyond 30
days there is no cluster edge.

Clustering is **deterministic for a given input and override set**. `threads.cluster_key` is the
sorted list of seed entity ids that produced the thread and is `UNIQUE`, so re-running the clusterer
over the same data is idempotent: the same sessions land in the same thread under the same key. A
thread whose membership would change is **rebuilt** (rows replaced), never hand-edited.
`threads.title` derives from the dominant seed entity's `canonical_name`; `session_count`,
`first_seen_at` and `last_seen_at` derive from the member sessions. A session belongs to **at most
one** thread (`thread_sessions.session_id` is `UNIQUE`); merge and split rewrite those rows.

### 7h.5 Override precedence

`thread_overrides` rows (`rename` / `merge` / `split`) are applied **after** clustering and **always
win** over the clusterer. Precedence is total: for any session, an override-derived assignment beats
the clustered one, and a later override beats an earlier one of the same kind.

- **rename** sets `threads.title` only; membership is untouched.
- **merge** folds the source thread's `thread_sessions` rows into `target_thread_id` (the survivor),
  then removes the emptied source thread. `payload.session_ids` records the source's member set at
  merge time, for audit and for the undo path; the fold itself follows `thread_sessions`, so the list
  can never contradict the rows.
- **split** moves the listed sessions into a **new** thread seeded by the override (its
  `cluster_key` derives from the override, not from the clusterer), from which clustering continues.

`payload` is typed JSON per kind, validated by the `traits` types (`rename`: `{"title":…}`;
`merge`: `{"session_ids":[…]}`; `split`: `{"thread_id":…,"session_ids":[…]}`), and the table's CHECK
constrains which thread ids each kind may carry. The UI writes overrides through the normal store
API (P9); nothing else inserts them.

### 7h.6 Backfill semantics

Backfill is **one resumable job over the full history**, with progress on the existing setup/job
status surfaces (`§7g`). It is safe to interrupt at any point: each committed chunk leaves a
consistent graph, and a re-run continues from the checkpoint without duplicating rows.

### 7h.7 Job kinds (0.4.5 D11)

Four job kinds — `graph_extract` (heuristic extraction + linking), `graph_normalize` (the LLM alias
pass), `graph_cluster` (threads + overrides), `graph_backfill` (the resumable historical pass) — all
**on demand or on idle**, all under the existing scheduler and pressure throttle (`§5`). **None runs
on the capture hot path:** capture, OCR and storage never wait on a graph job, and a throttled graph
job degrades to "the graph is behind", never to a capture stall. Failure logs and degrades; it never
blocks capture, search or Ask. The job-kind registry gains these four values in **one** edit (P5
owns it; P6's `session_embed` and P8's `insight_generate` are added beside them).

## 7i. Ask v2 contract (0.4.5)

Ask stops being a frame search that answers and becomes a session-aware, multi-turn conversation over
history. Implemented by **P6**. The wire changes are explicitly allowed (0.4.5 D12): `/v1/ask` and
the report projections may change shape, `docs/API.md` / `docs/MCP.md` and the maintainer's own
consumers are updated in the same phase, and no existing field is repurposed to mean something new.

### 7i.1 Planning

Two planning steps, both before retrieval:

- **Time intent** — relative phrasing ("last week", "before the release", "this morning") resolves to
  an explicit half-open window `[window_start, window_end)`. With no time intent the window is the
  whole history. The resolved window is recorded on the turn.
- **Entity resolution** — entity mentions match against `entities.canonical_name` and
  `entity_aliases.alias`; a match constrains retrieval to sessions linked to that entity. With no
  graph row or no match the mention falls back to **plain text** matching, so **P6 does not block on
  P5**: Ask v2 is fully functional on a DB whose graph tables are still empty.

### 7i.2 Retrieval stages

Stage order is normative; a later stage only ever narrows an earlier one's candidate set.

1. **Threads** — threads whose `[first_seen_at, last_seen_at]` overlaps the planned window, or that
   contain an entity-resolved session.
2. **Sessions** — hybrid retrieval over those threads' sessions: FTS on session title/summary
   combined with `session_vectors` cosine similarity (the vec0 table in `§4`, same model and
   dimension as text embeddings, maintained and backfilled by a P6 job kind).
3. **Frames** — the existing hybrid frame search, restricted to frames inside the winning sessions.
4. **Assembly** — context packed to the existing token budget, best evidence first; truncation drops
   the tail, never the citations of what remains.

Each stage's candidate count and the assembled token total are recorded per turn so the P4 harness
can measure them — this section is the contract, the measured numbers live in `05`/`06`; no stage
silently re-orders another.

### 7i.3 Citations

Every answer cites **both** kinds of evidence, in the wire shape
`{"sessions":[session_id…],"frames":[frame_id…]}` on the answer turn: the sessions the answer drew
on and the specific frames. A cited id outside the planned window, or outside the retrieved
sessions, is a defect, not a display problem.

The existing `/v1/ask` **SSE stream is extended, not replaced** (0.4.5 D12): a new event field carries
the session citations, every existing field keeps its name and meaning, and a client that ignores the
new field still renders the answer it renders today. `docs/API.md` is updated in the same phase, and
`ask_screen_history` / `ask_session` surface the session citations too.

### 7i.4 Multi-turn

`ask_threads` / `ask_turns` persist conversations (`§4`). Each turn stores its question, its answer
(NULL while streaming and on failure), the resolved `[window_start, window_end)` and
`resolved_entities` as a JSON array of entity ids. A follow-up **reuses the previous turn's window
and resolved entities** unless the new question overrides them — "and the day before?" keeps the
entity scope while moving the window. Turn order is `ordinal`, unique per thread. Turns are
user-facing conversation state, not evidence: deleting a conversation cascades to its turns and
never touches frames, sessions, the graph or insights.

## 7j. Insights contract (0.4.5)

Insights are typed, evidenced statements about history. Implemented by **P8**; the storage is in
`§4`, and P9 renders them. Three rules frame everything below: the payload is the truth, evidence is
mandatory, and nothing here notifies.

### 7j.1 Insight types (0.4.5 D10)

`insights.kind` is a closed set of five:

| Kind | What it states |
|---|---|
| `thread_activity` | time and session counts per thread over a window, plus when it was last touched |
| `open_loop` | something unresolved — an unresolved mark, an open question from a weekly synthesis, a thread abandoned mid-way |
| `recurrence` | the same error string / document / ticket seen again after a gap (FTS near-duplicate plus entity match) |
| `semantic_neighbour` | sessions similar to a given session on other days, via session vectors |
| `resumption` | a thread picked up after N days, with what it was last doing |

A sixth type is a schema change: baseline edit plus `schema_version` bump (0.4.5 D2).

### 7j.2 Evidence rule

Every `insights` row has **at least one** `insight_evidence` row; an insight without evidence is a
**defect**. SQLite cannot express a deferred cross-table constraint, so the rule is enforced exactly
where it can be:

- The store's insert API writes the insight and its evidence **in one transaction** and returns an
  error when the evidence list is empty — no code path creates an unevidenced insight.
- A test asserts that rejection.
- A sweep query — `SELECT id FROM insights WHERE id NOT IN (SELECT insight_id FROM insight_evidence)`
  — returns zero rows, and runs as a harness invariant.

`insight_evidence.kind` is one of `frame`, `session`, `thread`, `entity`, `mark`, `report`; `ref_id`
is the referenced row's id and `ordinal` orders the evidence for display. Evidence deletes with its
insight.

### 7j.3 Generation triggers (0.4.5 D11)

Generation runs **on demand** and **on idle** only, under the existing scheduler and pressure
throttle (`§5`): never on the capture hot path, never blocking capture, search or Ask. A failing pass
logs and degrades silently — a missing insight is not a UI error state, and stale insights are
regenerated rather than repaired in place.

### 7j.4 Never notifies (0.4.5 D13)

Insights are **pull-based**. Nothing in this contract notifies, badges, counts streaks, scores or
grades; there is no daily prompt, no productivity metric, and no ordering that implies a target.
Insights describe what happened; they do not judge it. (Kept from 0.3.0/0.4.0 — the maintainer can
lift it explicitly in a later arc.)

### 7j.5 Storage and staleness

`insights.payload` is typed JSON and is the **truth** of the row: it renders completely without the
model. `generator='heuristic'` means the payload was produced deterministically;
`generator='llm_phrased'` means an LLM only **restated** a payload that already existed — phrasing is
additive and is never the source of a claim.

`stale=1` marks a row whose window, entities or evidence changed since `generated_at`. Regeneration
**replaces** the row — same kind, new payload and evidence — and never mutates a row in place under a
reader; the replaced evidence rows disappear with the old row. Windows are unix ms and half-open,
`[window_start, window_end)`, matching `§7i`.

## 8. Configuration / settings (keys in `settings`)

Every key below lives in the `settings` KV table; 48 of the 49 `file-only` rows are additionally
read from `advanced.toml` (app data dir, next to `screensearch.db`) at startup — one `[section]` per
key prefix (`[capture]`, `[sidecar]`, …) with the suffix as the field name. `app.tray_toast_done` is
the exception — a settings-KV marker (Where `—`), not an `advanced.toml` key. Thresholds are settings, never hardcoded (`§3b`/`§5`, `04 §4`).

**Settings v2 (0.4.5 D6, D9).** The Settings UI is a **closed set of 26 keys**, exactly the rows
whose Disposition is `UI` below, and nothing else may be added to it; that ceiling is binding (0.4.5
D6). The count rose from 25 to 26 at the 0.4.5 P2 pick (2026-11-04), which added `ui.appearance` so
the four themes are user-selectable instead of one hardcoded theme. Every other key is `file-only`
(displayed read-only under **Advanced** with the `advanced.toml` path) or `deleted`. Classification
is deterministic, not taste: (1) no live consumer after the 0.4.5 changes → `deleted`; (2) live
consumer whose value is engine tuning a person cannot make a better call on weekly → `file-only`;
(3) otherwise → `UI`, and only for a key on the ceiling list. Settings autosave per field with an
undo toast: there is no global Save (0.4.5 D9); destructive actions (data export/wipe, model
removal, thread-override reset) confirm first and never depend on the autosave toast (0.4.5 D9).
**Integrations** hosts `api.*` plus, per `§8c`/`§8d`, the Telegram and
Obsidian folder settings (`memory.*`, owned by those sections and not counted in this table).

| Key | Default | Disposition | Where | Notes |
|---|---|---|---|---|
| `answer.thinking` | `true` | UI | Intelligence | Ask reasoning toggle; off = faster, shallower answers. |
| `api.bind_address` | `127.0.0.1` when missing | UI | Integrations | Any IPv4/IPv6 literal, immediate apply (`§7c`); missing defaults to `127.0.0.1` for both fresh and existing profiles. |
| `api.enabled` | `false` | UI | Integrations | Master switch; the API crate is **not constructed** unless on (D11 of 0.4.0). |
| `api.network_notice_state` | `—` | deleted | — | Superseded notice state; no live consumer, and no acknowledgement command or notice status remains. |
| `api.port` | `43210` | UI | Integrations | Clamp 1024..=65535; a bind failure is **loud + guided-change** (`§7c`). |
| `api.token` | — generated on first enable | UI | Integrations | Bearer token, regenerable in Settings, never blank while enabled; not part of the typed `Settings` struct (`§7c`). **Shares this UI row with `api.read_token`** (maintainer decision 2026-11-03, 0.4.5 D6): the read-only credential is a `settings` KV row (`src-tauri/src/local_api.rs` `API_READ_TOKEN_KEY`) written only through the create / rotate / revoke / reveal commands (`§7f`), never through `get_settings`/`set_settings`; it is shown in the same Integrations credential control and does not count as a separate UI key. |
| `app.close_to_tray` | `true` | UI | General | Closing the main window hides to tray, capture continues; one-time explanatory toast; off = window close quits cleanly (`§7d`). |
| `app.run_at_startup` | `false` | UI | General | Registers/unregisters launch-at-login; an explicit user choice, never silently enabled (D3 of 0.3.2). |
| `app.tray_toast_done` | — set `true` after first show | file-only | — | Internal marker, not part of the typed `Settings` struct: written directly to the `settings` table and read at tray init, never through `get_settings`/`set_settings` or the ts-rs bindings. |
| `capture.diff_threshold` | `0.006` | file-only | advanced.toml | Frame-diff gate sensitivity. |
| `capture.event_debounce_ms` | `500` | file-only | advanced.toml | Clamp 100..=10000. |
| `capture.event_driven_enabled` | `false` | file-only | advanced.toml | Master switch for the opt-in `CaptureTrigger` source (`§5`); off = timer cadence, every frame tagged `Timer`. |
| `capture.event_fallback_interval_ms` | `30000` | file-only | advanced.toml | Clamp 1000..=3600000; a static screen is still sampled at least this often, tagged `Timer`. |
| `capture.event_idle_threshold_ms` | `5000` | file-only | advanced.toml | Clamp 1000..=60000. |
| `capture.event_min_interval_ms` | `1000` | file-only | advanced.toml | Clamp 250..=60000; the rate ceiling. |
| `capture.event_on_click` | `—` | deleted | — | Removed 0.3.0 PR2; no live consumer, named only by the stale-row sweep. |
| `capture.event_on_clipboard` | `—` | deleted | — | Removed 0.3.0 PR2; the clipboard listener was a privacy-optics landmine. |
| `capture.event_on_foreground` | `true` | file-only | advanced.toml | One of the two triggers kept by the 0.3.0 PR2 trim. |
| `capture.event_on_idle` | `false` | file-only | advanced.toml | One of the two triggers kept by the 0.3.0 PR2 trim. |
| `capture.event_on_scroll_stop` | `—` | deleted | — | Removed 0.3.0 PR2; no live consumer. |
| `capture.event_on_typing_pause` | `—` | deleted | — | Removed 0.3.0 PR2; redundant with idle (D1 of 0.3.0). |
| `capture.event_typing_pause_ms` | `—` | deleted | — | Removed 0.3.0 PR2 with the typing-pause trigger. |
| `capture.interval_ms` | `3000` | UI | Capture | Timer capture cadence. |
| `capture.monitors` | `[]` = all | UI | Capture | Monitor selection; empty means every monitor. |
| `capture.uia_latency_budget_ms` | `150` | file-only | advanced.toml | Clamp 20..=2000; soft per-walk budget, a 2× hard timeout guards a wedged worker. |
| `capture.uia_max_nodes` | `4000` | file-only | advanced.toml | Clamp 100..=20000; hard cap on nodes visited per walk, replacing the former hardcoded constant. |
| `capture.uia_max_textpattern_calls` | `64` | file-only | advanced.toml | Clamp 1..=4096; max live `TextPattern` visible-range reads per walk, the one uncacheable cross-process cost. |
| `capture.uia_min_text_chars` | `16` | file-only | advanced.toml | Clamp 0..=10000; below this the read is a thin yield → OCR. |
| `capture.uia_run_on_interactive` | `—` | deleted | — | Retired 0.3.2 PR5, inert since the 0.3.0 PR2 trigger trim removed its only firing triggers (`07` #83); no live consumer. |
| `capture.uia_suppress_during_input_ms` | `500` | file-only | advanced.toml | Clamp 0..=10000; `Timer` frames within this window of the last keyboard/mouse input skip UIA → OCR, `0` disables. The window always applies now that the `uia_run_on_interactive` bypass retired with the knob. |
| `capture.uia_text_enabled` | `true` | file-only | advanced.toml | Default ON; OCR carries any failure/timeout/thin-yield; hot-applies per frame. |
| `capture.uia_view_control_only` | `true` | file-only | advanced.toml | Default ON; control view collapses a Chromium page's per-text-run node explosion, off = raw view, far heavier. |
| `enrich.embed_idle_ttl_secs` | `900` | file-only | advanced.toml | Clamp 0..=86400; idle seconds before the in-process fastembed model is released, reloading on the next embed. `0` = never unload. The holder loads on first embed (or eagerly at startup so a first search is not cold) and is swept every 30 s; `dim`/model-name metadata never load the model. 0.4.5 baseline: app private 1352 MiB with the model resident vs 119 MiB with `enrich.embed_text=false` (a different configuration). An isolated real-model harness verified the holder itself — 1228 MiB loaded → 26 MiB after the sweep → 1234 MiB reloaded, identical 768-dim vector — but the whole-app footprint and reload latency remain unmeasured. The default is minutes because a cold reload costs a visible pause. |
| `enrich.embed_text` | `true` | file-only | advanced.toml | Text-embedding lane toggle. |
| `enrich.image_embeddings` | `—` | deleted | — | Removed 0.3.0 PR4 together with the image lane; no live consumer. |
| `enrich.vision_batch_size` | `20` | file-only | advanced.toml | Clamp 1..=500; max still-untagged frames a timer/idle tick enqueues. |
| `enrich.vision_idle_enabled` | `false` | file-only | advanced.toml | Idle-triggered vision enrichment. |
| `enrich.vision_idle_secs` | `300` | file-only | advanced.toml | Idle dwell required before an idle vision tick. |
| `enrich.vision_timer_enabled` | `false` | UI | Intelligence | Timer-driven vision enrichment. |
| `enrich.vision_timer_interval_ms` | `3600000` | UI | Intelligence | Timer enrichment cadence (one hour). |
| `enrich.worker_concurrency` | `2` | file-only | advanced.toml | Enrichment worker count. |
| `marks.hotkey` | `Ctrl+Alt+M` | UI | General | Mark-this-moment; same loud registration-failure handling as `overlay.hotkey` (D6 of 0.3.2). An unchanged chord is never re-registered by a settings save, and a retry after a failed registration is silent while the chord is unchanged (`07` QA2 D3). |
| `models.answer_tier` | `default` | UI | Models | Each ∈ {`default`,`quality`}; 0.3.0 retired `beta` — a persisted `beta` selection maps to `quality` on load, logged once plus the mapping persisted, and any Beta GGUF already on disk is left alone with no cleanup logic (D3/D4 of 0.3.0). |
| `models.vision_tier` | `default` | UI | Models | Each ∈ {`default`,`quality`}. |
| `overlay.hotkey` | `Ctrl+Alt+Z` | UI | General | Global summon; a failed registration is a **visible Settings warning + toast**, never silent (D6 of 0.3.2). An unchanged chord is never re-registered by a settings save, and a retry after a failed registration is silent while the chord is unchanged (`07` QA2 D3). |
| `overlay.max_results` | `8` | file-only | advanced.toml | Clamp 1..=50; top-N results in the overlay. |
| `privacy.excluded_apps` | `["1Password","KeePass","Bitwarden"]` | UI | Privacy | Skip the frame when the foreground app matches. |
| `privacy.pause_on_lock` | `true` | UI | Privacy | Pause capture while the session is locked. |
| `reports.daily_top_k` | `40` | UI | Intelligence | Daily report frame cap. |
| `reports.map_reduce_min_frames` | `20` | file-only | advanced.toml | Frame count above which a range uses map-reduce (`§8b`); set at the worst-case single-pass fit so frames are batched, not dropped, before the 8192 answer context overflows: ~400 tok/frame × 20 ≈ 8192. |
| `reports.weekly_top_k` | `200` | file-only | advanced.toml | Weekly report frame cap. |
| `resume.min_dwell_secs` | `120` | file-only | advanced.toml | Min sustained-context dwell for where-was-i (`§7b`; D9 of 0.3.0). |
| `retrieval.default_top_k` | `8` | UI | Intelligence | Replaces the hardcoded `ASK_TOP_K`; per-request override allowed. |
| `sessions.gap_close_secs` | `120` | UI | Intelligence | Pass-1 micro gap close, retuned by the 11-day sweep (usage review 2026-08-01 §6.7); clamp 60..=3600. |
| `sessions.min_len_secs` | `120` | UI | Intelligence | Minimum session length; mirrors `resume.min_dwell_secs` (`§7e`); clamp 30..=3600. |
| `sidecar.ctx_size` | `0` = auto | file-only | advanced.toml | Auto → per-lane default vision 4096 / answer 8192, else clamp 512..=32768 — the dominant VRAM lever. |
| `sidecar.flash_attn` | `auto` | file-only | advanced.toml | One of `auto`/`on`/`off`. |
| `sidecar.idle_ttl_secs` | `180` | file-only | advanced.toml | Resolved once at supervisor construction, so changes apply on app restart. |
| `sidecar.kv_cache_type` | `q8_0` | file-only | advanced.toml | One of `f16`/`q8_0`/`q4_0`, quantized only when flash attention is active. |
| `sidecar.ngl` | `99` | file-only | advanced.toml | GPU layers offloaded to the sidecar. |
| `sidecar.device` | `None` = llama.cpp default | file-only | advanced.toml | Optional llama.cpp `--device` selector (e.g. `Vulkan0`); blank or whitespace round-trips as `None`. Today a typed `Settings` field in the `settings` KV (`crates/kernel/src/settings.rs`); P3 moves it into `advanced.toml` with the rest of the `file-only` keys. |
| `sidecar.recycle_enabled` | `true` | file-only | advanced.toml | Recycle the sidecar process when committed host RAM crosses the ceiling; mitigates the upstream llama.cpp multimodal memory leak (`07` #72). |
| `sidecar.recycle_rss_mb` | `0` = auto | file-only | advanced.toml | Auto → ceiling derived from total installed RAM, else clamp 8192..=131072 — the committed-RAM ceiling in MiB at which the sidecar is recycled. The 8 GiB floor matches the auto-ceiling minimum so an explicit value clears the vision model's ~6.8 GB warmup baseline and cannot trigger a recycle loop; 0 is recommended for most users. Resolved once at supervisor construction like `sidecar.idle_ttl_secs`, so changes apply on app restart. |
| `storage.jpeg_quality` | `—` | deleted | — | Retired 0.3.2 PR5, provably inert since the lossless-WebP storage path (its own UI hint said it "has no effect today"); no live consumer. |
| `storage.max_width` | `1280` | UI | Data | Stored frame width cap; OCR runs on the full-res frame before this resize. |
| `storage.retention_days` | `0` = keep | UI | Data | `0` keeps everything; any other value prunes older frames. |
| `text.chrome_protect_min_chars` | `48` | file-only | advanced.toml | Lines longer than this are never suppressed for repeating. |
| `text.chrome_region_buckets` | `8` | file-only | advanced.toml | Grid resolution for `region_bucket`. |
| `text.chrome_suppress_min_seen` | `12` | file-only | advanced.toml | Appearances before a signature is marked chrome. |
| `text.include_chrome_default` | `false` | UI | Intelligence | Default search uses `content_text`; on = chrome text included. |
| `throttle.commit_enter_pct` | `90.0` | file-only | advanced.toml | Clamp 1..=100; system commit usage% (`committed/limit` from `GetPerformanceInfo`) at which pressure begins. Catches a process that committed far more than it is actively using: committed pages charge whether or not they are resident, while plain reservation charges nothing. |
| `throttle.commit_exit_pct` | `80.0` | file-only | advanced.toml | Clamp 0..=enter−1; hysteresis, must sit below `commit_enter_pct`. |
| `throttle.cpu_enter_pct` | `85.0` | file-only | advanced.toml | Clamp 1..=100; busy% at which CPU pressure begins. |
| `throttle.cpu_exit_pct` | `65.0` | file-only | advanced.toml | Clamp 0..=enter−1; hysteresis, must sit below `cpu_enter_pct`. |
| `throttle.embed_text_floor` | `1` | file-only | advanced.toml | Clamp 1..=16; min concurrent `embed_text` workers at level 2. |
| `throttle.enabled` | `false` | UI | Advanced | Master switch; throttle is opt-in and OFF by default. The twelve threshold rows are `file-only`. |
| `throttle.enter_after_ms` | `5000` | file-only | advanced.toml | Clamp 500..=120000; sustained-enter dwell before stepping up a level. |
| `throttle.exit_after_ms` | `8000` | file-only | advanced.toml | Clamp 500..=300000; recovered-exit dwell before stepping down a level. |
| `throttle.gpu_enter_pct` | `90.0` | file-only | advanced.toml | Clamp 1..=100; unused when GPU is unmonitored (`§5`). |
| `throttle.gpu_exit_pct` | `70.0` | file-only | advanced.toml | Clamp 0..=enter−1; hysteresis, must sit below `gpu_enter_pct`. |
| `throttle.mem_enter_pct` | `90.0` | file-only | advanced.toml | Clamp 1..=100; physical-memory usage% (`1 − available/total` from `GlobalMemoryStatusEx`) at which pressure begins. 90/80 sits above this app's own measured vision peak on the maintainer's box (77 % physical, 67 % commit), so the default does not throttle its own vision batches there; a host whose own workload does cross 90 % is throttled by design, which is why this is a file-only setting rather than a fixed promise. |
| `throttle.mem_exit_pct` | `80.0` | file-only | advanced.toml | Clamp 0..=enter−1; hysteresis, must sit below `mem_enter_pct`. |
| `throttle.sample_interval_ms` | `1000` | file-only | advanced.toml | Clamp 250..=10000; governor probe/sample cadence. |
| `ui.appearance` | `pine-light` | UI | General | Enum `pine-light` / `pine-dark` / `pine-system` / `ink-light` / `ink-dark` / `ink-system`; `*-system` follows the OS light/dark preference within the chosen accent. Consumer: the shell theme applier (sets `data-theme` on the root element) plus Settings > General. P7 renders the select and resolves `*-system` through `prefers-color-scheme`; P3 adds it to the settings v2 registry and baseline (0.4.5 P2 pick, 2026-11-04; `UI_REFERENCE §1`/`§2`). |

**Load rule (0.4.5 D6): retired keys are removed on rewrite.** A `settings` row or `advanced.toml`
entry naming a key that is not in the table is rewritten once with that key removed; no error, no
migration, no field kept parsed-but-unused. `advanced.toml` is strict in the same way: an unknown
section or field is dropped on that rewrite, so the file always matches the table.

**Deleted rows (evidence).** The nine `deleted` keys are proven inert: grep across `crates/` and
`ui/src/` finds no live consumer, and the current code names them only inside the stale-row sweep that
exists to drop them. `api.network_notice_state` has zero references anywhere. No replacement key is
promoted into `UI` to refill a retired slot — the ceiling is a maximum, not a target (0.4.5 D6).

**Event-driven capture** (opt-in `CaptureTrigger` source, `§5`/`07` #47): 0.3.0 (PR2) trimmed the six
triggers to foreground + idle — click / scroll-stop were the **only** consumers of the global
`WH_MOUSE_LL` mouse hook (deleted with them), the clipboard listener was a privacy-optics landmine, and
typing-pause was redundant with idle (D1 of 0.3.0). Those five removed keys (four triggers plus the
typing-pause threshold) are `deleted` rows in the table above. **No schema change** (0.4.5 D2): `frames.capture_trigger`
keeps its widened CHECK, so legacy `clipboard`/`typing_pause`/`click`/`scroll_stop` tokens stay readable
(the Moment "Captured via" row still renders them); new frames simply never emit them again.

`ApiStatus.bind_address: String` reports the canonical configured IP literal. The optional
`set_api_config.bind_address` preserves the current value when omitted; supplied values are
validated before mutation.

**API bind compatibility.** Missing `api.bind_address` defaults to `127.0.0.1` for both
fresh and existing profiles. Preserve enabled state, token and port. The superseded
`api.network_notice_state` key is retired: a config that still carries it is rewritten once without it
(0.4.5 D6); no acknowledgement command or notice status remains.
This preference uses the existing settings KV table and needs no schema migration.

Startup completion emits `api_status_changed` with a null/unit payload so the main UI
invalidates its status query even if the first read preceded startup binding. The event never
carries a token. Event subscription also refreshes status after registration to close the
subscribe-versus-startup race. Startup must read persisted enabled intent fallibly before
mutating configuration; a failed read must not silently persist a disabled preference.

There is also an **internal marker** `app.tray_toast_done` (not part of the typed `Settings` struct;
same pattern as `api.token`, `§7c`): set to `true` the first time the one-time close-to-tray toast is
shown so it never repeats. It is written directly to the `settings` table and read at tray init; it
never rides through `get_settings`/`set_settings` or the ts-rs bindings.

Capture honors `privacy.excluded_apps` (skip frame if foreground app matches) and
`privacy.pause_on_lock`; the **self-exclude capture gate also covers the 0.3.0 overlay window** (D7 —
the app's own overlay must never appear in its own capture history; asserted by the privacy-gate tests,
`§7b`). OCR runs on the **full-res** frame before resize/storage.

## 8b. Recall reports (0.2.x)

`generate_report(ReportRequest) -> ReportResponse` (`§7`) summarizes work/content over a time range
from **`frame_text.content_text`** (never raw full-screen text) and **cites the frames** it used.
The original 0.2.x command remains supported. The approved memory-report increment adds durable
runs and opt-in local schedules under `§8c`; the original no-saved-table rule is superseded only
for that increment.

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

## 8c. Durable reports and local daily memory (approved 2026-10-15)

The maintainer approved execution and the report policies. This separately reviewed amendment
permits default-off scheduled **local** reports and user-selected folder writes, an explicit
exception to earlier pull-only behavior. No scoring, nags, cloud inference, automatic model
downloads. Saved-output HTTP/MCP reads are promoted by the approved `§7f` contract; remote
generation/retry/delivery remain excluded. Telegram's outbound exception is the separately reviewed `§8d` amendment.

### Shared execution and lifecycle

Kernel orchestration owns one report execution service over `Store`/`AnswerProvider`; Tauri and
scheduling adapt it, preserving existing manual Recall and session evidence scope. Scheduling
serializes automatic inference through the existing model supervisor. An automatic provider
must resolve and use installed model files without calling the downloader: a preflight check
followed by the existing lazy-download provider is insufficient (tier/file races). Snapshot
configuration and selected model identity before execution; failure to load is visible BLOCKED.
Inspect empty windows before acquiring a model. Store errors are FAILED, never NO_DATA.
After a positive usable-content probe, coverage generation returning empty or blank prose is
GENERATION_FAILED, never a successful empty note/NO_DATA. Prompt-based manual Recall may still
return no-match output for a query with no relevant hits; blank model prose remains a failure.

Shutdown stops enqueue, sets cancellation, joins the report worker before sidecar shutdown and
preserves recoverable durable state. Existing model-pass cancellation remains cooperative;
the worker may abort awaiting work within a bounded shutdown timeout, then the supervisor kills
the sidecar. Never claim immediate in-flight report cancellation. Capture/OCR never waits on
the schedule. Kernel/runtime implementation chooses the bounded timeout and tests it explicitly.

### Typed contracts (traits; generated IPC types)

UUIDs are opaque strings; timestamps are UTC Unix milliseconds (`i64`); counts are `u32`.
Local dates are `YYYY-MM-DD`; zones are validated IANA identifiers, never an implicit moving
system-local zone. Optional values serialize as null, not invented placeholders.

- `ReportError { code: String, message: String }`: codes `INVALID_CONFIG`, `NOT_FOUND`,
  `BUSY`, `SOURCE_UNAVAILABLE`, `MODEL_UNAVAILABLE`, `STORE_ERROR`, `GENERATION_FAILED`,
  `FILE_CONFLICT`, `WRITE_FAILED`, `CREDENTIAL_UNAVAILABLE`, `DELIVERY_FAILED` identify failures
  without private content or credentials. IPC returns structured errors; existing legacy report
  commands may retain their string-error adapter. The original report increment added no HTTP
error surface; the later saved-read-only surface uses the `§7f` HTTP contract.
- `ReportScheduleConfig { enabled: bool, local_time: Option<String>, timezone: Option<String>,
  output_folder: Option<String> }`. Defaults false/null/null/null. Local time is strict `HH:mm` (hour/minute precision only; no seconds input);
  output folder is absolute. Enable requires valid time/zone/folder; save failure preserves the
  previous configuration. Manual Run now requires selected zone/folder but not an enabled schedule.
- `SavedReport { markdown: String, model: Option<String>, frame_ids: Vec<i64>,
  session_ids: Vec<i64>, cited_frame_ids: Vec<i64>, frames_sampled: u32,
  frames_summarized: u32, periods_total: u32, periods_covered: u32, passes: u32,
  truncated: bool }`. Frame IDs are the full distinct actually summarized union; citation IDs
  may keep the existing 200-item display cap. Session IDs snapshot known ownership at generation.
- `ReportRun { run_id: String, source_id: String, schedule_id: Option<String>,
  revision_of: Option<String>, window_start: i64, window_end: i64, timezone: String,
  local_date: String, phase: ReportRunPhase, outcome: Option<ReportOutcome>,
  error: Option<ReportError>, result: Option<SavedReport>, config_snapshot: String,
  created_at: i64, updated_at: i64 }`. `config_snapshot` is immutable nonsecret JSON containing
  report settings, model identity/version and configuration version; it is provenance, not a
  deduplication dimension. `ReportRunPhase` serializes `queued|running|blocked|finished`;
  outcomes serialize `OK|PARTIAL|NO_DATA|BLOCKED|FAILED`.
- `ReportDelivery { id: String, run_id: String, destination: ReportDestination,
  state: ReportDeliveryState, revision_of: Option<String>, destination_snapshot: String,
  path: Option<String>, hash: Option<String>, recovery_path: Option<String>, chunk_index: Option<u32>,
  chunk_count: Option<u32>, message_id: Option<i64>, error: Option<ReportError>,
  next_attempt_at: Option<i64>, attempt: u32, created_at: i64, updated_at: i64 }`.
  Destination is `vault|index|telegram`; Telegram is governed by the separate `§8d` amendment. State is `pending|sending|succeeded|conflict|failed|skipped|unknown_delivery|reviewed`.
  Delivery updates use expected-state compare-and-swap. Each attempt has its own ID; `revision_of` links a prior attempt. Destination snapshots are
  immutable and never contain tokens. Receipts are separate from generation.
- `MemorySettings { enabled: bool, local_time: Option<String>, timezone: Option<String>,
  output_folder: Option<String>, telegram_enabled: bool, telegram_chat_id: Option<String> }`
  (flat public fields; Telegram fields added by the separate §8d amendment) and
  `MemoryStatus { settings: MemorySettings, source_id: String, latest_run: Option<ReportRun>,
  missed_windows: Vec<TimeRange>, worker_running: bool, worker_error: Option<ReportError>,
  telegram_credential_available: bool }`.
  Worker errors remain visible alongside settings so a failed background attempt cannot hide
  the controls needed to disable or fix automation. No credential value is returned.

`MemorySchedulePreview { local_date: String, window_start: i64, window_end: i64,
due_at: i64 }` describes the first eligible scheduled window and firing time. The read-only
`preview_memory_schedule(settings) -> Option<MemorySchedulePreview>` computes it from the shared
calendar policy before saving; incomplete time/zone returns null, invalid supplied values return
INVALID_CONFIG. It never enqueues, writes settings or requires a model. UI never duplicates DST
policy in JavaScript.

IPC: `get_memory_status() -> MemoryStatus`; `set_memory_settings(settings) -> MemoryStatus`;
`run_memory_report(range?: MemoryReportRange) -> ReportRun` enqueues the selected dates or,
when omitted, the previous completed calendar day in the selected zone, even before automatic activation; `retry_memory_report(run_id) -> ReportRun`;
`regenerate_memory_report(run_id) -> ReportRun`; `list_memory_runs(limit) -> Vec<ReportRun>`
(newest first, default 20, clamp 1–100);
`get_memory_run(run_id) -> ReportRun`; `get_memory_deliveries(run_id) -> Vec<ReportDelivery>`;
`reset_memory_source_identity() -> MemoryStatus` requires schedules/delivery disabled and no
active run. `memory_status_changed: null` invalidates mounted status/run queries; it contains
no report text or tokens and produces no notification.

### Durable state and transitions

Current schema inventory is 11. The next migration, **12**, adds report runs, destination
attempts, scheduling activation/cursor/gap state and generated-file ownership. Forward-only;
populated-schema-11 migration tests must preserve existing frames/sessions/settings. Exact DDL
belongs in the store module; these invariants are binding:

- Base scheduled runs have a partial unique key `(schedule_id, window_start, window_end)` where
  `revision_of IS NULL`; regeneration creates a linked child, not another base run. Persist idempotent enqueue before advancing the schedule cursor; replay after a crash reuses
  the unique base run. Internal `ReportScheduleState { schedule_id: String, activated_at: i64,
  last_window_end: Option<i64>, gap_start: Option<i64>, gap_end: Option<i64> }` is separate KV JSON. Concurrent manual previous-day requests reuse a pending
  matching manual base run; deliberate regeneration is explicit.
- New runs: queued/null → running/null → finished/OK, PARTIAL, NO_DATA or FAILED; a missing
  prerequisite yields blocked/BLOCKED. Retry clears the failure and requeues the **same run**
  when no result exists. Saved results remain immutable: retry resumes pending/failed delivery
  without another generation. A failed vault write never discards a successful saved report.
- Restart changes interrupted running generation to queued, preserving identity/config/window.
  Interrupted file writes reconcile durable file/hash intent before retry; the old valid file
  is preserved. Delivery recovery never invents success from generation success.
- Regenerate creates a new UUID with `revision_of` pointing to the prior run and a fresh config
  snapshot, preserving the original window/source and prior result. Replace the daily note only
  if unchanged and owned; no automatic repeat of a confirmed Telegram delivery in the revision
  chain (enforced by the later adapter). Already-active same-run actions return BUSY.
- Run history expires after 90 days from last terminal update. Never prune queued/running/blocked
  runs or runs with active pending/sending/unknown/conflict delivery or failed delivery with
  nonnull next_attempt_at. Failed delivery without a pending retry is terminal and stays visible. Pruning does not delete
  exported notes. Ownership records `(path, source_id, run_id, last_hash)` outlive run pruning;
  they have no cascading run FK. Evidence IDs have no cascading frame/session FK.

### Calendar schedule and profile identity

First enable records activation now. The first eligible window is the first **full calendar
day starting at or after activation**; no partial activation day or earlier history is scheduled.
Fire at the selected local time on the following day, reporting `[day_start,next_day_start)` in
UTC. DST gap: first valid instant after the requested wall time; overlap: earlier instant once.
Persist the selected zone: machine timezone changes do not move it. Config time/zone edits and
re-enable create a future activation period; retain old UTC windows/results and do not backfill
disabled periods. Output-folder changes affect future requests, not saved destination snapshots.

On restart process eligible missed windows oldest-first, at most seven daily windows; persist
and expose older omitted gaps so later ticks do not silently catch them up or claim coverage.
No duplicate finalized UTC window for the same schedule. Activation/config changes never mutate
old deduplication keys. Settings previews the first eligible report date before enabling.

Create and persist a random profile UUID once. Moves/restores preserve it; it contains no user,
host or path. Independently active clones must explicitly reset identity before enabling their
schedule/delivery; document that byte copies retain identity and cannot be detected reliably.
Reset affects future runs only and requires disabled schedules/delivery with no active worker.
It does not transfer ownership of prior notes or rewrite historical provenance.

### Local output and honest coverage

OK and PARTIAL write saved content; PARTIAL prominently explains incomplete coverage. NO_DATA
writes a factual no-data note without a model. BLOCKED/FAILED write no daily note and remain
visible in app status. Empty evidence is only a successful inspection; sampling never implies
exhaustive capture. Missing later evidence is disclosed, not silently treated as still available.

Path: `<selected folder>/ScreenSearch/Daily/YYYY-MM-DD.md`; index:
`<selected folder>/ScreenSearch/Index.md`. Atomic same-directory temporary write/replace with
persisted SHA-256 ownership independent of run retention. Precheck edited/unrelated files
and preserve them untouched. Use Windows ReplaceFileW with a unique backup for replacement;
compare the displaced backup hash with the expected ownership hash. On a concurrent swap,
preserve displaced bytes in the recovery backup, expose its recovery_path in a conflict receipt,
and never delete an unmatched backup. Reject symlinks/reparse points within the generated subtree. Index follows the same rules and links only successfully written notes,
without duplicates, with date/status/brief source-backed topic labels. No tokens or invented
desktop deep links. Rollback removes only run-owned temporary files, never user content.

Frontmatter schema 1 keys: `schema` integer 1; `run_id`, `source_id`, `timezone`, `status`
strings; `window_start`, `window_end` RFC3339 UTC strings; `model` nullable string;
`frames_sampled`, `frames_summarized`, `periods_total`, `periods_covered` integers;
`truncated` boolean; `frame_ids`, `session_ids` integer arrays. Full frame-ID count equals
frames_summarized; NO_DATA uses empty arrays and null model. Encode scalar strings as JSON-compatible YAML scalars; typed roundtrip tests required.
Body headings cover activity, explicitly evidenced decisions, open questions and next steps;
tentative screen text is never promoted into a commitment. Existing manual report prose stays
compatible; the daily-memory output framing uses the actual dated window, never hardcoded today.

### Settings, upgrade and acceptance

Persist `memory.schedule.enabled=false`, `memory.schedule.local_time=null`,
`memory.schedule.timezone=null`, `memory.schedule.output_folder=null` with internal profile UUID,
activation/cursor/gaps in durable storage. Existing installations remain off; do not infer a
folder or timezone. Model/config provenance is snapshotted per run. Native acceptance requires
a real installed-model report, inspected coverage, a real note visible in the selected folder/
Obsidian, preserved edited notes, DST/restart tests and bounded worker shutdown evidence.
Unit fixtures do not certify live inference or output usefulness. No release/merge is implied.

### Manual date and date-range reports (requested 2026-10-15)

The maintainer requested choosing a date or range for the same durable local/Telegram report.
`MemoryReportRange { start_date: String, end_date: String }` is a generated typed request of
inclusive, strict `YYYY-MM-DD` dates interpreted in the saved Memory timezone. Equal endpoints
select a single day. `run_memory_report(range: Option<MemoryReportRange>)` retains the previous
completed-day shortcut when omitted. Selecting dates alone never enqueues or sends anything.
Validate both dates and their order before enqueue; reject future dates in the selected timezone.
Resolve local midnight with the existing DST/calendar rules; the exclusive UTC end is midnight
after the selected inclusive end date. If that date is today, freeze the end at enqueue time.
Expose that this is an in-progress report: real-evidence output is PARTIAL with its exact as-of
time, never a claim to cover the rest of today. Genuine empty evidence remains NO_DATA and does
not send Telegram. Retry and Regenerate preserve the frozen original window.

`ReportRun.requested_range: Option<MemoryReportRange>` defaults to null when reading legacy JSON
and is immutable. Scheduled/legacy runs retain null and their current behavior. This adds an
optional backward-compatible field to existing JSON payloads, with no SQL DDL change or schema
version bump (schema remains 12). Calendar-grid coverage and existing bounded inference apply to
longer windows; neither screenshots nor raw OCR enter the Telegram digest.

Explicit manual requests have independent run UUIDs, including repeated selections of the same
dates. Store active-window deduplication remains only for legacy previous-day requests and must
not reuse an explicit range run. The source/UTC-window/requested dates cannot mutate on update.
No change to daily scheduling activation, its catch-up cursor or configured send time.

An explicit selection writes `ScreenSearch/Reports/<start>_to_<end>_<base-run-UUID>.md` under the
saved output folder, including for a single date. Regenerated revisions reuse the root ancestor's
file identity. This avoids overwriting scheduled Daily notes or unrelated manual runs. Existing
ownership/hash/reparse/recovery rules apply. Frontmatter adds optional `requested_range` and
`base_run_id` with defaults for old notes, so generated filenames and index provenance remain
verifiable after run-history pruning. The index includes owned Daily and Reports notes with
single-date/range labels. Telegram digests and run history label the selected date range and
in-progress/partial coverage, using the same saved result and independent receipts.

Verify strict dates/order/future rejection, single-day and multi-day UTC bounds, today cutoff,
DST transitions, legacy JSON compatibility, immutable request metadata, distinct manual runs,
revision filename reuse, index links and existing scheduled behavior. No test changes the live
profile, rewrites the reported notes, restarts the running app or sends a real Telegram message.

## 8d. Telegram text digest (separate approved exception, 2026-10-15)

This amendment permits **explicitly enabled** outbound HTTPS calls to Telegram's Bot API for
text-only digests of saved reports to the user-configured destination. It is a narrow exception
to the no-cloud-upload/network rules and to pull-only delivery; storage and inference remain
local. Opt-in copy states that selected digest content leaves the device and reaches Telegram
and the selected conversation. No screenshots, raw OCR, arbitrary export, cloud inference,
two-way commands, inferred recipient or telemetry. Disable stops new sends; an already accepted
message cannot be recalled by disabling. Existing profiles upgrade with Telegram off.

### Settings and credentials

Flat `MemorySettings` extends §8c with `telegram_enabled: bool` (default false) and
`telegram_chat_id: Option<String>` (default null). Persist nonsecret configuration under
`memory.telegram.enabled` and `memory.telegram.chat_id`. Enable requires a nonempty explicitly
selected chat ID and available credential. Destination snapshots capture that chat ID for each
delivery; changing settings does not redirect old attempts. Disable suppresses dispatch of
pending attempts until re-enabled; do not send to a changed recipient without a new explicit
attempt targeting that recipient.

Store the bot token only in Windows Credential Manager, scoped to the profile identity. It
never enters ordinary settings, IPC responses, status/events, prompts, exports, notes or logs.
`set_telegram_token(token) -> MemoryStatus` and `clear_telegram_token() -> MemoryStatus` are
write-only credential actions; `telegram_credential_available` reports only availability.
Credential access failure is visible CREDENTIAL_UNAVAILABLE, never a successful save/send.
Identity reset does not copy credentials into the new identity or erase historical provenance;
the user must explicitly configure the new profile credential. Existing API-token storage is
unchanged. Redact Bot API request URLs, including token-bearing path segments, from all errors.

### Digest and receipts

Derive a concise digest from `SavedReport.markdown` and its window/status/coverage; never invoke
a second model. OK sends; PARTIAL sends with a prominent limitation. NO_DATA/BLOCKED/FAILED do
not send; persist skipped delivery with the precise reason. Mask supported patterns for API
keys/bearer tokens/password assignments before output. State that masking is not comprehensive
PII detection; existing capture exclusions remain relevant. Do not claim arbitrary secrets are
removed. Send plain text without parse-mode markup; bound digest length before splitting.
Each chunk must fit Telegram's 4096-character limit, including labels; use conservative UTF-16
unit accounting and never split a Unicode scalar. Preserve the saved original report locally.

Persist a separate delivery intent **before** the network call, with stable attempt ID, run,
destination snapshot, chunk index/count, and pending→sending compare-and-swap. Process chunks
in order; persist each returned `message_id` immediately as succeeded before the next chunk.
Retries skip confirmed chunks. Store the rendered chunk text or immutable digest snapshot with
the intent so retries use the same saved content; no credentials in that payload.

Explicit rejection known not to have accepted a message may retry only when transient. Bound
automatic retries to **three after the first attempt**, with delays 30 s, 120 s, 600 s; honor a
larger valid Telegram `retry_after`. Authentication/invalid-destination errors are permanent and
visible. An HTTP 429 structured rejection is transient; transport timeout/disconnect after
dispatch, malformed acceptance or a crash after sending but before durable receipt is
UNKNOWN_DELIVERY. Ambiguous errors are never classified as explicit transient rejection.
On startup, Telegram attempts left sending become unknown_delivery, never pending.

`resolve_telegram_delivery(attempt_id, resolution) -> ReportRun`, with resolution
`reviewed|resend`, requires an explicit user action. Reviewed changes the original attempt to
reviewed while retaining uncertainty/error and any known receipts; it never invents a message
ID or changes it to succeeded. Resend atomically marks the original reviewed and creates a new
linked attempt (`revision_of`) after a duplicate-risk explanation. If the compare-and-swap
loses, do not enqueue another resend. Unresolved unknown attempts block pruning; resolved
reviewed history can expire under §8c. Confirmed delivery is never repeated automatically,
including for regenerated revisions. A regenerated report records skipped/already-confirmed if
any ancestor was confirmed; unknown ancestors require review before any new send. Vault/index
failure never triggers report regeneration and does not erase Telegram acceptance receipts.

### Acceptance

Test credential unavailability, token exclusion from prompts/errors/settings, Unicode chunk
bounds, saved-report reuse, retry bounds, unknown recovery, concurrent resolution, confirmed
chunk suppression, regeneration suppression and disabled dispatch. Real acceptance additionally
requires a user-approved recipient/send and visual inspection in Telegram. A returned message
ID proves service acceptance, never that a person read the message. No test credentials,
hardcoded destinations, synthetic receipts or undocumented sends. Rollback disables delivery
independently of local schedules and preserves reports, notes and receipts.

## 8e. Deeper memory: weekly synthesis and project note (approved 2026-10-17)

The maintainer approved this deeper-memory contract on 2026-10-17 (`02 §5i`), explicitly
waiving the direction plan's F1-pilot contingency. This separately reviewed amendment extends
the `§8c` exception with exactly two layers above the saved daily report — one scheduled
weekly synthesis and one user-created project note — plus the single `kind` schema addition
they require; the weekly Telegram digest (G6) rides the existing `§8d` exception. Loop: saved
daily reports → weekly synthesis citing them → explicit user-confirmed promotion → the
project note. No new capture, no scoring, nags or unsolicited automation, no cloud inference,
no automatic model downloads, no new outbound exception. Approval is contract-level: this
section authorizes no code, and each increment G1–G6 requires real per-unit verification,
independent review and separate merge authorization.

### Weekly synthesis runs

- Kind token `weekly_synthesis`, executed through the shared report application service (C1) —
  no second engine, no second model path. It is deliberately distinct from the on-demand
  `ReportKind::Weekly` (`crates/traits/src/ipc.rs:158`, `§8b`: trailing 7 local days,
  frame-based map-reduce), which keeps its name, label ("Weekly",
  `ui/src/components/domain/ReportBuilder.tsx:62`) and `screensearch-report-weekly-…`
  filenames (`ui/src/lib/time.ts:86`). The on-demand report and the scheduled weekly synthesis
  never share a token, label or filename stem.
- Run identity: schedule ID + explicit UTC window `[week_start, week_end)`. The window is the
  ISO week (Monday 00:00 local → next Monday 00:00 local) in the selected IANA timezone;
  DST-shortened/lengthened weeks produce 167/169-hour windows, never 24·7 UTC hours.
- Uniqueness is `(schedule_id, window)`; retries reuse the run; Regenerate creates a linked
  revision, mirroring `§8c`. `kind` is not a deduplication dimension.
- Truthful statuses reuse C4: `NO_DATA` for a week with no saved dailies and no evidence;
  `PARTIAL` when any day in the window lacks a saved daily report or its coverage is marked
  partial/truncated; `BLOCKED` when prerequisites (model) are missing; `FAILED` on execution
  errors; `OK` only with complete measured coverage of the window. Shared `§8c` execution
  rules apply unchanged (inspect empty windows before acquiring a model, store errors are
  FAILED, blank prose after a positive probe is GENERATION_FAILED, model-load failure is
  visible BLOCKED).
- Inputs: the saved daily reports in the window, their stored evidence IDs and coverage
  records. A weekly run may re-read evidence but must record the `daily_run_ids` it consumed
  in its provenance; a weekly report citing no daily run for a day it describes is a defect.

### Persistence, kind and retention

- `kind` is an indexed `report_runs` column (`kind TEXT NOT NULL DEFAULT 'daily'`), added by
  the increment's single forward-only 12 → 13 migration with the `schema_version` bump
  (`crates/store/src/schema.rs:10`, `:456-469`); run listings filter on it. Existing runs read
  as `daily`; new runs persist their kind explicitly at enqueue. The existing partial unique
  key (`crates/store/src/schema.rs:467-468`) stays. A populated-schema-12 migration test must
  preserve existing runs, frames, sessions and settings.
- `ReportSummary` and `ReportDetail` (`crates/traits/src/reports.rs:125-181`, `§7f.6`) gain
  required `kind` (`daily` | `weekly_synthesis`) on the `GET /v1/reports` projections — the
  deliberate wire-contract addition reviewed with this migration, not an existing field that
  surfaces automatically. The on-demand `ReportKind::Weekly` (`§8b`) is not this kind.
- Weekly runs use the same run store, `ReportRun`/`ReportDelivery` machinery, destination
  snapshots and delivery receipts as daily runs. No HTTP/MCP generation, retry or delivery
  surface is added (C3 stays deferred); promotion has no HTTP/MCP surface in this slice.
- Retention interplay: finished runs of any kind prune 90 days after their last terminal
  update under the existing `§8c` exemptions (`src-tauri/src/memory.rs:20`). Weekly notes and
  the project note are user-owned output and are never pruned by run retention; ownership
  records outlive run pruning. Provenance IDs may outlive their sources; frame/session and
  report reads report missing evidence honestly (404) and never invent availability.

### Weekly note, index and frontmatter

- Path: `<selected folder>/ScreenSearch/Weekly/YYYY-Www.md` (e.g. `2026-W42.md`), with the
  same writer guarantees as daily notes: atomic same-directory temporary write/replace,
  persisted SHA-256 ownership independent of run retention, edited/unrelated files prechecked
  and preserved untouched, ReplaceFileW replacement with the displaced-backup hash compare,
  conflict receipts carrying `recovery_path`, reparse-point rejection, no tokens or invented
  deep links. `edited_weekly_note_is_preserved` and the existing interrupted-write behavior
  apply.
- `ScreenSearch/Index.md` gains weekly entries (label `YYYY-Www`, status, brief source-backed
  topic) linking the week to its weekly note and the daily notes it consumed, under the same
  atomic-write, ownership and conflict rules. The index builder's folder filter
  (`src-tauri/src/memory.rs:1219`), its provenance re-derivation check
  (`src-tauri/src/memory.rs:1261-1276`) and `note_relative_path`
  (`src-tauri/src/memory_files.rs:95-120`) each gain a weekly branch so a
  `Weekly/YYYY-Www.md` file passes the same check as daily notes. Index.md grows by design;
  any split rule is a review decision, never silent bounding.
- Frontmatter schema v2 adds `kind`, `daily_run_ids` and `window_local_dates` to the schema-1
  keys (`src-tauri/src/memory_files.rs:62-83`); weekly notes write schema 2 and schema-1
  notes must still parse (`frontmatter_v1_still_parses`), with a typed parse/serialization
  roundtrip. `kind` defaults to `daily` when absent; the same coverage counts, RFC3339 UTC
  `window_start`/`window_end` and frame/session ID arrays as daily apply.
- Headings: `Activity`, `Explicitly evidenced decisions`, `Open questions`, `Next steps` —
  the daily headings with `Open questions` and `Next steps` added for cross-day synthesis
  (`crates/kernel/src/reports.rs:786`). Every item cites frame/session/daily-run IDs;
  tentative screen text is never stated as a commitment.

### Project note and promotion

- Exactly one user-created, user-named project note at
  `<selected folder>/ScreenSearch/Projects/<slug>.md`; the app never invents, renames or
  deletes it, creation is manual and default-empty, and multiple notes are out of scope.
- Promotion is the only write path: the user selects items from a weekly synthesis or daily
  report and confirms; the app appends one dated, provenance-carrying entry in one atomic
  write. No promotion without explicit confirmation; generated prose never enters a project
  note unreviewed, and rejected proposals are not persisted anywhere. No automatic edits to
  personal identity or existing knowledge pages.
- Fixed sections: `## Decisions`, `## Open questions`, `## Next steps`, `## Log`. Each
  appended entry carries its date, source `run_id`(s) and frame/session IDs; the note is an
  append-only ledger — existing lines are never rewritten, and the app appends only to
  unchanged owned files. After first write the note is user-owned under the same ownership
  rules; user edits surface as conflicts, including an edit between selection and
  confirmation, and are never overwritten.

### Weekly scheduling

- Default-off weekly schedule beside the daily one; Settings controls mirror D4 (enable,
  local time, timezone; the output folder is the `§8c` selected folder, not a separate
  choice; Run now; last-run status). Persist weekly enable/time/zone with weekly activation,
  cursor and gap state beside the daily schedule state (`memory.weekly.*` mirroring
  `memory.schedule.*`); enable requires valid time and zone and the selected folder. Run now
  enqueues the most recent complete ISO week and requires a selected zone but not an enabled
  schedule.
- First eligible window is the first complete ISO week after activation; no partial
  activation week is scheduled. Fire at the selected local time after the week closes,
  mirroring the daily rule. Timezone selection is fixed while travelling; DST gap/overlap
  follow the `§8c` rules (missing local time → first valid instant; overlapping → earlier
  instant once).
- Restart catch-up processes missed weeks oldest-first, capped at the 4 most recent complete
  ISO weeks; older gaps stay visible and are never claimed as covered. No duplicate finalized
  UTC window for the same schedule; config changes never mutate old deduplication keys.

### Weekly Telegram digest (G6, after G5)

- A weekly digest with its own weekly-specific format — week label `YYYY-Www`, truthful
  status and coverage, and the saved synthesis highlights with source IDs — under the
  existing `§8d` opt-in exception: default-off, explicitly enabled, text only, derived from
  the saved weekly output, user-selected destination, no raw OCR or screenshots, no second
  generation and no second model.
- `§8d` credential handling, masking, chunk bounds, delivery intents, retry bounds,
  unknown-delivery handling and resolution apply unchanged to the weekly destination.

### Non-goals

- No automatic edits to personal identity or existing knowledge pages; no promotion without
  explicit confirmation; rejected proposals are not persisted.
- No report generation, retry or delivery over HTTP/MCP (C3 stays deferred); weekly runs are
  local, scheduled and default-off like daily; no HTTP/MCP surface for weekly runs or
  promotion in this slice.
- No new capture, no audio, no cross-platform capture, no cloud inference, no automatic model
  downloads; no scoring, nags or unsolicited automation (pull-based, D4/D11).
- No multiple project notes and no additional rollup layer above the weekly synthesis in this
  slice.
- Schema changes only as the single forward-only 12 → 13 migration, never guessed or
  downgraded.

### Increments and acceptance

Each increment is separately reviewable and lands only with its named checks; a name here is
not evidence that a check ran. G1–G6 need no repeated blanket GO.

- **G1 Contract + migration:** freeze run identity, window, statuses, the `weekly_synthesis`
  kind, the persisted `kind` discriminator (12 → 13), promotion and note-frontmatter schema
  v2 in this section. `weekly_window_handles_dst`,
  `weekly_run_identity_reuses_schedule_window`,
  `migration_12_to_13_defaults_existing_runs_to_daily`, `frontmatter_v1_still_parses`.
- **G2 Engine:** weekly run through the shared service consuming saved dailies; truthful
  statuses. `weekly_requires_saved_dailies`, `partial_week_reports_missing_days`,
  `weekly_no_model_is_blocked`.
- **G3 Writer:** atomic weekly note + index/path/provenance branches.
  `weekly_note_contains_provenance`, `weekly_index_links_daily_and_weekly`,
  `weekly_note_path_passes_provenance_check`, `edited_weekly_note_is_preserved`.
- **G4 Promotion:** select → confirm → append; conflicts preserved.
  `promotion_requires_explicit_confirm`, `promotion_appends_without_rewrite`,
  `promotion_to_edited_note_surfaces_conflict`.
- **G5 Scheduler:** weekly schedule, catch-up, visible gaps. `weekly_catchup_oldest_first`,
  `weekly_backfill_cap_reports_gap`.
- **G6 Weekly Telegram digest:** weekly-format rendering from the saved weekly report and
  reuse of the `§8d` receipt/retry machinery. `weekly_digest_uses_saved_weekly_report`,
  `weekly_digest_renders_weekly_format`, `weekly_digest_chunks_fit_telegram_limit`.

Native acceptance mirrors the daily arc: a real weekly run over a real week of saved dailies,
the rendered note inspected in Obsidian with working links, and one promotion performed in
the running app. G6 additionally requires a user-approved destination/send and inspection in
Telegram, mirroring `§8d`; approval is not authorization to message a recipient. No test
changes the live profile, rewrites user notes or sends a real message without that approval.
Review focus: week-boundary/DST correctness, synthesis over-claiming (uncited claims are a
defect, not style), promotion races and kind separation. Rollback disables the weekly
schedule independently of the daily schedule and preserves reports, notes and receipts;
never delete or rewrite user notes.

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
The workspace MSRV is **Rust 1.95**; local rustup, CI, and release builds pin
**1.95.0**. The committed `libsqlite3-sys` 0.38.1 build script uses `cfg_select!`,
stabilized in 1.95 (despite omitting rust-version in its own manifest). The `quinn-proto` security update (PR #112, GHSA-4w2j-m93h-cj5j) requires at
least 1.85 and is retained at 0.11.16. CI and release builds reject affected
`quinn-proto` lockfile entries with `scripts/check-quinn-security.mjs`; Cargo CI
and MCP staging use `--locked` so verification cannot silently resolve another graph.

GitHub Actions on `windows-latest`: `cargo fmt --check`, `cargo clippy --workspace -D warnings`,
`cargo build`, `cargo test` (GPU/WinRT tests `#[ignore]`d), `ui` `npm ci && npm run build`, and a
`tauri build` artifact job. Release workflow (`.github/workflows/release.yml`; tag push publishes a full release since v0.4.1): **NSIS** installer + updater `.sig` + `latest.json` (NSIS shipped v0.1.0; code-signing pending). Inno/MSI/portable ZIP dropped — `07` #26.

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
- **Schema** — baseline DDL at the current `schema_version` (`§4`, 0.4.5 D2); a DB at any other
  version refuses to open and names the `ssv2c-rebuild` tool (0.4.5 D3). (0.2.x–0.4.2: forward-only
  migrations via `schema_version`, each with an idempotent up-script — history only.)
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
6. **Local API + export (PR7; historical 0.3.0 acceptance).** The loopback-only assertion below
   describes the shipped release; the Gate A network increment supersedes that assertion with
   `§7c` acceptance. API off by default (fresh profile: nothing listens); with it on, a
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

## 13d. Definition of done (0.4.5 arc)

Each item is demonstrated with verbatim command output or a described-and-performed live check
(`04 §6`). The per-phase detail lives in `docs/0.4.5.md`; the normative contracts it implements are
`02 §5k` (scope and waves), `03 §7h`/`§7i`/`§7j` (work graph, Ask v2, insights) and `03 §8`
(settings v2, 0.4.5 D6). "Executing" ends at merge; the debug window is the maintainer dogfooding
that merged build; a phase is done only with its acceptance evidence merged, and the next wave
starts when all three phases of the previous one are merged.

**Gates.** The three gates close a wave's debug window and tag a release through the existing signed
updater. Each is demonstrated, never asserted:

- **Gate A → v0.4.3:** the design direction is recorded in `UI_REFERENCE §1`; the rebuild tool was
  used for real (P3's run, counts pasted); the app runs on v14 for **at least 3 working days without
  a blocker**; P4's baseline numbers are frozen as the P5/P6 thresholds; the F3 native interactive
  acceptance is performed (closes that `05` gate — recorded at `specs/archive/05_BUILD_REVIEW.v0.4.5.md`). Ships the pending `main` increments (#145, setup
  visibility).
- **Gate B → v0.4.4:** the P5/P6 thresholds are met on the P4 automatic set (report pasted); Settings
  v2 is in daily use; the external-agent Settings UI is verified natively.
- **Gate C → v0.4.5:** the full `04` audit; every `UI_REFERENCE §10` criterion on every screen; the
  P4 automatic set rerun end to end; the separate-machine API check; `05`–`08` swept and archived per
  `04 §7`; CHANGELOG and release notes.

**Per-phase acceptance.** Each phase's own acceptance line is binding:

1. **P1 — specs contract.** No code: `git diff --name-only main` shows only `*.md`. Every decision
   D1–D18 is normalized into the specs at its landing site; the v14 baseline DDL is complete enough
   that P3 and P5 are implementable from `03` alone; a fresh agent session can start any wave-A phase
   without reopening `docs/0.4.5.md`.
2. **P2 — design system v2.** `/kitchen` renders every primitive in every state and theme; an
   automated contrast check over the semantic pairs passes AA; a lint rule (or a test) fails on raw
   hex, px font sizes or non-token spacing in `ui/src/components/primitives`; existing screens still
   render through `tokens.compat.css` (screenshots attached to the PR); full verification suite green.
   *Status (2026-11-04):* met and merged to `main` as `7e73e30` for four themes. Raw
   output for every item is in `specs/archive/05_BUILD_REVIEW.v0.4.5.md`; the enforced checks are `ui/test/contrast.test.mjs` and
   `ui/test/tokens.test.mjs`, each demonstrated failing once before passing. Two evidence gaps stay
   explicit: populated screenshots need the Tauri app with a seeded DB (`docs/SCREENSHOTS.md`), so
   the PR carries the `/kitchen` gallery in all four themes instead, and one detector finding
   (`[layout-transition] transition: width`) is emitted by three v1 `components/domain/**` files
   outside P2's ownership (`07`, owner P10). Merged by the maintainer (D18).
3. **P3 — baseline schema v14 + settings v2 model.** The rebuild tool runs against a copy of the
   maintainer's real DB with per-table row counts before/after pasted verbatim and a 20-random-frame
   spot check opening in Moment; the app boots on v14 and refuses any other version naming
   `ssv2c-rebuild`, with `*.v13.bak` created only after the maintainer confirms the new file opens
   (0.4.5 D3); a settings round-trip test over the v2 key set passes and a retired key is rewritten
   away on load without error (0.4.5 D6); `cargo test` green; bindings diff committed.
4. **P4 — automatic intelligence evaluation.** The baseline report is pasted verbatim; the question
   generators are deterministic for a given DB and seed (test); the harness opens the DB read-only
   (test asserts it); **zero maintainer input is required to run it** — no hand-written question set,
   no labelling, no golden file (0.4.5 D17).
5. **P5 — work graph engine.** Entity recovery on the P4 automatic set is at or above the thresholds
   frozen at Gate A; the backfill over the real DB completes with counts pasted; a determinism test
   shows the same input producing the same threads; override precedence is tested (rename, merge and
   split each).
6. **P6 — Ask v2.** On the P4 automatic set, recall@10 and the known-fact answer rate beat the
   baseline by the Gate A margins; citation validity is 100% (both session and frame ids, every one
   in window and in a retrieved session); p95 latency is within the Gate A budget; full suite green.
7. **P7 — shell, navigation, Settings v2.** Every UI key in `03 §8` is reachable and round-trips;
   settings search finds every setting by label and by key; a keyboard-only pass through all
   categories; the state matrix (loading / empty / error / partial / populated) holds per category;
   screenshots in all four themes (`UI_REFERENCE §1`); the external-agent read-credential UI
   verified natively (closes that `05` gate — recorded at `specs/archive/05_BUILD_REVIEW.v0.4.5.md`).
8. **P8 — insights + connections engine, agent surface.** Each insight type has a fixture test and a
   real-DB sample pasted; no insight row exists without evidence (insert-time rejection, its test, and
   the zero-row sweep, `§7j`); the 0.4.5 API routes are covered by integration tests behind read
   credentials; the four MCP tools round-trip against the live app.
9. **P9 — intelligence UI.** Full state matrix per screen (Work list, Thread page, Insights v2, Ask v2
   conversation); keyboard paths; every rendered claim links to its evidence; screenshots in all four
   themes; a 10-question spot check drawn from the P4 automatic set, asked through the UI, results
   recorded.
10. **P10 — Recall surfaces refont + shim removal.** Zero legacy tokens (grep pasted); the lint gate
    covers all of `ui/src`; per-screen state matrix; the `UI_REFERENCE §8` performance budgets
    re-measured and pasted; the overlay is still covered by the self-exclude capture gate (test).
11. Full verification suite green for every PR: `cargo fmt --check` · `cargo clippy --workspace
    --all-targets -D warnings` · `cargo build --workspace` · `cargo test --workspace` · `ui`
    `npm run lint && npm run build` · `git diff --exit-code -- ui/src/bindings` clean.

---

*Next layer:* `04_CLAUDE_CODE_BUILD_PROMPT.md` — how the agent operates against `00`–`03`
(reading order, source-of-truth, build order, guardrails, stop-at-ambiguity).


### Memory path normalization correction (2026-10-15)

New memory settings, delivery destination snapshots and receipt/recovery paths use forward
slashes. Read legacy backslash ownership paths compatibly and persist canonical ownership keys
transactionally to avoid duplicate ownership/conflict records. This is Windows-only path
normalization; preserve native atomic file operations. No schema migration is required.
