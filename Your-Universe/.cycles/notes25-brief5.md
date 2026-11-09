# Cycle 29 brief: performance, the wake stall, and the draw-call line (OWNER-NOTES 2026-11-09 14:00, item 5)

Owner words, binding: "Performance: the 1.0 to 1.1 s frame at dream wake comes from traffic
repopulation in one frame. Spread repopulation over at least 30 frames (time-sliced, no per-frame
allocation). Gate: desktop p1LowFps >= 100 and no frame over 33 ms after warmup, phone4x
p1LowFps >= 60. Set the draw-call line in the header to the measured value (101 desktop with
assets, 55 noassets) or get under 80; the header and the measurement must agree."

## File ownership

`src/agents.mjs` for the repopulation slicing; `src/CONTRACT.md` for the draw-call line and the
documented repopulation rule; `src/main.mjs` only if the bench or the phase marks must name the
new slicing. Do not touch `src/world.mjs` (cycles 26 to 28 own the art), `data.mjs`,
`simday.mjs`, `camera.mjs` or `dream.mjs`.

## What I measured before you start (baseline, pre-cycle-27 art, hour 22, headless)

`.cycles/wake25.js` `wakeStall(8731)`: 3,344 frames recorded across one sleep and one wake.
Max frame **9.8 ms** (first frames after load), sleep window max 6.1 ms, wake window max 6.8 ms,
dream edges `awake to dreaming` 5.8 ms, `dreaming to waking` 5.7 ms, `waking to offered` 5.8 ms.
So a steady page does not show the owner's 1.0 to 1.1 s frame at that moment.

**But the bench does, intermittently, in the wake phase.** Four `benchState(8731, {})` runs on the
same build:

| run | avgFps | p1LowFps | maxFrameMs | phaseMaxMs |
| 1 | 142.6 | 80.6 | 369.8 | [0, 5.7, 5.9, 5.7, 5.8, **369.8**, 19.1] |
| 2 | 180 | 175.4 | 6.1 | [0, 5.9, 6.1, 5.7, 5.7, 5.7, 5.8] |
| 3 | 179.9 | 140.8 | 12.9 | [0, 9.8, 12.9, 11.2, 5.9, 5.8, 7.8] |
| 4 | 180 | 175.4 | 6.2 | [0, 5.7, 6.2, 5.7, 5.7, 6, 5.8] |

Phase 5 is the 14 to 17 s phase, the one that sleeps and wakes. Runs 2 and 4 stayed clean, so the
stall is one-off per page life, which points at first-time GPU work on the meshes the dream hides
and the wake reveals: a program compile and a buffer upload for a fleet that has never been drawn
in that page. Treat that as the first hypothesis to prove or kill, then fix what the trace shows.

## The mechanism to fix

`agents.mjs update()` skips every road step while `dreaming` (`stepRingCars`, `stepAvenueCars`,
`stepSpurCars`, `stepExpressway`, `stepBuses`, `stepTrains`, `stepTram`). The data clock keeps
advancing through the dream, so the first non-dreaming frame has to bring every schedule and every
pool instance up to the new `tData` at once: the cars are not merely moved, they are re-seeded and
their buffers re-uploaded in one frame. That is the repopulation the owner measured, and it is a
one-frame burst by construction.

Requirements for the fix:
- Reseeding after the dream (and after any `tData` jump larger than one frame's worth, including
  the wrap at `t1 + 1 h` and a `setDataModel` schedule rebuild) is spread over **at least 30
  frames**: a frame budget counter that advances a deterministic slice of the fleet per frame and
  keeps the untouched slots exactly as they were, so nothing pops or teleports. Cars that are not
  yet reseeded keep their last valid position, which is what a parked district looks like.
- No per-frame allocation: no closures, no array literals, no `Math.random`, no new objects in the
  sliced path. Preallocate the slice cursors and per-pool markers at factory scope.
- The slicing must not change what a steady frame draws: after the burst window the frame must be
  identical to the unsliced build (same calls, same triangles, same vehicles count).
- The dream edge itself stays a single visibility flip; do not add per-frame work to the awake
  path.

## Measurements you must run and paste

1. `node tools/assemble.mjs`, `node tools/check.mjs`, `node .cycles/gate.mjs` PASS.
2. `.cycles/wake25.js` `wakeStall(8731)` before and after: `maxMs`, `wakeWindow.max`, and the three
   dream-edge windows, plus the number of frames the reseed took (log the counter once from the
   page, for example through a read-only `agents.reseed` report; publish it for the harness and
   keep it honest).
3. `benchState(8731, {})` twice and `benchState(8731, { width: 412, height: 915, dpr: 2.625,
   touch: true, throttle: 4 })` twice, from `.cycles/measure.js`: report `avgFps`, `p1LowFps`,
   `maxFrameMs`, `phaseMaxMs`, `phaseFirstMs`, `calls`, `triangles`. Gate: desktop `p1LowFps`
   >= 100 with no frame over 33 ms after warmup; phone4x `p1LowFps` >= 60.
4. The state table from `measureStates(8731, {})`: day, street, night, dream, noassets, phone day.
   Calls and triangles must not rise; report the deltas.
5. A same-page A/B proving the sliced path costs nothing on a steady frame.

## The draw-call line

Measure it, do not copy the owner's numbers blindly: the desktop day frame reads **105 calls**
(assets) and the fallback **55**. The owner asked for the header to carry the measured value or for
the build to get under 80. Decide with the numbers:
- If you can genuinely merge calls (the walking cohort's 4, the pack fleet's per-model meshes, the
  parked fleet's ten) without losing required behaviour, do it and report what each merge cost.
- Otherwise set the header line to the measured pair, and say plainly in the cycle notes that the
  under-80 target was not met and why, with the call breakdown.

The header is the `Draw-call budget` line in `src/CONTRACT.md` (currently "40 for the whole frame,
triangle budget 150k"), which is the number the owner reads. It must state the measured value for
both modes and the triangle line, and the cycle notes must show the measurement that proves it.

## Bench protocol, learned the hard way in cycle 26

Always `await browser.close({ all: true })` before a bench run, then run one bench per fresh tab
and close it. A shared headless browser with many managed tabs still animating reported this same
build at avg 168 to 172, p1Low 42 to 59 and 33 to 39 ms frames; with no other tab open the page
measures 180 / 175.4 / 6.8. Report the bench JSONs, not a summary, and say how many tabs were open.

## Cycle 26 end state you start from

Day 105 calls / 372,040 triangles. Fallback 55 / 325,189. Street 105 / 372,832. Night 103 /
343,536. Dream 91 / 288,522. Phone 54 / 166,344. Desktop bench 180 avg, 175.4 p1Low, 6.8 ms max.
Corners: worst flat 48.9, worst edge 6.09.
