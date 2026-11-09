# 05 - Build Review

The 0.4.5 arc shipped in v0.4.5. Ten phases, PRs #150-#166: P1 #150 `e27f42c`, P2 #151 `7e73e30`,
P3 #153 `0dfa2f5`, P4 #155 `aceca58`, P5 #157 `722f29e` (plus the review-fix pass #159), P6 #160
`6d906a7`, P7 #161 `5d15301`, `60c60d8`, `8e0883a`, P8 #164 `eb556d1`, P9 #165 `9112087` and
P10 #166 `e459185`. The full build record, every ordered gate, raw log excerpt, acceptance walk,
measured budget and review-fix pass included, is archived at
`specs/archive/05_BUILD_REVIEW.v0.4.5.md`. New entries start below.

## Current arc

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
stamp-sw: CACHE_VERSION mbe1267a6 matches the manifest and pack payload
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
| 5 | `tools/stamp-sw.mjs:27` — the offline-cache fingerprint ignores pack bytes | valid, reproduced | fingerprint extracted to `tools/cache-version.mjs`, hashing the manifest *and* every pack it names; `stamp-sw` and `check-dom` share it |
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

Why those two wait: `index.html` is generated from every `src/` fragment, and the owner loop's cycle 22
has four `src/` files uncommitted (+475 lines) with `index.html` not yet regenerated. A `src/` fix
could only be committed together with that unfinished work, so both land with the loop's snapshot
commit and stay listed as open items until then.
