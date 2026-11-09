// smoke-data.mjs: verifies src/data.mjs against a synthetic export whose answers
// are known by construction. Pure Node, no test framework. Prints one PASS or
// FAIL line per assertion and exits nonzero when anything fails.

import { readFileSync } from 'node:fs';
import { analyzeExport } from '../src/data.mjs';

const MIN = 60 * 1000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const BASE = Date.UTC(2026, 10, 5, 0, 0, 0, 0);

let pass = 0;
let fail = 0;

function check(name, ok, detail) {
  if (ok) {
    pass += 1;
    console.log('PASS  ' + name);
  } else {
    fail += 1;
    let extra = '';
    if (detail !== undefined) extra = '  [got: ' + String(detail) + ']';
    console.log('FAIL  ' + name + extra);
  }
}

function eq(name, actual, expected) {
  check(name, JSON.stringify(actual) === JSON.stringify(expected), JSON.stringify(actual));
}

function frame(id, atMs, app, title, text) {
  return {
    frame_id: id,
    captured_at: atMs,
    app_hint: app,
    window_title: title,
    browser_url: null,
    activity_type: null,
    content_text: text
  };
}

function exportOf(frames) {
  return {
    schema: 'screensearch.export.v1',
    exported_at: BASE,
    from: null,
    to: null,
    frames: frames,
    marks: []
  };
}

// Independent recomputation of the switch total, mirroring only the documented
// frame normalisation (null or blank app becomes Unknown, missing stamps are
// dropped). It must equal the sum of windows10[].switches for any export.
function normApp(value) {
  if (typeof value !== 'string') return 'Unknown';
  const trimmed = value.trim();
  return trimmed === '' ? 'Unknown' : trimmed;
}

function countRawChanges(exportFrames) {
  const list = [];
  for (let i = 0; i < exportFrames.length; i++) {
    const raw = exportFrames[i];
    if (!raw || typeof raw !== 'object') continue;
    const at = Number(raw.captured_at);
    if (!Number.isFinite(at)) continue;
    list.push({ t: at, app: normApp(raw.app_hint) });
  }
  list.sort(function (a, b) { return a.t - b.t; });
  let changes = 0;
  for (let i = 1; i < list.length; i++) {
    if (list[i].app !== list[i - 1].app) changes += 1;
  }
  return changes;
}

// ---------------------------------------------------------------------------
// Main fixture. Nine apps, one day, every derived value chosen on purpose:
//   Alpha   0..180 min, one frame a minute, a 3 h session, titles main.rs at
//           minute 10 and minute 100 (two distinct hours, a tram stop).
//   Code/Term  200..209.83 min alternating, exactly 40 switches in window 20.
//   Zeta 236 min and Chi 243 min: opened once each.
//   Beta 290..299 and Gamma 301..309 share three salient words and never share
//           an hour, plus the parcel phrase at 299 -> 301.
//   Delta 620..625 and Epsilon 640..645 share two words inside hour 10, so
//           sameHour is true and dreamPair must skip them.
//   Gamma -> Delta is the single gap over 4 h (11 min past 5 h).
// ---------------------------------------------------------------------------
const mainFrames = [];
let fid = 1;

for (let m = 0; m <= 180; m++) {
  const title = (m === 10 || m === 100) ? 'main.rs' : 'alpha-h' + Math.floor(m / 60) + '.md';
  mainFrames.push(frame(fid++, BASE + m * MIN, 'Alpha', title, 'Riverside workshop ledger'));
}

for (let i = 0; i <= 40; i++) {
  const at = BASE + 200 * MIN + i * 14750;
  if (i % 2 === 0) mainFrames.push(frame(fid++, at, 'Code', 'code.log', 'Compile bundle watcher'));
  else mainFrames.push(frame(fid++, at, 'Term', 'term.log', 'Terminal cursor blink'));
}

mainFrames.push(frame(fid++, BASE + 236 * MIN, 'Zeta', 'zeta.txt', 'Swift visit notes'));
mainFrames.push(frame(fid++, BASE + 243 * MIN, 'Chi', 'chi.txt', 'Short glance'));

