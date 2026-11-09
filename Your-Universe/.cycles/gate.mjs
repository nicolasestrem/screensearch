// Static half of the cycle gate. Lives outside src/ and tools/ so check.mjs does not scan it.
// Usage: node .cycles/gate.mjs
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const steps = [
  ['assemble', 'tools/assemble.mjs', []],
  ['check', 'tools/check.mjs', []],
  ['check-dom', 'tools/check-dom.mjs', []],
  ['check-assets', 'tools/check-assets.mjs', []],
  ['stamp-sw', 'tools/stamp-sw.mjs', ['--check']],
  ['check-sample', 'tools/check-sample.mjs', []],
  ['smoke-data', 'tools/smoke-data.mjs', []],
  ['check-pixels', 'tools/check-pixels.mjs', ['screenshots/desktop-day.png']],
];

let failed = 0;
for (const [label, file, args] of steps) {
  const res = spawnSync(process.execPath, [file, ...args], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  const out = `${res.stdout || ''}${res.stderr || ''}`;
  const bad = out.split(/\r?\n/).filter((line) => /\bFAIL\b|\bnot ok\b|ERR!|^Error:|Cannot find module/.test(line));
  const ok = res.status === 0 && bad.length === 0;
  if (!ok) {
    failed += 1;
    console.log(`GATE ${label} FAIL exit=${res.status}`);
    for (const line of bad.slice(0, 25)) console.log(`   ${line.trim()}`);
    if (bad.length === 0) console.log(out.slice(-1500));
  } else {
    const tally = out.split(/\r?\n/).filter((l) => /\d+ *\/ *\d+|all PASS|passed, 0 failed/.test(l)).pop() || '';
    console.log(`GATE ${label} ok ${tally.trim()}`);
  }
}
console.log(failed === 0 ? 'GATE RESULT: PASS' : `GATE RESULT: FAIL (${failed} step(s))`);
process.exit(failed === 0 ? 0 : 1);
