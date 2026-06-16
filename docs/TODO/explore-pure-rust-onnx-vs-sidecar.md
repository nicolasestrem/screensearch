# Case Study: Replacing the Python Sidecar with Truly-Native Rust Inference (PR#63)

> **This is a feasibility / effort case study, not an implementation plan.** The goal is to
> let you contemplate the cost and risk of dropping the Python sidecar introduced in PR#63
> in favor of zero-C++-runtime Rust inference (candle / tract). No code changes are proposed
> for execution yet.

## Context

PR#63 ("Modernize local OCR, RAG, and AI runtime", branch `feat/ai-rag-ocr-modernization`)
**removed** the previous in-Rust ONNX path (`ort` + `tokenizers` running MiniLM, 384-dim,
in `screensearch-embeddings`) and **replaced it with a Python sidecar** (`sidecar/app.py`,
PyInstaller-bundled) that hosts three models over localhost HTTP on `127.0.0.1:3132`:

| Model | Purpose | Python stack | Output |
|---|---|---|---|
| **PP-OCRv5** (PaddleOCR 3.x) | screen OCR | `paddleocr` + `paddlepaddle` | lines: text, confidence, bbox |
| **Qwen3-Embedding-0.6B** | semantic vectors | `sentence-transformers` + `torch` | 1024-dim vectors |
| **Qwen3-Reranker-0.6B** | result reranking | `sentence-transformers` (CrossEncoder) | per-doc scores |

The Rust side is now a thin HTTP client:
- `screensearch-capture/src/sidecar_ocr.rs` — multipart JPEG → `/v1/ocr`, bbox remapping.
- `screensearch-capture/src/ocr_provider.rs` — sidecar-or-Windows-OCR fallback selection.
- `screensearch-embeddings/src/engine.rs` — `/v1/embeddings`, `/v1/rerank`, `/v1/chunk`, `/v1/models/*`.
- `src/main.rs` (`ensure_quality_sidecar`, ~L743–859) — spawn, UUID bearer token, 60 s health-poll, `kill_on_drop`, stdout/stderr log forwarding.
- `screensearch-db/src/vector_search.rs` + `queries.rs` — `sqlite-vec` KNN + RRF hybrid (this is **independent of the inference backend** and stays either way).

### What the sidecar costs (the motivation to contemplate)
- **Distribution size**: torch (~1.5–2 GB) + sentence-transformers/transformers + paddleocr/paddlepaddle → roughly a **4–6 GB uncompressed** PyInstaller bundle.
- **Fragility**: `sidecar/build.py` already has to hand-refresh `vcruntime140.dll`/`msvcp140.dll` from `System32` to dodge torch `WinError 1114`; recent repo commits (`f1b3ddd`, `798a79b`) are sidecar/MSVC-runtime firefighting.
- **Two-runtime operational surface**: a second process to spawn, health-poll, authenticate, supervise, and ship; cold start waits up to 60 s for model load/download.
- **Build complexity**: Linux-first Bash + PowerShell orchestration, a dedicated `quality.yml` smoke test, PyInstaller metadata preservation hacks.

A single self-contained Rust binary with no Python, no torch, and no C++ runtime is the prize.

## The chosen target: truly-native (candle / tract), zero C++ runtime

Scope (per your selection): **OCR + embeddings + reranker**. The bundled Ministral generation
LLM is **out of scope** (separate, larger decision). `ort`-based options (`fastembed-rs`,
`oar-ocr`) are **excluded** because they link the ONNX Runtime C++ library — that is "pure Rust"
only in the build sense, not zero-native-deps.

Truly-native means:
- **Transformers (embed + rerank)** → `candle` / `candle-transformers` (pure-Rust, loads safetensors). Qwen3 is implemented in `candle-transformers`; the candle-based `aha` crate already exposes `qwen3-embedding` + `qwen3-reranker`.
- **OCR** → `tract` (pure-Rust ONNX), via the DBNet(det) + SVTR(rec) pipeline pattern shown by `pure-onnx-ocr-sync`, or a candle port.