for (let m = 290; m <= 299; m++) {
  mainFrames.push(frame(fid++, BASE + m * MIN, 'Beta', 'beta.xlsx', 'Quarterly warehouse invoice'));
}
for (let m = 301; m <= 309; m++) {
  mainFrames.push(frame(fid++, BASE + m * MIN, 'Gamma', 'gamma.xlsx', 'Quarterly warehouse invoice'));
}
for (let m = 620; m <= 625; m++) {
  mainFrames.push(frame(fid++, BASE + m * MIN, 'Delta', 'delta.csv', 'Budget forecast'));
}
for (let m = 640; m <= 645; m++) {
  mainFrames.push(frame(fid++, BASE + m * MIN, 'Epsilon', 'epsilon.csv', 'Budget forecast'));
}

const mainExport = exportOf(mainFrames);
const model = analyzeExport(mainExport, { tzOffsetMs: 0 });
const names = model.quarters.map(function (q) { return q.name; });
const betaIdx = model.quarterOf.get('Beta');
const gammaIdx = model.quarterOf.get('Gamma');

console.log('-- main fixture --');
check('main: 255 frames survive normalisation', model.frames.length === 255, model.frames.length);
check('main: frames are sorted', model.frames.every(function (f, i) { return i === 0 || f.t >= model.frames[i - 1].t; }));
check('main: frames normalised into id/t/app/title/url/text', model.frames.every(function (f) {
  return Object.keys(f).join(',') === 'id,t,app,title,url,text';
}));
check('main: time range is t0 to t1', model.t0 === BASE && model.t1 === BASE + 645 * MIN);
check('main: explicit tzOffsetMs is kept', model.tzOffsetMs === 0);

check('main: nine quarters', model.quarters.length === 9, model.quarters.length);
check('main: nine apps known to quarterOf', model.quarterOf.size === 9, model.quarterOf.size);
check('main: rank 0 is the three hour app', names[0] === 'Alpha', names.join(','));
eq('main: every app got a quarter', names.slice().sort(), ['Alpha', 'Beta', 'Chi', 'Code', 'Delta', 'Epsilon', 'Gamma', 'Term', 'Zeta']);
check('main: shares sum to one', Math.abs(model.quarters.reduce(function (s, q) { return s + q.share; }, 0) - 1) < 1e-9);
check('main: top quarter has 40 buildings', model.quarters[0].buildingCount === 40, model.quarters[0].buildingCount);
check('main: building counts inside 6..40', model.quarters.every(function (q) { return q.buildingCount >= 6 && q.buildingCount <= 40; }));
check('main: building counts never rise with rank', model.quarters.every(function (q, i) {
  return i === 0 || q.buildingCount <= model.quarters[i - 1].buildingCount;
}));
eq('main: tower counts fall 3/2/1 then zero', model.quarters.map(function (q) { return q.towerCount; }), [3, 2, 1, 0, 0, 0, 0, 0, 0]);
check('main: nothing is neglected with the data-relative default now', model.quarters.every(function (q) { return q.neglected === false; }));
check('main: an explicit now 10 days later neglects every quarter',
  analyzeExport(mainExport, { tzOffsetMs: 0, now: BASE + 10 * DAY }).quarters.every(function (q) { return q.neglected === true; }));

// Sessions -------------------------------------------------------------------
const longest = model.longestSessionBefore(BASE + 205 * MIN);
check('main: 48 sessions', model.sessions.length === 48, model.sessions.length);
check('main: the 3 h focus block is one session of 181 frames', !!longest && longest.frameCount === 181, longest && longest.frameCount);
check('main: its duration is 3.025 h', !!longest && longest.durationMs === 10890000, longest && longest.durationMs);
check('main: the longest session belongs to quarter 0', !!longest && longest.q === 0);
check('main: longestSessionBefore before the first frame is null', model.longestSessionBefore(BASE - 1) === null);
const insideBlock = model.activeSessionAt(BASE + 90 * MIN);
check('main: activeSessionAt inside the focus block finds it', !!insideBlock && insideBlock.q === 0 && insideBlock.frameCount === 181);
check('main: activeSessionAt in the five hour absence is null', model.activeSessionAt(BASE + 450 * MIN) === null);
check('main: session q values are quarter indices', model.sessions.every(function (s) { return s.q >= 0 && s.q < 9; }));

