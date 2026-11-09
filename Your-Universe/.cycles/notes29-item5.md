# Cycle 29, owner item 5: performance, the first-wake stall, and the draw-call line

Owner words (OWNER-NOTES 2026-11-09 14:00): "Performance: the 1.0 to 1.1 s frame at dream wake comes
from traffic repopulation in one frame. Spread repopulation over at least 30 frames (time-sliced, no
per-frame allocation). Gate: desktop p1LowFps >= 100 and no frame over 33 ms after warmup, phone4x
p1LowFps >= 60. Set the draw-call line in the header to the measured value (101 desktop with assets,
55 noassets) or get under 80; the header and the measurement must agree."

This file is the measurement record. Scratch probes live beside it in `.cycles/` (`wake29trace.js`,
`phase29.js`, `prog29.js`, `gl29.js`, `ladder29.js`, `sw29.js`, `swtrace29.js`, `calls29.js`,
`wake29parse.mjs`, and for the window itself `reseed30.js` in the browser plus `slice30.mjs`, which
runs the shipped resync block with no page).

## 1. The stall is not the re-population

`wakeStall(8731)` before the change, first run of the session, fresh profile: **max 936.5 ms** at
the `dreaming -> waking` edge, 1,555 frames, its JS time **0.9 ms**. Reproduced on a second page:
max 980.1 ms, JS 0.6 ms. Cycle 1 of three sleep/wake cycles in one page: 980.1 ms; cycles 2 and 3:
12.8 ms and 8.1 ms. So it is one-off per page life.

JS time on that frame is one to two milliseconds, and the six pool ramps are rate-limited to
`easeTo(..., 3.2, dt)` from the cycle 26 build onward, i.e. the refill is already spread over about
55 frames at 180 fps. There is no one-frame repopulation to find in `agents.mjs`: the dream freezes
the pools, and waking resumes them from their last valid arcs. A 1.0 s frame cannot be 0.9 ms of
pool work.

Attribution, in the order it was run (each row is one fresh tab, one bench or one recorder):

