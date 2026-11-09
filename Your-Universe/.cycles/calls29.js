// Cycle 29: where the 105 draw calls go. Walks the live scene and counts the objects that can
// submit a draw (visible, non-zero instance count), split by top-level group, plus the shadow
// casters (one extra submission each) and the post chain's fixed pass count. Read-only, so the
// breakdown and `renderer.info.render.calls` describe the same frame.
//
//   const b = await calls29(8731, { url: '?hour=13' });
//   const f = await calls29(8731, { url: '?hour=13&noassets=1' });
//
const c29 = (ms) => new Promise((r) => setTimeout(r, ms));

const C29_COUNT = `(() => {
  const L = window.localtime;
  const world = L.world;
  const scene = world && world.group ? world.group.parent : null;
  if (!scene) return JSON.stringify({ error: 'no scene' });
  const groups = [];
  const seen = new Set();
  const count = (root) => {
    let meshes = 0, instances = 0, shadows = 0, hidden = 0;
    root.traverse((o) => {
      if (!o.isMesh) return;
      const n = o.isInstancedMesh ? o.count : 1;
      if (!o.visible || n <= 0) { hidden += 1; return; }
      meshes += 1;
      instances += n;
      if (o.castShadow) shadows += 1;
    });
    return { meshes, instances, shadows, hidden };
  };
  for (const child of scene.children) {
    if (child === world.group) { groups.push({ name: 'world', ...count(child) }); continue; }
    const name = child.name || (child.type + ':' + child.id);
    groups.push({ name, ...count(child) });
    seen.add(name);
  }
  return JSON.stringify({ groups });
})()`;

async function calls29(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'c29', url: 'about:blank', viewport: { width: o.width || 1600, height: o.height || 900 }, timeout: 240 });
  if (o.touch) await tab.addInitScript('Object.defineProperty(navigator, "maxTouchPoints", { get: () => 5 });');
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: v.dpr, isMobile: v.touch, hasTouch: v.touch });
  }, { args: [{ width: o.width || 1600, height: o.height || 900, dpr: o.dpr || 1, touch: !!o.touch }] });
  await tab.goto(`http://127.0.0.1:${p}/index.html${o.url}&cb=c29${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await c29(400);
  }
  await c29(5000);
  const counted = JSON.parse(await tab.evaluate(C29_COUNT));
  const stats = JSON.parse(await tab.evaluate('JSON.stringify(window.localtime.stats())'));
  await tab.close();
  const total = {
    meshes: counted.groups.reduce((s, g) => s + g.meshes, 0),
    shadows: counted.groups.reduce((s, g) => s + g.shadows, 0),
    instances: counted.groups.reduce((s, g) => s + g.instances, 0),
  };
  return { url: o.url, calls: stats.calls, triangles: stats.triangles, groups: counted.groups, total };
}