// Windows and rates ----------------------------------------------------------
check('main: 65 ten-minute windows cover the day', model.windows10.length === 65, model.windows10.length);
check('main: window 20 starts at 200 minutes', model.windows10[20].start === BASE + 200 * MIN);
const mainRawChanges = countRawChanges(mainFrames);
const mainWindowSwitches = model.windows10.reduce(function (s, w) { return s + w.switches; }, 0);
check('main: the independent raw change count is 47', mainRawChanges === 47, mainRawChanges);
check('main: window switches equal every app change, repeats included', mainWindowSwitches === mainRawChanges, mainWindowSwitches + ' vs ' + mainRawChanges);
check('main: window 20 holds the forty alternations plus the entry from Alpha',
  model.windows10[20].switches === 41, model.windows10[20].switches);
const mainPeak = Math.max.apply(null, model.windows10.map(function (w) { return w.switches; }));
check('main: 41 is the busiest window of the day', mainPeak === 41, mainPeak);
check('main: switchRateAt clamps the busiest window to 1', model.switchRateAt(BASE + 205 * MIN) === 1, model.switchRateAt(BASE + 205 * MIN));
check('main: switchRateAt at the end of the burst is still 1', model.switchRateAt(BASE + 209 * MIN) === 1);
check('main: switchRateAt is 0 inside the focus block', model.switchRateAt(BASE + 90 * MIN) === 0, model.switchRateAt(BASE + 90 * MIN));
let steadyPeak = 0;
for (let w = 0; w < 20; w++) {
  const steadyRate = model.switchRateAt(model.windows10[w].start + MIN);
  if (steadyRate > steadyPeak) steadyPeak = steadyRate;
}
check('main: the whole steady stretch reads 0 congestion', steadyPeak === 0, steadyPeak);
check('main: fragmentationAt is 1 across the fragmented hour', model.fragmentationAt(BASE + 205 * MIN) === 1, model.fragmentationAt(BASE + 205 * MIN));
check('main: fragmentationAt is 0 in the focus block', model.fragmentationAt(BASE + 90 * MIN) === 0, model.fragmentationAt(BASE + 90 * MIN));
check('main: windows carry one activity slot per quarter', model.windows10.every(function (w) { return w.activity.length === 9; }));

// Activity -------------------------------------------------------------------
const act = model.activityLastHour(BASE + 170 * MIN);
check('main: activityLastHour returns a Float32Array of 9', act instanceof Float32Array && act.length === 9);
check('main: the focus quarter fills its last hour', act[0] === 1, act[0]);
check('main: every other quarter is dark in that hour', Array.prototype.slice.call(act, 1).every(function (v) { return v === 0; }), Array.prototype.join.call(act, ','));

// Cranes ---------------------------------------------------------------------
check('main: no crane before 45 minutes', model.cranesAt(BASE + 40 * MIN).length === 0);
const crane50 = model.cranesAt(BASE + 50 * MIN);
check('main: one crane at 50 minutes on the focus quarter, one floor',
  crane50.length === 1 && crane50[0].q === 0 && crane50[0].floors === 1, JSON.stringify(crane50));
check('main: crane has two floors at 56 minutes', model.cranesAt(BASE + 56 * MIN)[0].floors === 2, model.cranesAt(BASE + 56 * MIN)[0].floors);
check('main: crane clamps at 12 floors by 180 minutes', model.cranesAt(BASE + 180 * MIN)[0].floors === 12, model.cranesAt(BASE + 180 * MIN)[0].floors);
check('main: no crane once the long session is over', model.cranesAt(BASE + 400 * MIN).length === 0);

