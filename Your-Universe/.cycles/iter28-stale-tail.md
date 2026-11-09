## Cycle 28, owner item 4: the ring stops reading as a wall

The owner named two crossings. Two now exist, plus the third that the south-west quadrant needed,
and each carries a frontage row on both kerbs out past the frame edge.

- `SPUR_PTS[1]` (east, to the rail yard) runs to (330, -76) and its row is continuous from the
  ring's pavement to the frame edge. The rail corridor is windowed at the crossing: ballast, rails
  and sleepers stop at a paved goods apron (`RAIL_APRON_HALF` 26) with buffer stops, so the row
  stands on the apron instead of on fifteen metres of ballast.
- `SPUR_PTS[0]` is the south-west crossing, the west arterial's own line turned down the valley
  along the deck's north flank. It replaced the old west arterial rather than standing beside it:
  the corridor between the deck and `SPUR_PTS[3]` is 58 m at its widest, and one fronted street
  needs 31 m of deck clearance plus 20.25 m of row, so two streets do not fit.
- `SPUR_PTS[3]` (westLower) runs to (-338, -70) and its row reaches the frame edge.
- Each side of a street draws its own lot limit: one limit for both kerbs let the first kerb eat
  the share and left the second empty. Outer parcels are laid 2.1x (crossings) and 1.5x (the
  remaining three arterials) wider than the district's, which halves the houses the frame pays for
  without breaking the row.
- `fabricTail` publishes where the outer fabric begins in the per-building batches
  (`fabric.first`, `fabric.tail`), so item 4's cost can be read in place: clamping the three tail
  counts on one page takes the day frame from 385,008 to 377,646 triangles with the draw calls
  unchanged at 105, i.e. the outer fabric is 7,362 triangles of the frame.
- The pack villa kits are gone from the outer fabric: four villa lots cost three draw calls and
  about 9,700 triangles on top of the procedural blocks they replaced, which the crossing rows
  need and which the draw-call cap does not allow.
- The flourish pool keeps 4 percent fewer rank-2 pieces (`DETAIL_BUDGET` 6,540) to hold the
  working line: day frame 385,404 triangles against 390,000, fallback 324,031 against 325,000.

Measured, `spurFrontage(8731)`, 10 m bins on the on-screen stretch, buildings within 26 m of each
kerb (before -> after):

| spur | crossings | on-screen bins | empty | one-sided | last bin with BOTH sides |
| 0 | southWest | 21 | 3 (arcs 45, 55, 205) | 7 | 195 of 205 |
| 1 | east | 24 | 0 | 1 | 235 of 235 |
| 2 | north (crest) | 6 | 1 (arc 55) | 4 | 45 of 55 |
| 3 | westLower | 24 | 0 | 4 | 235 of 235 |
| 4 | hillside | 25 | 0 | 10 | 245 of 245 |

`node .cycles/gate.mjs` PASS. State table: day 105 calls / 385,404 triangles, street 105 / 386,196,
night 103 / 367,866, dream 91 / 312,706, noassets 55 / 324,031, phone 54 / 168,600.

### Cycle 28 repair: the three gates item 4 broke

