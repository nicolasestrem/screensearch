// Cycle gate, browser half: the draw-call and triangle table per state, the two benches, and the
// screenshots. One call per cycle, same recipe every time, so numbers stay comparable.
//
// Usage, in an eval cell after `%load ".cycles/measure.js"`:
//   const t = await measureStates(8760, { shots: "iter-20" });
//   const d = await benchState(8780, {});
//   const r = await checkRules(8760);   // camera, dream boot restore and card rules, one call
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

// Same-page A/B. The triangle count of one page moves by several thousand with the data clock
// (the sun angle decides how many objects count as shadow casters), so a delta measured across
// two loads is worth about +/-8k and will mislead: cycle 23's world worker reported a +4,000
// frame change as a "measurement convention artefact" for exactly this reason. Any small delta
// must be read on ONE page, with the change applied in place.
//
//   const ab = await abSamePage(8760, {
//     url: "?hour=17",
//     tier: "street",
//     off: "window.localtimeCohort.setEnabled(false)",
//     on: "window.localtimeCohort.setEnabled(true)"
//   });
//   -> { off: {calls, tris, ped}, on: {calls, tris, ped}, delta: {calls, tris} }
//
// `tier` is applied with rig.setTier(tier, true) before either read; `off` runs first, then a
// settle, then `on`, then a settle, so the second read reflects a resting frame.
async function abSamePage(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const tab = await openState('ab-page', `http://127.0.0.1:${p}/index.html${o.url || '?hour=17&cb=ab'}`, {
    width: o.width || 1600, height: o.height || 900, dpr: o.dpr || 1, touch: !!o.touch,
  });
  await waitSettled(tab, 24000);
  await settle(4000);
  if (o.tier) await setTier(tab, o.tier);
  if (o.off) {
    await tab.evaluate(`(() => { ${o.off}; return true; })()`);
    await settle(o.settle || 1800);
  }
  const off = await statsOf(tab);
  if (o.on) {
    await tab.evaluate(`(() => { ${o.on}; return true; })()`);
    await settle(o.settle || 1800);
  }
  const on = await statsOf(tab);
  await tab.close();
  return {
    off: { calls: off.calls, tris: off.triangles, ped: off.pedestrians },
    on: { calls: on.calls, tris: on.triangles, ped: on.pedestrians },
    delta: { calls: on.calls - off.calls, tris: on.triangles - off.triangles },
  };
}

