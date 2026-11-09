/* dream.mjs - the night dream.

   One top-level identifier: createDream. Every helper, constant and scratch object
   lives inside that factory's closure, because the assembler flattens all fragments
   into one module scope.

   The whole chantier is built once at construction and reused for every dream:
   cones, striped barriers, two floodlight masts (emissive faces, ground light
   pools and shadowless point lights), a road crew of six hi-vis figures at about
   2.5 units tall, the freshly painted road ribbon with its white centre dashes
   and its two crisp edge lines, the additive light strip that runs the length of
   that ribbon, and a pop-up market.

   Every chantier surface carries baked vertex colours on an unlit
   MeshBasicMaterial, so the ten percent district light of the dream cannot
   darken it: at night the works are the brightest, most saturated thing in the
   district, which is the whole point of the dream. The two floodlights stay in
   the render list forever and only ever animate intensity, so the shader
   programs never recompile. Seven draw calls are visible while the provisional
   works stand (ribbon, light strip, cones, barriers, masts, pools, crew) and
   three once the taken road is asphalt and the market is up: the edge lines and
   the extra pool rings ride inside those same geometries and add no call.
   On the phone profile (`ui.compact`) the crew, the barriers, the cones, the
   stalls and the awnings are never shown, which leaves four calls with the works
   standing (ribbon, light strip, masts, pools) and one once a road is taken.
   Nothing is allocated inside update(dt, sim), and warm() is a one shot for the
   material compile, called by main once after install.

   The returned object also carries two read-only helpers for a screenshot
   harness: chantierCenter (a THREE.Vector3 at the middle of the provisional
   ribbon) and chantierSpan (its length in world units). */

