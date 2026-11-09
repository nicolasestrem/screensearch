// Cycle 28 repair probe: live state, poses and corner isolates at the street tier.
// Scratch tool, not part of the deliverable.
//
//   await fx28Probe(8731)                 // state + poses, no captures
//   await fx28Iso(8731, '<abs dir>')      // state + 6 corner isolates at the north pose
//
// Absolute output paths only: the harness write() resolves a relative path against the session
// cwd, which is how a previous cycle measured one file and audited another.

const fx28Settle = (ms) => new Promise((r) => setTimeout(r, ms));

async function fx28Tab(port, opts) {
  const o = opts || {};
  const tab = await browser.open({
    name: o.name || 'fx28',
    url: 'about:blank',
    viewport: { width: 1600, height: 900 },
    timeout: 240
  });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${port}/index.html?hour=${o.hour === undefined ? 17 : o.hour}${o.query ? '&' + o.query : ''}&cb=fx28${Date.now()}`);
  const deadline = Date.now() + 45000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await fx28Settle(400);
  }
  await fx28Settle(o.settle === undefined ? 5000 : o.settle);
  return tab;
}

async function fx28Shot(tab, path) {
  const b64 = await tab.run(async ({ page }) => {
    const buf = await page.screenshot();
    return Buffer.from(buf).toString('base64');
  });
  await write(path, Buffer.from(b64, 'base64'));
  return path;
}

async function fx28State(tab) {
  const json = await tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L && L.world;
    if (!w) return JSON.stringify({ error: 'no world' });
    const s = L.stats();
    const out = {
      calls: s.calls, tris: s.triangles, fps: Math.round(s.fps * 10) / 10, tier: L.rig ? L.rig.tier : null,
      cam: L.rig.camera.position.toArray().map((v) => Math.round(v * 10) / 10),
      windowCount: w.windowCount,
      models: w.models,
      roof: w.roofClutter,
      details: w.details,
      buildings: w.buildings.count,
      towers: w.towers.placed,
      fabric: w.fabric
    };
    const meshes = {};
    for (const c of w.group.children) {
      if (!c.isMesh && !c.isInstancedMesh) continue;
      const g = c.geometry;
      const per = (g.index ? g.index.count : (g.attributes.position ? g.attributes.position.count : 0)) / 3;
      const n = c.isInstancedMesh ? c.count : 1;
      meshes[c.name || c.type] = { count: n, tris: Math.round(per * n), vis: c.visible, per: Math.round(per * 10) / 10 };
    }
    out.meshes = meshes;
    return JSON.stringify(out);
  })()`);
  return JSON.parse(json);
}

async function fx28Probe(port) {
  const tab = await fx28Tab(port, { name: 'fx28-state' });
  const rows = [];
  rows.push({ tag: 'district', ...(await fx28State(tab)) });
  await tab.evaluate(`(() => { window.localtime.rig.setTier('street', true); })()`);
  await fx28Settle(4200);
  for (let r = 0; r < 4; r += 1) {
    rows.push({ tag: 'street-yaw' + r, ...(await fx28State(tab)) });
    if (r < 3) {
      await tab.evaluate(`(() => { window.localtime.rig.rotate(1); })()`);
      await fx28Settle(1500);
    }
  }
  await tab.close();
  return rows;
}

