// Cycle 29: what GL work lands on the slow block-tier frame? Wraps the WebGL prototypes before
// the page loads (addInitScript) and records every call that blocks or uploads: the link handshake
// (linkProgram + getProgramParameter LINK_STATUS), shader compiles, buffer and texture uploads,
// and each one's own wall time. Scratch tool.
//
//   await browser.close({ all: true });
//   const r = await glTrace(8731, { url: '?hour=22', actions: [
//     { name: 'street', js: "window.localtime.rig.setTier('street', true)" },
//     { name: 'block',  js: "window.localtime.rig.setTier('block', true)" },
//   ]});
//
const gl29 = (ms) => new Promise((r) => setTimeout(r, ms));

const GL29_INIT = `(() => {
  if (window.__gl29) return;
  const log = { frames: 0, entries: [] };
  window.__gl29 = log;
  let frame = 0;
  const origRaf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = function (cb) {
    return origRaf(function (ts) {
      frame += 1;
      log.frames = frame;
      return cb(ts);
    });
  };
  const wrap = (proto, name, kind) => {
    const orig = proto[name];
    if (typeof orig !== 'function') return;
    proto[name] = function () {
      const a = performance.now();
      let out;
      try {
        out = orig.apply(this, arguments);
      } finally {
        const ms = performance.now() - a;
        if (ms > 1.5) {
          log.entries.push({ f: frame, call: name, kind, ms: Math.round(ms * 10) / 10, bytes: kind === 'upload' ? (arguments[1] && arguments[1].byteLength) || (arguments[4] && arguments[4].byteLength) || 0 : 0 });
        }
      }
      return out;
    };
  };
  for (const ctor of [window.WebGL2RenderingContext, window.WebGLRenderingContext]) {
    if (!ctor) continue;
    const p = ctor.prototype;
    wrap(p, 'linkProgram', 'link');
    wrap(p, 'getProgramParameter', 'link');
    wrap(p, 'compileShader', 'compile');
    wrap(p, 'getShaderParameter', 'compile');
    wrap(p, 'bufferData', 'upload');
    wrap(p, 'bufferSubData', 'upload');
    wrap(p, 'texImage2D', 'upload');
    wrap(p, 'texSubImage2D', 'upload');
    wrap(p, 'generateMipmap', 'upload');
    wrap(p, 'readPixels', 'read');
    wrap(p, 'finish', 'sync');
    wrap(p, 'getError', 'sync');
  }
})()`;

async function glTrace(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'gl29', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 240 });
  await tab.addInitScript(GL29_INIT);
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html${o.url}&cb=gl29${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await gl29(400);
  }
  if (o.setup) await tab.evaluate(`(() => { ${o.setup}; return true; })()`);
  await gl29(5000);
  await tab.evaluate(`window.__gl29.entries.length = 0`);
  const marks = [];
  for (const a of o.actions || []) {
    marks.push({ name: a.name, frame: await tab.evaluate(`window.__gl29.frames`) });
    if (a.js) await tab.evaluate(`(() => { ${a.js}; return true; })()`);
    await gl29(a.wait === undefined ? 3500 : a.wait);
  }
  marks.push({ name: 'end', frame: await tab.evaluate(`window.__gl29.frames`) });
  const log = JSON.parse(await tab.evaluate(`JSON.stringify(window.__gl29)`));
  await tab.close();
  const windows = [];
  for (let i = 0; i + 1 < marks.length; i += 1) {
    const a = marks[i];
    const b = marks[i + 1];
    const entries = log.entries.filter((e) => e.f >= a.frame && e.f < b.frame);
    const total = entries.reduce((s, e) => s + e.ms, 0);
    windows.push({
      name: a.name,
      frames: b.frame - a.frame,
      slowCalls: entries.length,
      slowMs: Math.round(total * 10) / 10,
      byCall: entries.reduce((acc, e) => {
        const k = e.call + '/' + e.kind;
        acc[k] = acc[k] || { n: 0, ms: 0 };
        acc[k].n += 1;
        acc[k].ms = Math.round((acc[k].ms + e.ms) * 10) / 10;
        return acc;
      }, {}),
      worst: entries.sort((x, y) => y.ms - x.ms).slice(0, 6),
    });
  }
  return { url: o.url, windows };
}
