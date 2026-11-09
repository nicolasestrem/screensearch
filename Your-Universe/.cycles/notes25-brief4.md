# Cycle 28 brief: the ring is not a wall (OWNER-NOTES 2026-11-09 14:00, item 4)

Owner words, binding: "The ring is still a wall. Continuous streets with buildings on both sides
must cross the ring road and continue outside it on at least two sides (south-west toward the
expressway, east toward the rail yard), so the district has no visible edge inside the frame.
Phase 1 of masterpiece-prompt.md is not done until this is true."

## File ownership

`src/world.mjs` only, plus the smallest accurate note in `src/CONTRACT.md` if a documented rule
changes. Do not touch data, camera, dream, agents, `main.mjs` or any capture.

## What exists today (read the source, but here is the map)

`world.fabric` on this build reports exactly what the fabric pass placed:

    lots 76 of a cap of 76, offStreet 0,
    streets: ring 24, westLower 15, hillside 13, east 13, west 11,
    beltDepth 32, band 36, cell 2, maskCells 24387

So the ring's outer frontage takes 24 lots, and each spur takes 11 to 15 and then stops: the row
ends while the carriageway keeps going, which is the "wall with a road through it" the owner sees.
The lot cap is the binding constraint, not the geometry budget.

- `SPUR_PTS` holds five arterials: index 0 west (z 42 to -4, leaving the left edge low), 1 east
  (96,10 toward 248,-46), 2 north (the hillside avenue), 3 westLower (parallel, forty metres down
  the slope), 4 hillside (leaves the ring's north-west arc and climbs west).
- The expressway is `EX_PTS` on a fixed deck; the rail yard is along the +X edge; the ring is
  `ringPath` with outer frontage fill; `fillPath(path, SPUR_FRONT, 3)` is the one call that lays a
  frontage row on both sides of a path.


## What "no visible edge inside the frame" means, measurably

Establish it before you change anything, with numbers you report:
1. For each spur that crosses the ring, walk the sampled path from the ring centreline to where it
   leaves the 1600x900 frame at district tier, and count, per 10 m of path, buildings whose
   frontage sits on each side within the pavement depth. A street that "has buildings on both
   sides" means every 10 m bin on the on-screen stretch has at least one building on each side.
   Report the per-bin counts per spur, before and after.
2. Identify the last building of each street row and its distance from the frame edge. Any street
   whose row stops before the frame edge is a wall the owner can see.

## The change

- Make at least two crossings continuous and fully fronted: **south-west toward the expressway**
  and **east toward the rail yard**, as the owner names them. "Continuous" means the carriageway,
  the pavements, the kerbs, the street lamps and the building rows all continue together from
  inside the ring, across the ring's own frontage, and out past the frame edge, with no gap where
  the fabric stops.
- The crossing must read as a junction of two streets, not a road through a gap in a wall: the
  ring's own frontage rows must break cleanly at the mouth, with a corner building on each side,
  and the frontage must resume outside.
- Remove the caps that stop the rows early for those two crossings (raise the lot caps and, if the
  fabric ceiling binds, re-allocate from the ring's oversubscribed arc). Keep the remaining three
  spurs coherent; do not leave any of them obviously truncated inside the frame.

## Budget and rules

- Draw calls must not increase (105 with assets, 55 without). Reuse `fillPath`, the existing
  building instancing, the merged statics and the existing paint/kerb layers: a street is already
  a curve plus `fillPath`, and the whole street furniture set reads the path list.
- Triangles: the day frame is under a 390,000 working line and a 400,000 hard gate with about
  9,000 of headroom; any spend must be shown as a same-page A/B pair, and new building rows are
  the expensive part, so prefer the existing archetypes and instance tints.
- `?noassets=1` stays coherent; phone profile stays under its own caps and must not inherit the
  desktop frontage ceiling.
- No per-frame allocation, no em dash (U+2014), nothing outside this folder, no new dependencies.
- Do not regress the corners: worst flat at or under 49.5, worst edge at or above 6.05 on the four
  street-tier rotations.

## Verification you must run and paste

1. `node tools/assemble.mjs`, `node tools/check.mjs`, `node .cycles/gate.mjs` PASS.
2. The per-bin frontage table above, before and after, per crossing.
3. Captures via `.cycles/c24.js` (`c24Open`, `c24Wait`, `c24Shot`; `tab.screenshot({ path })`
   writes NOTHING here) at 1600x900 hour 17: `iter-28-day.png`, street tier rotations
   `iter-28-street-r0..r3.png` (the four corners), `iter-28-noassets.png`, and a
   `iter-28-pixel6.png` at 412x915 dpr 2.625 touch.
4. `python .cycles/audit19.py judge` on day and on the four rotations: whole-frame luma/flat/edge
   and the corner numbers, before and after. `python .cycles/top25.py` to prove the top band did
   not regress while you were working (streaks under 0.15 percent, mud under 8 percent, purple 0).
5. The state table (`measureStates(8731, {})`) calls and triangles for day, street, night, dream,
   noassets, phone day.

## The measured baseline for this cycle (mine, `.cycles/spurfront.js`, `spurFrontage(8731)`)

For every building instance, the nearest sample on each spur path, attributed to (spur, arc
length, side); then per 10 m bin of the on-screen stretch, buildings within 26 m of the
centreline on each side.

| spur | direction | on-screen bins | buildings L/R | empty bins | one-sided bins | last bin with BOTH sides |
| 0 | west, leaves the left edge low | 17 | 20 / 17 | 7 | 3 | 95 m of 165 |
| 1 | east toward the rail yard | 17 | 5 / 18 | 9 | 7 | **5 m of 162** |
| 2 | north, the hillside avenue | 17 | 0 / 14 | 13 | 4 | never |
| 3 | westLower, parallel down the slope | 21 | 5 / 23 | 12 | 8 | 5 m of 206 |
| 4 | hillside, off the north-west arc | 25 | 10 / 22 | 16 | 7 | 25 m of 247 |

Read it as the owner sees it: the east and westLower streets front the ring and then die, so the
carriageway continues across open ground inside the frame. The owner names south-west toward the
expressway and east toward the rail yard as the two crossings that must be continuous; the table
says east stops at 5 m and westLower at 5 m, so both of the named crossings are the two worst.

Acceptance for this cycle, measured the same way: for the two named crossings every on-screen bin
has at least one building on each side (empty bins 0, one-sided bins 0), and `lastBinWithBothSides`
equals the last on-screen bin. The other three spurs must not be left truncated at 5 m: report
their before/after table and get each of them past at least half its on-screen length with both
sides fronted.
