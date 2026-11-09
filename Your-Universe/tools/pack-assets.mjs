// Builds assets/<pack>.glb (one GLB per selected pack) and assets/manifest.json
// from tools/asset-selection.json. Node built-ins only.
//
//   node tools/pack-assets.mjs          build (rewrites assets/)
//   node tools/pack-assets.mjs --check  verify existing outputs without rebuilding
//
// Per model: source graph is walked, every vertex is transformed by
// T_recenter * R_y(rotateY) * S(scale) * M_world, recentered so min y = 0 and
// x/z centred, normals transformed by the inverse-transpose rotation, and all
// primitives sharing one material identity are merged into a single mesh
// ("name"; extra material groups become "name__m2", "name__m3", ... top-level
// nodes and manifest entries of category "part").
//
// Textured kits keep their palette PNG embedded (deduped per pack) with one
// material per distinct texture+factors set. Unlit, untextured sources
// (nature kit) get their baseColorFactor baked into a COLOR_0 attribute and a
// single vertex-colour PBR material per pack (metallic 0, roughness 0.9), so
// trees receive light and remain tintable.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { parseGLB, makeReader, IDENT, mul, nodeMatrix, xform, rotateY, walkNodes, decimateGeometry, decodePNG } from './gltf-lib.mjs';
import { verifyCarLod } from './check-assets.mjs';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const SELECTION = join(ROOT, 'tools', 'asset-selection.json');
const ASSETS = join(ROOT, 'assets');

const check = process.argv.includes('--check');
if (check) {
  const { verify } = await import('./check-assets.mjs');
  const ok = verify();
  process.exitCode = ok ? 0 : 1;
} else {
  const ok = build();
  if (!ok) process.exitCode = 1;
}

function sourceRoots(json) {
  return json.scenes[json.scene ?? 0].nodes;
}

// Per input triangle, the painted colour it carries: the palette texel under
// each corner UV, averaged. The car kit paints flat swatches on flat faces, so
// nearest-texel sampling returns the exact paint the face shows.
function faceColours(g, tex) {
  const nf = g.idx.length / 3;
  const out = new Float32Array(nf * 3);
  for (let f = 0; f < nf; f++) {
    let r = 0, gg = 0, b = 0;
    for (let k = 0; k < 3; k++) {
      const v = g.idx[f * 3 + k];
      const x = Math.min(tex.w - 1, Math.max(0, Math.round(g.uv[v * 2] * tex.w - 0.5)));
      const y = Math.min(tex.h - 1, Math.max(0, Math.round(g.uv[v * 2 + 1] * tex.h - 0.5)));
      const o = (y * tex.w + x) * tex.ch;
      r += tex.data[o] / 255;
      gg += tex.data[o + 1] / 255;
      b += tex.data[o + 2] / 255;
    }
    out[f * 3] = r / 3;
    out[f * 3 + 1] = gg / 3;
    out[f * 3 + 2] = b / 3;
  }
  return out;
}

// Material identity + normalized description for one source primitive.
// Untextured sources get baseColorFactor baked into COLOR_0 and a forced
// matte, double-sided PBR material (unlit sources, e.g. the nature kit's
// top-level KHR_materials_unlit, would otherwise arrive as metallic 1).
function materialInfo(json, prim, dir) {
  const mat = (prim.material !== undefined && json.materials[prim.material]) || {};
  const pbr = mat.pbrMetallicRoughness || {};
  const factor = pbr.baseColorFactor || [1, 1, 1, 1];
  const doubleSided = !!mat.doubleSided;
  const alphaMode = mat.alphaMode || 'OPAQUE';
  const texRef = pbr.baseColorTexture;
  if (texRef !== undefined) {
    const tex = json.textures[texRef.index];
    const img = json.images[tex.source];
    const bytes = readFileSync(join(dir, img.uri));
    const hash = createHash('sha256').update(bytes).digest('hex');
    const sampler = tex.sampler !== undefined ? json.samplers[tex.sampler] : {};
    const metallic = pbr.metallicFactor ?? 1;
    const roughness = pbr.roughnessFactor ?? 1;
    return {
      mode: 'tex', hash, bytes, imgName: img.name || '', mimeType: 'image/png',
      sampler: { magFilter: sampler.magFilter ?? 9729, minFilter: sampler.minFilter ?? 9729 },
      factor, metallic, roughness, doubleSided, alphaMode,
      key: JSON.stringify(['tex', hash, factor, metallic, roughness, doubleSided, alphaMode])
    };
  }
  // baked vertex-colour material: matte and double-sided by contract
  return {
    mode: 'vc', factor, metallic: 0, roughness: 0.9, doubleSided: true, alphaMode,
    key: JSON.stringify(['vc', 0, 0.9, true, alphaMode])
  };
}

