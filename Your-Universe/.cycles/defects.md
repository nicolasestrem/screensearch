# Defect backlog (scratch, not part of the deliverable)

Working notes for the pixel-perfect cycles. Every item is a measurement, and it moves into
`ITERATIONS.md` under the cycle that closes it.

## Measured on the cycle 1 build

1. UI not on whole pixels: every label box lands on fractional coordinates
   (`VS Code 692.890625,119.875`, `Figma 671.1875,510.875`), caused by `translate(-50%,-100%)`
   over a fractional text width. At 412x915 the rotation chevrons sit at `y 433.5` because
   `50%` of 915 is 457.5 and the 48 px box is centred on it. Owner rule: labels and buttons on
   whole pixels. Fix in `main.mjs` + `style.css`: integer-size label boxes, integer transform
   origin, chevron offset rounded on resize.
2. Per-frame string allocation: `main.updateLabels` writes a new transform string for every
   label every frame, and the fps readout writes another. Heap trace: 36 collections in 60 s,
   range 15.8 to 22.2 MB, drift +1.5 MB, nodes and listeners constant (113/113, 30/30), so it
   is churn, not a leak. Fix: skip the write when the rounded pixel position is unchanged.
3. Instances: 11 tree instances sit inside building footprints (trees 4, 13, 16, 17, 18 and 6
   more). 0 trees within 6.5 m of a road centreline, so no trees on carriageways.
4. Building footprints, exact test: an oriented-box (SAT) audit over the 323 instances finds 28
   overlapping pairs, of which 17 are under 0.6 m (party walls, tolerance) and 11 exceed it
   (worst: instances 18/75 at 1.05 m, 18/284 at 0.88, 42/203 at 0.83, 72/288 at 0.83, 304/310 at
   0.81). An earlier axis-aligned pass reported 149 "interpenetrating" pairs; that number was
   wrong because it ignored yaw, and the yaw extraction that fed it mixed scale into the angle.
   Keep the SAT version.
5. Composition: 16x9 edge-density grid on the day frame shows the city confined to columns 4 to
   11 (8 to 13.8) while the left and right thirds sit at 2 to 5 and the top row at 0.3 to 1.6.
   The target wants urban fabric and groves filling the frame margins.
6. Distant background: top band edge density 1.4 of 10.7, so the horizon carries no hills or bay.

## Harness notes

- Fresh tab per measurement: a reused tab can wedge at boot (`frames 0`, `calls 0`); a fresh tab
  boots in 1.7 to 2.5 s. Always `page.bringToFront()` first, since a backgrounded tab throttles
  rAF and reports 0 fps.
- Pixel ratio: the tool's `emulate()` does not reach DPR here; go through the CDP session with
  `Emulation.setDeviceMetricsOverride` plus `Emulation.setTouchEmulationEnabled` and
  `Emulation.setCPUThrottlingRate`.
- Screenshots default to webp at DPR 1.25; pass `format: "png"` and set DPR explicitly.

## Found later, still open

- Sign atlas (`world.signs.map`, 960x640) and barrier atlas are correctly mipmapped
  (LinearMipmapLinear / Linear) but `anisotropy` is 1. At the street tier a fascia seen at a
  shallow angle will blur. Raising it to `renderer.capabilities.getMaxAnisotropy()` (usually 16)
  for those two textures is a one-line polish with no draw-call cost.
- Night light density, measured on the cycle 3 frame: 112 connected bright-warm blobs covering
  2.03 percent of the frame, blue family 95.7 percent, warm 3.18 percent. Target from the owner:
  every lit window warm and streets as ribbons of sodium light.

## Cycle 5 findings (measured)

- Phone profile call ceiling breached twice: night district tier 46 calls (ceiling 45) and the
  dream phase 54 calls. The dream also costs one 39.6 ms frame on the first sleep, over the 33 ms
  ceiling, which looks like a material compile on the dream group's first visibility change.
- Not a defect: a fresh-profile dream shows zero cards until the second press of the sleep button,
  because cards are revealed when the road glide finishes. Verified correct on a clean profile:
  sleep, then wake, then offered at +2.5 s with exactly two cards, each carrying a Take it button,
  the taken sentence appended to `localtime.dreams.taken` and the state back to awake.