// Six captures at the pose the north-facing street frame uses: base, outer fabric hidden,
// details hidden, windows+frames hidden, model buckets hidden, all buildings hidden. Each is
// a full 1600x900 frame; the corner metrics are run outside the browser on the absolute paths.
async function fx28Iso(port, dir, opts) {
  const o = opts || {};
  const tab = await fx28Tab(port, { name: 'fx28-iso' });
  const state = await fx28State(tab);
  await tab.evaluate(`(() => { window.localtime.rig.setTier('street', true); })()`);
  await fx28Settle(4200);
  const rotates = o.rotations === undefined ? 2 : o.rotations;
  for (let r = 0; r < rotates; r += 1) {
    await tab.evaluate(`(() => { window.localtime.rig.rotate(1); })()`);
    await fx28Settle(1500);
  }
  const pose = await tab.evaluate(`(() => window.localtime.rig.camera.position.toArray().map((v) => Math.round(v * 10) / 10))()`);
  const shots = [];
  const take = async (tag) => {
    const p = `${dir}/fx28-iso-${tag}.png`;
    await fx28Shot(tab, p);
    shots.push({ tag, path: p });
  };
  const set = async (js) => { await tab.evaluate(js); await fx28Settle(1100); };
  const restore = async (js) => { await tab.evaluate(js); await fx28Settle(900); };

  await take('base');

  await set(`(() => {
    const w = window.localtime.world;
    window.__fx28bc = w.buildings.count;
    w.buildings.count = w.fabric.tail.buildings;
  })()`);
  await take('no-outer-walls');
  await restore(`(() => { window.localtime.world.buildings.count = window.__fx28bc; })()`);

  await set(`(() => {
    const w = window.localtime.world;
    const m = w.group.getObjectByName('details');
    window.__fx28dv = m.visible;
    m.visible = false;
  })()`);
  await take('no-details');
  await restore(`(() => { window.localtime.world.group.getObjectByName('details').visible = window.__fx28dv; })()`);

  await set(`(() => {
    const g = window.localtime.world.group;
    const a = g.getObjectByName('windows');
    const b = g.getObjectByName('frames');
    window.__fx28wv = a.visible;
    window.__fx28fv = b ? b.visible : null;
    a.visible = false;
    if (b) b.visible = false;
  })()`);
  await take('no-windows');
  await restore(`(() => {
    const g = window.localtime.world.group;
    g.getObjectByName('windows').visible = window.__fx28wv;
    const b = g.getObjectByName('frames');
    if (b && window.__fx28fv !== null) b.visible = window.__fx28fv;
  })()`);

  await set(`(() => {
    const g = window.localtime.world.group;
    window.__fx28mv = [];
    for (const c of g.children) {
      if ((c.isMesh || c.isInstancedMesh) && c.userData && c.userData.quarterOf) {
        window.__fx28mv.push([c, c.visible]);
        c.visible = false;
      }
    }
  })()`);
  await take('no-models');
  await restore(`(() => {
    for (const e of window.__fx28mv) e[0].visible = e[1];
  })()`);

  await set(`(() => {
    const w = window.localtime.world;
    window.__fx28bv = w.buildings.visible;
    w.buildings.visible = false;
  })()`);
  await take('no-buildings');
  await restore(`(() => { window.localtime.world.buildings.visible = window.__fx28bv; })()`);

  await tab.close();
  return { state, pose, shots };
}

// The four canonical frames plus the fallback and the phone frame, written to absolute paths,
// with the state the build reports beside each one. One tab at a time, closed between runs.
async function fx28Final(port, dir, opts) {
  const o = opts || {};
  const hour = o.hour === undefined ? 17 : o.hour;
  const rows = {};

  const dtab = await fx28Tab(port, { name: 'fx28-day', hour });
  rows.day = await fx28State(dtab);
  await fx28Shot(dtab, `${dir}/iter-28-day.png`);
  await dtab.evaluate(`(() => { window.localtime.rig.setTier('street', true); })()`);
  await fx28Settle(4200);
  const street = [];
  for (let r = 0; r < 4; r += 1) {
    const st = await fx28State(dtab);
    street.push({ r, cam: st.cam, calls: st.calls, tris: st.tris });
    await fx28Shot(dtab, `${dir}/iter-28-street-r${r}.png`);
    if (r < 3) {
      await dtab.evaluate(`(() => { window.localtime.rig.rotate(1); })()`);
      await fx28Settle(1500);
    }
  }
  rows.street = street;
  rows.streetState = await fx28State(dtab);
  await dtab.close();

  const ftab = await fx28Tab(port, { name: 'fx28-fallback', hour, query: 'noassets=1' });
  rows.fallback = await fx28State(ftab);
  await fx28Shot(ftab, `${dir}/iter-28-noassets.png`);
  await ftab.close();

  const ptab = await browser.open({
    name: 'fx28-phone',
    url: 'about:blank',
    viewport: { width: 412, height: 915 },
    timeout: 240
  });
  await ptab.addInitScript('Object.defineProperty(navigator, "maxTouchPoints", { get: () => 5 });');
  await ptab.run(async ({ page }) => {
    await page.setViewport({ width: 412, height: 915, deviceScaleFactor: 2.625, isMobile: true, hasTouch: true });
  });
  await ptab.goto(`http://127.0.0.1:${port}/index.html?hour=${hour}&cb=fx28p${Date.now()}`);
  const deadline = Date.now() + 45000;
  while (Date.now() < deadline) {
    const ready = await ptab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await fx28Settle(400);
  }
  await fx28Settle(6000);
  rows.phone = await fx28State(ptab);
  await fx28Shot(ptab, `${dir}/iter-28-pixel6.png`);
  await ptab.close();
  return rows;
}