// Gaps -----------------------------------------------------------------------
check('main: exactly one gap over 4 h', model.gaps.length === 1, JSON.stringify(model.gaps));
check('main: the gap runs from 309 to 620 minutes',
  model.gaps[0].start === BASE + 309 * MIN && model.gaps[0].end === BASE + 620 * MIN);

// Stops ----------------------------------------------------------------------
check('main: one tram stop', model.stops.length === 1, JSON.stringify(model.stops));
check('main: main.rs is the stop, seen in two hours by Alpha',
  model.stops[0].title === 'main.rs' && model.stops[0].hours === 2 &&
  model.stops[0].visits === 2 && model.stops[0].q === 0);

// Phrases, pairs, parcels ----------------------------------------------------
check('main: Alpha keeps its phrase', model.quarters[0].phrases.indexOf('Riverside workshop ledger') >= 0, model.quarters[0].phrases.join(' | '));
check('main: lastPhrase is Alpha\'s most recent phrase', model.quarters[0].lastPhrase === 'Riverside workshop ledger');
check('main: phrases are chronological', model.phrases.every(function (p, i) { return i === 0 || p.at >= model.phrases[i - 1].at; }));
check('main: every phrase has a quarter and a timestamp', model.phrases.every(function (p) { return p.q >= 0 && p.q < 9 && Number.isFinite(p.at); }));

check('main: two pairs share salient words', model.pairs.length === 2, model.pairs.map(function (p) { return p.words.join('+'); }).join(' ; '));
const sharedPair = model.pairs.filter(function (p) { return p.words.length === 3; })[0];
check('main: Beta and Gamma share the three planted words',
  !!sharedPair && sharedPair.words.join(',') === 'invoice,quarterly,warehouse', sharedPair && sharedPair.words.join(','));
check('main: Beta and Gamma never share an hour', !!sharedPair && sharedPair.sameHour === false);
const sameHourPair = model.pairs.filter(function (p) { return p.words.length === 2; })[0];
check('main: Delta and Epsilon share budget and forecast',
  !!sameHourPair && sameHourPair.words.join(',') === 'budget,forecast', sameHourPair && sameHourPair.words.join(','));
check('main: Delta and Epsilon do share an hour', !!sameHourPair && sameHourPair.sameHour === true);
check('main: the stronger pair is Beta and Gamma', !!sharedPair && !!sameHourPair && sharedPair.strength > sameHourPair.strength,
  sharedPair && sameHourPair && (sharedPair.strength + ' vs ' + sameHourPair.strength));
check('main: dreamPair is the strongest pair that never shares an hour', model.dreamPair === sharedPair);
check('main: dreamPair words are the planted three', !!model.dreamPair && model.dreamPair.words.join(',') === 'invoice,quarterly,warehouse');

check('main: exactly one parcel', model.parcels.length === 1, JSON.stringify(model.parcels));
const parcel = model.parcels[0];
check('main: the parcel carries the shared phrase Beta to Gamma',
  !!parcel && parcel.phrase === 'Quarterly warehouse invoice' && parcel.from === betaIdx && parcel.to === gammaIdx,
  parcel && (parcel.phrase + ' ' + parcel.from + '->' + parcel.to));
check('main: the parcel lands at 301 minutes', !!parcel && parcel.at === BASE + 301 * MIN, parcel && parcel.at);
check('main: the parcel crosses two quarters within ten minutes',
  !!parcel && parcel.from !== parcel.to && parcel.at - (BASE + 299 * MIN) <= 10 * MIN);
check('main: pair indices are ordered a < b', model.pairs.every(function (p) { return p.a < p.b; }));

// ---------------------------------------------------------------------------
// Empty input
// ---------------------------------------------------------------------------
console.log('-- edge cases --');
const empty = analyzeExport({ schema: 'screensearch.export.v1', frames: [], marks: [] }, { tzOffsetMs: 0 });
check('empty: zero quarters', empty.quarters.length === 0);
check('empty: t0 equals t1 and is finite', empty.t0 === empty.t1 && Number.isFinite(empty.t0));
check('empty: safe scalar queries', empty.switchRateAt(0) === 0 && empty.fragmentationAt(0) === 0 &&
  empty.activeSessionAt(0) === null && empty.longestSessionBefore(0) === null);