| probe | measurement |
| --- | --- |
| plain `rig.setTier('block')` at ~13 s, no dream at all | 311.2 ms frame, JS 1.1 ms, calls and triangles unchanged at 103 / 377,304 |
| same on a second page, SW registration blocked in the page | 6.5 ms max over the whole 4 s window |
| same, after a first load, SW controlling with 270 cached entries | 6.1 ms max |
| `prog29.js` (wraps createProgram/useProgram from before the page's own JS) | 41 programs, **none** first used after boot, so no shader compile on the stall frame |
| `gl29.js` (wraps linkProgram/getProgramParameter/bufferData/texImage2D) | no GL call over 1.5 ms in any of 2,899 frames |
| `ladder29.js` (quality notch and pixel ratio per frame) | notch 0 / ratio 1 for the whole run, no stage change, no slow frame |
| `sw29.js`: caches cleared and worker unregistered, then reload with a frame recorder | stall at frame 15, 387.2 ms, JS 1.1 ms, exactly as cache entries went 0 -> 9 -> 12 |
| `swtrace29.js`, 17 s trace with the SW installing | only task over 30 ms in the renderer is a 385 ms boot microtask checkpoint, and it also appears with the SW blocked; no renderer task anywhere near the 13 s stall |
| `wakeBisect` at `?hour=22&post=0` / shadow map 0 | wake max 18.1 ms / 22.4 ms, against 300.4 ms baseline in the same batch |
| `wakeBisect` `?noassets=1` and cohort off | 815.4 ms and 978.3 ms, so neither the packs nor the walking cohort |

Conclusion: the frame the owner feels at wake is not page work. On this headless box it is the
offline worker's first-visit cache fill landing during the first twenty seconds: the transient is
one-off per profile and every warm-profile run of the same source is clean. One correction to the
first reading of the entry count, from OfflinePerf: `install` itself precaches 12 entries, and the
270 that a measurement profile accumulates are navigation copies, because the worker's `networkFirst`
handler stores every navigation URL and every probe navigates with a fresh `?cb=`. So the count is a
symptom of many navigations in one profile, not one 270-entry install burst; what the bench actually
catches is the worker writing cache entries while the bench runs. The exact vendor-internal mechanism
(which thread is starved) was not named; the trace shows the renderer main thread did nothing on the
stall frame, so it is not a JS task. On a real GPU the same class of one-off cost is pipeline state
creation at a new camera framing, which is why the owner's number is larger than this box's.

No warm-up machinery was added for it: the trace proves no compile or upload to remove.

## 2. The window is real work now: per-pool slices, measured per frame

The cycle 29 first attempt (a single cap of `ceil(PED_CAP / 30)` = 24 slots per frame on every pool)
was rejected: at 180 fps the pools' own 3.2/s ease is already under 24 slots per frame, so the cap
never bound (`heldSlots` 0), the small car pools finished long before 30 frames, and the car pools
kept the counts they had while the dream froze them, so the one frame that put the fleet meshes back
revealed all of it. A frame counter that reaches 30 is not evidence of 30 frames of work.

What `src/agents.mjs` does now, per pool (ring, avenue, spur, expressway, buses, pedestrians):

- At the sleep edge `captureResyncSpans()` records each pool's drawn population, before the walkers
  are hidden.
- At the awake edge `beginResync('wake')` drops every pool to zero, and the same `update()` then
  walks the pools and admits the first slice, so the first awake frame draws about a thirtieth of
  each pool rather than the fleet (the recorder's frame 1 measured 16 slots on desktop, 8 on the
  phone). Each pool's own step hides its tail on the way and re-enters slots in index order, and a
  slot above the slice keeps the zero matrix. The pre-dream arcs are kept in the pool arrays, so an
  admitted slot resumes where it was parked instead of teleporting.
- Inside the window a pool advances by exactly `max(sleep span, target) / RESYNC_FRAMES` slots per
  frame, clamped at its target, and never by a rate that depends on `dt`. A two-bus pool and a
  seven-hundred-walker pool both arrive on frame 30. The drawn count is fractional; `admitted` is the
  change in its ceiling, so a pool whose slice is under one slot still reports one slot as it crosses
  and can report more than a thirtieth of itself on that frame (the buses: span 2, step 0.067,
  `admitted` 1 on frame 1 and nothing until frame 16, while the fractional count still arrives at
  frame 30).
- `endResyncFrame()` runs after every pool has stepped, so the frame the budget names is a frame that
  really carried work; the window closes after frame 30 once every pool has arrived, with a hard stop
  at 60 frames for a target that keeps moving under it.
- A backwards data clock and a live `setDataModel` open the same window without emptying anything:
  the rate is sliced, the population is not, because a data change is not a sleep. Under a sleep
  neither one opens a window at all (`beginResync` returns when `kind !== 'wake'` and the district is
  dreaming), and an open window counts no frames while the district dreams, so no idle 30 is ever
  recorded.
- No allocation: five fixed arrays and one object mutated in place. Outside the window the pools do
  the same `easeTo` they did before, behind one integer compare.

### 2a. Desktop 1600x900 dpr 1, `wakeSlice(8731)` (`.cycles/reseed30.js`)

One page, one sleep, one wake, the recorder sampling `window.localtimeReseed` after each frame:

```
report kind wake frames 30 framesLeft 0 lastFrames 30 open false settled true windows 1
span   [10, 76.87, 4, 8, 2, 85.71]      (what the sleep took; ring, ave, spur, exp, bus, peds)
step   [3.33, 2.56, 0.47, 1.17, 0.067, 4.47]
admittedTotal [100, 24, 14, 35, 2, 134]  (what the pools actually brought back, = their targets)

frame  1  ad [4, 3, 1, 2, 1, 5]  cum 16   veh  25  peds  5  calls 106  tris 374,716
frame 15  ad [4, 0, 1, 1, 0, 4]  cum 169  veh 117  peds 67  calls 110  tris 380,519
frame 30  ad [3, 0, 0, 1, 0, 4]  cum 309  veh 192  peds 134 calls 110  tris 389,098
settled                             veh 241  peds 145                 tris 389,582
```

Every pool admits on frame 1 and the frame after it and the frame on which the window closes; the
largest pool (pedestrians, step 4.47) admits 5, 4, 4 on frames 1, 15, 30. The fleet grows
monotonically from 25 vehicles to 192 over the window, and the walkers from 5 to 134, which is the
repopulation the owner asked to see spread.

### 2b. Phone 412x915 dpr 2.625 touch, `wakeSlice(8731, {..., throttle: 4})`

```
report kind wake frames 30 framesLeft 0 lastFrames 30 windows 1
span   [10, 10, 4, 8, 2, 65.73]
step   [0.67, 0.33, 0.20, 0.50, 0.067, 2.53]
admittedTotal [20, 10, 6, 15, 2, 76]

frame  1  ad [1, 1, 1, 1, 1, 3]  cum   8  veh 19  peds  3  calls 47  tris 165,957
frame 15  ad [0, 0, 1, 1, 0, 2]  cum  66  veh 43  peds 38  calls 47  tris 168,797
frame 30  ad [0, 0, 0, 0, 0, 2]  cum 129  veh 70  peds 76  calls 47  tris 171,321
frames 27 to 30                 cum 118, 122, 127, 129, so the last four frames each carry work
settled                          veh 70  peds 81                 tris 171,541
```

The buses on the phone profile are the sharp case: two slots, step 0.067, admitted on frame 1 and on
frames 27 to 30 rather than in one frame. Calls never move (47) and triangles only rise with the
population that is actually visible.

### 2c. The same arithmetic, run from the source text with no page

`.cycles/slice30.mjs` slices the resync block out of `src/agents.mjs` verbatim, declares the six pool
draw counts as stubs and steps frames at 1/180 s, so the shipped code (not a copy of its formula) is
checked against three shapes:

```
desktop shape, spans [10, 76.87, 4, 8, 2, 85.71], targets [100, 24, 14, 35, 2, 134]
  step    3.333, 2.562, 0.467, 1.167, 0.067, 4.467
  f1 ad [4 3 1 2 1 5]  f15 ad [4 0 1 1 0 5]  f30 ad [3 0 0 1 0 4]  totals 100,24,14,35,2,134
  PASS closes on frame 30, frame 1 and frame 30 both carry work, every pool reaches its target
  (identical to the page numbers in 2a, which is the point: the harness and the browser agree)

full pool and a single bus, spans [700, 0, 0, 0, 1, 700]
  f1 ad [24 0 0 0 1 24]  f15 ad [23 0 0 0 0 23]  f30 ad [23 0 0 0 0 23]  totals 700 / 1 / 700
  PASS closes on frame 30: one bus and seven hundred walkers both spread over thirty frames

moving target (congestion under the window), spans [300, 40, 10, 20, 2, 400]
  closes on frame 35, inside the 60-frame hard stop, with every pool at its current target
  PASS
```

`node .cycles/slice30.mjs` prints those lines and exits non-zero on any failure.

### 2d. What the window costs

`admitted` counts real slot entries per frame, so the per-frame work of the burst is now bounded by
the largest pool's step (4.47 walkers per frame on desktop, 2.53 on phone) instead of the whole
population. The one-off cost is the wake frame's tail hide, which is the same work the pools did
before, just done once at the edge.

## 3. A steady frame is the pre-change frame

The cycle 29 A/B switch (`setResyncFrames`, `setEnabled`, `setFrames`) has been deleted: with the
per-pool slice there is no cap to demonstrate, and a test-only toggle does not belong in the shipped
module. What replaces it as evidence:

- The closed-window path is literally the old expression: `rampSlots` returns `easeTo(cur, target,
  3.2, dt)` behind one boolean, so a steady frame does the same arithmetic plus one `if`.
- The steady-frame numbers on this build (`benchState`, section 4) are 180 avg / 175.4 p1Low / 5.8 to
  6.4 ms max, the same as cycle 26's 180 / 175.4 / 6.8 on the pre-change tree, and the settled state
  table below matches cycle 26's calls exactly (105 / 55 / 54 / 103 / 91).
- The wake frame itself is no longer over budget: `wakeStall(8731)` after the change, 3,343 frames,
  max 12.5 ms, 0 frames over 33 ms.

## 4. Benches and the state table, gate arithmetic

`benchState` from `.cycles/measure.js`, `browser.close({all:true})` and zero other tabs before every
run, one bench per fresh tab, tab closed after. Raw JSON:

```
desktop 1  {"avgFps":180,"p1LowFps":175.4,"maxFrameMs":6.4,"calls":112,"triangles":397616,
            "mode":"desktop","phaseMaxMs":[0,5.7,5.9,6.4,6.4,5.7,5.8],
            "phaseFirstMs":[0,0,5.6,5.6,5.5,5.5,5.6]}
desktop 2  {"avgFps":180,"p1LowFps":175.4,"maxFrameMs":6.1,"calls":112,"triangles":397616,
            "mode":"desktop","phaseMaxMs":[0,6.1,5.9,5.7,6,5.8,5.7],
            "phaseFirstMs":[0,0,5.5,5.5,5.5,5.6,5.5]}
desktop 3  {"avgFps":180,"p1LowFps":175.4,"maxFrameMs":5.8,"calls":112,"triangles":397616,
            "mode":"desktop","phaseMaxMs":[0,5.7,5.8,5.7,5.7,5.8,5.8],
            "phaseFirstMs":[0,0,5.5,5.5,5.6,5.6,5.5]}
phone4x    {"avgFps":179.9,"p1LowFps":175.4,"maxFrameMs":14.1,"calls":57,"triangles":170508,
            "mode":"mobile","phaseMaxMs":[0,5.7,5.7,14.1,6.1,5.7,5.9],
            "phaseFirstMs":[0,0,5.6,5.5,5.5,5.5,5.6]}
```

Desktop p1Low 175.4 >= 100, no phase maximum over 33 ms. Phone4x p1Low 175.4 >= 60, max 14.1 ms.
An earlier run of this same build (17:31 CET) was taken while another agent's captures were in
flight and reads `maxFrameMs 17.7` with a 14.2 ms phase 4; it is reported here for completeness and
excluded from the conclusion.

### 4b. The same bench, same bytes, with the offline cache mid-install (the gate's other face)

Three runs on the shipped bundle within ten minutes, the only difference being the service worker's
cache state, all with zero other tabs:

```
cache 36 of 270 entries, install in flight
  {"avgFps":126,"p1LowFps":87.7,"maxFrameMs":311.1,"calls":112,"triangles":397540,
   "phaseMaxMs":[0,17.4,5.9,5.7,11.6,311.1,16.7],"phaseFirstMs":[0,0,5.6,5.6,5.5,22.2,11.1]}
cache mid-install, a second profile state
  {"avgFps":125.5,"p1LowFps":89.3,"maxFrameMs":1016.1,"calls":112,"triangles":397244,
   "phaseMaxMs":[0,8.3,5.8,5.7,7.8,300,1016.1],"phaseFirstMs":[0,0,5.6,5.6,5.6,5.5,11.1]}
worker settled (no install running)
  {"avgFps":180,"p1LowFps":175.4,"maxFrameMs":5.9,"calls":112,"triangles":397616,
   "phaseMaxMs":[0,5.7,5.9,5.7,5.8,5.7,5.7],"phaseFirstMs":[0,0,5.4,5.4,5.4,5.6,5.6]}
```

Same source, same calls, same triangles: the only variable is whether the offline worker is writing
cache entries. Note the corrected reading from section 1: the entry count in these profiles is
inflated by `networkFirst` navigation copies from repeated `?cb=` loads, so "36 of 270" and "settled"
describe a worker mid-write against a worker idle, not the six pack precaches. That is the causal
pair for section 1, taken in the bench itself. The desktop gate ("no frame over 33 ms after warmup")
therefore passes on a profile whose worker is idle and fails on one whose worker is writing, and no
page-side change can fix it without moving the work, which Main ruled out. OfflinePerf owns the fix
and is measuring the cold and settled cases with the cache state recorded.

`measureStates(8731, {})`, cycle 29:

| state | calls | triangles | vehicles | peds |
| --- | --- | --- | --- | --- |
| desktop day | 105 | 388,476 | 69 | 78 |
| desktop street | 105 | 389,268 | 47 | 96 |
| desktop night | 103 | 378,196 | 68 | 78 |
| desktop dream | 91 | 323,160 | 86 | 0 |
| desktop noassets | 55 | 322,291 | 70 | 78 |
| phone day | 54 | 168,556 | 49 | 66 |

Against the cycle 26 end state (day 105 / 372,040, fallback 55 / 325,189, street 105 / 372,832,
night 103 / 343,536, dream 91 / 288,522, phone 54 / 166,344): **calls are unchanged in every row**
and triangles move by the art the same build carries (the world agent's cycles 26 to 28), not by
this change. The resync window adds no mesh, no draw call and no instance.

`wakeStall(8731)` after the change: 3,343 frames, **max 12.5 ms**, wake window max 12.5 ms, sleep
window max 8.5 ms, dream edges 6.1 / 5.6 / 5.9 ms (first frame of each new state), 0 frames over
33 ms.

Before/after at a glance: `wakeStall` max 936.5 ms (fresh profile, stall at the wake edge) and
980.1 ms on a second page, against 12.5 ms after; every warm-profile run of the *pre-change* build
was already 6.3 ms, so the large drop is the offline-cache install leaving the measurement window,
not the slicing. The slicing's own effect is the per-frame bound in section 2.

## 5. The draw-call line

Measured, not copied: the desktop day frame is **105 calls / 388,476 triangles** and the
`?noassets=1` fallback is **55 / 322,291**. The 105 decomposes exactly, counting the live scene
graph (`calls29.js`):

```
day (assets):   51 world meshes + 24 agents meshes = 75 scene draws
                24 of those meshes cast shadows -> 24 depth-pass draws
                 6 post passes (bright, two blurs, two half-res blurs, composite)
                = 105, which is the measured call count
fallback:       25 + 14 = 39 scene draws, 10 shadow casters, 6 post = 55
```

The under-80 target was **not met**. Getting there needs 26 calls off the day frame, and the only
two levers are structural: drop the desktop directional shadow map (24 depth-pass draws, and the
contract's day lighting depends on it), or merge the fleet's per-model pools into fewer meshes
(which loses the per-model materials that make the pack cars read as different vehicles) plus the
world's 51 static passes that belong to `world.mjs`, which this cycle does not own. Neither is
bookkeeping: both change what is drawn. `src/CONTRACT.md` now carries the measured pair and this
breakdown, so the header and the measurement agree.

## 6. What was changed

- `src/agents.mjs`: the window (`RESYNC_FRAMES`, `RESYNC_MAX_FRAMES`, `captureResyncSpans`,
  `beginResync`, `rampSlots`, `endResyncFrame`, `reseedReport`), applied to the six pool ramps
  (`rampSlots(RESYNC_RING, ...)` and so on); the sleep edge records the spans, the awake edge empties
  the pools and opens the window, a backwards data clock and a live `setDataModel` open it without
  emptying; the counter is advanced after the pools step. Report on `agents.reseed` and
  `window.localtimeReseed`, read-only. No test-only control ships.
- `src/CONTRACT.md`: the draw-call line replaced with the measured pair plus the decomposition; the
  wake window documented in the `agents.mjs` section and in the `Agents` api block.
- `CHANGELOG.md`: one entry for this cycle.
- Nothing else: `src/world.mjs`, `src/dream.mjs`, `src/data.mjs`, `src/simday.mjs`, `src/camera.mjs`,
  `src/post.mjs` and `screenshots/` are untouched, and no git command was run.

`node tools/assemble.mjs`, `node tools/check.mjs` (38/38), `node .cycles/gate.mjs` PASS at the end.
The world was in flux for part of these runs (another agent edited `src/world.mjs` in cycles 26 to
28); every number above is from a tree that `assemble.mjs` had just rebuilt, and Main re-verifies on
the final tree.

## 7. Open, not fixed here

- The offline worker's cache-fill frame (sections 1 and 4b). It is the only variable that moves the
  same bytes between p1Low 175.4 / max 5.9 ms and p1Low 87.7 / max 311 ms, so it is a real gate
  risk, but the mechanism is not the one this item first assumed: `install` precaches 12 entries,
  and the 270 are `networkFirst` navigation copies from repeated `?cb=` loads, which a normal visit
  does not produce. The ownership and the fix belong to OfflinePerf, who is measuring the cold and
  settled cases with the cache state recorded; this item only reports the correlation and the
  numbers.
- The desktop day frame is 105 calls with the packs; under 80 needs an art decision (section 5).
