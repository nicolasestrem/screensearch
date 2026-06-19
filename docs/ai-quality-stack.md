# AI Quality Stack

ScreenSearch runs its quality inference fully in-process in Rust:

- Native Windows OCR (WinRT `Media.Ocr`) performs OCR in-process and returns
  confidence and line bounding boxes (`screensearch-capture/src/ocr.rs`,
  provider wrapper `screensearch-capture/src/ocr_provider.rs`).
- EmbeddingGemma-300M (via the `fastembed` crate on ONNX Runtime / `ort` 2.x)
  produces normalized 768-dimensional vectors
  (`screensearch-embeddings`, `engine.rs`).
- An optional cross-encoder (`bge-reranker-v2-m3`, also via `fastembed`,
  **off by default**) can rerank candidates produced by FTS5 and sqlite-vec.
- sqlite-vec stores and searches vectors in SQLite. Reciprocal Rank Fusion
  combines lexical and semantic result ranks; reranking, when enabled, runs
  afterward.

There is no separate inference process — OCR and embeddings run in the main
binary. The only external component is the answer-generation LLM (see
**Answer Generation** below), which runs as a managed llama.cpp server.

The embedding model (EmbeddingGemma-300M) is downloaded and cached from
HuggingFace on first use. Each embedding stores provider, model, dimension, and
content hash. A changed model contract clears incompatible vectors and queues a
resumable reindex.

Embedding model initialization is serialized inside the engine. Concurrent
first-use requests cannot create duplicate model instances. Use
**Download / verify** before enabling indexing or issuing the first AI request
when predictable first-request latency matters.

OCR uses the native Windows OCR engine, so there is no OCR model download and no
external Paddle/PyTorch runtime. A typical frame takes roughly 70–80 ms to OCR.

Because OCR cost scales with pixel count, the capture client downscales frames to
a 2000 px longest side before OCR. The OCR engine returns boxes in the coordinate
space of the image it received, so the client multiplies them back by the inverse
scale to recover original-frame coordinates; stored frame resolution is
unaffected.

Quality changes must be measured with the versioned cases under `evaluation/`.
Track Recall@10, MRR, OCR character/word error rate, citation correctness,
p95 latency, peak memory, and index size on Windows systems.

## Startup And Initialization

OCR is available immediately because the native Windows OCR engine requires no
model download or warm-up. The embedding engine loads its model
(EmbeddingGemma-300M) lazily on first use or when **Download / verify** is
invoked; the REST API and web UI are available within about two seconds of
startup regardless. The embedding worker retries until the embedding model is
ready.

## Performance Notes

OCR runs in-process via the Windows OCR API at roughly 70–80 ms/frame, so it is
no longer the throughput bottleneck and requires no GPU. Embedding inference runs
on CPU through ONNX Runtime (`ort`); the model is small (300M parameters,
quantized variant `EmbeddingGemma300MQ`), keeping memory and latency modest.

## Fixed And Configurable Components

Native Windows OCR, EmbeddingGemma-300M embeddings, sqlite-vec, and RRF are fixed
quality-stack components for this release. The optional `bge-reranker-v2-m3`
cross-encoder is off by default and can be enabled for higher precision. The
provider options shown under **Answer Generation** configure only the LLM that
writes descriptions, answers, digests, and reports.

Changing the generation LLM does not change OCR or retrieval. Remote generation
can receive selected grounded context; the quality stack itself remains local.

## Answer Generation

Answer generation (AI reports and RAG answers) is handled by an **external
llama.cpp server** (Vulkan GPU with CPU fallback) managed by the
`screensearch-llm` crate over an OpenAI-compatible HTTP API. The runtime is
model-agnostic: it auto-discovers any `*.gguf` file dropped into `.models/`
(repo root) or the application models directory and uses the first one it finds
(`resolve_model_path` / `discover_local_models`). Ministral-3B remains the
downloadable default fallback when no local GGUF is present.
`GET /api/ai/model/status` reports the active model and an `available_models`
list of discovered GGUF files.

## Build And Packaging

Run `npm ci` in `screensearch-ui` before the first Cargo build. The API build
script then rebuilds changed frontend sources before embedding them. No Python is
required to build or run ScreenSearch — there is no sidecar to build or bundle.

The Rust crates compile with `cargo build` and cross-compile cleanly to
`x86_64-pc-windows-msvc` via `cargo-xwin`; `fastembed` and `ort` link without any
toolchain-provided import libraries (see the Developer Guide → "Native Windows
release build" and `docs/cross-compilation.md`). The release build is normally
produced by `scripts/build-release.ps1 -Version <x>`, which configures the MSVC
linker and locates Inno Setup automatically.

## Index Integrity And Retrieval

All chunks for one frame are replaced in a single SQLite transaction. Metadata
rows and sqlite-vec rows either commit together or remain unchanged. The
sqlite-vec `vec0` virtual table `embedding_vectors` is declared as
`embedding float[768] distance_metric=cosine`, and the schema enforces a unique
`(frame_id, chunk_index)` identity.

For time-filtered search, ScreenSearch overfetches global sqlite-vec candidates,
retries with a larger window when too few survive, and finally searches the
complete vector index when needed. This prevents false-empty results for narrow
time windows.

The embedding engine requires the response to match the configured model,
768-dimensional contract, input count, and per-vector dimension. Model-name
validation is intentionally strict because mixing a different model into an
existing index invalidates similarity scores. Index metadata records
`embeddings_model="EmbeddingGemma-300M"`, `embeddings_provider="fastembed"`, and
`embeddings_dimension="768"`.

Changing the embedding model contract clears incompatible legacy vectors.
Captures and OCR text are retained, but large histories may take substantial time
to return to 100% indexing coverage.
