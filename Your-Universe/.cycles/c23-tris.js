// Cycle 23 world probe: per-mesh triangle inventory of the live scene, per tier.
// Read-only: traverses world.group (and the scene, for the agents' group) and prints
// name, instance count, triangles per instance, total triangles, and visibility.
async function trisOf(port, opts) {
  const o = opts || {};
  const p = port || 8760;
  const tier = o.tier || 'street';
  const hour = o.hour === undefined ? 17 : o.hour;
  const tab = await browser.open({ name: 'c23-tris', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 180 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=${hour}&cb=c23t${Date.now()}`);
  const t0 = Date.now();
  while (Date.now() - t0 < 40000) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; return !!(L && L.state && L.state().firstFrameAt > 0); })()`);
    if (ready) break;
    await new Promise((r) => setTimeout(r, 400));
  }
  await new Promise((r) => setTimeout(r, 4000));
  if (tier !== 'district') {
    await tab.evaluate(`(() => { if (window.localtime.rig) window.localtime.rig.setTier(${JSON.stringify(tier)}, true); })()`);
    await new Promise((r) => setTimeout(r, 4000));
  }
  const rows = await tab.evaluate(`(() => {
    const L = window.localtime;
    const out = [];
    const seen = new Set();
    function walk(obj, prefix) {
      if (!obj || seen.has(obj)) return;
      seen.add(obj);
      const kids = obj.children || [];
      for (const c of kids) {
        const name = (prefix ? prefix + '/' : '') + (c.name || c.type);
        if (c.isMesh && c.geometry) {
          const g = c.geometry;
          const per = (g.index ? g.index.count : (g.attributes.position ? g.attributes.position.count : 0)) / 3;
          const inst = c.isInstancedMesh ? c.count : 1;
          out.push({ name: name, inst: inst, per: Math.round(per * 10) / 10, tris: Math.round(per * inst), vis: c.visible, cast: c.castShadow });
        }
        walk(c, name);
      }
    }
    walk(L.world ? L.world.group : null, '');
    walk(L.scene || (L.world && L.world.group && L.world.group.parent) || null, 'S');
    out.sort((a, b) => b.tris - a.tris);
    const info = L.stats();
    return JSON.stringify({ stats: { calls: info.calls, triangles: info.triangles, tier: L.rig ? L.rig.tier : null }, rows: out }, null, 0);
  })()`);
  await tab.close();
  return JSON.parse(rows);
}
