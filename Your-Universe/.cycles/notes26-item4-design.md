# Cycle 26 item 4 design: two streets that cross the ring

Status: design only. `src/` is owned by another writer; nothing here is applied.

Line numbers are from a `src/world.mjs` read on 2026-11-09 while that file was being edited: it grew
from 11,710 to 11,786 lines during the investigation, so a line below can be off by ten or so near the
bottom of the file. Every anchor is given with its symbol; locate by symbol, not by number.
`src/agents.mjs` was stable at 5,646 lines for the whole read, so its numbers are firm.

Owner text (OWNER-NOTES.md:9): "The ring is still a wall. Continuous streets with buildings on both
sides must cross the ring road and continue outside it on at least two sides (south-west toward the
expressway, east toward the rail yard), so the district has no visible edge inside the frame."

---

## 0. Why the ring reads as a wall: the measured mechanism

The wall is not art direction, it is three numbers:

1. The urban region stops 8.8 m inside the ring centreline: `inRingInterior` is
   `ringSigned(x, z) < -8.8` (`src/world.mjs:1038-1040`), and `walkStreet` keeps a grid street only
   where `inUrban` is true (`src/world.mjs:1084-1133`).
2. The ring's inner kerb is 7.0 m inside the centreline: `RING_HALF + RING_PAVE` = 4.4 + 2.6
   (`src/world.mjs:100-101`).
3. The ring frontage row's facade stands 7.35 m inside: `RING_FRONT = RING_HALF + RING_PAVE + 0.35`
   (`src/world.mjs:122`), placed by `fillPath(ringPath, RING_FRONT, 2)` (`src/world.mjs:1706`), and
   its body is 5.8 to 11.4 m deep (`fillPathRun`, `src/world.mjs:1627-1672`), so it occupies roughly
   7.35 to 17 m inside.

So every interior street stops 1.8 m behind the ring's kerb and **1.45 m inside the ring row's
footprint**. `tryParcel` (`src/world.mjs:1400-1474`) refuses the row's lots where the street's
`B_STREET` cells are, so the row opens a 4.4 m slit and the street's asphalt dies inside it. No
interior street continues outward, and no arterial continues inward: the only outward roads are the
five `SPUR_PTS` curves (`src/world.mjs:614-621`), which start *on* the ring centreline and have no
inside half. Meanwhile the ring's own pavement and kerb are already cut 14.9 m wide at each mouth by
the `B_SPUR` mark disk (`markPath(spurPaths[i], SPUR_EDGE + 0.25, B_SPUR)`, `src/world.mjs:969`, with
`SPUR_EDGE + 0.25` = 7.45 m), and `ringPaveSkip` (`src/world.mjs:4074-4077`) reads it. The result is a
15 m gap in the kerb line with a 4.4 m slit behind it: a gate in a wall, not a street.

The fix is therefore small in metres (1.8 m of asphalt plus a 15 m cut in the row) and large in
reading: it makes two of those slits into streets.

---

## 1. Code map

### 1.1 Ring road

| symbol | file:line | what it is |
| --- | --- | --- |
| `RING_CZ = 26`, `RING_HALF = 4.4`, `RING_PAVE = 2.6` | `src/world.mjs:99-101` | kidney radii and section |
| `RING_FRONT` | `src/world.mjs:122` | 7.35 m inside, the ring parcel front |
| `RING_PTS` | `src/world.mjs:541-546` | 30 hand-placed control points, also the containment polygon; the south-west arc is `[-85,74] [-74,90] [-58,100]`, the east frontage is `[96,40] [97,20] [97,-2]` |
| `ringPoly` | `src/world.mjs:548-560` | the polygon plus the roundabout detour (`DETOUR_SEGS`) |
| `ringSigned(x, z)` | `src/world.mjs:563-584` | signed distance to the centreline, negative inside; one pass gives containment and distance |
| `ringCtl`, `ringWorld`, `ringPath` | `src/world.mjs:586-599`, `735` | closed `CatmullRomCurve3`, sampled at 1.5 m by `samplePath` (`src/world.mjs:711-733`) |
| ring paving | `src/world.mjs:4071-4089` | asphalt `band(ringPath, -RING_HALF, RING_HALF, ...)` (4071-4073), pavements with `ringPaveSkip` (4078-4079), `ringMarkSkip` (4080-4082), `dashes` (4083), two `paintStrip` edge lines (4086-4087), two `kerbStones` (4088-4089) |

`ringMarkSkip` (`src/world.mjs:4080-4082`) skips only the roundabout and `B_AVE`, so the ring's centre
dashes and white edge lines are **painted across every spur mouth today** (5 mouths). That is a
defect this change should fix in passing: add `B_SPUR` to the mask.

### 1.2 Avenue

| symbol | file:line | what it is |
| --- | --- | --- |
| `AVE_HALF 12.95`, `AVE_PAVE 2.6`, `AVE_CAR_IN 7.15`, `AVE_FRONT` | `src/world.mjs:106-122` | boulevard section |
| `AVE_Z0`, `AVE_Z1`, `avenueX(z)`, `aveCtl`, `avenueCurve` | `src/world.mjs:651-663` | S-curve from the roundabout to the hilltop |
| `avePath` | `src/world.mjs:736` | sampled at 1.5 m |
| avenue paving | `src/world.mjs:4139-4178` | asphalt (4142-4143), `avePaveSkip` (4144-4146), pavements (4147-4148), median (4150-4152), `laneMark` (4158-4160), `dashes` (4161), six edge lines (4164-4169), kerbs including the tram bed and median (4171-4178) |
| avenue frontage | `src/world.mjs:1709-1715` | `fillPath(avePath, AVE_FRONT, 1)` on both kerbs |

