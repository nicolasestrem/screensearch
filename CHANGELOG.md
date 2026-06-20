# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Fixed
- **Vision "Auto-select" could pick a `thinking` model, so every frame failed with
  "Failed to parse VisionAnalysis JSON".** When `vision_model` was empty/Auto,
  `resolve_vision_model`'s fallback returned the first discovered vision pair, which
  on a box with a `*-thinking` build (e.g. `Qwen3VL-4B-Thinking`) sitting first was a
  reasoning model — it emits chain-of-thought, not the strict JSON the vision worker
  parses, so analysis failed on every frame. Auto-select now skips `thinking`/`action`
  variants and prefers a vanilla instruct build (only falling back to a specialized
  build if that is all that exists); naming one explicitly in `vision_model` still
  honors the override. File: `screensearch-llm/src/download.rs`.
- **Vision crashed with the local provider when an unrelated text model sat beside
  the projector (`llama-server exit code 1`).** `resolve_mmproj_for` paired a model
  to a multimodal projector by size token alone, so a text-only model like
  `NVIDIA-Nemotron3-Nano-4B` was matched with the Qwen3-VL **4B** `mmproj` —
  llama.cpp then aborted on the embedding-dim mismatch (`n_embd 3136 ≠ 10240`) in
  both GPU and CPU modes. Pairing now also requires a **same-family** match (shared
  family token, robust to `qwen3-vl` vs `qwen3vl` hyphenation), so only a real
  vision model takes its own projector. This also cleans up `GET /api/vision/models`,
  which no longer lists non-vision models (Nemotron, etc.) as selectable.
  File: `screensearch-llm/src/download.rs`.
- **Semantic indexing never progressed for frames with long OCR (and
  `POST /api/embeddings/generate` failed outright).** The quantized text embedder
  (`EmbeddingGemma300MQ`) errors on any multi-input `embed` call — *"Dynamic
  quantization cannot be used with batching"* — which aborted whole batches and left
  multi-chunk frames permanently un-indexed. The engine now embeds **one input at a
  time** under a single model lock, so coverage climbs to 100% regardless of OCR
  length. File: `screensearch-embeddings/src/engine.rs`.

### Added
- **Pick the local answer-generation model (and use it for Reports).** A new answer-
  model pin lets you choose which discovered GGUF the unified llama-server runs for
  "Ask" answers and local Reports (e.g. a dedicated text model like
  `NVIDIA-Nemotron3-Nano-4B`) instead of always auto-selecting. New
  `POST /api/ai/model/select` (and a `selected` field on `GET /api/ai/model/status`)
  persist the choice as the `answer_model` setting; the server rebuilds onto it on
  next use. The AI-provider settings panel gains a **Local engine / Remote** choice
  so Reports can run on the local server, and the "Local answer engine" panel gains
  a model dropdown. (When vision is enabled the unified server runs the vision model,
  so the pin applies to the text-only server.) New `resolve_answer_model`. Files:
  `screensearch-llm/src/{download,lib}.rs`, `screensearch-api/src/state.rs`,
  `screensearch-api/src/handlers/ai.rs`, `screensearch-api/src/routes.rs`,
  `screensearch-ui/src/pages/Settings.tsx`,
  `screensearch-ui/src/lib/{api,hooks,types}.ts`.
- **Vision settings: clearer local-model guidance + nearer server download.** When
  the local provider has no vision-capable model, the panel explains that a vision
  GGUF **and** its matching `*mmproj*.gguf` (same family) must be dropped into
  `.models/`; the "Download llama-server" action is now also surfaced in the Vision
  panel (it shares the local server). File: `screensearch-ui/src/pages/Settings.tsx`.

### Investigated (no code change)
- **POC: direct screenshot embedding for visual recall — GO (via fastembed), not via
  llama.cpp.** Spiked whether screenshots could be embedded so non-OCR visual content
  (icons, charts, canvases) becomes searchable.
  - **Qwen3-VL-Embedding-2B on the pinned llama.cpp `b9728` server: NO-GO.** Image
    data is **silently ignored** by `llama-server`'s `/embedding` endpoint — two
    different images produce byte-identical vectors (`cos = 1.0`), reproducing upstream
    issue #19525; the feature was never merged upstream (PR #18665 closed). Text
    embedding works but the 2B is a net regression for text-only (~5 GB VRAM + GPU
    contention vs the in-process CPU EmbeddingGemma-300M).
  - **In-process fastembed image embeddings: GO (proven).** `nomic-embed-vision-v1.5`
    (screenshots) + `nomic-embed-text-v1.5` (queries), both **768-dim** (drop-in to the
    existing schema), ONNX/CPU, no Python/llama.cpp/GPU. Test: **3/3 text queries
    retrieved the correct screenshot by pixels, including a textless bar chart** with
    no OCR recall path. Recommended rollout: Tier 1 — embed the existing vision
    `description` (zero new models); Tier 2 — add the fastembed nomic image index,
    fused with OCR results via RRF. Full findings + verbatim evidence:
    `docs/qwen3vl-embedding-poc.md`.

### Added
- **Visual recall, Tier 2: optional in-process image-embedding index.** Screenshots
  are now embedded directly from pixels with `nomic-embed-vision-v1.5` (768-dim) into a
  separate sqlite-vec index, and `hybrid` search fuses image hits in via Reciprocal
  Rank Fusion (queries encoded with the aligned `nomic-embed-text-v1.5`). This catches
  non-OCR visual content the vision description misses — charts, dense canvases,
  icon-only screens. Runs fully in-process (fastembed/ONNX, CPU); no Python, llama.cpp,
  or GPU. **Off by default** (`embeddings.image_enabled`, also toggleable at runtime via
  `POST /api/embeddings/image/enable`); the nomic models download on first use and the
  feature enables fastembed's `image-models` codecs (longer build). New: migration 013
  (`image_embeddings` + `image_embedding_vectors` + contract metadata),
  `ImageEmbeddingEngine`, `image_embedding_worker` (always spawned, lazy-loads only when
  enabled), `hybrid_search` image fusion, and `GET/POST /api/embeddings/image/{status,
  generate,enable}`. Proven 3/3: text queries retrieve the matching screenshot
  (including a textless bar chart) via the shipping engine. Files:
  `screensearch-embeddings/src/image_engine.rs`, `screensearch-db/src/{migrations,queries,vector_search}.rs`,
  `screensearch-api/src/{state,server,routes}.rs`,
  `screensearch-api/src/workers/image_embedding_worker.rs`,
  `screensearch-api/src/handlers/{embeddings,search}.rs`, `src/main.rs`.

- **Visual recall, Tier 1: the vision description is now embedded.** The generative
  vision worker's per-frame `description` + `visible_text` labels are now folded into
  each frame's text embedding (previously they were stored as metadata and never
  embedded), giving non-OCR visual content — charts, design canvases, icon-heavy UIs —
  a semantic recall path through the existing 768-dim EmbeddingGemma index. No new
  models, dependencies, or schema changes. Frames are now embeddable when they have
  OCR text **or** vision text, so a frame with no OCR at all (a pure chart/canvas) is
  embedded once vision describes it. Because vision analysis is asynchronous and may
  finish after a frame was first embedded on OCR alone, `complete_analysis_task` now
  clears that frame's embeddings when it records vision text (a non-empty `description`
  or `visible_text` labels), so the background worker re-embeds it with the vision
  fields included. The combined embedding text is built by a single shared
  `build_frame_embedding_text` helper used by both the background worker and the manual
  generate-embeddings handler. Files: `screensearch-api/src/workers/embedding_worker.rs`,
  `screensearch-api/src/handlers/embeddings.rs`, `screensearch-db/src/queries.rs`.

