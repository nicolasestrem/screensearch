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
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/06_PATCH_PLAN.v0.4.1.md`.
> Live rows: #15 (upstream llama.cpp leak), #23 (PDH blind to Vulkan).

When the spec contradicts itself, stop, ask the user, and log the resolution here before coding.

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
