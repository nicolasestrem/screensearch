# Localtime source contract

Localtime ships as ONE self-contained `index.html`. It is authored as fragments in `src/` and
assembled by `tools/assemble.mjs`. This file is the binding interface between fragments.
Read `../build-prompt.md` for the product requirements; this file only fixes the seams.

## Assembly rules (every fragment MUST follow)

- Fragments are ES modules named `src/*.mjs` so `node --check src/<file>.mjs` parses them.
- The assembler concatenates fragments, in the order listed below, into a single
  `<script type="module">`. It emits `import * as THREE from 'three';` at the very top of the
  bundle and deletes every fragment line that imports bare `three`. Named imports of
  `three/addons/<file>.js` are collected from every fragment, deduplicated by module path with
  their names merged, and emitted as single-line imports directly after the THREE import; any
  other import is an assembly error. It then strips the leading `export ` keyword from
  `export function` / `export const` / `export class` lines.
  Therefore: one shared module scope. Every top-level identifier you declare is visible to all
  other fragments. Each fragment declares EXACTLY ONE top-level identifier (its factory, below)
  and keeps all helpers, constants and classes inside that factory's closure. The one
  documented exception is `assets.mjs`, which also exports `registerOfflineCache` as a second
  top-level function. Nothing else at top level. No top-level `await`.
- `THREE` is a module-scope binding provided by `main.mjs` (`import * as THREE from 'three'`),
  but the factories also receive it as their first argument; use the argument.
  `simday.mjs` and `data.mjs` must not reference THREE or the DOM at all (they run in Node).
- Fragment order in the bundle: `simday`, `data`, `assets`, `world`, `agents`, `camera`,
  `dream`, `post`, `main`. `main` runs last and is the only fragment with side effects at load.
- Import map in the shell:
  `{"imports":{"three":"https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js","three/addons/":"https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/"}}`.
  Nothing else from the network: the only addon used is `GLTFLoader` (and any file it imports
  from the same pinned version). Write other three.js addon behaviour inline.
- No em dash character (U+2014) anywhere: code, comments, strings, CSS, markdown. Use a plain
  hyphen or a colon. `tools/check.mjs` fails the build if one is found.
- No per-frame allocations in anything called from the render loop: hoist `THREE.Vector3`,
  `Matrix4`, `Quaternion`, `Color`, `Object3D` dummies to factory scope and reuse them; no
  array literals, closures, or string building inside `update(dt, sim)`.
- Draw-call budget is 40 for the whole frame, triangle budget 150k. Use `InstancedMesh` for
  buildings, windows, trees, cars, vans, pedestrians, cones, barriers, clouds. Merge static
  roads, pavements, terrain into as few meshes as possible. Shop signs and posters share ONE
  canvas atlas texture and ONE mesh.
- Colors: saturated but plausible, warm; use `THREE.Color` hex numbers. Night is never gloomy:
  warm lit windows (0xffc477-ish), sodium street lights (0xffa64d-ish).
- Coordinates: Y up. The district spans roughly X in [-120, 120], Z in [-120, 120] with the
  hill rising toward -Z (top of the view at the default rotation). Units are metres-ish. The
  ring road circles the lower edge (around +Z and the sides), the avenue runs from the ring
  road at +Z up the middle to the hilltop at -Z, the roundabout with the tram sits where the
  avenue meets the ring road, the market square is mid-slope on the avenue, the rail yard is
  along the +X edge.
- Dates in data are UTC epoch ms. "Hour of day" for stops/pairs is computed in the local
  timezone of the data day, taken as UTC+1 (Europe/Paris, CET in November) for the simulated
  day, and the browser's local timezone for real data; expose `model.tzOffsetMs`.

## File ownership