check('empty: activityLastHour is an empty Float32Array',
  empty.activityLastHour(0) instanceof Float32Array && empty.activityLastHour(0).length === 0);
check('empty: cranesAt is an empty array', empty.cranesAt(0).length === 0);
check('empty: every derived list is empty',
  empty.windows10.length === 0 && empty.stops.length === 0 && empty.pairs.length === 0 &&
  empty.parcels.length === 0 && empty.gaps.length === 0 && empty.phrases.length === 0 &&
  empty.sessions.length === 0 && empty.dreamPair === null);
check('empty: a missing frames key is tolerated', analyzeExport({}, { tzOffsetMs: 0 }).quarters.length === 0);
check('empty: a null export is tolerated', analyzeExport(null).quarters.length === 0);

// ---------------------------------------------------------------------------
// Nulls everywhere, unsorted
// ---------------------------------------------------------------------------
const nullOrder = [2, 0, 4, 1, 3];
const nullFrames = nullOrder.map(function (k) {
  return {
    frame_id: k + 1,
    captured_at: BASE + k * MIN,
    app_hint: null,
    window_title: null,
    browser_url: null,
    activity_type: null,
    content_text: null
  };
});
const nullModel = analyzeExport(exportOf(nullFrames), { tzOffsetMs: 0 });
check('nulls: one Unknown quarter', nullModel.quarters.length === 1 && nullModel.quarters[0].name === 'Unknown', nullModel.quarters.length && nullModel.quarters[0].name);
check('nulls: frames default to Unknown, empty strings and null url', nullModel.frames.every(function (f) {
  return f.app === 'Unknown' && f.title === '' && f.url === null && f.text === '';
}));
check('nulls: frames are sorted', nullModel.frames.every(function (f, i) { return i === 0 || f.t >= nullModel.frames[i - 1].t; }));
check('nulls: one session and one window', nullModel.sessions.length === 1 && nullModel.windows10.length === 1);
check('nulls: no thresholds trip', nullModel.stops.length === 0 && nullModel.pairs.length === 0 &&
  nullModel.parcels.length === 0 && nullModel.gaps.length === 0);

// ---------------------------------------------------------------------------
// Unsorted main fixture
// ---------------------------------------------------------------------------
const reversedFrames = mainFrames.slice().reverse();
const reversedHeadAt = reversedFrames[0].captured_at;
const reversedFrameCount = reversedFrames.length;
const shuffled = analyzeExport(exportOf(reversedFrames), { tzOffsetMs: 0 });
check('unsorted: the input frame array is never mutated',
  reversedFrames.length === reversedFrameCount && reversedFrames[0].captured_at === reversedHeadAt);
eq('unsorted: same quarter order', shuffled.quarters.map(function (q) { return q.name; }), names);
check('unsorted: same session count', shuffled.sessions.length === model.sessions.length);
check('unsorted: same time range', shuffled.t0 === model.t0 && shuffled.t1 === model.t1);
const totalSwitches = function (m) { return m.windows10.reduce(function (s, w) { return s + w.switches; }, 0); };
check('unsorted: same switch total', totalSwitches(shuffled) === totalSwitches(model), totalSwitches(shuffled) + ' vs ' + totalSwitches(model));
check('unsorted: same dreamPair', !!shuffled.dreamPair && shuffled.dreamPair.words.join(',') === model.dreamPair.words.join(','));

