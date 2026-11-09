# Localtime masterpiece loop: iteration log

Method note, important: this agent has no image input, so every art judgement here is a
measurement, not a look. Each cycle runs `lt.analyze` (palette buckets by share, 3x3 region
luminance/saturation/edge density, ten horizontal bands of edge density) and `lt.spectrum`
(hue-family shares) on the screenshots, plus in-page probes. Claims about "what it looks like"
are derived from those numbers and from the owner's written verdict.

Gates per cycle: desktop 1600x900 under 80 draw calls and 400k triangles, bench avgFps >= 60,
p1LowFps >= 55, no measured frame over 33 ms after the first 2 s; Pixel 6 (412x915, DPR 2.625,
touch, 4x CPU throttling through CDP) under 45 draw calls and 180k triangles, bench avgFps >= 60,
p1LowFps >= 50, same frame ceiling. Zero console errors. Labels: no two visible labels overlap,
none outside the viewport.

Note on frame rate in this harness: headless Chromium runs unthrottled rAF, so absolute fps reads
about 180 in every configuration. The meaningful numbers are average frame time (16.7 ms is 60
fps) and `maxFrameMs`; `avgFps` is reported because the owner asked for it, and it is honest for
frame cost, not for vsync headroom.

## Cycle 0: baseline (art pass 2, before any change)

Checks: assemble 20/20, check 29/29, check-dom 99/99, check-sample 6/6, smoke-data 120/120,
check-pixels PASS. Zero console errors.

Desktop 1600x900: `{ calls: 33, triangles: 171721, vehicles: 108, pedestrians: 68, shadows: true }`.
Night dream: calls 42, triangles 175165. Pixel 6: calls 22, triangles 102334, shadows false.
Boot: first frame at 173 ms. Bench harness (mine, pre-existing build): 10 s fixed script gave
`avgFps 179.5, p1LowFps 172.4, maxFrameMs 33.7` desktop and `avgFps 178.5, p1LowFps 163.9,
maxFrameMs 140.7` phone.

Flaws, from the owner's written verdict plus measurements of the baseline frames:
1. Closed circular ring road around a packed core, with small gabled cottages outside it: the
   owner's words are "a map from 1993" and "stupid Middle Ages". Structural, so it was not a
   measurement question.
2. One hue family across the whole frame: the eight dominant palette buckets of the day frame were
   not separable into distinct materials, and only 11.6 percent of pixels fell in a green hue band.
3. 1993 shading: the source used `MeshLambertMaterial` for ground and buildings and
   `MeshBasicMaterial` for windows, so nothing had roughness, specular response, contact occlusion
   or long shadows.
4. Phone framing: 8 labels present, district projected into a small box (356x221 px in a 412x915
   viewport) with the rest of the portrait frame carrying nothing.
5. No benchmark mode, no adaptive quality, no `?hour=` pin: performance and screenshots were
   unverifiable, and the wall clock silently decided day or night (the first baseline "day" shot
   rendered at 22:00 local as night).

## Cycle 1: de-medievalise the plan and the shading, instrument everything

Slices: `world.mjs` layout and shading (irregular arterial loop plus three spurs leaving the frame,
elevated expressway on piers with a tunnel portal, contemporary form vocabulary instead of
cottages, small hilltop village, curtain-wall towers, contact blobs, procedural terrain, gradient
sky with sun lobes, `MeshStandardMaterial` everywhere, `setShadowMapSize`/`setDetailLevel`),
`post.mjs` (62 percent sharp band at half the old blur radius, mild bloom, grade, 4x MSAA on
desktop and inline FXAA on mobile, `setResolutionScale`), `main.mjs` (bench, notch ladder,
mobile profile, `?hour=` pin, label declutter, safe areas), `agents.mjs` (300 vehicles and 300
pedestrians, expressway brake-light stream, trains, articulated tram, kerb parking,
`setPopulationScale`).

Checks: assemble 20/20, check 29/29, check-dom 99/99, check-sample 6/6, smoke-data 120/120,
check-pixels PASS. Zero console errors on every load.

Bench JSON:
- Desktop 1600x900, DPR 1: `{"avgFps":179.9,"p1LowFps":175.4,"maxFrameMs":13.5,"calls":51,"triangles":226046,"mode":"desktop"}`
- Pixel 6, DPR 2.625, 4x CPU: `{"avgFps":179.8,"p1LowFps":172.4,"maxFrameMs":23.6,"calls":42,"triangles":134046,"mode":"mobile"}`
- District tier stats: desktop `{calls:46, triangles:223610, shadows:true}`, phone `{calls:37, triangles:131610, shadows:false}`, dream `{calls:56}`.
- Labels: 8 visible, 0 overlaps, 0 offscreen at 1600x900 and at 412x915.
- Boot: first frame 219 ms desktop, 844 ms phone with 4x throttling.

Fixed inside cycle 1: the first bench run reported `maxFrameMs 201.6` desktop and `281.9` phone.
Isolated to `dream.sleep()` (50 ms), because the two floodlights were created with `visible:false`,
so making them visible changed the scene light count and three recompiled every material in the
city. They now stay in the light list from the first frame with `intensity 0`. Re-measured:
`maxFrameMs 13.5` desktop, `23.6` phone, sleep/wake frames 6.0/5.9 ms.

Flaws after cycle 1 (measured):
1. Day frame is still one hue family. The eight dominant palette buckets are all khaki to olive
   (`937c3c` 3.1 percent, `5a5c3c` 2.2, `937c44` 2.0, `9a7d3c` 1.8, `8c7b44` 1.6, `9c834c` 1.6,
   `8d7b3b` 1.5, `9c8244` 1.5). Only 2.8 percent of pixels are darker than luma 0.18 and only
   3.4 percent are cool: no dark asphalt mass, no blue-grey glass, no salmon or ochre accent.
2. Night is grey-green, not the required deep blue. The two dominant night buckets are `323534`
   (18.8 percent) and `2d3332` (17.5 percent), both with blue at or below green, and mean
   saturation 0.15. A deep blue frame needs blue clearly above red across the dominant buckets.
3. Night detail collapses: peak edge density falls from 10.7 by day to 5.9 at night, so the
   streets are not the "ribbons of sodium light" the target asks for, they are soft blobs.
4. Phone portrait wastes its top third: the top region reads luma 0.617 with edge density 0.5 to
   0.6 against 9.2 in the city band, so a third of the phone screen is a featureless bright band.
5. Street tier reveals no more structure than district tier: the centre crop measures edge density
   6.1 to 9.3 against 8.9 to 11.5 at district, and its palette is 13.5 percent pale tan pavement
   plus olive and grey. Zooming in does not produce the readable signs, awnings, zebra crossings
   and parked detail the references demand.

Canonical copies promoted: `screenshots/desktop-day.png` (cycle 1 day), `desktop-night-dream.png`
(cycle 1 dream), `pixel6-day.png` (cycle 1 phone).

## Cycle 2: hue families, deep blue night, a chantier that reads

Slices: `world.mjs` palette and light (facades in 15 hashed slots from ochre to faded blue-grey,
terrain families per 37x31 m cell, darker asphalt and pavements, distant relief built into the
existing height field at zero draw cost plus a bay mesh, night re-keyed to blue), `post.mjs`
(night bloom threshold 0.95, knee 0.30, strength 0.22, radius x0.75, blue-dominant near-black
lift, day grade byte-identical), `dream.mjs` (floodlights 240 to 780 cd and distance 95 to 150,
pools 9.2 to 18 radius, edge lines on the ribbon, 24 cones at 1.8 scale, 14 barriers repainted,
6 crew figures, `chantierCenter` and `chantierSpan` getters).

Day, 1600x900 `?hour=17.6`, measured on the rendered PNG:
- dark share (luma below 0.18) 2.8 to **11.0 percent**, cool family 3.4 to **4.69 percent**,
  green 11.7 to 4.98 percent (the khaki mass is gone), five distinct clusters among the eight
  dominant buckets, largest bucket only 2.6 percent (was a 3.1 percent khaki plus a long tail of
  identical khakis).
- edge density per tenth of height 0.3 / 0.6 / 2.9 / 5.5 / 8.8 / 8.2 / 11.5 / 10 / 1.2 before,
  **7.3 / 7.1 / 8.6 / 9.6 / 10.8 / 10.9 / 10.2 / 9.6 / 4.9** after: the frame is full top to
  bottom instead of centred with empty margins.
- three pre-existing bugs found by the palette slice: the bay plane was wound backwards and
  back-face culled (0 percent of the frame), the curtain-wall towers had never existed because
  their lots straddled the ring road, and every far ridge sat outside the horizontal field of
  view. All three are why the distance had no structure.

Night, `?hour=21.6`: dominant buckets now `232345` 10.3 percent, `121b44` 10.2, `152343` 9.9,
`1b2345` 9.4, `1b244a` 8.2, all with blue 30 to 55 above red (was `323534` / `2d3332` grey-green),
cool share 95.7 percent, mean luma 0.13 to 0.18, salt-and-pepper speckle 0.

Dream chantier, located by the two named floodlights and measured in a 320x240 crop around them:
mean luma inside 0.373 against 0.162 outside, **ratio 2.31** at district tier and 2.00 at block
tier (requirement 1.6), with **6.5 percent orange and 0.8 percent red** pixels at saturation
above 0.45.

Mobile profile with 4x CPU throttling, steady state over four 2 s windows after a 6 s settle:
**5.55 ms mean frame (180 fps), p99 6.2 ms, max 10.6 ms**, notch 0, DPR 1.5, 39 draw calls,
131438 triangles.

Flaws after cycle 2:
1. Night has no warm family at all: warm share 1.8 percent and not one warm bucket in the eight
   dominant ones, while the requirement is every lit window warm and streets as ribbons of sodium
   light. The blue ambient is correct now; the light sources behind it are not.
2. Night is soft: peak edge density 5.0 against 10.9 by day, so the city's structure dissolves
   after dark instead of turning into crisp ribbons of light.
3. Street tier still reveals nothing new: bright pixels (luma above 0.72) 0.7 percent in the
   centre crop, palette dominated by 13.5 percent pale tan pavement, edge density 6.1 to 9.3
   against 8.9 to 12.2 at district. No painted markings, no awnings, no shopfront glass.
4. Geometry hygiene: 11 trees inside building footprints, and 11 building pairs overlap by more
   than 0.6 m under an exact oriented-box test (worst 1.05 m).
5. The adaptive ladder trips during the loading window: at 412x915 with 4x throttling the very
   first reading was 50.8 fps and the notch had already dropped to 1 (pixel ratio 1.25), while
   steady state is 180 fps at notch 0. Boot frames must not walk the ladder.

Bench numbers for this cycle were taken while the cycle 3 slices were still being written, so
they are recorded in the cycle 3 entry instead, where they were re-measured on a settled tree.

## Cycle 3: whole-pixel UI, no backdrop blur, allocation-free hot paths

Slices: `main.mjs` + `style.css` (integer label boxes, integer chevron centring, no per-frame
string writes, every `backdrop-filter` over the canvas removed, plus two fixes I made myself after
measuring), `agents.mjs` (allocation and upload hygiene only: no behaviour, population or rule
changed).

UI audit, live `getBoundingClientRect` at three sizes: **0 issues** (no fractional origin or size
on any label or control, no label/control overlap, no label/label overlap, nothing outside the
viewport, every control at least 44 px). Visible labels 8 at 1600x900, 8 at 1920x1080, 7 at
412x915 (the declutter hides one). Before this cycle every label landed on a fraction
(`692.890625,119.875`) and the phone chevrons sat at `y 433.5`.

Bench JSON, both through `?bench=1`:
- Desktop 1600x900, DPR 1: `{"avgFps":179.9,"p1LowFps":175.4,"maxFrameMs":12.6,"calls":54,"triangles":275848,"mode":"desktop"}`
- Pixel 6, DPR 2.625, 4x CPU: `{"avgFps":179.8,"p1LowFps":172.4,"maxFrameMs":25.9,"calls":44,"triangles":133982,"mode":"mobile"}`
- District tier: desktop `{calls:49, triangles:273304}`, phone `{calls:39, triangles:131438, shadows:false}`, notch 0 on both profiles, zero console errors.

Two defects found and fixed inside this cycle, both by measurement rather than by reading:
1. `?bench=1` used to start the run the instant the first frame rendered, so the 20 s script
   measured the shader-compile and upload burst: it reported `avgFps 97.2, maxFrameMs 177.8`.
   A phase-by-phase probe of the same page showed 5.56 ms per frame in every phase (street,
   district, rotation, sleep, wake, worst frame 20.2 ms). The auto run now waits
   `BENCH_SETTLE_MS = 4000` after install; re-measured through the same URL it reports the
   numbers above.
2. The quality ladder walked down on the loading burst: at 412x915 with 4x throttling a fresh
   page read 50.8 fps and had already dropped to notch 1 (pixel ratio 1.25) while its steady
   state is 180 fps at notch 0 (four consecutive 2 s windows: 5.55 ms mean, p99 6.2, max 10.6).
   The ladder now ignores frame cost until `bootAgeMs` passes `BENCH_SETTLE_MS`, and `install()`
   resets that clock.

Heap, 412x915 at 4x throttling, 60 s at 1 Hz: **26 collections** (was 36), heap 16.07 to 22.60 MB
(was 15.77 to 22.21), drift +2.39 MB, `Nodes` 143 to 143, `JSEventListeners` 41 to 41. With a
static camera, label style mutations over 60 s: **0**. The agents slice measured its own frame
path at 15.3 to 6.5 KB per frame and 90.7 to 38.3 KB of instance upload per frame, with zero
uncovered instances across 4804 frames including data swaps and population notches.

Dream rules re-verified after the UI change: 2 cards, exactly one button each, text built from
`dreamPair.words`, `Take it` appends to `localStorage['localtime.dreams.taken']` and returns the
city to `awake`.

Flaws after cycle 3:
1. Night still has no warm family: 1.8 percent warm share, no warm bucket among the dominant
   eight. The blue is right, the lights behind it are not.
2. Night is soft: peak edge density 5.0 against 10.9 by day.
3. Street tier is still the district view magnified: bright pixels 0.4 percent in the centre
   crop, palette 13.3 percent `9d7b49` pavement plus `b65c40` terracotta, band edge densities
   5.2 / 8.5 / 10 / 4.8 / 3.7 / 4.8 / 7.2 / 5.2 / 5.4 / 5.3. No painted markings, no shopfront
   glass, no awnings.
4. Phone portrait wastes 60 percent of the frame: band edge density
   0.5 / 0.9 / 1.6 / 6.7 / 8.5 / 7.8 / 6.6 / 3.1 / 1.8 / 1.4, region means 1.4 to 1.6 in the top
   third and 2.1 to 2.4 in the bottom third, against 7.2 to 8.3 in the middle band. The district
   occupies a 336x248 px box in a 412x915 viewport.
5. Geometry hygiene, still open: 11 trees inside building footprints and 11 oriented-box
   footprint overlaps above 0.6 m.

## Cycle 4: street-tier legibility, geometry hygiene, light on the moving life

Slices: `world.mjs` street furniture (a merged paint layer above the asphalt carrying zebras on
all six avenue crossings plus spur mouths and ring mouths, stop bars, edge lines, lane arrows,
centre dashes and hatched bus cages; shopfronts with glazing, jambs, sills, fascia boards and
striped awnings, capped at 150 desktop and 70 mobile; window frame rings, parapets with coping,
balconies with a top rail, kerb stones, bus shelters, bins, bollards and a news kiosk; the sign
atlas widened to 6x8 with 24 named shop fascias), plus two geometry passes, and `agents.mjs`
headlights, tail lights, road wash, brake pools, lit bus window bands, destination signs and tram
destination blinds, all one instanced mesh gated by `night && !dreaming`.