The avenue is not touched by this item. It is listed because it is the one road whose junction with
the ring is signalised (`junctionRecords`, `src/world.mjs:9636-9656`), which is why the ring's own
crossings already exist and can be reused at the two new mouths.

### 1.3 Spurs (`roads.spurs`)

| symbol | file:line | what it is |
| --- | --- | --- |
| `SPUR_HALF 5.0`, `SPUR_PAVE 2.2`, `SPUR_EDGE 7.2`, `SPUR_FRONT 7.6` | `src/world.mjs:132-135` | arterial section |
| `SPUR_PTS` | `src/world.mjs:614-621` | five open curves; index 1 is east, 3 lower-west, 4 hillside |
| `spurs` | `src/world.mjs:623-630` | one `CatmullRomCurve3` per entry, `lifted(..., LIFT_ROAD)` |
| `spurPaths` | `src/world.mjs:739-740` | `samplePath(spurs[i], 2.0)` |
| `markPath(spurPaths[i], SPUR_EDGE + 0.25, B_SPUR)` | `src/world.mjs:969` | the 7.45 m occupancy mouth that cuts ring pavement and kerb |
| outer fabric region | `src/world.mjs:988-1028` | `OUTER_BELT_DEPTH 32`, `OUTER_BAND 36`, mask disks around the ring and every spur (`markFabricDisk`, ~1005-1013), `inOuterFabric` (1016-1028) |
| spur paving | `src/world.mjs:4093-4108` | `inRingBand` = `abs(ringSigned) < RING_HALF + RING_PAVE` (4094-4096); per path asphalt, 2 pavements, dashes, 2 edge strips, 2 kerb strips, all with that skip |
| wear and joints | `src/world.mjs:4499-4560` | `spurRingSkip` (4499-4501), `wheelTracks(p, 2.5, ...)` and `repairPatches(p, [-2.5, 2.5], 23 + s, ...)`, `paveJoints` both kerbs (`JOINT_STEP`, `src/world.mjs:4429`) |
| night ribbon | `src/world.mjs:10208-10233` | `ribbon(spurPaths[i], 0, SPUR_HALF + SPUR_PAVE * 0.8, 0.7, isTouch ? 8 : 4.5, false, 12)`; `ribbon` has **no skip parameter**, so a ribbon crossed the ring would glow across its asphalt |
| mouth zebra and tactile | `src/world.mjs:4570-4620` | per spur: a zebra just outside the ring band (found by scanning `abs(ringSigned) > RING_HALF + RING_PAVE + 1.0`, then `i + 2`) and a ring crossing either side of `path.xs[0]`, each with `tactileBand` (`src/world.mjs:4466`) |
| junction record | `src/world.mjs:9657-9672` | `{ kind: 'spur', x: path.xs[0], z: path.zs[0], ringS: nearest ring sample }`, not signalised |
| signal heads | `src/world.mjs:9805-9848` | ring-facing pair at `best +- 6` samples around `path.xs[0]` |
| level crossing | `src/world.mjs:~5020-5060` | finds `spurPaths[1]`'s nearest sample to `RAIL_X` and paves the rails under it |
| published roads | `src/world.mjs:11635-11649` | `spurs: spurs`, `spurHalfWidth: SPUR_HALF`, `spurPavementOffset`, `spurFrontage` |

### 1.4 Plots and buildings, inside versus outside

