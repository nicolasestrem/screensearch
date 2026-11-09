// Frame-time recorder that does not touch the dream: open a state, record 15 s, report the worst
// frames. Scratch tool.
//
//   await frameProfile(8731, { url: '?hour=17', name: 'awake' });
//   await frameProfile(8731, { url: '?hour=22', sleep: true, name: 'dream' });

const fp25settle = (ms) => new Promise((r) => setTimeout(r, ms));

const FP25_REC = `(() => {
  if (window.__fp) return true;
  const rec = { ms: [], calls: [], tris: [] };
  window.__fp = rec;
  let last = performance.now();
  const loop = () => {
    const now = performance.now();
    rec.ms.push(Math.round((now - last) * 100) / 100);
    last = now;
    const L = window.localtime;
    const s = L && L.stats ? L.stats() : null;
    rec.calls.push(s ? s.calls : -1);
    rec.tris.push(s ? s.triangles : -1);
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
  return true;
})()`;

async function frameProfile(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: o.name || 'fp25', url: 'about:blank', viewport: { width: o.width || 1600, height: o.height || 900 }, timeout: 180 });
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: v.dpr || 1, isMobile: !!v.touch, hasTouch: !!v.touch });
  }, { args: [{ width: o.width || 1600, height: o.height || 900, dpr: o.dpr || 1, touch: !!o.touch }] });
  await tab.goto(`http://127.0.0.1:${p}/index.html${o.url || '?hour=17'}&cb=fp25${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await fp25settle(400);
  }
  await fp25settle(5000);
  await tab.evaluate(FP25_REC);
  if (o.sleep) {
    await tab.click('#sleep');
    await fp25settle(3000);
    await tab.evaluate(`window.__fp.ms.length = 0; window.__fp.calls.length = 0; window.__fp.tris.length = 0;`);
  }
  await tab.evaluate(`window.__fp.ms.length = 0; window.__fp.calls.length = 0; window.__fp.tris.length = 0;`);
  await fp25settle(o.seconds ? o.seconds * 1000 : 15000);
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__fp.ms, calls: window.__fp.calls, tris: window.__fp.tris })`));
  await tab.close();
  const ms = rec.ms;
  const sorted = ms.map((v, i) => ({ v, i })).sort((a, b) => b.v - a.v).slice(0, 8);
  const mean = ms.reduce((a, b) => a + b, 0) / ms.length;
  return {
    name: o.name || (o.sleep ? 'dream' : 'awake'),
    frames: ms.length,
    meanMs: Math.round(mean * 100) / 100,
    maxMs: Math.max(...ms),
    worst: sorted.map((r) => ({ ms: r.v, calls: rec.calls[r.i], tris: rec.tris[r.i] })),
    overTen: ms.filter((v) => v > 10).length,
  };
}
