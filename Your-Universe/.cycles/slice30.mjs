// Cycle 30: the wake window's arithmetic, run from the shipped source text with no page.
//
// The resync block is sliced out of src/agents.mjs verbatim and evaluated against stub pool
// variables, so this exercises the code that ships rather than a copy of its formula.
//
//   node .cycles/slice30.mjs
//
// It prints the per-frame entrances for three cases and exits non-zero if any invariant fails:
// frame 1 carries work, frame 30 carries work, the window closes on frame 30, every pool reaches
// its target, and a target that moves every frame is still closed by the hard stop.
import { readFileSync } from 'node:fs';

const src = readFileSync(new URL('../src/agents.mjs', import.meta.url), 'utf8');
const a = src.indexOf('  const RESYNC_FRAMES = 30;');
const b = src.indexOf('  // ---------------------------------------------------------------- scratch');
if (a < 0 || b < 0 || b <= a) throw new Error('resync block not found');
const block = src.slice(a, b);

const harness = `
  const PED_CAP = 700;
  function easeTo(cur, target, rate, dt) { const k = dt * rate; return cur + (target - cur) * (k > 1 ? 1 : k); }
  let ringShown = 0, aveShown = 0, spurShown = 0, expShown = 0, busShown = 0, pedShown = 0;
  let dreamPrev = false;
  ${block}
  return {
    get: () => [ringShown, aveShown, spurShown, expShown, busShown, pedShown],
    set: (v) => { ringShown = v[0]; aveShown = v[1]; spurShown = v[2]; expShown = v[3]; busShown = v[4]; pedShown = v[5]; },
    captureResyncSpans, beginResync, rampSlots, endResyncFrame, reseedReport,
    setDream: (v) => { dreamPrev = v; },
    RESYNC_RING, RESYNC_AVE, RESYNC_SPUR, RESYNC_EXP, RESYNC_BUS, RESYNC_PED,
    RESYNC_POOLS, RESYNC_FRAMES, RESYNC_MAX_FRAMES
  };
`;
const make = new Function(harness);
const H = make();
const P = [H.RESYNC_RING, H.RESYNC_AVE, H.RESYNC_SPUR, H.RESYNC_EXP, H.RESYNC_BUS, H.RESYNC_PED];

function run(label, spansAtSleep, targetOf, expectFrame) {
  H.set(spansAtSleep.slice());
  H.captureResyncSpans();
  H.beginResync('wake');
  const frames = [];
  for (let f = 1; f <= H.RESYNC_MAX_FRAMES + 5; f += 1) {
    const targets = targetOf(f);
    let slots = H.get();
    for (let p = 0; p < H.RESYNC_POOLS; p += 1) slots[p] = H.rampSlots(p, slots[p], targets[p], 1 / 180);
    H.set(slots);
    H.endResyncFrame();
    const r = H.reseedReport;
    frames.push({ f, sum: r.admittedSum, ad: Array.from(r.admitted), open: r.open, shown: Array.from(slots) });
    if (!r.open) break;
  }
  const last = frames[frames.length - 1];
  const r = H.reseedReport;
  const totals = Array.from(r.admittedTotal);
  const spans = Array.from(r.span);
  const tgt = targetOf(last.f);
  const ok = [];
  if (expectFrame === 'hard-stop') {
    ok.push(['closed inside the hard stop', last.f >= H.RESYNC_FRAMES && last.f <= H.RESYNC_MAX_FRAMES, last.f]);
  } else {
    ok.push(['closes on frame 30', last.f === 30, last.f]);
  }
  ok.push(['frame 1 carries work', frames[0].ad.reduce((x, y) => x + y, 0) > 0, frames[0].ad.join(',')]);
  ok.push(['frame 30 carries work', last.ad.reduce((x, y) => x + y, 0) > 0, last.ad.join(',')]);
  const reached = P.every((p) => Math.abs(tgt[p] - last.shown[p]) <= 1);
  ok.push(['every pool reaches its target', reached, last.shown.map((v) => Math.round(v * 100) / 100).join(',') + ' vs ' + tgt.join(',')]);
  console.log(`\n${label}`);
  console.log(`  span at sleep ${spans.map((v) => Math.round(v * 100) / 100).join(', ')}`);
  console.log(`  step         ${Array.from(r.step).map((v) => Math.round(v * 1000) / 1000).join(', ')}`);
  console.log(`  totals       ${totals.join(', ')}`);
  console.log(`  first        f${frames[0].f} ad [${frames[0].ad.join(' ')}] cum ${frames[0].sum}`);
  const mid = frames.find((x) => x.f === 15);
  if (mid) console.log(`  mid          f15 ad [${mid.ad.join(' ')}] cum ${mid.sum}`);
  console.log(`  last         f${last.f} ad [${last.ad.join(' ')}] cum ${last.sum}`);
  for (const [name, pass, detail] of ok) console.log(`  ${pass ? 'PASS' : 'FAIL'} ${name} (${detail})`);
  return ok.every((x) => x[1]);
}