| fragment | owner | exports exactly |
| --- | --- | --- |
| `src/simday.mjs` | A | `export function generateSimulatedDay(seed = 20261105)` |
| `src/data.mjs` | B | `export function analyzeExport(exportObj, options = {})` |
| `src/assets.mjs` | I | `export function createAssetLoader(THREE, GLTFLoader)` and `export function registerOfflineCache()` |
| `src/world.mjs` | C | `export function buildWorld(THREE, model, opts)` |
| `src/agents.mjs` | D | `export function createAgents(THREE, world, model, opts)` |
| `src/camera.mjs` | E | `export function createCameraRig(THREE, dom, bounds, opts)` |
| `src/dream.mjs` | F | `export function createDream(THREE, world, model, agents, rig, ui)` |
| `src/post.mjs` | G | `export function createPost(THREE, renderer, scene, camera, opts)` |
| `src/style.css`, `src/shell.html` | G | markup + CSS, see DOM contract |
| `src/main.mjs` | H | no export; bootstraps (`function main()` as its single top-level identifier, invoked at the end of the fragment via `main();` which is the one permitted statement) |
| `tools/assemble.mjs`, `tools/check.mjs` | H | build + lint |
| `tools/gen-sample.mjs`, `tools/check-sample.mjs` | A | writes and validates `sample-export.json` |

## Export format (input)

```json
{ "schema": "screensearch.export.v1", "exported_at": 1720000000000, "from": null, "to": null,
  "frames": [ { "frame_id": 1, "captured_at": 1719999990000, "app_hint": "VS Code",
                "window_title": "main.rs", "browser_url": null, "activity_type": null,
                "content_text": "..." } ],
  "marks": [] }
```
`app_hint`, `window_title`, `browser_url`, `activity_type`, `content_text` may be null. Frames
may be unsorted. `analyzeExport` must tolerate missing/empty `frames` (return a model with zero
quarters; `main` then falls back to the simulated day).

## simday.mjs (A)

`generateSimulatedDay(seed)` is pure and deterministic (own mulberry32 PRNG seeded by `seed`),
returns a complete export object as above. Day: 2026-11-05, frames from 08:00 to 23:00 local
(UTC+1), about 600 frames, 7 or 8 apps with realistic names (e.g. "VS Code", "Firefox",
"Slack", "Figma", "Obsidian", "Spotify", "Outlook", "Terminal"), realistic window titles and
`content_text` of 20 to 80 words each (filtered OCR flavour: UI words, sentences, numbers).
Must contain, verifiably:
1. one 3-hour focus block: a single session (same app, gaps under 90 s) lasting at least 3 h;
2. one fragmented hour containing 40 app switches;
3. two apps opened exactly once for under a minute total;
4. two unrelated apps (e.g. Figma and Outlook) whose texts share exactly three salient words
   (words of 5+ letters that appear in both and in no other app), e.g. "invoice", "quarterly",
   "warehouse";
5. several window titles revisited in different hours (tram stops);
6. at least two "same phrase in two apps within 10 minutes" events (parcels).
The same function, called with the default seed in the browser, must produce the same bytes
as `sample-export.json` (`JSON.stringify(obj, null, 1)`), so the file and the fallback agree.

## data.mjs (B)

`analyzeExport(exportObj, { now, tzOffsetMs, maxQuarters = 9 })` returns a `Model`:

