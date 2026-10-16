# CLAUDE.md — ScreenSearch V2c

Shared repository guidance for Claude Code, Codex and other coding agents.
`AGENTS.md` points here; maintain the instructions in this file.

## What this is
A standalone, **Windows-only**, local-first desktop app (Rust + Tauri 2) that captures the screen,
makes it searchable by text and meaning, and answers questions about it — fully on-device. This is
a **clean-slate** project; it shares no code or data with any prior version.

**Current state: v0.1.0 shipped 2026-06-24; the 0.2.x arc (attention-first text signal + recall
reports) is shipped; the 0.3.0 arc — "P7: surface reduction + flow recall + local API"
(`docs/0.3.0.md`, `02 §5c`) — shipped as v0.3.0 (2026-07-04); the 0.3.1 patch — "P7.1: post-0.3.0
triage" (`docs/0.3.1.md`) — is complete (PR1 specs contract; PR2 fixed the #64 vision-throughput
regression profile-first — `VISION_MAX_EDGE` back to 1280 px, 102 % of the pre-WebP baseline;
PR3 polish — #59 inline Moment text, #65 report filename/footer, #57-partial version link;
PR4 audited + released v0.3.1). The **0.3.2 arc — "P7.2: product shell mini-arc"**
(`docs/0.3.2.md`) — **shipped as v0.3.2 (2026-07-06)**: lifecycle — **auto-update #69**
(`tauri-plugin-updater` + a signed GitHub-Releases `latest.json`, pull-based passive UX per D1;
v0.3.2 is the updater's **genesis release** — every release after it reaches installs
automatically) and **systray #56 + quick actions #57** (native Tauri tray in
`src-tauri/src/tray.rs`, passive state icon, close-to-tray default-on, run-at-startup via
`tauri-plugin-autostart` default-off, Load/Unload-model + Start/Stop-vision incl. `cancel_vision`,
all pull-based/non-shaming per D4) — and interface — **shell-layout hardening** (D9: one scroll
context per route, no-CLS skeleton parity, NavRail layer isolation as the WebView2 ghost-rail
mitigation `07` #106) and the **Settings two-tier IA** (D6: Essentials + seven collapsed Advanced
expanders; the gap-#100 cross-chord conflict warning; the two dead settings retired via
`RETIRED_SETTINGS_KEYS` tolerate-and-drop, D8). Zero DB schema migrations (D10, schema stays 10);
PR6 audited D1–D12 (all PASS), fixed #89 (daily/weekly report filenames carry the kind), and
released with the signed updater manifest. The **0.3.3 hotfix shipped** (`v0.3.3`, 2026-07-07 — UIA
skips Chromium/Electron windows to stop browser freezes, `07` #93). **The 0.4.0 sessions arc (P8,
"frames → sessions reframe") shipped as v0.4.0 (2026-07-11)** (`docs/0.4.0.md`, `02 §5d`, `03 §7e`):
the specs contract, the ground-truth harness, the arc's **one** schema migration (10 → 11, PR3
only, D4), the pure-heuristic segmentation engine + recognition, the Timeline sessions surface, the
read-only API/MCP session tools, and the PR7 audit. Sessions are **additive** (D10, zero frame-level
behavior change), **pull-based/non-shaming** (D11), **no audio** (D14), **no new NavRail route**
(D13). v0.4.0 is the **first auto-delivered release** (D16): the live update gate passed before the
tag (0.3.3 install, auto-update, restart, first-boot 10 → 11 migration, sessions over pre-existing
history), and it reaches 0.3.2+ installs automatically. `#88` (AI-tool recall) closed at the release.
The full app exists —
a 15-crate Rust workspace + a React/TS UI. The specs remain the contract; the build-loop docs
(`05`/`06`/`07`/`08`) are the live status of record. Code-signing is the lone packaging follow-up
(the 0.3.2 minisign updater signature is not Authenticode — `03 §11b`).

