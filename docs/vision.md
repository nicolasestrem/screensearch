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
server that answers AI reports, launching it with `--mmproj` so a single model
serves *both* text generation and image analysis. This keeps one model in VRAM
and avoids running two servers. The default vision model is
**Qwen3-VL-4B-Instruct** (a lighter vision encoder and faster decode than the
previous Gemma 4 E4B default — ~1 s/frame vs 5–10 s on an RTX 5060 Ti); Gemma 4
still works if you prefer it.

For speed, the server is launched with `--image-max-tokens 1024` and
`--flash-attn on` when a projector is loaded, and each analysis request caps
output (`max_tokens` ≈ 320) with a prompt that does **not** re-transcribe
on-screen text — native OCR already captures it — so responses stay compact and
always close as valid JSON.

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
     unified llama.cpp server (Qwen3-VL-4B + --mmproj)  ◄── also serves AI reports
                 │
   frame.description / visible_text_json / activity_type / app_hint / confidence
```

The server is rebuilt automatically when vision is toggled: enabling vision
switches the loaded model to the vision model + projector; disabling it switches
back to the first discovered text GGUF.

### Model and projector selection (local provider)

- `resolve_vision_model()` chooses the unified model. It first tries to match the
  `vision_model` setting against a discovered model filename — but a *generic*
  preference excludes `*-thinking` (slow, chain-of-thought) and third-party
  `*-action`/agent fine-tunes unless the preference names them, so the default
  `Qwen3-VL-4B-Instruct` resolves to the vanilla instruct build. Otherwise it
  falls back to a **Gemma 4 E4B** model, then to the first vision-capable model
  found. **Within each of those tiers it picks the best quantization** —
  Q4 is favoured over a lower Q2/Q3 and over heavier Q6/Q8 quants — so dropping
  several quants of the same model into `.models/` selects a sensible default
  automatically (see `quant_desirability`).
- Model discovery (`discover_local_models`) skips files that are **not loadable
  primary models**: multimodal projectors (`*mmproj*.gguf`), non-standalone
  helper heads (`mtp-*.gguf`), and any GGUF below a size floor (so a truncated
  download is ignored).
- `resolve_mmproj_for()` pairs that model with the correct `*mmproj*.gguf`
  projector sitting beside it, matched by **size signature** (e.g. an `E4B` model
  gets the `E4B` projector, a `12B` model the `12B` projector). A text-only model
  such as Qwen3.5 has no matching projector and is never mis-paired.
- A model is "vision-capable" only if a matching projector is found next to it.

The default `vision_model` setting is **`Qwen3-VL-4B-Instruct`** (migration
`012_qwen3vl_vision_default`), which the substring match resolves to the
`Qwen3VL-4B-Instruct` GGUF you provide (best quant wins). To pin an exact file,
choose it in **Settings → Data & AI → Vision model** (a dropdown populated from
`GET /api/vision/models`), or set `vision_model` to a substring of the filename
(e.g. `Q4_K_M`). To deliberately use a `*-thinking` or `*-action` build, set
`vision_model` to include that word.

## Setup (local, on-device)

1. **Drop a vision model and its projector into `.models/`** (repo root, next to
   the executable, or the app models directory). You need both files. For the
   default:
   - a model GGUF, e.g. `Qwen3VL-4B-Instruct-Q4_K_M.gguf`;
   - its projector, e.g. `mmproj-Qwen3VL-4B-Instruct-F16.gguf`.

   (Gemma 4 still works — e.g. `gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf` +
   `gemma-4-E4B-it-mmproj.gguf` — if you set `vision_model` back to `gemma-4-E4B`.)
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
          "vision_model":"Qwen3-VL-4B-Instruct","vision_endpoint":"http://127.0.0.1:31130",
          "vision_api_key":null}'
   ```

4. The worker starts the unified server (first request loads the model; this can
   take 20–60 s) and begins analyzing frames.

> **Tip — pick the model.** When several quants sit in `.models/`, selection
> prefers a **Q4** automatically. To force a specific file, use the **Vision
> model** dropdown in Settings → Data & AI (or set `vision_model` to a substring
> such as `Q4_K_M`). The chosen model also serves AI reports while vision is
> enabled.

### GPU acceleration (Vulkan) and visibility

The unified server is launched with `-ngl 99` to offload all layers to the GPU
via the Vulkan llama.cpp build, falling back to CPU if Vulkan initialization
fails. Two things make the GPU path observable:

- **llama-server logs are captured to `bin/llama-server.log`** (previously
  discarded). Vulkan device selection, the offloaded-layer summary, and any
  fallback reason are visible there.
- **`GET /api/ai/server/status` reports `acceleration`** (`"gpu"`, `"cpu"`, or
  `"unknown"`), and Settings → Data & AI shows a **"Running on GPU (Vulkan)"** /
  **"Running on CPU"** badge while the server is up.

The GPU-mode health-check timeout **scales with model size** (base + per-GB,
capped), so a multi-GB model loading into VRAM on first use is not mistaken for a
GPU failure and bounced to CPU. If the badge says CPU unexpectedly, check
`bin/llama-server.log` for the Vulkan reason (e.g. missing runtime, no compatible
device).

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
| `GET`  | `/api/vision/models` | Discovered `(model, mmproj)` pairs for the picker; flags the selected one |
| `GET`  | `/api/ai/server/status` | Server state incl. `acceleration` (`gpu`/`cpu`) |
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

- **`image input is not supported … you may need to provide the mmproj`**: the
  loaded model is **text-only** (no projector). This happened when the default
  `vision_model` was Ministral-3B; the default is now **Qwen3-VL-4B-Instruct**.
  Ensure a vision model **and** its `*mmproj*.gguf` are in `.models/`, then pick
  it in Settings → Data & AI → Vision model.
- **Nothing gets analyzed / `queue_depth` stays 0**: confirm `vision_enabled=1`
  and (local) that llama-server is downloaded and current. The worker logs
  `Vision enabled (local) but llama-server is not downloaded/current` when the
  binary is missing/outdated.
- **`no vision model + mmproj projector was found`**: you have a model but no
  matching `*mmproj*.gguf` beside it (or only a text-only model like Qwen3.5).
  Add the projector for your vision model (e.g. `mmproj-Qwen3VL-4B-Instruct-F16.gguf`).
- **First analysis is slow**: the unified server loads the model on the first
  request (20–60 s) and the first image also triggers one-time Vulkan
  vision-graph warmup. Subsequent frames are much faster (~1 s on an RTX 5060 Ti).
- **Server runs on CPU instead of GPU**: check the `acceleration` badge in
  Settings → Data & AI and `bin/llama-server.log` for the Vulkan reason. The
  GPU health-check timeout scales with model size, so large models are given
  enough time to load into VRAM before any CPU fallback.
- **Results are low quality**: pick a higher-quality quant in the Vision model
  dropdown (or pin one via `vision_model`).
- **`database is locked` in the vision worker**: addressed by a SQLite
  `busy_timeout`; the writer now waits for the lock under concurrent capture/OCR
  writes instead of erroring.
