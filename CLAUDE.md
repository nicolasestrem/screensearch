# CLAUDE.md — ScreenSearch V2c

Shared repository guidance for Claude Code, Codex and other coding agents.
`AGENTS.md` points here; maintain the instructions in this file.

## What this is
A standalone, **Windows-only**, local-first desktop app (Rust + Tauri 2) that captures the screen,
makes it searchable by text and meaning, and answers questions about it — fully on-device. This is
a **clean-slate** project; it shares no code or data with any prior version.

**Current state:** latest release v0.4.1 (2026-10-15), DB schema 12, auto-update live since v0.3.2.
The full app exists — a 15-crate Rust workspace + a React/TS UI. Active work is tracked in the live
build-loop docs (`05`/`06`/`07`/`08`); shipped history is in `CHANGELOG-ARCHIVE.md` and
`specs/archive/`. Code-signing is the lone packaging follow-up (`03 §11b`).

## ⛔ Read the spec before changing anything (targeted, not exhaustive)
The specs are large (`03` alone is ~150 KB) — **do not read them end to end**. Each session:
1. Read `specs/01_PROJECT_CONTEXT.md` (short: env, constraints, non-goals).
2. Read only the `specs/04_CLAUDE_CODE_BUILD_PROMPT.md` §1 pointer for the current arc/task.
3. For `02` / `03` / `UI_REFERENCE.md`, grep the headings (`^#`) and read only the sections the
   task touches (e.g. `03 §7f`), plus anything those sections cross-reference.
Consult `specs/00_PROJECT_INTAKE.md` and `specs/MODEL_REGISTRY.md` only for model/license facts.
The files are the source of truth, not your memory — re-read the relevant section, never recall it.

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
  This is approved scope, not a claim of implemented portable/remote behavior.
  Nico approved execution of the external-agent memory increment (`02 §5h`, normative `03 §7f`):
  chronological full-text reads, truthful bounded coverage, independent read credentials and
  saved-report HTTP/MCP reads. Only saved output is promoted; remote generation/retry/delivery
  remain excluded. Do not repeat blanket plan approval; require independent boundary review,
  real per-unit verification and separate merge/release authorization. Schema remains 12 unless
  a separately approved migration is needed; never weaken allocator/identity protection to fit KV.
  Read authorization is explicit per route/method; no new scope is implicitly granted. Existing
  local workflows, listener security/defaults and Windows NSIS bundling remain compatible.
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
- **PR review is on-demand only (cost).** Nothing reviews a PR automatically. Never trigger a
  review yourself — no `/claude-review` or `@claude` comments, no manual workflow runs, no ritual
  "do not merge" reminders — unless the maintainer explicitly asks. Address actionable review
  feedback in code; bot-thread replies are unnecessary. Workflows:
  `.github/workflows/claude-code-review.yml` runs on a `/claude-review` PR comment or a manual
  dispatch; `.github/workflows/claude.yml` runs on an `@claude` mention.
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