Street tier, centre crop of the 1600x900 frame at 0.34 of the width and height:
- bright pixels (luma above 0.72) **0.4 to 3.2 percent**, dark 5.6 percent
- edge density per tenth of height **9.6 / 14.7 / 17.8 / 7.5 / 5.2 / 6.0 / 9.5 / 7.0 / 10.7 / 11.0**
  against 5.2 / 8.5 / 10.0 / 4.8 / 3.7 / 4.8 / 7.2 / 5.2 / 5.4 / 5.3 before the slice
- palette gains `9d7c49` 11.1 percent (pavement), `464c24` 3.8, `b65c40` 3.6, `705b45` 3.1,
  `67645b` 2.8, `676030` 2.7 where it was dominated by one 13.3 percent pavement bucket

District tier, same day hour: peak band edge density 10.9 to **11.8**, dark share 10.3 percent,
cool 5.7 percent, eight dominant buckets between 2.2 and 0.9 percent. The day palette did not
regress, which was the standing risk of this slice.

Geometry, verified by my own probes rather than accepted from the report:
- building footprints, exact oriented-box test over 311 instances: **12 overlapping pairs, every
  one under 0.6 m** (party walls), **0 above 0.6 m** (was 11, worst 1.05 m)
- trees inside building footprints, tested in each building's own rotated frame: **0 of 1371**
  (an axis-aligned test reported 3 false positives, which is why the oriented test is the one
  that counts)
- floating instances: **0 buildings, 0 trees**

Night with the traffic lights on, 1600x900 `?hour=21.6`, awake: warm share (hue 0 to 60, luma
above 0.3) **1.66 to 4.24 percent** measured by the slice, blue family still dominant at 94.5
percent, night frame 54 calls and 371005 triangles. My own frame measurement of the same build
gives bright-warm area 0.76 to 1.33 percent and peak band edge density 5.0 to 6.1.

Budgets: day 52 calls and 385971 triangles, night awake 54 calls and 371005, phone 42 calls and
168129 with shadows off and notch 0. All inside 80 / 400k and 45 / 180k.

Flaws after cycle 4:
1. Night light density is still far from the target: bright-warm area 1.33 percent of the frame
   with 128 blobs, against a requirement of every lit window warm and streets as ribbons of
   sodium light. The blue is right; the light sources are still too few and too small.
2. Night edge density 6.1 against 11.8 by day: the city still softens after dark rather than
   turning into crisp light.
3. The shopfronts, which are the point of the street tier, are barely visible in the exact centre
   crop because the camera looks along the avenue there (measured by the slice at 0.11 percent of
   the crop by material mask). They need either a camera-on-the-street composition or facades
   angled into the view to be judged.
4. Phone portrait still spends 60 percent of its height on low-detail bands; the district box is
   336x248 in a 412x915 viewport.
5. Sign atlas anisotropy is 1, so fascia text blurs at the shallow angles the street tier produces.

## Cycle 5: light at night, and the phone budget paid back

Slices: `world.mjs` night (sodium ribbons merged into one strip mesh lifted 15.5 cm over the
asphalt with polygon offset on the ring, both avenue carriageways, the spurs and the secondary
grid; one instanced additive sheet carrying tower floor cells 93, luminaire heads 69, entrance
lanterns 311, roof-terrace lamps 286 and per-window halos 872; every night tint moved from deep
orange to sodium with green at 0.6), `agents.mjs` (ten mover pools hidden while dreaming),
`dream.mjs` and `main.mjs` (dressing and market gated off on the touch profile, forceSinglePass on
the two additive dream materials there, and a one-time `dream.warm()` precompile).

Night, 1600x900 `?hour=21.6`, awake, measured by me on the rendered frame:
- bands (edge per tenth of height) 2.8 / 3.5 / 6.6 / 8.5 / 11.2 / **12.2** / 11.6 / 10.8 / 9.0 / 2.2
  against 2.8 / 3.4 / 5.2 / 6.5 / 8.1 / 8.6 / 8.3 / 8.2 / 7.0 / 2.5 before. Night now peaks higher
  than the day frame did at cycle 4 and is no longer the soft half of the pair.
- bright warm pixels (luma above 0.42, red 25 above blue) **0.76 to 9.74 percent**, connected warm
  blobs 139 to **251**, total warm-lit area 914 to 12574 pixels.
- families: warm 11.0 percent, cool 84.7 percent, dark 18.3 percent. Palette top eight are all
  navy: `121c43` 8.4, `1c2b54` 5.5, `242345` 5.4, `1b244b` 5.1, `152343` 4.3, `292445` 4.0,
  `1b2444` 3.9, `242b4b` 3.1. Deep blue, never black, which is the standing night rule.
- centre cell luma 0.283 with edge 17.2, corners 0.148 to 0.198: the town is where the light is.

Phone, 412x915 DPR 2.625 touch, CPU throttled 4x:
- day district 42 to **40** calls, 161243 to 149519 triangles, after `frames` (1447 instances) and
  `shop-glass` (74) are not built at all below 800 px wide.
- night district 46 to **44** calls, 176841 to 165117 triangles.
- dreaming district 54 to **42** calls, after the dream stops drawing cones, barriers, crew, stalls
  and awnings on touch, and the ten idling mover pools stop being drawn at all.
- `?bench=1` max frame: 39.6 ms before, **24.3 and 22.7 ms** on two quiet runs after the precompile.
  The one-time compile that cost the first sleep 35.7 ms now lands in the loading window instead:
  first sleep 21.6 ms, second 19.1 ms.

Desktop is untouched by the phone work, verified structurally rather than trusted: `frames` 1447
and `shop-glass` 154 still present and visible, 52 calls and 341945 triangles by day, 56 and
369793 at night, 54 and 333321 while dreaming (down from 64, same mechanism).

Verified behaviour, not assumed:
- dream on a clean profile: sleep, wake, then exactly **2 cards**, each with a `Take it` button,
  the taken sentence appended to `localtime.dreams.taken`, state back to `awake`, 0 cards left.
- phone dream screen: two cards at y 581 and 714, width 388 of 412 with 12 px margins, the four
  controls at y 434 and 855, five labels, **zero overlaps and zero offscreen**.
- after a taken road the works are gone and `road-permanent`, `market-stalls` and `market-awnings`
  replace them, which is the rule working.
- motion: cars, vans, buses, tram and pedestrians all move between samples while buildings, trees,
  lamps, cranes and detail instances do not.
- night z-fighting sweep, 4 rotations by 2 tiers: speckle **0.000 to 0.002 percent**.
- transparent layers: every transparent material has `depthWrite: false`, additive ones sit at
  render order 2 to 3 in a fixed order, so nothing can pop by resorting.
- 60 s heap: 26 collections, drift **-1.56 MB**, DOM nodes 113 constant, listeners 30 constant.
- phone steady state without the bench: worst frame **9.2 ms**, all frames 42 calls at the time.

Flaws after cycle 5:
1. The day frame has no cool half at all: hue 0 to 30 is 29.1 percent, 30 to 60 is 38.3, and the
   210 to 240 bucket is only 2.8 percent, so 80.7 percent of the picture is yellow to orange.
2. The phone portrait top third is a flat plate: edge density 0.2 in every tenth, one colour
   `696e69` at 17.8 percent, cool family 0 percent.
3. The phone portrait bottom third fades out: edge 1.4 to 1.8 where it should carry a hillside.
4. The sign atlas is mipmapped but has anisotropy 1, so a fascia at a grazing angle blurs.
5. The phone has no call headroom left: 44 awake, 42 dreaming, and 45 once a provisional road has
   been taken and becomes permanent.

## Cycle 6: the sky is water, and the cool half of the frame

This cycle started from a measurement I could not explain: the top third of the phone frame was a
single flat colour covering 17.8 percent of the crop with edge density 0.2. The slice that fixed it
proved it was none of my guesses. Recolouring every sky uniform and then hiding the dome entirely
changed **zero pixels** in that band: the sky is never visible at the phone tier. The plate was the
**bay water plane**, 72.7 percent of the top tenth and 15.6 percent of the whole frame. The far
ridges were already there (31.4 percent of that band) but had no value step, no hue step and no
texture to read as ridges. Fog was not the cause either.

What changed in `world.mjs`: the terrain belt resolves far crests (20 m spacing out to 520 m, then
1.22 growth), six tall bay islands and eight skerries stack the chain instead of one dome, aerial
perspective is now keyed on **world radius** rather than camera distance (a blue shift up to 0.86
plus a radius value ramp and a two stop haze), fog became a far veil from 1100 to 2800 m at
`0x8299e0`, the day sky palette was blued, bay water moved to `0x7794cc`, the far groves were
extended to the island band with pale silver canopies, terrace paint was extended to 700 m with a
coarse 14 m wall contour, and the near slope gained a switchback lane, risers, retaining walls,
eight villas and near cypresses.

Measured, all four profiles, zero console errors:

|profile|frame|calls|triangles|
|---|---|---|---|
|desktop 1920x1080|day|52|362089|
|desktop 1600x900|night|56|389937|
|phone 412x915|day|38|163667|
|phone 412x915|night|42|179265|

- Desktop day bands 7.8 / 7.7 / 9.1 / 10.2 / 11.9 / 12.2 / 11.3 / 9.7 / 9.0 / 5.0, warm 50.2,
  cool 19.8, dark 9.9, speckle 0.003.
- Desktop night bands 2.8 / 3.4 / 6.6 / 8.5 / 11.3 / 12.1 / 11.5 / 10.5 / 9.1 / 2.4, cool 84.6,
  warm 10.9, dark 16.6, speckle 0.001.
- Phone portrait bands **2.7 / 2.0 / 1.6 / 7.3 / 9.3 / 9.2 / 7.0 / 3.3 / 2.9 / 3.1** against
  0.5 / 0.9 / 1.7 / 7.0 / 9.2 / 9.1 / 7.0 / 3.1 / 1.8 / 1.4: the top three tenths went from a plate
  to content and the bottom two from fading roofs to hillside. Phone night bands 1.0 / 0.7 / 0.6 /
  5.8 / 10.2 / 11.4 / 7.4 / 1.1 / 1.0 / 1.4.
- Phone, every dream state measured one after another: 38 calls day, 42 night, **35 dreaming**,
  **44 while the cards are offered**, **43 after a road has been taken**. The 45 call breach that
  existed at cycle 5 is closed in every state, not just the idle one.
- Quiet benches, no other browser alive: desktop **179.9 fps, p1 175.4, worst frame 14.3 ms**,
  57 calls, 364633 triangles. Phone with the 4x throttle: **179.8 fps, p1 175.4, worst frame
  20.7 ms**, 40 calls, 164099 triangles.
- z-fighting sweep, 4 rotations by 2 tiers, day: speckle 0.000 to 0.004 percent.
- labels: 8 visible on desktop, 7 on phone, zero overlaps, zero offscreen.
- 40 s heap: 19 collections, drift -1.28 MB, DOM nodes 113 constant, listeners 30 constant.

Flaws after cycle 6:
1. Phone night triangles are 179265 against a 180000 ceiling: 735 spare. Any further geometry
   breaks the phone at night, so the next slices have to buy back triangles before they add any.
2. Desktop night is 389937 against 400000, so the same is true there, though with more room.
3. The phone night frame's bottom four tenths read 1.1 / 1.0 / 1.4 at edge density: at night the
   nearest ground goes quiet, which is plausible but leaves the bottom of the portrait empty.
4. The portrait top third reaches edge density 1.2 in only three of its ten tenths, below the seven
   I asked for; the remaining ones are bay water and the town's own crest at 700 m.
5. The dark share on the desktop day frame drifted down to 9.9 percent from 11.0 across the night
   and grade work, so the shadow side is slowly washing out.

## Cycle 7: the cool half, and text that survives a grazing angle

Two changes, both measured on the same build.

**The grade.** The day frame had no cool half at all: hue 0 to 30 was 29.1 percent, 30 to 60 was
38.3, and the 210 to 240 bucket only 2.8, so 80.7 percent of the picture was yellow to orange. The
composite shader now carries a per pixel mix with two keys, one target: a shade key on linear luma
below about 0.034, and a distance key on the upper frame (which is a stable depth proxy because the
rig pins pitch at 40 degrees and never rolls), both mixing toward `V * (0.576, 0.735, 0.955)`,
reached through a rose midpoint so the hue path runs warm to rose to violet instead of crossing the
desaturated cyan between them. Zero cost: it lives in the pass the phone already runs, and it is
gated on day, so night is bit-identical (verified: 250 light blobs, 9.74 percent bright warm, cool
84.6 percent, every band within 0.1 of cycle 5).

Result at 1920x1080 day, measured by me: cool family **6.2 to 19.8 percent**, the 210 to 240 bucket
**2.9 to 16.5**, warm **50.2 percent** (still the majority), grey share 3.0 percent, mean
saturation 0.377, bands still 7.8 to 12.2 across the middle.

**The text.** Both atlases were already mipmapped (`LinearMipmapLinear`) but with `anisotropy: 1`,
so a fascia or a barrier stripe seen at a shallow angle blurred. `main.mjs` now sets both to
`renderer.capabilities.getMaxAnisotropy()`, which measures **16** on this device, in one function
called once per install, next to the existing warm-up and with no per-frame cost. It reaches the
sign atlas through `world.signs.texture` and the barrier stripes through the dream group, both
surfaces the world and dream slices had already exposed rather than new plumbing.

Flaws after cycle 7:
1. Warm is 50.2 percent and cool 19.8, which is a real contrast, but the coolest contribution is
   concentrated in the upper frame: the lower third of the day frame is still almost purely warm.
2. Night is untouched by design, so the night frame still has no cool edge to the warm windows
   except the sodium pools' own falloff.
3. Anisotropy is raised on two atlases only; any future atlas has to remember to do the same, since
   there is no central place that owns texture filtering yet.
4. The two remaining pixel questions from cycle 6 are untouched: phone night triangles at 735 spare,
   and the phone top third reaching its edge target in three tenths of ten.
5. Still no measure of how the frame reads to a person who has not watched it evolve, which the
   numbers cannot answer.

## Cycle 8: buying back the budget and putting the shadow back

Three jobs, all in `world.mjs`, all chosen because the numbers said they were the binding limits.

**The night triangle budget.** Desktop night was 389937 against 400000 and phone night 179265 against
180000, so nothing could be added anywhere until this was bought back. The night now culls, only
while it is night, the things night cannot see, each one selected by hiding it and differencing the
captured frame: `frames` 11576 triangles for 0.284 percent of pixels changed, `contact` 2454 for
0.042, `cranes` 2366 for 0.055, `houses` 12 for 0.005, and the detail layer drawn one rank lower for
7968 triangles and 0.076 percent at a mean channel delta of 0.006. Touch additionally drops `sky`,
`signs`, `bay`, `towers` and `expressway` at night, 5638 triangles, and `sky` changed 0.000 percent of
pixels with a maximum channel delta of 1. Symmetric on the day branch, verified by walking night to
dawn and back.

|profile|frame|before|after|
|---|---|---|---|
|desktop 1600x900|night|56 calls, 389937|**50 calls, 366149**|
|phone 412x915|night|42 calls, 179265|**35 calls, 173145**|

