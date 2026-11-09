// Cycle 25, owner item 1: the three defects whose evidence lives in the page, not in the source.
// Scratch tool, not part of the deliverable.
//
//   const r = await runDefects25Page(8731);   // after %load ".cycles/defects25-page.js"
//
// (b) a card whose dream was disposed must not survive the install that replaced it.
// (c) an export dropped before the sample fetch resolves must be the city that boots.
// (d) install() must free the sun's shadow map: live GL texture count must not grow per load.

const d25settle = (ms) => new Promise((r) => setTimeout(r, ms));

async function d25Open(port, url, opts) {
  const o = opts || {};
  const tab = await browser.open({
    name: o.name || 'd25',
    url: 'about:blank',
    viewport: { width: o.width || 1600, height: o.height || 900 },
    timeout: 180,
  });
  if (o.init) await tab.addInitScript(o.init);
  await tab.run(async ({ page }, v) => {
    await page.setViewport({ width: v.width, height: v.height, deviceScaleFactor: 1, isMobile: false, hasTouch: false });
  }, { args: [{ width: o.width || 1600, height: o.height || 900 }] });
  await tab.goto(url);
  return tab;
}

async function d25Ready(tab, ms) {
  const deadline = Date.now() + (ms || 25000);
  while (Date.now() < deadline) {
    const ok = await tab.evaluate(`(() => { const L = window.localtime; const st = L && L.state ? L.state() : null; return !!(st && st.firstFrameAt > 0); })()`);
    if (ok) return true;
    await d25settle(400);
  }
  return false;
}

// A tiny but valid export: two quarters, one hour, so a quarter name identifies which load won.
const D25_EXPORT = `(function () {
  const base = Date.UTC(2026, 9, 9, 9, 0, 0);
  const frames = [];
  for (let i = 0; i < 40; i += 1) {
    const app = i % 2 === 0 ? 'Delta Editor' : 'Gamma Terminal';
    frames.push({
      frame_id: i + 1,
      captured_at: base + i * 90000,
      app_hint: app,
      window_title: app + ' window ' + (i % 3),
      browser_url: null,
      activity_type: null,
      content_text: 'Delta Editor and Gamma Terminal hold the same quarterly ledger line while the audit runs.'
    });
  }
  return { schema: 'screensearch.export.v1', exported_at: base + 3600000, from: null, to: null, frames: frames, marks: [] };
})()`;

async function defectCards(port) {
  const p = port || 8731;
  const tab = await d25Open(p, `http://127.0.0.1:${p}/index.html?hour=22&cb=d25a${Date.now()}`, { name: 'd25-cards' });
  await d25Ready(tab, 30000);
  await d25settle(4500);
  await tab.click('#sleep');
  let dreaming = false;
  const dreamDeadline = Date.now() + 30000;
  while (Date.now() < dreamDeadline) {
    const st = await tab.evaluate(`(() => { const L = window.localtime; return L.state ? L.state().dream : null; })()`);
    if (st === 'dreaming') { dreaming = true; break; }
    await d25settle(600);
  }
  // Cards exist only once the dream hands over: the second press wakes it.
  await tab.click('#sleep');
  const deadline = Date.now() + 40000;
  let cards = 0;
  while (Date.now() < deadline) {
    cards = await tab.evaluate(`document.querySelectorAll('#cards .dream-card').length`);
    if (cards > 0) break;
    await d25settle(600);
  }
  const before = await tab.evaluate(`document.querySelectorAll('#cards .dream-card').length`);
  const handlers = await tab.evaluate(`(() => { const b = document.querySelector('#cards .dream-card button'); return b ? getComputedStyle(b).display : null; })()`);
  await tab.evaluate(`(() => { window.localtime.loadExport(${D25_EXPORT}); return true; })()`);
  await d25settle(2500);
  const after = await tab.evaluate(`document.querySelectorAll('#cards .dream-card').length`);
  const dreamState = await tab.evaluate(`(() => (window.localtime.state ? window.localtime.state().dream : null))()`);
  await tab.close();
  return { cardsBefore: before, buttonStyle: handlers, cardsAfterInstall: after, dreamAfterInstall: dreamState,
    pass: before > 0 && after === 0 };
}

