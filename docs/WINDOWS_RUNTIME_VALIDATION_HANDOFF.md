# Session Handoff — Windows Runtime Validation of the In-Process Rust ML Stack

> **Read this file first and in full before taking any action.** It is the
> authoritative continuation point. Every path is repo-relative unless marked
> absolute. Every command, version, constant, commit hash, and API field below
> is literal and verified as of this handoff. Do not infer or substitute values.

---

## 0. How to use this document

- This handoff was written on a **Linux** machine. The next session runs on a
  **Windows 10/11** machine because the work that remains can only be done and
  verified on Windows.
- The previous session's work is **already merged to `main`** (see §2). You are
  continuing from `main`, not from an open branch.
- Sections are ordered: objective → what's done → current state → the remaining
  task → exact step-by-step strategy → reference material. Follow §5 in order.

---

## 1. Overall objective (the end goal we are working toward)

ScreenSearch must run **all machine learning fully in-process in Rust on Windows,
with no Python sidecar**, and this must be **proven to work at runtime on a real
Windows machine**, then shipped in the Windows installer and portable ZIP.

"Working at runtime on Windows" means, concretely, all five of these are observed
to succeed on a Windows host (not just compile):

1. Native Windows OCR (WinRT) populates `frames` + `ocr_text` from live capture.
2. The background embedding worker fills the sqlite-vec `embedding_vectors` table
   with **768-dimensional** vectors produced by EmbeddingGemma via `fastembed`.
3. `GET /api/embeddings/status` reports `provider":"fastembed"`, `dimension":768`,
   `engine_ready":true`.
4. Semantic search and hybrid (FTS5 + vector + RRF) search return results.
5. `POST /api/ai/generate` produces a grounded summary using a GGUF model that
   was auto-discovered from the `.models/` directory and run by the bundled
   external llama.cpp server.

The single remaining blocker to (2)–(4) is shipping a matching
**`onnxruntime.dll`** next to the executable (see §4 and §6).

---

## 2. What was accomplished in the previous (Linux) session

We reverted PR #63 (which had replaced in-Rust ML with a slow Python "quality
sidecar") and restored a fully in-process Rust stack, then merged it.

- **Merged to `main` as squash commit `19d41cd`** — PR #66, title
  "Revert PR #63 sidecar; restore in-process Rust ML stack (#66)". The feature
  branch `feature/revert-63-inprocess-rust-ml` was deleted after merge.
- The `sidecar/` directory, `screensearch-capture/src/sidecar_ocr.rs`, the
  `ensure_quality_sidecar()` process management in `src/main.rs`, the `uuid`
  dependency, and all sidecar build/bundle steps were removed.
- **OCR**: native Windows OCR (WinRT) only — `screensearch-capture/src/ocr.rs`
  via the thin wrapper `screensearch-capture/src/ocr_provider.rs`.
- **Embeddings**: `screensearch-embeddings` rewritten as a `fastembed`
  (ONNX Runtime) engine — model **EmbeddingGemma-300M, 768-dim** (fastembed enum
  variant `EmbeddingModel::EmbeddingGemma300MQ`). In-process chunker added
  (`screensearch-embeddings/src/chunker.rs`). Optional cross-encoder reranker
  (`bge-reranker-v2-m3`), **off by default** (RRF is the default fusion).
- **DB**: sqlite-vec contract changed from `float[1024]` to `float[768]`
  (`screensearch-db/src/migrations.rs`, migration `MIGRATION_009_QUALITY_RAG`).
  No migration of existing data — the table is rebuilt (there are no users).
- **LLM (answer generation)**: still the external llama.cpp server managed by
  `screensearch-llm`, but now **model-agnostic**: it auto-discovers any `*.gguf`
  in `.models/` (or the app models dir) and uses the first found; Ministral-3B
  remains the downloadable fallback.
- **CI fix (commit `e44de3b`, included in the squash)**: `fastembed` is built
  with `default-features = false, features = ["ort-load-dynamic",
  "hf-hub-native-tls"]`. The default static-link of ONNX Runtime failed to link
  under the repo's forced static-CRT + lld config (unresolved UCRT symbols).
  Dynamic loading fixes the link **and** is the correct model for a portable
  Windows binary. **This is why the DLL bundling task in §4 exists.**