```js
Model = {
  frames,            // sorted copy, each frame normalised: { id, t, app, title, url, text } (app defaults to "Unknown", text to "")
  t0, t1,            // ms, first and last frame
  tzOffsetMs,        // used for hour-of-day bucketing
  quarters: [{       // ranked by totalMs desc, max maxQuarters, min 1 (there are no padding quarters)
    idx, name, color /* hex number from a fixed warm palette by rank */, totalMs, share /* 0..1 of all time */,
    frameCount, firstActiveAt, lastActiveAt,
    buildingCount /* 6..40, proportional to share */, towerCount /* 0..3, for the top-ranked quarters */,
    neglected /* true when refNow - lastActiveAt > 3 days, refNow = options.now ?? t1 (data-relative,
                 so the one-day simulated day has NO neglected quarters; a multi-day export whose app
                 went quiet 4 days before its last frame does) */,
    phrases /* string[] of 3..6 word phrases from this app's text, deduped, max 24 */,
    lastPhrase /* string or "" */,
    titles /* distinct window titles */,
  }],
  quarterOf,         // Map app name -> quarter idx (apps beyond maxQuarters map to undefined)
  sessions: [{ q /* quarter idx or -1 */, start, end, durationMs, frameCount }], // gap >= 90s splits
  windows10: [{ start, end, switches, activity /* number[] per quarter, ms active */ }], // 10 min windows covering t0..t1
  stops: [{ title, q, hours /* distinct local hours */, visits }],   // titles seen in 2+ distinct local hours (or days), max 2 stops per quarter, filled to 8 by global visits, q = quarter with most visits
  phrases: [{ text, q, at }],                                        // all phrases, chronological
  pairs: [{ a, b, words /* string[] */, strength /* words.length weighted by frequency */, sameHour /* both quarters active in some common local hour */ }],
  parcels: [{ phrase, from, to, at }],                               // same phrase in two different quarters within 10 min
  dreamPair,         // strongest pair with sameHour === false, or null
  gaps: [{ start, end }],                                            // gaps > 4 h between consecutive frames
  switchRateAt(t),      // 0..1: switches in the 10-min window containing t divided by 8, clamped.
                        // 8 switches per 10 min = a switch every 75 s, which is high real-world
                        // fragmentation (40 switches in an hour spread over ~7 per window), so the
                        // fragmented hour of the sample day reads near 1.0 and the focus block 0.
                        // Do NOT divide by 40: that would render the fragmented hour as no traffic.
  fragmentationAt(t),   // 0..1: switches in the trailing 60 min divided by 40, clamped (the hourly
                        // scale, which is what the cloud cover follows).
  activityLastHour(t),  // Float32Array per quarter: active ms in [t-1h, t] divided by 1h, clamped 0..1 (reuse one array, no allocation)
  activeSessionAt(t),   // the session containing t, or null
  cranesAt(t),          // array (reused) of { q, floors } for quarters whose active session at t exceeds 45 min; floors = Math.min(12, 1 + Math.floor((elapsed - 45min) / 10min))
  longestSessionBefore(t), // the longest session with start <= t, or null (its quarter gets the green-wave avenue)
}
```

Salient words: lowercase, letters only, length >= 5, not in a small stop list (common English
plus UI words: "window", "button", "settings", "untitled", "https", "search", "about", "which",
"their", "there", "would", "could", "should", "where", "while"). Phrase picking: split text on
sentence punctuation and newlines, keep 3..6 word runs with no digits-only tokens, trim by whole
words to 40 characters so a short sentence survives intact (32 cut sentences mid-phrase), title-case first letter.

## assets.mjs (I)

`createAssetLoader(THREE, GLTFLoader)` returns `{ load(options) }`. `load` resolves with
`Assets | null`, never throws, and logs at most one `console.warn`; null means keep the
procedural fallback. `options = { url = 'assets/manifest.json', tier = 'district',
timeoutMs = 8000, onProgress }`; only models whose manifest `tier` matches are loaded, so a
caller that wants street detail calls `load({ tier: 'street' })` again and merges the result.

`assets/manifest.json` (version 1) is the asset contract:

```json
{ "version": 1,
  "packs": [ { "id": "<packId>", "file": "assets/<pack>.glb",
               "licence": "assets/LICENSES/<pack>.txt", "source": "<pack page>" } ],
  "models": { "<modelName>": { "pack": "<packId>", "category": "apartment|shop|tower|villa|church|warehouse|tree-olive|tree-cypress|tree-palm|tree-plane|car|van|truck|bus|lamp|bench|shelter|signal|parasol|prop",
                                "tier": "district|street", "tris": 0,
                                "bounds": { "min": [0, 0, 0], "max": [1, 1, 1] }, "tintable": true } } }
```

