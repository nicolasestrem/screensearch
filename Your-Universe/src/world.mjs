// Localtime: the static hillside district.
// One top-level identifier: buildWorld. Every helper, table and class lives in this closure,
// because all fragments are concatenated into one module scope by tools/assemble.mjs.
//
// Art direction: a contemporary Mediterranean hillside city on a real slope, seen from above
// at six in the evening, warm low sun from the right. The arterial ring is an irregular
// kidney, not a circle: a wide lower bowl, one long straight commercial frontage down the
// east side, and a lobe that climbs the west flank toward the crest. Open arterials leave it
// on three sides and run off the frame, so the city continues outward instead of stopping at
// a wall. The avenue is a tram-lined boulevard with a planted median, climbing past the
// market square to a small pale stone village on the very top, a stone church and a bell
// tower: a crown, not a town. Office towers stand on the upper plateau, curtain walls with a
// vertical sun gradient; the outer slopes carry flat-roofed apartment blocks with balconies,
// a handful of villas with pools, a supermarket, a petrol station, a school and a sports hall;
// warehouses with loading bays line the rail yard, and an elevated expressway curves across
// the lower left on a stone wall, piers and a tunnel portal that swallows it at the west edge.
//
// The street grid, the block parcels, the outer blocks, the civic plots and the dream corridor
// are all decided on a one metre occupancy grid, so nothing is ever placed on a road, a
// pavement, the corridor or inside another building.
//
// Shading: every diffuse surface is MeshStandardMaterial with a plausible roughness, ambient
// occlusion is baked into instance colours plus one instanced ground-contact blob per
// building, the terrain carries its own vertex-colour patchwork (dry grass, olive clumps,
// field boundaries, dirt tracks, terraces), and the sky is a gradient dome with a sun glow and
// a haze band. Emissive glass (window quads, sign faces, lamp pools) stays unlit on purpose.
// The narrow profile, where the blob is not built, carries the same contact darkening as a
// baked two-step skirt in the detail pool instead.
//
// Every box in the world comes from one shared unit box with an open bottom: five faces, ten
// triangles. The camera never looks up, so the missing face is never visible, and the shadow
// pass loses only the detached patch of an elevated horizontal face at this low sun.
//
// Runtime knobs, all allocation free: world.lights.setShadowMapSize(n) takes the directional
// shadow map from off (0) up through 512 to 4096 without leaking render targets,
// world.setDetailLevel(0..2) trims the optional detail instances out of a pre-baked rank order
// so only the instance count changes, and world.roads plus world.crossings carry the lane and
// crossing geometry the agents drive and walk on.
//
// Draw calls of the static world: terrain, sky, paving, statics, buildings, details, windows,
// trees, houses, parked, lamps, lamp-glow, parasols, signs, cranes, towers, expressway,
// signal heads, signal lenses, contact blobs, window frame rings, shopfront glazing. The touch
// profile leaves window frame rings and shopfront glazing out at build time, because a 412 px
// frame cannot resolve a 12 cm trim ring or a shop pane at district tier, and it skips the
// contact blobs, whose darkening lives in the detail skirt instead; see NARROW_VIEW below.
// Shadow casters: buildings, details, towers, trees, houses, parked, statics,
// expressway, cranes.

export function buildWorld(THREE, model, opts) {
  const settings = opts || {};
  const shadowsOn = settings.shadows !== false;
  const isTouch = !!settings.isTouch;
  const richDetail = !isTouch;

  // The pack cache is optional and shared across installs. When the loader failed, timed out or
  // ?noassets=1 skipped it, this is null and every substitution below falls back to the exact
  // procedural build. Nothing is loaded here: world only consumes models main already resolved.
  const assets = settings.assets && typeof settings.assets.pick === 'function' ? settings.assets : null;

  // Substitution caps. They are budgets, not targets: each ladder step is gated on the measured
  // frame, and the numbers were settled against the 400k / 180k triangle gates with the 55 / 41
  // call baselines. A model mesh that ends with zero instances is never added to the group.
  const MODEL_LOT_MAX = isTouch ? 22 : 42;
  // Model towers are parked this cycle. Their baked facade carries no lit-window grid, so at
  // night they lose both the art direction ("towers with lit floors") and the data-to-motion
  // rule that lights a window from the last hour of activity. The path below stays intact and
  // documented for a later cycle that pairs it with a procedural window overlay; setting this
  // cap above zero is all it takes to re-enable it. Procedural towers are the only ones built.
  const MODEL_TOWER_MAX = 0;
  const MODEL_TREE_MAX = isTouch ? 46 : 150;
  const MODEL_CYPRESS_MAX = isTouch ? 6 : 14;
  const MODEL_PALM_MAX = isTouch ? 4 : 10;
  const MODEL_LAMP_MAX = isTouch ? 0 : 48;
  const MODEL_PARASOL_MAX = isTouch ? 0 : 15;
  const MODEL_PROP_MAX = isTouch ? 0 : 26;
  const MODEL_VILLA_MAX = isTouch ? 0 : 4;

  // The reduced mesh set is the phone's, and it follows the one capability decision every
  // fragment now shares: `opts.isTouch`, which main resolves once as (pointer: coarse) or
  // navigator.maxTouchPoints > 0 and hands to buildWorld and to agents. It is deliberately not
  // a viewport width test any more. World used to read window.innerWidth here, so a 700 px
  // desktop window got the desktop render profile and the desktop population caps while losing
  // the window frames, shop glazing, ground contacts and parasols: a mesh set no device asks
  // for. Four meshes are dropped on a touch profile because a 412 px frame cannot resolve a
  // 12 cm trim ring, a shop pane or a contact blob at district tier; every other profile keeps
  // them.
  const NARROW_VIEW = isTouch;

  // ------------------------------------------------------------------ layout constants
  const SPAN = 120;
  const HILL = 26;                 // ring road (+Z) to hilltop (-Z)
  const Z_LOW = 112;
  const Z_TOP = -116;
  const GROUND_EXTENT = 1400;      // the ground runs more than 5x the district span each way

  // The ring is an irregular kidney, not a circle: a wide lower bowl, one long straight
  // commercial frontage down the east side, and a lobe that climbs the west flank toward the
  // crest. Radii about (0, RING_CZ) run from 79 to 113 metres, so no quadrant reads as round.
  const RING_CZ = 26;
  const RING_HALF = 4.4;           // asphalt half width; ring cars run at +-1.5
  const RING_PAVE = 2.6;
  const ROUND_LANE = 12;
  const ROUND_R = ROUND_LANE + RING_HALF;
  const ROUND_CZ = 101;            // roundabout centre, on the bottom edge of the kidney
  const ISLAND_R = 5.9;
  const TRAM_LOOP_R = 8.3;

  // The avenue is a tram-lined boulevard. Half offsets from the avenue axis:
  // 0..3.4 planted kerbed median, 3.8..7.0 grass tram bed, 7.15..12.95 carriageway,
  // 13.05..15.65 pavement. The tram curve stays at +-TRAM_OFF, so the median sits between the
  // two tracks and the carriageways sit outside them.
  const TRAM_OFF = 5.4;            // tram track axis from the avenue axis
  const TRAM_BED = 1.6;
  const AVE_MEDIAN = 3.4;
  const AVE_CAR_IN = 7.15;         // carriageway inner edge, outside the tram bed
  const AVE_HALF = 12.95;          // outer kerb of the carriageway
  const AVE_PAVE = 2.6;
  const AVE_PAVE_IN = AVE_HALF;
  const AVE_PAVE_OFF = AVE_HALF + AVE_PAVE * 0.5;   // walk-line centre, exposed to agents
  const AVE_LANE_CENTRE = (AVE_CAR_IN + AVE_HALF) * 0.5;
  const AVE_FRONT = AVE_HALF + AVE_PAVE + 0.3;      // avenue parcel front, on the kerb
  const RING_FRONT = RING_HALF + RING_PAVE + 0.35;  // ring parcel front, on the kerb
  const STREET_HALF = 2.2;
  const STREET_PAVE = 1.0;
  const STREET_EDGE = STREET_HALF + STREET_PAVE;
  const STREET_SET = STREET_EDGE + 0.12;   // parcel front, on the kerb
  const RAIL_X = 117;
  const TRACK_OFFS = [-5.25, -1.75, 1.75, 5.25];

  // Open arterials that leave the kidney and run off the frame: low west, east past the yard,
  // north over the crest. Same section as the ring, a touch wider, with their own pavements.
  const SPUR_HALF = 5.0;
  const SPUR_PAVE = 2.2;
  const SPUR_EDGE = SPUR_HALF + SPUR_PAVE;
  const SPUR_FRONT = SPUR_EDGE + 0.4;

  // Elevated expressway on the lower left. A constant deck level means the structure is either
  // on its stone abutment, on piers, or inside the tunnel portal at the west edge.
  const EX_HALF = 8.6;
  const EX_LANE = 7.0;             // one carriageway, inside the guard rails
  const EX_MEDIAN = 0.7;           // painted median half width
  const EX_DECK_Y = 11.5;
  const EX_PIER_STEP = 13.0;

  const PLAZA_CX = 36;
  const PLAZA_CZ = 12;
  const PLAZA_W = 24;
  const PLAZA_D = 17;

  const LIFT_STREET = 0.13;
  const LIFT_ROAD = 0.17;
  const LIFT_BED = 0.19;
  const LIFT_STREET_PAVE = 0.2;
  const LIFT_MARK = 0.22;
  const LIFT_PAVE = 0.25;
  const LIFT_KERB = LIFT_PAVE + 0.055;   // kerb stones stand a step above their pavement
  const LIFT_PAINT = LIFT_ROAD + 0.058;  // road paint, its own plane above the asphalt
  const LIFT_MEDIAN = 0.3;
  const LIFT_PLAZA = 0.27;
  const LIFT_RAIL = 0.31;
  // Farm tracks ride a little above the ground like a pavement, high enough to clear the
  // curvature the terrain triangles leave between their 3.5 metre vertices.
  const LIFT_TRACK = 0.215;

  // Street grid: six streets along the slope (constant X) and seven across it (constant Z).
  // Spacing is 28 to 34 metres everywhere and the grid stays continuous, so blocks always abut.
  const V_LINES = [-87, -55, -23, 23, 55, 87];
  const H_LINES = [-100, -68, -36, -8, 26, 58, 90];
  const GRID_Z0 = -114;
  const GRID_Z1 = 102;
  const GRID_X0 = -98;
  const GRID_X1 = 98;
  // Cell boundaries for block paving: the grid plus the outer clip edges.
  const CELL_X = [-98, -87, -55, -23, 23, 55, 87, 98];
  const CELL_Z = [-114, -100, -68, -36, -8, 26, 58, 90, 102];

  // The stone crown on the summit: the one part of the hilltop the contemporary city never
  // enters. Its ground is claimed before the street grid and before any parcel, because the
  // village is not an overlay on a block, it is the block. Three short alleys run along the
  // slope with a terrace of stone houses pressed to either side of each; the band starts a
  // lane's width east of the church wall and stops at the avenue's west pavement. The lanes
  // themselves are painted, not flagged: the old test counted the alley a house fronts as an
  // obstacle, so all but one house of the village was rejected.
  const OLD_X0 = -34.0;
  const OLD_X1 = -15.2;
  const OLD_Z0 = -115.2;
  const OLD_Z1 = -91.6;
  const OLD_LANES = [-109.6, -103.4, -97.2];
  const OLD_ROW = 3.0;              // house centre to the alley centre it fronts
  const OLD_ALLEY = 1.0;            // the alley's walkable half width, paving plus gutter
  // The village block's wall lines. The west line stands one lane off the church wall, the
  // east line is the back of the terrace that fronts the avenue, and its front wall sits on the
  // avenue kerb. The lanes are painted between the west line and the avenue terrace.
  const OLD_ROW_X0 = -35.35;
  const OLD_ROW_X1 = -18.9;
  const OLD_CROSS_X1 = -15.4;
  const OLD_CROSS_D = 3.5;          // one depth for the avenue terrace, so both lines are straight
  const OLD_PAVE_X0 = -34.2;        // the lanes stop clear of the churchyard cypresses
  const OLD_PAVE_X1 = OLD_ROW_X1 - 0.2;
  const OLD_Z_END0 = -114.55;       // outer wall lines of the two end terraces
  const OLD_Z_END1 = -92.25;

  const quartersModel = (model && model.quarters) || [];
  const QUARTER_CAP = 9;

  // Instance caps. Touch halves everything that is decoration; the block count itself is set
  // by the street frontage so it stays honest on every device.
  const BUILDING_CAP = 540;
  const WINDOW_CAP = isTouch ? 3600 : 7800;
  const DETAIL_CAP = isTouch ? 5413 : 9000;   // plinths, shopfronts, awnings, street furniture
  // The last few hundred flourishes are worth more than they cost in a frame that has to hold
  // under four hundred thousand triangles with a dream on screen: the flush keeps the highest
  // ranked props and drops the tail of the lowest rank instead of drawing all of them. The raised
  // budget buys the rooftop clutter its boxes: 540 of them, 10800 triangles on the desktop day
  // frame (a ten triangle box drawn twice, once into the shadow map and once into the colour
  // pass), and nothing at all at night, where the level drops to 1 and rank 2 is never drawn.
  // The market raised both caps by 213, its own count of rank 0 and rank 1 instances, so the
  // rank 2 tail it displaced stays exactly as long and nothing on the roofs goes dark.
  const DETAIL_BUDGET = isTouch ? 5413 : 6653;
  const GLASS_CAP = isTouch ? 900 : 1600;     // shop windows and shelter panels, unlit glass
  const SHOP_MAX = isTouch ? 70 : 150;        // shopfront storeys, so the detail budget holds
  // The cap pays for the hillside planting, not for a bigger district: every tree costs 24
  // triangles (a sixteen triangle crown over a four sided trunk), drawn twice on a desktop
  // frame because the canopies cast into the shadow map. The near slopes, the terraces and the
  // farm tracks are planted before the distance is read, so the foreground always gets its rows.
  const TREE_CAP = isTouch ? 1280 : 2600;
  // The air over the far country. Two warm stops, not one: the near crests keep a cream haze
  // close to their own dry grass, and the outer shore steps up into a pale blue air so the
  // silhouettes layer by distance. Neither is violet: a lilac haze under a warm sky was the
  // purple mud the top of the frame opened with, and it is gone.
  const AIR_HUE = new THREE.Color(0xe6d3b0);
  const AIR_BLUE = new THREE.Color(0xc8cfd6);
  const LAMP_CAP = isTouch ? 140 : 280;
  const HOUSE_CAP = isTouch ? 52 : 88;       // the stone crown on the summit only
  const CAR_CAP = isTouch ? 62 : 155;
  const PARASOL_CAP = isTouch ? 12 : 26;
  const CONTACT_CAP = BUILDING_CAP;          // one ground-contact blob per building
  const CROSS_CAP = 12;

  // Quarter sites by rank: the anchor points of the Voronoi split of the urban fabric. The two
  // biggest quarters sit on the hilltop, where their towers read best.
  const SITES = [
    [-44, -84], [46, -84], [-40, 8], [60, -8], [-36, 62],
    [42, 62], [-76, 34], [79, 34], [-90, -66]
  ];

  // Shared Mediterranean facade palette, weighted by repetition so the frame carries five
  // separable families instead of one cream: ochre and salmon dominate, terracotta and pale pink
  // sit beside them, and roughly forty percent of the blocks land in the saturated warm half.
  // Six of fifteen entries are the saturated half: 40 percent by construction, not by luck.
  //
  // Chroma, measured. The 1600x900 day frame put the middle tenths at saturation 0.32 to 0.37
  // against the 0.40 the owner asked for, and a hide test showed why: buildings cover about
  // 54 percent of those bands and their own mean saturation was 0.29, so the city was carrying
  // the frame's colour down toward the asphalt. Each entry keeps its hue and its value and only
  // its chroma moves, by a factor chosen per family: the warm half is already chromatic and
  // gains 15 to 30 percent, while the three neutral families were nearly grey and are lifted
  // toward the same perceived chroma (cream 0.222 to 0.400, pink 0.290 to 0.465, wash 0.149 to
  // 0.298 on the HSV scale). Nothing is multiplied channel-wise, so no wall clips and the five
  // families stay five: the mix is the mix, it is simply coloured.
  const FACADES = [
    0xc9863c, 0xc9863c, 0xc9863c,           // ochre, the dominant wall of the town
    0xc86345, 0xc86345,                     // salmon
    0xa84e35,                               // terracotta
    0xd8bd82, 0xd8bd82, 0xd8bd82,           // deep cream, now a warm sand
    0xd98c74, 0xd98c74,                     // pale pink render
    0x769aa8, 0x769aa8, 0x769aa8, 0x769aa8  // faded blue-grey wash
  ];
  const GLASS = [0x7c9bb0, 0x8aa8bb, 0x6f8ea6];
  // The occasional hashed block in stone takes the same lift, so a stone corner is not the one
  // neutral wall left in a coloured street.
  const STONE = [0xbfa982, 0xcbb992, 0xa99472, 0xc6ab77, 0x9f917c];

  // Facade colour is a hash of the block position, not of its build order: neighbouring blocks
  // differ, a rebuild lays out the same town, and no Math.random enters the palette.
  function facadeAt(x, z) {
    const h = hash2(Math.round(x * 2.5) + 4096, Math.round(z * 2.5) + 4096);
    const i = (h * FACADES.length) | 0;
    return FACADES[i < 0 ? 0 : i >= FACADES.length ? FACADES.length - 1 : i];
  }

  // Tower curtain wall: the shade side is a clear blue-grey mass, the sun side catches a warm
  // reflection near the crown, with a darker spandrel band at every floor line and a recessed
  // core seam. The two ends are far enough apart in hue that the towers read as the cool family
  // rather than as pale stone.
  const TOWER_FLOOR_H = 3.4;
  const TOWER_SUN = new THREE.Color(0xd9a468);
  const TOWER_SHADE = new THREE.Color(0x4a6070);
  const TOWER_SPANDREL = new THREE.Color(0x35434e);
  const TOWER_MULLION = new THREE.Color(0x9fb0ba);
  const TOWER_PLANT = new THREE.Color(0xc9c6bd);

  // The one sun direction of the whole scene, shared by the light, the sky glow and the tower
  // curtain-wall gradient.
  const SUN_DIR = new THREE.Vector3(0.84, 0.42, 0.34).normalize();

  const group = new THREE.Group();
  group.name = 'district';

  // Night cull: meshes whose night frame contribution is below the tolerance the cycle gate
  // measures are switched off by lights.setTimeOfDay and back on at dawn. The measured share of
  // the 1600x900 night frame each one covers: window frame rings 0.28 percent, ground contact
  // blobs 0.04, crane masts 0.06, the stone crown 0.005. Their triangle costs are 11576, 2454,
  // 2366 and 12. The switch is a boolean write, so there is no allocation on the night edge.
  const nightCull = [];
  // The touch frame has 412 device pixels across, so the far field of the district is below one
  // pixel there: the water, the elevated deck, the tower silhouettes, the lettered surfaces and
  // the sky dome. Measured on the 412x915 night frame by hiding each one and differencing the
  // captured frame: the dome moves 0.000 percent of pixels (largest channel change 1), the bay
  // 0.145 mean with no pixel past 24 levels, the lettering 0.067 mean, the towers 0.214 and the
  // deck 0.240, and no band moves by more than 0.1. Together they are 5638 triangles, which is
  // what buys the touch night frame back under its ceiling. Desktop keeps every one of them.
  const nightCullTouch = [];

  const dummy = new THREE.Object3D();
  dummy.rotation.order = 'YXZ';
  const colorScratch = new THREE.Color();
  const colorScratch2 = new THREE.Color();
  const v3a = new THREE.Vector3();
  const v3b = new THREE.Vector3();
  const m4 = new THREE.Matrix4();
  const m3 = new THREE.Matrix3();
  const quat = new THREE.Quaternion();
  const mPos = new THREE.Vector3();
  const euler = new THREE.Euler(0, 0, 0, 'YXZ');
  const scaleV = new THREE.Vector3();

  function clamp(v, a, b) { return v < a ? a : v > b ? b : v; }
  function smooth(a, b, x) {
    const t = clamp((x - a) / (b - a), 0, 1);
    return t * t * (3 - 2 * t);
  }
  function mulberry(seed) {
    let a = seed >>> 0;
    return function () {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  function hash2(i, j) {
    let h = (i * 374761393 + j * 668265263) | 0;
    h = Math.imul(h ^ (h >>> 13), 1274126177);
    return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
  }

  // ------------------------------------------------------------------ terrain function
  // A real hill: about 26 units from the ring road up to the hilltop, with three soft terraces
  // (flats near z = 112, 36, -40, -116), gentle undulation, and shoulders rising at the sides.
  // Beyond the district the ground falls away into a coastal basin holding a ring of ridges and
  // a bay, so the top of the frame carries hill silhouettes and water instead of one flat plain.
  // The camera's horizontal field is 72 degrees, so the whole ridge field is clustered between
  // azimuths -128 and -52: relief placed outside that wedge is never in the frame, however large
  // it is. Height drops with distance as well, because the sight line from a camera 213 units up
  // at a 40 degree pitch crosses y = 213 - 0.32 * d, and a crest above that line is cropped.
  // Every ridge sits outside the district bounds, so picking, agents and the camera clamp never
  // see it: the relief is a height field the existing ground mesh already carries.
  const FAR_RIDGES = [
    // azimuth (degrees), radius, crest half width, height above the basin floor
    [-116, 300, 88, 46], [-64, 300, 88, 44],     // the two shoulders that frame the bay
    [-110, 374, 80, 44], [-70, 374, 80, 44],     // the next crests along the same shore
    [-128, 336, 118, 30], [-52, 336, 118, 30],   // the corners of the frame
    [-120, 452, 96, 26], [-60, 452, 96, 26]      // the low skyline at the very top edge
  ];
  // The island chain in the mouth of the bay. The table above frames the water, but every one
  // of those crests lands between the fifth and the seventh tenth of the top third on a phone,
  // because the portrait framing pulls the camera back to a 652 metre radius: above them there
  // is only water and air, which is the flat plate. These islands stand inside the bay, 380 to
  // 610 metres out, where the same sight line crosses them 20 to 130 pixels below the top edge:
  // a crest at 470 metres and 56 units up projects at py 79, and the water behind it is eleven
  // degrees farther away and therefore paler by the fog alone. Each crest is at a different
  // radius, so they stack as separate blue silhouettes with air between them instead of one
  // mass, and the nearest is the darkest.
  const BAY_ISLETS = [
    // azimuth (degrees), radius, crest half width, height above the dug bay floor
    [-96, 470, 118, 160], [-78, 430, 96, 124], [-104, 520, 110, 164],
    [-84, 610, 150, 176], [-70, 386, 74, 96], [-110, 434, 88, 108],
    // Rocks and skerries in the mouth of the bay. They stand a few metres over the water, so
    // their silhouettes are the one straight line the middle of the top third can hold, and at
    // a kilometre they are the difference between a sea and a blue field.
    [-88, 372, 22, 34], [-99, 398, 30, 26], [-74, 452, 26, 30], [-112, 476, 34, 22],
    [-90, 556, 40, 30], [-82, 648, 30, 26], [-118, 604, 26, 24], [-66, 512, 24, 20]
  ];
  const FAR_RIDGE_XZ = (function () {
    const out = new Float64Array((FAR_RIDGES.length + BAY_ISLETS.length) * 4);
    for (let i = 0; i < FAR_RIDGES.length; i += 1) {
      const r = FAR_RIDGES[i];
      const a = r[0] * Math.PI / 180;
      out[i * 4] = Math.cos(a) * r[1];
      out[i * 4 + 1] = Math.sin(a) * r[1];
      out[i * 4 + 2] = r[2];
      out[i * 4 + 3] = r[3];
    }
    for (let i = 0; i < BAY_ISLETS.length; i += 1) {
      const r = BAY_ISLETS[i];
      const a = r[0] * Math.PI / 180;
      const k = (FAR_RIDGES.length + i) * 4;
      out[k] = Math.cos(a) * r[1];
      out[k + 1] = Math.sin(a) * r[1];
      out[k + 2] = r[2];
      out[k + 3] = r[3];
    }
    return out;
  })();
  const BAY_AZ = -Math.PI * 0.5;   // the bay opens toward -Z, the top of the default view
  const BAY_HALF = 0.30;           // radians of shoreline either side of the bay axis
  const WATER_LEVEL = -8;          // the bay surface, and the level its plane is built at

  function districtHeight(x, z) {
    const tRaw = (Z_LOW - z) / (Z_LOW - Z_TOP);
    const t = clamp(tRaw, 0, 1);
    const s = t * 3;
    let i = Math.floor(s);
    if (i > 2) i = 2;
    const terr = (i + smooth(0.3, 0.72, s - i)) / 3;
    let y = HILL * (0.3 * t + 0.7 * terr);
    if (tRaw > 1) y += Math.min(16, (tRaw - 1) * 14);
    else if (tRaw < 0) y += Math.max(-5, tRaw * 7);
    y += Math.sin(x * 0.031 + 0.4) * 0.9 + Math.cos(z * 0.027 + x * 0.012) * 0.6;
    y += 7 * smooth(128, 240, Math.abs(x));
    return y;
  }

  function farHeight(x, z) {
    const d2 = x * x + z * z;
    if (d2 < 32400) return 0;                 // inside 180 the district owns the ground
    const d = Math.sqrt(d2);
    const mask = smooth(180, 300, d);
    // Basin floor: the plateau behind the district drops 40 units, so the far ground is not a
    // wall standing at exactly the height of the frame's top edge, and the hills in the basin
    // get room to stand up with their crests inside the frame.
    let y = Math.sin(x * 0.0071 + 1.3) * 7 + Math.cos(z * 0.0083 - 0.4) * 6 +
      Math.sin((x - z) * 0.0047) * 5 - 40 * smooth(200, 380, d);
    // The bay: a basin dug below the water level in one arc, so its shore and its surface show
    // through the gap between two ridges. Dug before the ridges are added, so a headland that
    // stands in the mouth of the bay rises out of the water instead of being drowned by it.
    let da = Math.atan2(z, x) - BAY_AZ;
    if (da > Math.PI) da -= Math.PI * 2;
    else if (da < -Math.PI) da += Math.PI * 2;
    const bay = (1 - smooth(BAY_HALF * 0.5, BAY_HALF, Math.abs(da))) *
      smooth(228, 356, d) * (1 - smooth(900, 1180, d));
    if (bay > 0) y -= 32 * bay;
    for (let i = 0; i < FAR_RIDGE_XZ.length; i += 4) {
      const w = FAR_RIDGE_XZ[i + 2];
      const dx = x - FAR_RIDGE_XZ[i];
      const dz = z - FAR_RIDGE_XZ[i + 1];
      const dd2 = dx * dx + dz * dz;
      if (dd2 < w * w) {
        const u = Math.sqrt(dd2) / w;
        const dome = 0.5 * (1 + Math.cos(Math.PI * u));
        y += FAR_RIDGE_XZ[i + 3] * dome;
        // The islands are the only ground in the top third of a portrait frame, and a smooth
        // dome lit from one side is one gradient with nothing in it. A slow wave across each
        // crest turns the flank into ribs: every rib has a face turned into the sun and a face
        // turned away, and the crease between them is a line the eye can follow at a kilometre
        // and a half. No triangles: the grid already has the vertices, this only moves them.
        if (i >= FAR_RIDGES.length * 4) {
          const ia = Math.atan2(FAR_RIDGE_XZ[i + 1], FAR_RIDGE_XZ[i]);
          const ca = Math.cos(ia);
          const sa = Math.sin(ia);
          const q = (dx * sa - dz * ca) / w;
          y += FAR_RIDGE_XZ[i + 3] * 0.26 * dome * dome * Math.sin(q * 7.1 + i * 0.7);
        }
      }
    }
    return y * mask;
  }

  function groundHeightAt(x, z) {
    return districtHeight(x, z) + farHeight(x, z);
  }

  function normalAt(x, z, out) {
    const e = 0.7;
    const hx = groundHeightAt(x + e, z) - groundHeightAt(x - e, z);
    const hz = groundHeightAt(x, z + e) - groundHeightAt(x, z - e);
    return out.set(-hx, 2 * e, -hz).normalize();
  }

  // ------------------------------------------------------------------ road curves
  // Every control point sits on the road surface (ground + lift), densely enough that the
  // curve, which agents drive on, never leaves the asphalt ribbon built from the same ground.
  function lifted(x, z, lift) {
    return new THREE.Vector3(x, groundHeightAt(x, z) + lift, z);
  }

  const roundaboutCenter = new THREE.Vector3(0, 0, ROUND_CZ);
  roundaboutCenter.y = groundHeightAt(roundaboutCenter.x, roundaboutCenter.z);

  // ------------------------------------------------------------------ ring outline
  // Hand-placed control points, not a formula: a wide lower bowl, a straight commercial frontage
  // down the east side (x about 97 from z 40 down to z -24, nearly a straight line), a lobe that
  // climbs the west flank to z = -80, and a detour round the south half of the roundabout so ring
  // traffic visibly circles it. The control points are also the containment polygon.
  const RING_PTS = [
    [26, 104], [52, 100], [74, 92], [88, 78], [94, 60], [96, 40], [97, 20], [97, -2],
    [95, -24], [88, -44], [74, -56], [54, -61], [32, -62], [14, -64], [2, -72], [-14, -80],
    [-32, -80], [-50, -72], [-66, -62], [-78, -52], [-88, -34], [-94, -14], [-96, 8],
    [-95, 32], [-91, 54], [-85, 74], [-74, 90], [-58, 100], [-40, 105], [-26, 106]
  ];
  const DETOUR_SEGS = 8;
  const ringPoly = (function () {
    const pts = [];
    for (let i = 0; i < RING_PTS.length; i += 1) pts.push(RING_PTS[i][0], RING_PTS[i][1]);
    for (let k = 0; k <= DETOUR_SEGS; k += 1) {
      const phi = Math.PI * (1 - k / DETOUR_SEGS);
      pts.push(
        roundaboutCenter.x + Math.cos(phi) * ROUND_LANE,
        roundaboutCenter.z + Math.sin(phi) * ROUND_LANE
      );
    }
    return pts;
  })();

  // Signed distance to the ring centreline, negative inside. One pass does containment and the
  // distance to the nearest segment, so the inset interior and the frontage band share it.
  function ringSigned(x, z) {
    let inside = false;
    let best = Infinity;
    const n = ringPoly.length;
    for (let i = 0, j = n - 2; i < n; j = i, i += 2) {
      const xi = ringPoly[i];
      const zi = ringPoly[i + 1];
      const xj = ringPoly[j];
      const zj = ringPoly[j + 1];
      if ((zi > z) !== (zj > z) && x < ((xj - xi) * (z - zi)) / (zj - zi) + xi) inside = !inside;
      const dx = xi - xj;
      const dz = zi - zj;
      let t = ((x - xj) * dx + (z - zj) * dz) / (dx * dx + dz * dz);
      t = t < 0 ? 0 : t > 1 ? 1 : t;
      const px = xj + dx * t - x;
      const pz = zj + dz * t - z;
      const d = px * px + pz * pz;
      if (d < best) best = d;
    }
    const dist = Math.sqrt(best);
    return inside ? -dist : dist;
  }

  const ringCtl = [];
  for (let i = 0; i < RING_PTS.length; i += 1) {
    ringCtl.push(lifted(RING_PTS[i][0], RING_PTS[i][1], LIFT_ROAD));
  }
  // West to east round the south half of the roundabout.
  for (let k = 0; k <= DETOUR_SEGS; k += 1) {
    const phi = Math.PI * (1 - k / DETOUR_SEGS);
    ringCtl.push(lifted(
      roundaboutCenter.x + Math.cos(phi) * ROUND_LANE,
      roundaboutCenter.z + Math.sin(phi) * ROUND_LANE,
      LIFT_ROAD
    ));
  }
  const ringWorld = new THREE.CatmullRomCurve3(ringCtl, true, 'centripetal');

  // ------------------------------------------------------------------ arterial spurs
  // Three open arterials that leave the kidney and run off the frame on three sides, so the city
  // continues outward instead of stopping at a wall. They start on the ring centreline. Each one
  // now runs far enough past the camera's ground footprint to leave the frame rather than stop
  // inside it: the west and east arterials were twenty to thirty metres short of the edge, which
  // read as a road that gives up in the field.
  //
  // Two more were added this cycle for the outer fabric: a second lower-west arterial a street's
  // width south of the first, and a hillside arterial that leaves the ring on the west flank and
  // climbs the north-west slope out of the left edge of the frame. Both are the same section as
  // the others, so the paving, kerbs, joints, gullies, lamps, night ribbons and the traffic
  // tables all apply to them with no new code: a street is a curve in this list, and everything
  // that makes a street a street already reads the list.
  const SPUR_PTS = [
    [[-92, 42], [-112, 38], [-136, 30], [-166, 20], [-208, 6], [-250, -4]],
    [[96, 10], [112, 6], [130, -2], [158, -14], [204, -30], [248, -46]],
    [[-40, -76], [-54, -94], [-64, -114], [-70, -140], [-78, -182], [-86, -228]],
    // The lower-west spill: parallel to the first arterial a street grid's depth down the slope,
    // out past the left edge of the district frame. It stands forty metres off the first one,
    // not twenty-four: the two avenues have to be far enough apart to hold a block between them,
    // and at twenty-four metres neither could carry a forty metre plot on its far side without
    // standing in the other's carriageway.
    [[-96, 4], [-130, -8], [-170, -22], [-210, -34], [-250, -46], [-292, -58]],
    // The hillside spill: off the ring's north-west arc, climbing the slope and leaving the left
    // edge at about z -110. It runs well north of the two valley avenues, not beside them: three
    // parallel arterials twenty-eight metres apart left no room for a block between any two of
    // them, so the hillside one was moved up the slope where it serves the crest instead.
    [[-66, -62], [-104, -70], [-148, -82], [-196, -98], [-248, -118], [-300, -140]]
  ];
  const spurs = [];
  for (let i = 0; i < SPUR_PTS.length; i += 1) {
    const pts = SPUR_PTS[i];
    const ctl = [];
    for (let k = 0; k < pts.length; k += 1) ctl.push(lifted(pts[k][0], pts[k][1], LIFT_ROAD));
    spurs.push(new THREE.CatmullRomCurve3(ctl, false, 'centripetal'));
  }

  // ------------------------------------------------------------------ elevated expressway
  // Lower left, running outside the ring on the valley floor: constant deck level, so it is on
  // its stone abutment near the west edge and on piers for the rest. It leaves the frame at both
  // ends; the tunnel portal is at the west one.
  const EX_PTS = [
    [-152, 46], [-140, 58], [-126, 72], [-112, 86], [-98, 100], [-84, 114], [-72, 128], [-62, 142]
  ];
  const exCtl = [];
  for (let i = 0; i < EX_PTS.length; i += 1) {
    exCtl.push(new THREE.Vector3(EX_PTS[i][0], EX_DECK_Y, EX_PTS[i][1]));
  }
  const expresswayCurve = new THREE.CatmullRomCurve3(exCtl, false, 'centripetal');

  // The avenue climbs from the roundabout to the hilltop with a gentle S.
  const AVE_Z0 = roundaboutCenter.z - ROUND_R;
  const AVE_Z1 = -112;
  function avenueX(z) {
    return 5 * Math.sin(Math.PI * (AVE_Z0 - z) / 205);
  }
  const aveCtl = [];
  for (let i = 0; i <= 28; i += 1) {
    const z = AVE_Z0 + (AVE_Z1 - AVE_Z0) * (i / 28);
    aveCtl.push(lifted(avenueX(z), z, LIFT_ROAD));
  }
  const avenueCurve = new THREE.CatmullRomCurve3(aveCtl, false, 'centripetal');

  // The tram: up the east side of the avenue, a turning loop at the hilltop, down the west side,
  // then round the roundabout island by the south.
  const tramCtl = [];
  {
    const C = roundaboutCenter;
    const zJoin = C.z - Math.sqrt(TRAM_LOOP_R * TRAM_LOOP_R - TRAM_OFF * TRAM_OFF);
    const phiW = Math.atan2(zJoin - C.z, -TRAM_OFF) + Math.PI * 2;
    const phiE = Math.atan2(zJoin - C.z, TRAM_OFF);
    for (let k = 1; k <= 5; k += 1) {
      const phi = phiW + (phiE - phiW) * (k / 6);
      tramCtl.push(lifted(C.x + Math.cos(phi) * TRAM_LOOP_R, C.z + Math.sin(phi) * TRAM_LOOP_R, LIFT_BED));
    }
    tramCtl.push(lifted(TRAM_OFF, zJoin, LIFT_BED));
    const STEPS = 22;
    const p = new THREE.Vector3();
    const t = new THREE.Vector3();
    for (let i = 0; i <= STEPS; i += 1) {
      const u = i / STEPS;
      avenueCurve.getPointAt(u, p);
      avenueCurve.getTangentAt(u, t);
      tramCtl.push(lifted(p.x - t.z * TRAM_OFF, p.z + t.x * TRAM_OFF, LIFT_BED));
    }
    avenueCurve.getPointAt(1, p);
    avenueCurve.getTangentAt(1, t);
    for (let k = 1; k <= 3; k += 1) {
      const psi = (k / 4) * Math.PI;
      const c = Math.cos(psi) * TRAM_OFF;
      const s = Math.sin(psi) * TRAM_OFF;
      tramCtl.push(lifted(p.x - t.z * c + t.x * s, p.z + t.x * c + t.z * s, LIFT_BED));
    }
    for (let i = STEPS; i >= 0; i -= 1) {
      const u = i / STEPS;
      avenueCurve.getPointAt(u, p);
      avenueCurve.getTangentAt(u, t);
      tramCtl.push(lifted(p.x + t.z * TRAM_OFF, p.z - t.x * TRAM_OFF, LIFT_BED));
    }
    tramCtl.push(lifted(-TRAM_OFF, zJoin, LIFT_BED));
  }
  const tramCurve = new THREE.CatmullRomCurve3(tramCtl, true, 'centripetal');

  const railCtl = [];
  for (let z = -104; z <= 100.001; z += 12) {
    railCtl.push(lifted(RAIL_X + 0.4 * Math.sin(z * 0.05), z, LIFT_RAIL));
  }
  const railCurve = new THREE.CatmullRomCurve3(railCtl, false, 'centripetal');

  // Arc-length samples with unit right-hand normals in the XZ plane.
  function samplePath(curve, step) {
    const len = curve.getLength();
    const n = Math.max(8, Math.ceil(len / step));
    const xs = new Float32Array(n + 1);
    const zs = new Float32Array(n + 1);
    const nx = new Float32Array(n + 1);
    const nz = new Float32Array(n + 1);
    for (let i = 0; i <= n; i += 1) {
      const u = i / n;
      curve.getPointAt(u, v3a);
      curve.getTangentAt(u, v3b);
      let tx = v3b.x;
      let tz = v3b.z;
      const l = Math.sqrt(tx * tx + tz * tz) || 1;
      tx /= l;
      tz /= l;
      xs[i] = v3a.x;
      zs[i] = v3a.z;
      nx[i] = -tz;
      nz[i] = tx;
    }
    return { n: n, len: len, step: len / n, xs: xs, zs: zs, nx: nx, nz: nz };
  }

  const ringPath = samplePath(ringWorld, 1.5);
  const avePath = samplePath(avenueCurve, 1.5);
  const tramPath = samplePath(tramCurve, 1.5);
  const railPath = samplePath(railCurve, 1.5);
  const spurPaths = [];
  for (let i = 0; i < spurs.length; i += 1) spurPaths.push(samplePath(spurs[i], 2.0));
  const exPath = samplePath(expresswayCurve, 2.0);

  // ------------------------------------------------------------------ farm tracks
  // Dirt tracks on the hillside. Two leave the ring at the bottom of the frame and climb across
  // the near slope to the switchback lane's hairpins, one leaves the east arterial and one the
  // west arterial and climb the outer shoulders, where the frame's left and right edges are.
  // They are sampled exactly like the roads and drawn by the same band builder, so a rut
  // follows the ground instead of floating over a hill, and their cells are flagged before the
  // terrain mesh is built so relief() leaves the corridor flat under them.
  const TRACK_HALF = 1.3;
  const TRACK_PTS = [
    // The west near slope: the ring's southern carriageway at z 104 out to the lane at (-24, 183).
    [[-46, 104], [-43, 122], [-38, 138], [-33, 156], [-29, 170], [-24, 183]],
    // The middle near slope: the ring at (26, 104) out to the lane's hairpin at (-6, 192).
    [[26, 104], [20, 128], [14, 140], [4, 152], [-2, 168], [-6, 192]],
    // The east shoulder: east arterial out across the outer belt, and up toward the crest.
    [[118, 4], [128, 14], [141, 27], [154, 43], [166, 62], [176, 84], [184, 108], [188, 128]],
    // The west shoulder: west arterial out under the ridge line, clear of the expressway.
    [[-166, 26.5], [-172, 36], [-180, 50], [-188, 68], [-196, 88], [-201, 108]]
  ];
  const trackCurves = [];
  const trackPaths = [];
  for (let i = 0; i < TRACK_PTS.length; i += 1) {
    const pts = TRACK_PTS[i];
    const ctl = [];
    for (let k = 0; k < pts.length; k += 1) ctl.push(lifted(pts[k][0], pts[k][1], LIFT_ROAD));
    const curve = new THREE.CatmullRomCurve3(ctl, false, 'centripetal');
    trackCurves.push(curve);
    trackPaths.push(samplePath(curve, 2.0));
  }

  // The same four polylines as a published table, the way the road curves are published: the
  // centre line, its right-hand normal and the height of the drawn dirt band at every sample.
  // An agent can then walk a track without re-deriving its geometry or its surface lift.
  const trackTables = [];
  for (let t = 0; t < trackPaths.length; t += 1) {
    const path = trackPaths[t];
    const ys = new Float32Array(path.n + 1);
    for (let i = 0; i <= path.n; i += 1) ys[i] = groundHeightAt(path.xs[i], path.zs[i]) + LIFT_TRACK;
    trackTables.push({
      n: path.n,
      len: path.len,
      step: path.step,
      xs: path.xs,
      zs: path.zs,
      nx: path.nx,
      nz: path.nz,
      ys: ys
    });
  }

  function inRoundabout(x, z, r) {
    const dx = x - roundaboutCenter.x;
    const dz = z - roundaboutCenter.z;
    return dx * dx + dz * dz < r * r;
  }

  // ------------------------------------------------------------------ zebra crossings
  // One per cross street that lands on the boulevard. The paint spans pavement to pavement, the
  // planted median is cut over the same band, and the list is exposed so pedestrian agents walk
  // the paint instead of a guessed line.
  const crossings = [];
  {
    for (let h = 0; h < H_LINES.length && crossings.length < CROSS_CAP; h += 1) {
      const targetZ = H_LINES[h];
      let best = 0;
      let bd = Infinity;
      for (let i = 0; i <= avePath.n; i += 1) {
        const d = Math.abs(avePath.zs[i] - targetZ);
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      if (bd > 3.5) continue;
      const px = avePath.xs[best];
      const pz = avePath.zs[best];
      if (inRoundabout(px, pz, ROUND_R + 2)) continue;
      const dirX = -avePath.nz[best];
      const dirZ = avePath.nx[best];
      crossings.push({
        x: px,
        z: pz,
        dirX: dirX,
        dirZ: dirZ,
        halfWidth: 2.1,
        halfSpan: AVE_HALF + AVE_PAVE,
        span: 2 * (AVE_HALF + AVE_PAVE),
        road: 'avenue'
      });
    }
  }

  function nearCrossing(x, z, pad) {
    for (let i = 0; i < crossings.length; i += 1) {
      const c = crossings[i];
      const along = (x - c.x) * c.dirX + (z - c.z) * c.dirZ;
      const across = (x - c.x) * -c.dirZ + (z - c.z) * c.dirX;
      if (Math.abs(along) < c.halfWidth + pad && Math.abs(across) < c.halfSpan + 2) return true;
    }
    return false;
  }

  // Every zebra actually painted, avenue, spur and ring, with its true footprint. The drains
  // and covers added below are placed only when they stay a clear pad away from every entry,
  // so a grate can never land in a crossing whatever road the crossing belongs to.
  const zebraPaint = [];

  function crossingClear(x, z, pad) {
    for (let i = 0; i < zebraPaint.length; i += 1) {
      const c = zebraPaint[i];
      const dx = x - c.x;
      const dz = z - c.z;
      const along = dx * c.tx + dz * c.tz;
      const across = dx * -c.tz + dz * c.tx;
      if (Math.abs(along) < c.depth + pad && Math.abs(across) < c.half + pad) return false;
    }
    return true;
  }

  // ------------------------------------------------------------------ occupancy grid
  // One metre cells over the district. Bits keep roads, streets, buildings, props, trees, houses
  // and the dream corridor apart so nothing is ever scattered over a road or inside a building.
  const B_RING = 1;
  const B_AVE = 2;
  const B_ROUND = 4;
  const B_PLAZA = 8;
  const B_RAIL = 16;
  const B_STREET = 32;
  const B_BUILD = 64;
  const B_TRAM = 128;
  const B_TREE = 256;
  const B_PROP = 512;
  const B_SIDEWALK = 1024;
  const B_CORR = 2048;
  const B_OLD = 4096;
  const B_HOUSE = 8192;
  const B_SPUR = 16384;
  const B_VIA = 32768;
  // The farm tracks of the near slopes. They are unpaved, but every rule that keeps a tree or a
  // prop off a road applies to them unchanged, so they ride the roads mask; the paving passes
  // that must not draw over asphalt ask B_PAVED, which is the hard surfaces only.
  const B_TRACK = 65536;
  // A churchyard cypress's ground, claimed before the first house so no wall can arrive on it.
  // It is deliberately outside every solid mask: the cypress is planted on it, and only the
  // house test reads it.
  const B_CYPRESS = 131072;
  const B_PAVED = B_RING | B_AVE | B_ROUND | B_PLAZA | B_RAIL | B_TRAM | B_SPUR | B_STREET | B_SIDEWALK | B_VIA;
  const B_ROADS = B_RING | B_AVE | B_ROUND | B_PLAZA | B_RAIL | B_TRAM | B_SPUR | B_TRACK;
  const B_SOLID = B_ROADS | B_STREET | B_SIDEWALK | B_BUILD | B_PROP | B_CORR | B_HOUSE | B_VIA;
  const B_BLOCK = B_ROADS | B_STREET | B_SIDEWALK | B_BUILD | B_PROP | B_CORR | B_OLD | B_VIA;
  // The occupancy grid grew from 440 to 512 cells a side this cycle: the outer fabric reaches
  // two hundred and fifty metres along the new arterials, and a lot beyond the edge of the grid
  // used to be placed with no collision test at all, because rectScan clamps its scan box to the
  // grid and returns true for a rectangle that lies entirely outside it.
  const G_MIN = -256;
  const G_N = 512;
  const grid = new Uint32Array(G_N * G_N);

  function flagsAt(x, z) {
    const i = Math.floor(x - G_MIN);
    const j = Math.floor(z - G_MIN);
    if (i < 0 || j < 0 || i >= G_N || j >= G_N) return 0;
    return grid[j * G_N + i];
  }

  function markDisk(x, z, r, bit) {
    const i0 = Math.max(0, Math.floor(x - r - G_MIN));
    const i1 = Math.min(G_N - 1, Math.floor(x + r - G_MIN));
    const j0 = Math.max(0, Math.floor(z - r - G_MIN));
    const j1 = Math.min(G_N - 1, Math.floor(z + r - G_MIN));
    const r2 = r * r;
    for (let j = j0; j <= j1; j += 1) {
      const dz = j + G_MIN + 0.5 - z;
      for (let i = i0; i <= i1; i += 1) {
        const dx = i + G_MIN + 0.5 - x;
        if (dx * dx + dz * dz <= r2) grid[j * G_N + i] |= bit;
      }
    }
  }

  function diskFree(x, z, r, mask) {
    const i0 = Math.max(0, Math.floor(x - r - G_MIN));
    const i1 = Math.min(G_N - 1, Math.floor(x + r - G_MIN));
    const j0 = Math.max(0, Math.floor(z - r - G_MIN));
    const j1 = Math.min(G_N - 1, Math.floor(z + r - G_MIN));
    const r2 = r * r;
    for (let j = j0; j <= j1; j += 1) {
      const dz = j + G_MIN + 0.5 - z;
      for (let i = i0; i <= i1; i += 1) {
        const dx = i + G_MIN + 0.5 - x;
        if (dx * dx + dz * dz <= r2 && (grid[j * G_N + i] & mask)) return false;
      }
    }
    return true;
  }

  // Scans the cells of an oriented rectangle (centre, unit axis u, half extents). With a mask,
  // returns false as soon as a cell carries one of its bits; with a bit, marks every cell.
  function rectScan(cx, cz, ux, uz, hu, hv, mask, bit) {
    const R = Math.sqrt(hu * hu + hv * hv) + 0.8;
    const i0 = Math.max(0, Math.floor(cx - R - G_MIN));
    const i1 = Math.min(G_N - 1, Math.floor(cx + R - G_MIN));
    const j0 = Math.max(0, Math.floor(cz - R - G_MIN));
    const j1 = Math.min(G_N - 1, Math.floor(cz + R - G_MIN));
    for (let j = j0; j <= j1; j += 1) {
      const dz = j + G_MIN + 0.5 - cz;
      for (let i = i0; i <= i1; i += 1) {
        const dx = i + G_MIN + 0.5 - cx;
        const lu = dx * ux + dz * uz;
        const lv = -dx * uz + dz * ux;
        if (lu < -hu || lu > hu || lv < -hv || lv > hv) continue;
        const k = j * G_N + i;
        if (mask && (grid[k] & mask)) return false;
        if (bit) grid[k] |= bit;
      }
    }
    return true;
  }

  function markPath(path, r, bit) {
    for (let i = 0; i <= path.n; i += 1) markDisk(path.xs[i], path.zs[i], r, bit);
  }

  markPath(ringPath, RING_HALF + RING_PAVE + 0.2, B_RING);
  markPath(avePath, AVE_PAVE_IN + AVE_PAVE + 0.1, B_AVE);
  markPath(tramPath, TRAM_BED + 0.3, B_TRAM);
  markPath(railPath, 8.6, B_RAIL);
  for (let i = 0; i < spurPaths.length; i += 1) markPath(spurPaths[i], SPUR_EDGE + 0.25, B_SPUR);
  markPath(exPath, EX_HALF + 1.8, B_VIA);
  markDisk(roundaboutCenter.x, roundaboutCenter.z, ROUND_R + 0.6, B_ROUND);
  rectScan(PLAZA_CX, PLAZA_CZ, 1, 0, PLAZA_W * 0.5 + 0.4, PLAZA_D * 0.5 + 0.4, 0, B_PLAZA);

  // ------------------------------------------------------------------ outer fabric region
  // The buildable ground outside the ring. Until this cycle the outer belt had no region test at
  // all: `tryParcel` accepted any free cell for a kind 3 lot, which is how a handful of slabs
  // with murals came to stand alone in the fields on the left, the right and the bottom of the
  // frame. The region is now the ground a street actually serves, and nothing else:
  //
  //   the outside face of the ring, from its pavement out to OUTER_BELT_DEPTH, and
  //   the two sides of every arterial, out to OUTER_BAND either side of its centreline.
  //
  // A kind 3 lot must have its front and its body inside that band, so its facade always faces
  // kerb and pavement, and continuous streets are the only thing that can create frontage: the
  // city can spill outward on any side, but only along a road that is really there. The band is
  // a two metre raster of the same occupancy extent, so the test is one array read and the
  // build-time cost is a pass over the road samples.
  const OUTER_BELT_DEPTH = 32;      // ring pavement to the back of the outer belt
  const OUTER_BAND = 36;            // arterial centreline to the back of its blocks
  const FABRIC_CELL = 2;
  const FABRIC_MIN = -300;
  const FABRIC_N = 300;
  const fabricMask = new Uint8Array(FABRIC_N * FABRIC_N);
  function markFabricDisk(x, z, r) {
    const i0 = Math.max(0, Math.floor((x - r - FABRIC_MIN) / FABRIC_CELL));
    const i1 = Math.min(FABRIC_N - 1, Math.floor((x + r - FABRIC_MIN) / FABRIC_CELL));
    const j0 = Math.max(0, Math.floor((z - r - FABRIC_MIN) / FABRIC_CELL));
    const j1 = Math.min(FABRIC_N - 1, Math.floor((z + r - FABRIC_MIN) / FABRIC_CELL));
    const r2 = r * r;
    for (let j = j0; j <= j1; j += 1) {
      const dz = FABRIC_MIN + (j + 0.5) * FABRIC_CELL - z;
      for (let i = i0; i <= i1; i += 1) {
        const dx = FABRIC_MIN + (i + 0.5) * FABRIC_CELL - x;
        if (dx * dx + dz * dz <= r2) fabricMask[j * FABRIC_N + i] = 1;
      }
    }
  }
  for (let s = 0; s < spurPaths.length; s += 1) {
    const path = spurPaths[s];
    for (let i = 0; i <= path.n; i += 1) markFabricDisk(path.xs[i], path.zs[i], OUTER_BAND);
  }
  for (let i = 0; i <= ringPath.n; i += 1) {
    markFabricDisk(ringPath.xs[i], ringPath.zs[i], OUTER_BELT_DEPTH);
  }
  // Only the outside face of the ring is buildable, and only past its own pavement.
  function inOuterFabric(x, z) {
    const s = ringSigned(x, z);
    if (s < -RING_FRONT - 1.2) return false;
    const i = Math.floor((x - FABRIC_MIN) / FABRIC_CELL);
    const j = Math.floor((z - FABRIC_MIN) / FABRIC_CELL);
    if (i < 0 || j < 0 || i >= FABRIC_N || j >= FABRIC_N) return false;
    return fabricMask[j * FABRIC_N + i] === 1;
  }

  // The old village and the hilltop landmarks claim their ground here, before the street grid
  // walks a line and before the first parcel is laid. Marking them after the parcels, as the
  // crown once did, reserved nothing: the street grid had already put a lane through the
  // village and a block against the church wall.
  rectScan((OLD_X0 + OLD_X1) * 0.5, (OLD_Z0 + OLD_Z1) * 0.5, 1, 0,
    (OLD_X1 - OLD_X0) * 0.5, (OLD_Z1 - OLD_Z0) * 0.5, 0, B_OLD);
  rectScan(-46, -104, 1, 0, 8.5, 12.5, 0, B_PROP);   // the church and its steps
  markDisk(-49, -110, 3.2, B_PROP);                  // the campanile's own ground
  markDisk(76, -98, 5.2, B_PROP);                    // the water tower on the east crest

  // ------------------------------------------------------------------ urban region
  // The fabric fills the kidney interior, inset by the ring's asphalt and pavement, and the
  // hilltop band above it, whose outline is a lens that tapers toward the top of the hill.
  function inRingInterior(x, z) {
    return ringSigned(x, z) < -8.8;
  }

  function inHillTop(x, z) {
    const ax = x / 96;
    const az = (z + 74) / 42;
    return ax * ax + az * az < 1;
  }

  function inUrban(x, z) {
    return inRingInterior(x, z) || inHillTop(x, z);
  }

  // The ring frontage sits just inside the ring, between the pavement edge and the inset region,
  // so a parcel can abut the ring pavement without being inside the inset interior.
  function inRingFrontage(x, z) {
    return ringSigned(x, z) < -7.0;
  }

  // ------------------------------------------------------------------ quarter sites
  const qCount = Math.min(quartersModel.length, QUARTER_CAP);
  const siteX = new Float32Array(Math.max(1, qCount));
  const siteZ = new Float32Array(Math.max(1, qCount));
  const qLevel = new Float32Array(Math.max(1, qCount));
  let sumBC = 0;
  for (let qi = 0; qi < qCount; qi += 1) {
    const qs = quartersModel[qi] || {};
    const site = SITES[qi] || [0, 0];
    siteX[qi] = site[0];
    siteZ[qi] = site[1];
    const bc = clamp(qs.buildingCount || 8, 1, 40);
    qLevel[qi] = bc / 40;
    sumBC += bc;
  }

  // ------------------------------------------------------------------ street grid
  // Streets are walked in two metre steps, kept only where they stay inside the urban region
  // and clear of the ring, the avenue, the tram, the roundabout, the plaza and the rails. The
  // valid runs become one asphalt quad each, and their pavements are laid in three metre steps
  // that stop wherever another street's asphalt is.
  const streetRunsV = [];
  const streetRunsH = [];
  const lampSpots = [];
  let streetFrontage = 0;

  function walkStreet(vertical, coord, out) {
    const from = vertical ? GRID_Z0 : GRID_X0;
    const to = vertical ? GRID_Z1 : GRID_X1;
    const STEP = 2;
    let runStart = -1;
    let nextLamp = 0;
    let side = 1;
    for (let t = from; t < to; t += STEP) {
      const m = t + STEP * 0.5;
      const mx = vertical ? coord : m;
      const mz = vertical ? m : coord;
      const ok = inUrban(mx, mz) &&
        diskFree(mx, mz, STREET_EDGE + 0.55, B_ROADS | B_OLD | B_PROP);
      if (ok) {
        if (vertical) {
          rectScan(mx, mz, 0, 1, STEP * 0.5 + 0.1, STREET_HALF, 0, B_STREET);
          rectScan(mx, mz, 0, 1, STEP * 0.5 + 0.1, STREET_EDGE, 0, B_SIDEWALK);
        } else {
          rectScan(mx, mz, 1, 0, STEP * 0.5 + 0.1, STREET_HALF, 0, B_STREET);
          rectScan(mx, mz, 1, 0, STEP * 0.5 + 0.1, STREET_EDGE, 0, B_SIDEWALK);
        }
        if (runStart < 0) {
          runStart = t;
          nextLamp = t + 6;
        }
        streetFrontage += STEP * 2;
        if (t >= nextLamp) {
          nextLamp = t + 24;
          const off = side * (STREET_HALF + 0.55);
          const lx = vertical ? coord + off : m;
          const lz = vertical ? m : coord + off;
          if (!(flagsAt(lx, lz) & (B_BUILD | B_PROP))) {
            lampSpots.push({
              x: lx, z: lz,
              tx: vertical ? coord : m,
              tz: vertical ? m : coord
            });
          }
          side = -side;
        }
      } else if (runStart >= 0) {
        out.push({ coord: coord, t0: runStart, t1: t });
        runStart = -1;
      }
    }
    if (runStart >= 0) out.push({ coord: coord, t0: runStart, t1: to });
  }

  for (let i = 0; i < V_LINES.length; i += 1) walkStreet(true, V_LINES[i], streetRunsV);
  for (let i = 0; i < H_LINES.length; i += 1) walkStreet(false, H_LINES[i], streetRunsH);

  // ------------------------------------------------------------------ quarter centres
  // A centre is a street intersection inside the quarter's blocks: open ground, so the dream's
  // provisional road never has to cross a building.
  const quads = [];
  const usedCenters = [];
  for (let qi = 0; qi < qCount; qi += 1) {
    const sx = siteX[qi];
    const sz = siteZ[qi];
    let bx = sx;
    let bz = sz;
    let best = Infinity;
    for (let a = 0; a < V_LINES.length; a += 1) {
      for (let b = 0; b < H_LINES.length; b += 1) {
        const x = V_LINES[a];
        const z = H_LINES[b];
        if (!inUrban(x, z)) continue;
        const f = flagsAt(x, z);
        if (!(f & B_STREET)) continue;
        if (f & (B_BUILD | B_PLAZA | B_RAIL | B_PROP)) continue;
        let dup = false;
        for (let k = 0; k < usedCenters.length && !dup; k += 1) {
          const u = usedCenters[k];
          if (Math.abs(u[0] - x) < 18 && Math.abs(u[1] - z) < 18) dup = true;
        }
        if (dup) continue;
        const dx = x - sx;
        const dz = z - sz;
        const d = dx * dx + dz * dz;
        if (d < best) {
          best = d;
          bx = x;
          bz = z;
        }
      }
    }
    if (best === Infinity) {
      // No intersection survived the clip: fall back to the first free ground near the site.
      for (let k = 0; k < 90; k += 1) {
        const ang = k * 2.399;
        const rr = 3 + (k / 90) * 26;
        const x = sx + Math.cos(ang) * rr;
        const z = sz + Math.sin(ang) * rr;
        if (inUrban(x, z) && diskFree(x, z, 3.2, B_SOLID)) {
          bx = x;
          bz = z;
          break;
        }
      }
    }
    usedCenters.push([bx, bz]);
    quads.push({
      idx: qi,
      name: (quartersModel[qi] || {}).name,
      color: (quartersModel[qi] || {}).color,
      center: new THREE.Vector3(bx, groundHeightAt(bx, bz), bz),
      radius: 20,
      labelPosition: new THREE.Vector3(bx, groundHeightAt(bx, bz) + 16, bz),
      signSlots: [],
      craneSlot: { position: new THREE.Vector3(bx, groundHeightAt(bx, bz), bz) },
      doorPoints: []
    });
  }

  // ------------------------------------------------------------------ dream corridor
  // The exact pair rule from the contract: model.dreamPair, else the strongest pair in array
  // order on ties, else nothing. A fourteen unit strip along that straight segment stays free
  // of buildings, props and trees, and is surfaced as a vacant gravel lot.
  let dreamA = -1;
  let dreamB = -1;
  {
    const pairs = (model && model.pairs) || [];
    let pair = (model && model.dreamPair) || null;
    if (!pair) {
      for (let i = 0; i < pairs.length; i += 1) {
        if (!pair || pairs[i].strength > pair.strength) pair = pairs[i];
      }
    }
    if (pair && typeof pair.a === 'number' && typeof pair.b === 'number' &&
      pair.a >= 0 && pair.b >= 0 && pair.a < qCount && pair.b < qCount && pair.a !== pair.b) {
      dreamA = pair.a;
      dreamB = pair.b;
    }
  }

  const corridor = {
    has: dreamA >= 0 && dreamB >= 0,
    ax: 0, az: 0, bx: 0, bz: 0, len: 0, dx: 0, dz: 1
  };
  if (corridor.has) {
    const A = quads[dreamA].center;
    const B = quads[dreamB].center;
    corridor.ax = A.x;
    corridor.az = A.z;
    corridor.bx = B.x;
    corridor.bz = B.z;
    const dx = B.x - A.x;
    const dz = B.z - A.z;
    const len = Math.sqrt(dx * dx + dz * dz) || 1;
    corridor.len = len;
    corridor.dx = dx / len;
    corridor.dz = dz / len;
    const steps = Math.max(1, Math.ceil(len / 3));
    for (let i = 0; i <= steps; i += 1) {
      markDisk(A.x + dx * (i / steps), A.z + dz * (i / steps), 7, B_CORR);
    }
  }

  // ------------------------------------------------------------------ parcel budget
  // The parcel width comes from the frontage that survived the clip, so the block count lands
  // in the 250 to 450 band whatever the model throws at it, and the count of quarters only
  // moves it gently.
  const frontageTotal = streetFrontage + ringPath.len * 0.92 + avePath.len * 1.8;
  const targetBuildings = qCount === 0 ? 0 : clamp(Math.round(170 + sumBC * 1.05), 240, 400);
  const parcelW = targetBuildings === 0 ? 7 : clamp((frontageTotal * 0.355) / targetBuildings, 4.8, 10);

  // ------------------------------------------------------------------ geometry builder
  // Accumulates positions, normals and vertex colours, then emits one indexed geometry.
  function makeBuilder() {
    const P = [];
    const N = [];
    const C = [];
    const I = [];
    function vert(x, y, z, nx, ny, nz, col) {
      P.push(x, y, z);
      N.push(nx, ny, nz);
      C.push(col.r, col.g, col.b);
      return P.length / 3 - 1;
    }
    function groundVert(x, z, lift, col) {
      normalAt(x, z, v3b);
      return vert(x, groundHeightAt(x, z) + lift, z, v3b.x, v3b.y, v3b.z, col);
    }
    function flatQuad(ax, az, bx, bz, cx, cz, dx, dz, lift, col) {
      const a = groundVert(ax, az, lift, col);
      const b = groundVert(bx, bz, lift, col);
      const c = groundVert(cx, cz, lift, col);
      const d = groundVert(dx, dz, lift, col);
      const cross = (bz - az) * (cx - ax) - (bx - ax) * (cz - az);
      if (cross > 0) I.push(a, b, c, a, c, d);
      else I.push(a, c, b, a, d, c);
    }
    function addGeometry(geo, matrix, col, useVertexCol) {
      const pos = geo.attributes.position;
      const nor = geo.attributes.normal;
      const vcol = useVertexCol ? geo.attributes.color : null;
      m3.getNormalMatrix(matrix);
      const base = P.length / 3;
      for (let k = 0; k < pos.count; k += 1) {
        v3a.fromBufferAttribute(pos, k).applyMatrix4(matrix);
        v3b.fromBufferAttribute(nor, k).applyMatrix3(m3).normalize();
        if (vcol) {
          colorScratch2.setRGB(vcol.getX(k), vcol.getY(k), vcol.getZ(k)).multiply(col);
          vert(v3a.x, v3a.y, v3a.z, v3b.x, v3b.y, v3b.z, colorScratch2);
        } else {
          vert(v3a.x, v3a.y, v3a.z, v3b.x, v3b.y, v3b.z, col);
        }
      }
      if (geo.index) {
        const ia = geo.index.array;
        for (let k = 0; k < ia.length; k += 1) I.push(base + ia[k]);
      } else {
        for (let k = 0; k < pos.count; k += 1) I.push(base + k);
      }
    }
    function idx(i0, i1, i2) { I.push(i0, i1, i2); }
    function build() {
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(P), 3));
      geo.setAttribute('normal', new THREE.BufferAttribute(new Float32Array(N), 3));
      geo.setAttribute('color', new THREE.BufferAttribute(new Float32Array(C), 3));
      const vc = P.length / 3;
      geo.setIndex(new THREE.BufferAttribute(vc > 65535 ? new Uint32Array(I) : new Uint16Array(I), 1));
      geo.computeBoundingSphere();
      return geo;
    }
    return { vert: vert, flatQuad: flatQuad, addGeometry: addGeometry, idx: idx, build: build };
  }

  function composeMatrix(x, y, z, rotY, sx, sy, sz, rotX) {
    euler.set(rotX || 0, rotY || 0, 0, 'YXZ');
    quat.setFromEuler(euler);
    mPos.set(x, y, z);
    scaleV.set(sx, sy, sz);
    return m4.compose(mPos, quat, scaleV);
  }

  // The shared unit box: five faces, ten triangles. The camera never looks up, because
  // camera.mjs fixes the pitch forty degrees below horizontal and even the top edge of the
  // view frustum stays below the horizon, so the -Y face is never a front face in the colour
  // pass. Dropping it saves two triangles on every instance and every merged piece: the detail
  // layer, the buildings, the cranes, the parked cars, the statics and the expressway parts.
  // The faces keep outward per-face normals and there is no vertex colour attribute, exactly
  // like the BoxGeometry this replaces; nothing samples the box's uv.
  //
  // The shadow pass does render back faces (three flips a FrontSide material to BackSide when
  // material.shadowSide is null), so the -Y face is the only surface that writes the depth of a
  // floating horizontal slab. Losing it changes nothing under a grounded box: the ground there
  // is inside the footprint and hidden by the volume itself, and the visible shadow beside it is
  // still written by the leeward side faces. What does go is the detached patch a low sun throws
  // from an elevated horizontal face, displaced by height / tan(25 degrees) and therefore far
  // from the awning, balcony, car or jib that casts it. No consumer in this world needs that
  // patch enough to justify a closed box, and the expressway soffit and the tunnel throat are
  // hand-built quads that do not come from this geometry.
  const unitBox = (function () {
    // +X, -X, +Y, +Z, -Z, each with four vertices wound counter-clockwise seen from outside.
    const faces = [
      { n: [1, 0, 0], v: [[0.5, 0.5, 0.5], [0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5]] },
      { n: [-1, 0, 0], v: [[-0.5, 0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5]] },
      { n: [0, 1, 0], v: [[-0.5, 0.5, -0.5], [-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5]] },
      { n: [0, 0, 1], v: [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]] },
      { n: [0, 0, -1], v: [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]] }
    ];
    const pos = new Float32Array(faces.length * 4 * 3);
    const nor = new Float32Array(faces.length * 4 * 3);
    const idx = new Uint16Array(faces.length * 6);
    for (let f = 0; f < faces.length; f += 1) {
      const a = f * 12;
      const i = f * 6;
      for (let k = 0; k < 4; k += 1) {
        pos[a + k * 3] = faces[f].v[k][0];
        pos[a + k * 3 + 1] = faces[f].v[k][1];
        pos[a + k * 3 + 2] = faces[f].v[k][2];
        nor[a + k * 3] = faces[f].n[0];
        nor[a + k * 3 + 1] = faces[f].n[1];
        nor[a + k * 3 + 2] = faces[f].n[2];
      }
      const b = f * 4;
      idx[i] = b; idx[i + 1] = b + 1; idx[i + 2] = b + 2;
      idx[i + 3] = b; idx[i + 4] = b + 2; idx[i + 5] = b + 3;
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    geo.setAttribute('normal', new THREE.BufferAttribute(nor, 3));
    geo.setIndex(new THREE.BufferAttribute(idx, 1));
    return geo;
  })();

  // ------------------------------------------------------------------ block parcels
  // Every parcel is a rectangle whose front edge sits on a street pavement, laid out along one
  // street line at a time. Consecutive parcels abut, so each street gets a continuous facade;
  // the second row of the block faces the opposite street, and the gap between the rows is the
  // courtyard.
  const blds = [];
  // Filled by the separation pass below, then used by the tree and furniture siting tests.
  let bldObb = null;
  const bldsOfQuarter = [];
  for (let q = 0; q < qCount; q += 1) bldsOfQuarter.push([]);
  const parcelRnd = mulberry(20261105);

  function nearestQuarter(cx, cz) {
    let best = -1;
    let bd = Infinity;
    for (let qi = 0; qi < qCount; qi += 1) {
      const dx = cx - siteX[qi];
      const dz = cz - siteZ[qi];
      const d = dx * dx + dz * dz;
      if (d < bd) {
        bd = d;
        best = qi;
      }
    }
    return best;
  }

  // kind: 0 street frontage, 1 avenue frontage, 2 ring frontage, 3 outer belt or spur frontage.
  function tryParcel(fx, fz, nx, nz, wide, deep, doorInset, kind) {
    if (blds.length >= BUILDING_CAP) return null;
    const ux = nz;
    const uz = -nx;
    const cx = fx - nx * deep * 0.5;
    const cz = fz - nz * deep * 0.5;
    if (kind === 2) {
      if (!inRingFrontage(cx, cz)) return null;
    } else if (kind === 3) {
      // An outer-belt block exists only where a street serves it: its front, its body and its
      // back all have to be inside the outer fabric band, which is the ring's outer face and
      // the flanks of the arterials. This is the rule that stops a big slab from standing alone
      // in a field; before it, kind 3 had no region test at all.
      if (!inOuterFabric(fx, fz) || !inOuterFabric(cx, cz)) return null;
    } else {
      if (!inUrban(fx, fz) || !inUrban(cx, cz)) return null;
    }
    const hu = wide * 0.5;
    const hv = deep * 0.5;
    if (!rectScan(cx, cz, ux, uz, hu - 0.05, hv - 0.05, B_BLOCK, 0)) return null;
    rectScan(cx, cz, ux, uz, hu - 0.05, hv - 0.05, 0, B_BUILD);
    const qi = nearestQuarter(cx, cz);
    const b = {
      qi: qi,
      cx: cx,
      cz: cz,
      fx: fx,
      fz: fz,
      nx: nx,
      nz: nz,
      ux: ux,
      uz: uz,
      w: wide,
      d: deep,
      hu: hu,
      hv: hv,
      rotY: -Math.atan2(uz, ux),
      fYaw: Math.atan2(nx, nz),
      doorInset: doorInset,
      kind: kind,
      tower: false,
      floors: 4,
      gMin: 0,
      gMax: 0,
      bottom: 0,
      top: 0,
      color: 0,
      shade: 1,
      rnd: parcelRnd
    };
    blds.push(b);
    if (qi >= 0) bldsOfQuarter[qi].push(b);
    return b;
  }

  // Towers first: three or four office slabs on the upper plateau, count from the model's
  // towerCount, capped at four. They are the city's vertical statement, so they get the big
  // free plots before the street-front rows claim them.
  //
  // The first cycle tried eight fixed spots with a ten step spiral each and the city grew one
  // tower: most of those spots sat on the avenue's fifteen metre cartilage or on the market
  // square, where no plot exists at any offset, and the two that had open ground beside them
  // were abandoned because the spiral only ever reached the same block. The search is a list of
  // fronts instead. Every two metres of both kerbs of the three upper streets, and every six
  // metres of the ring's inner pavement, is a candidate; each one is handed to the same
  // `tryParcel` a building lot goes through, so the nineteen metre footprint is tested against
  // the same occupancy grid, the same urban region and the same frontage band, and nothing
  // here re-implements a rule. Candidates are ranked by distance from the middle of the old
  // spot set, so the slabs that land stand as one cluster on the crest; the ring frontage comes
  // last in that ranking, which is where it belongs, because a tower ringed by the arterial
  // reads as a gatehouse rather than as the city's core.
  let towerWanted = 0;
  for (let qi = 0; qi < qCount; qi += 1) {
    towerWanted += Math.max(0, Math.min(3, (quartersModel[qi] || {}).towerCount || 0));
  }
  towerWanted = qCount === 0 ? 0 : Math.min(4, Math.max(3, towerWanted));

  const TOWER_W = 17.5;
  const TOWER_GAP = 1.0;       // clear air between two slabs, so they read as separate volumes
  const TOWER_LINES = [H_LINES[3], H_LINES[2], H_LINES[1]];
  // The centre the cluster is grown from: the mean of the old spot set, which was chosen for
  // the upper plateau between the two ranked quarters.
  const TOWER_ANCHOR_X = 14;
  const TOWER_ANCHOR_Z = -16;
  const towerFronts = [];
  function addTowerFront(x, z, nx, nz, kind) {
    for (let i = 0; i < towerFronts.length; i += 1) {
      const f = towerFronts[i];
      if (Math.abs(f.x - x) < 0.9 && Math.abs(f.z - z) < 0.9) return;
    }
    const dx = x - TOWER_ANCHOR_X;
    const dz = z - TOWER_ANCHOR_Z;
    towerFronts.push({ x: x, z: z, nx: nx, nz: nz, kind: kind, d: dx * dx + dz * dz });
  }
  // Both kerbs of the three upper streets, two metres apart.
  for (let li = 0; li < TOWER_LINES.length; li += 1) {
    const line = TOWER_LINES[li];
    for (let side = -1; side <= 1; side += 2) {
      for (let x = GRID_X0 + 3; x <= GRID_X1 - 3; x += 2) {
        addTowerFront(x, line + side * STREET_SET, 0, -side, 0);
      }
    }
  }
  // The ring's inner frontage, six metres apart, normal pointing out at the arterial and the
  // body growing inward like every ring lot.
  for (let i = 0; i < ringPath.n; i += 4) {
    const ix = -ringPath.xs[i];
    const iz = RING_CZ - ringPath.zs[i];
    const l = Math.sqrt(ix * ix + iz * iz) || 1;
    const dx = ix / l;
    const dz = iz / l;
    addTowerFront(ringPath.xs[i] + dx * RING_FRONT, ringPath.zs[i] + dz * RING_FRONT,
      -dx, -dz, 2);
  }
  towerFronts.sort(function (a, b) { return a.d - b.d; });

  const towerPlaced = [];
  function towerClear(cx, cz) {
    for (let i = 0; i < towerPlaced.length; i += 1) {
      const r = towerPlaced[i];
      if (Math.abs(r.cx - cx) < TOWER_W + TOWER_GAP &&
        Math.abs(r.cz - cz) < TOWER_W + TOWER_GAP) return false;
    }
    return true;
  }
  for (let i = 0; i < towerFronts.length && towerPlaced.length < towerWanted; i += 1) {
    const f = towerFronts[i];
    const cx = f.x - f.nx * TOWER_W * 0.5;
    const cz = f.z - f.nz * TOWER_W * 0.5;
    if (!towerClear(cx, cz)) continue;
    const b = tryParcel(f.x, f.z, f.nx, f.nz, TOWER_W, TOWER_W,
      f.kind === 2 ? 2.7 : 0.6, f.kind);
    if (!b) continue;
    b.tower = true;
    towerPlaced.push(b);
  }

  // A row is filled with abutting parcels whose count comes from the free length, so a street
  // never ends up with a sliver of unused frontage. The free length is measured on the occupancy
  // grid, so a row stops exactly where the crossing street or an already placed corner building
  // begins: no overlap, no hole.
  const SLOT_MIN = 4.7;
  const SLOT_MAX = 17.5;
  const ROW_STEP = 2.0;

  function rowSlot(vertical, coord, side, a, b, perp) {
    const wide = b - a;
    const tc = a + wide * 0.5;
    // A corner plot is shallow on purpose: its flank faces the crossing street, and a deep corner
    // would swallow the whole frontage of that street. A block that was already cut by a courtyard
    // gets a shallower house rather than an empty plot.
    let corner = false;
    for (let k = 0; k < perp.length; k += 1) {
      if (Math.abs(tc - perp[k]) < 12) corner = true;
    }
    const base = corner ? 5.9 + parcelRnd() * 1.0 : 8.4 + parcelRnd() * 3.0;
    for (let k = 0; k < 3; k += 1) {
      const deep = base - k * 1.8;
      if (deep < 4.7) break;
      const b2 = vertical
        ? tryParcel(coord + side * STREET_SET, tc, -side, 0, wide, deep, 0.6, 0)
        : tryParcel(tc, coord + side * STREET_SET, 0, -side, wide, deep, 0.6, 0);
      if (b2) return b2;
    }
    return null;
  }

  function rowRun(vertical, coord, side, a, b, perp) {
    const len = b - a;
    if (len < SLOT_MIN) return;
    let n = Math.max(1, Math.round(len / parcelW));
    if (len / n > SLOT_MAX) n = Math.ceil(len / SLOT_MAX);
    for (let i = 0; i < n; i += 1) {
      const s0 = a + (len * i) / n + 0.16;
      const s1 = a + (len * (i + 1)) / n - 0.16;
      if (rowSlot(vertical, coord, side, s0, s1, perp)) continue;
      // Half a plot is better than a hole: the free half of a rejected slot still gets a house.
      const mid = (s0 + s1) * 0.5;
      if (mid - s0 > SLOT_MIN) rowSlot(vertical, coord, side, s0, mid - 0.22, perp);
      if (s1 - mid > SLOT_MIN) rowSlot(vertical, coord, side, mid + 0.22, s1, perp);
    }
  }

  // Is the front strip of the plot at arc position s on this side clear?
  function lineFrontFree(vertical, coord, side, s) {
    const fx = vertical ? coord + side * STREET_SET : s;
    const fz = vertical ? s : coord + side * STREET_SET;
    const dx = vertical ? side : 0;
    const dz = vertical ? 0 : side;
    if (!inUrban(fx + dx * 5, fz + dz * 5)) return false;
    if (vertical) return rectScan(fx + dx * 2.7, fz, 0, 1, 1.4, 2.5, B_BLOCK, 0);
    return rectScan(fx, fz + dz * 2.7, 1, 0, 1.4, 2.5, B_BLOCK, 0);
  }

  function fillLine(vertical, coord, side) {
    const from = vertical ? GRID_Z0 : GRID_X0;
    const to = vertical ? GRID_Z1 : GRID_X1;
    const perp = vertical ? H_LINES : V_LINES;
    const count = Math.floor((to - from) / ROW_STEP);
    let runStart = -1;
    for (let i = 0; i <= count; i += 1) {
      const s = from + i * ROW_STEP;
      const free = i < count && lineFrontFree(vertical, coord, side, s + ROW_STEP * 0.5);
      if (free && runStart < 0) runStart = i;
      if (!free && runStart >= 0) {
        rowRun(vertical, coord, side, from + runStart * ROW_STEP + 0.34, from + i * ROW_STEP - 0.34, perp);
        runStart = -1;
      }
    }
    if (runStart >= 0) rowRun(vertical, coord, side, from + runStart * ROW_STEP + 0.34, to - 0.34, perp);
  }

  // Curved frontage: walk the sampled path, keep the stretches whose front strip is clear, and
  // fill each of them with abutting parcels that follow the curve.
  const pathDirX = new Float32Array(4096);
  const pathDirZ = new Float32Array(4096);

  // Outer-fabric frontage is metered: the band around the ring and the arterials has more
  // kerbside than the frame has room for blocks, so each street is filled up to its own share
  // and the rest of its length stays open verge. The count is a build budget, like the model lot
  // and tree caps, and it is what keeps the extra fabric inside the triangle gate.
  let fabricLots = 0;
  let fabricLimit = 0;
  function fabricFull(kind) {
    return kind === 3 && fabricLots >= fabricLimit;
  }

  function fillPathRun(path, front, kind, from, to) {
    const len = (to - from + 1) * path.step;
    if (len < SLOT_MIN) return;
    if (fabricFull(kind)) return;
    let n = Math.max(1, Math.round(len / parcelW));
    if (len / n > SLOT_MAX) n = Math.ceil(len / SLOT_MAX);
    for (let i = 0; i < n; i += 1) {
      if (fabricFull(kind)) break;
      const ia = Math.min(path.n, Math.round(from + (to + 1 - from) * (i / n)));
      const ib = Math.min(path.n, Math.round(from + (to + 1 - from) * ((i + 1) / n)));
      const im = Math.min(path.n, (ia + ib) >> 1);
      // Curved rows: neighbouring plots are rotated against each other, so their flanks would
      // interpenetrate at the back. A narrow gap keeps each house a separate volume.
      const wide = Math.max(2, (ib - ia) * path.step - 1.15);
      const dx = pathDirX[im];
      const dz = pathDirZ[im];
      const fx = path.xs[im] + dx * front;
      const fz = path.zs[im] + dz * front;
      let placed = false;
      for (let k = 0; k < 3; k += 1) {
        const deep = (8.6 + parcelRnd() * 2.8) - k * 1.8;
        if (deep < 5.8) break;
        if (tryParcel(fx, fz, -dx, -dz, wide, deep, kind === 2 ? 2.7 : 2.3, kind)) {
          placed = true;
          if (kind === 3) fabricLots += 1;
          break;
        }
      }
      if (!placed && wide > 6.4) {
        // Retry as two narrower plots so a partly taken stretch still reads as built frontage.
        const half = wide * 0.5 - 0.3;
        const tx2 = path.nz[im] * half * 0.5;
        const tz2 = -path.nx[im] * half * 0.5;
        for (let k = 0; k < 2; k += 1) {
          const off = k === 0 ? -1 : 1;
          for (let j = 0; j < 2; j += 1) {
            const deep2 = (8.4 + parcelRnd() * 2.4) - j * 1.6;
            if (deep2 < 5.2) break;
            if (tryParcel(fx + tx2 * off, fz + tz2 * off, -dx, -dz, half, deep2, kind === 2 ? 2.7 : 2.3, kind)) {
              if (kind === 3) fabricLots += 1;
              break;
            }
          }
        }
      }
    }
  }

  function fillPath(path, front, kind) {
    let runStart = -1;
    for (let i = 0; i <= path.n; i += 1) {
      const dx = pathDirX[i];
      const dz = pathDirZ[i];
      const fx = path.xs[i] + dx * front;
      const fz = path.zs[i] + dz * front;
      let free = i < path.n &&
        rectScan(fx + dx * 4.2, fz + dz * 4.2, path.nz[i], -path.nx[i], path.step * 0.75, 4.0, B_BLOCK, 0);
      if (free && kind === 1) {
        free = inUrban(fx + dx * 3, fz + dz * 3);
      } else if (free && kind === 2) {
        free = inRingFrontage(fx + dx * 3, fz + dz * 3);
      }
      if (free && runStart < 0) runStart = i;
      if (!free && runStart >= 0) {
        fillPathRun(path, front, kind, runStart, i - 1);
        runStart = -1;
      }
    }
    if (runStart >= 0) fillPathRun(path, front, kind, runStart, path.n - 1);
  }

  // Ring frontage: the inner face of the ring road, pointing at the middle of the ellipse.
  for (let i = 0; i <= ringPath.n; i += 1) {
    let ix = -ringPath.xs[i];
    let iz = RING_CZ - ringPath.zs[i];
    const l = Math.sqrt(ix * ix + iz * iz) || 1;
    pathDirX[i] = ix / l;
    pathDirZ[i] = iz / l;
  }
  fillPath(ringPath, RING_FRONT, 2);

  // Avenue frontage: both kerbs, one side at a time so the two rows do not fight over a plot.
  for (let side = -1; side <= 1; side += 2) {
    for (let i = 0; i <= avePath.n; i += 1) {
      pathDirX[i] = avePath.nx[i] * side;
      pathDirZ[i] = avePath.nz[i] * side;
    }
    fillPath(avePath, AVE_FRONT, 1);
  }

  // The grid itself: the inner pair of streets first because they carry the most frontage,
  // then the outer ones, then the cross streets from the bottom of the hill upward. Vertical
  // rows go first so they own the corners they can fill.
  const V_ORDER = [3, 2, 4, 1, 5, 0];
  const H_ORDER = [4, 5, 6, 3, 2, 1, 0];
  for (let i = 0; i < V_ORDER.length; i += 1) {
    fillLine(true, V_LINES[V_ORDER[i]], 1);
    fillLine(true, V_LINES[V_ORDER[i]], -1);
  }
  for (let i = 0; i < H_ORDER.length; i += 1) {
    fillLine(false, H_LINES[H_ORDER[i]], 1);
    fillLine(false, H_LINES[H_ORDER[i]], -1);
  }

  // ------------------------------------------------------------------ farm track occupancy
  // The tracks are flagged after the street grid and its parcels, so the district's own blocks
  // are laid out exactly as before, and before the outer belt, the civic plots and the villas,
  // so a farm track threads the hillside instead of walking through a finished building. They
  // are also flagged before the terrain, the terraces, every prop and every tree: relief()
  // leaves their corridor flat, and the walls and the planting both stop at the ruts.
  for (let i = 0; i < trackPaths.length; i += 1) markPath(trackPaths[i], TRACK_HALF + 0.55, B_TRACK);

  // ------------------------------------------------------------------ civic plots
  // Hand-sited in the outer belt, each with room for its apron or pitch. They were moved onto
  // the ring's outer frontage this cycle: a 42 by 34 supermarket, a 46 by 40 school, a sports
  // hall or a pair of warehouses dropped at a random offset from the ring read as a toy box in a
  // field, which is exactly what the owner singled out on the left, the right and the bottom of
  // the frame. Each one now stands behind the ring's own kerb and pavement, with the frontage
  // fill abutting it on both sides, so it is part of a block instead of a thing on its own.
  //
  // The plot's local +v axis points away from the road, so the apron, the car park or the pitch
  // that the civic pass draws at negative lv sits between the kerb and the building, which is
  // how a supermarket forecourt or a school pitch really meets the street.
  const civic = [];
  const villas = [];

  function reservePlot(cx, cz, yaw, w, d) {
    const ux = Math.cos(yaw);
    const uz = -Math.sin(yaw);
    if (!rectScan(cx, cz, ux, uz, w * 0.5, d * 0.5, B_BLOCK, 0)) return false;
    rectScan(cx, cz, ux, uz, w * 0.5, d * 0.5, 0, B_BUILD);
    return true;
  }

  function ringOutward(i, out) {
    const px = ringPath.xs[i];
    const pz = ringPath.zs[i];
    let ox = px - 0;
    let oz = pz - RING_CZ;
    const l = Math.sqrt(ox * ox + oz * oz) || 1;
    out[0] = ox / l;
    out[1] = oz / l;
    out[2] = px;
    out[3] = pz;
    return out;
  }

  // A plot whose front edge stands a kerb's width off the ring's outer pavement. The search
  // walks the ring outward from the nearest sample to the requested station, so a station whose
  // ground is already taken slides along the frontage instead of hopping into the fields.
  // Is the whole front edge of this plot clear of the ring's pavement? Seven samples across the
  // width, each required to stand at least a metre outside the kerb line.
  function plotFrontClear(cx, cz, yaw, w, d) {
    const c = Math.cos(yaw);
    const s = Math.sin(yaw);
    const lv = -d * 0.5;
    for (let k = 0; k <= 6; k += 1) {
      const lu = (k / 6 - 0.5) * w;
      const x = cx + lu * c + lv * s;
      const z = cz - lu * s + lv * c;
      if (ringSigned(x, z) < RING_FRONT + 0.95) return false;
    }
    return true;
  }

  const civicOut = [0, 0, 0, 0];
  function placeCivicOnRing(type, w, d, target) {
    for (let t = 0; t < target.length; t += 1) {
      const rec = placeCivicAtStation(type, w, d, target[t][0], target[t][1]);
      if (rec) return rec;
    }
    return null;
  }

  // Distance from a point to a sampled path centreline, in metres.
  function distToPath(path, x, z) {
    let best = Infinity;
    for (let i = 0; i < path.n; i += 1) {
      const ax = path.xs[i];
      const az = path.zs[i];
      const dx = path.xs[i + 1] - ax;
      const dz = path.zs[i + 1] - az;
      let t = ((x - ax) * dx + (z - az) * dz) / (dx * dx + dz * dz || 1);
      t = t < 0 ? 0 : t > 1 ? 1 : t;
      const px = ax + dx * t - x;
      const pz = az + dz * t - z;
      const d = px * px + pz * pz;
      if (d < best) best = d;
    }
    return Math.sqrt(best);
  }

  // The same plot search along an arterial: the front edge faces the carriageway, both flanks
  // are tried, and the whole edge has to clear that street's pavement.
  function placeCivicOnSpur(type, w, d, spurIndex, tx, tz) {
    const path = spurPaths[spurIndex];
    if (!path) return null;
    let best = 0;
    let bd = Infinity;
    for (let i = 0; i <= path.n; i += 1) {
      const dx = path.xs[i] - tx;
      const dz = path.zs[i] - tz;
      const dd = dx * dx + dz * dz;
      if (dd < bd) {
        bd = dd;
        best = i;
      }
    }
    const gap = 1.25;
    const kerb = SPUR_EDGE + 0.95;
    for (let k = 0; k <= 60; k += 1) {
      for (let side = -1; side <= 1; side += 2) {
        if (k === 0 && side > 0) continue;
        const i = Math.max(0, Math.min(path.n, best + side * k));
        const nx = path.nx[i] * side;
        const nz = path.nz[i] * side;
        const yaw = Math.atan2(nx, nz);
        for (let push = 0; push <= 4; push += 1) {
          const off = SPUR_FRONT + gap + d * 0.5 + push;
          const cx = path.xs[i] + nx * off;
          const cz = path.zs[i] + nz * off;
          if (!plotFrontClearPath(path, cx, cz, yaw, w, d, kerb)) continue;
          if (!reservePlot(cx, cz, yaw, w, d)) continue;
          const rec = { type: type, x: cx, z: cz, yaw: yaw, w: w, d: d, via: 'spur' + spurIndex };
          civic.push(rec);
          return rec;
        }
      }
    }
    return null;
  }

  // Front-edge clearance against a straight-ish arterial rather than the ring.
  function plotFrontClearPath(path, cx, cz, yaw, w, d, kerb) {
    const c = Math.cos(yaw);
    const s = Math.sin(yaw);
    const lv = -d * 0.5;
    for (let k = 0; k <= 6; k += 1) {
      const lu = (k / 6 - 0.5) * w;
      const x = cx + lu * c + lv * s;
      const z = cz - lu * s + lv * c;
      if (distToPath(path, x, z) < kerb) return false;
    }
    return true;
  }

  function placeCivicAtStation(type, w, d, tx, tz) {
    let best = 0;
    let bd = Infinity;
    for (let i = 0; i <= ringPath.n; i += 1) {
      const dx = ringPath.xs[i] - tx;
      const dz = ringPath.zs[i] - tz;
      const dd = dx * dx + dz * dz;
      if (dd < bd) {
        bd = dd;
        best = i;
      }
    }
    const gap = 1.25;
    // A 42 m plot is a chord against a curve, so its two front corners stand closer to the ring
    // than its front centre does: on the south arc that is about three metres, which is enough
    // to bury both corners in the ring's own pavement and fail the plot outright. The offset
    // therefore grows until the whole front edge clears the kerb, and a station that still does
    // not fit at eight metres is skipped in favour of the next one along the ring.
    for (let k = 0; k <= 80; k += 1) {
      for (let side = -1; side <= 1; side += 2) {
        if (k === 0 && side > 0) continue;
        const i = (best + side * k + ringPath.n + 1) % (ringPath.n + 1);
        ringOutward(i, civicOut);
        const ox = civicOut[0];
        const oz = civicOut[1];
        const yaw = Math.atan2(ox, oz);
        const ux = Math.cos(yaw);
        const uz = -Math.sin(yaw);
        for (let push = 0; push <= 14; push += 1) {
          const off = RING_FRONT + gap + d * 0.5 + push;
          const cx = civicOut[2] + ox * off;
          const cz = civicOut[3] + oz * off;
          // The curve, not the station, decides where the front edge can stand. A 46 m plot is
          // a chord against a curving kerb, so its corners stand metres closer to the ring than
          // its middle does; a fixed offset buried both corners in the ring's own pavement and
          // failed the two widest plots on every station of the ring. The test below walks the
          // front edge itself and grows the offset until the whole edge clears the kerb, so a
          // plot that cannot sit tight against the pavement does not get placed at all.
          if (!plotFrontClear(cx, cz, yaw, w, d)) continue;
          if (!reservePlot(cx, cz, yaw, w, d)) continue;
          const rec = { type: type, x: cx, z: cz, yaw: yaw, w: w, d: d, via: 'ring' };
          civic.push(rec);
          return rec;
        }
      }
    }
    return null;
  }

  // Last resort, kept for the case where the whole ring frontage is somehow occupied: a short
  // spiral near the requested station. It is reported by the civic audit as `offStreet`, so a
  // plot that could not reach a kerb is a number and not a surprise in a screenshot.
  let civicOffStreet = 0;
  function placeCivicNear(type, tx, tz, yaw, w, d) {
    for (let k = 0; k < 70; k += 1) {
      const ang = k * 2.399;
      const rr = k === 0 ? 0 : 2 + (k / 70) * 30;
      const cx = tx + Math.cos(ang) * rr;
      const cz = tz + Math.sin(ang) * rr;
      if (!reservePlot(cx, cz, yaw, w, d)) continue;
      const rec = { type: type, x: cx, z: cz, yaw: yaw, w: w, d: d, via: 'near' };
      civic.push(rec);
      civicOffStreet += 1;
      return rec;
    }
    return null;
  }

  // The stations follow what the district frame shows: the supermarket on the south-east arc
  // (the right margin), the sports hall at the bottom of the frame, the school on the
  // south-west arc (the left margin), the filling station on the west lobe, and the two
  // warehouses beside the yard on the south-east.
  // Each use carries a list of candidate stations, tried in order: the first stretch of outer
  // frontage whose whole plot clears the kerb, the expressway, the rail and the farm tracks
  // wins. One station was not enough for the two deepest plots: the south-west arc that used to
  // hold the school is crossed by the expressway, so the school had nowhere to stand and simply
  // vanished from the district.
  // The ring's outer belt is a narrow, crowded band: the expressway crosses the south-west,
  // four farm tracks climb out of it and five arterials leave through it. The two largest civic
  // uses therefore stand on the new arterials instead, which is where a supermarket and a
  // secondary school really stand, and it puts them in the left and right margins the owner
  // pointed at. The ring keeps the filling station and the two warehouses, the uses that fit a
  // short frontage.
  const civicMart = placeCivicOnSpur('mart', 42, 34, 1, 176, -20) ||
    placeCivicOnSpur('mart', 42, 34, 1, 210, -34) ||
    placeCivicOnRing('mart', 42, 34, [[86, 80], [100, 60]]) ||
    placeCivicNear('mart', 176, -20, -0.35, 42, 34);
  const civicFuel = placeCivicOnRing('fuel', 24, 20, [[-96, -6], [-96, 30], [-95, 54]]) ||
    placeCivicNear('fuel', -96, -6, 0.22, 24, 20);
  const civicSchool = placeCivicOnSpur('school', 40, 36, 3, -178, -40) ||
    placeCivicOnSpur('school', 40, 36, 1, 190, -60) ||
    placeCivicOnSpur('school', 40, 36, 0, -172, 16) ||
    placeCivicOnRing('school', 40, 36, [[-40, 104], [20, 108], [-72, 96]]) ||
    placeCivicNear('school', -178, -10, 0.06, 40, 36);
  const civicSport = placeCivicOnSpur('sport', 44, 34, 3, -150, -30) ||
    placeCivicOnSpur('sport', 44, 34, 0, -150, 12) ||
    placeCivicOnRing('sport', 44, 34, [[18, 104], [-30, 104], [60, 96]]) ||
    placeCivicNear('sport', -150, 12, 0, 44, 34);
  const civicDockA = placeCivicOnRing('warehouse', 14, 30, [[52, 100], [30, 106]]) ||
    placeCivicOnSpur('warehouse', 14, 30, 1, 150, -10) ||
    placeCivicNear('warehouse', 52, 100, 0, 14, 30);
  const civicDockB = placeCivicOnRing('warehouse', 14, 26, [[72, 94], [94, 70]]) ||
    placeCivicOnSpur('warehouse', 14, 26, 1, 168, -16) ||
    placeCivicNear('warehouse', 72, 94, 0, 14, 26);

  // Villas with pools, only on the far outer slopes, one per promontory plot.
  const VILLA_SPOTS = [
    [-26, 242], [16, 258], [40, 234],
    [-152, 6], [-168, -36], [-148, -76], [-120, -120], [-88, -152],
    [20, 154], [76, 144], [136, 106], [152, 42], [142, -48], [120, -142], [62, -158],
    // The near slope: the bottom third of a phone frame looks at this band, where one villa is
    // two hundred pixels across and its lit wall is the only built thing on the hillside.
    [-62, 236], [44, 268], [112, 216], [-108, 292], [16, 330]
  ];
  {
    const rnd = mulberry(60660);
    for (let i = 0; i < VILLA_SPOTS.length; i += 1) {
      const tx = VILLA_SPOTS[i][0];
      const tz = VILLA_SPOTS[i][1];
      const yaw = Math.atan2(-tx, -tz) + Math.PI + (rnd() - 0.5) * 0.3;
      for (let k = 0; k < 40; k += 1) {
        const ang = k * 2.399;
        const rr = k === 0 ? 0 : 2 + (k / 40) * 22;
        const cx = tx + Math.cos(ang) * rr;
        const cz = tz + Math.sin(ang) * rr;
        if (!reservePlot(cx, cz, yaw, 26, 24)) continue;
        villas.push({ x: cx, z: cz, yaw: yaw, pool: rnd() < 0.85 });
        break;
      }
    }
  }

  // ------------------------------------------------------------------ outer fabric frontage
  // Outside the kidney the city does not stop, and it does not start in a field either. The ring
  // is now built on both faces and every arterial on both flanks, with the same abutting-parcel
  // fill the inner ring face and the boulevard use, so every outer block has kerb, pavement and
  // a neighbour and its facade faces a street. No pitched roof out here: the only tile left in
  // the district is the stone crown on the summit.
  //
  // Each street gets a share of the metered budget, in the order the district frame reads them:
  // the ring's outer face first (it starts at the bottom of the frame and runs east), then the
  // two new west arterials, then the east arterial, then the original west one. The northern
  // arterial is left alone: the hilltop band above the ring is already built.
  const FABRIC_LOT_MAX = isTouch ? 26 : 76;
  const fabricAudit = { ring: 0, westLower: 0, hillside: 0, east: 0, west: 0, lots: 0 };
  {
    // The ring's outer face. pathDir is the outward direction, so fillPath places the facade on
    // the outer pavement and grows the body away from the road.
    for (let i = 0; i <= ringPath.n; i += 1) {
      ringOutward(i, civicOut);
      pathDirX[i] = civicOut[0];
      pathDirZ[i] = civicOut[1];
    }
    fabricLimit = Math.min(FABRIC_LOT_MAX, isTouch ? 9 : 24);
    let mark = blds.length;
    fillPath(ringPath, RING_FRONT, 3);
    for (let i = mark; i < blds.length; i += 1) blds[i].street = 'ring';
    fabricAudit.ring = fabricLots;

    const shares = [
      { spur: 3, name: 'westLower', max: 15 },
      { spur: 4, name: 'hillside', max: 13 },
      { spur: 1, name: 'east', max: 13 },
      { spur: 0, name: 'west', max: 13 }
    ];
    for (let s = 0; s < shares.length && fabricLots < FABRIC_LOT_MAX; s += 1) {
      const path = spurPaths[shares[s].spur];
      if (!path) continue;
      fabricLimit = Math.min(FABRIC_LOT_MAX, fabricLots + (isTouch ? shares[s].max * 0.45 | 0 : shares[s].max));
      const before = fabricLots;
      mark = blds.length;
      for (let side = -1; side <= 1; side += 2) {
        for (let i = 0; i <= path.n; i += 1) {
          pathDirX[i] = path.nx[i] * side;
          pathDirZ[i] = path.nz[i] * side;
        }
        fillPath(path, SPUR_FRONT, 3);
      }
      for (let i = mark; i < blds.length; i += 1) blds[i].street = shares[s].name;
      fabricAudit[shares[s].name] = fabricLots - before;
    }
  }

  // ------------------------------------------------------------------ building separation
  // The occupancy grid keeps parcels apart to the cell, but two rotated neighbours can still
  // interpenetrate by a metre in continuous space once their exact oriented boxes are compared.
  // This pass runs before any height, window or detail is derived from a footprint: a pair that
  // overlaps by more than a quarter metre is separated by shrinking the later plot along its own
  // narrower interpenetrating axis, so both plots keep their street frontage on the pavement.
  // Party walls up to 0.6 m are legitimate and stay. Shrinking can never create a new overlap,
  // so three passes are enough to settle a whole district.
  {
    const n = blds.length;
    const hit = { pen: 0, axis: 0, other: -1 };
    for (let pass = 0; pass < 3; pass += 1) {
      let resolved = 0;
      for (let i = 0; i < n; i += 1) {
        const a = blds[i];
        const ax = Math.cos(a.rotY);
        const az = -Math.sin(a.rotY);
        const bx = Math.sin(a.rotY);
        const bz = Math.cos(a.rotY);
        for (let j = i + 1; j < n; j += 1) {
          const c = blds[j];
          const dx = c.cx - a.cx;
          const dz = c.cz - a.cz;
          const reach = a.hu + a.hv + c.hu + c.hv + 1;
          if (dx * dx + dz * dz > reach * reach) continue;
          const cx = Math.cos(c.rotY);
          const cz = -Math.sin(c.rotY);
          const ex = Math.sin(c.rotY);
          const ez = Math.cos(c.rotY);
          // Four candidate separating axes: the two local axes of each box.
          let minPen = Infinity;
          let separated = false;
          for (let axIx = 0; axIx < 4 && !separated; axIx += 1) {
            let ux;
            let uz;
            if (axIx === 0) { ux = ax; uz = az; }
            else if (axIx === 1) { ux = bx; uz = bz; }
            else if (axIx === 2) { ux = cx; uz = cz; }
            else { ux = ex; uz = ez; }
            const ra = Math.abs(ax * ux + az * uz) * a.hu + Math.abs(bx * ux + bz * uz) * a.hv;
            const rc = Math.abs(cx * ux + cz * uz) * c.hu + Math.abs(ex * ux + ez * uz) * c.hv;
            const dist = Math.abs(dx * ux + dz * uz);
            const pen = ra + rc - dist;
            if (pen <= 0.0001) {
              separated = true;
              break;
            }
            if (pen < minPen) minPen = pen;
          }
          if (separated || minPen <= 0.3) continue;
          // Resolve on the later plot's own axis with the smaller penetration, falling back to its
          // other axis, and finally to a lateral slide when no axis can give up the width: a plot
          // is never shrunk below its minimum, and a slide keeps it on the street's facade line.
          const raU = Math.abs(ax * cx + az * cz) * a.hu + Math.abs(bx * cx + bz * cz) * a.hv;
          const raV = Math.abs(ax * ex + az * ez) * a.hu + Math.abs(bx * ex + bz * ez) * a.hv;
          const penU = raU + c.hu - Math.abs(dx * cx + dz * cz);
          const penV = raV + c.hv - Math.abs(dx * ex + dz * ez);
          const minW = c.tower ? 9 : 3.8;
          let settled = false;
          for (let k = 0; k < 2 && !settled; k += 1) {
            const onWidth = (k === 0) === (penU <= penV);
            const p = k === 0 ? Math.min(penU, penV) : Math.max(penU, penV);
            const shrink = 2 * (p + 0.05);
            if (onWidth) {
              if (c.w - shrink < minW) continue;
              c.w -= shrink;
              c.hu = c.w * 0.5;
            } else {
              if (c.d - shrink < minW) continue;
              c.d -= shrink;
              c.hv = c.d * 0.5;
              // Keep the front face exactly on its pavement: the plot is set back, not moved.
              c.cx += c.nx * shrink * 0.5;
              c.cz += c.nz * shrink * 0.5;
            }
            settled = true;
          }
          if (!settled) {
            const slide = Math.min(Math.min(penU, penV) + 0.08, 3.2);
            if (penU <= penV) {
              const dir = ((c.cx - a.cx) * cx + (c.cz - a.cz) * cz) >= 0 ? 1 : -1;
              c.cx += cx * slide * dir;
              c.cz += cz * slide * dir;
            } else {
              const dir = ((c.cx - a.cx) * ex + (c.cz - a.cz) * ez) >= 0 ? 1 : -1;
              c.cx += ex * slide * dir;
              c.cz += ez * slide * dir;
            }
          }
          resolved += 1;
        }
      }
      if (resolved === 0) break;
    }
    // Exact oriented boxes for the trees and the furniture to test against.
    const obb = {
      n: blds.length,
      cx: new Float32Array(blds.length),
      cz: new Float32Array(blds.length),
      ux: new Float32Array(blds.length),
      uz: new Float32Array(blds.length),
      vx: new Float32Array(blds.length),
      vz: new Float32Array(blds.length),
      hw: new Float32Array(blds.length),
      hd: new Float32Array(blds.length)
    };
    for (let i = 0; i < blds.length; i += 1) {
      const b = blds[i];
      obb.cx[i] = b.cx;
      obb.cz[i] = b.cz;
      obb.ux[i] = Math.cos(b.rotY);
      obb.uz[i] = -Math.sin(b.rotY);
      obb.vx[i] = Math.sin(b.rotY);
      obb.vz[i] = Math.cos(b.rotY);
      obb.hw[i] = b.hu;
      obb.hd[i] = b.hv;
    }
    bldObb = obb;
  }

  // Is this point inside any building footprint, grown by pad?
  function insideBuilding(x, z, pad) {
    if (!bldObb) return false;
    for (let i = 0; i < bldObb.n; i += 1) {
      const dx = x - bldObb.cx[i];
      const dz = z - bldObb.cz[i];
      const reach = bldObb.hw[i] + bldObb.hd[i] + pad + 1;
      if (dx * dx + dz * dz > reach * reach) continue;
      const lu = dx * bldObb.ux[i] + dz * bldObb.uz[i];
      const lv = dx * bldObb.vx[i] + dz * bldObb.vz[i];
      if (Math.abs(lu) < bldObb.hw[i] + pad && Math.abs(lv) < bldObb.hd[i] + pad) return true;
    }
    return false;
  }

  // ------------------------------------------------------------------ heights and colour
  for (let bi = 0; bi < blds.length; bi += 1) {
    const b = blds[bi];
    const rnd = mulberry(424242 + bi * 131);
    const corners = [
      b.cx + b.ux * b.hu + b.nx * b.hv, b.cz + b.uz * b.hu + b.nz * b.hv,
      b.cx - b.ux * b.hu + b.nx * b.hv, b.cz - b.uz * b.hu + b.nz * b.hv,
      b.cx - b.ux * b.hu - b.nx * b.hv, b.cz - b.uz * b.hu - b.nz * b.hv,
      b.cx + b.ux * b.hu - b.nx * b.hv, b.cz + b.uz * b.hu - b.nz * b.hv
    ];
    let gMin = Infinity;
    let gMax = -Infinity;
    for (let c = 0; c < 8; c += 2) {
      const g = groundHeightAt(corners[c], corners[c + 1]);
      if (g < gMin) gMin = g;
      if (g > gMax) gMax = g;
    }
    b.gMin = gMin;
    b.gMax = gMax;
    b.rnd = rnd;
    if (b.tower) {
      b.floors = 12 + Math.floor(rnd() * 9);
      b.top = gMax + b.floors * TOWER_FLOOR_H + 2.4;
      b.bottom = gMin - 1.2;
      b.color = GLASS[Math.floor(rnd() * GLASS.length)];
      b.shade = 0.94 + rnd() * 0.1;
    } else if (b.kind === 3) {
      // Contemporary flat-roofed apartment blocks on the outer slopes: five to eight floors.
      b.floors = 5 + Math.floor(rnd() * 4);
      b.top = gMax + b.floors * 3.0 + 0.7;
      b.bottom = gMin - 1.2;
      const pick = rnd();
      const alt = rnd();           // keeps the per-block stream identical, drives the stone pick
      b.color = pick < 0.2 ? STONE[Math.floor(alt * STONE.length)] : facadeAt(b.cx, b.cz);
      b.shade = 0.93 + rnd() * 0.13;
    } else {
      const ql = b.qi >= 0 ? qLevel[b.qi] : 0.4;
      let floors = 3;
      if (rnd() < 0.22 + ql * 0.5) floors += 1;
      if (rnd() < ql * 0.55) floors += 1;
      if (rnd() < ql * 0.4) floors += 1;
      if (rnd() < 0.1) floors -= 1;
      b.floors = clamp(floors, 3, 7);
      b.top = gMax + b.floors * 3.0 + 0.5;
      b.bottom = gMin - 1.0;
      const pick = rnd();
      const alt = rnd();           // keeps the per-block stream identical, drives the stone pick
      b.color = pick < 0.08 ? STONE[Math.floor(alt * STONE.length)] : facadeAt(b.cx, b.cz);
      // Street-canyon walls sit a touch darker than the ringside facades, and every block is
      // slightly under its base colour: the baked part of the ambient occlusion.
      b.shade = (0.92 + rnd() * 0.14) * (b.kind === 0 ? 0.958 : 1);
    }
  }

  // ------------------------------------------------------------------ pack model lots
  // The lots nearest the district heart give up their procedural box and its whole facade layer
  // for a real pack model. The swap is roughly triangle neutral or better, because one lot's
  // windows, frames, socle, parapet, shop band and balconies cost more than a low-detail model:
  // the measured delta per lot is reported at the end of this pass. Signs and murals are assigned
  // only to procedural lots, so no decal ever floats in front of a model wall.
  //
  // Every model is bucketed by its own name, one InstancedMesh per model and material, and a
  // bucket is only created for lots that actually took it, so a missing pack costs no draw call.
  const MODEL_WHITE = new THREE.Color(0xffffff);
  const modelTintScratch = new THREE.Color();
  const modelLots = [];
  const modelTowers = [];
  const modelVillas = [];
  const MODEL_APARTMENT_CORE = 4.6;      // the low-detail kits are 4 x 4 metres on the ground
  const MODEL_CORE_VARIANTS = isTouch ? 3 : 5;

  function packTint(out, hex, rnd, low, high) {
    out.setHex(hex);
    const m = Math.max(out.r, out.g, out.b) || 1;
    out.multiplyScalar(1 / m);           // keep the palette hue, restore full brightness
    out.lerp(MODEL_WHITE, 0.38);         // the pack texture already carries its own albedo
    return out.multiplyScalar(low + rnd * (high - low));
  }

  function modelFit(model, b, fillXZ) {
    const fp = model.footprint;
    if (!fp || fp.w <= 0 || fp.d <= 0 || fp.h <= 0) return null;
    const sxz = Math.min((b.w - 0.3) / fp.w, (b.d - 0.3) / fp.d) * fillXZ;
    if (!(sxz > 0.55)) return null;
    const target = b.top - b.gMin + 0.25;
    return { sxz: sxz, sy: target / fp.h, target: target };
  }

  let coreSet = [];
  let shopModel = null;
  let warehouseModel = null;
  let towerSet = [];
  let villaSet = [];
  if (assets) {
    const apartmentAll = assets.byCategory('apartment') || [];
    const coreAll = [];
    for (let i = 0; i < apartmentAll.length; i += 1) {
      const m = apartmentAll[i];
      if (m.footprint.w <= MODEL_APARTMENT_CORE && m.footprint.d <= MODEL_APARTMENT_CORE) {
        coreAll.push(m);
      }
    }
    // A stride over the 4 x 4 kits picks a spread of silhouettes instead of the first three
    // neighbours of the manifest, so the core never repeats one facade down a whole street.
    for (let i = 0; i < MODEL_CORE_VARIANTS && coreAll.length > 0; i += 1) {
      const m = coreAll[(i * 3 + 1) % coreAll.length];
      if (coreSet.indexOf(m) < 0) coreSet.push(m);
    }
    const shopList = assets.byCategory('shop') || [];
    shopModel = shopList.length > 0 ? shopList[0] : null;
    const warehouseList = assets.byCategory('warehouse') || [];
    warehouseModel = warehouseList.length > 0 ? warehouseList[0] : null;
    const towerList = assets.byCategory('tower') || [];
    for (let i = 0; i < towerList.length; i += 1) towerSet.push(towerList[i]);
    const villaList = assets.byCategory('villa') || [];
    for (let i = 0; i < villaList.length && villaSet.length < 3; i += 1) {
      const m = villaList[(i * 2) % villaList.length];
      if (villaSet.indexOf(m) < 0) villaSet.push(m);
    }
  }

  // Towers first: they are the tallest statement and the easiest swap to judge in the frame.
  {
    let count = 0;
    for (let bi = 0; bi < blds.length && count < MODEL_TOWER_MAX; bi += 1) {
      const b = blds[bi];
      if (!b.tower || towerSet.length === 0) continue;
      const model = towerSet[count % towerSet.length];
      const fit = modelFit(model, b, 0.9);
      if (!fit) continue;
      b.model = true;
      b.modelDef = {
        model: model,
        sxz: fit.sxz,
        sy: clamp(fit.sy, 1.0, 2.2),
        yaw: 0.03 * (hash2(bi * 13 + 3, 7) - 0.5),
        pitch: 0,
        tint: 0.9 + hash2(bi * 7 + 5, 11) * 0.12,
        y: b.gMin - 0.3,
        tower: true
      };
      modelTowers.push(b);
      count += 1;
    }
  }

  // Core lots, nearest (0, 4) first. A lot is accepted only when a variant fits its ground
  // footprint and its height can be reached without an absurd squash.
  {
    const candidates = [];
    for (let bi = 0; bi < blds.length; bi += 1) {
      const b = blds[bi];
      if (b.tower || b.model) continue;
      if (b.kind === 3) continue;
      const dx = b.cx;
      const dz = b.cz - 4;
      candidates.push({ b: b, bi: bi, d: dx * dx + dz * dz });
    }
    candidates.sort(function (a, b) { return a.d - b.d || a.bi - b.bi; });
    for (let i = 0; i < candidates.length && modelLots.length < MODEL_LOT_MAX; i += 1) {
      const b = candidates[i].b;
      const bi = candidates[i].bi;
      let chosen = null;
      let chosenFit = null;
      // A wide avenue plot can carry the 8 x 4 shop, a big back plot the warehouse. Both are
      // single district models, so the look stays controlled and the call stays one each.
      if (shopModel && b.kind === 1 && b.w >= 8.0 && b.d >= 4.4) {
        const fit = modelFit(shopModel, b, 0.94);
        if (fit) {
          chosen = shopModel;
          chosenFit = fit;
        }
      }
      if (!chosen && warehouseModel && b.kind === 0 && b.w >= 9.5 && b.d >= 4.6) {
        const fit = modelFit(warehouseModel, b, 0.94);
        if (fit) {
          chosen = warehouseModel;
          chosenFit = fit;
        }
      }
      if (!chosen) {
        const fits = [];
        for (let k = 0; k < coreSet.length; k += 1) {
          const m = coreSet[k];
          const fit = modelFit(m, b, 0.96);
          if (!fit) continue;
          if (fit.sy < 0.6 || fit.sy > 2.0) continue;
          fits.push({ m: m, fit: fit });
        }
        if (fits.length > 0) {
          // A hash, not the height optimum, decides which of the acceptable variants a lot
          // takes: a street of the one kit whose height matches the floor count exactly was
          // the first thing this pass measured, and it read as a factory row.
          const pickIndex = (hash2(bi * 41 + 7, 23) * fits.length) | 0;
          chosen = fits[pickIndex].m;
          chosenFit = fits[pickIndex].fit;
        }
      }
      if (!chosen) continue;
      if (chosenFit.sy < 0.55 || chosenFit.sy > 2.4) continue;
      b.model = true;
      b.modelDef = {
        model: chosen,
        sxz: chosenFit.sxz,
        sy: chosenFit.sy,
        yaw: 0.045 * (hash2(bi * 17 + 11, 3) - 0.5),
        pitch: 0.006 * (hash2(bi * 19 + 7, 5) - 0.5),
        tint: 0.86 + hash2(bi * 23 + 13, 9) * 0.2,
        y: b.gMin - 0.22,
        tower: false
      };
      modelLots.push(b);
    }
  }

  // Villas on the outer slopes: low houses among the blocks, not stretched apartment towers.
  // This step is last because it is the most expensive per lot; street tier only, and only when
  // the frame has the room left for it.
  if (villaSet.length > 0 && MODEL_VILLA_MAX > 0) {
    const outer = [];
    for (let bi = 0; bi < blds.length; bi += 1) {
      const b = blds[bi];
      if (b.kind !== 3 || b.tower || b.model) continue;
      if (b.w < 8.6 || b.d < 6.2) continue;
      const dx = b.cx;
      const dz = b.cz - 4;
      outer.push({ b: b, bi: bi, d: dx * dx + dz * dz });
    }
    outer.sort(function (a, b) { return a.d - b.d || a.bi - b.bi; });
    for (let i = 0; i < outer.length && modelVillas.length < MODEL_VILLA_MAX; i += 1) {
      const b = outer[i].b;
      const bi = outer[i].bi;
      let chosen = null;
      let chosenFit = null;
      const fits = [];
      for (let k = 0; k < villaSet.length; k += 1) {
        const m = villaSet[k];
        const fit = modelFit(m, b, 0.92);
        if (!fit) continue;
        if (fit.sy < 0.9) continue;
        fits.push({ m: m, fit: fit });
      }
      if (fits.length > 0) {
        const pickIndex = (hash2(bi * 43 + 11, 29) * fits.length) | 0;
        chosen = fits[pickIndex].m;
        chosenFit = fits[pickIndex].fit;
      }
      if (!chosen) continue;
      b.model = true;
      b.modelDef = {
        model: chosen,
        sxz: chosenFit.sxz,
        sy: clamp(chosenFit.sy, 1.0, 1.45),
        yaw: 0.06 * (hash2(bi * 29 + 3, 13) - 0.5),
        pitch: 0.008 * (hash2(bi * 31 + 5, 17) - 0.5),
        tint: 0.88 + hash2(bi * 37 + 7, 19) * 0.16,
        y: b.gMin - 0.2,
        tower: false
      };
      modelVillas.push(b);
    }
  }


  {
    const plazaQuarter = (function () {
      let best = -1;
      let bd = Infinity;
      for (let qi = 0; qi < qCount; qi += 1) {
        const dx = siteX[qi] - PLAZA_CX;
        const dz = siteZ[qi] - PLAZA_CZ;
        const d = dx * dx + dz * dz;
        if (d < bd) {
          bd = d;
          best = qi;
        }
      }
      return best;
    })();

    for (let qi = 0; qi < qCount; qi += 1) {
      const q = quads[qi];
      const list = bldsOfQuarter[qi];
      const cx = q.center.x;
      const cz = q.center.z;
      let roofTop = q.center.y + 9;
      let span = 12;
      for (let i = 0; i < list.length; i += 1) {
        const b = list[i];
        if (b.top > roofTop) roofTop = b.top;
        const dx = b.cx - cx;
        const dz = b.cz - cz;
        const r = Math.sqrt(dx * dx + dz * dz) + Math.sqrt(b.hu * b.hu + b.hv * b.hv);
        if (r > span) span = r;
      }
      // A quarter never claims more ground than the halfway line to its nearest neighbour.
      let nearest = 999;
      for (let oj = 0; oj < qCount; oj += 1) {
        if (oj === qi) continue;
        const ndx = quads[oj].center.x - cx;
        const ndz = quads[oj].center.z - cz;
        const nd = Math.sqrt(ndx * ndx + ndz * ndz);
        if (nd < nearest) nearest = nd;
      }
      if (span > nearest * 0.62) span = nearest * 0.62;
      q.radius = span;
      q.labelPosition.set(cx, roofTop + 7, cz);

      // Sign slots: wide facades facing a street, nearest the centre first.
      const order = [];
      for (let i = 0; i < list.length; i += 1) {
        const b = list[i];
        if (b.tower || b.model) continue;
        const dx = b.cx - cx;
        const dz = b.cz - cz;
        order.push({ b: b, d: dx * dx + dz * dz });
      }
      order.sort(function (a, b) { return a.d - b.d; });
      const slots = [];
      for (let i = 0; i < order.length && slots.length < 3; i += 1) {
        const b = order[i].b;
        if (b.w < 5.0) continue;
        if (b.top - b.gMax < 6) continue;
        let dup = false;
        for (let s = 0; s < slots.length && !dup; s += 1) {
          const p = slots[s].position;
          if (Math.abs(p.x - b.fx) < 16 && Math.abs(p.z - b.fz) < 16) dup = true;
        }
        if (dup) continue;
        slots.push({
          position: new THREE.Vector3(b.fx + b.nx * 0.28, b.gMax + 4.9, b.fz + b.nz * 0.28),
          rotationY: b.fYaw
        });
      }
      for (let s = 0; slots.length < 3 && s < 8; s += 1) {
        const a = (s / 8) * Math.PI * 2 + 0.37 * qi;
        const sx = cx + Math.cos(a) * (Math.min(q.radius, 26) + 3);
        const sz = cz + Math.sin(a) * (Math.min(q.radius, 26) + 3);
        slots.push({
          position: new THREE.Vector3(sx, groundHeightAt(sx, sz) + 4.7, sz),
          rotationY: Math.atan2(cx - sx, cz - sz)
        });
      }
      q.signSlots = slots;

      // Door points: pavements in front of the quarter's own facades, spread apart.
      const doors = [];
      for (let i = 0; i < order.length && doors.length < 5; i += 1) {
        const b = order[i].b;
        // Outward from the facade: the door opens onto the pavement, never into the house.
        const dx = b.fx + b.nx * b.doorInset;
        const dz = b.fz + b.nz * b.doorInset;
        let dup = false;
        for (let k = 0; k < doors.length && !dup; k += 1) {
          const p = doors[k];
          if (Math.abs(p.x - dx) < 21 && Math.abs(p.z - dz) < 21) dup = true;
        }
        if (dup) continue;
        doors.push(new THREE.Vector3(dx, groundHeightAt(dx, dz) + LIFT_STREET_PAVE, dz));
      }
      while (doors.length < 3) {
        const a = doors.length * 2.31 + 0.8;
        const dx = cx + Math.cos(a) * (Math.min(q.radius, 22) + 2);
        const dz = cz + Math.sin(a) * (Math.min(q.radius, 22) + 2);
        doors.push(new THREE.Vector3(dx, groundHeightAt(dx, dz), dz));
      }
      if (qi === plazaQuarter) {
        const extra = [[PLAZA_CX - 9, PLAZA_CZ + 3], [PLAZA_CX + 9, PLAZA_CZ - 3]];
        for (let i = 0; i < extra.length && doors.length < 6; i += 1) {
          const dx = extra[i][0];
          const dz = extra[i][1];
          doors.push(new THREE.Vector3(dx, groundHeightAt(dx, dz) + LIFT_PLAZA, dz));
        }
      }
      q.doorPoints = doors;

      // A crane stands in one of the quarter's courtyards, always inside the built fabric.
      let craneX = cx;
      let craneZ = cz;
      for (let k = 0; k < 400; k += 1) {
        const a = k * 2.399;
        const rr = 2 + (k / 400) * Math.max(10, Math.min(q.radius + 4, 38));
        const x = cx + Math.cos(a) * rr;
        const z = cz + Math.sin(a) * rr;
        if (!inUrban(x, z)) continue;
        if (flagsAt(x, z) & B_OLD) continue;
        if (diskFree(x, z, 2.2, B_SOLID)) {
          craneX = x;
          craneZ = z;
          break;
        }
      }
      markDisk(craneX, craneZ, 2, B_PROP);
      q.craneSlot.position.set(craneX, groundHeightAt(craneX, craneZ), craneZ);
    }
  }

  // ------------------------------------------------------------------ the stone crown
  // The very top of the hill keeps a small old village and nothing else: three narrow lanes,
  // stone houses tight on both sides, cypresses, and the church with its bell tower. It has to
  // read as a pale crown on the summit, not as a town, so it covers about 19 by 23 metres and
  // the rest of the crest is contemporary. Its ground, and the church's, were claimed on the
  // occupancy grid before the street grid and the parcels, so nothing contemporary enters it.

  // ------------------------------------------------------------------ houses
  // A house is two instanced pieces plus one baked piece. The instanced body is four walls and
  // the two plaster gable ends, tinted by the instance colour, which is the wall wash. The two
  // tile slopes are baked per house into the static props mesh, so an instanced house can carry
  // a roof colour that has nothing to do with its wall colour: a single instance colour cannot
  // tint walls and roof differently, and a second instanced mesh would cost a draw call.
  // Only the crown uses this house type: nowhere else in the city is pitched.
  const HOUSE_RIDGE = 1.34;         // ridge height as a fraction of the wall height
  const HOUSE_SLOPE_N = 0.78;       // roof plane normal, in unit house space

  function houseQuad(b, ax, ay, az, bx, by, bz, cx, cy, cz, dx, dy, dz, nx, ny, nz, col) {
    const i0 = b.vert(ax, ay, az, nx, ny, nz, col);
    const i1 = b.vert(bx, by, bz, nx, ny, nz, col);
    const i2 = b.vert(cx, cy, cz, nx, ny, nz, col);
    const i3 = b.vert(dx, dy, dz, nx, ny, nz, col);
    b.idx(i0, i1, i2);
    b.idx(i0, i2, i3);
  }

  function houseTri(b, ax, ay, az, bx, by, bz, cx, cy, cz, nx, ny, nz, col) {
    const i0 = b.vert(ax, ay, az, nx, ny, nz, col);
    const i1 = b.vert(bx, by, bz, nx, ny, nz, col);
    const i2 = b.vert(cx, cy, cz, nx, ny, nz, col);
    b.idx(i0, i1, i2);
  }

  const houseGeo = (function () {
    const b = makeBuilder();
    const wall = new THREE.Color(1, 1, 1);
    const side = new THREE.Color(0.87, 0.87, 0.87);
    const R = HOUSE_RIDGE;
    // Four walls, y from 0 to 1, footprint in [-0.5, 0.5].
    houseQuad(b, -0.5, 0, 0.5, 0.5, 0, 0.5, 0.5, 1, 0.5, -0.5, 1, 0.5, 0, 0, 1, wall);
    houseQuad(b, 0.5, 0, -0.5, -0.5, 0, -0.5, -0.5, 1, -0.5, 0.5, 1, -0.5, 0, 0, -1, wall);
    houseQuad(b, 0.5, 0, 0.5, 0.5, 0, -0.5, 0.5, 1, -0.5, 0.5, 1, 0.5, 1, 0, 0, side);
    houseQuad(b, -0.5, 0, -0.5, -0.5, 0, 0.5, -0.5, 1, 0.5, -0.5, 1, -0.5, -1, 0, 0, side);
    // Plaster gable ends up to the ridge, which the baked roof meets exactly.
    houseTri(b, 0.5, 1, 0.5, -0.5, 1, 0.5, 0, R, 0.5, 0, 0, 1, wall);
    houseTri(b, -0.5, 1, -0.5, 0.5, 1, -0.5, 0, R, -0.5, 0, 0, -1, wall);
    return b.build();
  })();

  // The two tile slopes, same unit space, eaves at y = 1. Baked once per house.
  const houseRoofGeo = (function () {
    const b = makeBuilder();
    const tile = new THREE.Color(1, 1, 1);
    const R = HOUSE_RIDGE;
    const N = HOUSE_SLOPE_N;
    houseQuad(b, 0.5, 1, 0.5, 0.5, 1, -0.5, 0, R, -0.5, 0, R, 0.5, N, 0.63, 0, tile);
    houseQuad(b, -0.5, 1, -0.5, -0.5, 1, 0.5, 0, R, 0.5, 0, R, -0.5, -N, 0.63, 0, tile);
    return b.build();
  })();

  const houseMat = new THREE.MeshStandardMaterial({
    vertexColors: true,
    roughness: 0.8,
    metalness: 0.02
  });
  const houses = new THREE.InstancedMesh(houseGeo, houseMat, HOUSE_CAP);
  houses.castShadow = shadowsOn;
  houses.receiveShadow = shadowsOn;
  houses.name = 'houses';
  houses.frustumCulled = false;
  group.add(houses);
  nightCull.push(houses);

  // Stone palettes for the crown: grey-beige walls under darker, older tile. Every other
  // building in the district is plaster-faced with a flat roof. The walls keep their pale stone
  // value and take a third more chroma than the district stone, because at this distance a
  // 0.14 chroma wall reads as the one grey note left in a coloured hilltop, and the tile takes
  // more again: a roof is the largest visible face of a crown house from this camera.
  const OLD_WALLS = [0xcfbe8f, 0xc4b587, 0xd6c699, 0xb8a778, 0xc9b98b, 0xbfaf85];
  const OLD_ROOFS = [0x6b3e2c, 0x5f433b, 0x743926, 0x564e44, 0x7b4029];

  let houseCursor = 0;
  const houseSpots = [];
  function addHouse(x, z, w, d, wallH, yaw, col, roofCol) {
    if (houseCursor >= HOUSE_CAP) return false;
    const cs = Math.cos(yaw);
    const sn = Math.sin(yaw);
    let gMin = Infinity;
    let gMax = -Infinity;
    for (let k = 0; k < 4; k += 1) {
      const lu = ((k & 1) ? 1 : -1) * w * 0.5;
      const lv = ((k & 2) ? 1 : -1) * d * 0.5;
      const px = x + lu * cs + lv * sn;
      const pz = z - lu * sn + lv * cs;
      const g = groundHeightAt(px, pz);
      if (g < gMin) gMin = g;
      if (g > gMax) gMax = g;
    }
    const base = gMin - 0.5;
    const h = wallH + (gMax - gMin) + 1.0;
    dummy.position.set(x, base, z);
    dummy.rotation.set(0, yaw, 0);
    dummy.scale.set(w, h, d);
    dummy.updateMatrix();
    houses.setMatrixAt(houseCursor, dummy.matrix);
    houses.setColorAt(houseCursor, col);
    houseCursor += 1;
    // The footprint, not a disc around it: a terrace shares its party walls, so the house line
    // has to be exactly the cells the walls stand on and no wider.
    rectScan(x, z, cs, -sn, w * 0.5, d * 0.5, 0, B_HOUSE);
    houseSpots.push({
      x: x, y: base, z: z, w: w, h: h, d: d, yaw: yaw, roof: roofCol,
      gMin: gMin, gMax: gMax, base: base,
      hx: Math.abs(cs) * w * 0.5 + Math.abs(sn) * d * 0.5,
      hz: Math.abs(sn) * w * 0.5 + Math.abs(cs) * d * 0.5
    });
    return true;
  }

  // The cypresses the churchyard keeps: one at each alley's west end, where the lane runs out
  // against the church wall, and the row behind the apse that gives the summit its silhouette
  // from the district tier. The apse row stands clear of the apse wall, and every alley cypress
  // stands in the lane, a metre clear of the house walls on either side. Their ground is
  // claimed before the first house, so no wall can arrive on top of one; the planting pass
  // below reuses this table.
  const CROWN_CYPRESS = [
    [-35.2, -109.6], [-35.2, -103.4], [-35.2, -97.2],
    [-40.5, -120.3], [-44.0, -121.1], [-47.5, -120.3]
  ];
  for (let i = 0; i < CROWN_CYPRESS.length; i += 1) {
    const cx = CROWN_CYPRESS[i][0];
    const cz = CROWN_CYPRESS[i][1];
    if (flagsAt(cx, cz) & B_ROADS) continue;
    markDisk(cx, cz, 1.25, B_CYPRESS);
  }

  // What a stone house must not touch: every solid bit of the grid except B_HOUSE, which only
  // its own terrace may share, and B_OLD, which is the village's own ground. The alley a house
  // fronts is painted and never flagged, so it is absent by construction; its walkable width is
  // a rectangle test instead. A churchyard cypress's disc is a stop of its own.
  const CROWN_FORBID = (B_SOLID & ~B_HOUSE) | B_CYPRESS;

  // One stone house against one tree crown. The crown is a squashed bipyramid: unit y from 1.7
  // to 6.7 with its widest rim at 4.2 and radius 2.1, scaled by the tree's own instance. The
  // house is a box from its buried base to the ridge, so the test is the horizontal distance
  // from the trunk to the wall against the crown's radius at the height the two share. A plane
  // tree six metres wide standing beside a gable really does brush it, and the summit is planted
  // densely, so this is what keeps branches out of the stonework.
  function treeHitsHouse(x, z, sx, sy, ty, hs) {
    const dx = Math.max(Math.abs(x - hs.x) - hs.hx, 0);
    const dz = Math.max(Math.abs(z - hs.z) - hs.hz, 0);
    const d = Math.sqrt(dx * dx + dz * dz);
    if (d >= 2.1 * sx) return false;
    const lo = Math.max(ty + 1.7 * sy, hs.base);
    const hi = Math.min(ty + 6.7 * sy, hs.base + 1.34 * hs.h);
    if (lo >= hi) return false;
    const rim = ty + 4.2 * sy;
    const y = rim < lo ? lo : rim > hi ? hi : rim;
    const uy = (y - ty) / sy;
    const rr = uy <= 4.2 ? 2.1 * (uy - 1.7) / 2.5 : 2.1 * (6.7 - uy) / 2.5;
    return d < rr * sx;
  }

  {
    const rnd = mulberry(778899);
    const stoneCol = new THREE.Color();
    const stoneRoof = new THREE.Color();

    // The house's own footprint against the alley paving: a rectangle test, so the row beside a
    // lane passes at a hand's width while a wall across the lane does not.
    function laneOverlap(x, z, hx, hz) {
      for (let l = 0; l < OLD_LANES.length; l += 1) {
        const lane = OLD_LANES[l];
        if (x + hx > OLD_PAVE_X0 && x - hx < OLD_PAVE_X1 &&
          z + hz > lane - OLD_ALLEY && z - hz < lane + OLD_ALLEY) return true;
      }
      return false;
    }

    // One house: the footprint decided first, then the ground it stands on. The test is the
    // exact wall rectangle against the forbidden cells, the alley paving, and every house
    // already placed, so a terrace can abut on its party line but nothing overlaps. The half
    // extents are the world-axis ones, which for the two yaws used here are exact.
    function crownHouse(x, z, w, d, wallH, yaw) {
      const cs = Math.cos(yaw);
      const sn = Math.sin(yaw);
      const hx = Math.abs(cs) * w * 0.5 + Math.abs(sn) * d * 0.5;
      const hz = Math.abs(sn) * w * 0.5 + Math.abs(cs) * d * 0.5;
      if (!rectScan(x, z, cs, -sn, w * 0.5, d * 0.5, CROWN_FORBID, 0)) return false;
      if (laneOverlap(x, z, hx, hz)) return false;
      for (let i = 0; i < houseSpots.length; i += 1) {
        const hs = houseSpots[i];
        if (Math.abs(x - hs.x) < hx + hs.hx - 0.02 &&
          Math.abs(z - hs.z) < hz + hs.hz - 0.02) return false;
      }
      stoneCol.setHex(OLD_WALLS[Math.floor(rnd() * OLD_WALLS.length)]);
      stoneCol.multiplyScalar(0.95 + rnd() * 0.1);
      stoneRoof.setHex(OLD_ROOFS[Math.floor(rnd() * OLD_ROOFS.length)]);
      stoneRoof.multiplyScalar(0.9 + rnd() * 0.16);
      return addHouse(x, z, w, d, wallH, yaw, stoneCol, stoneRoof);
    }

    // A terrace fills its row from one wall line to the other: houses of 3.1 to 4.7 metres of
    // frontage, abutting on a shared wall line, with the last one taking exactly the slack. The
    // width bounds are made feasible for the slots that remain, so the row is continuous with no
    // gap and no overlap, and every height is drawn in 2.7 to 4.6 metres so the roofline steps
    // along the street instead of running level. A fixed depth keeps the avenue terrace's two
    // wall lines straight.
    function fillRow(alongX, coord, from, to, slots, depth) {
      let t = from;
      for (let k = 0; k < slots; k += 1) {
        const left = slots - k;
        const rem = to - t;
        const lo = Math.max(3.1, rem - 4.7 * (left - 1));
        const hi = Math.min(4.7, rem - 3.1 * (left - 1));
        const w = k === slots - 1 ? rem : lo + rnd() * (hi - lo);
        const d = depth > 0 ? depth : 3.1 + rnd() * 0.8;
        const h = 2.7 + rnd() * 1.9;
        const c = t + w * 0.5;
        if (alongX) crownHouse(c, coord, w, d, h, 0);
        else crownHouse(coord, c, w, d, h, Math.PI * 0.5);
        t += w;
      }
    }

    // Three alleys, stone houses pressed against both sides: a cluster, not a grid. Each strip
    // of houses backs onto the next lane, six metres across, which is the narrowest a cart could
    // still pass. The avenue terrace closes the block on the kerb side, a wall of gables over
    // the road that climbs to the village, so the crown reads as a village edge and not as an
    // island of huts.
    for (let l = 0; l < OLD_LANES.length; l += 1) {
      for (let side = -1; side <= 1; side += 2) {
        fillRow(true, OLD_LANES[l] + side * OLD_ROW, OLD_ROW_X0, OLD_ROW_X1, 5, 0);
      }
    }
    fillRow(false, (OLD_ROW_X1 + OLD_CROSS_X1) * 0.5, OLD_Z_END0, OLD_Z_END1, 7, OLD_CROSS_D);
  }
  houses.count = houseCursor;
  if (houses.instanceColor) houses.instanceColor.needsUpdate = true;
  houses.instanceMatrix.needsUpdate = true;

  // ------------------------------------------------------------------ olive groves (data)
  // Chosen after the houses so a grove never swallows a garden, and before the terrain so the
  // soil under each grove can be tinted.
  const groves = [];
  const nearGroves = [];
  {
    const rnd = mulberry(77031);
    for (let tries = 0; tries < 900 && groves.length < 26; tries += 1) {
      const a = rnd() * Math.PI * 2;
      const d = 118 + rnd() * 78;
      const x = Math.cos(a) * d * 1.06;
      const z = Math.sin(a) * d + 22;
      const r = 10 + rnd() * 11;
      if (x > 100 && x < 138 && z > -118 && z < 112) continue;
      if (!diskFree(x, z, r * 0.8, B_SOLID)) continue;
      let ok = true;
      for (let g = 0; g < groves.length && ok; g += 1) {
        const dx = x - groves[g].x;
        const dz = z - groves[g].z;
        if (dx * dx + dz * dz < (r + groves[g].r + 5) * (r + groves[g].r + 5)) ok = false;
      }
      if (ok) groves.push({ x: x, z: z, r: r });
    }
    // The near slope's own groves. The bottom third of a phone frame is the ground 80 to 270
    // metres in front of the ring road, where one vertex of the terrain grid is 500 pixels
    // wide: at that scale the ground cover has to be a row of canopies and a plot boundary,
    // not a field colour. They are kept in their own list because they are planted last, after
    // the far groves: on a desktop the tree cap is already full, so the near slope simply gets
    // no trees there rather than stealing them from the far rows that carry the top of the
    // frame, while the touch profile has room for them.
    // The portrait frame is only a hundred and ten metres wide where the near slope is, so the
    // rows are laid inside that corridor rather than across the whole hillside: a grove planted
    // at a hundred and thirty metres out is a grove nobody ever sees on a phone, and the desktop
    // frame already carries the outer ones.
    const NEAR = [
      [-50, 134], [-22, 128], [8, 138], [38, 132], [-38, 150], [18, 160],
      [48, 172], [-8, 168], [30, 190], [-46, 196], [-16, 210], [42, 214],
      [12, 232], [-34, 240], [28, 255], [-2, 268], [52, 258], [-52, 272],
      [20, 290], [-24, 300], [44, 312], [8, 325], [-96, 196], [104, 252],
      [-118, 268], [130, 300],
      // The strip the phone camera stands over: forty odd metres inside the frame at z 220, so
      // these are the canopies the bottom tenth of the portrait frame is made of.
      [-40, 222], [-20, 228], [0, 224], [22, 230], [42, 226], [-46, 244],
      [-24, 250], [-2, 246], [24, 252], [44, 248], [-40, 264], [-16, 268],
      [10, 264], [32, 270], [50, 266],
      [-36, 236], [-18, 238], [0, 236], [18, 240], [36, 238], [-28, 252],
      [-8, 254], [12, 256], [30, 258], [-44, 270], [-2, 276], [24, 282],
      [46, 272], [-22, 286], [6, 292]
    ];
    for (let i = 0; i < NEAR.length; i += 1) {
      const x = NEAR[i][0];
      const z = NEAR[i][1];
      const r = 9 + hash2(Math.round(x), Math.round(z)) * 5;
      // A grove only needs a free metre to stand in: the block test in the planting loop is what
      // keeps the canopies off the road, the pads and the kerbs. Asking for a clear disc the size
      // of the grove dropped nearly every row the near slope had.
      if (!diskFree(x, z, 1.2, B_SOLID)) continue;
      let ok = true;
      for (let g = 0; g < groves.length && ok; g += 1) {
        const dx = x - groves[g].x;
        const dz = z - groves[g].z;
        if (dx * dx + dz * dz < (r + groves[g].r + 5) * (r + groves[g].r + 5)) ok = false;
      }
      for (let g = 0; g < nearGroves.length && ok; g += 1) {
        const dx = x - nearGroves[g].x;
        const dz = z - nearGroves[g].z;
        if (dx * dx + dz * dz < (r + nearGroves[g].r + 5) * (r + nearGroves[g].r + 5)) ok = false;
      }
      if (ok) nearGroves.push({ x: x, z: z, r: r });
    }
  }

  // ------------------------------------------------------------------ static props mesh
  // Merged vertex-coloured pieces that are not worth instancing: the fountain, the rail depot,
  // the roundabout island planter, the church with its bell tower, the water tower, every civic
  // building in the outer belt, the villas and the terrace retaining walls.
  const LANE_PATH = [
    [-84, 116], [-52, 132], [-14, 150], [-46, 172], [-6, 192], [-56, 214],
    [-10, 236], [-68, 258], [-24, 282], [-78, 306]
  ];

  // Metres from a point to the farm lane's centre line, for the passes that are not allowed to
  // build on it: the terrace walls break where the track climbs through them.
  function laneDistance(x, z) {
    let best = 1e9;
    for (let s = 0; s < LANE_PATH.length - 1; s += 1) {
      const ax = LANE_PATH[s][0];
      const az = LANE_PATH[s][1];
      const dx = LANE_PATH[s + 1][0] - ax;
      const dz = LANE_PATH[s + 1][1] - az;
      const len2 = dx * dx + dz * dz;
      let t = len2 > 0 ? ((x - ax) * dx + (z - az) * dz) / len2 : 0;
      if (t < 0) t = 0;
      else if (t > 1) t = 1;
      const px = ax + dx * t - x;
      const pz = az + dz * t - z;
      const d2 = px * px + pz * pz;
      if (d2 < best) best = d2;
    }
    return Math.sqrt(best);
  }

  const statics = makeBuilder();

  const CIV_WALL = new THREE.Color(0xe7e2d6);
  const CIV_WALL2 = new THREE.Color(0xd7cfbe);
  const CIV_GLASS = new THREE.Color(0x3b515c);
  const CIV_STEEL = new THREE.Color(0xb3b7b8);
  const CIV_ROOF = new THREE.Color(0x8a8d8e);
  const CIV_ACCENT = new THREE.Color(0xc2603a);
  const CIV_TURF = new THREE.Color(0x5d8a45);
  const CIV_POOL = new THREE.Color(0x5cc0d4);
  const CIV_SOLAR = new THREE.Color(0x25354a);
  const WALL_STONE = new THREE.Color(0xbdae90);
  const WALL_CAP = new THREE.Color(0xd8ccb2);

  {
    const fx = PLAZA_CX;
    const fz = PLAZA_CZ;
    const fy = groundHeightAt(fx, fz) + LIFT_PLAZA;
    const basin = new THREE.CylinderGeometry(3.4, 3.6, 0.8, 22);
    const water = new THREE.CylinderGeometry(3.05, 3.05, 0.1, 22);
    const column = new THREE.CylinderGeometry(0.36, 0.48, 2.0, 10);
    const bowl = new THREE.CylinderGeometry(1.2, 0.55, 0.38, 14);
    const bowlWater = new THREE.CylinderGeometry(1.02, 1.02, 0.06, 14);
    statics.addGeometry(basin, composeMatrix(fx, fy + 0.4, fz, 0, 1, 1, 1), new THREE.Color(0xdccfb3));
    statics.addGeometry(water, composeMatrix(fx, fy + 0.72, fz, 0, 1, 1, 1), new THREE.Color(0x5aa2c8));
    statics.addGeometry(column, composeMatrix(fx, fy + 1.4, fz, 0, 1, 1, 1), new THREE.Color(0xd2c4a6));
    statics.addGeometry(bowl, composeMatrix(fx, fy + 2.45, fz, 0, 1, 1, 1), new THREE.Color(0xdccfb3));
    statics.addGeometry(bowlWater, composeMatrix(fx, fy + 2.62, fz, 0, 1, 1, 1), new THREE.Color(0x6fb2d4));
  }

  // Rail depot: a low maintenance shed east of the running lines, with a photovoltaic roof and
  // two roller doors onto the yard side.
  {
    const sx = RAIL_X + 11.5;
    const sz = -78;
    const gB = groundHeightAt(sx, sz - 15);
    const gF = groundHeightAt(sx, sz + 15);
    const g = Math.min(gB, gF) - 0.6;
    const h = Math.max(gB, gF) - Math.min(gB, gF) + 7.2;
    statics.addGeometry(unitBox, composeMatrix(sx, g + h * 0.5, sz, 0, 12, h, 30), CIV_WALL2);
    statics.addGeometry(unitBox, composeMatrix(sx - 6.15, g + 3.1, sz - 7, 0, 0.3, 5.2, 6.4), CIV_GLASS);
    statics.addGeometry(unitBox, composeMatrix(sx - 6.15, g + 3.1, sz + 7, 0, 0.3, 5.2, 6.4), CIV_GLASS);
    statics.addGeometry(unitBox, composeMatrix(sx, g + h + 0.28, sz, 0, 12.5, 0.36, 30.5), CIV_ROOF);
    statics.addGeometry(unitBox, composeMatrix(sx + 0.5, g + h + 0.78, sz, 0, 10.4, 0.16, 27.4, -0.1), CIV_SOLAR);
    statics.addGeometry(unitBox, composeMatrix(sx, g + h + 2.1, sz - 11, 0, 5.2, 3.2, 5.2), CIV_WALL);
    statics.addGeometry(unitBox, composeMatrix(sx - 6.6, g + 0.6, sz, 0, 1.6, 1.2, 26), WALL_CAP);
  }

  {
    const C = roundaboutCenter;
    const kerb = new THREE.CylinderGeometry(ISLAND_R, ISLAND_R + 0.1, 0.55, 28);
    const soil = new THREE.CylinderGeometry(ISLAND_R - 0.35, ISLAND_R - 0.35, 0.62, 28);
    statics.addGeometry(kerb, composeMatrix(C.x, C.y + 0.28, C.z, 0, 1, 1, 1), new THREE.Color(0xd3c7ad));
    statics.addGeometry(soil, composeMatrix(C.x, C.y + 0.31, C.z, 0, 1, 1, 1), new THREE.Color(0x6e9446));
  }

  // Hilltop landmarks. The church keeps its nave and apse on the crest west of the avenue's
  // terminus and gains a campanile engaged at the nave's north-west corner: 30 units of pale
  // stone under a small dark pyramid, the only vertical on the west half of the hilltop that is
  // tall enough to read from district tier over the roofscape.
  {
    const cx = -44;
    const cz = -104;
    const cy = groundHeightAt(cx, cz);
    const stone = new THREE.Color(0xe4dcc6);
    const stoneDark = new THREE.Color(0xcfc4a8);
    const roof = new THREE.Color(0xb05f42);
    const dark = new THREE.Color(0x35302a);
    const cap = new THREE.Color(0x4a4038);
    // Nave.
    statics.addGeometry(unitBox, composeMatrix(cx, cy + 4.4, cz, 0.06, 14, 8.8, 22), stone);
    statics.addGeometry(unitBox, composeMatrix(cx, cy + 9.1, cz, 0.06, 15.0, 1.9, 23.2), roof);
    // Apse.
    statics.addGeometry(unitBox, composeMatrix(cx, cy + 3.4, cz - 12.4, 0.06, 8.4, 6.8, 5.2), stoneDark);
    // Campanile: a slender shaft standing inside the nave's own footprint, so it collides with
    // nothing the church did not already occupy.
    const tx = cx - 5.0;
    const tz = cz - 6.0;
    statics.addGeometry(unitBox, composeMatrix(tx, cy + 13.0, tz, 0, 3.8, 26, 3.8), stone);
    // A stepped base and a cornice, which is what gives the shaft its slenderness.
    statics.addGeometry(unitBox, composeMatrix(tx, cy + 1.1, tz, 0, 4.5, 2.2, 4.5), stoneDark);
    statics.addGeometry(unitBox, composeMatrix(tx, cy + 26.5, tz, 0, 4.35, 1.0, 4.35), stoneDark);
    statics.addGeometry(unitBox, composeMatrix(tx, cy + 27.4, tz, 0, 3.9, 0.8, 3.9), stone);
    // Belfry openings on the sunlit and the valley-facing sides.
    statics.addGeometry(unitBox, composeMatrix(tx + 1.88, cy + 23.2, tz, 0, 0.3, 2.8, 1.9), dark);
    statics.addGeometry(unitBox, composeMatrix(tx, cy + 23.2, tz + 1.88, 0, 1.9, 2.8, 0.3), dark);
    // Small dark pyramid over the drum: the spire tops out at 30 units above the hilltop.
    const spire = new THREE.ConeGeometry(2.95, 2.4, 4);
    statics.addGeometry(spire, composeMatrix(tx, cy + 28.8, tz, Math.PI * 0.25, 1, 1, 1), cap);
    spire.dispose();
    // Steps up to the door.
    statics.addGeometry(unitBox, composeMatrix(cx, cy + 0.4, cz + 11.6, 0.06, 12, 0.5, 2.0), stoneDark);
  }

  // The water tower east of the avenue's terminus: four braced legs and a round tank, 21 units
  // to the vent. It stands north of the z = -100 street and well clear of the glass towers,
  // which run from z = -78 south, so it breaks the hilltop skyline without hiding behind them.
  {
    const wx = 76;
    const wz = -98;
    const wy = groundHeightAt(wx, wz);
    const concrete = new THREE.Color(0xded6c4);
    const steel = new THREE.Color(0xc8c2b4);
    const band = new THREE.Color(0x9f9c93);
    const LEG = 11.5;
    for (let k = 0; k < 4; k += 1) {
      const lx = wx + ((k & 1) ? 2.4 : -2.4);
      const lz = wz + ((k & 2) ? 2.4 : -2.4);
      statics.addGeometry(unitBox, composeMatrix(lx, wy + LEG * 0.5, lz, 0, 0.9, LEG, 0.9), steel);
    }
    // Cross bracing on the two long faces, so the legs read as a frame and not four posts.
    for (let k = 0; k < 2; k += 1) {
      const lx = wx + (k ? 2.4 : -2.4);
      statics.addGeometry(unitBox, composeMatrix(lx, wy + LEG * 0.5, wz, 0, 0.22, 12.4, 0.22, 0.4), steel);
      statics.addGeometry(unitBox, composeMatrix(lx, wy + LEG * 0.5, wz, 0, 0.22, 12.4, 0.22, -0.4), steel);
    }
    // Tank, rim, conical cap and vent.
    const tank = new THREE.CylinderGeometry(3.5, 3.5, 6.4, 16);
    statics.addGeometry(tank, composeMatrix(wx, wy + LEG + 3.2, wz, 0, 1, 1, 1), concrete);
    tank.dispose();
    const rim = new THREE.CylinderGeometry(3.62, 3.62, 0.35, 16);
    statics.addGeometry(rim, composeMatrix(wx, wy + LEG + 6.25, wz, 0, 1, 1, 1), band);
    rim.dispose();
    const lid = new THREE.ConeGeometry(3.8, 2.2, 16);
    statics.addGeometry(lid, composeMatrix(wx, wy + LEG + 7.5, wz, 0, 1, 1, 1), steel);
    lid.dispose();
    statics.addGeometry(unitBox, composeMatrix(wx, wy + 20.5, wz, 0, 0.7, 0.9, 0.7), band);
  }

  // ------------------------------------------------------------------ civic buildings
  // Contemporary volumes in the outer belt: flat roofs, glazed strips, hardstanding. Each plot
  // was reserved on the occupancy grid, so every piece lands on free ground.
  function plotGround(cx, cz, yaw, w, d) {
    const c = Math.cos(yaw);
    const s = Math.sin(yaw);
    let gMin = Infinity;
    let gMax = -Infinity;
    for (let k = 0; k < 4; k += 1) {
      const lu = ((k & 1) ? 1 : -1) * w * 0.5;
      const lv = ((k & 2) ? 1 : -1) * d * 0.5;
      const g = groundHeightAt(cx + lu * c + lv * s, cz - lu * s + lv * c);
      if (g < gMin) gMin = g;
      if (g > gMax) gMax = g;
    }
    return { gMin: gMin, gMax: gMax };
  }

  function civicBox(r, lu, lv, y, w, h, d, col, rotX) {
    const c = Math.cos(r.yaw);
    const s = Math.sin(r.yaw);
    statics.addGeometry(unitBox,
      composeMatrix(r.x + lu * c + lv * s, y, r.z - lu * s + lv * c, r.yaw, w, h, d, rotX), col);
  }

  const civicGround = [];
  for (let i = 0; i < civic.length; i += 1) {
    civicGround.push(plotGround(civic[i].x, civic[i].z, civic[i].yaw, civic[i].w, civic[i].d));
  }

  for (let i = 0; i < civic.length; i += 1) {
    const r = civic[i];
    const g = civicGround[i].gMin - 0.7;
    if (r.type === 'mart') {
      civicBox(r, 0, 5, g + 4.2, 33, 8.4, 20, CIV_WALL);
      civicBox(r, 0, 5, g + 1.3, 33.4, 2.6, 20.4, CIV_GLASS);
      civicBox(r, 0, 5, g + 8.6, 33.8, 0.7, 20.8, CIV_ACCENT);
      civicBox(r, 0, 5, g + 9.4, 10, 1.4, 7, CIV_ROOF);
      civicBox(r, -8.5, 5, g + 9.25, 12, 0.2, 8, CIV_SOLAR);
      civicBox(r, 0, -5.4, g + 4.9, 12, 0.5, 3.2, CIV_ROOF);
      civicBox(r, 0, -5.4, g + 2.5, 0.9, 4.8, 0.9, CIV_STEEL);
    } else if (r.type === 'fuel') {
      civicBox(r, 0, 5.5, g + 2.3, 10, 4.6, 7.5, CIV_WALL);
      civicBox(r, 0, 5.5, g + 1.1, 10.4, 2.2, 7.9, CIV_GLASS);
      civicBox(r, 0, -2, g + 5.65, 16, 0.7, 12, CIV_WALL);
      civicBox(r, 0, -2, g + 5.3, 16.6, 0.45, 12.6, CIV_ACCENT);
      for (let k = 0; k < 4; k += 1) {
        civicBox(r, (k & 1) ? 5.6 : -5.6, -2 + ((k & 2) ? 4.4 : -4.4), g + 2.5, 0.5, 5.0, 0.5, CIV_STEEL);
      }
      for (let k = -1; k <= 1; k += 1) {
        civicBox(r, k * 4.4, -2, g + 0.16, 3.4, 0.32, 1.4, CIV_WALL2);
        civicBox(r, k * 4.4, -2, g + 1.0, 0.7, 1.6, 0.7, CIV_STEEL);
      }
      civicBox(r, 7.4, -8, g + 3.1, 1.5, 6.2, 1.0, CIV_ACCENT);
      civicBox(r, 7.4, -8, g + 5.6, 1.9, 1.4, 1.2, CIV_WALL);
    } else if (r.type === 'school') {
      // The hall is thirty metres, not thirty-four: a forty-six metre plot is a chord too wide
      // for the ring's lobes, and the school could not find a station anywhere on the frontage.
      // Thirty by thirteen inside a forty by thirty-six plot sits on the south-east arc where
      // the kerb is gentle, which is where a school would be sited anyway.
      civicBox(r, 0, 8, g + 5.1, 30, 10.2, 13, CIV_WALL);
      civicBox(r, 0, 8, g + 1.3, 30.4, 2.6, 13.4, CIV_GLASS);
      civicBox(r, 0, 8, g + 10.5, 30.6, 0.6, 13.6, CIV_ACCENT);
      civicBox(r, 0, 8, g + 11.4, 8, 1.4, 6, CIV_ROOF);
      civicBox(r, -13, 0, g + 3.6, 12, 7.2, 12, CIV_WALL2);
      civicBox(r, 2, 1.2, g + 5.7, 5, 11.4, 5, CIV_WALL2);
      civicBox(r, 2, 1.2, g + 12.0, 5.6, 0.6, 5.6, CIV_ROOF);
      civicBox(r, -13, -17, g + 1.1, 0.18, 2.2, 1.1, CIV_WALL);
      civicBox(r, 13, -17, g + 1.1, 0.18, 2.2, 1.1, CIV_WALL);
    } else if (r.type === 'sport') {
      civicBox(r, 0, 8, g + 6.6, 30, 13.2, 20, CIV_WALL2);
      civicBox(r, 0, 8, g + 1.4, 30.4, 2.8, 20.4, CIV_GLASS);
      civicBox(r, 0, 8, g + 13.5, 31, 0.7, 21, CIV_ROOF);
      civicBox(r, -6, 8, g + 13.9, 12, 0.18, 6, CIV_SOLAR);
      for (let k = 0; k < 3; k += 1) {
        const sh = 1.2 + k * 1.2;
        civicBox(r, 0, -1.6 - k * 2.3, g + sh * 0.5, 24, sh, 2.2, CIV_WALL2);
      }
      civicBox(r, 0, -7.0, g + 5.6, 24, 0.5, 6.4, CIV_ROOF);
      for (let k = 0; k < 4; k += 1) {
        civicBox(r, (k & 1) ? 11 : -11, -7.0, g + 2.8, 0.4, 5.6, 0.4, CIV_STEEL);
      }
      for (let k = 0; k < 4; k += 1) {
        const lu = (k & 1) ? 16 : -16;
        const lv = (k & 2) ? 12 : -14;
        civicBox(r, lu, lv, g + 6, 0.4, 12, 0.4, CIV_STEEL);
        civicBox(r, lu, lv, g + 12.2, 2.6, 0.5, 0.9, CIV_ROOF);
      }
    } else if (r.type === 'warehouse') {
      civicBox(r, 0, 0, g + 4.8, 12, 9.6, 26, CIV_WALL2);
      civicBox(r, 0, 0, g + 9.75, 12.5, 0.5, 26.5, CIV_ROOF);
      civicBox(r, 0.6, 0, g + 9.95, 10.4, 0.16, 22, CIV_SOLAR);
      for (let k = -1; k <= 1; k += 1) {
        civicBox(r, 6.15, k * 8, g + 2.6, 0.3, 5.0, 4.6, CIV_GLASS);
      }
      civicBox(r, 7.4, 0, g + 0.7, 3.0, 1.4, 22, WALL_CAP);
      civicBox(r, -3.5, 12, g + 3.6, 6, 7.2, 6, CIV_WALL);
    }
  }

  // Villas with pools, only on the far outer slopes: one storey, flat roof, terrace, pergola.
  for (let i = 0; i < villas.length; i += 1) {
    const v = villas[i];
    const gr = plotGround(v.x, v.z, v.yaw, 26, 24);
    const g = gr.gMin - 0.6;
    civicBox(v, 0, 2.5, g + 1.9, 11, 3.6, 8, CIV_WALL);
    civicBox(v, 0, 2.5, g + 3.85, 11.4, 0.4, 8.4, CIV_WALL2);
    civicBox(v, -6.5, 5.5, g + 1.5, 4.6, 2.8, 5.4, CIV_WALL2);
    civicBox(v, 0, -4.6, g + 0.25, 9, 0.4, 6, WALL_CAP);
    for (let k = 0; k < 4; k += 1) {
      civicBox(v, (k & 1) ? 3.4 : -3.4, (k & 2) ? -2.4 : -6.2, g + 1.5, 0.22, 2.6, 0.22, CIV_WALL2);
    }
    civicBox(v, 0, -4.3, g + 2.95, 7.4, 0.22, 4.2, CIV_WALL2);
    if (v.pool) {
      civicBox(v, 4.6, -9.2, g + 0.12, 7.2, 0.28, 3.9, WALL_CAP);
      civicBox(v, 4.6, -9.2, g + 0.24, 6.4, 0.14, 3.2, CIV_POOL);
    }
  }

  // ------------------------------------------------------------------ terraces
  // Dry-stone retaining walls on every unbuilt slope: the gentle near slope in front of the
  // ring, the shoulders either side of the district, and the back of the hill. A run is levelled
  // before one stone is laid. Its cap is one horizontal plane, the ground under it is allowed to
  // rise and fall, and the base is sunk below the lowest ground the run crosses, so the wall
  // sits on the slope with a level top instead of tilting with it or floating at one end.
  //
  // Siting is the occupancy grid's job, not a second set of rules: a run is rejected whole if
  // any cell of its footprint already carries a road, a pavement, a parcel, a prop, the dream
  // corridor, a house or a farm track, and it breaks at the switchback lane, which is painted on
  // the terrain rather than flagged. That is also what makes the tracks visibly climb through
  // the terraces: their cells are flagged before this pass runs.
  const terraceRuns = [];
  let wallMetres = 0;
  let wallSegmentCount = 0;
  const WALL_MASK = (B_BLOCK & ~B_PROP) | B_HOUSE;
  const WALL_STONE_B = new THREE.Color(0xb3a286);

  function addWallRun(x0, z0, x1, z1, levelTop) {
    const dx = x1 - x0;
    const dz = z1 - z0;
    const len = Math.sqrt(dx * dx + dz * dz);
    if (len < 4.5) return false;
    const ux = dx / len;
    const uz = dz / len;
    const cx = (x0 + x1) * 0.5;
    const cz = (z0 + z1) * 0.5;
    if (!rectScan(cx, cz, ux, uz, len * 0.5 + 0.2, 0.78, WALL_MASK, 0)) return false;
    for (let s = 0; s <= len; s += 2) {
      if (laneDistance(x0 + ux * s, z0 + uz * s) < 3.4) return false;
    }
    let gMax = -Infinity;
    let gMin = Infinity;
    for (let s = 0; s <= len; s += 1.5) {
      const g = groundHeightAt(x0 + ux * s, z0 + uz * s);
      if (g > gMax) gMax = g;
      if (g < gMin) gMin = g;
    }
    const capY = levelTop === undefined ? gMax + 1.05 : levelTop;
    let base = gMin - 0.8;
    if (base > capY - 1.85) base = capY - 1.85;
    const height = capY - base;
    const segs = Math.max(1, Math.round(len / 15));
    const step = len / segs;
    const yaw = Math.atan2(-uz, ux);
    const stone = (wallSegmentCount & 1) ? WALL_STONE_B : WALL_STONE;
    for (let i = 0; i < segs; i += 1) {
      const t = (i + 0.5) * step;
      const x = x0 + ux * t;
      const z = z0 + uz * t;
      statics.addGeometry(unitBox,
        composeMatrix(x, (capY + base) * 0.5, z, yaw, step + 0.55, height, 0.9), stone);
      statics.addGeometry(unitBox,
        composeMatrix(x, capY + 0.09, z, yaw, step + 0.7, 0.22, 1.16), WALL_CAP);
      wallSegmentCount += 1;
    }
    rectScan(cx, cz, ux, uz, len * 0.5 + 0.2, 0.78, 0, B_PROP);
    wallMetres += len;
    terraceRuns.push({ x0: x0, z0: z0, x1: x1, z1: z1, top: capY, base: base, len: len });
    return true;
  }

  // A straight terrace across the near slope. The ground is so nearly level along the contour
  // here that a straight line is a contour to within half a metre; each twenty seven metre run
  // takes its own level, so the terrace descends in steps as the hill does.
  function addStraightTerraces(z, xFrom, xTo, runLen) {
    const total = xTo - xFrom;
    const runs = Math.max(1, Math.round(total / runLen));
    const step = total / runs;
    for (let i = 0; i < runs; i += 1) {
      const x0 = xFrom + step * i + 0.6;
      const x1 = x0 + step - 1.2;
      addWallRun(x0, z, x1, z, undefined);
    }
  }
  // Seven terraces across the near slope, from the ring's outer edge to the top of the near
  // hill. The rows at 118, 128, 142 and 156 are the portrait frame's foreground: they fall in
  // its bottom fifth at block tier, where the ground was eighty percent flat, and stacked at
  // fourteen metres they read as a walled hillside rather than one long line. The rows above
  // carry the slope for the desktop frame, whose bottom edge is the ring itself.
  {
    const NEAR_T = [118, 128, 142, 156, 190, 230, 280];
    // The even rows of the foreground stack stop at x ninety six, because that is the whole
    // visible width of the portrait frame at its bottom edge; the odd rows run the full width so
    // the desktop's lower corners still find a wall.
    for (let i = 0; i < NEAR_T.length; i += 1) {
      const narrow = (i & 1) === 0 && i < 4;
      addStraightTerraces(NEAR_T[i], narrow ? -96 : -186, narrow ? 96 : 186, narrow ? 24 : 27);
    }
    // The portrait frame's window is only about a hundred metres wide at the bottom edge, and the
    // lane, two tracks and a villa cut it into short spans. Four more rows thread through those
    // spans in thirteen metre runs; the run-level rejection keeps the ones that would cross a
    // path, a track or a footprint, so the slope reads as terraced right up to each interruption
    // instead of stopping at the first one.
    const EDGE_T = [124, 134, 144, 154];
    for (let i = 0; i < EDGE_T.length; i += 1) addStraightTerraces(EDGE_T[i], -64, 64, 13);
  }

  // A contour wall, traced height by height: for each x the ground height is solved for the
  // level and the wall runs along the level set, so its top can be one plane across its whole
  // length. Two levels on each shoulder and one behind the crest carry the outer hillside.
  function contourZAt(x, level, zMin, zMax) {
    let zPrev = zMin;
    let fPrev = groundHeightAt(x, zPrev) - level;
    for (let z = zMin + 2; z <= zMax; z += 2) {
      const f = groundHeightAt(x, z) - level;
      if ((fPrev > 0 && f <= 0) || (fPrev < 0 && f >= 0)) {
        let a = zPrev;
        let b = z;
        for (let k = 0; k < 14; k += 1) {
          const m = (a + b) * 0.5;
          const fm = groundHeightAt(x, m) - level;
          if ((fPrev > 0 && fm > 0) || (fPrev < 0 && fm < 0)) a = m;
          else b = m;
        }
        return (a + b) * 0.5;
      }
      zPrev = z;
      fPrev = f;
    }
    return NaN;
  }

  function addContourTerrace(level, xFrom, xTo, zMin, zMax) {
    const STEP_X = 8;
    let px = NaN;
    let pz = NaN;
    let runX = NaN;
    let runZ = NaN;
    let runLen = 0;
    for (let x = xFrom; x <= xTo + 1e-6; x += STEP_X) {
      const z = contourZAt(x, level, zMin, zMax);
      if (!isFinite(z)) {
        if (runLen > 6) addWallRun(runX, runZ, px, pz, level + 1.05);
        runX = NaN;
        runLen = 0;
        px = NaN;
        continue;
      }
      if (!isFinite(px)) {
        px = x;
        pz = z;
        runX = x;
        runZ = z;
        runLen = 0;
        continue;
      }
      const dx = x - px;
      const dz = z - pz;
      runLen += Math.sqrt(dx * dx + dz * dz);
      if (runLen >= 26) {
        addWallRun(runX, runZ, x, z, level + 1.05);
        runX = x;
        runZ = z;
        runLen = 0;
      }
      px = x;
      pz = z;
    }
    if (runLen > 6) addWallRun(runX, runZ, px, pz, level + 1.05);
  }

  {
    const SIDE_LEVELS = [5, 12];
    for (let i = 0; i < SIDE_LEVELS.length; i += 1) {
      addContourTerrace(SIDE_LEVELS[i], -276, -118, -200, 260);
      addContourTerrace(SIDE_LEVELS[i], 118, 276, -200, 260);
    }
    addContourTerrace(31, -190, 190, -250, -118);
  }

  // ------------------------------------------------------------------ terrain mesh
  // A non-uniform grid: 3.5 m cells over the district, growing geometrically out to the
  // horizon. Vertex colours: dry grass, sunburnt patches, earthy terrace risers, pale built-up
  // ground under the fabric, tilled soil under groves, a patchwork of fields in the countryside,
  // and a hazy blue-green toward the background hills.
  function axisCoords() {
    const inner = 140;
    const step = 3.5;
    const mid = [];
    for (let v = -inner; v <= inner + 1e-6; v += step) mid.push(v);
    // The belt beyond the district carries the far ridges, and it is seen at 400 to 1500
    // metres: a cell that grows straight from 3.5 metres leaves the crests unresolved, so a
    // silhouette that should be a line of hills becomes two facets. Twenty metre cells out to
    // 520 give a 150 metre crest seven vertices and one silhouette segment per twenty pixels,
    // for nine thousand triangles; past 520 the grid grows at 1.22 and is haze anyway. The
    // nearest ground in the frame is painted by props instead of by vertices for the same
    // reason: at 25 pixels to the metre one cell is five hundred pixels wide.
    const belt = [];
    for (let v = inner + 20; v <= 520 + 1e-6; v += 20) belt.push(v);
    const outer = [];
    let s = 20;
    let v = 520;
    while (v < GROUND_EXTENT) {
      s *= 1.22;
      v = Math.min(GROUND_EXTENT, v + s);
      outer.push(v);
    }
    const out = [];
    for (let i = outer.length - 1; i >= 0; i -= 1) out.push(-outer[i]);
    for (let i = belt.length - 1; i >= 0; i -= 1) out.push(-belt[i]);
    for (let i = 0; i < mid.length; i += 1) out.push(mid[i]);
    for (let i = 0; i < belt.length; i += 1) out.push(belt[i]);
    for (let i = 0; i < outer.length; i += 1) out.push(outer[i]);
    return out;
  }

  // Dirt tracks worked into the countryside: the small roads the fields are reached from.
  const DIRT_TRACKS = [
    [[-148, 40], [-188, 62], [-236, 104], [-268, 150]],
    [[118, -58], [166, -78], [214, -106], [252, -146]],
    [[-44, 152], [-62, 188], [-86, 232], [-104, 286]],
    [[172, 34], [216, 46], [258, 44]],
    [[-120, -128], [-158, -164], [-196, -212]]
  ];

  // The lane the near slope is worked from: one track leaving the ring road and climbing the
  // terraces in hairpins. It is paint in the ground, not a mesh, because the nearest ground in
  // the frame is 25 device pixels to the metre and a four metre track reads as a line long
  // before it needs a surface. The hairpins are 22 to 26 metres apart in z, so they stay
  // separate bands at both tiers instead of fusing into one pale smear.

  const terrainMesh = (function () {
    const ax = axisCoords();
    const n = ax.length;
    const pos = new Float32Array(n * n * 3);
    const col = new Float32Array(n * n * 3);
    const fieldId = new Int32Array(n * n);
    const fieldW = new Float32Array(n * n);
    // Ground cover: one dominant dry-grass family with only slow, large-scale variation. The
    // old per-cell hash poured four hues (green scrub, salmon soil, blue rock) into 37 by 31
    // metre patches, which read as noise from the air and is exactly the blotchy countryside the
    // owner rejected. A field is now one calm plane: the hue stays in the golden band, the value
    // moves under four percent across tens of metres, and the only stronger colours left are the
    // terraces, the groves' own soil and the tracks, all tied to real features.
    // Pale warm albedo on purpose. The golden hour light supplies the gold: a saturated albedo
    // multiplied by this sun doubles its chroma and produced the noisy orange/green quilt. These
    // greys read as dry golden grass and olive when the low sun tints them, and stay calm when
    // they do not.
    //
    // Chroma, measured. The calm family was calm enough to be nearly neutral: HSV chroma 0.16 to
    // 0.20, and the countryside read as grey sand between the coloured city and the blue sky.
    // Each tone gains 35 percent chroma with its hue and value held, which lifts the outside
    // ground into dry grass and straw without touching the large-scale variation that keeps the
    // quilt out: the spread between tones is the same fraction of a larger chroma, the drift
    // amplitude is unchanged, and the aerial fade still takes the distance to the same pale air.
    const cDry = new THREE.Color(0xa29169);        // dry golden grass, the one dominant tone
    const cStraw = new THREE.Color(0xad9c76);      // the pale end of the same family
    const cOliveField = new THREE.Color(0x9c8b65); // a worked olive band, a few percent only
    const cSoil = new THREE.Color(0x957f59);
    // The ground the city stands on is its own family: packed earth and worn stone, not dry
    // grass under an urban veil. It is the largest single surface in the upper tenths (terrain
    // covers 51 percent of the third tenth, which measured 0.374 saturation against the 0.40
    // target), so it takes the strongest lift of the set.
    const cUrban = new THREE.Color(0xb29c64);
    const cGrove = new THREE.Color(0x8c744b);
    const cAir = AIR_HUE;
    const cAirBlue = AIR_BLUE;
    const cStone = new THREE.Color(0xaa976d);
    const cTread = new THREE.Color(0xb29c64);
    const cLane = new THREE.Color(0xa6946c);
    const cDirt = new THREE.Color(0x89744d);
    const cShade = new THREE.Color(0x7f704f);
    const c = new THREE.Color();

    // Squared distance from a point to one polyline, shared by the dirt tracks and the lane.
    function distSqToPath(x, z, pts) {
      let best = 1e9;
      for (let i = 0; i < pts.length - 1; i += 1) {
        const ax0 = pts[i][0];
        const az0 = pts[i][1];
        const dx = pts[i + 1][0] - ax0;
        const dz = pts[i + 1][1] - az0;
        let u = ((x - ax0) * dx + (z - az0) * dz) / (dx * dx + dz * dz);
        u = u < 0 ? 0 : u > 1 ? 1 : u;
        const px = ax0 + dx * u - x;
        const pz = az0 + dz * u - z;
        const d = px * px + pz * pz;
        if (d < best) best = d;
      }
      return best;
    }

    function distToPath(x, z, pts) {
      return Math.sqrt(distSqToPath(x, z, pts));
    }

    // Dirt tracks across the countryside: the small roads the fields are worked from.
    function trackWeight(x, z) {
      let best = 1e9;
      for (let t = 0; t < DIRT_TRACKS.length; t += 1) {
        const d = distSqToPath(x, z, DIRT_TRACKS[t]);
        if (d < best) best = d;
      }
      return 1 - smooth(1.4, 3.6, Math.sqrt(best));
    }

    // A little vertical relief away from every road, pavement, pad and prop, so the ground is
    // never a plane. It fades to exactly zero within six metres of anything occupied, which is
    // what keeps the roads and the building pads flat and the paving unbroken.
    function relief(x, z) {
      for (let i = 0; i < 8; i += 1) {
        const a = i * 0.785398;
        if (flagsAt(x + Math.cos(a) * 5.6, z + Math.sin(a) * 5.6) & B_SOLID) return 0;
      }
      return Math.sin(x * 0.077 + Math.cos(z * 0.061) * 2.1) * Math.cos(z * 0.083 - x * 0.013) * 0.55 +
        Math.sin(x * 0.19 + z * 0.17) * 0.12;
    }

    for (let j = 0; j < n; j += 1) {
      const z = ax[j];
      for (let i = 0; i < n; i += 1) {
        const x = ax[i];
        const k = (j * n + i) * 3;
        pos[k] = x;
        pos[k + 1] = groundHeightAt(x, z) + relief(x, z);
        pos[k + 2] = z;

        // One calm ground plane: dry golden grass with a slow drift toward straw and a hint of
        // olive. The wavelengths are tens of metres, so a field reads as one coherent surface
        // and the value stays inside four percent instead of jumping between patch colours.
        const slowA = Math.sin(x * 0.0064 + z * 0.0041 + 0.7) *
          Math.cos(z * 0.0053 - x * 0.0027 - 0.2);
        const slowB = Math.sin(x * 0.0170 - z * 0.0130 + 1.9);
        const rd = Math.sqrt(x * x + z * z);
        c.copy(cDry);
        c.lerp(cOliveField, 0.10 + 0.08 * slowA);
        c.lerp(cStraw, 0.10 + 0.08 * slowB);
        c.multiplyScalar(1 + 0.020 * slowA + 0.012 * slowB);
        // Parcel boundaries are still drawn, but on a coarser grid than the old 37 by 31 metre
        // cells, so the countryside carries a few tidy plot lines instead of a patchwork quilt.
        const fwx = x * 0.999 + z * 0.045;
        const fwz = -x * 0.045 + z * 0.999;
        const fi = Math.floor(fwx / 86);
        const fj = Math.floor(fwz / 62);
        fieldId[j * n + i] = fi * 73 + fj;
        fieldW[j * n + i] = 1;

        const gx = districtHeight(x + 1, z) - districtHeight(x - 1, z);
        const gz = districtHeight(x, z + 1) - districtHeight(x, z - 1);
        const g = Math.sqrt(gx * gx + gz * gz) * 0.5;
        c.lerp(cSoil, smooth(0.14, 0.34, g) * 0.10);

        // Terraces: the country outside the ring road is stepped, so a contour crossed at every
        // 2.8 metres of height reads as a dry-stone riser with a pale tread above it. Because
        // the band is keyed on absolute height, the paint follows the contours of the slope and
        // draws the horizontal lines a terraced hillside has, at no triangle cost: this is the
        // vertex colour of ground the grid already carries.
        const box0 = Math.max(Math.abs(x), Math.abs(z));
        const ty = pos[k + 1];
        if (box0 > 116 && box0 < 700) {
          const frac = ty / 2.8 - Math.floor(ty / 2.8);
          const riser = 1 - smooth(0.04, 0.26, frac);
          const tread = smooth(0.30, 0.52, frac) * (1 - smooth(0.70, 0.94, frac));
          const near = smooth(116, 150, box0) * (1 - smooth(560, 700, box0));
          c.lerp(cStone, riser * 0.52 * near);
          c.lerp(cTread, tread * 0.26 * near);
          // Every fifth riser is a wall rather than a step. On the near slope the 2.8 metre step
          // is two metres of ground and needs no help, but across the island chain it is a few
          // pixels wide, so the coarser contour is the only one that survives the distance and
          // still draws the horizontal lines a worked hillside has.
          const f5 = ty / 14 - Math.floor(ty / 14);
          const wall = 1 - smooth(0.03, 0.13, f5);
          c.lerp(cStone, wall * 0.36 * near * smooth(330, 470, box0));
        }

        if (box0 < 190) {
          const urban = inUrban(x, z) ? 1 : (inRingFrontage(x, z) ? 0.85 : 0);
          if (urban > 0) c.lerp(cUrban, urban * 0.8);
        }
        const dc = rd;
        if (dc < 380) {
          let gw = 0;
          for (let q = 0; q < groves.length; q += 1) {
            const dx = x - groves[q].x;
            const dz = z - groves[q].z;
            const d = Math.sqrt(dx * dx + dz * dz);
            gw = Math.max(gw, 1 - smooth(groves[q].r * 0.75, groves[q].r + 3, d));
          }
          for (let q = 0; q < nearGroves.length; q += 1) {
            const dx = x - nearGroves[q].x;
            const dz = z - nearGroves[q].z;
            const d = Math.sqrt(dx * dx + dz * dz);
            gw = Math.max(gw, 1 - smooth(nearGroves[q].r * 0.75, nearGroves[q].r + 3, d));
          }
          c.lerp(cGrove, gw * 0.45);
        }

        // The switchback farm lane: a pale limestone track cut down the near slope in hairpins,
        // with a darker trodden edge, which is what gives the nearest ground a line to read
        // instead of a wash of one dirt colour.
        if (box0 > 112 && box0 < 340) {
          const ld = distToPath(x, z, LANE_PATH);
          if (ld < 5.2) {
            const core = 1 - smooth(1.9, 2.9, ld);
            const edge = (1 - smooth(2.9, 4.4, ld)) * smooth(1.7, 2.9, ld);
            c.lerp(cLane, core * 0.88);
            c.lerp(cDirt, edge * 0.5);
          }
        }

        // Aerial perspective: distance carries the warm air, then the pale blue of the far
        // horizon. Two stops, keyed on distance from the district, so the ridge chain layers:
        // the nearest crests keep their own golden ground through a thin cream veil, the middle
        // ones lift into straw and the outer shore into a clean pale blue. No lilac anywhere.
        if (dc > 270) {
          const warm = smooth(270, 430, dc);
          const far = smooth(330, 480, dc);
          if (warm > 0) {
            const k = warm * 0.52;
            const r0 = c.r;
            const g0 = c.g;
            const b0 = c.b;
            const v0 = r0 * 0.42 + g0 * 0.42 + b0 * 0.16;
            c.setRGB(
              r0 + ((cAir.r * 0.62 + v0 * 0.48) - r0) * k,
              g0 + ((cAir.g * 0.62 + v0 * 0.42) - g0) * k,
              b0 + ((cAir.b * 0.52 + v0 * 0.46) - b0) * k
            );
          }
          if (far > 0) c.lerp(cAirBlue, far * 0.70);
          // Air is a value as well as a hue: every crest that crosses another has to step up in
          // lightness at its own silhouette, which is the only edge the distance can carry.
          c.multiplyScalar(0.90 + 0.28 * smooth(300, 700, dc));
        }

        // A warm shadow on every flank that turns away from the sun: the far ridges need one
        // step darker than their lit sides to read as hills, but the shade is grey-warm, not
        // blue, so it stays inside the golden countryside instead of adding a second cold hue.
        if (box0 > 74) {
          const ex = groundHeightAt(x + 2.5, z) - groundHeightAt(x - 2.5, z);
          const ez = groundHeightAt(x, z + 2.5) - groundHeightAt(x, z - 2.5);
          const el = Math.sqrt(ex * ex + ez * ez);
          if (el > 0.25) {
            const away = -(ex * 0.927 + ez * 0.375) / el;
            if (away > 0) c.lerp(cShade, smooth(0.05, 0.7, away) * smooth(0.06, 0.42, el / 5) * 0.45);
          }
        }

        if (box0 > 130) c.lerp(cDirt, trackWeight(x, z) * 0.7);

        col[k] = c.r;
        col[k + 1] = c.g;
        col[k + 2] = c.b;
      }
    }

    // Field boundaries: one quiet line wherever two parcels meet. The line is a seven percent
    // step, not the old quarter, so the plots read as worked land without drawing a dark grid
    // over the countryside.
    for (let j = 0; j < n; j += 1) {
      for (let i = 0; i < n; i += 1) {
        const id = fieldId[j * n + i];
        if (id === -999999 || fieldW[j * n + i] < 0.06) continue;
        let edge = false;
        if (i + 1 < n && fieldId[j * n + i + 1] !== id) edge = true;
        if (j + 1 < n && fieldId[(j + 1) * n + i] !== id) edge = true;
        if (!edge) continue;
        const k = (j * n + i) * 3;
        col[k] *= 0.93;
        col[k + 1] *= 0.935;
        col[k + 2] *= 0.92;
      }
    }

    const idx = new Uint32Array((n - 1) * (n - 1) * 6);
    let o = 0;
    for (let j = 0; j < n - 1; j += 1) {
      for (let i = 0; i < n - 1; i += 1) {
        const a = j * n + i;
        const b = a + 1;
        const d = a + n;
        const e = d + 1;
        idx[o] = a;
        idx[o + 1] = d;
        idx[o + 2] = b;
        idx[o + 3] = b;
        idx[o + 4] = d;
        idx[o + 5] = e;
        o += 6;
      }
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    geo.setAttribute('color', new THREE.BufferAttribute(col, 3));
    geo.setIndex(new THREE.BufferAttribute(idx, 1));
    geo.computeVertexNormals();
    geo.computeBoundingSphere();
    // The far air: the lit ground alone cannot be blue under this warm sun, so the terrain in
    // the top of the frame is dissolved into the sky's own air in the shader. The mix is keyed
    // to the vertex's projected height, not its distance, because the near ridge also reaches
    // the top of frame; it keeps a fraction of the lit colour plus the vertex colour's own
    // value, which is what preserves the ridge silhouettes inside the haze.
    const terrainMat = new THREE.MeshStandardMaterial({
      vertexColors: true,
      roughness: 1.0,
      metalness: 0.0
    });
    terrainMat.onBeforeCompile = function (shader) {
      shader.uniforms.uAirWarm = { value: new THREE.Color(0xefe0c2) };
      shader.uniforms.uAirBlue = { value: new THREE.Color(0xa9c4e0) };
      shader.vertexShader = 'varying float vAir;\nvarying vec3 vGround;\n' +
        shader.vertexShader.replace(
          '#include <begin_vertex>',
          '#include <begin_vertex>\nvGround = color;'
        ).replace(
          '#include <project_vertex>',
          '#include <project_vertex>\nvAir = smoothstep(0.05, 0.82, gl_Position.y / max(gl_Position.w, 1e-4));'
        );
      const inject = [
        'float airT = 0.72 * smoothstep(0.0, 0.85, vAir);',
        'vec3 airCol = mix(uAirWarm, uAirBlue, smoothstep(0.55, 0.95, vAir));',
        'airCol *= 0.88 + 0.14 * clamp(vAir, 0.0, 1.0);',
        'float gL = dot(vGround, vec3(0.2126, 0.7152, 0.0722));',
        'airCol *= 0.70 + 0.62 * clamp(gL, 0.0, 1.0);',
        'gl_FragColor.rgb = mix(gl_FragColor.rgb, airCol, airT);'
      ].join('\n');
      shader.fragmentShader = 'varying float vAir;\nvarying vec3 vGround;\n' +
        'uniform vec3 uAirWarm;\nuniform vec3 uAirBlue;\n' +
        shader.fragmentShader.replace('#include <opaque_fragment>', '#include <opaque_fragment>\n' + inject);
    };
    // A unique key per material: a constant key lets a rebuilt world reuse the first material's
    // cached program, which skips this hook and leaves the air uniforms unset.
    terrainMat.customProgramCacheKey = function () { return 'terrain-air-' + this.uuid; };
    const mesh = new THREE.Mesh(geo, terrainMat);
    mesh.receiveShadow = shadowsOn;
    mesh.name = 'terrain';
    // The height of the triangles the frame actually draws, not the analytic ground: outside the
    // district relief() and the twenty metre grid put the drawn surface up to a metre away from
    // groundHeightAt, and the lane's light has to lie on the drawn surface instead of sinking
    // through it. Build-time only; setting anything per frame calls it once, never in a loop.
    function axisIndex(v) {
      if (v <= ax[0]) return 0;
      if (v >= ax[n - 1]) return n - 2;
      let lo = 0;
      let hi = n - 1;
      while (hi - lo > 1) {
        const mid = (lo + hi) >> 1;
        if (ax[mid] <= v) lo = mid;
        else hi = mid;
      }
      return lo;
    }
    mesh.userData.surfaceHeightAt = function (x, z) {
      const i = axisIndex(x);
      const j = axisIndex(z);
      const u = clamp((x - ax[i]) / (ax[i + 1] - ax[i]), 0, 1);
      const v = clamp((z - ax[j]) / (ax[j + 1] - ax[j]), 0, 1);
      const a = (j * n + i) * 3 + 1;
      const b = a + 3;
      const d = a + n * 3;
      const e = d + 3;
      if (u + v <= 1) return pos[a] + u * (pos[b] - pos[a]) + v * (pos[d] - pos[a]);
      return pos[e] + (1 - u) * (pos[d] - pos[e]) + (1 - v) * (pos[b] - pos[e]);
    };
    return mesh;
  })();
  group.add(terrainMesh);

  // ------------------------------------------------------------------ the bay
  // One flat plane for the water, the only draw call the distant relief adds: a ring sector in
  // the same arc the basin is dug in, at the water level, so its shore is wherever the ground
  // crosses that level. It carries a slow tonal drift instead of one flat colour, and the fog
  // does the rest, which is what keeps it background rather than a second sky. The blue stays a
  // step deeper than the warm lilac haze, so the bay reads as cool water against sunlit hills
  // instead of dissolving into the air with them.
  const bayMesh = (function () {
    const SEG = 30;
    const RINGS = [252, 300, 360, 430, 520, 640, 800, 1010];
    const rings = RINGS.length;
    const pos = new Float32Array(SEG * rings * 3);
    const col = new Float32Array(SEG * rings * 3);
    const near = new THREE.Color(0x3a669b);
    const far = new THREE.Color(0x6f8bc4);
    const cc = new THREE.Color();
    const az0 = BAY_AZ - BAY_HALF;
    const az1 = BAY_AZ + BAY_HALF;
    for (let r = 0; r < rings; r += 1) {
      const rad = RINGS[r];
      const dt = r / (rings - 1);
      for (let s = 0; s < SEG; s += 1) {
        const a = az0 + (az1 - az0) * (s / (SEG - 1));
        const k = (r * SEG + s) * 3;
        pos[k] = Math.cos(a) * rad;
        pos[k + 1] = WATER_LEVEL;
        pos[k + 2] = Math.sin(a) * rad;
        // A gentle cross-swell in the tone, so the water is not one flat bucket in the frame.
        const swell = 0.5 + 0.5 * Math.sin(a * 17.0 + rad * 0.012);
        cc.copy(near).lerp(far, dt * 0.55 + swell * 0.22);
        col[k] = cc.r;
        col[k + 1] = cc.g;
        col[k + 2] = cc.b;
      }
    }
    const idx = new Uint32Array((rings - 1) * (SEG - 1) * 6);
    let o = 0;
    for (let r = 0; r < rings - 1; r += 1) {
      for (let s = 0; s < SEG - 1; s += 1) {
        const a = r * SEG + s;
        const b = a + 1;
        const d = a + SEG;
        const e = d + 1;
        // Counter-clockwise seen from above, so the water faces the sky.
        idx[o] = a; idx[o + 1] = b; idx[o + 2] = d;
        idx[o + 3] = b; idx[o + 4] = e; idx[o + 5] = d;
        o += 6;
      }
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    geo.setAttribute('color', new THREE.BufferAttribute(col, 3));
    geo.setIndex(new THREE.BufferAttribute(idx, 1));
    geo.computeVertexNormals();
    geo.computeBoundingSphere();
    const mesh = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({
      vertexColors: true,
      roughness: 0.42,
      metalness: 0.06
    }));
    mesh.name = 'bay';
    return mesh;
  })();
  group.add(bayMesh);
  if (NARROW_VIEW) nightCullTouch.push(bayMesh);

  // ------------------------------------------------------------------ paving
  // Everything flat on the ground in one vertex-coloured mesh: ring and avenue asphalt with
  // pavements, dashes and crosswalks, the roundabout, the tram beds and rails, the street grid
  // with its kerbs, the block courtyards, the dream corridor lot, the market square, and the
  // rail yard ballast, sleepers and rails.
  const C_ASPHALT_ROUND = new THREE.Color(0x332f2a);
  const C_STREET = new THREE.Color(0x3c3732);
  // Pavements: warm mid stone in six tones, picked by a position hash, so a run of pavement is
  // six slabs rather than one pale mass. The kerb line stays a shade lighter than the pavement,
  // so the edge still reads against the darker asphalt.
  //
  // Chroma, measured. Paving covers about 30 percent of the middle tenths and its own mean
  // saturation was 0.288, so it was the second largest drag on the frame after the facades.
  // Every tone keeps its hue and value and gains 30 percent chroma, and the asphalt leaves its
  // neutral graphite for a warm 30 degree hue at HSV 0.17 to 0.22 with its value range intact:
  // a sun-baked street is not neutral, and the grey it was reading as is what made the lower
  // third flat.
  const PAVE_TONES = [
    new THREE.Color(0xc2a771), new THREE.Color(0xbb9f66), new THREE.Color(0xb59a63),
    new THREE.Color(0xb19f64), new THREE.Color(0xbfa46a), new THREE.Color(0xb39c61)
  ];
  const C_KERB = new THREE.Color(0xb5a27d);
  const C_KERB_STONE = new THREE.Color(0xe7d7ac);
  const C_KERB_STONE_B = new THREE.Color(0xd8c697);
  const C_PAINT = new THREE.Color(0xfbfaf3);
  const C_BED = new THREE.Color(0x587a28);
  const C_BED_CROSS = new THREE.Color(0x565145);
  const C_RAIL = new THREE.Color(0xc9ccd1);
  const C_BALLAST = new THREE.Color(0x6f6251);
  const C_SLEEPER = new THREE.Color(0x4a392b);
  const C_TRACK = new THREE.Color(0x8b9298);
  const C_PLAZA_A = new THREE.Color(0xd8c18e);
  const C_PLAZA_B = new THREE.Color(0xcbb481);
  const C_PLAZA_EDGE = new THREE.Color(0xbba066);
  const C_COURT = new THREE.Color(0xb09e68);
  const C_COURT_G = new THREE.Color(0x74803a);
  const C_LOT = new THREE.Color(0x8f7a5a);
  const C_LOT_GRASS = new THREE.Color(0x7c8443);
  const C_MEDIAN_SOIL = new THREE.Color(0x4a6426);
  const C_TURF = new THREE.Color(0x3c701f);
  const C_TURF_MOWN = new THREE.Color(0x457726);
  const C_APRON = new THREE.Color(0x464850);
  const C_BAY = new THREE.Color(0xcdc1a5);
  const C_TRACK_LINE = new THREE.Color(0xf0ece0);

  // One pavement tone per six metres of frontage, hashed on the quad midpoint.
  const paveScratch = new THREE.Color();
  function paveAt(x, z) {
    const h = hash2(Math.round(x / 6) + 8192, Math.round(z / 6) + 8192);
    const i = (h * PAVE_TONES.length) | 0;
    return paveScratch.copy(PAVE_TONES[i >= PAVE_TONES.length ? 0 : i]);
  }

  // Asphalt wear. The carriageway is not one flat tone: five tones inside a 0.031 value band,
  // chosen by a coarse drift (9 m cells) with a fine grain (3.1 m) mixed in, keyed on position
  // so every rebuild repeats it. The colour is a warm 30 degree hue, not a neutral graphite: the
  // grey asphalt was the largest near-neutral surface in the lower third, and at golden hour a
  // street this warm reads as dust and sun-bleached binder rather than as slate. Value is
  // untouched, so the palette's range still runs 0.188 to 0.275 against a base of 0.227, a
  // spread of 0.087, inside the 0.12 the brief allows. The repair patches and the polished
  // wheel tracks added below carry the two stronger notes.
  const ASPHALT_TONES = [
    new THREE.Color(0x37332e), new THREE.Color(0x3a3530), new THREE.Color(0x3a342f),
    new THREE.Color(0x3d3730), new THREE.Color(0x3f3831)
  ];
  const asphaltScratch = new THREE.Color();
  function asphaltAt(x, z) {
    const coarse = hash2(Math.round(x / 9) + 4100, Math.round(z / 9) + 2200);
    const fine = hash2(Math.round(x / 3.1) + 3300, Math.round(z / 3.1) + 5600);
    const k = Math.min(ASPHALT_TONES.length - 1,
      ((coarse * 0.5 + fine * 0.5) * ASPHALT_TONES.length) | 0);
    return asphaltScratch.copy(ASPHALT_TONES[k]);
  }

  const paving = makeBuilder();
  // Road paint rides its own mesh, a plane above the asphalt rather than a coplanar colour
  // change: white markings keep a crisp edge at street tier instead of dissolving into the
  // asphalt at a grazing angle, and a 5.8 cm height separation rules out any depth flicker.
  const paint = makeBuilder();

  function band(path, a, b, lift, col, skip, colFn) {
    const last = path.n;
    for (let i = 0; i < last; i += 1) {
      const j = i + 1;
      const mx = (path.xs[i] + path.xs[j]) * 0.5 + (path.nx[i] + path.nx[j]) * 0.25 * (a + b);
      const mz = (path.zs[i] + path.zs[j]) * 0.5 + (path.nz[i] + path.nz[j]) * 0.25 * (a + b);
      if (skip && skip(mx, mz)) continue;
      const cc = colFn ? colFn(mx, mz) : col;
      paving.flatQuad(
        path.xs[i] + path.nx[i] * a, path.zs[i] + path.nz[i] * a,
        path.xs[j] + path.nx[j] * a, path.zs[j] + path.nz[j] * a,
        path.xs[j] + path.nx[j] * b, path.zs[j] + path.nz[j] * b,
        path.xs[i] + path.nx[i] * b, path.zs[i] + path.nz[i] * b,
        lift, cc
      );
    }
  }

  // A continuous painted ribbon along a road edge, inside the asphalt, at paint lift.
  function paintStrip(path, a, b, lift, skip) {
    const last = path.n;
    for (let i = 0; i < last; i += 1) {
      const j = i + 1;
      const mx = (path.xs[i] + path.xs[j]) * 0.5 + (path.nx[i] + path.nx[j]) * 0.25 * (a + b);
      const mz = (path.zs[i] + path.zs[j]) * 0.5 + (path.nz[i] + path.nz[j]) * 0.25 * (a + b);
      if (skip && skip(mx, mz)) continue;
      paint.flatQuad(
        path.xs[i] + path.nx[i] * a, path.zs[i] + path.nz[i] * a,
        path.xs[j] + path.nx[j] * a, path.zs[j] + path.nz[j] * a,
        path.xs[j] + path.nx[j] * b, path.zs[j] + path.nz[j] * b,
        path.xs[i] + path.nx[i] * b, path.zs[i] + path.nz[i] * b,
        lift, C_PAINT
      );
    }
  }

  // Kerb stones: a 0.28 m stone strip a shade lighter than the pavement, cut into stones on
  // the path's own step, so the kerb line reads as a crisp two-tone edge from the air.
  function kerbStones(path, a, b, lift, skip) {
    for (let i = 0; i < path.n; i += 1) {
      const j = i + 1;
      const mx = (path.xs[i] + path.xs[j]) * 0.5 + (path.nx[i] + path.nx[j]) * 0.25 * (a + b);
      const mz = (path.zs[i] + path.zs[j]) * 0.5 + (path.nz[i] + path.nz[j]) * 0.25 * (a + b);
      if (skip && skip(mx, mz)) continue;
      const cc = (i & 1) ? C_KERB_STONE : C_KERB_STONE_B;
      paving.flatQuad(
        path.xs[i] + path.nx[i] * a, path.zs[i] + path.nz[i] * a,
        path.xs[j] + path.nx[j] * a, path.zs[j] + path.nz[j] * a,
        path.xs[j] + path.nx[j] * b, path.zs[j] + path.nz[j] * b,
        path.xs[i] + path.nx[i] * b, path.zs[i] + path.nz[i] * b,
        lift, cc
      );
    }
  }

  function dashes(path, half, dashLen, gapLen, lift, skip) {
    const per = Math.max(1, Math.round(dashLen / path.step));
    const gap = Math.max(1, Math.round(gapLen / path.step));
    for (let i = 0; i + per <= path.n; i += per + gap) {
      const j = i + per;
      const mx = (path.xs[i] + path.xs[j]) * 0.5;
      const mz = (path.zs[i] + path.zs[j]) * 0.5;
      if (skip && skip(mx, mz)) continue;
      paint.flatQuad(
        path.xs[i] - path.nx[i] * half, path.zs[i] - path.nz[i] * half,
        path.xs[j] - path.nx[j] * half, path.zs[j] - path.nz[j] * half,
        path.xs[j] + path.nx[j] * half, path.zs[j] + path.nz[j] * half,
        path.xs[i] + path.nx[i] * half, path.zs[i] + path.nz[i] * half,
        lift, C_PAINT
      );
    }
  }

  // A bold zebra: bars run with the traffic, 0.62 m of paint on a 1.14 m pitch, as deep as a
  // crossing is walked. tx,tz is the road direction, cx,cz the centre of the crossing.
  function zebra(cx, cz, tx, tz, half, lift) {
    const nx = -tz;
    const nz = tx;
    const depth = 1.95;
    const bar = 0.34;
    const pitch = 1.12;
    const count = Math.floor((2 * half) / pitch);
    const start = -(count - 1) * 0.5 * pitch;
    for (let k = 0; k < count; k += 1) {
      const off = start + k * pitch;
      const a0 = off - bar;
      const a1 = off + bar;
      paint.flatQuad(
        cx + nx * a0 - tx * depth, cz + nz * a0 - tz * depth,
        cx + nx * a1 - tx * depth, cz + nz * a1 - tz * depth,
        cx + nx * a1 + tx * depth, cz + nz * a1 + tz * depth,
        cx + nx * a0 + tx * depth, cz + nz * a0 + tz * depth,
        lift, C_PAINT
      );
    }
    const tl = Math.sqrt(tx * tx + tz * tz) || 1;
    zebraPaint.push({
      x: cx, z: cz, tx: tx / tl, tz: tz / tl,
      half: half * tl, depth: depth * tl
    });
  }

  // A lane arrow: shaft and head, 2.6 m long, painted along tx,tz.
  function laneArrow(cx, cz, tx, tz, lift) {
    const nx = -tz;
    const nz = tx;
    const shaftL = 1.65;
    const headL = 0.95;
    const shaftW = 0.17;
    const headW = 0.52;
    paint.flatQuad(
      cx - tx * shaftL + nx * shaftW, cz - tz * shaftL + nz * shaftW,
      cx + tx * headL * 0.2 + nx * shaftW, cz + tz * headL * 0.2 + nz * shaftW,
      cx + tx * headL * 0.2 - nx * shaftW, cz + tz * headL * 0.2 - nz * shaftW,
      cx - tx * shaftL - nx * shaftW, cz - tz * shaftL - nz * shaftW,
      lift, C_PAINT
    );
    const ay = groundHeightAt(cx, cz) + lift;
    const bx = cx + tx * headL;
    const bz = cz + tz * headL;
    const p1x = cx + tx * headL * 0.2 + nx * headW;
    const p1z = cz + tz * headL * 0.2 + nz * headW;
    const p2x = cx + tx * headL * 0.2 - nx * headW;
    const p2z = cz + tz * headL * 0.2 - nz * headW;
    const i0 = paint.vert(bx, groundHeightAt(bx, bz) + lift, bz, 0, 1, 0, C_PAINT);
    const i1 = paint.vert(p1x, groundHeightAt(p1x, p1z) + lift, p1z, 0, 1, 0, C_PAINT);
    const i2 = paint.vert(p2x, groundHeightAt(p2x, p2z) + lift, p2z, 0, 1, 0, C_PAINT);
    const cross = (p1z - bz) * (p2x - bx) - (p1x - bx) * (p2z - bz);
    if (cross > 0) paint.idx(i0, i1, i2);
    else paint.idx(i0, i2, i1);
  }

  // Ring road.
  band(ringPath, -RING_HALF, RING_HALF, LIFT_ROAD, null, function (x, z) {
    return inRoundabout(x, z, ROUND_R - 0.3);
  }, asphaltAt);
  const ringPaveSkip = function (x, z) {
    return inRoundabout(x, z, ROUND_R) ||
      (flagsAt(x, z) & (B_AVE | B_SPUR)) !== 0;
  };
  band(ringPath, RING_HALF, RING_HALF + RING_PAVE, LIFT_PAVE, null, ringPaveSkip, paveAt);
  band(ringPath, -RING_HALF - RING_PAVE, -RING_HALF, LIFT_PAVE, null, ringPaveSkip, paveAt);
  const ringMarkSkip = function (x, z) {
    return inRoundabout(x, z, ROUND_R + 1) || (flagsAt(x, z) & B_AVE) !== 0;
  };
  dashes(ringPath, 0.13, 3, 4.5, LIFT_PAINT, ringMarkSkip);
  // A white edge line against each kerb: the kerb stone strip then reads against paint, not
  // against asphalt, which is what makes the road edge a drawn line at street tier.
  paintStrip(ringPath, RING_HALF - 0.44, RING_HALF - 0.26, LIFT_PAINT, ringMarkSkip);
  paintStrip(ringPath, -RING_HALF + 0.26, -RING_HALF + 0.44, LIFT_PAINT, ringMarkSkip);
  kerbStones(ringPath, RING_HALF, RING_HALF + 0.28, LIFT_KERB, ringPaveSkip);
  kerbStones(ringPath, -RING_HALF - 0.28, -RING_HALF, LIFT_KERB, ringPaveSkip);

  // Arterial spurs: asphalt, pavements and dashes, running off the frame on three sides. They
  // stop at the ring's asphalt so the junction is paved once, by the ring.
  {
    const inRingBand = function (x, z) {
      return Math.abs(ringSigned(x, z)) < RING_HALF + RING_PAVE;
    };
    for (let s = 0; s < spurPaths.length; s += 1) {
      const path = spurPaths[s];
      band(path, -SPUR_HALF, SPUR_HALF, LIFT_ROAD, null, inRingBand, asphaltAt);
      band(path, SPUR_HALF, SPUR_EDGE, LIFT_PAVE, null, inRingBand, paveAt);
      band(path, -SPUR_EDGE, -SPUR_HALF, LIFT_PAVE, null, inRingBand, paveAt);
      dashes(path, 0.12, 3, 5, LIFT_PAINT, inRingBand);
      paintStrip(path, SPUR_HALF - 0.42, SPUR_HALF - 0.24, LIFT_PAINT, inRingBand);
      paintStrip(path, -SPUR_HALF + 0.24, -SPUR_HALF + 0.42, LIFT_PAINT, inRingBand);
      kerbStones(path, SPUR_HALF, SPUR_HALF + 0.26, LIFT_KERB, inRingBand);
      kerbStones(path, -SPUR_HALF - 0.26, -SPUR_HALF, LIFT_KERB, inRingBand);
    }
  }

  // Farm tracks: a pale limestone dust band, a trodden crown down the middle and two darker
  // ruts where the wheels run. The same band builder as the asphalt carries them, so each quad
  // takes the ground height at its own four corners and the track follows the slope exactly.
  // They stop on any paved cell, which is what makes the mouth at the ring or the arterial a
  // clean join instead of a dirt patch laid over the paint.
  {
    const C_TRACK_DUST = new THREE.Color(0xc0af8b);
    const C_TRACK_DUST_B = new THREE.Color(0xb3a17c);
    const C_TRACK_CROWN = new THREE.Color(0x8f8768);
    const C_TRACK_RUT = new THREE.Color(0x82715a);
    const trackScratch = new THREE.Color();
    const trackSkip = function (x, z) { return (flagsAt(x, z) & B_PAVED) !== 0; };
    const trackTone = function (x, z) {
      const h = hash2(Math.round(x * 0.45) + 3001, Math.round(z * 0.45) + 5003);
      return trackScratch.copy(C_TRACK_DUST).lerp(C_TRACK_DUST_B, h);
    };
    for (let t = 0; t < trackPaths.length; t += 1) {
      const path = trackPaths[t];
      band(path, -TRACK_HALF, TRACK_HALF, LIFT_TRACK, null, trackSkip, trackTone);
      band(path, -0.16, 0.16, LIFT_TRACK + 0.008, C_TRACK_CROWN, trackSkip, null);
      band(path, -0.82, -0.36, LIFT_TRACK + 0.014, C_TRACK_RUT, trackSkip, null);
      band(path, 0.36, 0.82, LIFT_TRACK + 0.014, C_TRACK_RUT, trackSkip, null);
    }
  }

  // Avenue: a tram-lined boulevard. Planted median in the middle, the tram beds either side of
  // it, a carriageway outside each bed, pavements on the outside. The median is cut at every
  // zebra and at the roundabout throat, so a pedestrian crosses street to street on the paint.
  const bvSkipRound = function (x, z) {
    return inRoundabout(x, z, ROUND_R - 0.3);
  };
  band(avePath, AVE_CAR_IN, AVE_HALF, LIFT_ROAD, null, bvSkipRound, asphaltAt);
  band(avePath, -AVE_HALF, -AVE_CAR_IN, LIFT_ROAD, null, bvSkipRound, asphaltAt);
  const avePaveSkip = function (x, z) {
    return inRoundabout(x, z, ROUND_R) || (flagsAt(x, z) & B_RING) !== 0;
  };
  band(avePath, AVE_HALF, AVE_HALF + AVE_PAVE, LIFT_PAVE, null, avePaveSkip, paveAt);
  band(avePath, -AVE_HALF - AVE_PAVE, -AVE_HALF, LIFT_PAVE, null, avePaveSkip, paveAt);
  // The median: a kerbed planter, cut wherever a zebra crosses it.
  band(avePath, -AVE_MEDIAN, AVE_MEDIAN, LIFT_MEDIAN + 0.02, C_MEDIAN_SOIL, function (x, z) {
    return inRoundabout(x, z, ROUND_R - 0.5) || nearCrossing(x, z, 1.2);
  });
  const medianKerb = function (x, z) {
    return inRoundabout(x, z, ROUND_R - 0.5) || nearCrossing(x, z, 1.2);
  };
  // Lane divider dashes inside each carriageway, and the white edge line at the outer kerb.
  const laneMark = function (x, z) {
    return inRoundabout(x, z, ROUND_R + 1) || (flagsAt(x, z) & B_RING) !== 0;
  };
  dashes(avePath, 0.12, 3, 4.5, LIFT_PAINT, laneMark);
  // A double white edge line against each kerb: two 0.12 m lines with a 0.2 m gap. From the air
  // the pair reads as one crisp band, and at street tier it is the widest paint on the road.
  paintStrip(avePath, AVE_HALF - 0.62, AVE_HALF - 0.5, LIFT_PAINT, laneMark);
  paintStrip(avePath, AVE_HALF - 0.3, AVE_HALF - 0.18, LIFT_PAINT, laneMark);
  paintStrip(avePath, -AVE_HALF + 0.18, -AVE_HALF + 0.3, LIFT_PAINT, laneMark);
  paintStrip(avePath, -AVE_HALF + 0.5, -AVE_HALF + 0.62, LIFT_PAINT, laneMark);
  paintStrip(avePath, AVE_CAR_IN + 0.2, AVE_CAR_IN + 0.36, LIFT_PAINT, laneMark);
  paintStrip(avePath, -AVE_CAR_IN - 0.36, -AVE_CAR_IN - 0.2, LIFT_PAINT, laneMark);
  // Kerb stones: the boulevard's outer kerbs, and the stone edges of the tram bed.
  kerbStones(avePath, AVE_HALF, AVE_HALF + 0.28, LIFT_KERB, avePaveSkip);
  kerbStones(avePath, -AVE_HALF - 0.28, -AVE_HALF, LIFT_KERB, avePaveSkip);
  kerbStones(avePath, TRAM_BED + 0.15, TRAM_BED + 0.41, LIFT_KERB, laneMark);
  kerbStones(avePath, -TRAM_BED - 0.41, -TRAM_BED - 0.15, LIFT_KERB, laneMark);
  // Kerbed median edges, as cut stone rather than one painted band.
  kerbStones(avePath, AVE_MEDIAN, AVE_MEDIAN + 0.25, LIFT_MEDIAN + 0.03, medianKerb);
  kerbStones(avePath, -AVE_MEDIAN - 0.25, -AVE_MEDIAN, LIFT_MEDIAN + 0.03, medianKerb);

  // ------------------------------------------------------------------ street surface incident
  // The street references are explicit that the carriageway and its pavement carry detail:
  // the wear of a used road, the cast iron of its drains, joints, and tactile paving at the
  // crossings. Everything here is a flat quad, or a triangle fan for a cover, added to the
  // paving and paint builders that already exist, so the whole block costs no draw call. Each
  // piece takes its height from the road or pavement band that is actually drawn, so it
  // follows the band's cross-slope instead of the terrain under it, and every offset stays
  // below the 58 mm road paint lift so no marking is covered: repair patches 12 mm, wheel
  // tracks 24 mm, gullies and covers 30 to 32 mm, pavement joints 16 mm and tactile bands
  // 32 mm above their pavement. The smallest, the 12 mm patch, is still about two depth steps
  // at the 411 m district distance, so none of them shimmers against the surface it lies on.
  const C_ASPHALT_PATCH = new THREE.Color(0x302c28);
  const C_ASPHALT_TRACK = new THREE.Color(0x46403a);
  const C_IRON = new THREE.Color(0x3f4348);
  const C_IRON_DEEP = new THREE.Color(0x2b2e33);
  const C_IRON_HI = new THREE.Color(0x585d63);
  const C_TACTILE = new THREE.Color(0x7c4c32);
  const C_TACTILE_B = new THREE.Color(0x8a5739);

  // The road and pavement bands are planes between their two edge lines, built from the
  // ground at those edges, so an overlay that measured the terrain under the middle of the
  // road would sink wherever the cross-section is concave. This reads the band's own surface
  // instead: the edge ground at the two path samples that bracket the point, interpolated
  // across to the point's lateral offset, so an overlay sits on the plane that is actually
  // drawn.
  function bandSurface(path, a, b, lift, x, z) {
    let best = 0;
    let bd = Infinity;
    let bt = 0;
    for (let i = 0; i < path.n; i += 1) {
      const x0 = path.xs[i];
      const z0 = path.zs[i];
      const dx = path.xs[i + 1] - x0;
      const dz = path.zs[i + 1] - z0;
      const l2 = dx * dx + dz * dz || 1;
      let t = ((x - x0) * dx + (z - z0) * dz) / l2;
      t = t < 0 ? 0 : t > 1 ? 1 : t;
      const px = x0 + dx * t;
      const pz = z0 + dz * t;
      const d = (x - px) * (x - px) + (z - pz) * (z - pz);
      if (d < bd) {
        bd = d;
        best = i;
        bt = t;
      }
    }
    const i = best;
    const j = i + 1;
    const t = bt;
    const ga0 = groundHeightAt(path.xs[i] + path.nx[i] * a, path.zs[i] + path.nz[i] * a);
    const ga1 = groundHeightAt(path.xs[j] + path.nx[j] * a, path.zs[j] + path.nz[j] * a);
    const gb0 = groundHeightAt(path.xs[i] + path.nx[i] * b, path.zs[i] + path.nz[i] * b);
    const gb1 = groundHeightAt(path.xs[j] + path.nx[j] * b, path.zs[j] + path.nz[j] * b);
    const ga = ga0 + (ga1 - ga0) * t;
    const gb = gb0 + (gb1 - gb0) * t;
    const nx = path.nx[i] + (path.nx[j] - path.nx[i]) * t;
    const nz = path.nz[i] + (path.nz[j] - path.nz[i]) * t;
    const nl = Math.sqrt(nx * nx + nz * nz) || 1;
    const cx = path.xs[i] + (path.xs[j] - path.xs[i]) * t;
    const cz = path.zs[i] + (path.zs[j] - path.zs[i]) * t;
    const lat = ((x - cx) * nx + (z - cz) * nz) / nl;
    return ga + (gb - ga) * ((lat - a) / (b - a)) + lift;
  }

  // flatQuad with an explicit height per corner, so an overlay can follow the band plane
  // under it. Same winding test as flatQuad, so every face still points up.
  function surfaceQuad(mesh, ax, az, bx, bz, cx, cz, dx, dz, ya, yb, yc, yd, col) {
    normalAt(ax, az, v3b);
    const a = mesh.vert(ax, ya, az, v3b.x, v3b.y, v3b.z, col);
    normalAt(bx, bz, v3b);
    const b = mesh.vert(bx, yb, bz, v3b.x, v3b.y, v3b.z, col);
    normalAt(cx, cz, v3b);
    const c = mesh.vert(cx, yc, cz, v3b.x, v3b.y, v3b.z, col);
    normalAt(dx, dz, v3b);
    const d = mesh.vert(dx, yd, dz, v3b.x, v3b.y, v3b.z, col);
    const cross = (bz - az) * (cx - ax) - (bx - ax) * (cz - az);
    if (cross > 0) {
      mesh.idx(a, b, c);
      mesh.idx(a, c, d);
    } else {
      mesh.idx(a, c, b);
      mesh.idx(a, d, c);
    }
  }

  // Polished wheel tracks: two pale strips per lane, 0.42 m wide, at the 1.56 m track of a
  // passenger car. One strip per wheel path, so a lane reads as two polished lines rather
  // than one washed band. Every second path sample, a 3 m chord, is straight enough inside a
  // lane and halves the quad count against the 1.5 m pavement sampling.
  function wheelTracks(path, laneCentre, skip, bandPos, bandNeg) {
    const half = 0.78;
    const width = 0.21;
    for (let i = 0; i + 2 <= path.n; i += 2) {
      const j = i + 2;
      for (let s = -1; s <= 1; s += 2) {
        for (let t = -1; t <= 1; t += 2) {
          const off = s * laneCentre + t * half;
          const ax = path.xs[i] + path.nx[i] * (off - width);
          const az = path.zs[i] + path.nz[i] * (off - width);
          const bx = path.xs[j] + path.nx[j] * (off - width);
          const bz = path.zs[j] + path.nz[j] * (off - width);
          const cx = path.xs[j] + path.nx[j] * (off + width);
          const cz = path.zs[j] + path.nz[j] * (off + width);
          const dx = path.xs[i] + path.nx[i] * (off + width);
          const dz = path.zs[i] + path.nz[i] * (off + width);
          const mx = (path.xs[i] + path.xs[j]) * 0.5 + (path.nx[i] + path.nx[j]) * 0.5 * off;
          const mz = (path.zs[i] + path.zs[j]) * 0.5 + (path.nz[i] + path.nz[j]) * 0.5 * off;
          if (skip && (skip(mx, mz) || skip(ax, az) || skip(bx, bz) ||
            skip(cx, cz) || skip(dx, dz))) continue;
          const band = off >= 0 ? bandPos : bandNeg;
          const lift = LIFT_ROAD + 0.024;
          surfaceQuad(paving, ax, az, bx, bz, cx, cz, dx, dz,
            bandSurface(path, band[0], band[1], lift, ax, az),
            bandSurface(path, band[0], band[1], lift, bx, bz),
            bandSurface(path, band[0], band[1], lift, cx, cz),
            bandSurface(path, band[0], band[1], lift, dx, dz),
            C_ASPHALT_TRACK);
        }
      }
    }
  }

  // Repair patches: a darker rectangle, 0.9 to 1.9 m wide and 3 m long, keyed on its path
  // index, so a rebuild puts every patch back in the same metre.
  function repairPatches(path, lanes, seed, skip, bandPos, bandNeg) {
    for (let i = 4; i + 2 < path.n; i += 16) {
      if (hash2(i + seed * 37, seed + 91) < 0.34) continue;
      const lane = lanes[(hash2(i, seed + 5) * lanes.length) | 0];
      const w = 0.45 + 0.5 * hash2(i, seed + 9);
      const off = lane + (hash2(i, seed + 13) - 0.5) * 1.1;
      const j = i + 2;
      const mx = (path.xs[i] + path.xs[j]) * 0.5 + (path.nx[i] + path.nx[j]) * 0.5 * off;
      const mz = (path.zs[i] + path.zs[j]) * 0.5 + (path.nz[i] + path.nz[j]) * 0.5 * off;
      const ax = path.xs[i] + path.nx[i] * (off - w);
      const az = path.zs[i] + path.nz[i] * (off - w);
      const bx = path.xs[j] + path.nx[j] * (off - w);
      const bz = path.zs[j] + path.nz[j] * (off - w);
      const cx = path.xs[j] + path.nx[j] * (off + w);
      const cz = path.zs[j] + path.nz[j] * (off + w);
      const dx = path.xs[i] + path.nx[i] * (off + w);
      const dz = path.zs[i] + path.nz[i] * (off + w);
      if (skip && (skip(mx, mz) || skip(ax, az) || skip(bx, bz) ||
        skip(cx, cz) || skip(dx, dz))) continue;
      if (nearCrossing(mx, mz, 2.2)) continue;
      const band = off >= 0 ? bandPos : bandNeg;
      const lift = LIFT_ROAD + 0.012;
      surfaceQuad(paving, ax, az, bx, bz, cx, cz, dx, dz,
        bandSurface(path, band[0], band[1], lift, ax, az),
        bandSurface(path, band[0], band[1], lift, bx, bz),
        bandSurface(path, band[0], band[1], lift, cx, cz),
        bandSurface(path, band[0], band[1], lift, dx, dz),
        C_ASPHALT_PATCH);
    }
  }

  // Lateral extents of the paint on each carriageway, read off the paintStrip and dashes
  // calls above. A gully or cover is only placed when its own radial extent stays at least
  // 0.10 m clear of every one of them, so the cast iron never covers a line or a dash.
  const RING_PAINT_LAT = [[-4.14, -3.96], [-0.13, 0.13], [3.96, 4.14]];
  const AVE_PAINT_LAT = [
    [-12.81, -12.65], [-12.45, -12.33], [-7.51, -7.35],
    [7.35, 7.51], [12.33, 12.45], [12.65, 12.81]
  ];
  const SPUR_PAINT_LAT = [[-4.76, -4.58], [-0.12, 0.12], [4.58, 4.76]];
  function lateralClear(lat, half, ranges) {
    for (let k = 0; k < ranges.length; k += 1) {
      if (lat + half > ranges[k][0] - 0.10 && lat - half < ranges[k][1] + 0.10) return false;
    }
    return true;
  }

  // Gully grating: a cast-iron frame and the darker grate, 2 mm above the frame, 0.38 m
  // across the kerb line and 0.55 m along it. It is refused when it would touch a painted
  // line, and when it is within 4.5 m of any painted zebra footprint, which is the registry
  // every zebra fills as it is drawn, so no grate can land in a crossing on any road.
  function gullyAt(path, i, lat, paintRanges, bandPos, bandNeg) {
    if (!lateralClear(lat, 0.19, paintRanges)) return false;
    const nx = path.nx[i];
    const nz = path.nz[i];
    const tx = path.nz[i];
    const tz = -path.nx[i];
    const px = path.xs[i];
    const pz = path.zs[i];
    const mx = px + nx * lat;
    const mz = pz + nz * lat;
    if (inRoundabout(mx, mz, ROUND_R) || !crossingClear(mx, mz, 4.5)) return false;
    const band = lat >= 0 ? bandPos : bandNeg;
    const fa = LIFT_ROAD + 0.030;
    const ax = px - tx * 0.275 + nx * (lat - 0.19);
    const az = pz - tz * 0.275 + nz * (lat - 0.19);
    const bx = px + tx * 0.275 + nx * (lat - 0.19);
    const bz = pz + tz * 0.275 + nz * (lat - 0.19);
    const cx = px + tx * 0.275 + nx * (lat + 0.19);
    const cz = pz + tz * 0.275 + nz * (lat + 0.19);
    const dx = px - tx * 0.275 + nx * (lat + 0.19);
    const dz = pz - tz * 0.275 + nz * (lat + 0.19);
    surfaceQuad(paving, ax, az, bx, bz, cx, cz, dx, dz,
      bandSurface(path, band[0], band[1], fa, ax, az),
      bandSurface(path, band[0], band[1], fa, bx, bz),
      bandSurface(path, band[0], band[1], fa, cx, cz),
      bandSurface(path, band[0], band[1], fa, dx, dz),
      C_IRON_HI);
    const ga = LIFT_ROAD + 0.032;
    surfaceQuad(paving,
      px - tx * 0.235 + nx * (lat - 0.15), pz - tz * 0.235 + nz * (lat - 0.15),
      px + tx * 0.235 + nx * (lat - 0.15), pz + tz * 0.235 + nz * (lat - 0.15),
      px + tx * 0.235 + nx * (lat + 0.15), pz + tz * 0.235 + nz * (lat + 0.15),
      px - tx * 0.235 + nx * (lat + 0.15), pz - tz * 0.235 + nz * (lat + 0.15),
      bandSurface(path, band[0], band[1], ga,
        px - tx * 0.235 + nx * (lat - 0.15), pz - tz * 0.235 + nz * (lat - 0.15)),
      bandSurface(path, band[0], band[1], ga,
        px + tx * 0.235 + nx * (lat - 0.15), pz + tz * 0.235 + nz * (lat - 0.15)),
      bandSurface(path, band[0], band[1], ga,
        px + tx * 0.235 + nx * (lat + 0.15), pz + tz * 0.235 + nz * (lat + 0.15)),
      bandSurface(path, band[0], band[1], ga,
        px - tx * 0.235 + nx * (lat + 0.15), pz - tz * 0.235 + nz * (lat + 0.15)),
      C_IRON_DEEP);
    return true;
  }

  // Round cover: a twelve-segment cast-iron disc, 0.68 m across, its rim tones alternating
  // so the radial pattern reads at street tier without a texture. One fan, twelve triangles.
  function coverAt(path, cx, cz, lat, paintRanges, bandPos, bandNeg) {
    const r = 0.34;
    if (!lateralClear(lat, r, paintRanges)) return false;
    if (inRoundabout(cx, cz, ROUND_R) || !crossingClear(cx, cz, 4.5)) return false;
    const band = lat >= 0 ? bandPos : bandNeg;
    const lift = LIFT_ROAD + 0.030;
    const N = 12;
    normalAt(cx, cz, v3b);
    const ci = paving.vert(cx, bandSurface(path, band[0], band[1], lift, cx, cz), cz,
      v3b.x, v3b.y, v3b.z, C_IRON_HI);
    const rim = [];
    for (let k = 0; k < N; k += 1) {
      const a = (k / N) * Math.PI * 2;
      const x = cx + Math.cos(a) * r;
      const z = cz + Math.sin(a) * r;
      normalAt(x, z, v3b);
      rim.push(paving.vert(x, bandSurface(path, band[0], band[1], lift, x, z), z,
        v3b.x, v3b.y, v3b.z, (k & 1) ? C_IRON : C_IRON_DEEP));
    }
    for (let k = 0; k < N; k += 1) paving.idx(ci, rim[(k + 1) % N], rim[k]);
    return true;
  }

  // Pavement joints: a 0.064 m line across the pavement at every path sample, which is the
  // 1.5 m pitch of the sampled pavement, so each joint lands exactly on a band boundary. Low
  // contrast by construction: the joint is the pavement's own tone at 0.9, never a black line.
  const jointScratch = new THREE.Color();
  // A 1.5 m joint pitch is 4 px wide on a 412 px phone at street tier, so the touch build lays
  // them every second sample: the pitch doubles to 3 m, which is still a pavement grid, and the
  // phone night frame keeps 1.4k triangles of headroom.
  const JOINT_STEP = isTouch ? 2 : 1;
  function paveJoints(path, a, b, skip, closed) {
    const last = closed ? path.n - 1 : path.n;
    for (let i = 0; i <= last; i += JOINT_STEP) {
      const px = path.xs[i];
      const pz = path.zs[i];
      const nx = path.nx[i];
      const nz = path.nz[i];
      const tx = path.nz[i];
      const tz = -path.nx[i];
      const mx = px + nx * (a + b) * 0.5;
      const mz = pz + nz * (a + b) * 0.5;
      if (skip && skip(mx, mz)) continue;
      const c = jointScratch.copy(paveAt(mx, mz)).multiplyScalar(0.9);
      const lift = LIFT_PAVE + 0.016;
      const ax = px - tx * 0.032 + nx * a;
      const az = pz - tz * 0.032 + nz * a;
      const bx = px + tx * 0.032 + nx * a;
      const bz = pz + tz * 0.032 + nz * a;
      const cx = px + tx * 0.032 + nx * b;
      const cz = pz + tz * 0.032 + nz * b;
      const dx = px - tx * 0.032 + nx * b;
      const dz = pz - tz * 0.032 + nz * b;
      surfaceQuad(paving, ax, az, bx, bz, cx, cz, dx, dz,
        bandSurface(path, a, b, lift, ax, az),
        bandSurface(path, a, b, lift, bx, bz),
        bandSurface(path, a, b, lift, cx, cz),
        bandSurface(path, a, b, lift, dx, dz),
        c);
    }
  }

  // Tactile paving at a zebra's kerb ends: three 0.2 m ribs make up the 0.6 m band, aligned
  // to the crossing. It starts 0.30 m from the kerb line, which is the back edge of the 0.28 m
  // kerb stone, and sits on the pavement at the crossing's own width, taking its height from
  // the pavement band so it follows the slope of the kerb. The paint mesh already carries the
  // crossing, so the band joins it at no extra draw call.
  function tactileBand(path, cx, cz, tx, tz, halfDepth, latIn, latOut, bandPos, bandNeg) {
    const l = Math.sqrt(tx * tx + tz * tz) || 1;
    tx /= l;
    tz /= l;
    const nx = -tz;
    const nz = tx;
    const band = latIn >= 0 ? bandPos : bandNeg;
    const lift = LIFT_PAVE + 0.032;
    for (let k = 0; k < 3; k += 1) {
      const a = latIn + (latOut - latIn) * (k / 3);
      const b = latIn + (latOut - latIn) * ((k + 1) / 3);
      const c = (k & 1) ? C_TACTILE_B : C_TACTILE;
      const ax = cx + nx * a - tx * halfDepth;
      const az = cz + nz * a - tz * halfDepth;
      const bx = cx + nx * a + tx * halfDepth;
      const bz = cz + nz * a + tz * halfDepth;
      const qx = cx + nx * b + tx * halfDepth;
      const qz = cz + nz * b + tz * halfDepth;
      const dx = cx + nx * b - tx * halfDepth;
      const dz = cz + nz * b - tz * halfDepth;
      surfaceQuad(paint, ax, az, bx, bz, qx, qz, dx, dz,
        bandSurface(path, band[0], band[1], lift, ax, az),
        bandSurface(path, band[0], band[1], lift, bx, bz),
        bandSurface(path, band[0], band[1], lift, qx, qz),
        bandSurface(path, band[0], band[1], lift, dx, dz),
        c);
    }
  }

  // The carriageways that get the wear. The ring and the boulevard are the two the street
  // tier frames, the arterials are their continuation off the map; the 1 m grid sidewalks
  // carry their own six-tone paving and go without the extra lines.
  const ringAsphaltSkip = function (x, z) { return inRoundabout(x, z, ROUND_R - 0.3); };
  const spurRingSkip = function (x, z) {
    return Math.abs(ringSigned(x, z)) < RING_HALF + RING_PAVE;
  };
  const RING_ASPHALT = [-RING_HALF, RING_HALF];
  const AVE_ASPHALT_POS = [AVE_CAR_IN, AVE_HALF];
  const AVE_ASPHALT_NEG = [-AVE_HALF, -AVE_CAR_IN];
  const SPUR_ASPHALT = [-SPUR_HALF, SPUR_HALF];
  wheelTracks(ringPath, 1.5, ringAsphaltSkip, RING_ASPHALT, RING_ASPHALT);
  wheelTracks(avePath, AVE_LANE_CENTRE, bvSkipRound, AVE_ASPHALT_POS, AVE_ASPHALT_NEG);
  for (let s = 0; s < spurPaths.length; s += 1) {
    wheelTracks(spurPaths[s], 2.5, spurRingSkip, SPUR_ASPHALT, SPUR_ASPHALT);
  }
  repairPatches(ringPath, [-1.5, 1.5], 3, ringAsphaltSkip, RING_ASPHALT, RING_ASPHALT);
  repairPatches(avePath, [-AVE_LANE_CENTRE, AVE_LANE_CENTRE], 11, bvSkipRound,
    AVE_ASPHALT_POS, AVE_ASPHALT_NEG);
  for (let s = 0; s < spurPaths.length; s += 1) {
    repairPatches(spurPaths[s], [-2.5, 2.5], 23 + s, spurRingSkip, SPUR_ASPHALT, SPUR_ASPHALT);
  }

  // Gully and cover positions are laid after the zebra block below, because the clearance
  // check has to see every crossing that was actually painted, avenue, spur and ring.
  const RING_PAVE_POS = [RING_HALF, RING_HALF + RING_PAVE];
  const RING_PAVE_NEG = [-RING_HALF - RING_PAVE, -RING_HALF];
  const AVE_PAVE_POS = [AVE_HALF, AVE_HALF + AVE_PAVE];
  const AVE_PAVE_NEG = [-AVE_HALF - AVE_PAVE, -AVE_HALF];
  const SPUR_PAVE_POS = [SPUR_HALF, SPUR_EDGE];
  const SPUR_PAVE_NEG = [-SPUR_EDGE, -SPUR_HALF];

  // Joints on the boulevard, the ring and the arterials: the pavements a street tier camera
  // stands on. Each road's own skip keeps a joint off the roundabout and off a junction mouth.
  paveJoints(ringPath, RING_HALF, RING_HALF + RING_PAVE, ringPaveSkip, true);
  paveJoints(ringPath, -RING_HALF - RING_PAVE, -RING_HALF, ringPaveSkip, true);
  paveJoints(avePath, AVE_HALF, AVE_HALF + AVE_PAVE, avePaveSkip);
  paveJoints(avePath, -AVE_HALF - AVE_PAVE, -AVE_HALF, avePaveSkip);
  for (let s = 0; s < spurPaths.length; s += 1) {
    paveJoints(spurPaths[s], SPUR_HALF, SPUR_EDGE, spurRingSkip);
    paveJoints(spurPaths[s], -SPUR_EDGE, -SPUR_HALF, spurRingSkip);
  }

  // Zebra paint: bold bars on every junction of the avenue, and on the mouth of each spur
  // where it meets the ring. Bars run with the traffic, so from above they read as a ladder of
  // white across the asphalt, and the arrows beside them point the way out of the junction.
  {
    for (let i = 0; i < crossings.length; i += 1) {
      const c = crossings[i];
      zebra(c.x, c.z, c.dirX, c.dirZ, AVE_HALF - 0.3, LIFT_PAINT + 0.012);
      // Tactile paving at both kerb ends: a 0.6 m band on the pavement, aligned to the zebra
      // and starting at the back edge of the kerb stones, the cue a contemporary crossing gives.
      for (let s = -1; s <= 1; s += 2) {
        const bx = c.x + -c.dirZ * s * (AVE_HALF + 0.60);
        const bz = c.z + c.dirX * s * (AVE_HALF + 0.60);
        if (avePaveSkip(bx, bz)) continue;
        tactileBand(avePath, c.x, c.z, c.dirX, c.dirZ, 1.95,
          s * (AVE_HALF + 0.30), s * (AVE_HALF + 0.90), AVE_PAVE_POS, AVE_PAVE_NEG);
      }
      // One arrow per carriageway, 5.6 m before the crossing on the approach side.
      for (let s = -1; s <= 1; s += 2) {
        const off = s * AVE_LANE_CENTRE;
        const ax = c.x + -c.dirZ * off - c.dirX * 5.6;
        const az = c.z + c.dirX * off - c.dirZ * 5.6;
        if (flagsAt(ax, az) & B_RING) continue;
        laneArrow(ax, az, c.dirX * s, c.dirZ * s, LIFT_PAINT);
      }
    }
    // Spur junctions: a zebra just outside the ring band on each arterial, plus the crossing
    // over the ring carriageway at each mouth, so the junction is legible from both roads.
    for (let s = 0; s < spurPaths.length; s += 1) {
      const path = spurPaths[s];
      let inside = -1;
      for (let i = 0; i <= path.n; i += 1) {
        if (Math.abs(ringSigned(path.xs[i], path.zs[i])) > RING_HALF + RING_PAVE + 1.0) {
          inside = i;
          break;
        }
      }
      if (inside >= 0 && inside + 2 < path.n) {
        const i = inside + 2;
        const tx = (path.xs[i + 1] - path.xs[i]) / path.step;
        const tz = (path.zs[i + 1] - path.zs[i]) / path.step;
        zebra(path.xs[i], path.zs[i], tx, tz, SPUR_HALF - 0.3, LIFT_PAINT + 0.012);
        for (let side = -1; side <= 1; side += 2) {
          tactileBand(path, path.xs[i], path.zs[i], tx, tz, 1.95,
            side * (SPUR_HALF + 0.30), side * (SPUR_HALF + 0.90),
            SPUR_PAVE_POS, SPUR_PAVE_NEG);
        }
      }
      // The ring's own crossing, walked over the carriageway beside the spur mouth.
      let best = 0;
      let bd = Infinity;
      for (let i = 0; i <= ringPath.n; i += 1) {
        const dx = ringPath.xs[i] - path.xs[0];
        const dz = ringPath.zs[i] - path.zs[0];
        const d = dx * dx + dz * dz;
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      for (let sgn = -1; sgn <= 1; sgn += 2) {
        const i = best + sgn * 6;
        if (i < 0 || i > ringPath.n) continue;
        const j = (i + 1) % (ringPath.n + 1);
        const rtx = ringPath.xs[j] - ringPath.xs[i];
        const rtz = ringPath.zs[j] - ringPath.zs[i];
        const rtlen = Math.sqrt(rtx * rtx + rtz * rtz) || 1;
        zebra(ringPath.xs[i], ringPath.zs[i], rtx, rtz,
          RING_HALF - 0.32, LIFT_PAINT + 0.012);
        for (let side = -1; side <= 1; side += 2) {
          const bx = ringPath.xs[i] + ringPath.nx[i] * side * (RING_HALF + 0.60);
          const bz = ringPath.zs[i] + ringPath.nz[i] * side * (RING_HALF + 0.60);
          if (ringPaveSkip(bx, bz)) continue;
          tactileBand(ringPath, ringPath.xs[i], ringPath.zs[i], rtx, rtz, 1.95 * rtlen,
            side * (RING_HALF + 0.30), side * (RING_HALF + 0.90),
            RING_PAVE_POS, RING_PAVE_NEG);
        }
      }
    }
  }

  // Gullies at the boulevard's inner kerb, one in each gap between junctions, alternating
  // sides. The gully's inner edge, 7.73 m, stays 0.22 m outboard of the inner edge paint
  // (7.35 to 7.51) and 1.69 m inboard of the bus cage (9.8 to 12.4). Every candidate is
  // refused unless its own radial extent clears the paint by 0.10 m and it stands 4.5 m clear
  // of every painted zebra.
  {
    const stops = [70.8, 42, 9, -22, -52, -84];
    for (let k = 0; k < stops.length; k += 1) {
      let best = 0;
      let bd = Infinity;
      for (let i = 0; i <= avePath.n; i += 1) {
        const d = Math.abs(avePath.zs[i] - stops[k]);
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      if (bd > 3) continue;
      const side = (k & 1) ? -1 : 1;
      gullyAt(avePath, best, side * (AVE_CAR_IN + 0.77), AVE_PAINT_LAT,
        AVE_ASPHALT_POS, AVE_ASPHALT_NEG);
    }
    const spurGullies = [[14, 1], [30, -1], [22, 1]];
    for (let s = 0; s < spurPaths.length; s += 1) {
      const g = spurGullies[s];
      if (!g || g[0] > spurPaths[s].n) continue;
      gullyAt(spurPaths[s], g[0], g[1] * (SPUR_HALF - 0.87), SPUR_PAINT_LAT,
        SPUR_ASPHALT, SPUR_ASPHALT);
    }
  }

  // Covers: four in the boulevard's inner lane and four between the ring's lane tracks, where
  // the cast iron stays clear of the centre dashes (0.13 m) and the edge lines (3.96 m) by
  // more than a metre.
  {
    const aveCovers = [[40, 1], [33, -1], [-45, 1], [-78, -1]];
    for (let k = 0; k < aveCovers.length; k += 1) {
      let best = 0;
      let bd = Infinity;
      for (let i = 0; i <= avePath.n; i += 1) {
        const d = Math.abs(avePath.zs[i] - aveCovers[k][0]);
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      if (bd > 3) continue;
      const lat = aveCovers[k][1] * 8.6;
      const cx = avePath.xs[best] + avePath.nx[best] * lat;
      const cz = avePath.zs[best] + avePath.nz[best] * lat;
      coverAt(avePath, cx, cz, lat, AVE_PAINT_LAT, AVE_ASPHALT_POS, AVE_ASPHALT_NEG);
    }
    const ringCovers = [[60, 1], [150, -1], [250, 1], [340, -1]];
    for (let k = 0; k < ringCovers.length; k += 1) {
      const i = ringCovers[k][0];
      if (i > ringPath.n) continue;
      const lat = ringCovers[k][1] * 1.65;
      const cx = ringPath.xs[i] + ringPath.nx[i] * lat;
      const cz = ringPath.zs[i] + ringPath.nz[i] * lat;
      coverAt(ringPath, cx, cz, lat, RING_PAINT_LAT, RING_ASPHALT, RING_ASPHALT);
    }
  }

  // Bus-stop cages: a long white box beside the kerb, hatched diagonally, placed either side of
  // every avenue junction, which is where the boulevard's shelters stand. This is the marking
  // that only exists at street scale: from the air the boulevard reads as a ladder of boxes and
  // stripes, and up close each bay has a legible piece of street furniture on the asphalt.
  {
    function pathNear(path, x, z) {
      let best = 0;
      let bd = Infinity;
      for (let i = 0; i <= path.n; i += 1) {
        const dx = path.xs[i] - x;
        const dz = path.zs[i] - z;
        const d = dx * dx + dz * dz;
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      return best;
    }
    function bandRun(path, i0, i1, a, b, lift) {
      for (let i = i0; i < i1; i += 1) {
        const j = i + 1;
        paint.flatQuad(
          path.xs[i] + path.nx[i] * a, path.zs[i] + path.nz[i] * a,
          path.xs[j] + path.nx[j] * a, path.zs[j] + path.nz[j] * a,
          path.xs[j] + path.nx[j] * b, path.zs[j] + path.nz[j] * b,
          path.xs[i] + path.nx[i] * b, path.zs[i] + path.nz[i] * b,
          lift, C_PAINT
        );
      }
    }
    function capAt(path, i, a, b, lift, thick) {
      const j = Math.min(path.n, i + 1);
      const k = Math.max(0, i - 1);
      const tx = path.xs[j] - path.xs[k];
      const tz = path.zs[j] - path.zs[k];
      const l = Math.sqrt(tx * tx + tz * tz) || 1;
      const h = thick || 0.13;
      const ux = (tx / l) * h * 0.5;
      const uz = (tz / l) * h * 0.5;
      paint.flatQuad(
        path.xs[i] + path.nx[i] * a - ux, path.zs[i] + path.nz[i] * a - uz,
        path.xs[i] + path.nx[i] * b - ux, path.zs[i] + path.nz[i] * b - uz,
        path.xs[i] + path.nx[i] * b + ux, path.zs[i] + path.nz[i] * b + uz,
        path.xs[i] + path.nx[i] * a + ux, path.zs[i] + path.nz[i] * a + uz,
        lift, C_PAINT
      );
    }
    const cageA = AVE_HALF - 3.15;
    const cageB = AVE_HALF - 0.55;
    const cageStep = Math.max(1, Math.round(14 / avePath.step));
    const gapStep = Math.max(1, Math.round(21 / avePath.step));
    const skew = Math.max(1, Math.round(0.85 / avePath.step));
    for (let ci = 0; ci < crossings.length; ci += 1) {
      const c = crossings[ci];
      const mid = pathNear(avePath, c.x, c.z);
      for (let sgn = -1; sgn <= 1; sgn += 2) {
        // A stop bar across each carriageway on the approach, 2.4 m before the crossing paint.
        const iStop = mid + sgn * Math.max(2, Math.round(2.4 / avePath.step));
        if (iStop > 1 && iStop < avePath.n - 1) {
          capAt(avePath, iStop, AVE_CAR_IN + 0.12, AVE_HALF - 0.12, LIFT_PAINT, 0.45);
        }
        const i0 = mid + sgn * gapStep;
        const i1 = i0 + sgn * cageStep;
        const lo = Math.min(i0, i1);
        const hi = Math.max(i0, i1);
        if (lo < 2 || hi > avePath.n - 2) continue;
        bandRun(avePath, lo, hi, cageA, cageA + 0.13, LIFT_PAINT);
        bandRun(avePath, lo, hi, cageB - 0.13, cageB, LIFT_PAINT);
        capAt(avePath, lo, cageA, cageB, LIFT_PAINT);
        capAt(avePath, hi, cageA, cageB, LIFT_PAINT);
        const nb = Math.max(3, Math.round(((hi - lo) * avePath.step) / 1.9));
        for (let k = 1; k < nb; k += 1) {
          const ik = Math.round(lo + ((hi - lo) * k) / nb);
          const ia = Math.max(lo, ik - skew);
          const ib = Math.min(hi, ik + skew);
          const x0 = avePath.xs[ia] + avePath.nx[ia] * cageA;
          const z0 = avePath.zs[ia] + avePath.nz[ia] * cageA;
          const x1 = avePath.xs[ib] + avePath.nx[ib] * cageB;
          const z1 = avePath.zs[ib] + avePath.nz[ib] * cageB;
          const dx = x1 - x0;
          const dz = z1 - z0;
          const l = Math.sqrt(dx * dx + dz * dz) || 1;
          const px = (-dz / l) * 0.075;
          const pz = (dx / l) * 0.075;
          paint.flatQuad(x0 - px, z0 - pz, x1 - px, z1 - pz,
            x1 + px, z1 + pz, x0 + px, z0 + pz, LIFT_PAINT, C_PAINT);
        }
      }
    }
  }

  // Roundabout: an asphalt annulus around a planted island. Each segment takes its own tone
  // from the same hashed wear family, so the apron is not one flat ring either.
  {
    const SEG = 48;
    const C = roundaboutCenter;
    const roundScratch = new THREE.Color();
    for (let s = 0; s < SEG; s += 1) {
      const a0 = (s / SEG) * Math.PI * 2;
      const a1 = ((s + 1) / SEG) * Math.PI * 2;
      const rc = roundScratch.copy(C_ASPHALT_ROUND)
        .multiplyScalar(0.95 + 0.1 * hash2(s + 640, 4400));
      paving.flatQuad(
        C.x + Math.cos(a0) * ISLAND_R, C.z + Math.sin(a0) * ISLAND_R,
        C.x + Math.cos(a1) * ISLAND_R, C.z + Math.sin(a1) * ISLAND_R,
        C.x + Math.cos(a1) * ROUND_R, C.z + Math.sin(a1) * ROUND_R,
        C.x + Math.cos(a0) * ROUND_R, C.z + Math.sin(a0) * ROUND_R,
        LIFT_ROAD - 0.01, rc
      );
    }
  }

  // Tram: a grass bed in the boulevard reservation, stone where it crosses the ring, carrying
  // two bright rails.
  band(tramPath, -1.75, 1.75, LIFT_BED, C_BED, null, function (x, z) {
    return (flagsAt(x, z) & B_RING) !== 0 && !inRoundabout(x, z, ROUND_R + 2) ? C_BED_CROSS : C_BED;
  });
  band(tramPath, -0.8, -0.52, LIFT_RAIL, C_RAIL, null);
  band(tramPath, 0.52, 0.8, LIFT_RAIL, C_RAIL, null);

  // Street grid: one asphalt quad per run, pavements in three metre steps.
  const streetScratch = new THREE.Color();
  function emitRuns(runs, vertical) {
    for (let i = 0; i < runs.length; i += 1) {
      const r = runs[i];
      if (r.t1 - r.t0 < 1) continue;
      const c = r.coord;
      const sc = hash2(i * 5 + 11, vertical ? 71 : 113);
      const streetTone = streetScratch.copy(C_STREET).multiplyScalar(0.955 + 0.09 * sc);
      if (vertical) {
        paving.flatQuad(c - STREET_HALF, r.t0, c - STREET_HALF, r.t1,
          c + STREET_HALF, r.t1, c + STREET_HALF, r.t0, LIFT_STREET, streetTone);
      } else {
        paving.flatQuad(r.t0, c - STREET_HALF, r.t1, c - STREET_HALF,
          r.t1, c + STREET_HALF, r.t0, c + STREET_HALF, LIFT_STREET, streetTone);
      }
      for (let side = -1; side <= 1; side += 2) {
        const a = side * STREET_HALF;
        const b = side * STREET_EDGE;
        for (let t = r.t0; t < r.t1; t += 3) {
          const t1 = Math.min(t + 3, r.t1);
          const mid = (t + t1) * 0.5;
          const mx = vertical ? c + (a + b) * 0.5 : mid;
          const mz = vertical ? mid : c + (a + b) * 0.5;
          if (flagsAt(mx, mz) & B_STREET) continue;
          const pc = paveAt(mx, mz);
          if (vertical) {
            paving.flatQuad(c + a, t, c + a, t1, c + b, t1, c + b, t, LIFT_STREET_PAVE, pc);
          } else {
            paving.flatQuad(t, c + a, t1, c + a, t1, c + b, t, c + b, LIFT_STREET_PAVE, pc);
          }
          // Kerb stones on the road edge: two stones per pavement step, alternating tone.
          const ka = side * STREET_HALF;
          const kb = side * (STREET_HALF + 0.24);
          for (let h = 0; h < 2; h += 1) {
            const s0 = t + (t1 - t) * h * 0.5;
            const s1 = t + (t1 - t) * (h + 1) * 0.5;
            const kc = ((i + h) & 1) ? C_KERB_STONE : C_KERB_STONE_B;
            if (vertical) {
              paving.flatQuad(c + ka, s0, c + ka, s1, c + kb, s1, c + kb, s0,
                LIFT_STREET_PAVE + 0.05, kc);
            } else {
              paving.flatQuad(s0, c + ka, s1, c + ka, s1, c + kb, s0, c + kb,
                LIFT_STREET_PAVE + 0.05, kc);
            }
          }
        }
      }
    }
  }
  emitRuns(streetRunsV, true);
  emitRuns(streetRunsH, false);

  // Block courtyards: the inside of every grid cell is yard or garden, never open grass.
  for (let i = 0; i < CELL_X.length - 1; i += 1) {
    for (let j = 0; j < CELL_Z.length - 1; j += 1) {
      const x0 = CELL_X[i] + STREET_EDGE;
      const x1 = CELL_X[i + 1] - STREET_EDGE;
      const z0 = CELL_Z[j] + STREET_EDGE;
      const z1 = CELL_Z[j + 1] - STREET_EDGE;
      if (x1 - x0 < 4 || z1 - z0 < 4) continue;
      for (let a = 0; a < 2; a += 1) {
        for (let b = 0; b < 2; b += 1) {
          const qx0 = x0 + (x1 - x0) * 0.5 * a;
          const qx1 = x0 + (x1 - x0) * 0.5 * (a + 1);
          const qz0 = z0 + (z1 - z0) * 0.5 * b;
          const qz1 = z0 + (z1 - z0) * 0.5 * (b + 1);
          const cx = (qx0 + qx1) * 0.5;
          const cz = (qz0 + qz1) * 0.5;
          if (!inUrban(cx, cz) && !inRingFrontage(cx, cz)) continue;
          const green = hash2(i * 4 + a, j * 4 + b) < 0.26;
          paving.flatQuad(qx0, qz0, qx1, qz0, qx1, qz1, qx0, qz1,
            LIFT_STREET - 0.005, green ? C_COURT_G : C_COURT);
        }
      }
    }
  }

  // Dream corridor: a vacant gravel lot waiting for the provisional road.
  if (corridor.has) {
    const steps = Math.max(1, Math.ceil(corridor.len / 2.6));
    const nx = -corridor.dz;
    const nz = corridor.dx;
    for (let i = 0; i < steps; i += 1) {
      const a = i / steps;
      const b = (i + 1) / steps;
      const ax = corridor.ax + (corridor.bx - corridor.ax) * a;
      const az = corridor.az + (corridor.bz - corridor.az) * a;
      const bx = corridor.ax + (corridor.bx - corridor.ax) * b;
      const bz = corridor.az + (corridor.bz - corridor.az) * b;
      const mx = (ax + bx) * 0.5;
      const mz = (az + bz) * 0.5;
      if (flagsAt(mx, mz) & (B_ROADS | B_STREET | B_PLAZA)) continue;
      if (!inUrban(mx, mz) && !inRingFrontage(mx, mz)) continue;
      const hw = 6.4;
      const col = hash2(i * 7, 3) < 0.26 ? C_LOT_GRASS : C_LOT;
      paving.flatQuad(
        ax - nx * hw, az - nz * hw,
        bx - nx * hw, bz - nz * hw,
        bx + nx * hw, bz + nz * hw,
        ax + nx * hw, az + nz * hw,
        LIFT_STREET + 0.012, col
      );
    }
  }

  // Market square: stone paving in two tones with a darker border.
  {
    const cell = 1.8;
    const nx = Math.round(PLAZA_W / cell);
    const nz = Math.round(PLAZA_D / cell);
    const x0 = PLAZA_CX - PLAZA_W * 0.5;
    const z0 = PLAZA_CZ - PLAZA_D * 0.5;
    const sx = PLAZA_W / nx;
    const sz = PLAZA_D / nz;
    for (let j = 0; j < nz; j += 1) {
      for (let i = 0; i < nx; i += 1) {
        const edge = i === 0 || j === 0 || i === nx - 1 || j === nz - 1;
        const col = edge ? C_PLAZA_EDGE : ((i + j) & 1) ? C_PLAZA_A : C_PLAZA_B;
        const ax = x0 + i * sx;
        const az = z0 + j * sz;
        paving.flatQuad(ax, az, ax + sx, az, ax + sx, az + sz, ax, az + sz, LIFT_PLAZA, col);
      }
    }
  }

  // The crown's lanes: three narrow stone alleys between the stone houses, from the first
  // terrace at the cypresses to the last one at the avenue kerb. The paving stops a hand's
  // width inside the house lines, so it reads as a gutter against every wall.
  {
    const lanes = OLD_LANES;
    for (let i = 0; i < lanes.length; i += 1) {
      const z = lanes[i];
      for (let x = OLD_PAVE_X0; x < OLD_PAVE_X1; x += 2.6) {
        const x1 = Math.min(x + 2.6, OLD_PAVE_X1);
        paving.flatQuad(x, z - OLD_ALLEY, x1, z - OLD_ALLEY, x1, z + OLD_ALLEY, x, z + OLD_ALLEY,
          LIFT_STREET_PAVE, C_PLAZA_EDGE);
      }
    }
    // A small stone apron in front of the church, up to the village wall line.
    paving.flatQuad(-60, -95, OLD_X0 - 0.2, -95, OLD_X0 - 0.2, -91, -60, -91,
      LIFT_STREET_PAVE, C_PLAZA_B);
  }

  // ------------------------------------------------------------------ civic aprons
  // Hardstanding, car parks and pitches for the civic plots, in the same plot frame as their
  // buildings: local +X is the plot's width axis, local +Z is its depth axis.
  function plotQuad(r, lu0, lv0, lu1, lv1, lift, col) {
    const c = Math.cos(r.yaw);
    const s = Math.sin(r.yaw);
    paving.flatQuad(
      r.x + lu0 * c + lv0 * s, r.z - lu0 * s + lv0 * c,
      r.x + lu1 * c + lv0 * s, r.z - lu1 * s + lv0 * c,
      r.x + lu1 * c + lv1 * s, r.z - lu1 * s + lv1 * c,
      r.x + lu0 * c + lv1 * s, r.z - lu0 * s + lv1 * c,
      lift, col
    );
  }

  for (let i = 0; i < civic.length; i += 1) {
    const r = civic[i];
    const lift = 0.15;
    if (r.type === 'mart') {
      plotQuad(r, -21, -16.8, 21, -5.2, lift, C_APRON);
      for (let k = 0; k < 13; k += 1) {
        const lu = -19.2 + k * 3.2;
        plotQuad(r, lu - 0.07, -15.8, lu + 0.07, -10.6, lift + 0.012, C_BAY);
        plotQuad(r, lu - 0.07, -9.4, lu + 0.07, -5.4, lift + 0.012, C_BAY);
      }
      plotQuad(r, -3.4, -18.2, 3.4, -16.8, lift + 0.008, C_BAY);
    } else if (r.type === 'fuel') {
      plotQuad(r, -11.4, -12.4, 11.4, -1.6, lift, C_APRON);
      for (let k = -1; k <= 1; k += 1) {
        plotQuad(r, k * 4.4 - 1.9, -3.9, k * 4.4 + 1.9, -0.1, lift + 0.01, C_BAY);
      }
    } else if (r.type === 'school') {
      for (let k = 0; k < 8; k += 1) {
        plotQuad(r, -17, -20 + k * 2.25, 17, -20 + (k + 1) * 2.25, lift,
          (k & 1) ? C_TURF : C_TURF_MOWN);
      }
      // Touchlines and a half-way line.
      plotQuad(r, -17, -20, 17, -19.7, lift + 0.01, C_BAY);
      plotQuad(r, -17, -2.3, 17, -2.0, lift + 0.01, C_BAY);
      plotQuad(r, -17, -20, -16.7, -2.0, lift + 0.01, C_BAY);
      plotQuad(r, 16.7, -20, 17, -2.0, lift + 0.01, C_BAY);
      plotQuad(r, -0.15, -20, 0.15, -2.0, lift + 0.01, C_BAY);
      // An apron along the school's entrance side.
      plotQuad(r, -23, -2, 23, 2.4, lift, C_APRON);
    } else if (r.type === 'sport') {
      for (let k = 0; k < 9; k += 1) {
        plotQuad(r, -14, -20 + k * 2.1, 14, -20 + (k + 1) * 2.1, lift,
          (k & 1) ? C_TURF_MOWN : C_TURF);
      }
      plotQuad(r, -14, -20, 14, -19.7, lift + 0.01, C_BAY);
      plotQuad(r, -14, -1.3, 14, -1.0, lift + 0.01, C_BAY);
      plotQuad(r, -14, -20, -13.7, -1.0, lift + 0.01, C_BAY);
      plotQuad(r, 13.7, -20, 14, -1.0, lift + 0.01, C_BAY);
      plotQuad(r, -0.15, -20, 0.15, -1.0, lift + 0.01, C_BAY);
    } else if (r.type === 'warehouse') {
      plotQuad(r, 5.6, -13, 12, 13, lift, C_APRON);
    }
  }

  // Rail yard: ballast, sleepers and two rails per track.
  band(railPath, -7.8, 7.8, 0.12, C_BALLAST, null);
  for (let t = 0; t < TRACK_OFFS.length; t += 1) {
    const off = TRACK_OFFS[t];
    band(railPath, off - 0.82, off - 0.62, LIFT_RAIL, C_TRACK, null);
    band(railPath, off + 0.62, off + 0.82, LIFT_RAIL, C_TRACK, null);
    for (let i = 1; i < railPath.n; i += 1) {
      const x = railPath.xs[i] + railPath.nx[i] * off;
      const z = railPath.zs[i] + railPath.nz[i] * off;
      const tx = railPath.nz[i];
      const tz = -railPath.nx[i];
      const nx = railPath.nx[i];
      const nz = railPath.nz[i];
      paving.flatQuad(
        x - nx * 1.35 - tx * 0.22, z - nz * 1.35 - tz * 0.22,
        x + nx * 1.35 - tx * 0.22, z + nz * 1.35 - tz * 0.22,
        x + nx * 1.35 + tx * 0.22, z + nz * 1.35 + tz * 0.22,
        x - nx * 1.35 + tx * 0.22, z - nz * 1.35 + tz * 0.22,
        0.2, C_SLEEPER
      );
    }
  }

  // Level crossing where the east arterial crosses the four running lines: paved over the
  // rails, so the road reads as continuous and the rails stop under it.
  {
    const path = spurPaths[1];
    let best = 0;
    let bd = Infinity;
    for (let i = 0; i <= path.n; i += 1) {
      const d = Math.abs(path.xs[i] - RAIL_X);
      if (d < bd) {
        bd = d;
        best = i;
      }
    }
    const px = path.xs[best];
    const pz = path.zs[best];
    const nx = path.nx[best];
    const nz = path.nz[best];
    const tx = path.nz[best];
    const tz = -path.nx[best];
    paving.flatQuad(
      px - tx * 11 - nx * SPUR_HALF, pz - tz * 11 - nz * SPUR_HALF,
      px + tx * 11 - nx * SPUR_HALF, pz + tz * 11 - nz * SPUR_HALF,
      px + tx * 11 + nx * SPUR_HALF, pz + tz * 11 + nz * SPUR_HALF,
      px - tx * 11 + nx * SPUR_HALF, pz - tz * 11 + nz * SPUR_HALF,
      LIFT_RAIL + 0.03, C_STREET
    );
    for (let s = -1; s <= 1; s += 2) {
      const off = s * (SPUR_HALF - 0.7);
      paving.flatQuad(
        px - tx * 11 - nx * (off + 0.2), pz - tz * 11 - nz * (off + 0.2),
        px + tx * 11 - nx * (off + 0.2), pz + tz * 11 - nz * (off + 0.2),
        px + tx * 11 - nx * (off - 0.2), pz + tz * 11 - nz * (off - 0.2),
        px - tx * 11 - nx * (off - 0.2), pz - tz * 11 - nz * (off - 0.2),
        LIFT_RAIL + 0.045, C_TRACK_LINE
      );
    }
  }

  const pavingMesh = new THREE.Mesh(paving.build(), new THREE.MeshStandardMaterial({
    vertexColors: true,
    roughness: 0.88,
    metalness: 0.04
  }));
  pavingMesh.receiveShadow = shadowsOn;
  pavingMesh.name = 'paving';
  group.add(pavingMesh);

  // ------------------------------------------------------------------ the five frames
  // The app can look at the district in five ways: the district shot camera.mjs composes (45
  // degrees vertical, a 40 degree pitch, framed on the bounds sphere with the 1.05 pad), and the
  // four street-tier rotations, which are the same pitch at 0.38 of that distance. Two build
  // passes need to know what those frames can see, so the eyes are built once here instead of
  // each pass carrying its own copy of the camera numbers: the mural pass paints only a wall
  // whose panel is large enough and inside a frame, and the expressway paint has to widen its
  // lines to the distance they are read at. Written out rather than imported, because the rig
  // lives in camera.mjs and this file must not reach across the seam for a camera constant.
  const VIEW_CY = 12;
  const VIEW_CZ = 4;
  const VIEW_R = 114;
  const HALF_V = 0.5 * 45 * Math.PI / 180;
  const TAN_V = Math.tan(HALF_V);
  const TAN_H = TAN_V * (16 / 9);
  const VIEW_DISTRICT_D = (VIEW_R * 1.05) / Math.sin(HALF_V);
  const VIEW_SIN_P = Math.sin(40 * Math.PI / 180);
  const VIEW_COS_P = Math.cos(40 * Math.PI / 180);
  const EYE_X = 0;
  const EYE_Y = VIEW_CY + VIEW_DISTRICT_D * VIEW_SIN_P;
  const EYE_Z = VIEW_CZ + VIEW_DISTRICT_D * VIEW_COS_P;
  const FWD_Y = (VIEW_CY - EYE_Y) / VIEW_DISTRICT_D;
  const FWD_Z = (VIEW_CZ - EYE_Z) / VIEW_DISTRICT_D;
  const UP_Y = -FWD_Z;
  const UP_Z = FWD_Y;
  // The street tier: 0.38 of the district distance, eyes on that circle about the district
  // centre. The numbers come from the rig's own tier zoom and pitch; a rotation that drifted
  // would move the mural picks and the paint widths together, which is the point of sharing.
  const VIEW_STREET_D = VIEW_DISTRICT_D * 0.38;
  const VIEW_STREET_H = VIEW_STREET_D * VIEW_COS_P;
  const VIEW_STREET_Y = VIEW_CY + VIEW_STREET_D * VIEW_SIN_P;
  const FRAME_VIEWS = (function () {
    const out = [{
      ex: EYE_X, ey: EYE_Y, ez: EYE_Z,
      fx: 0, fy: FWD_Y, fz: FWD_Z,
      rx: 1, ry: 0, rz: 0,
      ux: 0, uy: UP_Y, uz: UP_Z
    }];
    for (let r = 0; r < 4; r += 1) {
      const a = r * Math.PI * 0.5;
      const ex = -VIEW_STREET_H * Math.sin(a);
      const ez = VIEW_CZ - VIEW_STREET_H * Math.cos(a);
      let fx = -ex;
      let fy = VIEW_CY - VIEW_STREET_Y;
      let fz = VIEW_CZ - ez;
      const fl = Math.sqrt(fx * fx + fy * fy + fz * fz) || 1;
      fx /= fl; fy /= fl; fz /= fl;
      // right = normalize(cross(fwd, worldUp)), then up = cross(right, fwd): the basis a
      // look-at camera with worldUp +Y produces, so a projected coordinate here is the NDC a
      // render of that view would produce.
      let rx = -fz;
      let rz = fx;
      const rl = Math.sqrt(rx * rx + rz * rz) || 1;
      rx /= rl; rz /= rl;
      out.push({
        ex: ex, ey: VIEW_STREET_Y, ez: ez,
        fx: fx, fy: fy, fz: fz,
        rx: rx, ry: 0, rz: rz,
        ux: -rz * fy,
        uy: rz * fx - rx * fz,
        uz: rx * fy
      });
    }
    return out;
  })();

  // ------------------------------------------------------------------ elevated expressway
  // A constant-height deck outside the ring: two carriageways and a painted median on a
  // continuous slab, a fascia and a soffit, guard rails, stone piers with caps where the valley
  // is deep, and a solid abutment where the deck meets the slope. At the west end it runs into
  // a tunnel portal: headwall, wing walls and a dark throat, so the road leaves the frame into
  // the hill instead of stopping in mid air.
  const expresswayMesh = (function () {
    const b = makeBuilder();
    const EX_ASPHALT = new THREE.Color(0x3a3b40);
    const EX_SHOULDER = new THREE.Color(0x4c4d51);
    const EX_PAINT = new THREE.Color(0xe4e0d4);
    const EX_MEDIAN_PAINT = new THREE.Color(0xd2ad45);
    const EX_FASCIA = new THREE.Color(0x8c8577);
    const EX_SOFFIT = new THREE.Color(0x6d675d);
    const EX_PIER = new THREE.Color(0x9a9284);
    const EX_STEEL = new THREE.Color(0xa8adb2);
    const EX_WALL = new THREE.Color(0x87806f);
    const EX_DARK = new THREE.Color(0x111214);
    const yT = EX_DECK_Y;
    const yB = yT - 1.15;
    const UNDERSIDE = yB - 0.05;

    function quad(ax, ay, az, bx, by, bz, cx, cy, cz, dx, dy, dz, nx, ny, nz, col) {
      const ux = bx - ax;
      const uy = by - ay;
      const uz = bz - az;
      const vx = cx - ax;
      const vy = cy - ay;
      const vz = cz - az;
      const s = ux * (vy * nz - vz * ny) + uy * (vz * nx - vx * nz) + uz * (vx * ny - vy * nx);
      const i0 = b.vert(ax, ay, az, nx, ny, nz, col);
      const i1 = b.vert(bx, by, bz, nx, ny, nz, col);
      const i2 = b.vert(cx, cy, cz, nx, ny, nz, col);
      const i3 = b.vert(dx, dy, dz, nx, ny, nz, col);
      if (s >= 0) {
        b.idx(i0, i1, i2);
        b.idx(i0, i2, i3);
      } else {
        b.idx(i0, i2, i1);
        b.idx(i0, i3, i2);
      }
    }

    function at(i, off) {
      return [exPath.xs[i] + exPath.nx[i] * off, exPath.zs[i] + exPath.nz[i] * off];
    }

    // A horizontal band running with the road, at an absolute height.
    function bandY(i, j, lo, hi, y, col) {
      const a = at(i, lo);
      const c = at(i, hi);
      const d = at(j, hi);
      const e = at(j, lo);
      quad(a[0], y, a[1], c[0], y, c[1], d[0], y, d[1], e[0], y, e[1], 0, 1, 0, col);
    }

    // A vertical band along one edge: sign +1 faces outward on the positive side.
    function bandV(i, j, off, yLo, yHi, sign, col) {
      const a = at(i, off);
      const c = at(j, off);
      quad(a[0], yLo, a[1], c[0], yLo, c[1], c[0], yHi, c[1], a[0], yHi, a[1],
        exPath.nx[i] * sign, 0, exPath.nz[i] * sign, col);
    }

    let pierAcc = 0;
    // The paint has to survive the sample rate at the distance it is read from. A 0.16 m edge
    // line is one pixel wide at 220 m and, worse, the pixel it lands on flips from segment to
    // segment, which is exactly the salt-and-pepper cluster the speck detector finds in the
    // upper left of street rotation 0 (69 pixels at x 0..200, y 112..225). A painted line holds
    // together only while it covers about 1.6 device pixels, and one device pixel at distance d
    // subtends 2 d tan(22.5) / (900 x 1.25) metres in the 1600x900 capture, so every band here
    // is widened in place, symmetrically about its own centre, once its authored half width is
    // under that. The distance is the furthest of the five shared frames, because that is the
    // frame where the line is thinnest on screen; the widening is continuous in d, so nothing
    // pops as the camera moves, and it is capped so a near view never reads a racing stripe.
    const EX_PX_PER_M = 2 * TAN_V / (900 * 1.25);
    function exPaintHalf(authored, x, z) {
      let far = 0;
      for (let v = 0; v < FRAME_VIEWS.length; v += 1) {
        const view = FRAME_VIEWS[v];
        const dx = x - view.ex;
        const dy = yT - view.ey;
        const dz = z - view.ez;
        const d = Math.sqrt(dx * dx + dy * dy + dz * dz);
        if (d > far) far = d;
      }
      const need = far * EX_PX_PER_M * 0.8;
      return need > authored ? Math.min(0.22, need) : authored;
    }
    for (let i = 0; i < exPath.n; i += 1) {
      const j = i + 1;
      const x0 = exPath.xs[i];
      const z0 = exPath.zs[i];
      const tx = exPath.nz[i];
      const tz = -exPath.nx[i];
      const rotY = Math.atan2(tx, tz);
      const g = groundHeightAt(x0, z0);
      const len = Math.sqrt(
        (exPath.xs[j] - x0) * (exPath.xs[j] - x0) + (exPath.zs[j] - z0) * (exPath.zs[j] - z0)
      ) * 1.12;
      const edgeH = exPaintHalf(0.08, x0, z0);
      const medH = exPaintHalf(0.07, x0, z0);
      const dashH = exPaintHalf(0.07, x0, z0);

      // Deck surface: shoulders outboard, two carriageways, a painted median between them.
      bandY(i, j, -EX_HALF, -7.7, yT, EX_SHOULDER);
      bandY(i, j, -7.7, -0.7, yT, EX_ASPHALT);
      bandY(i, j, -0.7, 0.7, yT, EX_ASPHALT);
      bandY(i, j, 0.7, 7.7, yT, EX_ASPHALT);
      bandY(i, j, 7.7, EX_HALF, yT, EX_SHOULDER);
      // Edge lines and the double line of the median, each about its authored centre.
      bandY(i, j, -7.64 - edgeH, -7.64 + edgeH, yT + 0.02, EX_PAINT);
      bandY(i, j, 7.64 - edgeH, 7.64 + edgeH, yT + 0.02, EX_PAINT);
      bandY(i, j, -0.51 - medH, -0.51 + medH, yT + 0.02, EX_MEDIAN_PAINT);
      bandY(i, j, 0.51 - medH, 0.51 + medH, yT + 0.02, EX_MEDIAN_PAINT);
      // Lane dashes, alternating so they read as a broken line.
      if (i % 4 < 2) {
        bandY(i, j, -4.23 - dashH, -4.23 + dashH, yT + 0.02, EX_PAINT);
        bandY(i, j, 4.23 - dashH, 4.23 + dashH, yT + 0.02, EX_PAINT);
      }
      // Fascia and soffit.
      bandV(i, j, -EX_HALF, yB, yT, -1, EX_FASCIA);
      bandV(i, j, EX_HALF, yB, yT, 1, EX_FASCIA);
      quad(
        at(i, -EX_HALF)[0], yB, at(i, -EX_HALF)[1],
        at(i, EX_HALF)[0], yB, at(i, EX_HALF)[1],
        at(j, EX_HALF)[0], yB, at(j, EX_HALF)[1],
        at(j, -EX_HALF)[0], yB, at(j, -EX_HALF)[1],
        0, -1, 0, EX_SOFFIT
      );
      // Guard rails: a steel band along each edge with a top cap.
      for (let s = -1; s <= 1; s += 2) {
        const off = s * (EX_HALF - 0.3);
        bandV(i, j, off, yT + 0.52, yT + 1.06, s, EX_STEEL);
        bandV(i, j, off - 0.13, yT + 0.52, yT + 1.06, -s, EX_STEEL);
        bandY(i, j, off - 0.13, off + 0.13, yT + 1.06, EX_STEEL);
      }
      // Posts every other segment.
      if ((i & 1) === 0) {
        for (let s = -1; s <= 1; s += 2) {
          const p = at(i, s * (EX_HALF - 0.3));
          b.addGeometry(unitBox, composeMatrix(p[0], yT + 0.55, p[1], rotY, 0.14, 1.1, 0.14),
            EX_STEEL);
        }
      }
      // Where the valley floor is close, the deck sits on a solid abutment; elsewhere it is
      // carried on piers with caps.
      if (UNDERSIDE - g < 3.4) {
        b.addGeometry(unitBox, composeMatrix(x0, (g - 1.2 + UNDERSIDE) * 0.5, z0, rotY,
          EX_HALF * 2 + 0.5, UNDERSIDE - g + 1.2, len), EX_WALL);
      } else {
        pierAcc += exPath.step;
        if (pierAcc >= EX_PIER_STEP) {
          pierAcc = 0;
          const capTop = UNDERSIDE;
          const capY = capTop - 0.45;
          const shaftTop = capY - 0.9;
          const shaftBase = g - 1.0;
          b.addGeometry(unitBox, composeMatrix(x0, (shaftBase + shaftTop) * 0.5, z0, rotY,
            3.4, shaftTop - shaftBase, 2.6), EX_PIER);
          b.addGeometry(unitBox, composeMatrix(x0, capY, z0, rotY,
            EX_HALF * 2 - 1.6, 0.9, 3.2), EX_PIER);
        }
      }
    }

    // Tunnel portal at the far west end: the headwall closes the deck, two wing walls splay back
    // into the slope, and a dark throat six metres in reads as depth into the hill.
    {
      const x0 = exPath.xs[0];
      const z0 = exPath.zs[0];
      const tx = exPath.nz[0];
      const tz = -exPath.nx[0];
      const rotY = Math.atan2(tx, tz);
      const wallW = EX_HALF * 2 + 5.2;
      b.addGeometry(unitBox, composeMatrix(x0, yB + 5.0, z0, rotY, wallW, 13.6, 2.0), EX_WALL);
      // Wing walls, splayed back from the corners of the portal.
      for (let s = -1; s <= 1; s += 2) {
        const px = x0 + exPath.nx[0] * s * (EX_HALF + 2.0);
        const pz = z0 + exPath.nz[0] * s * (EX_HALF + 2.0);
        b.addGeometry(unitBox, composeMatrix(px, yB + 2.4, pz, rotY + s * 0.58,
          12.0, 8.0, 1.6), EX_WALL);
      }
      // The dark throat, recessed behind the headwall and facing back down the road.
      const k = 3;
      const m0 = at(k, -(EX_HALF - 1.0));
      const m1 = at(k, EX_HALF - 1.0);
      const mouthY = yB - 0.3;
      quad(
        m0[0], mouthY, m0[1], m1[0], mouthY, m1[1],
        m1[0], mouthY + 9.2, m1[1], m0[0], mouthY + 9.2, m0[1],
        -exPath.nz[k], 0, exPath.nx[k], EX_DARK
      );
      // The crown of the arch, stepped so it is not a plain rectangle.
      b.addGeometry(unitBox, composeMatrix(x0, yB + 12.0, z0, rotY, wallW - 3.4, 1.3, 2.4),
        EX_WALL);
    }

    return new THREE.Mesh(b.build(), new THREE.MeshStandardMaterial({
      vertexColors: true,
      roughness: 0.62,
      metalness: 0.1
    }));
  })();
  expresswayMesh.castShadow = shadowsOn;
  expresswayMesh.receiveShadow = shadowsOn;
  expresswayMesh.name = 'expressway';
  group.add(expresswayMesh);
  if (NARROW_VIEW) nightCullTouch.push(expresswayMesh);

  // ------------------------------------------------------------------ buildings
  const buildingMat = new THREE.MeshStandardMaterial({
    color: 0xffffff,
    roughness: 0.76,
    metalness: 0.02
  });
  const buildings = new THREE.InstancedMesh(unitBox, buildingMat, BUILDING_CAP);
  buildings.castShadow = shadowsOn;
  buildings.receiveShadow = shadowsOn;
  buildings.name = 'buildings';
  buildings.frustumCulled = false;
  group.add(buildings);

  const buildingQuarter = new Int16Array(BUILDING_CAP);
  buildingQuarter.fill(-1);
  const buildingBase = new Float32Array(BUILDING_CAP * 3);
  const buildingsOfQuarter = [];
  for (let q = 0; q < qCount; q += 1) buildingsOfQuarter.push([]);

  let buildingCursor = 0;
  for (let bi = 0; bi < blds.length && buildingCursor < BUILDING_CAP; bi += 1) {
    const b = blds[bi];
    // A model lot has no procedural box at all: its pack mesh is the volume, and the missing
    // instance is also what suppresses its windows, frames, shop band and detail dressing.
    if (b.tower || b.model) continue;
    const h = b.top - b.bottom;
    dummy.position.set(b.cx, b.bottom + h * 0.5, b.cz);
    dummy.rotation.set(0, b.rotY, 0);
    dummy.scale.set(b.w, h, b.d);
    dummy.updateMatrix();
    buildings.setMatrixAt(buildingCursor, dummy.matrix);
    colorScratch.setHex(b.color).multiplyScalar(b.shade);
    buildings.setColorAt(buildingCursor, colorScratch);
    colorScratch.toArray(buildingBase, buildingCursor * 3);
    buildingQuarter[buildingCursor] = b.qi;
    if (b.qi >= 0) buildingsOfQuarter[b.qi].push(buildingCursor);
    b.instance = buildingCursor;
    buildingCursor += 1;
  }
  buildings.count = buildingCursor;
  if (buildings.instanceColor) buildings.instanceColor.needsUpdate = true;
  buildings.instanceMatrix.needsUpdate = true;

  // ------------------------------------------------------------------ pack model meshes
  // One InstancedMesh per pack model and material, built only for the buckets that actually
  // received a lot. The helper is the single seam into the asset cache: it picks, allocates,
  // marks the mesh as borrowing the cache's geometry and material, and creates the per-instance
  // quarter map main reads when a tap lands on the mesh.
  function modelMesh(category, options) {
    if (!assets) return null;
    const o = options || {};
    const picked = o.model || assets.pick(category, o.seed === undefined ? 0 : o.seed);
    if (!picked) return null;
    const count = Math.max(1, Math.floor(o.count === undefined ? 1 : o.count));
    const mesh = assets.makeInstanced(picked, count, { shadows: o.shadows === true });
    mesh.userData.sharedGeometry = true;
    mesh.userData.quarterOf = new Int16Array(count);
    mesh.userData.quarterOf.fill(-1);
    mesh.name = picked.name;
    return { mesh: mesh, model: picked };
  }

  const modelMeshList = [];
  const modelMeshQuarters = [];
  const modelMeshBase = [];
  const modelPickMeshes = [];
  const modelStats = {
    lots: modelLots.length,
    towers: modelTowers.length,
    villas: modelVillas.length,
    towerLots: 0,
    instances: 0,
    tris: 0,
    byModel: []
  };
  for (let bi = 0; bi < blds.length; bi += 1) {
    if (blds[bi].tower) modelStats.towerLots += 1;
  }

  function buildModelBuckets(list) {
    const buckets = new Map();
    for (let i = 0; i < list.length; i += 1) {
      const b = list[i];
      const def = b.modelDef;
      if (!def || !def.model) continue;
      const key = def.model.name;
      let bucket = buckets.get(key);
      if (!bucket) {
        bucket = { model: def.model, lots: [] };
        buckets.set(key, bucket);
      }
      bucket.lots.push(b);
    }
    buckets.forEach(function (bucket) {
      const made = modelMesh(bucket.model.category, {
        model: bucket.model,
        count: bucket.lots.length,
        shadows: shadowsOn
      });
      if (!made) return;
      const mesh = made.mesh;
      const quarterMap = mesh.userData.quarterOf;
      const tintBase = mesh.instanceColor ? new Float32Array(bucket.lots.length * 3) : null;
      for (let i = 0; i < bucket.lots.length; i += 1) {
        const b = bucket.lots[i];
        const def = b.modelDef;
        const jx = 0.92 + hash2(b.cx * 5 + 3, b.cz * 5 + 7) * 0.2;
        const jz = 0.92 + hash2(b.cx * 7 + 11, b.cz * 7 + 13) * 0.2;
        const yaw = b.rotY + def.yaw;
        dummy.position.set(b.cx, def.y, b.cz);
        dummy.rotation.set(def.pitch || 0, yaw, 0);
        dummy.scale.set(def.sxz * jx, def.sy, def.sxz * jz);
        dummy.updateMatrix();
        mesh.setMatrixAt(i, dummy.matrix);
        if (tintBase) {
          packTint(modelTintScratch, b.color, hash2(b.cx * 11 + 17, b.cz * 11 + 19), 0.86, 1.06);
          mesh.setColorAt(i, modelTintScratch);
          modelTintScratch.toArray(tintBase, i * 3);
        }
        quarterMap[i] = b.qi;
      }
      mesh.count = bucket.lots.length;
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
      if (mesh.instanceMatrix) mesh.instanceMatrix.needsUpdate = true;
      group.add(mesh);
      modelMeshList.push(mesh);
      modelMeshQuarters.push(quarterMap);
      modelMeshBase.push(tintBase);
      modelPickMeshes.push(mesh);
      modelStats.instances += bucket.lots.length;
      modelStats.tris += bucket.model.tris * bucket.lots.length;
      modelStats.byModel.push(bucket.model.name + ' x' + bucket.lots.length);
    });
  }

  buildModelBuckets(modelLots);
  buildModelBuckets(modelTowers);
  buildModelBuckets(modelVillas);

  // ------------------------------------------------------------------ shop frontages
  // The avenue, the three arterials that leave the frame, and the core streets inside the
  // 62 by 78 metre retail heart carry a distinct ground storey. Every other block keeps its
  // plain ground floor, so the shopfront band stays a readable signature of the main streets
  // instead of being smeared over the whole city.
  const spurProbe = [];
  for (let s = 0; s < spurPaths.length; s += 1) {
    const sp = spurPaths[s];
    for (let i = 0; i <= sp.n; i += 3) spurProbe.push(sp.xs[i], sp.zs[i]);
  }
  function nearSpur(x, z, r) {
    const r2 = r * r;
    for (let i = 0; i < spurProbe.length; i += 2) {
      const dx = spurProbe[i] - x;
      const dz = spurProbe[i + 1] - z;
      if (dx * dx + dz * dz < r2) return true;
    }
    return false;
  }
  function shopAt(b) {
    if (b.tower) return false;
    if (b.kind === 1) return true;
    if (b.kind === 3) return nearSpur(b.cx, b.cz, 36);
    return b.kind === 0 && Math.abs(b.cx) < 62 && Math.abs(b.cz) < 78;
  }

  // ------------------------------------------------------------------ towers
  // Three or four office towers on the upper plateau, one merged curtain-wall mesh: a per-floor
  // grid whose colour climbs from the shaded base to a warm reflection toward the sun near the
  // crown, a darker spandrel at every floor line, a projecting core seam, a parapet, a plant
  // room and a mast. Glass is rough-then-smooth metal, so it does not read as a blue box.
  const towerRecs = [];
  for (let bi = 0; bi < blds.length; bi += 1) {
    // A tower that took a pack model has its volume in the model mesh above. Only the remaining
    // procedural towers enter the merged curtain-wall mesh and its lit window grid.
    if (blds[bi].tower && !blds[bi].model) towerRecs.push(blds[bi]);
  }

  const towerMesh = (function () {
    if (towerRecs.length === 0) return null;
    const b = makeBuilder();
    const sunLen = Math.sqrt(SUN_DIR.x * SUN_DIR.x + SUN_DIR.z * SUN_DIR.z) || 1;
    const sx = SUN_DIR.x / sunLen;
    const sz = SUN_DIR.z / sunLen;
    const cBot = new THREE.Color();
    const cTop = new THREE.Color();
    const cSpand = new THREE.Color();
    const cCap = new THREE.Color();
    const tint = new THREE.Color();

    function face(ax, ay, az, bx, by, bz, cx, cy, cz, dx, dy, dz,
      nx, ny, nz, c0, c1, c2, c3) {
      const ux = bx - ax;
      const uy = by - ay;
      const uz = bz - az;
      const vx = cx - ax;
      const vy = cy - ay;
      const vz = cz - az;
      const s = ux * (vy * nz - vz * ny) + uy * (vz * nx - vx * nz) + uz * (vx * ny - vy * nx);
      const i0 = b.vert(ax, ay, az, nx, ny, nz, c0);
      const i1 = b.vert(bx, by, bz, nx, ny, nz, c1);
      const i2 = b.vert(cx, cy, cz, nx, ny, nz, c2);
      const i3 = b.vert(dx, dy, dz, nx, ny, nz, c3);
      if (s >= 0) {
        b.idx(i0, i1, i2);
        b.idx(i0, i2, i3);
      } else {
        b.idx(i0, i2, i1);
        b.idx(i0, i3, i2);
      }
    }

    for (let t = 0; t < towerRecs.length; t += 1) {
      const r = towerRecs[t];
      const hw = r.w * 0.5;
      const hd = r.d * 0.5;
      const ux = Math.cos(r.rotY);
      const uz = -Math.sin(r.rotY);
      const vx = Math.sin(r.rotY);
      const vz = Math.cos(r.rotY);
      const y0 = r.bottom + 0.5;
      const y1 = r.top;
      const fh = (y1 - y0) / r.floors;
      // Each tower keeps a little of its own facade colour, so four slabs are not four twins.
      tint.setHex(r.color);
      const shadeF = r.shade;
      const ax0 = r.cx - ux * hw - vx * hd;
      const az0 = r.cz - uz * hw - vz * hd;
      const bx0 = r.cx + ux * hw - vx * hd;
      const bz0 = r.cz + uz * hw - vz * hd;
      const cx0 = r.cx + ux * hw + vx * hd;
      const cz0 = r.cz + uz * hw + vz * hd;
      const dx0 = r.cx - ux * hw + vx * hd;
      const dz0 = r.cz - uz * hw + vz * hd;
      const sides = [
        { ax: ax0, az: az0, bx: bx0, bz: bz0, nx: -vx, nz: -vz },
        { ax: bx0, az: bz0, bx: cx0, bz: cz0, nx: ux, nz: uz },
        { ax: cx0, az: cz0, bx: dx0, bz: dz0, nx: vx, nz: vz },
        { ax: dx0, az: dz0, bx: ax0, bz: az0, nx: -ux, nz: -uz }
      ];
      for (let s = 0; s < 4; s += 1) {
        const side = sides[s];
        const sunF = clamp(side.nx * sx + side.nz * sz, 0, 1);
        for (let f = 0; f < r.floors; f += 1) {
          const fy0 = y0 + f * fh;
          const glass0 = fy0 + fh * 0.16;
          const glass1 = fy0 + fh * 0.96;
          const climb = f / Math.max(1, r.floors - 1);
          const warm = 0.14 + 0.62 * climb * (0.2 + 0.8 * sunF);
          cBot.copy(TOWER_SHADE).lerp(tint, 0.3).lerp(TOWER_SUN, warm).multiplyScalar(shadeF);
          // The crown catches the warm reflection, but it stops short of the sun tone itself: a
          // face blended all the way to TOWER_SUN under the 3.9 intensity sun reads as a white
          // cap against the pale haze and blooms. The top blend is capped at 0.76, which leaves
          // the crown glass well under the 0.85 luminance line before post.
          cTop.copy(TOWER_SHADE).lerp(tint, 0.2)
            .lerp(TOWER_SUN, Math.min(0.76, warm + 0.12)).multiplyScalar(shadeF);
          cSpand.copy(TOWER_SPANDREL).lerp(cBot, 0.28);
          for (let c = 0; c < 3; c += 1) {
            const t0 = c / 3;
            const t1 = (c + 1) / 3;
            const p0x = side.ax + (side.bx - side.ax) * t0;
            const p0z = side.az + (side.bz - side.az) * t0;
            const p1x = side.ax + (side.bx - side.ax) * t1;
            const p1z = side.az + (side.bz - side.az) * t1;
            face(p0x, glass0, p0z, p1x, glass0, p1z, p1x, glass1, p1z, p0x, glass1, p0z,
              side.nx, 0, side.nz, cBot, cBot, cTop, cTop);
          }
          face(side.ax, fy0, side.az, side.bx, fy0, side.bz, side.bx, glass0, side.bz,
            side.ax, glass0, side.az, side.nx, 0, side.nz, cSpand, cSpand, cSpand, cSpand);
        }
      }
      // Core seam on two opposite faces: a projecting vertical band, the expansion joint.
      for (let s = 0; s < 4; s += 2) {
        const side = sides[s];
        const mx = (side.ax + side.bx) * 0.5;
        const mz = (side.az + side.bz) * 0.5;
        const tx = side.bx - side.ax;
        const tz = side.bz - side.az;
        const tl = Math.sqrt(tx * tx + tz * tz) || 1;
        const wx = tx / tl;
        const wz = tz / tl;
        const ox = side.nx * 0.36;
        const oz = side.nz * 0.36;
        face(mx - wx * 0.7 + ox, y0, mz - wz * 0.7 + oz,
          mx + wx * 0.7 + ox, y0, mz + wz * 0.7 + oz,
          mx + wx * 0.7 + ox, y1, mz + wz * 0.7 + oz,
          mx - wx * 0.7 + ox, y1, mz - wz * 0.7 + oz,
          side.nx, 0, side.nz, TOWER_MULLION, TOWER_MULLION, TOWER_MULLION, TOWER_MULLION);
        face(mx - wx * 0.7, y0, mz - wz * 0.7,
          mx - wx * 0.7 + ox, y0, mz - wz * 0.7 + oz,
          mx - wx * 0.7 + ox, y1, mz - wz * 0.7 + oz,
          mx - wx * 0.7, y1, mz - wz * 0.7,
          -wx, 0, -wz, TOWER_SPANDREL, TOWER_SPANDREL, TOWER_SPANDREL, TOWER_SPANDREL);
        face(mx + wx * 0.7 + ox, y0, mz + wz * 0.7 + oz,
          mx + wx * 0.7, y0, mz + wz * 0.7,
          mx + wx * 0.7, y1, mz + wz * 0.7,
          mx + wx * 0.7 + ox, y1, mz + wz * 0.7 + oz,
          wx, 0, wz, TOWER_SPANDREL, TOWER_SPANDREL, TOWER_SPANDREL, TOWER_SPANDREL);
      }
      // Roof slab.
      cCap.copy(TOWER_SPANDREL).multiplyScalar(0.8);
      face(ax0, y1, az0, bx0, y1, bz0, cx0, y1, cz0, dx0, y1, dz0,
        0, 1, 0, cCap, cCap, cCap, cCap);
      // Parapet crown, slightly proud of the curtain wall.
      const cw = 0.34;
      const crownY0 = y1;
      const crownY1 = y1 + 0.95;
      for (let s = 0; s < 4; s += 1) {
        const side = sides[s];
        const ox = side.nx * cw;
        const oz = side.nz * cw;
        face(side.ax + ox, crownY0, side.az + oz, side.bx + ox, crownY0, side.bz + oz,
          side.bx + ox, crownY1, side.bz + oz, side.ax + ox, crownY1, side.az + oz,
          side.nx, 0, side.nz, TOWER_MULLION, TOWER_MULLION, TOWER_MULLION, TOWER_MULLION);
      }
      // Plant room on the roof and a slim mast beside it.
      b.addGeometry(unitBox,
        composeMatrix(r.cx, y1 + 1.6, r.cz, r.rotY, hw * 1.1, 3.2, hd * 1.1), TOWER_PLANT);
      b.addGeometry(unitBox,
        composeMatrix(r.cx + ux * hw * 0.55, y1 + 4.4, r.cz + uz * hw * 0.55, r.rotY,
          0.28, 6.0, 0.28), TOWER_MULLION);
    }
    // The scene has no environment map: a metalness near one would leave the glass with a
    // single analytic highlight and no diffuse at all, which reads as black. So the curtain
    // wall is a glossy dielectric, low metalness, and the gradient in the vertices does the
    // work of the sky reflection.
    return new THREE.Mesh(b.build(), new THREE.MeshStandardMaterial({
      vertexColors: true,
      roughness: 0.38,
      metalness: 0.18
    }));
  })();
  if (towerMesh) {
    towerMesh.name = 'towers';
    towerMesh.castShadow = shadowsOn;
    towerMesh.receiveShadow = shadowsOn;
    group.add(towerMesh);
    if (NARROW_VIEW) nightCullTouch.push(towerMesh);
  }

  // ------------------------------------------------------------------ ground contact
  // One instanced dark blob under every building: a soft radial disc, tilted to the ground
  // normal, that grounds each volume instead of letting it sit on a clean bright pavement.
  // polygonOffset and depthWrite false keep it clear of the paving and of its neighbours.
  //
  // The same narrow viewport test the frames and the shop glass use. At 412 px a blob covers a
  // couple of device pixels, and on the touch profile a call is the scarcest thing in the frame:
  // the taken road permanence has to fit under the ceiling too. The darkening is not lost there,
  // it moves into the detail pool as the two-step contact skirt (see the details section), which
  // is already drawn and costs no call.
  if (!NARROW_VIEW) {
    const contactGeo = (function () {
      // Eight segments and one ring: eight triangles an instance. The blob is a soft dark gradient
      // with no silhouette to speak of, so an octagon of vertices is all it takes, and the saving
      // against a two ring disc is six thousand triangles across the city.
      const SEG = 8;
      const RINGS = 1;
      const pos = [];
      const col = [];
      const idx = [];
      pos.push(0, 0, 0);
      col.push(0, 0, 0, 0.66);
      for (let r = 1; r <= RINGS; r += 1) {
        const rr = r / RINGS;
        const a = 0.5 * (1 - rr) * (1 - rr);
        for (let s = 0; s < SEG; s += 1) {
          const ang = (s / SEG) * Math.PI * 2;
          pos.push(Math.cos(ang) * rr, 0, Math.sin(ang) * rr);
          col.push(0, 0, 0, a);
        }
      }
      for (let s = 0; s < SEG; s += 1) {
        idx.push(0, 1 + s, 1 + ((s + 1) % SEG));
      }
      for (let r = 1; r < RINGS; r += 1) {
        const a0 = 1 + (r - 1) * SEG;
        const a1 = 1 + r * SEG;
        for (let s = 0; s < SEG; s += 1) {
          const n = (s + 1) % SEG;
          idx.push(a0 + s, a1 + s, a1 + n);
          idx.push(a0 + s, a1 + n, a0 + n);
        }
      }
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(pos), 3));
      geo.setAttribute('color', new THREE.BufferAttribute(new Float32Array(col), 4));
      geo.setIndex(idx);
      geo.computeVertexNormals();
      return geo;
    })();
    const contactMat = new THREE.MeshBasicMaterial({
      // A warm dark brown, not a green-black: the blob is the shadow the block casts on its own
      // pavement, and under this sun the ambient it removes is warm. Measured, the neutral
      // value was one more reason the lower third sat 0.02 under its target.
      color: 0x120c07,
      vertexColors: true,
      transparent: true,
      opacity: 0.88,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -4,
      polygonOffsetUnits: -4
    });
    const contacts = new THREE.InstancedMesh(contactGeo, contactMat, CONTACT_CAP);
    contacts.name = 'contact';
    contacts.frustumCulled = false;
    contacts.renderOrder = 1;
    group.add(contacts);
    nightCull.push(contacts);
    const UP_AXIS = new THREE.Vector3(0, 1, 0);
    let contactCursor = 0;
    function addContact(cx, cz, w, d, rotY) {
      if (contactCursor >= CONTACT_CAP) return;
      normalAt(cx, cz, v3a);
      dummy.position.set(cx, groundHeightAt(cx, cz) + 0.32, cz);
      dummy.quaternion.setFromUnitVectors(UP_AXIS, v3a);
      dummy.scale.set(w * 0.72, 1, d * 0.72);
      dummy.updateMatrix();
      contacts.setMatrixAt(contactCursor, dummy.matrix);
      contactCursor += 1;
    }
    for (let bi = 0; bi < blds.length; bi += 1) {
      const b = blds[bi];
      // Model lots have no procedural instance but they still need the same dark apron, or a
      // real building floats on the pavement while every procedural neighbour stays grounded.
      if (b.instance === undefined && !b.tower && !b.model) continue;
      addContact(b.cx, b.cz, b.w, b.d, b.rotY);
    }
    for (let i = 0; i < houseSpots.length; i += 1) {
      const hs = houseSpots[i];
      addContact(hs.x, hs.z, hs.w * 1.2, hs.d * 1.2, hs.yaw);
    }
    contacts.count = contactCursor;
    if (contacts.instanceMatrix) contacts.instanceMatrix.needsUpdate = true;
  }

  // ------------------------------------------------------------------ windows
  const windowMat = new THREE.MeshBasicMaterial({ color: 0xffffff });
  const windows = new THREE.InstancedMesh(new THREE.PlaneGeometry(1, 1), windowMat, WINDOW_CAP);
  windows.name = 'windows';
  windows.frustumCulled = false;
  group.add(windows);

  // A window frame rides its own instance: a rectangular ring of four bars around a one unit
  // opening, pushed 7 cm proud of the pane, so every window reads as a recessed opening with a
  // crisp trim line around it instead of a flat dark quad. One draw call for the whole city.
  //
  // At district tier on a phone each bar is 0.2 of a CSS pixel wide: 1447 rings measured
  // against the district frame cover 2295 CSS px, 0.6 percent of it, for one of the phone's
  // forty two draw calls. A narrow viewport therefore never builds the mesh: the instance
  // buffer is not filled and no draw call exists. Tablet and desktop build it as before.
  let frames = null;
  let frameCursor = 0;
  if (!NARROW_VIEW) {
    const frameGeo = (function () {
      const pos = new Float32Array(4 * 4 * 3);
      const nor = new Float32Array(4 * 4 * 3);
      const idx = new Uint16Array(4 * 6);
      const OUT = 0.62;
      const IN = 0.5;
      const bars = [
        [-OUT, -OUT, OUT, -IN],
        [-OUT, IN, OUT, OUT],
        [-OUT, -IN, -IN, IN],
        [IN, -IN, OUT, IN]
      ];
      for (let b = 0; b < 4; b += 1) {
        const x0 = bars[b][0];
        const y0 = bars[b][1];
        const x1 = bars[b][2];
        const y1 = bars[b][3];
        const v = b * 4;
        pos[v * 3] = x0; pos[v * 3 + 1] = y0; pos[v * 3 + 2] = 0;
        pos[v * 3 + 3] = x1; pos[v * 3 + 4] = y0; pos[v * 3 + 5] = 0;
        pos[v * 3 + 6] = x1; pos[v * 3 + 7] = y1; pos[v * 3 + 8] = 0;
        pos[v * 3 + 9] = x0; pos[v * 3 + 10] = y1; pos[v * 3 + 11] = 0;
        for (let k = 0; k < 4; k += 1) {
          nor[(v + k) * 3] = 0;
          nor[(v + k) * 3 + 1] = 0;
          nor[(v + k) * 3 + 2] = 1;
        }
        const o = b * 6;
        idx[o] = v; idx[o + 1] = v + 1; idx[o + 2] = v + 2;
        idx[o + 3] = v; idx[o + 4] = v + 2; idx[o + 5] = v + 3;
      }
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
      geo.setAttribute('normal', new THREE.BufferAttribute(nor, 3));
      geo.setIndex(new THREE.BufferAttribute(idx, 1));
      geo.computeBoundingSphere();
      return geo;
    })();
    const frameMat = new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.84, metalness: 0.02 });
    frames = new THREE.InstancedMesh(frameGeo, frameMat, WINDOW_CAP);
    frames.name = 'frames';
    frames.frustumCulled = false;
    group.add(frames);
    nightCull.push(frames);
  }

  // Shopfront glazing: unlit glass, so a shop window stays the brightest thing at street level
  // even inside the shadow of its own street canyon, which is exactly what glazing does at
  // golden hour. One instanced quad, one draw call, no per-shop mesh.
  //
  // The same narrow viewport test applies. On the phone's district frame the 74 panes (shopfronts
  // plus two bus shelters) measure 216 device pixels, 0.008 percent of the frame, 0.4 square
  // CSS px each: the call is dropped with the mesh. Street tier on a phone loses its glazing
  // with it, which is the honest trade for the call the profile cannot afford.
  let shopGlass = null;
  let glassCursor = 0;
  if (!NARROW_VIEW) {
    const shopGlassMat = new THREE.MeshBasicMaterial({ color: 0xffffff });
    shopGlass = new THREE.InstancedMesh(new THREE.PlaneGeometry(1, 1), shopGlassMat, GLASS_CAP);
    shopGlass.name = 'shop-glass';
    shopGlass.frustumCulled = false;
    group.add(shopGlass);
  }
  const SHOP_GLASS_TONES = [0xe8f0ee, 0xdde8e8, 0xf1e6c9, 0xe4ecdd, 0xcfdfe3, 0xf3eadb];

  function addShopGlass(x, y, z, rotY, sw, sh, qi, tone) {
    if (!shopGlass || glassCursor >= GLASS_CAP) return;
    dummy.position.set(x, y, z);
    dummy.rotation.set(0, rotY, 0);
    dummy.scale.set(sw, sh, 1);
    dummy.updateMatrix();
    shopGlass.setMatrixAt(glassCursor, dummy.matrix);
    const h = hash2(Math.round(x * 3) + 1024, Math.round(z * 3) + 1024);
    colorScratch.setHex(tone || SHOP_GLASS_TONES[(h * SHOP_GLASS_TONES.length) | 0]);
    shopGlass.setColorAt(glassCursor, colorScratch);
    glassCursor += 1;
  }

  const windowQuarter = new Int16Array(WINDOW_CAP);
  windowQuarter.fill(-1);
  const windowWant = new Uint8Array(WINDOW_CAP);
  const windowShown = new Uint8Array(WINDOW_CAP);
  const windowsOfQuarter = [];
  for (let q = 0; q < qCount; q += 1) windowsOfQuarter.push([]);
  let windowCursor = 0;

  // A lit pane is drawn hot: at district distance the pane is only a few pixels wide, so it has
  // to sit well above the night haze in the frame or the whole city reads as a dark plate with
  // a scatter of dots on it.
  const LIT_WARM = new THREE.Color(0xffd8a6);
  const LIT_DIM = new THREE.Color(0x313946);
  const SHUTTER = new THREE.Color(0x76593f);

  // Every pane keeps its placement, because the night halo pass is built from the same numbers:
  // the halo is the pane's own footprint grown by a fixed margin, so it stays glued to the
  // reveal at every zoom tier instead of floating as a screen space blob.
  const winX = new Float32Array(WINDOW_CAP);
  const winY = new Float32Array(WINDOW_CAP);
  const winZ = new Float32Array(WINDOW_CAP);
  const winRot = new Float32Array(WINDOW_CAP);
  const winW = new Float32Array(WINDOW_CAP);
  const winH = new Float32Array(WINDOW_CAP);
  const winHue = new Float32Array(WINDOW_CAP);

  function addWindow(x, y, z, rotY, sw, sh, qi, frameCol) {
    if (windowCursor >= WINDOW_CAP) return;
    dummy.position.set(x, y, z);
    dummy.rotation.set(0, rotY, 0);
    dummy.scale.set(sw, sh, 1);
    dummy.updateMatrix();
    windows.setMatrixAt(windowCursor, dummy.matrix);
    windows.setColorAt(windowCursor, LIT_DIM);
    winX[windowCursor] = x;
    winY[windowCursor] = y;
    winZ[windowCursor] = z;
    winRot[windowCursor] = rotY;
    winW[windowCursor] = sw;
    winH[windowCursor] = sh;
    winHue[windowCursor] = hash2(Math.round(x * 5) + 8192, Math.round(z * 5) + 8192);
    windowQuarter[windowCursor] = qi;
    if (qi >= 0) windowsOfQuarter[qi].push(windowCursor);
    if (frames && frameCol && frameCursor < WINDOW_CAP) {
      const fx = Math.sin(rotY);
      const fz = Math.cos(rotY);
      dummy.position.set(x + fx * 0.075, y, z + fz * 0.075);
      dummy.rotation.set(0, rotY, 0);
      dummy.scale.set(sw * 1.13, sh * 1.12, 1);
      dummy.updateMatrix();
      frames.setMatrixAt(frameCursor, dummy.matrix);
      frames.setColorAt(frameCursor, frameCol);
      frameCursor += 1;
    }
    windowCursor += 1;
  }

  // The towers' lit window grid goes in first, so the window budget can never eat the city's
  // most visible night lights: three bays per face, every floor, on the quarter whose site is
  // nearest, so the activity pattern drives their dusk lights like every other window.
  for (let t = 0; t < towerRecs.length; t += 1) {
    const r = towerRecs[t];
    const hw = r.w * 0.5;
    const hd = r.d * 0.5;
    const ux = Math.cos(r.rotY);
    const uz = -Math.sin(r.rotY);
    const vx = Math.sin(r.rotY);
    const vz = Math.cos(r.rotY);
    const y0 = r.bottom + 0.5;
    const fh = (r.top - y0) / r.floors;
    const qi = nearestQuarter(r.cx, r.cz);
    const faces = [
      { ax: -vx, az: -vz, cx: r.cx - vx * hd, cz: r.cz - vz * hd, wide: r.w },
      { ax: ux, az: uz, cx: r.cx + ux * hw, cz: r.cz + uz * hw, wide: r.d },
      { ax: vx, az: vz, cx: r.cx + vx * hd, cz: r.cz + vz * hd, wide: r.w },
      { ax: -ux, az: -uz, cx: r.cx - ux * hw, cz: r.cz - uz * hw, wide: r.d }
    ];
    for (let f = 0; f < faces.length; f += 1) {
      const face = faces[f];
      const rotY = Math.atan2(face.ax, face.az);
      const tx = face.az;
      const tz = -face.ax;
      for (let fl = 0; fl < r.floors; fl += 1) {
        const y = y0 + fl * fh + fh * 0.56;
        for (let c = 0; c < 3; c += 1) {
          const t = (c - 1) * (face.wide / 3.4);
          addWindow(face.cx + face.ax * 0.1 + tx * t, y, face.cz + face.az * 0.1 + tz * t,
            rotY, face.wide * 0.21, fh * 0.5, qi);
        }
      }
    }
  }

  // Windows go on the street face and on the side faces that are not party walls; the courtyard
  // face stays blank, which is what a perimeter block really looks like from above. Outer-slope
  // blocks carry only their street grid, so the window budget stays in the urban core.
  const FRAME_TRIM = new THREE.Color(0xe7dcc3);
  const frameTint = new THREE.Color();
  // Two more sources of night light that the district view can actually see, because at this
  // pitch the roofs are the most visible surface of the city and the streets hide their own
  // ground floors: a lamp on a roof terrace, and a lantern over every entrance.
  const roofLamps = [];
  const doorLamps = [];
  // How many panes each face actually took. A zero on a side face is a blind wall, and the
  // mural pass below reads this grid instead of probing every wall a second time.
  const faceWindows = new Int16Array(blds.length * 4);
  let modelWindowCount = 0;
  for (let bi = 0; bi < blds.length; bi += 1) {
    const b = blds[bi];
    if (b.instance === undefined) continue;
    if (windowCursor >= WINDOW_CAP - 8) break;
    const outer = b.kind === 3;
    const shop = shopAt(b);
    if (hash2(bi * 29 + 11, 5) < 0.92) {
      const rh = hash2(bi * 17 + 3, 9);
      roofLamps.push({
        x: b.cx + (rh - 0.5) * b.w * 0.42,
        z: b.cz + (hash2(bi * 41 + 7, 6) - 0.5) * b.d * 0.42,
        y: b.top + 0.09,
        s: 5.3 + rh * 2.9,
        k: 0.62 + hash2(bi * 7 + 1, 13) * 0.34
      });
    }
    frameTint.setHex(b.color).multiplyScalar(b.shade * 0.86).lerp(FRAME_TRIM, 0.6);
    const faces = [
      { nx: b.nx, nz: b.nz, half: b.hv, wide: b.w, front: true },
      { nx: b.ux, nz: b.uz, half: b.hu, wide: b.d, front: false },
      { nx: -b.ux, nz: -b.uz, half: b.hu, wide: b.d, front: false },
      { nx: -b.nx, nz: -b.nz, half: b.hv, wide: b.w, front: false }
    ];
    const floorH = 3.0;
    const rows = b.floors;
    for (let f = 0; f < faces.length; f += 1) {
      const face = faces[f];
      if (outer && f !== 0) continue;
      if (!face.front) {
        // Skip a party wall: another building sits right behind it.
        const probe = face.half + 1.4;
        if (flagsAt(b.cx + face.nx * probe, b.cz + face.nz * probe) & B_BUILD) continue;
        if (face.nx === -b.nx && face.nz === -b.nz) continue;
      }
      const rotY = Math.atan2(face.nx, face.nz);
      const ax = face.nz;
      const az = -face.nx;
      const ox = b.cx + face.nx * (face.half + 0.03);
      const oz = b.cz + face.nz * (face.half + 0.03);
      const wide = face.wide;
      if (face.front) {
        // The entrance is opposite the shop window, so its lantern hangs over that half of the
        // frontage, above the ground floor band and clear of the first floor sill.
        const dt = wide * 0.16;
        doorLamps.push({
          x: ox + ax * dt + face.nx * 0.11,
          y: b.gMax + 4.2,
          z: oz + az * dt + face.nz * 0.11,
          rot: rotY,
          k: 0.6 + hash2(bi * 31 + 7, 3) * 0.32
        });
      }
      const nc = outer ? 2
        : (face.front ? clamp(Math.floor((wide - 0.6) / 3.9), 1, 3)
          : clamp(Math.floor((wide - 1.2) / 5.0), 1, 2));
      const spacing = wide / (nc + 0.4);
      const stride = outer ? 2 : (face.front ? 1 : 2);
      const faceStart = windowCursor;
      for (let fl = 0; fl < rows; fl += stride) {
        const y = b.gMax + 1.9 + fl * floorH;
        if (y > b.top - 1.4) break;
        if (fl === 0 && face.front) {
          if (shop) continue;      // the shopfront storey owns this band
          // Ground floor: one shop window off centre, the door takes the other side.
          const t = -wide * 0.16;
          addWindow(ox + ax * t, b.gMax + 1.55, oz + az * t, rotY,
            Math.min(wide * 0.46, 4.4), 1.9, b.qi, frameTint);
          continue;
        }
        for (let c = 0; c < nc; c += 1) {
          const t = (c - (nc - 1) * 0.5) * spacing;
          addWindow(ox + ax * t, y, oz + az * t, rotY, 1.2, 1.5, b.qi, frameTint);
        }
      }
      faceWindows[bi * 4 + f] = windowCursor - faceStart;
    }
  }

  // ------------------------------------------------------------------ model facade windows
  // The pack lots are the only built things in the district with no lit window on them: a glTF
  // box carries its own baked facade, and at night the four biggest blocks around the heart went
  // dark while every procedural neighbour lit up. The same two-triangle pane is therefore
  // overlaid on their four bounding-box walls, with the procedural rhythm, the same ground-floor
  // shop break and the same quarter registration, so `setWindowLit` drives them from the last
  // hour of activity like every other window in the city.
  //
  // The pane stands nine centimetres proud of the bounding box rather than using a polygon
  // offset: a real height offset cannot z-fight at any angle, and the pack models fill about
  // ninety per cent of their footprint, so the panes sit just off the wall they belong to.
  // No frame ring: the model already carries trim, and eight triangles a pane on four hundred
  // panes is a real cost on a phone.
  {
    const winStart = windowCursor;
    const lists = [modelLots, modelVillas];
    for (let li = 0; li < lists.length; li += 1) {
      const list = lists[li];
      for (let i = 0; i < list.length; i += 1) {
        const b = list[i];
        if (windowCursor >= WINDOW_CAP - 8) break;
        const def = b.modelDef;
        if (!def || !def.model || !def.model.footprint) continue;
        const fp = def.model.footprint;
        const jx = 0.92 + hash2(b.cx * 5 + 3, b.cz * 5 + 7) * 0.2;
        const jz = 0.92 + hash2(b.cx * 7 + 11, b.cz * 7 + 13) * 0.2;
        const yaw = b.rotY + def.yaw;
        const hw = fp.w * def.sxz * jx * 0.5;
        const hd = fp.d * def.sxz * jz * 0.5;
        const roof = b.gMin - 0.3 + fp.h * def.sy;
        const shop = shopAt(b);
        const faces = [
          { nx: Math.sin(yaw), nz: Math.cos(yaw), half: hd, wide: hw * 2, front: true },
          { nx: -Math.sin(yaw), nz: -Math.cos(yaw), half: hd, wide: hw * 2, front: false },
          { nx: Math.cos(yaw), nz: -Math.sin(yaw), half: hw, wide: hd * 2, front: false },
          { nx: -Math.cos(yaw), nz: Math.sin(yaw), half: hw, wide: hd * 2, front: false }
        ];
        for (let f = 0; f < faces.length; f += 1) {
          const face = faces[f];
          if (face.wide < 3.4) continue;
          const rotY = Math.atan2(face.nx, face.nz);
          const ax = face.nz;
          const az = -face.nx;
          const ox = b.cx + face.nx * (face.half + 0.09);
          const oz = b.cz + face.nz * (face.half + 0.09);
          const nc = face.front ? clamp(Math.floor((face.wide - 0.6) / 3.9), 1, 3)
            : clamp(Math.floor((face.wide - 1.2) / 5.0), 1, 2);
          const spacing = face.wide / (nc + 0.4);
          for (let fl = 0; fl < b.floors; fl += 1) {
            const y = b.gMax + 1.9 + fl * 3.0;
            if (y > roof - 1.4) break;
            if (fl === 0 && face.front) {
              if (shop) continue;
              const t = -face.wide * 0.16;
              addWindow(ox + ax * t, b.gMax + 1.55, oz + az * t, rotY,
                Math.min(face.wide * 0.46, 4.4), 1.9, b.qi, null);
              continue;
            }
            for (let c = 0; c < nc; c += 1) {
              const t = (c - (nc - 1) * 0.5) * spacing;
              addWindow(ox + ax * t, y, oz + az * t, rotY, 1.2, 1.5, b.qi, null);
            }
          }
        }
      }
    }
    modelWindowCount = windowCursor - winStart;
  }

  windows.count = windowCursor;
  if (windows.instanceColor) windows.instanceColor.needsUpdate = true;
  windows.instanceMatrix.needsUpdate = true;
  if (frames) {
    frames.count = frameCursor;
    if (frames.instanceColor) frames.instanceColor.needsUpdate = true;
    frames.instanceMatrix.needsUpdate = true;
  }
  if (shopGlass) {
    shopGlass.count = glassCursor;
    if (shopGlass.instanceColor) shopGlass.instanceColor.needsUpdate = true;
    shopGlass.instanceMatrix.needsUpdate = true;
  }

  // ------------------------------------------------------------------ mural walls
  // A mural goes on a blind wall. The wall has to be blind for a reason: it is the rear wall a
  // block shows its courtyard, or a flank of an outer-belt slab whose only windows sit on its
  // street face. Sampling the roofs in front of the wall tells the pass how much of it stands
  // clear of the neighbour, which is exactly the band a painter would use. The paint is one
  // canvas atlas, one mesh and one draw call, and every panel leans 6 cm off its wall with its
  // own polygon offset, the same discipline as the road paint, so no panel is ever coplanar with
  // the wall it is painted on.
  const MURAL_MAX = 10;
  const MURAL_LIFT = 0.06;
  // Ten designs and ten cells: the atlas carries exactly the paint the city can show and no cell
  // is drawn that no wall can use.
  const MURAL_COLS = 5;
  const MURAL_ROWS = 2;
  const MURAL_CELL_W = 256;
  const MURAL_CELL_H = 384;
  // A blind wall is tall and narrow: a party wall between two plots is 4 to 9 metres wide with 8
  // to 20 metres of clear height above the neighbour, so the panel is portrait and the atlas cell
  // is portrait with it, one design per cell, nothing stretched.
  const MURAL_ASPECT = MURAL_CELL_H / MURAL_CELL_W;
  // The frame is the district shot camera.mjs composes, 1920 by 1080: the eyes and tangents
  // themselves are the shared FRAME_VIEWS built before the expressway.
  const FRAME_HALF_W = 1920 * 0.5;
  const FRAME_HALF_H = 1080 * 0.5;
  const MURAL_MIN_PX = 700;
  const MURAL_TERRA = '#b4512f';
  const MURAL_TERRA_DEEP = '#8f3c22';
  const MURAL_MUSTARD = '#d9a12c';
  const MURAL_COBALT = '#2c4e9e';
  const MURAL_COBALT_DEEP = '#1f3873';
  const MURAL_SEA = '#2a7d6a';
  const MURAL_OFF = '#f1e6cd';
  const MURAL_INK = '#35261f';
  const MURAL_PLUM = '#6d2f3a';

  // A mural has to be seen, and "seen" is not one camera. Cycle 18's pass only looked at walls
  // whose normal pointed at the district view, which was right when the outer belt was a handful
  // of free-standing slabs facing the frame. Now the belt fronts five arterials, so the exposed
  // flanks of those blocks face sideways and backwards, and a district-only cull rejects every
  // one of them: the programme fell from ten panels to five while the outer fabric grew.
  //
  // The pass therefore scores a wall in every view the app can take: the district frame, and the
  // four street-tier rotations (the same 40 degree pitch at 0.38 of the district distance, which
  // is where a panel at eye level actually reads). A wall is painted when the best of those five
  // views holds the whole panel inside the frame at MURAL_MIN_PX. Everything else about the
  // panel is unchanged: blind wall, party-wall clearance, a 0.7 m inset from every corner and the
  // 12 m spacing rule.
  const MURAL_VIEWS = FRAME_VIEWS;

  // The projected position of a world point in one view: normalised x and y, where the frame
  // spans -1..1 in each axis. Returns false for a point behind the eye.
  const muralProj = [0, 0];
  function projectWall(view, x, y, z) {
    const vx = x - view.ex;
    const vy = y - view.ey;
    const vz = z - view.ez;
    const depth = vx * view.fx + vy * view.fy + vz * view.fz;
    if (depth < 1) return false;
    muralProj[0] = (vx * view.rx + vy * view.ry + vz * view.rz) / (depth * TAN_H);
    muralProj[1] = (vx * view.ux + vy * view.uy + vz * view.uz) / (depth * TAN_V);
    return true;
  }

  // The tallest roof standing in front of a wall, sampled outward from its plane. A party wall's
  // neighbour is what a mural has to clear; an exposed flank sees only the ground and returns
  // -Infinity.
  function roofInFront(wx, wz, nx, nz, reach) {
    let top = -Infinity;
    for (let d = 1; d <= reach; d += 1) {
      const px = wx + nx * d;
      const pz = wz + nz * d;
      for (let i = 0; i < bldObb.n; i += 1) {
        const dx = px - bldObb.cx[i];
        const dz = pz - bldObb.cz[i];
        const span2 = bldObb.hw[i] + bldObb.hd[i];
        if (dx * dx + dz * dz > span2 * span2) continue;
        const lu = dx * bldObb.ux[i] + dz * bldObb.uz[i];
        const lv = dx * bldObb.vx[i] + dz * bldObb.vz[i];
        if (Math.abs(lu) > bldObb.hw[i] || Math.abs(lv) > bldObb.hd[i]) continue;
        if (blds[i].top > top) top = blds[i].top;
      }
    }
    return top;
  }

  const muralWalls = [];
  const muralAudit = { faces: 0, blind: 0, room: 0, framed: 0, over700: 0, over1500: 0 };
  for (let bi = 0; bi < blds.length; bi += 1) {
    const b = blds[bi];
    if (b.instance === undefined || b.tower) continue;
    if (b.top - b.gMax < 7.5) continue;
    const faces = [
      { nx: b.nx, nz: b.nz, off: b.hv, span: b.w },
      { nx: b.ux, nz: b.uz, off: b.hu, span: b.d },
      { nx: -b.ux, nz: -b.uz, off: b.hu, span: b.d },
      { nx: -b.nx, nz: -b.nz, off: b.hv, span: b.w }
    ];
    for (let f = 0; f < 4; f += 1) {
      const face = faces[f];
      const nx = face.nx;
      const nz = face.nz;
      muralAudit.faces += 1;
      if (faceWindows[bi * 4 + f] !== 0) continue;
      muralAudit.blind += 1;
      const span = face.span - 1.4;
      if (span < 4.6) continue;
      const wx = b.cx + nx * face.off;
      const wz = b.cz + nz * face.off;
      const front = roofInFront(wx, wz, nx, nz, 7.5);
      let base = b.gMax + 2.4;
      if (isFinite(front)) {
        if (front + 1.2 < base) continue;       // the neighbour stands as tall as the wall
        base = front + 1.2;
      }
      const head = b.top - 1.1;
      const room = head - base;
      if (room < 5.0) continue;
      muralAudit.room += 1;
      const width = Math.min(span, 9.0);
      const height = Math.min(room, width * MURAL_ASPECT);
      if (width < 4.6 || height < 5.0) continue;
      const yc = base + room * 0.5;
      // The four corners, projected into each view in turn: the panel counts where the best of
      // the five frames holds the whole of it, which is what puts paint on the flanks that face
      // the new arterials and not only on the walls that face the district shot.
      const tx = nz;
      const tz = -nx;
      const hw = width * 0.5;
      const hh = height * 0.5;
      let bestPx = 0;
      let bestView = -1;
      for (let v = 0; v < MURAL_VIEWS.length; v += 1) {
        const view = MURAL_VIEWS[v];
        let x0 = Infinity;
        let x1 = -Infinity;
        let y0 = Infinity;
        let y1 = -Infinity;
        let ok = true;
        for (let k = 0; k < 4 && ok; k += 1) {
          const sx = k & 1 ? hw : -hw;
          const sy = k & 2 ? hh : -hh;
          ok = projectWall(view, wx + tx * sx + nx * MURAL_LIFT, yc + sy, wz + tz * sx + nz * MURAL_LIFT);
          if (!ok) break;
          if (muralProj[0] < x0) x0 = muralProj[0];
          if (muralProj[0] > x1) x1 = muralProj[0];
          if (muralProj[1] < y0) y0 = muralProj[1];
          if (muralProj[1] > y1) y1 = muralProj[1];
        }
        if (!ok) continue;
        if (x0 < -0.97 || x1 > 0.97 || y0 < -0.97 || y1 > 0.97) continue;
        const px = (x1 - x0) * FRAME_HALF_W * (y1 - y0) * FRAME_HALF_H;
        if (px > bestPx) {
          bestPx = px;
          bestView = v;
        }
      }
      if (bestView < 0) continue;
      muralAudit.framed += 1;
      if (bestPx >= 700) muralAudit.over700 += 1;
      if (bestPx >= 1500) muralAudit.over1500 += 1;
      if (bestPx < MURAL_MIN_PX) continue;
      muralWalls.push({
        nx: nx, nz: nz, wx: wx, wz: wz, y: yc, w: width, h: height, px: bestPx,
        bi: bi, view: bestView
      });
    }
  }
  // Largest projected wall first, and no two panels closer than a fifth of the district, so the
  // paint spreads over the city as a public-art programme instead of clustering on one street.
  // A building takes one panel: with all four faces now eligible, the flanks of a twenty metre
  // block would otherwise be painted twice, which reads as a wrapped billboard rather than as
  // the one blind wall a painter was given.
  muralWalls.sort(function (a, b2) { return b2.px - a.px; });
  const muralPicks = [];
  const muralTaken = new Uint8Array(blds.length);
  let muralPx = 0;
  for (let i = 0; i < muralWalls.length && muralPicks.length < MURAL_MAX; i += 1) {
    const w = muralWalls[i];
    if (muralTaken[w.bi]) continue;
    let near = false;
    for (let k = 0; k < muralPicks.length; k += 1) {
      const dx = muralPicks[k].wx - w.wx;
      const dz = muralPicks[k].wz - w.wz;
      if (dx * dx + dz * dz < 12 * 12) {
        near = true;
        break;
      }
    }
    if (near) continue;
    muralTaken[w.bi] = 1;
    // Which street the painted wall stands on. The building already carries the fabric pass's
    // own label for the ring and the five arterials; a core block is named by the nearest of
    // them, so the audit answers "on which streets" with a word and not a coordinate.
    let on = blds[w.bi].street || '';
    if (!on) {
      let best = Infinity;
      const probes = [
        ['avenue', avePath], ['ring', ringPath],
        ['west', spurPaths[0]], ['east', spurPaths[1]], ['north', spurPaths[2]],
        ['westLower', spurPaths[3]], ['hillside', spurPaths[4]]
      ];
      for (let p = 0; p < probes.length; p += 1) {
        const path = probes[p][1];
        if (!path) continue;
        let bd = Infinity;
        for (let s = 0; s <= path.n; s += 3) {
          const dx = path.xs[s] - w.wx;
          const dz = path.zs[s] - w.wz;
          const d = dx * dx + dz * dz;
          if (d < bd) bd = d;
        }
        if (bd < best) {
          best = bd;
          on = probes[p][0];
        }
      }
      if (!on) on = 'grid';
    }
    w.on = on;
    muralPicks.push(w);
    muralPx += w.px;
  }

  // The atlas: twelve portrait designs, and the six cobalt fields lead the assignment so the
  // largest walls of the programme carry the colour that is new to this district.
  const MURAL_CELLS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
  function paintMurals(ctx) {
    function wave(s, w, y0, amp, crests, thick, col) {
      ctx.strokeStyle = col;
      ctx.lineWidth = thick;
      ctx.beginPath();
      for (let k = 0; k <= 24; k += 1) {
        const x = (k / 24) * w;
        const y = y0 + Math.sin((k / 24) * crests * Math.PI * 2) * amp;
        if (k === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }
    function disc(cx, cy, r, col) {
      ctx.fillStyle = col;
      ctx.beginPath();
      ctx.arc(cx, cy, r, 0, Math.PI * 2);
      ctx.fill();
    }
    function tri(x0, y0, x1, y1, x2, y2, col) {
      ctx.fillStyle = col;
      ctx.beginPath();
      ctx.moveTo(x0, y0);
      ctx.lineTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.closePath();
      ctx.fill();
    }
    function cell(i, draw) {
      const ox = (i % MURAL_COLS) * MURAL_CELL_W;
      const oy = Math.floor(i / MURAL_COLS) * MURAL_CELL_H;
      ctx.save();
      ctx.beginPath();
      ctx.rect(ox, oy, MURAL_CELL_W, MURAL_CELL_H);
      ctx.clip();
      ctx.translate(ox, oy);
      draw(MURAL_CELL_W, MURAL_CELL_H);
      ctx.restore();
    }
    // 0: a quarter sun with its rays over a cobalt field, the widest of the cobalt panels.
    cell(0, function (w, h) {
      ctx.fillStyle = MURAL_COBALT;
      ctx.fillRect(0, 0, w, h);
      ctx.strokeStyle = MURAL_MUSTARD;
      ctx.lineWidth = w * 0.05;
      for (let k = 0; k < 9; k += 1) {
        const a = Math.PI + (k / 8) * Math.PI;
        ctx.beginPath();
        ctx.moveTo(w * 0.5 + Math.cos(a) * w * 0.3, h * 0.36 + Math.sin(a) * w * 0.3);
        ctx.lineTo(w * 0.5 + Math.cos(a) * w * 0.46, h * 0.36 + Math.sin(a) * w * 0.46);
        ctx.stroke();
      }
      disc(w * 0.5, h * 0.36, w * 0.28, MURAL_MUSTARD);
      disc(w * 0.5, h * 0.36, w * 0.11, MURAL_TERRA);
      ctx.fillStyle = MURAL_OFF;
      ctx.fillRect(0, h * 0.84, w, h * 0.16);
      ctx.fillStyle = MURAL_INK;
      ctx.fillRect(0, h * 0.84, w, h * 0.02);
    });
    // 1: waves crossing the cobalt, an off-white moon in the corner.
    cell(1, function (w, h) {
      ctx.fillStyle = MURAL_COBALT;
      ctx.fillRect(0, 0, w, h);
      wave(w, w, h * 0.34, w * 0.06, 2.5, w * 0.07, MURAL_OFF);
      wave(w, w, h * 0.56, w * 0.06, 2.5, w * 0.07, MURAL_OFF);
      wave(w, w, h * 0.78, w * 0.06, 2.5, w * 0.07, MURAL_MUSTARD);
      disc(w * 0.72, h * 0.14, w * 0.15, MURAL_OFF);
      disc(w * 0.63, h * 0.12, w * 0.15, MURAL_COBALT);
    });
    // 2: a diamond lattice with a terracotta heart, diamonds stretched with the wall.
    cell(2, function (w, h) {
      ctx.fillStyle = MURAL_COBALT;
      ctx.fillRect(0, 0, w, h);
      ctx.strokeStyle = MURAL_OFF;
      ctx.lineWidth = w * 0.03;
      const rows = 5;
      for (let k = 0; k < rows; k += 1) {
        const cy = (h / rows) * (k + 0.5);
        ctx.beginPath();
        ctx.moveTo(0, cy - h / rows * 0.5);
        ctx.lineTo(w * 0.5, cy);
        ctx.lineTo(w, cy - h / rows * 0.5);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(0, cy + h / rows * 0.5);
        ctx.lineTo(w * 0.5, cy);
        ctx.lineTo(w, cy + h / rows * 0.5);
        ctx.stroke();
      }
      ctx.fillStyle = MURAL_TERRA;
      ctx.beginPath();
      ctx.moveTo(w * 0.5, h * 0.36);
      ctx.lineTo(w * 0.72, h * 0.5);
      ctx.lineTo(w * 0.5, h * 0.64);
      ctx.lineTo(w * 0.28, h * 0.5);
      ctx.closePath();
      ctx.fill();
    });
    // 3: three arches over cobalt with sea green keystones.
    cell(3, function (w, h) {
      ctx.fillStyle = MURAL_COBALT;
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = MURAL_OFF;
      const aw = w * 0.19;
      const aw2 = w * 0.11;
      for (let k = 0; k < 3; k += 1) {
        const cx = w * (0.2 + k * 0.3);
        const spring = h * (0.46 + k * 0.04);
        ctx.beginPath();
        ctx.moveTo(cx - aw2, h * 0.94);
        ctx.lineTo(cx - aw2, spring);
        ctx.arc(cx, spring, aw2, Math.PI, 0);
        ctx.lineTo(cx + aw2, h * 0.94);
        ctx.closePath();
        ctx.fill();
        disc(cx, spring, aw * 0.18, MURAL_SEA);
      }
      ctx.fillStyle = MURAL_MUSTARD;
      ctx.fillRect(0, h * 0.94, w, h * 0.06);
    });
    // 4: a stepped ziggurat, off-white against cobalt.
    cell(4, function (w, h) {
      ctx.fillStyle = MURAL_COBALT;
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = MURAL_OFF;
      for (let k = 0; k < 5; k += 1) {
        const w2 = w * (0.74 - k * 0.13);
        ctx.fillRect((w - w2) * 0.5, h * (0.88 - k * 0.13), w2, h * 0.085);
      }
      disc(w * 0.5, h * 0.16, w * 0.11, MURAL_MUSTARD);
    });
    // 5: vertical terracotta bars with an off-white ring.
    cell(5, function (w, h) {
      ctx.fillStyle = MURAL_COBALT;
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = MURAL_TERRA;
      for (let k = 0; k < 4; k += 1) ctx.fillRect(w * (0.06 + k * 0.24), 0, w * 0.09, h);
      ctx.strokeStyle = MURAL_OFF;
      ctx.lineWidth = w * 0.05;
      ctx.beginPath();
      ctx.arc(w * 0.5, h * 0.5, w * 0.27, 0, Math.PI * 2);
      ctx.stroke();
      disc(w * 0.5, h * 0.5, w * 0.08, MURAL_MUSTARD);
    });
    // 6: a terracotta fan of rays, the first of the warm panels.
    cell(6, function (w, h) {
      ctx.fillStyle = MURAL_TERRA;
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = MURAL_OFF;
      ctx.beginPath();
      ctx.moveTo(w * 0.5, h);
      ctx.arc(w * 0.5, h, w * 0.86, Math.PI, 0);
      ctx.closePath();
      ctx.fill();
      ctx.strokeStyle = MURAL_TERRA_DEEP;
      ctx.lineWidth = w * 0.02;
      for (let k = 0; k < 7; k += 1) {
        const a = Math.PI + (k / 6) * Math.PI;
        ctx.beginPath();
        ctx.moveTo(w * 0.5, h);
        ctx.lineTo(w * 0.5 + Math.cos(a) * w * 0.86, h + Math.sin(a) * w * 0.86);
        ctx.stroke();
      }
      disc(w * 0.5, h * 0.78, w * 0.12, MURAL_MUSTARD);
    });
    // 7: mustard and off-white verticals with a terracotta chevron band.
    cell(7, function (w, h) {
      ctx.fillStyle = MURAL_MUSTARD;
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = MURAL_OFF;
      for (let k = 0; k < 5; k += 1) ctx.fillRect(w * (0.04 + k * 0.2), 0, w * 0.1, h * 0.6);
      ctx.fillStyle = MURAL_TERRA;
      for (let k = 0; k < 3; k += 1) {
        const y = h * (0.68 + k * 0.1);
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w * 0.5, y + h * 0.055);
        ctx.lineTo(w, y);
        ctx.lineTo(w, y + h * 0.04);
        ctx.lineTo(w * 0.5, y + h * 0.095);
        ctx.lineTo(0, y + h * 0.04);
        ctx.closePath();
        ctx.fill();
      }
    });
    // 8: sea green with off-white arcs, a quiet panel for a shaded wall.
    cell(8, function (w, h) {
      ctx.fillStyle = MURAL_SEA;
      ctx.fillRect(0, 0, w, h);
      ctx.strokeStyle = MURAL_OFF;
      ctx.lineWidth = w * 0.05;
      for (let k = 0; k < 5; k += 1) {
        ctx.beginPath();
        ctx.arc(w * 0.5, h * 1.02, w * (0.2 + k * 0.17), Math.PI * 1.08, Math.PI * 1.92);
        ctx.stroke();
      }
      disc(w * 0.5, h * 0.16, w * 0.1, MURAL_MUSTARD);
    });
    // 9: a terracotta and off-white chequer with a cobalt heart.
    cell(9, function (w, h) {
      ctx.fillStyle = MURAL_TERRA;
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = MURAL_OFF;
      for (let r = 0; r < 6; r += 1) {
        for (let c = 0; c < 4; c += 1) {
          if (((r + c) & 1) === 0) continue;
          ctx.fillRect(c * w * 0.25, r * h / 6, w * 0.25, h / 6);
        }
      }
      disc(w * 0.5, h * 0.5, w * 0.19, MURAL_COBALT_DEEP);
    });
  }

  if (muralPicks.length > 0) {
    const muralCanvas = (typeof document !== 'undefined' && document && document.createElement)
      ? document.createElement('canvas')
      : null;
    let muralTex = null;
    if (muralCanvas) {
      muralCanvas.width = MURAL_COLS * MURAL_CELL_W;
      muralCanvas.height = MURAL_ROWS * MURAL_CELL_H;
      const muralCtx = muralCanvas.getContext('2d');
      if (muralCtx) {
        paintMurals(muralCtx);
        muralTex = new THREE.CanvasTexture(muralCanvas);
        muralTex.colorSpace = THREE.SRGBColorSpace;
        muralTex.anisotropy = 4;
      }
    }
    if (!muralTex) {
      // No 2d context (a build without a document): the panels still stand on their walls, in the
      // terracotta of the first field, so the geometry and the draw call do not depend on it.
      muralTex = new THREE.DataTexture(new Uint8Array([180, 81, 47, 255]), 1, 1, THREE.RGBAFormat);
      muralTex.needsUpdate = true;
    }
    const muralPos = new Float32Array(muralPicks.length * 12);
    const muralNor = new Float32Array(muralPicks.length * 12);
    const muralUv = new Float32Array(muralPicks.length * 8);
    const muralIdx = new Uint16Array(muralPicks.length * 6);
    for (let i = 0; i < muralPicks.length; i += 1) {
      const m = muralPicks[i];
      const tx = m.nz;
      const tz = -m.nx;
      const px = m.wx + m.nx * MURAL_LIFT;
      const pz = m.wz + m.nz * MURAL_LIFT;
      const sw = m.w * 0.5;
      const sh = m.h * 0.5;
      const p0 = i * 12;
      muralPos[p0] = px - tx * sw;
      muralPos[p0 + 1] = m.y - sh;
      muralPos[p0 + 2] = pz - tz * sw;
      muralPos[p0 + 3] = px + tx * sw;
      muralPos[p0 + 4] = m.y - sh;
      muralPos[p0 + 5] = pz + tz * sw;
      muralPos[p0 + 6] = px - tx * sw;
      muralPos[p0 + 7] = m.y + sh;
      muralPos[p0 + 8] = pz - tz * sw;
      muralPos[p0 + 9] = px + tx * sw;
      muralPos[p0 + 10] = m.y + sh;
      muralPos[p0 + 11] = pz + tz * sw;
      for (let k = 0; k < 4; k += 1) {
        muralNor[p0 + k * 3] = m.nx;
        muralNor[p0 + k * 3 + 1] = 0;
        muralNor[p0 + k * 3 + 2] = m.nz;
      }
      const cellIndex = MURAL_CELLS[i % MURAL_CELLS.length];
      const col = cellIndex % MURAL_COLS;
      const row = Math.floor(cellIndex / MURAL_COLS) % MURAL_ROWS;
      const u0 = col / MURAL_COLS;
      const u1 = (col + 1) / MURAL_COLS;
      const v1 = 1 - row / MURAL_ROWS;
      const v0 = 1 - (row + 1) / MURAL_ROWS;
      const p1 = i * 8;
      muralUv[p1] = u0;
      muralUv[p1 + 1] = v0;
      muralUv[p1 + 2] = u1;
      muralUv[p1 + 3] = v0;
      muralUv[p1 + 4] = u0;
      muralUv[p1 + 5] = v1;
      muralUv[p1 + 6] = u1;
      muralUv[p1 + 7] = v1;
      const q0 = i * 6;
      const vv = i * 4;
      muralIdx[q0] = vv;
      muralIdx[q0 + 1] = vv + 1;
      muralIdx[q0 + 2] = vv + 2;
      muralIdx[q0 + 3] = vv + 2;
      muralIdx[q0 + 4] = vv + 1;
      muralIdx[q0 + 5] = vv + 3;
    }
    const muralGeo = new THREE.BufferGeometry();
    muralGeo.setAttribute('position', new THREE.BufferAttribute(muralPos, 3));
    muralGeo.setAttribute('normal', new THREE.BufferAttribute(muralNor, 3));
    muralGeo.setAttribute('uv', new THREE.BufferAttribute(muralUv, 2));
    muralGeo.setIndex(new THREE.BufferAttribute(muralIdx, 1));
    muralGeo.computeBoundingSphere();
    const muralMat = new THREE.MeshStandardMaterial({
      map: muralTex,
      roughness: 0.84,
      metalness: 0.02,
      polygonOffset: true,
      polygonOffsetFactor: -1,
      polygonOffsetUnits: -1
    });
    const muralMesh = new THREE.Mesh(muralGeo, muralMat);
    muralMesh.name = 'murals';
    muralMesh.receiveShadow = shadowsOn;
    group.add(muralMesh);
  }

  const quarterNeglected = new Uint8Array(Math.max(1, qCount));

  function applyWindowColor(i) {
    const q = windowQuarter[i];
    const code = (q >= 0 && quarterNeglected[q] === 1) ? 2 : windowWant[i];
    if (windowShown[i] === code) return false;
    windowShown[i] = code;
    windows.setColorAt(i, code === 2 ? SHUTTER : code === 1 ? LIT_WARM : LIT_DIM);
    // The halo is the pane's own light, so it follows the state that is actually shown and not
    // the one that is wanted: a shuttered quarter has no glow on its facades either.
    setHaloLit(i, code === 1);
    return true;
  }

  // A neglected quarter keeps its shutters down: its windows show shutters whatever the
  // activity says, and they come back to the requested state when the flag clears.
  function setWindowLit(i, lit) {
    if (i < 0 || i >= windowCursor) return;
    windowWant[i] = lit ? 1 : 0;
    if (applyWindowColor(i) && windows.instanceColor) windows.instanceColor.needsUpdate = true;
  }
  windows.setWindowLit = setWindowLit;

  // ------------------------------------------------------------------ details
  // One instanced unit box for every small piece: plinths, parapets, roof slabs, doors, awnings,
  // balconies, pergolas, rooftop plant, solar panels, market stalls, benches, tables, hedges,
  // the median planting, the rolling stock, and on the narrow profile the pavement-level step of
  // the baked contact skirt that replaces the ground-contact blob (rank 0, one instance per
  // volume).
  //
  // Every piece carries a detail rank. The buffers are sorted by rank when they are flushed, so
  // lowering the detail level is only a change of instance count: level 0 keeps the structure,
  // level 1 adds the minor props, level 2 the flourishes.
  const detailMat = new THREE.MeshStandardMaterial({
    color: 0xffffff,
    roughness: 0.66,
    metalness: 0.05
  });
  const details = new THREE.InstancedMesh(unitBox, detailMat, DETAIL_CAP);
  details.castShadow = shadowsOn;
  details.receiveShadow = shadowsOn;
  details.name = 'details';
  details.frustumCulled = false;
  group.add(details);
  const detailBase = new Float32Array(DETAIL_CAP * 3);
  const detailsOfQuarter = [];
  for (let q = 0; q < qCount; q += 1) detailsOfQuarter.push([]);
  const dPos = new Float32Array(DETAIL_CAP * 3);
  const dRot = new Float32Array(DETAIL_CAP * 2);
  const dScale = new Float32Array(DETAIL_CAP * 3);
  const dCol = new Float32Array(DETAIL_CAP * 3);
  const dRank = new Uint8Array(DETAIL_CAP);
  const dQuarter = new Int16Array(DETAIL_CAP);
  const detailLevelCount = new Int32Array(3);
  let detailCursor = 0;
  let shopCount = 0;

  function addDetail(x, y, z, sx, sy, sz, rotY, col, qi, rotX, rank) {
    if (detailCursor >= DETAIL_CAP) return;
    const k = detailCursor;
    dPos[k * 3] = x;
    dPos[k * 3 + 1] = y;
    dPos[k * 3 + 2] = z;
    dRot[k * 2] = rotX || 0;
    dRot[k * 2 + 1] = rotY || 0;
    dScale[k * 3] = sx;
    dScale[k * 3 + 1] = sy;
    dScale[k * 3 + 2] = sz;
    col.toArray(dCol, k * 3);
    dRank[k] = rank || 0;
    dQuarter[k] = qi;
    detailCursor += 1;
  }

  // A detail in a building's local frame (lu along the frontage, lv into the block).
  function addLocal(b, lu, lv, y, sx, sy, sz, col, rotYExtra, rotX, rank) {
    addDetail(
      b.cx + b.ux * lu + b.nx * lv, y, b.cz + b.uz * lu + b.nz * lv,
      sx, sy, sz, b.rotY + (rotYExtra || 0), col, b.qi, rotX, rank
    );
  }

  // The details pool reports what it holds after the flush: how many pieces were laid, how
  // many the budget kept, and how many of the rank-2 tail the cap dropped, so a fabric pass
  // that overfills the pool is a number in the audit and not a mystery in a screenshot.
  const detailAudit = { cursor: 0, rank0: 0, rank1: 0, rank2: 0, budget: DETAIL_BUDGET, kept: 0, dropped: 0 };
  function flushDetails() {
    const c0 = detailCursor;
    let n0 = 0;
    let n1 = 0;
    for (let i = 0; i < c0; i += 1) {
      if (dRank[i] === 0) n0 += 1;
      else if (dRank[i] === 1) n1 += 1;
    }
    detailAudit.cursor = c0;
    detailAudit.rank0 = n0;
    detailAudit.rank1 = n1;
    detailAudit.rank2 = c0 - n0 - n1;
    detailAudit.kept = Math.min(c0, DETAIL_BUDGET);
    detailAudit.dropped = Math.max(0, c0 - DETAIL_BUDGET);
    detailLevelCount[0] = n0;
    detailLevelCount[1] = n0 + n1;
    // The tail of the lowest rank is what the budget drops: ranks are already in ascending
    // order here, so the kept set stays a prefix and setDetailLevel keeps working unchanged.
    detailLevelCount[2] = Math.min(c0, DETAIL_BUDGET);
    let at = 0;
    for (let r = 0; r < 3; r += 1) {
      for (let i = 0; i < c0; i += 1) {
        if (dRank[i] !== r) continue;
        if (at >= detailLevelCount[2]) break;
        euler.set(dRot[i * 2], dRot[i * 2 + 1], 0, 'YXZ');
        quat.setFromEuler(euler);
        mPos.set(dPos[i * 3], dPos[i * 3 + 1], dPos[i * 3 + 2]);
        scaleV.set(dScale[i * 3], dScale[i * 3 + 1], dScale[i * 3 + 2]);
        m4.compose(mPos, quat, scaleV);
        details.setMatrixAt(at, m4);
        colorScratch.fromArray(dCol, i * 3);
        details.setColorAt(at, colorScratch);
        colorScratch.toArray(detailBase, at * 3);
        const q = dQuarter[i];
        if (q >= 0) detailsOfQuarter[q].push(at);
        at += 1;
      }
    }
    details.count = at;
    if (details.instanceColor) details.instanceColor.needsUpdate = true;
    details.instanceMatrix.needsUpdate = true;
  }

  // The detail layer at night runs one rank lower than the notch ladder asked for: the rank 2
  // flourishes (roof clutter, minor plant, the smallest props) cover 0.068 percent of the phone
  // night frame and 0.076 of the desktop one, because the dark leaves them no light to read by,
  // while they cost 4332 and 7968 triangles there. The ladder keeps its own rung and gets it
  // back at dawn, and nothing here rebuilds or reallocates: only the drawn instance count moves.
  let detailLevel = 2;
  let nightNow = false;
  function applyDetailCount() {
    const l = nightNow && detailLevel > 1 ? 1 : detailLevel;
    details.count = detailLevelCount[l];
    // The street pool draws by the same prefix rule: everything at level 2, the avenue prefix at
    // level 1 (which is what the night draw uses), nothing at level 0 or before the pool exists.
    // The pool's instances are boxes, not pieces, so the level 2 count is the box tally the
    // build pass recorded. On touch the pool does not exist, and the phone's triangle budget is
    // the reason it does not.
    if (streetMesh) streetMesh.count = l === 2 ? streetAudit.boxes : l === 1 ? streetLevel1 : 0;
  }

  function setDetailLevel(level) {
    const n = (typeof level === 'number' && isFinite(level)) ? Math.round(level) : 2;
    detailLevel = n < 0 ? 0 : n > 2 ? 2 : n;
    applyDetailCount();
    return detailLevel;
  }

  const C_COPING = new THREE.Color(0xe8dcc2);
  const C_WOOD = new THREE.Color(0xae8256);
  const C_POST = new THREE.Color(0x7d5636);
  const C_PLANT = new THREE.Color(0x5f8c3c);
  const C_SOLAR = new THREE.Color(0x2c3f5c);
  const C_TANK = new THREE.Color(0xbfc4c8);
  // Rooftop life extensions: a chimney stack in faded plaster with a coping cap, and the
  // painted steel of antenna masts, stair bulkheads and drying racks.
  const C_CHIMNEY = new THREE.Color(0x8f8175);
  const C_BULKHEAD = new THREE.Color(0xcabfa9);
  const ROOF_NEUTRAL = new THREE.Color(0xd8ccb6);
  // Street-detail palette: the litter of a working pavement. Forest green bins with cream lids,
  // painted steel bollards with their reflective band, oiled bench slats on dark steel frames,
  // terracotta planters with clipped shrub, sandwich boards, fruit crates, a red scooter and
  // one grey service box per block face, with the lamps the wall brackets carry at night.
  const C_BIN = new THREE.Color(0x2f4f3a);
  const C_BIN_LID = new THREE.Color(0xd8dcc9);
  const C_BOLLARD = new THREE.Color(0x274b52);
  const C_BOLLARD_BAND = new THREE.Color(0xe6e0cf);
  const C_BENCH_WOOD = new THREE.Color(0xa5793f);
  const C_PLANTER = new THREE.Color(0x9c5a36);
  const C_SHRUB = new THREE.Color(0x4b7238);
  const C_ABOARD = new THREE.Color(0x2a2622);
  const C_UTIL = new THREE.Color(0x77736a);
  const C_LAMP_METAL = new THREE.Color(0xd8c48a);
  // The dish arm colour is shared by the facade dish and the rooftop clutter below, so it is
  // declared with the facade palette, before either pass runs.
  const C_DISH_ARM = new THREE.Color(0x565a5c);
  // Shopfront vocabulary. Awning canvas is saturated on purpose: a deep red, a teal, a mustard,
  // a forest green and one cream, so the boulevard carries separate saturated clusters at street
  // tier instead of one beige family. The trims are painted joinery, not raw concrete.
  const AWNING_TONES = [
    new THREE.Color(0xb02a1e), new THREE.Color(0x0f6f6b), new THREE.Color(0xd39b18),
    new THREE.Color(0x1f5c33), new THREE.Color(0xb02a1e)
  ];
  const C_AWNING_CREAM = new THREE.Color(0xefe3c6);
  const C_SHOP_TRIM = new THREE.Color(0x4a3f34);
  const C_SHOP_TRIM_LIGHT = new THREE.Color(0x8b7a63);
  const C_SHOP_DOOR = new THREE.Color(0x2b2f33);
  const SHOP_NAMES = [
    'Forno', 'Tabacchi', 'Bar Centrale', 'Gelateria', 'Salumeria', 'Libreria', 'Caffe',
    'Panetteria', 'Emporio', 'Trattoria', 'Ottica', 'Edicola', 'Merceria', 'Pasticceria',
    'Ferramenta', 'Profumeria', 'Cartoleria', 'Boutique', 'Pescheria', 'Macelleria',
    'Osteria', 'Cioccolato', 'Ceramiche', 'Farmacia', 'Frutteria', 'Sartoria'
  ];
  const accentCol = new THREE.Color();
  const roofCol = new THREE.Color();
  const doorCol = new THREE.Color();
  const parapetCol = new THREE.Color();
  const socleCol = new THREE.Color();
  const awningCol = new THREE.Color();
  const shopSigns = [];

  // Rooftop occupancy in the building's own frame, four numbers per rectangle. Every piece the
  // loop below puts on a roof claims its footprint here first, so the clutter pass after the
  // loop can place condensers, tanks and dishes by rule instead of by luck.
  const roofRects = new Array(blds.length);
  function roofClaim(bi, lu, lv, su, sv, pad) {
    let list = roofRects[bi];
    if (!list) {
      list = [];
      roofRects[bi] = list;
    }
    const mu = su * 0.5 + (pad || 0);
    const mv = sv * 0.5 + (pad || 0);
    list.push(lu - mu, lu + mu, lv - mv, lv + mv);
  }

  for (let bi = 0; bi < blds.length; bi += 1) {
    const b = blds[bi];
    if (b.instance === undefined) {
      // A model lot borrows its whole volume from the pack, so its procedural socle, parapet,
      // roof clutter, shop band and balconies are all suppressed. The narrow profile has no
      // contact blob, so it keeps the same baked contact skirt every procedural volume gets:
      // one 0.62 tall slab, 0.3 m proud, 0.64 of the building's colour, rank 0.
      if (b.model) {
        if (NARROW_VIEW) {
          socleCol.setHex(b.color).multiplyScalar(b.shade * 0.64);
          addLocal(b, 0, 0, b.gMax + 0.25, b.w + 0.6, 0.62, b.d + 0.6, socleCol, 0, 0, 0);
        }
        continue;
      }
      // A tower has no building instance, because its curtain wall is the dedicated mesh below.
      // The narrow profile still needs the contact apron at its base, and this is the only
      // detail a tower takes from this pool: a 0.9 tall slab, 0.3 m proud, 0.64 of the tower's
      // own colour, standing on the parcel datum with its open bottom 6 cm into the ground.
      if (NARROW_VIEW && b.tower) {
        socleCol.setHex(b.color).multiplyScalar(b.shade * 0.64);
        addLocal(b, 0, 0, b.gMax + 0.39, b.w + 0.6, 0.9, b.d + 0.6, socleCol, 0, 0, 0);
      }
      continue;
    }
    const qs = (b.qi >= 0 ? quartersModel[b.qi] : null) || {};
    const rnd = b.rnd;
    accentCol.setHex(qs.color || 0xc8643c);
    roofCol.copy(accentCol).lerp(ROOF_NEUTRAL, 0.24);
    doorCol.copy(accentCol).multiplyScalar(0.66);
    const hw = b.hu;
    const hd = b.hv;
    const faceW = b.w;
    const rotExtra = b.fYaw - b.rotY;
    const lvFront = hd - 0.15;
    const lvBack = -hd + 0.15;
    const isShop = shopAt(b);

    // Ground-contact socle: a dark band where the block meets the pavement, the geometry half
    // of the baked ambient occlusion, tinted from the facade so it belongs to the wall. A shop
    // block keeps a low plinth instead, because the shopfront starts just above it.
    socleCol.setHex(b.color).multiplyScalar(b.shade * 0.54);
    if (isShop) {
      addLocal(b, 0, 0, b.gMax + 0.16, b.w + 0.2, 0.32, b.d + 0.2, socleCol, 0, 0, 0);
    } else {
      addLocal(b, 0, 0, b.gMax + 0.36, b.w + 0.26, 0.72, b.d + 0.26, socleCol, 0, 0, 0);
    }

    // Contact apron, narrow profile only. The phone frame has no shadow map and no contact
    // blob, so a volume reads as pasted on the pavement. This is that missing ring, in the
    // details pool and at rank 0, so the mesh already in the frame draws it at detail level 1
    // and at night with no new call. It is a 0.62 tall slab standing on the parcel datum, 0.3 m
    // proud of the wall on every side, tinted 0.64 of the building's own instance colour: the
    // pavement-level half of the baked contact gradient, with the existing socle rising above
    // it as the inner step. Its bottom is 6 cm below the datum and is open, so the only faces
    // that meet the pavement are vertical; its sides stand 0.3 m clear of the wall and its top
    // ledge is a free horizontal face, so nothing is coplanar and no depth bias is needed. It is
    // not built for the desktop frame.
    if (NARROW_VIEW) {
      socleCol.setHex(b.color).multiplyScalar(b.shade * 0.64);
      addLocal(b, 0, 0, b.gMax + 0.25, b.w + 0.6, 0.62, b.d + 0.6, socleCol, 0, 0, 0);
    }

    parapetCol.setHex(b.color).lerp(C_COPING, 0.5);
    addLocal(b, 0, 0, b.top + 0.06, b.w - 0.5, 0.12, b.d - 0.5, roofCol, 0, 0, 0);
    // A parapet that closes all four roof edges, so the roofline reads as a drawn line against
    // the sky from every rotation instead of only on the street front.
    addLocal(b, 0, lvFront, b.top + 0.36, b.w, 0.72, 0.28, parapetCol, 0, 0, 0);
    addLocal(b, 0, lvBack, b.top + 0.36, b.w, 0.72, 0.28, parapetCol, 0, 0, 0);
    addLocal(b, hw - 0.13, 0, b.top + 0.36, 0.26, 0.72, b.d, parapetCol, 0, 0, 1);
    addLocal(b, -hw + 0.13, 0, b.top + 0.36, 0.26, 0.72, b.d, parapetCol, 0, 0, 1);

    // Rooftop life on about a third of the roofs: the optional clutter that the detail level
    // thins out first.
    if (richDetail) {
      const roll = rnd();
      if (roll < 0.13) {
        const tw = Math.min(4.4, b.w * 0.55);
        const td = Math.min(4.2, b.d * 0.6);
        const su = rnd() < 0.5 ? 1 : -1;
        const sv = rnd() < 0.5 ? 1 : -1;
        const cu = su * (hw - 0.5 - tw * 0.5);
        const cv = sv * (hd - 0.5 - td * 0.5);
        addLocal(b, cu, cv, b.top + 0.13, tw, 0.08, td, C_WOOD, 0, 0, 1);
        for (let p = 0; p < 4; p += 1) {
          const pu = cu + ((p & 1) ? 1 : -1) * (tw * 0.5 - 0.25);
          const pv = cv + ((p & 2) ? 1 : -1) * (td * 0.5 - 0.25);
          addLocal(b, pu, pv, b.top + 1.3, 0.15, 2.3, 0.15, C_POST, 0, 0, 2);
        }
        for (let s = 0; s < 3; s += 1) {
          const sv2 = cv + (s / 2 - 0.5) * (td - 0.5);
          addLocal(b, cu, sv2, b.top + 2.4, tw - 0.2, 0.11, 0.22, C_WOOD, 0, 0, 1);
        }
        addLocal(b, -cu * 0.55, -cv * 0.45, b.top + 0.32, 0.75, 0.6, 0.75, C_PLANT, 0, 0, 2);
        addLocal(b, -cu * 0.55 + 1.1, -cv * 0.45, b.top + 0.32, 0.75, 0.6, 0.75, C_PLANT, 0, 0, 2);
        roofClaim(bi, cu, cv, tw, td, 0.55);
        roofClaim(bi, -cu * 0.55, -cv * 0.45, 0.75, 0.75, 0.3);
        roofClaim(bi, -cu * 0.55 + 1.1, -cv * 0.45, 0.75, 0.75, 0.3);
      } else if (roll < 0.22) {
        addLocal(b, hw * 0.35, -hd * 0.4, b.top + 0.9, 1.5, 1.8, 1.5, C_TANK, 0, 0, 1);
        roofClaim(bi, hw * 0.35, -hd * 0.4, 1.5, 1.5, 0.3);
      } else if (roll < 0.29) {
        addLocal(b, -hw * 0.2, hd * 0.3, b.top + 0.42, Math.min(3.6, b.w * 0.5), 0.16, 2.0,
          C_SOLAR, 0, 0.42, 1);
        roofClaim(bi, -hw * 0.2, hd * 0.3, Math.min(3.6, b.w * 0.5), 2.2, 0.32);
      } else if (roll < 0.34) {
        addLocal(b, hw * 0.45, hd * 0.42, b.top + 0.34, 0.8, 0.55, 0.8, C_PLANT, 0, 0, 2);
        addLocal(b, hw * 0.45 - 1.1, hd * 0.42, b.top + 0.34, 0.8, 0.55, 0.8, C_PLANT, 0, 0, 2);
        roofClaim(bi, hw * 0.45, hd * 0.42, 0.8, 0.8, 0.3);
        roofClaim(bi, hw * 0.45 - 1.1, hd * 0.42, 0.8, 0.8, 0.3);
      } else if (roll < 0.40) {
        // A chimney: a plaster stack with a coping cap, breaking the roof edge line the way
        // the older blocks do.
        const chu = (rnd() < 0.5 ? 1 : -1) * hw * 0.3;
        const chv = (rnd() < 0.5 ? 1 : -1) * hd * 0.3;
        addLocal(b, chu, chv, b.top + 0.62, 0.55, 1.1, 0.55, C_CHIMNEY, 0, 0, 1);
        addLocal(b, chu, chv, b.top + 1.19, 0.75, 0.08, 0.75, C_COPING, 0, 0, 1);
        roofClaim(bi, chu, chv, 0.55, 0.55, 0.25);
      } else if (roll < 0.44) {
        // A television aerial: a thin mast with one crossbar, reading as a spike against the
        // sky from every district rotation.
        const anu = (rnd() < 0.5 ? 1 : -1) * hw * 0.4;
        const anv = (rnd() < 0.5 ? 1 : -1) * hd * 0.4;
        addLocal(b, anu, anv, b.top + 1.3, 0.08, 2.4, 0.08, C_DISH_ARM, 0, 0, 2);
        addLocal(b, anu, anv, b.top + 2.1, 0.95, 0.06, 0.06, C_DISH_ARM, 0, 0, 2);
        roofClaim(bi, anu, anv, 0.5, 0.5, 0.05);
      }
    }

    if (isShop && faceW >= 2.9 && shopCount < SHOP_MAX && detailCursor < DETAIL_CAP - 24) {
      // ---------------------------------------------------------------- shopfront storey
      // A one storey band from the plinth to the floor line: glazing just proud of the wall, a
      // deep joinery frame standing in front of it, a fascia board carrying the shop's name and,
      // on most shops, a striped awning over the glass. Nothing here is a per-shop mesh: the
      // glass is one instanced quad, the joinery one instanced box, the name one atlas cell.
      // Avenue plots are narrow (three and a half to four and a half metres), so every size here
      // scales with the face instead of assuming a wide frontage.
      shopCount += 1;
      const shopW = faceW - 0.56;
      const glassW = shopW - 0.44;
      const glassY = b.gMax + 1.19;
      const fq = hash2(Math.round(b.cx * 2) + 512, Math.round(b.cz * 2) + 512);
      // The pane sits just proud of the wall plane, with the joinery standing 0.22 in front of
      // it: a solid block has no hole to recess into, so the reveal is carried by the frame.
      addShopGlass(b.fx + b.nx * 0.03, glassY, b.fz + b.nz * 0.03, b.fYaw, glassW, 1.53, b.qi);
      // On the phone the joinery is dropped, not thinned: jambs, head, sill, door, string course
      // and the fallback board are 0.06 to 0.36 m deep boxes seen at 412x915 through a viewport
      // where the whole shopfront is under 40 pixels tall, so each reads as sub-pixel line noise
      // at a measured 4,200 triangles across the frame. The pane, the fascia name board and the
      // awning stay: those are the pieces the phone's own screenshots were judged on.
      if (!NARROW_VIEW) {
        const jamb = Math.min(0.2, shopW * 0.07);
        addLocal(b, -(shopW * 0.5 - jamb * 0.5), hd + 0.04, b.gMax + 1.22, jamb, 2.34, 0.36,
          C_SHOP_TRIM, 0, 0, 1);
        addLocal(b, (shopW * 0.5 - jamb * 0.5), hd + 0.04, b.gMax + 1.22, jamb, 2.34, 0.36,
          C_SHOP_TRIM, 0, 0, 1);
        addLocal(b, 0, hd + 0.04, b.gMax + 2.04, shopW, 0.2, 0.34, C_SHOP_TRIM, 0, 0, 1);
        addLocal(b, 0, hd + 0.04, b.gMax + 0.37, shopW, 0.16, 0.32, C_SHOP_TRIM_LIGHT, 0, 0, 1);
        // Glass door at one end of the shopfront, proud of the pane so nothing is coplanar.
        const doorW = Math.min(0.94, shopW * 0.36);
        addLocal(b, shopW * 0.5 - doorW * 0.5 - 0.14, hd + 0.09, b.gMax + 1.16, doorW, 1.86, 0.06,
          C_SHOP_DOOR, 0, 0, 1);
      }
      // The fascia board: its name is drawn into the shared atlas by the signs section. A shop
      // without a spare atlas cell still gets the painted board, so the band never reads empty;
      // on the phone the name atlas still draws, so only the fallback board is dropped with the
      // rest of the joinery.
      if (shopSigns.length < 40) {
        shopSigns.push({
          x: b.fx + b.nx * 0.03,
          y: b.gMax + 2.56,
          z: b.fz + b.nz * 0.03,
          rotY: b.fYaw,
          w: Math.min(shopW - 0.24, 3.6),
          h: 0.78,
          name: SHOP_NAMES[((fq * 61) % SHOP_NAMES.length) | 0]
        });
      } else if (!NARROW_VIEW) {
        addLocal(b, 0, hd + 0.03, b.gMax + 2.56, shopW - 0.2, 0.78, 0.12,
          C_AWNING_CREAM, 0, 0, 1);
      }
      // A painted string course closes the top of the storey.
      if (!NARROW_VIEW) {
        addLocal(b, 0, hd + 0.02, b.gMax + 3.0, shopW + 0.12, 0.08, 0.3, C_SHOP_TRIM_LIGHT, 0, 0, 1);
      }
      // Striped awning over most of the shopfronts, alternating canvas and cream. It hangs from
      // the top of the fascia and projects a metre: any deeper and, from a high camera, the
      // canvas would cover the glazing underneath it and the shopfront would lose its window.
      if (shopW >= 2.3 && fq < 0.78) {
        const awnW = shopW - 0.18;
        const strips = Math.max(3, Math.min(6, Math.round(awnW / 0.68)));
        const stripW = awnW / strips;
        const drop = shopW < 3.4 ? 0.78 : 1.0;
        // The canvas tone comes from the building's own position hash, so neighbouring shops
        // never land on the same colour and the boulevard reads as separate saturated clusters.
        const toneIx = (hash2(Math.round(b.cx * 3) + 77, Math.round(b.cz * 3) + 31) *
          AWNING_TONES.length) | 0;
        awningCol.copy(AWNING_TONES[toneIx >= AWNING_TONES.length ? 0 : toneIx]);
        for (let s = 0; s < strips; s += 1) {
          const lu = -awnW * 0.5 + stripW * (s + 0.5);
          addLocal(b, lu, hd + 0.48, b.gMax + 2.52, stripW - 0.02, 0.08, drop,
            (s & 1) ? awningCol : C_AWNING_CREAM, 0, 0.34, 1);
        }
        for (let s = 0; s < strips; s += 1) {
          const lu = -awnW * 0.5 + stripW * (s + 0.5);
          addLocal(b, lu, hd + 0.48 + drop * 0.5, b.gMax + 2.52 - drop * 0.17, stripW - 0.02, 0.26, 0.06,
            (s & 1) ? C_AWNING_CREAM : awningCol, 0, 0, 2);
        }
      }
    } else {
      // Street front: a door in the quarter colour.
      addLocal(b, -faceW * 0.3, lvFront + 0.04, b.gMax + 1.62, 1.35, 2.3, 0.14, doorCol, rotExtra, 0, 0);
    }

    // Balconies: bay-aligned on the frontage (and, on the outer slopes, on the back face too),
    // riding the same window rhythm the facade draws above the ground band. Each bay of each
    // residential floor can carry a slab and a rail; a per-floor, per-bay hash skips about a
    // third of them so the rhythm stays irregular, and rank 2 puts them where the ladder and
    // the night pass shed them like the other flourishes.
    if (b.kind !== 2) {
      const balOuter = b.kind === 3;
      const balNc = balOuter ? 2 : clamp(Math.floor((b.w - 0.6) / 3.9), 1, 3);
      const balSpacing = b.w / (balNc + 0.4);
      const balW = Math.min(balSpacing * 0.72, 2.7);
      for (let fl = 1; fl < b.floors; fl += 1) {
        const y = b.gMax + 1.4 + fl * 3.0;
        if (y > b.top - 1.6) break;
        for (let c = 0; c < balNc; c += 1) {
          const t = (c - (balNc - 1) * 0.5) * balSpacing;
          if (hash2(bi * 17 + fl * 7 + c * 13, 21) > 0.62) continue;
          addLocal(b, t, lvFront + 0.55, y, balW, 0.14, 0.95, parapetCol, rotExtra, 0, 2);
          addLocal(b, t, lvFront + 0.96, y + 0.5, balW, 0.95, 0.09, parapetCol, rotExtra, 0, 2);
          if (balOuter) {
            addLocal(b, -t, lvFront + 0.55, y, balW, 0.14, 0.95, parapetCol, rotExtra, 0, 2);
            addLocal(b, -t, lvFront + 0.96, y + 0.5, balW, 0.95, 0.09, parapetCol, rotExtra, 0, 2);
          }
        }
      }
    }
  }

  // ------------------------------------------------------------------ rooftop clutter
  // Every flat roof now carries plant: condenser cases lined along the parapet, a water tank on
  // the bigger decks and a dish on a quarter of them. All of it is the same instanced detail box
  // the rest of the city's small pieces use, at rank 2, so the night pass and the notch ladder
  // shed it exactly as they shed the other flourishes and the draw call never changes.
  //
  // Placement is a rule, not a roll. A candidate must sit inside the roof slab (which is 8 cm
  // inside its own parapets), clear every piece already on the roof (the terrace, the tank, the
  // solar panel, the planters the pass above may have added) and clear every piece this pass has
  // already added. When nothing fits, the roof keeps what it has.
  const roofPlaced = [];
  const roofPre = new Int32Array(blds.length);
  const C_AC_CASE = new THREE.Color(0xc8c3b6);
  const C_AC_CASE_DARK = new THREE.Color(0xa9a69c);
  const C_AC_DARK = new THREE.Color(0x767b7e);
  const C_TANK_WHITE = new THREE.Color(0xdad4c5);
  const C_TANK_GREY = new THREE.Color(0xa6abae);
  const C_DISH = new THREE.Color(0xe6e1d3);
  let roofBoxes = 0;
  let roofFirst = 0;
  let roofAcs = 0;
  let roofTanks = 0;
  let roofDishes = 0;
  let roofBulkheads = 0;
  let roofRacks = 0;

  function roofFits(list, lu, lv, su, sv, inU, inV) {
    if (Math.abs(lu) + su * 0.5 > inU) return false;
    if (Math.abs(lv) + sv * 0.5 > inV) return false;
    const u0 = lu - su * 0.5 - 0.16;
    const u1 = lu + su * 0.5 + 0.16;
    const v0 = lv - sv * 0.5 - 0.16;
    const v1 = lv + sv * 0.5 + 0.16;
    for (let k = 0; k < list.length; k += 4) {
      if (u0 < list[k + 1] && u1 > list[k] && v0 < list[k + 3] && v1 > list[k + 2]) return false;
    }
    return true;
  }

  if (richDetail || isTouch) {
    const roofRnd = mulberry(606061);
    roofFirst = detailCursor;
    // The desktop lays the full programme: one condenser bank per roof, tanks, dishes,
    // bulkheads and drying racks. The touch frame gets a reduced subset: the condenser banks
    // only, under a small box cap, because a 412 px frame still reads a case battery on a
    // bare deck but must not spend its triangle budget on it.
    const ROOF_BOX_CAP = richDetail ? 540 : 110;
    // The eye ranks the decks: the roofs that project largest take the extras first, so the
    // tank and dish budget lands where a viewer can see it instead of on the far crown.
    const roofOrder = [];
    for (let bi = 0; bi < blds.length; bi += 1) {
      const b = blds[bi];
      if (b.instance === undefined) continue;
      const inU = b.hu - 0.33;
      const inV = b.hv - 0.33;
      if (inU < 0.8 || inV < 0.75) continue;
      const dx = b.cx - EYE_X;
      const dy = b.top - EYE_Y;
      const dz = b.cz - EYE_Z;
      roofPre[bi] = roofRects[bi] ? roofRects[bi].length : 0;
      roofOrder.push({ bi: bi, score: (4 * inU * inV) / (dx * dx + dy * dy + dz * dz) });
    }
    roofOrder.sort(function (a, b2) { return b2.score - a.score; });

    function roofListOf(bi) {
      let list = roofRects[bi];
      if (!list) {
        list = [];
        roofRects[bi] = list;
      }
      return list;
    }

    // One condenser bank on every roof. A single case the length of two units is what a battery
    // of condensers reads as at this scale, and it leaves the deck free for the tank and the dish.
    for (let o = 0; o < roofOrder.length && roofBoxes < ROOF_BOX_CAP; o += 1) {
      const b = blds[roofOrder[o].bi];
      const bi = roofOrder[o].bi;
      const inU = b.hu - 0.33;
      const inV = b.hv - 0.33;
      const list = roofListOf(bi);
      const deck = b.top + 0.12;
      const su = Math.min(1.9, inU * 2 - 0.1);
      const sv = 0.8;
      if (su < 0.9) continue;
      for (let t = 0; t < 12; t += 1) {
        // Along the inner face of the front or back parapet, which is where a condenser battery
        // really sits: it leaves the middle of the deck to the tank and the view.
        const lu = (roofRnd() * 2 - 1) * Math.max(0, inU - su * 0.5);
        const lv = (t & 1 ? 1 : -1) * Math.max(0, inV - sv * 0.5 - 0.02);
        if (!roofFits(list, lu, lv, su, sv, inU, inV)) continue;
        const tone = roofRnd();
        const caseCol = tone < 0.34 ? C_AC_CASE_DARK : tone < 0.62 ? C_AC_CASE : C_AC_DARK;
        addLocal(b, lu, lv, deck + 0.31, su, 0.62, sv, caseCol, 0, 0, 2);
        list.push(lu - su * 0.5 - 0.16, lu + su * 0.5 + 0.16,
          lv - sv * 0.5 - 0.16, lv + sv * 0.5 + 0.16);
        roofPlaced.push({ bi: bi, lu: lu, lv: lv, su: su, sv: sv, y: deck });
        roofBoxes += 1;
        roofAcs += 1;
        break;
      }
    }

    // Water tanks next, until three quarters of the box budget is spent: a body and a lid, so a
    // tank reads as a tank and not as a second condenser case. Desktop only: on the touch frame
    // the tank would cost real triangles the condensers already spent better.
    for (let o = 0; richDetail && o < roofOrder.length && roofBoxes + 2 <= 480; o += 1) {
      const b = blds[roofOrder[o].bi];
      const bi = roofOrder[o].bi;
      const inU = b.hu - 0.33;
      const inV = b.hv - 0.33;
      if (4 * inU * inV < 30) continue;
      if (roofRnd() > 0.42) continue;
      const list = roofListOf(bi);
      const deck = b.top + 0.12;
      const su = 1.3;
      const sv = 1.2;
      for (let t = 0; t < 16; t += 1) {
        const lu = (roofRnd() * 2 - 1) * Math.max(0, inU - su * 0.5);
        const lv = (roofRnd() * 2 - 1) * Math.max(0, inV - sv * 0.5);
        if (!roofFits(list, lu, lv, su, sv, inU, inV)) continue;
        const skin = roofRnd() < 0.6 ? C_TANK_WHITE : C_TANK_GREY;
        addLocal(b, lu, lv, deck + 0.75, su, 1.5, sv, skin, 0, 0, 2);
        addLocal(b, lu, lv, deck + 1.53, su * 1.04, 0.09, sv * 1.04, C_TANK_GREY, 0, 0, 2);
        list.push(lu - su * 0.5 - 0.16, lu + su * 0.5 + 0.16,
          lv - sv * 0.5 - 0.16, lv + sv * 0.5 + 0.16);
        roofPlaced.push({ bi: bi, lu: lu, lv: lv, su: su, sv: sv, y: deck });
        roofBoxes += 2;
        roofTanks += 1;
        break;
      }
    }

    // A few dishes last, on the smallest share of the decks, tilted at the sky.
    for (let o = 0; o < roofOrder.length && roofBoxes + 2 <= ROOF_BOX_CAP; o += 1) {
      const b = blds[roofOrder[o].bi];
      const bi = roofOrder[o].bi;
      const inU = b.hu - 0.33;
      const inV = b.hv - 0.33;
      if (!richDetail || roofRnd() > 0.16) continue;
      const list = roofListOf(bi);
      const deck = b.top + 0.12;
      const su = 0.95;
      const sv = 0.8;               // the plate plus the plan depth its tilt needs
      for (let t = 0; t < 16; t += 1) {
        const lu = (roofRnd() * 2 - 1) * Math.max(0, inU - su * 0.5);
        const lv = (roofRnd() * 2 - 1) * Math.max(0, inV - sv * 0.5);
        if (!roofFits(list, lu, lv, su, sv, inU, inV)) continue;
        addLocal(b, lu, lv, deck + 0.42, 0.09, 0.84, 0.09, C_DISH_ARM, 0, 0, 2);
        addLocal(b, lu, lv - 0.12, deck + 1.02, su, 0.06, su, C_DISH, 0, -0.95, 2);
        list.push(lu - su * 0.5 - 0.16, lu + su * 0.5 + 0.16,
          lv - sv * 0.5 - 0.16, lv + sv * 0.5 + 0.16);
        roofPlaced.push({ bi: bi, lu: lu, lv: lv, su: su, sv: sv, y: deck });
        roofBoxes += 2;
        roofDishes += 1;
        break;
      }
    }

    // Stair bulkheads and drying racks last, on the roofs the eye ranks highest that still
    // have room: a box with a lid reads as the stair housing a real deck must have, and two
    // posts with a line read as washing hung out to dry. Desktop only.
    if (richDetail) {
      for (let o = 0; o < roofOrder.length && roofBoxes + 2 <= ROOF_BOX_CAP; o += 1) {
        const b = blds[roofOrder[o].bi];
        const bi = roofOrder[o].bi;
        const inU = b.hu - 0.33;
        const inV = b.hv - 0.33;
        if (4 * inU * inV < 34) continue;
        if (roofRnd() > 0.3) continue;
        const list = roofListOf(bi);
        const deck = b.top + 0.12;
        const su = Math.min(1.7, inU * 2 - 0.3);
        const sv = Math.min(2.0, inV * 2 - 0.3);
        if (su < 1.1 || sv < 1.2) continue;
        for (let t = 0; t < 14; t += 1) {
          const lu = (roofRnd() * 2 - 1) * Math.max(0, inU - su * 0.5);
          const lv = (roofRnd() * 2 - 1) * Math.max(0, inV - sv * 0.5);
          if (!roofFits(list, lu, lv, su, sv, inU, inV)) continue;
          addLocal(b, lu, lv, deck + 0.62, su, 1.1, sv, C_BULKHEAD, 0, 0, 2);
          addLocal(b, lu, lv, deck + 1.19, su * 1.1, 0.1, sv * 1.1, C_COPING, 0, 0, 2);
          list.push(lu - su * 0.5 - 0.16, lu + su * 0.5 + 0.16,
            lv - sv * 0.5 - 0.16, lv + sv * 0.5 + 0.16);
          roofPlaced.push({ bi: bi, lu: lu, lv: lv, su: su, sv: sv, y: deck });
          roofBoxes += 2;
          roofBulkheads += 1;
          break;
        }
      }
      for (let o = 0; o < roofOrder.length && roofBoxes + 3 <= ROOF_BOX_CAP; o += 1) {
        const b = blds[roofOrder[o].bi];
        const bi = roofOrder[o].bi;
        const inU = b.hu - 0.33;
        const inV = b.hv - 0.33;
        if (4 * inU * inV < 24) continue;
        if (roofRnd() > 0.22) continue;
        const list = roofListOf(bi);
        const deck = b.top + 0.12;
        const su = Math.min(2.3, inU * 2 - 0.3);
        const sv = 0.5;
        for (let t = 0; t < 12; t += 1) {
          const lu = (roofRnd() * 2 - 1) * Math.max(0, inU - su * 0.5);
          const lv = (roofRnd() * 2 - 1) * Math.max(0, inV - sv * 0.5);
          if (!roofFits(list, lu, lv, su, sv, inU, inV)) continue;
          addLocal(b, lu - su * 0.5 + 0.2, lv, deck + 0.44, 0.07, 0.85, 0.07, C_POST, 0, 0, 2);
          addLocal(b, lu + su * 0.5 - 0.2, lv, deck + 0.44, 0.07, 0.85, 0.07, C_POST, 0, 0, 2);
          addLocal(b, lu, lv, deck + 0.86, su - 0.25, 0.04, 0.04, C_WOOD, 0, 0, 2);
          list.push(lu - su * 0.5 - 0.16, lu + su * 0.5 + 0.16,
            lv - sv * 0.5 - 0.16, lv + sv * 0.5 + 0.16);
          roofPlaced.push({ bi: bi, lu: lu, lv: lv, su: su, sv: sv, y: deck });
          roofBoxes += 3;
          roofRacks += 1;
          break;
        }
      }
    }
  }

  // The audit: re-test every piece this pass placed against the geometry it was placed on. A
  // piece must sit exactly on the deck, its footprint must clear both parapets and the roof
  // slab's own edge, and it must not overlap another occupant of the same roof (before or
  // after this pass). Zero is the only count this may ever report.
  let roofViolations = 0;
  for (let i = 0; i < roofPlaced.length; i += 1) {
    const p = roofPlaced[i];
    const b = blds[p.bi];
    if (Math.abs(p.y - (b.top + 0.12)) > 0.001) roofViolations += 1;
    if (Math.abs(p.lu) + p.su * 0.5 > b.hu - 0.26) roofViolations += 1;
    if (Math.abs(p.lv) + p.sv * 0.5 > b.hv - 0.30) roofViolations += 1;
    // the occupants that were on the roof before this pass
    const list = roofRects[p.bi];
    const pre = roofPre[p.bi];
    const pu0 = p.lu - p.su * 0.5;
    const pu1 = p.lu + p.su * 0.5;
    const pv0 = p.lv - p.sv * 0.5;
    const pv1 = p.lv + p.sv * 0.5;
    for (let k = 0; k < pre; k += 4) {
      if (pu0 < list[k + 1] && pu1 > list[k] && pv0 < list[k + 3] && pv1 > list[k + 2]) {
        roofViolations += 1;
        break;
      }
    }
    for (let j = 0; j < roofPlaced.length; j += 1) {
      if (j === i) continue;
      const q = roofPlaced[j];
      if (q.bi !== p.bi) continue;
      const au0 = p.lu - p.su * 0.5;
      const au1 = p.lu + p.su * 0.5;
      const av0 = p.lv - p.sv * 0.5;
      const av1 = p.lv + p.sv * 0.5;
      const bu0 = q.lu - q.su * 0.5;
      const bu1 = q.lu + q.su * 0.5;
      const bv0 = q.lv - q.sv * 0.5;
      const bv1 = q.lv + q.sv * 0.5;
      if (au0 < bu1 && au1 > bu0 && av0 < bv1 && av1 > bv0) {
        roofViolations += 1;
        break;
      }
    }
  }
  const roofClutter = {
    pieces: roofPlaced.length,
    boxes: roofBoxes,
    acUnits: roofAcs,
    tanks: roofTanks,
    dishes: roofDishes,
    bulkheads: roofBulkheads,
    racks: roofRacks,
    violations: roofViolations,
    // The instance window the rooftop pass owns in the shared detail mesh, so an audit can find
    // exactly those instances in the buffer and nothing else.
    first: roofFirst,
    last: detailCursor
  };

  // ------------------------------------------------------------------ kerbside furniture
  // Bus shelters, litter bins and bollards along the pavements, all inside the one detail
  // instance mesh, all rank 1 or 2 so the mobile profile drops only the smallest pieces.
  {
    const C_SHELTER = new THREE.Color(0x39414a);
    const C_SHELTER_POST = new THREE.Color(0x24292f);
    const C_BENCH = new THREE.Color(0x7a5a3c);
    const C_KIOSK = new THREE.Color(0x3b5f45);
    const C_SHELTER_GLASS = 0xd4e4e6;

    // One bus shelter: a flat roof on two posts, a glass back and end, and a bench. Placed on
    // the avenue pavements where nothing already stands, so the boulevard gets street furniture
    // rather than a forest of poles.
    function addShelter(x, z, rotY) {
      if (!diskFree(x, z, 1.9, B_BUILD | B_PROP)) return false;
      const g = groundHeightAt(x, z);
      const fx = Math.cos(rotY);
      const fz = -Math.sin(rotY);
      addDetail(x, g + 2.62, z, 4.3, 0.18, 1.5, rotY, C_SHELTER, -1, 0, 1);
      for (let e = -1; e <= 1; e += 2) {
        addDetail(x + fx * e * 1.95, g + 1.32, z + fz * e * 1.95, 0.13, 2.64, 0.13,
          rotY, C_SHELTER_POST, -1, 0, 1);
      }
      addDetail(x - fz * 0.66, g + 0.46, z + fx * 0.66, 2.5, 0.12, 0.46, rotY, C_BENCH, -1, 0, 2);
      addDetail(x - fz * 0.66, g + 0.22, z + fx * 0.66, 2.2, 0.44, 0.18, rotY, C_BENCH, -1, 0, 2);
      addShopGlass(x - fz * 0.72, g + 1.35, z + fx * 0.72, rotY, 4.0, 2.2, -1, C_SHELTER_GLASS);
      addShopGlass(x + fx * 1.95, g + 1.35, z + fz * 1.95, rotY + Math.PI * 0.5, 1.2, 2.2, -1,
        C_SHELTER_GLASS);
      markDisk(x, z, 2.1, B_PROP);
      return true;
    }

    let shelters = 0;
    for (let i = 20; i < avePath.n - 20 && shelters < 3; i += 26) {
      const side = shelters % 2 === 0 ? 1 : -1;
      const off = side * (AVE_PAVE_OFF + 0.35);
      const x = avePath.xs[i] + avePath.nx[i] * off;
      const z = avePath.zs[i] + avePath.nz[i] * off;
      if (inRoundabout(x, z, ROUND_R + 4) || nearCrossing(x, z, 3.5)) continue;
      if (flagsAt(x, z) & B_BUILD) continue;
      const tx = avePath.nz[i];
      const tz = -avePath.nx[i];
      if (addShelter(x, z, Math.atan2(-tz, tx))) shelters += 1;
    }

    // Litter bins and bollards used to be placed here, inside the shared details pool; the
    // street pool now owns both roles on the same pavements with the same claim discipline, so
    // the old loops were removed rather than run alongside their replacement.

    // A news kiosk on the market square corner, with its own cream canopy.
    {
      const kx = PLAZA_CX - 13.6;
      const kz = PLAZA_CZ + 7.6;
      if (diskFree(kx, kz, 2.4, B_BUILD | B_PROP)) {
        const g = groundHeightAt(kx, kz) + LIFT_PLAZA;
        addDetail(kx, g + 1.25, kz, 2.3, 2.5, 1.9, 0.1, C_KIOSK, -1, 0, 1);
        addDetail(kx, g + 2.62, kz, 2.9, 0.16, 2.4, 0.1, C_AWNING_CREAM, -1, 0, 1);
        addDetail(kx - 1.05, g + 1.7, kz + 0.05, 0.1, 1.6, 1.5, 0.1, C_SHOP_DOOR, -1, 0, 2);
        markDisk(kx, kz, 2.4, B_PROP);
      }
    }
  }
  if (shopGlass) {
    shopGlass.count = glassCursor;
    if (shopGlass.instanceColor) shopGlass.instanceColor.needsUpdate = true;
    shopGlass.instanceMatrix.needsUpdate = true;
  }

  // ------------------------------------------------------------------ street detail
  // The street tier is the close view, and cycle 19 measured what a quarter of its frame lands
  // on: blank wall and empty pavement, the worst corner flat through 62 percent of its 9x9
  // windows with edge density 4.29. What fills that band is not another thousand windows, it is
  // the litter a real pavement carries: bins and bollards at the kerb, benches and planters
  // under the trees, A-boards against the shopfronts, utility boxes and wall lamps on the
  // facades. They are one instanced unit box, exactly like the rest of the city's small pieces,
  // so a whole category costs instances and never a draw call. The avenue's old details-pool
  // bins and bollards moved here wholesale: one system per role instead of two overlapping ones.
  //
  // It is a pool of its own, not the shared details pool, for a measured reason: the details
  // budget is already spent past its cap (the rank 2 cursor finished at 6653 of 6653 with rank 2
  // truncated), so rank 2 props would either be invisible or would push out the roof clutter
  // that is already drawn. A separate pool costs one draw call and lets the phone drop the whole
  // set, which is how it stays inside its triangle budget; the phone does not draw it at all.
  // Level 2 draws everything, level 1 keeps the avenue prefix (the line the tier's four
  // rotations all frame), level 0 draws none, same prefix rule as the details pool.
  const streetAudit = { pieces: 0, boxes: 0, tried: 0, byKind: {} };
  // The cap: 140 boxes at ten triangles each is 1,400 triangles, and the heaviest street roll
  // the sim produced set the size. With the full 280-box draft the r1 street frame measured
  // 391,220 triangles at 107 pedestrians, over the 390,000 working limit; 140 boxes returns
  // about 1,400 of those and keeps the same frame near 389,8xx. The measured mix runs 140
  // boxes, so the cap is a ceiling, not the run rate. Level 1 draws the avenue prefix plus the
  // facade line, so the night frame carries the strongest line only.
  const STREET_CAP = isTouch ? 0 : 140;
  let streetLevel1 = 0;
  const streetMesh = isTouch ? null : (function () {
    const mesh = new THREE.InstancedMesh(unitBox, detailMat, STREET_CAP);
    mesh.name = 'street';
    mesh.castShadow = false;
    mesh.receiveShadow = shadowsOn;
    mesh.count = 0;
    return mesh;
  })();
  if (streetMesh) {
    const rnd = mulberry(4451);
    const dummy = new THREE.Object3D();
    let sCount = 0;
    function addPiece(x, y, z, sx, sy, sz, rot, col) {
      if (sCount >= STREET_CAP) return false;
      dummy.position.set(x, y, z);
      dummy.rotation.set(0, rot, 0);
      dummy.scale.set(sx, sy, sz);
      dummy.updateMatrix();
      streetMesh.setMatrixAt(sCount, dummy.matrix);
      streetMesh.setColorAt(sCount, col);
      sCount += 1;
      return true;
    }
    // Bins, bollards, benches, planters and shrubs, A-boards, utility boxes: every piece one or
    // two boxes, nothing more. The bench's steel legs and the crates and scooters of the first
    // draft were cut for budget: at twenty to eighty metres the legs are sub-pixel and the
    // small floor clutter does not register, while their boxes cost the same as a planter.
    function propBin(x, z, rot, g) {
      const w = 0.5 + rnd() * 0.1;
      return addPiece(x, g + 0.42, z, w, 0.84, w, rot, C_BIN) &&
        addPiece(x, g + 0.88, z, w + 0.06, 0.09, w + 0.06, rot, C_BIN_LID);
    }
    function propBollard(x, z, rot, g) {
      return addPiece(x, g + 0.45, z, 0.16, 0.9, 0.16, rot, C_BOLLARD) &&
        addPiece(x, g + 0.94, z, 0.2, 0.09, 0.2, rot, C_BOLLARD_BAND);
    }
    function propBench(x, z, rot, g) {
      const tz = Math.sin(rot);
      return addPiece(x, g + 0.42, z, 1.8, 0.12, 0.52, rot, C_BENCH_WOOD) &&
        addPiece(x, g + 0.62, z - tz * 0.2, 1.8, 0.42, 0.09, rot, C_BENCH_WOOD);
    }
    function propPlanter(x, z, rot, g) {
      return addPiece(x, g + 0.28, z, 1.1, 0.56, 1.1, rot, C_PLANTER) &&
        addPiece(x, g + 0.74, z, 0.96, 0.4, 0.96, rot, C_SHRUB);
    }
    function propABoard(x, z, rot, g) {
      return addPiece(x, g + 0.62, z, 0.72, 1.24, 0.06, rot, C_ABOARD) &&
        addPiece(x, g + 0.04, z, 0.72, 0.08, 0.5, rot, C_ABOARD);
    }
    function propUtility(x, z, rot, g) {
      return addPiece(x, g + 0.85, z, 0.62, 1.06, 0.3, rot, C_UTIL);
    }
    const KINDS = [
      [propBin, 'bin'], [propBollard, 'bollard'], [propBench, 'bench'],
      [propPlanter, 'planter'], [propABoard, 'aboard'], [propUtility, 'utility']
    ];
    // One piece at one kerb spot. The spot is rejected when it is not on open ground the
    // pavement pass already marked (the same flagsAt contract the kerbside furniture uses), and
    // a placed piece claims its disk so the next candidate cannot overlap it.
    function placePiece(kindIdx, x, z, rot) {
      streetAudit.tried += 1;
      const kind = KINDS[kindIdx];
      // Every kind is at most two boxes, so this reserve guarantees a piece never truncates
      // halfway into its boxes.
      if (sCount + 2 > STREET_CAP) return false;
      if (!diskFree(x, z, 0.62, B_BUILD | B_PROP | B_TRAM)) return false;
      const g = groundHeightAt(x, z);
      if (!kind[0](x, z, rot, g)) return false;
      markDisk(x, z, 0.62, B_PROP);
      streetAudit.pieces += 1;
      streetAudit.byKind[kind[1]] = (streetAudit.byKind[kind[1]] || 0) + 1;
      return true;
    }
    // The avenue first, both pavements, in the band every rotation frames; then the plaza
    // fringe; then the grid pavements. Within each band the hash picks the kind, so the run is
    // deterministic and the mix is the kind of street litter a real pavement carries.
    const AVE_EDGE = AVE_HALF + 0.9;
    for (let i = 2; i < avePath.n - 2; i += 4) {
      for (let side = -1; side <= 1; side += 2) {
        if (hash2(i, side) < 0.55) continue;           // gaps read as doorways, not clutter
        const off = AVE_EDGE + hash2(i, side + 9) * 0.7;
        const x = avePath.xs[i] + avePath.nx[i] * off * side;
        const z = avePath.zs[i] + avePath.nz[i] * off * side;
        const k = Math.floor(hash2(i, side + 3) * KINDS.length) % KINDS.length;
        const rot = Math.atan2(avePath.nx[i], avePath.nz[i]) + (hash2(i, side + 5) - 0.5) * 0.5;
        if (!diskFree(x, z, 0.62, B_BUILD | B_PROP | B_TRAM)) continue;
        placePiece(k, x, z, rot);
      }
    }
    const PLAZA_EX = PLAZA_W * 0.5 + 0.8;             // plaza fringe, rectangle half extents
    const PLAZA_EZ = PLAZA_D * 0.5 + 0.8;
    for (let i = 0; i < 16; i += 1) {
      const side = i % 4;
      const t = (Math.floor(i / 4) + 0.5) / 4 * 2 - 1;
      const x = PLAZA_CX + (side === 0 ? t * PLAZA_EX : side === 2 ? -t * PLAZA_EX : side * PLAZA_EX);
      const z = PLAZA_CZ + (side === 1 ? t * PLAZA_EZ : side === 3 ? -t * PLAZA_EZ : side * PLAZA_EZ);
      if (side % 2 === 0) {
        placePiece(Math.floor(hash2(i, 1) * 3) % 3, x, z, side === 0 ? Math.PI : 0);
      } else {
        placePiece(Math.floor(hash2(i, 1) * 3) % 3, x, z, side === 1 ? -Math.PI / 2 : Math.PI / 2);
      }
    }
    // Wall lamps and utility boxes ride the facade line of the avenue blocks: same hash, half a
    // metre off the wall, at the height a real service box sits. They run before the grid so
    // the band every rotation frames keeps its wall detail even when the cap truncates deep
    // grid placements.
    for (let i = 3; i < avePath.n - 3; i += 7) {
      for (let side = -1; side <= 1; side += 2) {
        if (hash2(i * 3, side + 2) < 0.6) continue;
        const off = AVE_HALF + 0.35;
        const x = avePath.xs[i] + avePath.nx[i] * off * side;
        const z = avePath.zs[i] + avePath.nz[i] * off * side;
        if (!diskFree(x, z, 0.4, B_BUILD | B_PROP)) continue;
        const g = groundHeightAt(x, z);
        if (hash2(i, side) < 0.5 ? addPiece(x, g + 2.72, z, 0.5, 0.16, 0.16, 0, C_LAMP_METAL)
          : addPiece(x, g + 0.85, z, 0.62, 1.06, 0.3, 0, C_UTIL)) {
          markDisk(x, z, 0.4, B_PROP);
          streetAudit.pieces += 1;
        }
      }
    }
    streetLevel1 = sCount;                          // the prefix level 1 keeps at night
    // The grid pavements alternate vertical and horizontal streets at every step, so a cap
    // truncation thins both directions together instead of starving the second loop to zero.
    for (let t = -70; t <= 88; t += 5) {
      for (let v = 0; v < V_LINES.length; v += 1) {
        const cx = V_LINES[v];
        if (Math.abs(cx) > 78 || t < -66) continue;
        for (let side = -1; side <= 1; side += 2) {
          if (hash2(v * 7 + 13, t) < 0.62) continue;
          const x = cx + side * (STREET_HALF + 0.55);
          const z = t + (hash2(v, t + 3) - 0.5) * 2;
          const k = Math.floor(hash2(v * 3, t + side) * KINDS.length) % KINDS.length;
          const rot = hash2(v, t + side) < 0.5 ? 0 : Math.PI / 2;
          if (!diskFree(x, z, 0.62, B_BUILD | B_PROP | B_TRAM)) continue;
          placePiece(k, x, z, rot);
        }
      }
      for (let h = 0; h < H_LINES.length; h += 1) {
        const cz = H_LINES[h];
        if (cz < -70 || cz > 92 || t > 70) continue;
        for (let side = -1; side <= 1; side += 2) {
          if (hash2(h * 11 + 5, t) < 0.62) continue;
          const z = cz + side * (STREET_HALF + 0.55);
          const x = t + (hash2(h, t + 3) - 0.5) * 2;
          const k = Math.floor(hash2(h * 5, t + side) * KINDS.length) % KINDS.length;
          const rot = hash2(h, t + side) < 0.5 ? 0 : Math.PI / 2;
          if (!diskFree(x, z, 0.62, B_BUILD | B_PROP | B_TRAM)) continue;
          placePiece(k, x, z, rot);
        }
      }
    }
    streetMesh.count = sCount;
    streetAudit.cap = STREET_CAP;
    streetAudit.boxes = sCount;
    if (streetMesh.instanceColor) streetMesh.instanceColor.needsUpdate = true;
    streetMesh.instanceMatrix.needsUpdate = true;
    group.add(streetMesh);
  }

  // ------------------------------------------------------------------ market square props
  // A working market, not a cluster of tents: fourteen stalls in two loose rows with a clear
  // central walk to the fountain. Every stall is a stripped counter with a red and white cloth
  // front, four posts, a canvas back panel, a white and red striped canopy in six strips with
  // its valance, and three produce trays in saturated colours. The ground crates, the hand
  // carts and the two name boards stand in the open aisles and at the four corners, where a
  // camera forty degrees up sees the goods past the canvas, and four cafe tables with umbrellas
  // edge the two entrances.
  //
  // Placement is the plaza's own occupancy rule, unchanged. A stall is a disk of STALL_R that
  // must clear the block bits B_BUILD, B_PROP, B_TRAM, B_AVE, B_SIDEWALK, B_STREET, B_RING,
  // B_ROUND, B_SPUR, B_VIA and B_CORR (so the tram bed, the avenue carriageway and the pavement
  // walk lines, the ring and the roundabout are all in that mask), must keep half a metre from
  // an avenue zebra, and must stand outside the fountain's four metre ground disk. Every
  // accepted piece claims B_PROP through markDisk, so a later candidate sees the earlier ones,
  // exactly as the kiosk and the bus shelters already do. The audit below reports the counts.
  const marketAudit = {
    stalls: 0,
    stallTries: 0,
    rejected: 0,
    stallRadius: 1.3,
    fountainClear: 4,
    nearestFountain: 0,
    nearestStall: 0,
    crates: 0,
    trolleys: 0,
    boards: 0,
    parasols: 0,
    furnitureTries: 0,
    furnitureRejected: 0
  };
  const marketBoards = [];
  {
    const canopyRed = new THREE.Color(0xc0392b);
    const canopyWhite = new THREE.Color(0xf7f3e8);
    const counterWood = new THREE.Color(0xa77b52);
    const counterTop = new THREE.Color(0xe4d7bd);
    const postCol = new THREE.Color(0x6b5440);
    const backCanvas = new THREE.Color(0xefe6cf);
    const crateWood = new THREE.Color(0xb08a5a);
    const cartWood = new THREE.Color(0xa87f52);
    const cartSteel = new THREE.Color(0x6b7278);
    const boardWood = new THREE.Color(0x7a5a3c);
    const tableCol = new THREE.Color(0xe8e0cd);
    const benchCol = new THREE.Color(0x7a5a3c);
    // Produce in four saturated families: tomatoes, citrus, greens, aubergine. A stall takes
    // three of them and its neighbour starts one step further round, so no two boxes on a row
    // land on the same colour side by side.
    const produce = [
      new THREE.Color(0xd8392a),
      new THREE.Color(0xf0a11c),
      new THREE.Color(0x4f9a34),
      new THREE.Color(0x5b3a70)
    ];
    const STALL_R = marketAudit.stallRadius;
    const FOUNTAIN_CLEAR = marketAudit.fountainClear;
    const MARKET_MASK = B_BUILD | B_PROP | B_TRAM | B_AVE | B_SIDEWALK | B_STREET | B_RING |
      B_ROUND | B_SPUR | B_VIA | B_CORR;
    function marketFree(x, z, r) {
      if (nearCrossing(x, z, r + 0.5)) return false;
      const lim = r + FOUNTAIN_CLEAR;
      const dx = x - PLAZA_CX;
      const dz = z - PLAZA_CZ;
      if (dx * dx + dz * dz < lim * lim) return false;
      return diskFree(x, z, r, MARKET_MASK);
    }
    // The fountain claims its ground before the first stall instead of after the last: the
    // stalls now read it as an occupied disk like any other prop on the square.
    markDisk(PLAZA_CX, PLAZA_CZ, FOUNTAIN_CLEAR, B_PROP);

    function addStall(x, z, face, goods) {
      const y = groundHeightAt(x, z) + LIFT_PLAZA;
      addDetail(x, y + 0.47, z, 2.2, 0.94, 1.05, 0, counterWood, -1, 0, 0);
      addDetail(x, y + 0.97, z, 2.35, 0.09, 1.15, 0, counterTop, -1, 0, 0);
      // The back panel closes the stall, so a row seen along its length reads as stalls.
      addDetail(x, y + 1.3, z - face * 0.5, 2.2, 1.55, 0.07, 0, backCanvas, -1, 0, 0);
      // The cloth front, five strips in the canopy's own red and white.
      for (let s = 0; s < 5; s += 1) {
        addDetail(x - 0.88 + 0.44 * s, y + 0.56, z + face * 0.54, 0.42, 0.7, 0.04, 0,
          (s & 1) ? canopyWhite : canopyRed, -1, 0, 1);
      }
      for (let p = 0; p < 4; p += 1) {
        addDetail(x + ((p & 1) ? 1.04 : -1.04), y + 1.03, z + ((p & 2) ? 0.54 : -0.54),
          0.09, 2.06, 0.09, 0, postCol, -1, 0, 0);
      }
      // Six alternating strips, red leading, tilted down toward the aisle.
      for (let s = 0; s < 6; s += 1) {
        addDetail(x - 1.125 + 0.375 * (s + 0.5), y + 2.14, z, 0.36, 0.08, 1.5, 0,
          (s & 1) ? canopyRed : canopyWhite, -1, face * 0.1, 0);
      }
      // The valance on the leading edge, the counter's stripes carried up onto the canvas.
      for (let s = 0; s < 6; s += 1) {
        addDetail(x - 1.125 + 0.375 * (s + 0.5), y + 2.0, z + face * 0.73, 0.36, 0.24, 0.05, 0,
          (s & 1) ? canopyWhite : canopyRed, -1, 0, 1);
      }
      // Three goods trays on the counter.
      for (let g = 0; g < 3; g += 1) {
        addDetail(x - 0.72 + g * 0.72, y + 1.1, z + face * 0.02, 0.58, 0.2, 0.5, 0,
          produce[(goods + g) % produce.length], -1, 0, 1);
      }
      markDisk(x, z, STALL_R, B_PROP);
    }

    // Two loose rows of seven at a three metre pitch, eleven and a half metres apart, with a
    // fifteen centimetre alternating offset so the canopies do not stand on one line. The
    // rows stop two metres short of the plaza edges and the central walk runs the full width.
    const rowX = [-9, -6, -3, 0, 3, 6, 9];
    const rowJit = [-0.12, 0.15, -0.06, 0.1, -0.15, 0.05, 0.13];
    const stallX = [];
    const stallZ = [];
    for (let row = 0; row < 2; row += 1) {
      const face = row === 0 ? 1 : -1;
      for (let k = 0; k < rowX.length; k += 1) {
        const x = PLAZA_CX + rowX[k];
        const z = PLAZA_CZ + face * -5.7 + rowJit[k];
        marketAudit.stallTries += 1;
        if (!marketFree(x, z, STALL_R)) {
          marketAudit.rejected += 1;
          continue;
        }
        addStall(x, z, face, (k + row * 2) % produce.length);
        marketAudit.stalls += 1;
        stallX.push(x);
        stallZ.push(z);
      }
    }
    // The two clearances the test enforces, measured back from the placed stalls: the nearest
    // stall to the fountain and the closest pair of stall centres.
    for (let i = 0; i < stallX.length; i += 1) {
      const dfx = stallX[i] - PLAZA_CX;
      const dfz = stallZ[i] - PLAZA_CZ;
      const df = Math.sqrt(dfx * dfx + dfz * dfz);
      if (marketAudit.nearestFountain === 0 || df < marketAudit.nearestFountain) {
        marketAudit.nearestFountain = df;
      }
      for (let j = i + 1; j < stallX.length; j += 1) {
        const dx = stallX[i] - stallX[j];
        const dz = stallZ[i] - stallZ[j];
        const d = Math.sqrt(dx * dx + dz * dz);
        if (marketAudit.nearestStall === 0 || d < marketAudit.nearestStall) {
          marketAudit.nearestStall = d;
        }
      }
    }

    // Ground crates at the four corners, in pairs: goods spilling out of the rows, in the open
    // where the district camera reads their colour between the canopies.
    const crateSpots = [
      [26.9, 4.2, 0], [27.9, 4.2, 1], [44.1, 4.2, 2], [45.1, 4.2, 3],
      [26.9, 19.7, 1], [27.9, 19.7, 2], [44.1, 19.7, 3], [45.1, 19.7, 0]
    ];
    for (let i = 0; i < crateSpots.length; i += 1) {
      const x = crateSpots[i][0];
      const z = crateSpots[i][1];
      marketAudit.furnitureTries += 1;
      if (!marketFree(x, z, 0.45)) {
        marketAudit.furnitureRejected += 1;
        continue;
      }
      const y = groundHeightAt(x, z) + LIFT_PLAZA;
      addDetail(x, y + 0.2, z, 0.68, 0.4, 0.54, 0.08 * i, crateWood, -1, 0, 1);
      addDetail(x, y + 0.47, z, 0.62, 0.16, 0.48, 0.08 * i, produce[crateSpots[i][2]], -1, 0, 1);
      markDisk(x, z, 0.45, B_PROP);
      marketAudit.crates += 1;
    }

    // Three hand carts: a plank bed, two steel wheels on an axle, two grips leaning back. Five
    // boxes each, in the same detail pool, so the phone keeps them at detail level 1.
    function addCart(x, z, yaw) {
      const y = groundHeightAt(x, z) + LIFT_PLAZA;
      const c = Math.cos(yaw);
      const s = Math.sin(yaw);
      addDetail(x, y + 0.62, z, 1.24, 0.1, 0.68, yaw, cartWood, -1, 0, 1);
      for (let e = -1; e <= 1; e += 2) {
        addDetail(x + e * 0.36 * s, y + 0.3, z + e * 0.36 * c, 0.44, 0.44, 0.09, yaw,
          cartSteel, -1, 0, 1);
        addDetail(x - 0.66 * c + e * 0.22 * s, y + 0.92, z + 0.66 * s + e * 0.22 * c,
          0.06, 0.72, 0.06, yaw - Math.PI * 0.5, cartSteel, -1, 0.6, 1);
      }
      markDisk(x, z, 1, B_PROP);
    }
    const cartSpots = [[26.9, 10.9, 0.32], [45.3, 13.6, 2.4], [40.4, 19.6, 1.1]];
    for (let i = 0; i < cartSpots.length; i += 1) {
      marketAudit.furnitureTries += 1;
      if (!marketFree(cartSpots[i][0], cartSpots[i][1], 1)) {
        marketAudit.furnitureRejected += 1;
        continue;
      }
      addCart(cartSpots[i][0], cartSpots[i][1], cartSpots[i][2]);
      marketAudit.trolleys += 1;
    }

    // Two name boards on the diagonal corners, lettered from the shared sign atlas: one cell
    // each, painted once at build time, no extra mesh and no extra call.
    const boardSpots = [
      [25, 4.4, Math.PI * 0.25, 'Mercato'],
      [47, 19.6, Math.PI * 1.25, 'Piazza del Mercato']
    ];
    for (let i = 0; i < boardSpots.length; i += 1) {
      const bx = boardSpots[i][0];
      const bz = boardSpots[i][1];
      const ry = boardSpots[i][2];
      marketAudit.furnitureTries += 1;
      if (!marketFree(bx, bz, 1.25)) {
        marketAudit.furnitureRejected += 1;
        continue;
      }
      const gy = groundHeightAt(bx, bz) + LIFT_PLAZA;
      const tx = Math.cos(ry);
      const tz = -Math.sin(ry);
      for (let e = -1; e <= 1; e += 2) {
        addDetail(bx + tx * e * 0.95, gy + 1.08, bz + tz * e * 0.95, 0.1, 2.16, 0.1, ry,
          boardWood, -1, 0, 1);
      }
      marketBoards.push({
        x: bx, y: gy + 1.72, z: bz, rotY: ry, w: 2.2, h: 0.72, name: boardSpots[i][3]
      });
      markDisk(bx, bz, 1.25, B_PROP);
      marketAudit.boards += 1;
    }

    // Cafe tables on the two entrance flanks, clear of the central walk, four umbrellas over
    // them in the parasol pass below.
    const terraces = [[-10.4, -3], [-10.4, 3], [10.4, -3], [10.4, 3]];
    for (let i = 0; i < terraces.length; i += 1) {
      const x = PLAZA_CX + terraces[i][0];
      const z = PLAZA_CZ + terraces[i][1];
      const y = groundHeightAt(x, z) + LIFT_PLAZA;
      addDetail(x, y + 0.74, z, 0.95, 0.08, 0.95, 0, tableCol, -1, 0, 1);
      addDetail(x, y + 0.37, z, 0.12, 0.74, 0.12, 0, postCol, -1, 0, 1);
    }
    // Benches on the diagonals of the fountain, leaving the west and east walks to the square
    // open from the pavement to the water.
    const angles = [0.62, -0.62, Math.PI - 0.62, Math.PI + 0.62];
    for (let i = 0; i < angles.length; i += 1) {
      const a = angles[i];
      const x = PLAZA_CX + Math.cos(a) * 6.2;
      const z = PLAZA_CZ + Math.sin(a) * 6.2;
      const y = groundHeightAt(x, z) + LIFT_PLAZA;
      const rot = Math.atan2(-Math.cos(a), -Math.sin(a)) + Math.PI / 2;
      addDetail(x, y + 0.45, z, 1.9, 0.12, 0.55, rot, benchCol, -1, 0, 1);
      addDetail(x + Math.cos(a) * 0.28, y + 0.75, z + Math.sin(a) * 0.28, 1.9, 0.5, 0.1,
        rot, benchCol, -1, 0, 1);
    }
    marketAudit.nearestFountain = Math.round(marketAudit.nearestFountain * 100) / 100;
    marketAudit.nearestStall = Math.round(marketAudit.nearestStall * 100) / 100;
  }

  // ------------------------------------------------------------------ rail rolling stock
  // A regional train waiting on the outer track: a power car and three coaches in white livery
  // with a red waistband, plus a rake of container wagons on the inner road.
  {
    const livery = new THREE.Color(0xe9e6df);
    const stripe = new THREE.Color(0xbc3a2c);
    const glassBand = new THREE.Color(0x2b3540);
    const roof = new THREE.Color(0xa9a9a6);
    const chassis = new THREE.Color(0x2e2b29);
    const containerA = new THREE.Color(0x3f6f9e);
    const containerB = new THREE.Color(0xa4472f);
    const containerC = new THREE.Color(0x4f8a5c);

    function railHeight(off, z, half) {
      const x = RAIL_X + off;
      const gB = groundHeightAt(x, z - half);
      const gF = groundHeightAt(x, z + half);
      return { y: (gB + gF) * 0.5 + LIFT_RAIL, pitch: Math.atan2(gB - gF, half * 2) };
    }

    // The train on the outer road: one power car, three coaches.
    for (let c = 0; c < 4; c += 1) {
      const z = -6 + c * 24;
      const r = railHeight(TRACK_OFFS[3], z, 11);
      const isLoco = c === 0;
      addDetail(RAIL_X + TRACK_OFFS[3], r.y + 0.5, z, 2.5, 0.6, 21.5, 0, chassis, -1, r.pitch, 0);
      addDetail(RAIL_X + TRACK_OFFS[3], r.y + 2.0, z, 2.9, 2.7, 21.5, 0, livery, -1, r.pitch, 0);
      addDetail(RAIL_X + TRACK_OFFS[3], r.y + 2.4, z, 2.94, 0.72, 18.4, 0, glassBand, -1, r.pitch, 0);
      addDetail(RAIL_X + TRACK_OFFS[3], r.y + 1.15, z, 2.96, 0.34, 20.2, 0, stripe, -1, r.pitch, 0);
      addDetail(RAIL_X + TRACK_OFFS[3], r.y + 3.45, z, 2.7, 0.4, 21.2, 0, roof, -1, r.pitch, 0);
      if (isLoco) {
        addDetail(RAIL_X + TRACK_OFFS[3] - 1.5, r.y + 3.9, z - 3, 0.24, 1.1, 1.2, 0,
          chassis, -1, r.pitch, 0);
      }
    }

    // Container wagons on the middle road.
    const plan = [
      [TRACK_OFFS[1], 46, containerA], [TRACK_OFFS[1], 58, containerB],
      [TRACK_OFFS[1], 70, containerC], [TRACK_OFFS[2], -60, containerB],
      [TRACK_OFFS[2], -48, containerA], [TRACK_OFFS[0], 12, containerC],
      [TRACK_OFFS[0], 24, containerB]
    ];
    for (let i = 0; i < plan.length; i += 1) {
      const off = plan[i][0];
      const z = plan[i][1];
      const r = railHeight(off, z, 6.4);
      addDetail(RAIL_X + off, r.y + 0.55, z, 2.2, 0.7, 12.6, 0, chassis, -1, r.pitch, 0);
      addDetail(RAIL_X + off, r.y + 0.95, z, 2.9, 0.3, 11.8, 0, chassis, -1, r.pitch, 0);
      addDetail(RAIL_X + off, r.y + 2.5, z, 2.85, 2.7, 11.6, 0, plan[i][2], -1, r.pitch, 0);
    }
  }

  // ------------------------------------------------------------------ crown gardens
  // A clipped hedge and a low wall behind the crown houses that have open ground behind them.
  // The village is packed, so most terraces back onto another lane and get no garden at all;
  // only the two outermost rows, at the crest and at the church end of the village, have room.
  {
    const hedge = new THREE.Color(0x54763c);
    const wall = new THREE.Color(0xa89d86);
    const rnd = mulberry(31337);
    for (let i = 0; i < houseSpots.length; i += 1) {
      if (rnd() > 0.62) continue;
      const h = houseSpots[i];
      if (h.yaw !== 0) continue;
      const cs = Math.cos(h.yaw);
      const sn = Math.sin(h.yaw);
      // Behind the house first, then in front: the first side that clears every alley by more
      // than the hedge can reach gets the garden. A packed terrace fails both tests.
      let gx = 0;
      let gz = 0;
      let found = false;
      for (let side = -1; side <= 1 && !found; side += 2) {
        const lv = side * (h.d * 0.5 + 0.5);
        const px = h.x + sn * lv;
        const pz = h.z + cs * lv;
        let clear = true;
        for (let l = 0; l < OLD_LANES.length && clear; l += 1) {
          if (Math.abs(pz - OLD_LANES[l]) < OLD_ALLEY + 0.55) clear = false;
        }
        if (clear) {
          gx = px;
          gz = pz;
          found = true;
        }
      }
      if (!found) continue;
      const g = groundHeightAt(gx, gz) + 0.02;
      if (rnd() < 0.55) {
        addDetail(gx, g + 0.55, gz, h.w * 1.02, 1.0, 0.5, h.yaw, hedge, -1, 0, 2);
      } else {
        addDetail(gx, g + 0.34, gz, h.w * 1.05, 0.68, 0.44, h.yaw, wall, -1, 0, 1);
      }
    }
  }

  // ------------------------------------------------------------------ pack street props
  // Street tier only. A handful of real kerbside objects, placed on the occupancy grid like
  // every other prop: courtyard dumpsters behind the blocks, planters on the avenue pavement,
  // and clipped bushes at the median's ornamental stations. Each kind is one InstancedMesh, and
  // the whole pass disappears when the tier is not loaded.
  const propEntries = new Map();
  let propModelUsed = 0;
  function propModelEntry(seed, cap) {
    if (!assets || MODEL_PROP_MAX <= 0) return null;
    const key = String(seed);
    if (propEntries.has(key)) return propEntries.get(key);
    const made = modelMesh('prop', { seed: seed, count: cap, shadows: shadowsOn });
    if (!made) return null;
    made.mesh.count = 0;
    const entry = { mesh: made.mesh, model: made.model, cap: cap, used: 0 };
    propEntries.set(key, entry);
    return entry;
  }
  function placeProp(entry, x, y, z, scale, yaw) {
    if (!entry || entry.used >= entry.cap || propModelUsed >= MODEL_PROP_MAX) return false;
    dummy.position.set(x, y, z);
    dummy.rotation.set(0, yaw, 0);
    dummy.scale.set(scale, scale, scale);
    dummy.updateMatrix();
    entry.mesh.setMatrixAt(entry.used, dummy.matrix);
    entry.used += 1;
    propModelUsed += 1;
    markDisk(x, z, 1.2, B_PROP);
    return true;
  }
  if (assets && MODEL_PROP_MAX > 0) {
    // Courtyard dumpsters: one per block interior where the grid is clear, capped at eight.
    const dumpster = propModelEntry(0, 8);
    if (dumpster) {
      for (let qi = 0; qi < quads.length && dumpster.used < 8; qi += 1) {
        const q = quads[qi];
        for (let k = 0; k < 60 && dumpster.used < 8; k += 1) {
          const a = k * 2.399 + qi * 0.7;
          const rr = 5 + (k / 60) * Math.min(18, q.radius);
          const x = q.center.x + Math.cos(a) * rr;
          const z = q.center.z + Math.sin(a) * rr;
          if (!diskFree(x, z, 2.0, B_SOLID)) continue;
          if (insideBuilding(x, z, 1.4)) continue;
          const g = groundHeightAt(x, z) + (flagsAt(x, z) & B_PAVED ? LIFT_PAVE : LIFT_STREET);
          if (placeProp(dumpster, x, g, z, 1.0, a)) break;
        }
      }
    }
    // Planters on the avenue pavement, alternating sides between the lamps.
    const planter = propModelEntry(5, 10);
    if (planter) {
      for (let i = 9; i < avePath.n - 9 && planter.used < 10; i += 37) {
        const side = (i % 2) ? 1 : -1;
        const off = side * (AVE_HALF + 1.35);
        const x = avePath.xs[i] + avePath.nx[i] * off;
        const z = avePath.zs[i] + avePath.nz[i] * off;
        if (inRoundabout(x, z, ROUND_R + 3)) continue;
        if (flagsAt(x, z) & (B_BUILD | B_PROP)) continue;
        if (!diskFree(x, z, 1.0, B_SOLID)) continue;
        const g = groundHeightAt(x, z) + LIFT_PAVE;
        placeProp(planter, x, g, z, 1.0, avePath.zs[i] * 0.2);
      }
    }
  }

  // ------------------------------------------------------------------ boulevard median
  // Clipped shrubs down the centre of the avenue, between the tram tracks, with an occasional
  // small ornamental tree. Crossings and the roundabout stay clear.
  {
    const shrub = new THREE.Color(0x4b7238);
    const shrubTop = new THREE.Color(0x6f9a4a);
    const trunk = new THREE.Color(0x6b5540);
    for (let i = 8; i < avePath.n - 8; i += 5) {
      const x = avePath.xs[i];
      const z = avePath.zs[i];
      if (inRoundabout(x, z, ROUND_R + 3)) continue;
      if (nearCrossing(x, z, 2.8)) continue;
      const y = groundHeightAt(x, z) + LIFT_MEDIAN;
      const s = 0.86 + hash2(i, 11) * 0.4;
      addDetail(x, y + 0.4, z, s * 1.25, 0.8, s * 1.25, i * 0.7, shrub, -1, 0, 1);
      if ((i % 15) === 8) {
        const bush = propModelEntry(8, 10);
        if (!bush || !placeProp(bush, x, y, z, 1.55, i * 0.7)) {
          addDetail(x, y + 1.5, z, 0.24, 2.6, 0.24, 0, trunk, -1, 0, 1);
          addDetail(x, y + 3.0, z, 2.1, 1.7, 2.1, i * 0.33, shrubTop, -1, 0, 0);
        }
      }
    }
  }

  // Commit the pack props after the median pass, because the bush entry is created there.
  if (assets && MODEL_PROP_MAX > 0) {
    propEntries.forEach(function (entry) {
      if (!entry || entry.used <= 0) return;
      entry.mesh.count = entry.used;
      if (entry.mesh.instanceMatrix) entry.mesh.instanceMatrix.needsUpdate = true;
      group.add(entry.mesh);
    });
  }

  // ------------------------------------------------------------------ parked cars
  // Static, along the kerbs of the secondary streets and the ring, in the occupancy grid so
  // trees, lamps and props keep off them.
  const carGeo = (function () {
    const b = makeBuilder();
    b.addGeometry(unitBox, composeMatrix(0, 0.62, 0, 0, 1.62, 0.74, 4.05), new THREE.Color(0xffffff));
    b.addGeometry(unitBox, composeMatrix(0, 1.21, -0.12, 0, 1.34, 0.52, 2.15), new THREE.Color(0x555c66));
    return b.build();
  })();
  const cars = new THREE.InstancedMesh(
    carGeo,
    new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.36, metalness: 0.16 }),
    CAR_CAP
  );
  cars.castShadow = shadowsOn;
  cars.receiveShadow = shadowsOn;
  cars.name = 'parked';
  cars.frustumCulled = false;
  group.add(cars);
  let carCursor = 0;
  {
    const rnd = mulberry(5150);
    const palette = [0xb8312f, 0x2f5f8a, 0xe8e2d6, 0x3c3c3e, 0xd9a43a, 0x6f8f5a, 0xf1efe9, 0x8a4a5c];
    const col = new THREE.Color();
    const bayMarks = [];
    function parkAt(x, z, heading, lift) {
      if (carCursor >= CAR_CAP) return;
      if (!diskFree(x, z, 1.7, B_PROP | B_BUILD | B_PLAZA | B_TRAM)) return;
      const gB = groundHeightAt(x - Math.sin(heading) * 2, z - Math.cos(heading) * 2);
      const gF = groundHeightAt(x + Math.sin(heading) * 2, z + Math.cos(heading) * 2);
      const g = (gB + gF) * 0.5 + LIFT_STREET;
      dummy.position.set(x, g, z);
      dummy.rotation.set(Math.atan2(gB - gF, 4) * -1, heading, 0);
      const s = 0.94 + rnd() * 0.12;
      dummy.scale.set(s, s, s);
      dummy.updateMatrix();
      cars.setMatrixAt(carCursor, dummy.matrix);
      col.setHex(palette[Math.floor(rnd() * palette.length)]).multiplyScalar(0.92 + rnd() * 0.14);
      cars.setColorAt(carCursor, col);
      markDisk(x, z, 1.9, B_PROP);
      // Bay markings: a thin white tick at each end of the bay, across the parking lane.
      bayMarks.push(x, z, heading, lift);
      carCursor += 1;
    }
    // Kerbside parking down the secondary streets.
    for (let s = 0; s < 2; s += 1) {
      const runs = s === 0 ? streetRunsV : streetRunsH;
      const vertical = s === 0;
      for (let i = 0; i < runs.length; i += 1) {
        const r = runs[i];
        for (let t = r.t0 + 2; t < r.t1 - 2; t += 7.4) {
          if (rnd() > 0.42) continue;
          const side = rnd() < 0.5 ? -1 : 1;
          const off = side * (STREET_HALF - 0.95);
          const x = vertical ? r.coord + off : t;
          const z = vertical ? t : r.coord + off;
          parkAt(x, z, vertical ? 0 : Math.PI / 2, LIFT_STREET + 0.055);
        }
      }
    }
    // Kerbside parking on the ring road.
    for (let i = 6; i < ringPath.n; i += 9) {
      if (rnd() > 0.5) continue;
      const side = rnd() < 0.5 ? -1 : 1;
      const off = side * (RING_HALF - 1.0);
      const x = ringPath.xs[i] + ringPath.nx[i] * off;
      const z = ringPath.zs[i] + ringPath.nz[i] * off;
      if (inRoundabout(x, z, ROUND_R + 2)) continue;
      parkAt(x, z, Math.atan2(ringPath.nz[i], -ringPath.nx[i]), LIFT_PAINT);
    }
    // One tick at each end of every occupied bay.
    for (let i = 0; i < bayMarks.length; i += 4) {
      const x = bayMarks[i];
      const z = bayMarks[i + 1];
      const heading = bayMarks[i + 2];
      const lift = bayMarks[i + 3];
      const fx = Math.sin(heading);
      const fz = Math.cos(heading);
      const ax = Math.cos(heading);
      const az = -Math.sin(heading);
      for (let e = -1; e <= 1; e += 2) {
        const cx = x + fx * e * 2.12;
        const cz = z + fz * e * 2.12;
        paint.flatQuad(
          cx - fx * 0.055 - ax * 1.24, cz - fz * 0.055 - az * 1.24,
          cx + fx * 0.055 - ax * 1.24, cz + fz * 0.055 - az * 1.24,
          cx + fx * 0.055 + ax * 1.24, cz + fz * 0.055 + az * 1.24,
          cx - fx * 0.055 + ax * 1.24, cz - fz * 0.055 + az * 1.24,
          lift, C_PAINT
        );
      }
    }
  }
  cars.count = carCursor;
  if (cars.instanceColor) cars.instanceColor.needsUpdate = true;
  cars.instanceMatrix.needsUpdate = true;

  // ------------------------------------------------------------------ road paint mesh
  // One merged mesh for every painted line: both zebra crossings, centre dashes, edge lines,
  // lane arrows and parking bays. It sits 5.8 cm above the asphalt, so no painted quad is ever
  // coplanar with the road, and the emissive lift carries the white through the deep shadow of a
  // street canyon, which is where the boulevard's own paint lives: measured, an asphalt-shadowed
  // zebra only reads as paint at this lift, and the sunlit paint of the outer roads stays white
  // because the lift is a share of the same white albedo. Night drops it to a trace, since the
  // lamps already light the road and paint must not glow on its own.
  const PAINT_DAY_GLOW = 0.62;
  const PAINT_NIGHT_GLOW = 0.05;
  const paintMat = new THREE.MeshStandardMaterial({
    vertexColors: true,
    roughness: 0.7,
    metalness: 0.02,
    emissive: 0xffffff,
    emissiveIntensity: PAINT_DAY_GLOW
  });
  const paintMesh = new THREE.Mesh(paint.build(), paintMat);
  paintMesh.receiveShadow = shadowsOn;
  paintMesh.name = 'paint';
  group.add(paintMesh);

  // ------------------------------------------------------------------ parasols
  // One instanced pole plus cone: the cafe terraces of the market square, and a few on the
  // avenue terraces. At 412 px the canopy is about two device pixels across, so the touch
  // profile does not build it at all: two calls of headroom is exactly what the permanent
  // road needs to stay under the ceiling, and this is the second of them.
  if (!NARROW_VIEW) {
    const spots = [
      [18.6, 10.4], [18.6, 13.4], [18.6, 16.6], [21.4, 9.2], [21.4, 18.8],
      [23.6, 10.6], [23.6, 17.4], [19.8, 12.0], [20.6, 15.6], [17.4, 19.0], [22.2, 12.4],
      [25.6, 9.0], [25.6, 15.0], [46.4, 9.0], [46.4, 15.0]
    ];
    // Street tier carries real cafe umbrellas with their own table and base. Two models cover
    // the fifteen terraces; when the tier is not loaded the procedural pole and cone stand
    // exactly as before.
    const parasolModels = assets ? (assets.byCategory('parasol') || []) : [];
    let placed = 0;
    if (parasolModels.length > 0 && MODEL_PARASOL_MAX > 0) {
      const cap = Math.min(MODEL_PARASOL_MAX, spots.length);
      const entries = [];
      for (let i = 0; i < parasolModels.length && entries.length < 2; i += 1) {
        const made = modelMesh('parasol', {
          model: parasolModels[i], count: cap, shadows: shadowsOn
        });
        if (!made) continue;
        made.mesh.count = 0;
        entries.push({ mesh: made.mesh, model: parasolModels[i], used: 0 });
      }
      if (entries.length > 0) {
        for (let i = 0; i < spots.length && placed < cap; i += 1) {
          const entry = entries[i % entries.length];
          const x = spots[i][0];
          const z = spots[i][1];
          const y = groundHeightAt(x, z) + LIFT_PLAZA;
          const scale = 1.0 + hash2(Math.round(x * 3) + 41, Math.round(z * 3) + 43) * 0.16;
          dummy.position.set(x, y, z);
          dummy.rotation.set(0, (i * 0.83) % 3.14, 0);
          dummy.scale.set(scale, scale, scale);
          dummy.updateMatrix();
          entry.mesh.setMatrixAt(entry.used, dummy.matrix);
          entry.used += 1;
          placed += 1;
          markDisk(x, z, 1.1, B_PROP);
        }
        for (let i = 0; i < entries.length; i += 1) {
          const entry = entries[i];
          if (entry.used <= 0) continue;
          entry.mesh.count = entry.used;
          if (entry.mesh.instanceMatrix) entry.mesh.instanceMatrix.needsUpdate = true;
          group.add(entry.mesh);
        }
      }
    }
    if (placed === 0) {
      const parasolGeo = (function () {
        const b = makeBuilder();
        const white = new THREE.Color(0xffffff);
        b.addGeometry(new THREE.CylinderGeometry(0.055, 0.065, 2.3, 6), composeMatrix(0, 1.15, 0, 0, 1, 1, 1), white);
        b.addGeometry(new THREE.CylinderGeometry(0.1, 1.55, 0.6, 8, 1, true), composeMatrix(0, 2.5, 0, 0, 1, 1, 1), white);
        return b.build();
      })();
      const parasols = new THREE.InstancedMesh(
        parasolGeo,
        new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.72, metalness: 0.04 }),
        PARASOL_CAP
      );
      parasols.castShadow = shadowsOn;
      parasols.name = 'parasols';
      parasols.frustumCulled = false;
      group.add(parasols);
      const cream = new THREE.Color(0xf3ead4);
      const red = new THREE.Color(0xc24338);
      const blue = new THREE.Color(0x3c6f9e);
      const ochre = new THREE.Color(0xe0a93a);
      const tones = [cream, red, blue, ochre, cream];
      for (let i = 0; i < spots.length && placed < PARASOL_CAP; i += 1) {
        const x = spots[i][0];
        const z = spots[i][1];
        const y = groundHeightAt(x, z) + LIFT_PLAZA;
        dummy.position.set(x, y, z);
        dummy.rotation.set(0, (i * 0.7) % 3.14, 0);
        dummy.scale.set(1, 1, 1);
        dummy.updateMatrix();
        parasols.setMatrixAt(placed, dummy.matrix);
        parasols.setColorAt(placed, tones[i % tones.length]);
        markDisk(x, z, 1.1, B_PROP);
        placed += 1;
      }
      parasols.count = placed;
      if (parasols.instanceColor) parasols.instanceColor.needsUpdate = true;
      parasols.instanceMatrix.needsUpdate = true;
    }
    // The last four umbrellas stand over the market's cafe tables; the rest are the avenue
    // terraces. The touch profile builds no parasols at all, so its audit reports zero.
    marketAudit.parasols = Math.max(0, placed - 11);
  }

  // ------------------------------------------------------------------ trees
  // One instanced mesh, three silhouettes from per-instance scale: round plane trees in rows,
  // low wide silver olives in groves, tall dark cypresses near the hilltop.
  //
  // Twenty four triangles a tree: a four sided open trunk (eight) under a crown of two eight
  // sided cones joined at their rims (sixteen). The old icosahedron crown cost twenty and the
  // five sided trunk ten, so the same money buys a fifth more canopy on the hillside, and the
  // two cones are a better cypress than a round blob: squashed they are an olive clump, stretched
  // they are the one pointed silhouette the near slope reads against the haze.
  const treeGeo = (function () {
    const b = makeBuilder();
    const trunk = new THREE.CylinderGeometry(0.2, 0.3, 2.6, 4, 1, true);
    const white = new THREE.Color(0xffffff);
    b.addGeometry(trunk, composeMatrix(0, 1.3, 0, 0, 1, 1, 1), new THREE.Color(0x6b5338));
    trunk.dispose();
    // The crown: an eight sided bipyramid, sixteen triangles, written by hand because a cone
    // primitive spends sixteen on one of its halves (each side quad degenerates at the apex).
    // The rim is the crown's widest line, so an olive squashed to 0.62 is a clump and a cypress
    // stretched to 2.0 is the pointed silhouette that reads at four hundred metres.
    const RIM = 2.1;
    const MID = 4.2;
    const UP = 6.7;
    const DOWN = 1.7;
    const px = [];
    const pz = [];
    for (let i = 0; i < 8; i += 1) {
      const a = (i / 8) * Math.PI * 2;
      px.push(Math.cos(a) * RIM);
      pz.push(Math.sin(a) * RIM);
    }
    function tri(ax, ay, az, bx, by, bz, cx, cy, cz) {
      const ux = bx - ax;
      const uy = by - ay;
      const uz = bz - az;
      const vx = cx - ax;
      const vy = cy - ay;
      const vz = cz - az;
      let nx = uy * vz - uz * vy;
      let ny = uz * vx - ux * vz;
      let nz = ux * vy - uy * vx;
      const nl = Math.sqrt(nx * nx + ny * ny + nz * nz) || 1;
      nx /= nl;
      ny /= nl;
      nz /= nl;
      // Outward: the face normal must point away from the crown's own centre.
      const mx = (ax + bx + cx) * 0.3333;
      const my = (ay + by + cy) * 0.3333 - MID;
      const mz = (az + bz + cz) * 0.3333;
      if (nx * mx + ny * my + nz * mz < 0) {
        const t = bx;
        bx = cx; cx = t;
        const ty = by;
        by = cy; cy = ty;
        const tz = bz;
        bz = cz; cz = tz;
        nx = -nx; ny = -ny; nz = -nz;
      }
      const i0 = b.vert(ax, ay, az, nx, ny, nz, white);
      const i1 = b.vert(bx, by, bz, nx, ny, nz, white);
      const i2 = b.vert(cx, cy, cz, nx, ny, nz, white);
      b.idx(i0, i1, i2);
    }
    for (let i = 0; i < 8; i += 1) {
      const j = (i + 1) & 7;
      tri(px[i], MID, pz[i], px[j], MID, pz[j], 0, UP, 0);
      tri(px[j], MID, pz[j], px[i], MID, pz[i], 0, DOWN, 0);
    }
    return b.build();
  })();
  const trees = new THREE.InstancedMesh(
    treeGeo,
    new THREE.MeshStandardMaterial({
      vertexColors: true,
      flatShading: true,
      roughness: 0.86,
      metalness: 0.0
    }),
    TREE_CAP
  );
  trees.castShadow = shadowsOn;
  trees.receiveShadow = shadowsOn;
  trees.name = 'trees';
  trees.frustumCulled = false;
  group.add(trees);

  const T_PLANE = 0;
  const T_OLIVE = 1;
  const T_CYPRESS = 2;
  const T_PALM = 3;
  // Olive and cypress are genuinely dark and varied, in the 0x4a5a2a to 0x2f401f band with the
  // cypress near 0x27351a, so the canopy reads as its own cluster instead of dissolving into
  // the khaki terrain behind it. Two tones per kind, hashed on position.
  // Canopy tones carry 30 percent more chroma than the old table, on the same values: the warm
  // sun multiplies the red channel hardest, so a muted green albedo lightens toward khaki and
  // the canopies measured 0.288 saturation against 0.38 in the ground behind them. The hue
  // holds at the olive-green the species actually is. Cypress stays as it was: a cypress is
  // nearly black at this distance and its value is the note, not its chroma.
  const T_PLANE_TONES = [0x425e1d, 0x3c561a, 0x486820];
  const T_OLIVE_TONES = [0x425e1d, 0x364e17, 0x3d571c, 0x2f4316];
  const T_CYPRESS_TONES = [0x27351a, 0x2c3a1c, 0x223016];
  // Pack trees near the centre, procedural 24 triangle boxes beyond the model budget. The two
  // cheap 42 triangle suburban trees carry the bulk (a canopy that reads as a real crown for 36
  // extra triangles over a box, drawn twice), while the cypress and palm models are capped to
  // the curated hero rows because each costs twelve times that.
  const modelTreeEntries = [];
  const modelTreeRecords = [];
  let modelTreeUsed = 0;
  let modelCypressUsed = 0;
  let modelPalmUsed = 0;
  const treeModelTint = new THREE.Color();
  function makeTreeModelTrees() {
    const specs = [
      { kind: T_PLANE, category: 'tree-plane', cap: MODEL_TREE_MAX },
      { kind: T_OLIVE, category: 'tree-olive', cap: MODEL_TREE_MAX },
      { kind: T_CYPRESS, category: 'tree-cypress', cap: MODEL_CYPRESS_MAX },
      { kind: T_PALM, category: 'tree-palm', cap: MODEL_PALM_MAX }
    ];
    for (let i = 0; i < specs.length; i += 1) {
      const s = specs[i];
      if (s.cap <= 0) continue;
      // The cheapest model of the species, chosen by its measured triangle count rather than
      // by its position in a merged tier: the suburban pair costs 42 triangles where the nature
      // kit's plane and olive cost 114 and 196, and the budget is spent on crowns, not on leaves.
      const list = assets ? (assets.byCategory(s.category) || []) : [];
      let chosen = null;
      for (let k = 0; k < list.length; k += 1) {
        if (!chosen || list[k].tris < chosen.tris) chosen = list[k];
      }
      if (!chosen) continue;
      const made = modelMesh(s.category, { model: chosen, count: s.cap, shadows: shadowsOn });
      if (!made) continue;
      made.mesh.count = 0;
      modelTreeEntries[s.kind] = { mesh: made.mesh, model: made.model, cap: s.cap, used: 0 };
    }
  }
  makeTreeModelTrees();
  const treeCol = new THREE.Color();
  const treeRnd = mulberry(4242);
  let treeCursor = 0;
  let cypressPlanted = 0;
  const cypressSpots = [];
  function addTree(x, z, kind, size, opts) {
    const entry = modelTreeEntries[kind];
    const isPalm = kind === T_PALM;
    const hero = !!(opts && opts.hero);
    if (isPalm) {
      if (!entry || entry.used >= entry.cap) return false;
    } else if (treeCursor >= TREE_CAP) {
      return false;
    }
    // A tree row is sited from the path offset alone, so a trunk can land inside a plot whose
    // footprint is rotated or set back. Walk a deterministic spiral until the trunk clears every
    // building footprint and every road cell; if nothing within about ten metres is free, the
    // tree is simply not planted rather than growing out of a wall.
    if (insideBuilding(x, z, 0.9)) {
      let found = false;
      for (let attempt = 0; attempt < 10; attempt += 1) {
        const ang = hash2(Math.round(x * 0.5) + attempt * 37 + 911,
          Math.round(z * 0.5) + attempt * 53 + 977) * Math.PI * 2;
        const rr = 2.0 + attempt * 1.15;
        const nx2 = x + Math.cos(ang) * rr;
        const nz2 = z + Math.sin(ang) * rr;
        if (insideBuilding(nx2, nz2, 0.9)) continue;
        if (flagsAt(nx2, nz2) & TREE_BLOCK) continue;
        x = nx2;
        z = nz2;
        found = true;
        break;
      }
      if (!found) return false;
    }
    const r = treeRnd();
    const hs = hash2(Math.round(x * 0.7) + 2048, Math.round(z * 0.7) + 2048);
    let sx;
    let sy;
    if (kind === T_PLANE) {
      const s = 1.0 + r * 0.32;
      sx = s * 1.2;
      sy = s * 1.12;
      treeCol.setHex(T_PLANE_TONES[(hs * T_PLANE_TONES.length) | 0]);
      treeCol.multiplyScalar(0.86 + treeRnd() * 0.26);
    } else if (kind === T_OLIVE) {
      const s = 0.78 + r * 0.32;
      sx = s * 1.1;
      sy = s * 0.62;
      treeCol.setHex(T_OLIVE_TONES[(hs * T_OLIVE_TONES.length) | 0]);
      treeCol.multiplyScalar(0.84 + treeRnd() * 0.3);
    } else if (isPalm) {
      // Palms exist only as pack models: without one there is no procedural palm to plant.
      sx = 1;
      sy = 1;
    } else {
      const s = size || 0.9 + r * 0.35;
      sx = s * 0.42;
      sy = s * 2.0;
      treeCol.setHex(T_CYPRESS_TONES[(hs * T_CYPRESS_TONES.length) | 0]);
      treeCol.multiplyScalar(0.86 + treeRnd() * 0.28);
    }
    const ty = groundHeightAt(x, z) - 0.15;
    // The village is planted last but its houses already stand, so a crown that would brush a
    // wall is not planted at all rather than corrected afterwards.
    if (houseSpots.length > 0 && x > OLD_X0 - 12 && x < OLD_X1 + 12 &&
      z > OLD_Z0 - 12 && z < OLD_Z1 + 12) {
      for (let i = 0; i < houseSpots.length; i += 1) {
        if (treeHitsHouse(x, z, sx, sy, ty, houseSpots[i])) return false;
      }
    }
    // Near the centre and inside the model budget, the canopy becomes a pack tree. The far
    // slopes keep the 24 triangle box, which is the whole point of the cap: a model costs 36
    // extra triangles for the cheap suburban trees and about 400 for a cypress, drawn twice on
    // the desktop shadow pass. The planting order puts the avenue, the ring, the plaza and the
    // roundabout first, so the models land where the camera reads them.
    const nearCentre = x * x + (z - 4) * (z - 4) < 96 * 96;
    const crownNeed = !!(opts && opts.crown);
    // The curated crown cypresses are the hilltop silhouette, so the last six of the cypress
    // budget are reserved for them: the avenue's own cypresses cannot spend it first.
    const modelLimit = kind === T_CYPRESS && entry && !crownNeed
      ? Math.max(0, entry.cap - 6)
      : (entry ? entry.cap : 0);
    if (entry && entry.used < modelLimit && (isPalm || nearCentre || hero)) {
      let scale;
      if (kind === T_PLANE) scale = sy;
      else if (kind === T_OLIVE) scale = 0.82 + r * 0.32;
      else if (kind === T_CYPRESS) scale = sy * 0.76;
      else scale = 1.05 + r * 0.34;
      const jitter = 0.92 + treeRnd() * 0.2;
      dummy.position.set(x, ty, z);
      dummy.rotation.set((treeRnd() - 0.5) * 0.035, treeRnd() * Math.PI * 2, 0);
      dummy.scale.set(scale * jitter, scale, scale * jitter);
      dummy.updateMatrix();
      entry.mesh.setMatrixAt(entry.used, dummy.matrix);
      const t = isPalm ? 0.94 + treeRnd() * 0.1 : 0.92 + treeRnd() * 0.12;
      treeModelTint.setRGB(0.97 * t, 1.0 * t, 0.94 * t);
      if (entry.mesh.instanceColor) entry.mesh.setColorAt(entry.used, treeModelTint);
      modelTreeRecords.push({ x: x, z: z, sx: scale * jitter, sy: scale, kind: kind });
      entry.used += 1;
      modelTreeUsed += 1;
      if (kind === T_CYPRESS) {
        modelCypressUsed += 1;
        cypressSpots.push({ x: x, z: z, sx: scale * jitter, sy: scale, y: ty });
      }
      if (isPalm) modelPalmUsed += 1;
      markDisk(x, z, 1.2, B_TREE);
      return true;
    }
    if (isPalm) return false;
    dummy.position.set(x, ty, z);
    dummy.rotation.set(0, treeRnd() * Math.PI * 2, 0);
    dummy.scale.set(sx, sy, sx);
    dummy.updateMatrix();
    trees.setMatrixAt(treeCursor, dummy.matrix);
    trees.setColorAt(treeCursor, treeCol);
    markDisk(x, z, 1.2, B_TREE);
    if (kind === T_CYPRESS) {
      cypressPlanted += 1;
      // Kept for the crown audit: the trunk position and the crown's own scale and base, so a
      // stone wall and a church cypress are compared as the cone it is drawn as.
      cypressSpots.push({ x: x, z: z, sx: sx, sy: sy, y: ty });
    }
    treeCursor += 1;
    return true;
  }
  // B_OLD is in the block mask: the village's own ground is paved and built, not an unbuilt
  // slope, so only the curated crown table plants inside it.
  const TREE_BLOCK = B_SOLID | B_TREE | B_OLD;

  // Plane trees in the avenue pavements; cypresses where it reaches the hilltop.
  for (let side = -1; side <= 1; side += 2) {
    for (let i = 3; i < avePath.n; i += 6) {
      const off = side * (AVE_PAVE_IN + AVE_PAVE * 0.62);
      const x = avePath.xs[i] + avePath.nx[i] * off;
      const z = avePath.zs[i] + avePath.nz[i] * off;
      if (inRoundabout(x, z, ROUND_R + 3)) continue;
      if (flagsAt(x, z) & TREE_BLOCK) continue;
      addTree(x, z, z < -78 ? T_CYPRESS : T_PLANE);
    }
  }
  // Plane trees on the verges of the ring road, both sides.
  for (let side = -1; side <= 1; side += 2) {
    for (let i = 4; i < ringPath.n; i += 7) {
      const off = side * (RING_HALF + RING_PAVE + 1.7);
      const x = ringPath.xs[i] + ringPath.nx[i] * off;
      const z = ringPath.zs[i] + ringPath.nz[i] * off;
      if (inRoundabout(x, z, ROUND_R + 3)) continue;
      if (flagsAt(x, z) & TREE_BLOCK) continue;
      addTree(x, z, T_PLANE);
    }
  }
  // Around the market square.
  {
    const spots = [[13.4, 5.2], [41.6, 5.2], [13.4, 22.8], [41.6, 22.8], [41.6, 14], [13.4, 14]];
    for (let i = 0; i < spots.length; i += 1) {
      if (diskFree(spots[i][0], spots[i][1], 1.0, TREE_BLOCK)) addTree(spots[i][0], spots[i][1], T_PLANE);
    }
  }
  // Palms in the market's four tree pits, just outside the stall rows and inside the plaza
  // edges. The plaza is paved, so the general tree mask would reject every pit: the palm test
  // clears other trees only and lets the pit stand on the paving, which is what a tree pit is.
  // A palm with no pack model is simply not planted, and the six calls consume no RNG before
  // they return, so the fallback keeps the exact plane trees it had.
  {
    const palmSpots = [[25.4, 4.4], [46.6, 4.4], [25.4, 19.6], [46.6, 19.6]];
    for (let i = 0; i < palmSpots.length; i += 1) {
      const px = palmSpots[i][0];
      const pz = palmSpots[i][1];
      if (diskFree(px, pz, 1.2, B_TREE)) addTree(px, pz, T_PALM, 0, { hero: true });
    }
  }
  // The roundabout island: an olive and, at the two south corners, palms when the pack has
  // them and the two cypresses the procedural scene has always used when it does not.
  addTree(roundaboutCenter.x, roundaboutCenter.z, T_OLIVE);
  if (!addTree(roundaboutCenter.x - 2.6, roundaboutCenter.z + 1.2, T_PALM, 0, { hero: true })) {
    addTree(roundaboutCenter.x - 2.6, roundaboutCenter.z + 1.2, T_CYPRESS);
  }
  if (!addTree(roundaboutCenter.x + 2.6, roundaboutCenter.z + 1.2, T_PALM, 0, { hero: true })) {
    addTree(roundaboutCenter.x + 2.6, roundaboutCenter.z + 1.2, T_CYPRESS);
  }
  // Olive groves in loose rows on the free slopes, with cypress accents.
  for (let g = 0; g < groves.length; g += 1) {
    const gr = groves[g];
    const a = treeRnd() * Math.PI;
    const ca = Math.cos(a);
    const sa = Math.sin(a);
    const sp = 6.4;
    const n = Math.ceil(gr.r / sp);
    for (let j = -n; j <= n; j += 1) {
      for (let i = -n; i <= n; i += 1) {
        const lu = i * sp + (treeRnd() - 0.5) * 1.8;
        const lv = j * sp + (treeRnd() - 0.5) * 1.8;
        if (lu * lu + lv * lv > gr.r * gr.r) continue;
        if (treeRnd() < 0.12) continue;
        const x = gr.x + lu * ca - lv * sa;
        const z = gr.z + lu * sa + lv * ca;
        if (flagsAt(x, z) & TREE_BLOCK) continue;
        addTree(x, z, treeRnd() < 0.18 ? T_CYPRESS : T_OLIVE);
      }
    }
  }
  // Gardens inside the blocks: the courtyards that the paving left green.
  for (let li = 0; li < quads.length; li += 1) {
    const L = quads[li];
    let placed = 0;
    for (let k = 0; k < 90 && placed < 8; k += 1) {
      const a = k * 2.399 + li;
      const rr = Math.sqrt((k + 0.5) / 90) * Math.min(L.radius, 34);
      const x = L.center.x + Math.cos(a) * rr;
      const z = L.center.z + Math.sin(a) * rr;
      if (!diskFree(x, z, 1.5, TREE_BLOCK)) continue;
      if (addTree(x, z, treeRnd() < 0.6 ? T_PLANE : T_OLIVE)) placed += 1;
    }
  }
  // A cypress row along the hilltop edge, outside the old village and never through it.
  for (let i = 0; i < 24; i += 1) {
    const x = -78 + i * 6.9 + (treeRnd() - 0.5) * 2;
    const z = -108 + (treeRnd() - 0.5) * 7;
    if (!inUrban(x, z)) continue;
    if (flagsAt(x, z) & TREE_BLOCK) continue;
    if (treeRnd() < 0.35) continue;
    addTree(x, z, T_CYPRESS);
  }
  // The crown's own planting: paired cypresses at the church front and a row behind the apse,
  // the silhouette the hilltop needs when it is read from the district tier. The table is the
  // one the houses were laid around, so a cypress never appears inside a wall.
  {
    const spots = CROWN_CYPRESS;
    for (let i = 0; i < spots.length; i += 1) {
      const x = spots[i][0];
      const z = spots[i][1];
      // B_OLD is the village ground this table is allowed to stand on; any other solid bit means
      // the reservation moved and the spot is no longer the one the houses were laid around.
      if (flagsAt(x, z) & (B_SOLID & ~B_OLD)) continue;
      // A hand-set size: the curated cypresses stand in two metre alleys, so they are grown to
      // clear both walls with their crowns instead of taking the random scale of the groves.
      // These six are the hilltop silhouette, so they take pack pines first and keep the last
      // six slots of the cypress budget.
      addTree(x, z, T_CYPRESS, 0.8, { hero: true, crown: true });
    }
  }
  const hillsideTreeStart = treeCursor;
  const hillsideCypressStart = cypressPlanted;
  let plantTerrace = 0;
  let plantTread = 0;
  let plantBand = 0;
  let plantSide = 0;
  let plantTrack = 0;
  // Terraces and farm tracks, planted before the distance so the foreground always gets its
  // rows. The terrace record carries its own run: the row is laid three metres uphill of the
  // wall, on the tread it retains, spaced loosely along the contour and jittered across it. A
  // tree that would land on a road, a track, another wall, a parcel or the corridor is skipped
  // by the same occupancy test every other row uses, and every planted tree marks its own
  // B_TREE disc through addTree.
  {
    const gr = mulberry(73117);
    let runIndex = 0;
    for (let r = 0; r < terraceRuns.length; r += 1) {
      const run = terraceRuns[r];
      const len = run.len;
      if (len < 6) continue;
      const ux = (run.x1 - run.x0) / len;
      const uz = (run.z1 - run.z0) / len;
      const mx = (run.x0 + run.x1) * 0.5;
      const mz = (run.z0 + run.z1) * 0.5;
      const gx = groundHeightAt(mx + 2.5, mz) - groundHeightAt(mx - 2.5, mz);
      const gz = groundHeightAt(mx, mz + 2.5) - groundHeightAt(mx, mz - 2.5);
      let nx;
      let nz;
      const gl = Math.sqrt(gx * gx + gz * gz);
      if (gl > 0.006) {
        nx = -gx / gl;
        nz = -gz / gl;
      } else {
        const tx = -mx;
        const tz = 4 - mz;
        const tl = Math.sqrt(tx * tx + tz * tz) || 1;
        nx = tx / tl;
        nz = tz / tl;
      }
      const off = 2.9;
      const step = isTouch ? 9.2 : 7.0;
      for (let s = 3.2; s < len - 2.4; s += step + gr() * 1.5) {
        const x = run.x0 + ux * s + nx * off + (gr() - 0.5) * 1.8;
        const z = run.z0 + uz * s + nz * off + (gr() - 0.5) * 1.8;
        if (!diskFree(x, z, 1.55, TREE_BLOCK)) continue;
        if (insideBuilding(x, z, 1.0)) continue;
        if (addTree(x, z, gr() < 0.09 ? T_CYPRESS : T_OLIVE)) plantTerrace += 1;
      }
      // A cypress pair at the end of every ninth run: the vertical accent that separates a
      // worked terrace from a slope that has merely been planted.
      if (runIndex % 9 === 4) {
        for (let k = 0; k < 2; k += 1) {
          const s = len - 3.2 - k * 2.1;
          const x = run.x0 + ux * s + nx * (off + 0.9) + (gr() - 0.5) * 0.9;
          const z = run.z0 + uz * s + nz * (off + 0.9) + (gr() - 0.5) * 0.9;
          if (!diskFree(x, z, 1.4, TREE_BLOCK)) continue;
          if (insideBuilding(x, z, 1.0)) continue;
          if (addTree(x, z, T_CYPRESS)) plantTerrace += 1;
        }
      }
      runIndex += 1;
    }
  }

  // The treads between the near terraces: loose olive rows along the contours, following the
  // slope in one direction and the terrace steps in the other. Rows are tightest in the band the
  // portrait frame's bottom tenths look at (z 120 to 165) and open up as the slope falls away,
  // so the canopies cluster where the frame needs them instead of spreading evenly over a hill
  // nobody sees. A third of the stations are deliberately empty: a grove is not a carpet.
  {
    const gr = mulberry(81883);
    const nearRows = isTouch
      ? [129, 139, 149, 159, 172]
      : [190, 230, 276];
    const step = isTouch ? 7.4 : 8.2;
    for (let r = 0; r < nearRows.length; r += 1) {
      const z0 = nearRows[r];
      // The portrait frame only ever sees the band up to z one seventy five, and there it sees a
      // hundred metre window, so the rows inside that band are twice as tight and half as
      // thinned: a grove dense enough to read at two hundred metres, not a scatter.
      const nearBand = isTouch && z0 <= 175;
      const rowStep = nearBand ? 5.0 : step;
      const rowSkip = nearBand ? 0.3 : 0.45;
      for (let x = -190; x <= 190; x += rowStep) {
        if (gr() < rowSkip) continue;
        const jx = x + (gr() - 0.5) * (rowStep * 0.5);
        const jz = z0 + (gr() - 0.5) * 5.5;
        if (!diskFree(jx, jz, 1.55, TREE_BLOCK)) continue;
        if (insideBuilding(jx, jz, 1.0)) continue;
        if (addTree(jx, jz, gr() < 0.1 ? T_CYPRESS : T_OLIVE)) plantTread += 1;
      }
    }
    // The bottom edge of the desktop frame: the outer belt either side of the ring's southern
    // arc, which is the only ground the bottom fifth can show. Rows run along the contour there
    // too, out to the frame's corners.
    if (!isTouch) {
      const bandZ = [92, 104, 114, 122];
      for (let r = 0; r < bandZ.length; r += 1) {
        for (let side = -1; side <= 1; side += 2) {
          for (let d = 108; d <= 184; d += 4.5) {
            if (gr() < 0.3) continue;
            const bx = side * (d + (gr() - 0.5) * 3.2);
            const bz = bandZ[r] + (gr() - 0.5) * 4.4;
            if (!diskFree(bx, bz, 1.55, TREE_BLOCK)) continue;
            if (insideBuilding(bx, bz, 1.0)) continue;
            if (addTree(bx, bz, gr() < 0.08 ? T_CYPRESS : T_OLIVE)) plantBand += 1;
          }
        }
      }
    }
  }

  // The outer shoulders, as far as the desktop frame's left and right edges: rows along the
  // contours on ground no parcel owns. The touch profile sees this band only when the phone is
  // rotated or zoomed out, so it keeps its canopy money for the bottom of the portrait frame.
  if (!isTouch) {
    const gr = mulberry(51749);
    const sideRows = [-72, -36, 0, 36, 72, 108];
    for (let r = 0; r < sideRows.length; r += 1) {
      const z0 = sideRows[r];
      for (let side = -1; side <= 1; side += 2) {
        for (let d = 124; d <= 236; d += 7.5) {
          if (gr() < 0.4) continue;
          const x0 = side * (d + (gr() - 0.5) * 4.5);
          const jz = z0 + (gr() - 0.5) * 5;
          if (!diskFree(x0, jz, 1.55, TREE_BLOCK)) continue;
          if (insideBuilding(x0, jz, 1.0)) continue;
          if (addTree(x0, jz, gr() < 0.08 ? T_CYPRESS : T_OLIVE)) plantSide += 1;
        }
      }
    }
  }

  // Cypress clusters along the farm tracks, set back three and a half metres from the ruts on
  // alternating sides, in pairs: the row that marks a track climbing through a terraced slope.
  {
    const gr = mulberry(39019);
    for (let t = 0; t < trackPaths.length; t += 1) {
      const path = trackPaths[t];
      let side = 1;
      for (let i = 6; i < path.n - 4; i += 7) {
        side = -side;
        if (gr() < 0.42) continue;
        for (let k = 0; k < 2; k += 1) {
          const s = i + k * 1.4;
          const x = path.xs[s] + path.nx[s] * (TRACK_HALF + 2.3) * side + (gr() - 0.5) * 1.2;
          const z = path.zs[s] + path.nz[s] * (TRACK_HALF + 2.3) * side + (gr() - 0.5) * 1.2;
          if (!diskFree(x, z, 1.4, TREE_BLOCK)) continue;
          if (insideBuilding(x, z, 1.0)) continue;
          if (addTree(x, z, T_CYPRESS)) plantTrack += 1;
        }
      }
    }
  }
  const hillsideTrees = treeCursor - hillsideTreeStart;
  const hillsideCypress = cypressPlanted - hillsideCypressStart;
  // No canopies beyond the district. The far groves this pass used to plant read as dark
  // speckle and long silhouette streaks across the top of the frame, the exact dirt the owner
  // called out. The distant hills are now bare, soft silhouettes in haze; every tree left in
  // the mesh stands on the near slopes, inside the ring and along the terraces.
  const farPlanted = 0;
  const laneStart = treeCursor;
  // A cypress row down the lane, alternating sides, three and a half metres off the track
  // surface: the vertical accent that separates a worked slope from a bare one.
  {
    let side = 1;
    for (let s = 0; s < LANE_PATH.length - 1 && treeCursor < TREE_CAP; s += 1) {
      const ax0 = LANE_PATH[s][0];
      const az0 = LANE_PATH[s][1];
      const dx = LANE_PATH[s + 1][0] - ax0;
      const dz = LANE_PATH[s + 1][1] - az0;
      const len = Math.sqrt(dx * dx + dz * dz);
      if (len < 1) continue;
      const nx = -dz / len;
      const nz = dx / len;
      for (let t = 4; t < len && treeCursor < TREE_CAP; t += 11) {
        const u = t / len;
        const x = ax0 + dx * u + nx * 3.5 * side;
        const z = az0 + dz * u + nz * 3.5 * side;
        side = -side;
        if (flagsAt(x, z) & TREE_BLOCK) continue;
        addTree(x, z, treeRnd() < 0.7 ? T_CYPRESS : T_OLIVE);
      }
    }
  }
  const lanePlanted = treeCursor - laneStart;
  trees.count = treeCursor;
  if (trees.instanceColor) trees.instanceColor.needsUpdate = true;
  trees.instanceMatrix.needsUpdate = true;

  // The pack canopies join the group only when they actually took an instance, so a full
  // procedural fallback (or a spent budget) adds no draw call at all.
  for (let k = 0; k < modelTreeEntries.length; k += 1) {
    const entry = modelTreeEntries[k];
    if (!entry || entry.used <= 0) continue;
    entry.mesh.count = entry.used;
    if (entry.mesh.instanceColor) entry.mesh.instanceColor.needsUpdate = true;
    if (entry.mesh.instanceMatrix) entry.mesh.instanceMatrix.needsUpdate = true;
    group.add(entry.mesh);
  }

  // ------------------------------------------------------------------ street lamps
  const lampGeo = (function () {
    const b = makeBuilder();
    const grey = new THREE.Color(0xffffff);
    b.addGeometry(unitBox, composeMatrix(0, 3.2, 0, 0, 0.2, 6.4, 0.2), grey);
    b.addGeometry(unitBox, composeMatrix(0.6, 6.5, 0, 0, 1.5, 0.3, 0.55), grey);
    return b.build();
  })();
  const lampMat = new THREE.MeshStandardMaterial({ color: 0x8d8a80, roughness: 0.46, metalness: 0.2 });
  const streetLampMesh = new THREE.InstancedMesh(lampGeo, lampMat, LAMP_CAP);
  streetLampMesh.name = 'lamps';
  streetLampMesh.frustumCulled = false;
  group.add(streetLampMesh);

  // Warm pools of light under each lamp at night, additive, one draw call.
  const glowGeo = new THREE.CircleGeometry(1, 16);
  glowGeo.rotateX(-Math.PI / 2);
  {
    const count = glowGeo.attributes.position.count;
    const cols = new Float32Array(count * 3);
    for (let i = 0; i < count; i += 1) {
      const x = glowGeo.attributes.position.getX(i);
      const z = glowGeo.attributes.position.getZ(i);
      const v = 1 - Math.min(1, Math.sqrt(x * x + z * z));
      cols[i * 3] = v;
      cols[i * 3 + 1] = v;
      cols[i * 3 + 2] = v;
    }
    glowGeo.setAttribute('color', new THREE.BufferAttribute(cols, 3));
  }
  const glowMat = new THREE.MeshBasicMaterial({
    color: 0xffbe78,
    vertexColors: true,
    transparent: true,
    opacity: 0.72,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    fog: false,
    polygonOffset: true,
    polygonOffsetFactor: -2,
    polygonOffsetUnits: -2
  });
  const lampGlow = new THREE.InstancedMesh(glowGeo, glowMat, LAMP_CAP);
  lampGlow.name = 'lamp-glow';
  lampGlow.frustumCulled = false;
  lampGlow.visible = false;
  lampGlow.renderOrder = 2;
  group.add(lampGlow);

  let lampCursor = 0;
  let lampMeshCursor = 0;
  // Pack lamps replace the ring and avenue post boxes when the street tier is loaded. The warm
  // pool and the luminaire disc stay either way: they are what actually lights the asphalt.
  const lampModelEntries = [];
  let lampModelUsed = 0;
  function lampModelEntry(index) {
    if (!assets || MODEL_LAMP_MAX <= 0) return null;
    if (lampModelEntries[index]) return lampModelEntries[index];
    const list = assets.byCategory('lamp') || [];
    if (list.length === 0) return null;
    const model = list[index % list.length];
    const made = modelMesh('lamp', { model: model, count: MODEL_LAMP_MAX, shadows: shadowsOn });
    if (!made) return null;
    made.mesh.count = 0;
    const entry = { mesh: made.mesh, model: model, used: 0 };
    lampModelEntries[index] = entry;
    return entry;
  }
  const lampHeadX = new Float32Array(LAMP_CAP);
  const lampHeadY = new Float32Array(LAMP_CAP);
  const lampHeadZ = new Float32Array(LAMP_CAP);
  const lampHeadR = new Float32Array(LAMP_CAP);
  function addLamp(x, z, towardX, towardZ, poolScale, surfaceY, modelKind) {
    if (lampCursor >= LAMP_CAP) return;
    if (flagsAt(x, z) & (B_TREE | B_BUILD | B_PROP)) return;
    const dx = towardX - x;
    const dz = towardZ - z;
    const l = Math.sqrt(dx * dx + dz * dz) || 1;
    const g = surfaceY === undefined ? groundHeightAt(x, z) : surfaceY;
    const pool = poolScale === undefined ? 5.0 : poolScale;
    const yaw = Math.atan2(-dz, dx);
    const entry = (modelKind === 0 || modelKind === 1) ? lampModelEntry(modelKind) : null;
    let modelScale = 1;
    if (entry && lampModelUsed < MODEL_LAMP_MAX) {
      modelScale = 1.14;
      // The model's arm runs along its local +Z, so the post is turned to face the road instead
      // of using the procedural arm's local +X convention.
      dummy.position.set(x, g, z);
      dummy.rotation.set(0, Math.atan2(dx, dz), 0);
      dummy.scale.set(modelScale, modelScale, modelScale);
      dummy.updateMatrix();
      entry.mesh.setMatrixAt(entry.used, dummy.matrix);
      entry.used += 1;
      lampModelUsed += 1;
    } else {
      dummy.position.set(x, g, z);
      dummy.rotation.set(0, yaw, 0);
      dummy.scale.set(1, 1, 1);
      dummy.updateMatrix();
      streetLampMesh.setMatrixAt(lampMeshCursor, dummy.matrix);
      lampMeshCursor += 1;
    }
    // The luminaire on the end of the arm, in world space: the night pass puts a small warm
    // disc on it, so every lamp is a light source as well as a pool on the asphalt.
    lampHeadX[lampCursor] = x + (dx / l) * 0.7 * modelScale;
    lampHeadY[lampCursor] = g + (entry && modelScale !== 1
      ? (entry.model.footprint.h - 0.25) * modelScale
      : 6.5);
    lampHeadZ[lampCursor] = z + (dz / l) * 0.7 * modelScale;
    lampHeadR[lampCursor] = 0.9 + hash2(Math.round(x * 7) + 4096, Math.round(z * 7) + 4096) * 0.5;
    const hx = x + (dx / l) * 1.1;
    const hz = z + (dz / l) * 1.1;
    dummy.position.set(hx, (surfaceY === undefined ? groundHeightAt(hx, hz) : surfaceY) + 0.42, hz);
    dummy.rotation.set(0, 0, 0);
    dummy.scale.set(pool, 1, pool);
    dummy.updateMatrix();
    lampGlow.setMatrixAt(lampCursor, dummy.matrix);
    markDisk(x, z, 0.8, B_PROP);
    lampCursor += 1;
  }
  for (let i = 0; i < ringPath.n; i += 15) {
    const side = ((i / 15) & 1) ? 1 : -1;
    const off = side * (RING_HALF + RING_PAVE - 0.5);
    const x = ringPath.xs[i] + ringPath.nx[i] * off;
    const z = ringPath.zs[i] + ringPath.nz[i] * off;
    if (inRoundabout(x, z, ROUND_R + 1) || (flagsAt(x, z) & B_AVE)) continue;
    addLamp(x, z, ringPath.xs[i], ringPath.zs[i], undefined, undefined, 0);
  }
  // The avenue every twelve metres, alternating sides: a ribbon of light at night.
  for (let i = 4; i < avePath.n; i += 8) {
    const side = ((i / 8) & 1) ? 1 : -1;
    const off = side * (AVE_PAVE_IN + 0.6);
    const x = avePath.xs[i] + avePath.nx[i] * off;
    const z = avePath.zs[i] + avePath.nz[i] * off;
    if (inRoundabout(x, z, ROUND_R + 1) || (flagsAt(x, z) & B_RING)) continue;
    addLamp(x, z, avePath.xs[i], avePath.zs[i], undefined, undefined, 1);
  }
  // Secondary streets, sparsely.
  for (let i = 0; i < lampSpots.length; i += 1) {
    const s = lampSpots[i];
    addLamp(s.x, s.z, s.tx, s.tz);
  }
  // The two plaza lamps stand off the central walk, on the diagonals of the fountain, so the
  // market's own walk from the avenue pavement to the water is never blocked by a pole.
  addLamp(PLAZA_CX - 10, PLAZA_CZ + 1.8, PLAZA_CX, PLAZA_CZ);
  addLamp(PLAZA_CX + 10, PLAZA_CZ - 1.8, PLAZA_CX, PLAZA_CZ);
  // The near switchback's own row: the lane is the only structure in the bottom tenths of the
  // portrait frame, and after dark those tenths carry nothing but dark slope. Thirteen poles on
  // the lower hairpins give the near ground its pools. They are smaller than a street lamp's
  // (3.4 instead of 5 metres) because this is a farm track, and they stand on the drawn terrain
  // surface rather than the analytic one, so no pool sinks into the slope. Their stations were
  // picked from the projection: the phone frame's eighth tenth runs from the third hairpin
  // (z 178 to 200) down, so three of the thirteen stand there and none of them is wasted above
  // the seventh tenth, where the lane is already lit by the town.
  {
    const surf = terrainMesh.userData.surfaceHeightAt;
    const laneSpots = [
      [3, 0.5], [3, 0.9], [4, 0.06], [4, 0.34], [4, 0.72], [5, 0.12], [5, 0.52],
      [6, 0.86], [6, 0.28], [7, 0.68], [7, 0.06], [8, 0.44], [8, 0.8]
    ];
    let side = 1;
    for (let i = 0; i < laneSpots.length; i += 1) {
      const s = laneSpots[i][0];
      const u = laneSpots[i][1];
      const ax0 = LANE_PATH[s][0];
      const az0 = LANE_PATH[s][1];
      const dx = LANE_PATH[s + 1][0] - ax0;
      const dz = LANE_PATH[s + 1][1] - az0;
      const len = Math.sqrt(dx * dx + dz * dz) || 1;
      const nx = -dz / len;
      const nz = dx / len;
      const cx = ax0 + dx * u;
      const cz = az0 + dz * u;
      const x = cx + nx * 2.6 * side;
      const z = cz + nz * 2.6 * side;
      side = -side;
      addLamp(x, z, cx, cz, 3.4, surf(x, z));
    }
  }
  streetLampMesh.count = lampMeshCursor;
  lampGlow.count = lampCursor;
  streetLampMesh.instanceMatrix.needsUpdate = true;
  lampGlow.instanceMatrix.needsUpdate = true;
  for (let i = 0; i < lampModelEntries.length; i += 1) {
    const entry = lampModelEntries[i];
    if (!entry || entry.used <= 0) continue;
    entry.mesh.count = entry.used;
    if (entry.mesh.instanceMatrix) entry.mesh.instanceMatrix.needsUpdate = true;
    group.add(entry.mesh);
  }

  const streetLights = {
    setOn: function (on) {
      if (on) {
        lampMat.emissive.setHex(0xffc27e);
        lampMat.emissiveIntensity = 1.25;
        lampMat.color.setHex(0xf2e2c4);
      } else {
        lampMat.emissive.setHex(0x000000);
        lampMat.emissiveIntensity = 0;
        lampMat.color.setHex(0x8d8a80);
      }
      lampGlow.visible = !!on;
      setNightGlow(!!on);
    }
  };

  // ------------------------------------------------------------------ traffic lights
  // Real signal heads stand where two road families share asphalt, and nowhere else: the two
  // arms of the avenue mouth and the three spur mouths. Each is a dark mast, a pale backboard
  // and a housing, merged into one instanced unit so every mast, board and housing in the city
  // is one draw call. The board is what makes a head legible at district distance: three dots on
  // a dark pole disappear into the masonry, while a pale panel with three dots reads as a
  // signal from the far tier. Five junctions, fourteen heads: two avenue-facing at each arm of
  // the avenue mouth, a near-side and a far-side ring-facing pair at its two arms, and a
  // ring-facing pair at each of the three spur mouths.
  //
  // The lenses are a second instanced mesh, one small box drawn three times per head. They cannot
  // live in the merged geometry: vertex colour is baked per vertex and every head shares one
  // instance of that geometry, so merged lenses can only ever show all three colours at once on
  // every head in the city. Per-instance colour is the phase mechanism, so the lenses get their
  // own InstancedMesh with a white standard material and `setColorAt` writes the active lamp at
  // full value and the other two at one tenth. The cost is exactly one draw call for the whole
  // city. A single-mesh variant would need a per-instance phase attribute and a hand-patched lit
  // shader, which is a much larger surface to keep working for one call.
  //
  // The lens material is standard, not emissive: at night the sodium lamps own the frame, and a
  // head that glowed would fight them and drain the blue the night plate is built on. Standard
  // shading keeps a lit lens in the same light as the board it sits on instead of reading as a
  // flat sticker pasted over it. The heads cast no shadow either: a 7 m post's shadow is a line
  // at this distance, and the call it would spend in the shadow pass is worth more to the roof
  // clutter.
  const SIGNAL_CAP = 20;
  const SIGNAL_MAST_TOP = 6.8;
  const SIGNAL_LENS_Y = [6.35, 5.75, 5.15];   // red, amber, green, on the local +Z face
  const SIGNAL_LENS_Z = 0.41;
  const SIGNAL_LENS_DIM = 0.10;
  const signalGeo = (function () {
    const b = makeBuilder();
    const C_MAST = new THREE.Color(0x64696a);
    const C_HOUSING = new THREE.Color(0x2c3236);
    const C_BOARD = new THREE.Color(0xe2d8c2);
    b.addGeometry(unitBox, composeMatrix(0, SIGNAL_MAST_TOP * 0.5, 0, 0, 0.18, SIGNAL_MAST_TOP, 0.18),
      C_MAST);
    b.addGeometry(unitBox, composeMatrix(0, 5.80, -0.06, 0, 1.05, 1.9, 0.08), C_BOARD);
    b.addGeometry(unitBox, composeMatrix(0, 5.75, 0.16, 0, 0.66, 1.5, 0.42), C_HOUSING);
    return b.build();
  })();
  const signalMat = new THREE.MeshStandardMaterial({
    vertexColors: true,
    roughness: 0.54,
    metalness: 0.12
  });
  const signals = new THREE.InstancedMesh(signalGeo, signalMat, SIGNAL_CAP);
  signals.name = 'signals';
  signals.receiveShadow = shadowsOn;
  signals.frustumCulled = false;
  group.add(signals);

  // One lens box, three instances per head: 0.4 by 0.4 by 0.1, the same size the merged geometry
  // used, placed on the same local offsets so the face still sits on the housing. The instance
  // colour is the only thing that differs between the three, which is the whole point.
  const LENS_RED = new THREE.Color(0xc8402c);
  const LENS_AMBER = new THREE.Color(0xcf9722);
  const LENS_GREEN = new THREE.Color(0x2c7f60);
  const lensMat = new THREE.MeshStandardMaterial({
    color: 0xffffff,
    vertexColors: false,
    roughness: 0.42,
    metalness: 0.0
  });
  const signalLenses = new THREE.InstancedMesh(unitBox, lensMat, SIGNAL_CAP * 3);
  signalLenses.name = 'signalLenses';
  signalLenses.castShadow = false;
  signalLenses.receiveShadow = shadowsOn;
  signalLenses.frustumCulled = false;
  group.add(signalLenses);
  const lensScale = new THREE.Matrix4().makeScale(0.4, 0.4, 0.1);
  const lensOffset = new THREE.Matrix4();
  const lensMatrix = new THREE.Matrix4();
  const signalHeadXs = new Float32Array(SIGNAL_CAP);
  const signalHeadZs = new Float32Array(SIGNAL_CAP);
  // Per head: the junction record it belongs to and the road family it governs. Both are
  // written by the placement loop that put the head there, from the same table entry, so no
  // later pass has to match a head back to a junction by its position.
  const signalHeadJunction = new Int8Array(SIGNAL_CAP);
  const signalHeadFamily = new Uint8Array(SIGNAL_CAP);
  let signalCursor = 0;

  // The two road families a head can govern, published on the signals api so agents reads the
  // same pair the placement wrote instead of guessing it from a yaw.
  const FAMILY_AVENUE = 0;
  const FAMILY_RING = 1;

  // --- the junction records ---------------------------------------------------------
  // A junction is a place where two road families share asphalt. There are five, and none of
  // them is authored: the two arms of the avenue mouth come from the segment intersection of
  // each carriageway's own lane centre with the ring centreline, both polylines the world
  // already draws, and the three spur mouths are the first point of each spur table against the
  // ring. A head is placed from a record and written with that record's id; the record publishes
  // its centre, its arms and the approaches it owns.
  const SIGNAL_AVE_LANE = AVE_CAR_IN + 1.55;   // the car lane centre agents holds, mirrored here
  const junctionRecords = [];
  const signalPath = { x: 0, z: 0, nx: 0, nz: 0 };

  // One arc lookup over a sampled path: position and unit normal, interpolated between samples.
  // Build time only, and it writes the caller's scratch, so nothing allocates per frame.
  function pathOn(path, s, out) {
    let d = s;
    if (d < 0) d = 0;
    else if (d > path.len) d = path.len;
    const f0 = d / path.step;
    let i = f0 | 0;
    let f = f0 - i;
    if (i >= path.n) {
      i = path.n - 1;
      f = 1;
    }
    const nx0 = path.nx[i];
    const nz0 = path.nz[i];
    out.x = path.xs[i] + (path.xs[i + 1] - path.xs[i]) * f;
    out.z = path.zs[i] + (path.zs[i + 1] - path.zs[i]) * f;
    out.nx = nx0 + (path.nx[i + 1] - nx0) * f;
    out.nz = nz0 + (path.nz[i + 1] - nz0) * f;
    return out;
  }

  // XZ segment intersection. False when parallel or disjoint; t runs along the first segment
  // (the avenue lane) and u along the second (the ring), both in 0..1.
  const segHit = { t: 0, u: 0 };
  function segmentCross(ax, az, bx, bz, cx, cz, dx, dz) {
    const ex = bx - ax;
    const ez = bz - az;
    const fx = dx - cx;
    const fz = dz - cz;
    const den = ex * fz - ez * fx;
    if (den > -1e-9 && den < 1e-9) return false;
    const px = cx - ax;
    const pz = cz - az;
    const t = (px * fz - pz * fx) / den;
    const u = (px * ez - pz * ex) / den;
    if (t < 0 || t > 1 || u < 0 || u > 1) return false;
    segHit.t = t;
    segHit.u = u;
    return true;
  }

  const mouthArms = [];
  for (let side = 0; side < 2; side += 1) {
    const off = side === 0 ? -SIGNAL_AVE_LANE : SIGNAL_AVE_LANE;
    for (let i = 0; i < avePath.n; i += 1) {
      const ax = avePath.xs[i] + avePath.nx[i] * off;
      const az = avePath.zs[i] + avePath.nz[i] * off;
      const bx = avePath.xs[i + 1] + avePath.nx[i + 1] * off;
      const bz = avePath.zs[i + 1] + avePath.nz[i + 1] * off;
      let found = false;
      for (let j = 0; j < ringPath.n; j += 1) {
        if (!segmentCross(ax, az, bx, bz,
          ringPath.xs[j], ringPath.zs[j], ringPath.xs[j + 1], ringPath.zs[j + 1])) continue;
        mouthArms.push({
          side: side,
          x: ax + (bx - ax) * segHit.t,
          z: az + (bz - az) * segHit.t,
          aveS: (i + segHit.t) * avePath.step,
          ringS: (j + segHit.u) * ringPath.step
        });
        found = true;
        break;
      }
      if (found) break;
    }
  }
  // One record per avenue arm. Both arms carry the same phase group: the two crossings sit
  // 20.4 m apart on the ring, and independent phases would park a queue between them that the
  // upstream arm's own box rule then refuses to discharge through.
  for (let a = 0; a < mouthArms.length; a += 1) {
    junctionRecords.push({
      id: junctionRecords.length,
      kind: 'avenue',
      route: 'avenue mouth',
      x: mouthArms[a].x,
      z: mouthArms[a].z,
      arms: [mouthArms[a]],
      approaches: { avenue: true, ring: true, spur: false },
      signalised: true,
      phaseGroup: 0
    });
  }
  const spurRecordBase = junctionRecords.length;
  for (let s = 0; s < spurPaths.length; s += 1) {
    const path = spurPaths[s];
    const x = path.xs[0];
    const z = path.zs[0];
    let ringS = 0;
    let bd = Infinity;
    for (let i = 0; i <= ringPath.n; i += 1) {
      const dx = ringPath.xs[i] - x;
      const dz = ringPath.zs[i] - z;
      const d = dx * dx + dz * dz;
      if (d < bd) {
        bd = d;
        ringS = i * ringPath.step;
      }
    }
    junctionRecords.push({
      id: junctionRecords.length,
      kind: 'spur',
      route: 'spur mouth',
      spur: s,
      x: x,
      z: z,
      arms: [{ spur: s, x: x, z: z, aveS: -1, ringS: ringS }],
      approaches: { avenue: false, ring: true, spur: true },
      signalised: false,
      phaseGroup: 1 + s
    });
  }

  // The avenue and the ring mark their whole corridor, pavement included, so their own bit has
  // to be allowed where the pole stands and the other road's bit rejected instead: a pole on the
  // avenue pavement must not stand in the ring corridor, and the reverse.
  function signalFree(x, z, otherRoad) {
    if (flagsAt(x, z) & (B_BUILD | B_PROP | B_TREE | B_STREET | B_TRAM | B_ROUND | B_PLAZA |
      B_RAIL | B_VIA | B_SPUR | B_CORR)) return false;
    if (otherRoad && (flagsAt(x, z) & otherRoad)) return false;
    return !insideBuilding(x, z, 0.3);
  }

  // A head faces the traffic that has to obey it: the mast stands on the right of that traffic,
  // so the armless head looks straight down the approach. The face is the local +Z of the unit
  // geometry, which is rotY = atan2(nx, nz) for an outward normal (nx, nz). The junction and
  // the family are stamped on the head here, so the lamp a viewer reads and the phase a car
  // obeys come from one record and cannot drift apart.
  function addSignal(x, z, faceX, faceZ, otherRoad, junction, family) {
    if (signalCursor >= SIGNAL_CAP) return false;
    if (!signalFree(x, z, otherRoad)) return false;
    const g = groundHeightAt(x, z);
    dummy.position.set(x, g, z);
    dummy.rotation.set(0, Math.atan2(faceX, faceZ), 0);
    dummy.scale.set(1, 1, 1);
    dummy.updateMatrix();
    signals.setMatrixAt(signalCursor, dummy.matrix);
    // The three lenses inherit the head's yaw, so they land on its local +Z face: offset in the
    // head's own frame, then scaled, then the head matrix carries the pair to the pavement.
    for (let j = 0; j < 3; j += 1) {
      lensOffset.makeTranslation(0, SIGNAL_LENS_Y[j], SIGNAL_LENS_Z);
      lensMatrix.copy(dummy.matrix).multiply(lensOffset).multiply(lensScale);
      signalLenses.setMatrixAt(signalCursor * 3 + j, lensMatrix);
    }
    signalHeadXs[signalCursor] = x;
    signalHeadZs[signalCursor] = z;
    signalHeadJunction[signalCursor] = junction;
    signalHeadFamily[signalCursor] = family;
    signalCursor += 1;
    return true;
  }

  // An avenue arm: a pair facing the approach, one on the outer kerb and one on the median
  // kerb, both looking down the carriageway that has to stop. The search walks back from the
  // arm because the pavement already carries trees, bins, bollards and the lamps, and none of
  // them is worth moving for a signal box.
  function placeAvenueHeads(record, arm) {
    pathOn(avePath, arm.aveS, signalPath);
    const nx = signalPath.nx;
    const nz = signalPath.nz;
    const tx = nz;
    const tz = -nx;
    const dir = arm.side === 0 ? 1 : -1;
    const faceX = -dir * tx;
    const faceZ = -dir * tz;
    const outer = arm.side === 0 ? -(AVE_HALF + 1.25) : AVE_HALF + 1.25;
    const inner = arm.side === 0 ? -(AVE_CAR_IN - 1.35) : AVE_CAR_IN - 1.35;
    let placed = 0;
    for (let k = 0; k < 18 && placed < 2; k += 1) {
      const back = 2.4 + k * 0.95;
      const off = (k & 1) === 0 ? outer : inner;
      const x = signalPath.x + nx * off - dir * tx * back;
      const z = signalPath.z + nz * off - dir * tz * back;
      if (addSignal(x, z, faceX, faceZ, B_RING, record.id, FAMILY_AVENUE)) placed += 1;
    }
    return placed;
  }

  // A ring arm: a pair facing the oncoming ring, one on each side of the carriageway. The near
  // pair at the mouth's entry arm stands before its crossing; the far pair at the exit arm stands
  // just beyond that arm's crossing, because the 20.4 m between the arms is the avenue's own
  // asphalt and pavement and no ring pole can stand there. Both pairs face the same oncoming
  // ring traffic, so between the arms a driver reads the far pair across the junction.
  function placeRingHeads(record, arm, dir) {
    pathOn(ringPath, arm.ringS, signalPath);
    const nx = signalPath.nx;
    const nz = signalPath.nz;
    const tx = nz;
    const tz = -nx;
    let placed = 0;
    // Six lateral attempts per side cover the trees and bins on the ring pavement. Walking
    // forward (dir = 1) from the exit arm leaves the avenue corridor at once; walking back
    // (dir = -1) from the entry arm does the same on its upstream side. Both walks therefore
    // stay on the ring's own pavement, which is why the near pair is placed at the entry arm
    // and the far pair beyond the exit arm.
    for (let b = 0; b < 20 && placed < 2; b += 1) {
      const back = 2.4 + b * 0.85;
      for (let c = 0; c < 6 && placed < 2; c += 1) {
        const lat = RING_HALF + 0.9 + (c % 3) * 0.5;
        const side = c < 3 ? -1 : 1;
        const x = signalPath.x + nx * lat * side + tx * back * dir;
        const z = signalPath.z + nz * lat * side + tz * back * dir;
        if (addSignal(x, z, -tx, -tz, B_AVE, record.id, FAMILY_RING)) placed += 1;
      }
    }
    return placed;
  }

  {
    // The avenue mouth is one signalised node with two arms, and the ring meets it once. The
    // entry arm carries the near-side ring pair; the exit arm carries the far-side pair beyond
    // its crossing. One phase governs both arms, so all four ring-facing panels inside the mouth
    // read the same code.
    let entryRingS = Infinity;
    let entryRecord = null;
    let exitRecord = null;
    for (let j = 0; j < junctionRecords.length; j += 1) {
      const record = junctionRecords[j];
      if (record.kind !== 'avenue') continue;
      placeAvenueHeads(record, record.arms[0]);
      const arm = record.arms[0];
      if (!arm) continue;
      if (arm.ringS < entryRingS) {
        entryRingS = arm.ringS;
        entryRecord = record;
      }
    }
    for (let j = 0; j < junctionRecords.length; j += 1) {
      const record = junctionRecords[j];
      if (record.kind !== 'avenue' || record === entryRecord) continue;
      exitRecord = record;
    }
    if (entryRecord) placeRingHeads(entryRecord, entryRecord.arms[0], -1);
    if (exitRecord) placeRingHeads(exitRecord, exitRecord.arms[0], 1);
    // Spur mouths: the ring crossing beside each mouth gets a pair, on the pavement outside the
    // arterial. The ring cars all run one way along the polyline, so both heads of the pair face
    // the oncoming ring traffic, which is how a paired approach is signalled. These are priority
    // mouths: their record is not signalised, so the pair reads green and the spur keeps giving
    // way to the ring on the existing rule.
    for (let s = 0; s < spurPaths.length; s += 1) {
      const path = spurPaths[s];
      const record = junctionRecords[spurRecordBase + s];
      if (!record) continue;
      let best = 0;
      let bd = Infinity;
      for (let i = 0; i <= ringPath.n; i += 1) {
        const dx = ringPath.xs[i] - path.xs[0];
        const dz = ringPath.zs[i] - path.zs[0];
        const d = dx * dx + dz * dz;
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      for (let sgn = -1; sgn <= 1; sgn += 2) {
        const i = best + sgn * 6;
        if (i < 1 || i >= ringPath.n) continue;
        const j = (i + 1) % (ringPath.n + 1);
        const tx = ringPath.xs[j] - ringPath.xs[i];
        const tz = ringPath.zs[j] - ringPath.zs[i];
        const tl = Math.sqrt(tx * tx + tz * tz) || 1;
        let placed = false;
        // the pavement outside the ring first, the inner one only as a fallback
        for (let pass = 0; pass < 3 && !placed; pass += 1) {
          for (let k = 0; k < 10 && !placed; k += 1) {
            const off = RING_HALF + 1.0 + k * 0.45;
            for (let side = 1; side >= -1 && !placed; side -= 2) {
              const x = ringPath.xs[i] + ringPath.nx[i] * off * side;
              const z = ringPath.zs[i] + ringPath.nz[i] * off * side;
              const outward = ringSigned(x, z) >= 0;
              if (pass === 0 && !outward) continue;
              if (pass === 1 && outward) continue;
              placed = addSignal(x, z, -tx / tl, -tz / tl, B_AVE, record.id, FAMILY_RING);
            }
          }
        }
      }
    }
    signals.count = signalCursor;
    if (signals.instanceMatrix) signals.instanceMatrix.needsUpdate = true;
    signalLenses.count = signalCursor * 3;
    if (signalLenses.instanceMatrix) signalLenses.instanceMatrix.needsUpdate = true;
  }

  // What each junction actually got, split by the family of the head. It is a count of what was
  // planted, not a claim about what should have been: a junction with no avenue lamp is visible
  // here instead of only in a screenshot.
  for (let j = 0; j < junctionRecords.length; j += 1) {
    junctionRecords[j].headCount = 0;
    junctionRecords[j].avenueHeads = 0;
    junctionRecords[j].ringHeads = 0;
  }
  for (let i = 0; i < signalCursor; i += 1) {
    const record = junctionRecords[signalHeadJunction[i]];
    if (!record) continue;
    record.headCount += 1;
    if (signalHeadFamily[i] === FAMILY_RING) record.ringHeads += 1;
    else record.avenueHeads += 1;
  }

  // The phase surface agents drive: one slot per built head, a code per slot, and a rewrite of
  // the lens instance colours on demand. World never advances a phase itself. Agents owns the
  // timing and calls apply only when a head actually changes, so the loop below runs once per
  // phase change and not once per frame. The build calls it once so the first frame shows the
  // opening phase instead of the material's white: the avenue mouth opens on the avenue, so its
  // ring heads read red, and a priority mouth reads green because the ring keeps the right of
  // way there.
  //
  // Nothing here allocates: the scratch colour and the instance buffers are created at build,
  // and `setColorAt` writes into the existing array.
  const signalHeads = {
    xs: signalHeadXs.slice(0, signalCursor),
    zs: signalHeadZs.slice(0, signalCursor),
    junction: signalHeadJunction.subarray(0, signalCursor),
    family: signalHeadFamily.subarray(0, signalCursor),
    count: signalCursor
  };
  const signalState = new Float32Array(signalCursor);
  for (let i = 0; i < signalCursor; i += 1) {
    const record = junctionRecords[signalHeadJunction[i]];
    const ringHead = signalHeadFamily[i] === FAMILY_RING;
    signalState[i] = ringHead && record && record.signalised ? 2 : 0;
  }
  const lensCol = new THREE.Color();
  function applySignals() {
    const cnt = signalHeads.count;
    for (let i = 0; i < cnt && i < SIGNAL_CAP; i += 1) {
      // Phase codes: 0 green, 1 amber, 2 red. The lens slots run red, amber, green from the top
      // of the housing down, which is the order a driver reads them.
      const phase = signalState[i] | 0;
      const activeSlot = phase === 0 ? 2 : phase === 1 ? 1 : 0;
      for (let j = 0; j < 3; j += 1) {
        if (j === 0) lensCol.copy(LENS_RED);
        else if (j === 1) lensCol.copy(LENS_AMBER);
        else lensCol.copy(LENS_GREEN);
        if (j !== activeSlot) lensCol.multiplyScalar(SIGNAL_LENS_DIM);
        signalLenses.setColorAt(i * 3 + j, lensCol);
      }
    }
    if (signalLenses.instanceColor) signalLenses.instanceColor.needsUpdate = true;
  }
  applySignals();
  const signalsApi = {
    heads: signalHeads,
    state: signalState,
    apply: applySignals,
    junctions: junctionRecords,
    families: { avenue: FAMILY_AVENUE, ring: FAMILY_RING }
  };

  // ------------------------------------------------------------------ night light
  // A night city reads by its light sources. At district distance a pane is a handful of pixels
  // wide and a lamp a dot, so the whole district collapsed into a dark blue plate with a scatter
  // of specks on it. This block adds the three things that make it a city at night, and it adds
  // them as emissive geometry: no light, no shadow, no per-frame work beyond a slot swap.
  //
  // 1. A halo round every lit window, and a warm disc on every luminaire. The halo is the pane
  //    grown by a fixed margin, so the aggregate reads as a field of warm windows instead of a
  //    grid of pinpricks. It is driven by exactly the same lit state as the pane, which is still
  //    the activity curve from the model: dim, lit and shuttered variants are untouched.
  // 2. Bands of lit floors on the curtain-wall towers, three cells per floor between the
  //    mullions, in runs of two or three floors and deterministic per tower and face.
  // 3. Ribbons of sodium light on the asphalt of the ring, the avenue, the spurs and the grid,
  //    one merged strip with a vertex alpha profile across and along, so a road at night reads
  //    as a continuous warm ribbon and not as a chain of separate pools.
  //
  // Cost: two draw calls, one instanced quad mesh (halos, luminaires, tower cells) and one merged
  // strip mesh (the ribbons). Both are invisible by day, so the daylight frame is byte for byte
  // the frame it was. Nothing here allocates once the world is built.
  const GLOW_VERT = [
    'attribute vec3 aTint;',
    'attribute float aShape;',
    'varying vec2 vUv;',
    'varying vec3 vTint;',
    'varying float vShape;',
    'void main() {',
    '  vUv = uv;',
    '  vTint = aTint;',
    '  vShape = aShape;',
    '  gl_Position = projectionMatrix * modelViewMatrix * instanceMatrix * vec4(position, 1.0);',
    '}'
  ].join('\n');
  const GLOW_FRAG = [
    'varying vec2 vUv;',
    'varying vec3 vTint;',
    'varying float vShape;',
    'void main() {',
    '  vec2 p = vUv * 2.0 - 1.0;',
    '  float a;',
    '  if (vShape > 1.5) {',
    '    // A lit window: the pane plus the spill around it. The skirt stays flat to the edge so the',
    '    // pane still reads as a lit window at district scale, where a window is only a few pixels.',
    '    float d = max(abs(p.x), abs(p.y));',
    '    a = pow(clamp(1.0 - d, 0.0, 1.0), 0.32);',
    '  } else if (vShape > 0.5) {',
    '    // A lit floor cell: full width between the mullions, soft at the slab above and below.',
    '    float ax = clamp(1.0 - abs(p.x), 0.0, 1.0);',
    '    float ay = clamp(1.0 - abs(p.y), 0.0, 1.0);',
    '    a = pow(ax, 0.3) * pow(ay, 0.5);',
    '  } else {',
    '    // A luminaire: a hot core and a soft skirt, the shape of spilled light.',
    '    float r = clamp(length(p), 0.0, 1.0);',
    '    a = pow(1.0 - r, 0.32);',
    '  }',
    '  gl_FragColor = vec4(vTint * a, 1.0);',
    '  #include <colorspace_fragment>',
    '}'
  ].join('\n');

  // The towers' lit floors, resolved here so the instanced sheet can be sized once. Three cells
  // per floor with a gap between them is what a curtain wall does: the mullions stay dark, an
  // occupied floor reads as a band, and the bands come in runs rather than lighting every floor.
  const towerLit = [];
  const TOWER_CELL_INSET = 0.9;
  const TOWER_CELL_GAP = 0.62;
  for (let t = 0; t < towerRecs.length; t += 1) {
    const r = towerRecs[t];
    const hw = r.w * 0.5;
    const hd = r.d * 0.5;
    const ux = Math.cos(r.rotY);
    const uz = -Math.sin(r.rotY);
    const vx = Math.sin(r.rotY);
    const vz = Math.cos(r.rotY);
    const y0 = r.bottom + 0.5;
    const fh = (r.top - y0) / Math.max(1, r.floors);
    const faces = [
      { ax: -vx, az: -vz, cx: r.cx - vx * hd, cz: r.cz - vz * hd, wide: r.w },
      { ax: ux, az: uz, cx: r.cx + ux * hw, cz: r.cz + uz * hw, wide: r.d },
      { ax: vx, az: vz, cx: r.cx + vx * hd, cz: r.cz + vz * hd, wide: r.w },
      { ax: -ux, az: -uz, cx: r.cx - ux * hw, cz: r.cz - uz * hw, wide: r.d }
    ];
    for (let f = 0; f < faces.length; f += 1) {
      const face = faces[f];
      const span = face.wide - 2 * TOWER_CELL_INSET;
      const cellW = (span - 2 * TOWER_CELL_GAP) / 3;
      if (cellW < 1.1) continue;
      const rot = Math.atan2(face.ax, face.az);
      const tx = face.az;
      const tz = -face.ax;
      for (let fl = 0; fl < r.floors; fl += 1) {
        if (isTouch && (fl & 1)) continue;
        // A run of two or three floors goes dark or lit together: offices are let by floor.
        if (hash2(t * 977 + f * 131 + 7, (fl / 2.6) | 0) >= 0.54) continue;
        const y = y0 + fl * fh + fh * 0.54;
        const warm = 0.74 + hash2(t * 41 + f * 17 + 3, fl * 5 + 1) * 0.26;
        for (let c = 0; c < 3; c += 1) {
          const off = (c - 1) * (cellW + TOWER_CELL_GAP);
          towerLit.push({
            x: face.cx + face.ax * 0.19 + tx * off,
            y: y,
            z: face.cz + face.az * 0.19 + tz * off,
            w: cellW,
            h: fh * 0.78,
            rot: rot,
            warm: warm
          });
        }
      }
    }
  }

  const GLOW_HEAD_BASE = towerLit.length;
  const GLOW_DOOR_BASE = GLOW_HEAD_BASE + lampCursor;
  const GLOW_ROOF_BASE = GLOW_DOOR_BASE + doorLamps.length;
  const GLOW_HALO_BASE = GLOW_ROOF_BASE + roofLamps.length;
  const GLOW_CAP = GLOW_HALO_BASE + WINDOW_CAP;
  const glowTint = new Float32Array(GLOW_CAP * 3);
  const glowShape = new Float32Array(GLOW_CAP);
  const haloGeo = new THREE.PlaneGeometry(1, 1);
  const glowTintAttr = new THREE.InstancedBufferAttribute(glowTint, 3);
  const glowShapeAttr = new THREE.InstancedBufferAttribute(glowShape, 1);
  glowTintAttr.setUsage(THREE.DynamicDrawUsage);
  haloGeo.setAttribute('aTint', glowTintAttr);
  haloGeo.setAttribute('aShape', glowShapeAttr);
  const haloMat = new THREE.ShaderMaterial({
    uniforms: {},
    vertexShader: GLOW_VERT,
    fragmentShader: GLOW_FRAG,
    transparent: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    depthTest: true,
    side: THREE.FrontSide,
    fog: false
  });
  const glowMesh = new THREE.InstancedMesh(haloGeo, haloMat, GLOW_CAP);
  glowMesh.name = 'night-glow';
  glowMesh.frustumCulled = false;
  glowMesh.renderOrder = 3;
  glowMesh.visible = false;
  group.add(glowMesh);
  const glowMatrix = glowMesh.instanceMatrix.array;
  let glowUsed = GLOW_HALO_BASE;

  function fillGlowSlot(slot, x, y, z, rotX, rotY, w, h, r, g, b, shape) {
    dummy.position.set(x, y, z);
    dummy.rotation.set(rotX, rotY, 0);
    dummy.scale.set(w, h, 1);
    dummy.updateMatrix();
    glowMesh.setMatrixAt(slot, dummy.matrix);
    glowTint[slot * 3] = r;
    glowTint[slot * 3 + 1] = g;
    glowTint[slot * 3 + 2] = b;
    glowShape[slot] = shape;
  }

  for (let i = 0; i < towerLit.length; i += 1) {
    const c = towerLit[i];
    const k = 0.82 * c.warm;
    fillGlowSlot(i, c.x, c.y, c.z, 0, c.rot, c.w, c.h,
      1.0 * k, 0.66 * k, 0.34 * k, 1);
  }
  // Luminaires: a small warm disc lying on the lamp head, so the light has a visible source and
  // not just a pool. A horizontal disc at this pitch still presents about two thirds of its area.
  for (let i = 0; i < lampCursor; i += 1) {
    const r = lampHeadR[i];
    fillGlowSlot(GLOW_HEAD_BASE + i, lampHeadX[i], lampHeadY[i], lampHeadZ[i],
      -Math.PI / 2, 0, r * 2.8, r * 2.8, 0.92, 0.57, 0.29, 0);
  }
  // Entrance lanterns: upright discs on the frontage, so they read as wall lights and not as
  // holes in the facade.
  for (let i = 0; i < doorLamps.length; i += 1) {
    const d = doorLamps[i];
    fillGlowSlot(GLOW_DOOR_BASE + i, d.x, d.y, d.z, 0, d.rot, 2.0, 1.65,
      0.95 * d.k, 0.58 * d.k, 0.28 * d.k, 0);
  }
  // Roof lamps: lying flat on the roof slab, which at this pitch is the surface the frame sees.
  for (let i = 0; i < roofLamps.length; i += 1) {
    const r = roofLamps[i];
    fillGlowSlot(GLOW_ROOF_BASE + i, r.x, r.y, r.z, -Math.PI / 2, 0, r.s, r.s,
      1.0 * r.k, 0.62 * r.k, 0.31 * r.k, 0);
  }

  // Every lit pane takes a slot, and gives it back when it goes dark. Slots are compacted by
  // moving the last halo into the hole, so the drawn instance count is exactly the number of lit
  // panes: an unlit city costs only the towers and the lamps.
  const haloSlotOf = new Int32Array(WINDOW_CAP);
  haloSlotOf.fill(-1);
  const haloWinOf = new Int32Array(GLOW_CAP);

  function writeHalo(slot, i) {
    const rot = winRot[i];
    const hue = winHue[i];
    const k = 0.72 + hue * 0.24;
    fillGlowSlot(slot,
      winX[i] + Math.sin(rot) * 0.13, winY[i], winZ[i] + Math.cos(rot) * 0.13,
      0, rot, winW[i] * 1.55 + 0.7, winH[i] * 1.35 + 0.6,
      1.0 * k, (0.58 + hue * 0.12) * k, (0.26 + hue * 0.1) * k, 2);
  }

  function copyGlowSlot(to, from) {
    const a = to * 16;
    const b = from * 16;
    for (let k = 0; k < 16; k += 1) glowMatrix[a + k] = glowMatrix[b + k];
    glowTint[to * 3] = glowTint[from * 3];
    glowTint[to * 3 + 1] = glowTint[from * 3 + 1];
    glowTint[to * 3 + 2] = glowTint[from * 3 + 2];
  }

  function setHaloLit(i, on) {
    if (i < 0 || i >= windowCursor) return;
    const slot = haloSlotOf[i];
    if (on) {
      if (slot >= 0 || glowUsed >= GLOW_CAP) return;
      haloSlotOf[i] = glowUsed;
      haloWinOf[glowUsed] = i;
      writeHalo(glowUsed, i);
      glowUsed += 1;
      glowMesh.count = glowUsed;
      glowTintAttr.needsUpdate = true;
      glowMesh.instanceMatrix.needsUpdate = true;
      return;
    }
    if (slot < 0) return;
    const last = glowUsed - 1;
    if (slot !== last) {
      const moved = haloWinOf[last];
      haloWinOf[slot] = moved;
      haloSlotOf[moved] = slot;
      copyGlowSlot(slot, last);
    }
    haloSlotOf[i] = -1;
    haloWinOf[last] = -1;
    glowUsed = last;
    glowMesh.count = glowUsed;
    glowTintAttr.needsUpdate = true;
    glowMesh.instanceMatrix.needsUpdate = true;
  }

  // The ribbons. One merged strip per carriageway, sampled from the same arc length paths the
  // road mesh and the traffic use, so the light follows the asphalt exactly. The cross section is
  // a soft plateau: nothing at the kerb, a wide warm body, a slightly hotter centre line. The
  // vertex alpha carries the three attenuations that keep it honest: a zebra keeps its white
  // paint, the roundabout keeps its markings, and the strip dies out before the frame edge
  // instead of ending at a hard line in the hills.
  // Above the kerb stones, so one sheet covers asphalt and both pavements without a step,
  // and high enough over the paint plane that polygon offset never has to argue with it.
  const RIB_LIFT = LIFT_KERB + 0.02;
  const RIB_OFF = [-1, -0.46, 0, 0.46, 1];
  const RIB_ALPHA = [0.2, 0.85, 1, 0.85, 0.2];
  const ribPos = [];
  const ribCol = [];
  const ribIdx = [];

  function ribStation(path, i, off, halfW, peak, body) {
    const base = ribPos.length / 3;
    for (let a = 0; a < RIB_OFF.length; a += 1) {
      const o = off + RIB_OFF[a] * halfW;
      const x = path.xs[i] + path.nx[i] * o;
      const z = path.zs[i] + path.nz[i] * o;
      const y = groundHeightAt(x, z) + RIB_LIFT;
      let k = RIB_ALPHA[a] * peak * body;
      if (k > 0) {
        const rr = Math.sqrt(x * x + z * z);
        k *= 1 - 0.45 * smooth(95, 250, rr);
        if (nearCrossing(x, z, 1.4)) k *= 0.3;
        else if (inRoundabout(x, z, ROUND_R)) k *= 0.55;
      }
      ribPos.push(x, y, z);
      ribCol.push(1.0 * k, 0.6 * k, 0.29 * k);
    }
    return base;
  }

  function ribJoin(prevBase, base) {
    for (let a = 0; a < RIB_OFF.length - 1; a += 1) {
      const v0 = prevBase + a;
      const v1 = prevBase + a + 1;
      const v2 = base + a + 1;
      const v3 = base + a;
      ribIdx.push(v0, v1, v2, v0, v2, v3);
    }
  }

  function ribbon(path, off, halfW, peak, step, closed, taper) {
    const stride = Math.max(1, Math.round(step / path.step));
    const stations = [];
    for (let i = 0; i < path.n; i += stride) stations.push(i);
    stations.push(closed ? 0 : path.n);
    let prevBase = -1;
    for (let s = 0; s < stations.length; s += 1) {
      const i = stations[s];
      const along = i * path.step;
      let body = 1;
      if (!closed && taper > 0) {
        body = smooth(0, taper, along) * smooth(0, taper, path.len - along);
      }
      const base = ribStation(path, i, off, halfW, peak, body);
      if (prevBase >= 0) ribJoin(prevBase, base);
      prevBase = base;
    }
  }

  const ribStep = isTouch ? 9.0 : 5.0;
  ribbon(ringPath, 0, RING_HALF + RING_PAVE * 0.8, 0.7, ribStep, true, 0);
  ribbon(avePath, AVE_LANE_CENTRE, AVE_LANE_CENTRE - AVE_CAR_IN + 0.7, 0.86, ribStep, false, 9);
  ribbon(avePath, -AVE_LANE_CENTRE, AVE_LANE_CENTRE - AVE_CAR_IN + 0.7, 0.86, ribStep, false, 9);
  for (let i = 0; i < spurPaths.length; i += 1) {
    ribbon(spurPaths[i], 0, SPUR_HALF + SPUR_PAVE * 0.8, 0.7, isTouch ? 8 : 4.5, false, 12);
  }
  // The secondary grid gets the same treatment, dimmer: those streets carry one lamp every
  // twenty four metres rather than the boulevard's double row, and their frontages are lower.
  {
    const gridPeak = 0.65;
    const gridStep = isTouch ? 11 : 5.5;
    for (let pass = 0; pass < 2; pass += 1) {
      const runs = pass === 0 ? streetRunsV : streetRunsH;
      for (let i = 0; i < runs.length; i += 1) {
        const run = runs[i];
        const len = run.t1 - run.t0;
        if (len < 24) continue;
        const count = Math.max(2, Math.ceil(len / gridStep));
        let prevBase = -1;
        for (let s = 0; s <= count; s += 1) {
          const t = run.t0 + (len * s) / count;
          const body = smooth(0, 7, (len * s) / count) * smooth(0, 7, len - (len * s) / count);
          const nx = pass === 0 ? 1 : 0;
          const nz = pass === 0 ? 0 : 1;
          const base = ribPos.length / 3;
          for (let a = 0; a < RIB_OFF.length; a += 1) {
            const o = RIB_OFF[a] * STREET_EDGE;
            const x = (pass === 0 ? run.coord : t) + nx * o;
            const z = (pass === 0 ? t : run.coord) + nz * o;
            const y = groundHeightAt(x, z) + RIB_LIFT;
            let k = RIB_ALPHA[a] * gridPeak * body;
            if (k > 0) {
              const rr = Math.sqrt(x * x + z * z);
              k *= 1 - smooth(106, 186, rr);
              if (nearCrossing(x, z, 1.4)) k *= 0.3;
            }
            ribPos.push(x, y, z);
            ribCol.push(1.0 * k, 0.42 * k, 0.14 * k);
          }
          if (prevBase >= 0) ribJoin(prevBase, base);
          prevBase = base;
        }
      }
    }
  }

  // The lane's own light: one dim warm tread along the near switchback, sampled from the same
  // polyline the terrain paints the track onto and laid on the drawn surface height, so the
  // bottom of the portrait frame gets a lit line instead of a dark slope. Three columns and a
  // two metre half width, tapered where the lane leaves the ring road and again at the frame
  // edge so it never ends in a hard stroke. It rides in the night road sheet, so it adds no
  // draw call; it costs 256 triangles.
  {
    const laneSurf = terrainMesh.userData.surfaceHeightAt;
    const LANE_COLS = [-1, 0, 1];
    const LANE_ALPHA = [0.24, 1, 0.24];
    const LANE_HALF = 2.0;
    const LANE_PEAK = 0.42;
    let laneLen = 0;
    for (let s = 0; s < LANE_PATH.length - 1; s += 1) {
      laneLen += Math.sqrt(
        (LANE_PATH[s + 1][0] - LANE_PATH[s][0]) * (LANE_PATH[s + 1][0] - LANE_PATH[s][0]) +
        (LANE_PATH[s + 1][1] - LANE_PATH[s][1]) * (LANE_PATH[s + 1][1] - LANE_PATH[s][1])
      );
    }
    let prevBase = -1;
    let along = 0;
    for (let s = 0; s < LANE_PATH.length - 1; s += 1) {
      const ax0 = LANE_PATH[s][0];
      const az0 = LANE_PATH[s][1];
      const dx = LANE_PATH[s + 1][0] - ax0;
      const dz = LANE_PATH[s + 1][1] - az0;
      const len = Math.sqrt(dx * dx + dz * dz);
      if (len < 0.5) continue;
      const nx = -dz / len;
      const nz = dx / len;
      const steps = Math.max(1, Math.ceil(len / 7));
      for (let k = s === 0 ? 0 : 1; k <= steps; k += 1) {
        const u = k / steps;
        const cx = ax0 + dx * u;
        const cz = az0 + dz * u;
        const at = along + len * u;
        const body = smooth(0, 40, at) * smooth(0, 18, laneLen - at);
        const base = ribPos.length / 3;
        for (let a = 0; a < LANE_COLS.length; a += 1) {
          const o = LANE_COLS[a] * LANE_HALF;
          const x = cx + nx * o;
          const z = cz + nz * o;
          const y = laneSurf(x, z) + 0.12;
          const kk = LANE_ALPHA[a] * LANE_PEAK * body;
          ribPos.push(x, y, z);
          ribCol.push(1.0 * kk, 0.58 * kk, 0.26 * kk);
        }
        if (prevBase >= 0) {
          for (let a = 0; a < LANE_COLS.length - 1; a += 1) {
            const v0 = prevBase + a;
            const v1 = prevBase + a + 1;
            const v2 = base + a + 1;
            const v3 = base + a;
            ribIdx.push(v0, v1, v2, v0, v2, v3);
          }
        }
        prevBase = base;
      }
      along += len;
    }
  }

  const ribGeo = new THREE.BufferGeometry();
  ribGeo.setAttribute('position', new THREE.Float32BufferAttribute(ribPos, 3));
  ribGeo.setAttribute('aTint', new THREE.Float32BufferAttribute(ribCol, 3));
  ribGeo.setIndex(ribIdx);
  ribGeo.computeBoundingSphere();
  const ribMat = new THREE.ShaderMaterial({
    uniforms: {},
    vertexShader: [
      'attribute vec3 aTint;',
      'varying vec3 vTint;',
      'void main() {',
      '  vTint = aTint;',
      '  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);',
      '}'
    ].join('\n'),
    fragmentShader: [
      'varying vec3 vTint;',
      'void main() {',
      '  gl_FragColor = vec4(vTint, 1.0);',
      '  #include <colorspace_fragment>',
      '}'
    ].join('\n'),
    transparent: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    depthTest: true,
    side: THREE.DoubleSide,
    polygonOffset: true,
    polygonOffsetFactor: -2,
    polygonOffsetUnits: -2,
    fog: false
  });
  const ribbonMesh = new THREE.Mesh(ribGeo, ribMat);
  ribbonMesh.name = 'night-road-glow';
  ribbonMesh.frustumCulled = false;
  ribbonMesh.renderOrder = 3;
  ribbonMesh.visible = false;
  group.add(ribbonMesh);

  glowMesh.count = glowUsed;
  glowTintAttr.needsUpdate = true;
  glowShapeAttr.needsUpdate = true;
  glowMesh.instanceMatrix.needsUpdate = true;

  let nightGlowOn = false;
  // One switch for the whole night pass, driven by the same night flag the lamps and the dome
  // follow: by day neither mesh is submitted at all.
  function setNightGlow(on) {
    const want = !!on;
    if (want === nightGlowOn) return;
    nightGlowOn = want;
    glowMesh.visible = want;
    ribbonMesh.visible = want;
  }

  // ------------------------------------------------------------------ tram stops
  const tramStops = [];
  {
    const stops = (model && model.stops) || [];
    const point = new THREE.Vector3();
    const SAMPLES = 160;
    for (let i = 0; i < stops.length; i += 1) {
      const q = stops[i].q;
      const target = (q >= 0 && q < quads.length) ? quads[q].center : null;
      let bestU = (i / Math.max(1, stops.length)) % 1;
      let bestD = Infinity;
      if (target) {
        for (let s = 0; s < SAMPLES; s += 1) {
          const u = s / SAMPLES;
          tramCurve.getPointAt(u, point);
          const dx = point.x - target.x;
          const dz = point.z - target.z;
          const d = dx * dx + dz * dz;
          if (d < bestD) {
            bestD = d;
            bestU = u;
          }
        }
      }
      tramCurve.getPointAt(bestU, point);
      tramStops.push({
        position: new THREE.Vector3(point.x, groundHeightAt(point.x, point.z), point.z),
        u: bestU,
        stopIdx: i
      });
    }
  }

  // ------------------------------------------------------------------ standing crowds
  // The furniture tells you the street is used; people tell you it is inhabited. One pool of
  // pack mini characters stands at the spots a district viewer reads first: stall keepers and
  // a few buyers on the market rows, the cafe tables, two fountain benches, the bus shelters,
  // the church forecourt and two avenue corners. Every figure is an instanced character model
  // standing on the same ground heights the furniture uses, facing the thing it stands beside,
  // and every spot is guarded by the same occupancy rule the props used, so a figure can never
  // clip a stall, a cart, a lamppost or a crossing. The meshes are separate from the tintable
  // facade list, so the quarterly desaturation never repaints them, and they are not pickable:
  // a tap on a person is a tap past them. They cast no shadow and carry no audit cost beyond
  // their instance count, which the crowd audit reports.
  const CROWD_FACE = 0;
  const crowdAudit = {
    variants: 0,
    people: 0,
    tris: 0,
    tried: 0,
    dropped: 0,
    bySpot: { market: 0, cafe: 0, benches: 0, shelters: 0, forecourt: 0, corners: 0 },
    models: []
  };
  {
    const crowdMask = B_BUILD | B_PROP | B_TRAM;
    const crowdPeople = assets ? (assets.byCategory('person') || []) : [];
    const crowdFree = function (x, z, r) {
      if (nearCrossing(x, z, r + 0.5)) return false;
      return diskFree(x, z, r, crowdMask);
    };
    const crowdSpots = [];
    const crowdAt = function (x, z, rotY, lift, spot, r) {
      crowdAudit.tried += 1;
      if (!crowdFree(x, z, r)) {
        crowdAudit.dropped += 1;
        return;
      }
      crowdSpots.push({ x: x, y: groundHeightAt(x, z) + lift, z: z, rotY: rotY, spot: spot });
      crowdAudit.bySpot[spot] += 1;
    };
    // Market: one keeper behind every stall that actually stands (the stall's own centre claim
    // is the acceptance test, the free disk guard keeps the keeper clear of crates and carts),
    // plus a buyer in the aisle on about three stalls in ten.
    {
      const crowdRowX = [-9, -6, -3, 0, 3, 6, 9];
      const crowdRowJit = [-0.12, 0.15, -0.06, 0.1, -0.15, 0.05, 0.13];
      for (let row = 0; row < 2; row += 1) {
        const face = row === 0 ? 1 : -1;
        for (let k = 0; k < crowdRowX.length; k += 1) {
          const sx = PLAZA_CX + crowdRowX[k];
          const sz = PLAZA_CZ + face * -5.7 + crowdRowJit[k];
          if (!(flagsAt(sx, sz) & B_PROP)) continue;
          const jit = (hash2(k * 31 + row, 57) - 0.5) * 0.9;
          crowdAt(sx + jit, sz - face * 1.8, (face === 1 ? 0 : Math.PI) + CROWD_FACE,
            LIFT_PLAZA, 'market', 0.35);
          if (hash2(k * 17 + row, 59) < 0.3) {
            crowdAt(sx + jit * 0.6, sz + face * 1.85,
              (face === 1 ? Math.PI : 0) + CROWD_FACE, LIFT_PLAZA, 'market', 0.35);
          }
        }
      }
    }
    // Cafe tables: one figure on the plaza side of each table, facing it, with a second on
    // about half the tables along the table's other axis.
    {
      const crowdTerraces = [[-10.4, -3], [-10.4, 3], [10.4, -3], [10.4, 3]];
      for (let i = 0; i < crowdTerraces.length; i += 1) {
        const tx = PLAZA_CX + crowdTerraces[i][0];
        const tz = PLAZA_CZ + crowdTerraces[i][1];
        const side = tx < PLAZA_CX ? 1 : -1;
        crowdAt(tx + side * 1.6, tz, (side === 1 ? -Math.PI / 2 : Math.PI / 2) + CROWD_FACE,
          LIFT_PLAZA, 'cafe', 0.3);
        if (hash2(i, 61) < 0.5) {
          const zs = hash2(i, 63) < 0.5 ? 1.6 : -1.6;
          crowdAt(tx, tz + zs, (zs > 0 ? Math.PI : 0) + CROWD_FACE, LIFT_PLAZA, 'cafe', 0.3);
        }
      }
    }
    // Fountain benches: one figure behind two of the four benches, facing the water.
    {
      const crowdAngles = [0.62, Math.PI + 0.62];
      for (let i = 0; i < crowdAngles.length; i += 1) {
        const a = crowdAngles[i];
        crowdAt(PLAZA_CX + Math.cos(a) * 6.9, PLAZA_CZ + Math.sin(a) * 6.9,
          Math.atan2(-Math.cos(a), -Math.sin(a)) + CROWD_FACE, LIFT_PLAZA, 'benches', 0.35);
      }
    }
    // Bus shelters: the same candidate loop the shelter pass ran, re-tested against the claim
    // the standing shelter left, so a figure only appears where a shelter actually stands. One
    // figure to the side of the glass, facing in, a second alongside when the hash allows.
    {
      let shelterCrowd = 0;
      for (let i = 20; i < avePath.n - 20 && shelterCrowd < 3; i += 26) {
        const off = (shelterCrowd % 2 === 0 ? 1 : -1) * (AVE_PAVE_OFF + 0.35);
        const x = avePath.xs[i] + avePath.nx[i] * off;
        const z = avePath.zs[i] + avePath.nz[i] * off;
        if (!(flagsAt(x, z) & B_PROP)) continue;
        shelterCrowd += 1;
        const tx = avePath.nz[i];
        const tz = -avePath.nx[i];
        const fx = Math.cos(Math.atan2(-tz, tx));
        const fz = -Math.sin(Math.atan2(-tz, tx));
        const rx = -fz;
        const rz = fx;
        for (let s = 0; s < 2; s += 1) {
          if (s === 1 && hash2(i, 67) < 0.5) continue;
          const sgn = s === 0 ? 1 : -1;
          crowdAt(x + rx * sgn * 2.55 + fx * 0.9, z + rz * sgn * 2.55 + fz * 0.9,
            Math.atan2(-rx * sgn, -rz * sgn) + CROWD_FACE, 0, 'shelters', 0.35);
        }
      }
    }
    // The church forecourt: three figures on the open paving south of the church steps,
    // facing the church.
    {
      const forecourt = [[-47.5, -93], [-44.5, -92.6], [-50.5, -93.4]];
      for (let i = 0; i < forecourt.length; i += 1) {
        crowdAt(forecourt[i][0], forecourt[i][1], Math.PI + CROWD_FACE, 0, 'forecourt', 0.45);
      }
    }
    // Two avenue corners: a figure on the pavement kerb, facing the carriageway.
    {
      const cornerIdx = [44, 118];
      for (let ci = 0; ci < cornerIdx.length; ci += 1) {
        const i = cornerIdx[ci];
        const side = hash2(i, 65) < 0.5 ? 1 : -1;
        const off = (AVE_HALF + 0.9) * side;
        crowdAt(avePath.xs[i] + avePath.nx[i] * off, avePath.zs[i] + avePath.nz[i] * off,
          Math.atan2(-side * avePath.nx[i], -side * avePath.nz[i]) + CROWD_FACE, 0,
          'corners', 0.4);
      }
    }
    if (crowdSpots.length > 0 && crowdPeople.length > 0) {
      // The touch frame takes every other variant so its pool stays small without losing the
      // mix; the desktop frame takes all of them.
      const variantIdx = [];
      for (let v = 0; v < crowdPeople.length; v += 1) {
        if (!NARROW_VIEW || v % 2 === 0) variantIdx.push(v);
      }
      crowdAudit.variants = variantIdx.length;
      const perVariant = new Array(variantIdx.length);
      for (let v = 0; v < variantIdx.length; v += 1) perVariant[v] = 0;
      const entries = [];
      for (let s = 0; s < crowdSpots.length; s += 1) {
        const v = Math.min(variantIdx.length - 1,
          Math.floor(hash2(s * 13 + 3, 71) * variantIdx.length));
        entries.push({ spot: crowdSpots[s], v: v });
        perVariant[v] += 1;
      }
      const tintScratch = new THREE.Color();
      for (let v = 0; v < variantIdx.length; v += 1) {
        if (perVariant[v] === 0) continue;
        const made = modelMesh('person', { model: crowdPeople[variantIdx[v]], count: perVariant[v] });
        if (!made) continue;
        made.mesh.frustumCulled = false;
        let cursor = 0;
        for (let s = 0; s < entries.length; s += 1) {
          if (entries[s].v !== v) continue;
          const p = entries[s].spot;
          const sc = 2.0 + hash2(s * 13 + 3, 71) * 0.4;
          dummy.position.set(p.x, p.y, p.z);
          dummy.rotation.set(0, p.rotY, 0);
          dummy.scale.set(sc, sc, sc);
          dummy.updateMatrix();
          made.mesh.setMatrixAt(cursor, dummy.matrix);
          const tone = 0.88 + hash2(s * 17 + 9, 73) * 0.24;
          tintScratch.setRGB(tone, tone, tone);
          made.mesh.setColorAt(cursor, tintScratch);
          cursor += 1;
        }
        made.mesh.count = cursor;
        made.mesh.instanceMatrix.needsUpdate = true;
        if (made.mesh.instanceColor) made.mesh.instanceColor.needsUpdate = true;
        group.add(made.mesh);
        if (NARROW_VIEW) nightCullTouch.push(made.mesh);
        crowdAudit.models.push({ name: made.model.name, count: cursor, tris: made.model.tris });
        crowdAudit.tris += made.model.tris * cursor;
        crowdAudit.people += cursor;
      }
    }
  }

  flushDetails();

  // ------------------------------------------------------------------ house roofs
  // Every house carries its own two tile slopes, in its own colour, riding the static props
  // mesh: one merged mesh can hold a different roof colour per house, which the single
  // instanced colour of the house body cannot.
  for (let i = 0; i < houseSpots.length; i += 1) {
    const hs = houseSpots[i];
    statics.addGeometry(houseRoofGeo,
      composeMatrix(hs.x, hs.y, hs.z, hs.yaw, hs.w, hs.h, hs.d), hs.roof);
  }

  const staticsMesh = new THREE.Mesh(statics.build(), new THREE.MeshStandardMaterial({
    vertexColors: true,
    roughness: 0.78,
    metalness: 0.06
  }));
  staticsMesh.castShadow = shadowsOn;
  staticsMesh.receiveShadow = shadowsOn;
  staticsMesh.name = 'statics';
  group.add(staticsMesh);

  // ------------------------------------------------------------------ sky, haze and light
  const SKY_R = 1300;
  const skyUniforms = {
    uTop: { value: new THREE.Color(0x5c9bd6) },
    uHorizon: { value: new THREE.Color(0xf1d0a4) },
    uHaze: { value: new THREE.Color(0xe8dcc4) },
    uSunDir: { value: new THREE.Vector3(0.84, 0.42, 0.34).normalize() },
    uGlow: { value: new THREE.Color(0xffcf8a) },
    // Direction of the town on the horizon, and the warm lift the district throws up into the
    // sky above it. Both are uniform values only: no geometry, no program, no draw call.
    uCityDir: { value: new THREE.Vector3(0, 0, -1) },
    uCityGlow: { value: new THREE.Color(0x000000) }
  };
  const skyMat = new THREE.ShaderMaterial({
    uniforms: skyUniforms,
    vertexShader: [
      'varying vec3 vWorld;',
      'void main() {',
      '  vec4 w = modelMatrix * vec4(position, 1.0);',
      '  vWorld = w.xyz;',
      '  gl_Position = projectionMatrix * viewMatrix * w;',
      '}'
    ].join('\n'),
    fragmentShader: [
      'uniform vec3 uTop;',
      'uniform vec3 uHorizon;',
      'uniform vec3 uHaze;',
      'uniform vec3 uSunDir;',
      'uniform vec3 uGlow;',
      'uniform vec3 uCityDir;',
      'uniform vec3 uCityGlow;',
      'varying vec3 vWorld;',
      'void main() {',
      '  vec3 dir = normalize(vWorld - cameraPosition);',
      '  float h = dir.y;',
      '  float k = smoothstep(-0.05, 0.55, h);',
      '  vec3 col = mix(uHorizon, uTop, pow(k, 0.62));',
      // A dust band hugging the horizon, thicker along the sun azimuth so the haze has a side.
      '  float az = max(dot(normalize(vec3(dir.x, 0.0, dir.z)),',
      '    normalize(vec3(uSunDir.x, 0.0, uSunDir.z))), 0.0);',
      '  float band = exp(-max(h, 0.0) * 7.0) * (0.35 + 0.65 * az * az);',
      '  col = mix(col, uHaze, band * 0.55);',
      // The town lifts its own sky: a faint warm band on the horizon it sits under, so the dome
      // is never one flat plate of navy. Zero by day, a few percent of the dome's own value at
      // night, and it follows the camera round so it always points at the district.
      '  float cz = max(dot(normalize(vec3(dir.x, 0.0, dir.z)),',
      '    normalize(vec3(uCityDir.x, 0.0, uCityDir.z))), 0.0);',
      '  float city = exp(-max(h, 0.0) * 6.0) * (0.3 + 0.7 * cz * cz);',
      // Sun: a wide bloom, a mid halo and a small core, faded out below the horizon.
      '  float s = max(dot(dir, uSunDir), 0.0);',
      '  float above = smoothstep(-0.07, 0.14, h);',
      '  col += uGlow * (pow(s, 3.0) * 0.14 + pow(s, 24.0) * 0.32 + pow(s, 200.0) * 0.95) * above;',
      '  col += uGlow * band * pow(az, 8.0) * 0.32;',
      '  col += uCityGlow * city;',
      '  gl_FragColor = vec4(col, 1.0);',
      '  #include <colorspace_fragment>',
      '}'
    ].join('\n'),
    side: THREE.BackSide,
    depthWrite: false,
    depthTest: false,
    fog: false
  });
  const sky = new THREE.Mesh(new THREE.SphereGeometry(SKY_R, 32, 16), skyMat);
  sky.name = 'sky';
  sky.renderOrder = -1000;
  sky.frustumCulled = false;
  group.add(sky);
  if (NARROW_VIEW) nightCullTouch.push(sky);

  // Haze: a last veil over the water and the far shore, and nothing else. The portrait frame
  // pulls the camera back to a 652 metre radius, so a fog keyed on camera distance sits on the
  // town itself at that tier: it was the fog that greyed every wall and flattened the whole top
  // of the phone frame in the first pass of this slice. The air that carries the distance is the
  // aerial tint in the terrain's own vertex colours, keyed on distance from the district; this
  // fog only softens the last four hundred metres, where geometry can no longer resolve.
  const FOG_NEAR = 1100;
  const FOG_FAR = 2800;
  const fog = new THREE.Fog(0xe9d9b9, FOG_NEAR, FOG_FAR);
  group.addEventListener('added', function () {
    const parent = group.parent;
    if (parent && parent.isScene) parent.fog = fog;
  });

  const bounds = {
    minX: -SPAN,
    maxX: SPAN,
    minZ: -SPAN,
    maxZ: SPAN,
    center: new THREE.Vector3(0, 12, 4),
    radius: 114
  };

  // The sky is the first thing drawn every frame, so it is also where a shadow map rebuild is
  // requested: the renderer is only reachable here, and a resize happens between frames.
  let shadowDirty = false;
  const cityDir = new THREE.Vector3();
  sky.onBeforeRender = function (renderer, scene, camera) {
    const dx = camera.position.x - bounds.center.x;
    const dy = camera.position.y - bounds.center.y;
    const dz = camera.position.z - bounds.center.z;
    // The horizon glow follows the district: as the camera snaps round the ring the warm band
    // keeps pointing at the town it rises from. A uniform write, no allocation, no rebuild.
    cityDir.set(-dx, 0, -dz);
    if (cityDir.lengthSq() > 1e-6) {
      cityDir.normalize();
      skyUniforms.uCityDir.value.copy(cityDir);
    }
    if (shadowDirty && renderer && renderer.shadowMap) {
      // 0 means the shadow pass is off, any size means it is on: the flag and the renderer must
      // agree, or a quality step would raise the map and draw nothing.
      renderer.shadowMap.enabled = shadowSize > 0;
      renderer.shadowMap.needsUpdate = true;
      shadowDirty = false;
    }
  };

  // Golden hour: a strong low sun and a weaker, warm sky ambient. The ambient is what decides
  // whether the frame keeps its hues or greys out, so it is deliberately under the sun here: a
  // strong dome at full strength turned every warm wall into the same khaki. It is warm cream
  // rather than pale blue because the bounce under a Mediterranean afternoon comes off ochre
  // walls and dry earth.
  const hemi = new THREE.HemisphereLight(0xdedad2, 0x86817a, 0.95);
  group.add(hemi);

  const MOON_DIR = new THREE.Vector3(-0.45, 0.72, 0.4).normalize();
  const SUN_TARGET = new THREE.Vector3(0, 10, 0);
  const SUN_DIST = 460;

  // The shadow camera is framed for the whole district once, here, whether or not the profile
  // starts with shadows on: the quality controller can switch the map back on at any size and
  // must get a correctly framed shadow from the first frame.
  const sun = new THREE.DirectionalLight(0xffdcb0, 3.1);
  sun.shadow.camera.left = -178;
  sun.shadow.camera.right = 178;
  sun.shadow.camera.top = 178;
  sun.shadow.camera.bottom = -178;
  sun.shadow.camera.near = 120;
  sun.shadow.camera.far = 820;
  sun.shadow.bias = -0.0006;
  sun.shadow.normalBias = 0.5;
  if (shadowsOn) {
    sun.castShadow = true;
    sun.shadow.mapSize.set(1024, 1024);
  }
  sun.position.copy(SUN_DIR).multiplyScalar(SUN_DIST).add(SUN_TARGET);
  sun.target.position.copy(SUN_TARGET);
  group.add(sun);
  group.add(sun.target);

  // Day is a hazy golden hour; night is a deep blue that never goes black. The day fog is the
  // same pale warm lilac as the air over the far country, set a little deeper than the sky's own
  // haze band so the outermost crests still step back from the horizon instead of dissolving
  // into it. It is a veil on the last four hundred metres, never a wash on the town: FOG_NEAR
  // stays at 1100. Night keeps its own navy palette untouched.
  // Clear pale blue overhead, warm cream at the horizon, and a cream haze band between them.
  // The old haze and fog were lilac, which mixed with the warm sun into the purple band over
  // the far hills; both are now the horizon's own cream, so the air and the sky agree.
  const DAY_TOP = new THREE.Color(0x92b9d8);
  const DAY_HORIZON = new THREE.Color(0xf4dcb4);
  const DAY_HAZE = new THREE.Color(0xf0e2c2);
  const DAY_GLOW = new THREE.Color(0xffd08c);
  const DAY_FOG = new THREE.Color(0xe9d9b9);
  const NIGHT_TOP = new THREE.Color(0x0a1024);
  const NIGHT_HORIZON = new THREE.Color(0x142142);
  const NIGHT_HAZE = new THREE.Color(0x1d3055);
  const NIGHT_GLOW = new THREE.Color(0x2a3a63);
  const NIGHT_FOG = new THREE.Color(0x1a2745);
  // A few percent of warm light lifted into the lowest band of the dome, in the direction of
  // the town: the district's own sodium, seen on its own horizon.
  const NIGHT_CITY_GLOW = new THREE.Color(0xffa460).multiplyScalar(0.012);

  // Shadow map size control. The quality controller walks the ladder down and back up on the
  // same light, so the shadow has to be able to come back: the previous render target is
  // disposed and nulled, and the renderer is asked for one refresh through the sky hook.
  // Nothing is allocated when the size does not actually change.
  const SHADOW_LEVELS = [0, 512, 1024, 2048, 4096];
  let shadowSize = shadowsOn ? 1024 : 0;

  function setShadowMapSize(n) {
    if (typeof n !== 'number' || !isFinite(n)) return shadowSize;
    let want = 0;
    for (let i = 0; i < SHADOW_LEVELS.length; i += 1) {
      if (SHADOW_LEVELS[i] >= n) {
        want = SHADOW_LEVELS[i];
        break;
      }
    }
    if (want > 0) {
      // Only powers of two up to the cap are accepted; anything else falls back down a step.
      let p = 512;
      while (p * 2 <= want) p *= 2;
      want = Math.min(4096, p);
    }
    if (want === shadowSize) return shadowSize;
    shadowSize = want;
    if (want === 0) {
      sun.castShadow = false;
      if (sun.shadow.map) {
        sun.shadow.map.dispose();
        sun.shadow.map = null;
      }
      sun.shadow.mapSize.set(1, 1);
      shadowDirty = true;
      return shadowSize;
    }
    if (sun.shadow.map) {
      sun.shadow.map.dispose();
      sun.shadow.map = null;
    }
    sun.shadow.mapSize.set(want, want);
    sun.castShadow = true;
    sun.shadow.needsUpdate = true;
    shadowDirty = true;
    return shadowSize;
  }

  const lights = {
    sun: sun,
    hemi: hemi,
    setShadowMapSize: setShadowMapSize,
    // Day keeps the late-afternoon look at every daylight hour: warm sun from the right at
    // about 25 degrees, long shadows. Night is a deep blue that never goes black: the ambient
    // and the ground bounce are both blue dominant, the moon is a cool slate, and the fog takes
    // the haze to navy, so the dark half of the frame cannot settle into grey-green.
    setTimeOfDay: function (hour, night) {
      const h = ((hour % 24) + 24) % 24;
      if (night) {
        sun.position.copy(MOON_DIR).multiplyScalar(SUN_DIST).add(SUN_TARGET);
        sun.color.setHex(0x93a9dd);
        sun.intensity = 0.68;
        hemi.color.setHex(0x1d2b4a);
        hemi.groundColor.setHex(0x141a2b);
        hemi.intensity = 1.55;
        skyUniforms.uTop.value.copy(NIGHT_TOP);
        skyUniforms.uHorizon.value.copy(NIGHT_HORIZON);
        skyUniforms.uHaze.value.copy(NIGHT_HAZE);
        skyUniforms.uGlow.value.copy(NIGHT_GLOW);
        skyUniforms.uSunDir.value.copy(MOON_DIR);
        skyUniforms.uCityGlow.value.copy(NIGHT_CITY_GLOW);
        fog.color.copy(NIGHT_FOG);
        setNightGlow(true);
      } else {
        // A touch warmer toward the evening hours, never a different direction.
        const warm = smooth(12, 18, h);
        sun.position.copy(SUN_DIR).multiplyScalar(SUN_DIST).add(SUN_TARGET);
        colorScratch.setHex(0xffe6c4);
        colorScratch2.setHex(0xffcb90);
        sun.color.copy(colorScratch).lerp(colorScratch2, warm);
        // The shade deepened rather than the frame darkened: the sun carries eight percent more
        // light and the sky dome twenty one percent less, so a lit wall keeps its value while a
        // shaded flank and every cast shadow step down. Measured on the day frame: dark share
        // 7.5 to 9.6 percent under the midpoint test, mean value 0.335 to 0.325, bands inside
        // 0.2 of where they were, warm 52.6 to 54.7, cool 17.8 to 18.2.
        sun.intensity = 3.9;
        // The shade is not blue. Under this sky the bounce onto a shaded flank comes from ochre
        // walls, dry earth and dust, so what fills a shadow is warm neutral rather than a pale
        // blue dome: 0xdedad2 at hue 40 instead of 0xd8e4f0 at 210. Measured on the day frame,
        // the blue dome left every shadow at chroma 0.36 while the lit town sat at 0.45, and the
        // third tenth of the frame, half of which is the four towers' long shadows, could not
        // pass 0.40. The ambient chroma is deliberately small (0.054 against the old 0.100), so
        // the cool contrast that makes the sun read warm survives: the whole frame's blue share
        // moved 15.1 to 13.6 percent and its warm share 60.5 to 62.8. The value is held at 0.96
        // of the old dome's luminance, so a shaded wall keeps its darkness and only changes hue.
        hemi.color.setHex(0xdedad2);
        hemi.groundColor.setHex(0x86817a);
        hemi.intensity = 0.75;
        skyUniforms.uTop.value.copy(DAY_TOP);
        skyUniforms.uHorizon.value.copy(DAY_HORIZON);
        skyUniforms.uHaze.value.copy(DAY_HAZE);
        skyUniforms.uGlow.value.copy(DAY_GLOW);
        skyUniforms.uSunDir.value.copy(SUN_DIR);
        skyUniforms.uCityGlow.value.setHex(0x000000);
        fog.color.copy(DAY_FOG);
        setNightGlow(false);
      }
      // What the night cannot resolve is drawn at dawn and dropped again at dusk: the window
      // frame rings, the ground contact blobs, the crane masts and the stone crown. Symmetric
      // boolean writes, no allocation, and the day frame keeps every one of them.
      for (let i = 0; i < nightCull.length; i += 1) nightCull[i].visible = !night;
      for (let i = 0; i < nightCullTouch.length; i += 1) nightCullTouch[i].visible = !night;
      nightNow = night;
      applyDetailCount();
      sun.target.position.copy(SUN_TARGET);
      sun.target.updateMatrixWorld();
      paintMat.emissiveIntensity = night ? PAINT_NIGHT_GLOW : PAINT_DAY_GLOW;
    }
  };
  lights.setTimeOfDay(16, false);

  // ------------------------------------------------------------------ signs
  // One atlas and one mesh for every lettered surface in the city: the quarter nameplates and
  // the shop fascia boards. Shop slots carry their own quad size, so a 2.8 by 0.78 m fascia and
  // a 5.2 by 2.3 m quarter plate share the same texture and the same draw call.
  const ATLAS_COLS = 6;
  const ATLAS_ROWS = 9;
  const CELL_W = 160;
  const CELL_H = 80;
  const SLOT_CAP = ATLAS_COLS * ATLAS_ROWS;
  // The quarter plates keep the twenty four cells they always had; the ninth atlas row is the
  // market's two name boards.
  const QUARTER_SIGN_CAP = 24;
  const SHOP_SIGN_CAP = Math.min(24, shopSigns.length);

  const atlasCanvas = (typeof document !== 'undefined' && document && document.createElement)
    ? document.createElement('canvas')
    : null;
  let atlasCtx = null;
  let atlasTexture = null;
  if (atlasCanvas) {
    atlasCanvas.width = ATLAS_COLS * CELL_W;
    atlasCanvas.height = ATLAS_ROWS * CELL_H;
    atlasCtx = atlasCanvas.getContext('2d');
    if (atlasCtx) {
      atlasCtx.fillStyle = '#f2e8d5';
      atlasCtx.fillRect(0, 0, atlasCanvas.width, atlasCanvas.height);
      atlasTexture = new THREE.CanvasTexture(atlasCanvas);
      atlasTexture.colorSpace = THREE.SRGBColorSpace;
    }
  }
  if (!atlasTexture) {
    atlasTexture = new THREE.DataTexture(
      new Uint8Array([242, 232, 213, 255]),
      1,
      1,
      THREE.RGBAFormat
    );
    atlasTexture.needsUpdate = true;
  }

  const slotAssignments = [];
  for (let qi = 0; qi < quads.length; qi += 1) {
    const slots = quads[qi].signSlots;
    for (let s = 0; s < slots.length && slotAssignments.length < QUARTER_SIGN_CAP; s += 1) {
      slotAssignments.push({
        quarter: qi, slot: s, position: slots[s].position, rotationY: slots[s].rotationY,
        w: 5.2, h: 2.3, shop: false
      });
    }
  }
  // Shop fascia boards: the ones closest to the centre of the district take the lettered slots,
  // so the camera's own landing zone carries the named shops while the rest of the avenue keeps
  // its painted board. Sorting is stable and deterministic, by squared distance to the centre.
  if (SHOP_SIGN_CAP > 0) {
    const order = [];
    for (let i = 0; i < shopSigns.length; i += 1) {
      const s = shopSigns[i];
      const dx = s.x - bounds.center.x;
      const dz = s.z - bounds.center.z;
      order.push({ i: i, d: dx * dx + dz * dz });
    }
    order.sort(function (a, b) { return a.d - b.d; });
    for (let k = 0; k < SHOP_SIGN_CAP && slotAssignments.length < SLOT_CAP; k += 1) {
      const s = shopSigns[order[k].i];
      slotAssignments.push({
        quarter: -1, slot: 0, position: { x: s.x, y: s.y, z: s.z }, rotationY: s.rotY,
        w: s.w, h: s.h, shop: true, name: s.name
      });
    }
  }
  // The market's two name boards, painted once at build time like the shop fascias. Their
  // quarter is -1, so the agents' phrase pass never cycles them and no upload follows.
  for (let i = 0; i < marketBoards.length && slotAssignments.length < SLOT_CAP; i += 1) {
    const b = marketBoards[i];
    slotAssignments.push({
      quarter: -1, slot: 0, position: { x: b.x, y: b.y, z: b.z }, rotationY: b.rotY,
      w: b.w, h: b.h, shop: true, name: b.name
    });
  }
  const signSlotCount = slotAssignments.length;
  const signSlotQuarter = new Int16Array(Math.max(1, signSlotCount));
  const signSlotShop = new Uint8Array(Math.max(1, signSlotCount));
  for (let i = 0; i < signSlotCount; i += 1) {
    signSlotQuarter[i] = slotAssignments[i].quarter;
    signSlotShop[i] = slotAssignments[i].shop ? 1 : 0;
  }

  const signPositions = new Float32Array(Math.max(1, signSlotCount) * 4 * 3);
  const signUvs = new Float32Array(Math.max(1, signSlotCount) * 4 * 2);
  const signIndices = new Uint32Array(Math.max(1, signSlotCount) * 6);
  for (let i = 0; i < signSlotCount; i += 1) {
    const a = slotAssignments[i];
    const tx = Math.cos(a.rotationY);
    const tz = -Math.sin(a.rotationY);
    const px = a.position.x;
    const py = a.position.y;
    const pz = a.position.z;
    const sw = a.w * 0.5;
    const sh = a.h * 0.5;
    const base = i * 12;
    signPositions[base] = px - tx * sw;
    signPositions[base + 1] = py - sh;
    signPositions[base + 2] = pz - tz * sw;
    signPositions[base + 3] = px + tx * sw;
    signPositions[base + 4] = py - sh;
    signPositions[base + 5] = pz + tz * sw;
    signPositions[base + 6] = px - tx * sw;
    signPositions[base + 7] = py + sh;
    signPositions[base + 8] = pz - tz * sw;
    signPositions[base + 9] = px + tx * sw;
    signPositions[base + 10] = py + sh;
    signPositions[base + 11] = pz + tz * sw;

    const col = i % ATLAS_COLS;
    const row = Math.floor(i / ATLAS_COLS) % ATLAS_ROWS;
    const u0 = col / ATLAS_COLS;
    const u1 = (col + 1) / ATLAS_COLS;
    const v1 = 1 - row / ATLAS_ROWS;
    const v0 = 1 - (row + 1) / ATLAS_ROWS;
    const ub = i * 8;
    signUvs[ub] = u0;
    signUvs[ub + 1] = v0;
    signUvs[ub + 2] = u1;
    signUvs[ub + 3] = v0;
    signUvs[ub + 4] = u0;
    signUvs[ub + 5] = v1;
    signUvs[ub + 6] = u1;
    signUvs[ub + 7] = v1;

    const ib = i * 6;
    const v = i * 4;
    signIndices[ib] = v;
    signIndices[ib + 1] = v + 1;
    signIndices[ib + 2] = v + 2;
    signIndices[ib + 3] = v + 2;
    signIndices[ib + 4] = v + 1;
    signIndices[ib + 5] = v + 3;
  }

  const signGeo = new THREE.BufferGeometry();
  if (signSlotCount > 0) {
    signGeo.setAttribute('position', new THREE.BufferAttribute(signPositions, 3));
    signGeo.setAttribute('uv', new THREE.BufferAttribute(signUvs, 2));
    signGeo.setIndex(new THREE.BufferAttribute(signIndices, 1));
    signGeo.computeVertexNormals();
  }
  const signMat = new THREE.MeshBasicMaterial({
    map: atlasTexture,
    side: THREE.DoubleSide,
    transparent: false
  });
  const signMesh = new THREE.Mesh(signGeo, signMat);
  signMesh.name = 'signs';
  signMesh.visible = signSlotCount > 0;
  group.add(signMesh);
  if (NARROW_VIEW) nightCullTouch.push(signMesh);

  const drawnSigns = new Array(SLOT_CAP).fill('');
  // Reusable buffers for the canvas text layout: setText is called from the agents' sign
  // cycler, never from a render loop, and it must not build arrays or strings per word.
  const signWordW = new Float32Array(24);
  const signWordAt = new Int32Array(24);
  const signWordLen = new Int32Array(24);
  const signLineIdx = [0, 0, 0];
  const signLineWord = [0, 0, 0];

  function wrapSignText(ctx, text, cellX, cellY) {
    const src = String(text || '');
    const len = src.length;
    let count = 0;
    let i = 0;
    while (i < len && count < 24) {
      while (i < len && src.charCodeAt(i) === 32) i += 1;
      if (i >= len) break;
      const start = i;
      while (i < len && src.charCodeAt(i) !== 32) i += 1;
      signWordAt[count] = start;
      signWordLen[count] = i - start;
      signWordW[count] = ctx.measureText(src.slice(start, i)).width;
      count += 1;
    }
    const limit = CELL_W - 16;
    const spaceW = ctx.measureText(' ').width;
    let lineW = 0;
    let lineCount = 1;
    let lineStart = 0;
    const lineIdx = signLineIdx;
    const lineWord = signLineWord;
    let words = 0;
    for (let k = 0; k < count; k += 1) {
      const add = (words > 0 ? spaceW : 0) + signWordW[k];
      if (lineW + add > limit && words > 0) {
        lineIdx[lineCount - 1] = lineStart;
        lineWord[lineCount - 1] = words;
        lineCount += 1;
        if (lineCount > 3) {
          lineCount = 3;
          break;
        }
        lineStart = k;
        words = 0;
        lineW = 0;
      }
      lineW += (words > 0 ? spaceW : 0) + signWordW[k];
      words += 1;
    }
    if (lineCount <= 3) {
      lineIdx[lineCount - 1] = lineStart;
      lineWord[lineCount - 1] = words;
    }
    const startY = cellY + CELL_H * 0.5 - (lineCount - 1) * 11;
    for (let l = 0; l < lineCount; l += 1) {
      let x = cellX + CELL_W * 0.5;
      for (let w = 0; w < lineWord[l]; w += 1) {
        const k = lineIdx[l] + w;
        if (w === 0) x += signWordW[k] * 0.5;
        else x += signWordW[k] * 0.5;
        const a = signWordAt[k];
        const b = a + signWordLen[k];
        ctx.fillText(src.slice(a, b), x, startY + l * 22);
        x += signWordW[k] * 0.5 + spaceW;
      }
    }
  }

  function setSignText(slot, text) {
    if (slot < 0 || slot >= signSlotCount) return;
    const clean = String(text || '').slice(0, 44);
    if (drawnSigns[slot] === clean) return;
    drawnSigns[slot] = clean;
    if (!atlasCtx) return;
    const col = slot % ATLAS_COLS;
    const row = Math.floor(slot / ATLAS_COLS) % ATLAS_ROWS;
    const x = col * CELL_W;
    const y = row * CELL_H;
    if (signSlotShop[slot]) {
      // A painted shop fascia: cream board, dark reveal, the name in one line across the middle.
      // The name is drawn narrow so that the wide board does not stretch the letters.
      atlasCtx.fillStyle = '#f4ecda';
      atlasCtx.fillRect(x, y, CELL_W, CELL_H);
      atlasCtx.strokeStyle = '#5a4c3c';
      atlasCtx.lineWidth = 4;
      atlasCtx.strokeRect(x + 4, y + 4, CELL_W - 8, CELL_H - 8);
      atlasCtx.fillStyle = '#7a3a2a';
      atlasCtx.fillRect(x + 8, y + CELL_H - 15, CELL_W - 16, 4);
      atlasCtx.save();
      atlasCtx.translate(x + CELL_W * 0.5, y + CELL_H * 0.5);
      atlasCtx.scale(0.62, 1);
      atlasCtx.fillStyle = '#33291f';
      atlasCtx.font = '700 26px system-ui, sans-serif';
      atlasCtx.textAlign = 'center';
      atlasCtx.textBaseline = 'middle';
      atlasCtx.fillText(clean, 0, -2);
      atlasCtx.restore();
      atlasTexture.needsUpdate = true;
      return;
    }
    atlasCtx.fillStyle = '#f2e8d5';
    atlasCtx.fillRect(x, y, CELL_W, CELL_H);
    atlasCtx.strokeStyle = '#8a7a63';
    atlasCtx.lineWidth = 3;
    atlasCtx.strokeRect(x + 3, y + 3, CELL_W - 6, CELL_H - 6);
    atlasCtx.fillStyle = '#3a3229';
    atlasCtx.font = '600 15px system-ui, sans-serif';
    atlasCtx.textAlign = 'center';
    atlasCtx.textBaseline = 'middle';
    wrapSignText(atlasCtx, clean, x, y);
    atlasTexture.needsUpdate = true;
  }

  // The named shops are painted once at build time. Their slots are not part of any quarter, so
  // the agents' phrase cycler leaves them alone and no texture upload ever follows from them.
  for (let i = 0; i < signSlotCount; i += 1) {
    if (!signSlotShop[i]) continue;
    setSignText(i, slotAssignments[i].name || '');
  }

  const signs = {
    setText: setSignText,
    slotCount: signSlotCount,
    slotQuarter: signSlotQuarter,
    mesh: signMesh,
    // The atlas is mipmapped but built at anisotropy 1, and a fascia seen down a street at a
    // grazing angle is exactly what that setting blurs. The renderer only exists in main, so
    // the texture is exposed here and its anisotropy is raised there, once, at startup.
    texture: atlasTexture
  };

  // ------------------------------------------------------------------ cranes
  const CRANE_FLOORS = 12;
  const CRANE_CAP = Math.max(1, qCount) * (CRANE_FLOORS + 1);
  const craneMesh = new THREE.InstancedMesh(unitBox, new THREE.MeshStandardMaterial({
    color: 0xe0a63c,
    roughness: 0.5,
    metalness: 0.14
  }), CRANE_CAP);
  craneMesh.castShadow = shadowsOn;
  craneMesh.name = 'cranes';
  craneMesh.frustumCulled = false;
  group.add(craneMesh);
  nightCull.push(craneMesh);

  const craneFloors = new Int16Array(Math.max(1, qCount));
  const FLOOR_H = 3.0;

  function applyCranes() {
    let cursor = 0;
    for (let qi = 0; qi < quads.length; qi += 1) {
      const q = quads[qi];
      const floors = craneFloors[qi];
      const base = q.craneSlot.position;
      for (let f = 0; f < CRANE_FLOORS; f += 1) {
        const on = f < floors;
        dummy.position.set(base.x, base.y + FLOOR_H * (f + 0.5), base.z);
        dummy.rotation.set(0, 0, 0);
        dummy.scale.set(on ? 1.1 : 0.001, on ? FLOOR_H : 0.001, on ? 1.1 : 0.001);
        dummy.updateMatrix();
        craneMesh.setMatrixAt(cursor, dummy.matrix);
        cursor += 1;
      }
      const topY = base.y + FLOOR_H * floors;
      dummy.position.set(base.x + 5.5, topY + 0.4, base.z);
      dummy.rotation.set(0, 0.35, 0);
      dummy.scale.set(floors > 0 ? 12 : 0.001, floors > 0 ? 0.55 : 0.001, floors > 0 ? 0.55 : 0.001);
      dummy.updateMatrix();
      craneMesh.setMatrixAt(cursor, dummy.matrix);
      cursor += 1;
    }
    for (let i = cursor; i < CRANE_CAP; i += 1) {
      dummy.position.set(0, -400, 0);
      dummy.rotation.set(0, 0, 0);
      dummy.scale.set(0.001, 0.001, 0.001);
      dummy.updateMatrix();
      craneMesh.setMatrixAt(i, dummy.matrix);
    }
    craneMesh.instanceMatrix.needsUpdate = true;
  }

  const cranes = {
    setFloors: function (q, floors) {
      if (q < 0 || q >= quads.length) return;
      const next = Math.max(0, Math.min(CRANE_FLOORS, Math.floor(floors || 0)));
      if (craneFloors[q] === next) return;
      craneFloors[q] = next;
      applyCranes();
    }
  };
  applyCranes();

  // ------------------------------------------------------------------ neglect
  // Shutters down, and the quarter's buildings and trims desaturated and dimmed.
  function neglectTint(c) {
    const l = c.r * 0.3 + c.g * 0.59 + c.b * 0.11;
    c.r = (c.r + (l - c.r) * 0.65) * 0.62;
    c.g = (c.g + (l - c.g) * 0.65) * 0.62;
    c.b = (c.b + (l - c.b) * 0.65) * 0.62;
  }

  function setNeglected(q, flag) {
    if (q < 0 || q >= qCount) return;
    const on = flag ? 1 : 0;
    if (quarterNeglected[q] === on) return;
    quarterNeglected[q] = on;
    const bl = buildingsOfQuarter[q];
    for (let i = 0; i < bl.length; i += 1) {
      colorScratch.fromArray(buildingBase, bl[i] * 3);
      if (on) neglectTint(colorScratch);
      buildings.setColorAt(bl[i], colorScratch);
    }
    if (buildings.instanceColor) buildings.instanceColor.needsUpdate = true;
    // Pack models carry their own instance colours, one registry per mesh, so a quiet quarter
    // desaturates its real buildings exactly like its procedural boxes. Non tintable models
    // (lamps, parasols, props) have no instanceColor and are left alone.
    for (let mi = 0; mi < modelMeshList.length; mi += 1) {
      const mesh = modelMeshList[mi];
      const map = modelMeshQuarters[mi];
      const base = modelMeshBase[mi];
      if (!base || !mesh.instanceColor) continue;
      let dirty = false;
      for (let i = 0; i < map.length; i += 1) {
        if (map[i] !== q) continue;
        colorScratch.fromArray(base, i * 3);
        if (on) neglectTint(colorScratch);
        mesh.setColorAt(i, colorScratch);
        dirty = true;
      }
      if (dirty) mesh.instanceColor.needsUpdate = true;
    }
    const dl = detailsOfQuarter[q];
    for (let i = 0; i < dl.length; i += 1) {
      colorScratch.fromArray(detailBase, dl[i] * 3);
      if (on) neglectTint(colorScratch);
      details.setColorAt(dl[i], colorScratch);
    }
    if (details.instanceColor) details.instanceColor.needsUpdate = true;
    const wl = windowsOfQuarter[q];
    for (let i = 0; i < wl.length; i += 1) applyWindowColor(wl[i]);
    if (windows.instanceColor) windows.instanceColor.needsUpdate = true;
  }

  // ------------------------------------------------------------------ crown audit
  // The stone village is laid out by geometry rather than scattered on a grid, so this pass
  // re-tests every house against the rules it was built to: its walls against the forbidden
  // ground, its neighbours, the lanes, the church cypresses and the retaining walls, and its
  // base against the ground under it. The counts are exposed on world.crown and are expected
  // to be zero, so a regression shows as a number instead of a smudge in the frame. It runs
  // once at build time, over at most HOUSE_CAP houses.
  const crownAudit = {
    houses: 0, overlaps: 0, floating: 0, sunk: 0, forbidden: 0, lanes: 0, cypresses: 0,
    church: 0, trees: 0, walls: 0
  };
  {
    const EPS = 0.05;
    // Two rectangles with their own yaw overlap when no axis of either separates them. The
    // church is rotated 0.06 rad, so a plain AABB test would be wrong at its corners, and its
    // roof eaves overhang the nave by half a metre: the parts are tested as they are drawn.
    const churchParts = [
      [-44.0, -104.0, 7.0, 11.0, 0.06],     // nave walls
      [-44.0, -104.0, 7.5, 11.6, 0.06],     // nave roof eaves
      [-44.0, -116.4, 4.2, 2.6, 0.06],      // apse
      [-49.0, -110.0, 1.9, 1.9, 0],         // campanile shaft
      [-49.0, -110.0, 2.25, 2.25, 0],       // campanile base
      [-49.0, -110.0, 2.175, 2.175, 0],     // campanile cornice
      [-44.0, -92.4, 6.0, 1.0, 0.06]        // steps up to the door
    ];
    function rectsOverlap(ax, az, ahx, ahz, ayaw, bx, bz, bhx, bhz, byaw) {
      const dx = bx - ax;
      const dz = bz - az;
      for (let k = 0; k < 4; k += 1) {
        const yaw = k < 2 ? ayaw : byaw;
        const ux = k % 2 === 0 ? Math.cos(yaw) : Math.sin(yaw);
        const uz = k % 2 === 0 ? -Math.sin(yaw) : Math.cos(yaw);
        const sep = Math.abs(dx * ux + dz * uz);
        const ra = ahx * Math.abs(Math.cos(ayaw) * ux - Math.sin(ayaw) * uz) +
          ahz * Math.abs(Math.sin(ayaw) * ux + Math.cos(ayaw) * uz);
        const rb = bhx * Math.abs(Math.cos(byaw) * ux - Math.sin(byaw) * uz) +
          bhz * Math.abs(Math.sin(byaw) * ux + Math.cos(byaw) * uz);
        if (sep >= ra + rb) return false;
      }
      return true;
    }
    for (let a = 0; a < houseSpots.length; a += 1) {
      const ha = houseSpots[a];
      const hw = ha.hx;
      const hd = ha.hz;
      crownAudit.houses += 1;
      // The base against the lowest ground the walls cross: a half metre of buried plinth is
      // how every box in this district meets a slope, so above that lowest ground by a quarter
      // is a gap under a wall, and deeper than six tenths is a house that has sunk into the hill.
      if (ha.base > ha.gMin + 0.25) crownAudit.floating += 1;
      if (ha.base < ha.gMin - 0.6) crownAudit.sunk += 1;
      // The footprint against the forbidden bits. B_HOUSE and B_OLD are not forbidden: a
      // terrace shares its party wall with the next house, and the village's own ground is the
      // ground it is built on.
      if (!rectScan(ha.x, ha.z, Math.cos(ha.yaw), -Math.sin(ha.yaw),
        ha.w * 0.5 - 0.02, ha.d * 0.5 - 0.02, CROWN_FORBID, 0)) {
        crownAudit.forbidden += 1;
      }
      // The church as it is drawn, part by part: the grid reservation is a rectangle and the
      // nave roof turns half a metre outside it, so the exact rotated parts are tested too.
      for (let c = 0; c < churchParts.length; c += 1) {
        const cp = churchParts[c];
        if (rectsOverlap(ha.x, ha.z, ha.w * 0.5, ha.d * 0.5, ha.yaw,
          cp[0], cp[1], cp[2], cp[3], cp[4])) {
          crownAudit.church += 1;
          break;
        }
      }
      // The alley frontage stays walkable: no wall of the house may reach inside the paving.
      for (let l = 0; l < OLD_LANES.length; l += 1) {
        const lane = OLD_LANES[l];
        if (ha.x + hw > OLD_PAVE_X0 && ha.x - hw < OLD_PAVE_X1 &&
          ha.z + hd > lane - OLD_ALLEY && ha.z - hd < lane + OLD_ALLEY) {
          crownAudit.lanes += 1;
          break;
        }
      }
      for (let b = a + 1; b < houseSpots.length; b += 1) {
        const hb = houseSpots[b];
        // Two world-axis footprints overlap when their spans overlap on both axes; a party
        // wall shares a line, which is not an overlap, so the epsilon keeps it out.
        if (Math.abs(ha.x - hb.x) < ha.hx + hb.hx - EPS &&
          Math.abs(ha.z - hb.z) < ha.hz + hb.hz - EPS) {
          crownAudit.overlaps += 1;
        }
      }
      for (let c = 0; c < cypressSpots.length; c += 1) {
        const cp = cypressSpots[c];
        // The same crown cone the planting tests, against this house: a cypress is not a
        // cylinder, and its rim is above the eaves on the taller houses.
        if (treeHitsHouse(cp.x, cp.z, cp.sx, cp.sy, cp.y, ha)) {
          crownAudit.cypresses += 1;
          break;
        }
      }
      // Every retaining wall is sampled along its run, so a wall standing inside a footprint
      // is counted even when the one metre flag cell missed it.
      let wallHit = false;
      for (let r = 0; r < terraceRuns.length && !wallHit; r += 1) {
        const run = terraceRuns[r];
        const len = run.len;
        const ux = (run.x1 - run.x0) / len;
        const uz = (run.z1 - run.z0) / len;
        for (let s = 0; s <= len; s += 1) {
          const px = run.x0 + ux * s;
          const pz = run.z0 + uz * s;
          if (Math.abs(px - ha.x) < hw + 0.47 && Math.abs(pz - ha.z) < hd + 0.47) {
            wallHit = true;
            break;
          }
        }
      }
      if (wallHit) crownAudit.walls += 1;
    }
    // The crowns as they are drawn: every tree instance's matrix carries its horizontal and
    // vertical scale, and the same cone test the planting uses is repeated here, so the village
    // is proved clear of every canopy, not only of the six curated cypresses above.
    const treePos = new THREE.Vector3();
    for (let i = 0; i < treeCursor; i += 1) {
      trees.getMatrixAt(i, m4);
      treePos.setFromMatrixPosition(m4);
      const e = m4.elements;
      const tsx = Math.hypot(e[0], e[1], e[2]);
      const tsy = Math.hypot(e[4], e[5], e[6]);
      for (let a = 0; a < houseSpots.length; a += 1) {
        if (treeHitsHouse(treePos.x, treePos.z, tsx, tsy, treePos.y, houseSpots[a])) {
          crownAudit.trees += 1;
          break;
        }
      }
    }
    // The pack canopies are drawn from their own instance buffers, so the same cone test runs
    // over their recorded scales instead of over the procedural mesh.
    for (let i = 0; i < modelTreeRecords.length; i += 1) {
      const mt = modelTreeRecords[i];
      const mty = groundHeightAt(mt.x, mt.z) - 0.15;
      for (let a = 0; a < houseSpots.length; a += 1) {
        if (treeHitsHouse(mt.x, mt.z, mt.sx, mt.sy, mty, houseSpots[a])) {
          crownAudit.trees += 1;
          break;
        }
      }
    }
  }

  // ------------------------------------------------------------------ hillside audit
  // The terraces, the tracks and every canopy are sited by the occupancy grid, so this pass
  // re-reads that same grid and the road centrelines and counts anything that still landed on a
  // carriageway or inside a building footprint. The counts are exposed on world.hillside and
  // have to be zero; a regression is then visible as a number instead of as a smudge in a
  // screenshot. A farm track is allowed to touch the paved cells of the road it leaves, and only
  // there: any paved cell more than twelve metres along the track is a crossing and counts.
  const hillsideAudit = {
    trees: 0, treeRoad: 0, treeBuilding: 0,
    walls: 0, wallRoad: 0, wallBuilding: 0,
    trackSamples: 0, trackBuilding: 0, trackMouth: 0, trackCross: 0,
    trackMouths: [], trackBuildingSpots: []
  };
  function pathDistance(path, x, z) {
    let best = Infinity;
    for (let i = 0; i < path.n; i += 1) {
      const ax = path.xs[i];
      const az = path.zs[i];
      const dx = path.xs[i + 1] - ax;
      const dz = path.zs[i + 1] - az;
      const len2 = dx * dx + dz * dz;
      let t = len2 > 0 ? ((x - ax) * dx + (z - az) * dz) / len2 : 0;
      if (t < 0) t = 0;
      else if (t > 1) t = 1;
      const px = ax + dx * t - x;
      const pz = az + dz * t - z;
      const d2 = px * px + pz * pz;
      if (d2 < best) best = d2;
    }
    return Math.sqrt(best);
  }
  function onAsphalt(x, z) {
    const f = flagsAt(x, z);
    if (!(f & (B_RING | B_AVE | B_SPUR | B_ROUND | B_STREET | B_TRAM | B_RAIL | B_VIA | B_TRACK))) return false;
    if ((f & B_VIA) && pathDistance(exPath, x, z) < EX_HALF) return true;
    if ((f & B_ROUND) && inRoundabout(x, z, ROUND_R - 0.8) && !inRoundabout(x, z, ISLAND_R + 0.6)) return true;
    if ((f & B_RING) && Math.abs(ringSigned(x, z)) < RING_HALF + 0.35) return true;
    if ((f & B_AVE) && pathDistance(avePath, x, z) < AVE_HALF + 0.35) return true;
    if ((f & B_TRAM) && pathDistance(tramPath, x, z) < TRAM_BED + 0.3) return true;
    if ((f & B_RAIL) && pathDistance(railPath, x, z) < 7.8) return true;
    if (f & B_SPUR) {
      for (let s = 0; s < spurPaths.length; s += 1) {
        if (pathDistance(spurPaths[s], x, z) < SPUR_HALF + 0.35) return true;
      }
    }
    if (f & B_STREET) {
      for (let r = 0; r < streetRunsV.length; r += 1) {
        const run = streetRunsV[r];
        if (Math.abs(x - run.coord) < STREET_HALF + 0.3 && z > run.t0 - 0.3 && z < run.t1 + 0.3) return true;
      }
      for (let r = 0; r < streetRunsH.length; r += 1) {
        const run = streetRunsH[r];
        if (Math.abs(z - run.coord) < STREET_HALF + 0.3 && x > run.t0 - 0.3 && x < run.t1 + 0.3) return true;
      }
    }
    if ((f & B_TRACK)) {
      for (let t = 0; t < trackPaths.length; t += 1) {
        if (pathDistance(trackPaths[t], x, z) < TRACK_HALF + 0.2) return true;
      }
    }
    return false;
  }
  {
    const treePos = new THREE.Vector3();
    for (let i = 0; i < treeCursor; i += 1) {
      trees.getMatrixAt(i, m4);
      treePos.setFromMatrixPosition(m4);
      hillsideAudit.trees += 1;
      if (onAsphalt(treePos.x, treePos.z)) hillsideAudit.treeRoad += 1;
      if (flagsAt(treePos.x, treePos.z) & B_BUILD) hillsideAudit.treeBuilding += 1;
    }
    for (let i = 0; i < modelTreeRecords.length; i += 1) {
      const mt = modelTreeRecords[i];
      hillsideAudit.trees += 1;
      if (onAsphalt(mt.x, mt.z)) hillsideAudit.treeRoad += 1;
      if (flagsAt(mt.x, mt.z) & B_BUILD) hillsideAudit.treeBuilding += 1;
    }
    for (let r = 0; r < terraceRuns.length; r += 1) {
      const run = terraceRuns[r];
      hillsideAudit.walls += 1;
      const len = run.len;
      const ux = (run.x1 - run.x0) / len;
      const uz = (run.z1 - run.z0) / len;
      for (let s = 0; s <= len; s += 1) {
        const x = run.x0 + ux * s;
        const z = run.z0 + uz * s;
        const wf = flagsAt(x, z);
        if (wf & (B_PAVED | B_TRACK)) hillsideAudit.wallRoad += 1;
        if (flagsAt(x, z) & B_BUILD) hillsideAudit.wallBuilding += 1;
      }
    }
    for (let t = 0; t < trackPaths.length; t += 1) {
      const path = trackPaths[t];
      let mouths = 0;
      for (let i = 0; i <= path.n; i += 1) {
        const x = path.xs[i];
        const z = path.zs[i];
        hillsideAudit.trackSamples += 1;
        if (flagsAt(x, z) & B_BUILD) {
          hillsideAudit.trackBuilding += 1;
          hillsideAudit.trackBuildingSpots.push(t + '@' + Math.round(i * path.step) + ':' + Math.round(x) + ',' + Math.round(z));
        }
        if (flagsAt(x, z) & B_PAVED) {
          if (i * path.step <= 12) {
            hillsideAudit.trackMouth += 1;
            mouths += 1;
          } else {
            hillsideAudit.trackCross += 1;
            hillsideAudit.trackCrossTrack = t;
            hillsideAudit.trackCrossAt = Math.round(i * path.step);
          }
        }
      }
      hillsideAudit.trackMouths.push(mouths);
    }
  }
  const trackLengths = [];
  let trackMetres = 0;
  for (let t = 0; t < trackPaths.length; t += 1) {
    trackLengths.push(Math.round(trackPaths[t].len));
    trackMetres += trackPaths[t].len;
  }
  const hillside = {
    wallMetres: Math.round(wallMetres),
    wallRuns: terraceRuns.length,
    wallSegments: wallSegmentCount,
    trackCount: trackPaths.length,
    trackMetres: Math.round(trackMetres),
    trackLengths: trackLengths,
    trees: {
      total: treeCursor + modelTreeUsed,
      procedural: treeCursor,
      model: modelTreeUsed,
      modelCypress: modelCypressUsed,
      modelPalm: modelPalmUsed,
      district: hillsideTreeStart,
      hillside: hillsideTrees,
      far: farPlanted,
      lane: lanePlanted,
      terrace: plantTerrace,
      tread: plantTread,
      band: plantBand,
      side: plantSide,
      track: plantTrack,
      cypress: cypressPlanted,
      hillsideCypress: hillsideCypress,
      cap: TREE_CAP
    },
    audit: hillsideAudit
  };

  // The crown reports what it actually built: the stone village's house count against its cap,
  // the lanes it is threaded on, and the zero-or-fail counts of the audit above.
  const crown = {
    houses: houseSpots.length,
    cap: HOUSE_CAP,
    lanes: OLD_LANES.length,
    zone: { x0: OLD_X0, x1: OLD_X1, z0: OLD_Z0, z1: OLD_Z1 },
    audit: crownAudit
  };

  // ------------------------------------------------------------------ result
  const plazaCenter = new THREE.Vector3(PLAZA_CX, groundHeightAt(PLAZA_CX, PLAZA_CZ), PLAZA_CZ);
  return {
    group: group,
    bounds: bounds,
    groundHeightAt: groundHeightAt,
    quarters: quads,
    roads: {
      ring: ringWorld,
      avenue: avenueCurve,
      tram: tramCurve,
      rail: railCurve,
      expressway: expresswayCurve,
      expresswayDeckY: EX_DECK_Y,
      spurs: spurs,
      // The dirt tracks, published as tables (n, len, step, xs, zs, nx, nz, ys) so farm
      // traffic walks the exact polylines the hillside mesh draws and no mover invents a road.
      tracks: trackTables,
      // Boulevard cross section, for the traffic and pedestrian agents: the car lane centre,
      // the carriageway edges and the walk line on the pavement.
      avenueLaneCentre: AVE_LANE_CENTRE,
      avenueCarriageway: { inner: AVE_CAR_IN, outer: AVE_HALF },
      avenuePavementOffset: AVE_PAVE_OFF,
      // Arterials that leave the frame: one uniform cross section, asphalt to the kerb at
      // spurHalfWidth, walk line a pavement in from it.
      spurHalfWidth: SPUR_HALF,
      spurPavementOffset: SPUR_HALF + SPUR_PAVE * 0.5,
      spurFrontage: SPUR_FRONT
    },
    roundabout: { center: roundaboutCenter, radius: ROUND_R },
    marketSquare: { center: plazaCenter, size: PLAZA_W },
    // The market reports what it built and the clearances it measured, so a regression is a
    // number here and not only a picture: stall count, rejected candidates, the nearest stall
    // to the fountain and the closest stall pair, plus the furniture totals.
    market: marketAudit,
    tramStops: tramStops,
    buildings: buildings,
    buildingQuarter: buildingQuarter,
    windows: windows,
    windowQuarter: windowQuarter,
    windowCount: windowCursor,
    // How many of those panes were overlaid on a pack model this cycle, and how many pack lots
    // took one, so "the model lots light up now" is a number and not a claim about a picture.
    modelWindows: { panes: modelWindowCount, coreLots: modelLots.length, villas: modelVillas.length },
    trees: trees,
    signs: signs,
    cranes: cranes,
    setNeglected: setNeglected,
    setDetailLevel: setDetailLevel,
    crossings: crossings,
    // One slot per built signal head in world units, plus the phase buffer agents fill. The
    // lenses are the only geometry that reads the buffer; agents calls apply() only on a phase
    // change, so this never runs per frame.
    signals: signalsApi,
    lights: lights,
    streetLights: streetLights,
    // The rooftop pass audits itself as it builds: how much plant went down, and how many pieces
    // broke the placement rule (a count that has to be zero, so a regression is visible here
    // rather than in a screenshot).
    roofClutter: roofClutter,
    // The hillside pass reports what it actually laid: wall metres, track lengths, planted
    // canopies and the zero-or-fail intersection counts of the audit above.
    hillside: hillside,
    // The stone crown: house count against the cap, the lanes, the reserved zone and the
    // zero-or-fail placement audit.
    crown: crown,
    // The mural pass reports its own supply: how many blind walls faced the district view, how
    // many clear ones it found, and the projected area it finally painted.
    // Pack-model report: how many lots, towers and villas took a model, the instance and
    // triangle totals the build actually placed, and the model names behind them.
    models: modelStats,
    // The tower pass reports its own search: how many slabs the data asked for, how many the
    // occupancy grid allowed, and where each one stands, so a regression is a number here and
    // not only a missing silhouette. The first cycle built one of four and said nothing.
    towers: {
      wanted: towerWanted,
      placed: towerPlaced.length,
      fronts: towerFronts.length,
      recs: towerPlaced.map(function (r) {
        return {
          qi: r.qi,
          x: Math.round(r.cx * 10) / 10,
          z: Math.round(r.cz * 10) / 10,
          top: Math.round(r.top * 10) / 10,
          floors: r.floors,
          kind: r.kind
        };
      })
    },
    modelTrees: {
      used: modelTreeUsed,
      cypress: modelCypressUsed,
      palm: modelPalmUsed,
      caps: {
        trees: MODEL_TREE_MAX,
        cypress: MODEL_CYPRESS_MAX,
        palm: MODEL_PALM_MAX
      }
    },
    // The outer fabric reports the region rule it enforced and what the metered fill placed,
    // so "the city now continues past the ring on two sides" is a table of numbers rather than
    // a claim about a picture: lots per street, the mask the region test reads, and the count
    // of civic plots that could not reach a kerb and had to take a free spiral instead.
    fabric: {
      lots: fabricLots,
      cap: FABRIC_LOT_MAX,
      streets: {
        ring: fabricAudit.ring,
        westLower: fabricAudit.westLower,
        hillside: fabricAudit.hillside,
        east: fabricAudit.east,
        west: fabricAudit.west
      },
      beltDepth: OUTER_BELT_DEPTH,
      band: OUTER_BAND,
      cell: FABRIC_CELL,
      maskCells: fabricMask.reduce(function (n, v) {
        return n + (v ? 1 : 0);
      }, 0),
      offStreet: civicOffStreet,
      civic: civic.map(function (r) {
        return {
          type: r.type,
          via: r.via,
          x: Math.round(r.x * 10) / 10,
          z: Math.round(r.z * 10) / 10
        };
      })
    },
    murals: {
      panels: muralPicks.length,
      wallPx: Math.round(muralPx),
      faces: muralAudit.faces,
      blind: muralAudit.blind,
      clear: muralAudit.room,
      framed: muralAudit.framed,
      over700: muralAudit.over700,
      over1500: muralAudit.over1500,
      // One record per painted wall: where it stands and which street it fronts, plus the view
      // its size was scored in, so "the programme paints the arterials now" is a table.
      list: muralPicks.map(function (m) {
        return {
          on: m.on,
          x: Math.round(m.wx * 10) / 10,
          z: Math.round(m.wz * 10) / 10,
          px: Math.round(m.px),
          view: m.view
        };
      })
    },
    // The street-detail pass: piece count by kind and the tried count, so the density of the
    // kerbside litter is a number next to the corner audit that measures what it fixed.
    street: streetAudit,
    // The shared detail pool reports its own arithmetic: pieces laid per rank, the budget, the
    // kept prefix and the dropped rank-2 tail, so fabric passes that lean on the pool are
    // measurable instead of guessable.
    details: detailAudit,
    // The standing crowds report where the figures went and what the meshes cost, so the
    // population is a table of numbers rather than a claim about a picture.
    crowds: crowdAudit,
    // Every model building is pickable through its own instanceId -> quarter map, so a tap on
    // a pack facade opens the same quarter card a procedural box opens.
    pickTargets: [buildings].concat(modelPickMeshes)
  };
}
