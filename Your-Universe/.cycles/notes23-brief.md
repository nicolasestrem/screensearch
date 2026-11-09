# Cycle 23 brief (spawned 9 November, after cycle 22 closed)

Cycle 22 verified state (mine, assembled tree, idle, hour 17): day 101/386,842, street
101/387,634, night 99/347,890, dream 91/297,076, noassets 55/334,763, phone day 54/170,414,
phone night 44/170,279, phone dream 36/159,211; bench peak 395,982 at 108 calls; corners worst
flat 50.5, worst corner edge 5.98 (both better than cycle 20's 55.9 / 5.31); warm 77.5, clipping
1.108, purple 5.5.

## The budget math that drives this cycle
- Street line 390,000, current 387,634: 2,366 spare.
- Hard gate 400,000, bench peak 395,982: 4,018 spare.
- Walker cohort +6,300 (60 x 105) and the 1-in-6 fleet +4,540 cannot both land, and neither fits
  today. So: the world worker must free >= 4,100 triangles (street to <= 383,500), which lets the
  walkers land at ~389,800 street and a ~398,000 peak. The fleet step then still needs its own
  trade, logged as the cycle 24 candidate.

## Cycle23World (src/world.mjs only)
1. Free >= 4,100 triangles using the same class of waste cycle 22 found (no-detail geometry,
   coincident duplicates, per-floor loops to one quad, interior faces, over-tessellation), keeping
   the cycle 22 wins intact.
2. Spend part of it on district-tier facade rhythm (owner item 5: window rhythm, balconies, shop
   awnings at district distance), inside the freed budget.
3. Hold or improve corners: worst flat <= 50.5, worst corner edge >= 5.98, with the r0 bottom
   right (south pavements) as the target corner.

## Cycle23Agents (src/agents.mjs only)
Near-camera walking crowd: one InstancedMesh per pack person variant, cohort <= 60, <= +6,300
triangles desktop, zero extra calls, phone unchanged, facing = walking direction, night/dream
hides mirrored, no skinning, no population change. `MODEL_LANE_EVERY` untouched.

## Integration checklist (mine)
- assemble/check/check-dom/gate; my own stats table; corners on my captures; warm/purple/specks;
  benches median-of-3 both profiles; dream phases and card rule; finals + gallery copies; log.
- Fleet 1-in-6 stays OFF until a cycle trims another ~2,000 and the peak has room.
