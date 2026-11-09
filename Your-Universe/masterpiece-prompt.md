# Localtime, masterpiece loop. Run for the full time budget. Never stop early. Never ask a question.

Read `HANDOFF.md`, `src/CONTRACT.md`, `art-prompt-2.md`, then look at `screenshots/desktop-day.png`. The app works and is dense, but the owner's verdict is brutal and correct: "a map from 1993" and "stupid Middle Ages". Two root causes:
1. **It reads medieval.** A closed circular ring road around a packed old core, surrounded by scattered small gabled cottages with red roofs, reads as a walled medieval town. The target is a CONTEMPORARY city of 2026.
2. **It looks like 1993.** Flat unshaded materials, no contact shadows or ambient occlusion, a uniform yellow-green plain with no texture, cartoon clouds, a tilt-shift blur so strong it smears half the screen into mush.

## The target picture (the owner approved this exact image as the goal)
A contemporary Mediterranean hillside city at 6 pm, seen from a high oblique angle like a tilt-shift drone photo. From top to bottom of the frame: a hilltop old village with a stone church and cypresses (small, the only old thing); the city rising up the slope; a cluster of three or four modern glass and steel office towers, 12 to 20 floors, with warm lit windows; mid-rise apartment blocks of 5 to 8 floors in cream, ochre, salmon and pale pink with balconies, shutters, rooftop terraces with parasols, solar panels, AC units, water tanks and a few big painted murals on blind walls; a tram-lined boulevard with planted median running down the slope to a large roundabout with olive trees in the centre and a modern tram crossing it; a busy market square with white and red striped stall canopies and crowds; on the right a railway yard with a regional train and freight wagons and a modern low depot with solar roof; on the bottom left a curved elevated expressway on a stone retaining wall disappearing into a tunnel portal, packed with cars showing red brake lights; olive trees and cypresses on every unbuilt slope; scooters, buses, vans and pedestrians everywhere; in the far distance more hills and a glimpse of a bay in haze; sun low on the right, long warm shadows, golden light, slight bloom. Saturated but plausible, stylised 3D, not cartoon.

## Phase 1, de-medievalise (do this first)
- Break the closed circle: the ring road becomes an irregular arterial that leaves the frame on at least two sides, plus the elevated expressway on a retaining wall with a tunnel portal at the lower left. Roads continue outward into the suburbs. No continuous ring of buildings that looks like a wall.
- Replace every small gabled cottage with contemporary forms: flat-roof apartment blocks with balconies, villas with pools only on the outer slopes, a supermarket with a car park, a petrol station, a school with a sports pitch, a stadium or sports hall, warehouses near the rail yard.
- Hilltop old village: allowed, small, on the very top only.
- Modern office towers: glass curtain wall material (cool reflective blue-grey with a warm sun reflection gradient and lit window grid at dusk), not plain blue boxes.

## Phase 2, de-1993 the rendering
- Materials: switch to MeshStandardMaterial with sensible roughness and metalness; glass towers lower roughness. Bake ambient occlusion into vertex colours or instance colours: darker at the base of buildings and where they meet the ground, darker in street canyons. Add a cheap screen-space ambient occlusion or a contact-shadow decal under each building if the budget allows.
- Shadows: PCFSoftShadowMap, one 2048 directional shadow map on desktop with a tight frustum around the district, low sun, long shadows. Touch keeps the existing rule.
- Building geometry: bevelled or chamfered edges, a parapet, a different ground-floor storey (shopfronts with awnings in varied colours), window grids with recessed frames, balconies as thin slabs. Keep InstancedMesh; use 3 to 5 building archetypes.
- Terrain: procedural colour variation (noise): dry grass, olive-grove dark dots, field patches, dirt tracks, retaining terraces with stone walls on the slope. No flat yellow sheet.
- Sky: physically inspired gradient with a warm sun glow near the horizon, soft distant haze, small high clouds that are soft and translucent, not white hexagons.
- Post: tilt-shift much subtler (the central sharp band must cover at least 60 percent of the height; blur strength halved), plus mild bloom on lit windows and lamps, and a gentle colour grade (warm highlights, slightly cool shadows). Keep it under budget.
- Night: deep blue never black, every lit window warm, streets as ribbons of sodium light, towers with lit floors, the dream chantier the brightest thing.

## Phase 3, life and detail
Crowds on the market square and pavements, buses and scooters, parked cars along kerbs, the tram visibly modern (white with coloured stripe, articulated), trains moving in the yard, cranes on construction sites driven by the data, café terraces with parasols, street lamps along every road.

## Iteration protocol (the core of this task)
Work in cycles. Each cycle:
1. Make one coherent set of improvements.
2. `node tools/assemble.mjs` and all `node tools/check*.mjs`, fix until clean.
3. Browser tool, 1600x900: screenshot to `screenshots/iter-NN-day.png`, sleep button, screenshot `screenshots/iter-NN-night.png`; then 412x915 `screenshots/iter-NN-pixel6.png`. Zero console errors. `window.localtime.stats()` under 80 draw calls and 400k triangles on desktop; on Pixel 6 viewport under 60 calls.
4. Look at your own screenshot as a harsh art director against the target picture above. Write 5 concrete flaws into `ITERATIONS.md` under the cycle number (what is wrong, where on screen, what you will change). Then start the next cycle on those flaws.
Do at least 10 cycles. Keep going until the time budget ends; there is always something to improve (lighting, materials, density, life, composition, night, dream close-up, phone framing). Never stop because "it is good enough". Do not regress: if a cycle makes it worse, revert that change.
At the end of every cycle also copy the latest day, night and pixel6 shots to `screenshots/desktop-day.png`, `screenshots/desktop-night-dream.png`, `screenshots/pixel6-day.png`.

