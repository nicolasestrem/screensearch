// Cycle 29: prove that the tab is running the served source, not a cached one. The service
// worker serves index.html network-first but `assets/` and `sample-export.json` cache-first, so a
// stale cache can silently measure an older build (the pixel-metric trap of cycle 29).
//
//   const h = await servedCheck(8731);
//   -> { servedSha, diskSha, match, marker, swController, cacheKeys, cacheEntries }
//
const sc29 = (ms) => new Promise((r) => setTimeout(r, ms));

async function servedCheck(port, opts) {
  const o = opts || {};
  const p = port || 8731;
  const tab = await browser.open({ name: 'sc29', url: 'about:blank', viewport: { width: 800, height: 600 }, timeout: 180 });
  await tab.goto(`http://127.0.0.1:${p}/index.html?hour=22&cb=sc29${Date.now()}`);
  await sc29(6000);
  const served = JSON.parse(await tab.evaluate(`(async () => {
    const r = await fetch('index.html', { cache: 'no-store' });
    const text = await r.text();
    const buf = new TextEncoder().encode(text);
    const d = await crypto.subtle.digest('SHA-256', buf);
    const sha = Array.from(new Uint8Array(d)).map((b) => b.toString(16).padStart(2, '0')).join('');
    const regs = await navigator.serviceWorker.getRegistrations();
    const keys = await caches.keys();
    let entries = 0;
    for (const k of keys) { const c = await caches.open(k); entries += (await c.keys()).length; }
    return JSON.stringify({
      bytes: buf.length, sha, controller: !!navigator.serviceWorker.controller, regs: regs.length, keys, entries,
      marker: text.indexOf('CYCLE29') >= 0, dreamWarm: text.indexOf('dream-warm') >= 0,
    });
  })()`));
  await tab.close();
  const disk = await read('C:/Users/nicol/Documents/GitHub/screensearch/Your-Universe/index.html');
  const diskBuf = new TextEncoder().encode(disk);
  const dh = await crypto.subtle.digest('SHA-256', diskBuf);
  const diskSha = Array.from(new Uint8Array(dh)).map((b) => b.toString(16).padStart(2, '0')).join('');
  return {
    servedBytes: served.bytes,
    servedSha: served.sha,
    diskBytes: diskBuf.length,
    diskSha,
    match: served.sha === diskSha && served.bytes === diskBuf.length,
    marker: served.marker,
    swController: served.controller,
    cacheKeys: served.keys,
    cacheEntries: served.entries,
  };
}
