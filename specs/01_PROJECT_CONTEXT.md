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
- **Audience: a single user — the maintainer.** Distribution intent (public repo, NSIS installer)
  is unchanged, but no compatibility, adaptation, pilot-validation or multi-installation
  obligation exists toward anyone else (0.4.5 D1).
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
- **Single user (0.4.5 D1):** no forward-compat shims, no retired-key tolerance, no
  multi-environment validation.
- **Baseline schema, not a migration chain (0.4.5 D2):** `03 §4` holds the full DDL at
  `schema_version = 14`; a schema change edits the baseline and bumps the version. No
  populated-DB migration tests. The 0.2.x–0.4.2 forward-only migrations are history, not the rule.
- **History is carried, not migrated (0.4.5 D3):** the dev-only `ssv2c-rebuild` tool copies a v13
  DB into v14 read-only; the old file is kept as `*.v13.bak`.
- **Local-only (0.4.5 D15):** no new network destination; inference stays on the local sidecar;
  fonts and icons ship as bundled files.

## 7. Explicit non-goals (today)

macOS/Linux desktop/capture support (the approved standalone stdio HTTP-client wrapper exception
permits other OSes and explicitly selected HTTP/HTTPS remote destinations; it uses `.no_proxy()`,
refuses redirects and leaves app/inference Windows-only, inference loopback-only, and API listener/
CORS unchanged; approval is not implementation evidence) · OS automation (deferred to a later phase) · cloud sync or default telemetry ·
mobile · multi-user / accounts / tenancy · real-time vision · importing V1 data · compatibility
obligations toward other installations, retired-key load tolerance, and multi-environment pilot
validation (0.4.5 D1).

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
  scheduler and weekly Telegram digest). F3 native Windows interactive acceptance was **accepted**
  on the maintainer's report (`07`, 2026-10-19), and the read credential's native Settings UI was
  verified in P7 (`05`). Still open (`05_BUILD_REVIEW.md`): the external-agent API's separate-machine
  network gate. Only the Windows NSIS installer, its
  updater `.sig` and `latest.json` are release assets; standalone MCP wrapper archives are not
  published. Post-release increments on
  `main`: llama.cpp bootstrap recovery (#145, merged) and setup visibility (`02 §5j`, `03 §7g`,
  #146, merged). Active work is tracked in `05`-`08`, shipped history is in
  `CHANGELOG-ARCHIVE.md` and `specs/archive/`; code-signing is the lone packaging follow-up
  (`03 §11b`).

- **0.4.5 arc status (2026-11-06):** the arc is merged through Wave B — P1–P7 are on `main` — and
  **P8** is implemented on `feat/0.4.5-p8-insights-agent` (not committed, 0.4.5 D18).
  On `main`: **P1** specs contract (`e27f42c`, PR #150), **P2** design system v2 (`7e73e30`, PR
  #151), **P3** baseline schema **v14** plus settings v2 (`0dfa2f5`, PR #153; the working tree now
  carries the v14 baseline instead of the migration chain, `03 §4`, 0.4.5 D2), **P4** automatic
  evaluation with the frozen P5/P6 thresholds (`aceca58`, PR #155), **P5** work graph engine
  (`722f29e`, PR #157; pure `crates/graph`, graph persistence, four kernel job kinds and the idle
  scheduler; acceptance, decisions and evidence in `05`, `06` #37-#42 and `07`), **P6** Ask v2
  ([#160](https://github.com/nicolasestrem/screensearch/pull/160)) — planning, the four ordered
  retrieval stages, both-kind citations, multi-turn conversations and the `session_embed` job lane,
  whose acceptance numbers await the maintainer's live-DB run — and **P7** shell + Settings v2
  ([#161](https://github.com/nicolasestrem/screensearch/pull/161)), merged after the maintainer's
  review (0.4.5 D18; committed and pushed on their authorization), with its gate output, 26-row
  acceptance table, search/keyboard/state passes and screenshot set in `05`. Gate A (`v0.4.3`) and
  Gate B (`v0.4.4`) tags remain the maintainer's calls; **no `v0.4.3` or `v0.4.4` tag exists**, and
  the wave rule (`docs/0.4.5.md` §4.2 — a wave starts when every phase of the previous one is
  merged) is what allowed P8 to start. The executing phases ran under the maintainer's overrides
  (fixture evidence instead of the real-DB maintainer run, D18 waived per branch on the maintainer's
  explicit instruction). The full app is a Rust workspace of **18 module
  crates** plus `src-tauri` (the Tauri composition root) and a React/TS UI.

- **P8 insights + agent surface (2026-11-06):** implemented on `feat/0.4.5-p8-insights-agent`, not
  committed or pushed (0.4.5 D18). Five kinds (`thread_activity`, `open_loop`, `recurrence`,
  `semantic_neighbour`, `resumption`) generate from history as job kind `insight_generate` under the
  existing worker pool, pressure throttle and idle gate; the store writes each insight with its
  evidence in one transaction, and a sweep dead-letters a pass that would leave one unevidenced. Four
  read routes (`GET /v1/threads`, `/v1/threads/{id}`, `/v1/entities`, `/v1/insights`, all
  `history.read` with strict parsing) and four MCP tools (`list_threads`, `get_thread`,
  `find_connections`, `list_insights`; counts 13→17 normal / 10→14 read-only). Daily reports and
  weekly synthesis may cite the window's insights through an additive prompt context block. **No
  schema change** (v14 already carried `insights`, `insight_evidence`, `session_vectors`) and **no
  UI** — P9 renders the screens. Verification and evidence are in `05` (2026-11-06), contradictions
  `06` #93–#101.

- **P6 Ask v2 (2026-11-05):** merged ([#160](https://github.com/nicolasestrem/screensearch/pull/160)). Ask is now
  planned before it retrieves (time intent → half-open window in system-local calendar terms; entity
  mentions resolved through the work graph with a plain-text fallback), retrieves in the normative
  stage order (threads → sessions → frames → assembly, each stage narrowing the previous), cites both
  the frames it read and the sessions those frames belong to, and persists conversations
  (`ask_threads`/`ask_turns`) so a follow-up reuses the previous window and entity scope. One new
  terminal `citations` event precedes `done`; `done` additionally carries the per-stage counts and
  the assembled token total; `POST /v1/ask` accepts `thread_id`. A new `session_embed` job lane keeps
  `session_vectors` warm on idle and on demand, never on the capture path. The legacy frame-level Ask
  keeps its exact semantics and is now written once (`kernel::ask::ask_context`), and a database with
  an empty work graph degrades to that frame-level path with the planned window as the only added
  filter. **No schema, settings-key, table or route change.** Acceptance (evidence recall@10,
  known-fact rate, citation validity, p95) is the maintainer's live-DB run and is not yet recorded;
  the build record is `05`, the decisions `07`, the contradiction rows `06` #46–#53.
- **P5 review-fix pass (2026-11-05):** the merged P5 work graph (`722f29e`, PR #157) still had one
  P1 and three P2 automated-review findings unresolved at merge time. The fixes are on
  `fix/p5-157-review-findings` ([#159](https://github.com/nicolasestrem/screensearch/pull/159),
  **merged** 2026-11-05): per-session link
  reconciliation after session ownership changes, ownership-only linking, a sessions-history
  ordering gate, and the override-model corrections (rename membership, merge retention,
  stale-session filtering). No schema, settings, job-kind, command or UI change. The repository now
  carries a hard rule in `CLAUDE.md`: agents never merge pull requests; merges are maintainer-only.

---

*Next layer:* `02_STRATEGIC_PLAN.md` — "what should change, and why" (the capture-cheap /
enrich-deferred thesis, phased delivery, risks).
