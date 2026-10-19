# Architecture (as-built)

## Weekly memory and external-agent reads (shipped in v0.4.2)

F3 extends the daily memory below. `kernel::report_service::execute_weekly` runs a weekly
synthesis through the same shared service and model path, over that ISO week's saved daily
reports rather than raw captures. `src-tauri/src/memory_weekly.rs` is a child of the memory
runtime that owns only the default-off weekly settings, its own schedule ID/activation/cursor,
catch-up planning and missed-week gaps (settings KV, no extra migration); the parent keeps the
report worker, file writing and Telegram delivery. Store migration 13 adds `report_runs.kind`
(`daily` | `weekly_synthesis`). `memory_files` writes weekly notes with frontmatter schema 2
and accepts only schema 1 or 2. `src-tauri/src/promotion.rs` is the only write path into the
single user-created project note: preview, then one atomic append on confirm, with a persisted
write intent for recovery; it has no HTTP/MCP surface.

The external-agent reads (`03 §7f`) add `store::history_pages` (chronological frame pages and
coverage) and `store::report_pages` (saved-report pages), each in one read transaction, served
by `api::history` and `api::reports` under explicit per-route/method permissions
(`api::permissions`) for the legacy token and an independent read-only token. The
`screensearch-mcp` wrapper adds four passthrough read tools, a `--read-only` tool filter and
explicitly configured remote HTTP/HTTPS roots with no proxy and no redirects.

## Daily memory increment (shipped in v0.4.1)

`kernel::report_service` shares evidence inspection and execution between Recall and durable
memory runs. The automatic provider snapshots an installed model and cannot invoke downloads.
`kernel::memory_schedule` performs pure fixed-IANA calendar calculations. The composition root
in `src-tauri/src/memory.rs` wires one serial worker, typed IPC and persisted activation/cursors.
`traits::memory` defines generated DTOs; store migration 12 adds transactional run, delivery and
file-ownership records. Saved results and destination snapshots are immutable.

`memory_files` writes owned notes with native atomic Windows operations and recovery backups;
`memory_telegram` holds tokens in Credential Manager, derives masked text from saved output,
and classifies accepted, rejected and uncertain transport results. Intents precede effects;
each chunk receipt is saved before the next dispatch. Settings exposes independent outcomes.

Shutdown joins the asynchronous worker within its timeout before the model supervisor stops.
An already-started blocking Windows file operation is not cancellable through Tokio abort and
may finish after that await is cancelled. It owns only file inputs, no model or database handle;
the persisted intent allows restart reconciliation. The timeout is not a claim that a hung
filesystem call has been joined. Native slow/unavailable-folder acceptance remains open.

See [configuration and recovery](MEMORY.md) and `03 §8c/§8d/§8e` for the full contract. The following
sections retain the shipped 0.4.0 architecture history.

How ScreenSearch V2c is actually put together **as of 2026-07-11** — reflecting the complete 0.3.0
arc (PR1–PR8: surface reduction, Flow overlay, where-was-i + marks, local HTTP API + export, MCP
server), the shipped **0.3.1 patch** (P7.1 triage: the #64 vision-throughput fix —
`VISION_MAX_EDGE` capped back at 1280 px — plus polish: inline Moment text, dated report
filenames + self-describing footers, the NavRail version link, the UIA client-lifecycle fix, and
the `Ctrl+Alt+Z` overlay default), and the shipped **0.3.2 product-shell arc** (P7.2:
auto-update via `tauri-plugin-updater` + a signed GitHub-Releases `latest.json`
[`src-tauri/src/update.rs`, `.github/workflows/release.yml`, `scripts/make-latest-json.mjs`];
the native system tray with passive state icon, quick actions, close-to-tray, and
`tauri-plugin-autostart` run-at-startup [`src-tauri/src/tray.rs`, incl. the `cancel_vision`
command]; the D9 one-scroll-context shell contract; and the two-tier Settings IA), on top of the
shipped 0.2.x
attention-first / Recall work (capture -> OCR/UIA text -> content-text store -> text embeddings ->
hybrid search -> **inference sidecar**: vision tagging + grounded `ask` + reports -> Command Deck UI
and Flow overlay). This describes the
**implemented** system and how to navigate the code; the design intent and the *why* live in
[`specs/`](../specs) (`03_MASTER_PRODUCTION_SPEC.md` is authoritative for schema/traits/protocols).
Where they ever disagree, the specs win — open an issue.

- Implemented: P0 scaffold, P1 data spine, P2 capture, P3 enrichment + search,
  **P4 inference sidecar** (Job-Object lifecycle, vision tagging, streaming RAG answers,
  tiered runtime-downloaded models), and **P5 Command Deck UI** (Deck, Recall, Timeline, Moment,
  Insights, Settings), plus P5 hardening for bounded IPC, range-aware navigation, retention,
  storage telemetry, typed operational events, cancellable ask streams, adaptive charts, monitor
  enumeration, advanced sidecar device selection, and the 0.3.0 **Flow overlay** (a protected,
  global-hotkey second Tauri window for Search/Ask).
- Implemented for 0.2.0: `frame_text` / `text_spans` / raw-vs-content retrieval, PR3's
  attention-first classifier plus the later self-capture/backfill audit fix, the Recall
  Search/Ask/Reports UI, and Calendar-Grid Coverage Map-Reduce reports. PR7 manual audit
  evidence is local-only under ignored `docs/AUDIT*.md` / `.playwright-mcp/`; tracked summaries
  live in the build-loop docs and changelog.
