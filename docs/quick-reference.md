# ScreenSearch Quick Reference

## Services

| Service | Address |
|---|---|
| Application and API | `http://127.0.0.1:3131` |
| Bundled generation server | `http://127.0.0.1:31130` |

## Current Models

| Function | Model |
|---|---|
| OCR | native Windows OCR (WinRT `Media.Ocr`, in-process) |
| Embeddings | `EmbeddingGemma-300M` (in-process via `fastembed`, 768-dim) |
| Reranking | `bge-reranker-v2-m3` (optional, off by default) |
| Optional bundled generation | any `*.gguf` (auto-discovered; Ministral-3B default) |

## Search

```bash
curl "http://127.0.0.1:3131/api/search/?q=meeting&mode=fts"
curl "http://127.0.0.1:3131/api/search/?q=deployment%20failure&mode=semantic"
curl "http://127.0.0.1:3131/api/search/?q=deployment%20failure&mode=hybrid"
```

Use `hybrid` for general search, `fts` for exact text, and `semantic` when only
meaning matters.

## Embedding Status

```bash
curl "http://127.0.0.1:3131/api/embeddings/status"
```

Enable indexing:

```bash
curl -X POST "http://127.0.0.1:3131/api/embeddings/enable" \
  -H "Content-Type: application/json" \
  -d "true"
```

Process a batch:

```bash
curl -X POST "http://127.0.0.1:3131/api/embeddings/generate" \
  -H "Content-Type: application/json" \
  -d '{"batch_size":50}'
```

## Grounded Answer

```bash
curl -X POST "http://127.0.0.1:3131/api/generate" \
  -H "Content-Type: application/json" \
  -d '{"query":"What deployment issue was I investigating?"}'
```

Sources are returned as frame IDs. Context sent to the generation LLM is
marked with `[frame:<id>]`.

## Generation And Vision Provider

Generation and vision settings are stored through `/api/settings/`. They do not
control OCR or retrieval.

Supported `vision_provider` values (shared by generation and vision):

| Value | Runtime |
|---|---|
| `local` | Bundled llama.cpp server (auto-discovers `*.gguf`; Ministral-3B default). For vision it loads a Qwen3-VL-4B model + `--mmproj` and serves both text and images. |
| `ollama` | Ollama-compatible server |
| `openai` | OpenAI-compatible endpoint |

### Vision endpoints

| Method | Path |
|---|---|
| `POST` | `/api/vision/analyze/:frame_id` |
| `GET`  | `/api/vision/status` |

Vision is off by default; enable via `vision_enabled=1`. On-demand enqueue plus a
throttled background trickle. See `docs/vision.md`.

## Configuration

```toml
[ocr]
language = "en"
min_confidence = 0.7

[embeddings]
enabled = false
model = "fastembed"
model_name = "EmbeddingGemma-300M"
embedding_dim = 768
batch_size = 50
max_chunk_tokens = 512
chunk_overlap = 64
```

`hybrid_search_alpha` is retained for compatibility but is not used by the
current RRF implementation.

Migration 009 clears incompatible legacy embeddings, and migration 010 enforces
one row per frame chunk. Captures and OCR text are retained; enable embeddings
to rebuild coverage after upgrading.

## Development

Backend:

```bash
cargo check
cargo test -p screensearch-db
cargo test -p screensearch-embeddings --lib
```

Frontend:

```bash
cd screensearch-ui
npm ci
npm run lint
npm run build
```

Evaluation:

```bash
python evaluation/evaluate.py evaluation/results.jsonl
```

## Release Build Order

Install UI dependencies before Cargo. The API build script rebuilds changed
frontend sources before embedding `screensearch-ui/dist/`.

```bash
./scripts/build-local.sh --release
```

The native Linux development output is:

```text
target/release/screensearch-local/screensearch
```

Prepare a Windows release from Linux with:

```bash
./scripts/build-release.sh 0.4.35
./scripts/build-release.sh 0.4.35 --publish
```

The Windows CI runner creates the distributable installer after the tag is
pushed.

## Key Paths

| Path | Purpose |
|---|---|
| `config.toml` | Startup configuration |
| `screensearch-ui/dist/` | UI embedded into the Rust binary |
| `screensearch-embeddings/` | In-process embedding engine (`fastembed`) |
| `.models/` | Auto-discovered `*.gguf` generation models |
| `evaluation/` | Retrieval quality contract |
| `docs/ai-quality-stack.md` | Model details |

## Common Problems

| Symptom | Check |
|---|---|
| Old settings UI | Rebuild UI before Rust |
| Embedding engine not ready | Disk space and Hugging Face model download |
| Reindex required | Enable embeddings and allow background processing |
| Semantic search empty | Index coverage and embedding engine status |
| OCR returns no text | Install the matching Windows OCR language pack |
| Generation unavailable | Selected LLM runtime, model, endpoint, API key |
