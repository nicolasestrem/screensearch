// Validates sample-export.json against the six product requirements.
//
// Every property is recomputed here from the frame list alone: session splitting
// on the 90 second rule, per local hour switch counts, per app session totals,
// cross app salient word overlaps, title revisited hours and repeated phrases.
// Nothing is imported from the generator except the object itself, so a broken
// generator cannot hide behind its own intent.

import { readFileSync } from 'node:fs';
import { generateSimulatedDay } from '../src/simday.mjs';

const CET_MS = 3600000;          // Europe/Paris on 2026-11-05 is UTC+1
const SPLIT_MS = 90000;          // a same app session survives gaps under 90 s
const FOCUS_MS = 3 * 3600000;    // the required focus block length
const PARCEL_MS = 10 * 60000;    // same phrase, two apps, within ten minutes
const OUTPUT = new URL('../sample-export.json', import.meta.url);

const STOP = new Set([
  'about', 'after', 'again', 'being', 'button', 'could', 'every', 'https',
  'might', 'other', 'search', 'settings', 'shall', 'should', 'since', 'still',
  'their', 'there', 'these', 'those', 'untitled', 'until', 'using', 'where',
  'which', 'while', 'window', 'would'
]);

let failures = 0;

function report(label, ok, detail) {
  const line = (ok ? 'PASS ' : 'FAIL ') + label;
  console.log(detail ? line + ' :: ' + detail : line);
  if (!ok) failures++;
}

function localHour(t) {
  return new Date(t + CET_MS).getUTCHours();
}

function pad2(n) {
  return n < 10 ? '0' + n : String(n);
}

function clockText(t) {
  const d = new Date(t + CET_MS);
  return pad2(d.getUTCHours()) + ':' + pad2(d.getUTCMinutes()) + ':' + pad2(d.getUTCSeconds());
}

function minutesText(ms) {
  return (ms / 60000).toFixed(1) + ' min';
}

function countWords(text) {
  const parts = text.split(/\s+/);
  let n = 0;
  for (const part of parts) if (part.length) n++;
  return n;
}

function letterWords(text) {
  const found = text.toLowerCase().match(/[a-z]+/g);
  return found ? found : [];
}

function salientOf(texts) {
  const set = new Set();
  for (const text of texts) {
    for (const word of letterWords(text)) {
      if (word.length >= 5 && !STOP.has(word)) set.add(word);
    }
  }
  return set;
}

function longWordsOf(texts) {
  const set = new Set();
  for (const text of texts) {
    for (const word of letterWords(text)) if (word.length >= 5) set.add(word);
  }
  return set;
}

function phrasesOf(text) {
  const out = [];
  const sentences = text.split(/[.!?;:\n]+/);
  for (const sentence of sentences) {
    const parts = sentence.trim().split(/\s+/).filter(Boolean);
    if (parts.length < 3 || parts.length > 6) continue;
    let digits = false;
    for (const part of parts) {
      if (/^[0-9]+$/.test(part)) {
        digits = true;
        break;
      }
    }
    if (digits) continue;
    let phrase = parts.join(' ');
    if (phrase.length > 32) phrase = phrase.slice(0, 32);
    out.push(phrase.charAt(0).toUpperCase() + phrase.slice(1));
  }
  return out;
}

const day = generateSimulatedDay();

const rows = day.frames
  .map(function toRow(frame) {
    return {
      t: frame.captured_at,
      app: frame.app_hint || 'Unknown',
      title: frame.window_title || '',
      text: frame.content_text || ''
    };
  })
  .sort(function byTime(a, b) {
    return a.t - b.t;
  });

// Sessions: consecutive frames of one app whose gaps stay under 90 seconds.
function splitSessions(list) {
  const out = [];
  let current = null;
  for (const row of list) {
    if (current && current.app === row.app && row.t - current.end < SPLIT_MS) {
      if (row.t - current.end > current.maxGap) current.maxGap = row.t - current.end;
      current.end = row.t;
      current.frames++;
    } else {
      current = { app: row.app, start: row.t, end: row.t, frames: 1, maxGap: 0 };
      out.push(current);
    }
  }
  return out;
}

const sessions = splitSessions(rows);

// ---------------------------------------------------------------- file bytes
try {
  const onDisk = readFileSync(OUTPUT, 'utf8');
  const expected = JSON.stringify(generateSimulatedDay(), null, 1) + '\n';
  const same = onDisk === expected;
  report(
    'file byte-identical to generateSimulatedDay()',
    same,
    same
      ? onDisk.length + ' bytes'
      : 'disk ' + onDisk.length + ' bytes vs generated ' + expected.length + ' bytes'
  );
} catch (error) {
  report('file byte-identical to generateSimulatedDay()', false, String(error.message));
}

