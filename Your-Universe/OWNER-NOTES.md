# Owner decision, 2026-11-09 14:00. Highest priority. Overrides every earlier section where they conflict. Read at the start of every cycle.

1. Fix the four review defects in src/ now, exactly like this, before any art work. (a) agents.mjs: the green-wave gate uses activeSessionAt(t), becomes `!!active && active.q >= 0`; delete safeLongest and longestSessionBefore if nothing else calls them. (b) dream.mjs: add `clearCards: hideCards` to the returned API; main.mjs install(): call `dream.clearCards()` right before `disposeGroup(dream ? dream.group : null)`. (c) main.mjs: module-level `let userModel = null`, set in loadExport() after usable() passes; in the boot() callback, wrap both the sample `install(next)` and the portrait `rig.setTier('block', false)` in `if (!userModel) { ... }`; everything after them still runs. (d) main.mjs disposeGroup: before the no-material return, add `if (object.isLight && object.shadow && object.shadow.map) { object.shadow.map.dispose(); object.shadow.map = null; }`. Write a one-line gate for each in tools/ or .cycles/ and record all four as DONE in ITERATIONS.md.

2. Top of frame (desktop-day.png): the far tree rows render as streaked lines and the hills are a dirty band. Beyond the second ridge draw no individual trees at all: hills as clean soft silhouettes in warm haze, sky a clean gradient. No streaks, no purple or brown mud. Judge it on the top 20 percent of the 1600x900 frame.

3. Nothing floats: the big isolated boxes at left, right and bottom centre (depot, warehouses, sheds) must sit in a real block with a street, a car park and kerbs, or be removed. Zero objects dropped on bare ground.

4. The ring is still a wall. Continuous streets with buildings on both sides must cross the ring road and continue outside it on at least two sides (south-west toward the expressway, east toward the rail yard), so the district has no visible edge inside the frame. Phase 1 of masterpiece-prompt.md is not done until this is true.

5. Performance: the 1.0 to 1.1 s frame at dream wake comes from traffic repopulation in one frame. Spread repopulation over at least 30 frames (time-sliced, no per-frame allocation). Gate: desktop p1LowFps >= 100 and no frame over 33 ms after warmup, phone4x p1LowFps >= 60. Set the draw-call line in the header to the measured value (101 desktop with assets, 55 noassets) or get under 80; the header and the measurement must agree.

READY: when items 1 to 5 are done and `node .cycles/gate.mjs` passes, write READY.md (date, bench JSON, the four defect gates) and copy fresh desktop-day, desktop-night-dream, pixel6-day screenshots. Nicolas looks at the district only when READY.md exists; until then write no summary for him. Then continue cycling on masterpiece-prompt.md Phase 2 and 3 until the time budget ends. Data, camera and dream rules unchanged. No git. Nothing outside this folder.

# Owner review, 2026-11-09 07:25 (after cycle 18). Read at the start of every cycle. Highest priority.

The time budget is extended by 20 hours: keep cycling until about 03:30 Paris time on 2026-11-10. Never write a closing summary before then.

Verdict on `screenshots/desktop-day.png` at cycle 18: real progress (density, expressway and tunnel, murals, tower, tram, market, rail yard, performance), but it is not a masterpiece yet. Fix these first, in this order:
1. The top of the frame is dirty: a smeared purple-brown band where the distant hills and sky should be, with tree silhouettes streaked across it. Distant terrain must be clean, soft, hazy, warm, with readable hill silhouettes and a clear sky gradient. No smearing, no streaks, no purple mud.
2. The terrain looks blotchy: large noisy patches of orange, green and brown. Make the countryside calm and coherent: dry golden grass, olive groves in tidy rows, terraced fields with stone walls, a few tracks. Lower the noise amplitude and contrast.
3. Big isolated blocks float outside the city (the large boxes on the left and right with murals, the box at bottom centre). Either integrate them into proper neighbourhoods with streets, or remove them. Nothing should look dropped at random.
4. The core still reads as a closed ring. Make the urban fabric spill beyond the ring road on at least two sides with continuous streets, so the city has no wall.
5. Overall still reads as a toy. Push toward the target photo: richer facade detail at District tier (window rhythm, balconies, shop awnings), roofs with clutter, softer and longer shadows, a subtler tilt-shift, warmer golden light, more readable vehicles and crowds.
Keep the performance gates and every data, camera and dream rule unchanged.

# Owner decision, 2026-11-09 07:27. Overrides masterpiece-prompt.md wherever they conflict.

## Time budget: one week
Keep cycling until about 07:30 Paris time on 2026-11-16. Never write a closing summary before then. Each omp session has its own time limit; when a session ends, the next one reads ITERATIONS.md and OWNER-NOTES.md and continues.

## The procedural-only rule is LIFTED: use free CC0 3D assets
The owner approved downloading free CC0 (public domain) 3D model packs onto this PC. This is the main lever to leave the "toy" look behind.
- Allowed sources, CC0 only, from their official sites: Kenney (kenney.nl: City Kit Commercial, City Kit Suburban, City Kit Roads, Car Kit, Nature Kit, Mini Characters and similar), Quaternius (quaternius.com: Ultimate Buildings, Cars, Nature, Animated People and similar), Poly Pizza (poly.pizza) only for models explicitly marked CC0. Check each pack's licence file says CC0 and copy it into `assets/LICENSES/`.
- Store everything under `assets/` in this folder (glTF/GLB preferred). Optimise: strip unused meshes, merge, quantise, and build texture atlases so the whole `assets/` folder stays under 60 MB and the phone download stays reasonable (lazy-load the Street-tier detail).
- Use `GLTFLoader` (and `DRACOLoader` / `KTX2Loader` if you compress) from the matching three@0.160.0 addons on jsDelivr via the import map. Addons are now allowed.
- Every placed model must still be instanced (`InstancedMesh` per model and material) so the performance gates hold: desktop avgFps >= 60, Pixel 6 emulation avgFps >= 60 (p1Low >= 50), the budgets in masterpiece-prompt.md may rise to 120 draw calls desktop and 70 phone if the fps gates still pass.
- Deliverable changes from one HTML file to this folder: `index.html` + `assets/`. It must run from a local static server (`node tools/serve.mjs`) and keep working offline after the first load. Keep a procedural fallback if assets fail to load, so the app never shows an empty scene.
- Target look: the owner's reference photos (dense contemporary Mediterranean city at golden hour, high oblique view). Replace the procedural boxes with real building models (mid-rise apartments with balconies, shops with awnings, glass towers, a hilltop church), real cars, buses, trams, trucks, scooters, real trees (olive, plane, palm, cypress), animated pedestrians, street furniture (lamps, benches, bus shelters, traffic lights, parasols). Mix, tint per instance and vary scale so it never looks like a repeated kit.
- Everything else stays: data logic, the eight data-to-motion rules, camera rules, the dream (one provisional road, at most two cards, "Take it"), no em dash, no git, nothing outside this folder, ignore the parent repo's CLAUDE.md and AGENTS.md.