- A hypothesis already taken is never offered again, so a returning visitor can get a dream with
  no cards at all. Correct per the rules, worth remembering when reviewing the dream screen.

## Cycle 6 targets, measured

- Portrait 412x915 top third: edge density 0.2 in every tenth, one colour `696e69` covering 17.8
  percent of the crop, cool family 0 percent. The band is smooth fog with no far ridge in it, so a
  third of the phone frame is a flat grey-green plate.
- Portrait bottom third: edge 0.6 to 3.2 rising downward, 66.8 percent warm, palette `ca663b` 5.5
  (roofs) and `322c2a` 4.3, so the foreground is roofs fading to nothing rather than a hillside
  with groves, terraces and villas.
- Desktop 1080p top 300 px is olive hillside (`4c4c23` 2.0, `444422` 1.7), not sky, so the portrait
  plate is genuinely the fog swallowing the far ridges rather than the sky being grey.
- Sign atlas anisotropy is 1 (see above), still open.

## Colour balance, measured (cycle 5 frames)

Day at 1920x1080: mean saturation 0.417, 64.5 percent of pixels above saturation 0.35, grey 5.4,
mean value 0.423. Hue buckets of 30 degrees: 0-30 **29.1**, 30-60 **38.3**, 60-90 13.3, 90-120 3.1,
180-210 2.6, 210-240 **2.8**, the rest near zero. So 80.7 percent of the frame sits in the yellow to
orange range and the cool half of the wheel is 5.4 percent in total.

That is the actual reason the frame reads khaki: there is no blue anywhere, neither sky, nor haze,
nor shadow. The owner asked for warm and cool contrast at sunset, so a cool counterweight has to be
added deliberately, and it is the same defect as the flat grey-green plate at the top of the phone
frame. Night by contrast measures 69 percent in the 180-210 bucket with only 0.6 grey, which is the
blue the day frame is missing.

## Missing against the target picture (grep of the source, cycle 8)

- `mural`: 0 occurrences in `world.mjs`. The brief asks for "a few big painted murals on blind
  walls", which is one of the cheapest, loudest signals of a contemporary city.
- `acUnit`/`AC unit`/`waterTank`/`water tank`: 0 occurrences. The brief says roofs must be crowded
  with AC units, water tanks and solar panels, and that a roof empty of clutter reads as 1993.
  Solar panels are present (8 references), parasols are present (17).
- `trafficLight`/`traffic-light`: 0 occurrences, while the brief asks for traffic lights at the
  intersections that already have bold zebras.
- `palm`, `flower`: 0 occurrences. Reference C mentions palms and flower beds along the road; this
  is East-Mediterranean, so cypress and olive may be the right answer instead of palms, but the
  street has no flowering or planted accent at all.

## Closure audit, final build

Re-tested the open concerns that could still be real on the delivered tree; three were closed as
clean measurements and two were documentation gaps, now recorded.

- `?bench=1` console path: **zero console errors** on a full bench run (179.9 fps, p1 175.4, worst
  frame 12.9 ms, 59 calls, 379197 triangles). An earlier observation of one error per bench session
  does not reproduce on this build.
- Draw-call accumulation across dream cycles: two complete sleep, wake, take cycles on the phone read
  **37 awake, 38 after the first take, 38 after the second**, with only `road-permanent` added and
  never a second copy of the works. No per-cycle creep. Phone night after a taken road is 175221
  triangles, still under 180000.
- Roof clutter geometry: the drawn range is 350 instances of the 400 rolled pieces (the rest fall
  outside the detail capacity), and a pairwise 3D AABB test found exactly **one** overlapping pair,
  indices 6310 (0.15 x 2.3 x 0.15 mast) and 6313 (0.75 x 0.6 x 0.75 reflector) at the same roof.
  That is a satellite dish's mast and its own reflector, which is the design, not a collision. No
  real interpenetration.
- Cycle 7 has no `.cycles/07` snapshot: its state was superseded by cycle 8 before the copy was made,
  so the record for that cycle rests on its measurements rather than a restorable tree.