| symbol | file:line | what it is |
| --- | --- | --- |
| `V_LINES`, `H_LINES`, `GRID_X0/X1/Z0/Z1` | `src/world.mjs:174-179` | six north-south and seven east-west interior streets, 28-34 m apart |
| `streetRunsV`, `streetRunsH` | `src/world.mjs:1079-1080` | the surviving runs, `{ coord, t0, t1 }` |
| `walkStreet` | `src/world.mjs:1084-1133` | 2 m walk, `inUrban && diskFree(mx, mz, STREET_EDGE + 0.55, B_ROADS | B_OLD | B_PROP)`, marks `B_STREET` / `B_SIDEWALK`, accumulates `streetFrontage` and `lampSpots` |
| `emitRuns` | `src/world.mjs:4804-4855` | one asphalt quad per run, pavements in 3 m chunks with kerb stones, skipping its own asphalt |
| `inRingInterior`, `inHillTop`, `inUrban` | `src/world.mjs:1038-1048` | the inside region: ring inset 8.8 m, plus the hilltop lens |
| `inRingFrontage` | `src/world.mjs:1054-1056` | `ringSigned < -7.0`, the band a ring lot's body centre must sit in |
| `tryParcel(fx, fz, nx, nz, wide, deep, doorInset, kind)` | `src/world.mjs:1400-1474` | the one placement path: `BUILDING_CAP` guard (1401), region test by kind (1409-1420), `rectScan` footprint test and `B_BUILD` write (1421-1423), parcel record |
| kind meanings | `src/world.mjs:1399` | 0 street, 1 avenue, 2 ring, 3 outer belt or spur frontage |
| `fillLine`, `rowRun`, `rowSlot` | `src/world.mjs:1490-1590` | straight rows on the grid lines; `lineFrontFree` tests `inUrban` 5 m into the block and a 2.5 m `B_BLOCK` strip |
| `fillPath`, `fillPathRun` | `src/world.mjs:1675-1705`, `1627-1672` | abutting parcels along a sampled path; kind 1 tests `inUrban` at +3 m (1695-1697), kind 2 tests `inRingFrontage` (1698-1699) |
| ring inner frontage | `src/world.mjs:1706` | `fillPath(ringPath, RING_FRONT, 2)` |
| grid fills | `src/world.mjs:1720-1729` | `V_ORDER`, `H_ORDER`, vertical rows first "so they own the corners" |
| parcel budget | `src/world.mjs:1246-1248` | `frontageTotal`, `targetBuildings = clamp(round(170 + sumBC * 1.05), 240, 400)`, `parcelW = clamp(frontageTotal * 0.355 / targetBuildings, 4.8, 10)` |
| `fabricLots`, `fabricLimit`, `fabricFull` | `src/world.mjs:1610-1626` | the metered outer-lot counter; only kind 3 is metered |
| `FABRIC_LOT_MAX` | `src/world.mjs:2016` | 26 touch / 76 desktop |
| ring outer frontage | `src/world.mjs:2028` | `fillPath(ringPath, RING_FRONT, 3)` (visible face, 24 lots) |
| spur shares | `src/world.mjs:2032-2046` | `shares` array, `fabricLimit = min(FABRIC_LOT_MAX, fabricLots + (isTouch ? max * 0.45 | 0 : max))`, both flanks per path |
| `BUILDING_CAP` | `src/world.mjs:208` | 540 instances; `CONTACT_CAP` ties to it (287) |
| window faces | `src/world.mjs:6058-6100` | four faces per building; a side is skipped only when a neighbour is within `face.half + 1.4`; kind 3 gets the front face only. Corner end walls at a mouth therefore **do** carry windows |

### 1.5 Kerbs, pavements, crossings

`band` (`src/world.mjs:3945-3960`), `paintStrip` (3963-3975), `kerbStones` (`3976-3995`),
`dashes` (3997-4011), `zebra` (`4013-4040`), `tactileBand` (`4466-4496`), `paveJoints`
(`4430-4445`). Every one takes the same `skip(x, z)` callback and emits one `paving` or `paint`
quad per path segment, so a skip is exactly a triangle saving.

### 1.6 Expressway and rail anchors

| anchor | file:line | value |
| --- | --- | --- |
| `EX_HALF`, `EX_DECK_Y`, `EX_PIER_STEP` | `src/world.mjs:139-143` | 8.6, 11.5, 13.0 |
| `EX_PTS`, `exCtl`, `expresswayCurve` | `src/world.mjs:642-650` | `[-152,46] ... [-98,100] ... [-62,142]`; the tunnel portal is the west end |
| `exPath` | `src/world.mjs:741` | sampled at 2.0 m |
| deck, abutment and piers | `src/world.mjs:5140-5310` | `UNDERSIDE = yB - 0.05` = 10.3; `if (UNDERSIDE - g < 3.4)` builds a solid abutment, else piers every `EX_PIER_STEP` with no road test |
| rail line | `src/world.mjs:127` | `RAIL_X = 117`, `TRACK_OFFS = [-5.25, -1.75, 1.75, 5.25]` |
| `railCtl`, `railCurve`, `railPath` | `src/world.mjs:694-701`, `738` | z -104 to 100 at x 117 plus a 0.4 m sine |

Ground at the south-west crossing: `districtHeight` (`src/world.mjs:392-410`) gives about 0.75 m at
(-107, 90) and `farHeight` returns 0 inside 180 m, so `UNDERSIDE - g` is about 9.5 m: the expressway is
on **piers** there and a street can pass under it with 9 m of clearance. A pier may still stand in the
carriageway; see 3.6.

---

## 2. agents.mjs: how spurs get traffic, and what must not change

`createAgents` builds one table per published spur curve:

- `spurTables` from `roads.spurs`, `makeTable(curve, 120)` (`src/agents.mjs:621-627`).
- Section from `roads.spurHalfWidth` = 5.0: `spurLaneOff[k] = h - 3.27` = 1.73, `spurParkOff = h - 1.0`
  = 4.0, `spurParkOn = h >= 5.0` (`src/agents.mjs:635-646`).
- Pool split: `spurCap = max(4, round(POOL * 0.12))` (`src/agents.mjs:672`), `spurBase` at 682,
  `SPUR_LANES = max(2, spurTables.length * 2)` and `spurLaneLen` (`src/agents.mjs:1987-1997`).
- Slot assignment: `spurTab[i] = i % spurTables.length`, `spurLane[i] = side * spurLaneOff[...]`,
  `spurGrid.laneOf[i] = spurTab[i] * 2 + (side > 0)` (`src/agents.mjs:5430-5438`).
- Motion: `stepSpurCars` (`src/agents.mjs:4121-4214`): target `4 + 26 * congestion` eased at 3.2/s
  (4123-4129), grid insert, `SPUR_SPEED 8.4` (`src/agents.mjs:67`), follow gap, then the ring yield at
  `junctionLimit(spurS[i], dir, 0, spurCrossRingS[sp], tab.len, gap, v)` (4182).
