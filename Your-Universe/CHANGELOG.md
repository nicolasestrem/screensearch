# Changelog

## 2026-11-09

### Cycle 36: skyline value compression at the hilltop

- Pull the hilltop silhouette toward the air it is drawn against. The merged `towers` mesh and
  `statics` (the church, the campanile, the water tower and the stone crown on the summit) take a
  height keyed value pull toward `AIR_GOLD` #cba45e, the terrain shader's own day air stop:
  nothing at or under 30 world units moves, the ramp closes at 56 where the campanile spire stops,
  and the ceiling is 0.42, about half the 0.90 air the far plateau itself carries. The pull is one
  multiplicative luma scale, so every family keeps its hue and only its value travels: the shaded
  tower flank goes from linear luma 0.110 to 0.232 and the darkest spandrel from 0.053 to 0.199,
  while the warm crown glass, already at the air stop's own 0.400, moves 0.423 to 0.414 and the
  pale campanile stone comes down 0.718 to 0.602. Both ends of the value range converge on the
  backdrop instead of one end being bleached.
- It costs no vertex, triangle, draw call or material. The grading happens in `makeBuilder`'s one
  vertex sink and only for the meshes that ask for it, so paving, paint, the expressway, the trees
  and every other pass are unchanged, and every piece at grade keeps its colour to the digit.
- Not covered, and recorded rather than implied: building volumes carry one instance colour for
  walls and roof and cannot grade down a wall without moving the storeys at grade, and the detail
  pool that dresses the same roofs is left alone with them. Their share of the 150 to 180 slice
  (0.25 for the details, measured in cycle 35) is therefore not addressed by this change.
- Measure it rather than assuming it worked, and let the corner decide the strength. At 0.42 the
  slice goes from 1.77 to 1.69 but the r2 top left corner's flat share rises from 46.4 to 49.7, over
  its 49.5 ceiling; 0.30 measures 1.71 and 49.0; the shipped 0.22 measures **1.72** with streaks 0.34
  percent, mud 1.24 and the worst corner at 48.3. The curve also saturates: the slice's own floor is
  1.58 with `towers` hidden at all, so the 1.6 target predates that measurement and the record now
  watches the distance to 1.58.
- Fix a capture protocol bug the audit exposed. The cycle 35 dream gallery frame read purple 5.6
  percent, over the 5.5 ceiling, only because it was shot 1.6 s after `enterDream()` while the grade
  was still moving. A settled dream frame reads 3.3 percent, flat 53.1, edge 8.55, twice identically,
  and the frame is replaced. Settle 4 s before shooting the dream.

### Cycle 35, Phase 3: the pavements get people

- Fill the two cheapest pavement classes. All four fountain benches are now occupied, walked at the
  furniture's own angles rather than quartered around the fountain, and four cross street corners
  are taken by deriving them from the avenue's cross lines with a sidewalk guard instead of
  hardcoded coordinates. The crowd goes from 38 to 44 people, benches from 2 to 4, corners from 2 to
  6, for 896 measured triangles.
- Pay for it honestly. A first attempt cut 300 rank-2 detail pieces, spending 6,000 triangles of
  facade to buy 896 of people, and cost the row 30 edge two thirds of its value (0.29 to 0.09); the
  final budget cut is 40 pieces, 800 triangles, and rows 30 and 60 read 0.09 and 0.21. The lamp
  model cap goes 22 to 18 because the bench peak was not detail bound: it still read 399,844 at
  budget 6300, and the four heads take it to 399,188.
- End the seven-cycle silence on the 150 to 180 row edge. Hiding one pool at a time on the default
  framing gives towers 1.58, statics 1.59, details 1.52 and cranes 1.77 against a base of 1.77, so
  the band is the hilltop silhouette spread across three pools, and the fix has to be value
  compression at the skyline rather than deleting any of them.
- Record the bench caveat rather than hiding it: p1 low reads 166.7 and 163.9 against 175.4 in
  earlier cycles on a host with a screen recorder and several browsers resident, while p99 is 6.0 ms
  and no frame exceeds 6.5 ms.

### Cycle 34, Phase 3: somebody has to be sitting

- Seat people at the terraces. The terrace stations are shared out of the parasol pass and the crowd
  pass stands a figure at the twelve nearest tables, facing them: the crowd grows from 29 to 38
  people and the cafe class from 6 to 15, with 9 of the 32 terraces occupied and three candidates
  dropped by the same free-disc guard the market keepers use. A new `terracesWithPeople` audit field
  keeps people near a cafe from being mistaken for tables with somebody at them.
- Spend the room with a reason. A figure costs about 150 triangles and the bench had 2,398 spare, so
  twelve candidates is 1,800 and thirty-two would break the 400,000 gate; nine landed for 1,346 and
  the peak is 398,948, 1,052 under it.
