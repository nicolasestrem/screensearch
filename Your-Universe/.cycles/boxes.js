// Cycle 20 scratch: page-side screen box measurement, installed as window.__lt20.
// Evaluated once per page by the Eval harness; the file is not part of the deliverable.
(() => {
  function project(x, y, z) {
    const api = window.localtime;
    const cam = api.rig.camera;
    const v = api.rig.lookAt.clone();
    v.set(x, y, z);
    v.project(cam);
    return { x: (v.x * 0.5 + 0.5) * innerWidth, y: (-v.y * 0.5 + 0.5) * innerHeight };
  }

  function boxOf(pts) {
    const W = innerWidth, H = innerHeight;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const p of pts) {
      if (p.x < minX) minX = p.x;
      if (p.x > maxX) maxX = p.x;
      if (p.y < minY) minY = p.y;
      if (p.y > maxY) maxY = p.y;
    }
    const cx = Math.max(0, Math.min(W - 1, Math.round(minX)));
    const cy = Math.max(0, Math.min(H - 1, Math.round(minY)));
    const ax = Math.max(0, Math.min(W, Math.round(maxX)));
    const ay = Math.max(0, Math.min(H, Math.round(maxY)));
    return { x: cx, y: cy, w: Math.max(1, ax - cx), h: Math.max(1, ay - cy) };
  }

  // The tower pass merges the glass blocks into one mesh, so the blocks are recovered from an
  // 8 m occupancy grid of its vertices joined by 8-neighbour adjacency. Two blocks share one x
  // band in this district and stand 12 m apart in z, so a component whose z extent exceeds 24 m
  // holds exactly two 17.5 m blocks and is split at the midpoint of its z extent.
  function towerBoxes() {
    const api = window.localtime;
    const mesh = api.world.group.getObjectByName('towers');
    if (!mesh || !mesh.geometry) return [];
    const pos = mesh.geometry.attributes.position;
    const CELL = 8;
    const keyOf = (ix, iz) => ix + ',' + iz;
    const cells = new Map();
    for (let i = 0; i < pos.count; i += 1) {
      const x = pos.getX(i), y = pos.getY(i), z = pos.getZ(i);
      const ix = Math.floor(x / CELL), iz = Math.floor(z / CELL);
      const k = keyOf(ix, iz);
      let c = cells.get(k);
      if (!c) { c = { ix: ix, iz: iz, root: k, verts: [] }; cells.set(k, c); }
      c.verts.push(x, y, z);
    }
    const find = (k) => { let c = cells.get(k); while (c.root !== keyOf(c.ix, c.iz)) c = cells.get(c.root); return c; };
    for (const c of cells.values()) {
      for (let dx = -1; dx <= 1; dx += 1) {
        for (let dz = -1; dz <= 1; dz += 1) {
          if (dx === 0 && dz === 0) continue;
          const nk = keyOf(c.ix + dx, c.iz + dz);
          if (!cells.has(nk)) continue;
          const a = find(keyOf(c.ix, c.iz));
          const b = find(nk);
          if (a !== b) b.root = keyOf(a.ix, a.iz);
        }
      }
    }
    const groups = new Map();
    for (const c of cells.values()) {
      const r = find(keyOf(c.ix, c.iz));
      const rk = keyOf(r.ix, r.iz);
      let g = groups.get(rk);
      if (!g) { g = []; groups.set(rk, g); }
      for (let i = 0; i < c.verts.length; i += 3) g.push(c.verts[i], c.verts[i + 1], c.verts[i + 2]);
    }
    function bboxOf(verts, zLo, zHi) {
      let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity, minZ = Infinity, maxZ = -Infinity;
      for (let i = 0; i < verts.length; i += 3) {
        const z = verts[i + 2];
        if (z < zLo || z > zHi) continue;
        const x = verts[i], y = verts[i + 1];
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
        if (z < minZ) minZ = z;
        if (z > maxZ) maxZ = z;
      }
      return { minX: minX, maxX: maxX, minY: minY, maxY: maxY, minZ: minZ, maxZ: maxZ };
    }
    const boxes = [];
    for (const verts of groups.values()) {
      const all = bboxOf(verts, -Infinity, Infinity);
      if (!isFinite(all.minX)) continue;
      if (all.maxZ - all.minZ > 24) {
        const mid = (all.minZ + all.maxZ) / 2;
        boxes.push(bboxOf(verts, -Infinity, mid));
        boxes.push(bboxOf(verts, mid, Infinity));
      } else {
        boxes.push(all);
      }
    }
    const out = [];
    for (const g of boxes) {
      const pts = [];
      for (const x of [g.minX, g.maxX]) for (const y of [g.minY, g.maxY]) for (const z of [g.minZ, g.maxZ]) pts.push(project(x, y, z));
      out.push({
        extent: { minX: Math.round(g.minX), maxX: Math.round(g.maxX), minZ: Math.round(g.minZ), maxZ: Math.round(g.maxZ), base: Math.round(g.minY), top: Math.round(g.maxY) },
        box: boxOf(pts)
      });
    }
    out.sort((a, b) => b.extent.top - a.extent.top);
    return out;
  }

  function measure() {
    const api = window.localtime;
    const W = innerWidth, H = innerHeight;
    const res = { viewport: { W, H }, towers: [], market: null, corridor: null, chantier: null };
    res.towers = towerBoxes();
    const ms = api.world.marketSquare;
    if (ms) {
      const h = ms.size / 2;
      const pts = [];
      for (const dx of [-h, h]) for (const dz of [-h, h]) for (const dy of [0, 9]) pts.push(project(ms.center.x + dx, ms.center.y + dy, ms.center.z + dz));
      res.market = boxOf(pts);
    }
    const dp = api.model.dreamPair || null;
    let a = dp ? dp.a : -1, b = dp ? dp.b : -1;
    if (a < 0 || b < 0) {
      // No pair: nothing is laid, so the corridor is reported empty.
      res.chantier = { a: -1, b: -1 };
      return res;
    }
    const ca = api.world.quarters[a].center, cb = api.world.quarters[b].center;
    let dx = cb.x - ca.x, dz = cb.z - ca.z;
    const len = Math.hypot(dx, dz);
    dx /= len; dz /= len;
    const nx = -dz, nz = dx;
    const pts = [];
    for (const [cx, cz] of [[ca.x, ca.z], [cb.x, cb.z]]) {
      for (const s of [-4.5, 4.5]) {
        const px = cx + nx * s, pz = cz + nz * s;
        pts.push(project(px, api.world.groundHeightAt(px, pz) + 0.2, pz));
      }
    }
    res.corridor = boxOf(pts);
    // The works volume: the same strip lifted to the mast tops, so the box holds the
    // ribbon, the light strip, the crew and both floodheads.
    const tall = [];
    for (const [cx, cz] of [[ca.x, ca.z], [cb.x, cb.z]]) {
      for (const s of [-7, 7]) {
        const px = cx + nx * s, pz = cz + nz * s;
        const gy = api.world.groundHeightAt(px, pz);
        tall.push(project(px, gy + 0.2, pz));
        tall.push(project(px, gy + 8, pz));
      }
    }
    res.works = boxOf(tall);
    res.chantier = {
      a: a, b: b, span: Math.round(len),
      names: [api.model.quarters[a].name, api.model.quarters[b].name],
      ca: { x: Math.round(ca.x), z: Math.round(ca.z) },
      cb: { x: Math.round(cb.x), z: Math.round(cb.z) }
    };
    return res;
  }

  window.__lt20 = { measure: measure, project: project };
  return 'ok';
})()
