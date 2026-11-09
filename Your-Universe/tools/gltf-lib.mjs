// Minimal glTF/GLB and PNG reader/writer plus the geometry decimator shared by
// pack-assets.mjs and check-assets.mjs.
// Node built-ins only.
import { inflateSync, deflateSync } from 'node:zlib';

export function parseGLB(buf) {
  if (buf.length < 20 || buf.readUInt32LE(0) !== 0x46546c67) throw new Error('not a GLB (bad magic)');
  const version = buf.readUInt32LE(4);
  if (version !== 2) throw new Error('unsupported GLB version ' + version);
  const jsonLen = buf.readUInt32LE(12);
  const jsonType = buf.readUInt32LE(16);
  if (jsonType !== 0x4e4f534a) throw new Error('first GLB chunk is not JSON');
  const json = JSON.parse(buf.subarray(20, 20 + jsonLen).toString('utf8'));
  let bin = null;
  const after = 20 + jsonLen;
  if (after + 8 <= buf.length) {
    const binLen = buf.readUInt32LE(after);
    const binType = buf.readUInt32LE(after + 4);
    if (binType === 0x004e4942) bin = buf.subarray(after + 8, after + 8 + binLen);
  }
  return { json, bin };
}

// 8-bit non-interlaced PNG: grey, grey+alpha, RGB, RGBA and indexed (PLTE/tRNS).
export function decodePNG(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error('not png');
  let off = 8, w = 0, h = 0, depth = 0, colorType = 0, interlace = 0;
  const idat = [], plte = [];
  while (off + 8 <= buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString('ascii', off + 4, off + 8);
    const data = buf.subarray(off + 8, off + 8 + len);
    if (type === 'IHDR') { w = data.readUInt32BE(0); h = data.readUInt32BE(4); depth = data[8]; colorType = data[9]; interlace = data[12]; }
    else if (type === 'IDAT') idat.push(data);
    else if (type === 'PLTE') for (let i = 0; i + 2 < len; i += 3) plte.push([data[i], data[i + 1], data[i + 2]]);
    off += 12 + len;
  }
  if (depth !== 8 || interlace !== 0) throw new Error('unsupported png depth/interlace ' + depth + '/' + interlace);
  const bpp = colorType === 3 || colorType === 0 ? 1 : colorType === 4 ? 2 : colorType === 2 ? 3 : colorType === 6 ? 4 : 0;
  if (!bpp) throw new Error('unsupported png color type ' + colorType);
  const raw = inflateSync(Buffer.concat(idat));
  const stride = w * bpp;
  const out = Buffer.alloc(w * h * bpp);
  let p = 0;
  for (let y = 0; y < h; y++) {
    const f = raw[p++];
    const line = raw.subarray(p, p + stride); p += stride;
    const prev = y > 0 ? out.subarray((y - 1) * stride, y * stride) : null;
    const cur = out.subarray(y * stride, (y + 1) * stride);
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? cur[x - bpp] : 0;
      const b = prev ? prev[x] : 0;
      const c = x >= bpp && prev ? prev[x - bpp] : 0;
      let v = line[x];
      if (f === 1) v += a;
      else if (f === 2) v += b;
      else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) {
        const pp = a + b - c, pa = Math.abs(pp - a), pb = Math.abs(pp - b), pc = Math.abs(pp - c);
        v += (pa <= pb && pa <= pc) ? a : (pb <= pc) ? b : c;
      }
      cur[x] = v & 0xff;
    }
  }
  if (colorType === 3) {
    const rgb = Buffer.alloc(w * h * 3);
    for (let i = 0; i < w * h; i++) {
      const e = plte[out[i]] || [0, 0, 0];
      rgb[i * 3] = e[0]; rgb[i * 3 + 1] = e[1]; rgb[i * 3 + 2] = e[2];
    }
    return { w, h, ch: 3, data: rgb };
  }
  const ch = bpp;
  if (colorType === 0 || colorType === 4) {
    const rgb = Buffer.alloc(w * h * (colorType === 0 ? 1 : 2));
    for (let i = 0; i < w * h; i++) {
      rgb[i * ch] = out[i * bpp];
      if (ch === 2) rgb[i * ch + 1] = out[i * bpp + 1];
    }
    return { w, h, ch, data: rgb };
  }
  return { w, h, ch, data: out };
}

