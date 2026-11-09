// Cycle 21 worker B capture helpers. Same states and recipe as .cycles/measure.js, but the
// screenshots are written to disk with Puppeteer's own page.screenshot (the tab.screenshot
// helper has no path option). Tab names are prefixed LightB- so two workers never collide.
// Phone frames are saved at device pixels and also normalised to exactly 412x915.

const LightB_ROOT = 'C:/Users/nicol/Documents/GitHub/screensearch/Your-Universe/screenshots/';

async function LightB_saveShot(tab, name) {
  await tab.run(async ({ page }, p) => { await page.screenshot({ path: p }); }, { args: [LightB_ROOT + name] });
}

async function LightB_captureStates(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const shots = o.shots || 'LightB';
  const hour = o.hour === undefined ? 17 : o.hour;
  const base = `http://127.0.0.1:${p}/index.html`;
  const rows = {};

  const dt = await openState(`LightB-cs-${hour}`, `${base}?hour=${hour}&cb=cs${Date.now()}`);
  await waitSettled(dt, 20000);
  await settle(4200);
  rows['desktop day'] = await statsOf(dt);
  if (shots) await LightB_saveShot(dt, `${shots}-day.png`);
  await LightB_saveShot(dt, `${shots}-day-nopost-tmp.png`).catch(() => {});
  await dt.close();

  const nt = await openState(`LightB-cn-${hour}`, `${base}?hour=23&cb=cn${Date.now()}`);
  await waitSettled(nt, 20000);
  await settle(4200);
  rows['desktop night'] = await statsOf(nt);
  if (shots) await LightB_saveShot(nt, `${shots}-night-awake.png`);
  await nt.close();

  const dr = await openState(`LightB-cd-${hour}`, `${base}?hour=22&cb=cd${Date.now()}`);
  await waitSettled(dr, 20000);
  await settle(4200);
  const took = await enterDream(dr);
  rows['desktop dream'] = { ...(await statsOf(dr)), entered: took };
  if (shots) await LightB_saveShot(dr, `${shots}-dream.png`);
  await dr.close();

  const fa = await openState(`LightB-cf-${hour}`, `${base}?hour=${hour}&noassets=1&cb=cf${Date.now()}`);
  await waitSettled(fa, 20000);
  await settle(4200);
  rows['desktop noassets'] = await statsOf(fa);
  if (shots) await LightB_saveShot(fa, `${shots}-noassets.png`);
  await fa.close();

  const ph = await openState(`LightB-cp-${hour}`, `${base}?hour=${hour}&cb=cp${Date.now()}`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await waitSettled(ph, 24000);
  await settle(6000);
  rows['phone day'] = await statsOf(ph);
  rows['phone day'].frame = await frameSize(ph);
  if (shots) {
    await LightB_saveShot(ph, `${shots}-pixel6-full.png`);
    await ph.run(async ({ page }, p) => {
      const sharp = await import('sharp');
      await sharp(await page.screenshot()).resize(412, 915).toFile(p);
    }, { args: [LightB_ROOT + `${shots}-pixel6.png`] }).catch(() => {});
  }
  await ph.close();

  return Object.entries(rows).map(([state, v]) => ({ state, ...v }));
}

async function LightB_capturePhoneNight(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const shots = o.shots || null;
  const base = `http://127.0.0.1:${p}/index.html`;
  const rows = {};
  const nt = await openState(`LightB-pn-${Date.now()}`, `${base}?hour=23&cb=pn${Date.now()}`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await waitSettled(nt, 24000);
  await settle(6000);
  rows['phone night'] = await statsOf(nt);
  if (shots) await LightB_saveShot(nt, `${shots}-pixel6-night-full.png`);
  await nt.close();

  const dr = await openState(`LightB-pd-${Date.now()}`, `${base}?hour=22&cb=pd${Date.now()}`, { width: 412, height: 915, dpr: 2.625, touch: true });
  await waitSettled(dr, 24000);
  await settle(6000);
  const took = await enterDream(dr);
  rows['phone dream'] = { ...(await statsOf(dr)), entered: took };
  if (shots) await LightB_saveShot(dr, `${shots}-pixel6-dream-full.png`);
  await dr.close();
  return Object.entries(rows).map(([state, v]) => ({ state, ...v }));
}

// One state, one tab: any query, any viewport, stats plus an optional screenshot.
async function LightB_captureOne(port, opts) {
  const o = opts || {};
  const tab = await openState(`LightB-one-${Date.now()}`, o.url, { width: o.width || 1600, height: o.height || 900, dpr: o.dpr || 1, touch: !!o.touch });
  await waitSettled(tab, 24000);
  await settle(o.settle || 4200);
  const stats = await statsOf(tab);
  if (o.path) await LightB_saveShot(tab, o.path);
  await tab.close();
  return stats;
}

// Normalise every phone capture at device pixels down to exactly 412x915 with sharp.
async function LightB_normalisePhone(names) {
  for (const name of names) {
    const full = LightB_ROOT + name + '-full.png';
    const out = LightB_ROOT + name.replace('-full', '') + '.png';
    await (await import('sharp'))(full).resize(412, 915).toFile(out);
  }
}