async function defectUserModel(port) {
  const p = port || 8731;
  // The sample fetch is the loser: it lands 9 s late, long after the drop below.
  const init = `(() => {
    const real = window.fetch;
    window.fetch = function (input, opts) {
      const url = typeof input === 'string' ? input : (input && input.url) || '';
      if (url.indexOf('sample-export.json') >= 0) {
        return new Promise((resolve, reject) => {
          setTimeout(() => { real.call(window, input, opts).then(resolve, reject); }, 9000);
        });
      }
      return real.call(window, input, opts);
    };
  })();`;
  const tab = await d25Open(p, `http://127.0.0.1:${p}/index.html?hour=17&cb=d25c${Date.now()}`, { name: 'd25-user', init });
  // Drop before boot resolves: no first frame yet, so wait a moment only.
  await d25settle(900);
  const early = await tab.evaluate(`(() => { try { return window.localtime.loadExport(${D25_EXPORT}); } catch (e) { return { error: String(e && e.message) }; } })()`);
  await d25Ready(tab, 30000);
  // Past the delayed sample deadline: boot's own install would now overwrite a lost drop.
  await d25settle(12000);
  const quarters = await tab.evaluate(`(() => {
    const L = window.localtime;
    if (!L || !L.model) return null;
    return { count: L.model.quarters.length, names: L.model.quarters.map((q) => q.name), frames: L.model.frames.length };
  })()`);
  await tab.close();
  const names = quarters ? quarters.names.join(' | ') : '';
  return { earlyLoad: early && early.ok ? 'ok' : early, quarters: quarters && quarters.count, names,
    pass: !!quarters && quarters.names.indexOf('Delta Editor') >= 0 && quarters.names.indexOf('Gamma Terminal') >= 0 };
}

async function defectShadow(port) {
  const p = port || 8731;
  const init = `(() => {
    window.__tex = { created: 0, deleted: 0, live: 0 };
    for (const name of ['WebGLRenderingContext', 'WebGL2RenderingContext']) {
      const C = window[name];
      if (!C || !C.prototype || !C.prototype.createTexture) continue;
      const P = C.prototype;
      const create = P.createTexture;
      const del = P.deleteTexture;
      P.createTexture = function () { window.__tex.created += 1; window.__tex.live += 1; return create.apply(this, arguments); };
      P.deleteTexture = function (t) { window.__tex.deleted += 1; window.__tex.live -= 1; return del.apply(this, arguments); };
    }
  })();`;
  const tab = await d25Open(p, `http://127.0.0.1:${p}/index.html?hour=17&cb=d25d${Date.now()}`, { name: 'd25-shadow', init });
  await d25Ready(tab, 30000);
  await d25settle(4500);
  const series = [];
  series.push(await tab.evaluate(`JSON.stringify(window.__tex)`));
  for (let i = 0; i < 3; i += 1) {
    await tab.evaluate(`(() => { window.localtime.loadExport(${D25_EXPORT}); return true; })()`);
    await d25settle(3000);
    series.push(await tab.evaluate(`JSON.stringify(window.__tex)`));
  }
  const shadowMapLive = await tab.evaluate(`(() => { const s = window.localtime.world.lights.sun; return s && s.shadow ? !!s.shadow.map : null; })()`);
  await tab.close();
  const points = series.map((s) => JSON.parse(s));
  const live = points.map((s) => s.live);
  const growth = live[live.length - 1] - live[0];
  return { live, created: points[points.length - 1].created, deleted: points[points.length - 1].deleted,
    shadowMapLive, growth, pass: growth <= 4 };
}

async function runDefects25Page(port) {
  const p = port || 8731;
  const cards = await defectCards(p);
  const user = await defectUserModel(p);
  const shadow = await defectShadow(p);
  const pass = cards.pass && user.pass && shadow.pass;
  return { cards, user, shadow, pass };
}