const COMPONENT = { 5121: Uint8Array, 5123: Uint16Array, 5125: Uint32Array, 5126: Float32Array };
const NUMCOMP = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };
const COMPSIZE = { 5121: 1, 5123: 2, 5125: 4, 5126: 4 };

export function makeReader(json, bin) {
  function accessor(i) {
    const a = json.accessors[i];
    const bv = json.bufferViews[a.bufferView];
    const base = (bv.byteOffset || 0) + bin.byteOffset + (a.byteOffset || 0);
    const nc = NUMCOMP[a.type];
    if (!bv.byteStride) {
      return new COMPONENT[a.componentType](bin.buffer, base, a.count * nc);
    }
    // interleaved: copy into a contiguous typed array
    const size = COMPSIZE[a.componentType];
    const arr = new COMPONENT[a.componentType](a.count * nc);
    const dv = new DataView(bin.buffer, base, (a.count - 1) * bv.byteStride + nc * size);
    const read = a.componentType === 5126 ? (o) => dv.getFloat32(o, true)
      : a.componentType === 5123 ? (o) => dv.getUint16(o, true)
      : a.componentType === 5125 ? (o) => dv.getUint32(o, true)
      : (o) => dv.getUint8(o);
    for (let e = 0; e < a.count; e++) {
      const o = e * bv.byteStride;
      for (let c = 0; c < nc; c++) arr[e * nc + c] = read(o + c * size);
    }
    return arr;
  }
  return { accessor };
}

// Column-major 4x4 matrices, glTF convention.
export const IDENT = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
export function mul(a, b) {
  const o = new Array(16);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) {
    o[c * 4 + r] = a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
  }
  return o;
}
export function nodeMatrix(n) {
  let m = IDENT;
  if (n.matrix) { m = n.matrix.slice(); return m; }
  const t = n.translation || [0, 0, 0];
  const q = n.rotation || [0, 0, 0, 1];
  const s = n.scale || [1, 1, 1];
  const [x, y, z, w] = q;
  const r = [
    1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w), 0,
    2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w), 0,
    2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y), 0,
    0, 0, 0, 1
  ];
  m = [
    r[0] * s[0], r[1] * s[0], r[2] * s[0], 0,
    r[4] * s[1], r[5] * s[1], r[6] * s[1], 0,
    r[8] * s[2], r[9] * s[2], r[10] * s[2], 0,
    t[0], t[1], t[2], 1
  ];
  return m;
}
export function xform(m, x, y, z) {
  return [
    m[0] * x + m[4] * y + m[8] * z + m[12],
    m[1] * x + m[5] * y + m[9] * z + m[13],
    m[2] * x + m[6] * y + m[10] * z + m[14]
  ];
}
export function rotateY(deg) {
  const r = (deg * Math.PI) / 180, c = Math.cos(r), s = Math.sin(r);
  return [c, 0, -s, 0, 0, 1, 0, 0, s, 0, c, 0, 0, 0, 0, 1];
}

// Depth-first walk of a node graph, calling cb(meshIndex, worldMatrix) per mesh node.
export function walkNodes(json, rootIndex, world, cb) {
  const n = json.nodes[rootIndex];
  const m = n.matrix ? mul(world, n.matrix) : mul(world, nodeMatrix(n));
  if (n.mesh !== undefined) cb(n.mesh, m, n);
  for (const c of n.children || []) walkNodes(json, c, m, cb);
}

// All (mesh, world matrix) pairs of a scene root list, in order.
export function collectPrims(json, reader, roots) {
  const out = [];
  for (const r of roots) walkNodes(json, r, IDENT, (meshIndex, m) => {
    for (let pi = 0; pi < json.meshes[meshIndex].primitives.length; pi++) out.push({ meshIndex, primIndex: pi, m });
  });
  return out;
}

export function primTris(json, reader, prim) {
  if (prim.indices !== undefined) return reader.accessor(prim.indices).length / 3;
  return reader.accessor(prim.attributes.POSITION).length / 3;
}

// ---------------------------------------------------------------------------
// 8-bit PNG writer (colour type 2 for ch 3, 6 for ch 4), filter 0, zlib IDAT.
// ---------------------------------------------------------------------------
const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function pngChunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, 'ascii');
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}

