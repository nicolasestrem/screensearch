# ScreenSearch API Reference

## Conventions

- Base URL: `http://127.0.0.1:3131/api`
- Content type: `application/json`
- Authentication: none on the application API; keep it bound to loopback.
- Timestamps: ISO 8601 UTC.

## Health

### `GET /health`

Returns application health, version, uptime, frame count, and recent capture
information.

### `GET /system/readiness`

Aggregated startup readiness across subsystems, in plain language for the UI's
startup banner. Cheap and non-blocking, designed to be polled ~once a second
while the backend warms up.

```bash
curl "http://127.0.0.1:3131/api/system/readiness"
# -> {"core_ready":true,"all_ready":false,"stages":[
#      {"id":"core","label":"Core services","state":"ready",
#       "detail":"Capture, OCR, and keyword search are running.","progress":null,"eta_seconds":null},
#      {"id":"search_index","label":"Semantic search","state":"loading",
#       "detail":"Loading the search model — the first run downloads ~450 MB.",...},
#      {"id":"answer_generation","label":"AI answer generation","state":"downloading",
#       "detail":"Downloading the local AI server…","progress":42.5,"eta_seconds":37}]}
```

`state` is one of `ready`, `initializing`, `loading`, `downloading`,
`needs_setup`, or `disabled`. `initializing`/`loading`/`downloading` are
*transitional* (timed warm-up); `all_ready` is `true` when none remain. `progress`
(0–100) and `eta_seconds` are present only for `downloading`.

## Search

### `GET /search`

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

Semantic and hybrid modes require the in-process embedding engine to be ready.
Hybrid mode combines FTS5 and sqlite-vec candidates using RRF.

### `GET /search/keywords`

Returns keyword suggestions from OCR content.

## Frames

### `GET /frames`

Returns paginated frames with optional time, monitor, application, and tag
filters.

Each frame object includes `id`, `timestamp`, `app_name`, `window_name`,
`ocr_text`, `tags`, `monitor_index`, and (when available) the vision fields
`description`, `confidence`, `analysis_status`, `activity_type`, `app_hint`, and
`browser_url`. The vision/location fields (`activity_type`, `app_hint`,
`browser_url`, `monitor_index`) let the UI render activity timelines, apps/sites
insights, and per-monitor filtering.

### `GET /frames/:id`

Returns one frame and its OCR, tags, and analysis metadata (same fields as the
list endpoint above).

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
| `GET` | `/tags` | List tags |
| `POST` | `/tags` | Create a tag |
| `PUT` | `/tags/:id` | Update a tag |
| `DELETE` | `/tags/:id` | Delete a tag |

## Embeddings And RAG

### `GET /embeddings/status`

Example response:

```json
{
  "enabled": true,
  "model": "EmbeddingGemma-300M",
  "provider": "fastembed",
  "model_version": "main",
  "dimension": 768,
  "reindex_required": false,
  "engine_ready": true,
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

Pre-loads the in-process embedding model (EmbeddingGemma-300M), downloading and
caching it from Hugging Face on first use:

```bash
curl -X POST "http://127.0.0.1:3131/api/embeddings/models/prepare"
```

Poll `GET /embeddings/status` and check `engine_ready` to confirm the model is
loaded.

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

### Image embeddings (visual recall)

An optional, in-process image-embedding index (nomic-embed-vision-v1.5, 768-dim)
lets text queries retrieve frames by their pixels — including non-OCR visual
content (charts, design canvases, icon-heavy UIs). Image hits are fused into
`hybrid` search results via Reciprocal Rank Fusion. Off by default; the models
download on first use and add per-frame image-embedding CPU.

#### `GET /embeddings/image/status`

Same shape as `GET /embeddings/status` (enabled, model, provider, dimension,
coverage, `engine_ready`, …) but for the image index. `total_frames` counts all
stored frames (no OCR requirement).

#### `POST /embeddings/image/enable`

Enable/disable the image index at runtime (JSON boolean body). The background
image worker is always running and starts/stops processing accordingly.

```bash
curl -X POST "http://127.0.0.1:3131/api/embeddings/image/enable" \
  -H "Content-Type: application/json" \
  -d "true"
