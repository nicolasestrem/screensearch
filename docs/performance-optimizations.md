# Performance And Quality Notes

## Capture

- Frame differencing skips visually unchanged screenshots.
- JPEG storage and maximum-width resizing limit disk growth.
- Capture interval is configurable.
- Retention cleanup removes old frames and associated records.

Aggressive capture intervals increase OCR, storage, and indexing load.

## OCR

PP-OCRv5 runs in the managed sidecar and can use CPU or a compatible GPU.
Windows OCR is a fallback, not the primary performance baseline.

Measure:

- p50 and p95 request latency;
- initialization time;
- peak memory;
- confidence coverage;
- CER/WER;
- fallback frequency.

Do not compare providers only by latency. Code, terminal, multilingual, and
small-font accuracy are primary quality requirements.

## Embeddings

Document chunking uses the Qwen tokenizer with 512-token chunks and 64-token
overlap. Embeddings are generated in batches and stored once.

Each vector is 1024 float32 values. sqlite-vec performs KNN inside SQLite,
avoiding a full Rust-side vector scan.

Embedding provenance and content hashes allow stale vectors to be invalidated
when the model contract changes.

## Retrieval

Hybrid retrieval:

1. fetches a wider sqlite-vec candidate set;
2. fetches FTS5 candidates;
3. combines ranks with RRF;
4. reranks candidates with Qwen3-Reranker-0.6B;
5. truncates to the context budget.

RRF is used because lexical and cosine scores are not directly comparable.
The old raw-score weight is retained only as a configuration compatibility
field.

Measure Recall@10 and MRR before tuning candidate counts, RRF constants, or
context size.

## Sidecar Packaging

The sidecar uses a PyInstaller on-directory bundle. A one-file bundle would
unpack the large Torch and Paddle runtime on every launch and can hit archive
size limits.

Model weights are downloaded separately on first use. Allow up to 5 GB of disk
space for models and caches.

## Generation

Generation latency is separate from retrieval latency. Measure:

- retrieval time;
- reranking time;
- prompt assembly;
- first-token latency;
- total generation time.

Remote providers add network latency and transfer grounded context off-device.

## Current Baseline Policy

The repository does not claim fixed production latency numbers without a
reproducible benchmark artifact. Use the versioned evaluation cases and record
hardware, model cache state, CPU/GPU mode, corpus size, and index coverage.