- **Outer blocks are three to five floors**, not five to eight. The crossings put them in the near
  corner crops and against the sky at the top edge, where the taller slab read as a flat screen
  (r2 bottom left measured 52.9 flat / 4.52 edge on the reviewer's capture) and where a storey is
  a window row, a sill and a storey band on the frame's most expensive row of houses.
- **The ring's frontage is back**: `fabric.streets.ring` 2 -> 10 lots, paid from the same tail.
- **The pack villa kits are back** on the outer slopes: two lots, **one** kit
  (`MODEL_VILLA_MAX` 2, `villaSet` length 1), restricted to the near-slope side of the ring
  (`cz <= 40`) which is where the phone frame and the corner crops see them. One kit is two draw
  calls; the trade is `MODEL_CORE_VARIANTS` 5 -> 4, one apartment facade kit fewer, so the budget
  is unchanged.
- Budget after the repair: day **105 calls / 388,650 triangles**, street 105 / 389,398, night
  103 / 377,436, dream 91 / 322,386, noassets 55 / 322,673, phone 54 / 168,556.
  `fabric.streets` = ring 10, westLower 48, east 46, southWest 25, hillside 50, north 2; 181 lots.
  `world.models` = 42 core lots + **2 villas**, five buckets.
- `node .cycles/gate.mjs` PASS.

### Cycle 28 repair, second pass: measured state

Budgets after the repair, all from `c24Stats` on the built page, cache settled (one cache,
`localtime-assets-ma3977eae`, 39 entries):

| state | calls | triangles |
|---|---|---|
| desktop day (district, h17) | 105 | 388,124 |
| desktop `?noassets=1` | 55 | 322,155 |
| phone (412x915, dpr 2.625, touch) | 54 | 168,424 |

`gate.mjs` PASS. `fabric.streets` = ring 8, westLower 48, hillside 49, east 49, north 2,
southWest 26; 182 lots; `models.villas` 2; `fabric.corners` 2.

Corner audit, four street poses, floors flat <= 49.5 and edge >= 6.05: thirteen of sixteen
crops pass. The failing frame is the pose-2 street view, bottom left `53.0 / 4.62` and bottom
right `53.1 / 6.13`.

Isolate pass at that exact pose, one page, only visibility changed:

| state | bottom left | bottom right |
|---|---|---|
| base | 53.0 / 4.49 | 53.3 / 6.04 |
| `buildings` hidden | 39.6 / 6.80 | 48.5 / 6.33 |
| windows, frames, details, roofs, trees, contact hidden | 75.5 / 2.81 | 69.9 / 4.61 |

The buildings own the failure; the detail layers are the only articulation the wall has, so
removing them makes the tile worse on both axes. A furniture pass cannot fix this region.

Pose identity: `rig.rotate` advances `yawStep` by one per call and drops a second call of the
same direction inside 30 ms, so poses are exact quarter turns only when the calls are spaced.

Change kept from this pass: window panes draw in two tones (`LIT_PALE`, `LIT_DIM` x 1.55, 36
percent of panes by the same hash the night halo uses). Measured at the failing pose: bottom
left edge 4.49 -> 4.62, bottom right 6.04 -> 6.13 (crosses its floor), flat unchanged, zero
triangles and zero draw calls. Top band on the fresh day capture: row 150 = 1.76 against a 1.6
floor, streaks 0.35 percent, mud 1.46 percent, rows 0-120 at 0.04 / 0.29 / 0.29 / 0.11 / 0.30.

Benches, three desktop runs with all tabs closed and the cache settled: p1Low 175.4 and max
frame 19.3 ms, then 175.4 and 15.3 ms, then 89.3 and 294.4 ms. The third run caught the same
phase-6 spike WakePerf documented for an install in flight, so the honest gate number is the
clean pair (175.4 / 19.3 and 175.4 / 15.3). Phone bench: avg 179.9, p1Low 175.4, max 15.1 ms.

### Cycle 28 repair, exposed facades pass: measured, and what is still open

All numbers below are from captures in `Your-Universe/.cycles/`, absolute paths, SHA-256 printed
by `.cycles/fx28aud.py` in the same run. The failing frame is the **north** street pose: rig
`camera.position` `[0, 88.4, -87.1]`, which is the file `screenshots/iter-28-street-r1.png`
(`c5db926bc3be687b`) even though the pose index is 2 in a probe that starts at r0 = south. The
filename is not the pose; the camera position is.

Build measured (bundle assembled from this source, `node tools/assemble.mjs` 23/23):
day **388,990** triangles / 105 calls; street poses 389,826 (r0), 391,368 (r1), 391,526 (r2),
392,416 (r3); 3,020 panes; `details.blindBands` 92.

| file | sha256[:16] | TL | TR | BL | BR |
|---|---|---|---|---|---|
| `fx28-v10-r0.png` | `d4b2e0157440675b` | 28.6 / 14.01 | 15.7 / 15.78 | 28.8 / 11.95 | 40.6 / 7.02 |
| `fx28-v10-r1.png` | `21ac7d6c17495399` | 22.4 / 19.40 | 44.0 / 8.70 | 47.5 / 8.09 | 36.1 / 9.28 |
| `fx28-v10-r2.png` | `feb914fbf9016a88` | 46.8 / 8.76 | 32.4 / 17.56 | **52.0 / 4.71** | **51.6 / 6.26** |
| `fx28-v10-r3.png` | `c657ff66d3eca0f7` | 43.6 / 9.33 | 29.8 / 20.30 | 42.1 / 7.72 | 39.3 / 9.28 |

`STREET WORST: flat 52.0 (gate <= 49.5) edge 4.71 (gate >= 6.05) FAIL`. The north frame is
still two and a half flat points and one and a third edge points off its floor. Every other
rotation passes on this build.

**Which buildings own the crop.** Hiding one building instance at a time on the same page, same
pose (`fx28-hide-*.png`, same audit):

| hidden instance | position | footprint | BL flat / edge |
|---|---|---|---|
| none (base, `1ab8947432a441b0`) | | | 52.0 / 4.71 |
| 200 | (41, -44) | 4.3 x 9, h 16.7 | **47.2 / 5.94** |
| 51 | (26, -44) | 4.8 x 10.1, h 13.8 | 50.5 / 5.07 |
| 199 | (37, -40) | 4.3 x 7.1, h 13.7 | 51.2 / 4.88 |
| 103 | (47, -51) | 4.1 x 9.4, h 13.6 | 51.5 / 4.74 |
| 18 | (35, -50) | 3.3 x 11.3, h 10.8 | 51.9 / 4.61 |
| 19 | (22, -51) | 3.3 x 9.1, h 10.8 | 52.4 / 4.57 |
| 17 | (71, -44) | | 52.0 / 4.71 |

So the corner is not a diffuse field of blank walls: **one 4.3 x 9 m block is 4.8 of the 52
points**, and five blocks are about seven. Their visible faces are a 9 m flank and a 4.3 m rear,
and neither took a pane: the side grid gives a flank two bays on every second floor (six panes),
and the pass treated `floors - 1 = 5` panes as "furnished"; the rear was under the pass's 5 m
width floor. Both conditions are the bug, and both are changed in the source now: a face counts
as furnished only at one pane per storey, and the width floor is 3.4 m.

**What did not work, with the numbers** (all same pose, same audit):

- 5,154 panes, threshold 2,000 px (`fx28-diag-panesmax.png`, `b7d7da67df695597`): BL 52.5 / 4.70,
  BR 48.3 / 5.18. Covering more wall with panes makes the corner worse, not better: two 1.35 m
  panes fill over half of a 4.3 m rear, and a pane's interior is as flat as the stucco it replaces.
- Storey lines with the eye-facing test (`fx28-v9`, `c2cd6cabfc9daddf`): BL 52.0 / 4.71, i.e. the
  same as without the test. Painting the lines magenta and moving them 0.5 m proud
  (`fx28-mag2-r2.png`) changed **32,580 pixels in TL and TR and zero pixels in BL or BR**: the
  lines land in the far field only, because the near row's flanks and rears are party-walled and
  the pass skipped them. They are now laid from the height at which the wall clears its
  neighbour's roof (`clearWallBase`) instead of being skipped, which is the wall a camera in the
  street actually sees.
- Rank-2 detail pieces sorted by five-frame visibility instead of build order (`fx28-v8`,
  `3cd973e597a34594`): BL 52.0 / 4.71, BR 50.6 / 6.21. The pool's budget is already spent on the
  big rank 0/1 pieces (roof slabs, parapets, plinths), which are what the corner reads; the
  flourish tail is not the missing structure.

**Open, and deliberately unmeasured.** The three changes above (pane eligibility, 3.4 m width
floor, storey lines from the clear base) are in `src/world.mjs` and the bundle is assembled and
verified, but **no capture of that exact build is quotable**: the served page came back from the
service worker's cached shell (`world.sparseWalls` absent and every pose's triangle count
identical to the previous build), which is the same stale-file trap this cycle opened with, now
on the served side. The next pass must bypass the worker (or wait for its cache work to settle)
before any corner number is recorded. The acceptance line stays: north pose BL flat <= 49.5 and
edge >= 6.05, BR flat <= 49.5, day <= 390,000 triangles, 105 calls, phone <= 180,000.
