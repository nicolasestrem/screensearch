# Localtime, art pass 2: density and life. One shot. Do not stop until every check passes.

Localtime works and the first art pass is done. Read `README.md`, `src/CONTRACT.md`, `art-prompt.md` (the previous pass), and look at `screenshots/desktop-day.png` and `screenshots/desktop-night-dream.png`. The district now reads as a town, but it is far too sparse: a handful of isolated buildings floating in a large empty field inside the ring road. The target is a dense, lived-in contemporary Mediterranean hillside district seen from above at 6 pm, like a tilt-shift photograph of Montpellier or Nice: every block filled, streets lined with continuous facades, life everywhere.

## What you may edit
- `src/world.mjs` (main work), `src/agents.mjs` (only populations, spawn points and colours, not the data logic), `src/dream.mjs` (only to make the night chantier clearly visible), `src/post.mjs`, `src/style.css`.
- Do NOT change `src/data.mjs`, `src/simday.mjs`, `src/camera.mjs` behaviour, the `World` shape in the contract, or any data-to-motion rule. Every one of the eight data-driven behaviours must keep working.

## Density and composition (priority order)
1. Fill the whole area inside the ring road with urban fabric. No large empty grass inside the ring. Each quarter becomes a set of perimeter blocks: continuous rows of 3 to 7 storey buildings along every street, inner courtyards, varied heights and widths so the skyline is irregular. Target 250 to 450 building instances total, still one InstancedMesh (plus one for windows). `buildingCount` from the model scales the height and the share of the quarter's footprint, but every quarter must look built up.
2. A proper street grid inside the ring: secondary streets every 25 to 35 units, aligned with the slope, connecting to the avenue and the ring road. Pavements on both sides. Crosswalks at the avenue.
3. Outside the ring road: low-rise suburbs fading out (smaller houses with terracotta roofs, gardens, olive groves), then the background hills. The district must not end abruptly at the ring.
4. Hilltop: a few landmarks (a church bell tower, a water tower, an old stone quarter with narrow streets) and 2 to 3 glass office towers near the avenue's top, per `towerCount`.
5. Market square: denser stalls, awnings in several colours, a fountain, café terraces with parasols around it.
6. Rooftops: about 30 percent of roofs carry terraces, parasols, pergolas, plants, water tanks or solar panels. Balconies and awnings on facades facing the avenue.
7. Colour: a coherent warm palette (terracotta, ochre, sand, cream, pale pink, faded blue shutters), slight per-instance variation. The quarter colour stays an accent only. Avoid the uniform orange cast: keep greens of trees and greys of asphalt crisp.
8. Light: late golden-hour sun from the right, long shadows, warm haze on the horizon, gradient sky. Night: deep blue but never black, warm windows, sodium streets, the avenue a ribbon of light.

## Life (agents, keep data rules)
- Raise the visible population to the full budget at peak activity: up to 300 vehicles (cars, buses, scooters, delivery vans) and 300 pedestrians, spawned on pavements, crossings and the market square, never on grass or inside buildings. Parked cars along kerbs as static instances.
- Pedestrians: slightly bigger or brighter so they read at District tier as specks of colour.

## The dream must be unmistakable
- At night, during the dream, the chantier must be the brightest, most readable thing on screen at District tier: a freshly painted provisional road ribbon in light grey with white dashes, orange cones and red-and-white barriers along it, two floodlight pools on the ground, a crew of 4 to 6 figures in orange. Everything else dims. Take a dedicated close screenshot of it.

## Budgets
Keep under 60 draw calls and 250k triangles at District tier on desktop (raised from the previous pass for density). Touch devices under 800 px: shadows off and you may halve building detail and population. First frame under 3 s. No per-frame allocations.

## Verification before you stop (mandatory, loop until all pass)
1. Move the current screenshots into `screenshots/v2/`.
2. `node tools/assemble.mjs`, then `node tools/check.mjs` and every `tools/check-*.mjs`. Paste raw output. Fix until clean.
3. Browser tool at 1600x900, wait 3 s: zero console errors, `window.localtime.stats()` within the budgets above, `window.localtime.debugFraming()` ring road inside the viewport. Take `screenshots/desktop-day.png`.
4. Look at your screenshot critically: if more than 15 percent of the area inside the ring road is empty grass, or the buildings do not form continuous street frontages, go back and densify. Repeat until it passes. Then take `screenshots/desktop-street.png` at Street tier after one rotation.
5. Sleep button, wait 3 s, take `screenshots/desktop-night-dream.png` and `screenshots/dream-closeup.png` (glide or zoom to the chantier). Confirm the provisional road, cones, barriers, floodlights and crew are all visible. Press again: at most two cards appear, "Take it" makes the road permanent.
6. Viewport 412x915: whole district framed, take `screenshots/pixel6-day.png`, and confirm `stats()` within the touch budget.
7. Final message: raw console output, stats JSON for desktop and Pixel 6, sizes of index.html and each screenshot.

## Constraints
No em dash character anywhere. No external assets, fonts or images. Three.js 0.160.0 core only. No git. Nothing outside this directory. Nothing installed. Large coherent edits. Never stop to ask a question: make the decision and continue.
