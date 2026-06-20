# POC: Can `Qwen3-VL-Embedding-2B` unify ScreenSearch's retrieval layer?

**Status:** Spike complete. **NO-GO** for image embedding *on the llama.cpp server*
— but the goal itself (direct screenshot embedding for visual recall) is **GO via a
different, better-fitting path: in-process fastembed image embeddings** (proven below).
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

- **Qwen3-VL-Embedding image embedding on llama.cpp: broken** (verbatim proof below).
- **Text embedding via the 2B: works**, but adopting it for text *only* is a net
  regression (≈5 GB VRAM + GPU contention vs the current in-process CPU embedder,
  for no quality reason and no visual upside).
- **The prize is achievable another way — and it was proven this spike.** An
  in-process, ONNX, **aligned text↔image pair via fastembed** (`nomic-embed-vision-v1.5`
  for screenshots + `nomic-embed-text-v1.5` for queries, **both 768-dim**) embeds
  screenshots directly into one space with text. In the test, **3/3 text queries
  retrieved the correct screenshot from pixels — including a textless bar chart that
  has no OCR recall path today.** No Python, no llama.cpp, no GPU; reuses the existing
  fastembed/ort stack and the existing `float[768]` schema. See "Proven working path."
- **Recommendation:** keep EmbeddingGemma-300M for OCR text. Add visual recall in two
  tiers — a zero-new-model quick win (embed the vision `description`) and the real
  prize (the fastembed image index). See "Recommendation."

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

## Proven working path — direct screenshot embedding, in-process

The llama.cpp route is the only thing that's blocked; **embedding screenshots
directly is not.** `fastembed` (the crate already powering ScreenSearch's text
embeddings, v5.17.2 / ort 2.0) ships an aligned multimodal pair that runs the same
way as the current text model — ONNX, in-process, CPU, no Python, no GPU:

| Role | Model (fastembed) | Dim |
|---|---|---|
| Screenshots | `ImageEmbeddingModel::NomicEmbedVisionV15` (`nomic-ai/nomic-embed-vision-v1.5`) | **768** |
| Text queries | `EmbeddingModel::NomicEmbedTextV15` (`nomic-ai/nomic-embed-text-v1.5`) | **768** |

The two models are designed to share one latent space, and both emit **768 dims —
exactly the existing `embedding_vectors float[768]` schema.** The only gating
question — *does a text query actually retrieve the right screenshot from pixels?* —
was tested directly with three distinct generated screenshots (a dark code editor, a
white news page, and a **textless bar chart**) and three text queries:

```
image vec dim = 768, text vec dim = 768

==== cosine(query, image) matrix ====
              A_code    B_news   C_chart
      code    0.0888    0.0313    0.0368   -> best: A_code OK
      news    0.0426    0.0707    0.0437   -> best: B_news OK
     chart    0.0481    0.0365    0.0886   -> best: C_chart OK

image-image cos(code,news)=0.8692 cos(code,chart)=0.7605 cos(news,chart)=0.7431

RESULT: 3/3 queries retrieved the correct screenshot by pixels.
```

**Reading it:** every query's top hit is its matching screenshot; the diagonal beats
the off-diagonal by ~2–2.5×. The **textless bar chart** — which has zero OCR text and
therefore *no recall path today* — was retrieved correctly by the text query "a bar
chart graph of data." That is the prize, working in-process.

Caveats (honest):

- **Absolute cross-modal cosines are small** (~0.07–0.09). This is normal for the
  nomic vision↔text alignment; what carries signal is the *ranking*, not the raw
  value. Combine with the OCR-text results by **rank fusion (RRF)** — the project
  already uses RRF — not by a shared score threshold.
- The image side aligns with **nomic-text, not EmbeddingGemma.** So an image query
  must be encoded with nomic-text (see the two integration options below).
- Imagery here is synthetic; real screenshots should be validated during integration,
  and per-frame CPU latency measured (small ViT — expected tens-to-low-hundreds of ms;
  not formally benchmarked this spike).
- Enabling fastembed's `image-models` feature pulls extra image-codec deps
  (rav1e/ravif/exr/…) which lengthen the build; weigh that during integration.