- Kerbside parking: `parkCount < PARK_CAP` (64 touch / 190 desktop, `src/agents.mjs:50`), step 6.9 m,
  both kerbs, `PARK_SKIP_SPUR` 0.34 touch / 0.14 desktop (`src/agents.mjs:57`),
  `src/agents.mjs:1895-1910`.
- The ring yield arc is read from **the spur table's first point**:
  `nearestOnTable(ringTable, tab.pos[0], tab.pos[2])` with `JUNCTION_MEET = 3.0`
  (`src/agents.mjs:141`, `2059-2064`).

Consequences for this item:

- **A new outer street must be a curve in `SPUR_PTS`.** It then gets cars, kerbside parking, the
  mouth zebra, the ring-facing signal pair and the ring yield with no agents change at all. With six
  tables, `spurCap` stays 49 (desktop) and 12 (phone), so each street carries about 8 / 2 cars
  instead of 10 / 2.4. No buffer, mesh or draw call changes: `spurLaneLen` is 12 floats, built once.
- **The curves must keep their first point on the ring centreline.** If a spur were extended inward,
  four call sites would silently break: the junction record (`src/world.mjs:9657-9672`), the signal
  head search (`src/world.mjs:9805-9848`), the ring-crossing zebra search (`src/world.mjs:4570-4620`)
  and the agents yield (`src/agents.mjs:2059-2064`). All of them read `path.xs[0]` or `tab.pos[0]` and
  assume it lies on the ring within 3.0 m. The inner half of each street is therefore a **grid street
  extension, not a spur**: it carries no moving traffic, only the world's own kerbside parked cars
  (`parkAt` over `streetRunsV/H`, `src/world.mjs:8505-8520`).
- A sixth table also means the agents parked pool (`PARK_CAP`) reaches the new street last, because
  the loop walks tables in order and the new entry is index 5. Expect its moving cars but few or no
  agents-parked bays until `PARK_CAP` is re-budgeted; the world's own parked mesh still fills it.

---

## 3. The change plan

Two streets. Both are one continuous axis from inside the grid, through the ring, to outside the
frame. Inside they are the existing grid street, extended; outside they are spur curves. The mouth is
where the two families meet.

### 3.1 The two streets and their attach stations

**East, toward the rail yard ("yard street")**

- Interior axis: `H_LINE 26` (`src/world.mjs:175`), from `V_LINE 55` outward.
- Ring crossing: ring station nearest `(96.5, 26)`, between `RING_PTS` `[96,40]` and `[97,20]`
  (`src/world.mjs:541-546`).
- Outer curve: replace `SPUR_PTS[1]` (`src/world.mjs:616`) with
  `[[96, 26], [112, 18], [134, 2], [158, -14], [204, -30], [248, -46]]`.
  The last three points are the old ones, so every fabric lot beyond x = 158 keeps its station; only
  the first ~40 m re-places. The curve crosses the rail at about (117, 16), and the level-crossing
  block follows `spurPaths[1]` automatically.
- Why index 1 and not a new index: `spurGullies` is indexed by spur (`src/world.mjs:~4525`) and the
  level crossing hard-codes `spurPaths[1]`; appending would leave both pointing at nothing.

**South-west, toward the expressway ("expressway street")**

- Interior axis: `H_LINE 90` (`src/world.mjs:175`), from `V_LINE -87` outward (the run currently ends
  near x -65).
- Ring crossing: ring station nearest `(-74, 90)`, which is literally a `RING_PTS` control point
  (`src/world.mjs:541-546`, the south-west arc `[-85,74] [-74,90] [-58,100]`).
- Outer curve: append `SPUR_PTS[5]` =
  `[[-74, 90], [-98, 90], [-126, 90], [-158, 90], [-190, 90], [-224, 90]]`.
  It leaves the ring due west along the flat z = 90 shelf, passes under the expressway deck at about
  (-107, 90) (9.5 m clearance, piers branch), and leaves the frame at the lower left. It does not
  touch the farm tracks (`TRACK_PTS` leave at z 104 and climb), the terraces (`NEAR_T` rows start at
  z 118, `src/world.mjs:~3300`) or the stone walls, all of which are north of it.
- A due-west line is a skew crossing of the ring (about 55 degrees) on purpose: it is the only line
  that reuses `H_LINE 90`, stays on the shelf, and crosses the expressway where it is on piers.

### 3.2 The 1.8 m: closing the kerb gap

`walkStreet` currently stops at `inUrban` (8.8 m inset). For these two lines only, let the run continue
to the ring's inner kerb, 7.0 m inset:

```js
// src/world.mjs:1084, new fourth argument
function walkStreet(vertical, coord, out, through) {
  ...
  const ok = (inUrban(mx, mz) ||
      (through && ringSigned(mx, mz) < -(RING_HALF + RING_PAVE) && through.holds(mx, mz))) &&
    diskFree(mx, mz, STREET_EDGE + 0.55, B_ROADS | B_OLD | B_PROP);
```

with `through` passed only by the two calls at `src/world.mjs:1132-1133`:

```js
const THROUGH_H = [
  { coord: 26, holds: (x) => x > 80 },    // east, only the east end may extend
  { coord: 90, holds: (x) => x < -60 }    // south-west, only the west end
];
```