- **Complete frontend rebuild — "Command Deck" UI (greenfield).** The React UI was
  rebuilt from scratch with a new, deliberately non-generic visual identity:
  a warm-graphite + signal-orange instrument/telemetry aesthetic, Windows-native
  type pairing (Bahnschrift display / Consolas data / Segoe UI prose — no web-font
  downloads), and a real type/contrast scale (all text ≥ WCAG AA). Stack: Vite +
  React 18 + TypeScript + Tailwind, now with **real client-side routing**
  (`react-router-dom`), TanStack Query, a typed `fetch` client (dropped `axios`
  and `framer-motion`), and Zustand for ephemeral UI state. Files:
  `screensearch-ui/src/**` (entire tree replaced).
  - **Signature component — the Scanline Timeline** (`components/ScanlineTimeline.tsx`):
    a 24-hour track with a frame-density ribbon, vision **activity-type colour
    bands**, a live "now" line, and a draggable playhead to scrub the day. Compact
    on the Deck, full-width and interactive on the Timeline.
  - **Deck** (`/`) — mission-control overview: always-on status rail (capture
    state, frame count, span, GPU/Vulkan acceleration, index & vision coverage),
    an Ask box, real index coverage, the timeline, a live activity feed, and a
    real apps & sites breakdown. Every figure is wired to a live endpoint.
  - **Recall** (`/recall`) — *Ask your screen* (RAG Q&A) as a first-class feature:
    grounded answers from `POST /api/generate` with clickable **cited frame
    chips**; plus a daily/weekly/custom **Report** mode (`POST /api/ai/generate`).
  - **Timeline** (`/timeline`) + **Moment** (`/timeline/:id`) — interactive
    scanline + searchable, filterable contact sheet (date, app, monitor, activity,
    search mode fts/semantic/hybrid); deep-linkable per-frame detail with the full
    screenshot, vision panel, OCR text, metadata, tags, and on-demand re-analysis.
  - **Insights** (`/insights`) — real analytics from captured frames: activity
    mix, top apps & sites (incl. per-site via `browser_url`), and an hourly rhythm
    chart. Custom SVG/markup, no charting dependency.
  - **Settings** (`/settings`) — capture interval/retention/pause, monitor & excluded-app
    selection, semantic-search enable/index/model controls, vision provider/model
    picker, AI provider config + connection test, and the local answer-engine
    model/server controls with live download progress.
  - **⌘K command palette** (search + ask + navigation) and a real **readiness
    banner** driven by `GET /api/system/readiness` (replaces the old startup mock).