All CI on the merged commit was green: `rust` (windows-latest), `Cross-Compile
Windows Binary from Linux`, `frontend`, `rust-audit`, `evaluation`,
`claude-review`. CI proves the code **compiles, links, and unit-tests** on
Windows; it does **not** execute the models (no ONNX session is created in
tests), which is exactly why runtime validation on Windows is still required.

---

## 3. Current authoritative facts (do not deviate)

Verified constants (grep them to re-confirm before relying on them):

| Fact | Value | Location |
|---|---|---|
| Embedding model | EmbeddingGemma-300M | `screensearch-embeddings/src/lib.rs` `MODEL_NAME` |
| fastembed variant | `EmbeddingModel::EmbeddingGemma300MQ` | `screensearch-embeddings/src/engine.rs` |
| Embedding dimension | **768** | `EMBEDDING_DIM` (lib.rs), `float[768]` (migrations.rs), guard in `queries.rs` |
| Provider string | `fastembed` | `EmbeddingEngine::provider()` in engine.rs |
| Reranker (optional, default OFF) | `bge-reranker-v2-m3` | `RERANKER_MODEL_NAME` (lib.rs) |
| ONNX Runtime version required | **1.24.2** | pinned by `ort-sys` 2.0.0-rc.12 (pyke CDN `ms@1.24.2`) |
| ONNX Runtime load mode | dynamic (`onnxruntime.dll` at runtime) | `screensearch-embeddings/Cargo.toml` feature `ort-load-dynamic` |
| DLL search override env var | `ORT_DYLIB_PATH` | `configure_ort_dylib_path()` in engine.rs |
| Embedding worker batch default | 50 | `[embeddings] batch_size` in `config.toml` |
| API port | 3131 | `[api]` in `config.toml` |

EmbeddingGemma prompt prefixes are applied in `engine.rs` (required by the model
card; do not remove): query = `"task: search result | query: "`, document =
`"title: none | text: "`.

---

## 4. The remaining objective for THIS (Windows) session

Two deliverables, in this order:

**A. Prove the runtime works** (manual validation on Windows — §5 steps 1–6).

**B. Make the embedding runtime ship correctly**: bundle the matching
`onnxruntime.dll` (ONNX Runtime **1.24.2**, x64) so the installer and portable
ZIP produce a working semantic-search install. The Rust engine already looks for
`onnxruntime.dll` next to the executable (and honors `ORT_DYLIB_PATH`); what is
missing is the DLL actually being placed there by the build/release tooling.

Graceful-degradation note (already implemented, verify it holds): if
`onnxruntime.dll` is absent, embedding-engine initialization fails and the app
**must not crash** — semantic search falls back to FTS5 keyword search, and
`engine_ready` is `false`. OCR and keyword search work without the DLL.

---

## 5. Step-by-step strategy for this session (follow in order)

### Step 0 — Sync and confirm starting point
```powershell
cd C:\Users\nicol\Documents\GitHub\screensearch   # adjust to your path
git checkout main
git pull --ff-only origin main
git log --oneline -1     # MUST show: 19d41cd Revert PR #63 sidecar; ... (#66)
```
If the top commit is not `19d41cd` (or a later commit that includes it), stop and
reconcile before proceeding.

### Step 1 — Obtain `onnxruntime.dll` (version 1.24.2, x64 CPU)
Pick ONE source:
- **Microsoft release (recommended, canonical):**
  `https://github.com/microsoft/onnxruntime/releases/download/v1.24.2/onnxruntime-win-x64-1.24.2.zip`
  → unzip → copy `lib\onnxruntime.dll` (and, if present alongside it,
  `onnxruntime_providers_shared.dll`).
- **pyke CDN (the exact artifact `ort` itself uses; LZMA2 tarball):**
  `https://cdn.pyke.io/0/pyke:ort-rs/ms@1.24.2/x86_64-pc-windows-msvc.tar.lzma2`
  (sha256 `b685bfc8d336e0ba95c066a7a982c03aa6dedd528a492eb99ca4ccb7f3af9e7a`).

The version MUST be 1.24.2 to match `ort-sys` 2.0.0-rc.12. A mismatched DLL will
fail at session creation with an API-version error.

