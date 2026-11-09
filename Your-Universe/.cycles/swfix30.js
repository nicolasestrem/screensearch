// Cycle 30: what the offline cache writes on a first visit, what the interactive frames pay for
// it, and how the cache grows when the page is navigated with new query strings. Scratch tool.
//
//   await browser.close({ all: true });
//   const a = await swFirstVisit(8731, { seconds: 22 });   // cold: clear cache + unregister first
//   const b = await swNavGrowth(8731, { n: 6 });           // same profile, ?cb= per navigation
//   const c = await swOffline(8731);                       // network off, reload, real assets?
//
// Frame recorder is installed before the page's own scripts: it wraps requestAnimationFrame in
// the window, which is the context three's WebGLAnimation calls, so every frame of the page's
// render loop is timed with its JS time. The cache sampler runs in the page on the same clock, at
// 250 ms, and only reads key metadata (never bodies).

const s30 = (ms) => new Promise((r) => setTimeout(r, ms));
const BASE30 = (port) => `http://127.0.0.1:${port || 8731}/index.html`;

const REC30 = `(() => {
  if (window.__s30rec) return true;
  const rec = { t: [], ms: [], js: [], tl: [], ctrl: [], probes: [] };
  window.__s30rec = rec;
  try {
    navigator.serviceWorker.addEventListener('message', function (e) {
      const d = e && e.data;
      if (d && d.probe === 's30') rec.probes.push({ phase: d.phase, url: d.url, at: d.at, wall: d.wall, seen: Math.round(performance.now()) });
    });
  } catch (err) {}
  const orig = window.requestAnimationFrame.bind(window);
  let last = performance.now();
  window.requestAnimationFrame = function (cb) {
    return orig(function (ts) {
      const a = performance.now();
      try { return cb(ts); } finally {
        const now = performance.now();
        rec.t.push(Math.round(now));
        rec.ms.push(Math.round((now - last) * 10) / 10);
        rec.js.push(Math.round((now - a) * 10) / 10);
        last = now;
      }
    });
  };
  const poll = async () => {
    try {
      const keys = await caches.keys();
      let entries = 0;
      const each = {};
      for (const k of keys) {
        const c = await caches.open(k);
        const n = (await c.keys()).length;
        each[k] = n;
        entries += n;
      }
      rec.tl.push({ t: Math.round(performance.now()), entries, each, ctrl: !!navigator.serviceWorker.controller });
    } catch (e) { rec.tl.push({ t: Math.round(performance.now()), err: String(e && e.message || e) }); }
  };
  poll();
  setInterval(poll, 250);
  return true;
})()`;

const CACHE_DUMP = `(async () => {
  const keys = await caches.keys();
  const out = { caches: keys, entries: [], bytes: {} };
  for (const k of keys) {
    const c = await caches.open(k);
    const reqs = await c.keys();
    let sum = 0;
    for (const r of reqs) {
      const res = await c.match(r);
      let size = 0;
      try { size = (await res.clone().arrayBuffer()).byteLength; } catch (e) { size = -1; }
      sum += size;
      out.entries.push({ cache: k, url: r.url, size });
    }
    out.bytes[k] = { count: reqs.length, bytes: sum };
  }
  const est = await navigator.storage.estimate();
  out.estimate = { usage: est.usage, quota: est.quota };
  return JSON.stringify(out);
})()`;

async function openFresh30(name, width, height, dpr, touch) {
  const tab = await browser.open({ name, url: 'about:blank', viewport: { width, height }, timeout: 240 });
  if (touch) await tab.addInitScript('Object.defineProperty(navigator, "maxTouchPoints", { get: () => 5 });');
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: v.dpr, isMobile: v.touch, hasTouch: v.touch });
  }, { args: [{ width, height, dpr, touch }] });
  return tab;
}

async function clearOffline30(tab, port) {
  await tab.goto(`${BASE30(port)}?cb=s30clear${Date.now()}`);
  await s30(2500);
  return await tab.evaluate(`(async () => {
    const regs = await navigator.serviceWorker.getRegistrations();
    for (const r of regs) await r.unregister();
    const keys = await caches.keys();
    for (const k of keys) await caches.delete(k);
    const after = await caches.keys();
    return JSON.stringify({ unregistered: regs.length, deleted: keys.length, left: after });
  })()`);
}

