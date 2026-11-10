# 08 - AI Changelog

The 0.4.5 arc shipped: P1 through P10, landed as PRs #150-#166. Its full record, including every
phase's build narrative, gate output and review-fix pass, is archived at
`specs/archive/08_CHANGELOG_AI.v0.4.5.md`; that archive is frozen and this live file carries only
the current arc.

## Current arc

### 2026-11-09 — Off-arc: memory arm for the throttle, level-2 relief, embedder idle TTL

Three gaps are implemented on `feat/memory-pressure-throttle`, all found by measuring the installed 0.4.5
build rather than by reading the spec: the throttle could not see a machine that was out of RAM while
ScreenSearch was idle, level 2 paused lanes but freed no memory (the 6–8 GiB belongs to the sidecar),
and the in-process fastembed model held 1233 MiB of private bytes for the life of the process. A
fourth finding — the GPU arm is blind to the sidecar's Vulkan compute — is recorded in `06` #23 with
three options and deliberately not implemented.

**Memory arm.** `PressureSample` gained physical usage, system commit usage and the three byte counts
plus `mem_monitored`; `ThrottleStatus` and `PressureProbe` mirror the flag, and the pure
`ThrottleMachine` applies the same hysteresis as CPU/GPU: either reading at its enter threshold
escalates, both must fall below their exit thresholds before an exit dwell can step down, and an
unmonitored sample leaves the arm inactive rather than contributing a fabricated 0 %. Physical comes
from `GlobalMemoryStatusEx` (`ullTotalPhys`/`ullAvailPhys`), commit from `GetPerformanceInfo`
(`CommitTotal`/`CommitLimit` × `PageSize`) — two syscalls after `MEMORYSTATUSEX`'s page-file fields
turned out to answer a per-process question, and both must succeed or the sample reports
`mem_monitored = false`. Commit is watched because it counts committed pages whether or not they are
resident, which is the half physical usage misses; plain address-space reservation charges nothing.
Defaults stay 90/80 for both, `file-only`; the maintainer box's own vision peak was 77 % physical /
67 % commit, so the default does not fire there, and the spec now says that instead of promising the
arm can never fire on the app's own work.

**Level-2 relief.** Entering Sustained (edge-triggered, never per tick) awaits a new
`PressureRelief` hook into the supervisor. It goes through `unload_reasoned(reason, skip_if_pinned)`,
which drains the request gate before consulting the pin, kills the child through the one teardown
path and emits `Evicted`; the hook is held late-bound by the kernel so the composition root remains
the only owner of inference types. Rule A is that `true` argument: relief outranks the backfill's
keep-warm but never an explicit user pin, and the skip is logged. Holding level 2 does not retry —
only a full exit dwell and a fresh escalation can ask again, so a long starvation window cannot
become a kill/reload loop. Flipping to rule B is that one argument plus its matrix test; the pin's
separate job of suppressing idle-TTL eviction is untouched.

