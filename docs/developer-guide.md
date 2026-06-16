# ScreenSearch Developer Guide

## Prerequisites

- Rust stable with `rustfmt` and `clippy`;
- Node.js 22 and npm;
- Python **3.12.1 or newer** for sidecar development — **do not use 3.12.0**, which
  produces a sidecar that crashes at startup (see "Native Windows release build");
- Windows 10/11 and Visual Studio Build Tools for production validation;
- Inno Setup 6 or 7 for installer builds.

Linux is suitable for frontend work and most Rust checks. Windows remains the
authoritative platform for capture, Windows OCR fallback, UI Automation, tray
integration, sidecar packaging, and installers.

## Repository Setup

```bash
npm ci --prefix screensearch-ui
cargo check
```

Do not install dependencies or write model caches into the repository.

## Embedded Frontend Build

The dashboard is embedded by `screensearch-api` using `rust-embed`. Install
frontend dependencies before the first Cargo build:

```bash
cd screensearch-ui
npm ci
cd ..
cargo build
```

The API build script watches frontend sources and runs `npm run build`
automatically before embedding assets. Use `SCREENSEARCH_SKIP_UI_BUILD=1` only
when a current `screensearch-ui/dist/` already exists.

## Running Locally

Start the sidecar:

```bash
python -m pip install -r sidecar/requirements.txt
python sidecar/app.py
```

In another terminal:

```bash
cargo run
```

Normally the application starts the bundled sidecar itself. Direct sidecar
startup is useful for development.

The model loaders serialize first initialization around their cached loader.
Keep this behavior when changing model lifecycle code: `lru_cache` alone can
execute a loader more than once when concurrent cache misses arrive. Use
`POST /v1/models/prepare` before load testing to exclude initialization from
steady-state latency measurements.

To require authentication in manual development, set the same token for both
processes:

```powershell
$env:SCREENSEARCH_AI_SIDECAR_TOKEN = "development-token"
```

## Current AI Contracts

Do not change these independently:

| Contract | Value |
|---|---|
| OCR provider | PP-OCRv5 |
| Embedding model | `Qwen/Qwen3-Embedding-0.6B` |
| Reranker model | `Qwen/Qwen3-Reranker-0.6B` |
| Embedding dimension | 1024 |
| Vector distance | cosine |
| sqlite-vec table | `embedding_vectors` |

Changing embedding model or dimension requires:

1. a new database migration;
2. a new sqlite-vec table contract;
3. metadata invalidation;
4. a full reindex;
5. evaluation evidence.

## OCR Development

Provider routing lives in:

- `screensearch-capture/src/ocr_provider.rs`;
- `screensearch-capture/src/sidecar_ocr.rs`;
- `screensearch-capture/src/ocr.rs`.

PP-OCR response changes must preserve confidence, language, orientation, and
bounding boxes. Test Windows fallback behavior on Windows.

## Retrieval Development

Key files:

- `screensearch-db/src/migrations.rs`;
- `screensearch-db/src/queries.rs`;
- `screensearch-db/src/vector_search.rs`;
- `screensearch-embeddings/src/engine.rs`;
- `screensearch-api/src/handlers/rag_helpers.rs`.

Retrieval sequence:

1. tokenizer-aware document chunking;
2. Qwen document embeddings;
3. sqlite-vec cosine KNN;
4. FTS5 lexical candidates;
5. RRF with `k = 60`;
6. Qwen reranking;
7. context assembly with `[frame:<id>]` citations.

Do not reintroduce in-memory full-vector scans or synthetic hash embeddings.

## Generation Development

Generation is independent from retrieval. Existing database field names use
`vision_*`, but the UI describes them as answer-generation settings.

Supported runtimes:

- bundled Ministral through `screensearch-llm`;
- Ollama-compatible;
- OpenAI-compatible.

The local model-management routes belong to generation only. They must not be
used as quality-sidecar status.

## Local Linux Build

```bash
./scripts/build-local.sh
```

Output:

```text
target/debug/screensearch-local/
  screensearch
  bin/screensearch-ai-sidecar/
```

The script builds the dashboard, native Linux Rust executable, and native Linux
PyInstaller sidecar. Use `--release` for `target/release/screensearch-local`.
Use `--skip-sidecar-deps` only after the pinned Python requirements are already
installed.

This development bundle is not a Windows distributable. Linux cannot produce a
valid Windows PyInstaller sidecar.

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

