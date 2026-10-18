# 06 — Patch Plan

## 2026-10-18 — PR #139 review batch 3

- Four findings fixed in `fe72af8` (note path identity, per-item weekly citations with
  retained-frame provenance, saved-run timezone in the export footer) plus one
  integration-found coverage-wording defect. Full native Windows chain at `fe72af8`:
  `CHAIN_EXIT=0`, Rust 1094/0/15 ignored, UI 42/0; raw output in `05`.
- Two product decisions recorded rather than guessed (`07`): a missing note has no
  canonical identity (case-only respellings of a nonexistent note get distinct fallback
  keys), and a weekly `frame_ids` entry from a retained day means "frames the synthesis
  rests on", not frames re-read. No new schema, dependency or wire field.
- Documentation/publication authorized. New-head hosted checks, the supplementary
  `@claude`/`@codex` round and native interactive acceptance are separate from this
  record. Merge not performed by this work record.

## 2026-10-18 — PR #139 post-install conflict reporting

- Codex `r4307200508` is valid: a writer conflict can follow a successful install.
  Fixed in `e69ee1f`: settle the intent/live hash before returning the receipt;
  record ownership when the append is installed and retain the recovery path.
  The dialog uses receipt hash to distinguish written from no-append conflicts.
- Full native Windows chain passed at `e69ee1f`, `CHAIN_EXIT=0`; Rust 1086/0,
  UI 37/0. Exact outputs and test boundaries are recorded in `05`.
- Documentation/publication authorized. New-head hosted checks and native
  interactive acceptance are not implied by the earlier CI result. No additional
  requested review round; merge not performed by this work record.

> **Ordered fixes required before shipping**, plus any **spec contradictions** found during the
> build (`04 §5`). Empty until the build surfaces something.

| # | Priority | Issue | Source (spec §, file:line) | Fix | Status |
|---|---|---|---|---|---|
| 15 | P5 | **Upstream llama.cpp multimodal host-memory leak (build 9842).** ~149 MB committed host RAM leaked per vision inference; VRAM flat. Mitigated locally by the `sidecar.recycle_*` valve (recycle at a committed-RAM ceiling). Real fix requires a newer bundled `llama-server` build. | `07` #72; `specs/03_MASTER_PRODUCTION_SPEC.md §8`; `crates/inference` sidecar lifecycle | **Open (upstream).** Local mitigation shipped (`sidecar.recycle_enabled` default true, `sidecar.recycle_rss_mb` default 0=auto). Upgrade the bundled llama-server build when a fix lands upstream; re-run the 60-frame isolated test to confirm RSS stays flat. | open (upstream) |
| 23 | P3 | **PDH `\GPU Engine(*)\Utilization Percentage` is blind to the sidecar's Vulkan compute** on this NVIDIA box: ~1–2 % summed across all engines while `nvidia-smi` reads 80–100 % during vision inference. `crates/sysmon`'s GPU pressure probe uses exactly these counters, so the 0.2.1 enrichment throttle's GPU arm (`throttle.gpu_enter_pct`) can never engage for the workload it was built to throttle — `gpu_monitored=true` yet effectively unmonitored. CPU arm unaffected. Found while profiling `05` Pass 3 (v0.3.1 archive). | `crates/sysmon` (PDH probe); `03 §5` (throttle contract) | Not a 0.3.1 item (no new subsystems — D8). Future arc: NVML-based probe or per-`engtype` counter audit on Vulkan workloads. | open |