The night look did not move: bands identical to the last digit at 2.8 / 3.4 / 6.6 / 8.5 / 11.3 /
12.1 / 11.5 / 10.5 / 9.1 / 2.4, 251 blobs, 9.76 percent bright warm, 12591 lit pixels against 250 /
9.74 / 12574 before. Same night, twenty four thousand triangles cheaper.

**The shadow.** The desktop day dark share had drifted from 11.0 to 9.9 across the night and grade
work, so the sun went 3.6 to 3.9 and the hemisphere 0.95 to 0.75: shade and cast shadow step down
while lit surfaces keep their value. Measured: dark **9.9 to 12.2 percent**, warm 52.0, cool 20.3,
mean value fell 0.0099 against an allowance of 0.02, and the bands came out slightly crisper than
before rather than softer: 8.1 / 7.9 / 9.3 / 10.6 / 12.2 / 12.5 / 11.7 / 10.0 / 9.1 / 5.1.

**The phone portrait at night.** Its bottom tenths read 1.1 / 1.0 / 1.4. A lane light ribbon was
appended to the existing `night-road-glow` sheet (no new draw call, 272 triangles, tapered at both
ends so it never ends in a stroke), twelve lane lamps were sited from the projection onto the third
hairpin, and a build-time `surfaceHeightAt` lookup was added so the ribbon and pools lie on the drawn
slope instead of sinking into it. Bottom three tenths now **2.1 / 2.0 / 2.1**, and bands 1 to 7 moved
by 0 to 0.1, so none of the new light leaks upward. Phone night light: 271 blobs, 7.47 percent bright
warm.

Flaws after cycle 8:
1. `mural` still appears zero times in the world, and the brief asks for a few big painted murals on
   blind walls. It is the loudest missing signal of a contemporary city.
2. `acUnit`, `waterTank` still zero: the roofs are cleaner than a real Mediterranean roof, and the
   brief is explicit that an empty roof reads as 1993.
3. `trafficLight` still zero, while six avenue crossings already carry zebras, stop bars and bus cages.
4. The day darkness work lowered mean value by 0.0099 with no compensating lift anywhere, so the
   frame is a shade dimmer overall than at cycle 7.
5. Phone night is at 173145 against 180000, so the touch profile still has only about 7k triangles of
   room and every future addition has to be argued for.

## Cycle 9: murals, roofs that are lived on, and signals at the crossings

The three things grep said were missing from the target picture, added in `world.mjs` only.

**Murals.** Ten portrait panels on blind party walls, one canvas atlas and one mesh, so one draw
call and 24 triangles in total. Candidates are exact-blind faces (`faceWindows`), facing the view,
clear of the neighbouring block by sampling `roofInFront`, sized to fit tall narrow walls, and
ranked by projected pixels in the district frame using the camera's own composition, with panels
outside the frame rejected. Paint sits 6 cm proud with polygon offset, receives shadow, never casts.
Measured: **9444 device pixels of paint, 0.291 percent of the 1920x1080 frame**, of which cobalt is
2688 px or 0.083 percent.

**Roods that are lived on.** A condenser bank per roof in three greys, then water tanks with lids,
then satellite dishes, laid in the existing rank-2 detail stream rather than a new system, keeping
0.16 m clear of terraces, solar panels and planters. Self-audit: **400 pieces, 490 boxes, 310 AC
units, 45 tanks, 45 dishes, 0 violations**. Local edge energy at the projected instance points rose
**16.23 to 20.31, up 25.1 percent**, with 250 of the 349 in-frame instances changing the captured
pixels. Night pays nothing: the pieces start at detail index 6090 and night draws 5568.

**Signals.** Eighteen heads on poles, two per avenue crossing and two per spur mouth, cream board,
dark housing, red amber and green lamps, placed by flag and oriented-box rejection with a
same-approach fallback. 1296 triangles, no shadow pass. **18 of 18 heads project inside the frame**
and change the pixels at their own location with channel deltas from 44 to 110.

|profile|frame|before|after|
|---|---|---|---|
|desktop 1920x1080|day|52 calls, 362377|**54 calls, 376653**|
|desktop 1600x900|night|50 calls, 366149|**52 calls, 367465**|
|phone 393x851|day|38 calls, 163955|**40 calls, 165271**|
|phone 393x851|night|35 calls, 173145|**37 calls, 174461**|

Same-code A/B at identical simulated time, noise 0.00 to 0.01 per band: 3.14 / 4.07 / 7.46 / 9.81 /
11.69 / 11.78 / 10.60 / 8.52 / 6.71 / 2.07 becomes 3.10 / 4.15 / 7.57 / 9.82 / **12.16** / **12.01** /
10.55 / 8.49 / 6.65 / 2.04, so the middle of the frame got crisper where the roofs and murals sit and
nothing moved by more than 0.47. Speckle over four rotations by two tiers: 0.0008 to 0.0152 percent,
and the worst frame is the pre-existing one at that rotation.

Flaws after cycle 9:
1. The 0.4 percent palette bucket I asked for is arithmetically impossible: 0.4 percent of the frame
   is 8294 CSS pixels, the whole mural programme is 6044, and the largest single blind wall in the
   district frame is 1799. The honest ceiling for one mural colour is 0.29 percent and the largest
   bucket measures 0.033. My target was wrong, not the work.
2. Wall supply is the limit on murals: of 325 faces facing the view, 207 are blind, but only 17 clear
   their neighbour by 5 m and only 12 survive the frame gate, so ten panels is close to the maximum
   this city can hold without inventing walls.
3. Phone drops the roof clutter entirely, so on a phone the roofs are still cleaner than on desktop.
4. The signals are static: red, amber and green are painted once and never cycle with the data, so
   they read as street furniture rather than traffic control.
5. Desktop day is now 376653 triangles against a 400000 ceiling, leaving 23k for everything that comes
   after, which is not much for a frame that will still take a colour grade.

## Cycle 10: the cool reaches the street, the shadows survive it, and the gates close

**The colour correction.** Cycle 9 left the frame at warm 47.6, cool 22.8 and dark 10.2 by my
metric: the exposure lift had eaten 2.0 points of dark share, which is the one thing the picture can
least afford. The lift was rebuilt as a display-space midtone gain keyed on the same midpoint
lightness the dark metric uses, opening at 0.20 and closing at 0.34, so every pixel that reads as
shade is multiplied by exactly 1.0 and cannot move. Scaling the encoded channels by one factor also
leaves hue and saturation untouched, so the lift cannot push a pixel across a warm or cool boundary.
The lower-frame cooling was concentrated into the bottom third, because the middle third was carrying
tint that bought 0.0 cool and cost 0.7 dark and 1.3 warm.

|metric|cycle 9|now|
|---|---|---|
|warm (hue under 45 or over 340, sat above 0.15)|47.6|**48.3**|
|cool (hue 165 to 260)|22.8|**22.8**|
|dark (midpoint under 0.18)|10.2|**11.7**|
|mean value|0.432|0.431|
|bottom third cool|15.3|**15.7** (from 7.7 before the grade existed)|
|bands|8.6 / 8.5 / 9.9 / 11.3 / 13.3 / 13.4 / 12.5 / 10.7 / 9.8 / 5.5|8.8 / 8.6 / 10.1 / 11.6 / 13.5 / 13.7 / 12.7 / 10.9 / 9.9 / 5.6|

**The final gate, every number measured on the delivered build.**

|profile|frame|calls|triangles|gate|
|---|---|---|---|---|
|desktop 1920x1080|day|54|376653|80 calls, 400k|
|desktop 1600x900|night|52|367465|80 calls, 400k|
|desktop 1600x900|dream open|57|370153|80 calls, 400k|
|phone 412x915|day|40|165271|45 calls, 180k|
|phone 412x915|night|37|174461|45 calls, 180k|

- Benches with no other browser alive: desktop **179.9 fps, p1 low 172.4, worst frame 12.4 ms**,
  59 calls, 379197 triangles in the bench's own peak. Phone at a 4x CPU throttle **179.7 fps,
  p1 low 172.4, worst frame 23.0 ms**, 42 calls, 165703 triangles. Every gate is met: 60 fps, p1 55
  desktop and 50 phone, no frame over 33 ms.
- Phone dream states, measured in one sequence: 37 awake, **30 dreaming, 39 with the cards offered,
  38 after a road is taken**. The 45 ceiling is cleared with margin in every state, not just the idle one.
- Zero console errors on all five profiles, and zero failed requests: the page makes exactly two
  network calls, Three.js 0.160.0 from jsDelivr and the export.
- Labels: 8 visible on desktop, 7 on phone, **zero overlaps, zero offscreen**, through all four
  rotation snaps on the phone.
- Z-fighting: day 0.000 to 0.003 percent, night 0.000 to 0.001 percent, over four rotations by two tiers.
- Heap over 90 s: 38 collections reclaiming 136 MB, heap sawtoothing between 15.0 and 21.7 MB with the
  peak identical at 40 s and at 90 s, DOM nodes 114 constant, listeners 30 constant. No leak.
- Interaction stress, not covered by the bench: 1918 phone frames through six zoom steps, three
  rotation snaps and five zoom steps back, worst frame **9.4 ms**, none over 33. Desktop through four
  snaps, a tier change, a re-frame and a full sleep-wake-sleep cycle: 3960 frames, worst **17.1 ms**,
  none over 20.
- Frozen files verified byte-identical to cycle 0: `data.mjs`, `simday.mjs`, `camera.mjs`.

Flaws after cycle 10, stated rather than hidden:
1. Murals cannot colour the frame the way the brief imagines: the largest mural bucket is 0.033
   percent, because only 12 blind walls in the district frame are wide enough, and no arithmetic can
   make ten tall narrow panels into 0.4 percent of a 1920x1080 frame.
2. The traffic signals are painted once and never cycle, so they are street furniture rather than
   traffic control. The data has no signal phase to drive them from, and inventing one would break a
   data rule.
3. The phone frame is flatter than the desktop frame by construction: no shadows (dark 8.7 against
   11.7) and no roof clutter, because the touch budget cannot pay for either.
4. Desktop day sits at 376653 triangles, 94 percent of the ceiling, so the next addition needs a
   subtraction first.
5. The phone portrait still frames the district in a 336x248 box inside a 412x915 viewport. The
   camera is frozen, so a third of the phone frame is now well-built scenery rather than city.

## Closing summary

Ten cycles, every one recorded with its five measured flaws. The two numbers that best describe the
distance travelled, measured on the same harness every time:

- day frame bright-warm to edge crispness: street-tier centre crop bright pixels from 0.4 to 3.2
  percent, band edge density from a 5 to 10 range to an 8.6 to 13.7 range, dark share 10.3 to 11.7
  with the cool family absent and now at 22.8 percent.
- night frame: bright warm 0.76 to 9.75 percent of the picture, connected light sources 139 to 252,
  edge peak 5.0 to 12.2, still deep blue at 84.5 percent cool and never black.

And the two budgets that had to survive all of it: desktop day went from 362377 triangles and no
grade to 376653 with murals, roof clutter, signals and a graded frame; the phone went from 163955 to
165271 by day and from 173145 to 174461 at night while gaining murals and signals, and its dream
states now cost 30 to 39 calls against a ceiling it used to breach at 54.

## Post-delivery audit

Three concerns raised against earlier states were re-tested on the delivered build and closed:
the `?bench=1` path produces **zero console errors** across a full bench run; two complete sleep,
wake and take cycles on the phone leave the frame at **38 calls both times** with only the permanent
road added, so no visibility creeps from cycle to cycle; and a pairwise 3D test over the 350 drawn
roof-clutter instances found one overlapping pair which is a satellite dish's own mast and reflector,
by design, and nothing else. One record-keeping gap is stated rather than hidden: cycle 7 has no
`.cycles/07` snapshot, because its state was superseded by cycle 8 before the copy was taken.

## Loop resumed after cycle 10

The previous session wrote the closing summary above about seven hours early. The owner wants the
full budget, so the loop continues from here: cycle 11 and every cycle after it is appended below
that summary, and the real closing summary is written only when the budget is nearly spent. The
summary and audit above stand as the cycle-10 record; they are not the end of the work.

## Cycle 11: warm hills instead of a grey wall, and the grade stops lying about depth

Two defects dominated the frame at the start of this cycle, and both were measured before being
touched: the top band of the desktop day frame held only 2.2 percent warm pixels at the true
baseline and 0.67 percent blown white, and at street tier the whole upper frame was a monochrome
slate city.

**The far country was painted into the sky.** The far-grove rows multiplied the canopy colour by 2.6
past 330 metres, which put pale olive crowns over the bloom threshold, and that is what the white
hexagons in the top left of every earlier frame actually were: clouds are icosahedra of 6 to 17
metres, and the canopies were the flat white shapes. The multiplier is gone. Far canopies now fade
toward a pale warm lilac air by 0.15 to 0.60 across 300 to 700 metres, sized 1.6 to 2.6 on desktop
and capped so a touch crown stays under 7 metres, at 52 metre row spacing so the hills show ground
between rows. The air itself moved from a saturated blue (0x7f9ae0) to a pale warm lilac
(0xc9c0cf), the day fog to 0xc7c3d6, and the terrain aerial tint now mixes that same lilac with each
plot's own value, so 82 percent of vertices 300 to 700 metres out keep red above blue and the field
patchwork survives. The bay was deepened to hold its blue against the warmer hills. Measured on the
built world: 1011 desktop trees (429 far rows) and 736 touch trees, brightest far canopy channel
0.258.

**The post grade was faking depth with screen height.** The cool distance key is a screen-space
function of vUv.y, which is only a depth proxy at the district tier: at street tier the upper frame
is geometry 100 to 200 metres away, and the key painted it slate violet with a hard seam. It now
starts in the top fifth (0.80 to 1.0) at 0.35 instead of 0.80, and its amount is scaled by camera
distance against the framing distance from camera.mjs, zero at or below the block tier and one at
the district tier, with a 0.002 step guard so the uniform is not written every frame.

|metric, top 30 percent of the frame|before|after|
|---|---|---|
|desktop day warm share|2.2 percent|**62.4 percent**|
|desktop day blown white|0.67 percent|0.73 percent (clouds, see flaw 3)|
|phone day warm share|28.6 percent|**59.2 percent**|
|desktop day triangles|376653|**369393**|
|desktop night triangles|332025|324765|
|phone day triangles|165271|165271|
|calls, all profiles|54 / 52 / 50 / 40|54 / 52 / 50 / 40|

Benches on the delivered build: desktop **180 fps, p1 172.4, worst frame 9.6 ms**, 57 calls, 362749
triangles at the bench peak; phone at a 4x CPU throttle **179.9 fps, p1 169.5, worst 13.5 ms**, 39
calls, 174893 triangles. Zero console errors on desktop day, desktop night and phone day.

Flaws after cycle 11:
1. The clouds are the last 1993 element in the sky: flattened icosahedra in flat unlit white with no
   softness, several of them reading as blown white hexagons, especially the pair above the crown.
   They need to be soft radial alpha quads, higher and much larger, so they read as thin high cloud.
2. The green wave paints every avenue vehicle a saturated 0x5bc46e. On the boulevard, up close, it
   reads as bright green slabs lying on the tram lanes. The contract says the avenue segment is what
   tints at street scale, so the tint belongs on the lanes, not on the cars.
3. The white share of the top band did not fall, because the blowout was never the canopies alone:
   the cloud tint 0xfdf7ec also crosses the bloom threshold.
