// Writes sample-export.json from the simulated day generator.
//
// The bytes written here are the contract: main.mjs falls back to the same
// function when the file is not reachable, so both paths must agree exactly.

import { writeFileSync } from 'node:fs';
import { generateSimulatedDay } from '../src/simday.mjs';

const target = new URL('../sample-export.json', import.meta.url);
const day = generateSimulatedDay();
const json = JSON.stringify(day, null, 1) + '\n';

writeFileSync(target, json, 'utf8');

console.log('wrote sample-export.json');
console.log('frames: ' + day.frames.length);
console.log('bytes: ' + json.length);
