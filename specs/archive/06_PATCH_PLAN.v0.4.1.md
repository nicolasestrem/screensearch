<!-- Archived at the v0.4.1 release sweep (2026-10-15): the complete live file as of the v0.4.1 tag. -->

# 06 — Patch Plan

## 2026-10-15 — PR #123 review fix approval

Maintainer approved the review correction plan with GO. Preserve `03 §8c/§8d` contracts:
generation waits for inference settlement (success or failure); manual dedup covers only active
runs; changed Telegram recipients require explicit Retry and linked immutable attempts; partial
coverage is not engine truncation; saved-zone local dates determine memory periods. No schema
bump, release, merge, live delivery or product-policy change is part of this correction.

## 2026-10-15 — Approved report direction resolves C–E policy gate

The maintainer's explicit report-policy approval resolves the prior A3 decision blocker.
Replace the former saved-report deferral and pull-only rule narrowly via `03 §8c`: opt-in
local schedules/folder output, no telemetry or scoring. Telegram permission remains a
separate contract commit. Implementations must preserve failed/unknown outcomes and human files.

## 2026-10-15 — B review fixes and remaining release gates

Resolved before build handoff: fallible startup intent read, post-startup query invalidation,
visible acknowledgement persistence, and Windows exclusive socket binding. The initial failed
loopback-conflict test led to a server correction, not a weakened expected result. Final full
workspace tests pass; raw evidence is in `05`. Native acceptance remains open per
`docs/TESTING.md`; C–E policy decisions remain in the report contract proposal. No release is implied.

## 2026-10-15 — Memory/API contract contradiction

The execution request conflicts with the former all-interface prohibition in `03 §7c`
and the localhost-only rule in `04`/AGENTS. Gate A resolves the API portion in a separate
reviewed contract commit before code. Telegram outbound permission and scheduled delivery
need their separate amendments; this API amendment does not grant either. Implement B,
verify the full native build, then retain B6 cross-machine/runtime acceptance as open until
observed. Do not merge, release or replace an everyday installation.

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
> Live rows: #15 (upstream llama.cpp leak), #23 (PDH blind to Vulkan).

When the spec contradicts itself, stop, ask the user, and log the resolution here before coding.

## Approved maintenance scope — 2026-09-09

Maintainer approved a deletion-only pass: remove the unused `ScreenScaffold` file,
`IconSearch` alias, `startOfLocalDay` helper, `useSetMarkNote` hook, workspace
`thiserror` declaration, and OCR's direct `tracing` dependency. Preserve the live
mark-note command and every generated binding/test. No architecture, schema,
settings, dependency-version, or user-visible behavior change is authorized.

The implementation and local regression gates are recorded in `05` Pass 1. The
initial UI byte-parity failure was investigated, not waived: only the unused
scaffold's Tailwind rule and resulting asset references changed. Independent review
remains a separate gate. Broader event-hook/component consolidation and the npm
audit findings (`07`) require a separately accepted scope, not opportunistic edits.


## 2026-10-14 — PR #112 security maintenance

The maintainer requested that PR #112 get through and that this specific security
alert be resolved, including Claude/Codex feedback. Keep the patched Quinn version
and reconcile the toolchain contract: 1.95 is required by the
committed libsqlite3-sys 0.38.1 build script (cfg_select!, omitted MSRV metadata).
Verify the full Windows suite on pinned 1.95.0 before merging; never restore vulnerable 0.11.14 to preserve the obsolete 1.82 declaration.
CI and release reject versions affected by GHSA-4w2j-m93h-cj5j.

## 2026-10-15 — C–E implementation integration

Approved contracts landed separately as 4e0df3a (local reporting/scheduling) and 217f9ef
(outbound Telegram). Implementation uses default-off per-profile configuration, a shared
coverage report service, schema 12 durable runs/receipts, one serialized worker, installed-only
model resolution, owned atomic files, and write-only Windows credentials. Settings exposes
errors without hiding recovery controls. No release, install replacement or real send is implied.

Review corrections: propagate evidence-read failures rather than treating them as empty;
recover saved runs interrupted before delivery-intent creation; preserve unresolved attempts
and file ownership through retention; guard reparse paths before recovery reads as well as
writes. Verification results follow after the complete workspace gates.


