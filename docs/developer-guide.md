# ScreenSearch Developer Guide

## Prerequisites

- Rust stable with `rustfmt` and `clippy`;
- Node.js 22 and npm;
- Windows 10/11 and Visual Studio Build Tools for production validation;
- Inno Setup 6 or 7 for installer builds.

No Python is needed to build or run ScreenSearch. Linux is suitable for frontend
work and most Rust checks. Windows remains the authoritative platform for
capture, native Windows OCR, UI Automation, tray integration, and installers.

## Repository Setup

```bash
npm ci --prefix screensearch-ui
cargo check
```

Do not install dependencies or write model caches into the repository.

## Embedded Frontend Build

The web UI (the "Command Deck" — Vite + React 18 + TypeScript, `react-router-dom`,
TanStack Query, a typed `fetch` client, Zustand; see
[Frontend — Command Deck UI](frontend-design-system.md)) is embedded by
`screensearch-api` using `rust-embed`. Install frontend dependencies before the
first Cargo build:

```bash
cd screensearch-ui
npm ci
cd ..
cargo build
```

The build script (`build.rs`) runs `npm run build` automatically before embedding
`screensearch-ui/dist/`. Set `SKIP_UI_BUILD=1` to skip the npm step when a current
`dist/` already exists (the binary then embeds whatever is in `dist/`).

For UI-only work, run the dev server with hot reload; it proxies `/api` to a
running backend on `127.0.0.1:3131`:

```bash
cd screensearch-ui
npm run dev      # http://localhost:5173
npm run build    # tsc + vite (type-check + production bundle)
npm run lint     # eslint, zero warnings
```

Fonts are Windows-native (Bahnschrift / Consolas / Segoe UI), so nothing is
downloaded at runtime.

## Running Locally

```bash
cargo run
```

OCR and embeddings run in-process; there is no separate service to start. The
embedding model loads on first use and is cached locally. Use
`POST /api/embeddings/models/prepare` before load testing to pre-load the
in-process embedding model and exclude initialization from steady-state latency
measurements.

## Current AI Contracts

Do not change these independently:

| Contract | Value |
|---|---|
| OCR provider | native Windows OCR (WinRT `Media.Ocr`, in-process) |
| Embedding model | `EmbeddingGemma-300M` (in-process via `fastembed`) |
| Reranker model | `bge-reranker-v2-m3` (optional, off by default) |
| Embedding dimension | 768 |
| Vector distance | cosine |
| sqlite-vec table | `embedding_vectors` (`embedding float[768] distance_metric=cosine`) |

Changing embedding model or dimension requires:

1. a new database migration;
2. a new sqlite-vec table contract;
3. metadata invalidation;
4. a full reindex;
5. evaluation evidence.

## OCR Development

OCR lives in:

- `screensearch-capture/src/ocr_provider.rs` (provider wrapper);
- `screensearch-capture/src/ocr.rs` (native Windows OCR via WinRT `Media.Ocr`).

OCR changes must preserve confidence, language, orientation, and bounding boxes.
Test OCR behavior on Windows, where the WinRT OCR API is available.

## Retrieval Development

Key files:

- `screensearch-db/src/migrations.rs`;
- `screensearch-db/src/queries.rs`;
- `screensearch-db/src/vector_search.rs`;
- `screensearch-embeddings/src/engine.rs`;
- `screensearch-api/src/handlers/rag_helpers.rs`.

Retrieval sequence:

1. tokenizer-aware document chunking;
2. EmbeddingGemma-300M document embeddings (in-process via `fastembed`);
3. sqlite-vec cosine KNN;
4. FTS5 lexical candidates;
5. RRF with `k = 60`;
6. optional `bge-reranker-v2-m3` reranking (off by default);
7. context assembly with `[frame:<id>]` citations.

Do not reintroduce in-memory full-vector scans or synthetic hash embeddings.

## Generation Development

Generation is independent from retrieval. The `vision_*` database fields
configure both the answer-generation provider and the vision pipeline.

Supported runtimes:

- bundled llama.cpp server through `screensearch-llm` (Vulkan GPU with CPU
  fallback). It is model-agnostic: `resolve_model_path`/`discover_local_models`
  auto-discover any `*.gguf` file in `.models/` (repo root) or the app models
  directory and use the first one found; Ministral-3B is the downloadable
  default fallback. `GET /api/ai/model/status` returns an `available_models`
  list;
- Ollama-compatible;
- OpenAI-compatible.

The local model-management routes belong to generation only. They are not the
embedding-engine status.

## Vision Development

Vision analyzes screenshot pixels and writes `description`, `visible_text`,
`activity_type`, `app_hint`, and `confidence` onto frames. It is off by default
and shares the `vision_*` settings.

- **Local provider = unified server.** `AppState::get_llama_server`
  (`screensearch-api/src/state.rs`) is vision-aware: when vision is enabled with
  `vision_provider = "local"`, it runs the single llama.cpp server with
  `--mmproj` so one model (Qwen3-VL-4B-Instruct by default) serves both text and images, and it rebuilds
  the server when vision toggles. Model/projector selection lives in
  `screensearch-llm/src/download.rs` (`resolve_vision_model`,
  `resolve_mmproj_for`, `discover_vision_models`); `--mmproj` emission and the
  `mmproj_path` config field are in `screensearch-llm/src/server.rs`.
