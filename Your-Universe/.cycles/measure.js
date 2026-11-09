// Cycle gate, browser half: the draw-call and triangle table per state, the two benches, and the
// screenshots. One call per cycle, same recipe every time, so numbers stay comparable.
//
// Usage, in an eval cell after `%load ".cycles/measure.js"`:
//   const t = await measureStates(8760, { shots: "iter-20" });
//   const d = await benchState(8780, {});
//
// Everything goes through the page's real surface and its real controls:
//   window.localtime.stats()  -> { fps, calls, triangles, vehicles, pedestrians, shadows, dream }
//   window.localtime.state()  -> { ..., dream, firstFrameAt }
//   window.localtime.rig      -> setTier, rotate, frameDistrict
//   window.localtime.runBench -> the page's own 20 s script
//
// Two recipes are deliberate and were learned the hard way:
//   - the viewport is set with `page.setViewport`, never `Emulation.setDeviceMetricsOverride`; the
//     override fights the page viewport and yields a 1706x960 frame instead of 1600x900.
//   - the dream is entered by clicking the real `#sleep` button at `?hour=22` and waiting for
//     `state().dream === 'dreaming'`, then 20 s for the ribbon to lay and the crew to settle. Waiting
//     for a `.dream-card` samples the end of a dream, and waiting at `?hour=23` samples the awake
//     night, because the scheduled works come up there on their own data rules.

async function openState(name, url, opts) {
  const o = opts || {};
  const width = o.width || 1600;
  const height = o.height || 900;
  const dpr = o.dpr || 1;
  const touch = !!o.touch;
  const tab = await browser.open({ name, url: 'about:blank', viewport: { width, height }, timeout: 180 });
  if (touch) {
    await tab.addInitScript('Object.defineProperty(navigator, "maxTouchPoints", { get: () => 5 });');
  }
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: v.dpr, isMobile: v.touch, hasTouch: v.touch });
  }, { args: [{ width, height, dpr, touch }] });
  await tab.goto(url);
  return tab;
}

const settle = (ms) => new Promise((r) => setTimeout(r, ms));

function statsOf(tab) {
  return tab.evaluate(`(() => {
    const L = window.localtime;
    if (!L || typeof L.stats !== 'function') return null;
    const s = L.stats();
    const q = typeof L.quality === 'function' ? L.quality() : null;
    const st = typeof L.state === 'function' ? L.state() : null;
    return {
      calls: s.calls, triangles: s.triangles, fps: s.fps,
      vehicles: s.vehicles, pedestrians: s.pedestrians, shadows: s.shadows,
      dream: s.dream === undefined ? (st ? st.dream : null) : s.dream,
      mode: q ? q.mode : null, pixelRatio: q ? q.pixelRatio : null,
      shadowMap: q ? q.shadowMap : null, population: q ? q.population : null,
      assetsLoaded: L.assets ? L.assets.loaded : null,
      assetModels: L.assets ? L.assets.models : null,
      tier: L.rig ? L.rig.tier : null
    };
  })()`);
}

async function frameSize(tab) {
  return tab.evaluate(`JSON.stringify({ w: innerWidth, h: innerHeight, dpr: devicePixelRatio, touch: navigator.maxTouchPoints })`);
}