Each pack GLB has one scene whose top-level nodes are named `<modelName>`; a node may hold
several primitives sharing ONE material (the pack palette texture). Pack `file` paths resolve
against the page base (the site root), not the manifest URL. The loader merges every primitive
of a node into one geometry with position, normal and uv, baked through the node transform and
recentred so the origin is the base centre (min y = 0, x/z centred), and shares one
`MeshStandardMaterial` per pack (palette map, white color, glTF roughness and metalness).

`Assets = { models: Map<name, Model>, byCategory(category), pick(category, seed),
makeInstanced(model, count, { shadows, tintable, dynamic }) }`.
`Model = { name, category, tier, geometry, material, bounds: Box3, footprint: { w, d, h },
tris, tintable }`. `makeInstanced` sets castShadow and receiveShadow from `shadows`, sets
`frustumCulled = false`, allocates `instanceColor` when tintable, and switches the instance
matrix to `DynamicDrawUsage` only with `dynamic: true`. `registerOfflineCache()` registers
`assets/sw.js` on http(s) pages with scope `./` and never throws. Every placed model is an
`InstancedMesh` built through `makeInstanced`.

## world.mjs (C)

`buildWorld(THREE, model, { shadows, isTouch })` returns:

```js
World = {
  group,               // THREE.Group with everything static; main adds it to the scene
  bounds: { minX, maxX, minZ, maxZ, center /* Vector3 */, radius /* number, includes ring road */ },
  groundHeightAt(x, z) -> y,   // hillside height function; everything sits on it
  quarters: [{ idx, center /* Vector3 on ground */, radius, labelPosition /* Vector3 above roofs */,
               signSlots: [{ position, rotationY }] /* 2..4 per quarter */, craneSlot: { position } ,
               doorPoints: [Vector3] /* 3..6 pavement points where pedestrians spawn/arrive */ }],
  roads: { ring /* THREE.CatmullRomCurve3 closed */, avenue /* open, from ring at +Z to hilltop */,
           tram /* closed loop through the roundabout, the avenue and tram stops */,
           rail /* open, along the +X edge */ },
  roundabout: { center /* Vector3 */, radius },
  marketSquare: { center, size },
  tramStops: [{ position /* on tram curve */, u /* 0..1 param on tram curve */, stopIdx /* index in model.stops */ }],
  buildings,           // InstancedMesh; instance i belongs to buildingQuarter[i]
  buildingQuarter,     // Int16Array instanceId -> quarter idx (for picking)
  windows,             // InstancedMesh of window quads, colour settable: setWindowLit(i, lit) and windowQuarter Int16Array
  windowQuarter,
  windowCount,
  trees,               // InstancedMesh (olive and plane trees as two instance groups is fine if it is still one mesh)
  signs: { setText(slotIdx, text), slotCount, slotQuarter /* Int16Array */ , mesh },  // one atlas canvas (CanvasTexture), one mesh
  cranes: { setFloors(qIdx, floors /* 0 hides */ ) },      // instanced mast segments + jib; max 12 floors
  setNeglected(qIdx, bool),                                 // shutters down, desaturate/dim that quarter's instances
  lights: { sun /* DirectionalLight, castShadow if opts.shadows, 1024 map */, hemi, setTimeOfDay(hour /* 0..24 */, night /* bool */) },
  streetLights: { setOn(bool) },                            // instanced sodium lamp heads, emissive-looking colour
  pickTargets: [buildings],                                 // meshes main raycasts; instanceId -> buildingQuarter
}
```
Rail yard: a few static wagons and tracks along +X, part of `group`. Rooftop terraces, olive
and plane trees, a market square with stalls, mid-rise blocks and up to 3 towers per the
model's `towerCount`. Keep the complete static world under 20 draw calls.

Quality hooks (called by `main` only, never per frame, and they must not allocate):
- `lights.setShadowMapSize(n)`: `n` is 0 (directional shadow map off) or a power of two up to
  4096; replaces the shadow map and disposes the old one.
