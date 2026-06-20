# POC: Can `Qwen3-VL-Embedding-2B` unify ScreenSearch's retrieval layer?

**Status:** Spike complete — **NO-GO** for the primary goal (visual recall via image
embeddings on the existing llama.cpp server).
**Date:** 2026-06-20 · **Branch:** `feature/qwen3vl-embedding-poc`
**Build under test:** pinned `vendor/llama-b9728` — `version: 9728 (fabde3bf5)`,
built with Clang 20.1.8, Vulkan backend (RTX 5060 Ti).
**Model under test:** `.models/Qwen.Qwen3-VL-Embedding-2B.f16.gguf` (3.4 GB) +
paired `.models/mmproj-Qwen.Qwen3-VL-Embedding-2B.f16.gguf` (819 MB).

---

## TL;DR

`Qwen/Qwen3-VL-Embedding-2B` is a genuinely multimodal embedding model, but **the one
capability that motivated this investigation — embedding screenshots so non-OCR
visual content becomes searchable — does not work on llama.cpp.** On the pinned
`b9728` build, the `llama-server` `/embedding` endpoint **silently ignores image
data**: two completely different images produce byte-identical vectors, and adding an
image changes nothing versus text-only. This reproduces upstream issue #19525 locally.

- **Image embedding: broken** (verbatim proof below).
- **Text embedding: works**, but adopting the 2B model for text *only* is a net
  regression (≈5 GB VRAM + GPU contention vs the current in-process CPU embedder,
  for no quality reason and no visual upside).
- **Recommendation:** keep EmbeddingGemma-300M. Get the visual-recall win a different
  way — **embed the generative vision `description` that the vision worker already
  produces** (see "Recommended alternative"). No new model, no GPU, no schema change.

