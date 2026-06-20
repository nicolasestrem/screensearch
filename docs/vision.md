# Vision (Screen Understanding)

ScreenSearch can analyze the *pixels* of each captured screenshot — not just its
OCR text — to produce a short natural-language description, the prominent
on-screen text, an activity classification, and a confidence score. This is
optional and **off by default**.

Vision results are written back onto each frame and are visible through the
frames API (`description`, `visible_text`, `activity_type`, `app_hint`,
`confidence`).

## How it works

When vision is enabled with the **local** provider, ScreenSearch does **not**
start a second model server. Instead it reuses the same auto-managed `llama.cpp`
server that answers AI reports, launching it with `--mmproj` so a single
**Gemma 4** model serves *both* text generation and image analysis. This keeps
one model in VRAM and avoids running two servers (Option B — "unify on
gemma-4").

```
Capture → frames (analysis_status='pending')
                 │
   on-demand  ───┤  POST /api/vision/analyze/:frame_id        (priority 10)
   throttled  ───┘  background trickle of recent un-analyzed frames (batch 4)
                 │
         analysis_queue ──> vision worker (vision_worker.rs)
                 │                 │
                 │      OpenAI-compatible /v1/chat/completions (image_url)
                 │                 │
        unified llama.cpp server (gemma-4 + --mmproj)  ◄── also serves AI reports
                 │
   frame.description / visible_text_json / activity_type / app_hint / confidence
```

The server is rebuilt automatically when vision is toggled: enabling vision
switches the loaded model to the vision model + projector; disabling it switches
back to the first discovered text GGUF.

### Model and projector selection (local provider)

- `resolve_vision_model()` chooses the unified model. It first tries to match the
  `vision_model` setting against a discovered model filename; otherwise it
  prefers a **Gemma 4 E4B** model; otherwise it uses the first vision-capable
  model found.
- `resolve_mmproj_for()` pairs that model with the correct `*mmproj*.gguf`
  projector sitting beside it, matched by **size signature** (e.g. an `E4B` model
  gets the `E4B` projector, a `12B` model the `12B` projector). A text-only model
  such as Qwen3.5 has no matching projector and is never mis-paired.
- A model is "vision-capable" only if a matching projector is found next to it.

## Setup (local, on-device)

1. **Drop a Gemma 4 model and its projector into `.models/`** (repo root, next to
   the executable, or the app models directory). You need both files:
   - a model GGUF, e.g. `gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf`;
   - its projector, e.g. `gemma-4-E4B-it-mmproj.gguf`.
2. **Download the bundled llama-server** if you have not already (Settings →
   AI → Download Server, or `POST /api/ai/server/download`). Vision requires the
   pinned build; an outdated build is treated as missing.
3. **Enable vision**: Settings → Vision → toggle on, provider = *Bundled local
   model*. Equivalent API call:

   ```bash
   curl -X POST "http://127.0.0.1:3131/api/settings" \
     -H 'content-type: application/json' \
     -d '{"capture_interval":5,"monitors":"[]",
          "excluded_apps":"[]","is_paused":0,"retention_days":30,
          "vision_enabled":1,"vision_provider":"local",
          "vision_model":"gemma-4-E4B","vision_endpoint":"http://127.0.0.1:31130",
          "vision_api_key":null}'
   ```

4. The worker starts the unified server (first request loads the model; this can
   take 20–60 s) and begins analyzing frames.

> **Tip — model quality.** If `vision_model` does not match a specific file, the
> first Gemma 4 **E4B** by filename order is used, which may be a low-quality
> quant (e.g. `Q2_K_XL`). Set `vision_model` to a substring of the exact file you
> want (e.g. `Q4_K_XL`) to pin a higher-quality quant. The same model is then
> also used for AI reports while vision is enabled.

## Setup (external provider)

Set `vision_provider` to `ollama` or `openai` (any OpenAI-compatible vision
endpoint) and fill in `vision_endpoint`, `vision_model`, and `vision_api_key`.
No local server is started; the worker calls the configured endpoint. Screenshots
(as base64 JPEG), app names, and window titles are sent to that provider.

## Enqueue behavior — on-demand and throttled

Vision is **not** run on every frame automatically. Frames are analyzed:

- **On demand** — `POST /api/vision/analyze/:frame_id` queues a single frame at
  high priority (jumps the line).
- **Throttled trickle** — while vision is enabled, the worker enqueues a small
  batch (4) of recent un-analyzed frames per idle cycle, newest first, so it
  works through history without flooding the GPU. The throttle bounds the *rate*,
  not the total: over time it will analyze all eligible history. Disable vision
  to stop it.

A captured frame starts at `analysis_status = 'pending'`. "Eligible" means any
frame whose status is not `completed`, `processing`, or `failed` and that is not
already queued.

## API

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/api/vision/analyze/:frame_id` | Enqueue one frame (on demand) |
| `GET`  | `/api/vision/status` | Provider/model, per-status counts, queue depth |
| `POST` | `/api/test-vision` | Test the configured vision/generation provider |

See `docs/api-reference.md` for request/response details.

## Where things live

| Concern | File |
|---|---|
| `--mmproj` flag, server config | `screensearch-llm/src/server.rs` |
| Model/projector discovery | `screensearch-llm/src/download.rs` (`resolve_mmproj_for`, `resolve_vision_model`, `discover_vision_models`) |
| Unified server selection | `screensearch-api/src/state.rs` (`get_llama_server`, `resolve_server_models`) |
| Vision worker | `screensearch-api/src/workers/vision_worker.rs` |
| Vision client (HTTP) | `screensearch-vision/src/client.rs` |
| Endpoints | `screensearch-api/src/handlers/vision.rs`, `routes.rs` |
| Queue + status queries | `screensearch-db/src/queries.rs` (`enqueue_frame_for_analysis`, `claim_analysis_task`, `get_unanalyzed_frame_ids`, `get_vision_status`) |
| Settings columns | `screensearch-db/src/migrations.rs` (`vision_*`) |

## Privacy

With the **local** provider, vision runs entirely on-device — image bytes never
leave the machine. With an **external** provider, the screenshot, app name, and
window title for each analyzed frame are sent to the configured endpoint. See
`docs/security.md`.

## Troubleshooting

- **Nothing gets analyzed / `queue_depth` stays 0**: confirm `vision_enabled=1`
  and (local) that llama-server is downloaded and current. The worker logs
  `Vision enabled (local) but llama-server is not downloaded/current` when the
  binary is missing/outdated.
- **`no vision model + mmproj projector was found`**: you have a model but no
  matching `*mmproj*.gguf` beside it (or only a text-only model like Qwen3.5).
  Add the projector for your Gemma 4 model.
- **First analysis is slow**: the unified server loads the model on the first
  request (20–60 s). Subsequent frames are much faster.
- **Results are low quality**: pin a higher-quality quant via `vision_model`
  (see the tip above).
