# Build LOCALTIME, first playable tranche. One shot. Work until it runs, then stop.

You are building a living simulation called Localtime in the current directory. It is a contemporary hillside city district that lives on its own, nourished by a private record of one person's real digital activity. The person watches it, never plays it. Deliver working files, not a plan. Do not ask questions. Do not stop at a design document. Verify in a real browser before you finish.

## Deliverables (all in the current directory)
1. `index.html`: ONE self-contained file. Inline CSS and JS. The only external dependency allowed is Three.js loaded as an ES module from `https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js` (use an import map, exact version, nothing else from the CDN). No build step, no npm, no bundler, no framework, no server: it must open from `file://` by double-click and also run when served statically.
2. `sample-export.json`: a realistic simulated day of data in the ScreenSearch export format (schema below), about 600 frames across 7 or 8 apps, covering 08:00 to 23:00, including one 3-hour focus block, one fragmented hour of 40 switches, two apps opened once for under a minute, and two unrelated apps whose texts share three salient words.
3. `screenshots/desktop-day.png`, `screenshots/desktop-night-dream.png`, `screenshots/pixel6-day.png`: taken by you with the browser tool, viewport 1600x900 for desktop and 412x915 for Pixel 6.
4. `README.md`: 20 lines maximum. How to open it, how to load a real export, the camera controls, what each moving thing means.

## Data source
Two modes, auto-detected:
- Real data: the user drops (drag and drop or file picker) a ScreenSearch export JSON. Format:
  { "schema": "screensearch.export.v1", "exported_at": 1720000000000, "from": null, "to": null,
    "frames": [ { "frame_id": 1, "captured_at": 1719999990000, "app_hint": "VS Code",
                  "window_title": "main.rs", "browser_url": null, "activity_type": null,
                  "content_text": "..." } ],
    "marks": [] }
  `captured_at` is UTC epoch ms, frames are roughly every few seconds while the screen changes. `app_hint` and `window_title` may be null. `content_text` is filtered OCR text.
- Simulated day: when nothing is loaded, load `sample-export.json` if it is reachable, otherwise generate the same simulated day in code. The page must look alive within 2 seconds of opening with zero user action.

Derive from frames, never ask the user to type anything:
- Apps ranked by total time -> the 6 to 9 quarters of the district (each quarter has a name label taken from the app name, a colour, and a block of buildings sized by total time).
- Session = consecutive frames of the same app with gaps under 90 s. Switch rate per 10-minute window -> ring-road congestion. Longest current session -> the avenue of that quarter flows green and a crane rises on its block.
- Window titles revisited on different days (or hours, in a one-day dataset) -> tram stops.
- Text: pick short readable phrases (3 to 6 words, from `content_text`) -> shop signs and posters, rotating slowly.
- Time of day: use the real wall clock for sun position and window lights; use the data's own timestamps for what is happening in the streets. Night is never gloomy: warm lit windows, sodium street lights.

## The world (what the person sees)
- A hillside district seen from above, contemporary, Mediterranean inland feel: ring road around the lower edge, one avenue up through the middle, a roundabout with a tram, a market square, mid-rise blocks, a few office towers, rooftop terraces, olive and plane trees, a rail yard at one edge. No water, no fantasy, no sci-fi, no HUD, no game UI, no score, no streak.
- Style: clean stylised low-poly 3D, saturated but plausible colours, warm low sun, long shadows, tilt-shift post effect (a cheap two-band blur is fine). Think Mini Motorways meets a tilt-shift photo, not a game level.
- Population: fewer figurants, more signal. Target at most 300 vehicles and 300 pedestrians total, all instanced (InstancedMesh). Pedestrians can be simple capsules or billboards.
- WHAT MOVES (all eight must be visibly driven by the data):
  1. Ring-road cars: density and stop-and-go follow the switch rate; one long session -> the avenue flows as a green wave.
  2. Tram: runs on schedule between stops that are recurring window titles.
  3. Pedestrians: walk between the quarters they were "mentioned" in (apps whose texts share words).
  4. Delivery vans: carry a parcel from one quarter to another when the same phrase appears in two apps within 10 minutes.
  5. Shop signs and posters: show phrases actually read, rotating.
  6. Cranes: one per quarter with an active session over 45 minutes; grows floor by floor.
  7. Window lights: proportion lit follows activity of that quarter in the last hour.
  8. Sky: a fragmented hour brings gusty broken cloud; a steady hour clears it.
