// Cycle 25, owner item 1: one gate per review defect. Scratch tool, not part of the deliverable.
// Usage: node .cycles/defects25.mjs
//
// (a) is behavioural on the real data module: the old gate (longestSessionBefore) stays on for
// the rest of the day after the focus block closes, the new gate (activeSessionAt) does not.
// (b), (c) and (d) are source gates with ordered positions, because their effects live in the
// DOM/WebGL teardown that Node cannot build.
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyzeExport } from '../src/data.mjs';
import { generateSimulatedDay } from '../src/simday.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(resolve(root, p), 'utf8');
const MIN = 60 * 1000;

let failed = 0;
function gate(name, ok, detail) {
  console.log((ok ? 'PASS ' : 'FAIL ') + name + (detail ? '  ' + detail : ''));
  if (!ok) failed += 1;
}

// (a) green wave follows the live session, not the longest one ever started.
const agentsSrc = read('src/agents.mjs');
const model = analyzeExport(generateSimulatedDay(), { tzOffsetMs: 60 * MIN });
const block = model.sessions.reduce((a, b) => (b.durationMs > a.durationMs ? b : a));
const nextSession = model.sessions[model.sessions.indexOf(block) + 1];
const inside = model.activeSessionAt(block.start + 90 * MIN);
// The first break after the focus block: any point in it is outside every session, while the
// block itself is still the longest session that has started, so the two gates disagree there.
const breakT = block.end + Math.max(1000, (nextSession.start - block.end) / 2);
const after = model.activeSessionAt(breakT);
const longestAfter = model.longestSessionBefore(breakT);
const oldGate = !!longestAfter && typeof longestAfter.q === 'number' && longestAfter.q >= 0;
const newGate = !!after && after.q >= 0;
gate(
  '(a) green wave is on inside the focus block and off after it closes',
  inside !== null && inside.q >= 0 && oldGate === true && newGate === false,
  `inside q=${inside && inside.q} afterOld=${oldGate} afterNew=${newGate}`
);
gate(
  '(a) agents.mjs reads activeSessionAt and keeps no longestSessionBefore helper',
  /modelRef\.activeSessionAt\(tData\)/.test(agentsSrc) &&
    !/safeLongest/.test(agentsSrc) &&
    !/longestSessionBefore/.test(agentsSrc)
);

// (b) the dream API exposes the card clear and install calls it before disposing the group.
const dreamSrc = read('src/dream.mjs');
const mainSrc = read('src/main.mjs');
const dreamApi = /return \{[\s\S]*?clearCards: hideCards,[\s\S]*?\};/.test(dreamSrc);
const installAt = mainSrc.indexOf('function install(nextModel) {');
const installBody = mainSrc.slice(installAt, mainSrc.indexOf('function wrapGoldenLights') === installAt ? installAt + 4000 : mainSrc.indexOf('\n  }\n', installAt));
const clearAt = installBody.indexOf('if (dream) dream.clearCards();');
const disposeAt = installBody.indexOf('disposeGroup(dream ? dream.group : null);');
gate(
  '(b) dream.clearCards exists and install clears cards before disposing the dream group',
  dreamApi && clearAt >= 0 && disposeAt > clearAt,
  `api=${dreamApi} clear@${clearAt} dispose@${disposeAt}`
);

// (c) a dropped export wins over the boot sample, for the install and the portrait default.
const bootAt = mainSrc.indexOf('function boot() {');
const bootBody = mainSrc.slice(bootAt, mainSrc.indexOf('function debugFraming()', bootAt));
const guardedInstall = /if \(!userModel\) \{\s*install\(next\);[\s\S]*?rig\.setTier\('block', false\);[\s\S]*?\n      \}/.test(bootBody);
const loadsAt = mainSrc.indexOf('function loadExport(exportObject) {');
const loadBody = mainSrc.slice(loadsAt, mainSrc.indexOf('\n  }\n', loadsAt));
const setAt = loadBody.indexOf('userModel = next;');
const loadInstallAt = loadBody.indexOf('install(next);');
const declared = /let userModel = null;/.test(mainSrc);
gate(
  '(c) userModel is module state, set before the load install, and guards the boot install',
  declared && guardedInstall && setAt >= 0 && loadInstallAt > setAt,
  `declared=${declared} guarded=${guardedInstall} set@${setAt} install@${loadInstallAt}`
);

// (d) disposeGroup releases a light's shadow map before the material early return.
const disposeAtSrc = mainSrc.indexOf('function disposeGroup(group) {');
const disposeBody = mainSrc.slice(disposeAtSrc, mainSrc.indexOf('// ---------------- labels', disposeAtSrc));
const shadowAt = disposeBody.indexOf('object.shadow.map.dispose();');
const nullAt = disposeBody.indexOf('object.shadow.map = null;');
const materialAt = disposeBody.indexOf('const material = object.material;');
gate(
  '(d) disposeGroup disposes and clears the shadow map before the no-material return',
  shadowAt >= 0 && nullAt > shadowAt && materialAt > nullAt,
  `shadow@${shadowAt} null@${nullAt} material@${materialAt}`
);

console.log(failed === 0 ? 'DEFECTS25: PASS' : `DEFECTS25: FAIL (${failed})`);
process.exit(failed === 0 ? 0 : 1);