- Record the rest honestly: twenty-three terraces and sixty-four chairs are still empty, there is no
  seated crowd variant, the pavement classes are still two people at benches and two at corners, and
  the 150 to 180 row edge has been 1.77 for seven cycles.

### Cycle 33, Phase 3: tables and chairs

- Put furniture under every umbrella. The parasol pool's geometry becomes a terrace unit: pole and
  canopy, an eight sided table with pedestal and foot, and two chairs with real seats and backs, 32
  triangles against 12. All 32 terraces keep the fifteen market-row stations first and take the
  avenue for the rest, in one instanced call.
- Delete the modelled umbrellas to pay for it, `MODEL_PARASOL_MAX` 15 to 0, and check the call
  accounting by toggling rather than by arithmetic: the terrace pool measures 2 calls (main plus
  shadow, 102 to 100 hidden and 101 with `castShadow` cleared) where the two model meshes it
  replaced drew 4, so desktop calls fall from 105 to 101 (99 at night, 87 in the dream) while the day
  and night frames each lose 1,576 triangles.
- Verify the finished geometry instead of trusting the patch: 32 triangles, zero degenerate by cross
  product, vertices interleaved as x,y,z triples, colour attribute present, bounding box 0.00 to
  2.50 m from the foot to the canopy apex.
  The bench peak falls from 399,178 to 397,602, 2,398 under the hard gate, and the `?noassets=1`
  frame grows 1,640 triangles to draw the full terrace set at 55 calls.
- Fix a chair whose quad was emitted with the same pair of corners twice, so its seats had zero area
  and drew nothing while still counting triangles; caught by reading the geometry against the
  arithmetic before any capture.
- Record that the sixty-four new seats are empty: the static crowd is still 29 people with two at
  benches and two at corners, which is now the loudest thing missing from the frame.

### Cycle 32, Phase 3: terraces down the avenue

- Give the city more than one cafe terrace. Fifteen umbrellas stood on the market's west row and
  two more at x 46.4; the plaza itself has no room for a terrace, measured as zero free stations in
  a grid over the square because the stalls, crates, trolleys, boards, fountain, benches, lamps and
  trees already own it. The avenue now carries seventeen more umbrellas, one station every fourteen
  metres alternating kerbs at 1.9 m in from the kerb, so the world has 32 umbrellas on five
  frontages instead of 15 on one.
- Build them cheap. The new pool is a two quad pole under an eight sided open cone, 12 triangles
  against the pack models' 96 and 120, and it draws in a single instanced call, so the draw ceiling
  is untouched at 105 desktop, 55 without assets and 54 phone. Toggling the pool at street tier and
  diffing the frames changes 22,686 pixels, which is the proof that the umbrellas reach the frame.
- Fix two defects the measurements caught first: separate coordinate arrays uploaded as one
  interleaved buffer, and a pool sharing the modelled pass's cursor, which stacked fifteen
  umbrellas at the world origin while reporting count 26.
- Pay for the work with `MODEL_LAMP_MAX` going from 30 to 22, eight more 92-triangle heads traded
  for the 20-triangle procedural post. Day falls from 387,572 to 386,924 triangles and night from
  389,426 to 388,778, the bench peak from 399,826 to 399,178, and every bench stays at 175.4
  p1LowFps with no frame over 33 ms.
- Record what is still thin: no tables or chairs under the umbrellas, a static crowd of 29 with two
  people at benches and two at corners, the 150 to 180 row edge at 1.77, and no human eye on any of
  it in this environment.

### Cycle 31, Phase 3: a lamp on every road

- Light the five spurs. The ring, the avenue and the local grid each had lamps at 24 to 30 m, and
  the five spurs carry 1,001 m of the world's 1,651 m of arterial plus every crossing cycle 28
  built, but they had none at all: after dark the roads out of the district went black. Each spur
  now takes a lamp every 30 m, alternating kerbs, on the pavement edge. Lamp posts go from 39 to
  81 and glow discs from 81 to 111, inside the meshes that already exist, so no draw call changes.
- Pay for them with fewer fancy heads. `MODEL_LAMP_MAX` drops from 48 to 30, so the twelve model
  luminaires, at 60 and 92 triangles a head, fall back to the 20-triangle procedural post. The day
  frame ends 600 triangles lighter than cycle 30 (387,572) and the night frame 60 lighter
  (389,426) while carrying 29 more lit posts, and calls stay at 105 desktop, 55 without assets and
  54 phone.
- Keep the hard gate. A first attempt at 26 m spacing put the bench's own peak at 400,054
  triangles, 54 over the 400,000 hard gate, because the bench peaks at night where every post and
  glow disc draws. The 30 m spacing brings the peak to 399,826 with the desktop bench at 175.4
  p1LowFps, no frame over 33 ms, and the phone4x bench at 175.4.
