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
  `schema_version = 15`; a schema change edits the baseline and bumps the version. No
  populated-DB migration tests. The 0.2.x–0.4.2 forward-only migrations are history, not the rule.
- **History is carried, not migrated (0.4.5 D3):** the dev-only `ssv2c-rebuild` tool copies a v13
  DB into the current baseline read-only; the old file is kept as `*.v13.bak`.
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
- **Current state (2026-11-09):** latest release **v0.4.5** (tag `v0.4.5`, 2026-11-07), auto-update
  live since v0.3.2. DB **schema 15** (`crates/store/src/schema.rs:19`), created from one baseline
  (`03 §4`, 0.4.5 D2) with no migration chain: a database at any other version is refused
  untouched, and the dev-only `ssv2c-rebuild` tool carries a v13 history into the new file
  read-only, keeping the old file as `*.v13.bak` (0.4.5 D3). **Audience is a single user** (0.4.5
  D1). Only the Windows NSIS installer, its updater `.sig` and `latest.json` are release assets;
  standalone MCP wrapper archives are not published. Code-signing is the lone packaging follow-up
  (`03 §11b`). The app is a Rust workspace of **18 module crates** plus `src-tauri` (the Tauri
  composition root) and a React/TS UI.

- **0.4.6 in the tree, not released (2026-11-10):** every version declaration reads `0.4.6`
  (`package.json`, `ui/package.json`, the workspace `Cargo.toml` that `src-tauri/Cargo.toml` inherits
  through `version.workspace = true`, the two npm lock roots, `Cargo.lock` and
  `src-tauri/tauri.conf.json`)
  and `docs/release-notes/v0.4.6.md` exists, all on branch `fix/0.4.6-stabilisation`, whose code
  commits `main` already carries and whose documentation commits travel in **PR #177**. No tag has been
  cut and no GitHub release exists, so **v0.4.5 remains the latest release** until the maintainer tags
  one. At tag time this bullet and the release-state bullet above move to
  `specs/archive/01_STATUS_HISTORY.md`, per the archive-on-release rule. Only dev launches were exercised: 20 consecutive isolated cold launches passed
  with no panic, while the same control under the pre-fix ordering also passed 8 of 8, so the
  regression test rather than the launch series carries the discrimination; no release binary was
  launched and no updater download or install was tested.

- **What 0.4.6 changes:** the 0.4.5 launch panic (`state() called before manage()` for
  `screensearch_lib::update::UpdaterState`) is fixed by registering the updater state on the Tauri
  builder through `manage_pre_window_state`, before any window exists, with regression test
  `src-tauri/tests/launch_order.rs`; a settings save now invalidates `throttleStatus` through
  `SETTINGS_SAVE_INVALIDATIONS` (`ui/src/lib/ipc/queryKeys.ts`), with the `enabled` prop removed from
  `ThrottleReadout`; the app serves `Settings::default()` over IPC as `get_settings_defaults`
  (`settings_defaults` module in `src-tauri/src/lib.rs`), with contract tests
  `src-tauri/tests/settings_defaults_contract.rs` and `ui/test/settingsDefaultsContract.mjs`; and the
  `jobs.kind` comment lists all eight `JobKind` tokens (`crates/store/src/schema.rs` and the `03 §4`
  DDL comment). The release also carries the **memory-pressure relief** merged from `main` before
  this branch; that work shipped with its own records and gate, and this branch's gate covers only
  its own items.

- **0.4.6 does not close:** `06` #116 (no turn bound for an Ask turn whose provider never answers)
  and `07` #91 (no topology-change behaviour for a monitor unplug), both blocked on maintainer
  decisions and recorded in `07_KNOWN_GAPS.md`.

- **0.4.6 changes no persisted format or reach:** schema stays **15** with no migration, the closed
  26-key Settings set of `03 §8` gains no key (`get_settings_defaults` answers the 25 rows that
  store a value plus the `api.token` action), and there is no new network destination (0.4.5 D15).

- **Arcs shipped** (full history: `CHANGELOG-ARCHIVE.md`, `specs/archive/`): **v0.4.5**
  (2026-11-07) — work intelligence + design system v2, P1–P10 on `main` as PRs #150–#166
  (`02 §5k`); **v0.4.2** (2026-10-19) — external-agent memory API (`§1b`) and F3 deeper memory
  (`§1c`); **v0.4.1** (2026-10-15) — configurable-bind network API (`03 §7c`) and memory reports
  (`03 §8c`/`§8d`); **v0.4.0** (2026-07-11) — sessions arc; earlier arcs (v0.2.x–v0.3.3) are in
  the archives.

- **Open work:** the `Open, carried forward` list in `07_KNOWN_GAPS.md`, the open rows in
  `06_PATCH_PLAN.md`, and the deferred-but-specced entries in `TODO.md`; active build-loop logs
  live in `05`–`08`. Gate A (`v0.4.3`) and Gate B (`v0.4.4`) were never tagged: the arc shipped as
  v0.4.5 without them, and the questions they would have settled are named in `07`.

- **Next arc: none approved.** 0.4.5 was the last named arc (`02 §5k`); nothing is defined past
  it in `02` or `04`, so any next arc needs the maintainer's approval before work starts
  (`04 §5`).

- Pre-cleanup §8 status history (the per-phase v0.4.5 narratives and earlier current-state text,
  moved 2026-11-09): `specs/archive/01_STATUS_HISTORY.md`.

---

*Next layer:* `02_STRATEGIC_PLAN.md` — "what should change, and why" (the capture-cheap /
enrich-deferred thesis, phased delivery, risks).