`inRingInterior` and `inRingFrontage` are **not** edited: 8.8 m is the interior inset for every other
line, and relaxing it globally would open a mouth at every grid line that reaches the ring.

The extended cells are marked `B_STREET` / `B_SIDEWALK` by `walkStreet` as usual, so
`emitRuns` (`src/world.mjs:4804-4855`) draws the extra asphalt and the last 3 m pavement chunk with
its own kerb stones, in the grid street's own tone and at `LIFT_STREET`. Cost: one longer asphalt quad
plus at most 2 pavement quads and 4 kerb quads per street.

### 3.3 Corner plots at each mouth

Inside the ring the ring frontage row must be cut ~15 m wide, not 4.4 m, or the mouth stays a slit.
Two edits, in this order:

1. **Cut the row.** At each crossing, after `markPath(spurPaths...)` (`src/world.mjs:969`), add
   `markDisk(crossX, crossZ, STREET_EDGE + parcelW, B_STREET)`; with `parcelW` about 5.5 this is an
   8.7 m radius, so the row is refused over about 17 m of its frontage and 2 lots per mouth fall out.
   `parcelW` is defined at `src/world.mjs:1248`, before the marks, so the value is available.
2. **Put one corner lot back on each side.** After the grid fills (`src/world.mjs:1720-1729`) and
   before the farm-track marks, for each street build a short synthetic path along the street axis
   (6 samples, 2 m step, `xs/zs/nx/nz`) from `ringSigned = -9.0` to `-7.0` and call the existing
   filler twice:

```js
pathDirX[i] = 0; pathDirZ[i] = -1;      // south side of the east street
fillPath(approachEast, STREET_SET, 2);
pathDirX[i] = 0; pathDirZ[i] = 1;       // north side
fillPath(approachEast, STREET_SET, 2);
```

   kind 2 is the right kind: its region test is `inRingFrontage` (body centre at least 7.0 m inside,
   `src/world.mjs:1054-1056` and 1698-1699), its `doorInset` is 2.7, and its lots keep all four window
   faces (`src/world.mjs:6058-6100`). The facade lands on the street kerb at `STREET_SET` = 3.32,
   which is exactly the grid rows' setback, so the corner lot continues the row.
   `fillPathRun` spends `parcelRnd` (`src/world.mjs:1547, 1639, 1655`), and no later pass calls it
   (grep `parcelRnd(` and `.rnd(`: only those three lines), so running this **after** the grid fills
   leaves every existing parcel's width, depth and colour untouched. Running it before them would
   re-roll the whole district.
3. Meter it. The corner lots are counted against the same cohort budget as the outer fabric: add a
   `MOUTH_LOTS = isTouch ? 2 : 4` cap and require `sum(shares) + MOUTH_LOTS === FABRIC_LOT_MAX`
   (3.4 and 4.1). This is the "new buildings come out of existing cohort budget constants" rule, and
   it is checkable at build: `blds.length` growth from the two approach fills must not exceed
   `MOUTH_LOTS`.

### 3.4 Outer frontage and the share table

Outside the ring the same street is filled by the existing kind 3 fabric fill, which already walks
both flanks (`src/world.mjs:2036-2046`) and already starts at the ring's outer pavement, so the
near-ring lots are placed first and the budget runs out down the street. No new fill code.

`FABRIC_LOT_MAX` stays 76 desktop / 26 touch. The shares are re-split so the two crossing streets get
frontage and the four west-side streets pay for it. Replace `src/world.mjs:2026` and
`src/world.mjs:2032-2040`:

```js
fabricLimit = Math.min(FABRIC_LOT_MAX, isTouch ? 8 : 24);      // ring outer face, was 9 / 24
const shares = [
  { spur: 3, name: 'westLower', max: 11, touch: 4 },
  { spur: 4, name: 'hillside',  max: 9,  touch: 3 },
  { spur: 1, name: 'east',      max: 13, touch: 4 },
  { spur: 0, name: 'west',      max: 6,  touch: 2 },
  { spur: 5, name: 'southWest', max: 9,  touch: 3 }
];
fabricLimit = Math.min(FABRIC_LOT_MAX, fabricLots + (isTouch ? shares[s].touch : shares[s].max));
```

Desktop: 24 + 11 + 9 + 13 + 6 + 9 = 72, plus `MOUTH_LOTS` 4 = 76.
Touch: 8 + 4 + 3 + 4 + 2 + 3 = 24, plus `MOUTH_LOTS` 2 = 26.

Add `southWest: 0` to `fabricAudit` (`src/world.mjs:2017`) and to the published `fabric.streets`
(`src/world.mjs:~11660`), so the report names the new street. The ring's own outer face keeps 24 lots:
it is the band the owner is looking at, and it does not pay for this item.

### 3.5 Kerbs, paint and crossings at the ring

Already in place, and it needs no new code for the two mouths:

- Ring pavement and kerb break on both faces: `ringPaveSkip` sees `B_SPUR` from the 7.45 m mark disk
  (`src/world.mjs:969`, `4074-4077`, `4088-4089`).
- The arterial's asphalt and pavements stop at `inRingBand` (`src/world.mjs:4094-4107`), so the ring
  paves the junction once.
- A zebra just outside the ring band on each new street, plus the ring's own crossing each side of the
  mouth, each with tactile paving (`src/world.mjs:4570-4620`, `4466`).
