// Verifies assets/manifest.json against the built pack GLBs.
//   node tools/check-assets.mjs            print PASS/FAIL report, exit 0/1
//   imported by pack-assets.mjs --check    (single source of verification)
// Node built-ins only.
import { readFileSync, readdirSync, statSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseGLB, makeReader, IDENT, primTris, walkNodes, xform, decodePNG, encodePNG } from './gltf-lib.mjs';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const ASSETS = join(ROOT, 'assets');

// Triangle caps per category (main binding gate is the desktop 400k budget
// including the shadow pass, i.e. worst case every triangle drawn twice).
const CAPS = {
  apartment: 2000, shop: 2000, warehouse: 2000, tower: 2000, villa: 1700,
  car: 3500, van: 3500, truck: 3500, bus: 3500,
  'tree-olive': 400, 'tree-cypress': 400, 'tree-palm': 400, 'tree-plane': 400,
  lamp: 250, bench: 250, shelter: 250, signal: 250,
  parasol: 400, prop: 400, person: 250, part: Infinity
};
const TIER_BUDGET = 200000; // desktop budget 400k incl. shadow pass, worst case each tri drawn twice

function dirSize(dir) {
  let total = 0;
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    total += st.isDirectory() ? dirSize(p) : st.size;
  }
  return total;
}

function scanUri(value, where, fails) {
  if (Array.isArray(value)) { value.forEach((v, i) => scanUri(v, where + '[' + i + ']', fails)); return; }
  if (value && typeof value === 'object') {
    for (const [k, v] of Object.entries(value)) {
      if (k === 'uri') fails.push(where + '.' + k);
      scanUri(v, where + '.' + k, fails);
    }
  }
}

// Scene-root stats for one pack GLB: name -> {tris, min, max, materials}
function packNodeStats(json, bin) {
  const reader = makeReader(json, bin);
  const stats = new Map();
  const scene = json.scenes[json.scene ?? 0];
  for (const root of scene.nodes) {
    const node = json.nodes[root];
    if (node.name === undefined) continue;
    let tris = 0;
    const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
    const mats = [];
    walkNodes(json, root, IDENT, (meshIndex, world) => {
      for (const prim of json.meshes[meshIndex].primitives) {
        tris += primTris(json, reader, prim);
        if (prim.material !== undefined) {
          const name = (json.materials[prim.material] || {}).name || ('material' + prim.material);
          if (!mats.includes(name)) mats.push(name);
        }
        const P = reader.accessor(prim.attributes.POSITION);
        // accessor min/max may be absent; fall back to scanning positions
        let pmin, pmax;
        if (json.accessors[prim.attributes.POSITION].min) {
          pmin = json.accessors[prim.attributes.POSITION].min;
          pmax = json.accessors[prim.attributes.POSITION].max;
        } else {
          pmin = [Infinity, Infinity, Infinity]; pmax = [-Infinity, -Infinity, -Infinity];
          for (let i = 0; i < P.length; i += 3) {
            for (let k = 0; k < 3; k++) {
              if (P[i + k] < pmin[k]) pmin[k] = P[i + k];
              if (P[i + k] > pmax[k]) pmax[k] = P[i + k];
            }
          }
        }
        for (let cx = 0; cx < 2; cx++) for (let cy = 0; cy < 2; cy++) for (let cz = 0; cz < 2; cz++) {
          const x = cx ? pmax[0] : pmin[0], y = cy ? pmax[1] : pmin[1], z = cz ? pmax[2] : pmin[2];
          const p = xform(world, x, y, z);
          for (let k = 0; k < 3; k++) {
            if (p[k] < min[k]) min[k] = p[k];
            if (p[k] > max[k]) max[k] = p[k];
          }
        }
      }
    });
    stats.set(node.name, { tris, min, max, materials: mats });
  }
  return stats;
}

const r3 = (v) => Math.round(v * 1000) / 1000;

