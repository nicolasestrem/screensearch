# 06 — Patch Plan

## 2026-10-19 — Carried forward from the 0.4.2 arc

Resolved items and full history: `specs/archive/06_PATCH_PLAN.v0.4.2.md`. The 0.4.2 arc opened
no ordered rows; its one recorded contradiction (P3 saved-output visibility during delivery
retry, `03 §7f.10`) was resolved by Nico in favour of stored-presence projection. Still pending:

- **Pending Nico (`07`):** the failed-week edge cases (a)–(d) and the disable-before-catch-up
  reading. A changed answer becomes its own patch here. Also pending in `07`: whether a weekly
  synthesis may cite a revision rather than the base run of the same day.
- **Accepted on maintainer report (2026-10-19, `05`):** native acceptance (`04 §1c`) after the
  v0.4.2 release ("Tested everything green and field tested 0 bugs"). No per-item evidence
  was provided: the running-app screenshot of the weekly Settings hint (`04` UI guardrails) and
  an approved Telegram test send are not recorded as done.
- **Maintenance:** retire the `postcss-selector-parser` override (#125) when upstream ranges
  resolve patched copies.
- Remote report generation/retry/delivery remain excluded from the external-agent memory API.
- **Done in the release PR (#142, Codex `PRRT_kwDOTAuCtM6t5thb`):** the archived docs-drift
  follow-up — `docs/f3-g5-g6-verification.md` now points at the 0.4.2 archive and carries a
  dated correction of its candidate-only UI test counts (50 at `957b234`, 55 at v0.4.2).

> **Ordered fixes required before shipping**, plus any **spec contradictions** found during the
> build (`04 §5`). Empty until the build surfaces something.

| # | Priority | Issue | Source (spec §, file:line) | Fix | Status |
|---|---|---|---|---|---|
| 15 | P5 | **Upstream llama.cpp multimodal host-memory leak (build 9842).** ~149 MB committed host RAM leaked per vision inference; VRAM flat. Mitigated locally by the `sidecar.recycle_*` valve (recycle at a committed-RAM ceiling). Real fix requires a newer bundled `llama-server` build. | `07` #72; `specs/03_MASTER_PRODUCTION_SPEC.md §8`; `crates/inference` sidecar lifecycle | **Open (upstream).** Local mitigation shipped (`sidecar.recycle_enabled` default true, `sidecar.recycle_rss_mb` default 0=auto). Upgrade the bundled llama-server build when a fix lands upstream; re-run the 60-frame isolated test to confirm RSS stays flat. | open (upstream) |
| 26 | P2 | **Setup visibility (approved 2026-10-20).** Startup acquisition (engine, search model) and model-lane downloads had no persistent, truthful in-app status; a failed engine bootstrap was visible only in the log (v0.4.2 field failure). | `02 §5j`, `03 §7g`, `UI_REFERENCE §4`/`§5`; `07` #121 | Additive `SetupStatus` snapshot + `setup_changed` event + `get_setup_status`; engine phase/byte reporting in `download.rs`; StatusRail `SetupIndicator` + Settings `SetupDetails`. Native Windows run of a source-built app before merge. | implemented + natively run (`05` 2026-10-20); awaiting PR/merge approval |
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
> Shipped 0.4.2 history (dependency/security fixes #120/#121/#125, external-agent memory API
> #126–#130, Claude workflows #133–#135, F3 deeper memory #131/#136–#141; no rows opened) →
> `specs/archive/06_PATCH_PLAN.v0.4.2.md`.
> Live rows: #15 (upstream llama.cpp leak), #23 (PDH blind to Vulkan), #26 (setup visibility).

When the spec contradicts itself, stop, ask the user, and log the resolution here before coding.