- Ring-facing signal heads for each mouth (`src/world.mjs:9805-9848`), green by default because a spur
  mouth record is `signalised: false` (`src/world.mjs:9657-9672`).

Two small fixes belong to this item:

1. `ringMarkSkip` (`src/world.mjs:4080-4082`): include `B_SPUR`, so the ring's centre dashes and both
   white edge lines break at a mouth instead of being painted across it. This also fixes the five
   existing mouths and **saves** about 40 triangles per mouth (10 arc segments x 2 strips x 2 tris).
2. `ribbon` (`src/world.mjs:10208-10225`) has no skip. Give it the same optional `skip` argument as
   `band` and pass `inRingBand` for the spur ribbon call (`src/world.mjs:10231-10233`). Without it a
   new mouth gets a glowing sodium strip across the ring's asphalt at night. (The existing five mouths
   have this defect today; the fix covers them too.)

### 3.6 Expressway pier guard

The piers are laid every 13 m along `exPath` with no road test (`src/world.mjs:5283-5296`). The new
street crosses the deck at about (-107, 90), which is within a pier spacing of a pier station, so a
pier can stand in the carriageway. Add one line to the piers branch:

```js
} else if (diskFree(x0, z0, 2.4, B_SPUR | B_STREET)) {
  pierAcc += exPath.step;
  if (pierAcc >= EX_PIER_STEP) { ...existing pier and cap... }
}
```

A skipped pier leaves a 26 m span, which is plausible for a deck on 13 m piers. The abutment branch
(`UNDERSIDE - g < 3.4`) does not trigger at this station (9.5 m clearance), but the guard should be
applied to both branches if the street's z is ever moved.

### 3.7 Order of edits (each one independently checkable)

1. `SPUR_PTS[1]` realignment and `SPUR_PTS[5]` append (`src/world.mjs:614-621`).
2. `THROUGH_H` + `walkStreet` fourth argument (`src/world.mjs:1084, 1132-1133`).
3. Mouth mark disks (`src/world.mjs:969`).
4. Corner fills after the grid fills (`src/world.mjs:1729`), with `MOUTH_LOTS`.
5. Shares table and ring limit (`src/world.mjs:2026-2046`), `fabricAudit.southWest`.
6. `ringMarkSkip` (`src/world.mjs:4080-4082`) and `ribbon` skip (`10208`, `10231`).
7. Pier guard (`src/world.mjs:5283-5296`).
8. Publish `fabric.mouths` and `fabric.streets.southWest` in the result (`src/world.mjs:~11650`).

Nothing in `src/agents.mjs` changes.

---

## 4. Triangle payment

Measured baseline (cycle 25, assembled tree, idle browser; ITERATIONS.md:2496-2502): desktop day
380,516 / 105 calls, street 381,352 / 105, night 352,012 / 103, dream 296,998 / 91, `?noassets=1`
331,977 / 55, phone day 170,414 / 54, phone night 170,279 / 44. Working line 390,000, hard gate
400,000, phone line 174,000, phone hard gate 180,000, bench peak 389,656 at 112 calls.

Per-segment costs, from the code (a spur path is sampled at 2.0 m, `src/world.mjs:740`):

| writer | desktop tris/segment | phone tris/segment |
| --- | --- | --- |
| asphalt `band` | 2 | 2 |
| 2 pavements | 4 | 4 |
| dashes | 0.4 | 0.4 |
| 2 edge `paintStrip` | 4 | 4 |
| 2 `kerbStones` | 4 | 4 |
| `wheelTracks` (4 quads per 2 segments) | 4 | 4 |
| `repairPatches` (1 quad per 16) | 0.08 | 0.08 |
| `paveJoints` (2 sides, `JOINT_STEP` 1 / 2) | 4 | 2 |
| night `ribbon` (step 4.5 / 8, 4 quads per station) | 3.6 | 2 |
| **total** | **26.1** | **22.5** |

The ledger:

| item | desktop | phone | source |
| --- | --- | --- | --- |
| expressway street, 136 m = 68 segments | +1,775 | +1,530 | 68 x the row above |
| its night ribbon (30 stations) | +240 | +136 | `ribbon`, `src/world.mjs:10231` |
| its mouth: street zebra, 2 ring zebras, tactile | +100 | +100 | `zebra`, `src/world.mjs:4013` |
| yard street, 5 m shorter than today | -130 | -110 | 163 m vs 168 m |
| two grid run extensions (1.8 m, pavements and kerbs) | +24 | +24 | `emitRuns`, `src/world.mjs:4804` |
| ring paint breaks at 7 mouths | -308 | -250 | `ringMarkSkip`, `src/world.mjs:4080` |
| expressway pier removed | -20 | -20 | one shaft plus one cap, 10 tris each |
| 4 corner lots, 4 ring-row lots removed | 0 | 0 | `tryParcel`, `src/world.mjs:1400` |
| **total** | **+1,681** | **+1,410** | |

Predicted: day 382,197; street 383,033; night 353,693; dream 298,679; noassets 333,658; phone day
171,824; phone night 171,689. Calls unchanged in every state: all new geometry lands in the existing
merged `paving`, `paint`, `details` (kerbs and joints are paving; the ribbon is its own merged mesh)
and no mesh is added.

### 4.1 The cohort budget constants, named