// ---------------------------------------------------------------------------
// Car LOD gate. Renders each low variant and its full-detail source from the
// built pack GLB with the pack palette texture at 3 yaws, then measures the
// silhouette IoU and the mean absolute RGB difference over pixels lit in both
// renders. Must hold: IoU >= 0.93 and dRGB <= 0.06 per model per yaw. The
// comparison contact sheet is written to screenshots/assets/car-lod-compare.png.
// ---------------------------------------------------------------------------
const LOD_SIZE = 200;
const LOD_YAWS = [0, 90, 45];
const LOD_IOU_MIN = 0.93;
const LOD_DRGB_MAX = 0.06;
const LOD_REQUIRED = ['sedan-low', 'suv-low', 'hatchback-sports-low', 'taxi-low', 'van-low', 'delivery-low', 'truck-low'];
const LOD_SHEET = 'screenshots/assets/car-lod-compare.png';

// 3x5 bitmap font, one 15-character row-major string per glyph.
const FONT_3X5 = {
  '0': '111101101101111', '1': '010110010010111', '2': '111001111100111', '3': '111001111001111',
  '4': '101101111001001', '5': '111100111001111', '6': '111100111101111', '7': '111001010010010',
  '8': '111101111101111', '9': '111101111001111',
  A: '111101111101101', B: '110101110101110', C: '111100100100111', D: '110101101101110',
  E: '111100111100111', F: '111100111100100', G: '111100101101111', H: '101101111101101',
  I: '111010010010111', J: '001001001101111', K: '101101110101101', L: '100100100100111',
  M: '101111111101101', N: '101111111111101', O: '111101101101111', P: '111101111100100',
  Q: '111101101111001', R: '111101111110101', S: '111100111001111', T: '111010010010010',
  U: '101101101101111', V: '101101101101010', W: '101101111111101', X: '101101010101101',
  Y: '101101010010010', Z: '111001010100111',
  '-': '000000111000000', '.': '000000000000010', ':': '000010000010000', '/': '001001010100100',
  '>': '100010001010100', '%': '101001010100101', ',': '000000000010100', ' ': '000000000000000'
};

function drawText(buf, width, height, x, y, text, scale, cr, cg, cb) {
  let cx = x;
  for (let i = 0; i < text.length; i++) {
    const glyph = FONT_3X5[text[i]] || FONT_3X5[' '];
    for (let row = 0; row < 5; row++) {
      for (let col = 0; col < 3; col++) {
        if (glyph[row * 3 + col] !== '1') continue;
        for (let sy = 0; sy < scale; sy++) {
          const py = y + row * scale + sy;
          if (py < 0 || py >= height) continue;
          for (let sx = 0; sx < scale; sx++) {
            const px = cx + col * scale + sx;
            if (px < 0 || px >= width) continue;
            const o = (py * width + px) * 3;
            buf[o] = cr;
            buf[o + 1] = cg;
            buf[o + 2] = cb;
          }
        }
      }
    }
    cx += 4 * scale;
  }
}

function packNodeGeometry(json, bin, name) {
  const scene = json.scenes[json.scene ?? 0];
  const reader = makeReader(json, bin);
  for (const root of scene.nodes) {
    const node = json.nodes[root];
    if (node.name !== name || node.mesh === undefined) continue;
    const prims = json.meshes[node.mesh].primitives;
    if (prims.length !== 1) throw new Error(name + ' has ' + prims.length + ' primitives, expected one');
    const prim = prims[0];
    const P = reader.accessor(prim.attributes.POSITION);
    const N = prim.attributes.NORMAL !== undefined ? reader.accessor(prim.attributes.NORMAL) : null;
    const UV = prim.attributes.TEXCOORD_0 !== undefined ? reader.accessor(prim.attributes.TEXCOORD_0) : null;
    const I = prim.indices !== undefined ? reader.accessor(prim.indices) : null;
    return {
      pos: Float64Array.from(P),
      nor: N ? Float64Array.from(N) : null,
      uv: UV ? Float64Array.from(UV) : null,
      idx: I ? Uint32Array.from(I) : null,
      tris: I ? I.length / 3 : P.length / 9,
      material: prim.material
    };
  }
  return null;
}

