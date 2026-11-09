# Cycle 27 brief: nothing floats (OWNER-NOTES 2026-11-09 14:00, item 3)

Owner words, binding: "Nothing floats: the big isolated boxes at left, right and bottom centre
(depot, warehouses, sheds) must sit in a real block with a street, a car park and kerbs, or be
removed. Zero objects dropped on bare ground."

## File ownership

`src/world.mjs` only, plus the smallest accurate note in `src/CONTRACT.md` if a documented rule
changes. Do not touch data, camera, dream, agents, `main.mjs` or any capture.

## What "floating" means here, measurably

Before changing anything, find the objects that are on bare ground. Two probes already exist and
are loaded by `%load`; use them rather than inventing a new method.

1. `.cycles/float25.js` `structureScreen(8731)` projects every published structure
   (`world.fabric.civic` plus the rail depot) to screen coordinates and writes the record list
   `screenshots/iter-NN-boxes.json`.
2. `.cycles/isolate27.py <capture>.png <that json>` measures the apron around each projected box:
   asphalt, pavement and grass share. A structure whose apron is 40 percent grass or more with
   under 8 percent asphalt reads as dropped in a field.

Baseline I measured on this build (`iter-25-day.png`, hour 17, district tier):

    block     mart      spur1  ( 166.8, -44.6)  asphalt 12.4%  pavement 0.1%  grass  8.6%
    FLOATING  fuel      ring   (-112.6, -23.4)  asphalt  6.2%  pavement 0.8%  grass 48.4%
    block     school    spur3  (-176.2, -52.0)  asphalt 12.2%  pavement 0.0%  grass 39.3%
    block     sport     spur3  (-218.3, -63.5)  asphalt  0.8%  pavement 0.9%  grass 21.4%
    block     warehouse ring   (  67.5, 120.7)  asphalt 19.5%  pavement 0.2%  grass 33.2%
    block     warehouse ring   (  87.4, 109.3)  asphalt 15.6%  pavement 0.2%  grass  8.0%
    block     depot     rail   ( 128.0, -78.0)  asphalt 27.7%  pavement 0.0%  grass  0.0%

`world.fabric` also reports `lots: 76, cap: 76, offStreet: 0`, so every civic plot believes it
reached a kerb; the probe says the petrol station still stands in grass, and four of the six have
almost no pavement in their apron (0.0 to 0.8 percent), which is the "nothing looks kerbed" half of
the owner's complaint. Treat both halves as the defect: no structure on bare ground, and the ones
that stay must have a kerb and pavement along their frontage, not only tarmac.


Then, per failing structure, do one of exactly two things:
- Give it a real block: a short street stub joining it to the nearest carriageway, kerbs and
  pavement along its frontage, and a marked car park or working apron filling the frontage. The
  block must read from district tier (kerb line, parking rows, a street that continues).
- Remove it.

No structure may end the cycle standing on bare ground, and no civic or depot building may be
left with an apron that stops in mid-air over the hillside.

## Budget and rules

- Draw calls must not increase (the frame is at 105 with assets, 55 without; the district is the
  binding state). Prefer the existing merged static/paint/detail meshes and the `frames` quad mesh
  over a new mesh. Triangles: the day frame is 380,516 against a 390,000 working line and a
  400,000 hard gate, so there is about 9,000 to spend, and any spend must be paid from a same-page
  A/B that proves it.
- `?noassets=1` must stay coherent: any new static geometry lives in `statics` (merged) so the
  fallback gets it too, unless it is detail that the fallback's ladder does not buy.
- No per-frame allocation, no em dash (U+2014), no new dependency, nothing outside this folder.
- Asset-mode gating stays exactly as `CONTRACT.md` describes for quad details.

## Verification you must run and paste

1. `node tools/assemble.mjs`, `node tools/check.mjs`, `node .cycles/gate.mjs` (PASS).
2. Captures via the helpers in `.cycles/c24.js` (`c24Open`, `c24Wait`, `c24Shot`; `tab.screenshot`
   with a path writes NOTHING here) at 1600x900 hour 17:
   `iter-27-day.png`, then `rig.setTier('street')` and the four rotations
   `iter-27-street-r0..r3.png`, plus `iter-27-noassets.png`.
3. The same-page A/B for calls and triangles, and `python .cycles/audit19.py judge` on the day
   capture before and after: lower third luma, edge, and the four corner flat/edge numbers must
   not regress (worst flat at or under 49.5, worst edge at or above 6.05 as measured in cycle 24).
4. A list of the structures you touched, their coordinates, and which of the two actions each got.

## Two measurement rules learned this cycle, follow them exactly

1. **Close every browser tab before a bench.** `await browser.close({ all: true })` then run
   `benchState(8731, {})`. A shared headless browser with two dozen managed tabs still animating
   reported this build at avg 168 to 172, p1Low 42 to 59 and 33 to 39 ms frames; the same page
   alone measures 180 / 175.4 / 6.8. Any bench with another tab open is a measurement of the
   browser, not of the build.
2. **Corners of this build, from the cycle 26 captures:** worst flat 48.9, worst edge 6.09
   (floors 49.5 and 6.05). Lower third 0.333 luma / 27.7 flat / 11.98 edge. Use these as the
   regression reference, and capture the four rotations on the same quiet machine.

Day frame now: 105 calls, 372,040 triangles; fallback 55 calls, 325,189 triangles. That is 17,960
under the 390,000 working line, so a modest amount of new frontage/paving can be afforded, but
every spend still needs a same-page A/B pair.
