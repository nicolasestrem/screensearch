# Testing Guide

## Fast Validation

```bash
cargo check --locked \
  -p screensearch-db \
  -p screensearch-embeddings \
  -p screensearch-capture \
  -p screensearch-api

cargo test --locked -p screensearch-db
cargo test --locked -p screensearch-embeddings --lib
```

Frontend:

```bash
cd screensearch-ui
npm ci
npm run lint
npm run build
npm audit --audit-level=high
```

## Database Coverage

Database integration tests use temporary SQLite files and cover:

- migrations;
- frames and OCR text;
- FTS5;
- time and application filters;
- tags and pagination;
- metadata;
- retention cleanup;
- sqlite-vec KNN;
- vector synchronization after embedding deletion.

Run:

```bash
cargo test -p screensearch-db
```

## Embedding Coverage

Embedding unit tests cover:

- current constants and dimensions;
- configuration defaults;
- content hash stability;
- legacy chunker behavior.

The fastembed model is not downloaded during unit tests. End-to-end embedding
inference is validated through Windows smoke tests and evaluation.

## Retrieval Evaluation

`evaluation/cases.jsonl` is the versioned retrieval contract. Result files use:

```json
{"id":"terminal-error","retrieved_frames":[101,104,99]}
```

Run:

```bash
python evaluation/evaluate.py evaluation/results.jsonl
```

Required retrieval metrics:

- Recall@10;
- MRR;
- citation correctness;
- p95 retrieval latency;
- peak memory;
- index size.

Required OCR coverage:

- code and terminals;
- browser content;
- small fonts;
- multilingual text;
- rotated or skewed documents;
- duplicate frames;
- confidence calibration;
- CER and WER.

## Windows Validation

The following tests require Windows:

- native screen capture;
- native Windows OCR (WinRT `Media.Ocr`);
- OCR language packs;
- UI Automation;
- system tray;
- bundled llama-server and discovered GGUF model;
- Inno Setup installer;
- portable archive layout.

Linux test binaries can fail to link when upstream capture dependencies
reference Windows symbols. This is a platform boundary, not a retrieval test
failure.

## Manual Quality Smoke Test

1. Build the UI before Rust.
2. Start the application.
3. Confirm `GET /api/embeddings/status` reports the EmbeddingGemma-300M model
   and dimension 768.
4. Start `POST /api/embeddings/models/prepare`, poll status, and confirm
   `engine_ready` becomes `true`.
5. Capture English, multilingual, terminal, and small-font screens.
6. Confirm native Windows OCR metadata is stored.
7. Enable embeddings and wait for coverage to increase.
8. Compare `fts`, `semantic`, and `hybrid` searches.
9. Generate an answer and verify source frame IDs.
10. Restart and confirm indexing resumes.

## CI

`.github/workflows/quality.yml` runs:

- scoped formatting checks;
- strict clippy for modernized crates;
- Rust tests on Windows;
- Rust dependency audit;
- frontend lint, build, and audit.

`.github/workflows/release.yml` performs the Windows production build and
packages the portable artifacts.

## Test Data Safety

Use synthetic or sanitized screenshots. Never commit real screen captures,
OCR containing personal data, databases, API keys, logs, or model caches.
