// Cycle 29: bisect the first-wake GPU stall across the page's own switches. One fresh tab per
// configuration, browser closed to a single tab first, one sleep and one wake each, recorder
// only (no tracing: the GPU trace categories distort SwiftShader by a factor of hundreds).
//
//   await browser.close({ all: true });
//   const b = await wakeBisect(8731);
//
const w29bs = (ms) => new Promise((r) => setTimeout(r, ms));

const W29B_REC = `(() => {
  if (window.__w29b) return true;
  const rec = { ms: [], js: [], dream: [], calls: [] };
  window.__w29b = rec;
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
    const L = window.localtime;
    rec.dream.push(L && L.state ? L.state().dream : '?');
    rec.calls.push(L && L.stats ? L.stats().calls : -1);
    orig(loop);
  };
  orig(loop);
  return true;
})()`;

async function wakeOne(port, cfg) {
  const p = port || 8731;
  const tab = await browser.open({ name: 'w29b', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 240 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html${cfg.url}&cb=w29b${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await w29bs(400);
  }
  if (cfg.setup) await tab.evaluate(`(() => { ${cfg.setup}; return true; })()`);
  await w29bs(4000);
  await tab.evaluate(W29B_REC);
  await w29bs(1000);
  const before = await tab.evaluate(`window.__w29b.ms.length`);
  await tab.click('#sleep');
  await w29bs(6000);
  const mid = await tab.evaluate(`window.__w29b.ms.length`);
  const t0 = Date.now();
  await tab.click('#sleep');
  const clickMs = Date.now() - t0;
  await w29bs(8000);
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__w29b.ms, js: window.__w29b.js, dream: window.__w29b.dream, calls: window.__w29b.calls })`));
  await tab.close();
  const wake = rec.ms.slice(mid).map((ms, i) => ({ ms, js: rec.js[mid + i], dream: rec.dream[mid + i], calls: rec.calls[mid + i] }));
  wake.sort((a, b) => b.ms - a.ms);
  return {
    name: cfg.name,
    url: cfg.url,
    setup: cfg.setup || null,
    clickMs,
    wakeMax: wake[0] ? wake[0].ms : null,
    wakeTop: wake.slice(0, 3),
    over33: wake.filter((f) => f.ms > 33).length,
  };
}

async function wakeBisect(port, cfgs) {
  const list = cfgs || [
    { name: 'baseline', url: '?hour=22' },
    { name: 'post off', url: '?hour=22&post=0' },
    { name: 'noassets', url: '?hour=22&noassets=1' },
    { name: 'shadow map off', url: '?hour=22', setup: 'window.localtime.world.lights.setShadowMapSize(0)' },
    { name: 'cohort off', url: '?hour=22', setup: 'window.localtimeCohort.setEnabled(false)' },
    { name: 'day wall hour', url: '?hour=13' },
  ];
  const rows = [];
  for (const cfg of list) {
    await browser.close({ all: true });
    rows.push(await wakeOne(port, cfg));
  }
  return rows;
}
