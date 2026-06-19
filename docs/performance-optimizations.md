# Performance And Quality Notes

## Capture

- Frame differencing skips visually unchanged screenshots.
- JPEG storage and maximum-width resizing limit disk growth.
- Capture interval is configurable.
- Retention cleanup removes old frames and associated records.

Aggressive capture intervals increase OCR, storage, and indexing load.

## OCR

Native Windows OCR (WinRT `Media.Ocr`) runs in-process, with no model download.
It processes roughly 70-80 ms per frame on CPU.

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

Embeddings are generated in-process via the `fastembed` crate (ONNX Runtime)
using the EmbeddingGemma-300M model, in batches, and stored once. Document
chunking uses 512-token chunks with 64-token overlap.

Each vector is 768 float32 values. sqlite-vec performs KNN inside SQLite,
avoiding a full Rust-side vector scan.

Embedding provenance and content hashes allow stale vectors to be invalidated
when the model contract changes.

## Retrieval

Hybrid retrieval:

1. fetches a wider sqlite-vec candidate set;
2. fetches FTS5 candidates;
3. combines ranks with RRF;
4. optionally reranks candidates with a cross-encoder (`bge-reranker-v2-m3`
   via fastembed, off by default);
5. truncates to the context budget.

RRF is used because lexical and cosine scores are not directly comparable.
The old raw-score weight is retained only as a configuration compatibility
field.

Measure Recall@10 and MRR before tuning candidate counts, RRF constants, or
context size.

## Model Caching

The embedding model (EmbeddingGemma-300M) and the optional reranker
(`bge-reranker-v2-m3`) run in-process via `fastembed`/ONNX Runtime. Weights are
downloaded from Hugging Face and cached on first use. The optional answer-
generation GGUF model is loaded by an external llama.cpp server.

Allow disk space for the model and ONNX Runtime caches.

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
