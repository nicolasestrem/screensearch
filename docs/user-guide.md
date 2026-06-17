# ScreenSearch User Guide

ScreenSearch captures Windows screen activity, extracts searchable text, and
builds a local retrieval index for search, answers, and reports.

## Requirements

- Windows 10 or Windows 11, 64-bit;
- enough storage for captures and the configured retention period;
- up to 5 GB for PP-OCRv5, Qwen3 models, and runtime caches;
- internet access during the first model download;
- a generation provider only if you want generated answers or reports.

## First Run

1. Install or extract the quality build.
2. Start `screensearch.exe`.
3. Open `http://localhost:3131`.
4. In Settings, select **Download / verify** to prepare the quality models.
5. Open **Settings > Data & AI** to check quality-stack readiness.
6. Enable semantic indexing when you want semantic or hybrid search.

The first OCR or embedding request can take longer while models download and
initialize. The settings panel reports sidecar and index status.

## What Each AI Component Does

| Component | Purpose | User-selectable |
|---|---|---|
| PP-OCRv5 | Reads text and layout from screenshots | Fixed default |
| Windows OCR | Fallback when PP-OCRv5 is unavailable | `config.toml` |
| Qwen3-Embedding-0.6B | Converts OCR passages and queries to vectors | Fixed |
| sqlite-vec | Performs local cosine KNN search | Fixed |
| FTS5 + RRF | Combines keyword and semantic candidates | Fixed |
| Qwen3-Reranker-0.6B | Reorders candidates by query relevance | Fixed |
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

The settings panel displays the fixed quality stack:

- PP-OCRv5 with Windows OCR fallback;
- Qwen3-Embedding-0.6B;
- Qwen3-Reranker-0.6B;
- sqlite-vec KNN and FTS5/RRF fusion.

The embedding status card shows:

- whether indexing is enabled;
- sidecar readiness;
- active model contract;
- indexed frame coverage;
- whether reindexing is required;
- the latest error.

Enabling embeddings starts background indexing. Existing OCR frames are
processed in batches. The operation is resumable.

### Answer Generation

Answer generation is optional. It consumes grounded context retrieved by the
quality stack.

Available runtimes:

- **Bundled local Ministral-3-3B**: downloads a GGUF model and uses the bundled
  `llama-server` management endpoints.
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

Semantic mode uses Qwen3 query embeddings and sqlite-vec. It requires a ready
sidecar and an indexed corpus.

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

If the reranker is unavailable, results retain RRF order. If the quality
sidecar is unavailable, the UI reports degraded retrieval. ScreenSearch does
not substitute synthetic embeddings.

## OCR Configuration

OCR is configured in `config.toml` and requires a restart:

```toml
[ocr]
engine = "ppocr-v5"
sidecar_url = "http://127.0.0.1:3132"
sidecar_token_env = "SCREENSEARCH_AI_SIDECAR_TOKEN"
language = "en"
fallback_to_windows = true
min_confidence = 0.7
```

Use `engine = "windows"` to bypass the sidecar OCR provider. Use
`fallback_to_windows = false` when OCR failures should be surfaced instead of
retried through Windows OCR.

## Embedding Configuration

The model contract is fixed by the current database migration and sidecar:

```toml
[embeddings]
enabled = false
model = "quality-sidecar"
model_name = "Qwen/Qwen3-Embedding-0.6B"
embedding_dim = 1024
batch_size = 50
max_chunk_tokens = 512
chunk_overlap = 64
max_context_chunks = 20
```

Do not change `model_name` or `embedding_dim` independently. A different model
requires a schema and migration change because sqlite-vec dimensions are fixed
when the virtual table is created.

When upgrading to v0.4.35, ScreenSearch removes old 384-dimensional embedding
vectors because they are incompatible with the fixed Qwen3 1024-dimensional
index. Captures and OCR text remain intact. Settings reports that existing
vectors must be regenerated until the new index is complete.

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

### Quality sidecar unavailable

- Confirm the installer contains
  `bin/screensearch-ai-sidecar/screensearch-ai-sidecar.exe`.
- For Linux development, run `./scripts/build-local.sh --release`, then use
  `target/release/screensearch-local/screensearch`.
- For Windows, use the installer or portable ZIP produced by the release
  workflow. A cross-compiled `screensearch.exe` by itself is core-only and
  cannot download Qwen models.
- Confirm port `3132` is not occupied by another process.
- Check available disk space.
- Check outbound access to Hugging Face and Paddle model hosts.
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

### Windows OCR fallback is active

This indicates PP-OCRv5 initialization or an individual inference request
failed. Search the ScreenSearch log for `Quality sidecar:` to find the Python
exception and for `PP-OCRv5 request failed` to find the corresponding client
error. Model download, Paddle runtime, and unsupported response-shape errors
are reported there without logging the sidecar token or image contents.

Select **Download / verify** and wait for OCR preparation to become ready. If
the error persists, preserve the `Quality sidecar:` lines when reporting it.
ScreenSearch continues capture with Windows OCR when
`fallback_to_windows = true`; Windows OCR is expected only as a fallback unless
explicitly selected in `config.toml`.

## Data Locations

Production captures, databases, logs, models, and caches belong in the
platform data directories. Never place them in the Git repository.

The sidecar uses standard Hugging Face and Paddle caches. Removing those caches
forces model downloads on the next use.

### Download or verify quality models

Open **Settings > AI > AI Embeddings (RAG)** and select **Download / verify**.
ScreenSearch prepares PP-OCRv5, Qwen3 embeddings, and Qwen3 reranking in the
background. The panel shows the component currently being initialized and any
download or model-loading error. Leave ScreenSearch running until the status is
ready.

The button is available only when the quality runtime is running. If the panel
shows **Quality runtime unavailable**, install or build the sidecar first; no
Qwen download can start without it.

The packaged Windows directory has this layout:

```text
target\release\screensearch-local\
  screensearch.exe
  bin\screensearch-ai-sidecar\screensearch-ai-sidecar.exe
```

Keep the `bin` directory beside `screensearch.exe` when moving the portable
app. Linux development bundles use the same layout without `.exe` suffixes.