4. The street tier's upper frame is warm now but the far city there is still flat: no aerial
   perspective inside 400 metres, so the far blocks sit at the same value as the near ones.
5. Desktop day sits at 369393 triangles, still 92 percent of the ceiling, and the bench peak is
   362749, so any new geometry still needs a subtraction first.

## Cycle 12: the sky stops being 1993, the green wave becomes traffic, and 30k triangles come back

**Clouds.** They were flattened icosahedra, unlit flat white, 6 to 17 metres wide, which is exactly
the cartoon cloud the brief forbids and the source of the last blown white shapes in the frame. They
are now horizontal two-triangle quads with a soft alpha built in code: a 128x128 RGBA data texture
of four overlapping cubic radial lobes unioned and border-faded, peak alpha 0.60, mipmapped. Placed
150 to 260 units up, 35 to 90 metres wide and 19 to 72 deep, with a fixed per-cloud yaw. Tint is a
whole-deck cover colour warm 0xd6c9b4 to cool 0x9aa3ae by fragmentation, per plate biased toward the
sun side, hard clamped at 0.72 per channel so nothing can cross the bloom threshold at 0.75. Rule 8
is untouched: count 2 plus 126 by fragmentation, same drift and wrap, same dream tint. Still one
instanced mesh, one draw call, and the geometry went from 20 triangles an instance to 2.

**Green wave.** The rule is unchanged and still detected the same way, but the tint is no longer a
repaint: each avenue car now starts from its own palette colour and lerps 0.45 toward 0x5bc46e, so a
wave reads as traffic moving in step instead of bright green slabs lying on the tram lanes.

**Triangles.** The shared unit box is now a five-face open-bottom box: 10 triangles instead of 12.
The shadow-pass question was settled by measurement, not assumption: three r160 flips a FrontSide
material to BackSide for the depth pass when shadowSide is null, so the missing face did write depth,
but only as a sheet displaced by height over tan 25 degrees, which for a grounded box falls inside its
own footprint where the volume occludes it and the leeward walls still write the street shadow. The
real loss is the detached contact patch under floating slabs (awnings, balcony slabs, solar panels,
car roofs, crane jibs). No consumer needed it enough to justify a closed box, so the shared box stays
open and no shadowSide override was added.

**Phone ground contact.** The touch profile never had the contact blob mesh or a shadow map, so its
buildings sat on the pavement with no darkening where volume meets ground. Each building and the
tower now carry a rank 0 apron inside the existing detail instance pool: plan 0.6 m proud, 0.62 m
tall, base 6 cm buried, coloured 0.64 of the building's own instance colour. No new mesh, no new
call, and the pool's triangle total still fell because the open-bottom box applies to every instance.

|profile|frame|calls|triangles|before|
|---|---|---|---|---|
|desktop 1600x900|day|54|**336393**|369393|
|desktop 1600x900|night awake|52|331109|360205|
|desktop 1600x900|night dream|50|295669|324765|
|phone 412x915|day|40|**156943**|165271|
|phone 412x915|bench peak|39|166977|174893|

Benches: desktop **180 fps, p1 175.4, worst frame 10.2 ms**, 57 calls, 333653 triangles; phone at 4x
CPU throttle **179.9 fps, p1 172.4, worst 13.5 ms**, 39 calls, 166977. Zero console errors on
desktop day, desktop night and phone day. Desktop day now sits at 84 percent of the ceiling, 61k
under it, which is the headroom the next cycles spend.

Flaws after cycle 12:
1. The steady hour now has a genuinely clear sky, which is what rule 8 asks, but the target picture
   has thin high wisps at golden hour and the frame top is empty air. A base of about 118 instead of
   150 would let the two steady-hour plates enter the top of the frame as faint strands.
2. The phone portrait still opens at the district tier, so the city occupies a band about a third of
   the screen with 40 percent empty sky. The block tier, which is one of the three legal tiers, fills
   the portrait frame with the district instead.
3. The traffic signals are still painted once: red, amber and green never change, so they read as
   furniture rather than control, and no car ever queues at one.
4. The far country in the top band reads as one soft warm wash now: the olive rows and field
   patchwork are there in the data but the value separation between rows and ground is small.
5. At night the cloud layer is present but reads as the wrong thing: hiding the cloud mesh changes
   2.42 percent of the night frame with deltas up to 245 levels, so the plates are there and large,
   yet they sit as a slightly darker flat blue against the sky because the whole deck takes one
   dream tint (0x3b414a). A low sun city lights its cloud bases from below, so the night deck needs
   a value relationship rather than one colour, brightest over the lit city and dimmer toward the
   edges.

## Cycle 13: signals that change, a wisp at golden hour, and a phone frame that is a city

**The signals now control something.** Every head permanently showed red, amber and green at once,
because the three lenses were baked vertex colours inside one merged geometry, where no instance can
differ. The lenses moved out into their own instanced mesh of 3 instances per head (the shared open
box, 0.4 by 0.4 by 0.1, per-instance colour); an active lens draws at full hue, an inactive one at
0.10 of it, so a head is legible even turned away from the sun. The standard material was kept on
purpose: a basic lens would read as a sticker by day and self-illuminate at night, fighting the
sodium lamps the night plate depends on. One extra draw call, and the signalling triangle total is
unchanged at 1080 because removing the baked lenses freed exactly what the new ones cost.

`world.signals` now exposes `heads` (positions and count), `state` (phase per head) and `apply()`,
and world never writes a phase itself. Agents owns the timing: a 55 second round of 26 s green, 3 s
amber, 26 s red, advanced by the simulated clock with a per-head offset hashed from the head's own
position, so the two approaches of one junction are not in step. `apply()` is called only when a
head's code actually flips, guarded by a cached previous-code array, and the first frame is green
rather than white. Measured on the live page: state at t and t plus 30 s read
`022012220002220220` then `200200002220002002`, so the phases advance on their own.

**An honest limit, stated rather than hidden.** Vehicles do not obey the lamps. The movers have no
car-following model, so a hard stop would stack cars inside one another, which the pixel-perfect
rules forbid. The phase is the display of traffic control; obedience needs a following model and is
the next cycle's work, not a silent omission.

**A wisp at golden hour.** The old cloud base of 150 units with 110 of band put every plate above
the district frame, because that camera stands 213 units up and its top edge already looks 17.5
degrees down, so a cloud higher than 148 units cannot be in the picture at all. The deck is now
118 to 140 units, which is 28 units above the tallest roof, 73 to 95 below the district camera and
above the street camera. The four plates a clear hour keeps are seeded in a near lane, and the
projection puts them at ndcY 0.56 to 0.83, that is 76 to 198 px down from the top edge, so a steady
hour now has thin strands in the top quarter of the frame instead of nothing.

**A night deck with depth.** The single dream tint 0x3b414a is gone. Each plate is composed at night
from a deep stop, a city stop weighted to the lit bowl and a moon stop on the moon's side, then
dimmed toward the field edge, which gives a 2.4 to 1 value range from edge to city with blue at 2.85
to 3.18 times red everywhere, so no blend reaches grey. Refreshed on a 2.5 s cadence as the deck
drifts, and still far under the night bloom threshold.

**The phone frame is a city now.** The portrait phone opened at the district tier from a 652 metre
framing distance, which put the district in a band about a third of the screen with the rest haze.
It now opens at the block tier, one of the three legal tiers, applied once in `boot()` and never
again, so the resize path and the user's own zoom are untouched. Measured on the phone frame: the
city now spans six of the ten vertical tenths instead of four, edge density rose from 4.07 to 5.17
percent, and flat area fell from 64.7 to 61.0 percent.

**The far country separates.** Canopy value multiplier 0.9 to 0.74 and the air lerp floor moved from
0.15 to 0.08, measured against the ground under each canopy: median canopy to ground luminance ratio
was 0.555 to 0.645 across the four distance bands and is now 0.443 to 0.497, inside the target band,
with the brightest canopy channel down from 0.258 to 0.195 and the ground still red-dominant in 68.8
to 90.0 percent of sampled cells.

|profile|frame|calls|triangles|
|---|---|---|---|
|desktop 1600x900|day|55|336393|
|desktop 1600x900|night dream|51|295669|
|phone 412x915|day, block tier|41|156943|
|desktop bench peak|bench|58|333653|
|phone bench peak, 4x throttle|bench|40|166977|

Benches: desktop **180 fps, p1 172.4, worst frame 9.7 ms**; phone **179.9 fps, p1 175.4, worst
13.6 ms**. Zero console errors on desktop day, desktop night and phone day.

Flaws after cycle 13:
1. Life density is the weakest thing left on the desktop frame: 69 pedestrians and 76 vehicles
   project inside a 1600x900 frame, so the "crowds everywhere" of the target reads as a scatter. The
   pools are 300 each; the triangle budget now has 61k of headroom.
2. The phone frame's top two tenths are 96.7 and 90.8 percent flat and its bottom two 81.3 and 79.5,
   so a portrait phone is a city band between two empty plates.
3. No building projects into the bottom fifth of the desktop frame at all, and only 52 of 493
   on-screen trees are there: the near slope in front of the camera is bare.
4. The night frame is unchanged by the deck work in the measured bands (warm 12.7, flat 59.9 against
   60.0), because the deck covers only about 2.4 percent of the frame; the night still needs its
   clouds to be found by the eye or to be larger than they are.
5. The far country is now correctly separated in value but reads as static: the rows do not move and
   nothing on those hillsides is alive (no farm track traffic, no terraces with stone walls close
   enough to read at the near tier).

## Cycle 14: the hillside is worked land, and the streets have people on them

**The near slope stops being bare.** Measured before this cycle: 75 of 160 ray-sampled screen cells
were bare terrain, the bottom fifth of the frame held no building at all and only 52 of 493 on-screen
trees, and on the phone the bottom two tenths were 81.3 and 79.5 percent locally flat. What went in:

- 3146 metres of dry stone retaining wall in 101 levelled runs and 229 segments, each run capped
  level with its base sunk below the lowest ground it crosses, 15 metre segments with a pale cap
  stone. Seven rows cross the near slope at z 118 to 280, four short rows thread the portrait window
  between the lane, two tracks and a villa, and contour-traced walls run the outer shoulders and the
  crest. A run is rejected whole on any road, pavement, parcel, prop, house or track cell, and breaks
  where the switchback lane crosses it.
- Four farm tracks, 414 metres, each rooted on a real carriageway: 82, 95, 146 and 90 metres
  climbing off the ring, the east arterial and the west arterial, one joining the lane hairpin. Four
  paving bands each (dust base, trodden crown, two ruts) lifted 8 to 14 mm so nothing fights.
- Planting along the terraces, the treads, the tracks and the desktop shoulder, with the tree crown
  rebuilt as a hand-made 16 triangle bipyramid on an 8 triangle trunk so a tree is 24 triangles rather
  than 30, which paid for most of the density. Caps rose to 2600 desktop and 1280 touch.

Result, measured: trees planted 1011 to 1507 desktop and 736 to 1149 phone; tree centres in the
bottom two screen tenths 52 to 124 desktop and 32 to 104 phone; projected canopy coverage of the
bottom fifth 27.8 to 44.3 percent desktop and 29.2 to 55.5 phone; wall runs inside the frame 30
desktop and 23 phone, 11 of the desktop ones in the bottom fifth. An exact audit over every planted
trunk, every wall run sampled at a metre and every track sample reports tree-road 0, tree-building 0,
wall-road 0, wall-building 0, track-building 0, track-cross 0, with 15 track mouth samples counted
separately because a mouth is meant to touch asphalt.

**The streets have people on them.** Pedestrian pools went 300 to 700 desktop and the plaza share of
the cohort from 39.5 to 53.6 percent, with route choice now a two-draw tournament on quarter
activity so the busiest pavements carry the most walkers. Measured on the 1600x900 district frame:
pedestrians on screen 152 to 306 and in the plaza 60 to 164; scooters 14 to 22. Vehicles went 75 to
98 on screen and the vehicle density law was deliberately not touched: CONTRACT rule 1 fixes ring
density to switchRateAt, and at the 17.6 art hour the sample data reports no switches, so the crowd,
not the traffic, is what carries density at that hour. The phone was trimmed back to 200 pedestrians
and 100 car slots because its night lamp pool grows with the car cap and the gate is hard.

**Every state, measured on the frozen build.**

|state|calls|triangles|gate|
|---|---|---|---|
|desktop day 17.6|55|378437|400k|
|desktop night awake 21.6|53|381445|400k|
|desktop dream|51|331185|400k|
|desktop after a take|51|331977|400k|
|phone day, block tier|41|166499|45 calls, 180k|
|phone night awake|38|173055|45, 180k|
|phone dream|31|159179|45, 180k|
|phone after a take|31|159723|45, 180k|

Benches: desktop **180 fps, p1 175.4, worst frame 9.7 ms**, 58 calls, 383989 triangles at peak; phone
at a 4x CPU throttle **179.9 fps, p1 169.5, worst 14.3 ms**, 40 calls, 173487. Zero console errors on
every profile.

**Foreground, measured on the rendered frame.** Desktop flat area 39.8 to 33.2 percent, edge density
13.46 to 14.58 with the bottom tenths 46.6 and 61.7 flat becoming 33.5 and 41.8, and warm share in
the bottom tenth 40.2 to 54.5 as the stone walls and dry tracks arrive. Phone flat 61.0 to 56.4 with
its bottom tenths 81.3 and 79.5 becoming 56.4 and 62.0.

Flaws after cycle 14:
1. The hilltop village is one house. The crown loops attempt dozens and `diskFree(..., 2.1, B_SOLID)`
   rejects nearly all of them, because the alley rows stand 3.0 m from a lane centre whose corridor is
   itself flagged: the village that the brief calls "the only old thing" exists as a church and one
   stone house.
2. The bottom tenth of the desktop frame still measures 41.8 percent flat and 1.1 percent edge
   density: the terraces read as bands of stone but the foreground immediately under the camera is
   still the emptiest strip in the picture.
3. The phone's top tenth is 96.6 percent flat: at the block tier the frame top is sky and the cloud
   lane that was seeded for the district camera does not reach it.
4. Vehicles are still sparse at the art hour (98 on screen), and nothing about the traffic obeys the
   signals, which now cycle every 55 seconds.
5. The two new tracks are the only road-like things on the hillside with no traffic on them at all,
   which reads as a road closed rather than a farm track.

## Cycle 15: the hilltop village exists, and the expressway reads as a stream

**The village was one house.** Diagnosed exactly: the crown loop stands a row 3.0 m off each alley
centre and tests `diskFree(x, z, 2.1, B_SOLID)`, but the alley corridors are themselves flagged, so
almost every attempt rejected itself. The placement rule is now honest to what a hilltop village is:
the mask is `(B_SOLID & ~B_HOUSE) | B_CYPRESS` tested as an exact rectangle over the wall footprint,
so a terrace may share a party wall, plus a separate exact rectangle test that keeps the alley
walkable (front walls 1.05 m off each lane centre), plus a pairwise AABB test with a 2 cm party-wall
tolerance. The village ground and the church are reserved before the street walker runs, so the
walker now respects them instead of building nine blocks through the village.

Result: **27 houses desktop and 27 touch** (4 rows of 5 plus a 7-slot avenue terrace), widths 3.11 to
3.67 m, depths 3.11 to 3.90 m, walls 2.72 to 4.53 m, stone walls and stone roofs from the existing
palettes, roofs baked per house so the terrace's rotated roofs follow. Three cypress pairs at the
alley mouths, three behind the apse. Cleared by 1.0 m from the nave wall, 0.47 m from the nave eave,
0.54 m from the drawn avenue paving.