- `setDetailLevel(level)` for level 0, 1, 2: toggles optional instances (roof clutter, minor
  vegetation, small props) whose visibility was decided at build time. No geometry rebuild.
- `roads.expressway`: open `CatmullRomCurve3` of the elevated expressway deck centreline, and
  `roads.expresswayDeckY`: number, the deck surface Y, so traffic agents follow it.
- `roads.spurs`: open `CatmullRomCurve3` list of arterial roads that leave the frame.

Composition rules (owner verdict "a map from 1993", "stupid Middle Ages"): `roads.ring` stays a
closed curve for traffic but is an irregular arterial loop, never a circle, and the city
continues outward along `roads.spurs` instead of stopping at a wall. No small gabled cottage
outside the ring: contemporary forms only (flat-roof blocks, villas with pools far out, a
supermarket, a petrol station, a school with a pitch, a sports hall, warehouses by the yard).
The hilltop old village stays small, on the summit only.

## agents.mjs (D)

`createAgents(THREE, world, model, { maxVehicles = 420, maxPedestrians = 700, assets = null,
mobile = false })`
creates instanced movers (cars on the ring road and avenue, delivery vans, pedestrians, tram,
clouds) and returns:

`mobile` is the boolean main resolved from its single mobile predicate; it is a flag, never a
second test, so a narrow window is not a phone and a wide tablet is not a desktop. When true
the pools are halved (`>> 1`) and then trimmed to 100 car slots, 200 pedestrians, 64 parked
bays and 6 delivery vans, with the sparser spur kerbside fill; when false the caps are exactly
the two ceilings above. A caller that omits the option gets the desktop caps.

`assets` is the same `Assets | null` the world receives. When it carries the low car models
(`sedan-low`, `suv-low`, `hatchback-sports-low`, `taxi-low`, `van-low`, `delivery-low`,
`truck-low`), the avenue's modelled carriageway draws one `InstancedMesh` per model with the
same matrix write path, follow gap, lane offsets and junction rules as the procedural box; every
drawn body is fitted inside the 1.92 x 4.1 m envelope so no lane geometry moves. A null cache,
or a phone that never loads the street tier, keeps the procedural fleet exactly as before.

```js
Agents = {
  group,                       // added to scene by main
  update(dt, sim),             // sim = { tData, wallHour, night, dreaming, dt }
  setDataModel(model),         // called after a new export is loaded; rebuild schedules, no new meshes
  setPopulationScale(v),       // 0..1 multiplier on visible movers, applied by the notch ladder
  vans: { dispatch(fromQ, toQ, payloadKind) },  // payloadKind: 'parcel' | 'sign'; used by dream
  tram: { positionAt(out) },
  counts: { vehicles, pedestrians },  // live numbers for the F readout
}
```

`setPopulationScale` is the one control main's notch ladder turns to shed work: it hides the
highest-numbered movers and their lights, never adds a mesh and never reallocates an instance
buffer, so a scale change costs no draw call and no upload.
Behaviour (all data-driven):
1. Ring cars: density = 60 + 180 * switchRateAt(tData); stop-and-go when switchRateAt > 0.5.
   Avenue: cars flow in a green wave when `longestSessionBefore(tData)` belongs to a quarter, and
   its avenue segment tints green (set a uniform/colour on the avenue mesh via world, or tint
   the car instance colours on that segment).
2. Tram: loops `world.roads.tram` at constant speed, dwelling 3 s at each `tramStops` entry.
3. Pedestrians: walk between `doorPoints` of quarters linked by `model.pairs`; count scales with
   `activityLastHour` total.
4. Vans: for each `model.parcels` whose `at` is crossed by `tData`, dispatch one van from -> to
   carrying a parcel box; `dispatch(..., 'sign')` carries a small sign plane (dream).
5. Signs: cycle `world.signs.setText(slot, phrase)` every 8 s through the quarter's phrases.
6. Cranes: `world.cranes.setFloors(q, floors)` from `cranesAt(tData)`.
7. Window lights: lit proportion per quarter = max(night ? 0.25 : 0.0, activityLastHour[q]);
   update at most 1/10 of windows per frame to spread work.