// ---------------------------------------------------------------------------
// The 90 s session boundary
// ---------------------------------------------------------------------------
const B = BASE + 5 * DAY;
function boundaryModel(count) {
  const offsets = [0, 30000, 119999, 209999].slice(0, count);
  const frames = offsets.map(function (o, i) { return frame(i + 1, B + o, 'Solo', 'solo.txt', 'Steady solo work'); });
  return analyzeExport(exportOf(frames), { tzOffsetMs: 0 });
}
const keepOne = boundaryModel(3);
check('boundary: an 89.999 s gap keeps one session', keepOne.sessions.length === 1, keepOne.sessions.length);
const splitTwo = boundaryModel(4);
check('boundary: a 90.000 s gap splits the session', splitTwo.sessions.length === 2, splitTwo.sessions.length);
check('boundary: the first session ends at the split point',
  splitTwo.sessions[0].end === B + 209999 && splitTwo.sessions[1].start === B + 209999,
  splitTwo.sessions[0].end - B);

// ---------------------------------------------------------------------------
// More than nine apps
// ---------------------------------------------------------------------------
const manyFrames = [];
let manyId = 1;
for (let a = 0; a < 12; a++) {
  const count = 24 - a;
  for (let k = 0; k < count; k++) {
    manyFrames.push(frame(manyId++, BASE + (a * 30 + k * 0.5) * MIN, 'App' + a, 'app' + a + '.txt', null));
  }
}
const many = analyzeExport(exportOf(manyFrames), { tzOffsetMs: 0 });
check('12 apps: only the top nine become quarters', many.quarters.length === 9, many.quarters.length);
check('12 apps: quarterOf knows all twelve names', many.quarterOf.size === 12 && many.quarterOf.get('App0') === 0, many.quarterOf.size);
check('12 apps: an app beyond the top nine maps to undefined', many.quarterOf.get('App11') === undefined);
check('12 apps: excluded apps show up as q = -1', many.sessions.some(function (s) { return s.q === -1; }));
check('12 apps: every session q is a quarter index or -1', many.sessions.every(function (s) { return s.q === -1 || (s.q >= 0 && s.q < 9); }));
check('12 apps: the top quarter is App0 with 40 buildings', many.quarters[0].name === 'App0' && many.quarters[0].buildingCount === 40);

// ---------------------------------------------------------------------------
// Stops spread over quarters: at most two each, never padded
// ---------------------------------------------------------------------------
const spreadFrames = [];
let spreadId = 1;
function addTwoHourTitle(app, title, hourA, hourB) {
  spreadFrames.push(frame(spreadId++, BASE + hourA * HOUR, app, title, 'Quiet spread sample'));
  spreadFrames.push(frame(spreadId++, BASE + hourB * HOUR, app, title, 'Quiet spread sample'));
}
addTwoHourTitle('Busy', 'b1.md', 9, 10);
addTwoHourTitle('Busy', 'b2.md', 9, 10);
addTwoHourTitle('Busy', 'b3.md', 9, 10);
addTwoHourTitle('Busy', 'b4.md', 9, 10);
for (let i = 1; i <= 6; i++) addTwoHourTitle('Q' + i, 'q' + i + '.md', 10 + i, 11 + i);

const spreadModel = analyzeExport(exportOf(spreadFrames), { tzOffsetMs: 0 });
const spreadCount = new Map();
spreadModel.stops.forEach(function (s) { spreadCount.set(s.q, (spreadCount.get(s.q) || 0) + 1); });
let spreadMax = 0;
spreadCount.forEach(function (count) { if (count > spreadMax) spreadMax = count; });
check('stops: a quarter never contributes more than two stops', spreadMax === 2, spreadMax);
check('stops: seven qualifying quarters fill the eight slots', spreadModel.stops.length === 8, spreadModel.stops.length);
check('stops: seven distinct quarters are represented', spreadCount.size === 7, spreadCount.size);
check('stops: the over-subscribed quarter kept its best two',
  spreadModel.stops.filter(function (s) { return spreadModel.quarters[s.q].name === 'Busy'; })
    .map(function (s) { return s.title; }).join(',') === 'b1.md,b2.md');
check('stops: every chosen stop still qualifies on two distinct hours',
  spreadModel.stops.every(function (s) { return s.hours >= 2 && s.visits >= 2; }));
check('stops: a single qualifying quarter still yields one stop', model.stops.length === 1);


