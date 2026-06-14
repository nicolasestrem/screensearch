# ScreenSearch Developer Guide

## Prerequisites

- Rust stable with `rustfmt` and `clippy`;
- Node.js 22 and npm;
- Python 3.12 for sidecar development;
- Windows 10/11 and Visual Studio Build Tools for production validation;
- Inno Setup 6 for installer builds.

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

## Sidecar Build

```powershell
python -m pip install -r sidecar\requirements.txt
python sidecar\build.py
```

Output:

```text
sidecar/dist/screensearch-ai-sidecar/
```

PyInstaller builds an on-directory bundle. Installer and portable packaging
must preserve that directory structure.

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

Use:

```powershell
.\scripts\build-release.ps1 -Version 0.4.35
```

Models are not stored in the installer. They download on first use and can
consume up to 5 GB.

## Security Rules

- Keep API and sidecar bound to loopback.
- Never commit API keys, databases, captures, logs, model weights, or caches.
- Treat OCR text and frame metadata as sensitive.
- Validate remote generation URLs and disclose that grounded context leaves
  the machine.
- Avoid logging bearer tokens or full OCR context.