- Implemented for 0.2.1, then trimmed in 0.3.0 PR2: opt-in, default-off **event-driven capture**
  now uses foreground/app-switch + idle only over the debounce/rate-ceiling/idle-edge state machine.
  The old clipboard, typing-pause, click, and scroll-stop triggers remain readable as legacy
  `CaptureTrigger` values, but new captures no longer emit them.
- Implemented for 0.3.0 PR2–PR8: removed the invasive trigger surfaces, retired the Beta model
  tier, removed the unused image-embedding lane with schema v9, and added the Flow overlay with
  configurable hotkey/status controls (PR2–PR5); **where-was-i + mark-this-moment** — the
  `kernel::resume` last-sustained-context heuristic, the schema-v10 `marks` table, the
  `capture_now` diff-gate-bypass path, and a second global hotkey with a non-focus-stealing toast
  (PR6); the **opt-in localhost HTTP API + JSON export** (`crates/api`, axum, originally
  loopback-only with bearer auth — PR7; the current network increment broadens the listener
  to configurable IPv4/IPv6 binding with a loopback default); and the **`screensearch-mcp.exe` stdio MCP server** wrapping that API,
  bundled via NSIS `externalBin` (PR8).
- Implemented for 0.3.2: **auto-update** — `src-tauri/src/update.rs` (single-flight check →
  background download → signature-verify → install only on user-initiated restart; launch check
  in release builds only; manual check in the tray, Settings App section, and NavRail footer),
  the minisign public key baked into `tauri.conf.json` and the release pipeline
  (`.github/workflows/release.yml`) signing the NSIS installer and publishing `latest.json` —
  and the **system tray** — `src-tauri/src/tray.rs` (passive per-state icon fed by the kernel
  event bus, the six-item menu, close-to-tray default-on with a one-time first-restore toast,
  `tauri-plugin-autostart` run-at-startup default-off, and the new `cancel_vision` command).
  UI-side: the D9 shell-layout contract (one scroll context per route) and the two-tier Settings
  IA (Essentials + seven collapsed Advanced expanders; `storage.jpeg_quality` and
  `capture.uia_run_on_interactive` retired via `RETIRED_SETTINGS_KEYS` tolerate-and-drop).