// --------------------------------------------------------------------- shape
function checkShape() {
  const apps = new Set();
  let minWords = Infinity;
  let maxWords = 0;
  let badWordFrame = -1;
  for (const row of rows) {
    apps.add(row.app);
    const n = countWords(row.text);
    if (n < minWords) minWords = n;
    if (n > maxWords) maxWords = n;
    if ((n < 20 || n > 80) && badWordFrame < 0) badWordFrame = row.t;
  }
  const firstHour = localHour(rows[0].t);
  const lastHour = localHour(rows[rows.length - 1].t);
  const shaped =
    day.schema === 'screensearch.export.v1' &&
    Array.isArray(day.marks) &&
    rows.length >= 540 &&
    rows.length <= 660 &&
    apps.size === 8 &&
    minWords >= 20 &&
    maxWords <= 80 &&
    badWordFrame < 0 &&
    firstHour === 8 &&
    lastHour === 22;
  console.log(
    'frames: ' + rows.length + ', apps: ' + apps.size +
    ', words per frame: ' + minWords + ' to ' + maxWords +
    ', span: ' + clockText(rows[0].t) + ' to ' + clockText(rows[rows.length - 1].t)
  );
  report(
    'shape: span, frame count, app count, content_text length',
    shaped,
    shaped
      ? apps.size + ' apps, 540..660 frames, 20..80 words'
      : 'frames ' + rows.length + ', apps ' + apps.size + ', words ' + minWords + '..' + maxWords
  );
}

// ------------------------------------------- 1. one focus block of 3 h plus
function checkFocusBlock() {
  let best = null;
  for (const session of sessions) {
    const span = session.end - session.start;
    if (!best || span > best.end - best.start) best = session;
  }
  const long = sessions.filter(function isLong(session) {
    return session.end - session.start >= FOCUS_MS && session.maxGap < SPLIT_MS;
  });
  const ok = best !== null && long.length >= 1 && best.maxGap < SPLIT_MS;
  report(
    '1 focus block: one app session of 3 h with gaps under 90 s',
    ok,
    best === null
      ? 'no sessions'
      : best.app + ' from ' + clockText(best.start) + ' to ' + clockText(best.end) +
        ' (' + minutesText(best.end - best.start) + ', ' + best.frames +
        ' frames, largest gap ' + Math.round(best.maxGap / 1000) + ' s, ' +
        long.length + ' session(s) over 3 h)'
  );
}

// ------------------------------- 2. one fragmented hour with 40 app switches
function checkFragmentedHour() {
  const perHour = new Map();
  for (let i = 1; i < rows.length; i++) {
    if (rows[i].app !== rows[i - 1].app) {
      const hour = localHour(rows[i].t);
      perHour.set(hour, (perHour.get(hour) || 0) + 1);
    }
  }
  const hours = [...perHour.keys()].sort(function asc(a, b) {
    return a - b;
  });
  const with40 = hours.filter(function is40(hour) {
    return perHour.get(hour) === 40;
  });
  const summary = hours
    .map(function entry(hour) {
      return hour + 'h:' + perHour.get(hour);
    })
    .join(' ');
  report(
    '2 fragmented hour: exactly 40 app switches in one local hour',
    with40.length === 1,
    'hour ' + (with40.length ? with40[0] : '?') + ' has ' +
      (with40.length ? 40 : 'no hour') + ' switches; hours with switches: ' + summary
  );
}

// ------------------- 3. two apps opened once, under a minute of activity each
function checkShortOpenApps() {
  const byApp = new Map();
  for (const session of sessions) {
    if (!byApp.has(session.app)) byApp.set(session.app, []);
    byApp.get(session.app).push(session);
  }
  const tiny = [];
  for (const [app, list] of byApp) {
    if (list.length !== 1) continue;
    const span = list[0].end - list[0].start;
    if (span < 60000) tiny.push({ app: app, span: span, start: list[0].start });
  }
  tiny.sort(function asc(a, b) {
    return a.start - b.start;
  });
  const names = tiny.map(function entry(item) {
    return item.app + ' ' + (item.span / 1000).toFixed(0) + ' s at ' + clockText(item.start);
  });
  report(
    '3 two apps opened exactly once for under 60 s',
    tiny.length >= 2,
    names.length ? names.join('; ') : 'none found'
  );
}