`world.crown.audit` reads `houses 27, overlaps 0, floating 0, sunk 0, forbidden 0, lanes 0,
cypresses 0, church 0, trees 0, walls 0`, and the audit was self-tested by moving a house onto the
church, lifting one, burying one, merging two and brushing a tree crown against a gable, each of
which it caught. Trees now respect `B_OLD` and reject any crown that would brush a house, which
removed one olive from the central alley and two planes from the north gables.

Projected on the district frame: all 27 houses land in the third screen tenth, which is where the
brief puts the old village, below the ridge and above the city.

**The expressway reads as a stream.** The count was left alone on purpose: the only traffic term in
the model is `switchRateAt`, the expressway base already rides rule 1's expression, and at 17.6 that
term is 0, so a busy motorway would be a lie; `activityLastHour` would be worse, putting a jam on the
deck during a focus block. What changed is presentation from the same term: cities by headway
(`speed * 1.4 s`, floored at 5.2 m, ceilinged by the deck per lane) instead of a golden-ratio slot
lattice that left holes of about 83 m, so at the art hour two cars a lane now sit 17.4 m apart and
read as a stream, and at a jammed hour the spacing closes to the old 4.4 m with no overlap. Brake
plate red at free speed went 0.24 to 0.34 up to 0.46 to 0.62 on the same 118 plates, same triangles.
Dead code from the lattice (`gcd`, `expLaneStep`, `expSlotK`, `expLaneSlots`) was deleted.

**Kerbside parking.** Desktop parked pool 140 to 160 cars by lowering the spur fill skip from 0.34 to
0.14, ring fill untouched so the ring's own occupancy is unchanged; 480 triangles main pass only, and
the parked pool casts no shadow at all, so the shadow pass is bit-identical.

|state|calls|triangles|before|ceiling|
|---|---|---|---|---|
|desktop day|55|377889|378437|400k|
|desktop night awake|53|376385|381445|400k|
|desktop dream|51|325643|331185|400k|
|desktop after a take|51|326435|331977|400k|
|phone day|41|165023|166499|45 calls, 180k|
|phone night awake|38|171219|173055|45, 180k|
|phone dream|31|157365|159179|45, 180k|
|phone after a take|31|157909|159723|45, 180k|

Benches: desktop **180 fps, p1 172.4, worst frame 9.7 ms**, 58 calls, 378929 at peak; phone at 4x CPU
throttle **179.9 fps, p1 169.5, worst 14.3 ms**, 40 calls, 171651. Zero console errors everywhere.

Flaws after cycle 15:
1. Farm tracks still carry nothing: the world publishes only a metres count and an audit, no
   polylines, so no vehicle can be driven along them and four dirt roads on the near slope read as
   closed. The world has to publish the track polylines before agents can put a slow ute on one.
2. The night dream frame clips 1821 pixels to pure white in one patch (y 544 to 635, x 708 to 928),
   which contradicts the brief's "deep blue never black, every lit window warm": the chantier's
   brightest core needs to stay under the clip with a warm hue instead of white.
3. Desktop night awake is 376385 triangles, only 23.6k under the ceiling, and the night lamp pool
   scales with the car cap, so any further night addition needs a subtraction first.
4. The phone's bottom two tenths are still 56.6 and 65.2 percent flat even after the hillside work,
   because the block tier looks down on the near slope from a lower camera where the terraces are
   outside the frame.
5. The village is a crescent of stone houses in the third tenth and reads correctly, but at that
   distance its roofs are the only thing that distinguishes it from the new city: it needs one
   vertical accent (a campanile already exists) and a couple of cypress spikes to be unmistakably
   old from the air.

## Cycle 16: no more white hole at night, and the farm tracks carry a ute

**The clipped patch, identified before it was fixed.** The white plate the parent measured was not a
lamp: it was the dream's provisional road (`road-provisional` in `dream.mjs`), an unlit vertex
coloured corridor painted 0xd9dbd6 and 0xffffff, running between the two quarters the model pairs.
The dream's ten percent district light cannot dim an unlit material, so the road's 0.694 linear
neutral sat under an additive stack of the dream light strip, two warm flood pools, the world's
sodium ribbon and the white crossing paint, and the sum measured (2.27, 1.71, 1.17) linear. Isolation
over 960 deterministic frames attributed it: the road 1334 pixels, the flood pools 601, the sodium
ribbon 398, the white paint 229, bloom 363.

