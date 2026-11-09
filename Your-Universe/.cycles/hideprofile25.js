// Attribute a frame-time spike to a subsystem by hiding groups on ONE page and re-recording.
// Scratch tool.
//
//   const ab = await hideProfile(8731, { url: '?hour=17', sets: [['terrain'], ['towers'], ['details'], ['trees']] });

const hp25settle = (ms) => new Promise((r) => setTimeout(r, ms));

const HP25_REC = `(() => {
  if (window.__hp) return true;
  const rec = { ms: [] };
  window.__hp = rec;
  let last = performance.now();
  const loop = () => {
    const now = performance.now();
    rec.ms.push(Math.round((now - last) * 100) / 100);
    last = now;
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
  return true;
})()`;

async function hideProfile(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'hp25', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 180 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html${o.url || '?hour=17'}&cb=hp25${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await hp25settle(400);
  }
  await hp25settle(5000);
  await tab.evaluate(HP25_REC);
  const sample = async (label, hideExpr) => {
    await tab.evaluate(`(() => {
      const g = window.localtime.world.group;
      const names = ${JSON.stringify(null)};
      ${hideExpr || 'void 0'};
      return true;
    })()`);
    await hp25settle(1200);
    await tab.evaluate(`window.__hp.ms.length = 0`);
    await hp25settle(o.seconds ? o.seconds * 1000 : 12000);
    const ms = JSON.parse(await tab.evaluate(`JSON.stringify(window.__hp.ms)`));
    const mean = ms.reduce((a, b) => a + b, 0) / ms.length;
    return { label, frames: ms.length, meanMs: Math.round(mean * 100) / 100, maxMs: Math.max(...ms),
      overTen: ms.filter((v) => v > 10).length, overTwenty: ms.filter((v) => v > 20).length };
  };
  const out = [];
  out.push(await sample('all', ''));
  for (const set of o.sets || []) {
    const expr = `for (const n of ${JSON.stringify(set)}) { const m = g.getObjectByName(n); if (m) m.visible = false; }`;
    out.push(await sample('hide ' + set.join('+'), expr));
    const restore = `for (const n of ${JSON.stringify(set)}) { const m = g.getObjectByName(n); if (m) m.visible = true; }`;
    await tab.evaluate(`(() => { const g = window.localtime.world.group; ${restore}; return true; })()`);
  }
  await tab.close();
  return out;
}
