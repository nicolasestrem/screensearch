# 06 — Patch Plan

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