function summarize30(rec, from) {
  const idx = [];
  for (let i = 0; i < rec.ms.length; i += 1) if (rec.t[i] >= (from || 0)) idx.push(i);
  const ms = idx.map((i) => rec.ms[i]);
  const js = idx.map((i) => rec.js[i]);
  const sorted = ms.slice().sort((a, b) => a - b);
  const q = (p) => sorted[Math.min(sorted.length - 1, Math.max(0, Math.floor((p / 100) * sorted.length)))];
  const slow = idx.filter((i) => rec.ms[i] > 33).map((i) => ({ t: rec.t[i], ms: rec.ms[i], js: rec.js[i] }));
  const worst = idx.slice().sort((a, b) => rec.ms[b] - rec.ms[a]).slice(0, 6).map((i) => ({ t: rec.t[i], ms: rec.ms[i], js: rec.js[i] }));
  return {
    frames: ms.length,
    maxMs: ms.length ? Math.max(...ms) : 0,
    p1: sorted.length ? q(1) : 0,
    p50: sorted.length ? q(50) : 0,
    over33: slow.length,
    slow: slow.slice(0, 12),
    worst,
    jsMax: js.length ? Math.max(...js) : 0,
  };
}

async function swFirstVisit(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await openFresh30('s30-first', o.width || 1600, o.height || 900, o.dpr || 1, !!o.touch);
  const cleared = await clearOffline30(tab, p);
  await tab.addInitScript(REC30);
  const navAt = Date.now();
  await tab.goto(`${BASE30(p)}?hour=22&cb=s30a${navAt}`);
  await s30((o.seconds || 22) * 1000);
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify(window.__s30rec)`));
  const stats = JSON.parse(await tab.evaluate(`(() => { const L = window.localtime; const s = L && L.stats ? L.stats() : null; return JSON.stringify({ stats: s, assets: L && L.assets ? L.assets : null, ready: !!(L && L.state && L.state().firstFrameAt) }); })()`));
  const dump = JSON.parse(await tab.evaluate(CACHE_DUMP));
  await tab.close();
  return { cleared, seconds: o.seconds || 22, stats, frames: summarize30(rec),
    afterWarmup: summarize30(rec, 2000), timeline: rec.tl, probes: rec.probes, dump };
}

async function swNavGrowth(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await openFresh30('s30-nav', 1600, 900, 1, false);
  await tab.goto(`${BASE30(p)}?cb=s30nav0${Date.now()}`);
  await s30(6000);
  const steps = [];
  for (let i = 1; i <= (o.n || 6); i += 1) {
    await tab.goto(`${BASE30(p)}?hour=${10 + i}&cb=s30nav${i}-${Date.now()}`);
    await s30(o.settle || 3000);
    const counts = JSON.parse(await tab.evaluate(`(async () => {
      const keys = await caches.keys();
      const each = {}; let entries = 0;
      for (const k of keys) { const c = await caches.open(k); const n = (await c.keys()).length; each[k] = n; entries += n; }
      const est = await navigator.storage.estimate();
      return JSON.stringify({ entries, each, ctrl: !!navigator.serviceWorker.controller, usage: est.usage });
    })()`));
    steps.push({ i, ...counts });
  }
  const dump = JSON.parse(await tab.evaluate(CACHE_DUMP));
  await tab.close();
  return { steps, dump };
}

async function swOffline(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await openFresh30('s30-off', o.width || 1600, o.height || 900, o.dpr || 1, !!o.touch);
  await tab.emulate({ offline: true });
  await tab.addInitScript(REC30);
  await tab.goto(`${BASE30(p)}?hour=22&cb=s30off${Date.now()}`);
  await s30((o.seconds || 9) * 1000);
  const info = JSON.parse(await tab.evaluate(`(() => {
    const L = window.localtime;
    return JSON.stringify({
      ready: !!(L && L.state && L.state().firstFrameAt),
      stats: L && L.stats ? L.stats() : null,
      assets: L && L.assets ? L.assets : null,
      tier: L && L.rig ? L.rig.tier : null,
      banners: (document.querySelector('body').className || ''),
      warns: (window.__s30rec ? null : null)
    });
  })()`));
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify(window.__s30rec)`));
  await tab.emulate({ offline: false });
  await tab.close();
  return { info, frames: summarize30(rec) };
}

globalThis.swFirstVisit = swFirstVisit;
globalThis.swNavGrowth = swNavGrowth;
globalThis.swOffline = swOffline;
globalThis.swStubReport = null;
console.log('swfix30 loaded');