// ---------------------------------------------------------------------------
// Two apps: no padding, one hour of data
// ---------------------------------------------------------------------------
const twoModel = analyzeExport(exportOf([
  frame(1, BASE, 'One', 'one.txt', 'First app text'),
  frame(2, BASE + 10 * MIN, 'Two', 'two.txt', 'Second app text')
]), { tzOffsetMs: 0 });
check('two apps: no padding quarters', twoModel.quarters.length === 2, twoModel.quarters.length);

const hourFrames = [];
for (let i = 0; i < 6; i++) {
  hourFrames.push(frame(i + 1, BASE + 2 * DAY + i * MIN, i % 2 ? 'Left' : 'Right', 'h.txt', 'Ping pong'));
}
const hourModel = analyzeExport(exportOf(hourFrames), { tzOffsetMs: 0 });
check('one hour: a single ten-minute window', hourModel.windows10.length === 1, hourModel.windows10.length);
check('one hour: five switches read 0.625', hourModel.switchRateAt(BASE + 2 * DAY + 3 * MIN) === 0.625, hourModel.switchRateAt(BASE + 2 * DAY + 3 * MIN));

const sevenFrames = [];
for (let i = 0; i < 8; i++) {
  sevenFrames.push(frame(i + 1, BASE + 3 * DAY + i * MIN, i % 2 ? 'Pong' : 'Ping', 'r.txt', 'Ping pong rally'));
}
const sevenModel = analyzeExport(exportOf(sevenFrames), { tzOffsetMs: 0 });
const sevenRate = sevenModel.switchRateAt(BASE + 3 * DAY + 4 * MIN);
check('rate: seven switches land in the 0.8..1.0 congestion band', sevenRate >= 0.8 && sevenRate <= 1, sevenRate);
check('rate: seven switches read exactly 0.875', sevenRate === 0.875, sevenRate);

const steadyFrames = [];
for (let i = 0; i < 5; i++) {
  steadyFrames.push(frame(i + 1, BASE + 4 * DAY + i * MIN, 'Solo', 's.txt', 'Solo steady work'));
}
const steadyModel = analyzeExport(exportOf(steadyFrames), { tzOffsetMs: 0 });
check('rate: a steady single-app stretch reads 0 in every window',
  steadyModel.windows10.every(function (w) { return steadyModel.switchRateAt(w.start + 30000) === 0; }),
  steadyModel.windows10.map(function (w) { return steadyModel.switchRateAt(w.start + 30000); }).join(','));

// ---------------------------------------------------------------------------
// Multi-day data: stops across days and neglect
// ---------------------------------------------------------------------------
const multiFrames = [
  frame(1, BASE + 0 * DAY + 9 * HOUR, 'Steady', 'Weekly.md', 'Steady weekly ledger'),
  frame(2, BASE + 0 * DAY + 10 * HOUR, 'Steady', 'Other.md', 'Steady weekly ledger'),
  frame(3, BASE + 1 * DAY + 9 * HOUR, 'Steady', 'Weekly.md', 'Steady weekly ledger'),
  frame(4, BASE + 0 * DAY + 12 * HOUR, 'Legacy', 'Old.md', 'Old archive notes'),
  frame(5, BASE + 4 * DAY + 9 * HOUR, 'Anchor', 'Now.md', 'Anchor present day')
];
const multi = analyzeExport(exportOf(multiFrames), { tzOffsetMs: 0 });
const stopTitles = multi.stops.map(function (s) { return s.title; });
check('multi-day: Weekly.md becomes a tram stop', stopTitles.indexOf('Weekly.md') >= 0, stopTitles.join(','));
check('multi-day: that stop spans two distinct hours',
  multi.stops.filter(function (s) { return s.title === 'Weekly.md'; }).every(function (s) { return s.hours === 2; }));
const legacy = multi.quarters.filter(function (q) { return q.name === 'Legacy'; })[0];
const steady = multi.quarters.filter(function (q) { return q.name === 'Steady'; })[0];
check('multi-day: an app quiet for 3.9 days is neglected', !!legacy && legacy.neglected === true);
check('multi-day: an app active exactly 3 days before t1 is not neglected', !!steady && steady.neglected === false,
  steady && (multi.t1 - steady.lastActiveAt) / DAY);

