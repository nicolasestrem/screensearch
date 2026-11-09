# Cycle 26 brief: the top of the frame (OWNER-NOTES 2026-11-09 14:00, item 2)

Owner words, binding: "Top of frame (desktop-day.png): the far tree rows render as streaked lines
and the hills are a dirty band. Beyond the second ridge draw no individual trees at all: hills as
clean soft silhouettes in warm haze, sky a clean gradient. No streaks, no purple or brown mud.
Judge it on the top 20 percent of the 1600x900 frame."

## File ownership

You own `src/world.mjs` only. Do not touch `src/data.mjs`, `src/simday.mjs`, `src/camera.mjs`,
`src/dream.mjs`, `src/main.mjs`, `src/agents.mjs`, `index.html` (assembled), or any capture. If a
contract rule changes, edit `src/CONTRACT.md` with the smallest accurate wording.

## Measured baseline (mine, hour 17, 1600x900, district tier)

`python .cycles/top25.py screenshots/iter-25-day.png` on the crop rows 0..180:

    luma 0.692  sat 0.103  edge 0.86  warm 1.9  purple 0.00  haze 89.5
    streaks 0.96%  mud 29.29%
    rows edge    0: 0.07   30: 0.29   60: 0.27   90: 0.95   120: 1.62   150: 1.94

Read it as: the sky is clean (streaks 0.07), the haze layer is pale (sat 0.103) and the last two
row slices carry the streaked lines (edge 1.62 and 1.94 at rows 120..180), while 29 percent of the
crop is low-saturation warm brown (the dirty band).

## Targets for this cycle, on the same probe and the same camera state

- `streaks` under 0.15 percent, and no single 30-row slice above 0.9 edge while the hills are
  still visible as soft bands (a silhouette is a luma step, not an edge field).
- `mud` under 8 percent; `purple` stays 0.00.
- `warm` in the crop at least 8 percent: the haze has to read warm, not grey.
- `haze` stays above 75 percent (soft air still dominates the crop).
- No individual tree instance anywhere beyond the second ridge. `world.mjs` already sets
  `farPlanted = 0`; find what still draws line-like marks out there (the far belt relief, ridge
  geometry, terrace/retaining geometry, scrub quads, any `addTree`/`addLocalQuad`/`addFacadeQuad`
  path that reaches past the second ridge) and remove it at the source.
- Hills: warm soft silhouettes. Two or three luma steps, cleaner haze key, no per-vertex noise
  beyond the second ridge, hue kept in the cream to warm apricot band (no brown, no violet).
- Sky: keep the current clean gradient (rows 0..60 already read 0.07 to 0.29 edge); do not regress
  it.
- Draw calls must not increase, triangles must fall or stay equal, no per-frame allocation, no
  em dash (U+2014) anywhere, asset mode AND the `?noassets=1` fallback both stay coherent.

## Verification you must run and paste

1. `node tools/assemble.mjs`, `node tools/check.mjs`, `node .cycles/gate.mjs` (all must PASS).
2. A served-page capture at the same camera state as the baseline. A static server is already
   running at `http://127.0.0.1:8731/` serving this folder; do NOT start another one on that port.
   Capture with the helpers in `.cycles/c24.js` (`c24Open`, `c24Wait`, `c24Shot`), because
   `tab.screenshot({ path })` silently writes nothing in this harness:
   `await c24Open(8731, 'http://127.0.0.1:8731/index.html?hour=17&cb=c26a', { name: 'c26a' })`,
   then `await c24Shot(tab, 'screenshots/iter-26-day.png')`.
3. `python .cycles/top25.py screenshots/iter-26-day.png` before and after, and also on a
   `?noassets=1` capture of the same state.
4. Same-page A/B for draw calls and triangles where a delta matters (the triangle count of one
   page moves by thousands with the data clock; a cross-load delta is worthless). `c24Stats(tab)`
   reads `window.localtime.stats()`.
5. Confirm the district is unchanged below the top of the frame: `python .cycles/audit19.py judge`
   on your before and after captures, and report lower-third luma, edge and the four corners
   unchanged within 0.3 flat / 0.15 edge.

## What to report back

The exact code changes with the numbers that justified each, the before/after probe output, the
gate output, the state table row for `desktop day` and `desktop noassets` (calls and triangles),
and any change you rejected on its own numbers.
