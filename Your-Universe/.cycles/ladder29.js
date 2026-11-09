// Cycle 29: correlate slow frames with the quality ladder. The ladder's notch is read every
// frame; a stage change reallocates the post target and swaps the pixel ratio, which is a
// one-off GPU allocation, not a per-frame cost. Scratch tool.
//
//   await browser.close({ all: true });
//   const r = await ladderTrace(8731, { url: '?hour=22', seconds: 20 });
//
const ld29 = (ms) => new Promise((r) => setTimeout(r, ms));

const LD29_REC = `(() => {
  if (window.__ld29) return true;
  const rec = { ms: [], q: [], calls: [], tris: [] };
  window.__ld29 = rec;
  const orig = window.requestAnimationFrame.bind(window);
  let last = performance.now();
  const loop = () => {
    const now = performance.now();
    rec.ms.push(Math.round((now - last) * 100) / 100);
    last = now;
    const L = window.localtime;
    const q = L && L.quality ? L.quality() : null;
    rec.q.push(q ? [q.notch, q.pixelRatio, q.population, q.postScale].join('/') : '?');
    const s = L && L.stats ? L.stats() : null;
    rec.calls.push(s ? s.calls : -1);
    rec.tris.push(s ? s.triangles : -1);
    orig(loop);
  };
  orig(loop);
  return true;
})()`;

async function ladderTrace(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'ld29', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 240 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html${o.url}&cb=ld29${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await ld29(400);
  }
  await tab.evaluate(LD29_REC);
  await ld29(o.seconds === undefined ? 20000 : o.seconds * 1000);
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify(window.__ld29)`));
  await tab.close();
  const slow = [];
  for (let i = 0; i < rec.ms.length; i += 1) {
    if (rec.ms[i] > 20) {
      slow.push({ i, ms: rec.ms[i], q: rec.q[i], qPrev: i ? rec.q[i - 1] : null, calls: rec.calls[i], tris: rec.tris[i] });
    }
  }
  const changes = [];
  for (let i = 1; i < rec.q.length; i += 1) {
    if (rec.q[i] !== rec.q[i - 1]) changes.push({ i, from: rec.q[i - 1], to: rec.q[i], ms: rec.ms[i] });
  }
  return { url: o.url, frames: rec.ms.length, first: rec.q[0], last: rec.q[rec.q.length - 1], slow, changes };
}