---

## Recommendation (outline only — not built this session)

The original need is **a recall path for non-OCR visual content.** Deliver it in two
tiers, smallest-first:

### Tier 1 — quick win, zero new models: embed the vision `description`

The vision worker already produces a 1–2 sentence `description` + up to 6
`visible_text` labels per frame (`screensearch-vision/src/lib.rs`,
`client.rs:126-135`), stored on the frame but **never embedded**. Feed them to the
existing EmbeddingGemma pipeline and "a Figma canvas with a blue dashboard mockup"
becomes searchable as text — no new model, no GPU, no schema change.

- **`embedding_worker.rs:66-87`** — append `description` + `visible_text` to
  `combined_text` (alongside OCR text + metadata).
- **Ordering / reindex** — vision analysis is async and may finish *after* a frame is
  first embedded; gate on `analysis_status = 'completed'` or re-embed when the
  description lands. Reuse the existing `embeddings_reindex_required` flag
  (`migrations.rs:327-333`).

### Tier 2 — the real prize: an in-process image-embedding index (fastembed nomic)

Embed each stored screenshot with `NomicEmbedVisionV15` (768-dim) into the vector
store, and encode image-search queries with `NomicEmbedTextV15`. This catches content
the description misses (icons, dense canvases, charts, sparse-text screens).

Integration shape to scope:

1. **Embedding engine** — add an `ImageEmbedding` alongside the current
   `TextEmbedding` in `screensearch-embeddings` (the engine has no provider trait yet;
   this is the moment to introduce a small image-embed entry point). Enable fastembed's
   `image-models` feature.
2. **Storage** — reuse the `float[768]` `embedding_vectors` table; tag image rows
   (e.g. a `kind`/`modality` column or a parallel vec table) so they're queried with
   the nomic-text query encoder, not the EmbeddingGemma one.
3. **Query side, two options:**
   - **(A) Two indexes, fuse by RRF (lower risk):** keep EmbeddingGemma for OCR text;
     additionally encode the query with nomic-text for the image index; merge both
     result lists with the existing RRF. Preserves current text quality; adds one
     extra small query encoding.
   - **(B) Unify on nomic (simpler space):** move OCR-text embeddings to nomic-text too
     so a single query encoding searches one space. Requires a full text reindex and a
     quality check of nomic-text vs EmbeddingGemma on this corpus.
   Recommend starting with **(A)**.
4. **Worker** — embed screenshots from `frames.file_path` (re-decode the stored 1280px
   JPEG, which is already what the vision worker reads) on the existing background
   worker cadence; backfill existing frames once.

### Optional, later
Zero-shot `activity_type` could be explored with a small text classifier or the
existing generative model — independent of the above.

---

## Go / No-Go (revised)

- **Embed screenshots for visual recall — GO**, via the in-process fastembed nomic
  image index (Tier 2), **not** via Qwen3-VL-Embedding on llama.cpp.
- **Quick partial win — GO**, via embedding the vision `description` (Tier 1).
- **Qwen3-VL-Embedding-2B on llama.cpp — NO-GO**, until upstream lands multimodal
  `/embedding` (watch #19516 / a successor to #18665), at which point re-test.

---

## Reproduction

**llama.cpp no-go test:** generate two distinct images, launch the server per
"Decisive test," and POST text-only / image-A / image-B / no-image requests to
`/embedding` (Format 1: `[img-1]` marker + `image_data`). Expect
`cos(imgA, imgB) = 1.0` and `cos(imgA, noimg) = 1.0`.

**fastembed go proof:** with fastembed's `image-models` feature enabled and
`ORT_DYLIB_PATH` pointed at the bundled `onnxruntime.dll`, embed three distinct
screenshots with `ImageEmbeddingModel::NomicEmbedVisionV15` and three text queries
(prefixed `search_query:`) with `EmbeddingModel::NomicEmbedTextV15`, then print the
query×image cosine matrix. Expect each query's top hit to be its matching image.

> The throwaway harnesses and generated images are not committed; the verbatim outputs
> above are the evidence of record.