## Effort & risk breakdown per component

### 1. Qwen3 embeddings — **Moderate effort, Moderate risk**
- candle has Qwen3 blocks; need: load `Qwen3-Embedding-0.6B` safetensors, tokenizer (`tokenizers` crate, already used on `main`), last-token/mean pooling + L2 normalize, the `"Instruct: …\nQuery:{text}"` query-instruction prefix the sidecar applies, batching.
- **Parity risk**: vectors must match the sidecar's numerically enough that the **persisted 1024-dim index stays valid**. Any drift (pooling, normalization, dtype/quantization, instruction formatting) silently degrades retrieval. The PR's own contract validation (model/version/dim) exists precisely because mixing non-equivalent vectors corrupts one index → expect a **full re-embed** on switch regardless.
- **Perf risk**: candle CPU on a 0.6B model per batch of screen chunks — must measure against the current performance targets; may need quantization (GGUF/Q4) which further threatens parity.

### 2. Qwen3 reranker — **Moderate effort, Lower risk**
- Same candle loading story for `Qwen3-Reranker-0.6B`; reranker is a causal LM scoring yes/no logits → sigmoid, matching the sidecar's `/v1/rerank` contract.
- Lower stakes than embeddings: scores are **not persisted**, so parity drift only affects live ranking, not stored data. Easiest of the three to validate (compare top-k ordering on the existing `evaluation/cases.jsonl`).

### 3. PP-OCRv5 — **High effort, High risk — the crux**
This is where "truly native" gets expensive. The OCR pipeline is **not one model** — it's
detect → (angle classify) → crop/warp → recognize → decode, each with non-trivial pre/post-processing:
- **Detection (DBNet)**: resize/normalize/NCHW, run, then DB post-processing — binarization, contour extraction, polygon expansion (the Python stack uses `pyclipper`/`shapely`; both need Rust equivalents), box scoring/filtering.
- **Recognition (SVTR)**: per-box crop + perspective warp, force-resize, normalize, batch, run, **CTC decode** against a character dictionary.
- **Angle/orientation**: the sidecar enables `use_doc_orientation_classify` + `use_textline_orientation`; `pure-onnx-ocr-sync` has **no angle classification** at all.

**Hard, evidence-backed risks:**
- **tract operator coverage**: `pure-onnx-ocr-sync` documents that tract may **reject ops like `LayerNormalization` and `Scan`** — exactly the kind SVTR-style recognizers use. Mitigation is ONNX graph simplification, which can change numerics.
- **Accuracy is unsolved off-the-shelf**: that crate openly states *"the OCR results are still noisy and often incorrect. Root-cause analysis and debugging remain open tasks."* So the only **truly-native** PP-OCRv5 reference today is **not production-accurate**.
- Converting Paddle → ONNX (`paddle2onnx` / `ppocrv5-onnx`) is itself a step that must be validated op-by-op against tract.

A candle port of PP-OCRv5 avoids tract's op gaps but means **porting the entire pre/post-processing pipeline from scratch** (no maintained candle PP-OCRv5 exists) — larger code, same parity-validation burden.

### 4. What does NOT change
- `sqlite-vec` KNN + RRF hybrid retrieval (`screensearch-db`) is backend-agnostic — untouched.
- The chunker, `screensearch-api` handlers, and DB schema stay; only the *producer* of vectors/OCR/rerank-scores changes.
- Windows-OCR fallback (`ocr_provider.rs`) can remain as the safety net during/after migration.

## Effort estimate (relative, for contemplation)