## ⛔ Read the spec before doing anything (mandatory order)
1. `specs/01_PROJECT_CONTEXT.md` — what is true today (env, constraints, non-goals)
2. `specs/02_STRATEGIC_PLAN.md` — what to build, in what phase order (P0→P5)
3. `specs/03_MASTER_PRODUCTION_SPEC.md` — exactly how (schema, traits, protocols, DoD)
4. `specs/UI_REFERENCE.md` — the frontend contract (identity, tokens, screens, states) — for P5
5. `specs/04_CLAUDE_CODE_BUILD_PROMPT.md` — how to operate (this is your operating manual)
Consult `specs/00_PROJECT_INTAKE.md` and `specs/MODEL_REGISTRY.md` for facts (models, license).

Re-read each session — the files are the source of truth, not your memory.

## Source of truth
- *Why / scope / phases* → `02` · *Constraints / non-goals* → `01` · *How (schema, traits, job
  queue, sidecar, settings, DoD)* → `03` · *UI* → `UI_REFERENCE.md` · *Exact model repos/quants*
  → `MODEL_REGISTRY.md` · *How to operate* → `04`.

## Where the code lives
- `src-tauri/` — Tauri 2 shell + composition root (wires all impls; commands/IPC).
- `crates/traits` — module contracts + shared types · `crates/kernel` — orchestrator (event bus,
  worker pool, model supervisor, vision scheduler, where-was-i resume heuristic) · `crates/store` —
  SQLite + sqlite-vec + FTS5 store & job queue · `crates/capture` — WGC capture + diff gate +
  privacy + event triggers · `crates/ocr` — WinRT Media.Ocr · `crates/uia` — UI-Automation text
  source (OCR fallback) · `crates/embeddings` — fastembed (in-process ONNX) · `crates/inference` —
  llama.cpp sidecar client + Job-Object supervisor · `crates/textfilter` — attention-first span
  classifier · `crates/sysmon` — CPU/GPU pressure probe · `crates/doctor` — env smoke-check ·
  `crates/api` — opt-in authenticated HTTP API + export (`03 §7c`, configurable-bind contract) · `crates/mcp` — `screensearch-mcp.exe` stdio
  MCP wrapper over that API · `crates/sessions` — 0.4.0 pure heuristic session segmentation + tool
  recognition (no model calls) · `crates/harness` — 0.4.0 dev-only, read-only segmentation
  validation harness (not shipped in the app).
- `ui/` — React + TS + Vite; typed IPC bindings are generated into `ui/src/bindings/` — never
  hand-edit them.
- Module crates depend on `traits` only, never on each other's impls (`Cargo.toml`, spec `03 §2`).

## Hard rules (non-negotiable)
- **Stop at ambiguity.** Spec explicit → implement exactly. Spec silent → STOP, ask, append to
  `specs/07_KNOWN_GAPS.md`. Spec contradictory → STOP, ask, append to `specs/06_PATCH_PLAN.md`.
  Never guess a product decision to keep momentum.
- **Verbatim verification.** Never claim something works without pasting the raw output of the
  command (build / test / clippy / run). No paraphrase. "Done" = observed running, not "compiles."
- **No stubs / placeholders / hardcoded expected values** to make something look like it works.
  If blocked, stop and ask.
- **Windows-only by design** — use Windows-native APIs (WGC, WinRT OCR, WebView2); do not add
  cross-platform abstractions or stub them away.
- **Path spelling:** use forward slashes in documented, persisted and displayed paths. Restore
  native verbatim/device prefix syntax only at the Windows filesystem boundary.