8. Sky: instanced clouds; count and speed follow `fragmentationAt(tData)`; a steady hour clears.
9. Sun and street lights (agents owns all time-driven visual state): call
   `world.lights.setTimeOfDay(sim.wallHour, sim.night)` and `world.streetLights.setOn(sim.night)`
   only when the rounded hour or the night flag changes, never every frame.
10. Neglect: call `world.setNeglected(q, model.quarters[q].neglected)` for each quarter, only on a
    transition (guard with a per-quarter boolean cache), so a quiet quarter's shutters stay down.
During `sim.dreaming`, everything except vans dispatched by the dream idles (cars parked, peds
gone, lights at 10 percent).

## camera.mjs (E)

`createCameraRig(THREE, dom, bounds, { isTouch })` installs pointer, wheel, touch and keyboard
(Q/E) listeners on `dom` and returns:

```js
Rig = {
  camera,                      // PerspectiveCamera, pitch fixed at 40 degrees below horizontal
  update(dt),                  // tweens; no allocation
  rotate(dir),                 // +1 or -1 : 90 degree snap, 400 ms eased
  tier,                        // 'district' | 'block' | 'street' (read-only)
  setTier(name, animate = true),
  frameDistrict(),             // recompute the district distance so the whole bounds sphere fits the current aspect (portrait included), then set tier district
  onResize(width, height),
  glideTo(targetVector3, ms, onDone),  // moves the look-at target (and zoom to block) over ms, then returns control
  lookAt,                      // Vector3 current look-at target (read-only)
  raycast(clientX, clientY, meshes, outIntersections), // fills and returns intersections array (reused)
  onTap(handler),              // handler(clientX, clientY) fired for a click/tap that was not a drag
}
```
Rules: rotation only by 90 degree snaps (Q/E keys, two-finger twist on touch, and the
`#rot-left` / `#rot-right` buttons which main wires to `rotate`). Zoom only by wheel or pinch,
clamped between the district and street distances, snapping to the nearest tier when the
gesture ends (wheel idle 250 ms, or touchend). Pan by drag, clamped to `bounds`. Never free
look, never roll, never change pitch.

## dream.mjs (F)

`createDream(THREE, world, model, agents, rig, ui)` where
`ui = { cardsRoot /* HTMLElement */, setNightOverride(bool), onWake(fn) }` returns:

```js
Dream = {
  group,                       // crew, cones, barriers, floodlights, provisional road mesh, market; added by main
  update(dt, sim),
  sleep(),                     // starts the dream (night override on), builds the chantier between model.dreamPair quarters
  wake(),                      // glides the camera 2 s to the chantier, then shows at most two cards
  toggle(),                    // sleep button handler
  state,                       // 'awake' | 'dreaming' | 'waking' | 'offered'
  setDataModel(model),
  notifyGap(gapStart, gapEnd), // main calls when tData enters a gap > 4 h: equivalent to sleep(); leaving the gap wakes
}
```
Chantier: cones (instanced), striped barriers (instanced), two floodlights (emissive quads plus
two SpotLights or one PointLight pair with no shadows), a road crew of 4 to 6 pedestrian
capsules, and the provisional road mesh (a ribbon along a CatmullRom between the two quarter
centres following `groundHeightAt`) growing from 0 to 1 over the dream. Vans carry two shop
signs across via `agents.vans.dispatch(a, b, 'sign')`.
Cards: at most two, text built from `dreamPair.words`, pattern:
`The word 'X' kept appearing in both A and B. Open a note that joins them?` with one button
"Take it" only. Taking: road becomes permanent (asphalt colour, barriers removed, a pop-up
market with 3 to 5 stalls appears on it), the sentence is appended to the JSON array in
`localStorage['localtime.dreams.taken']`. Ignoring: at the next sleep or after 24 h of data
time, barriers pack away and the road fades to ground over 10 s. No reminders, no badges, no
counters. Also the whole district is dark and quiet while dreaming (agents handle their part).
If `model.dreamPair` is null, pick the strongest pair overall; if there are fewer than two
quarters, the dream only darkens the sky and shows no cards.

