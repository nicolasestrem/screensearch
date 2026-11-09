// Cycle 29: which program is first used on the slow block-tier frame? Wraps createProgram,
// shaderSource, linkProgram, useProgram and drawElements before the page loads, so every program
// and its exact GLSL prefix (the injected defines) is known, and every first use is stamped with
// the frame number. Scratch tool.
//
//   await browser.close({ all: true });
//   const r = await progTrace(8731, { url: '?hour=22', actions: [
//     { name: 'street', js: "window.localtime.rig.setTier('street', true)" },
//     { name: 'block',  js: "window.localtime.rig.setTier('block', true)" },
//   ]});
//
const pg29 = (ms) => new Promise((r) => setTimeout(r, ms));

const PG29_INIT = `(() => {
  if (window.__pg29) return;
  const st = { frame: 0, nextProgram: 0, nextShader: 0, progs: [], sources: [], firstUse: [], linkMs: [] };
  window.__pg29 = st;
  const origRaf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = function (cb) {
    return origRaf(function (ts) {
      st.frame += 1;
      return cb(ts);
    });
  };
  const pid = new WeakMap();
  const sid = new WeakMap();
  const uses = new WeakMap();
  for (const ctor of [window.WebGL2RenderingContext, window.WebGLRenderingContext]) {
    if (!ctor) continue;
    const p = ctor.prototype;
    const createProgram = p.createProgram;
    p.createProgram = function () {
      const prog = createProgram.apply(this, arguments);
      if (prog) {
        pid.set(prog, st.nextProgram);
        st.progs[st.nextProgram] = { id: st.nextProgram, shaders: [], linked: st.frame, linkMs: 0 };
        st.nextProgram += 1;
      }
      return prog;
    };
    const createShader = p.createShader;
    p.createShader = function (type) {
      const sh = createShader.apply(this, arguments);
      if (sh) {
        sid.set(sh, st.nextShader);
        st.sources[st.nextShader] = { type: type, src: '' };
        st.nextShader += 1;
      }
      return sh;
    };
    const shaderSource = p.shaderSource;
    p.shaderSource = function (sh, src) {
      const id = sid.get(sh);
      if (id !== undefined) st.sources[id].src = String(src).slice(0, 700);
      return shaderSource.apply(this, arguments);
    };
    const attachShader = p.attachShader;
    p.attachShader = function (prog, sh) {
      const i = pid.get(prog);
      const s = sid.get(sh);
      if (i !== undefined && s !== undefined) st.progs[i].shaders.push(s);
      return attachShader.apply(this, arguments);
    };
    const linkProgram = p.linkProgram;
    p.linkProgram = function (prog) {
      const i = pid.get(prog);
      const a = performance.now();
      const out = linkProgram.apply(this, arguments);
      if (i !== undefined) {
        st.progs[i].linkMs = Math.round((performance.now() - a) * 10) / 10;
        st.progs[i].linked = st.frame;
      }
      return out;
    };
    const useProgram = p.useProgram;
    p.useProgram = function (prog) {
      if (prog) {
        const i = pid.get(prog);
        if (i !== undefined && !uses.has(prog)) {
          uses.set(prog, st.frame);
          st.firstUse.push({ id: i, frame: st.frame });
        }
      }
      return useProgram.apply(this, arguments);
    };
  }
})()`;

async function progTrace(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'pg29', url: 'about:blank', viewport: { width: 1600, height: 900 }, timeout: 240 });
  await tab.addInitScript(PG29_INIT);
  await tab.run(async ({ page }) => {
    await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  });
  await tab.goto(`http://127.0.0.1:${p}/index.html${o.url}&cb=pg29${Date.now()}`);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const ready = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ready) break;
    await pg29(400);
  }
  await pg29(5000);
  const marks = [];
  for (const a of o.actions || []) {
    marks.push({ name: a.name, frame: await tab.evaluate(`window.__pg29.frame`), firstUse: await tab.evaluate(`window.__pg29.firstUse.length`) });
    if (a.js) await tab.evaluate(`(() => { ${a.js}; return true; })()`);
    await pg29(a.wait === undefined ? 3500 : a.wait);
  }
  marks.push({ name: 'end', frame: await tab.evaluate(`window.__pg29.frame`), firstUse: await tab.evaluate(`window.__pg29.firstUse.length`) });
  const data = JSON.parse(await tab.evaluate(`JSON.stringify({ frame: window.__pg29.frame, progCount: window.__pg29.progs.length, progs: window.__pg29.progs, sources: window.__pg29.sources, firstUse: window.__pg29.firstUse })`));
  await tab.close();
  // For each window, the programs first used inside it, with the distinguishing defines.
  const out = [];
  for (let i = 0; i + 1 < marks.length; i += 1) {
    const a = marks[i];
    const b = marks[i + 1];
    const fresh = data.firstUse.filter((u) => u.frame >= a.frame && u.frame < b.frame);
    out.push({
      name: a.name,
      frames: b.frame - a.frame,
      newPrograms: fresh.map((u) => {
        const prog = data.progs[u.id];
        const vs = prog.shaders.length ? data.sources[prog.shaders[0]] : null;
        const fs = prog.shaders.length > 1 ? data.sources[prog.shaders[1]] : null;
        const defines = ((fs && fs.src) || '').split('\n').filter((l) => l.indexOf('#define ') === 0).join(' ');
        return {
          id: u.id,
          frame: u.frame,
          linkMs: prog.linkMs,
          vsHead: vs ? vs.src.split('\n').filter((l) => l.indexOf('#define ') === 0).slice(0, 14).join(' ') : '',
          fsHead: defines.slice(0, 900),
        };
      }),
    });
  }
  return { url: o.url, totalPrograms: data.progCount, windows: out };
}
