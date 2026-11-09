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
check: 36 checks, 36 passed, 0 failed
total: 100 checks, all PASS                   (check-dom)
120 passed, 0 failed                          (smoke-data)
OK: all six properties hold                   (check-sample)
stamp-sw: CACHE_VERSION m730a18fd matches assets/manifest.json
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