**The fix is a night white guard, not a darker road.** The composite now eases any summed value whose
low channel reads pale above a shoulder, with one shared scale on all three channels so hue cannot
move, gated on the night flag so the day frame is bit identical (measured: zero changed pixels
between seeded A/B runs). Saturated emitters are untouched by construction: the audit shows no pair
of warm sources can reach white on its own (best pair's low channel 0.727 linear), so only the pale
neutral sums are eased, which is exactly where the white came from.

|night metric|before|after|
|---|---|---|
|fully clipped pixels, dream|2581 (0.179 percent)|**0**|
|fully clipped pixels, awake|1844 (0.128 percent)|**0**|
|luma above 0.95, dream|0.507 percent|**0.093 percent**|
|mean luma, dream|0.2279|0.2273|
|dark share under 0.18, dream|56.224 percent|**56.224 percent**|

Re-measured by me on the delivered frame: dream clipped **0** pixels, awake **0**, luma above 0.95
0.095 percent, mean 0.2269, dark 56.16. The patch keeps the five brightest pixels in the frame, now
warm yellow (0.965, 0.835, 0.708), and stays 3.2 times the window field and 3.8 times the sky.

**The tracks carry traffic.** The world publishes its track polylines as `world.roads.tracks` in the
same shape the lane tables already use (`n, len, step, xs, zs, nx, nz, ys`, the last being the drawn
dirt surface), and five movers now walk them at 1.3 to 2.2 m/s, one per track with two on the 146
metre east shoulder, laterally clamped to 0.7 m of the centreline with a hard 0.9 m clamp. Each end
of each track has a 2.6 second swept reversal, so nothing teleports and the heading crosses 180
degrees through the arc. Verified from the live instance matrices: maximum centre distance from the
polyline 0.700003 m, per frame position step at most 0.156 m, yaw step at most 0.181 rad, and a 60 fps
replica over 26 turns measured 0.041 m and 0.030 rad. Tint 0xd9a43a, the truck palette's dusty
ochre, over the van's cream material. Cost: 5 instances at 24 triangles, no new mesh, material or
call, and the scene inventory is identical to the control build.

|state|calls|triangles|before|
|---|---|---|---|
|desktop day|55|378009|377889|
|desktop night awake|53|376505|376385|
|desktop dream|51|325739|325643|
|desktop after a take|51|326555|326435|
|phone day|41|165143|165023|
|phone night awake|38|171339|171219|
|phone dream|31|157485|157365|
|phone after a take|31|158029|157909|

Benches: desktop **180 fps, p1 172.4, worst 9.7 ms**, 58 calls, 379049 at peak; phone at 4x CPU
throttle **179.9 fps, p1 172.4, worst 14.5 ms**, 40 calls, 171771. Zero console errors on desktop day,
desktop night and phone day.

Flaws after cycle 16:
1. Cars still drive through each other: at the busiest hour 13 pairs of car bodies sit closer than
   1.2 m and the worst is 0.05 m. The movers have no car following model, so a fragmented hour stacks
   them, and this is the one pixel-perfect rule the file has never satisfied.
2. Desktop night awake is 376505 triangles, 23.5k under the ceiling, and the night lamp pool scales
   with the car cap; the budgets are now the binding constraint on every night cycle.
3. The phone's top tenth is still 96.6 percent flat: at the block tier the cloud lane does not reach
   the frame top.
4. Nothing on the frame shows the tram as the modern white vehicle with a coloured stripe the brief
   asks for at district distance: it is there, but it is a white streak among white streaks.
5. The market square's crowd is now 164 of 306 pedestrians, but the stalls themselves are four
   canopies; the brief's "busy market square with white and red striped stall canopies" reads as a
   small cluster rather than a market.

## Cycle 17: cars stop driving through each other, and the square becomes a market

**The follow law.** Every car used to integrate its own speed from a position-based congestion
factor, so two cars could occupy the same arc length: at 13:00, 13 pairs of bodies sat closer than
1.2 m and the closest was 0.05 m apart. There is now a per-lane following model. Each lane is a loop
in arc space split into 6.5 m bins (372 bins total across two ring lanes, two avenue lanes and the
spur lanes, all allocated once); a flat `Int32Array` head is cleared each frame and a chained next
array links the cars in a bin; a car looks at its own bin plus three forward, 19.5 m of reach, which
covers the fastest following target. Speed is then `v = min(v, (gap - 4.6) / 1.1)`, floored at zero,
where the gap is the nearest car ahead in the same lane and direction; a car at or inside 4.6 m
stands with brake 1 through the existing brake path, so a queue reads as a line of red tails. Ring
lane offsets were quantised to plus or minus 1.2 m so two cars whose continuous offsets had opposite
signs cannot sit a decimetre apart laterally. Nothing about the data changed: the ring count, the
stop-and-go wave, the avenue green wave and the spur counts are exactly as they were, and a queue is
what the density now produces.

Ablation, with the spawn search and clamp removed and everything else identical: desktop 13:00 went
from 125 instant pairs, worst 0.016 m, 5511 same-lane pair-samples, to zero instant pairs and a
4.602 m same-lane floor. The measured floor across all four audited states is 4.602 m desktop and
5.620 m phone.

**What is honestly left.** On the live page after the change: 4 pairs of car centres still inside
1.2 m and the worst is 0.221 m, all of them cross-family, at the avenue's two carriageway crossings
and the three spur starts, where two roads share asphalt. A per-lane gap law cannot arbitrate a
junction. That is the next cycle's job, and it is stated rather than claimed away.

**The market.** The plaza's four canopies and twelve counter instances are now **14 stalls** in two
rows of seven at 3.0 m pitch across a clear central walk to the fountain: stripped counter with a
five-strip red and white cloth front, cream back panel, four posts, a six-strip canopy tilted to the
aisle, a six-strip valance and three produce trays (tomatoes 0xd8392a, citrus 0xf0a11c, greens
0x4f9a34, aubergine 0x5b3a70), plus eight larger ground crates in the same colours behind the rows
because a canopy at 2.14 m hides its own counter from a 40 degree camera. Furniture: three hand
carts, four parasols over cafe tables on the entrance flanks, two name boards ("Mercato", "Piazza
del Mercato") in the existing sign atlas, all rank 1 so the phone keeps them.

Clearance was enforced rather than assumed: `marketFree` rejects a placement near a crossing, within
4.0 m of the fountain, or on any built, prop, tram, avenue, sidewalk, street, ring, roundabout, spur,
via or corridor cell, while the plaza itself is the one permitted surface. 15 stalls tried, 14
placed, 0 rejected; nearest stall centre to the fountain 5.60 m against a 5.30 m limit; closest stall
pair 3.00 m against 2.60. Draw calls unchanged at 55 desktop and 41 phone: the market lives in the
shared detail pool, whose budget rose from 6440 to 6653 so the rank 2 tail keeps its drawn length.

|state|calls|triangles|before|
|---|---|---|---|
|desktop day|55|381393|378009|
|desktop night awake|53|379889|376505|
|desktop dream|51|329147|325763|
|desktop after a take|51|329939|326555|
|phone day|41|167277|165143|
|phone night awake|38|173469|171339|
|phone dream|31|159615|157485|
|phone after a take|31|160159|158029|

Benches: desktop **180 fps, p1 172.4, worst 9.5 ms**, 58 calls, 382433 at peak; phone at 4x CPU
throttle **179.9 fps, p1 169.5, worst 14.5 ms**, 40 calls, 173901. Zero console errors on every
profile. Day frame metrics held: flat 33.6 percent, edge 14.6, warm 57.0, dark 10.3.

Flaws after cycle 17:
1. Four cross-family car pairs still intersect at five shared crossings (worst 0.221 m), because
   there is no junction priority.
2. A share of the plaza's 164 walkers stand inside the new stalls: the crowd scatters over its own
   ellipse and never reads the occupancy grid, so people stand in counters.
3. The phone block-tier frame's top tenth is 96.6 percent flat: the steady-hour cloud lane was
   computed for the district camera and does not reach the block-tier frame.
4. At Street tier the corners of the frame are 43 to 52 percent flat: the road surface has kerbs,
   zebras and edge lines but no wear at all, so the asphalt around the camera is one plain tone.
5. Desktop night is 379889 triangles with 20k of headroom, and the night lamp pool still scales with
   the car cap, so the night is one addition away from the ceiling.

## Cycle 18: junctions get a rule, the crowd stops standing in the counters, and the asphalt gets wear

**Junction priority.** Two roads sharing asphalt cannot be arbitrated by a per-lane gap law, so the
minor approaches now yield to the ring, the arterial. Each avenue and spur car knows its own arc
distance to the five shared crossings (computed once at build by walking the lateral offset against
the ring table and bisecting 22 times, with a 3.0 m meet tolerance because spur first points sit up
to 1.71 m off the ring spline). Within 14 m of a crossing, a car may pass its stop line only if the
nearest ring car, over both ring lanes across a 52 m window plus a 2.6 m tail, is further than
`((ahead + 3.6) / max(free, 2) + 0.5) * 9.4`, a time-scaled right of way that credits the ring with
its own lane speed. A held car eases to rest 5.2 m before the crossing centre through the existing
follow clamp, so it lights its tails through the same brake term. A car may not commit unless its
leader already stands 3.6 m past the crossing plus a follow gap (8.2 m), so a queue stopping just
past the stop line cannot leave a committed car standing on the arterial: the box is never blocked.
Spawns validate against the lane grid first and retry rather than appearing inside a neighbour.

The measured result is what the rule promises and one thing it does not:

|hour|pairs of car centres under 1.2 m|worst distance|avenue crossings in 30 s|
|---|---|---|---|
|13:00|**0**|none|**0**|
|09:30|**0**|none|4 and 3 at the two crossings|
|before the rule|4|0.221 m|flowing|

So the residual intersection is closed, and the honest cost is that at 13:00 the ring is saturated
(104 ring cars at about 6.3 m mean spacing across the pair of lanes) and a priority road with no
signal never leaves a 14 m gap, so the avenue holds at its stop lines for that hour. No car is ever
left in the box, but a hero avenue that never moves is not the picture the brief asks for, and the
next cycle replaces the priority rule with the signal phase the junction already has lamp heads for.

**The crowd and the counters.** The plaza scatters its walkers over its own ellipse and never read
the occupancy grid, so people stood inside the new stalls: at 13:00, 28 walkers were within 1.3 m of
a stall centre, the nearest 0.033 m from the centre of a counter. A push-out pass now moves any
walker whose position falls inside a stall disk out to `radius + 0.25` along the row's outward
normal, along with its walk direction, with a bounded four-times span halving through a
segment-to-disk test and a standing-walker fallback. Measured before and after on the same build
with only the repair block removed: 28 inside at 13:00, 63 at 10:00, 22 at 21:00, all with a 0.033
to 0.198 m nearest approach, become **0 inside and a 1.55 m nearest approach** at all three hours,
and the square's walker count does not change at all, because the repair only repositions.

**The street surface.** Street tier had no surface incident at all: the carriageway around the
camera was one flat tone between the kerbs and the zebras. Five graphite tones now drift across the
ring, avenue and spur carriageways on a 9 m coarse and 3.1 m fine key; darker repair patches; two
0.42 m polished wheel tracks per lane at the 1.56 m car track; 9 kerbside gullies and 8 round covers
in cast iron; tactile bands at every zebra's kerb ends, three 0.2 m strips making a 0.6 m band; and a
0.064 m pavement joint on the 1.5 m sample pitch at the pavement's own tone times 0.9, so the joint
never reads as a drawn line. Total wear spread 0.087 of value against the 0.227 base, inside the
0.12 cap, and every piece takes its height from the band actually drawn rather than from the terrain
beneath it: a flat terrain-based placement would have sunk 23 percent of the wheel tracks and 41
percent of the patches under the road, because the cross-sections are concave. Cast iron clears the
white paint by 0.22 m laterally and the nearest zebra bar by 5.24 m.

Measured on the street-tier frame, four rotations: edge density 12.86 to 13.44 per hundred pixels
(plus 0.58) and flat share 33.3 to 32.4 percent (minus 0.9); per quadrant gradient 42.26 / 39.88 /
36.38 / 33.73 to 42.62 / 39.93 / 37.5 / 36.56. The change is real and small: the asphalt now has
grain and cast iron, and the corners of a street frame are still the flattest part of it.

|state|calls|triangles|before|
|---|---|---|---|
|desktop day|55|387153|381393|
|desktop night awake|53|385649|379889|
|desktop dream|51|334907|329147|
|desktop after a take|51|335699|329939|
|phone day|41|171723|167277|
|phone night awake|38|177915|173469|
|phone dream|31|164061|159615|
|phone after a take|31|164605|160159|

The phone night frame was the one at risk: the new ground took it to 177915 against the 180000
ceiling, so the touch build now lays pavement joints every second sample (3 m pitch instead of 1.5,
which is 4 px wide on a 412 px screen anyway) and keeps 2085 triangles in hand. Benches: desktop
**179.9 fps, p1 175.4, worst 12.6 ms**; phone at 4x throttle **179.8 fps, p1 169.5, worst 20.4 ms**,
under the 33 ms bar with the worst frame the ground work pushed up from 14.5.

**Allocation, measured rather than asserted.** The in-page heap over 24 s sawtooths between 27.2 and
39.2 MB with three collections and no growth (first 34.4, last 35.0). A 6 s heap sample attributes
the garbage to three.js internals, not to the simulation: `getParameters` and
`getProgramCacheKeyParameters` together hold the largest share, with `sort` and `join` from the
renderer's list handling behind them, and the simulation's own frames (`stepRingCars`,
`stepAvenueCars`, `stepExpressway`, `gridAhead`) hold about 3.5 MB of a 12 MB sample. There is no
visible cost: the worst frame on a 4x throttled phone is 20.4 ms.

Flaws after cycle 18:
1. The avenue starves at 13:00: with the arterial saturated, the priority rule lets zero avenue cars
   cross in 30 s. Correct as a priority junction, wrong as a picture.
2. Street tier's corners are still the flattest part of the frame: the new wear moved edge density
   up 0.58 and flat share down 0.9, which is not the incident the street references show.
3. Phone night is 177915 of 180000: the ground work consumed almost all of the phone's headroom,
   so the next phone-visible addition needs a cut somewhere else first.
4. The signal phases are per head and hashed, so two approaches of one junction are not
   complementary: the lamps are a beautiful display that no car obeys and that a careful viewer
   would read as wrong.
5. Garbage collection is three.js renderer churn at about 0.4 collections per second, unchanged by
   this cycle; the simulation's steps are allocation-free but the frame is not.

> OWNER NOTE 07:25: budget extended by 20 hours (until ~03:30 on 2026-11-10). Read OWNER-NOTES.md before the next cycle and fix its five items first. Do not write a closing summary before 03:30.


> OWNER NOTE 07:27, superseded the note above within two minutes: the budget is now ONE WEEK, until
> about 07:30 Paris on 2026-11-16, and the procedural-only rule is lifted. Free CC0 3D packs are
> allowed and expected; addons are allowed through the import map; every placed model must still be
> instanced; desktop draw calls may rise to 120 and phone to 70 if the fps gates hold; the
> deliverable becomes `index.html` plus `assets/`, served by `node tools/serve.mjs`, working offline
> after the first load, with a procedural fallback. This cycle starts on the owner's first five
> review items and on the packs.

## Cycle 19: real CC0 models enter the city, and the horizon stops being mud

The owner's verdict on cycle 18 was "not a masterpiece yet", with five ordered items: a smeared
purple-brown top band, blotchy terrain, floating blocks, a closed ring, and an overall toy look.
Items 1 and 2 are fixed here and measured; item 5 is attacked with real models; items 3 and 4 are
deferred to the next cycle on purpose, because the block layout is going to be re-cut around model
footprints anyway and doing it twice wastes the budget.

### The licence finding first, because it changed the plan

The owner named Kenney, Quaternius and Poly Pizza as CC0 sources. `tools/fetch-assets.mjs` resolves
each pack from its official page, verifies the zip signature, asserts the licence text contains
`CC0` and copies the licence verbatim into `assets/LICENSES/`. That worked for all five Kenney kits
(each `License.txt` says `License: (Creative Commons Zero, CC0)`). It did NOT work for Quaternius:
`quaternius.com/license.html` now serves the **Quaternius Asset License v1.0**, last updated
2026-08-28, which forbids redistributing the assets as assets, and the pack pages point at Google
Drive folders with no glTF at all (the buildings folder holds 102 .blend and .fbx files per pack and
zero .glb). The CC0 badge on those pages is stale. Poly Pizza still labels 1,395 of the 1,411
Quaternius models `CC0 1.0`, and that is where the fallback download came from, but a CC0 label that
predates a restrictive licence change is not something to ship without the owner's word.
So **this cycle ships Kenney only**, 5 packs, and the Quaternius licence evidence is kept in
`assets/LICENSES/` with the contradiction recorded in `SOURCES.md` for the owner to rule on. Raw
packs stay in `.cache/packs` (228 MB, never shipped).

### Pipeline, measured

`tools/fetch-assets.mjs` (download, verify, extract, licences, inventory) -> `tools/pack-assets.mjs`
(parse GLB containers with no dependencies, dedupe each kit's palette PNG by SHA-256, bake node
transforms, recentre to a ground-contact origin, face +Z, merge one material per pack) ->
`assets/<pack>.glb` + `assets/manifest.json` -> `src/assets.mjs` (`GLTFLoader` from the pinned
addons, merge primitives, one `MeshStandardMaterial` per pack, `makeInstanced`) -> `assets/sw.js`.

| | |
|---|---|
| packs shipped | 5 Kenney kits, one GLB each |
| models in the manifest | 74 (67 full plus 7 low vehicle variants) |
| manifest triangles | 47,791 (district 11,508, street 32,785, lows 3,498) |
| `assets/` total | 3,093,739 bytes, 2.98 MB of the 60 MB budget |
| materials per pack | 1 (a 512 px dithered palette, deduped to one 11 to 12 KB PNG each) |
| nature kit | no texture at all: `baseColorFactor` baked into `COLOR_0`, one vertex-colour material |

The GLB path also needed a real fix in the loader: `COLOR_0` was being dropped, so every nature-kit
tree rendered with the wrong colour. `bakeNode` now reads it into a float `color` attribute and
`createPackMaterial` turns `vertexColors` on only for packs that carry it.

### Owner items 1 and 2: the top band and the terrain

The purple band had three independent causes and all three had to go: a lilac air/fog/haze colour
chain, a violet-rose distance grade covering 35 percent of the frame, and far-grove tree instances
planted from 240 to 700 m whose crowns were then smeared sideways by the tilt-shift blur while the
distance grade cooled them. The fix removes the far grove entirely, makes the air warm cream to pale
blue, keeps the fog colour matched to the horizon so there is no purple mix, cuts the blur radius
from 0.70 to 0.55 and the cool-distance amount from 0.35 to 0.18.

|top tenth|cycle 18|now|
|---|---|---|
|purple share|57.4 percent|**0.0**|
|blue share|0.1|73.9|
|luma|0.392|0.719|
|edge density|0.16|0.29|

The terrain was 37 x 31 m hashed field cells with per-cell value noise, which is exactly the blotchy
orange-green-brown the owner saw. It is now a calm dry-golden base with olive rows and terraced
strips on an 86 x 62 m field grid, a wall on every fifth riser, and a screen-height air mix applied in
`onBeforeCompile` so the ground fades into the haze with distance instead of staying a lit plate.

|terrain region|before|after|
|---|---|---|
|flattened RGB standard deviation|0.1407|**0.0891 (-36.7 percent)**|
|mean-channel deviation|0.1041|0.0310 (-70.2 percent)|
|dominant tone|scattered patches|one warm tone, hue 40, 10 percent of frame|

The honest cost, recorded rather than hidden: calming the terrain also desaturated it. Lower-third
saturation fell from 0.391 to 0.267 with purple at zero. The model integration below won most of that
back (0.341) and the art pass in this same cycle is closing the rest.

### Models in the city: a substitution budget, not an addition

The gates did not move (desktop 400k triangles, phone 180k), so the models had to be paid for out of
the existing frame. That turned out to be possible because a model building already carries the
windows, awnings, balconies and roof detail that the procedural facade layer draws separately: a
model lot gives up its windows, frames, shop glass, parapet and socle. Measured by rebuilding with
the model-lot cap set to zero, a model core lot **saves** 281.6 triangles net.

That is the whole design: substitute where the model is better per triangle, keep the procedural
layer everywhere else, and make the split a measurement rather than a taste.

|category|desktop|phone|
|---|---|---|
|core model lots|42 (low-detail k 11, h 11, e 9, b 8, n 3)|22 (h 6, e 10, b 6)|
|towers|0 model (see the flaw list)|0 model|
|villas|4 (type-n 2, g 1, a 1)|0|
|model trees|54 (42 cheap crowns, 6 cypress, 6 palm)|53|
|model lamps|41 (26 curved, 15 square)|0|
|parasols / props|15 / 12|0 / 0|
|model instances / triangles|46 / 13,219|22 / 4,428|

Every instance carries a per-instance tint from the Mediterranean facade palette, a yaw, and a small
scale jitter, so a kit of five buildings does not read as five buildings.

The world audits stay clean: `treeRoad 0, treeBuilding 0, wallRoad 0, wallBuilding 0, trackBuilding 0,
trackCross 0`, with 1,011 trees and 247 building instances.

### One substitution was measured as a regression and reverted

Four `building-skyscraper-*` models replaced the procedural glass towers and were then measured
against the same frame: the tower boxes read luma **42.7 / 61.6 / 46.7 / 58.1** at night against
**73.9 / 104.9 / 61.5 / 86.2** for the procedural tower, and the street tier lost 1.80 edge density
in the tower quadrant. The cause is structural, not a tuning error: a model tower has no lit window
grid, and lit windows are one of the eight data rules. The cap is parked at 0 with the code path
kept, the single procedural tower is back (luma 74.28, **27.2 percent warm pixels** at night), and
model towers can return only together with a procedural window overlay on their facades.

### The budget came from idle instances, not from hiding the city

Every instanced pool submitted its whole capacity. On the day frame the pedestrian mesh submitted
700 instances for 86 live people, which is 30,800 triangles for a crowd that occupies about 3,800.
Each pool now writes `mesh.count` from its live prefix once per frame. This needed one real ordering
fix: the night lamp pool had the bus band and tram blind at the tail, so a short live car count would
have trimmed a light that was on screen; those moved to the front.

|frame|at capacity|live prefix|reclaimed|
|---|---|---|---|
|desktop day|383,525|349,449|**34,076**|
|desktop night|387,613|345,573|42,040|
|phone day|175,293|167,677|7,616|
|phone night|181,115|171,987|9,128|

That is what kept three of the four phone states legal, and it is what paid for the models.

While proving nothing had vanished, one real bug surfaced: the expressway expiry loop replaced a
departed car but not its scooter rider, leaving a rider blob hanging over the parapet. Fixed.

### A measurement trap worth writing down

The phone appeared to be 234,161 triangles against a 180,000 ceiling, with the pedestrian and car
pools at desktop caps. It was an instrument fault: Puppeteer's `emulate({hasTouch:true})` leaves
`navigator.maxTouchPoints` at 0, and `NARROW_TOUCH` in `agents.mjs` correctly requires a touch
screen. With `maxTouchPoints` injected as 5 before navigation the same build measures **167,379
triangles and 41 calls**. Every phone number in this cycle is measured with that injection, and the
DPR must come from `Emulation.setDeviceMetricsOverride` because the tool-level `deviceScaleFactor`
is ignored. No product code was changed for it.

### Vehicles: ready, small, and not yet in the scene

The Kenney car is 2,032 triangles, which is unaffordable at 406 instances. A dependency-free
edge-collapse decimator in the pack tool now produces seven `-low` variants at about 500 triangles
each, gated on a measure rather than an opinion: silhouette IoU against the full-detail render at
three yaws is **0.946 to 0.984** (bar 0.93) and mean colour difference **0.010 to 0.051** (bar 0.08).
Two failures were diagnosed and fixed on the way: a UV seam crease weight, and a paint penalty,
because geometry-only error metrics price a flat painted windshield at zero and let a body triangle
repaint it. Contact sheet: `screenshots/assets/car-lod-compare.png`.
Integrating them into the moving fleet is the next cycle's first job.

### Offline and fallback, verified

`assets/sw.js` caches `index.html`, the manifest, all five pack GLBs, `sample-export.json` and the
three jsDelivr modules (including `BufferGeometryUtils`, which `GLTFLoader` imports and which the
first version of the worker missed). Offline reload with the network disabled: 94 calls, 350,857
triangles, 74 models loaded, worker controlling the page, no console output. The only failed request
is the browser's own `favicon.ico` probe, which the page deliberately never asks for.
A local-file open cannot fetch the packs, so it now skips the request entirely and shows the
procedural city with a completely silent console. `?noassets=1` on the served build is the same
procedural city: 54 calls, 331,793 triangles, `assets.loaded: false`.

### Checks, and two stale assertions

`node tools/check-dom.mjs` still asserted "exactly one URL" and "the import map binds three only",
both of which predate the owner's addons allowance, so it failed on a legal build. Both assertions
now allow exactly the pinned addon directory and nothing else (99 checks, all PASS). `check.mjs` is
35/35, `check-assets.mjs` 74 models PASS, `check-sample.mjs` PASS, `smoke-data.mjs` 120 passed,
`check-pixels.mjs` PASS on the day frame.

### The avenue gets real cars, on one carriageway

The low variants went into the moving fleet as one `InstancedMesh` per model (six meshes, 35
instances), written through the existing column-major matrix path with one dirty-span upload per
pool per frame. The mapping is a uniform fit, `min(1, 1.92 / w, 4.1 / d)` times a 0.94 to 1.0 jitter,
so every drawn body stays inside the old 1.92 x 4.1 envelope and no lane offset, follow gap or
junction rule had to move. Measured drawn extent per model: width 1.854 to 1.919, length 3.481 to
4.069, height 1.569 to 2.057. 17,482 triangles for 35 modelled cars.

All 63 non-scooter avenue slots would measure **395,851** triangles at the busy hours, which is inside
the hard 400,000 gate but only 4.1k under it, because the world alone already sits at about 371.7k.
So one carriageway is modelled (slot parity, which is exactly how the avenue's two lanes are
assigned) and the other keeps the procedural car. That leaves the ceiling clear for the next cycle's
facade windows instead of spending the last 4k here. `MODEL_LANE = -1` models both carriageways in
one line if the budget is ever found.

The swap is proven per pixel, not asserted: for two identified cars, a one-instance mesh built from
the old procedural geometry at the model's own transform was rendered one frame later in the same
screen box. The model frame's modal colour is the model's own palette family (suv `#343434` 184 px,
taxi `#a47c1c` livery 56 px) where the box frame is 42.1 and 46.4 percent the old body colour, and
the cross-contamination is 0 and 55 px.

Night plates still sit on the bodies: lamp height 0.569 to 0.587 m above the base, tail pair 0.23 to
0.46 m beyond the local skin, zero vertices behind a plate.

The intersection test that closed cycle 18 stays closed: 729 samples at 2 Hz across 08:00 to 14:13
simulated, **0 pairs of car centres under 1.2 m** in every hour, closest pair 1.514 m at the quietest
hour and 1.9 m at 13:00 with 349 live bodies.

### The art pass: a skyline and the golden light back

Cycle 18's frame had one tower where the brief asks for a cluster. The tower pass could only find one
spot, because it only looked along single kerb lines and slide-retried within a rigid footprint. With
the search widened (both kerbs of the three upper streets every 2 m, the ring's inner frontage every
6 m, distance-ranked dedupe, then a slide-and-retry through the same `tryParcel` occupancy rules) the
build places **4 towers** from 682 candidate fronts: tops at 65.0, 87.3, 57.0 and 85.3 m, lit windows
in the tower mesh up from 1,307 to 1,827.

Night proof for all four tower screen boxes: luma **71.6 to 83.0**, warm pixels **21.6 to 28.7
percent**, edge density 27.2 to 34.0, against the procedural reference 74.3 / 27.2. The lit window
grid is there, which is the whole reason the model towers stay parked.

Chroma came back by palette and instance-colour work rather than a global grade, and clipping is
measured at 0.000 percent:

| | cycle 18 | after the terrain calm-down | now |
|---|---|---|---|
|whole frame saturation|0.397|0.256|**0.386**|
|middle bands (3 to 7)|0.377 to 0.449|0.310 to 0.343|**0.407 to 0.499**|
|lower third saturation|0.391|0.267|**0.477**|
|lower third warm share|68.5|65.5|**80.7**|
|top tenth purple share|57.4|0.0|**0.0**|

So the frame is now warmer and more saturated than the cycle 18 reference in the city, with a clean
sky where the reference had mud. The five dominant quantised colours are all warm ground and facade
tones (hue 30 to 40, saturation 0.59 to 0.67) plus two sky tones.

### Final gate table (measured this cycle, quiet browser, 1600x900 / 412x915)

|state|calls|triangles|gate|
|---|---|---|---|
|desktop day|94|354,973|120 / 400,000|
|desktop dream|83|300,081|120 / 400,000|
|desktop night awake|**100**|**361,687**|120 / 400,000|
|desktop street tier|99|364,239|120 / 400,000|
|phone day|47|165,677|70 / 180,000|
|phone dream|36|156,437|70 / 180,000|
|phone night awake|50|166,847|70 / 180,000|
|`?noassets=1` desktop|54|331,793|120 / 400,000|

Bench JSON, bench mode, quiet browser:
- desktop 1600x900: `{"avgFps":179.9,"p1LowFps":175.4,"maxFrameMs":12.5,"calls":100,"triangles":362973,"mode":"desktop"}`
- Pixel 6 412x915, DPR 2.625, `maxTouchPoints` 5, CPU throttle 4x verified at a measured 4.27x on a
  synthetic loop: `{"avgFps":179.8,"p1LowFps":172.4,"maxFrameMs":21.1,"calls":50,"triangles":167433,"mode":"mobile"}`

The 999.6 ms and 1016.8 ms maximum frames reported during the cycle do not reproduce: two agents were
driving the same shared headless browser with eight live WebGL tabs at the time. On a quiet browser
the desktop maximum frame is 12.5 ms. The lesson is to bench with every other tab closed.

### What the delivered folder is

`index.html` (one file, assembled from `src/`) plus `assets/` (3.0 MB: five pack GLBs, the manifest,
the service worker, the licences). `node tools/serve.mjs` serves it; the phone download is the same
3.0 MB and it works offline after the first load.

### A detector for the defect the brief calls flicker

"No z-fighting anywhere, no flicker" had been asserted, never measured. A coplanar surface pair
shimmers between two colour values along its seam, so in a single frame it looks like salt and
pepper on an otherwise flat surface. `.cycles/specks.py` counts exactly that: a pixel that differs
from all four neighbours by more than 0.22 while those neighbours agree with each other within 0.05,
inside a 5x5 window that is at least 60 percent one flat value. The window test is what separates a
defect from legitimately busy texture, and it was added after the checkerboard validation below
exposed the naive version.

Validation on synthetic frames, so the number can be trusted before it gates anything:

|synthetic frame|expected|reported|
|---|---|---|
|flat field with 7 planted lone specks|7 isolated, or fewer where two touch|5|
|1-pixel checkerboard (neighbours agree, window does not)|0|0|
|smooth two-axis ramp|0|0|

Baseline of the shipped build, as a share of pixels:

|frame|specks|percent|
|---|---|---|
|desktop day|65|0.0045|
|desktop dream|36|0.0025|
|street rotation 0|47|0.0033|
|street rotation 1|**103**|0.0072|
|street rotation 2|21|0.0015|
|street rotation 3|39|0.0027|
|Pixel 6 day|1|0.0003|
|Pixel 6 night|1|0.0003|

So there is no z-fighting catastrophe anywhere. The single outlier is street rotation 1, where 70 of
its 103 specks fall in one cluster at x 0..400, y 112..225, which is a localised defect in the
upper-left of that view rather than noise, and is now a named target rather than an impression.

### Flaws after cycle 19

1. **Street-tier corners are the thinnest part of the frame.** Across four rotations the worst corner
   is 62.3 percent flat with edge density 4.29 (rotation 1, top-right), and three more corners sit
   near 50 percent flat with edge 4.5 to 6.5, while every centre measures 10.5 to 22.7 edge. The
   camera is close at street tier, so a quarter of the frame falls on blank wall, empty pavement or
   sky. The next cycle puts instanced detail exactly there: shop signage and awnings at eye level,
   parked cars and bins along the kerb, awnings and parasols, crowds on the near pavement, and a
   litter of small props on the ground plane.
2. **Model facades have no lit windows.** Lit windows are one of the eight data rules and the 42 core
   model lots currently go dark at night while the procedural lots light up from the activity data.
   The fix is a procedural window overlay on the model facades (2-triangle planes, cheap), which is
   also what would let the model towers come back.
3. **Only one carriageway of the avenue has real cars.** The other is still procedural boxes. It is a
   deliberate budget choice this cycle (both would be 395.9k of 400k) and it should be closed by
   finding the 14k elsewhere rather than by spending the ceiling.
4. **The narrow-but-not-touch window is inconsistent.** `main.mjs` decides mobile by
   `(pointer: coarse) || innerWidth < 800`, while `agents.mjs` decides its caps by
   `innerWidth < 800 && maxTouchPoints > 0`. A 700 px desktop window therefore gets the mobile render
   profile but desktop population caps, and its frame can pass the phone triangle ceiling. They must
   agree, and the honest rule is to keep the desktop caps when there is no touch, but then not claim
   the phone profile either.
5. **The night dream frame is 54.7 percent flat** with edge density 8.69, and the lower third is 54.3
   percent flat at luma 0.231. The dream chantier is supposed to be the brightest thing in the frame,
   and at this measurement it is not carrying enough lit incident to hold the eye: the works, the
   provisional road and the two cards need to be the brightest cluster by a measured margin.

Also worth changing, though not a flaw in the art: `sample-export.json` is now precached by the
service worker so an offline visit opens the same day as the served one, and the model towers stay
parked until flaw 2 is fixed.

# Cycle 20

Two deferred owner items, the lit windows on the model facades, and the profile rule that had been
copied in three places. The street-detail pass and the final tables follow in the next section.

### The city stops ending at the ring (owner item 4)

The urban region had no rule at all for the outer belt: kind 3 lots were placed by a scatter with no
region test, which is exactly why the owner saw large blocks standing alone in a field. That scatter
is deleted and replaced by a frontage rule.

- **Five arterials** instead of three. The three existing ones are extended past the frame edge, and
  two are added: a lower-west avenue and a hillside arterial that climbs the slope. The two west
  avenues sit 40 m apart rather than the 24 m that left no room for a block between them.
- **One buildable region, stated**: the ring's outer face for 32 m, union a 36 m band either side of
  every arterial, and nothing else. It is rasterised into a 2 m mask over the whole grid (22,250
  cells) and `tryParcel` for kind 3 now requires front *and* body inside that band, so a slab cannot
  stand in a field by construction rather than by tuning.
- **76 fabric lots**: ring 24, lower west 15, hillside 13, east 13, west 11. Buildings 229 -> 291 on
  desktop and 261 on the phone. Frontage reuses the existing abutting-parcel filler, so every outer
  block has kerb, pavement and neighbours, which is what makes it read as a street rather than as
  buildings on grass.

### The floating blocks are gone (owner items 3)

Six civic plots (market hall, fuel, school, sports ground, two warehouses) were being placed by that
same scatter. All six now sit on real frontage: the mart and the school on arterials, the fuel stop
and both warehouses on the ring, the sports ground behind an arterial. Measured `fabric.civic`
reports `offStreet: 0`. The school shrank from 46 to 40 m because the ring's curvature lobes cannot
seat a 46 m chord, and its own geometry was rescaled with it.

One bug had to be fixed to get there: the occupancy grid was ±220 m while the new arterials run past
it, and `rectScan` returns *true* for a rectangle entirely outside the grid, so lots out there were
placed with no collision test at all. That is how the school and the sports ground first landed on
top of each other. The grid is now ±256 m.

### The model facades light up (flaw 2 of cycle 19)

Lit windows are one of the eight data rules, and the 42 core model lots were going dark at night
while every procedural lot lit from the activity data. A pass now rebuilds each model lot's instance
transform (the same jitter, yaw and footprint the placement used), derives the four wall planes and
the roof line from the baked bounds, and lays the procedural rhythm on them: 1 to 3 bays on the
front, 1 to 2 on the sides, the ground-floor shop break via the same `shopAt`, no frame ring (saves
8 triangles a pane), and every pane is a real `addWindow`, so `setWindowLit` drives them from the
data exactly as before. Measured: **376 panes over 42 lots** (131 on the phone), total window count
2,137 -> 2,513, **+752 triangles** in the window mesh.

The A/B is measured with the block disabled and re-enabled on the same layout at `?hour=22`, over
the 42 lots' projected box (438x321 px): warm pixels 25.1 -> 25.9 percent, luma 0.302 -> 0.307, edge
density 9.58 -> 10.30. The 42.7 luma and 3.5 percent warm in the cycle 19 log came from a different
region definition that this cycle could not recover, so it is not comparable and is not used.

### One capability decision, three call sites (flaw 4 of cycle 19)

The narrow-but-not-touch window was inconsistent: `main.mjs` chose the mobile render profile with
`(pointer: coarse) || innerWidth < 800`, `agents.mjs` chose population caps with
`innerWidth < 800 && maxTouchPoints > 0`, and `world.mjs` chose its mesh set with
`window.innerWidth < 800` while its comment claimed to share main's test. Three tests, three
answers, and a 700 px desktop window drew a phone's mesh set with a desktop's caps.

Now main resolves `isTouch = coarse || maxTouchPoints > 0` once, and hands it to `buildWorld` as
`opts.isTouch`, to `createAgents` as `opts.mobile`, and to the camera rig. `world.mjs` reads
`const NARROW_VIEW = isTouch` and no fragment reads `window.innerWidth` for a capability decision.
A narrow non-touch window is now a small desktop window on every axis: full mesh set, desktop caps,
desktop profile.

### Regions measured after the fabric change

Frame 2000x1125, audit19 definitions:

|region|baseline|after the spill|
|---|---|---|
|day, whole frame|flat 48.4, edge 10.82|flat 45.4, edge 14.00|
|left margin (where a block stood alone)|flat 30.8, edge 4.20|flat 26.7, edge 7.42|
|right margin|flat 25.6, edge 3.73|flat 28.0, edge 7.25|
|bottom band|flat 23.6, edge 3.19|flat 19.2, edge 4.50|
|hillside above the ring|flat 19.9, edge 3.82|flat 14.6, edge 8.24|
|two top corners|flat 84.5 / 82.5|flat 79.4 / 79.8|

The margins and the hillside gain structure, which is the point: the flat share falls and the edge
density roughly doubles where a field used to be. The right margin's flat share rises 2.4 points
while its edge density doubles, which is a wider street pattern with more sky between blocks, not a
regression to field.

Gates at this stage of the cycle: desktop 88 calls / 382,013 triangles, phone 47 / 170,617, with
`node tools/assemble.mjs` 23/23, `check.mjs` 36/36, `check-dom.mjs` 100/100 and no console errors on
either profile.

### Three tools the rest of the week will use

- `.cycles/specks.py` measures the single-frame signature of z-fighting (documented above, with its
  synthetic validation), so "no z-fighting" is a number instead of a claim.
- `tools/stamp-sw.mjs` writes the payload's own sha256 into `assets/sw.js` as the cache name, and
  `check-dom.mjs` now fails when they disagree. The worker serves `assets/` cache-first, which is
  what makes the second visit instant, and also what would have hidden every rebuilt pack from
  anyone who had already loaded the page. Verified both ways: a stale version fails the check, and
  after stamping an offline reload reproduces the identical frame.
- `.cycles/measure.js` is the browser half of the gate: it walks desktop day, street tier, night,
  dream, the `?noassets=1` fallback and phone day and dream through the page's own surface
  (`stats`, `quality`, `rig.setTier`, `rig.rotate`) and returns one table. It records how it decided
  the frame was the dream, because the dream has no readable state from outside: `sleep()` sets
  `dreaming` and the cards only appear at `waking`, so waiting for a card captures the end of a
  dream rather than the dream.

One hazard worth recording: two region tools disagree by design. `audit19.py` measures flatness as
mean absolute deviation and edge as dx+dy per pixel; `.cycles/c20.py` measures window range and
averages the two axes, which reads about 0.6x the flat share and 0.5x the edge density on the same
image (44.2 versus 26.6 flat, 14.28 versus 7.23 edge). Only audit19 numbers are comparable across
cycles, so only those are quoted here.

### One rule at last, measured on four viewports

`isTouch` is now `(pointer: coarse) || navigator.maxTouchPoints > 0`, resolved once in `main.mjs`,
handed to `createAgents` as `opts.mobile`, to `buildWorld` as `opts.isTouch` and to the camera rig.
`MOBILE_WIDTH` is gone and `world.mjs` no longer reads `innerWidth` for a capability decision.

|viewport|touch|mode|shadows|pixelRatio|postScale|shadowMap|population|calls / triangles|
|---|---|---|---|---|---|---|---|
|700x900 dsf1|0|desktop|true|1|1|2048|1|88 / 381,969|
|412x915 dsf2.625|0|desktop|true|1|1|2048|1|88 / 382,145|
|700x900 dsf1|5|mobile|false|1|0.5|0|0.5|47 / 170,573|
|412x915 dsf2.625|5|mobile|false|1.5|0.5|0|0.5|47 / 170,573|

A 700 px non-touch window is a small desktop on every axis now, and a phone-width non-touch window is
a desktop too, with pixel ratio capped at 2. Before, that window drew a phone's mesh set, a phone's
post chain and a desktop's population, which no device asks for.

### The dream is the brightest cluster

Same capture before and after, 2000x1125, identical projected boxes:

|metric|before|after|
|---|---|---|
|whole luma|0.354|0.367|
|whole flat share|59.1|55.6|
|whole edge density|6.81|7.95|
|lower third luma / flat / edge|0.245 / 59.6 / 4.56|0.266 / 56.0 / 5.25|
|bright share (luma > 0.80)|1.3|2.2|
|top tenth (sky) luma / flat|0.621 / 96.0|0.621 / 96.2|

The sky is bit-equal in luma, so the deep blue is untouched while the frame lost 3.5 points of flat
share and gained 1.14 edge. The works, the corridor and the two cards are lit by a fourth additive
wash plus barrier lamps placed at the mast feet: 14 instances in one draw call.

|box|before|after|corridor / box|
|---|---|---|---|
|corridor|0.729|0.769|1.00|
|works volume|0.669|0.744|1.03|
|market square|0.285|0.395|1.95|
|tower 0|0.294|0.335|2.30|
|tower 1|0.346|0.354|2.17|
|tower 2|0.297|0.350|2.20|
|tower 3|0.267|0.307|2.51|

So the chantier is now the brightest thing in the frame by a factor of about two over every other
candidate cluster, which is what the brief asks. One honest regression: the corridor's internal edge
density fell 26.09 to 20.74 at 2000x1125 because four additive layers compress the contrast between
paint and marking; two tuning passes recovered it from 16.70 by dropping the paint value from
`0xd9dbd6` to `0x868b84` and moving the pool centres off the paint onto the mast feet. The dream
rules are untouched: the `offered` state still shows exactly two cards with `Take it`, measured at
93 calls / 394,841 triangles, and `?noassets=1` still builds the ribbon and dreams (50 calls /
317,281 triangles, state `dreaming`).

### Verified independently, stage two

Measured by `.cycles/measure.js` on a separate harness rather than taken from the workers' reports.
The dream and the fallback match to the triangle; the rest differ by tens of triangles because
traffic moves between frames.

|state|calls|triangles|gate|
|---|---|---|---|
|desktop day|88|381,925|120 / 400,000|
|desktop street|88|382,717|pass|
|desktop night awake|86|382,485|pass|
|desktop dream|78|332,669|pass|
|desktop `?noassets=1`|54|333,955|pass|
|phone day|47|170,661|70 / 180,000|
|phone night awake|44|**174,725**|pass, 2,725 over the 172,000 headroom line|
|phone dream|36|163,657|pass|

The phone night frame is the tightest number in the project and it is world-side: it contains none of
the dream geometry, which is asleep at that hour. It passes the hard 180,000 gate. It does not pass
the headroom line this cycle was written against, and that is stated rather than hidden.

### A watch item: the violet on the dream hillside

The dream frame's purple share rose from 1.2 to **5.5 percent** (audit19 thresholds; 0.84 to 3.68
percent under a stricter hue-260-to-330, saturation-above-0.25 test). The hottest cell is x 400..600,
y 225..337 at 26.6 percent, and it was located rather than assumed:

- mean colour of those pixels RGB (55, 41, 71), hue 269, saturation 0.43, luma 0.18;
- the cell's 5x5 local range averages 0.132 with a p90 of 0.445 and luma sd 0.197, so this is
  high-frequency terrain detail, not a smooth band;
- its luma (0.309) sits between the sky above it (0.556) and the city below (0.315).

So it is the hillside in shadow under a deep-blue night, tinted violet where the blue ambient
dominates, which is physically what a shadowed olive slope looks like at night. It is *not* the
smeared purple-brown band the owner objected to at cycle 18, which was a smooth, low-frequency plate
in the day sky. Still filed as a watch item: if cycle 21 changes the night ambient or the bloom, the
purple share of the dream frame must not climb further, and 5.5 percent at audit19 thresholds is the
number to beat.

### The street pass, and what it reclaimed

The third worker delivered its code but its report died mid-capture, so its own numbers were
unavailable. Everything below was measured on the assembled build, after `node tools/assemble.mjs`
(`23/23 verifications passed`), `node tools/check.mjs` (`36/36`), `node tools/check-dom.mjs`
(`100/100`), `node .cycles/gate.mjs` (`GATE RESULT: PASS`), with the service cache hash stamped
(`m2ed91bc0` is now the packed `assets/manifest.json` SHA-256 prefix, and `check-dom` asserts it).

It built what the corner brief asked for, all instanced and ranked in the detail ladder: striped
awnings over most shopfronts (canvas plus cream, two stripes, a valance), painted-steel bollards with
reflective bands, oiled bench slats on dark frames, litter bins, planters with shrubs, A-boards and
crates and parked scooters against the shopfronts, utility boxes on the walls, and one news kiosk
with a cream canopy on the market square corner, plus bus shelters with glass backs and benches. A
drainage gully per painted band on the avenue and the spurs, placed only where its own radial extent
clears the paint, so it never clutters the marking. `DETAIL_CAP` scales by touch (5,413 phone /
9,000 desktop), which is how the phone stayed in budget while the desktop got the furniture.

|state|calls|triangles|delta from stage two|gate|
|---|---|---|---|---|
|desktop day|89|384,347|+2,422|120 / 400,000|
|desktop street|89|385,139|+2,422|pass|
|desktop night awake|87|383,033|+548|pass|
|desktop dream|79|333,217|+548|pass|
|desktop `?noassets=1`|55|336,365|+2,410|pass|
|phone day|47|166,177|**-4,484**|70 / 180,000|
|phone night awake|44|**170,241**|**-4,484**|pass, and back under the 172,000 headroom line|
|phone dream|36|159,173|**-4,484**|pass|

The phone gave back 4,484 triangles while the desktop grew by a third of that, because the cap is
per-touch and the new furniture is tier-ranked: the phone drops the furniture it cannot read at 412
pixels and keeps what it can. The bench after the street pass:

```json
desktop {"avgFps":179.9,"p1LowFps":169.5,"maxFrameMs":17.7,"calls":96,"triangles":393487}
phone4x {"avgFps":179.9,"p1LowFps":172.4,"maxFrameMs":11.2,"calls":50,"triangles":168129}
```

### Corners and specks, cycle 20 baseline

The worst street corner across the four rotations is bottom-right at flat 55.9 / edge 5.31, against
cycle 19's baseline of 62.3 flat / 4.29 edge: the corners are more detailed than last cycle, which is
the point. The centres hold edge 14.98..15.10, up from 12.9 at cycle 19, because the street furniture
gives the eye something at every distance.

Specks (z-fight detector, single frame, 2000x1125): day 62 (0.0028 percent), night awake 71
(0.0032), dream 24 (0.0011), noassets 69 (0.0031), street rotations 41..42 (0.0018-0.0019). The phone
frames are 412x915 with **0 specks** at any hour. Day and noassets concentrate in the sky band
(x 1250..1500, y 140..421, four cells each), which is the same region the cycle 19 ramp validated the
detector on: high-frequency cloud rendering, not geometry, and it does not move between `?hour` runs,
so it is classified render noise rather than z-fighting. The night frames sit in pavement cells
(x 250..750, y 421..843), consistent with the gully and bollard bases meeting the kerb; three cells,
under the cycle 19 count of five isolated reports.

### The capture harness is now a script

`.cycles/measure.js` (eval, `%load ".cycles/measure.js"`) drives the assembled build through all ten
states with the real `#sleep` dream entry and the corrected 412x915 DPR 2.625 phone emulation; the
phone frames it produces normalise to exactly 412x915 with the device-pixel originals kept. The
eight `screenshots/iter-20-*` frames above were captured with it, plus the four street rotations.
`final-1080p-day.png` and `final-1080p-night.png` are exactly 1920x1080, resized from the 2000x1125
canonicals, and the gallery copies (`desktop-day.png`, `desktop-night-awake.png`,
`desktop-night-dream.png`, `pixel6-day.png`) are refreshed from the same captures.

### What cycle 20 leaves unfinished, and what cycle 21 takes

1. **The toy read is still open.** Everything cycle 20 did, the spill, the model facades with lit
   windows, the furniture, the one-touch predicate, the dream brightness, is structural. It is not the
   thing the owner named in item 5: richer facade detail at district tier (window rhythm, balconies,
   shop awnings), roofs with clutter, softer and longer shadows, a subtler tilt-shift, warmer golden
   light, more readable vehicles and crowds. Seven levers, none yet pulled. Cycle 21's brief is
   written in `.cycles/notes21-brief.md` and splits them by file: Worker A owns `world.mjs` plus the
   packer (balconies, awnings where they are missing, roof clutter, standing crowds from the staged
   `kenney-mini-characters` kit, and the phone budget), Worker B owns `post.mjs` plus the light rig
   in `main.mjs` (softer and longer shadows, subtler tilt-shift, warmer key with a cooler fill,
   readable vehicles).
2. **The violet watch item** carries over with its number to beat: 5.5 percent purple on the dream
   frame at audit19 thresholds, located on shadowed hillside terrain, not smear. Any night-ambient or
   bloom change in cycle 21 must hold it there or lower.
3. **The bench numbers must hold.** Desktop p1Low 169.5 and phone4x p1Low 172.4 both have enormous
   headroom over the 55 fps gate; if cycle 21's facade detail moves the p1Low below 100, the detail
   is too expensive regardless of the triangle count.

# Cycle 21

Worker B slice of the item 5 brief: golden-hour key light with a cool fill, soft long shadows, a
subtler tilt shift, and the readable-vehicle fleet flag. Worker A's world pass (balconies, roof
clutter, crowds) landed alongside this one, so several numbers below are mixed-build measurements.
All pixel forensics at 1600x900 (cycle 20 used 2000x1125), day captures pinned at `?hour=17`.