// ---- write-cost A/B: is it the burst of Cache Storage writes, or the volume? ----
// Runs in one page, windows interleaved (burst / paced / idle) so machine-wide noise hits every
// arm equally. The bytes are prefetched once, so a window measures cache.put cost only.
const WINDOW_MS = 4000;
const PACED_GAP_MS = 700;

async function writeCost30(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await openFresh30('s30-wc', 1600, 900, 1, false);
  await tab.addInitScript(REC30);
  await tab.goto(`${BASE30(p)}?hour=22&cb=wc${Date.now()}`);
  await s30(11000);
  const urls = JSON.parse(await tab.evaluate(`(async () => {
    const m = await (await fetch('assets/manifest.json', { cache: 'no-store' })).json();
    const packs = m.packs.map(p => new URL(p.file, location.href).href);
    return JSON.stringify([
      new URL('index.html', location.href).href,
      new URL('sample-export.json', location.href).href,
      new URL('assets/manifest.json', location.href).href
    ].concat(packs));
  })()`));
  const arms = [];
  const plan = [];
  for (let i = 0; i < (o.rounds || 3); i += 1) plan.push('burst', 'paced', 'idle');
  for (const arm of plan) {
    const t0 = Date.now();
    const stats = JSON.parse(await tab.evaluate(`(async () => {
      const arm = ${JSON.stringify(arm)};
      const urls = ${JSON.stringify(urls)};
      const cache = await caches.open('s30-write-probe');
      const bufs = [];
      for (let i = 0; i < urls.length; i += 1) {
        const hit = await fetch(urls[i], { cache: 'force-cache' });
        bufs.push(await hit.arrayBuffer());
      }
      const t = performance.now();
      const writeOne = async (i) => { await cache.put(urls[i], new Response(bufs[i].slice(0))); };
      if (arm === 'burst') {
        for (let i = 0; i < urls.length; i += 1) cache.put(urls[i], new Response(bufs[i].slice(0)));  // fire together
        await new Promise((r) => setTimeout(r, 0));
      } else if (arm === 'paced') {
        for (let i = 0; i < urls.length; i += 1) { await writeOne(i); if (i < urls.length - 1) await new Promise((r) => setTimeout(r, ${PACED_GAP_MS})); }
      }
      const t1 = performance.now();
      await new Promise((r) => setTimeout(r, ${WINDOW_MS}));
      return JSON.stringify({ t0p: Math.round(t), t1p: Math.round(t1), wroteMs: Math.round(t1 - t), bytes: bufs.reduce((a, b) => a + b.byteLength, 0) });
    })()`));
    const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__s30rec.ms, t: window.__s30rec.t })`));
    const idx = [];
    for (let i = 0; i < rec.t.length; i += 1) if (rec.t[i] >= stats.t0p - 120 && rec.t[i] <= stats.t1p + WINDOW_MS) idx.push(i);
    const writeIdx = [];
    for (let i = 0; i < rec.t.length; i += 1) if (rec.t[i] >= stats.t0p - 20 && rec.t[i] <= stats.t1p + 60) writeIdx.push(i);
    const msv = idx.map((i) => rec.ms[i]);
    const sorted = msv.slice().sort((a, b) => a - b);
    arms.push({
      arm, ...stats,
      frames: msv.length,
      max: msv.length ? Math.max(...msv) : 0,
      p1: sorted[Math.floor(sorted.length * 0.01)] || 0,
      over33: msv.filter((v) => v > 33).length,
      over16: msv.filter((v) => v > 16).length,
      writeMax: writeIdx.length ? Math.max(...writeIdx.map((i) => rec.ms[i])) : 0,
      writeOver33: writeIdx.filter((i) => rec.ms[i] > 33).length,
    });
  }
  // delete cost: a cache holding 40 shell copies (~46 MB) removed in one call
  const del = JSON.parse(await tab.evaluate(`(async () => {
    const c = await caches.open('s30-delete-probe');
    const body = await (await fetch('index.html', { cache: 'force-cache' })).arrayBuffer();
    for (let i = 0; i < 40; i += 1) await c.put('index.html?copy=' + i, new Response(body.slice(0)));
    const n = (await c.keys()).length;
    const t = performance.now();
    await caches.delete('s30-delete-probe');
    return JSON.stringify({ entries: n, deleteMs: Math.round(performance.now() - t), bytes: body.byteLength * n });
  })()`));
  const rec2 = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__s30rec.ms, t: window.__s30rec.t })`));
  const tailMax = Math.max(...rec2.ms.slice(-60));
  await tab.evaluate(`(async () => { await caches.delete('s30-write-probe'); return true; })()`);
  await tab.close();
  return { arms, del, deleteWindowMax: tailMax };
}
globalThis.writeCost30 = writeCost30;

