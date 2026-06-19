# ScreenSearch User Guide

ScreenSearch captures Windows screen activity, extracts searchable text, and
builds a local retrieval index for search, answers, and reports.

## Requirements

- Windows 10 or Windows 11, 64-bit;
- enough storage for captures and the configured retention period;
- up to 2 GB for the EmbeddingGemma-300M model and runtime caches;
- internet access during the first model download;
- a generation provider only if you want generated answers or reports.

## First Run

1. Install or extract the build.
2. Start `screensearch.exe`.
3. Open `http://localhost:3131`.
4. In Settings, select **Download / verify** to prepare the embedding model.
5. Open **Settings > Data & AI** to check retrieval-stack readiness.
6. Enable semantic indexing when you want semantic or hybrid search.

The first embedding request can take longer while the model downloads and
initializes. The settings panel reports engine and index status. Native Windows
OCR runs in-process and needs no model download.

## What Each AI Component Does

| Component | Purpose | User-selectable |
|---|---|---|
| Windows OCR | Reads text from screenshots in-process (WinRT Media.Ocr) | Fixed |
| EmbeddingGemma-300M | Converts OCR passages and queries to vectors (in-process via fastembed) | Fixed |
| sqlite-vec | Performs local cosine KNN search | Fixed |
| FTS5 + RRF | Combines keyword and semantic candidates | Fixed |
| bge-reranker-v2-m3 | Optionally reorders candidates by query relevance (off by default) | Fixed |
| Generation LLM | Writes answers, descriptions, digests, and reports | Configurable |

The generation provider does not control OCR or RAG retrieval.

## Settings

### Capture

Configure:

- capture interval;
- monitor selection;
- frame differencing;
- pause state.

Short capture intervals increase storage and OCR load. Frame differencing
skips visually unchanged screenshots.

### Privacy

Exclude password managers, banking applications, private communication tools,
or any application that should never be captured. Pause capture when needed.

ScreenSearch stores screenshots and extracted text locally. The SQLite
database is not encrypted by default.

### Data And Retention

Set the number of days retained. Cleanup removes old frame records and their
related OCR and embedding data.

### Screen Understanding And Retrieval

The settings panel displays the fixed retrieval stack:

- native Windows OCR (in-process);
- EmbeddingGemma-300M embeddings (in-process via fastembed);
- optional bge-reranker-v2-m3 reranking (off by default);
- sqlite-vec KNN and FTS5/RRF fusion.

The embedding status card shows:

- whether indexing is enabled;
- engine readiness;
- active model contract;
- indexed frame coverage;
- whether reindexing is required;
- the latest error.

Enabling embeddings starts background indexing. Existing OCR frames are
processed in batches. The operation is resumable.

### Answer Generation

Answer generation is optional. It consumes grounded context retrieved by the
retrieval stack.

Available runtimes:

- **Bundled local llama.cpp server**: auto-discovers any `*.gguf` file in
  `.models/` (repo root) or the app models directory and uses the first one
  found; if none is present, Ministral-3B is the downloadable default. Uses the
  managed `llama-server` endpoints (Vulkan GPU with CPU fallback).
- **Ollama-compatible local server**: configure the base URL and model.
- **OpenAI-compatible endpoint**: configure the base URL, model, and API key.

With a remote provider, the selected OCR passages and frame metadata used for
generation leave the machine. OCR, embeddings, vector search, and reranking
remain local.

## Search Modes

### Keyword

Keyword mode uses SQLite FTS5. Use it for exact terms, code symbols, names,
URLs, and visible phrases.

### Semantic

Semantic mode uses EmbeddingGemma-300M query embeddings and sqlite-vec. It
requires a ready embedding engine and an indexed corpus.

### Hybrid

Hybrid mode is the recommended general-purpose mode. It combines FTS5 and
semantic candidates with Reciprocal Rank Fusion.

Filters for time and application are applied to retrieval. Search results
identify whether a result came from lexical, vector, hybrid, or reranked
retrieval.

## Grounded Answers And Reports

Grounded generation retrieves screen evidence before calling the generation
LLM. Context items carry stable citation identifiers such as `[frame:123]`.

Generated output depends on three independent readiness conditions:

1. OCR text exists for the requested time range.
2. Embeddings are enabled and sufficiently indexed for semantic retrieval.
3. A generation provider is configured and reachable.

If reranking is disabled or unavailable, results retain RRF order. If the
embedding engine is unavailable, the UI reports degraded retrieval. ScreenSearch
does not substitute synthetic embeddings.

## OCR Configuration

OCR uses the native Windows OCR API (WinRT `Media.Ocr`) in-process. It needs no
model download and runs at roughly 70-80 ms per frame. Configure it in
`config.toml` (changes require a restart):

```toml
[ocr]
language = "en"
min_confidence = 0.7
```

## Embedding Configuration

The model contract is fixed by the current database migration and the
in-process embedding engine:

```toml
[embeddings]
enabled = false
model = "fastembed"
model_name = "EmbeddingGemma-300M"
embedding_dim = 768
batch_size = 50
max_chunk_tokens = 512
chunk_overlap = 64
max_context_chunks = 20
```

Do not change `model_name` or `embedding_dim` independently. A different model
requires a schema and migration change because sqlite-vec dimensions are fixed
when the virtual table is created.

When the embedding dimension changes, ScreenSearch removes embedding vectors
that are incompatible with the fixed EmbeddingGemma-300M 768-dimensional index.
Captures and OCR text remain intact. Settings reports that existing vectors must
be regenerated until the new index is complete.

## Troubleshooting

### The settings panel still shows old AI wording

The dashboard is embedded into the Rust binary. Install frontend dependencies
before building the application:

```powershell
cd screensearch-ui
npm ci
cd ..
cargo build --release
```

The API build script rebuilds changed frontend sources automatically. For
diagnosis, run `npm run build` directly and confirm it succeeds.

### Embedding engine unavailable

- The embedding model loads in-process; there is no separate service to start.
- Check available disk space for the model cache.
- Check outbound access to Hugging Face during the first download.
- Select **Download / verify** again if model preparation was interrupted.

### Reindex required

Enable embeddings and leave ScreenSearch running. Indexing resumes in batches.
The warning clears after all OCR frames use the current model contract.

### Semantic search returns few results

- Wait for index coverage to increase.
- Use hybrid search for exact terms and semantic intent together.
- Confirm the requested time and application filters include the relevant
  frames.
- Review OCR confidence and language configuration.

### OCR returns no text

Native Windows OCR requires the matching Windows language pack. Open
**Settings > Time & language > Language** and confirm the OCR language (for
example English) is installed. Search the ScreenSearch log for OCR errors. If
the failure persists, preserve those log lines when reporting it.

## Data Locations

Production captures, databases, logs, models, and caches belong in the
platform data directories. Never place them in the Git repository.

The embedding engine uses the standard Hugging Face cache. Removing that cache
forces a model download on the next use.

### Download or verify the embedding model

Open **Settings > AI > AI Embeddings (RAG)** and select **Download / verify**.
ScreenSearch pre-loads the in-process EmbeddingGemma-300M model in the
background. The panel shows initialization progress and any download or
model-loading error. Leave ScreenSearch running until the status is ready.

Native Windows OCR runs in-process and needs no download. The bundled answer-
generation LLM (llama.cpp) is optional and only used for grounded answers and
reports.