function packMaterialPalette(json, bin, materialIndex) {
  const mat = (materialIndex !== undefined && json.materials[materialIndex]) || {};
  const pbr = mat.pbrMetallicRoughness || {};
  if (pbr.baseColorTexture === undefined) return null;
  const img = json.images[json.textures[pbr.baseColorTexture.index].source];
  const bv = json.bufferViews[img.bufferView];
  const off = bv.byteOffset || 0;
  const png = decodePNG(Buffer.from(bin.subarray(off, off + bv.byteLength)));
  return { w: png.w, h: png.h, ch: png.ch, data: png.data, factor: pbr.baseColorFactor || [1, 1, 1, 1] };
}

// orthographic frame shared by the source and the low render at one yaw
function lodFrame(geom, yaw, size) {
  const rad = (yaw * Math.PI) / 180, c = Math.cos(rad), s = Math.sin(rad);
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (let i = 0; i < geom.pos.length; i += 3) {
    const x = geom.pos[i] * c + geom.pos[i + 2] * s;
    const y = geom.pos[i + 1];
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  const span = Math.max(maxX - minX, maxY - minY) || 1;
  return { c, s, scale: (size * 0.88) / span, cx: (minX + maxX) / 2, cy: (minY + maxY) / 2, size };
}

function renderLod(geom, tex, frame) {
  const size = frame.size;
  const c = frame.c, s = frame.s, scale = frame.scale;
  const n = geom.pos.length / 3;
  const sx = new Float32Array(n), sy = new Float32Array(n), sz = new Float32Array(n);
  const rnx = new Float32Array(n), rny = new Float32Array(n), rnz = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const x = geom.pos[i * 3], y = geom.pos[i * 3 + 1], z = geom.pos[i * 3 + 2];
    sx[i] = size * 0.5 + (x * c + z * s - frame.cx) * scale;
    sy[i] = size * 0.5 - (y - frame.cy) * scale;
    sz[i] = -x * s + z * c;
    if (geom.nor) {
      const nx = geom.nor[i * 3], ny = geom.nor[i * 3 + 1], nz = geom.nor[i * 3 + 2];
      rnx[i] = nx * c + nz * s;
      rny[i] = ny;
      rnz[i] = -nx * s + nz * c;
    }
  }
  const mask = new Uint8Array(size * size);
  const rgb = new Float32Array(size * size * 3);
  const zbuf = new Float32Array(size * size).fill(-1e30);
  const LX = -0.45, LY = 0.72, LZ = 0.52;
  const ll = Math.hypot(LX, LY, LZ);
  const lx = LX / ll, ly = LY / ll, lz = LZ / ll;

  for (let t = 0; t < geom.idx.length; t += 3) {
    const ia = geom.idx[t], ib = geom.idx[t + 1], ic = geom.idx[t + 2];
    const ax = sx[ia], ay = sy[ia], az = sz[ia];
    const bx = sx[ib], by = sy[ib], bz = sz[ib];
    const cx = sx[ic], cy = sy[ic], cz = sz[ic];
    const area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    if (!(Math.abs(area) > 1e-9)) continue;
    const inv = 1 / area;
    const minx = Math.max(0, Math.floor(Math.min(ax, bx, cx)));
    const maxx = Math.min(size - 1, Math.ceil(Math.max(ax, bx, cx)));
    const miny = Math.max(0, Math.floor(Math.min(ay, by, cy)));
    const maxy = Math.min(size - 1, Math.ceil(Math.max(ay, by, cy)));
    const ua = geom.uv ? geom.uv[ia * 2] : 0, va = geom.uv ? geom.uv[ia * 2 + 1] : 0;
    const ub = geom.uv ? geom.uv[ib * 2] : 0, vb = geom.uv ? geom.uv[ib * 2 + 1] : 0;
    const uc = geom.uv ? geom.uv[ic * 2] : 0, vc = geom.uv ? geom.uv[ic * 2 + 1] : 0;
    const back = area > 0;
    for (let py = miny; py <= maxy; py++) {
      const pyc = py + 0.5;
      for (let px = minx; px <= maxx; px++) {
        const pxc = px + 0.5;
        const w0 = ((cx - bx) * (pyc - by) - (cy - by) * (pxc - bx)) * inv;
        if (w0 < 0) continue;
        const w1 = ((ax - cx) * (pyc - cy) - (ay - cy) * (pxc - cx)) * inv;
        if (w1 < 0) continue;
        const w2 = 1 - w0 - w1;
        if (w2 < 0) continue;
        const z = w0 * az + w1 * bz + w2 * cz;
        const pi = py * size + px;
        if (z <= zbuf[pi]) continue;
        zbuf[pi] = z;
        mask[pi] = 1;
        let px0 = 1, py0 = 1, pz0 = 0;
        if (geom.nor) {
          px0 = w0 * rnx[ia] + w1 * rnx[ib] + w2 * rnx[ic];
          py0 = w0 * rny[ia] + w1 * rny[ib] + w2 * rny[ic];
          pz0 = w0 * rnz[ia] + w1 * rnz[ib] + w2 * rnz[ic];
          const nl = Math.hypot(px0, py0, pz0) || 1;
          px0 /= nl;
          py0 /= nl;
          pz0 /= nl;
        }
        if (back) {
          px0 = -px0;
          py0 = -py0;
          pz0 = -pz0;
        }
        let ndl = px0 * lx + py0 * ly + pz0 * lz;
        if (ndl < 0) ndl = 0;
        const shade = 0.42 + 0.72 * ndl;
        let tr = 1, tg = 1, tb = 1;
        if (tex) {
          const u = w0 * ua + w1 * ub + w2 * uc;
          const v = w0 * va + w1 * vb + w2 * vc;
          const tx = u * tex.w - 0.5, ty = v * tex.h - 0.5;
          let x0 = Math.floor(tx), y0 = Math.floor(ty);
          const fx = tx - x0, fy = ty - y0;
          x0 = ((x0 % tex.w) + tex.w) % tex.w;
          y0 = ((y0 % tex.h) + tex.h) % tex.h;
          const x1 = (x0 + 1) % tex.w, y1 = (y0 + 1) % tex.h;
          const ch = tex.ch, d = tex.data;
          const o00 = (y0 * tex.w + x0) * ch, o10 = (y0 * tex.w + x1) * ch;
          const o01 = (y1 * tex.w + x0) * ch, o11 = (y1 * tex.w + x1) * ch;
          const w00 = (1 - fx) * (1 - fy), w10 = fx * (1 - fy), w01 = (1 - fx) * fy, w11 = fx * fy;
          tr = (d[o00] * w00 + d[o10] * w10 + d[o01] * w01 + d[o11] * w11) / 255;
          tg = (d[o00 + 1] * w00 + d[o10 + 1] * w10 + d[o01 + 1] * w01 + d[o11 + 1] * w11) / 255;
          tb = (d[o00 + 2] * w00 + d[o10 + 2] * w10 + d[o01 + 2] * w01 + d[o11 + 2] * w11) / 255;
          tr *= tex.factor[0];
          tg *= tex.factor[1];
          tb *= tex.factor[2];
        }
        const o = pi * 3;
        rgb[o] = Math.min(1, tr * shade);
        rgb[o + 1] = Math.min(1, tg * shade);
        rgb[o + 2] = Math.min(1, tb * shade);
      }
    }
  }
  return { mask, rgb, size };
}

