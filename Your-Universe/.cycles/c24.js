// Cycle 24 world probe: same-page triangle arithmetic for the detail pool and the frame mesh,
// plus the full state table and the corner captures. Scratch tool, not part of the deliverable.
//
// The point of the in-page probes is that a cross-load delta is worth about +/-8k triangles,
// so the per-instance cost of a box and of a frame is measured on ONE page by moving the drawn
// instance count and reading renderer.info, never by comparing two loads.

async function c24Open(port, url, opts) {
  const o = opts || {};
  const width = o.width || 1600;
  const height = o.height || 900;
  const dpr = o.dpr || 1;
  const touch = !!o.touch;
  const tab = await browser.open({ name: o.name || 'c24', url: 'about:blank', viewport: { width, height }, timeout: 180 });
  if (touch) {
    await tab.addInitScript('Object.defineProperty(navigator, "maxTouchPoints", { get: () => 5 });');
  }
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: v.dpr, isMobile: v.touch, hasTouch: v.touch });
  }, { args: [{ width, height, dpr, touch }] });
  await tab.goto(url);
  return tab;
}

const c24Settle = (ms) => new Promise((r) => setTimeout(r, ms));

async function c24Wait(tab, ms) {
  const deadline = Date.now() + (ms || 30000);
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) return true;
    await c24Settle(400);
  }
  return false;
}

async function c24Stats(tab) {
  return tab.evaluate(`(() => {
    const L = window.localtime;
    if (!L || typeof L.stats !== 'function') return null;
    const s = L.stats();
    const q = typeof L.quality === 'function' ? L.quality() : null;
    return { calls: s.calls, triangles: s.triangles, fps: s.fps, vehicles: s.vehicles,
      pedestrians: s.pedestrians, shadows: s.shadows, dream: s.dream, mode: q ? q.mode : null,
      tier: L.rig ? L.rig.tier : null };
  })()`);
}

async function c24Tier(tab, tier) {
  await tab.evaluate(`(() => { if (window.localtime.rig) window.localtime.rig.setTier(${JSON.stringify(tier)}, true); })()`);
  await c24Settle(4000);
}

// One capture, written through `write` because screenshot({path}) silently writes nothing.
async function c24Shot(tab, path) {
  const b64 = await tab.run(async ({ page }) => {
    const buf = await page.screenshot();
    return Buffer.from(buf).toString('base64');
  });
  await write(path, Buffer.from(b64, 'base64'));
  return path;
}

// The detail pool and the frame mesh, as the scene holds them: drawn instance count, geometry
// triangles per instance, castShadow, and the world audit numbers the build published.
async function c24Meshes(tab) {
  return tab.evaluate(`(() => {
    const L = window.localtime;
    const w = L && L.world;
    if (!w) return null;
    function tri(c) {
      const g = c.geometry;
      const per = (g.index ? g.index.count : (g.attributes.position ? g.attributes.position.count : 0)) / 3;
      return { per: Math.round(per * 10) / 10, inst: c.isInstancedMesh ? c.count : 1,
        tris: Math.round(per * (c.isInstancedMesh ? c.count : 1)), cast: c.castShadow, vis: c.visible };
    }
    const out = {};
    for (const name of ['details', 'frames', 'windows', 'shop-glass', 'buildings', 'statics', 'trees']) {
      const m = w.group.getObjectByName(name);
      if (m) out[name] = tri(m);
    }
    out.audit = w.details;
    out.roof = w.roofClutter;
    return out;
  })()`);
}

