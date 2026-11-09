// Cycle 29: when does the offline cache install, and which frame pays for it? Clears the cache
// and unregisters the worker, reloads with the recorder installed before the page's own JS, then
// samples the frame clock, visibility, the cache entry count and the controller every 250 ms.
// Scratch tool.
//
//   await browser.close({ all: true });
//   const t = await swTimeline(8731, { seconds: 24, deferMs: 0 });
//
const sw29 = (ms) => new Promise((r) => setTimeout(r, ms));

const SW29_REC = `(() => {
  if (window.__sw29) return true;
  const rec = { ms: [], js: [], vis: [] };
  window.__sw29 = rec;
  const orig = window.requestAnimationFrame.bind(window);
  let last = performance.now();
  window.requestAnimationFrame = function (cb) {
    return orig(function (ts) {
      const a = performance.now();
      try { return cb(ts); } finally { rec.js.push(Math.round((performance.now() - a) * 100) / 100); }
    });
  };
  const loop = () => {
    const now = performance.now();
    rec.ms.push(Math.round((now - last) * 100) / 100);
    last = now;
    rec.vis.push(document.visibilityState);
    orig(loop);
  };
  orig(loop);
  return true;
})()`;

async function swTimeline(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'sw29', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 240 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  // 1. Clear the offline cache and unregister, on the page's own origin.
  await tab.goto(`http://127.0.0.1:${p}/index.html?cb=sw29clear${Date.now()}`);
  await sw29(3000);
  const cleared = await tab.evaluate(`(async () => {
    const regs = await navigator.serviceWorker.getRegistrations();
    for (const r of regs) await r.unregister();
    const keys = await caches.keys();
    for (const k of keys) await caches.delete(k);
    return JSON.stringify({ regs: regs.length, keys });
  })()`);
  // 2. Reload with the recorder installed before the page's own scripts.
  await tab.addInitScript(SW29_REC);
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=22&cb=sw29b${Date.now()}`);
  const t0 = Date.now();
  const samples = [];
  const deadline = Date.now() + (o.seconds === undefined ? 24 : o.seconds) * 1000;
  while (Date.now() < deadline) {
    const s = await tab.evaluate(`(async () => {
      const keys = await caches.keys();
      let entries = 0;
      for (const k of keys) { const c = await caches.open(k); entries += (await c.keys()).length; }
      const L = window.localtime;
      return JSON.stringify({
        at: Math.round(performance.now()),
        frames: window.__sw29 ? window.__sw29.ms.length : 0,
        vis: document.visibilityState,
        entries, keys, controller: !!navigator.serviceWorker.controller,
        dream: L && L.state ? L.state().dream : '?',
      });
    })()`);
    samples.push(JSON.parse(s));
    await sw29(250);
  }
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__sw29.ms, js: window.__sw29.js, vis: window.__sw29.vis })`));
  await tab.close();
  const slow = [];
  for (let i = 0; i < rec.ms.length; i += 1) {
    if (rec.ms[i] > 25) slow.push({ i, ms: rec.ms[i], js: rec.js[i], vis: rec.vis[i] });
  }
  return {
    cleared,
    frames: rec.ms.length,
    maxMs: Math.max(...rec.ms),
    slow,
    timeline: samples.filter((s, i) => i === 0 || s.entries !== samples[i - 1].entries || s.controller !== samples[i - 1].controller || i % 8 === 0),
  };
}
