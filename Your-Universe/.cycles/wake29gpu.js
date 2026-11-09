// Cycle 29: which GPU task is the first-wake stall? Chrome trace, 4 s window around the wake
// click, GPU categories included. Scratch tool.
//
//   await browser.close({ all: true });
//   const g = await wakeGpu(8731);
//
const w29gs = (ms) => new Promise((r) => setTimeout(r, ms));

const W29G_CATS = [
  'toplevel', 'gpu', 'viz', 'cc', 'blink',
  'devtools.timeline', 'disabled-by-default-devtools.timeline',
  'disabled-by-default-gpu.service', 'disabled-by-default-gpu.driver',
  'disabled-by-default-gpu.angle', 'latencyInfo', 'v8', 'v8.execute',
];

async function wakeGpu(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const out = 'C:/Users/nicol/Documents/GitHub/screensearch/Your-Universe/.cycles/wake29-trace.json';
  const tab = await browser.open({ name: 'w29g', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 300 });
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  const url = o.url || '?hour=22';
  await tab.goto(`http://127.0.0.1:${p}/index.html${url}&cb=w29g${Date.now()}`);
  await w29gs(6000);
  await tab.click('#sleep');
  await w29gs(o.dreamMs || 6000);
  const res = await tab.run(async ({ page }, args) => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    await page.tracing.start({ path: args.path, categories: args.cats });
    await wait(600);
    const t0 = Date.now();
    await page.click('#sleep');
    await wait(args.after);
    await page.tracing.stop();
    return { clickMs: Date.now() - t0, path: args.path };
  }, { timeout: 120, args: [{ path: out, cats: W29G_CATS, after: o.after || 3500 }] });
  await tab.close();
  const raw = JSON.parse(await read(out));
  const events = raw.traceEvents || raw;
  const named = events.filter((e) => typeof e.dur === 'number' && e.dur > 20000);
  named.sort((a, b) => b.dur - a.dur);
  const top = named.slice(0, 26).map((e) => ({
    name: e.name, cat: e.cat, durMs: Math.round(e.dur / 100) / 10, pid: e.pid, tid: e.tid,
    args: e.args && (e.args.data || e.args.Args) ? JSON.stringify(e.args.data || e.args.Args).slice(0, 180) : undefined,
  }));
  const byPid = {};
  for (const e of events) {
    if (typeof e.dur !== 'number') continue;
    byPid[e.pid] = (byPid[e.pid] || 0) + e.dur;
  }
  const procName = {};
  for (const e of events) {
    if (e.name === 'process_name' && e.args && e.args.name) procName[e.pid] = e.args.name;
  }
  return { clickMs: res.clickMs, bytes: JSON.stringify(raw).length, events: events.length, top, busy: Object.entries(byPid).sort((a, b) => b[1] - a[1]).slice(0, 6).map(([pid, dur]) => ({ pid, name: procName[pid] || '', ms: Math.round(dur / 1000) })) };
}
