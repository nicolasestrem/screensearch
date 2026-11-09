// Cycle 28 repair probe: offline spur geometry. Scratch tool, not part of the deliverable.
//
// Reproduces the source's own sampling (samplePath, 2 m) for the five spur curves, the four
// street-camera poses and the expressway, so a frontage bin that the live probe calls empty can
// be named by its world position and by the mask that could be blocking it, without a browser.
import * as THREE from './three160.js';

const LIFT_ROAD = 0.14;
const lifted = (x, z, y) => new THREE.Vector3(x, y === undefined ? LIFT_ROAD : y, z);

const SPUR_PTS = [
  [[-72, 94], [-100, 64], [-130, 36], [-162, 24], [-200, 18], [-246, 12]],
  [[96, 10], [112, 6], [130, -2], [158, -14], [204, -30], [248, -46], [292, -62], [330, -76]],
  [[-40, -76], [-54, -94], [-62, -110], [-66, -120]],
  [[-96, 4], [-130, -8], [-170, -22], [-210, -34], [-250, -46], [-292, -58], [-338, -70]],
  [[-66, -62], [-104, -70], [-148, -82], [-196, -98], [-248, -118], [-300, -140]]
];
const EX_PTS = [[-168, 60], [-140, 58], [-126, 72], [-112, 86], [-98, 100], [-84, 114], [-72, 128]];
const TRACK_PTS = [
  [[-46, 104], [-43, 122], [-38, 138], [-33, 156], [-29, 170], [-24, 183]],
  [[26, 104], [20, 128], [14, 140], [4, 152], [-2, 168], [-6, 192]],
  [[154, 43], [166, 62], [176, 84], [184, 108], [188, 128]],
  [[-180, 50], [-188, 68], [-196, 88], [-201, 108]],
  [[-66, -120], [-70, -138], [-76, -156], [-84, -176], [-94, -204], [-104, -232]]
];

function curveOf(pts) {
  return new THREE.CatmullRomCurve3(pts.map((p) => lifted(p[0], p[1])), false, 'centripetal');
}

function samplePath(curve, step) {
  const len = curve.getLength();
  const n = Math.max(8, Math.ceil(len / step));
  const xs = [], zs = [], nx = [], nz = [];
  const a = new THREE.Vector3();
  const b = new THREE.Vector3();
  for (let i = 0; i <= n; i += 1) {
    const u = i / n;
    curve.getPointAt(u, a);
    curve.getTangentAt(u, b);
    let tx = b.x, tz = b.z;
    const l = Math.hypot(tx, tz) || 1;
    tx /= l; tz /= l;
    xs.push(a.x); zs.push(a.z); nx.push(-tz); nz.push(tx);
  }
  return { n, len, step: len / n, xs, zs, nx, nz };
}

// Camera rig, from src/camera.mjs: fixed 45 deg fov, 40 deg pitch, target (0,12,4), radius 114,
// street zoom 0.38, pad 1.05.
const FOV = 45 * Math.PI / 180;
const PITCH = 40 * Math.PI / 180;
const PAD = 1.05;
const TARGET = new THREE.Vector3(0, 12, 4);
const RADIUS = 114;
const ZOOM = 0.38;

function rigPose(yaw, aspect) {
  const halfV = FOV * 0.5;
  const halfH = Math.atan(Math.tan(halfV) * aspect);
  const half = Math.min(halfV, halfH);
  const dist = (RADIUS * PAD) / Math.max(0.02, Math.sin(half));
  const d = ZOOM * dist;
  const pos = new THREE.Vector3(
    TARGET.x + Math.sin(yaw) * d * Math.cos(PITCH),
    TARGET.y + d * Math.sin(PITCH),
    TARGET.z + Math.cos(yaw) * d * Math.cos(PITCH)
  );
  const cam = new THREE.PerspectiveCamera(45, aspect, 1, 4000);
  cam.position.copy(pos);
  cam.lookAt(TARGET);
  cam.updateMatrixWorld(true);
  return cam;
}

const ASPECT = 1600 / 900;
const VIEWS = [
  { label: 'yaw0 (south, +z)', yaw: 0 },
  { label: 'yaw90 (east, +x)', yaw: Math.PI / 2 },
  { label: 'yaw180 (north, -z)', yaw: Math.PI },
  { label: 'yaw270 (west, -x)', yaw: 3 * Math.PI / 2 }
].map((v) => ({ ...v, cam: rigPose(v.yaw, ASPECT) }));

function project(cam, x, y, z) {
  const v = new THREE.Vector3(x, y, z).project(cam);
  return { x: v.x, y: v.y, z: v.z, inside: Math.abs(v.x) < 1 && Math.abs(v.y) < 1 && v.z < 1 };
}

const exPath = samplePath(curveOf(EX_PTS), 2.0);
const trackPaths = TRACK_PTS.map((p) => samplePath(curveOf(p), 2.0));
function distTo(table, x, z) {
  let best = Infinity;
  for (let i = 0; i <= table.n; i += 1) {
    const d = Math.hypot(x - table.xs[i], z - table.zs[i]);
    if (d < best) best = d;
  }
  return best;
}

const spurs = SPUR_PTS.map((p) => samplePath(curveOf(p), 2.0));