- Neglect: a quarter with no activity for 3 days rolls its shutters down, dims, and waits. Nothing is ever removed.

## Camera (strict)
- Perspective camera, fixed pitch about 40 degrees. Never free-look, never first person.
- Rotation only in 90-degree snaps with a 400 ms eased tween: two-finger twist on touch, keys Q and E, and two on-screen chevrons.
- Zoom between two hard bounds with three named tiers: District (default, the whole district framed on open, on any screen ratio), Block, Street. Pinch and wheel; snap to the nearest tier when the gesture ends.
- Pan by one-finger drag or mouse drag, clamped to the district bounds.
- Tap or click a building: a small card (quarter name, time today, last phrase read). Click elsewhere closes it. No teleport.
- Default view on a 412x915 phone must frame the entire district with the ring road visible.

## The dream (required)
- Trigger: the data has a gap of over 4 hours (the person was away), or the user presses the single "sleep" button (bottom right, an icon, no label) which fast-forwards to night.
- During the dream (night): a road crew with cones, striped barriers and two floodlights cuts ONE provisional road between two quarters. Choose the pair by this rule: the strongest pair of apps that share salient words but never appeared in the same hour. Vans carry two shop signs across. The rest of the district is dark and quiet.
- At waking (data resumes or the user presses the button again): the camera glides to the chantier over 2 seconds, then returns control. Show at most TWO cards, each a hypothesis in one sentence built from the actual words shared, e.g. "The word 'invoice' kept appearing in both Slack and Figma. Open a note that joins them?" Each card has "Take it" and nothing else; ignoring it is the other option.
- Taken: the provisional road becomes permanent, a pop-up market appears on it, and the hypothesis sentence is stored in localStorage under `localtime.dreams.taken`.
- Ignored for 24 hours of data (or the next sleep): barriers are packed away and the road fades back to ground over 10 seconds.
- Hard limit: never more than two cards per waking, never any reminder, never any badge counting them.

## Performance budget
- 60 fps on a mid-range phone: under 150k triangles, under 40 draw calls, lights baked into vertex colours or a single directional light with one shadow map of 1024, shadows off on touch devices under 800px wide. Use InstancedMesh for buildings, trees, cars, pedestrians. No per-frame allocations in the render loop.
- Add an optional FPS readout toggled by pressing F (hidden by default).

## Verification before you stop (mandatory)
1. Open `index.html` with the browser tool at 1600x900. Wait 3 seconds. Confirm zero console errors, confirm the scene renders (not a black canvas), take `screenshots/desktop-day.png`.
2. Press the sleep button, wait 3 seconds, take `screenshots/desktop-night-dream.png`, confirm a road crew and the provisional road are visible. Press it again, confirm the camera glides and at most two cards appear.
3. Set the viewport to 412x915, reload, confirm the whole district is framed, take `screenshots/pixel6-day.png`.
4. Drag-and-drop or programmatically load `sample-export.json` and confirm the quarter labels change to the app names in it.
5. Paste the raw console output and the three file sizes in your final message. If any step fails, fix it and run the checks again. Do not declare done on a failed check.

## Constraints
- No em dash character anywhere in code, text or README.
- No external images, fonts or assets. Everything procedural.
- Do not touch any file outside the current directory. Do not use git. Do not install anything.
- Write the code in large coherent chunks, not dozens of tiny edits.