### The rig

`main.mjs` wraps `world.lights.setTimeOfDay` after the world is built (night branch passes through
untouched). Day branch: sun elevation 15 degrees on the same azimuth, sun 0xffbe70 at 6.2, hemi
0xcdd6e0 / ground 0x8a8478 at 0.88, shadow frustum widened to 230, sky `uSunDir` patched. Shadow
map type switched to PCFShadowMap at boot so `light.shadow.radius` (5 texels) is honoured; PCFSoft's
fixed kernel ignores it. `post.mjs` `BLUR_RADIUS` 0.55 to 0.45. Debug hooks: `?post=0` at load,
`#post=0` / `#post=1` hash toggle at runtime (wired once inside `ensurePost`). Pixel comparisons use
one-load hash-toggle pairs because separate loads diverge in traffic.

Vehicle fleet item 4 lives in `agents.mjs` behind a one-line switch: `MODEL_LANE_EVERY` (9999 = off,
24 = one in 24 slots, 6 = one in 6). Cost table at the mixed world base (hour 17, day district):
off 388,642 tris / 101 calls; 1-in-24 389,684 (+1,042); 1-in-6 393,182 (+4,540); calls delta 0.
Main integrates against the final world number.

### The measured day

Terminator, same open-ground boundary (rows 738-746, x1320-1420, raw luma ramps, 1600x900): cycle
20 PCFSoft 4-6 px with a dithered shadow side; golden rig PCF radius 5 gives 8-26 px depending on
row angle against an oblique edge, typically 12-18 px (radius 2.5 measured 3 px, radius 8 measured
30 px, proving the shader honours the radius). Cypress streak horizontal extent, same tree series:
about 68 px at cycle 20 elevation to about 117-131 px at 15 degrees (screen L/H about 1.6 to 3.0;
planar theory 2.16 to 3.73). Caster identities on the hillside overlap, so length carries a caveat;
the terminator boundary does not.