for (const si of [0, 1]) {
  const path = spurs[si];
  const bins = Math.floor(path.len / 10) + 1;
  console.log(`\nspur ${si}  len ${path.len.toFixed(1)} m  n ${path.n}  bins ${bins}`);
  for (let bin = 0; bin < bins; bin += 1) {
    const im = Math.max(0, Math.min(path.n - 1, Math.round(((bin + 0.5) * 10) / path.step)));
    const x = path.xs[im], z = path.zs[im];
    const vis = VIEWS.map((v) => {
      const p = project(v.cam, x, 0, z);
      return `${p.inside ? '*' : '.'}${p.x.toFixed(2)},${p.y.toFixed(2)}`;
    });
    const front = { l: [x + path.nx[im] * 12.2, z + path.nz[im] * 12.2], r: [x - path.nx[im] * 12.2, z - path.nz[im] * 12.2] };
    const exd = distTo(exPath, x, z);
    const tr = trackPaths.map((t, i) => `${i}:${distTo(t, x, z).toFixed(0)}`).join(' ');
    console.log(
      `  bin ${String(bin).padStart(2)} arc ${String(Math.round((bin + 0.5) * 10)).padStart(3)}  `
      + `at (${x.toFixed(1)}, ${z.toFixed(1)})  frontL ${front.l.map((v) => v.toFixed(0)).join(',')}  frontR ${front.r.map((v) => v.toFixed(0)).join(',')}\n`
      + `        view ${vis.join('  ')}   ex ${exd.toFixed(1)}   tracks ${tr}`
    );
  }
}

// ---------------------------------------------------------------- parcel body vs expressway
// The southWest crossing claims to hold 32 m off the deck's centreline. A parcel on the deck-side
// kerb starts SPUR_FRONT (12.2 m) from the centreline and its body grows 9.4 m further, so its
// back edge sits 21.6 m out; the expressway's own blocked corridor is EX_HALF + 1.8 = 10.4 m.
// 32 - 21.6 = 10.4 is exactly the corridor width, i.e. zero margin. This block measures the real
// clearance bin by bin, for both kerbs, so a bin with no parcel can be blamed on geometry.
const EX_HALF = 8.6;
const EX_CORR = EX_HALF + 1.8;
const SPUR_FRONT = 7.5 + 1.4 + 0.4;   // SPUR_EDGE + 0.4 (SPUR_HALF 7.5 + SPUR_PAVE 1.4)
function rectToPathDist(table, cx, cz, ux, uz, hu, hv) {
  let best = Infinity;
  for (let i = 0; i <= table.n; i += 1) {
    const dx = table.xs[i] - cx, dz = table.zs[i] - cz;
    const lu = Math.abs(dx * ux + dz * uz) - hu;
    const lv = Math.abs(dx * uz - dz * ux) - hv;
    const ox = Math.max(lu, 0), oz = Math.max(lv, 0);
    const d = Math.hypot(ox, oz);
    if (d < best) best = d;
  }
  return best;
}
for (const si of [0]) {
  const path = spurs[si];
  console.log(`\nspur ${si} kerb-body clearance to the expressway corridor (${EX_CORR.toFixed(1)} m needs > 0)`);
  for (let bin = 0; bin < Math.floor(path.len / 10) + 1; bin += 1) {
    const im = Math.max(0, Math.min(path.n - 1, Math.round(((bin + 0.5) * 10) / path.step)));
    const x = path.xs[im], z = path.zs[im];
    const out = [];
    for (const side of [1, -1]) {
      const nx = path.nx[im] * side, nz = path.nz[im] * side;
      const fx = x + nx * SPUR_FRONT, fz = z + nz * SPUR_FRONT;
      const deep = 9.4, wide = 15;
      const cx = fx - nx * deep * 0.5, cz = fz - nz * deep * 0.5;
      const ux = nz, uz = -nx;
      const d = rectToPathDist(exPath, cx, cz, ux, uz, wide * 0.5, deep * 0.5);
      out.push(`side${side > 0 ? '+' : '-'} body(${cx.toFixed(0)},${cz.toFixed(0)}) exClear ${(d - EX_CORR).toFixed(1)}`);
    }
    console.log(`  bin ${String(bin).padStart(2)} arc ${String(Math.round((bin + 0.5) * 10)).padStart(3)}  ${out.join('   ')}`);
  }
}

// ---------------------------------------------------------------- what each corner crop covers
// Intersect the four crop corners' rays with the ground plane, for each of the four street
// rotations, so "the bottom-right crop" is a rectangle of the city and not a memory of a picture.
function groundAt(cam, ndcX, ndcY, y) {
  const dir = new THREE.Vector3(ndcX, ndcY, 0.5).unproject(cam).sub(cam.position).normalize();
  if (Math.abs(dir.y) < 1e-6) return null;
  const t = (y - cam.position.y) / dir.y;
  if (t <= 0) return null;
  return { x: cam.position.x + dir.x * t, z: cam.position.z + dir.z * t };
}
console.log('\ncorner crop footprints on the ground (y = 0 and y = 24 m roofs)');
for (let v = 0; v < VIEWS.length; v += 1) {
  const cam = VIEWS[v].cam;
  const rows = [];
  for (const [name, ny] of [['BL', -2 / 3], ['BR', -2 / 3]]) {
    const nx = name === 'BL' ? -2 / 3 : 2 / 3;
    const g0 = groundAt(cam, nx - 1 / 3, ny + 1 / 3, 0);
    const g1 = groundAt(cam, nx + 1 / 3, ny - 1 / 3, 0);
    const r0 = groundAt(cam, nx - 1 / 3, ny + 1 / 3, 24);
    const r1 = groundAt(cam, nx + 1 / 3, ny - 1 / 3, 24);
    const f = (p) => (p ? `(${p.x.toFixed(0)},${p.z.toFixed(0)})` : 'sky');
    rows.push(`${name} ground ${f(g0)}..${f(g1)}  roof24 ${f(r0)}..${f(r1)}`);
  }
  console.log(`  ${VIEWS[v].label}  eye (${cam.position.x.toFixed(0)}, ${cam.position.z.toFixed(0)})\n      ${rows.join('\n      ')}`);
}