// ------------- 4. two apps sharing exactly three salient words, never one hour
function checkSharedWords() {
  const textsByApp = new Map();
  const hoursByApp = new Map();
  for (const row of rows) {
    if (!textsByApp.has(row.app)) {
      textsByApp.set(row.app, []);
      hoursByApp.set(row.app, new Set());
    }
    textsByApp.get(row.app).push(row.text);
    hoursByApp.get(row.app).add(localHour(row.t));
  }
  const apps = [...textsByApp.keys()].sort();
  const salient = new Map();
  const longWords = new Map();
  for (const app of apps) {
    salient.set(app, salientOf(textsByApp.get(app)));
    longWords.set(app, longWordsOf(textsByApp.get(app)));
  }

  const results = [];
  for (let i = 0; i < apps.length; i++) {
    for (let j = i + 1; j < apps.length; j++) {
      const a = apps[i];
      const b = apps[j];
      const shared = [...salient.get(a)].filter(function inB(word) {
        return salient.get(b).has(word);
      });
      const exclusive = shared
        .filter(function onlyTheseTwo(word) {
          for (const other of apps) {
            if (other !== a && other !== b && salient.get(other).has(word)) return false;
          }
          return true;
        })
        .sort();
      const rawShared = [...longWords.get(a)]
        .filter(function inBLong(word) {
          return longWords.get(b).has(word);
        })
        .sort();
      const commonHours = [...hoursByApp.get(a)].filter(function inB(hour) {
        return hoursByApp.get(b).has(hour);
      });
      results.push({ a: a, b: b, exclusive: exclusive, rawShared: rawShared, commonHours: commonHours });
    }
  }

  const candidates = results.filter(function qualifies(item) {
    return item.exclusive.length === 3 && item.commonHours.length === 0;
  });
  candidates.sort(function bySize(a, b) {
    return b.rawShared.length - a.rawShared.length;
  });
  const pick = candidates.length ? candidates[0] : null;
  const details = results
    .filter(function hasWords(item) {
      return item.exclusive.length > 0;
    })
    .map(function entry(item) {
      return item.a + '/' + item.b + ' [' + item.exclusive.join(',') + ']' +
        (item.commonHours.length ? ' hours ' + item.commonHours.sort().join(',') : ' hours none');
    });
  const ok =
    pick !== null &&
    pick.rawShared.length === 3 &&
    pick.exclusive.length === 3 &&
    pick.commonHours.length === 0;
  report(
    '4 two apps share exactly three salient words and never share an hour',
    ok,
    (pick === null
      ? 'no qualifying pair'
      : pick.a + ' and ' + pick.b + ' share ' + pick.exclusive.join(', ') +
        ' (raw 5+ letter overlap: ' + pick.rawShared.length + ')') +
      '; exclusive overlaps: ' + (details.length ? details.join(' | ') : 'none')
  );
}

// ------------------------- 5. several window titles revisited across hours
function checkTitles() {
  const byTitle = new Map();
  for (const row of rows) {
    if (!row.title) continue;
    if (!byTitle.has(row.title)) byTitle.set(row.title, { hours: new Set(), visits: 0 });
    const entry = byTitle.get(row.title);
    entry.hours.add(localHour(row.t));
    entry.visits++;
  }
  const revisited = [];
  for (const [title, entry] of byTitle) {
    if (entry.hours.size >= 2) {
      revisited.push({
        title: title,
        hours: [...entry.hours].sort(function asc(a, b) {
          return a - b;
        }),
        visits: entry.visits
      });
    }
  }
  revisited.sort(function byVisits(a, b) {
    return b.visits - a.visits;
  });
  const shown = revisited.slice(0, 5).map(function entry(item) {
    return '"' + item.title + '" ' + item.visits + 'x in h' + item.hours.join(',');
  });
  report(
    '5 several window titles revisited in 2+ distinct local hours',
    revisited.length >= 3,
    revisited.length + ' titles revisited: ' + shown.join(' | ')
  );
}

// ---------- 6. two phrases repeated in two apps within ten minutes (parcels)
function checkParcels() {
  const hits = new Map();
  for (const row of rows) {
    for (const phrase of phrasesOf(row.text)) {
      if (!hits.has(phrase)) hits.set(phrase, []);
      hits.get(phrase).push({ t: row.t, app: row.app });
    }
  }
  const best = new Map();
  for (const [phrase, list] of hits) {
    for (let i = 0; i < list.length; i++) {
      for (let j = i + 1; j < list.length; j++) {
        const x = list[i];
        const y = list[j];
        if (x.app === y.app) continue;
        const delta = Math.abs(x.t - y.t);
        if (delta > PARCEL_MS) continue;
        const pair = x.app < y.app ? x.app + '|' + y.app : y.app + '|' + x.app;
        const key = phrase + '#' + pair;
        const previous = best.get(key);
        if (!previous || delta < previous.delta) {
          best.set(key, {
            phrase: phrase,
            a: x.app,
            b: y.app,
            at: Math.min(x.t, y.t),
            delta: delta
          });
        }
      }
    }
  }
  const events = [...best.values()].sort(function byDelta(a, b) {
    return a.delta - b.delta;
  });
  const shown = events.slice(0, 4).map(function entry(item) {
    return '"' + item.phrase + '" in ' + item.a + ' and ' + item.b + ' ' +
      Math.round(item.delta / 1000) + ' s apart';
  });
  report(
    '6 two parcels: same phrase in two apps within 10 minutes',
    events.length >= 2,
    events.length + ' event(s): ' + (shown.length ? shown.join(' | ') : 'none')
  );
}

checkShape();
checkFocusBlock();
checkFragmentedHour();
checkShortOpenApps();
checkSharedWords();
checkTitles();
checkParcels();

if (failures > 0) {
  console.log('FAILED: ' + failures + ' check(s)');
  process.exit(1);
}
console.log('OK: all six properties hold');
