// data.mjs: turns a ScreenSearch export into the Model described in CONTRACT.md.
// Pure JS: no DOM, no THREE, no globals. Runs in Node and in the browser bundle.
// One top-level identifier (analyzeExport); every helper lives in the closure.

export function analyzeExport(exportObj, options = {}) {
  // ---------------------------------------------------------------- tuning
  const SPLIT_GAP_MS = 90 * 1000;          // a session splits at gaps >= 90 s
  const SLOT_MAX_MS = 90 * 1000;           // a frame owns at most 90 s of time
  const WINDOW_MS = 10 * 60 * 1000;        // switch-rate window
  const HOUR_MS = 60 * 60 * 1000;
  const DAY_MS = 24 * HOUR_MS;
  const BIG_GAP_MS = 4 * HOUR_MS;          // the dream threshold
  const NEGLECT_MS = 3 * DAY_MS;
  const PARCEL_WINDOW_MS = 10 * 60 * 1000;
  const CRANE_MIN_MS = 45 * 60 * 1000;
  const CRANE_STEP_MS = 10 * 60 * 1000;
  const FLOOR_MAX = 12;
  const MAX_PHRASES_PER_APP = 24;
  const PHRASE_MIN_WORDS = 3;
  const PHRASE_MAX_WORDS = 6;
  const PHRASE_MAX_CHARS = 40;
  // Congestion normalisation. A 10 minute window physically holds about 7
  // switches even in a badly fragmented hour (40 per hour is one every 90 s),
  // so the per-window signal divides by 8 (a switch every 75 s is heavy).
  // The hourly fragmentation signal keeps the 40 per hour budget.
  const RATE_SWITCHES_WINDOW = 8;
  const RATE_SWITCHES_HOUR = 40;
  const TEXT_SCAN_MAX = 4000;              // bound the work per frame text
  const STOPS_LIMIT = 8;
  const STOPS_PER_QUARTER = 2;             // so the tram has somewhere to go
  const BUILDING_MIN = 6;
  const BUILDING_MAX = 40;

  // Warm Mediterranean palette, assigned by rank so a quarter keeps its colour.
  const PALETTE = [
    0xf2733f, 0xe0913f, 0xd4553f, 0xf6c445, 0xb9c46a,
    0xc98a5e, 0xe07a5f, 0xb5533f, 0xd9b26f
  ];

  // Common English plus UI/OCR noise. Words shorter than 5 letters never qualify.
  const STOP_WORDS = new Set([
    'about', 'above', 'after', 'again', 'against', 'already', 'although', 'always',
    'among', 'another', 'anyone', 'anything', 'around', 'become', 'before', 'begin',
    'behind', 'being', 'below', 'beside', 'between', 'beyond', 'billion', 'bottom',
    'button', 'buttons', 'cannot', 'center', 'certain', 'check', 'choose', 'chrome',
    'class', 'click', 'clicked', 'client', 'close', 'closed', 'column', 'comment',
    'comments', 'common', 'complete', 'content', 'context', 'control', 'could',
    'create', 'cursor', 'custom', 'default', 'details', 'different', 'document',
    'documents', 'double', 'during', 'element', 'email', 'empty', 'enough', 'error',
    'errors', 'event', 'events', 'every', 'everything', 'example', 'except', 'false',
    'field', 'fields', 'figure', 'files', 'filter', 'first', 'folder', 'follow',
    'footer', 'format', 'found', 'frame', 'frames', 'function', 'future', 'github',
    'given', 'google', 'great', 'group', 'groups', 'header', 'hello', 'https',
    'image', 'images', 'index', 'input', 'inside', 'instead', 'issue', 'issues',
    'keyboard', 'known', 'label', 'labels', 'large', 'later', 'layout', 'learn',
    'leave', 'length', 'less', 'level', 'light', 'lines', 'list', 'little',
    'loading', 'local', 'login', 'logout', 'lower', 'maybe', 'means', 'media',
    'menu', 'message', 'messages', 'might', 'million', 'minus', 'minutes', 'modal',
    'modern', 'moment', 'mouse', 'moved', 'never', 'night', 'nobody', 'nothing',
    'notice', 'notification', 'notifications', 'number', 'numbers', 'offer', 'often',
    'option', 'options', 'order', 'other', 'others', 'outside', 'pages', 'panel',
    'panels', 'paper', 'paste', 'people', 'place', 'please', 'point', 'power',
    'press', 'preview', 'print', 'prior', 'process', 'processes', 'project',
    'projects', 'proper', 'public', 'quick', 'quite', 'rather', 'really', 'recent',
    'record', 'records', 'remove', 'repeat', 'replace', 'request', 'response',
    'result', 'results', 'return', 'right', 'saved', 'screen', 'script', 'scroll',
    'search', 'second', 'section', 'select', 'selected', 'server', 'service',
    'services', 'settings', 'several', 'sheet', 'short', 'should', 'shown', 'sidebar',
    'signin', 'signout', 'since', 'single', 'small', 'something', 'sometimes',
    'source', 'space', 'start', 'state', 'still', 'string', 'style', 'support',
    'switch', 'system', 'table', 'taken', 'target', 'terminal', 'text', 'their',
    'them', 'theme', 'there', 'these', 'thing', 'things', 'think', 'third', 'this',
    'those', 'though', 'thread', 'three', 'through', 'title', 'today', 'together',
    'tomorrow', 'toolbar', 'total', 'touch', 'toward', 'trigger', 'true', 'typed',
    'under', 'undo', 'unique', 'until', 'update', 'upper', 'usage', 'using', 'usual',
    'value', 'values', 'video', 'view', 'views', 'while', 'which', 'whole', 'window',
    'windows', 'without', 'workspace', 'would', 'write', 'writing', 'wrong',
    'yesterday', 'youtube'
  ]);

  const CHUNK_SPLIT = /[.!?,;:|\n\r\t\u2026\u00b7\u2022()\[\]{}"'\u00ab\u00bb\/\\]+/;
  const SPACE_SPLIT = /\s+/;

  // --------------------------------------------------------------- helpers
  function toFiniteNumber(value, fallback) {
    const n = typeof value === 'number' ? value : Number(value);
    return Number.isFinite(n) ? n : fallback;
  }

  function isLetterCode(c) {
    if (c >= 65 && c <= 90) return true;
    if (c >= 97 && c <= 122) return true;
    if (c >= 0xc0 && c <= 0x24f && c !== 0xd7 && c !== 0xf7) return true;
    return false;
  }

  // A phrase token: letters, with interior apostrophes/hyphens. Digits reject it.
  function cleanToken(raw) {
    if (!raw) return null;
    let a = 0;
    let b = raw.length;
    while (a < b && !isLetterCode(raw.charCodeAt(a))) a++;
    while (b > a && !isLetterCode(raw.charCodeAt(b - 1))) b--;
    if (b - a < 1 || b - a > 24) return null;
    const core = raw.slice(a, b);
    for (let i = 0; i < core.length; i++) {
      const c = core.charCodeAt(i);
      if (!isLetterCode(c) && c !== 39 && c !== 45 && c !== 0x2019) return null;
    }
    return core;
  }

  function titleCaseFirst(text) {
    const c = text.charCodeAt(0);
    if (c >= 97 && c <= 122) return String.fromCharCode(c - 32) + text.slice(1);
    return text;
  }

  // Index of the last element <= value in a sorted Float64Array-like, or -1.
  function lastIndexAtOrBefore(arr, count, value) {
    let lo = 0;
    let hi = count - 1;
    let found = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (arr[mid] <= value) {
        found = mid;
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    return found;
  }

  // ------------------------------------------------------------ input prep
  const src = exportObj && typeof exportObj === 'object' ? exportObj : null;
  const rawFrames = src && Array.isArray(src.frames) ? src.frames : [];

  const frames = [];
  for (let i = 0; i < rawFrames.length; i++) {
    const raw = rawFrames[i];
    if (!raw || typeof raw !== 'object') continue;
    const t = toFiniteNumber(raw.captured_at, NaN);
    if (!Number.isFinite(t)) continue;
    const appRaw = raw.app_hint;
    const app = typeof appRaw === 'string' && appRaw.trim() !== '' ? appRaw.trim() : 'Unknown';
    frames.push({
      id: raw.frame_id === undefined || raw.frame_id === null ? frames.length + 1 : raw.frame_id,
      t: t,
      app: app,
      title: typeof raw.window_title === 'string' ? raw.window_title : '',
      url: typeof raw.browser_url === 'string' ? raw.browser_url : null,
      text: typeof raw.content_text === 'string' ? raw.content_text : ''
    });
  }
  // Sorted copy. Array.prototype.sort is stable, so equal stamps keep input order.
  frames.sort(function (a, b) { return a.t - b.t; });

  const frameCount = frames.length;
  const t0 = frameCount ? frames[0].t : 0;
  const t1 = frameCount ? frames[frameCount - 1].t : 0;

  // Data-relative clock by default: a one-day export has no neglected quarter.
  const now = toFiniteNumber(options.now, frameCount ? t1 : 0);

  let maxQuarters = Math.floor(toFiniteNumber(options.maxQuarters, 9));
  if (!(maxQuarters >= 1)) maxQuarters = 1;
  if (maxQuarters > 64) maxQuarters = 64;

  // Hour-of-day bucketing. Explicit option wins, then an export-level hint, then
  // the runtime local zone (the simulated day's author passes +1 h explicitly).
  let tzOffsetMs = toFiniteNumber(options.tzOffsetMs, NaN);
  if (!Number.isFinite(tzOffsetMs) && src) tzOffsetMs = toFiniteNumber(src.tzOffsetMs, NaN);
  if (!Number.isFinite(tzOffsetMs)) {
    if (frameCount) {
      const localOffsetMinutes = new Date(t0).getTimezoneOffset();
      tzOffsetMs = Number.isFinite(localOffsetMinutes) ? -localOffsetMinutes * 60000 : 0;
    } else {
      tzOffsetMs = 0;
    }
  }

  // ------------------------------------------------------- per-frame slots
  // A frame owns the time until the next capture, capped at 90 s. The last frame
  // borrows the median gap so trailing time is not lost.
  const slots = new Float64Array(frameCount);
  let tailSlot = 5000;
  if (frameCount > 1) {
    const diffs = [];
    for (let i = 1; i < frameCount; i++) {
      const d = frames[i].t - frames[i - 1].t;
      if (d > 0) diffs.push(d > SLOT_MAX_MS ? SLOT_MAX_MS : d);
    }
    if (diffs.length) {
      diffs.sort(function (a, b) { return a - b; });
      const mid = diffs[(diffs.length - 1) >> 1];
      tailSlot = mid > 0 ? mid : 5000;
    }
  }
  for (let i = 0; i < frameCount; i++) {
    let d = i + 1 < frameCount ? frames[i + 1].t - frames[i].t : tailSlot;
    if (!(d > 0)) d = 0;
    else if (d > SLOT_MAX_MS) d = SLOT_MAX_MS;
    slots[i] = d;
  }

  // ------------------------------------------------------ pass 1: per app
  const appStats = new Map();
  function statsFor(appName) {
    let stats = appStats.get(appName);
    if (!stats) {
      stats = {
        name: appName,
        totalMs: 0,
        frameCount: 0,
        firstActiveAt: 0,
        lastActiveAt: 0,
        titles: new Map(),
        hours: new Set(),
        words: new Map(),
        phraseCounts: new Map(),
        phraseFirst: new Map(),
        phraseSeq: 0,
        lastPhrase: ''
      };
      appStats.set(appName, stats);
    }
    return stats;
  }
  for (let i = 0; i < frameCount; i++) {
    const f = frames[i];
    const stats = statsFor(f.app);
    if (stats.frameCount === 0) {
      stats.firstActiveAt = f.t;
      stats.lastActiveAt = f.t;
    } else if (f.t > stats.lastActiveAt) {
      stats.lastActiveAt = f.t;
    }
    stats.totalMs += slots[i];
    stats.frameCount += 1;
    if (f.title) stats.titles.set(f.title, (stats.titles.get(f.title) || 0) + 1);
  }

  // ----------------------------------------------------------- quarter rank
  const ranked = [];
  appStats.forEach(function (stats) { ranked.push(stats); });
  ranked.sort(function (a, b) {
    if (b.totalMs !== a.totalMs) return b.totalMs - a.totalMs;
    if (b.frameCount !== a.frameCount) return b.frameCount - a.frameCount;
    if (a.name < b.name) return -1;
    if (a.name > b.name) return 1;
    return 0;
  });

  let totalAll = 0;
  for (let i = 0; i < ranked.length; i++) totalAll += ranked[i].totalMs;
  const maxTotal = ranked.length ? ranked[0].totalMs : 0;

  const quarters = [];
  const quarterOf = new Map();
  for (let i = 0; i < ranked.length; i++) {
    const stats = ranked[i];
    if (i >= maxQuarters) {
      quarterOf.set(stats.name, undefined);
      continue;
    }
    quarterOf.set(stats.name, i);
    const share = totalAll > 0 ? stats.totalMs / totalAll : 0;
    const ratio = maxTotal > 0 ? stats.totalMs / maxTotal : 0;
    let buildingCount = BUILDING_MIN + Math.round((BUILDING_MAX - BUILDING_MIN) * ratio);
    if (buildingCount < BUILDING_MIN) buildingCount = BUILDING_MIN;
    if (buildingCount > BUILDING_MAX) buildingCount = BUILDING_MAX;
    let towerCount = 0;
    if (i === 0) towerCount = 3;
    else if (i === 1) towerCount = 2;
    else if (i === 2) towerCount = 1;
    quarters.push({
      idx: i,
      name: stats.name,
      color: PALETTE[i % PALETTE.length],
      totalMs: stats.totalMs,
      share: share,
      frameCount: stats.frameCount,
      firstActiveAt: stats.firstActiveAt,
      lastActiveAt: stats.lastActiveAt,
      buildingCount: buildingCount,
      towerCount: towerCount,
      neglected: now - stats.lastActiveAt > NEGLECT_MS,
      phrases: [],
      lastPhrase: '',
      titles: []
    });
  }

  // ------------------------------ pass 2: words, phrases, titles and hours
  const frameQ = new Int32Array(frameCount);
  const allPhrases = [];
  const titleHours = new Map();     // title -> Set of local hour slots
  const titleVisits = new Map();    // title -> frame count
  const titleQuarter = new Map();   // title -> Map quarter idx -> count

  const runWords = [];

  function pushOccurrence(text, appName, q, at) {
    const stats = appStats.get(appName);
    if (!stats.phraseCounts.has(text)) {
      stats.phraseCounts.set(text, 0);
      stats.phraseFirst.set(text, stats.phraseSeq);
      stats.phraseSeq += 1;
    }
    stats.phraseCounts.set(text, stats.phraseCounts.get(text) + 1);
    stats.lastPhrase = text;
    if (q < 0) return;
    const last = allPhrases.length ? allPhrases[allPhrases.length - 1] : null;
    if (last && last.at === at && last.q === q && last.text === text) return;
    allPhrases.push({ text: text, q: q, at: at });
  }

  function emitRun(run, appName, q, at) {
    let start = 0;
    while (start < run.length) {
      let best = 0;
      let words = 0;
      let chars = 0;
      for (let k = start; k < run.length && words < PHRASE_MAX_WORDS; k++) {
        const add = words === 0 ? run[k].length : run[k].length + 1;
        if (chars + add > PHRASE_MAX_CHARS) break;
        chars += add;
        words += 1;
        if (words >= PHRASE_MIN_WORDS) best = words;
      }
      if (best === 0) {
        start += 1;
        continue;
      }
      let text = run[start];
      for (let k = 1; k < best; k++) text = text + ' ' + run[start + k];
      pushOccurrence(titleCaseFirst(text), appName, q, at);
      start += best;
    }
  }

  function collectPhrases(text, appName, q, at) {
    if (!text) return;
    const limit = text.length > TEXT_SCAN_MAX ? TEXT_SCAN_MAX : text.length;
    const chunks = text.slice(0, limit).split(CHUNK_SPLIT);
    for (let ci = 0; ci < chunks.length; ci++) {
      const chunk = chunks[ci];
      if (!chunk) continue;
      const parts = chunk.split(SPACE_SPLIT);
      runWords.length = 0;
      for (let pi = 0; pi < parts.length; pi++) {
        const token = cleanToken(parts[pi]);
        if (token === null) {
          if (runWords.length) emitRun(runWords, appName, q, at);
          runWords.length = 0;
        } else {
          runWords.push(token);
        }
      }
      if (runWords.length) emitRun(runWords, appName, q, at);
      runWords.length = 0;
    }
  }

  function collectWords(text, out) {
    if (!text) return;
    const limit = text.length > TEXT_SCAN_MAX ? TEXT_SCAN_MAX : text.length;
    let word = '';
    for (let i = 0; i <= limit; i++) {
      const inside = i < limit;
      const c = inside ? text.charCodeAt(i) : 0;
      if (inside && isLetterCode(c)) {
        word = word + text[i];
        continue;
      }
      if (word.length >= 5) {
        const lower = word.toLowerCase();
        if (!STOP_WORDS.has(lower)) out.set(lower, (out.get(lower) || 0) + 1);
      }
      word = '';
    }
  }

  for (let i = 0; i < frameCount; i++) {
    const f = frames[i];
    const stats = appStats.get(f.app);
    const listed = quarterOf.get(f.app);
    const q = listed === undefined ? -1 : listed;
    frameQ[i] = q;

    const local = f.t + tzOffsetMs;
    const hourSlot = Math.floor(local / HOUR_MS);
    stats.hours.add(hourSlot);

    if (f.title) {
      titleVisits.set(f.title, (titleVisits.get(f.title) || 0) + 1);
      let hours = titleHours.get(f.title);
      if (!hours) {
        hours = new Set();
        titleHours.set(f.title, hours);
      }
      hours.add(hourSlot);
      let perQuarter = titleQuarter.get(f.title);
      if (!perQuarter) {
        perQuarter = new Map();
        titleQuarter.set(f.title, perQuarter);
      }
      if (q >= 0) perQuarter.set(q, (perQuarter.get(q) || 0) + 1);
    }

    collectWords(f.text, stats.words);
    collectPhrases(f.text, f.app, q, f.t);
  }

  // Quarter phrase lists and distinct titles, best first.
  for (let i = 0; i < quarters.length; i++) {
    const quarter = quarters[i];
    const stats = appStats.get(quarter.name);
    if (!stats) continue;
    const phraseList = [];
    stats.phraseCounts.forEach(function (_count, text) { phraseList.push(text); });
    phraseList.sort(function (a, b) {
      const ca = stats.phraseCounts.get(a) || 0;
      const cb = stats.phraseCounts.get(b) || 0;
      if (cb !== ca) return cb - ca;
      return (stats.phraseFirst.get(a) || 0) - (stats.phraseFirst.get(b) || 0);
    });
    if (phraseList.length > MAX_PHRASES_PER_APP) phraseList.length = MAX_PHRASES_PER_APP;
    quarter.phrases = phraseList;
    quarter.lastPhrase = stats.lastPhrase || '';

    const titleList = [];
    stats.titles.forEach(function (_count, title) { titleList.push(title); });
    titleList.sort(function (a, b) {
      const ca = stats.titles.get(a) || 0;
      const cb = stats.titles.get(b) || 0;
      if (cb !== ca) return cb - ca;
      if (a < b) return -1;
      if (a > b) return 1;
      return 0;
    });
    quarter.titles = titleList;
  }

  // -------------------------------------------------------------- sessions
  const sessions = [];
  let openStart = 0;
  let openEnd = 0;
  let openFrames = 0;
  let openApp = null;
  let openQ = -1;
  for (let i = 0; i < frameCount; i++) {
    const f = frames[i];
    const q = frameQ[i];
    const continues = openApp !== null && f.app === openApp && i > 0 &&
      f.t - frames[i - 1].t < SPLIT_GAP_MS;
    if (continues) {
      openEnd = f.t + slots[i];
      openFrames += 1;
    } else {
      if (openApp !== null) {
        sessions.push({
          q: openQ,
          start: openStart,
          end: openEnd,
          durationMs: openEnd - openStart,
          frameCount: openFrames
        });
      }
      openApp = f.app;
      openQ = q;
      openStart = f.t;
      openEnd = f.t + slots[i];
      openFrames = 1;
    }
  }
  if (openApp !== null) {
    sessions.push({
      q: openQ,
      start: openStart,
      end: openEnd,
      durationMs: openEnd - openStart,
      frameCount: openFrames
    });
  }

  const sessionCount = sessions.length;
  const sessionStarts = new Float64Array(sessionCount);
  const sessionBest = new Int32Array(sessionCount);
  let bestIdx = -1;
  for (let i = 0; i < sessionCount; i++) {
    sessionStarts[i] = sessions[i].start;
    if (bestIdx < 0 || sessions[i].durationMs > sessions[bestIdx].durationMs) bestIdx = i;
    sessionBest[i] = bestIdx;
  }

  // ------------------------------------------------------ 10 minute windows
  const windowBase = frameCount ? Math.floor(t0 / WINDOW_MS) * WINDOW_MS : 0;
  let windowCount = 0;
  if (frameCount) {
    windowCount = Math.floor((t1 - windowBase) / WINDOW_MS) + 1;
    if (!(windowCount > 0)) windowCount = 1;
  }
  const windows10 = [];
  const quarterCount = quarters.length;
  for (let w = 0; w < windowCount; w++) {
    windows10.push({
      start: windowBase + w * WINDOW_MS,
      end: windowBase + (w + 1) * WINDOW_MS,
      switches: 0,
      activity: quarterCount ? new Array(quarterCount).fill(0) : []
    });
  }

  for (let i = 0; i < frameCount; i++) {
    const qi = frameQ[i];
    if (qi < 0) continue;
    let start = frames[i].t;
    let remain = slots[i];
    let guard = 0;
    while (remain > 0 && guard < 3) {
      const w = Math.floor((start - windowBase) / WINDOW_MS);
      if (w < 0 || w >= windowCount) break;
      const win = windows10[w];
      const take = Math.min(remain, win.end - start);
      if (take <= 0) break;
      win.activity[qi] += take;
      remain -= take;
      start += take;
      guard += 1;
    }
  }

  // Every app change between consecutive captures is a switch, including a
  // return after a long pause and a repeat of an earlier transition
  // (A -> B -> A is two). Bucketing uses the time of the arriving frame.
  for (let i = 1; i < frameCount; i++) {
    const prev = frames[i - 1];
    const cur = frames[i];
    if (cur.app === prev.app) continue;
    const w = Math.floor((cur.t - windowBase) / WINDOW_MS);
    if (w >= 0 && w < windowCount) windows10[w].switches += 1;
  }

  const switchPrefix = new Int32Array(windowCount + 1);
  let switchAcc = 0;
  for (let w = 0; w < windowCount; w++) {
    switchPrefix[w] = switchAcc;
    switchAcc += windows10[w].switches;
  }
  switchPrefix[windowCount] = switchAcc;

  // ----------------------------------------------------------------- stops
  const stopList = [];
  titleHours.forEach(function (hours, title) {
    if (hours.size < 2) return;
    const perQuarter = titleQuarter.get(title);
    let q = -1;
    let qBest = 0;
    if (perQuarter) {
      perQuarter.forEach(function (count, idx) {
        if (count > qBest || (count === qBest && q >= 0 && idx < q)) {
          qBest = count;
          q = idx;
        }
      });
    }
    if (q < 0) return;
    stopList.push({ title: title, q: q, hours: hours.size, visits: titleVisits.get(title) || 0 });
  });
  stopList.sort(function (a, b) {
    if (b.visits !== a.visits) return b.visits - a.visits;
    if (b.hours !== a.hours) return b.hours - a.hours;
    if (a.title < b.title) return -1;
    if (a.title > b.title) return 1;
    return 0;
  });
  // Spread the chosen stops over quarters: at most two per quarter, best
  // visits first, and never a duplicate quarter to pad the list.
  const stops = [];
  const stopsPerQuarter = new Map();
  for (let i = 0; i < stopList.length && stops.length < STOPS_LIMIT; i++) {
    const candidate = stopList[i];
    const used = stopsPerQuarter.get(candidate.q) || 0;
    if (used >= STOPS_PER_QUARTER) continue;
    stopsPerQuarter.set(candidate.q, used + 1);
    stops.push(candidate);
  }

  // ----------------------------------------------------------------- pairs
  const pairs = [];
  for (let a = 0; a < quarterCount; a++) {
    const wordsA = appStats.get(quarters[a].name).words;
    if (!wordsA || wordsA.size === 0) continue;
    for (let b = a + 1; b < quarterCount; b++) {
      const wordsB = appStats.get(quarters[b].name).words;
      if (!wordsB || wordsB.size === 0) continue;
      const small = wordsA.size <= wordsB.size ? wordsA : wordsB;
      const big = small === wordsA ? wordsB : wordsA;
      const shared = [];
      let strength = 0;
      small.forEach(function (count, word) {
        const other = big.get(word);
        if (other) {
          shared.push(word);
          strength += Math.sqrt(count * other);
        }
      });
      if (!shared.length) continue;
      shared.sort();
      const hoursA = appStats.get(quarters[a].name).hours;
      const hoursB = appStats.get(quarters[b].name).hours;
      const smallSet = hoursA.size <= hoursB.size ? hoursA : hoursB;
      const bigSet = smallSet === hoursA ? hoursB : hoursA;
      let sameHour = false;
      smallSet.forEach(function (slot) {
        if (bigSet.has(slot)) sameHour = true;
      });
      pairs.push({
        a: a,
        b: b,
        words: shared,
        strength: strength,
        sameHour: sameHour
      });
    }
  }
  pairs.sort(function (x, y) {
    if (y.strength !== x.strength) return y.strength - x.strength;
    if (y.words.length !== x.words.length) return y.words.length - x.words.length;
    if (x.a !== y.a) return x.a - y.a;
    return x.b - y.b;
  });
  let dreamPair = null;
  for (let i = 0; i < pairs.length; i++) {
    if (pairs[i].sameHour === false) {
      dreamPair = pairs[i];
      break;
    }
  }

  // --------------------------------------------------------------- parcels
  const parcels = [];
  const lastPhraseAt = new Map();   // text -> { q, at }
  const lastParcelAt = new Map();   // text|from|to -> at
  for (let i = 0; i < allPhrases.length; i++) {
    const occurrence = allPhrases[i];
    const prev = lastPhraseAt.get(occurrence.text);
    if (prev && prev.q !== occurrence.q && occurrence.at - prev.at <= PARCEL_WINDOW_MS) {
      const key = occurrence.text + '\u0001' + prev.q + '\u0001' + occurrence.q;
      const previousParcel = lastParcelAt.get(key);
      if (previousParcel === undefined || occurrence.at - previousParcel >= PARCEL_WINDOW_MS) {
        parcels.push({
          phrase: occurrence.text,
          from: prev.q,
          to: occurrence.q,
          at: occurrence.at
        });
        lastParcelAt.set(key, occurrence.at);
      }
    }
    lastPhraseAt.set(occurrence.text, { q: occurrence.q, at: occurrence.at });
  }

  // ------------------------------------------------------------------ gaps
  const gaps = [];
  for (let i = 1; i < frameCount; i++) {
    const prev = frames[i - 1];
    const cur = frames[i];
    if (cur.t - prev.t > BIG_GAP_MS) gaps.push({ start: prev.t, end: cur.t });
  }

  // ---------------------------------------------------------- query: rates
  function windowIndexFor(t) {
    if (!(windowCount > 0)) return -1;
    if (!(t >= windowBase)) return 0;
    const idx = Math.floor((t - windowBase) / WINDOW_MS);
    return idx >= windowCount ? windowCount - 1 : idx;
  }

  function switchRateAt(t) {
    const value = toFiniteNumber(t, NaN);
    if (!Number.isFinite(value) || windowCount === 0) return 0;
    const rate = windows10[windowIndexFor(value)].switches / RATE_SWITCHES_WINDOW;
    return rate > 1 ? 1 : rate;
  }

  function fragmentationAt(t) {
    const value = toFiniteNumber(t, NaN);
    if (!Number.isFinite(value) || windowCount === 0) return 0;
    const hi = windowIndexFor(value);
    let lo = windowIndexFor(value - HOUR_MS);
    if (lo > hi) lo = hi;
    const rate = (switchPrefix[hi + 1] - switchPrefix[lo]) / RATE_SWITCHES_HOUR;
    if (rate > 1) return 1;
    return rate < 0 ? 0 : rate;
  }

  // ------------------------------------------------ query: activity shares
  const lastHourOut = new Float32Array(quarterCount);
  const lastHourStart = frameCount ? t0 : 0;

  function activityLastHour(t) {
    const out = lastHourOut;
    out.fill(0);
    const value = toFiniteNumber(t, NaN);
    if (!Number.isFinite(value) || windowCount === 0 || quarterCount === 0) return out;
    const hi = windowIndexFor(value);
    let lo = windowIndexFor(value - HOUR_MS);
    if (lo > hi) lo = hi;
    const from = value - HOUR_MS > lastHourStart ? value - HOUR_MS : lastHourStart;
    for (let w = lo; w <= hi; w++) {
      const win = windows10[w];
      const a = win.start > from ? win.start : from;
      const b = win.end < value ? win.end : value;
      const overlap = b - a;
      if (overlap <= 0) continue;
      const factor = overlap / WINDOW_MS;
      const act = win.activity;
      for (let q = 0; q < quarterCount; q++) out[q] += act[q] * factor;
    }
    for (let q = 0; q < quarterCount; q++) {
      const share = out[q] / HOUR_MS;
      out[q] = share > 1 ? 1 : share;
    }
    return out;
  }

  // ------------------------------------------------ query: session lookups
  function activeSessionAt(t) {
    if (sessionCount === 0) return null;
    const value = toFiniteNumber(t, NaN);
    if (!Number.isFinite(value)) return null;
    const idx = lastIndexAtOrBefore(sessionStarts, sessionCount, value);
    if (idx < 0) return null;
    const session = sessions[idx];
    return value <= session.end ? session : null;
  }

  function longestSessionBefore(t) {
    if (sessionCount === 0) return null;
    const value = toFiniteNumber(t, NaN);
    if (!Number.isFinite(value)) return null;
    const idx = lastIndexAtOrBefore(sessionStarts, sessionCount, value);
    if (idx < 0) return null;
    return sessions[sessionBest[idx]];
  }

  // -------------------------------------------------- query: crane heights
  // At most one session contains a given instant (session spans never overlap),
  // so one reused array and one reused entry are enough.
  const craneOut = [];
  const craneEntry = { q: 0, floors: 0 };

  function cranesAt(t) {
    craneOut.length = 0;
    const session = activeSessionAt(t);
    if (!session || session.q < 0) return craneOut;
    const elapsed = toFiniteNumber(t, 0) - session.start;
    if (!(elapsed > CRANE_MIN_MS)) return craneOut;
    let floors = 1 + Math.floor((elapsed - CRANE_MIN_MS) / CRANE_STEP_MS);
    if (floors > FLOOR_MAX) floors = FLOOR_MAX;
    craneEntry.q = session.q;
    craneEntry.floors = floors;
    craneOut.push(craneEntry);
    return craneOut;
  }

  // ----------------------------------------------------------------- model
  return {
    frames: frames,
    t0: frameCount ? t0 : now,
    t1: frameCount ? t1 : now,
    tzOffsetMs: tzOffsetMs,
    quarters: quarters,
    quarterOf: quarterOf,
    sessions: sessions,
    windows10: windows10,
    stops: stops,
    phrases: allPhrases,
    pairs: pairs,
    parcels: parcels,
    dreamPair: dreamPair,
    gaps: gaps,
    switchRateAt: switchRateAt,
    fragmentationAt: fragmentationAt,
    activityLastHour: activityLastHour,
    activeSessionAt: activeSessionAt,
    cranesAt: cranesAt,
    longestSessionBefore: longestSessionBefore
  };
}