- Implemented for 0.3.3 (hotfix): the UIA text source **skips Chromium/Electron windows** to stop
  browser freezes (`07` #93); this release is the first delivered automatically by the 0.3.2 updater.
- Implemented for 0.4.0 (P8 — the **sessions** arc; `docs/0.4.0.md`, `03 §7e/§13c`): frames are
  grouped into **sessions** — additively (D10, zero frame-level behavior change) and with **no model
  calls in segmentation** (D3). The one schema migration of the arc (**v10 → v11**) adds the
  `sessions` + `session_artifacts` tables and a nullable `frames.session_id`; the pure heuristic
  `sessions` engine (`crates/sessions`) segments per identity track and recognizes tool identity from
  a seed taxonomy; a dev-only, read-only validation harness (`crates/harness`) scores segmentation
  against hand-labeled ground truth. Sessions are pull-based / non-shaming (D11), audio-free (D14),
  and add **no new NavRail route** (D13); titles/summaries are lazily generated + cached (D3).
- Still open: Authenticode code signing (the updater's minisign signature is separate and live);
  the 0.4.0 sessions API/MCP surface (PR6) and the remaining follow-ups tracked in
  `specs/07_KNOWN_GAPS.md`.

---

## 1. Principles

- **Capture-cheap, enrich-deferred.** The always-on path does only cheap work (capture → OCR →
  store). Everything expensive (embeddings, vision tagging) is pushed into a durable SQLite **job
  queue** and run by background workers on user-controlled triggers (`03 §1/§5`).
- **Fault isolation by construction.** The only crash-prone, out-of-process component — the
  `llama-server` inference sidecar — is bound to the app via a Windows **Job Object** so it can
  never orphan; a failed enrichment job retries instead of taking capture down (`02 §2`, `03 §6`).
- **Trait-bounded modularity.** The `kernel` and module crates depend only on the contracts in
  `traits` — never on each other's concrete impls. `src-tauri` is the **composition root**: the one
  place that wires concrete impls into the kernel (`03 §2`).
- **Windows-native by design.** WGC capture, WinRT OCR, WebView2 — no cross-platform abstractions.
- **Rust-only ML runtime.** Embeddings via `fastembed` (in-process ONNX); vision/answers via the
  local `llama-server` sidecar (OpenAI-compatible HTTP over loopback). No Python in the runtime,
  no cloud calls — everything downloads from GitHub / HuggingFace and runs on-device.
- **Verify, never fabricate.** No stubs/hardcoded results; "done" means observed running. Schema
  changes are forward-only with a `schema_version` bump.

---

## 2. Crate map

A 15-crate Rust workspace plus the React/TS `ui/`. `src-tauri` is the **composition root**: it opens
the store, spawns OCR, builds the `kernel`, loads the embedder + inference off-thread, owns the
Flow overlay window/hotkey plumbing and the local-API host, and registers commands. `kernel`
orchestrates (event bus, capture loop, worker pool, vision scheduler, readiness, the where-was-i
resume heuristic);
the module crates — `capture` (WGC + triggers + privacy), `ocr` (WinRT), `uia` (UI-Automation text),
`store` (SQLite + sqlite-vec + FTS5), `embeddings` (fastembed), `inference` (Job-Object-bound
`llama-server` sidecar), `textfilter` (attention-first span classifier), `sysmon` (pressure probe),
`doctor` (env smoke-check), `api` (opt-in HTTP API + export), `mcp` (the
`screensearch-mcp.exe` stdio wrapper over that API), `sessions` (0.4.0 — the pure heuristic
segmentation + tool-recognition engine), and `harness` (0.4.0 — a dev-only, read-only segmentation
validation harness, not shipped in the app) — each
depend only on the contracts in `traits`, never on one another's concrete impls.

**Authoritative crate map & dependency rule: `specs/03_MASTER_PRODUCTION_SPEC.md` §2.** The
per-crate file-level guide to where each concern lives is the rest of this document (§4–§11).

---

## 3. Data model (SQLite, WAL)

Single file `screensearch.db`; forward-only migrations tracked in `schema_version` (`store::schema`).
Per-connection pragmas: `journal_mode=WAL`, `foreign_keys=ON`, `recursive_triggers=ON`,
`busy_timeout=5000`. **Authoritative as-built DDL (every table, column, index, trigger, and the full
v1→v13 migration chain): `crates/store/src/schema.rs` (`LATEST_SCHEMA_VERSION = 13`)** — this is
code, so it never drifts. `03 §4` is the design contract for the schema; where the migrations have
moved ahead of it (v3 drops legacy `ocr_text`, v4 adds `text_spans.line_index`, v5 adds the nullable
`frames.capture_trigger`, v6 widens that trigger token set for click / scroll-stop, v7 adds
`frames.image_purged` for degrade-to-text retention, v8 adds the partial
`idx_frames_image_retention` index for the retention sweep, v9 drops the removed image-embedding
lane, v10 adds the `marks` table — frame-cascading intention capture with `idx_marks_open` — and
**v10 → v11 (0.4.0 PR3)** adds the `sessions` and `session_artifacts` tables plus a nullable
`frames.session_id` and its three indexes, structure-only with no backfill, **v11 → v12 (0.4.1)**
adds the durable report tables `report_runs`, `report_deliveries` and `report_files`, and
**v12 → v13 (0.4.2, F3 G1)** adds `report_runs.kind` (`daily` | `weekly_synthesis`, default `daily`) with
`idx_report_runs_kind_created`), the code in `store::schema` wins.

Conceptually the schema groups into: capture rows (`frames`), the 0.2.x text signal (preserved raw
vs. filtered `content_text` plus per-span and static-chrome metadata, with content-text and raw-text
FTS mirrors), the text embedding lane, vision analysis (P4), the durable `jobs`
queue, tags, `marks` (0.3.0), **sessions** (`sessions` + `session_artifacts`, 0.4.0 — derived-but-
persisted: a wiped `sessions` table is fully recomputable from frames, so `frames.session_id` is
`ON DELETE SET NULL` while `session_artifacts` CASCADE with their session), and `settings`. The
notes below capture the two as-built decisions that the DDL alone doesn't convey.

Each embedding lives in **two** lock-step places — a metadata row and its `vec0` `FLOAT[768]` cosine
shadow keyed by the same id. Upserts do both in one transaction; deletes are handled by
`AFTER DELETE` triggers + the `frames` FK cascade (`store::embeddings`).

**Concurrency:** one `rusqlite::Connection` behind a `Mutex` for the store's lifetime; every async
`Store` method runs its SQL inside `spawn_blocking`, and the guard is never held across an `.await`
(`store::lib::with_conn`). SQLite is single-writer, so this is correct and simple.

---

## 4. Always-on capture pipeline (P2)

`kernel::Kernel::start_capture` builds a `CaptureSource` from the current `Settings` via the
composition root's factory and spawns `run_capture_loop` (`kernel::capture_loop`). Per changed
frame:

```
WgcCapture.next_frame()           # diff-gated + privacy-gated; only *changed* frames
  → WinRtOcr.recognize()          # on full-res pixels, before storage downscale
  → write JPEG (downscaled)       # <app-data>/frames/day-<n>/<captured_at>-<monitor>.jpg
  → store.insert_frame + insert_ocr
  → enqueue embed_text job        # if enrich.embed_text
  → emit KernelEvent::CaptureTick # drives the live timeline
```

**Capture cadence — timer OR (opt-in) event-driven (0.2.1; trimmed 0.3.0 PR2).** By default capture
paces to `capture.interval_ms` (the 0.2.0 timer cadence). When `capture.event_driven_enabled` is on
(default off), `WgcCapture` instead captures on real user activity: a pure debounce / rate-ceiling /
idle-edge **trigger state machine** (`capture::trigger`, no Win32, unit-tested) is fed by a dedicated
**input-events thread** (`capture::events`) that runs a bare message pump plus one out-of-context
foreground hook (`SetWinEventHook` `EVENT_SYSTEM_FOREGROUND`). Idle needs no hook — it polls
`user_idle_ms` (`GetLastInputInfo`). *(0.3.0 PR2 trimmed the six triggers to foreground + idle,
deleting the clipboard listener, the global `WH_MOUSE_LL` mouse hook — click/scroll-stop — and the
typing-pause edge, so a whole `unsafe` path and the message-only window are gone — `docs/0.3.0.md`,
`02 §5c`.)* A long fallback interval still samples a static screen, a debounce collapses bursts, and
a min-interval ceiling caps the rate. A failed hook install is non-fatal (falls back to the fallback
timer + idle polling). The kernel loop and the `CaptureSource` trait are unchanged; the event source
lives inside `WgcCapture`, which stamps each frame with a `CaptureTrigger`. New frames use only
`Timer`/`Idle`/`ForegroundChange`/`Manual`; the legacy `ClipboardChange`/`TypingPause`/`Click`/
`ScrollStop` tokens stay in the enum + the `frames.capture_trigger` CHECK (schema v6, unchanged) so
pre-trim frames still render in the Moment "Captured via" row. Event settings hot-apply through the
existing `set_settings`→`reload_capture` path (`CaptureConfig` is `PartialEq`).

Capture is **off until the user starts it** (privacy-first). If WinRT OCR cannot be created, the app
still boots but capture start fails with `capture = Unavailable` rather than storing empty OCR rows.
Per-frame errors are logged and the frame skipped — capture keeps running. If the capture source
itself shuts down without a user Stop, the kernel clears the live handle and reports
`capture = Error` so the UI cannot remain stuck on a stale Ready state. No screen content or OCR text
is logged at info level.
`vision_tag` is **never** auto-enqueued per frame — it is produced only on-demand (the
`enqueue_vision` command) or by the opt-in timer/idle scheduler (§7), so vision work never runs in
the always-on hot path.

---

## 5. Deferred enrichment (P3)

The new half of the system. The `embed_text` jobs the capture loop enqueues are drained into
vectors by a background worker pool.

### 5.1 Job queue (`store::jobs`)

State machine `pending → running → done`, or `running →` (fail) `→ pending` (retry with backoff)
`→ … → dead` (dead-letter at `max_attempts`, never silently dropped). Claims are a single atomic
`UPDATE … RETURNING` so no job is handed to two workers. Key methods: `enqueue_job`, `claim_jobs`,
`complete_job`, `fail_job(err, retry_at)`, `job_stats`, and `reset_stale_running_jobs` (P3).

### 5.2 Embedding provider (`embeddings::FastEmbedProvider`)

`fastembed` 5.17.2 (in-process ONNX, no Python). Text = `EmbeddingModel::EmbeddingGemma300MQ`
(768-dim, quantized → **embeds one input at a time**, it cannot batch) — the only embedding lane
(0.3.0 PR4 removed the optional nomic-embed-vision image lane). The lane is an `Arc<Mutex<…>>` whose
lock is taken **inside** a `spawn_blocking` closure (the model is a plain `Send` ONNX handle with no
thread affinity, unlike COM-bound OCR). It loads eagerly in `FastEmbedProvider::new` — called off the
launch thread — into `<app-data>/models/fastembed`, downloading from HuggingFace on first run.

### 5.3 Worker pool (`kernel::worker_pool`)

`Kernel::attach_embedder` injects the loaded provider into the store (lighting up the vector arm) and
fills the worker pool's shared embedder slot. `Kernel::attach_inference` fills the shared vision slot.
Both call the same idempotent `start_workers`, so the pool can start from either provider —
**independent of capture** — and pick up the other provider later without a restart.
`N = enrich.worker_concurrency` workers each loop:

```
claim_jobs(dynamic provider-backed lanes, 1, now)
  → process_job:                       # public, so tests drive one job deterministically
      embed_text:  read OCR text → embed_texts → upsert_text_embedding(chunk 0, source=ocr)
      vision_tag:  load JPEG → VisionProvider.analyze → insert_vision   (P4)
  → complete_job / fail_job(backoff) / dead-letter
  → emit KernelEvent::JobProgress(job_stats)
```

Workers build the claim-kind list on every poll. `EmbedText` is claimed only when an embedder is
attached and `enrich.embed_text` is enabled; `VisionTag` is claimed once the sidecar vision provider
is attached. Both providers live in `Arc<RwLock<Option<…>>>` **slots** that are snapshotted per job,
so `vision_tag` backlogs drain even when embeddings are disabled or unavailable.
If a claimed job somehow lacks its provider because of a race, it **retries** (not fails) so the
backlog drains when the provider appears.

Outcome rules: missing `frame_id` or a missing JPEG → **dead-letter** (won't fix itself); a purged
frame or empty/whitespace OCR → **complete** (nothing to embed is success, not failure); embed/
upsert/analyze errors → **retry** with backoff `1 s · 2^attempts` (cap 60 s). Idle poll backs off
250 ms → 2 s; shutdown is a `watch` channel that lets in-flight jobs finish.

**Stale-job recovery** (`03 §6`, gap #6): there is no per-job lease. A **startup sweep**
(`reset_stale_running_jobs(0)`) requeues anything left `running` by a dead worker; a **periodic
60 s sweep** with a 5-minute visibility timeout catches a worker that died while the app stayed up.

---

## 6. Hybrid search (`store::search`)

`hybrid_search(SearchQuery) → Vec<SearchHit>` fuses two ranked arms with **Reciprocal Rank Fusion**
(`k = 60`):

- **FTS arm** — BM25 over `frame_text_fts.content_text` (porter tokenizer), with highlighted
  snippets. User text is safely quoted per-term (no FTS-operator injection).
- **Raw/app-chrome arm** — only when `SearchQuery.include_chrome = true`, searches
  `frame_text_raw_fts.raw_text` so suppressed labels remain recoverable.
- **Vector arm** — embed the query once (via the injected `EmbeddingProvider`), then sqlite-vec
  cosine KNN over `embedding_vectors`, de-duped by frame. Active only once an embedder is attached;
  before that, search degrades to FTS-only.

`SearchQuery.limit` is normalized at the backend to `1..=2,000`: the Recall search UI still asks for
a smaller interactive page, while report retrieval can request larger bounded pools. Both arms
over-fetch a candidate pool (`max(limit·5, 50)`, capped at 2,000) and filter to the half-open time
range `[start, end)`. Results hydrate in two bulk `IN` queries (frame context + fallback snippets).
Ask, embeddings, and reports read `content_text`; raw/app chrome is opt-in. The PR7 audit's dominant
static-chrome failure was later traced to ScreenSearch capturing its own window and a cold-start
filter window; the PR3 audit fix self-excludes own-window captures and backfills older frames. The
remaining known limitation is rect-None / secondary-monitor chrome from other apps (`07` #58).

The embedder is **runtime-settable** (`SqliteStore.embedder` is `Arc<RwLock<Option<…>>>` +
`set_embedder`), so the composition root can attach the model *after* the off-thread load without
rebuilding the store; the search hot path clones the `Arc` out from under the lock before the
`.await`.

**Performance:** the `#[ignore]`d fixture `crates/store/tests/perf.rs` seeds 10 000 frames + 768-dim
vectors and measures **p95 ≈ 33 ms** — well under the `03 §13.4` ~200 ms bar.

---

## 7. Inference sidecar (P4)

The only out-of-process component. One `llama-server` child serves an OpenAI-compatible HTTP API on
`127.0.0.1:<ephemeral>`; the `inference` crate owns its whole lifecycle and exposes the two providers
the kernel drives. Built **lifecycle-first**: the no-orphan binding is proven before any real
inference (`04 §3`).

### 7.1 No-orphan guarantee (`inference::job_object`, `inference::process`)

A `ModelSupervisor` creates a Windows **Job Object** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
Every `llama-server` is spawned **suspended** (raw `CreateProcessW` with `CREATE_SUSPENDED` — `std`
can't do this), **assigned to the job before its main thread is resumed**, then resumed. Because the
OS closes every handle a process owns when it dies — including the last handle to the job — the
sidecar is terminated whenever the app exits for *any* reason (clean exit, panic, kill, power loss
after resume). This is the hard requirement (`03 §6`, DoD #7); it's proven by a cross-process test
(`tests/no_orphan.rs`): a helper holds the job + a grandchild, the test kills the helper, and asserts
the grandchild dies.

**Startup reap** (`supervisor::reap_stray`): on launch, a stray sidecar from a prior run is killed,
identified by a **pidfile** plus an **image-path sentinel** — the recorded pid is terminated only if
its running image is the `llama-server.exe` we installed under app-data, never an unrelated process
that recycled the pid.

### 7.2 Supervisor (`inference::supervisor`)

`ModelSupervisor` is the one process at a time. `acquire(spec)` ensures the requested model is
running and returns a `Lease` carrying a cloned HTTP client; the lease counts the request **in-flight**
so the idle-evictor can't pull a model out from under it. Lifecycle:

- **Lazy spawn + `/health` gate** — the process starts on the first request that needs it; the
  supervisor polls `/health` (up to a generous timeout — first-run model load is slow) before serving.
- **Idle evict** — a background task stops the sidecar after `sidecar.idle_ttl_secs` of no in-flight
  requests, freeing GPU/RAM (the footprint control). It respawns on the next request.
- **Model switch** — a request for a different tier resolves a different GGUF; `needs_restart`
  detects the change and the supervisor stops + respawns (vision adds `--mmproj`).
- **Status** — every transition (`Starting/Ready/Evicted/Crashed/Stopped`) is broadcast as
  `SidecarStatus`; the composition root bridges it into the kernel (§8).

### 7.3 Tiered models, runtime-downloaded (`inference::models`, `inference::download`)

Vision and answer each offer **Default / Quality** (`MODEL_REGISTRY`; 0.3.0 retired the Beta tier).
Nothing is bundled —
everything downloads on first use, Rust-only (no Python in the runtime):

- **Binary** — `ensure_binary` fetches a prebuilt llama.cpp **Vulkan** Windows release zip
  from GitHub into `<app-data>/sidecar/llama` (asset selected by a unit-tested
  `*-win-vulkan-x64.zip` matcher; overridable via `SSV2C_LLAMA_RELEASE_URL`). It scans the
  recent-releases list rather than `/releases/latest` and takes the **newest release that
  actually carries** the Vulkan asset — llama.cpp's CI sometimes publishes a release with an
  incomplete asset set, which a single-`latest` lookup would fail on outright.
- **Models** — `ensure_model` lists the tier's HuggingFace repo via `hf-hub`, picks the `Q4_K_M`
  weights (+ the **same-repo** `mmproj` for vision — a mismatched projector crashes the server), and
  copies them into `<app-data>/models/<lane>/<tier>`. Idempotent (skips files already present).
- `resolve_spec` scans the local dir for the weights/projector and builds the `ModelSpec
  { lane, tier, gguf_path, mmproj_path?, ngl }` the supervisor launches.

### 7.4 Providers (`inference::vision`, `inference::answer`)

- **`VisionSidecar` (`VisionProvider`)** — encodes the frame as a JPEG base64 data URL, sends a
  non-streaming chat completion asking for compact JSON (`description`/`activity_type`/`app_hint`/
  `confidence`), and parses it into a `VisionAnalysis`. A non-JSON reply falls back to raw text as the
  description with a `-1.0` "unknown" confidence sentinel — **never a fabricated score**.
- **`AnswerSidecar` (`AnswerProvider`)** — builds a grounded RAG prompt from the retrieved chunks
  (each tagged with its frame id), streams the SSE reply, and maps it to typed `AnswerDelta`s:
  reasoning → `Thinking`, answer text → `Token`, one `Citation` per grounding frame, then `Done`
  (or `Error`). Reasoning arrives two ways depending on the build — a `reasoning_content` delta field,
  or inline `<think>…</think>` tags, which a `ThinkSplitter` separates even when a tag is split across
  SSE chunks.

Both providers hold the active tier (changed via `set_tier`) and **lazily download** their model on
first use, mirroring fastembed's first-run UX.

### 7.5 Vision scheduling (`kernel::vision_scheduler`)

Vision is never real-time. Three triggers feed `vision_tag` jobs:

- **On-demand** — the `enqueue_vision` command (a frame, or all still-untagged frames in a time
  range). Always available.
- **Timer** — opt-in (`enrich.vision_timer_enabled`): every `enrich.vision_timer_interval_ms`, enqueue
  up to a batch (N = 20) of untagged frames (`Store::untagged_frame_ids`).
- **Idle** — opt-in (`enrich.vision_idle_enabled`): when the OS reports the user idle ≥
  `enrich.vision_idle_secs`, enqueue a batch (on the transition into idle, not every poll). Idle time
  comes from `capture::user_idle_ms` (`GetLastInputInfo`), injected as an `IdleSource` because the
  kernel forbids `unsafe`.

There is no pending-job dedup; a frame enqueued-but-not-yet-processed can be re-enqueued, but
`insert_vision` is an idempotent upsert so the only cost is a redundant analyze (logged, `07` #19).

---

## 8. Events, readiness, settings

**Event bus** (`kernel::events::KernelEvent`, a `tokio::broadcast`): `CaptureTick`,
`ReadinessChanged`, `JobProgress`, **`JobCompleted`**, **`SidecarStatus`** (P4), and **`Toast`**.
The kernel is shell-agnostic;
`src-tauri::forward_events` bridges these to Tauri events (`capture_tick`, `readiness_changed`,
`job_progress`, `job_completed`, `sidecar_status`, `toast`). The `ask` command streams
request-scoped **`answer_delta`** events (`AnswerEvent { request_id, delta }`) directly from its
forwarding task; `cancel_ask(request_id)` aborts a superseded stream. The Flow overlay is
shell-level, not a kernel subsystem: `src-tauri::overlay` emits `overlay_shown` / `overlay_hidden`
to the overlay webview and `open_moment` to the main window.

**Readiness** (`03 §7`): one `ComponentReadiness { status, detail? }` per subsystem — `capture`,
`db`, `embed_model`, `sidecar` — where `status ∈ {unknown, disabled, initializing, ready,
unavailable, error}`. `embed_model` flows Initializing → Ready / Unavailable / Disabled. **`sidecar`
(P4)** flows Initializing (resolving binary) → Ready (binary present; model downloads + spawns on
demand) / Unavailable (binary or supervisor init failed); thereafter the supervisor's `SidecarStatus`
maps live — `Starting`→Initializing, `Ready`→Ready, `Evicted`→Ready ("respawns on demand"),
`Crashed`→Error, `Stopped`→Disabled (`kernel::sidecar_component`).

**Settings** (`kernel::settings`): the strongly-typed `Settings` is assembled from the opaque
key/value `settings` table; a missing/unparsable value falls back to the per-key default (never an
error), and numeric values are backend-sanitized on both load and save so direct IPC or hand-edited DB
rows cannot wedge capture or sidecar controls. Enrichment keys: `enrich.embed_text` (true),
`enrich.worker_concurrency` (2). **P4 keys:** `enrich.vision_timer_enabled` (false) +
`enrich.vision_timer_interval_ms` (60 min), `enrich.vision_idle_enabled` (false) +
`enrich.vision_idle_secs` (5 min), `models.vision_tier` / `models.answer_tier` (`default`),
`answer.thinking` (true), `sidecar.idle_ttl_secs` (180), `sidecar.ngl` (99), and optional
`sidecar.device`. **0.3.0 keys:** `overlay.hotkey` (`Ctrl+Alt+Z`) and
`overlay.max_results` (default `8`, sanitized to `1..=50`); `resume.min_dwell_secs` (default `120`,
the where-was-i dwell threshold) and `marks.hotkey` (`Ctrl+Alt+M`); the local-API trio
`api.enabled` (default `false`), `api.port` (default `43210`), and `api.token` (generated on first
enable, shown/regenerable in Settings). Model tiers and sidecar launch options are applied live for the next request that
needs a sidecar; enrichment worker lanes are reconfigured by restarting the pool from current
settings after save. Capture's enqueue decisions for new `embed_text` jobs are still captured when a
capture session starts, so changing that toggle affects capture enqueueing on the next capture start.
Captures are stored as **lossless WebP** at `storage.max_width` (default `0` = native, no
downscale — keeps ultra-wide text legible). *(The inert `storage.jpeg_quality` knob was retired in
0.3.2 PR5; a persisted key is tolerated + dropped on load — `03 §8`, D8.)*
`storage.retention_days` (default `30`) is a **degrade-to-text** window, not a hard delete: a startup
and hourly sweeper removes the **screenshot file** of frames past the window and marks
`frames.image_purged = 1`, but keeps the row + raw/content text + spans + embeddings as durable,
searchable proof. The Moment view renders a text + layout reconstruction (from `text_spans`, via
`get_frame_spans`) for degraded frames. Candidates are listed via `frames_with_image_older_than`
(`image_purged = 0`) so each frame degrades exactly once; `0` keeps screenshots forever. (True
full-delete, `delete_frame`, remains for the one-time self-capture purge.)

---

## 9. Query → answer path (`ask`)

```
ask(AskRequest{request_id?, query, thinking, max_tokens})
  → store.hybrid_search(query, top-K = 8)                     # grounding candidates
  → per hit: get_enrichment_input → full OCR text (fallback: snippet) → RetrievedChunk
  → AnswerProvider.answer(query, context, opts, tx)           # background task
       supervisor.acquire(answer spec) → SidecarClient.stream(SSE)
       → AnswerDelta::Thinking / Token / Citation(per frame) / Done|Error
  → forwarder emits each delta as an `answer_delta` Tauri event tagged with request_id
```

The command returns immediately; the answer streams asynchronously. The lease is held for the whole
stream, so the idle-evictor never stops the model mid-answer. Starting a new UI ask cancels the old
request id, and the UI ignores stale deltas.

---

## 9b. Flow recall + open surface (0.3.0)

- **Where-was-i** (`kernel::resume`, PR6): a pure, unit-tested heuristic over a bounded recent
  window of frames — the *last sustained context* is the most recent run (same context key =
  `app_hint`, refined by browser domain) that persisted ≥ `resume.min_dwell_secs` and ended before
  the current foreground session began; ScreenSearch itself and excluded apps never qualify.
  Surfaced via the `where_was_i` command in the overlay's empty state, a Deck card, and
  `GET /v1/context/where-was-i`. Pull-based only (D14) — no notifications.
- **Marks** (PR6): `Ctrl+Alt+M` (configurable, `marks.hotkey`) → `kernel::capture_now` requests one
  frame from the live capture worker with a per-request **diff-gate bypass** (D8 — a demanded frame
  is never dropped as "unchanged"), then inserts a `marks` row (schema v10, `ON DELETE CASCADE`).
  A quiet overlay toast confirms without stealing focus; the Deck **Intentions** strip lists
  unresolved marks (open / resolve / dismiss — no badge counts, D14). Marked frames follow normal
  retention: the image may expire, the mark keeps the text reconstruction reachable (D10).
- **HTTP API + export** (`crates/api` + `src-tauri::local_api`, PR7): default **off**; when
  enabled the axum server binds the configured **`api.bind_address` (IPv4/IPv6 literal; missing defaults to `127.0.0.1`)** on `api.port` and every request
  needs the `api.token` bearer (constant-time compare, 401 otherwise; D11). Endpoints: `/v1/health`,
  `/v1/search`, `POST /v1/ask` (SSE; consumer disconnect aborts sidecar generation), `/v1/frames/{id}`
  (`?image=1` → WebP), `/v1/context/where-was-i`, marks list/create/resolve (the only write surface),
  and `/v1/export` (JSON, frames + content text + marks, **no images** — D12). The host is a seam
  behind a trait, **not constructed at all** while disabled; the Settings "Export…" button calls the
  same export code path directly, so export works with the API off. Reference: `docs/API.md`.
  The listener uses plain HTTP; reachable token holders can read history. The inference
  listener stays loopback-only. 0.4.2 adds `/v1/frames`, `/v1/coverage`, `/v1/reports` and
  `/v1/reports/{run_id}` reads and an independent read-only token (see the section at the top). Existing enabled profiles
  without a bind address retain loopback exposure; address/port edits apply immediately. Startup emits a payload-free `api_status_changed` invalidation event;
  tokens are never broadcast through this event. No schema migration is needed.
- **MCP server** (`crates/mcp` → `screensearch-mcp.exe`, PR8): a dependency-thin **stdio** JSON-RPC
  binary that never links the store (D13) — purely an HTTP client of the local API using
  `SCREENSEARCH_API_URL` / `SCREENSEARCH_API_TOKEN`. Nine tools through 0.4.1
  (`search_screen_history`, `ask_screen_history`, `get_moment`, `where_was_i`, `list_marks`,
  `add_mark`, and the 0.4.0 session trio `list_sessions`, `get_session`, `ask_session`), plus
  `list_frames`, `get_capture_coverage`, `list_reports` and `get_report` in 0.4.2; loopback by
  default, with explicit remote HTTP/HTTPS roots allowed from 0.4.2. API-off or bad-token
  states return guided errors ("enable the API in ScreenSearch Settings"), never a crash. Staged as
  the Tauri `externalBin` by `scripts/stage-mcp.mjs` and shipped inside the NSIS installer next to
  `ScreenSearch.exe`. Reference: `docs/MCP.md`.

---

## 10. Startup sequence (`src-tauri::run`)

1. Resolve `<app-data>`; create `logs/`; init tracing (console + daily-rotating file).
2. Open the store (`open_store`) → `db` readiness Ready / Error.
3. Spawn the retention sweeper. It runs once at startup and then hourly, using
   `storage.retention_days`, `frames_with_image_older_than`, and a containment-checked frame path
   under `<app-data>/frames`: it deletes the screenshot file then calls `purge_frame_image`
   (degrade-to-text), keeping the row + text. `0` disables it.
4. Build the `Kernel` (store + OCR worker + WGC capture factory). Capture starts `Disabled`.
5. Manage shell state, including the overlay hotkey/status state. Load settings and register the
   overlay + mark global shortcuts with `tauri-plugin-global-shortcut`; conflicts emit a toast and
   are visible via `get_hotkey_status`. If `api.enabled`, start the local-API host (a bind failure —
   e.g. port in use — is loud: toast + guided Settings state, never a silent no-op).
6. Spawn `forward_events`. Set `embed_model = Initializing` and spawn `init_embeddings` (load model
   off-thread → `attach_embedder`: store embedder + embedder worker slot, `embed_model = Ready`,
   idempotently start the worker pool). Set `sidecar = Initializing` and spawn **`init_inference`**:
   `ensure_binary` (off-thread) → build `SupervisorConfig` + `ModelSupervisor::new` (creates the job,
   reaps a stray) → build `VisionSidecar`/`AnswerSidecar` → fill the supervisor/vision/answer slots →
   bridge `supervisor.subscribe()` into `kernel.emit_sidecar_status` → `attach_inference`
   (vision into the worker slot, answer for `ask`, idempotently start the worker pool, start the
   vision scheduler with the idle source) → `sidecar = Ready`. The first worker start, whichever
   provider triggers it, performs the startup stale-job sweep before spawning workers. Failure at any
   step sets `sidecar = Unavailable` with a reason.
7. Register Tauri commands; run. On `ExitRequested`: `stop_vision_scheduler` + `stop_workers`, then
   `supervisor.shutdown()` (kills the sidecar; the Job Object would anyway). All best-effort —
   correctness doesn't depend on it (the startup sweep requeues interrupted jobs).

**Commands** (typed via `ts-rs`): `ping`, `get_readiness`, `get_job_stats`, `get_storage_stats`,
`get_monitors`, `list_sidecar_devices`, `get_frame`, `get_frame_spans`, `search`,
`capture_control`, **`enqueue_vision`**, **`ask`**, `cancel_ask`, `generate_report`,
`cancel_report`, **`set_model_tier`**, `load_model`, `unload_model`, `get_timeline`, `get_frames`,
`get_nearest_frame`, `get_frame_context`, `get_insights`, `get_settings`, `set_settings`,
`get_text_filter_stats`, `get_throttle_status`; the overlay set — `get_hotkey_status`,
`hide_overlay`, `overlay_shown_ack`, `open_moment`, `focus_overlay_for_note`,
`dismiss_mark_toast`; the 0.3.0 flow-recall set — **`where_was_i`**, **`add_mark`**, `list_marks`,
`resolve_mark`, `set_mark_note`; and the local-API set — `set_api_config`, `get_api_status`,
`regenerate_api_token`, **`export_data`**.

---

## 11. Testing

- **Unit / integration, platform-agnostic (run in CI):** store state-machine + retrieval + the
  `untagged_frame_ids` query against `:memory:` SQLite; capture-loop and worker-pool tests with
  **fake** sources/OCR/embedders/vision; the P3 end-to-end tests
  (`crates/kernel/tests/enrichment.rs`) drains a real job and proves the vector arm via a
  non-FTS-matching query, and prove that `vision_tag` jobs drain when inference attaches without an
  embedder; the P4 `vision_tag` routing tests drive `process_job` with a fake `VisionProvider`
  (writes the analysis; retries with no provider). Store search tests cover the backend
  `SearchQuery.limit` clamp. PR5 adds a Tauri config guard proving the overlay window is pre-created,
  hidden by default, and `contentProtected`. PR6 adds the `kernel::resume` heuristic unit suite, the
  v10 marks migration test, and `capture_now` pipeline tests (diff-gate bypass, capture-off denial).
  PR7 adds `crates/api/tests/http_api.rs` (fixture-DB integration: originally a loopback-only
  bind assertion, 401s, every endpoint, SSE ask, export). The current network increment replaces
  that bind assertion with configurable IPv4/IPv6 binding, safe upgrade defaults and loopback-client checks, and extends route-auth coverage. PR8 adds `crates/mcp/tests/stdio_mcp.rs`, which spawns the
  real compiled binary over stdio (handshake, nine tools, guided API-off/wrong-token errors).
- **Inference, deterministic (run in CI, no GPU/network):** the **no-orphan gate**
  (`tests/no_orphan.rs` — kill a parent, assert the Job-Object child dies), startup **reap**
  (`tests/reap.rs` — reaps a matching stray, never a foreign pid), the HTTP **client** against a
  `wiremock` sidecar (`tests/sidecar_client.rs` — vision parse + ordered SSE deltas), and the pure
  logic (model/asset selection, `ThinkSplitter`, vision JSON parse, supervisor decisions).
- **`#[ignore]`d (local / hardware / model-backed):** WGC + WinRT OCR smoke (`cfg(windows)`), the
  real-model embedding test (`-p embeddings`), the 10k-frame perf fixture (`-p store --test perf`),
  and the **real-llama-server smoke** (`cargo test -p inference --test smoke -- --ignored` — downloads
  a Vulkan build + GGUFs and runs a real vision tag + streamed answer on the GPU).
- **Gates:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test`, the UI build, and a `ts-rs` binding-drift guard — all on `windows-latest` (`03 §11`).

---

## 12. Deferred — packaging and polish

P4 completed the inference sidecar (vision tagging + grounded `ask`) and P5 completed the Command
Deck UI; the no-orphan gate passes. What remains for v1.0:

- **Packaging** — shipped an unsigned **NSIS** installer (v0.1.0; `bundle.targets=["nsis"]`).
  Inno/MSI/portable ZIP were dropped (Tauri 2 ships NSIS natively — `07` #26). `onnxruntime.dll`
  bundling is **moot** — `ort` static-links ONNX Runtime into the exe. Remaining: **code-signing**
  (see `07` — SignPath Foundation / Azure Trusted Signing / Certum). The `llama-server` binary and
  GGUF models are *not* bundled — they download at runtime.
- **Polish carried from P4** (`07` #19): a download-progress %% in `sidecar` readiness and optional
  pending-job dedup for the vision scheduler. Multi-GPU device selection is now available through
  `list_sidecar_devices` + `sidecar.device`.

---

*Pointers:* design rationale → [`specs/03_MASTER_PRODUCTION_SPEC.md`](../specs/03_MASTER_PRODUCTION_SPEC.md) ·
phase plan → [`specs/02_STRATEGIC_PLAN.md`](../specs/02_STRATEGIC_PLAN.md) ·
open decisions/gaps → [`specs/07_KNOWN_GAPS.md`](../specs/07_KNOWN_GAPS.md) ·
build records → [`specs/05_BUILD_REVIEW.md`](../specs/05_BUILD_REVIEW.md) ·
model pins → [`specs/MODEL_REGISTRY.md`](../specs/MODEL_REGISTRY.md).
