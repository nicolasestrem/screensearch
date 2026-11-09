# Post-READY cycle brief: Phase 2 gap audit (masterpiece-prompt.md Phase 2)

Written before the audit, so the audit is not shaped to fit a conclusion. This is the first cycle
after `READY.md` exists; the owner's instruction is "continue cycling on masterpiece-prompt.md
Phase 2 and 3 until the time budget ends", and Phase 2 is "de-1993 the rendering".

## Method: measure the checklist, then fix the two largest gaps

Phase 2 is a list, and most of it was built in cycles 8 to 22. Do not re-litigate what exists.
Establish, with numbers or counts from the live page and the current capture, which of these is
present, weak, or absent, and put the answer in a table in the cycle notes:

| Phase 2 item | how to measure it |
| standard materials with plausible roughness/metalness, glass towers lower roughness | count materials and print each one's roughness/metalness from `world.group.traverse` |
| ambient occlusion baked into vertex/instance colour, darker at the base and in canyons | print the contact-blob count and the darkest instance-colour step at the base of the tallest block; measure the luma of a building base against its ground |
| one tight 2048 directional shadow map, long low-sun shadows | `world.lights.sun.shadow.mapSize`, the frustum, the sun elevation, and the measured length in pixels of a known building's shadow |
| bevelled/chamfered edges, parapet, distinct ground-floor storey with awnings, recessed window frames, balcony slabs, 3 to 5 archetypes | count instances per archetype, count awnings, count balcony slabs, count parapets; report the archetype histogram |
| terrain colour variation: dry grass, olive groves, field patches, dirt tracks, retaining terraces | count terrace walls, tracks, field patches; measure the terrain hue spread in the near field from the capture |
| sky: gradient, sun glow at the horizon, soft translucent clouds, not hexagons | the four-band `sky26.py` table plus the cloud count and their opacity/softness settings |
| post: sharp band at least 60 percent of height with blur halved, mild bloom, gentle grade | `post.mjs` constants (`BAND_EDGE` 0.62, `BAND_FEATHER` 0.30, bloom thresholds) and a frame A/B with post off |
| night: deep blue, warm windows, sodium ribbons, lit tower floors, dream brightest | the night capture's blue/warm shares and the tower lit-floor count |

Score each item `present`, `weak` or `absent`, with the number that decided it.

## Then

Take the two items with the worst score against the owner's target picture, fix them in one
coherent change set (one theme per cycle, as every earlier cycle did), and verify with:

1. `node .cycles/gate.mjs` PASS, `node .cycles/defects25.mjs` still PASS.
2. `browser.close({all:true})` then three desktop benches and one phone4x; owner gate desktop
   p1LowFps >= 100 with no frame over 33 ms after warmup, phone4x p1LowFps >= 60.
3. `measureStates` table: calls must not rise (105 / 55), triangles reported with a same-page A/B.
4. Captures `iter-NN-day.png` at 1600x900 hour 17, four street rotations, `-noassets.png`,
   `-pixel6.png`; `audit19.py judge` on the day frame and the corners (worst flat at or under
   49.5, worst edge at or above 6.05); `top25.py` to prove the top band did not regress.
5. A before/after for the two items you fixed, in the same units the audit used.

## Standing rules for every later cycle

- One coherent change set per cycle, recorded in `ITERATIONS.md` with its five measured flaws and
  its gate output; the gallery copies refreshed (`desktop-day.png`, `desktop-night-dream.png`,
  `pixel6-day.png`).
- Never regress the top band (streaks, mud, purple), the night palette (deep blue, purple at or
  under 5.5 in the dream), or the corners.
- Bench only with `browser.close({all:true})` first; a number taken beside another animating tab
  is not a measurement of the build.