### Step 2 — Build the app on Windows
The repo's `.cargo/config.toml` pins `lld-link` + static CRT for the
`x86_64-pc-windows-msvc` target (this is for Linux→Windows cross-compile). On a
native Windows host you have two correct options — **do NOT edit
`.cargo/config.toml`**:

- **Option A (use the project script, handles the linker for you):**
  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts\build-local.ps1
  # produces target\release\screensearch-local\screensearch.exe
  ```
- **Option B (plain cargo, if LLVM/`lld-link` is on PATH):**
  ```powershell
  cd screensearch-ui; npm ci; npm run build; cd ..
  $env:SKIP_UI_BUILD = "1"
  cargo build --release
  ```
  If you get `linker 'lld-link' not found`, either install LLVM (gives
  `lld-link.exe`) or import MSVC vcvars and override the linker exactly as
  `scripts\build-release.ps1` step 2 does:
  `$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = "link.exe"`.

### Step 3 — Place the DLL and a test model
```powershell
# DLL next to the exe (engine auto-detects it; see configure_ort_dylib_path)
copy onnxruntime.dll target\release\screensearch-local\
# A small GGUF for answer generation (auto-discovered). The repo .models/ on the
# Linux box held gemma-4-12b-it-qat-q4_0.gguf (~6.8GB); on a CPU-only/modest box
# a ~4B model is faster. Drop any *.gguf here:
mkdir .models 2>$null
# copy <your-model>.gguf .models\
```

### Step 4 — Run and enable embeddings
```powershell
$env:RUST_LOG = "info"
target\release\screensearch-local\screensearch.exe
# Browser opens http://localhost:3131
```
Enable embeddings (off by default): in the UI Settings → Data & AI, toggle
Semantic Search on and click **Download / verify** (this calls
`POST /api/embeddings/models/prepare`, which loads EmbeddingGemma — first run
downloads ~a few hundred MB from HuggingFace and caches it). Or via API:
```powershell
curl -X POST http://localhost:3131/api/embeddings/enable -H "Content-Type: application/json" -d "true"
curl -X POST http://localhost:3131/api/embeddings/models/prepare
```

### Step 5 — Validate each runtime behavior (record actual output)
```powershell
curl http://localhost:3131/api/embeddings/status
```
EXPECT (the important fields): `"provider":"fastembed"`, `"dimension":768`,
`"engine_ready":true`, and after the worker runs, `frames_with_embeddings` > 0
with `coverage_percent` rising.

Then exercise search and generation:
```powershell
curl "http://localhost:3131/search?q=some+text+you+know+is+on+screen&use_embeddings=true"
curl http://localhost:3131/api/ai/model/status        # available_models should list your .models\*.gguf
curl -X POST http://localhost:3131/api/ai/generate -H "Content-Type: application/json" -d "{\"provider_url\":\"local\",\"model\":\"local\",\"prompt\":\"Summarize my last hour\"}"
```
EXPECT: semantic/hybrid search returns frames; `available_models` contains the
absolute path of your GGUF; `/api/ai/generate` returns a non-empty grounded
report. (The external llama.cpp server + its binary download is handled by
`screensearch-llm`; first generate may take time to fetch/start the server.)

Verify graceful degradation: temporarily remove `onnxruntime.dll`, restart, and
confirm the app still runs, `engine_ready` is `false`, and keyword `/search`
(without `use_embeddings`) still returns results.

### Step 6 — If anything fails, capture and diagnose
Record the exact error text and the `RUST_LOG=info`/`debug` output. The most
likely failure is an ONNX Runtime version/ABI mismatch (wrong DLL) → use exactly
1.24.2. The second most likely is the linker issue in Step 2 → use Option A.

### Step 7 — Implement deliverable B (DLL bundling), on a new branch
Create a branch (never commit to `main` directly), e.g.
`git checkout -b feature/bundle-onnxruntime-dll`, then:
- **Installer** `installer/screensearch.iss`: add a `[Files]` line copying
  `onnxruntime.dll` (staged into `..\target\release\`) into `{app}`. (PR #66
  removed the old sidecar `[Files]` lines here; this adds the DLL beside the exe.)
- **Portable ZIP** in `scripts\build-release.ps1` (step "Create Portable ZIP",
  the `Compress-Archive -Path` list) and the `.github/workflows/release.yml`
  "Create Portable ZIP" step: include `onnxruntime.dll` next to
  `screensearch.exe`.
- **Acquire the DLL in CI**: in `.github/workflows/release.yml` add a step before
  packaging that downloads ONNX Runtime 1.24.2 and copies `onnxruntime.dll` to
  `target\release\`. Pin the version and verify a checksum.
- **`scripts\build-local.ps1`**: copy a local `onnxruntime.dll` into the
  assembled `screensearch-local\` dir so local Windows runs work out of the box.
- Re-run validation (§5) against the packaged artifact, then open a PR to `main`
  and squash-merge once CI is green.

---

## 6. `onnxruntime.dll` specifics (unambiguous)

- Required version: **1.24.2**, architecture **x64**, CPU build is sufficient
  (DirectML/CUDA are optional accelerators, not needed for correctness).
- How the engine finds it (already implemented, do not re-implement):
  `screensearch-embeddings/src/engine.rs` → `configure_ort_dylib_path()` runs
  once on engine init. If `ORT_DYLIB_PATH` is unset, it looks for
  `onnxruntime.dll` in the **same directory as the running executable**; if found
  it sets `ORT_DYLIB_PATH` to it; otherwise `ort` uses its default OS search
  (system `PATH`). So: placing the DLL beside `screensearch.exe` is sufficient.
- For ad-hoc runs you can instead set the env var explicitly:
  `$env:ORT_DYLIB_PATH = "C:\full\path\to\onnxruntime.dll"`.

---

## 7. Files worked on this session / required next session

These are the files that define the current behavior. Read the starred (★) ones
before doing the DLL bundling.

In-process ML core:
- ★ `screensearch-embeddings/Cargo.toml` — fastembed `ort-load-dynamic` feature.
- ★ `screensearch-embeddings/src/engine.rs` — fastembed engine, EmbeddingGemma
  variant, prompt prefixes, `configure_ort_dylib_path()`.
- `screensearch-embeddings/src/lib.rs` — `EMBEDDING_DIM=768`, `MODEL_NAME`,
  `EmbeddingConfig`.
- `screensearch-embeddings/src/chunker.rs` — in-process chunker (+ tests).
- `screensearch-db/src/migrations.rs` — `MIGRATION_009_QUALITY_RAG` `float[768]`.
- `screensearch-db/src/queries.rs` — 768-dim query guard, status defaults.
- `screensearch-db/src/lib.rs` — `pub const EMBEDDING_DIM: usize = 768;`.

OCR (native WinRT):
- `screensearch-capture/src/ocr_provider.rs` — native-only, language fallback.
- `screensearch-capture/src/ocr_processor.rs` — `OcrProcessorConfig` (language).
- `screensearch-capture/src/ocr.rs` — the WinRT engine (unchanged in behavior).

API / app wiring:
- `screensearch-api/src/state.rs` — `get_embedding_engine`,
  `embedding_engine_initialized`, `get_llama_server` (uses `resolve_model_path`).
- ★ `screensearch-api/src/handlers/embeddings.rs` — `engine_ready` status field,
  `prepare_quality_models` pre-loads the model.
- `screensearch-api/src/handlers/ai.rs` — `available_models`,
  `local_model_available()` delegate, model status/download.
- `screensearch-api/src/handlers/system.rs` — `test_vision_config` uses
  `local_model_available()`.
- `screensearch-api/src/handlers/rag_helpers.rs` — query embed + hybrid + RRF.
- `src/main.rs` — sidecar removed; `OcrSettings` simplified.

LLM model discovery:
- ★ `screensearch-llm/src/download.rs` — `discover_local_models`,
  `resolve_model_path`, `local_model_available`, `model_search_dirs`.
- `screensearch-llm/src/lib.rs` — re-exports of the above.

Config / build / packaging (★ = edit for deliverable B):
- `config.toml` — `[ocr] language`, `[embeddings]`, `[llm]` model note.
- `.gitignore` — `/.models/` ignored.
- ★ `installer/screensearch.iss` — add `onnxruntime.dll` to `[Files]`.
- ★ `scripts/build-release.ps1`, ★ `scripts/build-local.ps1` — bundle the DLL.
- `scripts/build-release.sh`, `scripts/build-local.sh` — Linux build (no DLL).
- ★ `.github/workflows/release.yml` — acquire + bundle the DLL; portable ZIP.
- `.github/workflows/quality.yml` — CI (rustfmt file list, clippy, tests, eval).
- `.github/workflows/cross-compile-linux.yml` — cross-compile job.

Docs (current architecture; update if behavior changes):
- `CLAUDE.md`, `CHANGELOG.md` (`[Unreleased]`), `README.md`, `RELEASE_NOTES.md`,
  `docs/architecture.md`, `docs/api-reference.md`, `docs/ai-quality-stack.md`,
  `docs/embedded-llm.md`, `docs/PROJECT_INDEX.md`, `docs/CODE_NAVIGATION.md`.
- This handoff: `docs/WINDOWS_RUNTIME_VALIDATION_HANDOFF.md`.

Deleted this session (do not look for them): `sidecar/` (whole dir),
`screensearch-capture/src/sidecar_ocr.rs`, `docs/SESSION_HANDOFF.md`,
`docs/SESSION_HANDOFF_PR63.md`, `docs/NEXT_SESSION_HANDOFF.md`,
`docs/TODO/explore-pure-rust-onnx-vs-sidecar.md`.

---

## 8. Key code excerpts to orient quickly

DLL discovery (`screensearch-embeddings/src/engine.rs`):
```rust
fn configure_ort_dylib_path() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if std::env::var_os("ORT_DYLIB_PATH").is_some() { return; }
        let lib_name = if cfg!(windows) { "onnxruntime.dll" } /* ... */;
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join(lib_name);
                if candidate.exists() { std::env::set_var("ORT_DYLIB_PATH", &candidate); }
            }
        }
    });
}
```

fastembed dependency (`screensearch-embeddings/Cargo.toml`):
```toml
fastembed = { version = "5.17", default-features = false, features = [
    "ort-load-dynamic",
    "hf-hub-native-tls",
] }
```

GGUF auto-discovery (`screensearch-llm/src/download.rs`): `model_search_dirs()`
scans `.models` then `models` (CWD), then the same beside the exe, then AppData;
`resolve_model_path()` returns the first discovered `*.gguf` else the default
download path; `local_model_available()` = discovered OR default present.

sqlite-vec contract (`screensearch-db/src/migrations.rs`, MIGRATION_009):
```sql
CREATE VIRTUAL TABLE IF NOT EXISTS embedding_vectors USING vec0(
    embedding_id INTEGER PRIMARY KEY,
    embedding float[768] distance_metric=cosine
);
```

---

## 9. Hard rules for this session (avoid these specific mistakes)

- Do **not** edit `.cargo/config.toml` to fix the linker — use a build script or
  install LLVM (§5 Step 2).
- Do **not** statically link ONNX Runtime (do not re-enable
  `ort-download-binaries`) — it does not link under this repo's CRT/linker
  config. Keep `ort-load-dynamic`.
- Use ONNX Runtime **1.24.2** exactly. No other version.
- The embedding dimension is **768** everywhere. If you ever change the model,
  change `EMBEDDING_DIM` (lib.rs), the `float[N]` in MIGRATION_009, the guard in
  `queries.rs`, the metadata in MIGRATION_009, and any test fixtures together.
- Do **not** reintroduce a Python sidecar, PaddleOCR/PP-OCR, Qwen3, or port 3132.
- Do **not** commit to `main` directly; work on a feature branch and open a PR
  (the previous session merged via squash PR).
- The `.models/` directory is gitignored; large GGUFs are never committed.

---

## 10. Definition of done for this session

1. The five runtime behaviors in §1 are observed to succeed on Windows, with the
   actual `GET /api/embeddings/status` JSON recorded showing
   `provider":"fastembed"`, `dimension":768`, `engine_ready":true`.
2. `onnxruntime.dll` (1.24.2) is bundled by the installer, the portable ZIP, and
   `scripts/build-local.ps1`, and acquired in `release.yml`, so a fresh install
   has working semantic search.
3. Graceful degradation without the DLL is confirmed (app runs, keyword search
   works, `engine_ready":false`).
4. Changes land on `main` via a green-CI squash PR, and `CHANGELOG.md` is updated
   to note the bundled `onnxruntime.dll`.
