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
| 1 | `crates/embeddings/src/lib.rs` — a cancelled `acquire()` drops the borrowed async `load_gate` while the blocking load carries on, so the next caller can start a second ONNX load beside the first and the first never publishes | valid, proved from the call shape | fix shape: the gate is owned by the load, and load, publication and stamp all happen under it; the fix is in the working tree, uncommitted, and its test fails with a neutered gate and passes as written (raw output below) |

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
`git diff --check` exits 0, and the focused and module results above hold on the final code. No commit
exists yet, so no SHA is cited here. `06` #151 is resolved by this pass.

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