function lodCompare(a, b) {
  let inter = 0, union = 0;
  for (let i = 0; i < a.mask.length; i++) {
    const ma = a.mask[i], mb = b.mask[i];
    if (ma || mb) union++;
    if (ma && mb) inter++;
  }
  let sum = 0, n = 0;
  for (let i = 0; i < a.mask.length; i++) {
    if (!(a.mask[i] && b.mask[i])) continue;
    for (let k = 0; k < 3; k++) sum += Math.abs(a.rgb[i * 3 + k] - b.rgb[i * 3 + k]);
    n += 3;
  }
  return { iou: union ? inter / union : 0, drgb: n ? sum / n : 1, pixels: inter };
}

function lodSheet(rows) {
  const cell = LOD_SIZE, gap = 3, labelH = 26, headH = 32, cols = 6;
  const width = cols * cell;
  const height = headH + rows.length * (labelH + cell + gap);
  const buf = Buffer.alloc(width * height * 3);
  for (let i = 0; i < buf.length; i += 3) {
    buf[i] = 16;
    buf[i + 1] = 18;
    buf[i + 2] = 22;
  }
  drawText(buf, width, height, 8, 9, 'CAR LOD COMPARE', 3, 236, 238, 244);
  drawText(buf, width, height, 8 + 17 * 12, 11, 'SOURCE: LEFT 3   LOW: RIGHT 3   YAWS 0/90/45', 2, 168, 176, 190);
  for (let r = 0; r < rows.length; r++) {
    const row = rows[r];
    const y0 = headH + r * (labelH + cell + gap);
    const label = row.name.toUpperCase() + '  TRIS ' + row.srcTris + ' > ' + row.lowTris +
      '  IOU MIN ' + row.iouMin.toFixed(3) + '  DRGB MAX ' + row.drgbMax.toFixed(3);
    drawText(buf, width, height, 8, y0 + 5, label, 3, 226, 230, 238);
    for (let v = 0; v < 2; v++) {
      const views = v === 0 ? row.src : row.low;
      for (let yawIdx = 0; yawIdx < views.length; yawIdx++) {
        const view = views[yawIdx];
        const x0 = (v * 3 + yawIdx) * cell;
        const top = y0 + labelH;
        for (let y = 0; y < cell; y++) {
          for (let x = 0; x < cell; x++) {
            const sIdx = y * cell + x;
            const o = ((top + y) * width + (x0 + x)) * 3;
            if (view.mask[sIdx]) {
              buf[o] = Math.round(view.rgb[sIdx * 3] * 255);
              buf[o + 1] = Math.round(view.rgb[sIdx * 3 + 1] * 255);
              buf[o + 2] = Math.round(view.rgb[sIdx * 3 + 2] * 255);
            } else {
              buf[o] = 30;
              buf[o + 1] = 33;
              buf[o + 2] = 40;
            }
          }
        }
      }
    }
    const sep = 3 * cell;
    for (let y = y0; y < y0 + labelH + cell; y++) {
      const o = (y * width + sep) * 3;
      buf[o] = 70;
      buf[o + 1] = 76;
      buf[o + 2] = 92;
    }
  }
  return { width, height, data: buf };
}

