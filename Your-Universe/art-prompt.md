# Localtime, art pass. One shot. Only the look changes.

Localtime already works: read `README.md`, `src/CONTRACT.md` and look at `screenshots/desktop-day.png`. The simulation, data model, camera, agents and dream are done and verified. Do not touch them. Your job is to make it look like a real hillside district seen from above instead of a green plate with coloured blocks floating in a beige void.

## What you may edit
- `src/world.mjs` (the bulk of the work).
- `src/style.css` and `src/shell.html` only for the page background behind the canvas.
- `src/post.mjs` only if the tilt-shift bands need retuning for the new framing.
- Nothing else. `buildWorld(THREE, model, opts)` keeps its exact signature and returns exactly the same `World` shape described in `src/CONTRACT.md` (bounds, groundHeightAt, quarters with signSlots, craneSlot, doorPoints, roads as CatmullRomCurve3, roundabout, marketSquare, tramStops, buildings, windows, trees, signs, cranes, setNeglected, lights, streetLights, pickTargets). Agents, dream and main depend on it. If you move the ring road or the avenue, the curves you return must follow, and `world.bounds` must stay correct so the default framing still fits.

## The target picture (describe it to yourself before coding)
A contemporary Mediterranean inland district on a real slope, late afternoon, warm low sun from the right, long soft shadows. Seen from above at the default camera, the eye reads in this order: the dark grey ring road curving around the bottom, the avenue climbing the middle to the hilltop with the tram line on it, the roundabout where they meet, the market square mid-slope, then eight distinct quarters of mid-rise blocks with terracotta, ochre and cream facades, a few glass office towers near the top, a rail yard with tracks and wagons along the right edge, olive groves on the empty slopes, rows of plane trees along the avenue. Beyond the district, more hills fade into haze, no visible edge of the world. Clean stylised low-poly, saturated but plausible. Mini Motorways meets a tilt-shift photograph of a southern French town.

## Concrete changes, in priority order
1. Terrain: a real hill. Height difference of 20 to 30 units from the ring road at +Z to the hilltop at -Z, with two or three soft terraces so the quarters sit on steps. Extend the ground far beyond the district (at least 3x the district span) and fade its colour toward the sky at distance so no plate edge is ever visible at any zoom tier or rotation. Add 5 to 8 low background hills as simple smooth meshes on the horizon.
2. Roads as roads: the ring road and the avenue are dark asphalt ribbons with lighter pavements on both sides and short white centre dashes, built from the existing curves (ribbon geometry following `groundHeightAt`, not tubes). The roundabout is a real circle of asphalt with a planted centre. The avenue carries two visible tram rails. Secondary streets: a small grid of 2 or 3 streets inside each quarter, lighter asphalt, so buildings have something to line up on.
3. Buildings that look like a town: footprints aligned to the quarter's streets, varied widths and depths, 2 to 6 floors for blocks, towers only where `towerCount` says so. Facades in a shared Mediterranean palette (terracotta, ochre, sand, cream, pale pink); the quarter's colour becomes an accent (roofs, awnings, doors, the label swatch), not the whole building. Flat roofs with a parapet, a few rooftop terraces with pergolas. Keep everything in InstancedMesh with per-instance colour.
4. Market square: a paved plaza mid-slope on the avenue, 8 to 12 striped stalls, a fountain or a kiosk, people spawn points around it (doorPoints of the nearest quarter can include it).
5. Rail yard: 4 parallel tracks along the +X edge, 6 to 10 wagons in two colours, a small depot shed.
6. Vegetation: plane trees in rows along the avenue and the ring road pavements, olive trees in loose groves on the free slopes, cypress accents near the hilltop. No random scattering over the roads or inside buildings. Two or three tree silhouettes, still one InstancedMesh.
7. Sky and light: a gradient sky dome (warm near the horizon, blue above, deep blue at night with the same warm horizon kept dim), fog matching the horizon colour, sun from the right at about 25 degrees elevation for day, long shadows. Night keeps warm windows and sodium lamps; add a faint warm glow quad under each street lamp.
8. Street furniture at the Block and Street tiers: bus shelters at tram stops, benches on the square, a few parked cars along kerbs. Cheap, instanced.

## Budgets (unchanged, verify them)
Under 40 draw calls and 150k triangles at the District tier on desktop, measured with `window.localtime.stats()`. Shadows off on touch under 800 px. First frame under 2 s. No per-frame allocations introduced.

## Verification before you stop (mandatory)
1. Move the current screenshots to `screenshots/v1/` so the before and after can be compared.
2. Run `node tools/assemble.mjs` then `node tools/check.mjs` (and the other tools/check-*.mjs) and paste their raw output. Fix until clean.
3. Open `index.html` in the browser tool at 1600x900, wait 3 s: zero console errors, `window.localtime.stats()` within budget, `window.localtime.debugFraming()` shows the ring road fully inside the viewport. Take `screenshots/desktop-day.png`.
4. Rotate twice with the chevrons and zoom to Street tier: confirm no plate edge is visible in any direction, take `screenshots/desktop-street.png`.
5. Press the sleep button, wait 3 s, take `screenshots/desktop-night-dream.png`; press again, confirm the two cards still appear.
6. Viewport 412x915: whole district framed with the ring road visible, take `screenshots/pixel6-day.png`.
7. Final message: raw console output, the stats JSON, the sizes of index.html and the screenshots. Do not declare done on a failed check.

## Constraints
- No em dash character anywhere. `tools/check.mjs` enforces it.
- No external assets, fonts or images. Everything procedural. Three.js 0.160.0 core only, no addons.
- Do not touch git. Do not touch any file outside this directory. Do not install anything.
- Work in large coherent edits of `src/world.mjs`, not dozens of tiny patches.