> Resolved v0.1.0-era rows (#1–#4) → `specs/archive/06_PATCH_PLAN.v0.1.0.md` (ids preserved).
> Shipped 0.2.x rows (#5–#14 + the 0.2.1 build notes) → `specs/archive/06_PATCH_PLAN.v0.2.x.md` (ids preserved).
> Resolved 0.3.0-arc rows (#16–#20) → `specs/archive/06_PATCH_PLAN.v0.3.0.md` (ids preserved).
> Resolved 0.3.1-era rows (#21/#22/#24) → `specs/archive/06_PATCH_PLAN.v0.3.1.md` (ids preserved).
> 0.3.2 opened no rows → `specs/archive/06_PATCH_PLAN.v0.3.2.md` (stub, for per-release symmetry).
> Resolved 0.3.3 row (#25 — UIA skips Chromium/Electron) → `specs/archive/06_PATCH_PLAN.v0.3.3.md` (id preserved).
> Resolved / logged 0.4.0-arc rows (#26–#32 — the D9 gate MET, the §7e-folded concurrent model, the PR3/PR5/PR6 findings) → `specs/archive/06_PATCH_PLAN.v0.4.0.md` (ids preserved).
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/06_PATCH_PLAN.v0.4.1.md`.
> Live rows: #15 (upstream llama.cpp leak), #23 (PDH blind to Vulkan).

When the spec contradicts itself, stop, ask the user, and log the resolution here before coding.

## 2026-10-18 — PR #139 additional review findings

- Three new relevant Codex findings: `4305601207` (append/ownership persistence gap),
  `4305709131` (creation through a junction before writer validation), and `4305709134`
  (daily links lost after run retention). The older findings remain fixed; no third bot review
  or thread resolution is requested.
- Confirmed writes now use a per-note intent and last receipt in existing internal settings KV;
  recover the original identity before run lookup, distinguish selection/date, preserve edited
  bytes and keep IPC critical work alive independently of its response. Creation defers all
  parent creation to the existing guarded writer. Weekly indexing prefers the verified daily
  note's canonical path date before a run-record fallback. Implementation readings are in `07`.
- Native targeted checks: 26 promotion tests passed, including real SQLite-trigger failures,
  owned/unowned-note recovery, lost response, restart/pruning, detached work and an actual Windows
  junction. Full Windows `04 §1c` verification at `aba0c28` then passed with `CHAIN_EXIT=0`:
  Rust 1084 passed / 0 failed / 15 existing ignored; UI 35 passed / 0 failed. Bindings stayed
  clean. Exact output and the production-path retention/index regression are recorded in `05`;
  subsequent changes in this batch are documentation only.
- No product/schema/public-setting/wire change, G5/G6 work, installed-app access or live data.
  The shared checkout moved to a concurrent G5/G6 branch during execution; this batch is isolated
  on `fix/pr139-review-followup` in a local worktree. Only the ONE existing tbx checkout is used
  for native validation. Publication requires a fresh authorization; no merge is authorized.

## 2026-10-18 — PR #139 hosted scanner follow-up

- CodeQL's alert gate (not the passing analysis jobs) flagged incomplete regex escaping in a UI
  test at `66980b3`. The test now uses literal substring containment; native Windows lint and all
  35 UI tests pass (`05`). No production changes or new policy/schema decision. Publication and
  a fresh hosted scan remain pending; do not mark the old red alert gate as cleared.

## 2026-10-18 — PR #139 review corrections

- Addressed Claude's five findings and Codex's three inline findings on `fc39c65` within G3/G4:
  slug safety, current single-note confirmation, fenced/case-tolerant section handling,
  duplicate-selection multiplicity, ownership semantics, persisted ownership checks,
  settings-dependent note-status refresh and concurrent creation exclusion.
- `03 §8c`/`§8e` already require the ownership guard: restoring that behaviour is a bug fix, not
  a new adoption policy. An edited owned note remains a conflict; only an initially unowned note
  may be adopted. No product contract, schema, generation surface or G5/G6 scope changed.
- Full Windows CI-order verification passed at `1636a4f` (`05`); runtime source is frozen at that
  tested commit. Native real-week/Obsidian/interactive promotion acceptance remains open and
  publication/merge require their own authorization. Existing ordered rows remain unchanged.

## 2026-10-18 — F3 G3+G4 decisions closed (no new rows)

- **`YYYY-Www` position in `Index.md` (left open by `03 §8e`):** resolved to a chronological
  interleave with daily entries, newest first, the weekly row keyed on the window's last local date
  so it opens its own week's block. `src-tauri/src/memory.rs` sorts descending on
  `{date}/{relative_path}` and never on the week label. The spec's silence was filled rather than
  contradicted, so this is a resolution, not an ordered fix.
- **Weekly note content and `NO_DATA` output:** the note carries no second banner (the saved
  synthesis already has its PARTIAL banner), and a `NO_DATA` week writes neither note nor index,
  keeping its skip receipts. G2-era skip receipts are not retro-written; Regenerate makes a linked
  revision.
- **Promotion granularity, date and write shape (spec silent on all three):** one selectable unit is
  an individual bullet mapped `Explicitly evidenced decisions` → `Decisions`, `Open questions` →
  `Open questions`, `Next steps` → `Next steps`, `Activity` → `Log`; the entry date is the promotion
  date computed at prepare in the run's saved timezone and carried in the confirmed proposal; one
  promotion writes one `### date` block per touched section in a single atomic write. Implemented in
  `src-tauri/src/promotion.rs` and `src-tauri/src/memory_files.rs`.
- **No patch rows opened.** None of the above contradicts `02 §5i` or `03 §8e`, so rows #15 and #23
  stand unchanged and this increment adds no ordered fix. The one implementation edge — a consumed
  day whose daily note was rewritten by a linked revision rendering as a plain-text date — is
  recorded in `07`, because no fix is pending. No acceptance or merge is claimed here: the `04 §1c`
  Windows chain is still in progress (`05`).

## 2026-10-15 — Security maintenance

All six current alerts are triaged in `05`: five targeted dependency fixes and
one evidence-backed CodeQL false-positive dismissal. User authorized the work
and PR, with release-first sequencing. PR #124 is merged and v0.4.1 is tagged;
publication/assets must be confirmed before opening the security PR. No merge
or release is authorized here. Native Windows CI remains required before merge.
Retire the selector-parser override when upstream ranges resolve patched copies.

## 2026-10-16 — External-agent memory API P0

- **Resolved (Nico GO), P3 only — historical record preserved:** the approved plan asked for
  null results for unfinished runs, but `src-tauri/src/memory.rs` at `cd9f5fe` (lines 613–617,
  786–788) retains immutable saved output during queued/running delivery retries. `03 §7f.10`
  recorded the contradiction and Nico was asked whether reads should expose that existing output;
  the recommendation was stored-presence projection. **Nico resolved it in favour of stored-presence
  projection:** expose an immutable saved result immediately whenever it exists, including while
  delivery is queued or retrying; absent output is explicit null; stored `NO_DATA` stays factual; a
  malformed saved payload is a read error. The earlier "do not implement either visibility policy
  until answered" hold is **superseded** for P3; P1/P2 were independently authorized and are
  unaffected. P3 implementation is in progress on `feat/agent-memory-saved-reports`; runtime
  verification is pending and no P3 result is claimed. Remote generation/retry/delivery remain
  deferred.
- Remote-wrapper scope is a separately reviewable boundary amendment before runtime
  changes. Capture/app remain Windows-native; inference remains loopback-only.
  No listener-default change, schema migration, main merge or release is authorized.