// Verifies the seven low car variants: existence, triangle band, category,
// tier, and the measured render comparison. Writes the contact sheet.
// Returns { ok, lines } and never throws.
export function verifyCarLod(manifest) {
  const lines = [];
  let ok = true;
  const pass = (m) => lines.push('PASS ' + m);
  const fail = (m) => {
    lines.push('FAIL ' + m);
    ok = false;
  };
  const models = (manifest && manifest.models) || {};
  const packs = new Map(((manifest && manifest.packs) || []).map((p) => [p.id, p]));
  const packCache = new Map();

  function loadPack(pack) {
    let entry = packCache.get(pack.id);
    if (entry) return entry;
    const glbPath = join(ROOT, pack.file);
    const parsed = parseGLB(readFileSync(glbPath));
    entry = { json: parsed.json, bin: parsed.bin, palette: null };
    packCache.set(pack.id, entry);
    return entry;
  }

  const rows = [];
  for (const name of LOD_REQUIRED) {
    const low = models[name];
    const baseName = name.slice(0, -4);
    const src = models[baseName];
    if (!low) {
      fail(name + ': missing from the manifest');
      continue;
    }
    if (!src) {
      fail(name + ': no full-detail ' + baseName + ' entry to compare against');
      continue;
    }
    if (low.tier !== 'street') fail(name + ': tier must be street, got ' + String(low.tier));
    if (low.category !== src.category) fail(name + ': category ' + String(low.category) + ' should match its source (' + src.category + ')');
    if (!(low.tris >= 300 && low.tris <= 500)) fail(name + ': ' + low.tris + ' tris outside the 300..500 band');
    const pack = packs.get(low.pack);
    if (!pack) {
      fail(name + ': unknown pack ' + String(low.pack));
      continue;
    }
    let entry;
    try {
      entry = loadPack(pack);
    } catch (e) {
      fail(name + ': pack ' + pack.file + ' unreadable (' + e.message + ')');
      continue;
    }
    const lowGeom = packNodeGeometry(entry.json, entry.bin, name);
    const srcGeom = packNodeGeometry(entry.json, entry.bin, baseName);
    if (!lowGeom || !srcGeom) {
      fail(name + ': geometry node missing from ' + pack.file);
      continue;
    }
    if (!entry.palette) entry.palette = packMaterialPalette(entry.json, entry.bin, srcGeom.material);
    const tex = entry.palette;
    const row = { name, srcTris: srcGeom.tris, lowTris: lowGeom.tris, src: [], low: [], ious: [], drgbs: [], iouMin: 1, drgbMax: 0 };
    for (const yaw of LOD_YAWS) {
      const frame = lodFrame(srcGeom, yaw, LOD_SIZE);
      const a = renderLod(srcGeom, tex, frame);
      const b = renderLod(lowGeom, tex, frame);
      const cmp = lodCompare(a, b);
      if (cmp.pixels < 500) fail(name + ' yaw ' + yaw + ': only ' + cmp.pixels + ' px lit in both renders');
      row.src.push(a);
      row.low.push(b);
      row.ious.push(cmp.iou);
      row.drgbs.push(cmp.drgb);
      if (cmp.iou < row.iouMin) row.iouMin = cmp.iou;
      if (cmp.drgb > row.drgbMax) row.drgbMax = cmp.drgb;
    }
    rows.push(row);
    const detail = LOD_YAWS.map((yaw, i) => 'yaw' + yaw + ' IoU ' + row.ious[i].toFixed(3) + ' dRGB ' + row.drgbs[i].toFixed(3)).join(', ');
    if (row.iouMin >= LOD_IOU_MIN && row.drgbMax <= LOD_DRGB_MAX) {
      pass(name + ': ' + row.lowTris + ' tris, ' + detail);
    } else {
      fail(name + ': ' + row.lowTris + ' tris below the LOD bar (' + detail + '; need IoU >= ' + LOD_IOU_MIN + ' and dRGB <= ' + LOD_DRGB_MAX + ')');
    }
  }

  if (rows.length === LOD_REQUIRED.length) {
    const sheet = lodSheet(rows);
    try {
      const file = join(ROOT, LOD_SHEET);
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, encodePNG(sheet.width, sheet.height, 3, sheet.data));
      pass('contact sheet ' + LOD_SHEET + ' written (' + sheet.width + ' x ' + sheet.height + ', 7 rows)');
    } catch (e) {
      fail('contact sheet write failed: ' + e.message);
    }
  }
  return { ok, lines };
}