## Hard constraints (unchanged)
- Do not change the data logic (`src/data.mjs`, `src/simday.mjs`) or the eight data-to-motion rules, or the camera rules (40 degree pitch, 90 degree snaps, three zoom tiers, clamped pan). The dream rules stay: one provisional road, at most two cards, "Take it".
- One self-contained `index.html` via the assembler, Three.js 0.160.0 core from jsDelivr only, no addons, no external assets, everything procedural.
- No em dash character anywhere. No git. Nothing outside this folder. Nothing installed. Ignore the parent repo's CLAUDE.md and AGENTS.md.

## NEW OWNER REQUIREMENTS (override anything above that conflicts): pixel perfect, 60 fps PC and Android, 12 hours
The time budget is now 12 hours. Use all of it.

### 60 fps, measured, not assumed
- Add a benchmark mode: `index.html?bench=1` runs a fixed 20 s camera script (district view, two rotations, zoom to street, sleep, wake) and writes `{ avgFps, p1LowFps, maxFrameMs, calls, triangles }` to `window.localtime.bench` and to the console.
- Desktop gate: at 1600x900 in the browser tool, `avgFps >= 60` and `p1LowFps >= 55`, no frame over 33 ms after the first 2 s.
- Android gate (Pixel 6 class, Mali-G78): emulate at 412x915 with devicePixelRatio 2.625 and, if the browser tool supports it, CPU throttling 4x via the DevTools protocol (`Emulation.setCPUThrottlingRate`). Same gates: `avgFps >= 60`, `p1LowFps >= 50`. If throttling is unavailable, enforce a mobile profile that is provably cheap: under 45 draw calls, under 180k triangles, shadows off, render pixel ratio capped at 1.5, post-processing at half resolution, instanced crowds halved, no per-frame allocation (verify with a performance trace: no GC sawtooth over 20 s).
- Adaptive quality: if the measured fps over the last 2 s falls under 57, step quality down one notch (pixel ratio, shadow map size, post resolution, crowd count) until it holds 60; step back up after 10 s of headroom. Show nothing in the UI.
- Record the bench JSON for desktop and Pixel 6 in `ITERATIONS.md` at every cycle. A cycle that drops below the gate must be fixed or reverted before moving on.

### Pixel perfect
- No z-fighting anywhere (roads, pavements, markings, decals: use polygon offset or real height offsets), no flicker at any zoom tier or rotation, no instances intersecting each other or floating above or sinking into the terrain, no trees on roads or inside buildings, no cars driving through each other or off the asphalt, no pedestrians on the carriageway outside crossings.
- Crisp edges: MSAA on desktop, FXAA-quality edge pass on mobile inside the post pass, textures and atlases mipmapped, sign text sharp at Street tier.
- UI: labels and buttons aligned to whole pixels, no overlap between labels (declutter: hide lower-ranked labels that would collide), consistent margins, safe-area insets on phones, nothing cut off at 412x915 or 1600x900 or 1920x1080.
- Visual audit each cycle: zoom into four corners and the centre of each screenshot at Street tier and list any defect in `ITERATIONS.md`; fix them all before the end.
- Final deliverable screenshots at 1920x1080 as well: `screenshots/final-1080p-day.png`, `screenshots/final-1080p-night.png`, plus the Pixel 6 shots.

## Two more owner reference images (described, take the level of detail, not the layout)
**Reference B, a dense street-level neighbourhood at golden hour, high oblique view:** four- to six-storey red-brick and cream apartment buildings with black fire escapes, flat roofs crowded with grey AC units, solar panels, water tanks and rooftop terraces with string lights, parasols, pergolas and diners; a huge colourful painted mural on a blind brick wall; ground floors are named shops with readable signs and striped awnings (an auto repair garage with tyres stacked outside, a grocery with fruit crates on the pavement, a print shop, cafés with yellow parasols); a pocket park with a playground and flowering purple trees; intersections with bold white zebra crossings and traffic lights; a bus lane with three city buses and a queue of cars with red brake lights; a delivery truck and white vans; dozens of pedestrians on wide pavements, a cyclist; plane trees and palm trees along the streets; a downtown skyline of glass towers and hills in warm haze in the background.
**Reference C, a busy mixed district at golden hour, high oblique view:** a curved multi-lane arterial with a long queue of colourful delivery trucks, a modern bus interchange with angled bays, canopies and buses with lit destination signs, a multi-storey car park full of cars with a big P sign, a row of shops with bright readable signs (pharmacy, bakery, café, eatery, hardware, mobile shop, money transfer), a colourful mural, forklifts and pallets of fruit, crowds crossing a wide zebra, palm trees and flower beds along the road, port cranes and the sea far away in haze.
What to take from both: the density of small readable detail at Street tier (signs, awnings, rooftop clutter, crosswalks, kerbs, lamp posts, parked and moving vehicles, people everywhere), warm golden light with long shadows, crisp edges, a lived-in contemporary city. Do NOT build rows of identical market hangars.
