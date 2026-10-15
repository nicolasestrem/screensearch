# 01 — Project Context

> **Question this file answers:** *"What is true today?"* — and only that. It does **not** decide
> what to change (that's `02_STRATEGIC_PLAN.md`) or how to build it (that's
> `03_MASTER_PRODUCTION_SPEC.md`). Source: `00_PROJECT_INTAKE.md`.

---

## 1. What exists today

**Nothing is built yet.** `screensearch-v2c` is a greenfield, standalone repository
(`<repo>`) initialized with git (branch `main`) and a
`specs/` folder. There is **no source code, no dependencies installed, no scaffold**. This
document records the *starting truth* — environment, decisions already made, and the knowledge
inherited as reference — so later layers don't re-derive it.

**Relationship to "V1" (the prior `screensearch` project):** V2c is **standalone**. There is no
shared code, no database import, and no runtime link. V1 is used **only as reference knowledge**:
it proved which engines are reliable and which approaches failed (see §5).

## 2. Audience & purpose (today's understanding)

- **Product:** an open-source, local-first Windows app that continuously captures the screen,
  makes it searchable by **text and meaning**, and answers questions about what was seen.
- **Distribution intent:** unsigned NSIS installer (signing pending), website **screensearch.app**.
- **Primary users:** privacy-conscious Windows power users.
- **Privacy stance (non-negotiable):** all data local; no cloud upload; no telemetry by default.

## 3. Target environment

| Aspect | Truth today |
|---|---|
| OS | Windows 10/11 only (WinRT/WGC/WebView2 APIs). |
| GPU | Vulkan-capable GPU assumed for ~99% of users; **CPU fallback required**. |
| Runtime deps | WebView2 runtime (ships with Win11); models downloaded on first use. **No Python in the end-user runtime** (Python is fine for build/dev tooling). |
| Language/toolchain | Rust 2021 + Cargo; Node/npm for the React UI; Tauri 2 CLI. |

## 4. Intended stack (decided, not yet implemented)

These are **decisions already locked** in intake — recorded here as "true today" so the strategic
plan and spec build on a fixed base:

- **Shell:** Tauri 2 + WebView2; UI in React 18 + TS + Vite; IPC via Tauri commands/events typed
  with `ts-rs`.
- **Core:** Rust + tokio, a modular **kernel** with trait-bounded modules over a typed event bus.
- **Processing model:** **capture-cheap, enrich-deferred** — a durable **SQLite job queue** with
  worker pools triggered **on-demand / on-timer / when-idle**.
- **Capture:** Windows.Graphics.Capture (WGC). **OCR:** WinRT `Media.Ocr` (in-process, STA).
- **Embeddings:** fastembed (in-process ONNX) — EmbeddingGemma-300M text (768-dim). *(0.3.0 removed
  the optional nomic-embed-vision image lane — `02 §5c`.)*
- **Inference:** a single supervised, **model-agnostic llama.cpp sidecar** (Vulkan + CPU) that
  loads the user-selected tier on demand across two lanes — **vision** (Default Qwen3-VL-4B /
  Quality Qwen3-VL-8B) and **answer** (Default Ministral-3-3B-Reasoning / Quality
  Qwen3-4B-Thinking-2507). *(0.3.0 retired the Beta tier — `02 §5c`.)* The child is **bound to the
  app via a Windows Job Object** so it cannot orphan after a crash.
- **Data:** SQLite (WAL) + FTS5 (porter) + sqlite-vec (`vec0`, cosine); hybrid retrieval via RRF.

## 5. Inherited knowledge from V1 (reference only — what's true about the problem space)

**Proven reliable (reuse the *approach*, not the code):**
- SQLite + FTS5 + sqlite-vec at 768-dim with RRF hybrid retrieval works well on Windows.
- Native WinRT `Media.Ocr` is fast (~70–80 ms/frame) and needs no model download.
- In-process Rust embeddings via fastembed are validated on Windows (no Python).
- A llama.cpp server (Vulkan GPU + CPU fallback) over an OpenAI-compatible API is a solid local
  inference path.

**Proven to fail / to avoid (do not repeat):**
- A **Python ML sidecar** (PaddleOCR / Qwen-as-OCR) — fragile, reverted; not used here.
- An **in-memory vector index** (O(n), full load on startup) — superseded by sqlite-vec.
- **Real-time per-frame vision** — too slow/resource-hungry; V2c defers vision to on-demand/timed.
- **Qwen3-VL-Embedding on llama.cpp** — ignores images (cos≈1.0); use fastembed for image vectors.

## 6. Constraints true today

- **COM STA threading:** WinRT OCR and WGC capture must run on the thread that initialized COM.
- **EmbeddingGemma-Q cannot batch:** embed one input at a time.
- **mmproj must be same-family** as the VL model (else llama-server crashes).
- **OCR must run on the full-resolution frame** before any JPEG resize.
- **Sidecar must not orphan:** the llama.cpp child must die with the app (crash included).
- **No deadline:** correctness is prioritized over speed; phasing is unconstrained by time.

## 7. Explicit non-goals (today)

macOS/Linux support · OS automation (deferred to a later phase) · cloud sync or default telemetry ·
mobile · multi-user / accounts / tenancy · real-time vision · importing V1 data.

## 8. Status

- **All context items decided.** License is **MIT**. Nothing outstanding.

---

*Next layer:* `02_STRATEGIC_PLAN.md` — "what should change, and why" (the capture-cheap /
enrich-deferred thesis, phased delivery, risks).