// ---------------------------------------------------------------------------------------------
// The owner-facing rules, checked in one call: `await checkRules(8760)`.
//
// These are the promises that are not allowed to drift with the world or the light, and each one
// was verified by hand every cycle until this helper existed. It returns
//   { camera: { pitch, snap, ratios, panClamped, pass }, dream: { bootTaken, bootFresh, noReoffer,
//     pass }, cards: { max, allTakeIt, pass }, pass }
// Every number is measured on the live build, never derived from source constants.
async function checkRules(port) {
  const p = port || 8760;
  const base = `http://127.0.0.1:${p}/index.html`;
  const out = { camera: {}, dream: {}, cards: {}, pass: false };

  // 1. Camera: fixed 40 degree pitch, 90 degree snap, tier zooms 1 / 0.62 / 0.38, clamped pan.
  const camRead = `(function () {
    const rig = window.localtime.rig, cam = rig.camera, t = rig.lookAt;
    const dx = cam.position.x - t.x, dy = cam.position.y - t.y, dz = cam.position.z - t.z;
    return JSON.stringify({
      pitch: Math.atan2(dy, Math.sqrt(dx * dx + dz * dz)) * 180 / Math.PI,
      yaw: Math.atan2(dx, dz) * 180 / Math.PI,
      dist: Math.sqrt(dx * dx + dy * dy + dz * dz)
    });
  })()`;
  const cam = await openState('rules-cam', `${base}?hour=17&cb=rc1`);
  await waitSettled(cam, 24000);
  await settle(4000);
  const c0 = JSON.parse(await cam.evaluate(camRead));
  await cam.evaluate(`(() => { window.localtime.rig.rotate(1); return true; })()`);
  await settle(1500);
  const c1 = JSON.parse(await cam.evaluate(camRead));
  let dyaw = c1.yaw - c0.yaw;
  if (dyaw < -180) dyaw += 360;
  if (dyaw > 180) dyaw -= 360;
  const dists = {};
  for (const tier of ['street', 'block', 'district']) {
    await setTier(cam, tier);
    await settle(1400);
    dists[tier] = JSON.parse(await cam.evaluate(camRead)).dist;
  }
  const drag = `(function () {
    const cv = document.querySelector('canvas');
    const r = cv.getBoundingClientRect();
    const x0 = r.left + r.width / 2, y0 = r.top + r.height / 2;
    const mk = function (type, x, y) {
      return new PointerEvent(type, { clientX: x, clientY: y, bubbles: true, pointerId: 1,
        pointerType: 'mouse', button: 0, buttons: type === 'pointerup' ? 0 : 1, isPrimary: true });
    };
    cv.dispatchEvent(mk('pointerdown', x0, y0));
    cv.dispatchEvent(mk('pointermove', x0 + 30, y0 + 10));
    cv.dispatchEvent(mk('pointermove', x0 + 9000, y0 + 9000));
    cv.dispatchEvent(mk('pointerup', x0 + 9000, y0 + 9000));
    const t = window.localtime.rig.lookAt, b = window.localtime.world.bounds;
    return JSON.stringify({ x: t.x, z: t.z, minX: b.minX, maxX: b.maxX, minZ: b.minZ, maxZ: b.maxZ });
  })()`;
  const panned = JSON.parse(await cam.evaluate(drag));
  await settle(400);
  const pt = JSON.parse(await cam.evaluate(`(function () { const t = window.localtime.rig.lookAt; return JSON.stringify({ x: t.x, z: t.z }); })()`));
  await cam.close();
  out.camera = {
    pitch: +c0.pitch.toFixed(3),
    snap: +dyaw.toFixed(3),
    ratios: [+(dists.block / dists.district).toFixed(3), +(dists.street / dists.district).toFixed(3)],
    panClamped: pt.x >= panned.minX - 6 && pt.x <= panned.maxX + 6 && pt.z >= panned.minZ - 6 && pt.z <= panned.maxZ + 6,
  };
  out.camera.pass = Math.abs(out.camera.pitch - 40) < 0.01 && Math.abs(out.camera.snap - 90) < 0.01
    && Math.abs(out.camera.ratios[0] - 0.62) < 0.002 && Math.abs(out.camera.ratios[1] - 0.38) < 0.002
    && out.camera.panClamped;

  // 2. Dream: a taken road is permanent from the first frame, and an empty store restores nothing.
  const roadProbe = `(function () {
    const L = window.localtime;
    let d = null;
    const sc = L.world.group.parent;
    for (let i = 0; i < sc.children.length; i++) if (sc.children[i].name === 'dream') d = sc.children[i];
    let perm = false;
    if (d) d.traverse(function (o) { if (o.name === 'road-permanent' && o.visible) perm = true; });
    return JSON.stringify({ permanentRoadVisible: perm });
  })()`;
  const dr = await openState('rules-dream', `${base}?hour=17&cb=rd1`);
  await waitSettled(dr, 24000);
  await settle(3500);
  // The store keeps card sentences verbatim, so the first sentence a scripted dream offers is a
  // valid store entry for this data set, and the boot matcher must rebuild it exactly.
  await dr.evaluate(`(() => { window.__b = null; window.localtime.runBench().then(function (r) { window.__b = r; }); return true; })()`);
  let firstCard = '';
  for (let i = 0; i < 90 && !firstCard; i += 1) {
    const txt = await dr.evaluate(`(function () { var c = document.querySelector('.dream-card p'); return c ? c.textContent : ''; })()`);
    if (txt) firstCard = txt;
    await settle(250);
  }
  await settle(2000);
  await dr.close();
  const fresh = await openState('rules-fresh', `${base}?hour=17&cb=rd2`);
  await waitSettled(fresh, 24000);
  await settle(3500);
  const bootFresh = JSON.parse(await fresh.evaluate(roadProbe));
  await fresh.close();
  let bootTaken = { permanentRoadVisible: false };
  let reoffered = true;
  if (firstCard) {
    const taken = await openState('rules-taken', `${base}?hour=17&cb=rd3`);
    await taken.evaluate(`localStorage.setItem('localtime.dreams.taken', JSON.stringify([${JSON.stringify(firstCard)}])); true`);
    await taken.goto(`${base}?hour=22&cb=rd4`);
    await waitSettled(taken, 24000);
    await settle(3500);
    bootTaken = JSON.parse(await taken.evaluate(roadProbe));
    await taken.evaluate(`(() => { window.__b = null; window.localtime.runBench().then(function (r) { window.__b = r; }); return true; })()`);
    let seenCard = '';
    for (let i = 0; i < 60 && !seenCard; i += 1) {
      const txt = await taken.evaluate(`(function () { var c = document.querySelector('.dream-card p'); return c ? c.textContent : ''; })()`);
      if (txt) seenCard = txt;
      await settle(250);
    }
    reoffered = !!seenCard;
    await taken.evaluate(`localStorage.removeItem('localtime.dreams.taken'); true`);
    await taken.close();
  }
  out.dream = {
    bootTaken: bootTaken.permanentRoadVisible,
    bootFresh: bootFresh.permanentRoadVisible,
    noReoffer: !reoffered,
    pass: bootTaken.permanentRoadVisible === true && bootFresh.permanentRoadVisible === false && !reoffered,
  };

  // 3. Cards: at most two, and every card carries its own "Take it".
  const cd = await openState('rules-cards', `${base}?hour=22&cb=rd5`);
  await waitSettled(cd, 24000);
  await settle(3500);
  await cd.evaluate(`(() => { window.__b = null; window.localtime.runBench().then(function (r) { window.__b = r; }); return true; })()`);
  let maxCards = 0;
  let allTake = true;
  for (let i = 0; i < 90; i += 1) {
    const s = await cd.evaluate(`(function () {
      var cards = document.querySelectorAll('.dream-card');
      var ok = true;
      cards.forEach(function (c) {
        var b = c.querySelector('button');
        if (!b || b.textContent !== 'Take it') ok = false;
      });
      return JSON.stringify({ n: cards.length, ok: ok });
    })()`);
    const j = JSON.parse(s);
    if (j.n > maxCards) maxCards = j.n;
    if (j.n > 0 && !j.ok) allTake = false;
    await settle(250);
  }
  await cd.close();
  out.cards = { max: maxCards, allTakeIt: allTake, pass: maxCards > 0 && maxCards <= 2 && allTake };
  out.pass = out.camera.pass && out.dream.pass && out.cards.pass;
  return out;
}