| constant | file:line | value | role in this change |
| --- | --- | --- | --- |
| `FABRIC_LOT_MAX` | `src/world.mjs:2016` | 26 touch / 76 desktop | the **total** outer-plus-mouth lot budget. Unchanged. `sum(shares) + MOUTH_LOTS` must equal it (72 + 4, 24 + 2) |
| `fabricLots`, `fabricLimit`, `fabricFull` | `src/world.mjs:1610-1626` | counters | must meter the corner lots too, which is why `MOUTH_LOTS` exists rather than a fifth free fill |
| `BUILDING_CAP` | `src/world.mjs:208` | 540 | hard ceiling; the mouth is net zero (4 in, 4 out), so the count does not approach it. `tryParcel` refuses at 540 (`1401`) |
| `targetBuildings` | `src/world.mjs:1247` | 240..400 | unchanged: the inner extension adds under 20 m of frontage, so `frontageTotal` (1246) and `parcelW` (1248) stay as they are. This is the lever if a later cycle wants more inner mouth frontage: feed the new length into `frontageTotal`, never raise `BUILDING_CAP` |
| `parcelW` | `src/world.mjs:1248` | 4.8..10 | also the mouth cut radius (`STREET_EDGE + parcelW`), so cut width and lot width cannot drift apart |
| `MODEL_LOT_MAX` | `src/world.mjs:63` | 22 touch / 42 desktop | unchanged; a re-rolled model lot can move between asset and fallback mode, so both are measured (section 6) |
| `RING_BAND_MAX` | `src/world.mjs:255` | 120 | caps the facade band pass that kind 2 corner lots draw (`src/world.mjs:~7330`). 4 corner lots add at most 4 x floors bands and the pool has spare; do not raise it |
| `DETAIL_BUDGET`, `DETAIL_CAP`, `FRAME_CAP`, `WINDOW_CAP` | `250`, `218`, `217`, `209` | 6,986 / 9,000 / 11,200 / 7,800 desktop | unchanged; new buildings take detail and window slots through the existing per-building passes only |
| `STREET_CAP` | `src/world.mjs:7804` | 0 touch / 252 asset / 168 fallback | unchanged, and the two new streets take no kerbside furniture, so the pool is not spent |
| `CONTACT_CAP` | `src/world.mjs:287` | 540 | unchanged |
| `TREE_CAP`, `LAMP_CAP` | `262`, `283` | 2,600 / 280 desktop | unchanged: the new corridor is `B_SPUR`/`B_STREET`, so trees skip it, and spurs carry no lamps today (`addLamp` is called for the ring, the avenue and `lampSpots`, `src/world.mjs:9390-9403`) |
| `MOUTH_LOTS` | new, next to `FABRIC_LOT_MAX` | 2 touch / 4 desktop | the only new constant, and it is derived: `FABRIC_LOT_MAX - sum(shares)` |

---

## 5. Verification: probes and acceptance numbers

Run in this order, on the assembled tree, idle browser, hour 17 unless stated.

1. `node .cycles/gate.mjs`. Acceptance: `GATE RESULT: PASS` with assemble 23/23, check 38/38,
   check-dom 100/100, check-assets, stamp-sw, check-sample, smoke-data 120/120, check-pixels
   (`.cycles/gate.mjs:10-20`).
2. `c24States(8760, { hour: 17, street: true, shots: "iter-26" })` (`.cycles/c24.js:150-215`).
   Acceptance, desktop: day <= 384,200, street <= 385,000, night <= 355,400, dream <= 300,400,
   noassets <= 335,400; phone day <= 173,100, phone night <= 173,000. Calls exactly 105, 105, 103, 91,
   55 and phone 54: a single new call fails the item.
   `c24Meshes` (`.cycles/c24.js:70-93`) on the street state: `buildings.inst` must fall by 0 to 8 from
   the before value (4 corner lots in, 4 ring-row lots out, with a tolerance for the corner lot being
   refused), `details.cursor` <= 6,986 with `details.dropped` 0, roof placement violations 0.
3. `c24InstanceCost(8760, { mesh: "buildings", jump: 112 })` on the before and after tree
   (`.cycles/c24.js:96-135`): `perInstance` must agree within 0.55 tris per instance, which is the
   proof that no per-building cost changed (10 tris box drawn twice plus its panes and trims).
4. `measureStates(8760, { shots: "iter-26" })` and `benchState(8780, {})`
   (`.cycles/measure.js:101-179`, `182-250`). Acceptance: the same state table within the numbers
   above, bench peak <= 400,000 at <= 118 calls, desktop p1Low >= 100, no frame over 33 ms after
   warmup, phone4x p1Low >= 60 (ITERATIONS.md:1988-1992 and OWNER-NOTES.md:11).
5. `python .cycles/audit19.py judge screenshots/iter-26-before.png screenshots/iter-26-day.png`
   (`audit19.py:157-170`, metrics at 58-101). Acceptance, comparing the two lines the probe prints:
   - `left third`: edge density up >= 0.15, flat share not higher (this is the expressway street);
   - `right third`: edge density up >= 0.15 (the yard street);
   - `bottom third`: flat share down >= 0.30;
   - `whole`: flat share not higher, i.e. the frame did not get emptier;
   - the tenth-band table: at least one of bands 7 to 9 up >= 0.20 edge.
   Also run `python .cycles/audit19.py judge screenshots/iter-26-street-r0.png` on the street-tier
   rotation that frames the ring, with the same deltas.
