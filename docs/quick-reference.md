# ScreenSearch Quick Reference

## Services

| Service | Address |
|---|---|
| Application and API | `http://127.0.0.1:3131` |
| Quality sidecar | `http://127.0.0.1:3132` |
| Bundled generation server | `http://127.0.0.1:31130` |

## Current Models

| Function | Model |
|---|---|
| OCR | PP-OCRv5 |
| OCR fallback | Windows OCR |
| Embeddings | `Qwen/Qwen3-Embedding-0.6B` |
| Reranking | `Qwen/Qwen3-Reranker-0.6B` |
| Optional bundled generation | Ministral-3-3B GGUF |

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

## Generation Provider

Generation settings are stored through `/api/settings/`. They do not control
OCR or retrieval.

Supported `vision_provider` values:

| Value | Runtime |
|---|---|
| `local` | Bundled Ministral through `llama-server` |
| `ollama` | Ollama-compatible server |
| `openai` | OpenAI-compatible endpoint |

## Configuration

```toml
[ocr]
engine = "ppocr-v5"
sidecar_url = "http://127.0.0.1:3132"
language = "en"
fallback_to_windows = true

[embeddings]
enabled = false
model = "quality-sidecar"
model_name = "Qwen/Qwen3-Embedding-0.6B"
embedding_dim = 1024
batch_size = 50
max_chunk_tokens = 512
chunk_overlap = 64
```

`hybrid_search_alpha` is retained for compatibility but is not used by the
current RRF implementation.

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

Sidecar:

```bash
python -m pip install -r sidecar/requirements.txt
python sidecar/app.py
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
target/release/screensearch-local/bin/screensearch-ai-sidecar/
```

Prepare a Windows release from Linux with:

```bash
./scripts/build-release.sh 0.4.35
./scripts/build-release.sh 0.4.35 --publish
```

The Windows CI runner creates the distributable sidecar and installer after the
tag is pushed.

## Key Paths

| Path | Purpose |
|---|---|
| `config.toml` | Startup configuration |
| `screensearch-ui/dist/` | UI embedded into the Rust binary |
| `sidecar/` | PP-OCRv5 and Qwen inference |
| `evaluation/` | Retrieval quality contract |
| `docs/ai-quality-stack.md` | Model and fallback details |

## Common Problems

| Symptom | Check |
|---|---|
| Old settings UI | Rebuild UI before Rust |
| Sidecar not ready | Port 3132, bundle path, disk, model network access |
| Reindex required | Enable embeddings and allow background processing |
| Semantic search empty | Index coverage and sidecar status |
| Windows OCR active | Search the app log for `Quality sidecar:` and `PP-OCRv5 request failed` |
| Generation unavailable | Selected LLM runtime, model, endpoint, API key |