## post.mjs (G)

`createPost(THREE, renderer, scene, camera, opts)` with `opts = { enabled, msaa, fxaa, bloom,
resolutionScale }` returns `{ render(), setSize(w, h), setResolutionScale(s), setBloom(bool),
setDreamBloom(k), setDistanceGrade(k), enabled, dispose() }`. `msaa` and `fxaa` are
creation-time; dispose and recreate across the mobile boundary. `setResolutionScale(s)` takes
0.5..1 and scales the post
chain only, never the scene target. `setDreamBloom(k)` takes 0..1 and multiplies the night
bloom's strength, threshold and radius towards the warmer dream settings; the day frame is
untouched by construction, and at k = 0 the awake night frame is exactly the cycle 19 frame.
`setDistanceGrade(k)` takes 0..1 and scales the cool
distance amount only (`COOL_DIST_AMOUNT * k`), leaving the height key, the luminance key and
both shade keys alone; main calls it from the camera distance to the look-at.

Pipeline: scene into a target with `samples: msaa ? 4 : 0`; a bright pass at quarter resolution
plus two separable blurs added back in the composite (bloom); the scene blurred at half
resolution in two passes; a composite with a vertical two-band tilt-shift mask, the grade, the
FXAA taps when the scene target has no samples, and a subtle vignette. Materials via
`THREE.ShaderMaterial` + a full-screen triangle, no addons. When `enabled` is false, render
directly.

Tilt-shift: the sharp band covers the middle 62 percent of the height (`BAND_EDGE` 0.62,
`BAND_FEATHER` 0.30) and the gaussian radius is 0.7 half-res texels. Bloom is mild and hour
gated: linear luma threshold 0.75 with a 0.35 knee and strength 0.36 by day (`BLOOM_STRENGTH x
BLOOM_DAY_WEIGHT`), lerped at night to threshold 0.95, knee 0.30, strength 0.22 and a 0.75
radius factor, so lit windows, lamps and the sun disc glow while sky, stone and night street
paving stay clean. `setBloom(false)` skips the bright and blur passes entirely. The composite
keeps the exact sRGB transfer and the renderer stays on NoToneMapping. The near-black lift is
blue dominant at night (`uNightLift x uNight x vec3(0.05, 0.12, 1.0)`) so night never reads
grey-green, and the day term is untouched by construction (every night term is multiplied by
`uNight`).

Frame cost: 6 post draws with bloom, 3 with bloom off, 0 when disabled.

## DOM contract (G owns markup and CSS, H wires it)

`shell.html` is the `<body>` content. Required ids and roles:
- `<canvas id="view">` full-window, touch-action none.
- `<div id="labels">` overlay container; main creates `<div class="label">` children per quarter
  and positions them with `transform: translate(-50%,-100%) translate(xpx, ypx)`; CSS makes them
  unselectable, pointer-events none, small caps, with a colour swatch using `--q` custom property.
- `<button id="rot-left" class="chev">` and `<button id="rot-right" class="chev">` (inline SVG chevrons, no text).
- `<button id="sleep" aria-label="Sleep">` bottom right, icon only (inline SVG moon), no label.
- `<div id="card" hidden>` the building card: `<h3 class="card-title">`, `<p class="card-time">`, `<p class="card-phrase">`.
- `<div id="cards">` dream hypothesis cards container; each card is `<div class="dream-card"><p></p><button>Take it</button></div>`.
- `<div id="drop" hidden>` drop-zone overlay shown during dragover with the text "Drop a ScreenSearch export".
- `<input id="file" type="file" accept="application/json,.json" hidden>` and a small
  `<button id="open-file" aria-label="Load export">` (inline SVG folder icon) bottom left.