function build() {
  const sel = JSON.parse(readFileSync(SELECTION, 'utf8'));
  const byPack = new Map(sel.packs.map(p => [p.id, []]));
  for (const m of sel.models) {
    if (!byPack.has(m.pack)) throw new Error('model ' + m.name + ' references unknown pack ' + m.pack);
    byPack.get(m.pack).push(m);
  }

  const manifest = { version: 1, packs: [], models: {} };

  for (const pack of sel.packs) {
    const models = byPack.get(pack.id);
    if (!models.length) continue;
    const dir = join(ROOT, pack.dir);
    if (!existsSync(dir)) throw new Error('pack dir missing: ' + pack.dir);

    // pack GLB assembly state
    const views = [], accessors = [], meshes = [], nodes = [], sceneNodes = [];
    const materials = [], images = [], samplers = [], textures = [];
    const chunks = [];
    let binLen = 0;
    const imageIdx = new Map();   // sha256 -> image index
    const samplerIdx = new Map(); // sampler JSON -> index
    const materialIdx = new Map();// material key -> index
    let usesVertexColors = false;

    function addView(data, target) {
      const pad = (4 - (binLen % 4)) % 4;
      if (pad) { chunks.push(Buffer.alloc(pad)); binLen += pad; }
      const byteOffset = binLen;
      chunks.push(Buffer.from(data.buffer, data.byteOffset, data.byteLength));
      binLen += data.byteLength;
      const id = views.length;
      views.push(target === undefined
        ? { buffer: 0, byteOffset, byteLength: data.byteLength }
        : { buffer: 0, byteOffset, byteLength: data.byteLength, target });
      return id;
    }
    function addAccessor(data, componentType, count, type, opts = {}) {
      const bv = addView(data, opts.target);
      const id = accessors.length;
      const acc = { bufferView: bv, componentType, count, type };
      if (opts.normalized) acc.normalized = true;
      if (opts.min) acc.min = opts.min;
      if (opts.max) acc.max = opts.max;
      accessors.push(acc);
      return id;
    }

    function materialIndex(info) {
      let mi = materialIdx.get(info.key);
      if (mi !== undefined) return mi;
      const pbr = {};
      let name;
      if (info.mode === 'tex') {
        let ii = imageIdx.get(info.hash);
        if (ii === undefined) {
          const bv = addView(info.bytes);
          ii = images.length;
          images.push({ bufferView: bv, mimeType: info.mimeType, name: info.imgName });
          imageIdx.set(info.hash, ii);
        }
        const sk = JSON.stringify(info.sampler);
        let si = samplerIdx.get(sk);
        if (si === undefined) { si = samplers.length; samplers.push(info.sampler); samplerIdx.set(sk, si); }
        const ti = textures.length;
        textures.push({ sampler: si, source: ii });
        pbr.baseColorFactor = info.factor.map(v => +v.toFixed(6));
        pbr.baseColorTexture = { index: ti };
        pbr.metallicFactor = info.metallic;
        if (info.roughness !== 1) pbr.roughnessFactor = info.roughness;
        name = info.imgName || 'colormap';
      } else {
        usesVertexColors = true;
        pbr.baseColorFactor = [1, 1, 1, 1];
        pbr.metallicFactor = info.metallic;
        pbr.roughnessFactor = info.roughness;
        name = 'vertex-color';
      }
      const entry = { name, pbrMetallicRoughness: pbr };
      if (info.doubleSided) entry.doubleSided = true;
      if (info.alphaMode !== 'OPAQUE') entry.alphaMode = info.alphaMode;
      mi = materials.length;
      materials.push(entry);
      materialIdx.set(info.key, mi);
      return mi;
    }

    const packModels = [];
    // decoded palette per distinct texture (sha256), used by the LOD decimator
    const palettes = new Map();
    function paletteOf(hash, bytes) {
      let p = palettes.get(hash);
      if (!p) {
        p = decodePNG(bytes);
        palettes.set(hash, p);
      }
      return p;
    }
    for (const model of models) {
      const file = join(dir, model.file);
      if (!existsSync(file)) throw new Error('model file missing: ' + model.file);
      const src = parseGLB(readFileSync(file));
      const reader = makeReader(src.json, src.bin);
      const scale = model.scale ?? pack.scale;
      const S = mul(rotateY(model.rotateY ?? 0), [scale, 0, 0, 0, 0, scale, 0, 0, 0, 0, scale, 0, 0, 0, 0, 1]);

      // gather primitives grouped by material identity
      const groups = new Map();
      const groupOf = (info) => {
        let g = groups.get(info.key);
        if (!g) {
          g = { info, pos: [], nor: [], uv: [], col: [], idx: [], verts: 0, tris: 0 };
          groups.set(info.key, g);
        }
        return g;
      };

      for (const root of sourceRoots(src.json)) {
        walkNodes(src.json, root, IDENT, (meshIndex, world) => {
          for (const prim of src.json.meshes[meshIndex].primitives) {
            if (prim.mode !== undefined && prim.mode !== 4) throw new Error(model.name + ': non-triangle primitive');
            const info = materialInfo(src.json, prim, dir);
            const g = groupOf(info);
            const P = reader.accessor(prim.attributes.POSITION);
            const N = prim.attributes.NORMAL !== undefined ? reader.accessor(prim.attributes.NORMAL) : null;
            const UV = prim.attributes.TEXCOORD_0 !== undefined ? reader.accessor(prim.attributes.TEXCOORD_0) : null;
            // normals: inverse-transpose of the linear part of S*world (uniform scale + rotation here)
            const R = normalMatrix(mul(S, world));
            const f = info.factor;
            const bakeColor = info.mode === 'vc';
            const base = g.verts;
            const n = P.length / 3;
            for (let i = 0; i < n; i++) {
              const p = xform(mul(S, world), P[i * 3], P[i * 3 + 1], P[i * 3 + 2]);
              g.pos.push(p[0], p[1], p[2]);
              if (N) {
                const nx = N[i * 3], ny = N[i * 3 + 1], nz = N[i * 3 + 2];
                let tx = R[0] * nx + R[1] * ny + R[2] * nz;
                let ty = R[3] * nx + R[4] * ny + R[5] * nz;
                let tz = R[6] * nx + R[7] * ny + R[8] * nz;
                const l = Math.hypot(tx, ty, tz) || 1;
                g.nor.push(tx / l, ty / l, tz / l);
              }
              if (UV) g.uv.push(UV[i * 2], UV[i * 2 + 1]);
              if (bakeColor) {
                const cl = (v) => Math.max(0, Math.min(255, Math.round(v * 255)));
                g.col.push(cl(f[0]), cl(f[1]), cl(f[2]), info.alphaMode === 'OPAQUE' ? 255 : cl(f[3] ?? 1));
              }
            }
            const I = prim.indices !== undefined ? reader.accessor(prim.indices) : null;
            const count = I ? I.length : n;
            for (let t = 0; t < count; t++) g.idx.push(base + (I ? I[t] : t));
            g.verts += n;
            g.tris += count / 3;
          }
        });
      }
      if (!groups.size) throw new Error(model.name + ': no mesh data');

      // fleet LOD: collapse the merged geometry to about `decimate` triangles.
      // The full-detail entry (same source file, no decimate) is built separately.
      if (model.decimate) {
        const list = [...groups.values()];
        const before = list.reduce((a, g) => a + g.idx.length / 3, 0);
        const target = Math.max(4, Math.round(typeof model.decimate === 'number' ? model.decimate : model.decimate.tris));
        let remaining = target;
        for (let i = 0; i < list.length; i++) {
          const g = list[i];
          const tris = g.idx.length / 3;
          const groupTarget = i === list.length - 1
            ? Math.max(4, remaining)
            : Math.max(4, Math.round((target * tris) / before));
          remaining -= groupTarget;
          const opts = { seamWeight: 60, paintWeight: 3 };
          if (g.info.mode === 'tex' && g.uv.length) opts.faceColour = faceColours(g, paletteOf(g.info.hash, g.info.bytes));
          const out = decimateGeometry({ pos: g.pos, nor: g.nor, uv: g.uv, idx: g.idx }, groupTarget, opts);
          console.log('  ' + model.name + ': ' + tris + ' -> ' + out.tris + ' tris (' + out.collapses + ' collapses, ' + out.verts + ' verts)');
          g.pos = out.pos;
          g.nor = out.nor;
          g.uv = out.uv;
          g.idx = out.idx;
          g.verts = out.verts;
          g.tris = out.tris;
        }
      }

      // recenter: min y = 0, x/z centred on the whole model
      const bb = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
      for (const g of groups.values()) {
        for (let i = 0; i < g.pos.length; i += 3) {
          for (let k = 0; k < 3; k++) {
            if (g.pos[i + k] < bb.min[k]) bb.min[k] = g.pos[i + k];
            if (g.pos[i + k] > bb.max[k]) bb.max[k] = g.pos[i + k];
          }
        }
      }
      const rc = [-(bb.min[0] + bb.max[0]) / 2, -bb.min[1], -(bb.min[2] + bb.max[2]) / 2];
      for (const g of groups.values()) {
        for (let i = 0; i < g.pos.length; i += 3) {
          g.pos[i] += rc[0]; g.pos[i + 1] += rc[1]; g.pos[i + 2] += rc[2];
        }
      }
      const min = [bb.min[0] + rc[0], 0, bb.min[2] + rc[2]];
      const max = [bb.max[0] + rc[0], bb.max[1] + rc[1], bb.max[2] + rc[2]];
      const r4 = (v) => Math.round(v * 10000) / 10000;

      // emit one mesh per group, largest first (main model), rest as parts
      const ordered = [...groups.values()].sort((a, b) => b.tris - a.tris);
      const parts = [];
      ordered.forEach((g, gi) => {
        const name = gi === 0 ? model.name : model.name + '__m' + (gi + 1);
        const posF = Float32Array.from(g.pos);
        const minP = [Infinity, Infinity, Infinity], maxP = [-Infinity, -Infinity, -Infinity];
        for (let i = 0; i < posF.length; i += 3) {
          for (let k = 0; k < 3; k++) {
            if (posF[i + k] < minP[k]) minP[k] = posF[i + k];
            if (posF[i + k] > maxP[k]) maxP[k] = posF[i + k];
          }
        }
        const attrs = { POSITION: addAccessor(posF, 5126, posF.length / 3, 'VEC3', { min: minP, max: maxP }) };
        if (g.nor.length) attrs.NORMAL = addAccessor(Float32Array.from(g.nor), 5126, g.nor.length / 3, 'VEC3');
        if (g.uv.length) attrs.TEXCOORD_0 = addAccessor(Float32Array.from(g.uv), 5126, g.uv.length / 2, 'VEC2');
        if (g.col.length) attrs.COLOR_0 = addAccessor(Uint8Array.from(g.col), 5121, g.col.length / 4, 'VEC4', { normalized: true });
        const maxIndex = Math.max(...g.idx);
        const IdxArr = maxIndex < 65536 ? Uint16Array : Uint32Array;
        const indices = addAccessor(IdxArr.from(g.idx), maxIndex < 65536 ? 5123 : 5125, g.idx.length, 'SCALAR', { target: 34963 });
        const meshIndex = meshes.length;
        meshes.push({ name, primitives: [{ attributes: attrs, indices, material: materialIndex(g.info), mode: 4 }] });
        const nodeIndex = nodes.length;
        nodes.push({ name, mesh: meshIndex });
        sceneNodes.push(nodeIndex);
        const entry = {
          pack: pack.id,
          category: gi === 0 ? model.category : 'part',
          tier: model.tier,
          tris: g.tris,
          bounds: { min: min.map(r4), max: max.map(r4) },
          footprint: { w: r4(max[0] - min[0]), d: r4(max[2] - min[2]), h: r4(max[1]) },
          tintable: !!model.tintable,
          parts: []
        };
        manifest.models[name] = entry;
        if (gi > 0) parts.push(name);
        packModels.push(name);
      });
      if (parts.length) manifest.models[model.name].parts = parts;
    }

    // assemble GLB
    const gltf = {
      asset: { version: '2.0', generator: 'localtime pack-assets' },
      scene: 0,
      scenes: [{ name: pack.id, nodes: sceneNodes }],
      nodes, meshes, materials,
      accessors, bufferViews: views,
      buffers: [{ byteLength: binLen }]
    };
    if (images.length) gltf.images = images;
    if (samplers.length) gltf.samplers = samplers;
    if (textures.length) gltf.textures = textures;

    const tailPad = (4 - (binLen % 4)) % 4;
    if (tailPad) chunks.push(Buffer.alloc(tailPad));
    const bin = Buffer.concat(chunks);
    let jsonBuf = Buffer.from(JSON.stringify(gltf), 'utf8');
    if (jsonBuf.length % 4) jsonBuf = Buffer.concat([jsonBuf, Buffer.alloc(4 - (jsonBuf.length % 4), 0x20)]);
    const total = 12 + 8 + jsonBuf.length + 8 + bin.length;
    const header = Buffer.alloc(12);
    header.writeUInt32LE(0x46546c67, 0);
    header.writeUInt32LE(2, 4);
    header.writeUInt32LE(total, 8);
    const jh = Buffer.alloc(8);
    jh.writeUInt32LE(jsonBuf.length, 0);
    jh.writeUInt32LE(0x4e4f534a, 4);
    const bh = Buffer.alloc(8);
    bh.writeUInt32LE(bin.length, 0);
    bh.writeUInt32LE(0x004e4942, 4);
    const out = join(ASSETS, pack.id + '.glb');
    writeFileSync(out, Buffer.concat([header, jh, jsonBuf, bh, bin]));

    const packEntry = { id: pack.id, file: 'assets/' + pack.id + '.glb', licence: pack.licence, source: pack.source };
    if (usesVertexColors) packEntry.vertexColors = true;
    manifest.packs.push(packEntry);
    console.log('wrote ' + packEntry.file + ' (' + models.length + ' models, ' + (bin.length / 1024).toFixed(0) + ' KB bin)');
  }

  writeFileSync(join(ASSETS, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  const totalTris = Object.values(manifest.models).reduce((a, m) => a + m.tris, 0);
  console.log('wrote manifest.json (' + Object.keys(manifest.models).length + ' models, ' + totalTris + ' tris)');

  // measured quality gate, identical to `node tools/check-assets.mjs`
  const lod = verifyCarLod(manifest);
  for (const line of lod.lines) console.log(line);
  return lod.ok;
}

// inverse-transpose of the upper-left 3x3 of a column-major 4x4, row-major result
function normalMatrix(m) {
  const a = m[0], b = m[4], c = m[8];   // row 0
  const d = m[1], e = m[5], f = m[9];   // row 1
  const g = m[2], h = m[6], i = m[10];  // row 2
  const A = e * i - f * h, B = f * g - d * i, C = d * h - e * g;
  let det = a * A + b * B + c * C;
  if (!det) return [1, 0, 0, 0, 1, 0, 0, 0, 1];
  det = 1 / det;
  // (M^-1)^T in row-major: columns of M^-1 become rows
  return [
    A * det, B * det, C * det,
    (c * h - b * i) * det, (a * i - c * g) * det, (b * g - a * h) * det,
    (b * f - c * e) * det, (c * d - a * f) * det, (a * e - b * d) * det
  ];
}
