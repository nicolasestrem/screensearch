// Cycle 28 probe: does each arterial spur carry a continuous building frontage, both sides, on
// the on-screen stretch? Scratch tool, not part of the deliverable.
//
//   const f = await spurFrontage(8731);   // after %load ".cycles/spurfront.js"
//
// For every building instance it finds the nearest sample on each spur path, so each building is
// attributed to (spur, arc length, lateral offset, side). Then, per 10 m bin of on-screen path, it
// reports the number of buildings on each side within FRONT_MAX metres of the centreline. A street
// that "has buildings on both sides" has a non-zero count on both sides in every bin.

const sf25settle = (ms) => new Promise((r) => setTimeout(r, ms));

async function spurFrontage(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const FRONT = o.frontMax || 26;
  const BIN = o.bin || 10;
  const tab = await browser.open({ name: 'sf25', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 180 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=17&cb=sf25${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await sf25settle(400);
  }
  await sf25settle(4500);
  const json = await tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L && L.world;
    const rig = L && L.rig;
    if (!w || !rig) return JSON.stringify({ error: 'no world' });
    const cam = rig.camera;
    const W = innerWidth;
    const H = innerHeight;
    const v = cam.position.clone();
    function visible(x, z) {
      v.set(x, w.groundHeightAt(x, z), z);
      v.project(cam);
      return v.x > -1.15 && v.x < 1.15 && v.y > -1.15 && v.y < 1.15 && v.z < 1;
    }
    // Sample every spur into a dense polyline once.
    const spurs = (w.roads && w.roads.spurs) || [];
    const lines = [];
    for (let s = 0; s < spurs.length; s += 1) {
      const n = 220;
      const xs = [];
      const zs = [];
      for (let i = 0; i <= n; i += 1) {
        const pt = spurs[s].getPoint(i / n);
        xs.push(pt.x);
        zs.push(pt.z);
      }
      let len = 0;
      const acc = [0];
      for (let i = 1; i <= n; i += 1) {
        len += Math.hypot(xs[i] - xs[i - 1], zs[i] - zs[i - 1]);
        acc.push(len);
      }
      lines.push({ xs, zs, acc, len, n });
    }
    const b = w.buildings;
    const el = b.instanceMatrix.array;
    const bins = [];
    for (let s = 0; s < lines.length; s += 1) {
      const L2 = lines[s];
      const nb = Math.floor(L2.len / ${BIN}) + 1;
      const left = new Int32Array(nb);
      const right = new Int32Array(nb);
      const onscreen = new Uint8Array(nb);
      const cx = new Float32Array(nb);
      const cz = new Float32Array(nb);
      for (let i = 0; i < nb; i += 1) {
        const k = Math.min(L2.n, Math.round((i + 0.5) * ${BIN} / (L2.len / L2.n)));
        cx[i] = L2.xs[k];
        cz[i] = L2.zs[k];
        onscreen[i] = visible(cx[i], cz[i]) ? 1 : 0;
      }
      for (let i = 0; i < b.count; i += 1) {
        const x = el[i * 16 + 12];
        const z = el[i * 16 + 14];
        let best = 1e9;
        let bestK = -1;
        for (let k = 0; k <= L2.n; k += 1) {
          const dx = x - L2.xs[k];
          const dz = z - L2.zs[k];
          const d = dx * dx + dz * dz;
          if (d < best) { best = d; bestK = k; }
        }
        const dist = Math.sqrt(best);
        if (dist > ${FRONT} || bestK < 0) continue;
        // Side: the cross product of the path tangent with the offset.
        const k0 = Math.max(0, bestK - 1);
        const k1 = Math.min(L2.n, bestK + 1);
        const tx = L2.xs[k1] - L2.xs[k0];
        const tz = L2.zs[k1] - L2.zs[k0];
        const ox = x - L2.xs[bestK];
        const oz = z - L2.zs[bestK];
        const cross = tx * oz - tz * ox;
        const bin = Math.min(nb - 1, Math.floor(L2.acc[bestK] / ${BIN}));
        if (cross >= 0) left[bin] += 1; else right[bin] += 1;
      }
      const rows = [];
      for (let i = 0; i < nb; i += 1) {
        if (!onscreen[i]) continue;
        rows.push({ bin: i, at: Math.round((i + 0.5) * ${BIN}), cx: Math.round(cx[i]), cz: Math.round(cz[i]), left: left[i], right: right[i] });
      }
      bins.push({ spur: s, lengthM: Math.round(L2.len), onScreenBins: rows.length, rows });
    }
    return JSON.stringify(bins);
  })()`);
  await tab.close();
  const data = JSON.parse(json);
  if (data.error) return data;
  // Summarise: bins with nothing on either side, and the last bin that has both sides.
  return data.map((sp) => {
    const rows = sp.rows;
    const gaps = rows.filter((r) => r.left === 0 && r.right === 0).map((r) => r.at);
    const oneSided = rows.filter((r) => (r.left === 0) !== (r.right === 0)).map((r) => r.at);
    const lastBoth = rows.filter((r) => r.left > 0 && r.right > 0).map((r) => r.at).pop();
    const totals = rows.reduce((a, r) => ({ left: a.left + r.left, right: a.right + r.right }), { left: 0, right: 0 });
    return {
      spur: sp.spur,
      lengthM: sp.lengthM,
      onScreenBins: sp.onScreenBins,
      buildings: totals,
      emptyBins: gaps.length,
      emptyAt: gaps.slice(0, 12),
      oneSidedBins: oneSided.length,
      lastBinWithBothSides: lastBoth === undefined ? null : lastBoth,
    };
  });
}

// The ring's own frontage, measured the same way: buildings per 10 m bin on each side of the
// ring road, outer vs inner. Cycle 28 cut the ring's frontage lots to pay for the crossings; this
// is the probe that says whether the ring still reads built-up.
//   const r = await ringFront(8731);
async function ringFront(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const FRONT = o.frontMax || 26;
  const BIN = o.bin || 10;
  const tab = await browser.open({ name: 'rf25', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 180 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=17&cb=rf25${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await sf25settle(400);
  }
  await sf25settle(4500);
  const json = await tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L && L.world;
    const rig = L && L.rig;
    if (!w || !rig) return JSON.stringify({ error: 'no world' });
    const ring = w.roads && w.roads.ring;
    if (!ring) return JSON.stringify({ error: 'no ring' });
    const n = 260;
    const xs = [], zs = [];
    for (let i = 0; i <= n; i += 1) { const pt = ring.getPoint(i / n); xs.push(pt.x); zs.push(pt.z); }
    const acc = [0];
    for (let i = 1; i <= n; i += 1) acc.push(acc[i - 1] + Math.hypot(xs[i] - xs[i - 1], zs[i] - zs[i - 1]));
    const len = acc[n];
    const nb = Math.floor(len / ${BIN}) + 1;
    const outer = new Int32Array(nb), inner = new Int32Array(nb);
    const b = w.buildings;
    const el = b.instanceMatrix.array;
    for (let i = 0; i < b.count; i += 1) {
      const x = el[i * 16 + 12], z = el[i * 16 + 14];
      let best = 1e9, bk = -1;
      for (let k = 0; k <= n; k += 1) {
        const dx = x - xs[k], dz = z - zs[k];
        const d = dx * dx + dz * dz;
        if (d < best) { best = d; bk = k; }
      }
      const dist = Math.sqrt(best);
      if (dist > ${FRONT} || bk < 0) continue;
      const k0 = Math.max(0, bk - 1), k1 = Math.min(n, bk + 1);
      const tx = xs[k1] - xs[k0], tz = zs[k1] - zs[k0];
      const cross = tx * (z - zs[bk]) - tz * (x - xs[bk]);
      const bin = Math.min(nb - 1, Math.floor(acc[bk] / ${BIN}));
      // Ring centre is RING_CZ in source; outer is the side away from the district centre.
      if (cross >= 0) outer[bin] += 1; else inner[bin] += 1;
    }
    const rows = [];
    for (let i = 0; i < nb; i += 1) rows.push({ at: Math.round((i + 0.5) * ${BIN}), outer: outer[i], inner: inner[i] });
    return JSON.stringify({ lengthM: Math.round(len), bins: nb,
      outer: rows.reduce((a, r) => a + r.outer, 0), inner: rows.reduce((a, r) => a + r.inner, 0),
      emptyOuter: rows.filter((r) => r.outer === 0).length, rows });
  })()`);
  await tab.close();
  const d = JSON.parse(json);
  return d;
}