// A sleep under an open window: the window must not count frames while nothing steps, a rebuild
// issued under the dream must open nothing, and the wake must then run its own full window.
function runDreamPause() {
  H.setDream(false);
  H.set([120, 30, 6, 10, 2, 200]);
  H.captureResyncSpans();
  H.beginResync('wake');
  const targets = [120, 30, 6, 10, 2, 200];
  const stepAwake = () => {
    const slots = H.get();
    for (let p = 0; p < H.RESYNC_POOLS; p += 1) slots[p] = H.rampSlots(p, slots[p], targets[p], 1 / 180);
    H.set(slots);
    H.endResyncFrame();
  };
  for (let f = 0; f < 6; f += 1) stepAwake();
  const atSleep = { frames: H.reseedReport.frames, open: H.reseedReport.open, windows: H.reseedReport.windows };
  H.setDream(true);
  // Ten dream frames: update calls no ramp and no endResyncFrame while dreaming.
  for (let f = 0; f < 10; f += 1) { /* nothing steps under a dream */ }
  // A data model change under the dream asks for a window and must be refused.
  H.beginResync('rebuild');
  const duringDream = { frames: H.reseedReport.frames, open: H.reseedReport.open, windows: H.reseedReport.windows };
  H.setDream(false);
  H.beginResync('wake');
  const atWake = { frames: H.reseedReport.frames, open: H.reseedReport.open, windows: H.reseedReport.windows };
  for (let f = 0; f < 30; f += 1) stepAwake();
  const end = { frames: H.reseedReport.frames, open: H.reseedReport.open, last: H.reseedReport.lastFrames, windows: H.reseedReport.windows };
  const shown = H.get();
  const ok = [];
  ok.push(['no frames counted during the dream', duringDream.frames === atSleep.frames, duringDream.frames + ' vs ' + atSleep.frames]);
  ok.push(['the window stayed open across the dream', duringDream.open === true && atSleep.open === true, atSleep.open + '/' + duringDream.open]);
  ok.push(['a rebuild under the dream opened nothing', duringDream.windows === atSleep.windows, duringDream.windows + ' vs ' + atSleep.windows]);
  ok.push(['the wake opened one fresh window', atWake.windows === atSleep.windows + 1 && atWake.frames === 0, 'windows ' + atWake.windows + ' frames ' + atWake.frames]);
  ok.push(['the fresh window closes on awake frame 30', end.open === false && end.last === 30, 'last ' + end.last]);
  ok.push(['pools reach their targets', targets.every((t, p) => Math.abs(t - shown[p]) <= 1), shown.map((v) => Math.round(v * 100) / 100).join(',')]);
  console.log('\nSleep under an open window, then a rebuild under the dream, then the wake');
  console.log('  at sleep      frames ' + atSleep.frames + ' open ' + atSleep.open + ' windows ' + atSleep.windows);
  console.log('  after dream   frames ' + duringDream.frames + ' open ' + duringDream.open + ' windows ' + duringDream.windows);
  console.log('  at wake       frames ' + atWake.frames + ' open ' + atWake.open + ' windows ' + atWake.windows);
  console.log('  after 30      frames ' + end.frames + ' open ' + end.open + ' last ' + end.last);
  for (const [name, pass, detail] of ok) console.log('  ' + (pass ? 'PASS' : 'FAIL') + ' ' + name + ' (' + detail + ')');
  return ok.every((x) => x[1]);
}

let allOk = true;
// The desktop profile measured on the page: ring 10, ave 76.87, spur 4, exp 8, bus 2, peds 85.71.
allOk = run('desktop shape, targets [100, 24, 14, 35, 2, 134]',
  [10, 76.87285614013672, 4, 8, 2, 85.71435546875],
  () => [100, 24, 14, 35, 2, 134]) && allOk;
// A full seven hundred walkers and one bus: both must still take thirty frames.
allOk = run('full pool and a single bus, targets [700, 0, 0, 0, 1, 700]',
  [700, 0, 0, 0, 1, 700],
  () => [700, 0, 0, 0, 1, 700]) && allOk;
// A target that moves every frame: congestion during the window. The hard stop must close it.
allOk = run('moving target, closed by the hard stop',
  [300, 40, 10, 20, 2, 400],
  (f) => [200 + ((f * 37) % 200), 40, 10, 20, 2, 400], 'hard-stop') && allOk;
allOk = runDreamPause() && allOk;
console.log(`\nslice30: ${allOk ? 'PASS' : 'FAIL'}`);
process.exit(allOk ? 0 : 1);