// Who is on screen in the four corner crops of the current pose:
//   await fx28Who(8731, { rotations: 2 })
// Decomposes the buildings and frames instance matrices, projects every volume's eight corners
// into the camera and reports the largest screen footprints that overlap each crop, with the
// world size, the distance, and how many window panes and frame quads stand within the volume's
// own footprint. A blank wall is then a row with a size, a distance and a zero, not a guess.
async function fx28Who(port, opts) {
  const o = opts || {};
  const tab = await fx28Tab(port, { name: 'fx28-who' });
  await tab.evaluate(`(() => { window.localtime.rig.setTier('street', true); })()`);
  await fx28Settle(4200);
  for (let r = 0; r < (o.rotations === undefined ? 2 : o.rotations); r += 1) {
    await tab.evaluate(`(() => { window.localtime.rig.rotate(1); })()`);
    await fx28Settle(1500);
  }
  const json = await tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L.world;
    const cam = L.rig.camera;
    const W = 1600, H = 900;
    const scratch = w.buildings.matrixWorld;
    const m = scratch.clone();
    const p = cam.position.clone();
    const q = cam.quaternion.clone();
    const s = cam.scale.clone();
    const eul = cam.rotation.clone();
    const crops = { TL: [0, 0, 533, 300], TR: [1066, 0, 1600, 300], BL: [0, 600, 533, 900], BR: [1066, 600, 1600, 900] };
    function proj(x, y, z) {
      p.set(x, y, z).project(cam);
      return [ (p.x * 0.5 + 0.5) * W, (0.5 - p.y * 0.5) * H, p.z ];
    }
    const bset = w.buildings;
    const el = bset.instanceMatrix.array;
    const count = bset.count;
    const vols = [];
    // pane positions, so a wall can be reported with the number of panes standing on it
    const panes = [];
    const wm = w.windows;
    if (wm) {
      const we = wm.instanceMatrix.array;
      for (let i = 0; i < wm.count; i += 1) panes.push([we[i * 16 + 12], we[i * 16 + 13], we[i * 16 + 14]]);
    }
    const fr = w.group.getObjectByName('frames');
    const fpos = [];
    if (fr) {
      const fe = fr.instanceMatrix.array;
      for (let i = 0; i < fr.count; i += 1) fpos.push([fe[i * 16 + 12], fe[i * 16 + 13], fe[i * 16 + 14], fe[i * 16 + 1]]);
    }
    for (let i = 0; i < count; i += 1) {
      m.fromArray(el, i * 16);
      m.decompose(p, q, s);
      const hx = Math.abs(s.x) * 0.5, hy = Math.abs(s.y) * 0.5, hz = Math.abs(s.z) * 0.5;
      const cx = p.x, cy = p.y, cz = p.z;
      eul.setFromQuaternion(q, 'YXZ');
      const yaw = eul.y;
      const ca = Math.cos(yaw), sa = Math.sin(yaw);
      let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity, deep = true;
      for (let k = 0; k < 8; k += 1) {
        const lx = (k & 1 ? hx : -hx), ly = (k & 2 ? hy : -hy), lz = (k & 4 ? hz : -hz);
        const wx = cx + lx * ca + lz * sa;
        const wz = cz - lx * sa + lz * ca;
        const r = proj(wx, cy + ly, wz);
        if (r[2] > 1) deep = false;
        if (r[0] < x0) x0 = r[0];
        if (r[0] > x1) x1 = r[0];
        if (r[1] < y0) y0 = r[1];
        if (r[1] > y1) y1 = r[1];
      }
      if (!deep) continue;
      let panesHere = 0;
      let framesHere = 0;
      for (let k = 0; k < panes.length; k += 1) {
        const dx = panes[k][0] - cx, dz = panes[k][2] - cz;
        if (dx * dx + dz * dz < Math.max(hx, hz) * Math.max(hx, hz) * 2.4) panesHere += 1;
      }
      for (let k = 0; k < fpos.length; k += 1) {
        const dx = fpos[k][0] - cx, dz = fpos[k][2] - cz;
        if (dx * dx + dz * dz < Math.max(hx, hz) * Math.max(hx, hz) * 2.4) framesHere += 1;
      }
      vols.push({
        i: i, x: Math.round(cx), z: Math.round(cz),
        w: Math.round(hx * 2 * 10) / 10, d: Math.round(hz * 2 * 10) / 10, h: Math.round(hy * 2 * 10) / 10,
        yaw: Math.round(yaw * 100) / 100,
        bx0: Math.round(x0), bx1: Math.round(x1), by0: Math.round(y0), by1: Math.round(y1),
        dist: Math.round(Math.hypot(cx - cam.position.x, cy - cam.position.y, cz - cam.position.z)),
        panes: panesHere, frames: framesHere
      });
    }
    const out = {};
    for (const key of Object.keys(crops)) {
      const c = crops[key];
      const list = [];
      for (const v of vols) {
        const ix0 = Math.max(c[0], v.bx0), ix1 = Math.min(c[2], v.bx1);
        const iy0 = Math.max(c[1], v.by0), iy1 = Math.min(c[3], v.by1);
        if (ix1 <= ix0 || iy1 <= iy0) continue;
        list.push({ area: (ix1 - ix0) * (iy1 - iy0), ...v });
      }
      list.sort((a, b) => b.area - a.area);
      out[key] = { cropPx: (c[2] - c[0]) * (c[3] - c[1]), covered: list.reduce((a, r) => a + r.area, 0), top: list.slice(0, 12) };
    }
    return JSON.stringify({ cam: cam.position.toArray().map((v) => Math.round(v * 10) / 10), buildingCount: count, crops: out });
  })()`);
  await tab.close();
  return JSON.parse(json);
}