export function createDream(THREE, world, model, agents, rig, ui) {
  const shell = ui || {};

  // The phone profile (narrow, coarse pointer) cannot read the cheap dressing at 412 px, and
  // the owner gate caps that frame at 45 draw calls with the chantier standing. The crew, the
  // barriers, the cones, the stalls and the awnings are still built and laid exactly as usual,
  // they are simply never shown there: three draw calls while the works stand, two once a road
  // is taken. The provisional road, the ghost, the glow, the masts, the floodlight pools and
  // the lamps are untouched, so the chantier stays the brightest thing on screen.
  const compact = !!shell.compact;
  const dressing = !compact;

  // ---------------------------------------------------------------- constants
  const SAMPLES = 96;               // ribbon samples, fixed for every dream
  const SEGMENTS = SAMPLES - 1;
  const HALF_WIDTH = 4.5;           // 9 units of provisional road
  const ROAD_LIFT = 0.15;           // clears the terrain so nothing z-fights
  const DASH_LIFT = 0.05;           // centre dashes sit just above the ribbon
  const DASH_HALF = 0.32;           // dashed centre line, 0.64 units wide
  const DASH_LENGTH = 3.2;          // one dash has to read at district range
  const DASH_GAP_RATIO = 1.35;      // the gaps between dashes are longer
  const DASH_FROM = 0.08;           // a dash run starts a little into its segment
  const DASH_TO = 0.92;             // and stops early, so the run has clean ends
  const EDGE_LIFT = 0.065;          // the edge lines ride above the paint
  const EDGE_OUTER = 3.88;          // outer limit of the crisp edge lines
  const EDGE_INNER = 3.42;          // inner limit, so each line is 0.46 wide
  const SEGMENT_INDICES = 24;       // road, centre dash, and two edge line quads
  const ROAD_BASE_VERTS = SAMPLES * 2;
  const DASH_BASE = ROAD_BASE_VERTS;
  const EDGE_BASE = DASH_BASE + SEGMENTS * 4;
  const ROAD_VERTS = EDGE_BASE + SEGMENTS * 8;
  const CONE_COUNT = 24;
  const CONE_SCALE = 1.8;           // the cones are 1.8 times their modelled size
  const BARRIER_COUNT = 14;
  const BARRIER_STEP = 5;           // ribbon samples between two barriers per edge
  const CREW_COUNT = 6;
  const CREW_SCALE = 1.28;          // the cast stands about 2.5 units tall
  const STALL_COUNT = 4;
  const MAST_HEIGHT = 6.2;
  const LAMP_INTENSITY = 2400;      // candela: the pool is the brightest ground
  const LAMP_DISTANCE = 260;        // the wash reaches the far side of the works and beyond
  const POOL_RADIUS = 26.0;         // ground light pool under each mast
  const POOL_RINGS = 5;
  const POOL_SEGMENTS = 16;
  const POOL_GAIN = 0.48;           // additive pool brightness at its centre
  const GLOW_HALF = HALF_WIDTH * 3.0; // the light strip spills this far out
  // The wide warm wash under the road. Two extra lanes each side at five and a bit road
  // widths carry a faint, broad lift, so the works sit in a pool of light instead of on a
  // bright strip punched into a black district. It is the same geometry and the same draw
  // call as the core strip: two more lanes and nothing else.
  const GLOW_WASH = HALF_WIDTH * 5.4;
  const GLOW_LIFT = 0.03;           // the strip rides just above the fresh paint
  const GLOW_GAIN = 0.50;           // additive warmth at the strip core
  const GLOW_LANES = 7;             // seven lanes, so both edges fade to nothing
  const GLOW_SEGMENT_INDICES = 36;  // six quads per segment
  const GLOW_VERTS = SAMPLES * GLOW_LANES;
  const GLOW_OFFSET = [-GLOW_WASH, -GLOW_HALF, -GLOW_HALF * 0.55, 0, GLOW_HALF * 0.55, GLOW_HALF, GLOW_WASH];
  const GLOW_LANE_GAIN = [0.14, 0.34, 0.80, 1, 0.80, 0.34, 0.14];
  const GROW_SECONDS = 14;          // the ribbon is laid over this much of the dream
  const WAKE_MS = 2000;
  const FADE_SECONDS = 10;
  const PACK_SECONDS = 1.4;
  const HOUR_MS = 3600000;
  const IGNORE_MS = 24 * HOUR_MS;
  const GAP_MIN_MS = 4 * HOUR_MS;
  const CARD_LIMIT = 2;
  const STORE_KEY = 'localtime.dreams.taken';
  const RING = SEGMENTS * SEGMENT_INDICES;  // index count of the whole ribbon
  const MAST_SAMPLE_A = Math.round(SAMPLES * 0.34);
  const MAST_SAMPLE_B = Math.round(SAMPLES * 0.66);

  // The chantier palette. These are baked into vertex colours, so they are the
  // lit values: nothing here is multiplied by the dim dream lights.
  // Freshly painted provisional surface. It sits a little darker than the white markings on
  // purpose: under the flood wash and the bloom the paint climbed to a near-white sheet and its
  // own dashes and edge lines dissolved into it, so the value is set where the markings still
  // read as markings while the road stays the brightest surface in the district.
  const ROAD_GREY = 0x868b84;
  const ROAD_WHITE = 0xffffff;      // centre dashes of the provisional road
  const ROAD_EDGE = 0xffffff;       // the two crisp edge lines
  const ROAD_ASPHALT = 0x3c3e42;    // the same road once it is taken
  const ROAD_DASH_PERM = 0xf1f0e6;
  const ROAD_EDGE_PERM = 0xe9e7dd;
  const CONE_ORANGE = 0xff6a08;
  const CONE_BAND = 0xffffff;
  const CONE_PLATE = 0xdf5205;
  const CREW_ORANGE = 0xff7a10;     // pure hi-vis, so the cast reads at distance
  const CREW_TAPE = 0xffffff;
  const CREW_HELMET = 0xffffff;
  const MAST_GREY = 0x9aa1a7;
  const HEAD_WARM = 0xfff6e2;
  // The air around each flood head: warm, a little dimmer than the white face, wide enough to
  // read as thrown light at district distance and to feed the night bloom.
  const HEAD_HALO = 0xffcf8f;
  const POOL_WARM = 0xffd9a2;
  const GLOW_WARM = 0xfff0cc;       // warm white of the light strip
  const BARRIER_LAMP = 0xffd08a;    // the amber lamp on every barrier post

  // ------------------------------------------------------------------- state
  let state = 'awake';              // 'awake' | 'dreaming' | 'waking' | 'offered'
  let modelRef = model || null;

  let roadMode = 0;                 // 0 nothing laid, 1 provisional, 2 permanent
  let growth = 0;                   // 0..1 draw range of the provisional ribbon
  let growthHold = 0;               // seconds to wait before laying (after a fade)
  let worksDelay = 0;               // seconds until a fresh shift walks in
  let crewClock = 0;
  let glideElapsed = 0;
  let glideDone = false;
  let fading = false;
  let fadeT = 0;
  let packing = false;
  let packT = 0;
  let dataElapsed = 0;
  let dataLast = null;
  let gapSleeping = false;
  let gapEndAt = 0;

  let pairA = -1;
  let pairB = -1;
  let pairKey = '';
  let permKey = '';
  let hasPair = false;

  // Read-only measurement hooks for a screenshot harness: where the provisional
  // ribbon sits and how long it is. layoutRoad refreshes both on every data
  // change; no other code writes them.
  const chantierCentre = new THREE.Vector3();
  let chantierSpan = 0;

  const pairWords = [];             // shared words of the chosen pair
  const cardTexts = [];             // at most two sentences, prebuilt at sleep
  const cards = [];                 // live DOM records, empty unless state is offered
  let takenList = [];               // sentences already accepted, from localStorage
  const mutedSentences = [];        // sentences the person walked away from this session

  // ----------------------------------------------------------------- scratch
  const dummy = new THREE.Object3D();
  const scratchColor = new THREE.Color(0xffffff);
  const centreVec = new THREE.Vector3();
  const samplePoint = new THREE.Vector3();
  const curvePoints = [new THREE.Vector3(), new THREE.Vector3(), new THREE.Vector3()];

  const sampleX = new Float32Array(SAMPLES);
  const sampleZ = new Float32Array(SAMPLES);
  const sampleTX = new Float32Array(SAMPLES);
  const sampleTZ = new Float32Array(SAMPLES);
  const sampleNX = new Float32Array(SAMPLES);
  const sampleNZ = new Float32Array(SAMPLES);

  const crewPhase = new Float32Array(CREW_COUNT);
  const crewSpeed = new Float32Array(CREW_COUNT);
  const crewSide = new Float32Array(CREW_COUNT);
  const crewAlong = new Float32Array(CREW_COUNT);
  const crewAnchor = new Float32Array(CREW_COUNT); // 0 the growing front, 1 pool A
  // Two groups, both on the paved ribbon and never outside its edge lines: four
  // figures stand in the first floodlight pool, two walk the growing front of the
  // paint. Lateral offsets stay inside EDGE_INNER (3.42) even at full sway.
  crewPhase[0] = 0.0; crewSpeed[0] = 0.55; crewSide[0] = -2.80; crewAlong[0] = -1.10;
  crewPhase[1] = 1.7; crewSpeed[1] = 0.70; crewSide[1] = 1.20; crewAlong[1] = 1.90;
  crewPhase[2] = 3.1; crewSpeed[2] = 0.62; crewSide[2] = 3.05; crewAlong[2] = -0.50;
  crewPhase[3] = 4.4; crewSpeed[3] = 0.48; crewSide[3] = -0.60; crewAlong[3] = 2.40;
  crewPhase[4] = 5.6; crewSpeed[4] = 0.80; crewSide[4] = 2.20; crewAlong[4] = -1.60;
  crewPhase[5] = 2.4; crewSpeed[5] = 0.66; crewSide[5] = -2.00; crewAlong[5] = 0.70;
  crewAnchor[0] = 1;
  crewAnchor[1] = 1;
  crewAnchor[2] = 1;
  crewAnchor[3] = 1;
  crewAnchor[4] = 0;
  crewAnchor[5] = 0;

  const barrierBX = new Float32Array(BARRIER_COUNT);
  const barrierBZ = new Float32Array(BARRIER_COUNT);
  const barrierBY = new Float32Array(BARRIER_COUNT);
  const barrierR = new Float32Array(BARRIER_COUNT);

  const ground = (world && typeof world.groundHeightAt === 'function')
    ? function (x, z) { return world.groundHeightAt(x, z); }
    : function () { return 0; };

  // ------------------------------------------------------------------ scene
  const group = new THREE.Group();
  group.name = 'dream';

  // One ribbon geometry per role: provisional (grows), ghost (fades), permanent.
  // Each segment owns one road quad, one dashed centre quad and two edge line
  // quads, so the markings grow and fade with the road instead of needing a
  // second draw call. All four surfaces are coloured per vertex: grey (or
  // asphalt) for the road, white for the dashes and the edge lines.
  const roadIndex = new Uint16Array(RING);
  for (let i = 0; i < SEGMENTS; i++) {
    const v = i * 2;
    const o = i * SEGMENT_INDICES;
    roadIndex[o] = v;
    roadIndex[o + 1] = v + 1;
    roadIndex[o + 2] = v + 2;
    roadIndex[o + 3] = v + 1;
    roadIndex[o + 4] = v + 3;
    roadIndex[o + 5] = v + 2;
    const d = DASH_BASE + i * 4;
    roadIndex[o + 6] = d;
    roadIndex[o + 7] = d + 1;
    roadIndex[o + 8] = d + 2;
    roadIndex[o + 9] = d + 1;
    roadIndex[o + 10] = d + 3;
    roadIndex[o + 11] = d + 2;
    const e = EDGE_BASE + i * 8;
    roadIndex[o + 12] = e;
    roadIndex[o + 13] = e + 1;
    roadIndex[o + 14] = e + 2;
    roadIndex[o + 15] = e + 1;
    roadIndex[o + 16] = e + 3;
    roadIndex[o + 17] = e + 2;
    roadIndex[o + 18] = e + 4;
    roadIndex[o + 19] = e + 5;
    roadIndex[o + 20] = e + 6;
    roadIndex[o + 21] = e + 5;
    roadIndex[o + 22] = e + 7;
    roadIndex[o + 23] = e + 6;
  }

  function makeRoadGeometry() {
    const geo = new THREE.BufferGeometry();
    const pos = new Float32Array(ROAD_VERTS * 3);
    const col = new Float32Array(ROAD_VERTS * 3);
    const attr = new THREE.BufferAttribute(pos, 3);
    const colAttr = new THREE.BufferAttribute(col, 3);
    geo.setAttribute('position', attr);
    geo.setAttribute('color', colAttr);
    geo.setIndex(new THREE.BufferAttribute(roadIndex, 1));
    geo.setDrawRange(0, 0);
    return { geo: geo, pos: pos, col: col, attr: attr, colAttr: colAttr };
  }

  // Colours never change after boot: only the positions are relaid per dream.
  function paintRibbon(rec, roadHex, dashHex, edgeHex) {
    const arr = rec.col;
    scratchColor.setHex(roadHex);
    let r = scratchColor.r;
    let g = scratchColor.g;
    let b = scratchColor.b;
    for (let i = 0; i < ROAD_BASE_VERTS; i++) {
      const o = i * 3;
      arr[o] = r;
      arr[o + 1] = g;
      arr[o + 2] = b;
    }
    scratchColor.setHex(dashHex);
    r = scratchColor.r;
    g = scratchColor.g;
    b = scratchColor.b;
    for (let i = DASH_BASE; i < EDGE_BASE; i++) {
      const o = i * 3;
      arr[o] = r;
      arr[o + 1] = g;
      arr[o + 2] = b;
    }
    scratchColor.setHex(edgeHex);
    r = scratchColor.r;
    g = scratchColor.g;
    b = scratchColor.b;
    for (let i = EDGE_BASE; i < ROAD_VERTS; i++) {
      const o = i * 3;
      arr[o] = r;
      arr[o + 1] = g;
      arr[o + 2] = b;
    }
    rec.colAttr.needsUpdate = true;
  }

  const roadProvRibbon = makeRoadGeometry();
  const roadGhostRibbon = makeRoadGeometry();
  const roadPermRibbon = makeRoadGeometry();

  // Unlit for the works: the ten percent dream light must not touch the paint.
  const matProv = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false
  });
  const matGhost = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false,
    transparent: true, opacity: 1, depthWrite: false
  });
  // The taken road stays asphalt: it is lit so it sits in the daylight scene.
  const matPerm = new THREE.MeshStandardMaterial({
    vertexColors: true, roughness: 0.95, metalness: 0.0, side: THREE.DoubleSide
  });
  paintRibbon(roadProvRibbon, ROAD_GREY, ROAD_WHITE, ROAD_EDGE);
  paintRibbon(roadGhostRibbon, ROAD_GREY, ROAD_WHITE, ROAD_EDGE);
  paintRibbon(roadPermRibbon, ROAD_ASPHALT, ROAD_DASH_PERM, ROAD_EDGE_PERM);

  const roadProv = new THREE.Mesh(roadProvRibbon.geo, matProv);
  roadProv.name = 'road-provisional';
  roadProv.frustumCulled = false;
  roadProv.visible = false;

  const roadGhost = new THREE.Mesh(roadGhostRibbon.geo, matGhost);
  roadGhost.name = 'road-ghost';
  roadGhost.frustumCulled = false;
  roadGhost.renderOrder = 3;
  roadGhost.visible = false;

  const roadPerm = new THREE.Mesh(roadPermRibbon.geo, matPerm);
  roadPerm.name = 'road-permanent';
  roadPerm.frustumCulled = false;
  roadPerm.visible = false;

  group.add(roadProv);
  group.add(roadGhost);
  group.add(roadPerm);

  // The light strip: one additive band down the whole ribbon, spilling about 2.2
  // times the painted width, warm white at the paint and nothing at its edges.
  // Its own draw range follows the same quads as the ribbon, so it grows with the
  // paint, fades out with the ghost and is gone the moment the road is taken.
  const glowIndex = new Uint16Array(SEGMENTS * GLOW_SEGMENT_INDICES);
  for (let i = 0; i < SEGMENTS; i++) {
    const o = i * GLOW_SEGMENT_INDICES;
    for (let l = 0; l < GLOW_LANES - 1; l++) {
      const a = i * GLOW_LANES + l;
      const b = a + GLOW_LANES;
      const p = o + l * 6;
      glowIndex[p] = a;
      glowIndex[p + 1] = b;
      glowIndex[p + 2] = a + 1;
      glowIndex[p + 3] = b;
      glowIndex[p + 4] = b + 1;
      glowIndex[p + 5] = a + 1;
    }
  }
  const glowPos = new Float32Array(GLOW_VERTS * 3);
  const glowCol = new Float32Array(GLOW_VERTS * 3);
  scratchColor.setHex(GLOW_WARM);
  for (let l = 0; l < GLOW_LANES; l++) {
    const gain = GLOW_LANE_GAIN[l] * GLOW_GAIN;
    const gr = scratchColor.r * gain;
    const gg = scratchColor.g * gain;
    const gb = scratchColor.b * gain;
    for (let i = 0; i < SAMPLES; i++) {
      const o = (i * GLOW_LANES + l) * 3;
      glowCol[o] = gr;
      glowCol[o + 1] = gg;
      glowCol[o + 2] = gb;
    }
  }
  const glowGeo = new THREE.BufferGeometry();
  glowGeo.setAttribute('position', new THREE.BufferAttribute(glowPos, 3));
  glowGeo.setAttribute('color', new THREE.BufferAttribute(glowCol, 3));
  glowGeo.setIndex(new THREE.BufferAttribute(glowIndex, 1));
  glowGeo.setDrawRange(0, 0);
  const matGlow = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false,
    transparent: true, depthWrite: false, opacity: 1,
    blending: THREE.AdditiveBlending,
    // Three submits a transparent double sided material twice, back faces then front faces.
    // This strip is one flat layer, so a fragment belongs to one triangle either way, and
    // addition is commutative: the single pass draws the identical image for one call less on
    // the phone, where the frame budget is the constraint. Desktop keeps the two pass default.
    forceSinglePass: compact
  });
  const roadGlow = new THREE.Mesh(glowGeo, matGlow);
  roadGlow.name = 'road-glow';
  roadGlow.frustumCulled = false;
  roadGlow.renderOrder = 2;
  roadGlow.visible = false;
  group.add(roadGlow);

  // -------------------------------------------------------- small geometry kit
  // Runs once at construction. Everything here bakes colour into the vertices so
  // one unlit material can carry orange cones, white bands and helmets in a
  // single draw call each.

  function addFrustum(P, C, I, seg, y0, r0, y1, r1, hex, cap) {
    scratchColor.setHex(hex);
    const cr = scratchColor.r;
    const cg = scratchColor.g;
    const cb = scratchColor.b;
    const start = P.length / 3;
    const step = (Math.PI * 2) / seg;
    for (let s = 0; s < seg; s++) {
      const a = s * step;
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      P.push(r0 * ca, y0, r0 * sa);
      C.push(cr, cg, cb);
      P.push(r1 * ca, y1, r1 * sa);
      C.push(cr, cg, cb);
    }
    for (let s = 0; s < seg; s++) {
      const n = (s + 1) % seg;
      const b0 = start + s * 2;
      const t0 = b0 + 1;
      const b1 = start + n * 2;
      const t1 = b1 + 1;
      I.push(b0, b1, t0, b1, t1, t0);
    }
    if (cap) {
      const centre = P.length / 3;
      P.push(0, y1, 0);
      C.push(cr, cg, cb);
      for (let s = 0; s < seg; s++) {
        const n = (s + 1) % seg;
        I.push(centre, start + s * 2 + 1, start + n * 2 + 1);
      }
    }
  }

  // A flat quad in the XY plane at z = 0, facing +Z: used for the floodlight
  // faces, which are part of the mast geometry so the pair costs one draw.
  function addQuad(P, C, I, x0, y0, x1, y1, z, hex) {
    scratchColor.setHex(hex);
    const cr = scratchColor.r;
    const cg = scratchColor.g;
    const cb = scratchColor.b;
    const start = P.length / 3;
    P.push(x0, y0, z, x1, y0, z, x0, y1, z, x1, y1, z);
    for (let i = 0; i < 4; i++) C.push(cr, cg, cb);
    I.push(start, start + 1, start + 2, start + 1, start + 3, start + 2);
  }

  // An axis aligned box given its centre and half extents. Each face owns its
  // four corners, so computing normals later gives crisp flat faces instead of
  // an averaged, rounded look.
  function addBox(P, C, I, cx, cy, cz, hx, hy, hz, hex) {
    scratchColor.setHex(hex);
    const cr = scratchColor.r;
    const cg = scratchColor.g;
    const cb = scratchColor.b;
    const xa = cx - hx;
    const xb = cx + hx;
    const ya = cy - hy;
    const yb = cy + hy;
    const za = cz - hz;
    const zb = cz + hz;
    const face = function (x0, y0, z0, x1, y1, z1, x2, y2, z2, x3, y3, z3) {
      const start = P.length / 3;
      P.push(x0, y0, z0, x1, y1, z1, x2, y2, z2, x3, y3, z3);
      for (let i = 0; i < 4; i++) C.push(cr, cg, cb);
      I.push(start, start + 1, start + 2, start + 1, start + 3, start + 2);
    };
    face(xa, ya, zb, xb, ya, zb, xa, yb, zb, xb, yb, zb);   // +Z
    face(xb, ya, za, xa, ya, za, xb, yb, za, xa, yb, za);   // -Z
    face(xb, ya, zb, xb, ya, za, xb, yb, zb, xb, yb, za);   // +X
    face(xa, ya, za, xa, ya, zb, xa, yb, za, xa, yb, zb);   // -X
    face(xa, yb, zb, xb, yb, zb, xa, yb, za, xb, yb, za);   // +Y
    face(xa, ya, za, xb, ya, za, xa, ya, zb, xb, ya, zb);   // -Y
  }

  // A thin post standing at (x, z), used by the market stalls.
  function addPostAt(P, C, I, x, z, height, radius, hex) {
    const from = P.length;
    addFrustum(P, C, I, 5, 0, radius, height, radius, hex, false);
    for (let i = from; i < P.length; i += 3) {
      P[i] += x;
      P[i + 2] += z;
    }
  }

  function addSphere(P, C, I, cy, radius, hex, latSeg, lonSeg) {
    scratchColor.setHex(hex);
    const cr = scratchColor.r;
    const cg = scratchColor.g;
    const cb = scratchColor.b;
    const start = P.length / 3;
    for (let la = 0; la <= latSeg; la++) {
      const phi = (la / latSeg) * Math.PI;
      const sy = Math.cos(phi);
      const sr = Math.sin(phi);
      for (let lo = 0; lo < lonSeg; lo++) {
        const th = (lo / lonSeg) * Math.PI * 2;
        P.push(Math.cos(th) * sr * radius, cy + sy * radius, Math.sin(th) * sr * radius);
        C.push(cr, cg, cb);
      }
    }
    for (let la = 0; la < latSeg; la++) {
      for (let lo = 0; lo < lonSeg; lo++) {
        const lo2 = (lo + 1) % lonSeg;
        const a = start + la * lonSeg + lo;
        const b = start + la * lonSeg + lo2;
        const c = start + (la + 1) * lonSeg + lo;
        const d = start + (la + 1) * lonSeg + lo2;
        I.push(a, c, b, b, c, d);
      }
    }
  }

  function buildColoured(P, C, I, withNormals) {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(P, 3));
    geo.setAttribute('color', new THREE.Float32BufferAttribute(C, 3));
    geo.setIndex(I);
    if (withNormals) geo.computeVertexNormals();
    geo.computeBoundingSphere();
    return geo;
  }

  // Cones: a base plate, two orange bands with a white one between them. About
  // one unit tall after the instance scale, bright enough to read at 300 units.
  const coneGeo = (function () {
    const P = [];
    const C = [];
    const I = [];
    addFrustum(P, C, I, 8, 0, 0.30, 0.05, 0.29, CONE_PLATE, false);
    addFrustum(P, C, I, 8, 0.05, 0.22, 0.30, 0.15, CONE_ORANGE, false);
    addFrustum(P, C, I, 8, 0.30, 0.15, 0.41, 0.117, CONE_BAND, false);
    addFrustum(P, C, I, 8, 0.41, 0.117, 0.62, 0.035, CONE_ORANGE, true);
    return buildColoured(P, C, I);
  })();
  const matCone = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false
  });
  const cones = new THREE.InstancedMesh(coneGeo, matCone, CONE_COUNT);
  cones.name = 'cones';
  cones.frustumCulled = false;
  cones.visible = false;
  group.add(cones);

  // Striped barriers. The stripe texture is painted into a canvas: no external asset.
  function makeStripeTexture() {
    if (typeof document === 'undefined' || !document || typeof document.createElement !== 'function') return null;
    let canvas = null;
    try { canvas = document.createElement('canvas'); } catch (e) { return null; }
    if (!canvas || typeof canvas.getContext !== 'function') return null;
    canvas.width = 128;
    canvas.height = 64;
    let ctx = null;
    try { ctx = canvas.getContext('2d'); } catch (e) { return null; }
    if (!ctx) return null;
    ctx.fillStyle = '#fdfdfa';
    ctx.fillRect(0, 0, 128, 64);
    ctx.fillStyle = '#d7261a';
    for (let x = -64; x < 160; x += 36) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x + 18, 0);
      ctx.lineTo(x + 18 + 64, 64);
      ctx.lineTo(x + 64, 64);
      ctx.closePath();
      ctx.fill();
    }
    const tex = new THREE.CanvasTexture(canvas);
    if (THREE.RepeatWrapping !== undefined) {
      tex.wrapS = THREE.RepeatWrapping;
      tex.wrapT = THREE.RepeatWrapping;
    }
    if (THREE.SRGBColorSpace !== undefined) tex.colorSpace = THREE.SRGBColorSpace;
    return tex;
  }

  const stripeTexture = makeStripeTexture();
  // Unlit, and the only surface in the frame that is pure red, so the barriers
  // stay the most saturated thing in a night district.
  const matBarrier = new THREE.MeshBasicMaterial(
    stripeTexture
      ? { map: stripeTexture, side: THREE.DoubleSide, fog: false }
      : { color: 0xd7261a, side: THREE.DoubleSide, fog: false }
  );
  const barrierGeo = new THREE.BoxGeometry(2.4, 1.25, 0.16);
  const barriers = new THREE.InstancedMesh(barrierGeo, matBarrier, BARRIER_COUNT);
  barriers.name = 'barriers';
  barriers.frustumCulled = false;
  barriers.visible = false;
  group.add(barriers);

  // Barrier lamps. Every barrier carries one amber lamp on a short post, so the closed-off
  // stretch is lit from the ground up instead of being a red stripe on a black district. It is
  // one instanced mesh of 14 small pieces (12 triangles each, one draw call) in the same warm
  // family as the road and the pools, and its colour stays a warm amber rather than neutral
  // white on purpose: warm survives the night white guard, white would be eased away by it.
  const barrierLampGeo = (function () {
    const P = [];
    const C = [];
    const I = [];
    addFrustum(P, C, I, 5, 0, 0.07, 0.86, 0.055, MAST_GREY, false);
    addBox(P, C, I, 0, 0.93, 0, 0.20, 0.10, 0.09, BARRIER_LAMP);
    return buildColoured(P, C, I);
  })();
  const matBarrierLamp = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false
  });
  const barrierLamps = new THREE.InstancedMesh(barrierLampGeo, matBarrierLamp, BARRIER_COUNT);
  barrierLamps.name = 'barrier-lamps';
  barrierLamps.frustumCulled = false;
  barrierLamps.visible = false;
  group.add(barrierLamps);

  // Two floodlight masts: the pole, its flood face and the warm halo plate behind that face
  // are one vertex coloured geometry, so both masts cost a single draw call. The halo is the
  // plate the light throws on the air around the head: a wider, warmer, slightly dimmer quad
  // a few centimetres behind the white face, which is what the bloom picks up, so the machine
  // lamp heads read as the source of the light instead of as two white rectangles. Each
  // instance is turned to face the middle of the works.
  const mastGeo = (function () {
    const P = [];
    const C = [];
    const I = [];
    addFrustum(P, C, I, 6, 0, 0.17, MAST_HEIGHT, 0.11, MAST_GREY, true);
    addBox(P, C, I, 0, MAST_HEIGHT - 1.1, 0, 0.09, 0.22, 0.12, MAST_GREY);
    addQuad(P, C, I, -3.1, MAST_HEIGHT - 1.9, 3.1, MAST_HEIGHT + 1.5, 0.10, HEAD_HALO);
    addQuad(P, C, I, -1.62, MAST_HEIGHT - 1.30, 1.62, MAST_HEIGHT + 0.62, 0.14, HEAD_WARM);
    return buildColoured(P, C, I);
  })();
  const matMast = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false
  });
  const masts = new THREE.InstancedMesh(mastGeo, matMast, 2);
  masts.name = 'flood-masts';
  masts.frustumCulled = false;
  masts.visible = false;
  group.add(masts);

  // Ground light pools: one conforming fan per mast plus one very wide fan under the whole
  // chantier, all additive and vertex shaded warm white to nothing at the rim, so the light
  // fades without a texture. Three fans ride in one geometry, so the works cost one draw call
  // whether it stands on two pools or three. The wide fan is the district wash: a soft, low
  // warm gradient out to WASH_RADIUS, which is what carries the works across their own block
  // instead of leaving a bright strip punched into a black district.
  const POOL_VERTS_PER = 1 + POOL_RINGS * POOL_SEGMENTS;
  const POOL_RING_GAIN = [1.0, 0.80, 0.58, 0.38, 0.20, 0.07];
  const FAN_COUNT = 3;
  const WASH_FAN = 2;
  const WASH_RADIUS = 82.0;         // how far the district wash reaches
  const WASH_GAIN = 0.18;           // and how faint it is at its centre
  const poolPos = new Float32Array(FAN_COUNT * POOL_VERTS_PER * 3);
  const poolCol = new Float32Array(FAN_COUNT * POOL_VERTS_PER * 3);
  const poolIndex = [];
  const poolGeo = new THREE.BufferGeometry();
  (function () {
    scratchColor.setHex(POOL_WARM);
    const wr = scratchColor.r;
    const wg = scratchColor.g;
    const wb = scratchColor.b;
    for (let k = 0; k < FAN_COUNT; k++) {
      const base = k * POOL_VERTS_PER;
      const fanGain = k === WASH_FAN ? WASH_GAIN : 1;
      for (let r = 0; r <= POOL_RINGS; r++) {
        const gain = POOL_RING_GAIN[r] * POOL_GAIN * fanGain;
        const first = r === 0 ? 0 : 1 + (r - 1) * POOL_SEGMENTS;
        const count = r === 0 ? 1 : POOL_SEGMENTS;
        for (let s = 0; s < count; s++) {
          const o = (base + first + s) * 3;
          poolCol[o] = wr * gain;
          poolCol[o + 1] = wg * gain;
          poolCol[o + 2] = wb * gain;
        }
      }
      for (let s = 0; s < POOL_SEGMENTS; s++) {
        const n = (s + 1) % POOL_SEGMENTS;
        poolIndex.push(base, base + 1 + s, base + 1 + n);
      }
      for (let r = 2; r <= POOL_RINGS; r++) {
        const a0 = base + 1 + (r - 2) * POOL_SEGMENTS;
        const b0 = base + 1 + (r - 1) * POOL_SEGMENTS;
        for (let s = 0; s < POOL_SEGMENTS; s++) {
          const n = (s + 1) % POOL_SEGMENTS;
          poolIndex.push(a0 + s, b0 + s, a0 + n, a0 + n, b0 + s, b0 + n);
        }
      }
    }
    poolGeo.setAttribute('position', new THREE.BufferAttribute(poolPos, 3));
    poolGeo.setAttribute('color', new THREE.BufferAttribute(poolCol, 3));
    poolGeo.setIndex(poolIndex);
    poolGeo.setDrawRange(0, poolIndex.length);
    poolGeo.computeBoundingSphere();
  })();
  const matPool = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false,
    transparent: true, depthWrite: false, blending: THREE.AdditiveBlending,
    // Same reasoning as the light strip: one flat fan layer, additive, so the front and back
    // passes draw the same pixels and the phone only pays for one of them.
    forceSinglePass: compact
  });
  const pools = new THREE.Mesh(poolGeo, matPool);
  pools.name = 'flood-pools';
  pools.frustumCulled = false;
  pools.renderOrder = 2;
  pools.visible = false;
  group.add(pools);

  const lampA = new THREE.PointLight(0xffe2c0, 0, LAMP_DISTANCE, 2);
  lampA.name = 'flood-light-a';
  lampA.castShadow = false;
  lampA.visible = true;
  const lampB = new THREE.PointLight(0xffe2c0, 0, LAMP_DISTANCE, 2);
  lampB.name = 'flood-light-b';
  lampB.castShadow = false;
  lampB.visible = true;
  group.add(lampA);
  group.add(lampB);

  // Road crew: hi-vis figures with white helmets, built with the feet at the
  // origin so the per-frame squash bends them from the ground up. The cast is
  // about 2.5 units tall, a quarter taller than the 2.0 unit pedestrians, so it
  // carries at district distance against a nine unit wide road.
  const crewGeo = (function () {
    const P = [];
    const C = [];
    const I = [];
    const s = CREW_SCALE;
    addFrustum(P, C, I, 8, 0, 0.34 * s, 0.95 * s, 0.29 * s, CREW_ORANGE, false);
    addFrustum(P, C, I, 8, 0.95 * s, 0.29 * s, 1.09 * s, 0.285 * s, CREW_TAPE, false);
    addFrustum(P, C, I, 8, 1.09 * s, 0.285 * s, 1.52 * s, 0.245 * s, CREW_ORANGE, true);
    addSphere(P, C, I, 1.66 * s, 0.30 * s, CREW_HELMET, 4, 6);
    return buildColoured(P, C, I);
  })();
  const matCrew = new THREE.MeshBasicMaterial({
    vertexColors: true, side: THREE.DoubleSide, fog: false
  });
  const crew = new THREE.InstancedMesh(crewGeo, matCrew, CREW_COUNT);
  crew.name = 'crew';
  crew.frustumCulled = false;
  crew.visible = false;
  group.add(crew);

  // Pop-up market: one stall geometry (counter plus four posts, vertex coloured)
  // and one awning per stall. Two draw calls, so the market, the works and the
  // road still fit inside the dream group budget after a road is taken.
  const stallGeo = (function () {
    const P = [];
    const C = [];
    const I = [];
    addBox(P, C, I, 0, 0.46, 0, 1.0, 0.45, 0.5, 0x8a6a4a);
    addBox(P, C, I, 0, 0.92, 0, 1.02, 0.03, 0.52, 0x9a7a58);
    for (let sx = -1; sx <= 1; sx += 2) {
      for (let sz = -1; sz <= 1; sz += 2) {
        addPostAt(P, C, I, sx * 0.95, sz * 0.62, 2.2, 0.05, 0xf0e6d2);
      }
    }
    return buildColoured(P, C, I, true);
  })();
  const matStall = new THREE.MeshStandardMaterial({
    vertexColors: true, roughness: 0.88, metalness: 0.0, side: THREE.DoubleSide
  });
  const stalls = new THREE.InstancedMesh(stallGeo, matStall, STALL_COUNT);
  stalls.name = 'market-stalls';
  stalls.frustumCulled = false;
  stalls.visible = false;
  group.add(stalls);

  const awningGeo = new THREE.BoxGeometry(2.3, 0.1, 1.5);
  const matAwning = new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.8, metalness: 0.0 });
  const awnings = new THREE.InstancedMesh(awningGeo, matAwning, STALL_COUNT);
  awnings.name = 'market-awnings';
  awnings.frustumCulled = false;
  awnings.visible = false;
  group.add(awnings);
  const AWNING_TINT = [0xe4572e, 0xf2a541, 0x2f8f83, 0xd0506f];
  for (let i = 0; i < STALL_COUNT; i++) {
    scratchColor.set(AWNING_TINT[i % AWNING_TINT.length]);
    awnings.setColorAt(i, scratchColor);
  }
  if (awnings.instanceColor) awnings.instanceColor.needsUpdate = true;

  // --------------------------------------------------------------- storage
  function readTaken() {
    let raw = null;
    try {
      if (typeof localStorage !== 'undefined' && localStorage) raw = localStorage.getItem(STORE_KEY);
    } catch (e) { raw = null; }
    if (typeof raw !== 'string' || raw.length === 0) return [];
    let parsed = null;
    try { parsed = JSON.parse(raw); } catch (e) { parsed = null; }
    if (!parsed || Object.prototype.toString.call(parsed) !== '[object Array]') return [];
    const out = [];
    for (let i = 0; i < parsed.length; i++) {
      if (typeof parsed[i] === 'string' && parsed[i].length > 0) out.push(parsed[i]);
    }
    return out;
  }

  function writeTaken(list) {
    try {
      if (typeof localStorage === 'undefined' || !localStorage) return;
      localStorage.setItem(STORE_KEY, JSON.stringify(list));
    } catch (e) { /* private mode, quota, file://: the dream still works */ }
  }

  function storeSentence(sentence) {
    if (takenList.indexOf(sentence) === -1) takenList.push(sentence);
    const list = readTaken();                 // read
    if (list.indexOf(sentence) !== -1) return; // already stored: nothing to merge
    list.push(sentence);                       // merge
    writeTaken(list);                          // write
  }

  // ------------------------------------------------------------------ pair
  function validPair(p) {
    if (!p || typeof p !== 'object') return false;
    const qs = modelRef && modelRef.quarters;
    if (!qs || !qs.length) return false;
    const a = p.a;
    const b = p.b;
    if (typeof a !== 'number' || typeof b !== 'number') return false;
    if (a === b || a < 0 || b < 0 || a >= qs.length || b >= qs.length) return false;
    return !!(qs[a] && qs[b]);
  }

  function strongestPair() {
    const pairs = modelRef && modelRef.pairs;
    if (!pairs || !pairs.length) return null;
    let best = null;
    let bestStrength = -Infinity;
    for (let i = 0; i < pairs.length; i++) {
      const p = pairs[i];
      if (!validPair(p)) continue;
      const s = (typeof p.strength === 'number' && isFinite(p.strength)) ? p.strength : 0;
      if (s > bestStrength) { bestStrength = s; best = p; }
    }
    return best;
  }

  function pickPair() {
    pairA = -1;
    pairB = -1;
    pairKey = '';
    hasPair = false;
    pairWords.length = 0;
    const qs = modelRef && modelRef.quarters;
    if (!qs || qs.length < 2) return;

    let chosen = validPair(modelRef.dreamPair) ? modelRef.dreamPair : null;
    if (!chosen) chosen = strongestPair();          // fallback: strongest pair overall
    if (!chosen) return;

    pairA = chosen.a;
    pairB = chosen.b;
    pairKey = pairA + ':' + pairB;
    hasPair = true;
    const words = chosen.words;
    if (words && words.length) {
      for (let i = 0; i < words.length; i++) {
        if (typeof words[i] === 'string' && words[i].length > 0) pairWords.push(words[i]);
      }
    }
  }

  function quarterName(idx) {
    const qs = modelRef && modelRef.quarters;
    const q = (qs && qs[idx]) ? qs[idx] : null;
    if (!q) return 'somewhere else';
    return (typeof q.name === 'string' && q.name.length) ? q.name : 'somewhere else';
  }

  function buildCardTexts() {
    cardTexts.length = 0;
    if (!hasPair) return;
    // A pair whose road is already taken has nothing to offer: the asphalt, the light strip and
    // the market are the answer the owner already gave. Without this, a pair carrying two
    // hypotheses would re-offer its second word after the first was taken, against a road that
    // is already permanent.
    if (roadMode === 2 && permKey === pairKey) return;
    const nameA = quarterName(pairA);
    const nameB = quarterName(pairB);
    for (let i = 0; i < pairWords.length && cardTexts.length < CARD_LIMIT; i++) {
      const w = pairWords[i];
      const line = "The word '" + w + "' kept appearing in both " + nameA + ' and ' + nameB + '. Open a note that joins them?';
      if (takenList.indexOf(line) !== -1) continue;    // never re-offer what was taken
      if (mutedSentences.indexOf(line) !== -1) continue; // never repeat what was ignored
      cardTexts.push(line);
    }
  }

  // An ignored hypothesis is never pushed again during this data set: no reminder,
  // no badge, nothing that counts how many times it was turned down.
  function muteOffered() {
    for (let i = 0; i < cardTexts.length; i++) {
      if (mutedSentences.indexOf(cardTexts[i]) === -1) mutedSentences.push(cardTexts[i]);
    }
  }

  // ---------------------------------------------------------------- layout
  // Quarter centres are world data (Vector3 on the ground); model quarters only
  // carry the name and the stats. The model copy is a fallback for bare models.
  function quarterCenter(idx) {
    const wq = world && world.quarters;
    let w = (wq && wq[idx]) ? wq[idx] : null;
    if ((!w || !w.center) && wq && wq.length) {
      for (let i = 0; i < wq.length; i++) {
        if (wq[i] && wq[i].idx === idx) { w = wq[i]; break; }
      }
    }
    if (w && w.center && typeof w.center.x === 'number' && typeof w.center.z === 'number') return w.center;
    const mq = modelRef && modelRef.quarters;
    const m = (mq && mq[idx]) ? mq[idx] : null;
    if (m && m.center && typeof m.center.x === 'number' && typeof m.center.z === 'number') return m.center;
    return null;
  }

  function layoutRoad() {
    if (!hasPair) return false;
    const ca = quarterCenter(pairA);
    const cb = quarterCenter(pairB);
    if (!ca || !cb) return false;
    const ax = ca.x;
    const az = ca.z;
    const bx = cb.x;
    const bz = cb.z;

    const span = Math.sqrt((bx - ax) * (bx - ax) + (bz - az) * (bz - az));
    if (span < 0.001) return false;

    // The provisional road runs straight between the two quarter centres: the
    // district keeps that strip free of buildings, so a bow would only clip
    // blocks. The control points stay on the segment; the curve is the segment.
    const mx = (ax + bx) * 0.5;
    const mz = (az + bz) * 0.5;

    curvePoints[0].set(ax, 0, az);
    curvePoints[1].set(mx, 0, mz);
    curvePoints[2].set(bx, 0, bz);
    const curve = new THREE.CatmullRomCurve3(curvePoints);

    for (let i = 0; i < SAMPLES; i++) {
      curve.getPoint(i / SEGMENTS, samplePoint);
      sampleX[i] = samplePoint.x;
      sampleZ[i] = samplePoint.z;
    }

    for (let i = 0; i < SAMPLES; i++) {
      const i0 = i > 0 ? i - 1 : 0;
      const i1 = i < SEGMENTS ? i + 1 : SEGMENTS;
      let tx = sampleX[i1] - sampleX[i0];
      let tz = sampleZ[i1] - sampleZ[i0];
      const len = Math.sqrt(tx * tx + tz * tz);
      if (len > 1e-6) { tx /= len; tz /= len; } else { tx = 0; tz = 1; }
      sampleTX[i] = tx;
      sampleTZ[i] = tz;
      sampleNX[i] = -tz;
      sampleNZ[i] = tx;
    }

    const provPos = roadProvRibbon.pos;
    for (let i = 0; i < SAMPLES; i++) {
      const nx = sampleNX[i];
      const nz = sampleNZ[i];
      const cx = sampleX[i];
      const cz = sampleZ[i];
      const lx = cx + nx * HALF_WIDTH;
      const lz = cz + nz * HALF_WIDTH;
      const rx = cx - nx * HALF_WIDTH;
      const rz = cz - nz * HALF_WIDTH;
      const o = i * 6;
      provPos[o] = lx;
      provPos[o + 1] = ground(lx, lz) + ROAD_LIFT;
      provPos[o + 2] = lz;
      provPos[o + 3] = rx;
      provPos[o + 4] = ground(rx, rz) + ROAD_LIFT;
      provPos[o + 5] = rz;
    }

    // The light strip: the same samples, one lane out to 2.2 times the painted
    // half width, so it hugs the terrain and never crosses a block.
    for (let i = 0; i < SAMPLES; i++) {
      const nx = sampleNX[i];
      const nz = sampleNZ[i];
      const cx = sampleX[i];
      const cz = sampleZ[i];
      for (let l = 0; l < GLOW_LANES; l++) {
        const off = GLOW_OFFSET[l];
        const gxp = cx + nx * off;
        const gzp = cz + nz * off;
        const o = (i * GLOW_LANES + l) * 3;
        glowPos[o] = gxp;
        glowPos[o + 1] = ground(gxp, gzp) + ROAD_LIFT + GLOW_LIFT;
        glowPos[o + 2] = gzp;
      }
    }

    // Dashed centre line: whole segments per dash, so a dash stays about
    // DASH_LENGTH long and DASH_HALF wide whatever the distance between the two
    // quarters. A gap segment collapses its four corners onto one point, so it
    // costs triangles but draws nothing. The dashes still ride the ribbon's own
    // geometry and index buffer, so they grow and fade with it for free.
    const segLen = span / SEGMENTS;
    let dashSegs = Math.round(DASH_LENGTH / segLen);
    if (dashSegs < 1) dashSegs = 1;
    let gapSegs = Math.round(dashSegs * DASH_GAP_RATIO);
    if (gapSegs < 1) gapSegs = 1;
    const dashPeriod = dashSegs + gapSegs;
    for (let i = 0; i < SEGMENTS; i++) {
      const nx = sampleNX[i];
      const nz = sampleNZ[i];
      const dxs = sampleX[i + 1] - sampleX[i];
      const dzs = sampleZ[i + 1] - sampleZ[i];
      const o = (DASH_BASE + i * 4) * 3;
      const phase = i % dashPeriod;
      if (phase >= dashSegs) {
        const cxp = sampleX[i] + dxs * 0.5;
        const czp = sampleZ[i] + dzs * 0.5;
        const cyp = ground(cxp, czp) + ROAD_LIFT + DASH_LIFT;
        for (let v = 0; v < 4; v++) {
          const vo = o + v * 3;
          provPos[vo] = cxp;
          provPos[vo + 1] = cyp;
          provPos[vo + 2] = czp;
        }
        continue;
      }
      const runFrom = phase === 0 ? DASH_FROM : 0;
      const runTo = phase === dashSegs - 1 ? DASH_TO : 1;
      let px = sampleX[i] + dxs * runFrom + nx * DASH_HALF;
      let pz = sampleZ[i] + dzs * runFrom + nz * DASH_HALF;
      provPos[o] = px;
      provPos[o + 1] = ground(px, pz) + ROAD_LIFT + DASH_LIFT;
      provPos[o + 2] = pz;
      px = sampleX[i] + dxs * runFrom - nx * DASH_HALF;
      pz = sampleZ[i] + dzs * runFrom - nz * DASH_HALF;
      provPos[o + 3] = px;
      provPos[o + 4] = ground(px, pz) + ROAD_LIFT + DASH_LIFT;
      provPos[o + 5] = pz;
      px = sampleX[i] + dxs * runTo + nx * DASH_HALF;
      pz = sampleZ[i] + dzs * runTo + nz * DASH_HALF;
      provPos[o + 6] = px;
      provPos[o + 7] = ground(px, pz) + ROAD_LIFT + DASH_LIFT;
      provPos[o + 8] = pz;
      px = sampleX[i] + dxs * runTo - nx * DASH_HALF;
      pz = sampleZ[i] + dzs * runTo - nz * DASH_HALF;
      provPos[o + 9] = px;
      provPos[o + 10] = ground(px, pz) + ROAD_LIFT + DASH_LIFT;
      provPos[o + 11] = pz;
    }

    // The two crisp edge lines: one continuous 0.46 unit white band just inside
    // each painted edge, built per segment so it follows the terrain exactly. It
    // rides the same geometry as the road, so it grows with the paint for free.
    for (let i = 0; i < SEGMENTS; i++) {
      const nx = sampleNX[i];
      const nz = sampleNZ[i];
      const e = (EDGE_BASE + i * 8) * 3;
      for (let s = 0; s < 2; s++) {
        const side = s === 0 ? 1 : -1;
        const out = side * EDGE_OUTER;
        const inn = side * EDGE_INNER;
        const o = e + s * 12;
        let px = sampleX[i] + nx * out;
        let pz = sampleZ[i] + nz * out;
        provPos[o] = px;
        provPos[o + 1] = ground(px, pz) + ROAD_LIFT + EDGE_LIFT;
        provPos[o + 2] = pz;
        px = sampleX[i] + nx * inn;
        pz = sampleZ[i] + nz * inn;
        provPos[o + 3] = px;
        provPos[o + 4] = ground(px, pz) + ROAD_LIFT + EDGE_LIFT;
        provPos[o + 5] = pz;
        px = sampleX[i + 1] + nx * out;
        pz = sampleZ[i + 1] + nz * out;
        provPos[o + 6] = px;
        provPos[o + 7] = ground(px, pz) + ROAD_LIFT + EDGE_LIFT;
        provPos[o + 8] = pz;
        px = sampleX[i + 1] + nx * inn;
        pz = sampleZ[i + 1] + nz * inn;
        provPos[o + 9] = px;
        provPos[o + 10] = ground(px, pz) + ROAD_LIFT + EDGE_LIFT;
        provPos[o + 11] = pz;
      }
    }

    roadGhostRibbon.pos.set(provPos);
    roadPermRibbon.pos.set(provPos);
    roadProvRibbon.attr.needsUpdate = true;
    roadGhostRibbon.attr.needsUpdate = true;
    roadPermRibbon.attr.needsUpdate = true;
    roadProvRibbon.geo.computeVertexNormals();
    roadGhostRibbon.geo.computeVertexNormals();
    roadPermRibbon.geo.computeVertexNormals();
    glowGeo.attributes.position.needsUpdate = true;
    glowGeo.computeBoundingSphere();

    // The measured works, exposed read-only through chantierSpan and
    // chantierCenter so a screenshot harness knows where to aim: the length laid
    // along the samples, and the middle of the ribbon at road level.
    let length = 0;
    for (let i = 0; i < SEGMENTS; i++) {
      const lx = sampleX[i + 1] - sampleX[i];
      const lz = sampleZ[i + 1] - sampleZ[i];
      length += Math.sqrt(lx * lx + lz * lz);
    }
    chantierSpan = length;

    const mid = SEGMENTS >> 1;
    centreVec.set(sampleX[mid], ground(sampleX[mid], sampleZ[mid]) + 1.6, sampleZ[mid]);
    chantierCentre.set(sampleX[mid], ground(sampleX[mid], sampleZ[mid]) + ROAD_LIFT, sampleZ[mid]);
    return true;
  }

  function layoutWorks() {
    if (!hasPair) return;
    for (let k = 0; k < CONE_COUNT; k++) {
      const t = CONE_COUNT > 1 ? k / (CONE_COUNT - 1) : 0;
      let i = Math.round(6 + t * (SAMPLES - 13));
      if (i < 0) i = 0;
      if (i > SEGMENTS) i = SEGMENTS;
      const side = (k % 2 === 0) ? 1 : -1;
      const off = (HALF_WIDTH - 0.75) * side;
      const px = sampleX[i] + sampleNX[i] * off;
      const pz = sampleZ[i] + sampleNZ[i] * off;
      dummy.position.set(px, ground(px, pz) + ROAD_LIFT + 0.02, pz);
      dummy.rotation.set(0, 0, 0);
      dummy.scale.set(CONE_SCALE, CONE_SCALE, CONE_SCALE);
      dummy.updateMatrix();
      cones.setMatrixAt(k, dummy.matrix);
    }
    cones.instanceMatrix.needsUpdate = true;

    // Barriers: seven per edge, set just outside the fresh paint and spaced
    // tightly enough to read as a closed-off stretch in the middle of the works.
    const barrierPerSide = BARRIER_COUNT >> 1;
    const barrierFirst = (SEGMENTS >> 1) - ((barrierPerSide - 1) * BARRIER_STEP >> 1);
    for (let k = 0; k < BARRIER_COUNT; k++) {
      const side = (k % 2 === 0) ? -1 : 1;
      let i = barrierFirst + (k >> 1) * BARRIER_STEP;
      if (i < 0) i = 0;
      if (i > SEGMENTS) i = SEGMENTS;
      const off = (HALF_WIDTH + 0.25) * side;
      const px = sampleX[i] + sampleNX[i] * off;
      const pz = sampleZ[i] + sampleNZ[i] * off;
      const py = ground(px, pz) + 0.625;
      const rotY = Math.atan2(-sampleTZ[i], sampleTX[i]);
      barrierBX[k] = px;
      barrierBZ[k] = pz;
      barrierBY[k] = py;
      barrierR[k] = rotY;
      dummy.position.set(px, py, pz);
      dummy.rotation.set(0, rotY, 0);
      dummy.scale.set(1, 1, 1);
      dummy.updateMatrix();
      barriers.setMatrixAt(k, dummy.matrix);
    }
    barriers.instanceMatrix.needsUpdate = true;

    // One lamp per barrier, standing on the barrier's top edge at the same yaw, so the pair
    // reads as one piece of furniture and the stripe never carries a floating light.
    for (let k = 0; k < BARRIER_COUNT; k++) {
      dummy.position.set(barrierBX[k], barrierBY[k] + 0.62, barrierBZ[k]);
      dummy.rotation.set(0, barrierR[k], 0);
      dummy.scale.set(1, 1, 1);
      dummy.updateMatrix();
      barrierLamps.setMatrixAt(k, dummy.matrix);
    }
    barrierLamps.instanceMatrix.needsUpdate = true;

    for (let k = 0; k < 2; k++) {
      const i = k === 0 ? MAST_SAMPLE_A : MAST_SAMPLE_B;
      const side = k === 0 ? 1 : -1;
      const off = (HALF_WIDTH + 1.6) * side;
      const px = sampleX[i] + sampleNX[i] * off;
      const pz = sampleZ[i] + sampleNZ[i] * off;
      const gy = ground(px, pz);

      dummy.position.set(px, gy, pz);
      let fx = centreVec.x - px;
      let fz = centreVec.z - pz;
      const fl = Math.sqrt(fx * fx + fz * fz);
      if (fl > 1e-6) { fx /= fl; fz /= fl; } else { fx = 0; fz = 1; }
      dummy.rotation.set(0, Math.atan2(fx, fz), 0);
      dummy.scale.set(1, 1, 1);
      dummy.updateMatrix();
      masts.setMatrixAt(k, dummy.matrix);

      const lamp = k === 0 ? lampA : lampB;
      lamp.position.set(px + fx * 0.7, gy + MAST_HEIGHT - 0.6, pz + fz * 0.7);

      // The pool is centred just off the paint, so one disc washes the fresh
      // ribbon, the mast foot and the ground beside it. It follows the terrain.
      const po = (HALF_WIDTH + 6.0) * side;
      const cx = sampleX[i] + sampleNX[i] * po;
      const cz = sampleZ[i] + sampleNZ[i] * po;
      const base = k * POOL_VERTS_PER;
      let o = base * 3;
      poolPos[o] = cx;
      poolPos[o + 1] = ground(cx, cz) + 0.36;
      poolPos[o + 2] = cz;
      for (let r = 1; r <= POOL_RINGS; r++) {
        const rr = (POOL_RADIUS * r) / POOL_RINGS;
        for (let s = 0; s < POOL_SEGMENTS; s++) {
          const a = (s / POOL_SEGMENTS) * Math.PI * 2;
          const rx = cx + Math.cos(a) * rr;
          const rz = cz + Math.sin(a) * rr;
          o = (base + 1 + (r - 1) * POOL_SEGMENTS + s) * 3;
          poolPos[o] = rx;
          poolPos[o + 1] = ground(rx, rz) + 0.32;
          poolPos[o + 2] = rz;
        }
      }
    }
    // The district wash fan: one wide, faint pool centred on the middle of the ribbon, draping
    // whatever it crosses, so the light around the works falls off instead of stopping.
    const wbase = WASH_FAN * POOL_VERTS_PER;
    let wo = wbase * 3;
    poolPos[wo] = centreVec.x;
    poolPos[wo + 1] = ground(centreVec.x, centreVec.z) + 0.30;
    poolPos[wo + 2] = centreVec.z;
    for (let r = 1; r <= POOL_RINGS; r++) {
      const rr = (WASH_RADIUS * r) / POOL_RINGS;
      for (let s = 0; s < POOL_SEGMENTS; s++) {
        const a = (s / POOL_SEGMENTS) * Math.PI * 2;
        const rx = centreVec.x + Math.cos(a) * rr;
        const rz = centreVec.z + Math.sin(a) * rr;
        wo = (wbase + 1 + (r - 1) * POOL_SEGMENTS + s) * 3;
        poolPos[wo] = rx;
        poolPos[wo + 1] = ground(rx, rz) + 0.27;
        poolPos[wo + 2] = rz;
      }
    }

    masts.instanceMatrix.needsUpdate = true;
    poolGeo.attributes.position.needsUpdate = true;
  }

  function layoutMarket() {
    if (!hasPair) return;
    for (let k = 0; k < STALL_COUNT; k++) {
      const t = (k + 0.5) / STALL_COUNT;
      let i = Math.round(10 + t * (SAMPLES - 21));
      if (i < 0) i = 0;
      if (i > SEGMENTS) i = SEGMENTS;
      const side = (k % 2 === 0) ? 1 : -1;
      const off = 2.15 * side;
      const px = sampleX[i] + sampleNX[i] * off;
      const pz = sampleZ[i] + sampleNZ[i] * off;
      const gy = ground(px, pz) + ROAD_LIFT;
      const rotY = Math.atan2(-sampleTZ[i], sampleTX[i]);

      dummy.position.set(px, gy, pz);
      dummy.rotation.set(0, rotY, 0);
      dummy.scale.set(1, 1, 1);
      dummy.updateMatrix();
      stalls.setMatrixAt(k, dummy.matrix);

      dummy.position.set(px, gy + 2.16, pz);
      dummy.rotation.set(0, rotY, side * 0.14);
      dummy.scale.set(1, 1, 1);
      dummy.updateMatrix();
      awnings.setMatrixAt(k, dummy.matrix);
    }
    stalls.instanceMatrix.needsUpdate = true;
    awnings.instanceMatrix.needsUpdate = true;
  }

  // The strip's draw range rides whichever ribbon is on screen: the growing
  // provisional paint, or the ghost while an ignored one fades away.
  function setGlowRange(quads) {
    glowGeo.setDrawRange(0, quads * GLOW_SEGMENT_INDICES);
    roadGlow.visible = quads > 0;
  }

  function applyGrowth() {
    let quads = Math.floor(SEGMENTS * growth + 1e-6);
    if (quads < 0) quads = 0;
    if (quads > SEGMENTS) quads = SEGMENTS;
    roadProvRibbon.geo.setDrawRange(0, quads * SEGMENT_INDICES);
    // While a ghost is still fading the strip follows that ghost, so the light
    // does not jump when the next shift is only waiting to start. Once the paint
    // is growing again the strip is solid, whatever opacity the fade left behind.
    if (!fading) {
      matGlow.opacity = 1;
      setGlowRange(quads);
    }
  }

  // ------------------------------------------------------------------ card DOM
  function resolveDoc() {
    const root = shell.cardsRoot;
    if (root && root.ownerDocument) return root.ownerDocument;
    if (typeof document !== 'undefined' && document) return document;
    return null;
  }

  function hideCards() {
    for (let i = 0; i < cards.length; i++) {
      const rec = cards[i];
      if (!rec) continue;
      if (rec.btn && rec.handler && typeof rec.btn.removeEventListener === 'function') {
        try { rec.btn.removeEventListener('click', rec.handler); } catch (e) { /* ignore */ }
      }
      const el = rec.el;
      if (el) {
        if (el.parentNode && typeof el.parentNode.removeChild === 'function') el.parentNode.removeChild(el);
        else if (typeof el.remove === 'function') el.remove();
      }
    }
    cards.length = 0;
  }

  function showCards() {
    const doc = resolveDoc();
    const root = shell.cardsRoot;
    if (!doc || typeof doc.createElement !== 'function') return;
    if (!root || typeof root.appendChild !== 'function') return;
    const limit = cardTexts.length < CARD_LIMIT ? cardTexts.length : CARD_LIMIT;
    for (let i = 0; i < limit; i++) {
      const line = doc.createElement('div');
      line.className = 'dream-card';
      const text = doc.createElement('p');
      text.textContent = cardTexts[i];
      const button = doc.createElement('button');
      button.type = 'button';
      button.textContent = 'Take it';
      const record = { el: line, sentence: cardTexts[i], btn: button, handler: null, done: false };
      // The cards land on the darkest version of the scene, just under the works, so their
      // edge carries a warm rim instead of a flat panel line: the panel then belongs to the
      // light that is carrying the frame rather than reading as a dialog dropped on top of it.
      // Set inline from here because dream.mjs owns these nodes; the shell keeps its own rules.
      if (line.style) {
        line.style.boxShadow =
          '0 14px 36px rgba(6, 4, 3, 0.5), 0 0 0 1px rgba(255, 206, 150, 0.20), 0 0 26px rgba(255, 176, 92, 0.26)';
      }
      record.handler = function () { take(record); };
      if (typeof button.addEventListener === 'function') button.addEventListener('click', record.handler);
      line.appendChild(text);
      line.appendChild(button);
      root.appendChild(line);
      cards.push(record);
    }
  }

  // --------------------------------------------------------------- lamps/works
  // The two floodlights stay in the render list for the whole session and only their
  // intensity is animated. Toggling `visible` would change the scene light count, which
  // makes three recompile every material in the city on the first sleep: measured as a
  // 44 to 77 ms stall in the bench, right where the dream has to be smooth.
  function lampsOn(on) {
    lampA.visible = true;
    lampA.intensity = on ? LAMP_INTENSITY : 0;
    lampB.visible = true;
    lampB.intensity = on ? LAMP_INTENSITY : 0;
  }

  function hideWorks() {
    cones.visible = false;
    masts.visible = false;
    pools.visible = false;
    crew.visible = false;
    barrierLamps.visible = false;
    packing = false;
    packT = 0;
    lampsOn(false);
  }

  function showMarket(on) {
    const show = on && dressing;
    stalls.visible = show;
    awnings.visible = show;
  }

  // ------------------------------------------------------------------ vans
  function dispatchVans() {
    if (!hasPair) return;
    const vans = agents && agents.vans;
    if (!vans || typeof vans.dispatch !== 'function') return;
    vans.dispatch(pairA, pairB, 'sign');
    vans.dispatch(pairB, pairA, 'sign');
  }

  // ------------------------------------------------------------- transitions
  function startFade() {
    if (roadMode === 2) return;         // a permanent road never fades
    if (!hasPair) return;
    let quads = Math.floor(SEGMENTS * growth + 1e-6);
    if (quads < 0) quads = 0;
    if (quads > SEGMENTS) quads = SEGMENTS;
    roadGhostRibbon.geo.setDrawRange(0, quads * SEGMENT_INDICES);
    matGhost.opacity = 1;
    roadGhost.visible = quads > 0;
    matGlow.opacity = 1;
    setGlowRange(quads);
    roadProv.visible = false;
    roadMode = 0;
    fading = roadGhost.visible;
    fadeT = 0;
    cones.visible = false;
    masts.visible = false;
    pools.visible = false;
    crew.visible = false;
    barrierLamps.visible = false;
    lampsOn(false);
    if (barriers.visible) {
      packing = true;
      packT = 0;
    } else {
      packing = false;
      packT = 0;
    }
  }

  function take(record) {
    if (!record || record.done) return;
    record.done = true;
    storeSentence(record.sentence);

    if (hasPair) {
      roadPermRibbon.geo.setDrawRange(0, RING);
      roadPerm.visible = true;
      roadProv.visible = false;
      roadGhost.visible = false;
      roadGlow.visible = false;
      matGlow.opacity = 1;
      fading = false;
      fadeT = 0;
      roadMode = 2;
      permKey = pairKey;
      hideWorks();
      barriers.visible = false;
      showMarket(true);
    }

    hideCards();
    dataElapsed = 0;
    dataLast = null;
    worksDelay = 0;
    state = 'awake';
    if (typeof shell.setNightOverride === 'function') shell.setNightOverride(false);
  }

  function ignoreNow() {
    if (state !== 'offered') return;
    muteOffered();
    hideCards();
    dataElapsed = 0;
    dataLast = null;
    state = 'awake';
    startFade();
  }

  function revealCards() {
    if (state !== 'waking') return;
    glideDone = true;
    if (cardTexts.length === 0) {
      // Nothing to offer: the works simply come down again.
      state = 'awake';
      startFade();
      return;
    }
    state = 'offered';
    dataElapsed = 0;
    showCards();
  }

  function onGlideDone() {
    glideDone = true;
  }

  // ---------------------------------------------------------------- public flow
  function sleep() {
    if (state === 'dreaming' || state === 'waking') return;

    let hold = 0;
    if (state === 'offered') {
      // An offer that was never taken is being ignored: it packs away now, and the
      // sentence is not pushed again during this data set.
      muteOffered();
      hideCards();
      dataElapsed = 0;
      dataLast = null;
      startFade();
      hold = fading ? FADE_SECONDS : 0;
    } else if (fading) {
      hold = FADE_SECONDS - fadeT;
      if (hold < 0) hold = 0;
    }

    state = 'dreaming';
    crewClock = 0;
    glideElapsed = 0;
    glideDone = false;
    growthHold = hold;
    // When an ignored offering is packing away, the new shift arrives after it is gone.
    worksDelay = hold > 0 ? PACK_SECONDS : 0;
    if (typeof shell.setNightOverride === 'function') shell.setNightOverride(true);

    const permanent = (roadMode === 2 && permKey === pairKey);

    if (hasPair) {
      if (permanent) {
        // The road of this pair is already asphalt: keep it, the crew just works again.
        growth = 1;
        roadProv.visible = false;
        roadGhost.visible = false;
        roadGlow.visible = false;
      } else {
        roadMode = 1;
        growth = 0;
        applyGrowth();
        roadProv.visible = true;
      }
      if (worksDelay > 0) {
        cones.visible = false;
        masts.visible = false;
        pools.visible = false;
        crew.visible = false;
        barrierLamps.visible = false;
        lampsOn(false);
      } else {
        layoutWorks();
        cones.visible = dressing;
        barriers.visible = dressing;
        barrierLamps.visible = dressing;
        masts.visible = true;
        pools.visible = true;
        crew.visible = dressing;
        packing = false;
        packT = 0;
        lampsOn(true);
      }
      showMarket(permanent);
      takenList = readTaken();
      buildCardTexts();
      dispatchVans();
    } else {
      // Fewer than two quarters: only the night falls, and no card can exist.
      cardTexts.length = 0;
      takenList = readTaken();
      hideWorks();
      barriers.visible = false;
      showMarket(false);
    }
  }

  function wake() {
    if (state !== 'dreaming') return;
    worksDelay = 0;
    if (typeof shell.setNightOverride === 'function') shell.setNightOverride(false);
    crew.visible = false;
    lampsOn(false);
    masts.visible = false;
    pools.visible = false;
    if (!hasPair) {
      state = 'awake';
      return;
    }
    state = 'waking';
    glideElapsed = 0;
    glideDone = false;
    if (rig && typeof rig.glideTo === 'function') {
      rig.glideTo(centreVec, WAKE_MS, onGlideDone);
    }
  }

  function toggle() {
    if (state === 'dreaming') {
      wake();
      return;
    }
    if (state === 'awake' || state === 'offered') sleep();
  }

  function notifyGap(gapStart, gapEnd) {
    const dur = (typeof gapStart === 'number' && typeof gapEnd === 'number') ? gapEnd - gapStart : 0;
    if (!(dur > GAP_MIN_MS)) return;
    gapEndAt = gapEnd;
    gapSleeping = true;
    if (state !== 'dreaming' && state !== 'waking') sleep();
  }

  function setDataModel(nextModel) {
    hideCards();
    mutedSentences.length = 0;
    fading = false;
    fadeT = 0;
    packing = false;
    packT = 0;
    growth = 0;
    growthHold = 0;
    worksDelay = 0;
    roadMode = 0;
    permKey = '';
    dataElapsed = 0;
    dataLast = null;
    glideDone = false;
    glideElapsed = 0;
    if (state !== 'awake' && typeof shell.setNightOverride === 'function') shell.setNightOverride(false);
    state = 'awake';

    modelRef = nextModel || null;
    pickPair();
    roadProv.visible = false;
    roadGhost.visible = false;
    roadPerm.visible = false;
    roadGlow.visible = false;
    matGlow.opacity = 1;
    hideWorks();
    barriers.visible = false;
    barrierLamps.visible = false;
    showMarket(false);
    cardTexts.length = 0;

    if (hasPair && layoutRoad()) {
      applyGrowth();
      layoutWorks();
      layoutMarket();
      takenList = readTaken();
      buildCardTexts();
    } else {
      // A pair whose quarters have no centres cannot be laid: treat it as no pair.
      hasPair = false;
      pairA = -1;
      pairB = -1;
      pairKey = '';
      pairWords.length = 0;
      cardTexts.length = 0;
      chantierCentre.set(0, 0, 0);
      chantierSpan = 0;
    }
  }

  // ---------------------------------------------------------------- per frame
  function updateCrew(step) {
    crewClock += step;
    let tip = Math.round(growth * SEGMENTS);
    if (tip < 0) tip = 0;
    if (tip > SEGMENTS) tip = SEGMENTS;
    for (let k = 0; k < CREW_COUNT; k++) {
      // Two groups: one walks the growing front of the paint, one stands in the
      // first floodlight pool, so the cast is lit from either side of the works.
      const i = crewAnchor[k] > 0.5 ? MAST_SAMPLE_A : tip;
      const nx = sampleNX[i];
      const nz = sampleNZ[i];
      const ux = sampleTX[i];
      const uz = sampleTZ[i];
      const ang = crewPhase[k] + crewClock * crewSpeed[k];
      const sway = Math.sin(ang);
      const along = crewAlong[k] + Math.cos(ang) * 0.9;
      const lat = crewSide[k] + sway * 0.25;
      const px = sampleX[i] + ux * along + nx * lat;
      const pz = sampleZ[i] + uz * along + nz * lat;
      const gy = ground(px, pz);
      const bend = 0.5 + 0.5 * Math.sin(crewClock * 1.7 + crewPhase[k]);
      const sy = 1 - 0.22 * bend;
      dummy.position.set(px, gy + ROAD_LIFT + 0.02, pz);
      dummy.rotation.set(0, Math.atan2(-nx * crewSide[k], -nz * crewSide[k]) + sway * 0.25, 0);
      dummy.scale.set(1, sy, 1);
      dummy.updateMatrix();
      crew.setMatrixAt(k, dummy.matrix);
    }
    crew.instanceMatrix.needsUpdate = true;
  }

  function updatePacking(step) {
    packT += step;
    const k = packT / PACK_SECONDS;
    if (k >= 1) {
      packing = false;
      barriers.visible = false;
      return;
    }
    const lift = 2.6 * k;
    const squash = 1 - 0.9 * k;
    for (let i = 0; i < BARRIER_COUNT; i++) {
      dummy.position.set(barrierBX[i], barrierBY[i] + lift, barrierBZ[i]);
      dummy.rotation.set(0, barrierR[i], 0);
      dummy.scale.set(1, squash, 1);
      dummy.updateMatrix();
      barriers.setMatrixAt(i, dummy.matrix);
    }
    barriers.instanceMatrix.needsUpdate = true;
  }

  function addDataTime(tData) {
    if (dataLast === null) {
      dataLast = tData;
      return;
    }
    let d = tData - dataLast;
    if (d < 0) {
      const t0 = (modelRef && typeof modelRef.t0 === 'number') ? modelRef.t0 : 0;
      const t1 = (modelRef && typeof modelRef.t1 === 'number') ? modelRef.t1 : 0;
      if (t1 > t0) {
        const before = Math.max(0, t1 - Math.min(dataLast, t1));
        const after = Math.max(0, tData - t0);
        d = before + HOUR_MS + after;
      } else {
        d = 0;
      }
      if (!(d > 0)) d = 0;
    }
    dataElapsed += d;
    dataLast = tData;
  }

  function update(dt, sim) {
    const step = (typeof dt === 'number' && dt > 0) ? dt : 0;
    const tData = (sim && typeof sim.tData === 'number') ? sim.tData : null;

    // Leaving a long gap brings the district back.
    if (gapSleeping && tData !== null && tData >= gapEndAt) {
      gapSleeping = false;
      if (state === 'dreaming') wake();
    }

    if (state === 'dreaming') {
      if (step > 0) {
        if (worksDelay > 0) {
          worksDelay -= step;
          if (worksDelay <= 0) {
            worksDelay = 0;
            packing = false;
            packT = 0;
            layoutWorks();
            cones.visible = dressing;
            barriers.visible = dressing;
            barrierLamps.visible = dressing;
            masts.visible = true;
            pools.visible = true;
            crew.visible = dressing;
            lampsOn(true);
          }
        }
        if (growthHold > 0) {
          growthHold -= step;
          if (growthHold < 0) growthHold = 0;
        } else if (roadMode === 1 && growth < 1) {
          growth += step / GROW_SECONDS;
          if (growth > 1) growth = 1;
          applyGrowth();
        }
        if (crew.visible) updateCrew(step);
      }
    } else if (state === 'waking') {
      glideElapsed += step * 1000;
      if (glideDone || glideElapsed >= WAKE_MS) revealCards();
    } else if (state === 'offered') {
      if (tData !== null) {
        addDataTime(tData);
        if (dataElapsed >= IGNORE_MS) ignoreNow();
      }
    }
    // The data clock reference stays fresh in every state, so the 24 h ignore is
    // measured from the moment the cards appeared, not from the first frame after.
    if (tData !== null && state !== 'offered') dataLast = tData;

    if (packing) updatePacking(step);

    if (fading) {
      fadeT += step;
      const k = fadeT / FADE_SECONDS;
      if (k >= 1) {
        fading = false;
        matGhost.opacity = 0;
        roadGhost.visible = false;
        matGlow.opacity = 0;
        roadGlow.visible = false;
      } else {
        matGhost.opacity = 1 - k;
        matGlow.opacity = 1 - k;
      }
    }
  }

  // -------------------------------------------------------------- warm-up
  // Every mesh in the group starts hidden and the first sleep is what shows it, so three used
  // to link the dream's programs inside that frame: measured as a 35.7 ms frame on the 4x
  // throttled phone, over the 33 ms ceiling. renderer.compile() only walks visible objects, so
  // warm() shows the meshes that can ever be drawn for the length of one compile and puts the
  // group back exactly as it was. main calls it once, one frame after install, so the cost
  // lands in the loading window instead of in a measured frame. No per frame work, no
  // allocation: the set and its saved flags are built once here.
  const warmSet = [roadProv, roadGhost, roadPerm, roadGlow, masts, pools];
  if (dressing) warmSet.push(cones, barriers, barrierLamps, crew, stalls, awnings);
  const warmSaved = new Uint8Array(warmSet.length);

  function warm(compile) {
    if (typeof compile !== 'function') return false;
    if (state !== 'awake') return false;        // a dream in flight owns these flags
    for (let i = 0; i < warmSet.length; i++) {
      warmSaved[i] = warmSet[i].visible ? 1 : 0;
      warmSet[i].visible = true;
    }
    let ok = true;
    try {
      compile();
    } catch (e) {
      ok = false;
    }
    for (let i = 0; i < warmSet.length; i++) warmSet[i].visible = warmSaved[i] === 1;
    return ok;
  }

  // -------------------------------------------------------------------- boot
  // The pair is chosen once here and re-chosen by setDataModel on a new export.
  pickPair();
  takenList = readTaken();
  if (hasPair && layoutRoad()) {
    applyGrowth();
    layoutWorks();
    layoutMarket();
    buildCardTexts();
    // A road taken in an earlier session is permanent from the first frame. `take()` shows the
    // asphalt, the light strip and the market in the daylight scene at the moment of taking;
    // without this the same road boots hidden and only reappears the next time the owner
    // sleeps, which is not what the button promised. The check is the pair's own sentence,
    // built exactly as `buildCardTexts` builds it, so a stored line from a different pair (or a
    // different export's quarter names) never lights a road it does not belong to.
    const takenA = quarterName(pairA);
    const takenB = quarterName(pairB);
    for (let i = 0; i < pairWords.length; i++) {
      const takenLine = "The word '" + pairWords[i] + "' kept appearing in both " + takenA + ' and ' + takenB + '. Open a note that joins them?';
      if (takenList.indexOf(takenLine) === -1) continue;
      roadPermRibbon.geo.setDrawRange(0, RING);
      roadPerm.visible = true;
      roadProv.visible = false;
      roadGhost.visible = false;
      roadGlow.visible = false;
      matGlow.opacity = 1;
      roadMode = 2;
      permKey = pairKey;
      showMarket(true);
      break;
    }
  } else {
    // No usable pair: the dream can only darken and quiet the district.
    hasPair = false;
    pairA = -1;
    pairB = -1;
    pairKey = '';
    pairWords.length = 0;
    cardTexts.length = 0;
    chantierCentre.set(0, 0, 0);
    chantierSpan = 0;
  }

  if (typeof shell.onWake === 'function') {
    // The shell can nudge the dream awake on its own (data resumed, button pressed).
    try { shell.onWake(function () { wake(); }); } catch (e) { /* optional hook */ }
  }

  return {
    group: group,
    update: update,
    sleep: sleep,
    wake: wake,
    toggle: toggle,
    setDataModel: setDataModel,
    notifyGap: notifyGap,
    warm: warm,
    clearCards: hideCards,
    get state() { return state; },
    // Read-only measurement hooks for the screenshot harness: the middle of the
    // provisional ribbon at road level, and the length of that ribbon. The getter
    // returns the same Vector3 every time, refreshed in place by layoutRoad.
    get chantierCenter() { return chantierCentre; },
    get chantierSpan() { return chantierSpan; }
  };
}
