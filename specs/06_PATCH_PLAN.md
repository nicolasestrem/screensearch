# 06 - Patch Plan

The 0.4.5 arc shipped in v0.4.5. Ten phases, PRs #150-#166: P1 #150 `e27f42c`, P2 #151 `7e73e30`,
P3 #153 `0dfa2f5`, P4 #155 `aceca58`, P5 #157 `722f29e` (plus the review-fix pass #159), P6 #160
`6d906a7`, P7 #161 `5d15301`, `60c60d8`, `8e0883a`, P8 #164 `eb556d1`, P9 #165 `9112087` and
P10 #166 `e459185`. Every contradiction and ruling raised in those phases is archived at
`specs/archive/06_PATCH_PLAN.v0.4.5.md`. New rows start below.

## Still open from earlier arcs

| # | Priority | Issue | Source (spec §, file:line) | Fix | Status |
|---|---|---|---|---|---|
| 23 | P3 | **PDH `\GPU Engine(*)\Utilization Percentage` is blind to the sidecar's Vulkan compute** on this NVIDIA box: ~1–2 % summed across all engines while `nvidia-smi` reads 80–100 % during vision inference. `crates/sysmon`'s GPU pressure probe uses exactly these counters, so the 0.2.1 enrichment throttle's GPU arm (`throttle.gpu_enter_pct`) can never engage for the workload it was built to throttle — `gpu_monitored=true` yet effectively unmonitored. CPU arm unaffected. Found while profiling `05` Pass 3 (v0.3.1 archive). | `crates/sysmon` (PDH probe); `03 §5` (throttle contract) | Not a 0.3.1 item (no new subsystems — D8). Future arc: NVML-based probe or per-`engtype` counter audit on Vulkan workloads. | open |

## Current arc

Nothing yet: the 0.4.5 arc shipped and is archived.