// ---------------------------------------------------------------------------
// Query methods must not allocate per call
// ---------------------------------------------------------------------------
console.log('-- query reuse --');
const shape = function (m) {
  return [
    m.frames.length, m.quarters.length, m.sessions.length, m.windows10.length,
    m.stops.length, m.phrases.length, m.pairs.length, m.parcels.length, m.gaps.length
  ].join('|');
};
const shapeBefore = shape(model);
const activityRef = model.activityLastHour(BASE + 90 * MIN);
const craneRef = model.cranesAt(BASE + 50 * MIN);
const windowShape = model.windows10.map(function (w) { return w.activity.length; }).join('|');
for (let i = 0; i < 1000; i++) {
  const t = model.t0 + ((i * 37) % 640) * MIN;
  model.switchRateAt(t);
  model.fragmentationAt(t);
  model.activityLastHour(t);
  model.activeSessionAt(t);
  model.cranesAt(t);
  model.longestSessionBefore(t);
}
check('1000 calls: no derived array grew', shape(model) === shapeBefore, shapeBefore + ' vs ' + shape(model));
check('1000 calls: activityLastHour reuses one Float32Array', model.activityLastHour(BASE + 90 * MIN) === activityRef);
check('1000 calls: cranesAt reuses one array', model.cranesAt(BASE + 50 * MIN) === craneRef);
check('1000 calls: window activity arrays untouched', model.windows10.map(function (w) { return w.activity.length; }).join('|') === windowShape);

// ---------------------------------------------------------------------------
// The real sample export, when present: the window sum must equal every raw
// app change, and the busiest window must reach the congestion ceiling.
// ---------------------------------------------------------------------------
console.log('-- sample export --');
let sampleExport = null;
try {
  sampleExport = JSON.parse(readFileSync(new URL('../sample-export.json', import.meta.url), 'utf8'));
} catch (error) {
  console.log('SKIP  sample-export.json unavailable: ' + error.message);
}
if (sampleExport) {
  const sampleModel = analyzeExport(sampleExport, { tzOffsetMs: 0 });
  const sampleRaw = countRawChanges(sampleExport.frames || []);
  const sampleSwitches = sampleModel.windows10.reduce(function (s, w) { return s + w.switches; }, 0);
  check('sample: window switches equal every app change', sampleSwitches === sampleRaw, sampleSwitches + ' vs ' + sampleRaw);
  let peakWindow = sampleModel.windows10[0];
  for (let i = 1; i < sampleModel.windows10.length; i++) {
    if (sampleModel.windows10[i].switches > peakWindow.switches) peakWindow = sampleModel.windows10[i];
  }
  const peakRate = sampleModel.switchRateAt(peakWindow.start + 300000);
  check('sample: the busiest window holds at least eight switches', peakWindow.switches >= 8, peakWindow.switches);
  check('sample: switchRateAt reaches 1.0 at the busiest window', peakRate === 1, peakRate);
  let sampleMin = 2;
  for (let t = sampleModel.t0; t <= sampleModel.t1; t += MIN) {
    const rate = sampleModel.switchRateAt(t);
    if (rate < sampleMin) sampleMin = rate;
  }
  check('sample: the loose stretch of the day still reads 0', sampleMin === 0, sampleMin);
  check('sample: switchRateAt never exceeds 1', peakRate <= 1 && sampleMin >= 0);
  check('sample: the sample still yields quarters', sampleModel.quarters.length > 0, sampleModel.quarters.length);
  console.log('info  sample: ' + sampleModel.frames.length + ' frames, ' + sampleModel.sessions.length +
    ' sessions, ' + sampleSwitches + ' switches, peak window ' + peakWindow.switches +
    ', quarters ' + sampleModel.quarters.map(function (q) { return q.name; }).join(' / '));
}

console.log('');
console.log(pass + ' passed, ' + fail + ' failed');
process.exit(fail === 0 ? 0 : 1);
