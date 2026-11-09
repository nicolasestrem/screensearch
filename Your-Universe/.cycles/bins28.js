// Cycle 28 probe: the per-bin frontage census of spurFrontage.js, printed row by row for one
// arterial, so an empty or one-sided bin can be named with its coordinates and blocked by
// geometry. Scratch tool, not part of the deliverable.
//
//   const rows = await binsFor(8731, 3);   // after %load ".cycles/bins28.js"

const b28settle = (ms) => new Promise((r) => setTimeout(r, ms));

async function binsFor(port, spur, opts) {
  const o = opts || {};
  const p = port || 8731;
  const FRONT = o.frontMax || 26;
  const BIN = o.bin || 10;
  const tab = await browser.open({ name: 'b28', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 180 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=17&cb=b28${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await b28settle(400);
  }
  await b28settle(4200);
  const json = await tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L && L.world;
    const rig = L && L.rig;
    if (!w || !rig) return JSON.stringify({ error: 'no world' });
    const cam = rig.camera;
    const v = cam.position.clone();
    function visible(x, z) {
      v.set(x, w.groundHeightAt(x, z), z);
      v.project(cam);
      return v.x > -1.15 && v.x < 1.15 && v.y > -1.15 && v.y < 1.15 && v.z < 1;
    }
    const sp = w.roads.spurs[${spur}];
    const n = 260;
    const xs = [], zs = [];
    for (let i = 0; i <= n; i += 1) { const pt = sp.getPoint(i / n); xs.push(pt.x); zs.push(pt.z); }
    let len = 0; const acc = [0];
    for (let i = 1; i <= n; i += 1) { len += Math.hypot(xs[i] - xs[i - 1], zs[i] - zs[i - 1]); acc.push(len); }
    const nb = Math.floor(len / ${BIN}) + 1;
    const left = new Int32Array(nb), right = new Int32Array(nb);
    const on = new Uint8Array(nb);
    for (let i = 0; i < nb; i += 1) {
      const k = Math.min(n, Math.round((i + 0.5) * ${BIN} / (len / n)));
      on[i] = visible(xs[k], zs[k]) ? 1 : 0;
    }
    const b = w.buildings;
    const el = b.instanceMatrix.array;
    for (let i = 0; i < b.count; i += 1) {
      const x = el[i * 16 + 12], z = el[i * 16 + 14];
      let best = 1e9, bestK = -1;
      for (let k = 0; k <= n; k += 1) {
        const dx = x - xs[k], dz = z - zs[k];
        const d = dx * dx + dz * dz;
        if (d < best) { best = d; bestK = k; }
      }
      if (Math.sqrt(best) > ${FRONT} || bestK < 0) continue;
      const k0 = Math.max(0, bestK - 1), k1 = Math.min(n, bestK + 1);
      const tx = xs[k1] - xs[k0], tz = zs[k1] - zs[k0];
      const ox = x - xs[bestK], oz = z - zs[bestK];
      const cross = tx * oz - tz * ox;
      const bin = Math.min(nb - 1, Math.floor(acc[bestK] / ${BIN}));
      if (cross >= 0) left[bin] += 1; else right[bin] += 1;
    }
    const rows = [];
    for (let i = 0; i < nb; i += 1) {
      if (!on[i]) continue;
      const k = Math.min(n, Math.round((i + 0.5) * ${BIN} / (len / n)));
      rows.push([i, Math.round((i + 0.5) * ${BIN}), Math.round(xs[k]), Math.round(zs[k]), left[i], right[i]]);
    }
    return JSON.stringify({ len: Math.round(len), rows });
  })()`);
  await tab.close();
  return JSON.parse(json);
}