```

#### `POST /embeddings/image/generate`

Backfill image embeddings for frames that lack one (same `{"batch_size": N}`
body). Returns immediately; work runs in the background.

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

Retrieval uses hybrid RRF, with optional cross-encoder reranking
(`bge-reranker-v2-m3`, off by default). The selected generation provider writes
the final answer.

## Runtime Settings

### `GET /monitors`

Lists connected displays for the capture monitor picker. Indices come from the
same `screenshots::Screen::all()` source the capture engine uses, so they match
what is actually captured.

```json
[
  { "index": 0, "label": "Monitor 1 (3440x1440) — Primary", "width": 3440, "height": 1440, "is_primary": true },
  { "index": 1, "label": "Monitor 2 (1920x1080)", "width": 1920, "height": 1080, "is_primary": false }
]
```

### `GET /settings`

Returns capture, privacy, retention, and optional generation settings.

### `POST /settings`

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
  "vision_model": "Qwen3-VL-4B-Instruct",
  "vision_endpoint": "http://127.0.0.1:31130",
  "vision_api_key": null
}
```

`monitors` is a JSON array of monitor indices (see `GET /monitors`); an empty
array `"[]"` means **all monitors**. Updating it reconfigures the running capture
engine immediately — no restart required — and the value is restored at startup.

The `vision_*` names are retained by the database API for compatibility. They
configure only the optional generation LLM. OCR and retrieval are configured
through `config.toml` and run in-process.

## Generation LLM

### `POST /ai/validate`

Validates a report-generation provider:

```json
{
  "provider_url": "http://localhost:11434/v1",
  "model": "qwen3:8b"
}
```

Use `"provider_url": "local"` for the bundled llama.cpp runtime. The local
server is model-agnostic: it auto-discovers any `*.gguf` file in `.models/` or
the app models directory and uses the first one found, falling back to the
downloadable default (Ministral-3B) when none is present.

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
| `GET` | `/ai/model/status` | Local model status, including an `available_models` list |
| `POST` | `/ai/model/download` | Start local model download |
| `GET` | `/ai/server/status` | llama-server state, incl. `acceleration` (`gpu`/`cpu`/`unknown`) |
| `POST` | `/ai/server/start` | Start llama-server |
| `POST` | `/ai/server/stop` | Stop llama-server |
| `POST` | `/ai/server/ttl` | Set idle shutdown timeout |
| `POST` | `/ai/server/download` | Download llama-server |

### `POST /test-vision`

Legacy route name retained for compatibility. It tests the configured
generation provider; it does not test OCR or embedding retrieval.

## Vision (Screen Understanding)

On-device screenshot analysis. When vision is enabled with the `local` provider
(see `POST /settings/`), the unified auto-managed llama-server is launched with
`--mmproj` so a single model serves both text and image questions (default
**Qwen3-VL-4B-Instruct**, ~1 s/frame; Gemma&nbsp;4 also works);
analysis populates each frame's `description`, `visible_text`, `activity_type`,
`app_hint`, and `confidence`. Drop a vision GGUF **and** its
`*mmproj*.gguf` projector into `.models/`.

### `POST /vision/analyze/:frame_id`

Enqueue a single frame for vision analysis on demand (high priority — jumps
ahead of the background trickle). The worker performs the analysis
asynchronously.

```bash
curl -X POST "http://127.0.0.1:3131/api/vision/analyze/123"
# -> {"success":true,"frame_id":123,"queue_id":7,"already_queued":false}
```

Returns `404` if the frame does not exist; `queue_id` is `0` (and
`already_queued` is `true`) if the frame was already queued.

### `GET /vision/status`

Aggregate analysis status: configured provider/model plus per-status frame
counts and queue depth.

```bash
curl "http://127.0.0.1:3131/api/vision/status"
# -> {"enabled":1,"provider":"local","model":"Qwen3-VL-4B-Instruct...",
#     "total_frames":1024,"completed":300,"pending":4,"processing":1,
#     "failed":0,"queue_depth":5}
```

### `GET /vision/models`

List the locally discovered vision-capable models — each a GGUF in `.models/`
paired with a matching `*mmproj*.gguf` projector — so the UI can offer a picker
for the local provider. The entry the server currently resolves to (from the
`vision_model` setting) is flagged `selected`.

```bash
curl "http://127.0.0.1:3131/api/vision/models"
# -> {"models":[
#       {"id":"qwen3-vl-4b-instruct-q4_k_m","model_file":"qwen3-vl-4b-instruct-q4_k_m.gguf",
#        "mmproj_file":"mmproj-Qwen3VL-4B-Instruct-F16.gguf","selected":true}],
#     "selected":"qwen3-vl-4b-instruct-q4_k_m"}
```

Set the `vision_model` setting to a model's `id` (or any substring of it) to
select it; the unified local server rebuilds with that model + projector on the
next request.

## Download Progress

### `GET /downloads/status`

Returns progress for managed generation-model and llama-server downloads.
The in-process embedding model (EmbeddingGemma-300M) downloads to the Hugging
Face cache on first use; its readiness is reported through `engine_ready` on
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

Embedding-engine degradation is also represented in
`GET /embeddings/status` through `engine_ready`, `reindex_required`, and
`error`.
