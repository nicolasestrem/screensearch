// Cycle 23 world verification: the five required frames plus the per-state table.
// Captures go through the page's real surface inside tab.run, because screenshot({path}) writes
// nothing on this harness (learned in cycle 22).
async function openC23(name, url, opts) {
  const o = opts || {};
  const width = o.width || 1600;
  const height = o.height || 900;
  const dpr = o.dpr || 1;
  const touch = !!o.touch;
  const tab = await browser.open({ name, url: 'about:blank', viewport: { width, height }, timeout: 240 });
  if (touch) await tab.addInitScript('Object.defineProperty(navigator, "maxTouchPoints", { get: () => 5 });');
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: v.dpr, isMobile: v.touch, hasTouch: v.touch });
  }, { args: [{ width, height, dpr, touch }] });
  await tab.goto(url);
  const deadline = Date.now() + 40000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; return !!(L && L.state && L.state().firstFrameAt > 0); })()`);
    if (ready) break;
    await new Promise((r) => setTimeout(r, 400));
  }
  return tab;
}

async function c23Stats(tab) {
  return JSON.parse(await tab.evaluate(`(() => {
    const L = window.localtime;
    const s = L.stats();
    const q = L.quality ? L.quality() : null;
    return JSON.stringify({ calls: s.calls, triangles: s.triangles, fps: s.fps, vehicles: s.vehicles,
      pedestrians: s.pedestrians, shadows: s.shadows, dream: s.dream, tier: L.rig ? L.rig.tier : null,
      shadowMap: q ? q.shadowMap : null, population: q ? q.population : null,
      details: L.world && L.world.details ? L.world.details : null });
  })()`));
}

async function c23Capture(tab, file) {
  return tab.run(async ({ page }, v) => {
    await write(v.file, await page.screenshot());
    return v.file;
  }, { args: [{ file }] });
}

async function c23Run(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const base = `http://127.0.0.1:${p}/index.html`;
  const rows = {};
  const t = await openC23('c23-day', `${base}?hour=17&cb=c23d${Date.now()}`);
  await new Promise((r) => setTimeout(r, 4200));
  rows['desktop day'] = await c23Stats(t);
  await c23Capture(t, 'screenshots/iter-23-day.png');
  await t.evaluate(`(() => { window.localtime.rig.setTier('street', true); })()`);
  await new Promise((r) => setTimeout(r, 4000));
  rows['desktop street'] = await c23Stats(t);
  for (let r = 0; r < 4; r += 1) {
    await c23Capture(t, `screenshots/iter-23-street-r${r}.png`);
    if (r < 3) {
      await t.evaluate(`(() => { window.localtime.rig.rotate(1); })()`);
      await new Promise((r2) => setTimeout(r2, 1800));
    }
  }
  await t.close();

  const n = await openC23('c23-night', `${base}?hour=23&cb=c23n${Date.now()}`);
  await new Promise((r) => setTimeout(r, 4200));
  rows['desktop night'] = await c23Stats(n);
  await n.close();

  const d = await openC23('c23-dream', `${base}?hour=22&cb=c23m${Date.now()}`);
  await new Promise((r) => setTimeout(r, 4200));
  await d.click('#sleep');
  const dl = Date.now() + 25000;
  let entered = false;
  while (Date.now() < dl) {
    const st = await d.evaluate(`(() => window.localtime.state().dream)()`);
    if (st === 'dreaming') { entered = true; break; }
    await new Promise((r) => setTimeout(r, 500));
  }
  await new Promise((r) => setTimeout(r, 20000));
  rows['desktop dream'] = Object.assign(await c23Stats(d), { entered });
  await d.close();

  const f = await openC23('c23-fallback', `${base}?hour=17&noassets=1&cb=c23f${Date.now()}`);
  await new Promise((r) => setTimeout(r, 4200));
  rows['desktop noassets'] = await c23Stats(f);
  await f.close();

  const ph = await openC23('c23-phone', `${base}?hour=17&cb=c23p${Date.now()}`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await new Promise((r) => setTimeout(r, 6000));
  rows['phone day'] = await c23Stats(ph);
  await ph.close();

  const pn = await openC23('c23-phone-night', `${base}?hour=23&cb=c23q${Date.now()}`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await new Promise((r) => setTimeout(r, 6000));
  rows['phone night'] = await c23Stats(pn);
  await pn.close();

  return rows;
}