### Fixed
- **Local answer engine no longer mis-selects the wrong GGUF.** With a populated
  `.models/`, the answer-generation server picked the wrong model: a user-supplied
  embedding GGUF (e.g. `Qwen.Qwen3-VL-Embedding-2B.f16`) sorted first and was
  loaded as the answer model (embeddings can't generate text), and after excluding
  it the alphabetically-first remaining file was a slow `*-thinking` build.
  `is_loadable_model_gguf` now rejects `embed`-named GGUFs, and `resolve_model_path`
  scores candidates to prefer a vanilla `instruct` build while penalising
  `thinking`/`action` variants (mirroring the vision resolver). `get_model_status`
  also now reports the model the server will actually load (and its real size)
  rather than the first discovered file. Files: `screensearch-llm/src/download.rs`,
  `screensearch-api/src/handlers/ai.rs`.
- **Command Deck review fixes (PR #73).** Addressed the automated review feedback:
  - **Restored a render error boundary.** A new Command-Deck-styled
    `components/ErrorBoundary.tsx` wraps the routed page outlet in `AppShell.tsx`
    (keyed by route so navigation clears a faulted page), so a single page error
    shows a recovery panel instead of a blank screen.
  - **Bounded the frame-image cache.** `getFrameImage` (`lib/api.ts`) now uses a
    100-entry LRU and revokes evicted object URLs, preventing unbounded memory
    growth during long scrubbing sessions.
  - **Command-palette questions re-fire.** Removed a one-shot `askedOnce` guard in
    `Recall.tsx` that blocked a second seeded question from the ⌘K palette; the seed
    is cleared inside the effect so it still runs exactly once per submission.
  - **Settings no longer lose unsaved edits.** The form populates once via an
    `initialized` flag so a background settings refetch can't clobber in-progress
    edits; the shadowing `interval`/`setInterval` state was renamed to
    `captureInterval`/`setCaptureInterval`.
  - **Cleanups:** deferred `URL.revokeObjectURL` after report download, and removed
    all three `react-hooks/exhaustive-deps` suppressions (`Recall`, `Timeline`,
    `CommandPalette`) in favour of stable refs/`useCallback`/memoised deps.

### Changed
- **Frontend now surfaces previously-unused backend data.** `FrameResponse`
  (`screensearch-api/src/models.rs`) now passes through `activity_type`, `app_hint`,
  `browser_url`, and `monitor_index` from the DB record so the timeline, insights,
  and moment views can show vision activity, sites, and per-monitor filtering. No
  new endpoints or migrations.
- **Removed all placeholder UI.** Every "Coming Soon" card and mock/synthetic data
  generator (the old daily-digest sample text, the faked memory gauge, the
  synthetic productivity chart) is gone; all surfaces render real data or honest
  empty/loading/error states.
- **On-device vision is ~6–10× faster: default switched to Qwen3-VL-4B-Instruct
  and the frame pipeline was tuned.** Per-frame analysis dropped from **5–10 s** to
  **~1 s** on an RTX 5060 Ti (measured: image encode ~320 ms + generation ~770 ms,
  valid JSON every frame). The bottleneck was the vision *encoder* re-slicing
  large ultrawide screenshots (not token generation), so the fix is several small
  changes that compound:
  - **Default vision model is now `Qwen3-VL-4B-Instruct`** (was Gemma 4 E4B) — a
    lighter vision encoder and faster decode, strong at UI/screen content. New DB
    migration `012_qwen3vl_vision_default` migrates rows still on the `gemma-4-E4B`
    / legacy `ministral-3:3b` default; a deliberately-chosen model is left
    untouched. `resolve_vision_model` now prefers the vanilla *Instruct* build and
    excludes `*-thinking` (slow, chain-of-thought) and third-party `*-action`
    fine-tunes from a generic match unless explicitly requested. Drop
    `Qwen3VL-4B-Instruct` GGUF + its `mmproj-Qwen3VL-4B-Instruct-*.gguf` into
    `.models/` (Gemma 4 still works if you prefer it). File:
    `screensearch-llm/src/download.rs`, `screensearch-db/src/migrations.rs`.
  - **Captures are downscaled at the source** — `storage.max_width` default lowered
    `1920 → 1280`. OCR already runs on the full-resolution frame *before* this
    resize, so text accuracy is unaffected; the vision worker re-decodes the stored
    JPEG, so this directly cuts both disk use and the vision encoder's per-frame
    cost on 3440×1440 ultrawide / multi-monitor captures. File: `src/main.rs`.
  - **llama-server vision flags:** the unified server now launches with
    `--image-max-tokens 1024` (bounds vision-encoder cost for dynamic-resolution
    models) and `--flash-attn on` when a projector is loaded. File:
    `screensearch-llm/src/server.rs`.
  - **Bounded, terser vision output:** the analysis request now sends a
    `max_tokens` cap (512, with headroom so output always closes as valid JSON)
    and a tightened prompt that no longer asks the model to
    transcribe on-screen text (native OCR already captures it), so responses stay
    compact and always close as valid JSON. Also fixes the response key to
    `app_hint` (was `application`, which silently never populated). File:
    `screensearch-vision/src/client.rs`.

### Fixed
- **Settings AI card no longer hangs on its loading skeleton.** `GET
  /embeddings/status`'s cheap `engine_ready` probe used a blocking read lock that
  stalled for the entire first-run embedding-model download (~450 MB) because
  `get_embedding_engine` holds the write lock across init. It now uses `try_read`
  and reports "not ready" while initializing instead of blocking. A defensive 8s
  fetch timeout in the UI also prevents the card from getting stuck. Files:
  `screensearch-api/src/state.rs`, `screensearch-ui/src/components/EmbeddingsStatus.tsx`.
- **`database is locked` errors under concurrent writes.** Added a SQLite
  `busy_timeout` (10s) so a writer (e.g. the vision worker storing an analysis)
  waits for the lock instead of erroring when the capture/OCR pipeline is also
  writing. File: `screensearch-db/src/db.rs`.
- **Answer Generation no longer spams `image input is not supported`.** The local
  provider previously defaulted to the **text-only** Ministral-3B (`vision_model =
  'ministral-3:3b'`), so enabling vision sent images to a server with no
  multimodal projector and every frame failed. The default is now **Gemma 4 E4B**
  (multimodal): a new DB migration (`011_gemma4_vision_default`) migrates existing
  `ministral-3:3b` rows, and the Settings/AI UI defaults and labels were updated
  off Ministral. Drop your Gemma 4 GGUF(s) + `*mmproj*.gguf` into `.models/` and
  pick one in Settings → AI.

### Added
- **Startup readiness banner.** A new non-blocking banner explains, in plain
  language, what the backend is doing while it warms up and roughly how long is
  left — core services (DB/capture/OCR/search), the semantic-search model, and
  local AI answer generation (server/model download + load, with GPU/CPU shown).
  It only appears when there is actual warm-up to report (a fully-cached launch
  shows nothing) and auto-dismisses once everything is ready. Backed by a new
  `GET /api/system/readiness` aggregator. Files:
  `screensearch-api/src/handlers/system.rs`, `routes.rs`,
  `screensearch-ui/src/components/StartupStatus.tsx`, `App.tsx`.
- **Vision model picker** — `GET /api/vision/models` lists the locally discovered
  `(model, mmproj)` pairs, and Settings → AI shows a dropdown to choose which one
  the unified local server loads (writes the `vision_model` setting; the server
  rebuilds on the next request). Files: `screensearch-api/src/handlers/vision.rs`,
  `routes.rs`, `screensearch-ui/src/components/SettingsPanel.tsx`.
- **GPU acceleration is now visible.** `GET /api/ai/server/status` returns an
  `acceleration` field (`gpu` / `cpu` / `unknown`) and Settings → AI shows whether
  the local server is running on GPU (Vulkan) or fell back to CPU. llama-server's
  stdout/stderr are captured to `bin/llama-server.log` (previously discarded), so
  a Vulkan init failure is diagnosable. File: `screensearch-llm/src/server.rs`,
  `screensearch-api/src/handlers/ai.rs`.

### Changed
- **Better local model selection.** `resolve_vision_model()` now prefers a
  higher-quality quantization within a size tier (e.g. Q4 over Q2), and model
  discovery skips non-loadable GGUFs (multimodal projectors, `mtp-*` helper heads,
  and files below a size floor) so they're never loaded as the generation/vision
  model. File: `screensearch-llm/src/download.rs`.
- **GPU health-check timeout scales with model size** (base + per-GB, capped)
  instead of a fixed 45 s, so a multi-GB model loading into VRAM is no longer
  mistaken for a GPU failure and silently bounced to CPU. File:
  `screensearch-llm/src/server.rs`.
- **"Process frames" (embeddings indexing) no longer blocks.** `POST
  /api/embeddings/generate` starts the CPU-bound work on a background task and
  returns immediately; the UI polls `GET /api/embeddings/status` so the coverage
  bar climbs live. Chunks are batched across frames (bounded per call) and the
  fastembed batch size was raised 16 → 32 for better throughput. Files:
  `screensearch-api/src/handlers/embeddings.rs`,
  `screensearch-embeddings/src/lib.rs`,
  `screensearch-ui/src/components/EmbeddingsStatus.tsx`.
  (Note: embeddings still run on ONNX Runtime CPU; GPU execution providers are
  future work.)
- **UI rename:** the "ScreenSearch Intel" / "Intelligence" surfaces are now
  branded **"Insights"** (dashboard title, reports page/header, search footer,
  answer card, settings section; report files are saved as `insights-report-*`).

### Added
- **On-device vision (screen understanding) via the unified local llama-server.**
  When vision is enabled with the `local` provider, the same auto-managed
  llama.cpp server that answers AI reports is now launched with `--mmproj` so a
  single Gemma&nbsp;4 model serves **both** text generation and image analysis
  (Option B — unify on gemma-4). The vision worker analyzes frames against this
  local server's OpenAI-compatible `/v1/chat/completions` endpoint (no external
  Ollama/OpenAI needed) and populates each frame's `description`,
  `visible_text`, `activity_type`, `app_hint`, and `confidence`.
  - **Model/projector auto-discovery**: `resolve_mmproj_for()` pairs a model with
    the correct `*mmproj*.gguf` projector beside it by size signature (e.g. an
    E4B model gets the E4B projector, a 12B model the 12B projector; a text-only
    model like Qwen3.5 is never mis-paired). `resolve_vision_model()` picks the
    unified vision model, preferring a Gemma&nbsp;4 **E4B**. Drop a Gemma&nbsp;4
    GGUF and its `*mmproj*.gguf` into `.models/`.
  - **Enqueue is on-demand + throttled**: `POST /api/vision/analyze/:frame_id`
    queues a single frame at high priority; the worker also trickles in a small
    batch (4) of recent un-analyzed frames per idle cycle to keep GPU load
    bounded. `GET /api/vision/status` reports per-status frame counts and queue
    depth.
  - The unified server is rebuilt automatically when vision is toggled (it
    switches the loaded model to the vision model + projector and back).
  - Files: `screensearch-llm/src/{server.rs,download.rs}`,
    `screensearch-api/src/{state.rs,server.rs,routes.rs,handlers/vision.rs,
    workers/vision_worker.rs}`, `screensearch-db/src/{queries.rs,models.rs}`.
- **Bundled `onnxruntime.dll` (ONNX Runtime 1.24.2, x64) in all Windows
  artifacts**, so a fresh install has working semantic search out of the box. The
  in-process fastembed engine loads ONNX Runtime dynamically and looks for the DLL
  beside `screensearch.exe`; a new `scripts/fetch-onnxruntime.ps1` helper
  downloads and checksum-verifies the pinned version and stages it into
  `target/release`. It is now shipped by the installer (`installer/screensearch.iss`),
  the portable ZIP, `scripts/build-release.ps1`, and `scripts/build-local.ps1`, and
  acquired in CI (`.github/workflows/release.yml`). Validated end-to-end on
  Windows: native WinRT OCR, 768-dim EmbeddingGemma embeddings
  (`provider":"fastembed"`, `engine_ready":true`), semantic + hybrid (RRF) search,
  and local llama.cpp answer generation. Without the DLL the app still runs with
  OCR + keyword search (semantic degrades to FTS5).

### Changed
- **Bumped the pinned llama.cpp build from `b7562` to `b9728`** so the bundled
  answer-generation server can load current model architectures (e.g. `gemma4`,
  `qwen35`); older builds failed with "unknown model architecture". The installed
  build is now recorded in `bin/llama-version.txt` and an existing install
  **re-downloads** llama-server when the pin changes (previously the version-less
  presence check kept a stale binary). The server is launched with `--jinja` so it
  uses each model's embedded chat template, which is required for correct
  formatting of modern instruct models and surfaces reasoning/"thinking" output
  from models like Qwen3.5 / Gemma 4. To make reasoning models usable, the
  default context window was raised from 8192 to 16384 (chain-of-thought was
  filling the 8k window and truncating the answer to empty), the AI report now
  reports the **actual** resolved model name instead of a hardcoded
  "ministral-3b", and an empty answer (model spent the whole window thinking) now
  yields a clear note instead of a blank report. (`screensearch-llm/src/download.rs`,
  `server.rs`, `config.rs`; `screensearch-api/src/handlers/ai.rs`)
- **Reverted PR #63 (Python "quality sidecar") and restored a fully in-process
  Rust ML stack.** The sidecar ran PaddleOCR/Qwen3 over HTTP and made the app
  slow and unusable (OCR on the asyncio event loop took tens of seconds/frame,
  plus a 15-45 s PyInstaller cold start). Everything now runs in-process:
  - **OCR** reverts to the native Windows OCR (WinRT) API — ~70-80 ms/frame, no
    model download, no sidecar.
  - **Embeddings** run in-process via `fastembed` (ONNX Runtime) using
    **EmbeddingGemma-300M (768-dim)**, downloaded and cached from HuggingFace on
    first use. Replaces the Qwen3-Embedding-0.6B (1024-dim) sidecar contract.
  - **Reranking** is optional (`fastembed` cross-encoder, off by default);
    retrieval relies on sqlite-vec KNN + Reciprocal Rank Fusion, both retained
    from #63.
  - The sqlite-vec `embedding_vectors` contract changes from `float[1024]` to
    `float[768]`. No migration is provided (the embeddings table is rebuilt).
  - ONNX Runtime is loaded **dynamically** (`ort`'s `load-dynamic`): the app
    finds `onnxruntime.dll` next to the executable (or via `ORT_DYLIB_PATH` /
    the system path). This keeps the portable binary linkable under the repo's
    static-CRT + lld build. If the DLL is absent, embeddings degrade gracefully
    (semantic search falls back to FTS5); OCR and keyword search are unaffected.
    Windows release artifacts must ship a matching `onnxruntime.dll`.
- **Answer-generation LLM is now model-agnostic.** The bundled llama.cpp server
  auto-discovers any `*.gguf` dropped into `.models/` (or the app models dir)
  and uses the first one found, instead of hardcoding Ministral-3B. The default
  download remains available as a fallback. `GET /api/ai/model/status` now lists
  discovered models via `available_models`.

### Fixed
- **Vision worker no longer risks a tight infinite loop / CPU pegging on a
  failing frame.** Previously `fail_analysis_task` unlocked the task
  (`locked_until = NULL`) without removing it, so the worker re-claimed and
  re-failed the same task with zero delay — pegging a CPU core and flooding the
  log. Failures now apply exponential backoff (1→2→4 min) and the task is dropped
  from the queue after `MAX_ANALYSIS_ATTEMPTS` (3) tries; the frame stays marked
  `failed` for manual retry.
- **Frame-load and image-decode errors in the vision worker are now marked
  failed instead of bubbling up.** A missing frame, corrupt screenshot, or
  serialization error previously returned early via `?`, skipping
  `fail_analysis_task` and leaving the task locked for 5 minutes only to fail
  again. They are now logged, recorded as failed, and the worker continues. The
  worker also cools down briefly after a failure.
- **`get_vision_status` now does a single scan of `frames`** via conditional
  aggregation instead of five sequential `COUNT(*)` round-trips.

### Removed
- The Python sidecar (`sidecar/`), its PyInstaller packaging, and all sidecar
  process management, build steps, and bundling from `main.rs`, the build
  scripts, the installer, and the CI release workflow.
- `POST /api/embeddings/models/prepare` now pre-loads the in-process embedding
  model (instead of driving sidecar model preparation); the `sidecar_ready` /
  `model_preparation` fields of `GET /api/embeddings/status` are replaced by a
  single `engine_ready` flag.

## [0.4.37] - 2026-06-16

### Added
- `GET /api/monitors` enumerates connected displays (index, label, resolution,
  primary flag) from the same `screenshots::Screen::all()` source the capture
  engine indexes, so reported indices match what is actually captured.
- The Settings → Capture **Monitors** picker is now a real multi-select of the
  detected monitors. Selecting/deselecting monitors reconfigures the running
  capture engine live (no restart) via a `tokio::sync::watch` channel, and the
  selection is read back from the database at startup so it survives restarts.
- GPU-accelerated OCR via `scripts/build-release.ps1 -Gpu`. OCR is the throughput
  bottleneck — ~60 s/frame on CPU versus ~1.5 s on a CUDA GPU. The GPU build uses
  `paddlepaddle-gpu` (CUDA 12.9, supports Blackwell / compute capability 12.0) and
  `sidecar/requirements-gpu.txt`; `sidecar/build.py` auto-detects the CUDA build
  and bundles paddle's NVIDIA runtime DLLs. Embeddings/reranking remain on CPU
  torch (torch and paddle cannot share a CUDA runtime that also targets Blackwell
  in one process — `WinError 127`); `paddle_device()` falls back to CPU OCR when
  no GPU is present. `/health` now reports `ocr_device` alongside `device`.

### Fixed
- PR #63 review follow-up:
  - `AppState::get_embedding_engine` no longer races on first use. The fast-path
    read lock is retained, but initialization now happens under the write lock
    with a second `is_some` check, so concurrent first callers serialize instead
    of building and health-checking duplicate engines.
  - Hybrid search no longer reports a bogus `chunk_index` for FTS-only hits. The
    FTS match position is not an embedding chunk boundary, so FTS-only results now
    use `-1` as an "unknown chunk" sentinel instead of the match index.
  - The sidecar OCR client per-attempt timeout dropped from 120 s to 30 s so a
    hung sidecar can no longer stall a capture worker for minutes across the retry
    sequence before the Windows OCR fallback engages.
  - Sidecar model-preparation error logging now reads the failing component from a
    local variable instead of the shared `_preparation_status` outside its lock.
  - The `screensearch-api` integration test was updated for the new
    `ApiServer::new(..., monitor_config_tx)` signature so the test target compiles.
  - The `rust-audit` CI job (`cargo audit`) now passes. It failed on
    RUSTSEC-2023-0071 (`rsa` "Marvin Attack"), but `rsa` is never compiled — it
    is a lockfile-only artifact of `sqlx`'s optional `sqlx-mysql` backend, which
    we do not enable (`cargo tree -i rsa` is empty on every target). A documented
    `.cargo/audit.toml` ignores that single advisory; the remaining entries are
    "allowed" unmaintained/unsound warnings that do not fail the job.
  - The `rust` CI job's formatting step no longer references a deleted file. Its
    curated rustfmt file list still named `screensearch-embeddings/src/chunker.rs`,
    which this PR removed when chunking moved to the sidecar, so the step errored
    with "file does not exist" before clippy/tests could run. The stale path is
    dropped; the remaining 22 files are rustfmt-clean.
  - The two open `esbuild` Dependabot alerts (GHSA-gv7w-rqvm-qjhr high,
    GHSA-g7r4-m6w7-qqqr low; both `< 0.28.1`) are eliminated on this branch: the
    Vite 8 upgrade makes `esbuild` an optional peer dependency that is no longer
    installed (`npm ls esbuild` is empty, `npm audit` reports 0 vulnerabilities),
    so the vulnerable `esbuild` 0.27.7 still present on `main` is gone. The alerts
    will auto-dismiss once this PR merges.
- Capture no longer stops after a single frame. `FrameDiffer` defaulted to the
  histogram method, but the `0.006` threshold is calibrated for the pixel method
  ("0.6% pixel change"); the histogram's chi-squared-over-pixel-count value sits
  far below that scale, so once screen content was visually stable nearly every
  frame after the first was judged "unchanged" and skipped. The differ now
  defaults to `DiffMethod::Pixel` (configurable via `capture.diff_method`).
  Verified: a content-swap unit test where the histogram method misses the change
  and the pixel method detects it, plus a live run capturing continuously from
  both monitors.
- OCR failures no longer discard the captured screenshot. When the sidecar reset
  the connection (`WinError 10054`) the frame was logged and dropped, losing it
  from history; the frame is now stored with an empty OCR result through the
  existing `store_empty_frames` gate so the screenshot is preserved.
- The Monitors setting now actually drives capture. Previously it was written to
  the database but never applied to the running engine, and the frontend's "All
  Monitors" option sent `[0]` — which the backend reads as *only monitor 0*
  (empty `[]` means all monitors).
- The quality sidecar no longer freezes while OCR runs. The `/v1/ocr` route was
  declared `async` but called the blocking, CPU-bound PaddleOCR predict directly
  on the asyncio event loop, stalling every other route (`/health`,
  `/v1/models/status`) for the entire OCR duration. With OCR taking tens of
  seconds, this left the Settings → Data & AI panel stuck on its loading
  skeleton because `/api/embeddings/status` (which proxies to
  `/v1/models/status`) was queued behind the in-flight OCR. Model loading and
  inference now run via `run_in_threadpool`, so the sidecar stays responsive
  (verified: status returned HTTP 200 at ~234 ms while a multi-second OCR was in
  flight).
- `EmbeddingsStatus` now shows a retryable error state instead of an indefinite
  blank when `/api/embeddings/status` fails or times out.

### Changed
- OCR frames are downscaled to a 2000 px longest side before being sent to the
  sidecar (`screensearch-capture/src/sidecar_ocr.rs`), with returned boxes mapped
  back to original-frame coordinates. On a 3440×1440 ultrawide frame this cut
  PP-OCRv5 inference time ~3.2× (88.7 s → 27.6 s in a warm contended benchmark)
  with no change to stored frame resolution.
- MKLDNN/oneDNN acceleration is explicitly kept disabled for PP-OCRv5: enabling
  it crashes detection under PaddleOCR 3.x's PIR executor
  (`ConvertPirAttribute2RuntimeAttribute not support`).
- Sidecar startup is now non-blocking. `ensure_quality_sidecar` previously
  blocked the whole launch for up to 60 s polling `/health`; because the
  PyInstaller-bundled Torch/Paddle sidecar cold-starts in ~15-45 s, the API and
  UI were unusable for that long. The sidecar is now spawned and its readiness
  polled in a background task, so the API/UI come up in ~2 s while it warms up
  (`src/main.rs`).
- OCR no longer gets permanently demoted to Windows OCR at startup. With the
  non-blocking launch the sidecar is usually still cold-starting when the OCR
  provider initializes; `OcrProviderEngine::new` no longer gates on an initial
  health check, so PP-OCRv5 stays the preferred provider and frames fall back to
  Windows OCR per request only until the sidecar is healthy — then upgrade to
  PP-OCRv5 automatically with no restart (`screensearch-capture/src/ocr_provider.rs`).
- `sidecar/build.py` refuses to build under Python < 3.12.1. Python 3.12.0 has a
  PEP 709 (inlined-comprehension) codegen bug that, once frozen by PyInstaller,
  makes the bundled sidecar crash at startup importing scipy
  (`NameError: name 'obj' is not defined` in `scipy/stats/_distn_infrastructure.py`),
  which silently degraded OCR to the Windows fallback. Verified by bisection:
  same scipy 1.17.1 + PyInstaller 6.21.0, only the interpreter changed — 3.12.0
  fails, 3.12.10 works.
- `scripts/build-release.ps1` now builds on a native Windows host: it imports the
  MSVC environment and overrides the linker (the `.cargo/config.toml` `lld` linker
  is cross-compile-only), detects Inno Setup 6 or 7, and takes a `-PythonExe`
  parameter so the sidecar can be built with a specific Python 3.12.x.
- Toggling a monitor no longer appears to freeze capture. The capture-drain loop
  used a blocking `frame_tx.send().await`, which parked the whole select loop
  whenever OCR was backed up (the channel fills because a frame can take tens of
  seconds on CPU) — so monitor reconfiguration and shutdown could not be processed.
  It now uses `try_send` and sheds surplus frames via the bounded capture queue,
  keeping reconfiguration responsive under OCR backpressure (`src/main.rs`).
- The PP-OCRv5 document- and textline-orientation classifiers are disabled. Screen
  captures are upright, so they were pure overhead (the textline classifier ran
  once per detected line — dozens to hundreds per frame).

### Removed
- Dropped the dead ONNX-era `[embeddings]` config keys (`model`, `model_name`,
  `embedding_dim`, `max_chunk_tokens`, `chunk_overlap`, `hybrid_search_alpha`,
  `max_context_chunks`) from `EmbeddingsSettings` and `config.toml`. The model
  identity, dimension, chunking, and RRF retrieval are owned by the sidecar
  contract; only `enabled` and `batch_size` remain. Older config files with the
  removed keys still load (no `deny_unknown_fields`).
- Deleted the unused in-Rust `TextChunker` (`screensearch-embeddings/src/chunker.rs`);
  chunking is delegated to the sidecar `/v1/chunk` endpoint.

## [0.4.36] - 2026-06-15

### Changed
- Replaced legacy OCR and retrieval choices with the fixed local PP-OCRv5,
  Qwen3 embedding, Qwen3 reranking, sqlite-vec, and RRF quality stack.
- Added Linux-first local and release build scripts while retaining PowerShell
  helpers for Windows maintainers.
- Added packaged Windows sidecar validation that performs real PP-OCRv5
  inference before installer and portable artifacts are published.
- Split the release workflow's sidecar build into staged steps (pip upgrade,
  dependency install, PyInstaller bundle) so `--windows-bundle` watchers see
  progress instead of one ~7.5 min step that appears to hang, and added a
  heads-up message before `gh run watch`.

### Fixed
- The packaged Windows sidecar no longer crashes on launch with
  `OSError: [WinError 1114]` while importing torch. PyInstaller could bundle an
  older MSVC C runtime (`msvcp140`/`vcruntime140_1`) than PyTorch's `c10.dll`
  requires; because PyTorch's restricted DLL search prefers the bundle's
  `_internal` directory over System32, the stale runtime broke startup on every
  machine. `sidecar/build.py` now refreshes the bundled MSVC runtime from the
  host's System32 (the same version vc_redist installs) after PyInstaller runs.
- Frame embedding chunks are now replaced atomically, preventing partially
  indexed frames after an insertion failure.
- Migration 010 removes legacy duplicate chunks and enforces one embedding per
  `(frame_id, chunk_index)`.
- Time-filtered vector search expands its global candidate window and falls
  back to the full sqlite-vec index when required.
- Embedding responses validate model identity, revision, dimension, vector
  count, and per-vector dimensions.
- Sidecar model loaders are serialized during first initialization.
- PP-OCR uploads use JPEG transport and reject encoded images over 20 MiB or
  decoded images over 50 million pixels.
- PaddleOCR result cardinality mismatches are logged instead of silently
  truncated.

### Migration Notice
- Migration 009 intentionally deletes all legacy 384-dimensional embeddings
  because they are incompatible with the fixed 1024-dimensional Qwen3
  contract. Existing OCR data is retained and must be reindexed.

---

## [0.5.0] - 2026-06-10

### Changed
- **Brutalist / Minimalist Paper UI Redesign**: Overhauled the entire React frontend to match the print-media brutalist design of `screensearch-website`.
  - Replaced glassmorphism visual layout, translucent backdrops, radial gradients, glowing accents, and rounded corners with a flat, sharp, 90-degree corner design.
  - Implemented dynamic light and dark theme modes using custom HSL theme tokens (`bg-paper`, `text-ink`, `border-rule`).
  - Adopted newspaper-style typography: **Newsreader** (Serif) for headers and inputs, **Geist** (Sans) for UI copy, and **Geist Mono** (Monospace) for data metrics.
  - Redesigned `Sidebar.tsx`, `Timeline.tsx`, `FrameCard.tsx`, `SearchBar.tsx`, `Dashboard.tsx`, `Intelligence.tsx`, `AiSettings.tsx`, `SettingsPanel.tsx`, and `GlassCard.tsx` base components to utilize flat paper backgrounds, solid rule borders, and physical hover transforms instead of glows.

### Added
- **Dynamic Dark Mode**: Built support directly into the HSL variables in `index.css` via the `.dark` class, toggled by the existing state mechanism in `useStore.ts` and `App.tsx`.

---

## [0.4.32] - 2026-01-10

### Fixed
- **Installer Launch Failure (Error 740)** - Resolved "CreateProcess failed; code 740. The requested operation requires elevation" error that prevented app from launching after installation
  - Changed installation directory from `C:\Program Files\` to `%LOCALAPPDATA%\ScreenSearch\`
  - Removed admin privilege requirement - installer now runs without UAC prompt
  - Added `shellexec` flag to post-install launch for proper process handling
  - VC++ Redistributable installation now uses `/passive` mode with UAC prompt only when needed

### Technical
- Modified `installer/screensearch.iss`:
  - `DefaultDirName` changed from `{autopf}` to `{localappdata}`
  - `PrivilegesRequired` changed from `admin` to `lowest`
  - Post-install launch uses `shellexec` flag
  - VC++ install uses `shellexec` for UAC elevation

---

## [0.4.31] - 2026-01-10

### Fixed
- **Installer now bundles Visual C++ 2015-2022 Redistributable** - Fixes "VCRUNTIME140.dll not found" error on fresh Windows 11 installations when using the embedded LLM (llama-server.exe)
  - Installer automatically installs VC++ Runtime if not present (adds ~25MB to installer size)
  - Detection logic skips installation if VC++ Runtime already installed
- **Runtime DLL error detection** - Added user-friendly error messages for portable users when VC++ Runtime is missing
  - Detects Windows error codes 126 and 0xc0000135 (DLL_NOT_FOUND)
  - Provides direct download link: https://aka.ms/vs/17/release/vc_redist.x64.exe
  - Only affects llama-server.exe component; main app has no dependencies

### Technical
- Modified `installer/screensearch.iss` with `VCRedistNeedsInstall` registry check
- Updated `.github/workflows/release.yml` to download vc_redist.x64.exe during CI build
- Added `LlmError::MissingDependency` variant in `screensearch-llm/src/error.rs`
- Enhanced `screensearch-llm/src/server.rs` spawn logic with Windows-specific DLL detection

---

## [0.4.3] - 2026-01-09

### Fixed
- **Log File Permissions on Windows**: Fixed startup failure when running without admin privileges
  - Logs now written to `%LOCALAPPDATA%\ScreenSearch\logs\` in production builds
  - Previously failed with "Access is denied (os error 5)" when installed to `C:\Program Files\`
  - Development builds continue using relative path from `config.toml`
  - Follows same pattern as database and captures which already use LocalAppData

---

## [0.4.2] - 2026-01-08

### Fixed
- **Local LLM Auto-Start**: Fixed issue where llama-server would not auto-start when generating AI reports with the local provider
  - `generate_report()` now calls `ensure_started()` on the llama-server before making HTTP requests
  - Added prerequisite checks for model and server binary availability with clear error messages
  - Server now properly auto-starts on first AI request as designed (Lazy Loading)
  - Uses dynamic port from running server (handles fallback ports 31131, 31132 if default 31130 is busy)
  - Resolves "connection refused" errors on fresh install when user hadn't manually started server from Settings

- **GPU/Vulkan Fallback (VirtualBox Compatibility)**: Automatic fallback from GPU to CPU mode
  - First attempts GPU mode (Vulkan) with 45s timeout
  - If GPU fails (common in VirtualBox or systems without Vulkan), automatically retries in CPU-only mode
  - CPU mode uses 120s timeout to allow for slower model loading
  - Clear logging of which mode is being used

- **Health Check Robustness**: Improved llama-server startup reliability
  - Added INFO-level logging to track model loading progress during startup
  - Proper handling of HTTP 503 status (model still loading) vs connection errors
  - Clear error logging when server process exits unexpectedly or health check times out

- **TypeScript Build Error**: Fixed TS2532 "Object is possibly 'undefined'" in ReportGenerator.tsx
  - Added nullish coalescing operator for ISO date string parsing

- **CI Cross-Compilation**: Fixed `cargo-xwin` installation failure due to yanked `xwin` dependencies
  - Patched `cargo-xwin` v0.20.2 to use non-yanked `xwin = "0.6.8"` instead of yanked `^0.6.6`/`^0.6.7`
  - Clone from git, patch Cargo.toml, and install from local path to avoid dependency resolution failures
  - Maintains reproducible builds using pinned commit 074ac4d (2026-01-08)

### Technical Details
- Modified `screensearch-api/src/handlers/ai.rs`: Added auto-start logic to `generate_report()` function
- Modified `screensearch-llm/src/server.rs`: Refactored `start()` with GPU fallback and `wait_for_health_with_timeout()`
- Modified `screensearch-ui/src/components/ReportGenerator.tsx`: Fixed TypeScript strict null check
- Error messages now direct users to Settings → AI for missing dependencies
- Aligns with documented behavior in `docs/embedded-llm.md`: "Lazy Loading: Server starts only when first AI request is made"

---

## [0.4.1] - 2026-01-02

### Added
- **Virtual Scrolling**: Implemented react-window `FixedSizeGrid` for efficient rendering of large frame collections
  - New `VirtualFrameGrid` component with memoized cell renderer
  - `AutoSizeVirtualFrameGrid` wrapper for responsive layouts
  - Only renders visible items + 2 row overscan for smooth scrolling
  - 80% memory reduction for large frame collections (1000+ frames)

- **Performance Benchmarks**: Added Criterion.rs benchmark suite for database operations
  - `bench_get_frame`: Frame retrieval by ID
  - `bench_frame_range_query`: Range queries with varying dataset sizes
  - `bench_fts5_search`: Full-text search (single/multi-word)
  - `bench_frame_insertion`: Write performance testing
  - `bench_cosine_similarity`: Vector search simulation (384-dim)
  - `bench_statistics`: Statistics collection benchmark
  - Run with: `cargo bench -p screensearch-db`

- **Database Performance Indexes**: Migration 008 adds optimized indexes
  - `idx_frames_device_monitor_time`: Multi-column index for device/monitor queries
  - `idx_metadata_key`: Fast metadata lookups
  - `idx_analysis_queue_locked_priority`: Queue optimization
  - `idx_embeddings_frame_created`: Embedding lookup acceleration

- **Time-Filtered Embeddings Search**: New `search_embeddings_with_time_range()` method
  - Prevents loading all embeddings into memory
  - Bounded memory usage for large embedding sets
  - Optional start/end time parameters

### Changed
- **Database Connection Pool**: Reduced from 50 to 10 max connections
  - SQLite single-writer limitation makes 50 connections excessive
  - Lower memory usage and reduced contention
  - Optimal: 1 writer + 9 readers for concurrent access

- **API Logging**: Console logging now dev-only (`import.meta.env.DEV` check)
  - Eliminates 5-10ms overhead per request in production
  - Request/response logging still available in development

- **Bundle Optimization**: Vite manual chunk splitting for better caching
  - `vendor-react`: react, react-dom (141KB)
  - `vendor-query`: @tanstack/react-query, axios (76KB)
  - `vendor-ui`: framer-motion, lucide-react (114KB)
  - `vendor-markdown`: react-markdown (118KB)
  - Intelligence page lazy-loaded (10KB)

- **Code Splitting**: React.lazy() + Suspense for route-based splitting
  - IntelligencePage loaded on-demand
  - PageLoader fallback during chunk loading
  - 30-40% initial bundle reduction

### Fixed
- **Blob URL Memory Leak**: Implemented proper cleanup with `URL.revokeObjectURL()`
  - `blobUrlCache` Map tracks frame ID → blob URL mappings
  - `revokeBlobUrl()` function for manual cleanup
  - `revokeAllBlobUrls()` for bulk cleanup
  - useFrames hook auto-revokes on unmount

- **FrameCard Re-renders**: Wrapped in `React.memo()` with custom comparator
  - Only re-renders when frame data actually changes
  - Compares frame.id, timestamp, tags.length, and searchQuery
  - 20-30% faster grid rendering

- **Timeline Computation**: Wrapped `framesByDate` in `useMemo()`
  - O(1) for cached data instead of O(n) on every render
  - Significant improvement for large frame lists

- **RAG Context Building**: Early limit in first loop iteration
  - Prevents collecting all chunks then discarding most
  - Minor allocation reduction

- **React Error #310**: Fixed "Rendered fewer hooks than expected" in Timeline component
  - Root cause: `useMemo` called AFTER early returns for loading/error states
  - Solution: Moved `useMemo` before conditional rendering, use JSX ternary instead of early returns
  - All hooks now called unconditionally on every render per React Rules of Hooks
  - Prevents reconciliation errors during state transitions (loading → data)

### Technical Details
- Frontend bundle: 670KB total (properly chunked)
- All 14 database tests passing
- Rust workspace compiles cleanly
- react-window v1.8.10 with @types/react-window v1.8.8
- Criterion v0.5 with html_reports and async_tokio features

### Dependencies Added
- `react-window: ^1.8.10` - Virtual scrolling
- `@types/react-window: ^1.8.8` - TypeScript types
- `criterion: 0.5` (dev) - Rust benchmarks

---

## [0.4.0] - 2025-12-28

### Added
- **Embedded Ministral-3B LLM**: Local AI inference with no external API required
  - New `screensearch-llm` crate for managing llama.cpp server lifecycle
  - Auto-downloads llama-server binary and Ministral-3B-Instruct GGUF model on first use
  - GPU acceleration via Vulkan (works on NVIDIA, AMD, Intel) with CPU fallback
  - Model files stored in `%APPDATA%\ScreenSearch\models\`
  - Server runs on `http://127.0.0.1:31130` with OpenAI-compatible API

- **Local Provider UI Integration**:
  - "Local (Ministral-3B)" as default AI provider option in Settings
  - Model download progress indicator with real-time status
  - llama-server start/stop controls with health monitoring
  - Provider dropdown replacing free-form URL input
  - Automatic hiding of API key/model fields when local provider selected

- **Vision System Enhancements**:
  - Local provider handling in vision config test endpoint
  - Health check for llama-server at `http://127.0.0.1:31130/health`
  - Proper error messages for model download and server states

### Changed
- **Default AI Provider**: Changed from Ollama to local embedded LLM
- **Default Vision Settings**: Fresh installs now default to:
  - `vision_enabled = 0` (opt-in, not auto-start)
  - `vision_provider = 'local'`
  - `vision_endpoint = 'http://127.0.0.1:31130'`
- **llama-server Binary**: Switched from CPU-only to Vulkan GPU build for cross-platform acceleration
- **Database Migration**: Added migration to reset existing databases to local provider defaults

### Fixed
- **GitHub Actions Workflow**: Fixed cross-compile workflow failure caused by incorrect llvm-mingw download URL
  - Updated version format from `2024-11-01` to `20251216`
  - Fixed archive name from Windows binary (`x86_64-windows-gnu`) to Linux binary (`ucrt-ubuntu-22.04-x86_64`)
  - Removed non-existent packages (`llvm-tools-19`, `llvm-mingw`) from apt-get

### Technical Details
- llama-server manages Ministral-3B-Instruct-Q4_K_M.gguf (1.9GB model)
- GPU offloading with `-ngl 99` flag for maximum acceleration
- Vulkan binary auto-falls back to CPU if no GPU available
- DLL extraction from ZIP archives for Windows compatibility
- Server lifecycle managed via child process with graceful shutdown

---

## [0.3.0] - 2025-12-27

### Added
- **AI-First Dashboard**: New "Intel Dash" homepage with glassmorphism design aesthetic
  - **Daily Digest**: Auto-generated AI summaries of daily activity on page load
    - Session storage caching to avoid redundant API calls
    - Markdown rendering with custom bullet styling
    - Refresh button for on-demand regeneration
  - **Memory Status Gauge**: Circular radial gauge showing RAG indexing progress
    - Real-time embedding coverage percentage
    - Visual indicator for semantic search readiness
  - **Productivity Pulse**: Custom SVG line/area chart displaying hourly activity
    - Smooth cubic bezier curves
    - Interactive tooltip on hover
    - Gradient fill with glow effects
  - **Coming Soon Placeholders**: Knowledge Graph and Analytics previews

- **Smart Answer Card**: Enhanced AI-powered search results
  - Generates context-aware answers from screen history
  - Related Activity section showing app breakdown
  - Collapsible activity list with app icons

- **Glassmorphism Design System**: Premium UI components matching screensearch.app
  - `GlassCard` component with backdrop blur and translucent backgrounds
  - `CircularGauge` component with animated SVG progress
  - `ComingSoonCard` placeholder component with pulsing border
  - CSS custom properties for consistent theming (`--glass-bg`, `--glass-border`, `--glass-glow`)
  - Utility classes: `.glass-panel`, `.glass-card`, `.glow-blue`, `.gradient-text`
  - Animations: `fade-in-up`, `pulse-glow`, `border-glow`

- **Activity List Component**: App-specific breakdown of screen captures
  - Icon mapping for common applications (VS Code, Chrome, Slack, etc.)
  - Relative timestamps using date-fns
  - Frame count per application

### Changed
- **Primary Color**: Switched from violet to blue (#2563eb) to match screensearch.app branding
- **Default Page**: Dashboard is now the default landing page (previously Timeline)
- **Navigation**: Redesigned sidebar with AI Features section and Coming Soon badges
- **State Management**: Updated Zustand store with new `activeTab` options: `dashboard`, `timeline`, `reports`
- **Tailwind Config**: Extended theme with glass colors, primary light/dark variants, and new keyframe animations

### Technical Details
- Production bundle: 471.08 kB JS, 62.57 kB CSS
- Custom SVG charts avoid external charting library dependencies
- HSL color system enables consistent dark/light theme support
- Session storage caching reduces API calls for Daily Digest
- All new components use TypeScript with proper type safety

## [0.2.1] - 2025-12-14

### Added
- **Generic AI Provider Support**: Full compatibility with OpenAI-compatible APIs (e.g., LM Studio, vLLM, Ollama generic) via user settings.
- **AI Connectivity Test**: "Test Connection" button in settings to verify AI provider configuration immediately.
- **RAG Generation Endpoint**: New `/api/generate` endpoint for creating structured answers from search results.
- **Vision Client**: Dedicated `screensearch-vision` crate for managing AI interactions (Models, Prompts, Client).

### Changed
- **Settings UI**:
    - Added explicit "Save Configuration" button (removed auto-save on blur).
    - Added "Provider Protocol" selector (Ollama / OpenAI-compatible).
    - Added "Vision Model" configuration.
- **Cleaned UI**: Removed unused "Activity Graph" styling elements for a cleaner dashboard look.
- **Database Schema**: Added `vision_api_key` to settings for authenticated providers.

### Fixed
- **Settings Persistence**: Resolved issue where Vision AI settings (Provider, Model, Endpoint) were reset on save.
- **Ollama Defaulting**: Fixed backend hardcoding that forced Ollama usage even when other providers were selected.

## [0.2.0] - 2025-12-13

### Added
- **Timeline Visualization**: New "Activity Graph" component showing daily screen activity density in 10-minute buckets.
- **System Tray Integration**: Full system tray support with "Open" and "Quit" menu interactions.
- **Branding**: Complete "ScreenSearch" rebranding with premium "Tech-Panel" UI aesthetic matching `screensearch.app`.
- **Footer**: Added professional footer with author credits and repository links.
- **Icons**: New application icon (Blue/Cyan Activity Pulse) replacing placeholders.

### Changed
- **UI Overhaul**: Redesigned `App.tsx` layout with background grids, sidebar navigation, and glassmorphism effects.
- **Event Loop**: Refactored Winit event loop in `main.rs` for stable background operation and clean shutdown.
- **Performance**: Improved timeline data fetching with `useDailyActivity` hook for full-day statistics.

### Fixed
- **System Tray Infinite Loop**: Fixed critical bug where the browser would open endlessly on mouse hover events.
- **Search Reliability**: Hardened OCR text processing to prevent React rendering crashes on complex objects.
- **Build System**: Fixed Rust compilation errors related to accidental code truncation in `main.rs`.

## [0.1.4] - 2025-12-12

### Added
- **Retrieval Augmented Generation (RAG)**: Full support for RAG-based AI reports.
    - **In-Memory Vector Search**: Implemented high-performance in-memory semantic search (BGE-M3/MiniLM-L12 compatible) to bypass `sqlite-vec` limitations on Windows.
    - **Hybrid Search**: Combines Dense Retrieval (Embeddings) with Sparse Retrieval (FTS5) for robust context lookup.
    - **Reranker**: Added heuristic reranker boosting newer results and keyword matches.
- **Context Source Indicator**: Reports now include a footer (e.g., `*Context: Semantic Search (20 results)*`) indicating if the Vector Database or Traditional Fallback was used.
- **Database Schema**: Added `embedding` BLOB column to `embeddings` table (Migration 004).

### Changed
- **Dependency Optimization**: Downgraded `ort` (ONNX Runtime) to `2.0.0-rc.0` to match system-provided `1.17.1` DLLs, ensuring stability without external downloads.
- **API Response**: `AiReportResponse` now includes a `context_source` field.

### Fixed
- **Embedding Storage**: Fixed critical bug where embeddings were not being persisted (inserting 0 bytes), now correctly serializing `Vec<f32>` to BLOB.

## [0.1.3] - 2025-12-12

### Added
- **Storage Optimization**: Implemented JPEG compression for captured frames (default quality 80) to significantly reduce storage usage.
- **Image Resizing**: Added automatic resizing of captured frames to a maximum width (default 1920px) to further reduce file size.
- **Automatic Cleanup**: Implemented a background task that runs every 24 hours to enforce the data retention policy (deletes old frames based on `retention_days` setting).
- **Configuration**: Added `[storage]` section to `config.toml` for customizing format, quality, and max width.

### Changed
- **Default Image Format**: Changed default capture format from PNG to JPEG.

## [0.1.2] - 2025-12-11

### Added
- **Embedded UI Assets**: UI files are now embedded directly into the release binary using `rust-embed`, making the binary fully portable and self-contained
  - Binary can run from any directory without requiring `screen-ui/dist/` to exist
  - Assets served from memory for faster performance
  - Simpler deployment - just ship the binary
  - Binary size remains ~11MB (efficient compression)

### Fixed
- **JSX Structure in AI Settings**: Fixed orphaned form fields (API Key, Model Name, Test Button) in AiSettings component by correcting premature div closure (screen-ui/src/components/AiSettings.tsx:58)
- **Build Script Silent Failures**: npm install/build failures now properly fail the cargo build with clear error messages instead of silently continuing, preventing shipment of binaries without UI
- **npm Command Detection**: Improved Windows compatibility by using `npm.cmd` and proper `Command::current_dir()` instead of shell command strings

### Security
- **CORS Configuration**: Fixed invalid CORS setup that caused runtime panics - now properly uses explicit allow-lists for HTTP methods and headers when credentials are enabled (per CORS specification)
- **Information Disclosure in AI Errors**: Improved error message handling in AI endpoints - now provides error type information (HTTP status codes, error categories) for debugging while sanitizing sensitive response data from clients
- **URL Validation**: Added provider URL validation before making HTTP requests to AI endpoints - validates format and logs warnings for non-localhost URLs

### Changed
- **Browser Auto-Launch**: Made browser auto-launch configurable via `auto_open_browser` setting in config.toml (defaults to `true` for backward compatibility)
  - Set to `false` for headless servers, Docker containers, or background services

### Improvements
- **Code Documentation**: Added detailed comments explaining ServeDir SPA routing pattern in routes.rs
- **Code Clarity**: Documented magic number (1024*1024 = 1MiB) in request body limit configuration
- **Code Quality**: Extracted duplicated API key header logic to reusable `add_auth_header()` helper function
- **Developer Experience**: Added `SKIP_UI_BUILD` environment variable to allow skipping UI build during development for faster iteration (usage: `SKIP_UI_BUILD=1 cargo build`)

### Breaking Changes
- **Health Endpoint Route Change**: The health check endpoint has been moved from `/health` to `/api/health` to maintain consistency with other API routes
  - Update any monitoring systems, health check configurations, or client code that references the old `/health` endpoint
  - The endpoint functionality remains the same, only the path has changed

### Added
- **Search Autocomplete**: Intelligent search suggestions with keyboard navigation (↑↓ arrows, Enter, Escape)
  - Debounced API calls (300ms) for optimal performance
  - Text highlighting in suggestions
  - Visual hover and selected states
  - Click-outside to close dropdown
  - ARIA accessibility attributes
- **Timeline Filters**: Advanced filtering system for frames
  - Date range filter (start/end dates)
  - Application name filter
  - Tag-based filtering with multi-select support
  - URL bookmarkability - all filters reflected in query parameters
  - "Clear all filters" functionality
- **Complete Tag Management**: Full CRUD operations for tags
  - Create tags with custom names and colors
  - Edit existing tags (name and color)
  - Delete tags with confirmation dialog
  - Assign/remove tags to/from frames via frame modal
  - Tag picker dropdown showing unassigned tags
  - Hover-to-delete on assigned tags
  - Optimistic UI updates and toast notifications
- **Tag Filtering Backend**: Server-side support for filtering frames by multiple tags
  - Comma-separated tag IDs in API query parameters
  - Proper SQL joins for tag-based frame retrieval
  - Integration with existing date and app filters

### Improvements
- **Accessibility**: Added ARIA labels to icon-only buttons (theme toggle, settings, tag menu)
- **Code Organization**: Extracted OCR text handling logic to dedicated `lib/ocrUtils.ts` utility file with proper documentation
- **Performance Optimizations**:
  - Memoized OCR text extraction in FrameCard to prevent unnecessary re-computation
  - Memoized search highlighting to avoid re-processing on every render
  - Added cancel method to debounce utility for proper cleanup

### Performance
- **OCR Pipeline Optimization**: Eliminated 60-93ms bottleneck by implementing direct `SoftwareBitmap` creation from raw RGBA data
  - **Before**: 112-196ms per frame (PNG encode 40-60ms + PNG decode 15-25ms + OCR 50-100ms)
  - **After**: 55-105ms per frame (Direct bitmap 5ms + OCR 50-100ms)
  - **Improvement**: 53-73% faster, achieving <100ms target
  - Zero-copy memory transfer using unsafe Rust with safety guarantees
  - Removed `std::io::Cursor` and PNG-related dependencies from hot path
- **Tag Loading Optimization**: Implemented bulk tag loading to eliminate N+1 query problem
  - **Before**: 201 queries for 100 frames (1 frame query + 100 tag queries + 100 OCR queries)
  - **After**: 3 queries for 100 frames (1 frame query + 1 bulk tag query + 100 OCR queries)
  - **Improvement**: 15x faster tag loading, 100 frames now load in <200ms (down from ~1500ms)
  - New `get_tags_for_frames()` method uses single JOIN query with parameterized IN clause
  - Returns HashMap for O(1) tag lookup per frame
- **Tag Assignment Optimization**: Simplified `add_tag_to_frame` to rely on database FK constraints
  - **Before**: 3 queries (verify frame exists + verify tag exists + insert)
  - **After**: 1 query (insert with FK constraint error handling)
  - Gracefully handles duplicate assignments (idempotent behavior)
- **Memory Optimization**: Refactored frame differencing to use `Arc<RgbaImage>` reference counting, eliminating 8.2MB allocations per frame change (reduces memory pressure from 39GB/8hr to <1GB/8hr)
- **Query Sanitization**: Added FTS5 query input sanitization to prevent operator injection and handle special characters (e.g., `C++`, `$100`) correctly

### Fixed
- **Critical API Route Mismatch**: Fixed `addTagToFrame()` to send `tag_id` in request body instead of URL path (tag assignment was completely broken)
- **Search Autocomplete API**: Fixed parameter name from `q` to `keywords` to match backend expectation
- **OCR Text Extraction**: Updated API client to handle `OcrTextRecord[]` response format correctly
- **Filter Integration**: Connected all UI filters to backend API with proper query parameters
- **Tag API Response Format**: Fixed field mapping from `tag_name` (database) to `name` (API response)
- **Error Messages**: Enhanced error handling in FrameModal to extract and display specific validation errors from API responses
- **TypeScript Compilation**: Removed unused imports and variables causing build warnings
- **Null Safety**: Added optional chaining for `frame.tags?.length` to prevent undefined errors
- **FrameCard Rendering Error**: Fixed crash when `ocr_text` is returned as an object instead of string by adding robust type handling
- **Type Safety**: Replaced `any` types with proper TypeScript union types for OCR content (`OCRTextContent`, `OCRTextData`)
- **Click-Outside Handler**: Added missing click-outside handler for tag menu dropdown in FrameCard
- **Debounce Cleanup**: Added proper cleanup for debounced search queries to prevent memory leaks on component unmount

### Security
- **XSS Vulnerability**: Eliminated `dangerouslySetInnerHTML` usage in FrameCard and replaced with safe React element rendering for search highlighting
  - Text is now properly escaped and rendered as React elements instead of raw HTML
  - Search highlights use `<mark>` elements rendered safely through React
- **Hex Color Validation**: Added regex validation for tag colors (`#RRGGBB` or `#RRGGBBAA` format only)
- **Input Size Limits**: Enforced maximum lengths - tag_name (200 chars), description (1000 chars)
- **Request Body Limits**: Added 1MB max request size via `DefaultBodyLimit` middleware to prevent DoS attacks
- **Frontend Validation**: Added character counters and maxLength attributes to tag creation form

### Technical Details
- OCR processing time reduced from 112-196ms to 55-105ms (53-73% improvement)
- Tag loading optimized from 201 queries to 3 queries for 100 frames (15x faster)
- Direct `SoftwareBitmap::Create()` with `BitmapPixelFormat::Rgba8` eliminates intermediate format conversions
- Bulk tag loading uses dynamic SQL with parameterized IN clause and HashMap grouping
- Unsafe memory copy with buffer size validation and exclusive write lock ensures memory safety
- Frame differencing now uses 8-byte Arc pointer clones instead of full 8.2MB image clones
- Search queries now wrap user input in quotes with proper escaping to prevent FTS5 syntax errors
- System can now handle 1-second capture intervals (down from 3 seconds) while maintaining <5% CPU usage
- Tag filtering supports efficient SQL queries with proper JOIN operations on `frame_tags` table
- All filter operations update URL query parameters for shareable/bookmarkable states
- Foreign key constraint validation reduces redundant existence checks in `add_tag_to_frame`
- Error messages now include specific validation details from backend API responses

## [0.1.1] - 2025-12-10

### Added
- `GET /frames/:id` endpoint for retrieving individual frame details with OCR text and tags
- `GET /settings` endpoint for retrieving current capture configuration
- `POST /settings` endpoint for updating capture settings (interval, monitors, excluded apps, etc.)
- Settings panel in web UI with backend integration for:
  - Capture interval adjustment (2-30 seconds)
  - Monitor selection
  - Excluded applications management
  - Pause/resume capture control
  - Data retention settings

### Fixed
- "Frame not found" error when clicking frame cards in web interface
- Settings panel now properly loads and saves configuration via backend API
- Type mismatch between Rust snake_case field names and TypeScript camelCase expectations

### Changed
- Web interface status updated from "Broken" to fully functional
- Settings interface now uses snake_case field names to match backend API response format

## [0.1.0] - 2025-12-10

### Added
- Initial release with core functionality:
  - Continuous screen capture with frame differencing
  - OCR text extraction using Windows OCR API
  - SQLite database with FTS5 full-text search
  - REST API with 27 endpoints
  - UI automation via Windows accessibility APIs
  - React-based web interface
  - Privacy controls and application exclusions

[0.5.0]: https://github.com/nicolasestrem/screensearch/compare/v0.4.32...v0.5.0
[0.4.2]: https://github.com/nicolasestrem/screensearch/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/nicolasestrem/screensearch/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/nicolasestrem/screensearch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/nicolasestrem/screensearch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nicolasestrem/screensearch/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/nicolasestrem/screensearch/releases/tag/v0.1.0