python -m py_compile sidecar/app.py sidecar/build.py evaluation/evaluate.py
```

Run native capture, OCR fallback, automation, sidecar bundle, and installer
tests on Windows.

The repository has historical formatting and clippy findings outside the
modernized crates. CI scopes strict checks to the changed quality stack while
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
3. builds the Python sidecar;
4. compiles the quality installer;
5. creates the portable ZIP;
6. publishes checksums.

From Linux, validate and cross-compile with:

```bash
./scripts/build-release.sh 0.4.35
```

This produces an explicitly labeled Windows core-preview ZIP under
`target/x86_64-pc-windows-msvc/release/bundles/`. It is useful for checking the
cross-compiled executable but does not contain the Windows sidecar.

Build and download complete Windows artifacts without publishing a tag:

```bash
./scripts/build-release.sh 0.4.35 --windows-bundle
```

The helper matches the dispatched workflow by branch, commit SHA, and dispatch
time so an earlier successful run cannot be mistaken for the new build.
The Windows workflow recreates its portable staging directory after restoring
build caches, preventing stale sidecar files from contaminating or blocking a
new archive.

The installer, portable ZIP, and checksums are downloaded under
`target/x86_64-pc-windows-msvc/release/bundles/windows-full/`.

Publish the release tag only after validation:

```bash
./scripts/build-release.sh 0.4.35 --publish
```

The pushed tag triggers `.github/workflows/release.yml` on a Windows runner.
That job builds the Windows Python sidecar, Inno Setup installer, portable ZIP,
checksums, and draft GitHub release. Before packaging, it starts the generated
PyInstaller executable, prepares the English PP-OCRv5 models, and recognizes a
generated image. A healthy HTTP endpoint alone is not sufficient for the
release job to pass.

### Native Windows release build

`scripts/build-release.ps1` builds the entire bundle directly on a Windows host
(frontend, Rust binary, PyInstaller sidecar, Inno Setup installer, portable ZIP,
checksums):

```powershell
.\scripts\build-release.ps1 -Version 0.4.37 -PythonExe "C:\Path\to\python3.12\python.exe"
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

`-PythonExe` selects the interpreter used to install `sidecar/requirements.txt`
and run `sidecar/build.py`. **Use Python 3.12.1 or newer.** Python 3.12.0 has a
PEP 709 (inlined-comprehension) code-generation bug that corrupts module-level
loop/comprehension bindings in large modules once frozen by PyInstaller. It makes
the bundled sidecar abort at startup with
`NameError: name 'obj' is not defined` in `scipy/stats/_distn_infrastructure.py`,
which takes down sentence-transformers and the whole sidecar — OCR then silently
falls back to Windows OCR and embeddings never index. Plain `python`/`python -OO`
imports succeed; only the frozen build fails. Verified by bisection: same scipy
1.17.1 + PyInstaller 6.21.0, only the interpreter changed — 3.12.0 fails, 3.12.10
works. `sidecar/build.py` hard-errors when run under Python < 3.12.1 so a broken
sidecar can never be produced.

A clean patched interpreter can be provisioned with `uv` without touching the
system Python:

```powershell
uv venv --python 3.12 C:\path\to\venv
uv pip install --python C:\path\to\venv\Scripts\python.exe pip -r sidecar\requirements.txt
.\scripts\build-release.ps1 -Version 0.4.37 -PythonExe "C:\path\to\venv\Scripts\python.exe"
```

Use `-SkipSidecar` to reuse an existing `sidecar\dist\screensearch-ai-sidecar`,
`-SignBinary` to sign, and `-Clean` to force a from-scratch Rust build.

`sidecar/build.py` also copies distribution metadata for PaddleX OCR
dependencies. PaddleX validates optional OCR dependencies through
`importlib.metadata`; bundling only their Python modules causes pipeline
creation to fail even when the modules are importable.

The sidecar verifies PaddleOCR major version 3 at startup. The OCR endpoint
rejects encoded uploads above 20 MiB and decoded images above 50 million
pixels. The Rust OCR client sends quality-85 JPEG to avoid full-screen PNG
encoding overhead. Declared multipart requests above 21 MiB are rejected before
endpoint processing; the endpoint also uses a bounded read.

Embedding writers must use `DatabaseManager::insert_embeddings` for complete
frames. It replaces all chunks in one transaction. Do not restore per-chunk
writes because one successful chunk would make an incomplete frame appear
indexed.

PowerShell helpers are retained for maintainers working directly on Windows,
but they are not the primary development or release entrypoints.

Models are not stored in the installer. They are prepared from Settings or
downloaded on first model use and can consume up to 5 GB.

## Security Rules

- Keep API and sidecar bound to loopback.
- Never commit API keys, databases, captures, logs, model weights, or caches.
- Treat OCR text and frame metadata as sensitive.
- Validate remote generation URLs and disclose that grounded context leaves
  the machine.
- Avoid logging bearer tokens or full OCR context.
