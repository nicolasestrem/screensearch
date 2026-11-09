// Cycle 29: attribute the wake stall rather than guess at it.
//
// The recorder wraps window.requestAnimationFrame (three's WebGLAnimation calls
// context.requestAnimationFrame, a live property lookup on window), so every animation frame is
// timed twice: the wall delta between frames (what the bench reports) and the time spent inside
// the frame callback (what JS actually spent before yielding to the compositor). A 370 ms frame
// whose js time is also 370 ms is main-thread work (a compile, an upload, a matrix pass); one
// whose js time is 8 ms is GPU/compositor back pressure, which on this headless box (software GL)
// is the frame waiting for a program link or a buffer upload it cannot see.
//
//   await browser.close({ all: true });
//   const t = await wakeTrace(8731);                   // one sleep/wake
//   const t = await wakeTrace(8731, { cycles: 3 });    // three, to tell one-off from per-wake
//   const t = await wakeTrace(8731, { url: '?hour=22&post=0' });
//
const w29settle = (ms) => new Promise((r) => setTimeout(r, ms));

const W29_REC = `(() => {
  if (window.__w29) return true;
  const rec = { t: [], ms: [], js: [], dream: [], calls: [], tris: [], max: 0 };
  window.__w29 = rec;
  const orig = window.requestAnimationFrame.bind(window);
  let last = performance.now();
  window.requestAnimationFrame = function (cb) {
    return orig(function (ts) {
      const a = performance.now();
      try {
        return cb(ts);
      } finally {
        const b = performance.now();
        rec.js.push(Math.round((b - a) * 100) / 100);
      }
    });
  };
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
    orig(loop);
  };
  orig(loop);
  return true;
})()`;

// A CPU profile's self time per function, so the worst frame can be named.
function w29Hot(profile) {
  const byId = new Map();
  for (const n of profile.nodes || []) byId.set(n.id, n);
  const self = new Map();
  const dt = profile.timeDeltas || [];
  const samples = profile.samples || [];
  for (let i = 0; i < samples.length; i += 1) {
    const n = byId.get(samples[i]);
    if (!n) continue;
    const f = n.callFrame;
    const key = (f.functionName || '(anon)') + ' ' + (f.url || '').split('/').pop() + ':' + (f.lineNumber + 1);
    self.set(key, (self.get(key) || 0) + (dt[i] || 0));
  }
  return Array.from(self.entries()).sort((a, b) => b[1] - a[1]).slice(0, 14)
    .map(([k, v]) => ({ fn: k, ms: Math.round(v / 1000 * 10) / 10 }));
}

async function wakeTrace(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({
    name: o.name || 'w29',
    url: 'about:blank',
    viewport: { width: 1600, height: 900 },
    timeout: 180,
  });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  const url = o.url || '?hour=22';
  await tab.goto(`http://127.0.0.1:${p}/index.html${url}${url.indexOf('?') < 0 ? '?' : '&'}cb=w29${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await w29settle(400);
  }
  if (o.setup) await tab.evaluate(`(() => { ${o.setup}; return true; })()`);
  await w29settle(o.settle === undefined ? 4000 : o.settle);
  await tab.evaluate(W29_REC);
  await w29settle(1200);
  const marks = [await tab.evaluate(`window.__w29.ms.length`)];
  const cycles = o.cycles || 1;
  let hot = null;
  for (let c = 0; c < cycles; c += 1) {
    await tab.click('#sleep');
    await w29settle(o.dreamMs === undefined ? 8000 : o.dreamMs);
    marks.push(await tab.evaluate(`window.__w29.ms.length`));
    const sampled = !!o.sample && c === 0;
    if (sampled) await tab.profileStart();
    await tab.click('#sleep');
    await w29settle(o.wakeMs === undefined ? 9000 : o.wakeMs);
    if (sampled) {
      const prof = await tab.profileStop();
      if (prof) hot = w29Hot(prof);
    }
    marks.push(await tab.evaluate(`window.__w29.ms.length`));
  }
  const rec = JSON.parse(await tab.evaluate(`JSON.stringify({
    t: window.__w29.t, ms: window.__w29.ms, js: window.__w29.js,
    dream: window.__w29.dream, calls: window.__w29.calls, tris: window.__w29.tris, max: window.__w29.max })`));
  await tab.close();
  const frames = rec.ms.map((ms, i) => ({
    i, ms, js: rec.js[i] === undefined ? null : rec.js[i], dream: rec.dream[i], calls: rec.calls[i], tris: rec.tris[i],
  }));
  const edges = [];
  for (let i = 1; i < frames.length; i += 1) {
    if (frames[i].dream !== frames[i - 1].dream) {
      edges.push({ at: i, from: frames[i - 1].dream, to: frames[i].dream, window: frames.slice(i, i + 4).map((f) => f.ms), js: frames[i].js });
    }
  }
  const windows = [];
  for (let c = 0; c < cycles; c += 1) {
    const s0 = marks[c * 2];
    const s1 = marks[c * 2 + 1];
    const s2 = marks[c * 2 + 2];
    const sleepWin = frames.slice(s0, s1);
    const wakeWin = frames.slice(s1, s2);
    const top = wakeWin.slice().sort((a, b) => b.ms - a.ms).slice(0, 3);
    windows.push({
      cycle: c + 1,
      sleepMax: sleepWin.length ? Math.max(...sleepWin.map((f) => f.ms)) : null,
      wakeMax: wakeWin.length ? Math.max(...wakeWin.map((f) => f.ms)) : null,
      wakeTop: top.map((f) => ({ i: f.i, ms: f.ms, js: f.js, dream: f.dream, calls: f.calls, tris: f.tris })),
      wakeOver33: wakeWin.filter((f) => f.ms > 33).length,
      wakeFrames: wakeWin.length,
    });
  }
  const worst = frames.slice().sort((a, b) => b.ms - a.ms).slice(0, 6);
  return {
    url,
    frames: frames.length,
    maxMs: rec.max,
    worst,
    edges,
    windows,
    hot,
  };
}
