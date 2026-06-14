# ScreenSearch API Reference

## Conventions

- Base URL: `http://127.0.0.1:3131/api`
- Content type: `application/json`
- Authentication: none on the application API; keep it bound to loopback.
- Timestamps: ISO 8601 UTC.
- The quality sidecar on port `3132` is internal and bearer-authenticated when
  managed by ScreenSearch.

## Health

### `GET /health`

Returns application health, version, uptime, frame count, and recent capture
information.

## Search

### `GET /search/`

Query parameters:

| Name | Description |
|---|---|
| `q` | Search query |
| `mode` | `fts`, `semantic`, or `hybrid` |
| `limit` | Maximum results |
| `offset` | Pagination offset |
| `start_time` | Optional lower timestamp |
| `end_time` | Optional upper timestamp |
| `app` | Optional application filter |

Examples:

```bash
curl "http://127.0.0.1:3131/api/search/?q=invoice&mode=fts"
curl "http://127.0.0.1:3131/api/search/?q=why%20did%20the%20build%20fail&mode=hybrid"
```

Semantic and hybrid modes require a ready quality sidecar. Hybrid mode combines
FTS5 and sqlite-vec candidates using RRF.

### `GET /search/keywords`

Returns keyword suggestions from OCR content.

## Frames

### `GET /frames/`

Returns paginated frames with optional time, monitor, application, and tag
filters.

### `GET /frames/:id`

Returns one frame and its OCR, tags, and analysis metadata.

### `GET /frames/:id/image`

Returns the stored frame image.

### `GET /frames/:id/tags`

Returns tags assigned to a frame.

### `POST /frames/:id/tags`

Assigns a tag to a frame.

### `DELETE /frames/:id/tags/:tag_id`

Removes a tag from a frame.

## Tags

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/tags/` | List tags |
| `POST` | `/tags/` | Create a tag |
| `PUT` | `/tags/:id` | Update a tag |
| `DELETE` | `/tags/:id` | Delete a tag |

## Embeddings And RAG

### `GET /embeddings/status`

Example response:

```json
{
  "enabled": true,
  "model": "Qwen/Qwen3-Embedding-0.6B",
  "provider": "quality-sidecar",
  "model_version": "main",
  "dimension": 1024,
  "reindex_required": false,
  "sidecar_ready": true,
  "model_preparation": {
    "state": "ready",
    "current_component": null,
    "ready_components": ["ocr", "embeddings", "reranker"],
    "error": null
  },
  "error": null,
  "total_frames": 1523,
  "frames_with_embeddings": 890,
  "coverage_percent": 58.4,
  "last_processed_frame_id": 1200
}
```

`total_frames` counts frames with OCR text. `reindex_required` remains true
until all eligible frames use the active model contract.

### `POST /embeddings/models/prepare`

Starts background download and initialization of the fixed quality models:

```bash
curl -X POST "http://127.0.0.1:3131/api/embeddings/models/prepare"
```

Poll `GET /embeddings/status` while `model_preparation.state` is `preparing`.
A missing sidecar returns an HTTP error rather than reporting a successful
download.

### `POST /embeddings/enable`

The body is a JSON boolean:

```bash
curl -X POST "http://127.0.0.1:3131/api/embeddings/enable" \
  -H "Content-Type: application/json" \
  -d "true"
```

### `POST /embeddings/generate`

Processes OCR frames without current embeddings:

```json
{
  "batch_size": 50
}
```

The background worker also processes batches while embeddings are enabled.

### `POST /generate`

Generates a grounded answer:

```json
{
  "query": "What deployment issue was I investigating?"
}
```

Example response:

```json
{
  "answer": "The answer includes citations such as [frame:123].",
  "sources": [123, 127]
}
```

Retrieval uses hybrid RRF and Qwen reranking. The selected generation provider
writes the final answer.

## Runtime Settings

### `GET /settings/`

Returns capture, privacy, retention, and optional generation settings.

### `POST /settings/`

Example:

```json
{
  "capture_interval": 3,
  "monitors": "[0]",
  "excluded_apps": "[\"1Password\",\"KeePass\"]",
  "is_paused": 0,
  "retention_days": 30,
  "vision_enabled": 1,
  "vision_provider": "local",
  "vision_model": "ministral-3:3b",
  "vision_endpoint": "http://127.0.0.1:31130",
  "vision_api_key": null
}
```

The `vision_*` names are retained by the database API for compatibility. They
configure only the optional generation LLM. OCR and retrieval are configured
through `config.toml` and the fixed sidecar contract.

## Generation LLM

### `POST /ai/validate`

Validates a report-generation provider:

```json
{
  "provider_url": "http://localhost:11434/v1",
  "model": "qwen3:8b"
}
```

Use `"provider_url": "local"` for the bundled Ministral runtime.

### `POST /ai/generate`

Generates a report over a time range:

```json
{
  "provider_url": "local",
  "model": "ministral-3b",
  "start_time": "2026-06-14T00:00:00Z",
  "end_time": "2026-06-15T00:00:00Z",
  "prompt": "Summarize completed engineering work"
}
```

### Bundled Runtime Management

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/ai/model/status` | Local Ministral model status |
| `POST` | `/ai/model/download` | Start local model download |
| `GET` | `/ai/server/status` | llama-server state |
| `POST` | `/ai/server/start` | Start llama-server |
| `POST` | `/ai/server/stop` | Stop llama-server |
| `POST` | `/ai/server/ttl` | Set idle shutdown timeout |
| `POST` | `/ai/server/download` | Download llama-server |

### `POST /test-vision`

Legacy route name retained for compatibility. It tests the configured
generation provider; it does not test PP-OCRv5 or Qwen retrieval.

## Download Progress

### `GET /downloads/status`

Returns progress for managed generation-model and llama-server downloads.
Quality-sidecar model downloads currently use the Hugging Face and Paddle
caches and are reported through `model_preparation` on
`GET /embeddings/status`, rather than this endpoint.

## Automation

Windows-only endpoints:

| Method | Path |
|---|---|
| `POST` | `/automation/find-elements` |
| `POST` | `/automation/click` |
| `POST` | `/automation/type` |
| `POST` | `/automation/scroll` |
| `POST` | `/automation/press-key` |
| `POST` | `/automation/get-text` |
| `POST` | `/automation/list-elements` |
| `POST` | `/automation/open-app` |
| `POST` | `/automation/open-url` |

These endpoints can control the desktop. Do not expose the API beyond
loopback.

## Errors

Handlers return an HTTP error status and JSON error message for invalid input,
database failures, unavailable inference, or automation failures.

Quality-stack degradation is also represented in
`GET /embeddings/status` through `sidecar_ready`, `reindex_required`, and
`error`.