**Embedder idle TTL.** fastembed now lives behind `LazyEmbedder`: nothing loads until the first
`embed_texts` (single-flight) or an eager `load_now` at startup, so `Ready` is not followed by a cold
first search; `dim()` and the model name answer without loading; a 30 s sweep releases the ONNX
session after `enrich.embed_idle_ttl_secs` (default 900, `0` disables) with no embed — but only when
no embed still holds it: eviction checks the session's strong count under the slot write, so the
sweep and an explicit unload return `false` and leave the slot alone while a session is in use,
instead of dropping it mid-flight (the shape that let a second session load beside the first); the
load gate spans teardown so a reload cannot overlap it, and that gate is owned by the load rather than
by the awaiting caller (a cancelled first embed cannot release the flight before the model is
published — see the PR #175 review pass below); a completing embed stamps last-use before
releasing its handle, even on error; teardown runs off the async worker thread.

**Baseline, and what it is not.** These are installed-0.4.5 numbers measured before the branch, not
this build's footprint:

| Reading (installed 0.4.5) | Value |
|---|---|
| App private bytes, embedding model resident / `enrich.embed_text = false` | 1352 MiB / 119 MiB |
| Available physical RAM + commit charge held while the sidecar was loaded | ~5.8 GB / ~8.4 GB |
| VRAM held while the sidecar was loaded | ~6.8 GB |
| GPU, PDH sums vs `nvidia-smi`, during vision inference | 37.6 % vs 96 % |

The 1352 → 119 MiB pair is a delta, not an unload measurement: the 119 MiB came from a different
configuration. The holder itself is verified narrowly in isolation (next paragraph), not as the
app's steady state.

**Narrow runtime evidence.** A separate harness — `path` deps on the branch's `embeddings`, `sysmon`
and `traits`, repository lockfile, the real fastembed cache copied to `Temp/ss-memory-proof/cache`,
no app launch and no profile change — passed the real-model lifecycle (`artifact://288`): process
private bytes 7 MiB before load → 1228 MiB loaded → 26 MiB after the idle sweep (TTL 2 s) → 1234 MiB
after reload, with identical 768-dim query vectors (`maxdiff 0`). The real sweep logged `loaded
21:17:46.376`, `unloaded 21:18:16.484` and `reloaded 21:18:24.739`, and an explicit `unload()` after
35 s returned `false` because the sweep had already evicted the model, so nothing was resident to
unload (no embed was in flight). Host sample: physical 69.08 %,
commit 57.48 %, available 9841 MiB, charge 30068 MiB, limit 52310 MiB. This proves the holder's
lifecycle in isolation; 26 MiB is the harness process, not the app, so the 119 MiB whole-app figure
stays a different-configuration baseline, and reload latency, level-2 relief and Tauri/IPC remain
untested.

**Settings and UI.** Five new keys (four memory thresholds, one TTL), all `file-only` and clamped, so
the closed 26-key UI ceiling is untouched. The Settings · Advanced throttle readout shows
`RAM 71% (8.3 GiB available)` and a Commit percentage from the same sample, reports available memory
rather than "free", and says "not monitored" instead of 0 %; the level-2 line says the app tries to
free the AI model's memory unless the user explicitly kept it loaded, which is what the code can
promise — a kept-loaded or absent model is not released.
The parent rendered the real component in a disposable Vite harness with illustrative query fixtures
across seven states, then checked `scrollWidth <= innerWidth` in eight width/theme combinations with
no browser errors; after the copy settled on the final plain-language wording it was rendered and
inspected at 360 px pine-dark (`scrollWidth` 360, no browser errors), and the fixtures and temporary
server were removed — browser rendering evidence only, not Tauri acceptance and not a memory or
unload proof.

**Docs and gate.** `03` now carries the memory arm, the two-syscall probe, the relief rule and the
lazy-holder contract, and `05`/`06`/`07`/`08` plus `CHANGELOG` record the branch honestly: baseline
versus changed build, what the branch's seam tests, the isolated harness and the parent's browser
rendering cover, versus what no run has yet shown (the whole app's idle footprint, a timed reload and
a real level-2 release). This documentation pass ran no build, test, lint, formatter or git command.
Chronology, kept because the fixes came from it: the first targeted runs passed (`inference` 125/2
ignored incl. the five real-child pin tests; `sysmon` 18/0, `artifact://303`); the first full gate had
every build stage green before the full `cargo test` failed on the new Windows
`idle_ttl_bounds_eviction` (`Instant::checked_sub` rewind); after the forward-only clock and
completion-handle fixes, gate2 (`artifact://325/314`) showed embeddings unit 8/2 ignored with both
ignored real-model tests 2/0 on a copied cache against the final code, and `inference` 125,
`no_orphan` 1, `reap` 3, kernel 228 (throttle 3, settings 32) green, but stopped at
`rebuild::settings::spec_table_parses_to_the_committed_sets` (49 versus an incidental hardcoded 44),
a test-only count the runtime already parses as 49 from `03`. That test is corrected — obsolete
accessor and the 44 / deleted-9 assertions removed, the closed UI budget kept as
`ui_settings_respect_the_closed_ceiling` (`<= 26`). The narrow rebuild test passed 6/0 first, and
gate3 (`artifact://327` full, `artifact://331` tail, log
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

The verified aggregate over the full workspace (doctests counted as suites) was exactly
`{"suites":86,"passed":1650,"failed":0,"ignored":17}`. `BINDINGS_REGEN_CHANGED=[]` means the four
already-regenerated bindings hashed identically before and after `cargo_test`; `bindings_git_diff`
exited 1 at gate3 because the bindings were uncommitted and no commit was authorized, so gate3's
record does not call the binding guard green. The user has since authorized committing on the new
branch `feat/system-memory-relief`; all 31 feature paths are staged with the generated bindings,
`git diff --exit-code -- ui/src/bindings` now exits 0 with no output, and the staged diff check
excluding generated TypeScript also exits 0 — the guard is satisfied by the staged tree, not pending,
and nothing here claims the commit has landed. Build/test evidence only: no changed-build footprint,
timed reload, real level-2 relief or native Tauri acceptance is claimed. The 0.4.5 launch panic was
hit on this pass's launch attempts and worked around with a local relaunch harness — recorded as `06`
#150, not fixed, and deliberately not called a shipped retry loop. No GPU code was written.

### 2026-11-09 — Review pass on PR #175: a cancelled first load could double-load, fix implemented

The re-review of the holder head (`959b008`) raised one finding, and it is valid. `acquire()` took
`load_gate.lock().await` in the caller's own future and then awaited `spawn_blocking(M::load)`, so a
dropped caller released the borrowed guard while the blocking load ran on: the next embed saw an empty
slot, satisfied the sweep's strong-count test and started a second `M::load` beside a load that was
still allocating, and the first model was never published. The fix gives the gate to the work it
protects — the gate is taken as an owned guard and moved into the blocking closure, whose body opens
with `let _gate = gate;`, together with the slot and the idle clock, which load, publish and stamp the
session before it drops, leaving the awaiting caller with nothing but the join. `03 §5` now states that
rule, and the two earlier single-flight sentences in `05` and above are amended so they no longer read
as a caller-owned gate.

Recorded as `06` #151. Both directions of the new test were run, so what exists is a discrimination
rather than a single green line. Fail side (`artifact://422`): with the closure's `let _gate = gate;`
replaced by `drop(gate); // NEUTERED…`, `cargo test -p embeddings
cancelled_cold_load_publishes_one_session -- --exact
tests::cancelled_cold_load_publishes_one_session --nocapture` failed at
`crates/embeddings/src/lib.rs:697` on `assertion left == right failed: the published session is reused,
not loaded`, left 2 / right 1, with `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11
filtered out`. Pass side (`artifact://424`), on the restored source:

```text
abf2c22859b23abf4ab95e0b4ccbb5f3a8e885b75c21427fc570d482f12589be  crates/embeddings/src/lib.rs
test tests::cancelled_cold_load_publishes_one_session ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.02s
```

The same artifact reports the whole module as `10 passed; 0 failed; 2 ignored` — the two ignored entries
being the real-model tests — and doc-tests `0 passed; 0 failed; 0 ignored`. That hash is the restoration
of the proof snapshot, the value intended before the mutation, so it identifies the file the pass side
ran on and not the current file: `cargo fmt` reformatted the source after the proof. A grep for
`NEUTERED` found nothing. The build's one warning, `unused import: std::future::Future` at
`crates/embeddings/src/lib.rs:411`, was not cosmetic: the first full attempt (`artifact://428`) passed
`ui_ci`, `ui_lint`, `ui_build`, `stage_mcp`, `cargo_fmt` and the ignored real-model tests (`2 passed; 0
failed; 0 ignored; 0 measured; 10 filtered out`, two suites including doc-tests), then stopped at
`cargo_clippy EXIT=101` on that import, with `-D unused-imports` implied by `-D warnings` and
`STOPPED_AT=cargo_clippy`. Only the unused import was removed, no behaviour change. An earlier draft of
the test also failed to compile (`E0716`, a temporary dropped while borrowed) and was corrected by
binding `inputs` before the timeout; that compile failure is not evidence either way.

The final run (`artifact://429`, tail `artifact://432`, log `Temp/ss-pr175-review-gate.log`) reported
every step `EXIT=0` — `ui_ci`, `ui_lint`, `ui_build`, `stage_mcp`, `cargo_fmt`, `cargo_clippy`,
`cargo_build`, `cargo_test`, `bindings_diff` — with the verified workspace aggregate exactly
`{"suites":86,"passed":1652,"failed":0,"ignored":17}` and `git diff --check` exiting 0. `06` #151 is
resolved as a review-fix pass on PR #175, landed as `63e22f1` (`fix: preserve embedding load ownership
across cancellation`).

### 2026-11-09 — PR #175 follow-up: clean re-review at `63e22f1`, `mem.rs` FFI boundary cleared, one test comment corrected

**The re-review is clean at the exact head.** `63e22f1` carries the cancellation fix, and the automated
review of that SHA `completed` with no findings: `code_review` at that commit,
`has_any_findings=false`, `has_unresolved_findings=false`, delivered as issue-comment summary
`6752419125`. The original inline finding on the holder head, comment `4518433333`, remains resolved.
No REST review was requested for a later head, so this entry describes no bot review of any later
commit.

**A second, independent look at the `mem.rs` FFI boundary found nothing — within that scope only.** That
review read just the Windows FFI in `crates/sysmon/src/mem.rs`: the two direct syscalls
(`GlobalMemoryStatusEx` at `:108`, `GetPerformanceInfo` at `:118`), the
`MEMORYSTATUSEX`/`PERFORMANCE_INFORMATION` structs they fill, the `BOOL` result handling and the
page-count arithmetic. Struct sizes and layout match the Win32 definitions, a failed call returns an
unmonitored reading instead of a fabricated percentage, and the conversions to bytes are exact. It is a
verdict on that boundary, not on PR #175 as a whole.

**One valid finding, in a comment rather than the code.** `status_clamps_inconsistent_counter_snapshots`
justified its deliberately inconsistent input by saying the two commit counters are separate syscall
snapshots, so a pagefile shrink between them could leave the charge above its limit. Both come from one
`GetPerformanceInfo` call, so the explanation was wrong even though the assertion it guards is correct.
The comment now states that the input is normalized defensively over the `0..=100` contract and names
the single-snapshot fact (`crates/sysmon/src/mem.rs:203-205`). Assertions, code path and thresholds are
untouched, and no test was written for the wording: with no behaviour behind it there is nothing an
assertion could fail on, so adding one would only dress the change up as coverage. The correction stays
inside that test: a grep of `specs/` for `relief_should_act` returns no match, so no spec sentence named
that symbol and none needed correcting. Recorded as `06` #152.

**The cancellation fix's failure path was re-checked and is unchanged.** The gate now lives inside the
blocking closure, so the question is what happens when the load itself fails:
`crates/embeddings/src/lib.rs:359` takes the guard into the closure, `:360-362` load, publish and stamp,
and the `?` on a failed `M::load` unwinds the closure so the guard dies with the work — nothing is
published, the error reaches the caller, and the next call retries a fresh load instead of parking
behind a leaked guard. The neighbouring gate sites still hold: `evict` keeps the gate across teardown
(`:264`) and `take_evictable` decides and takes under the slot lock (`:283-285`). Parent-observed raw
output on this tree: `failed_load_releases_the_gate_for_a_retry` `1 passed; 0 failed; 11 filtered out`,
and `sysmon` `mem::tests::host_reports_a_plausible_memory_reading` `1 passed; 0 failed; 17 filtered
out`.

**Gate.** The advisory gate (`artifact://471`, log `Temp/ss-pr175-advisory-gate.log`) returned
`sysmon_module EXIT=0` on `test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
(doctests `0`), with `cargo_fmt`, `cargo_clippy`, `cargo_build`, `cargo_test`, `bindings_diff` and
`diff_check` all `EXIT=0` and the computed workspace aggregate exactly
`{"suites":86,"passed":1652,"failed":0,"ignored":17}`. The earlier full UI gate remains the record for
the UI and was not rerun, because this pass changes no UI, Node or dependency file. The commit is the
parent's step, so this pass cites no SHA.

### 2026-11-09 — Review pass round two on PR #173: payload fingerprint, two source fixes deferred

Three more findings landed on the head that carried the first fix. The cache fingerprint was wrong in
a way that serves stale packs to returning visitors, and is fixed; the other two are valid defects in
`src/` that cannot be committed while the owner loop is mid-cycle in that tree.

`tools/stamp-sw.mjs` hashed `assets/manifest.json` alone and called that the payload fingerprint, but
the manifest records paths, licences and metadata only: no digest of any pack, and a rebuild can change
textures, materials, normals or interior vertices without moving the triangle counts and bounds it does
record. One flipped byte inside `kenney-car-kit.glb` left the committed stamp reporting "matches" with
the payload already changed.
The fingerprint now lives in `tools/cache-version.mjs`, hashes the manifest, every pack it names and
the opening sample (the one other payload `sw.js` precaches and serves cache-first, regenerated by
`gen-sample.mjs` with no pack change), and is shared by `stamp-sw` and `check-dom` so the two cannot
drift apart; the same flipped byte now fails both with `mbe1267a6 vs mb477225d`, a flipped sample byte
fails them with `mbe1267a6 vs mc209a38d`, and the stamped payload reads `ma3977eae`.

`agents.mjs` gates the green wave on `longestSessionBefore(t)`, which returns the longest session that
started by `t` rather than the one open at `t`: over the shipped sample the wave is on with nothing open
for 472 of 892 sampled minutes (52.9 percent), holding the 1.28 avenue multiplier that is meant to
answer congestion. `main.mjs`'s `install()` disposes the dream group but not its `.dream-card` nodes,
which `dream.mjs` keeps in an internal array with no teardown in its returned API, so cards bound to a
discarded dream survive an export reload and the next waking exceeds the two-card limit. Both are
recorded in `07` with their fixes; they land with the loop's snapshot, because `index.html` is generated
from every `src/` fragment and the loop writes into four of them continuously (measured at +502
uncommitted lines, writing every 25 to 50 seconds, when this pass checked).

`check` now counts 38 checks, since `tools/cache-version.mjs` joined its `node --check` sweep.

### 2026-11-09 — Review pass round three on PR #173: one more source defect, deferred

The re-review of the sample-fix head raised one finding, and it is valid: `main.mjs` wires input before
`boot()`'s `Promise.all([fetchSample(), loadAssets()])` settles, so an export dropped inside that
window is installed by `loadExport()` and then thrown away by the callback's unconditional
`install(next)`. The 8000 ms pack deadline is the window, and `install()` disposes the world, agent and
dream groups before rebuilding, so the user's district does not survive the second call. Recorded in
`07` with the guard's shape: a `userModel` flag set in `loadExport()`, read once in the callback around
both the sample `install` and the portrait tier that was chosen for the sample's framing, leaving the
declutter interval, animation loop, cache registration and bench timeout to run either way. It ships
with the loop's snapshot for the same reason as its two predecessors.

### 2026-11-09 — Review pass round four on PR #173: the shadow target leak, deferred

The re-review of the record head raised one finding, and it is valid. `world.mjs` adds the sun to the
group `install()` disposes, and `disposeGroup` traverses it, but its traversal returns for any object
without a material, which a `DirectionalLight` is, so `sun.shadow.map` outlives the disposal and its GPU
allocation is released only when the page unloads. Each import of another export on a desktop with
shadows on costs one shadow target, the largest the app allocates. Recorded in `07` with both available
shapes and a preference for the light branch in `disposeGroup`, which keeps the rule that a disposed
group releases everything it owns. It ships with the loop's snapshot, like the other three.

### 2026-11-09 — Review pass on PR #173: server robustness and gate case-sensitivity

The bot review of #173 raised six findings: two were real defects in the deliverable's tooling, and
four were inaccurate as written, though the `check.mjs` one still prompted a hardening. Both kinds
are recorded in `05` with the raw fail-without and pass-with output.

`tools/serve.mjs` decoded the request path outside its error boundary, so one malformed escape
(`GET /%ZZ`) rejected the async handler, escaped unhandled and exited on `URIError: URI malformed`
while the client received `ECONNRESET`. The decode now sits inside a boundary that answers 400, and
`tools/test-serve.mjs` is the gate that keeps it honest: it fails with `exit code 1` on the old
server and passes on the new one, then re-requests `/index.html` to prove the process survived.

`tools/check-dom.mjs` matched `<script`, `<link` and the import map case-sensitively, so a hand-edited
`src/shell.html` carrying `<SCRIPT SRC="evil.js">` passed the whole gate: `total: 100 checks, all
PASS`, exit 0. HTML tag and attribute names are case-insensitive, so every one of those matches now
carries `i`, and the same file fails the gate with two checks.

`tools/check.mjs` built its CC0 host allowance as `new RegExp('^https://' + host...)` and escaped the
dots at construction. CodeQL reported the dots as unescaped, which was inaccurate, but the shape is
what invited the reading: the allowance is now a plain prefix comparison, verified equivalent over 23
URLs including the adversarial host-suffix forms, so no hostname is ever re-parsed as a pattern.

`check` consequently counts 37 checks rather than 36, since its `node --check` sweep picks up the new
gate script. `index.html` is untouched: none of these files is part of the generated bundle.

### 2026-11-09 — Specs rotation for the next arc

The shipped 0.4.5 build-loop history moved into `specs/archive/`: `05_BUILD_REVIEW.v0.4.5.md`,
`07_KNOWN_GAPS.v0.4.5.md` and `08_CHANGELOG_AI.v0.4.5.md` now carry the phase narrative, gaps and
gate output that were in the live files, and the live files keep a pointer plus only current
content. `06`'s "Still open from earlier arcs" row 23 gained an explicit owner. The instruction that
drove the rotation is archived at `specs/archive/specs-cleanup-prompt.md`.

Fixed here: `04 §1`'s arc pointer said the next arc was 0.4.5 after 0.4.5 had shipped; it now points
at the shipped-and-archived state and leaves the next product arc undefined.

Tasks 1-4 of that instruction (re-verify the older carried-forward gaps, prune `TODO.md`, tighten
`01 §8`, fix stale cross-references in `02`/`03`/`UI_REFERENCE`) were completed in the entry below,
which re-checked every item against the code at `9c45ae8` and kept anything it could not settle
open with "status unverified".

### 2026-11-09 — Your-Universe: Localtime delivered

`Your-Universe/` now carries "Localtime", a standalone Three.js living hillside district driven by
ScreenSearch's export shape, built against `Your-Universe/build-prompt.md` and iterated for 22
measured cycles against the owner verdicts in `Your-Universe/OWNER-NOTES.md`. Full cycle-by-cycle
narrative, measurements and gate output live in `Your-Universe/ITERATIONS.md`; the build review
summary is in `05`. This touches no product code: no crate, schema, IPC or UI change.

Delivered: `index.html` (1,027,378 bytes on disk, 1,026,873 as the stored LF blob, generated by
`tools/assemble.mjs`, byte-identical on rebuild), `assets/` (3,093,739 bytes of Kenney CC0 packs plus
licence texts, loaded through the
service worker with `?noassets=1` falling back to procedural), `src/` fragments bound by
`src/CONTRACT.md`, `tools/` gates and the asset pipeline, `sample-export.json`, and 24 deliverable
frames under `screenshots/`. Re-run gates on the committed tree: assemble 23/23, check 36/36,
check-dom 100/100, smoke-data 120/120, check-sample six properties, `stamp-sw` digest match,
`check-assets` PASS (87 models).

Kept out of git, on purpose, and listed in `Your-Universe/.gitignore`: the 2.3 GB
`masterpiece.jsonl` transcript, `.cache/` downloads, the `.cycles/` per-cycle snapshot directories
and bulk captures (the probe scripts and notes they are cited from, including `gate.mjs`, stay
tracked), and the baseline, tuning, diagnostic and per-cycle screenshot series (their numbers are
quoted in `ITERATIONS.md`). The commit is 98 files, 35.0 MB.

The owner loop was still cycling while this entry was written, so its fleet integration call landed
separately as `4c7fbea` (`MODEL_LANE_EVERY` 9999 to 24, re-measured at 388,884 triangles and 101
calls); later cycles will follow as more commits rather than as rewrites of this entry.

Open and recorded in `07` rather than claimed as met: the fps bench gate (a recurring 1.0-1.1 s
tail frame; p1Low regressed 169.5 to 41.7 with the crowd and asset pass), the draw-call line
disagreement (header says under 80, cycle-21 table measures 101, no cycle restated the call budget),
the 393k-398k bench-time triangle peak over the 390k line, the violet dream watch item at 5.5-5.6
percent, and the absence of any human visual pass over the final frames.

### 2026-11-09 — Specs cleanup: the carried-forward backlog re-verified against the code

Completes tasks 1-4 of that instruction. Every carried-forward item in `07`
(the 2026-11-03, 2026-10-19, 2026-10-15 sections, the maintenance follow-ups, the npm advisory and
the whole gap table), the `06` "Still open from earlier arcs" rows and every `TODO.md` entry was
checked against the v0.4.5 tree (`9c45ae8`) and the git/PR history. Each decision rests on a commit, a PR
or a `path:line`, and anything that could not be settled that way stayed open with "status
unverified" rather than being closed on a hunch.

Shape of the result: `07` now holds one `## Open, carried forward` section grouped into maintainer
decisions, acceptance gates not yet passed, and engineering hardening, with row ids preserved so the
citations in `02`/`03`/`UI_REFERENCE` still resolve; the closed items are listed one line of
evidence each in the new archive section `## 2026-11-09: Carried-forward items closed in the
post-0.4.5 specs cleanup` of `specs/archive/07_KNOWN_GAPS.v0.4.5.md`, whose frozen text stays
authoritative for their full wording. Twenty-nine items closed — fifteen resolved, fourteen
superseded by shipped code or by a later normative decision — and fifty-four remain open.

`06`: the archived rows that were still marked open or deferred at release were surfaced into the
live table, so archived no longer means silently dropped. `TODO.md`: the one shipped item moved to
the new `specs/archive/TODO.v0.4.5.md`; the five that still apply were re-verified line by line
against current code. `01 §8` is now a current-state summary after v0.4.5, with the per-phase
narrative moved verbatim to `specs/archive/01_STATUS_HISTORY.md`. `04 §1` points the next session at
the open work instead of the finished 0.4.5 phases; its rules are untouched.

Deliberately not done: `02`, `03` and `UI_REFERENCE` got only pointer and contradiction fixes, no
normative rewrites and no product decisions. That sentence was audited afterwards, hunk by hunk: 12
hunks across the three files (4 each), each one either a forward pointer (the old `05`/`06`/`07`
records now live in `specs/archive/`) or a statement contradicted by shipped code, checked against
the tree: the rebuild tool never renames its source and keeps `*.v13.bak`
(`crates/rebuild/src/lib.rs:28`, `crates/rebuild/src/main.rs:115`), the NavRail's Ask and Work
entries and their routes exist (`ui/src/app/nav.ts:51-58`), and Settings v2 ships the search box
(`ui/src/routes/settings/SettingsSearch.tsx`). `03`'s four hunks change six lines and replace six,
none removing normative text. Where a shipped behaviour differs from a spec sentence
(a settings-key count, a stale "deferred" label, the `03 §8` default table), the item is recorded as
open work above instead of being silently rewritten.

### 2026-11-10: Off-arc, 0.4.6 item 1, updater state registered on the builder (#150)

Fixes the launch panic recorded in `06` #150: `get_update_status` fired from the main window's
mount (`queries.ts`, `useUpdateStatus`) while `app.manage(update::UpdaterState::default())` had not
run yet. `get_update_status` takes an `AppHandle` (`src-tauri/src/update.rs:199`), and its
`current_status` helper calls `app.state::<UpdaterState>()` (`update.rs:72`), which is
`Manager::state` and panics on an unmanaged type instead of reporting a missing state. The root of
that panic is an ordering one: Tauri creates the config-declared `main` and `overlay` windows
inside its own setup before the app's setup closure (`tauri-2.11.3/src/app.rs:2521-2532`), so both
windows and their WebViews exist while that closure is still doing launch work.

`UpdaterState` is now registered on the `tauri::Builder` through the single seam
`manage_pre_window_state` (`src-tauri/src/lib.rs:1393`), which `run()` applies before `build()`;
the setup-closure registration is removed, leaving exactly one registration site. The row's second
shape (calling `try_state` so an absent state is tolerated) was not taken; the handler already took
`AppHandle`, so that shape was a lookup change inside `current_status`. No retry loop was added.
`UpdaterState` is re-exported (`#[doc(hidden)]`) and the seam is `pub` so the regression test
drives the real function; both carry doc comments saying they are a test seam and the single
registration site.

`src-tauri/tests/launch_order.rs` builds a `tauri::test` mock app from that same function, asserts
no WebView exists, then performs the same `app.state::<UpdaterState>()` lookup `current_status`
performs. With the shipped ordering it fails with the production panic verbatim
(`state() called before manage() for screensearch_lib::update::UpdaterState` at
`tauri-2.11.3/src/lib.rs:734`); with the fix it passes. The test is an integration test because
`cargo:rustc-link-arg-tests` does not reach lib unit-test targets, and the mock runtime needs the
Common Controls v6 application manifest that `tauri-build` only emits for `[[bin]]` targets; the
`*-tests` link args in `src-tauri/build.rs` supply it without touching shipped binaries. The
`tauri` `test` feature is a dev-dependency only.

Verification: focused failing-first and passing raw output in `05` (2026-11-10). The full gate
(`ui_ci`, `ui_lint`, `ui_build`, `ui_test`, `stage_mcp`, `cargo_fmt`, `cargo_clippy`, `cargo_build`,
`cargo_test`) reported `EXIT=0` on every step under the CI-pinned Node 22 (portable v22.23.4, probed
for both the invoked interpreter and a spawned child), with 1653 workspace tests passed, 172 UI node
tests passed and the binding guard clean. It ran with `CARGO_TARGET_DIR` pointed outside `Documents`,
because this host denies temp-dir creation to test executables built under the repository's own
`target` directory; that environment finding is recorded in `07` and is unrelated to this change. The live proof is now complete:
20 consecutive isolated cold dev launches, zero panics and zero early exits, including ten seconds
of post-readiness observation per launch. The pre-fix dev control also passed 8/8, so the live dev
series does not discriminate by itself; the failing-first regression does. Full raw output is in `05`.

### 2026-11-10 - `06` #116 recorded as blocked on a spec decision (no code change)

The 0.4.6 stabilisation run, item 2, verified from source that an Ask turn still has no terminal
path when its provider never produces a delta: `forward_turn` waits only on consumer closure and
`prx.recv()` (`crates/kernel/src/ask.rs:466-476`), its sole timeout is the 250 ms cancel grace
(`:636`), and with no answer weights `AnswerSidecar::ensure_spec` enters the unbounded first-use
download (`crates/inference/src/answer.rs:94-113`) before any delta exists. The client's 30 s
connect and idle timeouts apply only after the streaming request starts
(`crates/inference/src/client.rs:32-33`, `:305-326`), so nothing bounds the turn.

No code was changed. `03 §7i` defines no turn deadline, `03 §8` forbids a hardcoded threshold and
the Settings UI key set is a closed 26-key ceiling, and failing fast on absent weights would change
documented first-use download behavior (`docs/ARCHITECTURE.md:384-388`). The required product
decision is recorded instead.

Changed (documentation only):

- `specs/06_PATCH_PLAN.md` row 116: status extended with the 2026-11-10 blocked decision, the
  confirmed non-terminal path and pointers to the question.
- `specs/07_KNOWN_GAPS.md`: new `06` #116 record (explicitly not the session-model `#116`), with the
  confirmed code anchors, the reusable terminal paths, the latent zero-delta success path, and the
  blocking question (download during Ask, deadline value, clock start, threshold disposition).

No tests, build, lint or gate were run because no code changed. No runtime reproduction or model
use is claimed.

### 2026-11-10: `06` #143 recorded as unremovable, no code change

The 0.4.6 queue allowed the `get_insights` removal only when no importer exists. It has one:
`getInsights` (`ui/src/lib/ipc/commands.ts:211-215`) is wrapped by `useInsights`
(`ui/src/lib/ipc/queries.ts:303-311`) and read by `routes/Deck.tsx:29,72`. The `insights_summary`
aggregate and its two store tests keep that consumer, so nothing was deleted and no schema or
contract changed. Row 143 records the evidence; the ownership dispute it names is unchanged.

### 2026-11-10: `06` #137 and #138 recorded as unremovable, no code change

`Expander` has four live references (`ui/src/components/primitives/index.ts:13` re-export, the Ask
scope panel at `routes/Ask.tsx:46,593-707`, and the dev gallery plus its specimens at
`routes/Kitchen.tsx:20,306-322` and `routes/KitchenPrimitives.tsx:24,152-179,232-239`, which
`Kitchen.tsx:39` imports). `ui/src/components/icons` is imported by 27 files across the primitives,
the shell, the overlays and seven routes. Neither deletion condition holds, so no component, icon or
route was touched. Rows 137 and 138 record the evidence and the corrected premise for 137.

### 2026-11-10: `06` #42 stale `jobs.kind` comment corrected, comment only

`crates/store/src/schema.rs:225` and `specs/03_MASTER_PRODUCTION_SPEC.md:382` described `jobs.kind`
as holding only `embed_text` and `vision_tag`. `JobKind` has eight variants
(`crates/traits/src/jobs.rs:16-36`), so both comments now list all eight and name that type as the
source of truth, keeping the existing no-value-CHECK notes and `07` #82. No SQL, column, constraint,
index, default, Rust type or test changed; the store parity pair still passes
(`cargo test -p store --test p3_schema_parity`, 2 passed, 0 failed).
### 2026-11-10: Off-arc, 0.4.6 item 3, settings save invalidates the throttle status (TODO-4)

`useSetSettings.onSettled` invalidated `settings`, `hotkeyStatus` and `apiStatus`, so a readout
built on `useThrottleStatus()` kept its pre-save state until the next `throttle_changed` tick, and
P7 had worked around it at the call site with `<ThrottleReadout enabled={throttle.value === true} />`.
The three inline invalidations moved into `SETTINGS_SAVE_INVALIDATIONS` beside
`GRAPH_JOB_INVALIDATIONS` (`ui/src/lib/ipc/queryKeys.ts`) and the mutation consumes them through
`invalidateAfterSettingsSave` (`ui/src/lib/ipc/mutations.ts`); `throttleStatus` is the fourth key.

Only four of the twelve families the settings pages read change on a write: `settings` itself,
`hotkeyStatus` (every save re-registers the shell hotkeys), `apiStatus` (`apply_api_config`) and
`throttleStatus` (`reconfigure_throttle`). `readiness` is patched by `readiness_changed`, and the
rest are hardware, history, read-only or credential-command reads.

`ThrottleReadout` no longer takes a prop: it reads the switch from `useSettings()` and the numbers
from `useThrottleStatus()`, because `get_throttle_status` reports `enabled: false` both when off and
before the governor's first sample. Loading, probe-error, off, turning-on and populated states are
unchanged. `ui/test/settingsSaveInvalidations.test.mjs` fails on a three-key set with
`settings save invalidates settings, hotkeyStatus, apiStatus; throttleStatus is missing` and passes
at four. Full gate green under Node v22.23.4 (1653 workspace tests, 174 UI node tests).