// Same-page A/B on the live buffers: nudge one drawn instance count, read, restore, read, so
// the per-instance cost is measured against the noise floor of the same frame.
async function c24InstanceCost(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const tab = await c24Open(p, `http://127.0.0.1:${p}/index.html?hour=17&cb=c24i${Date.now()}`, { name: 'c24-cost' });
  await c24Wait(tab, 40000);
  await c24Settle(4200);
  if (o.tier) await c24Tier(tab, o.tier);
  const base = await c24Stats(tab);
  const jump = o.jump || 112;
  const target = o.mesh || 'details';
  const read = async () => {
    const t = await c24Stats(tab);
    return { calls: t.calls, tris: t.triangles, ped: t.pedestrians };
  };
  // repeat the neutral read once, so the noise of an unchanged frame is known
  await c24Settle(1200);
  const base2 = await read();
  await tab.evaluate(`(() => { const m = window.localtime.world.group.getObjectByName('${target}'); window.__c24n = m.count; m.count = Math.max(0, m.count - ${jump}); })()`);
  await c24Settle(1200);
  const off = await read();
  await tab.evaluate(`(() => { const m = window.localtime.world.group.getObjectByName('${target}'); m.count = window.__c24n; })()`);
  await c24Settle(1200);
  const back = await read();
  const res = {
    mesh: target, tier: o.tier || 'district', jump,
    base: { calls: base.calls, tris: base.triangles, ped: base.pedestrians },
    base2,
    off,
    back,
    noise: base2.tris - base.triangles,
    perInstance: (back.tris - off.tris) / jump,
    callsDelta: off.calls - base2.calls
  };
  await tab.close();
  return res;
}

// The full state table, same recipe as the cycle gate: district day, street, night, dream,
// noassets and phone day. Screenshots are taken only where asked.
async function c24States(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const hour = o.hour === undefined ? 17 : o.hour;
  const base = `http://127.0.0.1:${p}/index.html`;
  const rows = {};
  const dt = await c24Open(p, `${base}?hour=${hour}&cb=c24d${Date.now()}`, { name: 'c24-day' });
  await c24Wait(dt, 40000);
  await c24Settle(4200);
  rows['desktop day'] = await c24Stats(dt);
  if (o.shots) await c24Shot(dt, `screenshots/${o.shots}-day.png`);
  if (o.street) {
    await c24Tier(dt, 'street');
    rows['desktop street'] = await c24Stats(dt);
    for (let r = 0; r < 4; r += 1) {
      if (o.shots) await c24Shot(dt, `screenshots/${o.shots}-street-r${r}.png`);
      if (r < 3) {
        await dt.evaluate(`(() => { if (window.localtime.rig) window.localtime.rig.rotate(${o.rotate || 1}); })()`);
        await c24Settle(1800);
      }
    }
    if (o.audit) rows.audit = await c24Meshes(dt);
  }
  await dt.close();
  if (o.night !== false) {
    const nt = await c24Open(p, `${base}?hour=23&cb=c24n${Date.now()}`, { name: 'c24-night' });
    await c24Wait(nt, 40000);
    await c24Settle(4200);
    rows['desktop night'] = await c24Stats(nt);
    await nt.close();
  }
  if (o.dream !== false) {
    const dr = await c24Open(p, `${base}?hour=22&cb=c24m${Date.now()}`, { name: 'c24-dream' });
    await c24Wait(dr, 40000);
    await c24Settle(4200);
    await dr.click('#sleep');
    let ok = false;
    const deadline = Date.now() + 25000;
    while (Date.now() < deadline) {
      const d = await dr.evaluate(`(() => { const L = window.localtime; return L.state ? L.state().dream : null; })()`);
      if (d === 'dreaming') { ok = true; break; }
      await c24Settle(500);
    }
    if (ok) await c24Settle(20000);
    rows['desktop dream'] = await c24Stats(dr);
    await dr.close();
  }
  if (o.noassets !== false) {
    const fa = await c24Open(p, `${base}?hour=${hour}&noassets=1&cb=c24f${Date.now()}`, { name: 'c24-fallback' });
    await c24Wait(fa, 40000);
    await c24Settle(4200);
    rows['desktop noassets'] = await c24Stats(fa);
    await fa.close();
  }
  if (o.phone) {
    const ph = await c24Open(p, `${base}?hour=${hour}&cb=c24p${Date.now()}`, { name: 'c24-phone', width: 412, height: 915, dpr: 2.625, touch: true });
    await c24Wait(ph, 40000);
    await c24Settle(6000);
    rows['phone day'] = await c24Stats(ph);
    await ph.close();
  }
  return Object.entries(rows).map(([state, v]) => ({ state, ...v }));
}