- **Rust-only ML runtime.** The shipped app's ML is Rust-only — embeddings via fastembed,
  inference via the local llama.cpp sidecar; no Python *ML sidecar* in the runtime (the V1 approach
  that failed). Python is fine for build/dev tooling (model prep, the `hf` CLI, CI scripts).
  No cloud inference or arbitrary cloud uploads. Network exceptions: localhost, model downloads, the signed GitHub-Releases
  update check (`03 §11b`), and user-enabled authenticated HTTP API serving on a user-selected IPv4/IPv6 literal (default loopback)
  (`03 §7c`, Gate A API amendment). That API is plain HTTP: any reachable token holder can read
  history. No public hosting, automatic firewall/router changes, network-hosted MCP server, or exposed inference.
  Approved remote-wrapper boundary: only the standalone stdio HTTP-client wrapper may run on
  other OSes and use explicitly configured HTTP/HTTPS remote destinations (`03 §7c`). The app,
  capture and inference remain Windows-only; inference remains loopback-only. The wrapper keeps
  `.no_proxy()`, validates destinations and refuses all redirects; no listener or CORS changes.
  The approved local-memory amendment permits default-off scheduled local reports and user-selected
  folder writes (`03 §8c`), without nags or scoring. The separately approved Telegram exception
  (`03 §8d`) permits opt-in text digests of saved reports to a user-selected Bot API destination;
  never raw OCR/screenshots, automatic recipient selection, remote commands or cloud inference.
- **No real-time vision** — vision runs on-demand / timer / idle only (`03 §5`).
- **Sidecar must never orphan** — implement the Job-Object lifecycle exactly (`03 §6`); do not ship
  P4 until the no-orphan test passes.
- **Schema changes = forward-only migration** with a `schema_version` bump. No schema drift.
- **Branches, not main.** New work on a feature branch; no force-push; no commit to `main` without
  review. Never commit models, secrets, or DB files (see `.gitignore`).
- **PR review is automatic.** Opening or updating a PR triggers the configured reviewers; do not
  `@`-mention review bots for routine review or post ritual "do not merge" reminders. Mention a
  reviewer only when the maintainer explicitly asks for a targeted follow-up on a specific area.
  Address actionable bot feedback in code; bot-thread replies are unnecessary.
- **UI:** typed IPC via `ts-rs` only; every view defines all states (loading/empty/error/partial/
  populated); Rules-of-Hooks is an error-level gate; tokens only (no hardcoded hex/font/spacing).

## Build/verify (matches CI — `.github/workflows/ci.yml`)
Order matters: build the UI first — `src-tauri`'s `generate_context!` embeds `ui/dist` (git-ignored),
so cargo fails if the UI hasn't been built.
1. UI: `cd ui && npm ci && npm run lint && npm run build`  (lint = Rules-of-Hooks error gate)
2. Stage the MCP sidecar: `node scripts/stage-mcp.mjs` — `src-tauri` declares
   `screensearch-mcp.exe` as a `bundle.externalBin`, which `tauri-build` resolves on **every**
   compile of the app crate (not just at bundle time). A fresh clone must run this once before
   any `cargo` command or the workspace build fails (0.3.0 PR8). `beforeDevCommand`/
   `beforeBuildCommand` run it automatically; CI runs it too.
3. Rust: `cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
   `cargo build --workspace` · `cargo test --workspace`
4. Binding guard: `cargo test` regenerates the ts-rs bindings —
   `git diff --exit-code -- ui/src/bindings` must be clean (commit regenerated bindings, or CI fails).
- Run the app: `npm run dev` (NOT a direct executable and NOT `cargo tauri dev`).
  Package: `npm run build`.
- Toolchain: Rust 1.95.0 (pinned in `rust-toolchain.toml` and CI), Node 22. Paste verbatim output when reporting status.

## Build-loop notes (keep current)
Append your work record to `specs/05_BUILD_REVIEW.md`, `06_PATCH_PLAN.md`, `07_KNOWN_GAPS.md`,
`08_CHANGELOG_AI.md` as you go (`04 §7`).

**Archive on release.** On each version tag, move that version's shipped entries from the live
build-loop logs (`05`/`06`/`07`/`08`) and `CHANGELOG.md` into `specs/archive/` and
`CHANGELOG-ARCHIVE.md`. Live logs hold only the current arc; archives in `specs/archive/` preserve
full history.