// ---- noise floor: what this machine does to a page that performs no cache work at all ----
async function noiseFloor30(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await openFresh30('s30-noise', 1600, 900, 1, false);
  await tab.addInitScript(REC30);
  await tab.goto(`${BASE30(p)}?hour=22&cb=noise${Date.now()}`);
  await s30(14000);
  const windows = [];
  for (let w = 0; w < (o.windows || 3); w += 1) {
    const mark = JSON.parse(await tab.evaluate(`JSON.stringify({ t: Math.round(performance.now()) })`));
    await s30((o.seconds || 20) * 1000);
    const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__s30rec.ms, t: window.__s30rec.t, js: window.__s30rec.js })`));
    const idx = [];
    for (let i = 0; i < rec.t.length; i += 1) if (rec.t[i] >= mark.t) idx.push(i);
    const ms = idx.map((i) => rec.ms[i]);
    const sorted = ms.slice().sort((a, b) => a - b);
    windows.push({
      w, frames: ms.length,
      max: Math.max(...ms), p1: sorted[Math.floor(sorted.length * 0.01)],
      over33: idx.filter((i) => rec.ms[i] > 33).map((i) => ({ t: rec.t[i], ms: rec.ms[i], js: rec.js[i] })),
    });
  }
  await tab.close();
  return windows;
}
globalThis.noiseFloor30 = noiseFloor30;

// ---- cold first-visit A/B: worker installed vs worker absent, same page, same protocol ----
// 'nosw' removes navigator.serviceWorker before the page's own scripts, so registerOfflineCache()
// returns false and no worker, no install and no cache write happens; every other page path is
// identical. Boot and loop frames are reported apart: the boot frame is time to first frame, the
// loop frames are what a bench actually times.
async function coldAb30(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await openFresh30('s30-ab', o.width || 1600, o.height || 900, o.dpr || 1, !!o.touch);
  const cleared = await clearOffline30(tab, p);
  if (o.mode === 'nosw') await tab.addInitScript('Object.defineProperty(navigator, "serviceWorker", { value: undefined, configurable: true });');
  await tab.addInitScript(REC30);
  await tab.goto(`${BASE30(p)}?hour=22&cb=ab${o.mode || 'sw'}${Date.now()}`);
  await s30((o.seconds || 18) * 1000);
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ ms: window.__s30rec.ms, t: window.__s30rec.t, js: window.__s30rec.js, probes: window.__s30rec.probes })`));
  const page = JSON.parse(await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return JSON.stringify({ ff: st ? Math.round(st.firstFrameAt) : 0, assets: L && L.assets ? L.assets : null, calls: L && L.stats ? L.stats().calls : null, ctrl: !!(navigator.serviceWorker && navigator.serviceWorker.controller) }); })()`));
  const dump = JSON.parse(await tab.evaluate(CACHE_DUMP));
  await tab.close();
  const loopFrom = (page.ff || 0) + 250;
  const loopIdx = [];
  for (let i = 0; i < rec.t.length; i += 1) if (rec.t[i] >= loopFrom) loopIdx.push(i);
  const loopMs = loopIdx.map((i) => rec.ms[i]);
  const sorted = loopMs.slice().sort((a, b) => a - b);
  return {
    mode: o.mode || 'sw', cleared, page,
    bootFrame: rec.ms.length ? Math.max(...rec.ms.slice(0, 1)) : 0,
    loop: {
      frames: loopMs.length,
      max: loopMs.length ? Math.max(...loopMs) : 0,
      p1: sorted.length ? sorted[Math.floor(sorted.length * 0.01)] : 0,
      over33: loopIdx.filter((i) => rec.ms[i] > 33).map((i) => ({ t: rec.t[i], ms: rec.ms[i], js: rec.js[i] })),
    },
    cacheEntries: dump.entries.length,
    probes: rec.probes,
  };
}
globalThis.coldAb30 = coldAb30;