- Record Phase 3 honestly in `ITERATIONS.md`: crowds, buses and scooters, parked cars, the
  articulated tram, trains in the yard, data-driven cranes and the tram stops are all present and
  measured; cafe terraces with parasols remain thin at fifteen, and the static crowd's benches and
  corners carry two people each.

### Cycle 30, Phase 2 audit: the country and the weather

- Give the countryside a real hue spread. Every ground tone was an ochre between 38 and 45 degrees,
  so the day frame measured 88.9 percent of the right slope's saturated pixels in one 30 degree
  band. The parcel's family (straw, grazed grass, olive, fallow) is now applied last as a
  value-preserving tint, which leaves every terrace riser, track and grove line with its own value
  step and moves the right slope to 53.8 percent with 37 percent between 60 and 90 degrees. The
  grove tone becomes a real olive green.
- Thicken and lift the cloud deck: eleven plates at a clear hour instead of four, on a deck lifted
  to 132 m and deepened to 46 m, which moves 5.39 percent of the upper sky over 2.4 s of drift
  against 0.00 percent before, with the top band's streaks at 0.35 percent and its mud reading
  improved from 1.45 to 1.26.
- Report `frames`, `p99FrameMs` and `over33` from `runBench()` so the 1 percent low carries its own
  denominator, and correct the tilt-shift radius in the contract, which still said 0.7 where the
  shipped constant is 0.45.

### Cycle 29, owner item 5: performance, the wake window, and the draw-call line

- Slice the post-dream re-population as real per-pool work over 30 frames. A sleep captures each
  pool's drawn span; the awake frame drops every pool to zero and admits the first slice, and each
  pool then enters its own slots at `max(span, target) / 30` per frame, clamped at its target, with
  slots above the slice keeping the zero matrix and every parked arc preserved. The counter
  advances after the pools step, so the last entrances land on frame 30 (hard stop at 60 for a
  moving target); a wrap or a live model install slices the rate without emptying the population,
  and a window open at sleep pauses rather than counting idle frames. Steady-state cost is one
  integer compare per pool and nothing allocates. Read-only report: `window.localtimeReseed`.
- Key the offline cache by document and asset instead of by URL: query and hash are dropped for
  same-origin requests, every shell request maps to one `index.html` entry, install skips entries
  already held, and activation prunes per-query copies an older worker left. A first load ends
  with 12 entries and 6.09 MB; three further navigations no longer add a 1.15 MB shell copy each.
- Replace the stale draw-call line in `src/CONTRACT.md` with the measured frame: 105 calls and
  388,208 triangles on the desktop day, 55 and 321,707 in the `?noassets=1` fallback, plus the
  decomposition (scene, shadow pass, post) that shows the under-80 target is not reachable without
  dropping the desktop shadow map or merging the fleet.

### Bench report carries its own denominator

- `runBench()` now returns `frames`, `p99FrameMs` and `over33` beside `p1LowFps`, so the 1 percent
  low can be read against the sample it was drawn from and the frames a gate cares about are
  reported directly instead of inferred from the maximum.

### Cycle 28, owner item 4: streets that cross the ring

- Extend the east arterial past the frame edge and turn the west arterial down the valley as the
  south-west crossing, so two streets carry a building row on both kerbs out of frame; the rail
  corridor now stops at a paved goods apron so its row stands on paving, and the north spur ends
  on a designed track edge at the crest. `world.fabric` publishes lots per street, corner parcels
  and every gap the close pass could not fill.
- Rank exposed side and rear walls by their viewport-clipped projected area, start their pane rows
  above the neighbouring roofs with the shared `clearWallBase` query, and give those panes the pale
  trim so a shaded elevation reads as window reveals. All sixteen street-rotation corner crops are
  inside the flat and edge floors; the crop that failed for three cycles reads 42.3 / 6.53.

### Owner review fixes, cycles 25 to 27

- Gate the green wave on the currently active session rather than the longest earlier session. Keep the model's longest-session query because data smoke checks still use it.
- Clear dream cards before replacing their scene, preserve an uploaded export when a delayed sample load completes, and dispose light shadow maps during scene teardown.
- Clear distant tree rows and terrain ribs beyond the crest, with a height-based blue-to-warm atmospheric gradient for the top of the frame.
- Ground civic buildings and the rail depot in paved frontage blocks with access roads, parking, footways, kerbs, crossings and yard furniture.

Verification and remaining visual issues are recorded in the corresponding entries of `ITERATIONS.md`. Cycle 27 carries the r2 bottom-left corner regression forward; this changelog does not mark that visual gate as passed.