export function verify() {
  const lines = [];
  let ok = true;
  const pass = (m) => lines.push('PASS ' + m);
  const fail = (m) => { lines.push('FAIL ' + m); ok = false; };

  // manifest
  let manifest = null;
  try {
    manifest = JSON.parse(readFileSync(join(ASSETS, 'manifest.json'), 'utf8'));
  } catch (e) {
    fail('assets/manifest.json unreadable: ' + e.message);
    lines.forEach(l => console.log(l));
    return false;
  }
  if (manifest.version !== 1) fail('manifest version must be 1, got ' + manifest.version);
  if (!Array.isArray(manifest.packs) || !manifest.packs.length) fail('manifest has no packs');
  const models = manifest.models || {};
  const names = Object.keys(models);

  // assets folder size
  try {
    const bytes = dirSize(ASSETS);
    pass('assets total ' + (bytes / 1048576).toFixed(2) + ' MB < 60 MB');
    if (bytes >= 60 * 1048576) fail('assets folder >= 60 MB');
  } catch (e) {
    fail('assets folder unreadable: ' + e.message);
  }

  // packs: file parses, licence mentions CC0, no external uri references
  const packStats = new Map();
  let packPathsOk = manifest.packs.length > 0;
  for (const pack of manifest.packs) {
    if (typeof pack.file !== 'string' || pack.file.indexOf('assets/') !== 0) {
      fail('pack ' + pack.id + ' file must be the page-relative assets/<file> path, got ' + String(pack.file));
      packPathsOk = false;
      continue;
    }
    const glbPath = join(ROOT, pack.file);
    if (!existsSync(glbPath)) {
      fail('pack ' + pack.id + ' file does not resolve from the project root: ' + pack.file);
      packPathsOk = false;
      continue;
    }
    let json, bin;
    try { ({ json, bin } = parseGLB(readFileSync(glbPath))); } catch (e) { fail(pack.file + ' does not parse: ' + e.message); continue; }
    const uris = [];
    scanUri(json, pack.file, uris);
    if (uris.length) fail(pack.file + ' contains external uri references: ' + uris.join(', '));
    else pass(pack.file + ' fully embedded (no uri keys)');
    const licPath = join(ASSETS, 'LICENSES', pack.id + '.txt');
    if (!existsSync(licPath)) fail('missing licence file LICENSES/' + pack.id + '.txt');
    else {
      const lic = readFileSync(licPath, 'utf8');
      if (/CC0/i.test(lic)) pass('licence LICENSES/' + pack.id + '.txt contains CC0');
      else fail('licence LICENSES/' + pack.id + '.txt does not mention CC0');
    }
    try {
      packStats.set(pack.id, packNodeStats(json, bin));
    } catch (e) {
      fail(pack.file + ' node walk failed: ' + e.message);
    }
  }
  if (packPathsOk) pass('pack files are page-relative under assets/ and resolve from the project root (' + manifest.packs.length + ' packs)');

  // models
  const tierSums = {};
  const missingNode = [];
  const tableRows = [];
  for (const name of names) {
    const m = models[name];
    if (!m.category) { fail('model ' + name + ' has no category'); continue; }
    if (!m.tier) fail('model ' + name + ' has no tier');
    const pack = manifest.packs.find(p => p.id === m.pack);
    if (!pack) { fail('model ' + name + ' references unknown pack ' + m.pack); continue; }
    const stats = packStats.get(m.pack);
    if (!stats) continue; // pack already failed
    const st = stats.get(name);
    if (!st) { fail('model ' + name + ' has no top-level mesh node in ' + pack.file); missingNode.push(name); continue; }
    if (!st.materials.length) fail('model ' + name + ' node has no mesh materials');
    if (st.tris !== m.tris) fail('model ' + name + ' tris mismatch: manifest ' + m.tris + ', GLB ' + st.tris);
    const cap = CAPS[m.category];
    if (cap === undefined) fail('model ' + name + ' category ' + m.category + ' has no cap entry');
    else if (st.tris > cap) fail('model ' + name + ' tris ' + st.tris + ' exceeds ' + m.category + ' cap ' + cap);
    // bounds
    const b = m.bounds || {};
    const w = (b.max?.[0] ?? 0) - (b.min?.[0] ?? 0), d = (b.max?.[2] ?? 0) - (b.min?.[2] ?? 0), h = b.max?.[1] ?? 0;
    const problems = [];
    const near = (a, t) => Math.abs(a - t) <= 0.011;
    for (let k = 0; k < 3; k++) {
      if (!near(st.min[k], b.min?.[k] ?? NaN) || !near(st.max[k], b.max?.[k] ?? NaN)) {
        problems.push('bounds axis ' + k + ': manifest [' + (b.min?.[k]) + ',' + (b.max?.[k]) + '] vs GLB [' + r3(st.min[k]) + ',' + r3(st.max[k]) + ']');
      }
    }
    const centreX = (st.min[0] + st.max[0]) / 2, centreZ = (st.min[2] + st.max[2]) / 2;
    if (Math.abs(st.min[1]) > 0.011) problems.push('min y is ' + r3(st.min[1]) + ', expected 0');
    if (Math.abs(centreX) > 0.011 || Math.abs(centreZ) > 0.011) problems.push('x/z centre off: (' + r3(centreX) + ',' + r3(centreZ) + ')');
    if (problems.length) fail('model ' + name + ': ' + problems.join('; '));
    tierSums[m.tier] = (tierSums[m.tier] || 0) + st.tris;
    tableRows.push({ name, pack: m.pack, category: m.category, tier: m.tier, tris: st.tris, w, d, h, materials: st.materials.join('+') });
  }

  // reverse check: every mesh node in every pack must be manifest-listed
  for (const pack of manifest.packs) {
    const stats = packStats.get(pack.id);
    if (!stats) continue;
    for (const [nodeName] of stats) {
      if (!models[nodeName]) fail(pack.file + ' has unmanifested node ' + nodeName);
    }
  }

  // tier budget
  const totalTris = Object.values(tierSums).reduce((a, v) => a + v, 0);
  for (const [tier, n] of Object.entries(tierSums)) pass('tier ' + tier + ': ' + n + ' tris');
  if (totalTris <= TIER_BUDGET) pass('tri budget: ' + totalTris + ' total <= ' + TIER_BUDGET + ' (desktop budget 400k incl. shadow pass, worst case each tri drawn twice)');
  else fail('tri budget exceeded: ' + totalTris + ' > ' + TIER_BUDGET);

  // sedan footprint soft check: about 1.9 wide x 4.1 long facing +Z. The kit
  // sedan is scaled uniformly by 1.43 (geometric mean of the width and length
  // targets) so the wheels stay round; that lands at 2.14 x 3.65 m, inside the
  // +/-20% soft window.
  const sedan = models.sedan;
  if (sedan && sedan.footprint) {
    const wOk = Math.abs(sedan.footprint.w - 1.9) / 1.9 <= 0.20;
    const dOk = Math.abs(sedan.footprint.d - 4.1) / 4.1 <= 0.20;
    if (wOk && dOk) pass('sedan footprint ' + sedan.footprint.w + ' x ' + sedan.footprint.d + ' m within +/-20% of 1.9 x 4.1 (soft check; uniform 1.43 scale trade-off)');
    else fail('sedan footprint ' + sedan.footprint.w + ' x ' + sedan.footprint.d + ' m outside +/-20% of 1.9 x 4.1');
  } else {
    fail('manifest has no sedan entry with footprint for the soft check');
  }

  // car LOD gate: measured silhouette and colour comparison plus contact sheet
  const lod = verifyCarLod(manifest);
  for (const l of lod.lines) lines.push(l);
  if (!lod.ok) ok = false;

  // table
  const tableLines = [];
  if (tableRows.length) {
    const pad = (s, n) => String(s).padEnd(n);
    const wName = Math.max(5, ...tableRows.map(r => r.name.length));
    const wPack = Math.max(4, ...tableRows.map(r => r.pack.length));
    const wCat = Math.max(8, ...tableRows.map(r => r.category.length));
    const wTier = Math.max(4, ...tableRows.map(r => r.tier.length));
    tableLines.push(pad('model', wName) + '  ' + pad('pack', wPack) + '  ' + pad('category', wCat) + '  ' + pad('tier', wTier) + '  tris  footprint w x d x h (m)  materials');
    for (const r of tableRows) {
      tableLines.push(pad(r.name, wName) + '  ' + pad(r.pack, wPack) + '  ' + pad(r.category, wCat) + '  ' + pad(r.tier, wTier) + '  ' + String(r.tris).padEnd(4) + '  ' +
        r3(r.w) + ' x ' + r3(r.d) + ' x ' + r3(r.h) + '  ' + r.materials);
    }
  }

  for (const l of lines) console.log(l);
  if (tableLines.length) {
    console.log('');
    for (const l of tableLines) console.log(l);
  }
  console.log(ok ? 'CHECK RESULT: PASS (' + names.length + ' models)' : 'CHECK RESULT: FAIL');
  return ok;
}

const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  const ok = verify();
  process.exitCode = ok ? 0 : 1;
}
