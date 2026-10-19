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

macOS/Linux desktop/capture support (the approved standalone stdio HTTP-client wrapper exception
permits other OSes and explicitly selected HTTP/HTTPS remote destinations; it uses `.no_proxy()`,
refuses redirects and leaves app/inference Windows-only, inference loopback-only, and API listener/
CORS unchanged; approval is not implementation evidence) · OS automation (deferred to a later phase) · cloud sync or default telemetry ·
mobile · multi-user / accounts / tenancy · real-time vision · importing V1 data.

## 8a. Approved external-agent increment versus observed base (2026-10-16)

The historical greenfield inventory above describes intake, not the present checkout. The
external-agent planning base is `cd9f5fecfa35970962be0067f76105553b24bfba`, version 0.4.1, schema
12. Existing configurable-bind HTTP API, stdio MCP and durable saved reports are dependencies,
not features to rebuild. The new chronological feed, allocator continuity, read-only credential,
coverage and saved-report API/MCP projections are approved contracts in `03 §7f`, not behavior
verified at that base. Nico authorized plan execution including recommendations; the draft
attachment's pending-GO text is superseded, but that approval authorized no merge/release
(the increment has since merged and ships in v0.4.2, §8).

Local storage/inference and API default-off/default-loopback remain. Explicitly configured
external API readers may receive full raw and filtered captured text; retrieval/report defaults
remain content-text. Only the standalone stdio HTTP-client wrapper has the portable/explicit
remote-destination exception above; capture/Tauri/inference stay Windows-only. No listener,
CORS, cloud inference or default-upload expansion. Saved-output reads are approved; remote
report generation/retry/delivery are not. The implementation must protect distinct persistent
api.history_id and monotonic allocator metadata against reset/import/retirement; it cannot
claim continuity across old writers or restores. See `02 §5h` for ordered delivery and `03 §7f`
for self-contained contracts and source-inspected inventory.

## 8. Status

- **All context items decided.** License is **MIT**. Nothing outstanding.
- **Current state (2026-10-19):** latest release v0.4.2 (2026-10-19), auto-update live since
  v0.3.2. DB schema 13 ships in v0.4.2: the forward-only 12 → 13 migration (`report_runs.kind`,
  `03 §8e`) landed with F3 G1 (#137). v0.4.2 also ships the external-agent memory API (`02 §5h`,
  `03 §7f`, #126–#130: chronological evidence pages, bounded coverage, independent read-only
  credentials, saved-report HTTP/MCP reads) and F3 deeper memory G1–G6 (`02 §5i`, `03 §8e`,
  #136–#141: weekly synthesis, weekly notes and index entries, project-note promotion, weekly
  scheduler and weekly Telegram digest). Still open (`05_BUILD_REVIEW.md`): F3 native Windows
  interactive acceptance has not been performed, and the external-agent API's native Settings
  UI and separate-machine network gates remain open. Only the Windows NSIS installer, its
  updater `.sig` and `latest.json` are release assets; standalone MCP wrapper archives are not
  published. No next arc is defined yet (pending maintainer decision). The full app is a
  15-crate Rust workspace plus a React/TS UI; active work is tracked in `05`–`08`, shipped
  history in `CHANGELOG-ARCHIVE.md` and `specs/archive/`. Code-signing is the lone packaging
  follow-up (`03 §11b`).

---

*Next layer:* `02_STRATEGIC_PLAN.md` — "what should change, and why" (the capture-cheap /
enrich-deferred thesis, phased delivery, risks).
