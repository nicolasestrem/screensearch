// Cycle 27 prep: which structures stand on bare ground. Scratch tool, not part of the deliverable.
//
//   const f = await floatReport(8731);   // after %load ".cycles/float25.js"
//
// Method, all from the live page so it describes what is actually drawn:
//  - every building instance position from `world.buildings` (instanceMatrix),
//  - the road centrelines from `world.roads` (ring, avenue, spurs, expressway, rail, tram),
//  - each building's distance to the nearest centreline sample,
//  - a building is "fronted" when that distance is under FRONT_MAX metres, which is one
//    carriageway half-width plus a pavement plus a plot half-depth; everything beyond stands with
//    no street at its door,
//  - static merged structures cannot be enumerated per instance, so the report also lists the
//    civic/depot footprints the world publishes where it publishes them.
// The output is grouped into clusters 25 m apart so a lone box is named, not buried in a count.

const f25settle = (ms) => new Promise((r) => setTimeout(r, ms));

async function floatReport(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const FRONT_MAX = o.frontMax || 20;
  const tab = await browser.open({
    name: 'f25',
    url: 'about:blank',
    viewport: { width: 1600, height: 900 },
    timeout: 180,
  });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=17&cb=f25${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await f25settle(400);
  }
  await f25settle(4000);
  const json = await tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L && L.world;
    if (!w) return JSON.stringify({ error: 'no world' });
    const m = new DOMMatrix ? null : null;
    const b = w.buildings;
    const roadPts = [];
    const push = (curve, n) => {
      if (!curve || typeof curve.getPoint !== 'function') return;
      for (let i = 0; i <= n; i += 1) {
        const pt = curve.getPoint(i / n);
        roadPts.push([pt.x, pt.z]);
      }
    };
    const roads = w.roads || {};
    push(roads.ring, 220);
    push(roads.avenue, 120);
    push(roads.tram, 120);
    push(roads.rail, 80);
    push(roads.expressway, 80);
    if (roads.spurs) for (const s of roads.spurs) push(s, 90);

    const out = [];
    const el = b.instanceMatrix.array;
    for (let i = 0; i < b.count; i += 1) {
      const x = el[i * 16 + 12];
      const z = el[i * 16 + 14];
      let best = 1e9;
      for (let k = 0; k < roadPts.length; k += 1) {
        const dx = x - roadPts[k][0];
        const dz = z - roadPts[k][1];
        const d = dx * dx + dz * dz;
        if (d < best) best = d;
      }
      out.push([Math.round(x * 10) / 10, Math.round(z * 10) / 10, Math.round(Math.sqrt(best) * 10) / 10]);
    }
    return JSON.stringify({ frontMax: ${FRONT_MAX}, roadSamples: roadPts.length, buildings: out.length, d: out, roads: { get: Object.keys(roads) } });
  })()`);
  await tab.close();
  const data = JSON.parse(json);
  if (data.error) return data;
  const far = data.d.filter((r) => r[2] > FRONT_MAX);
  // Cluster the far set on a 25 m grid so one box is named, not a count.
  const cells = new Map();
  for (const r of far) {
    const cx = Math.round(r[0] / 25);
    const cz = Math.round(r[1] / 25);
    const key = cx + ',' + cz;
    const rec = cells.get(key) || { cx: cx * 25, cz: cz * 25, n: 0, maxD: 0, members: [] };
    rec.n += 1;
    rec.maxD = Math.max(rec.maxD, r[2]);
    if (rec.members.length < 6) rec.members.push({ x: r[0], z: r[1], d: r[2] });
    cells.set(key, rec);
  }
  const clusters = Array.from(cells.values()).sort((a, b) => b.n - a.n);
  return {
    buildings: data.buildings,
    frontMax: data.frontMax,
    far: far.length,
    farShare: Math.round((far.length / data.buildings) * 1000) / 10,
    clusters: clusters.slice(0, 20),
    roadKeys: data.roads.get,
  };
}

// The published structures projected to the frame, for `.cycles/isolate27.py`.
//   await write('Your-Universe/screenshots/iter-27-boxes.json', JSON.stringify(await structureScreen(8731)));
async function structureScreen(port) {
  const p = port || 8731;
  const tab = await browser.open({ name: 'ss25', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 180 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=17&cb=ss25${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await f25settle(400);
  }
  await f25settle(4500);
  const json = await tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L && L.world;
    const rig = L && L.rig;
    if (!w || !rig) return JSON.stringify({ error: 'no world or rig' });
    const cam = rig.camera;
    const H = innerHeight;
    const W = innerWidth;
    const v = cam.position.clone();
    function toScreen(x, z) {
      const y = w.groundHeightAt(x, z);
      v.set(x, y, z);
      v.project(cam);
      return [(v.x * 0.5 + 0.5) * W, (-v.y * 0.5 + 0.5) * H];
    }
    const recs = [];
    const civ = (w.fabric && w.fabric.civic) || [];
    for (const c of civ) {
      const a = toScreen(c.x - 9, c.z);
      const b = toScreen(c.x + 9, c.z);
      const d = toScreen(c.x, c.z - 9);
      const e = toScreen(c.x, c.z + 9);
      const ctr = toScreen(c.x, c.z);
      recs.push({ type: c.type, via: c.via, wx: c.x, wz: c.z,
        x: Math.round(ctr[0]), y: Math.round(ctr[1]),
        hw: Math.max(6, Math.abs(b[0] - a[0]) / 2), hh: Math.max(6, Math.abs(e[1] - d[1]) / 2) });
    }
    // The rail depot shed stands at RAIL_X + 11.5, z -78 in the source; probe it as published
    // structure the world can place, using the rail curve to find RAIL_X on this build.
    const rail = w.roads && w.roads.rail;
    if (rail) {
      const pt = rail.getPoint(0.5);
      const dx = 11.5;
      const ctr = toScreen(pt.x + dx, -78);
      recs.push({ type: 'depot', via: 'rail', wx: Math.round(pt.x + dx), wz: -78,
        x: Math.round(ctr[0]), y: Math.round(ctr[1]), hw: 10, hh: 10 });
    }
    return JSON.stringify(recs);
  })()`);
  await tab.close();
  return JSON.parse(json);
}