export function encodePNG(w, h, ch, data) {
  if (ch !== 3 && ch !== 4) throw new Error('encodePNG supports 3 or 4 channels, got ' + ch);
  const stride = w * ch;
  const raw = Buffer.alloc((stride + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (stride + 1)] = 0;
    for (let x = 0; x < stride; x++) raw[y * (stride + 1) + 1 + x] = data[y * stride + x] & 0xff;
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = ch === 3 ? 2 : 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', deflateSync(raw, { level: 9 })),
    pngChunk('IEND', Buffer.alloc(0))
  ]);
}

// ---------------------------------------------------------------------------
// Quadric-error edge-collapse decimation.
//
// Input is one indexed mesh whose vertices may be split along UV and normal
// seams (that is what the packed source meshes look like: a closed wheel with
// 168 welded positions carries 528 vertices). Coincident positions are welded
// into shared topology first, then edges are collapsed cheapest-first until the
// triangle target is reached. UVs and normals stay on their original triangle
// corners, so every surviving triangle keeps exactly the source texture mapping
// and shading; only vertex positions move. Boundary loops and crease lines
// (dihedral above featureAngle) receive extra plane quadrics, which is what
// holds the wheel arches and the greenhouse outline together at low triangle
// counts. Everything is deterministic: no randomness, no dependency.
//
// geom = { pos, nor, uv, idx }  (flat arrays, idx in vertices)
// returns the same shape plus { tris, verts, collapses }.
// ---------------------------------------------------------------------------
export function decimateGeometry(geom, targetTris, opts = {}) {
  const P = geom.pos, N = geom.nor, UV = geom.uv, IDX = geom.idx;
  const nvIn = P.length / 3;
  const triIn = IDX.length / 3;
  const target = Math.max(4, Math.floor(targetTris));
  const hasNor = !!(N && N.length === nvIn * 3);
  const hasUv = !!(UV && UV.length === nvIn * 2);
  if (triIn <= target) {
    return { pos: P, nor: N || [], uv: UV || [], idx: IDX, tris: triIn, verts: nvIn, collapses: 0 };
  }

  const featureCos = opts.featureCos ?? Math.cos(((opts.featureAngle ?? 50) * Math.PI) / 180);
  const boundaryWeight = opts.boundaryWeight ?? 90;
  const featureWeight = opts.featureWeight ?? 10;
  const seamWeight = opts.seamWeight ?? 40;
  const paintWeight = opts.paintWeight ?? 0;
  const flipCos = opts.flipCos ?? 0.15;

  // --- weld coincident positions -------------------------------------------
  // Vertices are merged on a 1e-4 m grid and then take that grid position, so
  // the collapse order does not depend on the last bits of the caller's floats.
  const vpList = [];
  const weld = new Int32Array(nvIn);
  const weldKey = new Map();
  for (let i = 0; i < nvIn; i++) {
    const ix = Math.round(P[i * 3] * 1e4);
    const iy = Math.round(P[i * 3 + 1] * 1e4);
    const iz = Math.round(P[i * 3 + 2] * 1e4);
    const k = ix + ',' + iy + ',' + iz;
    let w = weldKey.get(k);
    if (w === undefined) {
      w = vpList.length / 3;
      weldKey.set(k, w);
      vpList.push(ix / 1e4, iy / 1e4, iz / 1e4);
    }
    weld[i] = w;
  }
  const nv = vpList.length;
  const vp = Float64Array.from(vpList);

  // --- triangles with their original corner attributes ---------------------
  const triList = [];
  const norList = [];
  const uvList = [];
  const faceSrc = [];
  for (let t = 0; t < triIn; t++) {
    const a = weld[IDX[t * 3]], b = weld[IDX[t * 3 + 1]], c = weld[IDX[t * 3 + 2]];
    if (a === b || b === c || a === c) continue;
    triList.push(a, b, c);
    faceSrc.push(t);
    for (let k = 0; k < 3; k++) {
      const s = IDX[t * 3 + k];
      if (hasNor) norList.push(N[s * 3], N[s * 3 + 1], N[s * 3 + 2]);
      if (hasUv) uvList.push(UV[s * 2], UV[s * 2 + 1]);
    }
  }
  const fv = Int32Array.from(triList);
  const fNor = Float64Array.from(norList);
  const fUv = Float64Array.from(uvList);
  const nf = fv.length / 3;
  if (nf <= target) {
    return { pos: P, nor: N || [], uv: UV || [], idx: IDX, tris: triIn, verts: nvIn, collapses: 0 };
  }
  const alive = new Uint8Array(nf).fill(1);
  const deadV = new Uint8Array(nv);
  let liveTris = nf;

  const fgn = new Float64Array(nf * 3); // geometric face normals (unit)
  const farea = new Float64Array(nf);
  function faceNormal(f) {
    const a = fv[f * 3] * 3, b = fv[f * 3 + 1] * 3, c = fv[f * 3 + 2] * 3;
    const ux = vp[b] - vp[a], uy = vp[b + 1] - vp[a + 1], uz = vp[b + 2] - vp[a + 2];
    const wx = vp[c] - vp[a], wy = vp[c + 1] - vp[a + 1], wz = vp[c + 2] - vp[a + 2];
    const nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
    const len = Math.hypot(nx, ny, nz);
    if (len > 0) {
      fgn[f * 3] = nx / len;
      fgn[f * 3 + 1] = ny / len;
      fgn[f * 3 + 2] = nz / len;
      farea[f] = len * 0.5;
    } else {
      fgn[f * 3] = 0;
      fgn[f * 3 + 1] = 0;
      fgn[f * 3 + 2] = 0;
      farea[f] = 0;
    }
  }

  const vf = Array.from({ length: nv }, () => []);

  // Optional per-face importance (default 1). The caller can pass a weight per
  // input triangle to make painted colour regions resist simplification: the
  // kit paints body, glass and trim with different palette swatches, and a
  // plain area quadric happily deletes a small high-contrast strip.
  const fwIn = opts.faceWeight;
  const fw = new Float64Array(nf).fill(1);
  if (fwIn) for (let f = 0; f < nf; f++) fw[f] = fwIn[faceSrc[f]] ?? 1;

  // Optional per-face painted colour (rgb 0..1, per input triangle). A collapse
  // deletes the faces shared by its two vertices and hands their area to the
  // surrounding faces; if the paint differs, the deletion repaints that area.
  // The penalty below prices exactly that, so a large flat white windshield is
  // far more expensive to delete than another metre of body paint.
  const fColIn = opts.faceColour;
  const fCol = new Float64Array(nf * 3);
  if (fColIn) {
    for (let f = 0; f < nf; f++) {
      const s = faceSrc[f];
      fCol[f * 3] = fColIn[s * 3];
      fCol[f * 3 + 1] = fColIn[s * 3 + 1];
      fCol[f * 3 + 2] = fColIn[s * 3 + 2];
    }
  }

  const Q = new Float64Array(nv * 10);
  function addPlane(v, nx, ny, nz, d, w) {
    const o = v * 10;
    Q[o] += w * nx * nx;
    Q[o + 1] += w * nx * ny;
    Q[o + 2] += w * nx * nz;
    Q[o + 3] += w * ny * ny;
    Q[o + 4] += w * ny * nz;
    Q[o + 5] += w * nz * nz;
    Q[o + 6] += w * nx * d;
    Q[o + 7] += w * ny * d;
    Q[o + 8] += w * nz * d;
    Q[o + 9] += w * d * d;
  }

  for (let f = 0; f < nf; f++) {
    faceNormal(f);
    const a = fv[f * 3], b = fv[f * 3 + 1], c = fv[f * 3 + 2];
    vf[a].push(f);
    vf[b].push(f);
    vf[c].push(f);
    if (!(farea[f] > 0)) continue;
    const nx = fgn[f * 3], ny = fgn[f * 3 + 1], nz = fgn[f * 3 + 2];
    const d = -(nx * vp[a * 3] + ny * vp[a * 3 + 1] + nz * vp[a * 3 + 2]);
    const w = farea[f] * fw[f];
    addPlane(a, nx, ny, nz, d, w);
    addPlane(b, nx, ny, nz, d, w);
    addPlane(c, nx, ny, nz, d, w);
  }

  // --- boundary and crease constraints -------------------------------------
  const emap = new Map();
  for (let f = 0; f < nf; f++) {
    for (let k = 0; k < 3; k++) {
      const a = fv[f * 3 + k], b = fv[f * 3 + ((k + 1) % 3)];
      const lo = a < b ? a : b, hi = a < b ? b : a;
      const key = lo * nv + hi;
      let e = emap.get(key);
      if (!e) {
        e = { a: lo, b: hi, n: 0, f0: -1, f1: -1 };
        emap.set(key, e);
      }
      e.n++;
      if (e.f0 < 0) e.f0 = f;
      else if (e.f1 < 0) e.f1 = f;
    }
  }
  // corner index of vertex v inside face f (the face must contain v)
  function cornerOf(f, v) {
    if (fv[f * 3] === v) return 0;
    if (fv[f * 3 + 1] === v) return 1;
    return 2;
  }

  for (const e of emap.values()) {
    const ax = vp[e.a * 3], ay = vp[e.a * 3 + 1], az = vp[e.a * 3 + 2];
    const ex = vp[e.b * 3] - ax, ey = vp[e.b * 3 + 1] - ay, ez = vp[e.b * 3 + 2] - az;
    const elen = Math.hypot(ex, ey, ez);
    if (!(elen > 0)) continue;
    let nx = 0, ny = 0, nz = 0, weight = 0;
    if (e.n === 1) {
      // boundary: keep the loop by penalising movement out of the face plane
      nx = fgn[e.f0 * 3];
      ny = fgn[e.f0 * 3 + 1];
      nz = fgn[e.f0 * 3 + 2];
      weight = elen * elen * boundaryWeight * fw[e.f0];
    } else if (e.n === 2 && e.f1 >= 0) {
      // painted seam: the two faces put different UVs on the same edge, so this
      // edge is a colour boundary and must not be swept away by a collapse
      let seamEdge = false;
      if (hasUv) {
        const o0 = e.f0 * 6, o1 = e.f1 * 6;
        const a0 = o0 + cornerOf(e.f0, e.a) * 2, a1 = o1 + cornerOf(e.f1, e.a) * 2;
        const b0 = o0 + cornerOf(e.f0, e.b) * 2, b1 = o1 + cornerOf(e.f1, e.b) * 2;
        seamEdge = Math.abs(fUv[a0] - fUv[a1]) > 1e-5 || Math.abs(fUv[a0 + 1] - fUv[a1 + 1]) > 1e-5 ||
          Math.abs(fUv[b0] - fUv[b1]) > 1e-5 || Math.abs(fUv[b0 + 1] - fUv[b1 + 1]) > 1e-5;
      }
      const dot = fgn[e.f0 * 3] * fgn[e.f1 * 3] + fgn[e.f0 * 3 + 1] * fgn[e.f1 * 3 + 1] + fgn[e.f0 * 3 + 2] * fgn[e.f1 * 3 + 2];
      if (dot > featureCos && !seamEdge) continue;
      // crease: plane through the edge, perpendicular to the average normal
      let axN = fgn[e.f0 * 3] + fgn[e.f1 * 3];
      let ayN = fgn[e.f0 * 3 + 1] + fgn[e.f1 * 3 + 1];
      let azN = fgn[e.f0 * 3 + 2] + fgn[e.f1 * 3 + 2];
      const l = Math.hypot(axN, ayN, azN) || 1;
      axN /= l; ayN /= l; azN /= l;
      const px = ey * azN - ez * ayN;
      const py = ez * axN - ex * azN;
      const pz = ex * ayN - ey * axN;
      const pl = Math.hypot(px, py, pz);
      if (!(pl > 1e-12)) continue;
      nx = px / pl; ny = py / pl; nz = pz / pl;
      weight = elen * elen * (seamEdge ? seamWeight : featureWeight) * fw[e.f0];
    } else {
      continue;
    }
    const d = -(nx * ax + ny * ay + nz * az);
    addPlane(e.a, nx, ny, nz, d, weight);
    addPlane(e.b, nx, ny, nz, d, weight);
  }

  // --- collapse machinery ---------------------------------------------------
  const qs = new Float64Array(10);
  function sumQ(u, v) {
    const ou = u * 10, ov = v * 10;
    for (let k = 0; k < 10; k++) qs[k] = Q[ou + k] + Q[ov + k];
  }
  function costAt(x, y, z) {
    return (
      qs[0] * x * x + 2 * qs[1] * x * y + 2 * qs[2] * x * z +
      qs[3] * y * y + 2 * qs[4] * y * z + qs[5] * z * z +
      2 * (qs[6] * x + qs[7] * y + qs[8] * z) + qs[9]
    );
  }

  const heap = new MinHeap();
  function hasBoth(g, u, v) {
    let a = 0, b = 0;
    for (let k = 0; k < 3; k++) {
      const w = fv[g * 3 + k];
      if (w === u) a = 1;
      else if (w === v) b = 1;
    }
    return a + b === 2;
  }
  // Area handed to faces whose paint differs, per removed face: the colour
  // distance to the closest surviving neighbour times the face area.
  function paintPenalty(u, v, shared) {
    let total = 0;
    for (let i = 0; i < shared.length; i++) {
      const f = shared[i];
      const fr = fCol[f * 3], fg = fCol[f * 3 + 1], fb = fCol[f * 3 + 2];
      let best = 3;
      for (let pass = 0; pass < 2; pass++) {
        const list = pass === 0 ? vf[u] : vf[v];
        for (let j = 0; j < list.length; j++) {
          const g = list[j];
          if (!alive[g] || g === f || hasBoth(g, u, v)) continue;
          const dr = fCol[g * 3] - fr, dg = fCol[g * 3 + 1] - fg, db = fCol[g * 3 + 2] - fb;
          const d = dr * dr + dg * dg + db * db;
          if (d < best) best = d;
        }
      }
      total += farea[f] * Math.sqrt(best);
    }
    return total;
  }
  function bestFor(u, v) {
    sumQ(u, v);
    const ux = vp[u * 3], uy = vp[u * 3 + 1], uz = vp[u * 3 + 2];
    const vx = vp[v * 3], vy = vp[v * 3 + 1], vz = vp[v * 3 + 2];
    let bestX = (ux + vx) * 0.5, bestY = (uy + vy) * 0.5, bestZ = (uz + vz) * 0.5;
    let best = costAt(bestX, bestY, bestZ);
    const cu = costAt(ux, uy, uz);
    if (cu < best) { best = cu; bestX = ux; bestY = uy; bestZ = uz; }
    const cv = costAt(vx, vy, vz);
    if (cv < best) { best = cv; bestX = vx; bestY = vy; bestZ = vz; }
    const a11 = qs[0], a12 = qs[1], a13 = qs[2], a22 = qs[3], a23 = qs[4], a33 = qs[5];
    const det = a11 * (a22 * a33 - a23 * a23) - a12 * (a12 * a33 - a23 * a13) + a13 * (a12 * a23 - a22 * a13);
    const tr = Math.abs(a11) + Math.abs(a22) + Math.abs(a33);
    if (tr > 0 && Math.abs(det) > 1e-7 * tr * tr * tr) {
      const i11 = (a22 * a33 - a23 * a23) / det;
      const i12 = (a13 * a23 - a12 * a33) / det;
      const i13 = (a12 * a23 - a13 * a22) / det;
      const i22 = (a11 * a33 - a13 * a13) / det;
      const i23 = (a12 * a13 - a11 * a23) / det;
      const i33 = (a11 * a22 - a12 * a12) / det;
      const b1 = qs[6], b2 = qs[7], b3 = qs[8];
      const ox = -(i11 * b1 + i12 * b2 + i13 * b3);
      const oy = -(i12 * b1 + i22 * b2 + i23 * b3);
      const oz = -(i13 * b1 + i23 * b2 + i33 * b3);
      const elen = Math.hypot(vx - ux, vy - uy, vz - uz);
      if (Number.isFinite(ox + oy + oz) && Math.hypot(ox - bestX, oy - bestY, oz - bestZ) <= Math.max(elen * 2, 1e-6)) {
        const c = costAt(ox, oy, oz);
        if (c < best) { best = c; bestX = ox; bestY = oy; bestZ = oz; }
      }
    }
    return { x: bestX, y: bestY, z: bestZ, cost: best + (fColIn ? paintWeight * paintPenalty(u, v, findShared(u, v)) : 0) };
  }

  const markF = new Int32Array(nf);
  let stampF = 0;
  const sharedList = [];
  function findShared(u, v) {
    stampF++;
    sharedList.length = 0;
    for (let i = 0; i < vf[v].length; i++) {
      const f = vf[v][i];
      if (alive[f]) markF[f] = stampF;
    }
    for (let i = 0; i < vf[u].length; i++) {
      const f = vf[u][i];
      if (alive[f] && markF[f] === stampF) sharedList.push(f);
    }
    return sharedList;
  }

  const markShared = new Int32Array(nf);
  let stampS = 0;
  const markN = new Int32Array(nv);
  const seenN = new Int32Array(nv);
  let stampN = 0;
  const tmpN = new Float64Array(3);
  function normalAfter(f, mv, x, y, z) {
    const ia = fv[f * 3], ib = fv[f * 3 + 1], ic = fv[f * 3 + 2];
    const ax = ia === mv ? x : vp[ia * 3], ay = ia === mv ? y : vp[ia * 3 + 1], az = ia === mv ? z : vp[ia * 3 + 2];
    const bx = ib === mv ? x : vp[ib * 3], by = ib === mv ? y : vp[ib * 3 + 1], bz = ib === mv ? z : vp[ib * 3 + 2];
    const cx = ic === mv ? x : vp[ic * 3], cy = ic === mv ? y : vp[ic * 3 + 1], cz = ic === mv ? z : vp[ic * 3 + 2];
    const ux = bx - ax, uy = by - ay, uz = bz - az;
    const wx = cx - ax, wy = cy - ay, wz = cz - az;
    const nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
    const len = Math.hypot(nx, ny, nz);
    if (!(len > 0)) return 0;
    tmpN[0] = nx / len;
    tmpN[1] = ny / len;
    tmpN[2] = nz / len;
    return len * 0.5;
  }

  function canCollapse(u, v, x, y, z, shared) {
    // manifold link condition: the one-rings must meet only at the edge's
    // opposite vertices, otherwise the collapse pinches two sheets together
    stampN++;
    for (let i = 0; i < vf[u].length; i++) {
      const f = vf[u][i];
      if (!alive[f]) continue;
      markN[fv[f * 3]] = stampN;
      markN[fv[f * 3 + 1]] = stampN;
      markN[fv[f * 3 + 2]] = stampN;
    }
    const seen = stampN * 2 + 1;
    let common = 0;
    for (let i = 0; i < vf[v].length; i++) {
      const f = vf[v][i];
      if (!alive[f]) continue;
      for (let k = 0; k < 3; k++) {
        const w = fv[f * 3 + k];
        if (w !== u && w !== v && markN[w] === stampN && seenN[w] !== seen) {
          seenN[w] = seen;
          common++;
        }
      }
    }
    if (common !== shared.length) return false;
    stampS++;
    for (let i = 0; i < shared.length; i++) markShared[shared[i]] = stampS;
    for (let i = 0; i < vf[u].length; i++) {
      const f = vf[u][i];
      if (!alive[f] || markShared[f] === stampS) continue;
      if (normalAfter(f, u, x, y, z) <= 1e-12) return false;
      if (tmpN[0] * fgn[f * 3] + tmpN[1] * fgn[f * 3 + 1] + tmpN[2] * fgn[f * 3 + 2] < flipCos) return false;
    }
    for (let i = 0; i < vf[v].length; i++) {
      const f = vf[v][i];
      if (!alive[f] || markShared[f] === stampS) continue;
      if (normalAfter(f, v, x, y, z) <= 1e-12) return false;
      if (tmpN[0] * fgn[f * 3] + tmpN[1] * fgn[f * 3 + 1] + tmpN[2] * fgn[f * 3 + 2] < flipCos) return false;
    }
    return true;
  }

  let collapses = 0;
  function doCollapse(u, v, x, y, z) {
    vp[u * 3] = x;
    vp[u * 3 + 1] = y;
    vp[u * 3 + 2] = z;
    for (let i = 0; i < sharedList.length; i++) {
      if (alive[sharedList[i]]) {
        alive[sharedList[i]] = 0;
        liveTris--;
      }
    }
    const moving = vf[v];
    for (let i = 0; i < moving.length; i++) {
      const f = moving[i];
      if (!alive[f]) continue;
      for (let k = 0; k < 3; k++) if (fv[f * 3 + k] === v) fv[f * 3 + k] = u;
      vf[u].push(f);
    }
    vf[v].length = 0;
    deadV[v] = 1;
    for (let k = 0; k < 10; k++) Q[u * 10 + k] += Q[v * 10 + k];
    for (let i = 0; i < vf[u].length; i++) {
      const f = vf[u][i];
      if (alive[f]) faceNormal(f);
    }
    collapses++;
    pushRing(u);
  }

  const ringSeen = new Set();
  function pushRing(u) {
    ringSeen.clear();
    for (let i = 0; i < vf[u].length; i++) {
      const f = vf[u][i];
      if (!alive[f]) continue;
      for (let k = 0; k < 3; k++) {
        const w = fv[f * 3 + k];
        if (w === u || ringSeen.has(w)) continue;
        ringSeen.add(w);
        pushBest(u, w);
      }
    }
  }
  function pushBest(u, v) {
    if (deadV[u] || deadV[v] || u === v) return;
    const p = bestFor(u, v);
    heap.push({ c: p.cost, u: u, v: v });
  }

  for (const e of emap.values()) pushBest(e.a, e.b);

  let guard = nf * 40;
  while (liveTris > target && heap.size && guard-- > 0) {
    const node = heap.pop();
    const u = node.u, v = node.v;
    if (deadV[u] || deadV[v] || u === v) continue;
    const shared = findShared(u, v);
    if (!shared.length) continue;
    const p = bestFor(u, v);
    if (p.cost > node.c * 1.0005 + 1e-12) {
      heap.push({ c: p.cost, u: u, v: v });
      continue;
    }
    if (!canCollapse(u, v, p.x, p.y, p.z, shared)) continue;
    doCollapse(u, v, p.x, p.y, p.z);
  }

  // --- emit with deduplicated (position, normal, uv) corners ---------------
  const outPos = [], outNor = [], outUv = [], outIdx = [];
  const vkey = new Map();
  for (let f = 0; f < nf; f++) {
    if (!alive[f]) continue;
    for (let k = 0; k < 3; k++) {
      const vi = fv[f * 3 + k];
      let key = String(vi);
      if (hasNor) key += '|' + Math.round(fNor[f * 9 + k * 3] * 4000) + ',' + Math.round(fNor[f * 9 + k * 3 + 1] * 4000) + ',' + Math.round(fNor[f * 9 + k * 3 + 2] * 4000);
      if (hasUv) key += '|' + Math.round(fUv[f * 6 + k * 2] * 20000) + ',' + Math.round(fUv[f * 6 + k * 2 + 1] * 20000);
      const id = vkey.get(key);
      if (id !== undefined) {
        outIdx.push(id);
        continue;
      }
      const next = outPos.length / 3;
      vkey.set(key, next);
      outPos.push(vp[vi * 3], vp[vi * 3 + 1], vp[vi * 3 + 2]);
      if (hasNor) outNor.push(fNor[f * 9 + k * 3], fNor[f * 9 + k * 3 + 1], fNor[f * 9 + k * 3 + 2]);
      if (hasUv) outUv.push(fUv[f * 6 + k * 2], fUv[f * 6 + k * 2 + 1]);
      outIdx.push(next);
    }
  }
  return { pos: outPos, nor: outNor, uv: outUv, idx: outIdx, tris: liveTris, verts: outPos.length / 3, collapses: collapses };
}

// Binary min-heap keyed on `c`.
class MinHeap {
  constructor() {
    this.a = [];
  }
  get size() {
    return this.a.length;
  }
  push(node) {
    const a = this.a;
    a.push(node);
    let i = a.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (a[p].c <= a[i].c) break;
      const t = a[p];
      a[p] = a[i];
      a[i] = t;
      i = p;
    }
  }
  pop() {
    const a = this.a;
    const top = a[0];
    const last = a.pop();
    if (a.length) {
      a[0] = last;
      let i = 0;
      for (;;) {
        const l = i * 2 + 1, r = l + 1;
        let m = i;
        if (l < a.length && a[l].c < a[m].c) m = l;
        if (r < a.length && a[r].c < a[m].c) m = r;
        if (m === i) break;
        const t = a[m];
        a[m] = a[i];
        a[i] = t;
        i = m;
      }
    }
    return top;
  }
}
