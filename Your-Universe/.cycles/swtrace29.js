// Cycle 29: locate the first-visit stall in the trace. Same page source, cleared cache, tracing
// on across the navigation, minimal categories (no GPU instrumentation: those distort SwiftShader
// by orders of magnitude and hide every task). Reports the longest renderer tasks and the service
// worker events around the slow frame. Scratch tool.
//
//   await browser.close({ all: true });
//   const t = await swTrace(8731);
//
const swtr = (ms) => new Promise((r) => setTimeout(r, ms));

async function swTrace(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const out = 'C:/Users/nicol/Documents/GitHub/screensearch/Your-Universe/.cycles/sw29-trace.json';
  const tab = await browser.open({ name: 'swtr', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 240 });
  if (o.blockSw) {
    await tab.addInitScript(`(() => {
      try {
        Object.defineProperty(navigator, 'serviceWorker', {
          configurable: true,
          value: {
            register: function () { return Promise.reject(new Error('blocked by probe')); },
            getRegistrations: function () { return Promise.resolve([]); },
            addEventListener: function () {},
            removeEventListener: function () {},
            controller: null,
            ready: new Promise(function () {}),
          },
        });
      } catch (error) {}
    })()`);
  }
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html?cb=swtrclear${Date.now()}`);
  await swtr(3000);
  const cleared = await tab.evaluate(`(async () => {
    const regs = await navigator.serviceWorker.getRegistrations();
    for (const r of regs) await r.unregister();
    const keys = await caches.keys();
    for (const k of keys) await caches.delete(k);
    return JSON.stringify({ regs: regs.length, keys });
  })()`);
  const res = await tab.run(async ({ page }, args) => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    await page.tracing.start({
      path: args.path,
      categories: ['toplevel', 'devtools.timeline', 'disabled-by-default-devtools.timeline', 'blink.user_timing', 'latencyInfo', 'v8', 'serviceworker'],
    });
    await wait(200);
    await page.goto(args.url);
    await wait(args.after);
    await page.tracing.stop();
    return { ok: true };
  }, { timeout: 180, args: [{ path: out, url: `http://127.0.0.1:${p}/index.html?hour=22&cb=swtr${Date.now()}`, after: o.after || 6000 }] });
  await tab.close();
  const raw = JSON.parse(await read(out));
  const events = raw.traceEvents || raw;
  const procName = new Map();
  const threadName = new Map();
  for (const e of events) {
    if (e.name === 'process_name' && e.args && e.args.name) procName.set(e.pid, e.args.name);
    if (e.name === 'thread_name' && e.args && e.args.name) threadName.set(e.pid + ':' + e.tid, e.args.name);
  }
  const label = (e) => (procName.get(e.pid) || String(e.pid)) + ' / ' + (threadName.get(e.pid + ':' + e.tid) || e.tid);
  const tasks = events.filter((e) => typeof e.dur === 'number' && e.dur > 4000);
  tasks.sort((a, b) => b.dur - a.dur);
  const top = tasks.slice(0, 30).map((e) => ({ ms: Math.round(e.dur / 100) / 10, name: e.name, where: label(e) }));
  const swEvents = events.filter((e) => (e.cat || '').indexOf('serviceworker') >= 0 || String(e.name).indexOf('ServiceWorker') >= 0);
  const swNames = new Map();
  for (const e of swEvents) swNames.set(e.name, (swNames.get(e.name) || 0) + 1);
  const threads = new Map();
  for (const e of events) {
    if (typeof e.dur !== 'number') continue;
    const k = label(e);
    threads.set(k, (threads.get(k) || 0) + e.dur);
  }
  return {
    cleared,
    events: events.length,
    top,
    swCounts: Array.from(swNames.entries()).slice(0, 20),
    busiest: Array.from(threads.entries()).sort((a, b) => b[1] - a[1]).slice(0, 8).map(([k, v]) => ({ thread: k, ms: Math.round(v / 1000) })),
    path: out,
  };
}
