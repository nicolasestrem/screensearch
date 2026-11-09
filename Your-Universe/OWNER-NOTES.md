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