async function waitSettled(tab, ms) {
  const deadline = Date.now() + (ms || 20000);
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => {
      const L = window.localtime;
      if (!L || !L.rig) return false;
      const st = typeof L.state === 'function' ? L.state() : null;
      return !!(st && st.firstFrameAt > 0);
    })()`);
    if (ready) return true;
    await settle(400);
  }
  return false;
}

// The dream, entered exactly as the owner enters it.
async function enterDream(tab) {
  await tab.click('#sleep');
  const deadline = Date.now() + 25000;
  while (Date.now() < deadline) {
    const dream = await tab.evaluate(`(() => { const L = window.localtime; return L.state ? L.state().dream : null; })()`);
    if (dream === 'dreaming') {
      await settle(20000); // the ribbon lays over 14 s of dream time, then the crew settles
      return true;
    }
    await settle(500);
  }
  return false;
}

async function setTier(tab, tier) {
  await tab.evaluate(`(() => { if (window.localtime.rig) window.localtime.rig.setTier(${JSON.stringify(tier)}, true); })()`);
  await settle(4000);
}

async function measureStates(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const shots = o.shots || null;
  const hour = o.hour === undefined ? 13 : o.hour;
  const base = `http://127.0.0.1:${p}/index.html`;
  const rows = {};

  const dt = await openState('ms-desk', `${base}?hour=${hour}&cb=ms1`);
  await waitSettled(dt, 20000);
  await settle(4200);
  rows['desktop day'] = await statsOf(dt);
  if (shots) await dt.screenshot({ path: `screenshots/${shots}-day.png` });

  await setTier(dt, 'street');
  rows['desktop street'] = await statsOf(dt);
  if (shots) {
    for (let r = 0; r < 4; r += 1) {
      await dt.screenshot({ path: `screenshots/${shots}-street-r${r}.png` });
      if (r < 3) {
        await dt.evaluate(`(() => { if (window.localtime.rig) window.localtime.rig.rotate(1); })()`);
        await settle(1800);
      }
    }
  }
  await dt.close();

  const nt = await openState('ms-night', `${base}?hour=23&cb=ms2`);
  await waitSettled(nt, 20000);
  await settle(4200);
  rows['desktop night'] = await statsOf(nt);
  if (shots) await nt.screenshot({ path: `screenshots/${shots}-night-awake.png` });
  await nt.close();

  const dr = await openState('ms-dream', `${base}?hour=22&cb=ms3`);
  await waitSettled(dr, 20000);
  await settle(4200);
  const took = await enterDream(dr);
  rows['desktop dream'] = { ...(await statsOf(dr)), entered: took };
  if (shots) await dr.screenshot({ path: `screenshots/${shots}-night.png` });
  await dr.close();

  const fa = await openState('ms-fallback', `${base}?hour=${hour}&noassets=1&cb=ms4`);
  await waitSettled(fa, 20000);
  await settle(4200);
  rows['desktop noassets'] = await statsOf(fa);
  await fa.close();

  const ph = await openState('ms-phone', `${base}?hour=${hour}&cb=ms5`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await waitSettled(ph, 24000);
  await settle(6000);
  rows['phone day'] = await statsOf(ph);
  rows['phone day'].frame = await frameSize(ph);
  if (shots) await ph.screenshot({ path: `screenshots/${shots}-pixel6.png` });
  await ph.close();

  return Object.entries(rows).map(([state, v]) => ({ state, ...v }));
}

// Phone night and dream are separate tabs so the day frame above is not disturbed by the dream.
async function measurePhoneNight(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const base = `http://127.0.0.1:${p}/index.html`;
  const rows = {};
  const nt = await openState('ms-phone-night', `${base}?hour=23&cb=ms6`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await waitSettled(nt, 24000);
  await settle(6000);
  rows['phone night'] = await statsOf(nt);
  await nt.close();

  const dr = await openState('ms-phone-dream', `${base}?hour=22&cb=ms7`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await waitSettled(dr, 24000);
  await settle(6000);
  const took = await enterDream(dr);
  rows['phone dream'] = { ...(await statsOf(dr)), entered: took };
  if (o.shots) await dr.screenshot({ path: `screenshots/${o.shots}-pixel6-night.png` });
  await dr.close();
  return Object.entries(rows).map(([state, v]) => ({ state, ...v }));
}

async function benchState(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const tab = await openState('ms-bench', `http://127.0.0.1:${p}/index.html?cb=bench${Date.now()}`, {
    width: o.width || 1600, height: o.height || 900, dpr: o.dpr || 1, touch: !!o.touch,
  });
  await waitSettled(tab, 24000);
  await settle(4200);
  if (o.throttle) await tab.emulate({ cpuThrottlingRate: o.throttle });
  const json = await tab.evaluate(`(async () => {
    const L = window.localtime;
    if (!L || typeof L.runBench !== 'function') return JSON.stringify({ error: 'no runBench' });
    const out = await L.runBench();
    return JSON.stringify(out);
  })()`);
  await tab.close();
  return json;
}
