// Cycle 30: the wake re-population window, measured per frame on one page.
//
//   await browser.close({ all: true });
//   const r = await wakeSlice(8731);                                  // desktop
//   const r = await wakeSlice(8731, { width: 412, height: 915, dpr: 2.625, touch: true, throttle: 4 });
//   -> { report, first, mid, last, sequence, settled }
//
// The recorder samples window.localtimeReseed once per frame, after the app's own frame ran, so
// `admitted` is the number of slots that really entered on that frame and `frames` is the
// window's own counter. `first`, `mid` and `last` are the samples for frame 1, frame 15 and the
// frame the window closed on, which is the evidence the review asked for: work on the first
// frame, work in the middle, and work still landing on the last.
const ws30 = (ms) => new Promise((r) => setTimeout(r, ms));

const WS30_REC = `(() => {
  if (window.__ws30) return true;
  const rec = { frames: [], done: false };
  window.__ws30 = rec;
  const orig = window.requestAnimationFrame.bind(window);
  const loop = () => {
    const r = window.localtimeReseed;
    if (r) {
      const st = window.localtime && window.localtime.stats ? window.localtime.stats() : null;
      rec.frames.push({
        open: r.open,
        frames: r.frames,
        left: r.framesLeft,
        kind: r.kind,
        sum: r.admittedSum,
        ad: [r.admitted[0], r.admitted[1], r.admitted[2], r.admitted[3], r.admitted[4], r.admitted[5]],
        tot: [r.admittedTotal[0], r.admittedTotal[1], r.admittedTotal[2], r.admittedTotal[3], r.admittedTotal[4], r.admittedTotal[5]],
        stp: [Math.round(r.step[0] * 100) / 100, Math.round(r.step[1] * 100) / 100, Math.round(r.step[2] * 100) / 100,
              Math.round(r.step[3] * 100) / 100, Math.round(r.step[4] * 100) / 100, Math.round(r.step[5] * 100) / 100],
        peds: st ? st.pedestrians : -1,
        veh: st ? st.vehicles : -1,
        calls: st ? st.calls : -1,
        tris: st ? st.triangles : -1
      });
    }
    orig(loop);
  };
  orig(loop);
  return true;
})()`;

function ws30Pools(s) {
  return { span: s.span, step: s.step, admitted: s.admitted, admittedTotal: s.admittedTotal };
}

async function wakeSlice(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const width = o.width || 1600;
  const height = o.height || 900;
  const dpr = o.dpr || 1;
  const touch = !!o.touch;
  const tab = await browser.open({ name: 'ws30', url: 'about:blank', viewport: { width, height }, timeout: 300 });
  if (touch) await tab.addInitScript('Object.defineProperty(navigator, "maxTouchPoints", { get: () => 5 });');
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: v.dpr, isMobile: v.touch, hasTouch: v.touch });
  }, { args: [{ width, height, dpr, touch }] });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=22&cb=ws30${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await ws30(400);
  }
  await ws30(o.settle === undefined ? 6000 : o.settle);
  if (o.throttle) await tab.emulate({ cpuThrottlingRate: o.throttle });
  await tab.evaluate(WS30_REC);
  // Sleep, then wake: the wake edge is the only thing that starts this window.
  await tab.click('#sleep');
  await ws30(o.dreamMs === undefined ? 6000 : o.dreamMs);
  const dreamState = JSON.parse(await tab.evaluate(`JSON.stringify({ reseed: window.localtimeReseed, stats: window.localtime.stats() })`));
  const mark = await tab.evaluate(`window.__ws30.frames.length`);
  await tab.click('#sleep');
  await ws30(o.wakeMs === undefined ? 4000 : o.wakeMs);
  const report = JSON.parse(await tab.evaluate(`JSON.stringify(window.localtimeReseed)`));
  const seq = JSON.parse(await tab.evaluate(`JSON.stringify(window.__ws30.frames.slice(${mark}))`));
  const settled = JSON.parse(await tab.evaluate(`JSON.stringify(window.localtime.stats())`));
  await tab.close();
  // The window the wake opened starts at the first sampled frame with open true and ends on the
  // sample that reports the closing frame count, which still carries that frame's admissions.
  const start = seq.findIndex((f) => f.open === true);
  let win = [];
  if (start >= 0) {
    const stopAt = report.lastFrames;
    let end = start;
    for (let i = start; i < seq.length; i += 1) {
      end = i;
      if (seq[i].frames >= stopAt) break;
    }
    win = seq.slice(start, end + 1);
  }
  const pick = (n) => {
    const f = win.find((x) => x.frames === n);
    return f || null;
  };
  const lastFrames = report.lastFrames;
  return {
    viewport: { width, height, dpr, touch, throttle: o.throttle || 0 },
    dream: dreamState,
    report,
    pools: ws30Pools(report),
    frames: win.length,
    first: pick(1),
    mid: pick(15),
    last: pick(lastFrames),
    admittedOnLast: win.length ? win[win.length - 1].sum : null,
    lastFrameOpen: win.length ? win[win.length - 1].open : null,
    sequence: win.map((f) => ({ frames: f.frames, left: f.left, sum: f.sum, ad: f.ad, tot: f.tot, veh: f.veh, peds: f.peds, calls: f.calls, tris: f.tris })),
    settled
  };
}