- `<div id="fps" hidden>` monospace readout, top left, toggled with F; main writes
  `fps / calls / tris / vehicles / peds` into it.
No fonts loaded; use `system-ui`. No images. Buttons are 44px targets with translucent warm
backgrounds; nothing looks like a game HUD (no bars, no score, no streaks).

## main.mjs (H)

Bootstraps in this order: renderer on `#view` (antialias on desktop, `powerPreference
'high-performance'`, pixel ratio capped at 2 on desktop, 1.5 on touch), shadows on unless the
device is touch. There is exactly one mobile test, resolved here once: `pointer: coarse` or
`navigator.maxTouchPoints > 0`, with no width component, so a narrow desktop window keeps the
desktop profile and a wide tablet gets the phone's. Data: `fetch('sample-export.json')` with
a 1500 ms timeout, on any failure `generateSimulatedDay()`. Then `analyzeExport`, `buildWorld`, `createAgents`,
`createCameraRig`, `createDream`, `createPost`; first frame must render within 2 s of load.
Assets: one `createAssetLoader(THREE, GLTFLoader)` instance per page, and `load({ tier, timeoutMs })`
starts before the first `install` and resolves beside the sample fetch (`Promise.all`), so the
first synchronous frame already carries the model instances. The deadline is 8000 ms, 6000 ms
when `detectMode() === 'mobile'`, and a null result keeps the procedural world. The desktop
requests the `district` and `street` tiers in parallel and merges them into one cache, so the
street detail a close camera needs is already there at the first frame; the phone requests the
`district` tier only, because its triangle ceiling leaves no room for street detail. A
local-file page cannot fetch the packs and therefore keeps the procedural build without
spending a request that would only show up as a failed load. The same
`Assets | null` is handed to every later `install` as `buildWorld(THREE, model, { shadows,
isTouch, assets })` and is never reloaded. Flags: `?noassets=1` skips the loader entirely
(procedural fallback), `?assets=street` forces the street tier on a device that would otherwise
keep to the district set. `registerOfflineCache()` is called once, after
the first frame has been rendered, so it never sits in front of the paint.
Sim clock: `tData` advances at 60x real time from `model.t0`, wraps at `model.t1 + 1 h` back
to `t0`. `wallHour` from `new Date()` (local). `night = wallHour < 7 || wallHour >= 19 || nightOverride`.
Gap handling: when `tData` enters a `model.gaps` entry, call `dream.notifyGap`.
Drag and drop + file picker: parse JSON, validate `schema === 'screensearch.export.v1'`, rebuild
model, dispose and rebuild world/agents/dream (keep the camera, keep the asset cache), update labels.
Teardown: `disposeGroup` frees geometry, material and their maps for the procedural world, but an
object tagged `userData.sharedGeometry === true` only has its own instance attributes released
(`mesh.dispose()`), because its geometry, material and pack texture belong to the asset cache.
Labels: per quarter, projected `labelPosition`, hidden when behind the camera.
Building card: `rig.onTap` -> raycast `world.pickTargets`, show `#card` with quarter name,
time today (h:mm) and last phrase; the tapped instance maps through the mesh's
`userData.quarterOf` when it exists, else through `world.buildingQuarter`; tap elsewhere closes.
Keys: Q/E rotate, F toggles `#fps`. Chevrons call `rig.rotate`. Sleep button calls `dream.toggle()`.
Debug API (not UI): `window.localtime = { stats(), state(), loadExport(obj), model, world, rig,
debugFraming(), assets }` where `stats()` returns `{ fps, calls, triangles, vehicles,
pedestrians, shadows, dream }` from `renderer.info` plus the read-only dream phase, `assets` is the live pack report
`{ loaded, models, tier }`, and `debugFraming()` returns the projected screen-space bounding box
of `world.roads.ring` sample points `{ minX, minY, maxX, maxY, width, height }` so a test can
assert the ring road is fully inside the viewport.
