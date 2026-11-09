// Cycle 29: summarize a Chrome trace into what actually held the GPU. Scratch tool.
//
//   node .cycles/wake29parse.mjs .cycles/wake29-trace.json
//
// Complete events carry a duration; everything else is a B/E pair on one thread, so the run is
// reconstructed here instead of trusting `dur` alone (the first attempt found no event over
// 20 ms because the long GPU tasks are B/E pairs).
import { readFileSync } from 'node:fs';

const path = process.argv[2] || '.cycles/wake29-trace.json';
const raw = JSON.parse(readFileSync(path, 'utf8'));
const events = raw.traceEvents || raw;

const procName = new Map();
for (const e of events) {
  if (e.name === 'process_name' && e.args && e.args.name) procName.set(e.pid, e.args.name);
}
const threadName = new Map();
for (const e of events) {
  if (e.name === 'thread_name' && e.args && e.args.name) threadName.set(e.pid + ':' + e.tid, e.args.name);
}

// Complete events.
const complete = [];
for (const e of events) {
  if (typeof e.dur === 'number') complete.push(e);
}

// B/E pairs, per pid:tid per name.
const open = new Map();
const spans = [];
for (const e of events) {
  if (e.ph === 'B') {
    const key = e.pid + ':' + e.tid + ':' + e.name;
    if (!open.has(key)) open.set(key, []);
    open.get(key).push(e.ts);
  } else if (e.ph === 'E') {
    const key = e.pid + ':' + e.tid + ':' + e.name;
    const stack = open.get(key);
    if (stack && stack.length) {
      const ts = stack.pop();
      spans.push({ name: e.name, cat: e.cat, pid: e.pid, tid: e.tid, dur: e.ts - ts, ts });
    }
  }
}

const all = complete.concat(spans);
const label = (e) => (procName.get(e.pid) || String(e.pid)) + ' / ' + (threadName.get(e.pid + ':' + e.tid) || e.tid);

const longest = all.filter((e) => e.dur > 3000).sort((a, b) => b.dur - a.dur);

const totals = new Map();
for (const e of all) {
  const key = (procName.get(e.pid) || e.pid) + ' | ' + e.name;
  totals.set(key, (totals.get(key) || 0) + e.dur);
}
const topTotals = Array.from(totals.entries()).sort((a, b) => b[1] - a[1]).slice(0, 30);

console.log('events', events.length, 'complete', complete.length, 'pairs', spans.length);
console.log('\n--- longest spans (>3 ms), top 40 ---');
for (const e of longest.slice(0, 40)) {
  console.log((e.dur / 1000).toFixed(1).padStart(9), 'ms  ', e.name.padEnd(34), e.cat.slice(0, 30).padEnd(31), label(e));
}
console.log('\n--- total by process | name, top 30 ---');
for (const [k, v] of topTotals) {
  console.log((v / 1000).toFixed(0).padStart(8), 'ms  ', k);
}
