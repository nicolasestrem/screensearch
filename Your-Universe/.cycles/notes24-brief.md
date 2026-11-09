# Cycle 24 (spawned 9 November, after cycle 23 closed)

## Cycle24World (src/world.mjs only)
1. Band-to-quad conversion (bands as 2-triangle instances instead of 10-triangle boxes, about
   1,800 reported triangles) plus the same treatment for other one-face box detail where the
   per-face test justifies it. Target street <= 388,000 with corners held or improved.
2. r0 bottom right (50.4 flat) is the only corner above 50: kerb runs, low planting, boundary
   walls, fence lines, scooters, pavement instances. Revert anything under 0.15 movement.
3. District window rhythm out of what step 1 freed; no duplicate awnings or balconies.

## Main's own slice (done, before integration)
### The taken-road dream path: two real bugs found and fixed (src/dream.mjs)
- **Boot forgets a taken road.** `take()` shows the permanent ribbon, its light strip and the
  market in the daylight scene (3 visible dream meshes, measured), but at boot `roadPerm` is
  hidden and nothing restored it from `localStorage['localtime.dreams.taken']`, so after a reload
  the taken road existed only from the next sleep onward. Fixed: boot builds the pair's sentence
  exactly as `buildCardTexts` builds it and, when it is already in the stored array, sets
  drawRange RING, `roadPerm` visible, provisional/ghost/glow hidden, `matGlow.opacity` 1,
  roadMode 2, permKey and `showMarket(true)`.
  Measured: boot with a taken sentence = 3 dream meshes visible, `road-permanent` visible, state
  awake, 108 calls / 390,268 tris; boot with an empty store = 0 visible, 105 / 389,160 (exactly
  the cycle 23 table). Only the taken state pays the +3 calls / +1,108 triangles.
- **A taken pair could re-offer its second hypothesis.** `buildCardTexts` filtered per sentence,
  so a pair with two words would offer the second after the first was taken, against a road
  already permanent. Fixed with a pair-level guard (`roadMode === 2 && permKey === pairKey` ->
  no cards). Verified: with a taken sentence seeded, sleep keeps the permanent road visible and
  shows zero cards at 12 s and 20 s into the dream.

### Phone street tier (previously unverified)
412x915 DPR 2.625 touch: block tier 54 calls / 170,414 tris; after `rig.setTier('street')`
53 / 167,526, i.e. the phone reaches its lowest tier with fewer triangles than its block tier.
Frame audited: luma 0.458, flat 32.8, edge 19.59, warm 70.3, purple 0.0, specks 1. Captured as
`screenshots/iter-23-pixel6-street.png` (+ `-full`).

## Still to do after the world worker lands
Assemble/gate; my states table; corners on my captures; warm/purple/specks; benches median of 3;
dream phases + card rule on the default (untaken) path; captures; finals; log; CONTRACT/README
check.