6. Structural acceptance (numbers the probes cannot invent): publish `fabric.mouths` as
   `[{ x, z, ringS, cut, gap }]` and require two entries, each `cut >= 14` m (the ring frontage the
   row loses) and each street's asphalt `t1` within 0.4 m of
   `RING_HALF + RING_PAVE` inset on its own inner kerb. `world.roads.spurs.length === 6`,
   `world.fabric.lots <= 76`, `world.fabric.cap === 76`, `world.fabric.offStreet` unchanged.

If the day frame lands above 384,200, the first cuts to take are, in order: drop the expressway
street's night ribbon on touch (about 136 phone tris), then coarsen `samplePath(spurs[i], 2.0)` to
2.5 m on touch only (`src/world.mjs:740`, about 1,900 phone tris across all six spurs). Do not raise a
cap.

---

## 6. Risks

### 6.1 `?noassets=1`

`assets` is null when the loader is skipped (`src/world.mjs:58`), so `STREET_CAP` falls to 168
(`src/world.mjs:7804`), `MODEL_LOT_MAX` is unused, and every lot is a procedural box. Two effects:

- The corner lots are kind 2 and procedural in both modes, so they cost the fallback the same as the
  asset build; the ring-row lots they replace were also procedural boxes in both modes unless one of
  them had been picked as a model lot. `MODEL_LOT_MAX` selects inner lots with `b.kind !== 3` and
  width >= 8.6 (`src/world.mjs:2336, 2407`), so a mouth cut can move one model lot. Measure
  `buildings.inst` on **both** the asset and the `?noassets=1` state; the fallback number is the
  honest one, because it has no pack mesh to hide a box.
- Fallback headroom is large (331,977 against 400,000), but its calls must stay 55. All new geometry
  is in `paving` / `paint` / `details`, so it will.

### 6.2 Phone profile and `mobile`

`isTouch` (`src/world.mjs:52`) drives `NARROW_VIEW`, `FABRIC_LOT_MAX` 26, the ring limit 9 (proposed
8), `JOINT_STEP` 2 (`src/world.mjs:4429`), `ribStep` 9 (`10227`), `STREET_CAP` 0 and `nightCullTouch`
(`src/world.mjs:342-350`). Paving is **not** culled on touch: the new asphalt and pavements are drawn
on the phone in day, night and dream. At 22.5 tris per segment the item adds about 1,410 phone
triangles, taking phone night from 170,279 to about 171,689: 2,311 under the 174,000 line and 8,311
under the 180,000 gate. The phone's fabric lots are redistributed, not increased (24 shares + 2 mouth
= 26). If a later change adds to this corridor, the phone pays first.

`mobile` in agents (`src/agents.mjs:19`) halves the pools and caps them at 100 cars, 200 pedestrians,
64 parked bays and 6 vans (`src/agents.mjs:20-57`). A sixth spur table gives `spurCap` 12 phone slots,
2 cars per street, and the agents parked loop reaches the new street last, so on a phone the new
street will read as a quiet street with the world's own parked cars and almost no moving traffic.
That is acceptable and matches the existing phone spurs; it needs no `mobile` code change, but do not
"fix" it by raising `spurCap` or `PARK_CAP`: both are measured pool constants.

### 6.3 Randomness and the existing city

`parcelRnd` is consumed only in `rowSlot` and `fillPathRun` (`src/world.mjs:1547, 1639, 1655`) and is
never called after the fills, so the corner fills run **after** the grid fills and leave every
existing parcel's depth, width and colour untouched. Any new lot still shifts nothing for existing
buildings, because heights and colours are keyed on `bi` (`src/world.mjs:2185-2187`) and the new lots
are appended. Do not move the corner fill earlier: that re-rolls the whole district.

### 6.4 East arterial realignment

`SPUR_PTS[1]` moves from `[96,10]` to `[96,26]`, so 13 existing outer lots re-place along the first
40 m and the level crossing moves from about (117, 4) to about (117, 16). The last three control
points are unchanged, so lots beyond x = 158 keep their ground. If the owner rejects the re-placed
frontage, the fallback is a new interior axis at z = 10 (mid-block, not a grid line), which costs 4 to
6 extra inner lots and needs a share to pay for them; it is not the recommended path because a street
that starts mid-block does not read as part of the grid.

### 6.5 Expressway pier

Without the guard, a 3.4 x 2.6 m pier can stand in the new carriageway at about (-107, 90). With it,
the deck spans 26 m there. Verify in the frame and in the expressway audit geometry, not by eye alone.

### 6.6 Line drift

`src/world.mjs` grew by 76 lines during this read. The other writer owns the file; re-locate every
anchor by symbol before applying, and keep the order in 3.7 so each step can be gated alone.

---

## 7. What the owner should see

At district tier, hour 17, the south-west mouth: the ring's kerb line breaks for 15 m, a 6.4 m street
enters it between two windowed corner walls, runs west under the elevated deck on piers, and leaves
the frame at the lower left with buildings on both flanks for its first 40 m. The east mouth: the same
break at the rail-yard end, the street running through the level crossing past the yard and out to
the right edge. Two gaps in the wall, each reading as a street rather than a slit, and no new draw
call, no new mesh, no new cap.
