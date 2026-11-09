# Localtime

A contemporary hillside district that lives on its own, nourished by one person's real screen history. You watch it, never play it.

## Open
Serve the folder with the bundled static server and open the address it prints:

```
node tools/serve.mjs
```

`index.html` plus `assets/` is the deliverable. The page needs a server rather than `file://` because the city is built partly from glTF models that the browser may not read from a local file. After the first load a service worker keeps `index.html`, the model packs and the Three.js modules in the cache, so the district still opens with the network off.

The only network fetch is Three.js 0.160.0 and its `GLTFLoader` addon from jsDelivr. The models in `assets/` ship with the folder (3.1 MB, CC0, licences in `assets/LICENSES/`). If they fail to load, or with `?noassets=1`, the district falls back to its fully procedural build and nothing is missing from the scene.

## Your own history
Drag a ScreenSearch export onto the window, or use the folder button (bottom left): `{"schema":"screensearch.export.v1","frames":[{"captured_at":...,"app_hint":"...","window_title":"...","content_text":"..."}]}`. Apps become quarters, ranked by time.

## Camera
Drag to pan. Wheel or pinch to zoom across District, Block and Street, snapping to the nearest tier. Q, E, a two-finger twist or the chevrons rotate in 90 degree snaps. Click a building for its card. F toggles an FPS readout.

## What moves
Ring-road cars and stop-and-go: how often you switched. Green avenue flow: your longest session. Cranes: a session past 45 minutes. Tram: window titles you revisit. Pedestrians: apps sharing words. Vans: a phrase read in two apps within ten minutes. Signs: phrases actually read. Lit windows: last hour of activity. Clouds: how fragmented the hour was. A quiet quarter shuts its shutters.

## The dream
The moon button (bottom right) fast-forwards to night: a crew lays a provisional road between two quarters that share words but never shared an hour. Waking glides there and offers at most two hypotheses built from those words. "Take it" makes the road permanent.

## The models
Kenney CC0 kits: city commercial, city suburban, city roads, car kit and nature kit, merged by `tools/pack-assets.mjs` into one GLB per kit with one shared palette material. `node tools/fetch-assets.mjs` re-downloads the original packs into `.cache/packs` (not shipped), `--only <id>` for one of them; `node tools/pack-assets.mjs` rebuilds `assets/` from them; `node tools/check-assets.mjs` verifies size, licences, node names and triangle counts.

The staged `kenney-mini-characters` kit is fetched but not yet packed, so `assets/` carries a licence for it and no geometry; `assets/LICENSES/NOT-SHIPPED.md` says exactly what ships and why the Quaternius packs do not.

## Checks
```
node tools/assemble.mjs     # src/ fragments -> index.html
node tools/check.mjs        # lint and structural checks
node tools/check-assets.mjs # asset folder, licences, triangle totals
node tools/check-dom.mjs    # markup and stylesheet assertions
node tools/check-pixels.mjs # rendered-frame assertions
node tools/check-sample.mjs # export parsing
node tools/smoke-data.mjs   # data logic
node tools/stamp-sw.mjs     # assets/sw.js cache version <- sha256 of assets/manifest.json
node tools/stamp-sw.mjs --check  # fails when a rebuilt pack would serve behind a stale cache
```

The service worker serves `assets/` cache-first, which is what makes the second visit instant and an
offline visit possible, and also what would hide a rebuilt pack from anyone who already loaded the
page. So its cache name carries the payload's own digest, and `check-dom.mjs` fails if the two
disagree; run `stamp-sw.mjs` after `pack-assets.mjs`. Offline, a reload draws the identical frame it
drew online, including the same draw calls and triangles, with `favicon.ico` the only failed
request.