| Component | Native effort | Native risk | Off-the-shelf Rust building block |
|---|---|---|---|
| Embeddings (Qwen3-0.6B) | Medium | Medium (vector parity, re-embed forced) | candle-transformers / `aha` |
| Reranker (Qwen3-0.6B) | Medium | Low–Medium (scores not persisted) | candle-transformers / `aha` |
| **PP-OCRv5 (det+rec+angle)** | **High** | **High (tract op gaps, accuracy unsolved, full pipeline port)** | `pure-onnx-ocr-sync` (tract, *noisy*), candle port (from scratch) |
| Vector/RRF/DB | None | None | unchanged |

**Rough shape**: embeddings + reranker are a believable few-week spike each with candle; **OCR is the
multi-month, accuracy-gated research item** that dominates total cost and is the make-or-break. The
benefit (single binary, no torch, no MSVC-runtime hacks, no second process) is real and large — but it
is **gated almost entirely on solving native PP-OCRv5 accuracy**.

## Recommendation (to weigh, not to execute)
- **Don't go all-native in one move.** De-risk in this order, each independently shippable behind the existing provider/fallback switches:
  1. **Reranker** first (lowest risk, no persisted state) — proves the candle+Qwen3 toolchain end to end.
  2. **Embeddings** next (forces a one-time re-embed; validate vector parity on `evaluation/cases.jsonl` before committing the index).
  3. **OCR last, as a spike, not a commitment** — prototype tract DBNet+SVTR (or candle) and **measure character/line accuracy against PP-OCRv5-via-sidecar on real captured frames** before deciding. Keep Windows OCR + sidecar as fallbacks until native OCR meets an accuracy bar you set.
- **A hybrid endgame is legitimate**: native candle embeddings+reranker (kills the torch/sentence-transformers bulk, the largest dependency) while OCR stays on the sidecar or Windows OCR until native OCR is proven. This captures most of the size/complexity win at a fraction of the OCR risk.

## How to validate the case study (if you prototype)
This being a study, "verification" means producing evidence to confirm/deny the estimates:
1. **Embedding parity**: feed identical chunks to the sidecar `/v1/embeddings` and a candle prototype; cosine-compare vectors and re-run `evaluation/evaluate.py` over `evaluation/cases.jsonl` — retrieval ranking must not regress.
2. **Reranker parity**: same query/doc sets through `/v1/rerank` vs candle; compare top-k ordering.
3. **OCR accuracy**: run both engines over a fixed set of captured frames; compute char/line accuracy and latency vs the **Performance Targets** in `CLAUDE.md` (OCR < 100 ms). This single measurement decides whether native OCR is viable.
4. **Size/cold-start**: compare final binary size and startup time with vs without the sidecar.

> Per repo rules: any prototype must paste the **verbatim** output of its verification commands
> (`cargo test`, `evaluate.py`, accuracy/latency measurements) — no paraphrase — and must not stub
> or hardcode parity results.

## Sources
- [oar-ocr (ort-based OCR, GitHub)](https://github.com/greatv/oar-ocr) · [crates.io](https://crates.io/crates/oar-ocr)
- [pure-onnx-ocr-sync (tract, zero native deps; documents noisy results + op gaps)](https://lib.rs/crates/pure-onnx-ocr-sync) · [crates.io](https://crates.io/crates/pure-onnx-ocr-sync/0.1.1)
- [ppocrv5-onnx (PP-OCRv5 → ONNX conversion)](https://github.com/HoVDuc/ppocrv5-onnx)
- [rust-paddle-ocr model system (DeepWiki)](https://deepwiki.com/zibo-chen/rust-paddle-ocr)
- [candle-transformers models index (docs.rs)](https://docs.rs/candle-transformers/latest/candle_transformers/models/index.html)
- [aha (candle-based qwen3-embedding + qwen3-reranker, crates.io)](https://crates.io/crates/aha)
- [fastembed-rs (ort-based Qwen3 embed/rerank — excluded as not zero-native)](https://github.com/Anush008/fastembed-rs)
- [ort 2.0.0-rc.12 status (docs.rs)](https://docs.rs/crate/ort/latest)
