# 05 - Build Review

The 0.4.5 arc shipped in v0.4.5. Ten phases, PRs #150-#166: P1 #150 `e27f42c`, P2 #151 `7e73e30`,
P3 #153 `0dfa2f5`, P4 #155 `aceca58`, P5 #157 `722f29e` (plus the review-fix pass #159), P6 #160
`6d906a7`, P7 #161 `5d15301`, `60c60d8`, `8e0883a`, P8 #164 `eb556d1`, P9 #165 `9112087` and
P10 #166 `e459185`. The full build record, every ordered gate, raw log excerpt, acceptance walk,
measured budget and review-fix pass included, is archived at
`specs/archive/05_BUILD_REVIEW.v0.4.5.md`. New entries start below.

## Current arc

### 2026-11-09 — Off-arc: memory pressure arm, level-2 relief, lazy embedder (branch, not shipped)

**Scope.** Branch `feat/memory-pressure-throttle`, built from a measurement pass against the
installed 0.4.5 build. Off-arc, no phase: four gaps found by that measurement, of which three are
implemented here and the fourth (`06` #23, the GPU arm's blindness) is measured and recorded with
options only, by instruction. Capture, OCR and storage stay structurally outside the worker pool —
they never throttle — and screenpipe was never touched.

**Why, and what the baseline actually says.** Every number below is the **installed 0.4.5 baseline,
measured before any change**, not this branch's footprint:

| Reading (installed 0.4.5, before this branch) | Value |
|---|---|
| App private bytes, text embedding model resident | 1352 MiB |
| App private bytes, `enrich.embed_text = false` | 119 MiB (delta 1233 MiB) |
| Available physical RAM held while the sidecar was loaded | ~5.8 GB |
| Commit charge held over the same window | ~8.4 GB |
| VRAM held over the same window | ~6.8 GB |
| GPU as PDH sums it, peak during vision inference | 37.6 % (`engtype_3d` max 25.5 %, `compute 0/1` 0 %) |
| GPU as `nvidia-smi` reads it, same inference | 96 % |

Two of those motivated the change. The shipped throttle's arms are CPU and GPU only, so a machine
that is out of RAM while ScreenSearch is idle never throttles: no `vision_tag` claims are happening,
and CPU/GPU stay quiet. And level 2 only paused lanes — it freed nothing, because the 6–8 GiB is
held by the sidecar, not by the workers.

The 119 MiB row is **not** an unload measurement. It is the same app with the embedder disabled, so
the delta says what the model costs. A separate isolated harness with the real cached model now
verifies the holder's own unload/reload lifecycle (next section); it does not project that 119 MiB
onto the whole app, and the reload latency is still untimed.

**What changed, by seam.**
- `traits`: `PressureSample` gains `mem_used_pct`, `commit_pct`, `avail_phys_mb`,
  `commit_charge_mb`, `commit_limit_mb` and `mem_monitored`; `ThrottleStatus` gains
  `mem_monitored`; `PressureProbe` gains `mem_monitored()`; `PressureRelief::release_memory()` is the
  new async relief contract. All re-exported, bindings regenerated.
- `sysmon`: `mem.rs` reads physical usage from `GlobalMemoryStatusEx` (`ullTotalPhys`/`ullAvailPhys`)
  and system commit from `GetPerformanceInfo` (`CommitTotal`/`CommitLimit` × `PageSize`) — two
  syscalls, and both must succeed or the sample is `mem_monitored = false`. MiB fields floor.
- `kernel`: the clock-injected `ThrottleMachine` gains the memory hysteresis (either reading hot at
  its enter threshold; both must cool below their exit thresholds; an unmonitored sample leaves the
  arm inactive) and the governor asks `io.relief` only on the transition into Sustained
  (`should_relieve(prev, lvl)`), awaited inline so a level is not republished mid-release. The hook
  is a late-bound `Arc<RwLock<Option<Arc<dyn PressureRelief>>>>`, so the kernel still depends only
  on `traits` and the composition root stays the owner.
- `inference`: one teardown path, `unload_reasoned(reason, skip_if_pinned)`, takes the request gate
  first and consults the pin only after it, so a preload landing during relief cannot be killed;
  it emits `Evicted`. Relief is rule A: `skip_if_pinned = true`, a pinned model survives and the
  skip is logged. Manual Unload clears the pin and passes `false`.
- `embeddings`: `LazyEmbedder` holder — allocates nothing until first use or eager `load_now`,
  single-flight loads, metadata without loading, a 30 s sweep that evicts only when no embed still
  holds the session (strong count 1 under the slot write); sweep and explicit unload return `false`
  while one is active, the load gate spans teardown so a reload cannot overlap it and is held by the
  load rather than by the awaiting caller (so a cancelled first embed cannot release the flight
  before the model is published — see the PR #175 review pass below), a completing embed stamps
  last-use before releasing its handle, teardown off the async worker thread.
- `src-tauri`: `init_embeddings` builds `Arc<LazyEmbedder<FastEmbedProvider>>`, loads eagerly so a
  first search after `Ready` is not cold, arms the unloader only after a successful load, and
  attaches it; the composition root installs the supervisor as `dyn PressureRelief`.
- Settings/UI: four memory thresholds and one idle TTL, all `file-only` and clamped, so the closed
  26-key UI ceiling is untouched. The Settings · Advanced throttle readout shows `RAM 71% (8.3 GiB
  available)` plus a Commit percentage from the same sample, reports available memory rather than
  "free", says "not monitored" instead of 0 %, and its level-2 line says the app tries to free the AI
  model's memory unless the user explicitly kept it loaded. The rail tooltip adds RAM and commit
  percentages.

**Narrow runtime evidence: the real-cached-model harness (PASS).** Run by the parent on 2026-11-09
with `path` dependencies on this branch's `embeddings`, `sysmon` and `traits`, the repository
lockfile, and the real fastembed cache copied to `Temp/ss-memory-proof/cache`. It did not launch the
app and changed no real profile. Raw artifact: `artifact://288`. The branch's two ignored real-model
tests later ran separately against a copied cache on the final holder code and passed 2/0
(`artifact://325/314`), so the lifecycle is no longer covered only by this one-off harness.

| Harness process private bytes | Value |
|---|---|
| Before the model was loaded | 7 MiB |
| After `load_now` with the real model | 1228 MiB |
| After the idle sweep evicted it (TTL 2 s) | 26 MiB |
| After the next embed reloaded it | 1234 MiB |

The real background sweep logged `loaded 21:17:46.376`, `unloaded 21:18:16.484` and `reloaded
21:18:24.739` (UTC as recorded); an explicit `unload()` after 35 s returned `false` because the
sweep — not that call — had already evicted the model, so nothing was resident to unload (no embed was
in flight). The query before and after the cycle returned
identical 768-dim vectors, `maxdiff 0`. The host sample at the same run read physical 69.08 %, commit
57.48 %, available 9841 MiB, charge 30068 MiB, limit 52310 MiB.

What it establishes: with a real model, the holder loads on demand, the real 30 s sweep evicts it
after the TTL, the next embed reloads it, and embeddings are unchanged across the cycle. What it does
not: the whole app's idle footprint (26 MiB is the harness process, and the 119 MiB figure came from a
different configuration), reload latency (the unload→reloaded gap includes harness waiting, not a
timed reload), level-2 relief, or any Tauri/IPC path.

**UI proof, and what it does not prove.** The parent rendered the real `ThrottleReadout` component in
a disposable Vite harness with an explicitly illustrative `QueryClient` fixture (not native
telemetry), across the populated-L2, missing-GPU, missing-memory, before-first-sample, loading,
error and off states, and inspected Chromium screenshots at 1000 px pine-light and 360 px ink-dark;
then 360/1000 px × pine-light, pine-dark, ink-light, ink-dark reported `scrollWidth <= innerWidth`
in all eight combinations with no browser errors. After the copy settled on the final plain-language
wording, that wording was rendered and inspected at 360 px pine-dark: `scrollWidth` 360, no browser
errors. The fixtures and the temporary server were removed afterwards. This is **browser component
rendering evidence only** — it is not Tauri/WebView2 acceptance, not real telemetry, and not proof
that a level-2 relief actually unloads a model or that the idle TTL returns memory.

**Status: gate3 green on build and tests, with explicit caveats.** Gate history is kept because it is
what the fixes came from: the first full gate had UI ci/lint/build, MCP-sidecar staging,
`cargo fmt --check`, strict clippy and the workspace build green, then failed the full `cargo test` on
the new Windows `idle_ttl_bounds_eviction` test (`Instant::checked_sub` rewind); the forward-only
explicit-`Instant` clock and completion-handle race fixed it. Gate2 (`artifact://325/314`) then ran
embeddings unit 8 passed / 2 ignored, both ignored real-model tests 2/0 on a copied cache against the
final code, and `inference` 125, `no_orphan` 1, `reap` 3, kernel 228 unit + integration (throttle 3,
settings 32) green, but stopped at `rebuild::settings::spec_table_parses_to_the_committed_sets` (left
49, right 44) — an incidental hardcoded file-only count while the runtime policy already parses 49
from `03`. The parent removed the obsolete `file_only_count` accessor (no production callers,
grep-confirmed) and the 44 / deleted-9 assertions, keeping the closed UI budget as
`ui_settings_respect_the_closed_ceiling` (`<= 26`). The narrow rebuild test passed 6/0 first in the
final artifact, then gate3 (`artifact://327` full, `artifact://331` tail, log
`C:/Users/nicol/AppData/Local/Temp/ss-memory-gate.log`) recorded:

```text
ui_ci EXIT=0
ui_lint EXIT=0
ui_build EXIT=0
stage_mcp EXIT=0
cargo_fmt EXIT=0
cargo_clippy EXIT=0
cargo_build EXIT=0
cargo_test EXIT=0
BINDINGS_REGEN_CHANGED=[]
bindings_git_diff EXIT=1
```

The verified Node-parser aggregate over the full workspace (doctests counted as suites) was exactly
`{"suites":86,"passed":1650,"failed":0,"ignored":17}`. `BINDINGS_REGEN_CHANGED=[]` means the four
already-regenerated bindings hashed identically before and after `cargo_test`; the non-zero
`bindings_git_diff` was gate3's expected pre-commit caveat — at that point the bindings were
uncommitted and no commit was authorized — so gate3's record does not call the CI binding guard green.
Since then the user authorized committing on the new branch `feat/system-memory-relief`, and all 31
feature paths are staged with the generated bindings: `git diff --exit-code -- ui/src/bindings` now
exits 0 with no output, and the staged diff check excluding generated TypeScript also exits 0. The
guard is therefore satisfied by the staged tree, not pending; nothing here claims the commit has
landed — the parent's commit is the next step.
The two real-model tests (2/0 above) and the background-sweep harness (`artifact://288`) remain the
narrower runtime evidence. This is build/test evidence, not a whole-app measurement: no
changed-build idle footprint, timed reload, real level-2 relief or native Tauri acceptance is claimed.
This documentation pass ran no build, test, lint, formatter or git command, did not run the app, and
touched no real profile, model file or DB.

**Open on this branch (recorded, not claimed as met).**
- Whole-app live evidence: the changed build's idle private bytes, a timed reload, and one real
  level-2 relief on a starved machine. The isolated harness covers only the holder lifecycle, and
  the baseline table above cannot substitute.
- `06` #23 GPU blindness: findings and three options only; no NVML probe, no counter change.
- `06` #150 (0.4.5 launch panic): observed on the measurement pass's launch attempts and worked
  around with a local relaunch harness (not product code); untouched here.

**Review pass, PR #175 (2026-11-09).** One finding, on the head that carried the holder (`959b008`),
and it is valid.

| # | finding | verdict | resolution |
|---|---|---|---|
| 1 | `crates/embeddings/src/lib.rs` — a cancelled `acquire()` drops the borrowed async `load_gate` while the blocking load carries on, so the next caller can start a second ONNX load beside the first and the first never publishes | valid, proved from the call shape | fix shape: the gate is owned by the load, and load, publication and stamp all happen under it; the fix landed as `63e22f1`, with its test failing under a neutered gate and passing as written (raw output below) |

Why it holds: `acquire()` took `load_gate.lock().await` in the caller's own future and then awaited
`spawn_blocking(M::load)`. Dropping that future — a cancelled embed, a lost race, a shutting-down
task — released the borrowed guard at once, while `spawn_blocking` does not stop with the caller that
was awaiting it, so the load kept allocating with the gate free. The next embed then saw an empty
slot, passed the sweep's strong-count test (nothing was resident to protect) and started a second
`M::load`; the first load's model was dropped unpublished, so its allocation was made and released
without ever serving an embed. The fix shape moves the flight guard into the work it protects: the
gate is taken by value and moved into the blocking closure together with the slot and the idle clock,
whose body opens with `let _gate = gate;` so the guard outlives the load and the publication it
performs, and the waiting caller only awaits the join. The single-flight and teardown-spanning
sentences elsewhere in this file and in `08` described the caller-owned arrangement and are amended
above; they were true of the code they described, not a proof against cancellation.

**Verification: the test discriminates — fail side and pass side both observed.** The gate3 result
above, the two real-model tests and the sweep harness all drove load → idle → reload from live callers,
so none of them exercised a cancelled first load and none can be cited for this fix. That path now has
its own test, and the parent ran both directions of it.

Fail side (`artifact://422`): with the fixed closure's `let _gate = gate;` temporarily replaced by
`drop(gate); // NEUTERED…` — the gate released before the load finishes, which is the pre-fix
behaviour — `cargo test -p embeddings cancelled_cold_load_publishes_one_session -- --exact
tests::cancelled_cold_load_publishes_one_session --nocapture` failed at
`crates/embeddings/src/lib.rs:697` on `assertion left == right failed: the published session is reused,
not loaded`, left 2 / right 1, with `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11
filtered out`. Two loads where the test demands one is exactly the defect this row records, so the test
detects it rather than merely compiling.

Pass side (`artifact://424`), on the restored source:

```text
abf2c22859b23abf4ab95e0b4ccbb5f3a8e885b75c21427fc570d482f12589be  crates/embeddings/src/lib.rs
test tests::cancelled_cold_load_publishes_one_session ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.02s
```

The same artifact carries the whole affected module: `test result: ok. 10 passed; 0 failed; 2 ignored; 0
measured; 0 filtered out; finished in 0.04s`, its two ignored entries being the real-model tests, and
doc-tests `0 passed; 0 failed; 0 ignored`. The hash in that block is the restoration of the proof
snapshot — exactly the value intended before the mutation — so it identifies the file the pass side ran
on, not the current working-tree file: `cargo fmt` reformatted the source after the proof, and the
current file hashes differently. A grep for `NEUTERED` returned no matches at restoration, so the probe
was removed rather than left behind. That artifact's lib-test build emitted one warning — `unused
import: std::future::Future` at `crates/embeddings/src/lib.rs:411` — and the first full gate attempt
rejected it (below).

An earlier attempt at the test did not compile (`E0716`, a temporary dropped while borrowed); it was
corrected by binding `inputs` before the timeout, and that compile failure is not counted as evidence
for or against the fix.

**Gate.** The first full attempt (`artifact://428`) ran the ignored real-model tests and passed —
`test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 12.92s`,
across two suites including doc-tests — and `ui_ci`, `ui_lint`, `ui_build`, `stage_mcp` and `cargo_fmt`
each reported `EXIT=0`; `cargo_clippy` then reported `EXIT=101` on that unused import, with
`-D unused-imports` implied by `-D warnings` and `STOPPED_AT=cargo_clippy`. Only the unused import was
removed, no behaviour change. The final run (`artifact://429`, tail `artifact://432`, log
`Temp/ss-pr175-review-gate.log`) reported:

```text
ui_ci EXIT=0
ui_lint EXIT=0
ui_build EXIT=0
stage_mcp EXIT=0
cargo_fmt EXIT=0
cargo_clippy EXIT=0
cargo_build EXIT=0
cargo_test EXIT=0
bindings_diff EXIT=0
```

The verified workspace aggregate is exactly `{"suites":86,"passed":1652,"failed":0,"ignored":17}`,
`git diff --check` exits 0, and the focused and module results above hold on the final code. `06` #151
is resolved by this pass, which landed as `63e22f1` ("fix: preserve embedding load ownership across
cancellation").

**Follow-up review, PR #175 head `63e22f1` (2026-11-09).** The automated re-review of that exact head
`completed` with no findings — `code_review` at `63e22f1`, `has_any_findings=false`,
`has_unresolved_findings=false`, reported through issue-comment summary `6752419125`. The only earlier
bot inline, comment `4518433333`, stays resolved. No REST review was requested for a later head, so
nothing here claims a bot review of any later commit.

**Independent review of the `mem.rs` FFI boundary — scope: that boundary, not the PR.** A second,
read-only review read only the Windows FFI in `crates/sysmon/src/mem.rs`: the two direct syscalls
(`GlobalMemoryStatusEx` at `:108`, `GetPerformanceInfo` at `:118`), the
`MEMORYSTATUSEX`/`PERFORMANCE_INFORMATION` structs they fill, and the arithmetic over the returned
page counts. It found no defect — the struct sizes and layout match the Win32 definitions, the `BOOL`
results are checked so a failed call yields an unmonitored reading rather than a fabricated percentage,
and the page-count conversions to bytes are exact. That verdict covers that boundary only; it is not a
clean bill for the rest of PR #175, which the automated review above covers. The same two APIs are the
ones the contract surfaces describe — `crates/traits/src/ipc.rs:1117-1118` and
`crates/traits/src/contracts.rs:1258` both name `GlobalMemoryStatusEx` and `GetPerformanceInfo` — so the
boundary implements the documented reading rather than a private variant of it.

**Test-comment correction (documentation only, no behaviour change).** The test
`status_clamps_inconsistent_counter_snapshots` justified its deliberately inconsistent input by calling
the two commit counters separate syscall snapshots whose split could leave the charge above the limit.
They are not separate: `CommitTotal` and `CommitLimit` both come from the single `GetPerformanceInfo`
call in `mem.rs`, so the rationale was wrong while the assertion it guards stayed right. The comment now
says the input is normalized defensively over the `0..=100` contract and names the single-snapshot fact
(`crates/sysmon/src/mem.rs:203-205`). No assertion, code path or threshold changed. No test was added
for the wording: a comment carries no behaviour to fail without, so an assertion over source text would
prove nothing about the product. The correction reaches nothing beyond that test — a grep of `specs/`
for `relief_should_act` finds no match, so no spec sentence names that symbol and none needed
correcting.

**Unchanged behaviour re-checked: a failed load still frees the gate for a retry.** Moving the gate into
the blocking closure changes nothing about failure, and the tree shows why: `crates/embeddings/src/lib.rs:359`
takes the guard into the closure, `:360-362` load, publish and stamp, and the `?` on a failed `M::load`
unwinds the closure so the guard drops with the work instead of parking the next caller; publication
never happens, the mapped error reaches the caller, and a later call retries a fresh load. The
neighbouring gate sites are unchanged — `evict` holds the gate across teardown (`:264`) and
`take_evictable` decides and takes under the slot lock (`:283-285`). Parent-observed raw output on this
tree: `failed_load_releases_the_gate_for_a_retry` `1 passed; 0 failed; 11 filtered out` and `sysmon`
`mem::tests::host_reports_a_plausible_memory_reading` `1 passed; 0 failed; 17 filtered out`.

**Gate.** The advisory gate (`artifact://471`, log `Temp/ss-pr175-advisory-gate.log`) reported
`sysmon_module EXIT=0` with `test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered
out` and doctests `0`, then `cargo_fmt`, `cargo_clippy`, `cargo_build`, `cargo_test`, `bindings_diff`
and `diff_check`, each `EXIT=0`. The computed workspace aggregate is exactly
`{"suites":86,"passed":1652,"failed":0,"ignored":17}`. The full UI gate stands as history and was not
rerun — this pass changes no UI, Node or dependency file. The commit remains the parent's step, so no
SHA is cited for this pass.

### 2026-11-09 — Your-Universe: Localtime (standalone deliverable, not product code)

**Scope.** `Your-Universe/` holds "Localtime", a self-contained Three.js living hillside district
driven by ScreenSearch's export shape, built against the maintainer's brief in
`Your-Universe/build-prompt.md` and then iterated against the owner's written verdicts in
`Your-Universe/OWNER-NOTES.md`. The viewer watches a city that runs on its own: eight quarters
sized by app time, traffic density following session-switch rate, cranes on 45-minute focus
sessions, lit windows on the last hour of activity, and a night dream that builds one provisional
road between two quarters that share words but never shared an hour, offering at most two
"Take it" cards per waking. **No product surface is touched**: no crate, schema, IPC, UI or
contract change; the app's Rust and React trees are untouched, so `05`/`06` product gates are
not exercised by this work.

**Architecture.** `index.html` is generated, never hand-edited: `src/` holds one fragment per
top-level factory (`generateSimulatedDay`, `analyzeExport`, `buildWorld`, `createAgents`,
`createCameraRig`, `createDream`, `createPost`, `main`) plus `assets.mjs` for the model pipeline,
bound by the interfaces in `src/CONTRACT.md`; `tools/assemble.mjs` emits the single file and
validates it with `node --check`. Cycle 19 added the asset pipeline: `tools/fetch-assets.mjs` /
`pack-assets.mjs` fetch and pack Kenney CC0 kits into `assets/*.glb` listed by
`assets/manifest.json`, `tools/stamp-sw.mjs` keeps the `CACHE_VERSION` in `assets/sw.js` equal to
the manifest digest, and `?noassets=1` falls back to the fully procedural build. The deliverable is
`index.html` + `assets/`; glTF loading requires `node tools/serve.mjs` (no `file://`).

**Method.** 22 measured cycles are logged in `Your-Universe/ITERATIONS.md` (1,868 lines). The agent
has no image input, so every art judgement there is a pixel measurement (palette shares, 3x3 region
luminance/saturation/edge density, ten horizontal bands of edge density, speck counts) plus a
verbatim owner verdict: the log is explicitly "a measurement, not a look". Per-cycle gates: desktop
1600x900 and Pixel 6 412x915 (DPR 2.625, touch, 4x CPU throttling) draw-call and triangle lines,
bench fps floors, zero console errors, no overlapping or off-viewport labels.

**Verification re-run on the tree this record describes** (raw output):

```text
assemble: 23/23 verifications passed          (index.html byte-identical after reassemble)
check: 38 checks, 38 passed, 0 failed         (36 before test-serve.mjs and cache-version.mjs joined)
total: 100 checks, all PASS                   (check-dom)
OK: the server answers a malformed escape with 400 and keeps serving   (test-serve)
120 passed, 0 failed                          (smoke-data)
OK: all six properties hold                   (check-sample)
stamp-sw: CACHE_VERSION ma3977eae matches the precached payload (manifest, packs, sample)
CHECK RESULT: PASS (87 models)                (check-assets)
```

`index.html` is 1,027,378 bytes on disk (1,026,873 as the LF-normalized blob git stores) and
reproduces byte-identically from `src/`; `assets/` is
3,093,739 bytes of Kenney CC0 packs with their licence texts under `assets/LICENSES/`. Final
measured state, assembled build (desktop 1600x900; hour 17 day, hour 3 night): day 101 calls /
388,900 tris, night 99 / 346,840, dream 91 / 297,176, noassets 55 / 336,517, street 101 / 393,464,
phone day 54 / 170,498, phone night 44 / 170,191, phone dream 36 / 159,211. Speck counts (isolated
dark pixels, percent of frame): day 152 (0.0106), night 60, dream 29, noassets 156, phone day 1,
phone night 0, phone dream 0.

**Open items.** The fps bench gate did not pass in cycle 21, and the draw-call line is exceeded by the
shipped build. Re-measured on the committed snapshot (`4c7fbea`, loop's fleet integration call on)
over HTTP at 1600x900, hour 17, awake, `renderer.info` reports 101 calls with the asset fleet, 95
with `?post=0` and 55 with `?noassets=1`, against a header line that still says under 80; triangles
hold at 388,884 of 400,000. Both items, with numbers, attribution and the remaining questions, are
recorded in `07` rather than claimed as met.

**Snapshot note.** The owner loop was still cycling when this record was written (it made its fleet
integration call during the session, `MODEL_LANE_EVERY` 9999 to 24), so the numbers above describe
commit `4c7fbea` and later cycles will arrive as follow-up commits rather than as silent rewrites of
this entry.

**Also on this branch.** The 0.4.5 build-loop rotation and the carried-forward backlog re-verification
described in `08` (2026-11-09) run alongside this record: shipped 0.4.5 entries moved into
`specs/archive/*.v0.4.5.md`, the live logs restructured, and every older gap re-checked against the
v0.4.5 tree (`9c45ae8`). The cleanup instruction that drove them is archived at
`specs/archive/specs-cleanup-prompt.md`, and the `04 §1` arc pointer now points at the open work
instead of the finished 0.4.5 phases.

**Review pass, PR #173 (2026-11-09).** Six findings arrived on the PR: five CodeQL alerts against
the two gate scripts and one Codex P2 against the server. Each was checked against the code and
reproduced before anything was edited; nothing was changed to satisfy a claim that did not hold.

| # | finding | verdict | resolution |
|---|---|---|---|
| 1 | `tools/serve.mjs:23` — a malformed escape terminates the server (P2) | valid, reproduced | decode moved inside the error boundary, 400 returned; `tools/test-serve.mjs` added as the regression gate |
| 2 | `tools/check-dom.mjs:168` — regex does not match upper case `<SCRIPT>` | valid, and wider than reported | `i` flag on the script, link and import-map matches; the same blind spot covered `<LINK>` and the import-map parse |
| 3 | `tools/check.mjs:50,54,55` — unescaped `.` before the host names | inaccurate: line 126 escaped every dot at construction | pattern rebuilt as plain prefix comparison, so no host string is re-parsed as a regular expression |
| 4 | `tools/check.mjs:127` — the escaping does not escape backslashes | inaccurate: the inputs are a literal host list with no backslashes | removed together with #3 |

Finding 1, fail-without on the committed server and pass-with on the fixed one:

```text
$ node tools/test-serve.mjs          # unfixed
FAIL  GET /%ZZ answers 400 instead of dropping the connection (got 0, ECONNRESET)
FAIL  the server survives the malformed request (exit code 1)
FAIL  GET /index.html still answers 200 afterwards (got 0)
URIError: URI malformed
    at decodeURIComponent (<anonymous>)
    at Server.<anonymous> (file:///.../tools/serve.mjs:23:15)
FAILED: 3 check(s) failed                                     EXIT=1

$ node tools/test-serve.mjs          # fixed
PASS  GET /%ZZ answers 400 instead of dropping the connection (got 400)
PASS  the server survives the malformed request (exit code null)
PASS  GET /index.html still answers 200 afterwards (got 200)
OK: the server answers a malformed escape with 400 and keeps serving   EXIT=0
```

Finding 2, the same hand-edited `src/shell.html` carrying `<SCRIPT SRC="evil.js">` through both
versions of the gate. The committed gate is green with an external script in the shell:

```text
$ node <committed check-dom>          PASS  shell.html has exactly one script tag and it is the import map
                                      PASS  shell.html has no external script element
                                      total: 100 checks, all PASS            exit 0
$ node tools/check-dom.mjs            FAIL  shell.html has exactly one script tag and it is the import map
                                      FAIL  shell.html has no external script element
                                      total: 100 checks, 2 FAIL              exit 1
```

Finding 3, old regular expression against the new prefix comparison over 23 URLs (allowed hosts with
and without a trailing slash, template-expression forms, and adversarial `quaterniusXcom`,
`quaternius.com.evil.example`, `drive.google.comX` and host-in-query forms): `EQUIVALENT: 23 samples,
0 mismatches`, 13 allowed by both, 10 refused by both. The escaping the alert claimed was missing is
exactly what made the suffix attacks fall on the refused side before and after.

Fixes are in `328f83a`'s successors on `feat/your-universe-localtime`; `index.html` is unaffected,
since none of the three tools is part of the generated bundle. The regression gate is deliberately
separate from `check.mjs`, which spawns nothing and stays a static check over `src/` and the bundle.
The CodeQL analysis of the fix commit reports `results=0` for javascript-typescript, against 5 for
the commit before it. The alerts are not dismissed and their `state` field is still null once that
analysis has run, and `?state=open` lists none of them, so the evidence that they are resolved is the
analysis result; the security page settles the alert state on the default-branch scan after merge.

**Review pass, round two (PR #173, 2026-11-09).** Three more findings landed on the head that carried
the first fix, all inside the deliverable. One is fixed here; the other two are valid and recorded in
`07` as unshipped, because both call for edits to `src/` while the owner loop is rewriting those files
and `index.html` regenerates from all of `src/`.

| # | finding | verdict | resolution |
|---|---|---|---|
| 5 | `tools/stamp-sw.mjs:27` — the offline-cache fingerprint ignores pack bytes | valid, reproduced | fingerprint extracted to `tools/cache-version.mjs`, hashing the manifest, every pack it names *and* the opening sample, which is the one other cache-first payload outside `assets/`; `stamp-sw` and `check-dom` share it |
| 6 | `src/agents.mjs:4970` — the green wave stays on after its session ends | valid, measured | 472 of 892 sampled minutes wrong; the fix is `activeSessionAt(t)`; deferred to `07` |
| 7 | `src/main.mjs:1261` — dream cards survive an export reload | valid, traced | `hideCards()` is not exposed by `dream.mjs`; the fix is to expose and call it; deferred to `07` |

Finding 5, one flipped byte inside `assets/kenney-car-kit.glb` with the manifest untouched:

```text
$ node <committed stamp-sw> --check
stamp-sw: CACHE_VERSION m730a18fd matches assets/manifest.json          exit 0    (blind)
$ node tools/stamp-sw.mjs --check
stamp-sw: CACHE_VERSION is mbe1267a6 but the payload fingerprints to mb477225d
stamp-sw: run node tools/stamp-sw.mjs before serving or packing         exit 1    (seen)
$ node tools/check-dom.mjs
FAIL  assets/sw.js cache version matches the payload fingerprint (mbe1267a6 vs mb477225d)
total: 100 checks, 1 FAIL                                              exit 1
```

Restoring the pack byte-exactly (`8420fb3b74af7684`) returns both gates to pass. `CACHE_VERSION` moves
from `m730a18fd` to `mbe1267a6` because the fingerprint now covers the six pack GLBs as well as the
manifest that names them, and the version therefore moves when only a texture, material, normal or
interior vertex does.

The same hole sat one route further out. `assets/sw.js` precaches `./sample-export.json` and routes it
cache-first, so a regenerated sample with no pack change keeps the old version and opens a returning
visitor on the previous day; the `cache: 'no-store'` on `main.mjs`'s own fetch cannot prevent that,
because the worker answers before the network, and `tools/gen-sample.mjs` regenerates exactly that
file. The fingerprint hashes it too, measured on one flipped byte inside it:

```text
$ node tools/stamp-sw.mjs --check     # fingerprint without the sample
stamp-sw: CACHE_VERSION mbe1267a6 matches the manifest and pack payload         exit 0    (blind)
$ node tools/stamp-sw.mjs --check     # fingerprint with it
stamp-sw: CACHE_VERSION is mbe1267a6 but the payload fingerprints to mc209a38d
stamp-sw: run node tools/stamp-sw.mjs before serving or packing                exit 1    (seen)
```

With the payload restored the stamp reads `ma3977eae`, and the sample's own digest is
`729e75a3da016d00`.

Finding 6, `longestSessionBefore(t)` measured against `activeSessionAt(t)` over the shipped sample
(128 sessions, 892 sampled minutes):

```text
minutes sampled: 892 | minutes where the wave is wrong: 472 (52.9% of the day)
examples (wave on, nothing open):
  at 07:02 longest q=5 ended 07:01 active= false
  at 07:07 longest q=5 ended 07:06 active= false
```

Finding 7 is structural: `dream.mjs` keeps the card nodes and their handlers in an internal `cards`
array, and its `return` exposes `group`, `update`, `sleep`, `wake`, `toggle`, `setDataModel`,
`notifyGap`, `warm` and two measurement getters, with no card teardown among them, while `install()`
in `main.mjs` disposes three groups and drops the instance. The old cards therefore outlive the dream
that owns their handlers.

Why these wait: `index.html` is generated from every `src/` fragment, and the owner loop is mid-cycle in
those files, measuring four `src/` files uncommitted (+502 lines) and writing into the tree every 25 to
50 seconds when this pass checked. A `src/` fix
could only be committed together with that unfinished work, so they land with the loop's snapshot
commit and stay listed as open items until then.

**Review pass, round three (PR #173, 2026-11-09).** One finding, on the head that carried the sample
fix.

| # | finding | verdict | resolution |
|---|---|---|---|
| 8 | `src/main.mjs:1742` — the startup sample replaces an export imported during the asset wait | valid, proved from the call order | deferred to `07`; the guard is a flag set by `loadExport` and read once in the boot callback |

Finding 8 as the committed snapshot (`7d5efdc`) orders it:

```text
1891  wireInput();
1892  boot();            -> Promise.all([fetchSample(), loadAssets()]).then(...)  up to 8000 ms
1855    install(next);                        <- loadExport(), from a drop inside that window
1742      install(next);                      <- the sample, unconditional
```

`install()` (line 1255) opens by disposing the world, agent and dream groups and nulling all three, so
the later call does not overwrite a property: it discards the district the user just imported. Nothing
in between records that a user model exists, which is what makes the ordering alone sufficient.

**Review pass, round four (PR #173, 2026-11-09).** One finding, on the head that carried the record.

| # | finding | verdict | resolution |
|---|---|---|---|
| 9 | `src/main.mjs:721` — the replaced world's shadow render target is never disposed | valid, proved | deferred to `07`; the disposal needs a light branch in `disposeGroup`, and `setShadowMapSize(0)` already releases the target |

Finding 9 holds end to end: `world.mjs` adds the sun to the group that `install()` disposes (`group.add(sun)`, line 10321 at this commit),
`disposeGroup` traverses that group, and line 721 returns for every object without a material, which a
`DirectionalLight` is, so `sun.shadow.map` survives the disposal of the only thing that referenced it.
The map is the largest target the app allocates, so repeated imports in one session accumulate one per
import, and the GPU allocation is released only when the page unloads. `setShadowMapSize(0)`
(line 10353 at this commit) already disposes the map and nulls it, which is why the fix needs no new mechanism.



### 2026-11-10: Off-arc, 0.4.6 item 1, launch panic (#150)

**Scope.** `06` #150: the shipped 0.4.5 build panics on launch most attempts before the window is
usable (`state() called before manage() for screensearch_lib::update::UpdaterState`, process exit
`3221226505`). Only the row's first fix shape is implemented: register `UpdaterState` on the
`tauri::Builder` so it exists before any window or WebView, with exactly one registration site. The
row's second shape (calling `try_state` so the command tolerates an absent state) was not taken;
`get_update_status` already takes an `AppHandle`, so that shape was a lookup change inside
`current_status`, not a command-signature change. No retry loop was added, and no unrelated refactor
rides along.

**Why the ordering is the whole bug.** `tauri-2.11.3/src/app.rs:2521-2532` (`fn setup`) creates
every config-declared window first (`WebviewWindowBuilder::from_config(...).build()` in a loop over
`config().app.windows`) and only then calls the user setup closure, which is where
`app.manage(update::UpdaterState::default())` lived (`src-tauri/src/lib.rs:1438` at base). Both
`main` and `overlay` therefore exist, and their WebViews load, while the closure is still doing
blocking launch work. The updater path then reaches `Manager::state`: `get_update_status` takes an
`AppHandle` (`src-tauri/src/update.rs:199`) and its `current_status` helper calls
`app.state::<UpdaterState>()` (`update.rs:72`), which panics on an unmanaged type. A command that
declares `State<'_, T>` instead returns an invoke error when the type is missing, because
`CommandArg for State` uses `state_ref().try_get()` (`tauri-2.11.3/src/state.rs:60-68`).

**What changed.**
- `src-tauri/src/lib.rs`: new `pub fn manage_pre_window_state<R: tauri::Runtime>(builder) ->
  Builder<R>` that calls `builder.manage(update::UpdaterState::default())`. `run()` starts from
  `manage_pre_window_state(tauri::Builder::default())`, so the value is inserted at builder
  construction (`Builder::manage` stores it immediately, `tauri-2.11.3/src/app.rs:1943-1954`),
  before `build()` and before any window. The setup-closure
  `app.manage(update::UpdaterState::default())` is gone; one `UpdaterState::default` occurrence
  remains in the crate. The release-only launch-check `spawn` stays where it was, with its comment
  now naming the builder registration as the state's owner.
- `src-tauri/src/lib.rs`: `#[doc(hidden)] pub use update::UpdaterState;` so the regression test can
  perform the same `app.state::<UpdaterState>()` lookup `current_status` performs (`update.rs:72`).
  Both `pub` items carry doc comments stating that they exist as the test seam and the single
  registration site, not as API.
- `src-tauri/tests/launch_order.rs`: the regression test. It calls the same
  `manage_pre_window_state` that `run()` applies, builds a `tauri::test` mock app from
  `mock_context(noop_assets())` (which declares no window), asserts `webview_windows().is_empty()`,
  and then calls `app.state::<UpdaterState>()`, the same lookup that panicked in production.
  It lives in `tests/` rather than the lib unit tests because the test-target resource link args
  apply to integration test targets only (see the manifest note below).
- `src-tauri/Cargo.toml`: `[dev-dependencies] tauri = { version = "2", features = ["test"] }`.
  Dev-only; the workspace resolver is "2", so the feature cannot reach a shipped build.
- `src-tauri/build.rs`: `*-tests` link args only, so the mock-runtime test executable can load.

**Failing first.** The shipped ordering was restored by hand (registration back inside the setup
closure, the new seam left as a pass-through), then:

```text
$ cargo test -p screensearch --test launch_order
   Compiling screensearch v0.4.5 (C:\Users\nicol\Documents\GitHub\screensearch\src-tauri)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 51.95s
     Running tests\launch_order.rs (target\debug\deps\launch_order-5e2ef30499b21bd5.exe)

running 1 test
test updater_state_is_managed_before_any_webview_exists ... FAILED

failures:

---- updater_state_is_managed_before_any_webview_exists stdout ----

thread 'updater_state_is_managed_before_any_webview_exists' (31808) panicked at C:\Users\nicol\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\tauri-2.11.3\src\lib.rs:734:7:
state() called before manage() for screensearch_lib::update::UpdaterState
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    updater_state_is_managed_before_any_webview_exists

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

error: test failed, to rerun pass `-p screensearch --test launch_order`
```

That is the production failure verbatim: the same message, the same `tauri-2.11.3/src/lib.rs:734`
panic site, reproduced by the test without launching the app.

**After the fix** (builder registration restored, single site):

```text
$ cargo test -p screensearch --test launch_order
   Compiling screensearch v0.4.5 (C:\Users\nicol\Documents\GitHub\screensearch\src-tauri)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 54.17s
     Running tests\launch_order.rs (target\debug\deps\launch_order-5e2ef30499b21bd5.exe)

running 1 test
test updater_state_is_managed_before_any_webview_exists ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

Raw logs: `C:/Users/nicol/screensearch-evidence/0.4.6/item1-failing-first.log`,
`item1-passing.log`, `item1-gate.log`.

**Gate.** Run from the repository root with the item command
(`GATE_LOG=C:/Users/nicol/screensearch-evidence/0.4.6/item1-gate.log source
C:/Users/nicol/screensearch-evidence/0.4.6/gate.sh`). It stopped at `cargo_test`:

```text
node v26.12.1
 / cargo 1.95.0 (f2d3ce0bd 2026-03-21)
ui_ci EXIT=0
ui_lint EXIT=0
ui_test EXIT=0
ui_build EXIT=0
stage_mcp EXIT=0
cargo_fmt EXIT=0
cargo_clippy EXIT=0
cargo_build EXIT=0
cargo_test EXIT=101
STOPPED_AT=cargo_test
CARGO_TEST_AGGREGATE={"suites":3,"passed":88,"failed":2,"ignored":1}
UI_NODE_TEST_SUMMARY=["ℹ tests 172","ℹ suites 0","ℹ pass 172","ℹ fail 0","ℹ cancelled 0","ℹ skipped 0","ℹ todo 0"]
```

The two failures are in `crates/api/tests/http_api.rs`, both `tempfile::tempdir()` creations on
transient `%TEMP%` paths, and they reproduce when that target runs alone:

```text
$ cargo test -p api --test http_api
---- export_to_file_writes_valid_json_without_a_server stdout ----

thread 'export_to_file_writes_valid_json_without_a_server' (22336) panicked at crates\api\tests\http_api.rs:814:35:
called `Result::unwrap()` on an `Err` value: Custom { kind: PermissionDenied, error: PathError { path: "C:\\Users\\nicol\\AppData\\Local\\Temp\\.tmpajEuSN", err: Os { code: 5, kind: PermissionDenied, message: "Access is denied." } } }

---- reads_never_write_insights stdout ----

thread 'reads_never_write_insights' (17720) panicked at crates\api\tests\http_api.rs:1996:35:
called `Result::unwrap()` on an `Err` value: Custom { kind: PermissionDenied, error: PathError { path: "C:\\Users\\nicol\\AppData\\Local\\Temp\\.tmpWfISsm", err: Os { code: 5, kind: PermissionDenied, message: "Access is denied." } } }

test result: FAILED. 56 passed; 2 failed; 1 ignored; 0 measured; 0 filtered out; finished in 30.09s
```

The lead's host investigation the same day established that this is **not** transient and not caused
by this change: it is location-dependent on this host. The pristine `origin/main` copy of the same
test binary passes from `C:\Users\nicol\screensearch-evidence\...` (58 passed, 0 failed); an identical
binary (same name and size, and the copies were swapped both ways) fails when executed from
`Documents\GitHub\screensearch\target\debug\deps` and passes once moved out, and the reverse copy
fails in the repository target. Pointing `%TEMP%` at a fresh writable directory did not change the
outcome, so the failure follows the executable's location, not the temp path. Defender's Controlled
Folder Access and every ASR rule are off, `C:\Users\nicol\Documents` is not a reparse point, and only
Windows Defender is registered. The cause was not identified further; the gate below was therefore
run with `CARGO_TARGET_DIR` outside `Documents`, and the finding is recorded in `07` as a host note.
Neither the tests nor any timeout was edited.

GATE (lead). Final run, native PowerShell runner with the checksum-verified portable Node 22 on
`PATH` and as the invoked interpreter, and `CARGO_TARGET_DIR` outside `Documents`:

```text
$ powershell -NoProfile -File .../gate.ps1 -Log .../item1-native-node22-gate.log
node v22.23.4 / cargo 1.95.0 (f2d3ce0bd 2026-03-21)
node_environment EXIT=0
ui_ci EXIT=0
ui_lint EXIT=0
ui_build EXIT=0
ui_test EXIT=0
stage_mcp EXIT=0
cargo_fmt EXIT=0
cargo_clippy EXIT=0
cargo_build EXIT=0
cargo_test EXIT=0
GATE_STEPS_ALL_ZERO=1
CARGO_TEST_AGGREGATE={"suites":87,"passed":1653,"failed":0,"ignored":17}
```

`node_environment` is a probe asserting both the invoked interpreter and a child `node` it spawns
report `v22.23.4` (`EXPLICIT_NODE=v22.23.4`, `CHILD_NODE=v22.23.4`); it would exit non-zero
otherwise. UI node tests in the same log: `# tests 172`, `# pass 172`, `# fail 0`. An earlier run of
the same chain passed identically but reported `node v26.12.1` (the host default), so it is not
cited as Node 22 evidence. Full logs: `item1-native-node22-gate.log` and, for the earlier run,
`item1-gate-final.log`.

Binding guard, run after the workspace test step regenerates bindings: `git diff --exit-code -- ui/src/bindings`
reported `BINDINGS_DIFF_EXIT=0` with an empty `git status --short ui/src/bindings`.

**Consistency audit (report only, not fixed).** Every mount-time command of both config-declared
windows declares `State<'_, AppState>`, which is managed late inside the same setup closure
(`src-tauri/src/lib.rs:1668`, after `create_dir_all`, `init_tracing`, the advanced-config file
load, `open_store`, the blocking settings load, hotkey registration and the autostart reconcile).
They share the late-registration ordering with `UpdaterState`, but not its failure mode: a missing
`State<'_, T>` makes `CommandArg for State` return an invoke error
(`tauri-2.11.3/src/state.rs:60-68`), so those commands fail temporarily rather than panicking. The
panic belongs to explicit `Manager::state` lookups, which is what `current_status` performs. Only
three state types appear in command signatures: `AppState` (76 parameters), `OverlayState` (2), and
a doc mention of `T`. The table is inventory of what can be reached on mount and when it is
registered, not evidence of a panic defect.

| window | mount-time command | handler | state | managed at |
|---|---|---|---|---|
| main | `get_readiness` (`StatusRail.tsx:45`, `ReadinessBanner.tsx:28`, `Deck.tsx:64`) | `src-tauri/src/lib.rs:269` | `AppState` | `lib.rs:1668` |
| main | `get_job_stats` (`StatusRail.tsx:46`, `Deck.tsx:65`) | `lib.rs:291` | `AppState` | `lib.rs:1668` |
| main | `get_storage_stats` (`StatusRail.tsx:47`) | `lib.rs:318` | `AppState` | `lib.rs:1668` |
| main | `get_throttle_status` (`StatusRail.tsx:49`) | `lib.rs:303` | `AppState` | `lib.rs:1668` |
| main | `get_setup_status` (`StatusRail.tsx:81` via `SetupIndicator.tsx:22`) | `lib.rs:281` | `AppState` | `lib.rs:1668` |
| main | `get_update_status` (`NavRail.tsx:150` via `UpdateIndicator.tsx:32`) | `src-tauri/src/update.rs:199` | `UpdaterState` | fixed: builder (`lib.rs:1396`) |
| main | `get_settings` (`app/theme.ts:37` via `AppShell.tsx:26`) | `lib.rs:1194` | `AppState` | `lib.rs:1668` |
| main | `get_frames`, `get_insights`, `get_timeline` (`Deck.tsx:71-73`) | `lib.rs:1114`, `:1176`, `:1094` | `AppState` | `lib.rs:1668` |
| main | `list_marks` (`IntentionsStrip.tsx:25`, rendered by `Deck.tsx:267`) | `lib.rs:542` | `AppState` | `lib.rs:1668` |
| overlay | `get_settings` (`FlowOverlay.tsx:47`) | `lib.rs:1194` | `AppState` | `lib.rs:1668` |
| overlay | `get_advanced_config` (`FlowOverlay.tsx:53`) | `lib.rs:1209` | `AppState` | `lib.rs:1668` |
| overlay | `where_was_i` (`FlowOverlay.tsx:90`, enabled for the empty-query strip) | `lib.rs:505` | `AppState` | `lib.rs:1668` |
| overlay | `search` (`FlowOverlay.tsx:82`, once the advanced config resolves) | `lib.rs:443` | `AppState` | `lib.rs:1668` |

Other state registered inside the same closure: `OverlayState` at `lib.rs:1461`, after
`open_store`. `overlay_shown_ack` (`src-tauri/src/overlay.rs:114`, called from
`ui/src/overlay/FlowOverlay.tsx:127` on the `overlay_shown` event) and `get_hotkey_status`
(`overlay.rs:104`, Settings page) declare `State<'_, OverlayState>`, so an early call returns an
invoke error, not a panic. Explicit `app.state::<OverlayState>()` lookups do panic by construction;
the call sites are the hotkey registration and summon helpers (`overlay.rs:164,173,182,504,597`),
not a WebView mount command. `get_sidecar_status` and the NavRail version are not IPC commands
against managed state: the former is an event-sourced cache
(`queries.ts:96-105`), the latter uses Tauri's `getVersion` (`ui/src/lib/useAppVersion.ts`). All of
these are recorded here for the maintainer; only `UpdaterState` was fixed in this item.

**Test-target manifest note (why the test is an integration test).** `tauri::test`'s mock
`Builder` construction reaches the tray/menu code, whose static imports include
`TaskDialogIndirect`, a Common Controls v6-only export. A cargo test executable gets no v6
application manifest, so it aborted at load before any test ran:

```text
$ cargo test -p screensearch --lib updater_state_is_registered_on_the_builder_before_any_window
     Running unittests src\lib.rs (target\debug\deps\screensearch_lib-3b6b8f19e6c357b4.exe)
error: test failed, to rerun pass `-p screensearch --lib`

Caused by:
  process didn't exit successfully: `...\screensearch_lib-3b6b8f19e6c357b4.exe ...` (exit code: 0xc0000139, STATUS_ENTRYPOINT_NOT_FOUND)
```

`cargo:rustc-link-arg-tests` is not applied to the lib unit-test target (verified: the rustc link
line for that target carried no `/MANIFEST` argument), which is why the test moved to
`src-tauri/tests/launch_order.rs`; integration test targets do receive those args. Two mechanisms
were tried for the manifest: `/MANIFEST:EMBED` plus `/MANIFESTINPUT:<absolute path>` with a
checked-in `test-common-controls.manifest` linked and ran correctly (used for the failing-first and
passing runs above), while adding embedded double quotes around the path broke `mt.exe`
(`c1010070` "Failed to load and parse the manifest" with a mangled `\\?\C:\C:\...manifest"` path
and `LNK1327`). The tree handed to the lead carries the path-free variant instead:
`/MANIFEST:EMBED` plus an inline `/MANIFESTDEPENDENCY:` naming Microsoft.Windows.Common-Controls
6.0.0.0, which removes the path-quoting question entirely; the lead observed
`cargo test -p screensearch --test launch_order` pass with that variant (1 passed, 0 failed,
finished 0.00s). Production binaries are untouched either way: the args are `*-tests` only and
`tauri-build` still owns the shipped resource.

**Live proof completed.** Twenty consecutive isolated cold dev launches passed, with no panic and
no early exit. Each run required a newly appended `capture started` log line, then ten seconds of
survival, with stdout, stderr and the newly appended application log checked for panic messages.
Only the launched process tree was stopped between runs. The external config changed both
`identifier` and `productName`; the isolated `advanced.toml` disabled text embeddings and idle vision.
The corrected profile instructions now live in `docs/DEV_STATE_OVERRIDE.md`.

The pre-fix dev control also passed eight consecutive launches. Therefore the dev-launch series
does not by itself distinguish old from fixed code; the regression's exact failing-first panic
does. No installed-release launch test or updater download/install is claimed.

#### Item 1: raw consecutive cold-launch log

```text
[2026-11-10T11:14:08.668Z] run 1/20 OK 117.4s reason=capture started
[2026-11-10T11:15:01.809Z] run 2/20 OK 49.4s reason=capture started
[2026-11-10T11:15:22.215Z] run 3/20 OK 16.1s reason=capture started
[2026-11-10T11:15:42.124Z] run 4/20 OK 16.2s reason=capture started
[2026-11-10T11:16:06.007Z] run 5/20 OK 18.4s reason=capture started
[2026-11-10T11:16:29.244Z] run 6/20 OK 19.8s reason=capture started
[2026-11-10T11:17:25.563Z] run 7/20 OK 52.4s reason=capture started
[2026-11-10T11:17:52.211Z] run 8/20 OK 21.9s reason=capture started
[2026-11-10T11:18:13.515Z] run 9/20 OK 17.2s reason=capture started
[2026-11-10T11:18:33.863Z] run 10/20 OK 16.7s reason=capture started
[2026-11-10T11:19:58.553Z] run 11/20 OK 80.7s reason=capture started
[2026-11-10T11:20:19.226Z] run 12/20 OK 16.7s reason=capture started
[2026-11-10T11:20:39.551Z] run 13/20 OK 16.6s reason=capture started
[2026-11-10T11:20:59.389Z] run 14/20 OK 16.2s reason=capture started
[2026-11-10T11:21:19.607Z] run 15/20 OK 16.7s reason=capture started
[2026-11-10T11:21:39.194Z] run 16/20 OK 16.1s reason=capture started
[2026-11-10T11:21:58.978Z] run 17/20 OK 16.1s reason=capture started
[2026-11-10T11:22:18.618Z] run 18/20 OK 16.1s reason=capture started
[2026-11-10T11:22:38.291Z] run 19/20 OK 16.1s reason=capture started
[2026-11-10T11:22:57.816Z] run 20/20 OK 16.1s reason=capture started
{"label":"fixed-dev-20","attempted":20,"successes":20,"panics":0,"earlyExits":0,"firstFailure":-1}
```

### 2026-11-10 - 0.4.6 stabilisation, item 2: `06` #116 Ask turn with no first delta (blocked, no code change)

**Why this item stopped.** The row asks for a way to make an Ask turn terminal when its provider
never produces a delta. The specs define the turn record and its failure semantics but no time
bound, and the only number-free alternative (fail before the turn when the answer weights are
absent) contradicts documented first-use download behavior. That is a product decision, so the item
records the blocker and stops rather than inventing a threshold (`04 §5`).

**Scope and evidence discipline.** Verification here was read-only source and spec reading. No app
was launched, no model was loaded or downloaded, no profile or database was touched, and no runtime
reproduction of the stall is claimed (the last live observation is the one already recorded in
`05`). No test or gate was run, because no code changed. No git command was run.

**What is confirmed at this base (code read, line numbers checked).**

- The provider slot is filled unconditionally at startup, and the accessor returns `Some` whenever
  it is filled: `crates/kernel/src/lib.rs:1351` (attach), `:1364-1366` (`answer_provider`).
  `ask_turn` rejects only `None` with `NoAnswerModel`
  (`crates/kernel/src/ask.rs:136-139`), before any thread or turn exists.
- The turn is persisted with a NULL answer before generation (`crates/kernel/src/ask.rs:202-215`),
  and `forward_turn` is spawned before the stream handle is returned (`ask.rs:383-400`).
- `forward_turn`'s whole bounded wait is a two-arm `select!`: consumer closed, or `prx.recv()`
  (`crates/kernel/src/ask.rs:466-476`). There is no elapsed-time arm. The only `timeout` in the
  file is the 250 ms cancel grace (`ask.rs:529`, constant at `:636`), which cannot rescue a provider
  that never yields a delta.
- With no installed answer weights, `AnswerSidecar::ensure_spec` calls `downloader.ensure`
  (`crates/inference/src/answer.rs:94-113`), which downloads (`crates/inference/src/download.rs:773-782`)
  when files are absent. Total download time is unbounded; the stall watchdog only bounds a period
  with no new bytes (180 s, `download.rs:42`), and the transfer carries its own 15 s connect/API and
  30 s read timeouts.
- The client's 30 s stream connect and 30 s per-chunk idle timeouts
  (`crates/inference/src/client.rs:32-33`, `:305-326`) apply only after the streaming request
  starts, so they cannot bound model acquisition or supervisor wait.
- The UI phase moves only on `done` or `error`, so a channel that closes with no delta leaves the
  reducer at `"streaming"` (`ui/src/lib/ipc/useAskConversation.ts:111-119`, state at `:18-19`).
- Latent, currently unreachable path: if a provider returned `Ok` after zero deltas, `forward_turn`
  would fall through to synthesizing `Citations` + `Done` and persist an empty string as the answer
  (`crates/kernel/src/ask.rs:543-616`). Today's only production provider always emits a terminal
  delta first, so this path is not observed; a first-delta implementation must not treat "no delta"
  as success.

**Decisive spec sentences (verbatim, trimmed where marked).**

`03 §7i.4` (`specs/03_MASTER_PRODUCTION_SPEC.md:1674-1677`):

> `ask_threads` / `ask_turns` persist conversations (`§4`). Each turn stores its question, its
> answer (NULL while streaming and on failure), the resolved `[window_start, window_end)` and
> `resolved_entities` as a JSON array of entity ids.

`03 §7c` connection bounds (`specs/03_MASTER_PRODUCTION_SPEC.md:834-837`):

> This is not a response/body deadline: long Ask SSE and export responses must remain usable. These
> are fixed implementation constants, not new settings.

`03 §8` (`specs/03_MASTER_PRODUCTION_SPEC.md:1751`, `:1752-1754`):

> Thresholds are settings, never hardcoded

> The Settings UI is a **closed set of 26 keys**, exactly the rows whose Disposition is `UI` below,
> and nothing else may be added to it; that ceiling is binding

`docs/API.md` (`:421-427`, `:485-497`):

> `{"type":"error","message":"…"}` may replace the terminal `done`. A keep-alive comment is sent
> every 15 s. **Disconnecting the client cancels generation** ... Returns **503** if no answer model
> is loaded.

> Each turn is persisted before generation starts and answers are stored when generation completes

> answer | The final answer text; **`null` while the turn is streaming and when it fails**.

`specs/UI_REFERENCE.md:229`:

> sidecar unavailable → "answer model not loaded; load it?"; a failed turn keeps the question in the
> thread with a retry, never a lost turn

`docs/ARCHITECTURE.md:384-388`, reason the number-free alternative is a product change:

> everything downloads on first use

**Verdict.** Not a `06` contradiction. The API (terminal `error`, 503 with no model), `§7i.4`
(NULL on failure) and `UI_REFERENCE` (failed turn keeps the question with a retry) agree with each
other and with the shipped terminal-error paths. What is missing is a bound, so the question is a
`07` gap:

1. May Ask start the first-use answer-model download, or must it fail before the turn until the
   model is explicitly loaded?
2. If a first-delta deadline is required, what value?
3. Does the clock start at turn creation (covering a legitimate multi-GB download) or at model
   readiness?
4. Where does the threshold live, given `03 §8`'s "settings, never hardcoded" rule and the closed
   26-key Settings ceiling (`file-only` in `advanced.toml` is the likely home, but that is the
   maintainer's call)?

The question is appended to `specs/07_KNOWN_GAPS.md` under `06` #116, which is explicitly
disambiguated from the session-model `#116` there. If the answer is "no download during Ask" then
the change is a pre-turn failure reusing `NoAnswerModel`/503 and the existing UI block; if it is a
deadline, the change is a third `select!` arm in `forward_turn` that emits the existing `error`
delta, plus a settings row or a documented constant. Either shape also closes the zero-delta
success path above.

**Records updated by this item.** `specs/06_PATCH_PLAN.md` row 116 status only (blocked, confirmed
path, question location), and the `06` #116 record with the question in `specs/07_KNOWN_GAPS.md`.
No code file changed.

**Limitations.** No runtime reproduction was attempted, so "the stall still reproduces live" is not
claimed; the row's existing 2026-11-09 note that the live stall was not re-observed still stands.
Line numbers are as of this checkout. No build, lint, test or gate was run, and none would be
meaningful for a records-only change.

### 2026-11-10: Off-arc, 0.4.6 item 5, `get_insights` removal assessed (`06` #143)

**Scope.** The queue asked for the `get_insights` removal only if nothing imports it, and forbade the
removal when importers exist. No code changed.

**Verified at this base** (`grep` over `ui/src`, `src-tauri/src`, `crates`):

```text
ui/src/lib/ipc/commands.ts:211  export const getInsights = (
ui/src/lib/ipc/commands.ts:215    invoke<InsightsSummary>("get_insights", { range, bucketCount });
ui/src/lib/ipc/queries.ts:303   export function useInsights(
ui/src/lib/ipc/queries.ts:311     queryFn: () => cmd.getInsights(range, bucketCount),
ui/src/routes/Deck.tsx:29         useInsights,
ui/src/routes/Deck.tsx:72         const insights = useInsights(today, TODAY_BUCKETS);
```

So the command has exactly one live consumer chain and it ends in a mounted route. The
`insights_summary` aggregate, the `InsightsSummary` contract and its two store tests keep their
consumer. Removing them would break the Deck's enrichment line, which is production UI.

**Decision.** Recorded, not removed. `specs/06_PATCH_PLAN.md` row 143 now carries this evidence and
stays open for whoever owns the Tauri command, the contract, the store aggregate and the two store
tests together. No test or gate applies: no source, test, lint or dependency file changed.

### 2026-11-10: Off-arc, 0.4.6 item 6, `Expander` and the v1 icon set (`06` #137, #138)

**Scope.** The queue allowed both deletions only when nothing imports them. No code changed.

**Verified at this base.**

```text
ui/src/components/primitives/index.ts:13  export { Expander, type ExpanderProps, ... } from "./Expander";
ui/src/routes/Ask.tsx:46,593-707           Expander (scope panel)
ui/src/routes/Kitchen.tsx:20,306-322       Expander (dev gallery)
ui/src/routes/KitchenPrimitives.tsx:24,152-179,232-239  Expander (gallery specimens)
ui/src/routes/Kitchen.tsx:39               import { KitchenGallery } from "./KitchenPrimitives";
ui/src/app/router.tsx:84-86               the only dev route is /kitchen
```

`ui/src/components/icons` is imported by 27 files: 15 primitives, `app/nav.ts:24`, the shell files the
row names, four overlay files (`FlowOverlay.tsx:16`, `MarkToast.tsx:14`, `OverlayAsk.tsx:4`,
`WhereWasIStrip.tsx:10`) and seven routes (`Deck`, `Insights`, `Moment`, `Recall`, `Session`,
`Thread`, `Work`). The v2 glyph set is a different module, `ui/src/components/primitives/icons/`.

**Decision.** Both deletions are recorded as unsafe and `specs/06_PATCH_PLAN.md` rows 137 and 138 now
carry the live importer lists. Row 137's premise is corrected rather than repeated: the kickoff's
"nothing else imports `Expander`" was true when P7 removed the single-file `Settings.tsx`, and P9's
Ask scope panel brought it back. No test or gate applies: no source, test, lint or dependency file
changed.

### 2026-11-10: Off-arc, 0.4.6 item 7, stale `jobs.kind` comment (`06` #42)

**Scope.** Comment-only documentation fix. The DDL prose in two places listed only two of the eight
`JobKind` tokens, which misleads anyone reading the schema or the spec.

**What changed.**

- `crates/store/src/schema.rs:225` and `specs/03_MASTER_PRODUCTION_SPEC.md:382` now list all eight
  tokens: `embed_text`, `vision_tag`, `graph_extract`, `graph_normalize`, `graph_cluster`,
  `graph_backfill`, `session_embed`, `insight_generate`, and say they are exactly the `JobKind`
  variants in `crates/traits/src/jobs.rs`. Both keep their existing notes (no value `CHECK`; the
  optional hardening stays `07` #82; `0.3.0 PR4` removed `embed_image`).
- No SQL token changed, no column, constraint, index, default or Rust type changed. The comment is
  inside the DDL string, so the schema itself is byte-identical after comment stripping.

**Verification.**

```text
$ cargo test -p store --test p3_schema_parity
running 2 tests
test baseline_holds_every_spec_section_4_declaration_and_the_documented_extras ... ok
test a_fresh_store_materialises_exactly_the_baseline_ddl ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

The parity guard compares `schema.rs` against `03 §4` with comments stripped, so this pair is the
evidence that the two copies still agree after the edit.

**Source of truth.** `crates/traits/src/jobs.rs:16-36` defines exactly eight variants:
`EmbedText`, `VisionTag`, `GraphExtract`, `GraphNormalize`, `GraphCluster`, `GraphBackfill`,
`SessionEmbed`, `InsightGenerate`. No other stale copy exists: the shorthand in
`crates/traits/src/jobs.rs:7` (`'graph_*'`) is accurate rather than exhaustive.

**Limitations.** Nothing here proves runtime enqueue behaviour; it is a documentation accuracy fix
for a schema comment.

### 2026-11-10: Off-arc, 0.4.6 item 3, a settings save refreshes the throttle readout (TODO-4)

**Scope.** `specs/TODO.md` TODO-4 recorded that `useSetSettings.onSettled` invalidated `settings`,
`hotkeyStatus` and `apiStatus` only, so `useThrottleStatus()` kept its pre-save payload until the
next `throttle_changed` tick. P7 compensated at the call site with
`<ThrottleReadout enabled={throttle.value === true} />`. This item fixes the seam and removes the
compensation.

**What changed.**

- `ui/src/lib/ipc/queryKeys.ts`: new `SETTINGS_SAVE_INVALIDATIONS` beside `GRAPH_JOB_INVALIDATIONS`,
  holding `settings`, `hotkeyStatus`, `apiStatus`, `throttleStatus`.
- `ui/src/lib/ipc/mutations.ts`: `invalidateAfterSettingsSave` loops the list and
  `useSetSettings.onSettled` calls it; the three inline invalidations are gone.
- `ui/src/routes/settings/ThrottleReadout.tsx`: the `enabled` prop is gone. The master switch comes
  from `useSettings()` (optimistic, refetched after a save) and the measurements from
  `useThrottleStatus()`. The setting is still read because `get_throttle_status` reports
  `enabled: false` both when the throttle is off and before the governor's first sample, so the
  status alone cannot tell the off line from the honest intermediate.
- `ui/src/routes/settings/Advanced.tsx:122`: `<ThrottleReadout />`.
- `ui/test/settingsSaveInvalidations.test.mjs`: the regression.

**Dependency audit behind the four-key set.** Settings categories read `settings`, `advancedConfig`,
`hotkeyStatus`, `monitors`, `readiness`, `storageStats`, `updateStatus`, `apiStatus`,
`textFilterStats`, `setupStatus`, `sidecarStatus` and `throttleStatus`. Only four change on a write:
`settings` (the write), `hotkeyStatus` (every save re-registers the shell hotkeys,
`src-tauri/src/lib.rs:1286-1292`), `apiStatus` (`apply_api_config` when `api.*` changes, `:1309-1329`)
and `throttleStatus` (`reconfigure_throttle` on every save, `:1357-1360`). `readiness` is patched by
`readiness_changed`; `advancedConfig` is the read-only file projection; `monitors`, `storageStats` and
`textFilterStats` are hardware or history reads; `apiReadTokenStatus` moves only through credential
commands; the rest are event or updater state. Nothing was added that nothing reads.

**Failing first.** The lead reproduced this independently of the executor, by deleting the
`throttleStatus` entry from the shared list and re-running the new file alone:

```text
$ node --test test/settingsSaveInvalidations.test.mjs     # throttleStatus removed from the list
  error: 'settings save invalidates settings, hotkeyStatus, apiStatus; throttleStatus is missing'
# tests 2
# pass 0
# fail 2

$ node --test test/settingsSaveInvalidations.test.mjs     # restored
# tests 2
# pass 2
# fail 0
```

**Known limitation.** This is cache-level evidence: the tests prove the invalidation set the mutation
runs, not that a mounted Settings > Advanced row repaints after a real save. That remains a live
UI check for the maintainer, listed in `docs/kickoffs/MORNING.md`.

**Affected-suite output** (portable Node v22.23.4, executor-run): `ui` typecheck, lint and build all
`EXIT=0`; `npm --prefix ui test` `# tests 174`, `# pass 174`, `# fail 0`. The full ordered gate is
below.

GATE (lead). Full ordered chain, native PowerShell runner, portable Node v22.23.4 and the external
`CARGO_TARGET_DIR`:

```text
$ powershell -NoProfile -File .../gate.ps1 -Log .../item3-gate.log
node v22.23.4 / cargo 1.95.0 (f2d3ce0bd 2026-03-21)
node_environment EXIT=0
ui_ci EXIT=0
ui_lint EXIT=0
ui_build EXIT=0
ui_test EXIT=0
stage_mcp EXIT=0
cargo_fmt EXIT=0
cargo_clippy EXIT=0
cargo_build EXIT=0
cargo_test EXIT=0
GATE_STEPS_ALL_ZERO=1
CARGO_TEST_AGGREGATE={"suites":87,"passed":1653,"failed":0,"ignored":17}
```

UI node tests in that log: `# tests 174`, `# pass 174`, `# fail 0` (172 before this item's two cases).
Binding guard after the workspace run: `git diff --exit-code -- ui/src/bindings` clean.

### 2026-11-10: Off-arc, 0.4.6 item 8, monitor hot-unplug (`07` #91, blocked, no code change)

**Scope.** The queue asked for this row to be reproduced in a test seam and closed. Reading the row,
the `03` capture sections and the cited `UI_REFERENCE.md:234` cell shows the specs prescribe no
behaviour for a topology change, so the item records the blocker rather than inventing one.

**What the specs do and do not say.** `03` has no sentence on hot-plug, unplug, re-enumeration,
capture restart or a monitor-level capture state. Its nearest sentences are the `CaptureSource` shape
(`03:64-71`), "one frame per monitor" for `capture_now` (`:787-791`), `capture.monitors` (`:1791`),
the corrupt-frame skip (`:2462`) and the multi-monitor v1.0 definition-of-done line (`:2468`).
`UI_REFERENCE.md:234` requires only that an unplugged monitor "degrades honestly", and the same cell
records that `#91` stays out of scope.

**Why no seam was written.** A reproduction seam needs a defined expected behaviour to assert
(restart capture, drop the monitor, surface a state, or hold silently). The specs give none of those,
and `04 §5` forbids inventing a threshold or a product rule. The question and the reproduction seam
are recorded in `specs/07_KNOWN_GAPS.md` under `#91`.

**No code, no test, no gate.** Nothing in `crates/capture`, the test tree or the specs' normative
text changed, so a gate would measure only code an earlier item already measured. No runtime
reproduction is claimed: the stall was not re-triggered on this host and no display was unplugged.
