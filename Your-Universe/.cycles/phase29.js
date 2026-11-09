// Cycle 29: does a plain tier change pay the same first-use frame the first wake pays? The wake
// glide zooms to the block tier (dream.wake glides to the chantier centre, zoom to block), so a
// tier zoom with no dream in play is the clean control for "is this the dream or the camera".
//
//   await browser.close({ all: true });
//   const r = await phaseRec(8731, { url: '?hour=22', actions: [
//     { name: 'district', js: "window.localtime.rig.setTier('district', true)" },
//     { name: 'street',   js: "window.localtime.rig.setTier('street', true)" },
//     { name: 'block',    js: "window.localtime.rig.setTier('block', true)" },
//   ]});
//
const ph29 = (ms) => new Promise((r) => setTimeout(r, ms));

const PH29_REC = `(() => {
  if (window.__ph29) return true;
  const rec = { ms: [], js: [], tier: [], calls: [] };
  window.__ph29 = rec;
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
    rec.tier.push(L && L.rig ? L.rig.tier : '?');
    rec.calls.push(L && L.stats ? L.stats().calls : -1);
    orig(loop);
  };
  orig(loop);
  return true;
})()`;

async function phaseRec(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'ph29', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 240 });
  if (o.blockSw) {
    await tab.addInitScript(`(() => {
      try {
        Object.defineProperty(navigator, 'serviceWorker', {
          configurable: true,
          value: {
            register: function () { return Promise.reject(new Error('blocked by probe')); },
            getRegistrations: function () { return Promise.resolve([]); },
            addEventListener: function () {},
            removeEventListener: function () {},
            controller: null,
            ready: new Promise(function () {}),
          },
        });
      } catch (error) {}
    })()`);
  }
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html${o.url}&cb=ph29${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await ph29(400);
  }
  if (o.setup) await tab.evaluate(`(() => { ${o.setup}; return true; })()`);
  await ph29(o.settle === undefined ? 5000 : o.settle);
  await tab.evaluate(PH29_REC);
  await ph29(1000);
  const marks = [];
  for (const a of o.actions || []) {
    marks.push({ name: a.name, at: await tab.evaluate(`window.__ph29.ms.length`) });
    if (a.js) await tab.evaluate(`(() => { ${a.js}; return true; })()`);
    await ph29(a.wait === undefined ? 3000 : a.wait);
  }
  marks.push({ name: 'end', at: await tab.evaluate(`window.__ph29.ms.length`) });
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__ph29.ms, js: window.__ph29.js, tier: window.__ph29.tier, calls: window.__ph29.calls })`));
  await tab.close();
  const windows = [];
  for (let i = 0; i + 1 < marks.length; i += 1) {
    const a = marks[i];
    const b = marks[i + 1];
    const slice = rec.ms.slice(a.at, b.at).map((ms, k) => ({ ms, js: rec.js[a.at + k], tier: rec.tier[a.at + k], calls: rec.calls[a.at + k] }));
    slice.sort((x, y) => y.ms - x.ms);
    windows.push({ name: a.name, max: slice[0] ? slice[0].ms : null, top: slice.slice(0, 3), over33: slice.filter((f) => f.ms > 33).length, n: slice.length });
  }
  return { url: o.url, setup: o.setup || null, windows };
}
