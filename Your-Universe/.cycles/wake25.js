// Cycle 29 prep: reproduce the wake stall the owner measured (1.0 to 1.1 s frame) and attribute
// it. Scratch tool, not part of the deliverable.
//
//   const w = await wakeStall(8731);   // after %load ".cycles/wake25.js"
//
// The recorder is a plain rAF loop installed before the first wait, so every frame the page draws
// is timed from the same clock the bench uses. The dream edge is read from
// window.localtime.state().dream, so the timeline names the frame that crossed it.

const w25settle = (ms) => new Promise((r) => setTimeout(r, ms));

const W25_RECORDER = `(() => {
  if (window.__w25) return true;
  const rec = { t: [], ms: [], dream: [], calls: [], tris: [], max: 0 };
  window.__w25 = rec;
  let last = performance.now();
  const loop = () => {
    const now = performance.now();
    const dt = now - last;
    last = now;
    rec.t.push(Math.round(now));
    rec.ms.push(Math.round(dt * 100) / 100);
    const L = window.localtime;
    const st = L && L.state ? L.state() : null;
    const s = L && L.stats ? L.stats() : null;
    rec.dream.push(st ? st.dream : '?');
    rec.calls.push(s ? s.calls : -1);
    rec.tris.push(s ? s.triangles : -1);
    if (dt > rec.max) rec.max = dt;
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
  return true;
})()`;

async function wakeStall(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({
    name: 'w25',
    url: 'about:blank',
    viewport: { width: 1600, height: 900 },
    timeout: 180,
  });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=22&cb=w25${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await w25settle(400);
  }
  await w25settle(4000);
  await tab.evaluate(W25_RECORDER);
  await w25settle(1500);
  const mark = (i) => i;
  const before = await tab.evaluate(`window.__w25.ms.length`);
  await tab.click('#sleep');
  await w25settle(8000);
  const mid = await tab.evaluate(`window.__w25.ms.length`);
  // Wake: the dream hands over, the fleet returns and the cards are built.
  await tab.click('#sleep');
  await w25settle(9000);
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify({ t: window.__w25.t, ms: window.__w25.ms, dream: window.__w25.dream, calls: window.__w25.calls, tris: window.__w25.tris, max: window.__w25.max })`));
  await tab.close();
  const frames = rec.ms.map((ms, i) => ({ i, ms, dream: rec.dream[i], calls: rec.calls[i], tris: rec.tris[i] }));
  const worst = frames.slice().sort((a, b) => b.ms - a.ms).slice(0, 8);
  // The dream edge frames: the first frame of each state change, plus the next three.
  const edges = [];
  for (let i = 1; i < frames.length; i += 1) {
    if (frames[i].dream !== frames[i - 1].dream) {
      edges.push({ at: i, from: frames[i - 1].dream, to: frames[i].dream, window: frames.slice(i, i + 4).map((f) => f.ms) });
    }
  }
  const settleWindow = frames.slice(before, mid);
  const wakeWindow = frames.slice(mid);
  return {
    frames: frames.length,
    maxMs: rec.max,
    worst: worst.map((f) => ({ i: f.i, ms: f.ms, dream: f.dream, calls: f.calls, tris: f.tris })),
    dreamEdges: edges,
    sleepWindow: { max: Math.max(...settleWindow.map((f) => f.ms)), n: settleWindow.length },
    wakeWindow: { max: Math.max(...wakeWindow.map((f) => f.ms)), n: wakeWindow.length },
    mark: mark(0),
  };
}