- **Worker.** `screensearch-api/src/workers/vision_worker.rs` consumes
  `analysis_queue`, calls the provider via `screensearch-vision` (OpenAI-compat
  vision path for `local`), and writes results back. It takes `Arc<AppState>` via
  `ApiServer::start_vision_worker`.
- **Enqueue.** On demand (`POST /api/vision/analyze/:frame_id`) plus a throttled
  background trickle (`DatabaseManager::get_unanalyzed_frame_ids`, batch 4).
  `GET /api/vision/status` reports counts.
- **Frames default to `analysis_status = 'pending'`** but are not auto-queued;
  the trickle query treats anything not in `('completed','processing','failed')`
  and not already queued as needing analysis.

See `docs/vision.md` for the full guide.

## Local Linux Build

```bash
./scripts/build-local.sh
```

Output:

```text
target/debug/screensearch-local/
  screensearch
```

The script builds the dashboard and the native Linux Rust executable. Use
`--release` for `target/release/screensearch-local`.

This development bundle is not a Windows distributable. Native Windows OCR is
unavailable on Linux, so capture and OCR must be validated on Windows.

## Tests And Quality

```bash
cargo check --locked \
  -p screensearch-db \
  -p screensearch-embeddings \
  -p screensearch-capture \
  -p screensearch-api

cargo test --locked -p screensearch-db
cargo test --locked -p screensearch-embeddings --lib

cd screensearch-ui
npm run lint
npm run build
npm audit --audit-level=high
```

Run native capture, OCR, automation, and installer tests on Windows.

The repository has historical formatting and clippy findings outside the
modernized crates. CI scopes strict checks to the changed retrieval stack while
still compiling dependencies.

## Evaluation

Create a result JSONL with one object per case:

```json
{"id":"case-id","retrieved_frames":[12,18,20]}
```

Run:

```bash
python evaluation/evaluate.py evaluation/results.jsonl
```

Track Recall@10 and MRR for retrieval. OCR changes also require CER/WER,
confidence coverage, orientation, multilingual, small-text, and code/terminal
cases.

## Release

The release workflow:

1. builds the frontend;
2. builds Rust with `--locked`;
3. compiles the installer;
4. creates the portable ZIP;
5. publishes checksums.

`fastembed`/`ort` cross-compile cleanly to `x86_64-pc-windows-msvc`, and no
Python is involved in the build.

From Linux, validate and cross-compile with:

```bash
./scripts/build-release.sh 0.4.35
```

This produces a Windows ZIP under
`target/x86_64-pc-windows-msvc/release/bundles/`. It is useful for checking the
cross-compiled executable.

Build and download complete Windows artifacts without publishing a tag:

```bash
./scripts/build-release.sh 0.4.35 --windows-bundle
```

The helper matches the dispatched workflow by branch, commit SHA, and dispatch
time so an earlier successful run cannot be mistaken for the new build.
The Windows workflow recreates its portable staging directory after restoring
build caches, preventing stale files from contaminating or blocking a new
archive.

The installer, portable ZIP, and checksums are downloaded under
`target/x86_64-pc-windows-msvc/release/bundles/windows-full/`.

Publish the release tag only after validation:

```bash
./scripts/build-release.sh 0.4.35 --publish
```

The pushed tag triggers `.github/workflows/release.yml` on a Windows runner.
That job builds the Inno Setup installer, portable ZIP, checksums, and draft
GitHub release. Before packaging, it starts the executable and verifies OCR and
retrieval on a generated image. A healthy HTTP endpoint alone is not sufficient
for the release job to pass.

### Native Windows release build

`scripts/build-release.ps1` builds the entire bundle directly on a Windows host
(frontend, Rust binary, Inno Setup installer, portable ZIP, checksums):

```powershell
.\scripts\build-release.ps1 -Version 0.4.37
```

The script handles two native-Windows specifics automatically:

- **Linker.** `.cargo/config.toml` hard-codes the `lld` linker for the
  Linux→Windows cross-compile, which is absent on a native Windows host (`cargo`
  would fail with "linker `lld` not found"). The script imports the MSVC build
  environment via `vswhere` and overrides `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER`
  to `link.exe`. Do not edit `.cargo/config.toml`. (For plain `cargo build/check/
  test/clippy` outside the script, set those env vars yourself — import
  `vcvars64.bat`, then `$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="link.exe"`.)
- **Inno Setup.** It locates `ISCC.exe` across Inno Setup 6 and 7 install paths
  (and `PATH`) rather than assuming a single location.

Use `-SignBinary` to sign and `-Clean` to force a from-scratch Rust build. No
Python interpreter is required; OCR (native Windows OCR) and embeddings
(`fastembed`/`ort`) build and run in-process.

Embedding writers must use `DatabaseManager::insert_embeddings` for complete
frames. It replaces all chunks in one transaction. Do not restore per-chunk
writes because one successful chunk would make an incomplete frame appear
indexed.

PowerShell helpers are retained for maintainers working directly on Windows,
but they are not the primary development or release entrypoints.

Models are not stored in the installer. The EmbeddingGemma-300M embedding model
is prepared from Settings or downloaded from Hugging Face on first use. The
optional answer-generation GGUF is auto-discovered from `.models/` or downloaded
on demand.

## Security Rules

- Keep the API bound to loopback.
- Never commit API keys, databases, captures, logs, model weights, or caches.
- Treat OCR text and frame metadata as sensitive.
- Validate remote generation URLs and disclose that grounded context leaves
  the machine.
- Avoid logging bearer tokens or full OCR context.