warm21 shares (luma classes, 1600x900, hour 17): sunlit warm 73.8 cool 4.5 (median hue 33.0),
shadow warm 35.4 cool 9.9, versus cycle 20 sunlit 76.7 cool 3.9, shadow 44.7 cool 8.4. The
warm-cool contrast inside the frame widened from 32.0 to 38.4 points; the sunlit share did not rise
because the 15-degree sun dims direct ground light. Clipping: any-channel 1.438 percent (cycle 20
0.526), fully-clipped white 0.000 everywhere including the 1 px inset. audit19 judge, day: luma
0.438 edge 14.72 warm 59.1 whole, lower third warm 76.6 edge 11.88, top tenth blue 52.4 sat 0.127.
Night (hour 3): luma 0.339 (cycle 20 0.339), warm 16.0, purple 1.6, blue 76.9. Dream: luma 0.367,
warm 19.1, purple 5.6, blue 67.4. Purple attribution test: at BLUR_RADIUS 0.55 the dream measures
5.5, at 0.36 it measures 5.6, at 0.45 it measures 5.5. The final radius is 0.45 and the ceiling
holds. noassets day: luma 0.437 warm 59.0.

blur21 one-load hash-toggle pair, structural-mask HF profile (final build): band0 0.289 (cycle 20
0.292), band1 0.122 (0.137), bands 2-7 carry the same minus 0.03 to 0.11 grade bias, band8 0.078
(0.042), band9 0.469 (0.436). Bands 0-1 sharpened slightly; band 9 unchanged within noise and the
residual there is bloom, not tilt radius (nopost HF 9.24 vs post 4.91).

Specks, final set: day 152 (0.0106 percent), night 60, dream 29, noassets 156, pixel6 day 1, night
0, dream 0.

State table, final build, assembled index.html (district tier desktop 1600x900, hour 17 day / hour
3 night): day 101 calls / 388,900 tris; night 99 / 346,840; dream 91 / 297,176; noassets 55 /
336,517; street 101 / 393,464; phone day 54 / 170,498; phone night 44 / 170,191; phone dream 36 /
159,211. Budget lines hold in every state except the bench-time repopulation peak noted below.

Benches. The gate could not be met honestly this cycle. Desktop bench (20 s runBench script, warmup
excluded): one window gave avgFps 160.4, p1Low 41.7, maxFrame 91.6 ms, 108 calls / 398,034 tris at
peak; every other window, here and in Worker A's runs, contains one recurring 1.0-1.1 s frame that
destroys p1Low (values 1 to 42.7). The stall reproduces in a warm tab across three consecutive
runBench passes and with `?post=0`, and in the phone4x profile, which has no shadow maps and a
near-identical post chain, so it is not the tilt shift, the golden rig, or first-load decode; it
lands on the bench's dream wake, where traffic repopulation adds about 7.5k tris / 7 calls (Worker
A's own note), and pushes the desktop peak to 393k-398k tris, over the 390k line. Attribution: the
stall and the peak sit on the world-side repopulation path or the headless environment, not on
post.mjs or the light rig. Cycle 20's p1Low 169.5 / 172.4 against today's clean windows 41.7
(desktop, uncontaminated by the stall but still below the 100 gate) and 42.7 (phone4x) is a real
regression in tail-frame cost that grew with Worker A's crowd and asset pass (assetModels 74 to 87,
+12 desktop calls, +7 phone calls). `node .cycles/gate.mjs` prints GATE RESULT: PASS (static gates
assemble 23/23, check 36/36, check-dom 100/100, smoke-data 120/120, check-pixels ok).

Deliverable frames in screenshots/: iter-21-day{,-nopost}, -night-awake, -dream, -noassets,
-street-r0..r3 (1600x900), -pixel6-{pday,pnight,pdream}{,-full} normalised to 412x915, plus the
baseline and tuning series (iter-21-base-*, -t1..t9-*) kept for the comparisons above.