## 2026-10-15 — configurable bind and input corrections (supersedes earlier Gate A defaults)

The maintainer requested configurable IPv4/IPv6 literal binding, persisted as `api.bind_address`.
Missing values default to `127.0.0.1` for fresh and existing enabled profiles; no automatic
broadening occurs. Address/port edits apply immediately, with validation before mutation.
The earlier hard-coded wildcard and automatic-broadening notice contracts above are historical
build evidence, superseded by this correction. Bearer authentication, exclusive Windows binding,
visible bind errors and MCP/inference loopback restrictions remain.

Schedule input uses `HH:mm` hour/minute precision with no seconds field. New memory settings,
delivery destination snapshots and receipt/recovery paths use forward slashes. Legacy ownership
lookup accepts earlier separators; canonical ownership keys are saved transactionally to avoid
duplicates/conflicts. This is path normalization within the Windows-only app, with no schema
migration or cross-platform expansion. Native atomic file operations remain Windows operations.

Contract and implementation corrected. Current verification is recorded below/in
`05_BUILD_REVIEW.md`; earlier raw results remain evidence of their original revisions.


Correction verification passed and an unsigned NSIS installer was produced (`05`). This resolves
the automatic-broadening, seconds-display and path-separator defects in the test build. Native
UI/model/Obsidian, separate-machine API and approved live delivery/pilot gates remain before release.


## 2026-10-15 — live NO_DATA investigation and manual date/range selection

Read-only inspection of the reported running Windows installation found two finished previous-day
runs with zero frames in the requested window, NO_DATA output, successful vault/index receipts
and Telegram skipped with attempt 0. No model or transport request had run. Captures existed for
the following day, so retrying the previous-day window could not include them. Schedule activation
starts with a full subsequent calendar day and fires after that day finishes; selecting a near-future
clock time on the activation day does not bypass that contract. No live configuration, credentials,
notes or run records were changed, and the running app was not stopped.

The maintainer then requested choosing a date or date range. Contract commit `eb5f16d` adds an
optional typed inclusive calendar request, today's frozen in-progress window/PARTIAL framing,
independent explicit-run identities and Reports files, range labels in the shared index/history/
digest, and legacy previous-day/schedule compatibility. Optional backward-compatible JSON fields
require no SQL schema change. Native profile replacement and an actual Telegram send remain
outside this implementation/verification run. Current build/test evidence follows after checks.


Current date-range UI/Rust gates pass: 832 Rust tests, zero failed, 15 existing ignored;
UI install/lint/build/two existing tests, MCP stage, fmt, Clippy and binding guard. Source review
also corrected empty coverage generation after a positive evidence probe: report text must be
nonblank, and all-empty map output cannot become NO_DATA. Prompt-based no-match behavior stays
supported, including normalization of empty/whitespace prompts. Raw evidence and native
acceptance boundaries are in `05_BUILD_REVIEW.md`; packaging follows separately.


Unsigned date-range NSIS packaging passed; release profile completed in 3m 09s and the executable
returned `ScreenSearch 0.4.0` with exit 0. Artifact hash/size/raw output are in `05`. The running
installation, selected vault, settings, credentials and Telegram conversation were not changed.

## PR #122 comment reconciliation — inspected 34590d0

Read the current AGENTS/CLAUDE instructions and complete required 01 → 02 → 03 →
UI_REFERENCE → 04 specification sequence, plus the memory execution plan. PR #123 is
already merged into this head; older reviews of dda1c60 must be checked against the
configurable-bind/no-notice correction rather than applied literally. Preserve the
approved plain-HTTP contract; TLS is not an authorized review-driven policy change.
Independent workers own Telegram retarget regression repair and read-only API triage.
No new PR, remote branch, merge, thread resolution or third review solicitation.

Maintainer approved the remaining connection-bound plan with GO: 64 active pre-authentication
connections on every bind, immediate close at capacity, and a timed 30-second request-header
deadline that does not terminate SSE/export bodies. Preserve bounded shutdown with ownership of
all child tasks. Verify cap/recovery, incomplete headers, long streaming and shutdown on Windows
SSH (`tbx`) only. Rollback is one revert; no schema or settings changes. This exception resolves
the numeric-policy silence in `03 §7c` and revisits gap #89(b) without adding rate limiting.