No llama.cpp version bump was attempted: the upstream feature was never merged into
*any* build, and the only working path (Jina's fork) requires a Python image-patch
preprocessor that violates ScreenSearch's in-process / no-Python architecture.

---

## What ScreenSearch does today (verified, read-only)

- Text embeddings are **in-process** via `fastembed`/ONNX, hardcoded to
  EmbeddingGemma-300M: `EMBEDDING_DIM = 768` (`screensearch-embeddings/src/lib.rs:41`),
  `MODEL_NAME = "EmbeddingGemma-300M"`, `provider() = "fastembed"`
  (`engine.rs:77,264`). **There is no provider trait** — the engine instantiates
  `EmbeddingModel::EmbeddingGemma300MQ` directly.
- Vectors live in sqlite-vec with the dim baked into the schema:
  `embedding_vectors USING vec0(embedding float[768] distance_metric=cosine)` plus
  `embeddings_model/provider/dimension` metadata
  (`screensearch-db/src/migrations.rs:317-333`).
- The managed `llama-server` (pin `LLAMA_VERSION = "b9728"`,
  `screensearch-llm/src/download.rs:43`) runs **generative only** — `--embedding` is
  never passed. `is_loadable_model_gguf` excludes any GGUF whose filename contains
  `embed` (`download.rs:413-437`), so an embedding model must be launched manually.
- **Vision output is metadata-only and never embedded.** The vision worker writes
  `description / visible_text_json / activity_type / app_hint` to the `frames` table
  (`queries.rs:1280-1301`); the embedding worker builds its text from **OCR text +
  frame metadata only** (`screensearch-api/src/workers/embedding_worker.rs:66-87`).
  The vision prompt explicitly forbids transcription
  (`screensearch-vision/src/client.rs:126-135`). Captures are downscaled to
  `max_width = 1280` (`src/main.rs:240`), OCR running on full-res first.

**Conclusion:** today, search reaches OCR text (FTS5) and OCR-derived embeddings
only. Icons, charts, design canvases, and any screen with little/no text have **no
recall path**. That gap is what the Qwen3-VL-Embedding idea aimed to close.

---

## Decisive test — does the image influence the vector?

Server launched on the pinned build:

```
vendor/llama-b9728/x/llama-server.exe \
  -m .models/Qwen.Qwen3-VL-Embedding-2B.f16.gguf \
  --mmproj .models/mmproj-Qwen.Qwen3-VL-Embedding-2B.f16.gguf \
  --embedding --pooling last -c 4096 -ngl 99 --port 8090 --host 127.0.0.1
```

Server log confirmed the multimodal projector loaded:

```
0.06.282.443 I srv    load_model: loaded multimodal model, '.models/mmproj-Qwen.Qwen3-VL-Embedding-2B.f16.gguf'
0.06.411.198 I srv  llama_server: model loaded
0.06.411.204 I srv  llama_server: server is listening on http://127.0.0.1:8090
```

Two visually distinct, text-rich test images were generated locally (no network):
**A** = a dark code editor showing Rust source; **B** = a white news page with
headlines. The request used Format 1 from issue #19525 / discussion #13666 — an
`[img-1]` marker plus an `image_data` array — so **the text content string is
identical across A, B, and no-image; only the image bytes differ.**

Verbatim harness output (`python poc_test.py`):

```
[TXT-code  ] /embedding       65ms  len=2048           head=['-0.0176', '-0.0104', '-0.0098', '-0.0201', '0.0148', '-0.0173']
[TXT-news  ] /embedding     5434ms  len=2048           head=['-0.0210', '0.0447', '0.0322', '0.0047', '-0.0141', '0.0109']
[F1-imgA   ] /embedding     3353ms  len=2048           head=['-0.0132', '-0.0500', '-0.0100', '-0.0350', '-0.0056', '0.0113']
[F1-imgB   ] /embedding       25ms  len=2048           head=['-0.0132', '-0.0500', '-0.0100', '-0.0350', '-0.0056', '0.0113']
[F1-noimg  ] /embedding       37ms  len=2048           head=['-0.0132', '-0.0500', '-0.0100', '-0.0350', '-0.0056', '0.0113']
[F2-imgA   ] /embedding   ERROR: HTTP Error 500: Internal Server Error
[F2-imgB   ] /embedding   ERROR: HTTP Error 500: Internal Server Error

==== DECISIVE COSINES ====
cos(F1-imgA  , F1-imgB  ) = 1.0
cos(F1-imgA  , F1-noimg ) = 1.0
cos(TXT-code , TXT-news ) = 0.304743
cos(TXT-code , F1-imgA  ) = 0.389839
cos(TXT-code , F1-imgB  ) = 0.389839
```

Reading the result:

- **`cos(imgA, imgB) = 1.0`** — two totally different images give the *same* vector.
- **`cos(imgA, noimg) = 1.0`** — attaching an image vs attaching nothing is identical;
  the head floats are byte-for-byte equal across imgA / imgB / noimg. The vector is a
  pure function of the text string.
- The text path is healthy (`cos(TXT-code, TXT-news) = 0.305`), so the endpoint and
  pooling work — the failure is specific to images.
- Retrieval cannot discriminate: a code query is equidistant (0.390) from the code
  image and the news image, because their vectors are identical.

**Pass conditions (`vec(imgA) ≠ vec(noimg)`, `vec(imgA) ≠ vec(imgB)`,
`cos(query, match) > cos(query, non-match)`) all FAIL.**

### Why — from the server log

```
2.40.308.820 I slot      release: id  3 | task 0 | stop processing: n_tokens = 9, truncated = 0
...
2.49.182.983 E mtmd_tokenize: error: number of media markers in text (0) does not match number of bitmaps (1)
2.49.212.953 E mtmd_tokenize: error: number of media markers in text (0) does not match number of bitmaps (1)
```

- Format 1 (`image_data`): the request processed **`n_tokens = 9`** — just the text
  "Image: [img-1]." The image was never expanded into the ≥1024 image tokens the
  model needs (`load_hparams: Qwen-VL models require at minimum 1024 image tokens`).
  The image_data array is silently dropped — exactly the #19525 symptom ("vector
  length matches text tokens only").
- Format 2 (`<__media__>` + `multimodal_data`, PR #15108 style): hard error —
  `mtmd_tokenize` is not wired into the embedding path on this build, hence the 500s.

Both supported request shapes fail. Image embedding is not functional on `b9728`.

---

## Latency & VRAM

- **VRAM:** GPU baseline ≈ **2076 MiB** → **7121 MiB** with the f16 2B + mmproj
  loaded, i.e. ≈ **5 GB** for this model alone (freed back to ≈1897 MiB after
  shutdown — verified). On the shared 16 GB GPU this would contend hard with the
  existing generative vision model (~3–4 GB) and the Ministral-3B answer model
  (~2 GB) on the same server.
- **Text-embedding latency (`/embedding`, GPU, steady state):**
  `[2202, 51, 3324, 44, 27] ms` — a floor of **~30–50 ms** for short text, with
  multi-second spikes when the live capture pipeline is also using the GPU (the
  contention concern, observed directly).
- For comparison, EmbeddingGemma-300M runs **in-process on CPU/ONNX** with **zero
  VRAM** and no GPU contention. For text-only work the 2B model is strictly heavier
  for no quality reason.

---

## External evidence (primary sources)

- **Issue #19525** — *llama-server ignores my image data when trying to run embedding
  with Qwen3-VL-Embedding-2B* (the identical model + mmproj). **Closed as "not
  planned."** This is the bug reproduced above.
  <https://github.com/ggml-org/llama.cpp/issues/19525>
- **PR #18665** — adds text+image embedding for Qwen3-VL-Embedding. **Closed, NOT
  merged** (2026-04-22); the base model is missing `1_Pooling` and conversion needs
  `--sentence-transformers-dense-modules`. Never integrated upstream.
  <https://github.com/ggml-org/llama.cpp/pull/18665>
- **Discussion #13666** + Jina's "Multimodal Embeddings in llama.cpp and GGUF":
  upstream "multimodal embedding output is completely missing"; image embeddings work
  only in **Jina's fork**, and even there require a **Python service to pre-compute
  image patches** (ggml can't do the required ops).
  <https://github.com/ggml-org/llama.cpp/discussions/13666> ·
  <https://jina.ai/news/multimodal-embeddings-in-llama-cpp-and-gguf/>

The pinned `b9728` is *newer* than the build in the closed issue (b7999), yet the
feature was never merged into any build — and the local run above confirms it is still
absent on `b9728`.

---

## Go / No-Go by sub-goal

| Sub-goal | Verdict | Reason |
|---|---|---|
| **Embed screenshots (visual recall)** | **NO-GO** | Image data ignored on b9728 `/embedding`; unsupported upstream; only fork+Python works. |
| Replace EmbeddingGemma for **text** | **NO-GO** | Net regression: ~5 GB VRAM + GPU contention + 2B vs in-process CPU 300M, for no quality/visual gain. |
| Replace bge-reranker with Qwen3-VL-Reranker | **NO-GO (now)** | Pointless without the multimodal embedding path; the visual half would hit the same mtmd gap. |
| Run on existing llama.cpp server | n/a | Server *runs* in embedding mode, but the image path is the blocker, not the plumbing. |
| 768-dim via MRL to match schema | n/a | Native output is 2048-dim; MRL truncation to 768 is trivial client-side, but moot given the no-go. |

**Revisit trigger:** if a future upstream llama.cpp release lands Qwen3-VL-Embedding
multimodal `/embedding` support (watch #19516 / a successor to #18665), re-run
`poc_test.py` against that build. Only then does the go-path reopen — and it would
still require building an embedding-provider abstraction (none exists today) and a
schema/metadata migration off the fixed 768 dim.

---

## Recommended alternative (outline only — not built this session)

The original need was **a recall path for non-OCR visual content.** We can deliver
most of that today with components that already exist and already run, sidestepping
the broken image-embedding path entirely:

> **Embed the generative vision `description` (and `visible_text` labels) into the
> existing 768-dim pipeline.**

The vision worker already produces a 1–2 sentence semantic `description` plus up to 6
prominent labels per frame (`screensearch-vision/src/lib.rs`,
`client.rs:126-135`) and stores them on the frame. Today the embedding worker ignores
them. Feeding them to the embedder turns "a Figma canvas with a blue dashboard
mockup" or "a bar chart of Q3 revenue" into searchable semantic text — no new model,
no GPU, no schema change (stays EmbeddingGemma 768-dim / sqlite-vec).

Touch points to scope when implementing:

1. **`embedding_worker.rs:66-87`** — extend `combined_text` to append the frame's
   `description` + `visible_text` (alongside the existing OCR text + metadata). This
   is the core change.
2. **Ordering / reindex** — vision analysis is asynchronous and may complete *after* a
   frame is first embedded (OCR is available immediately; the description arrives
   later). Needs either: (a) gate embedding until `analysis_status = 'completed'`, or
   (b) re-embed a frame when its description lands. There is already an
   `embeddings_reindex_required` metadata flag (`migrations.rs:327-333`) and a
   reindex path to reuse.
3. **No schema migration** — same dim, same table, same vector index. A one-time
   backfill/reindex of existing frames is the only data step.
4. **Optional, later:** zero-shot `activity_type` could still be explored with a small
   *text* classifier or the existing generative model — independent of this change.

This is the high-leverage follow-up: it closes most of the visual-recall gap with a
~single-function change and zero new infrastructure, while the true image-embedding
approach waits on upstream llama.cpp.

---

## Reproduction

1. Generate two distinct test images (native .NET drawing; see the PowerShell snippet
   used: a dark code-editor image `A_code.jpg` and a white news-page `B_news.jpg`).
2. Launch the server with the command in "Decisive test" above.
3. Run the harness `poc_test.py` (stdlib only): it base64-encodes both images, posts
   text-only / image-A / image-B / no-image requests to `/embedding` in both request
   formats, prints vector lengths + leading components, and computes the decisive
   cosines.
4. Expect `cos(imgA, imgB) = 1.0` and `cos(imgA, noimg) = 1.0` — the no-go signature.

> The throwaway harness (`poc_test.py`) and generated images (`poc_imgs/`) are not
> committed; the verbatim outputs above are the evidence of record.
