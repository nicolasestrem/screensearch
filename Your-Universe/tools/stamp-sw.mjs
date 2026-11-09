// Stamp the service worker's cache version with a hash of the shipped asset manifest.
//
// The worker serves everything under assets/ cache-first, which is what makes the second visit
// instant and the offline visit possible, but it also means a changed pack is invisible to anyone
// who already loaded the page. The version therefore has to change whenever the payload changes,
// and a hand-typed version is exactly the kind of thing that gets forgotten when a pack is rebuilt.
//
// assets/manifest.json names every model and its digest, so hashing it is the payload fingerprint.
// This writes that fingerprint into assets/sw.js as `localtime-v<8 hex>`; run it after
// pack-assets.mjs and before serving. Idempotent: a second run changes nothing.
//
//   node tools/stamp-sw.mjs           stamp
//   node tools/stamp-sw.mjs --check   exit 1 if the file is stale (used by check-dom.mjs)

import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const MANIFEST = join(ROOT, 'assets', 'manifest.json');
const SW = join(ROOT, 'assets', 'sw.js');
const LINE = /^const CACHE_VERSION = '([^']*)';$/m;

async function fingerprint() {
  const bytes = await readFile(MANIFEST);
  return createHash('sha256').update(bytes).digest('hex').slice(0, 8);
}

const check = process.argv.includes('--check');

const hash = await fingerprint();
const want = `m${hash}`;
const sw = await readFile(SW, 'utf8');
const match = sw.match(LINE);
if (!match) {
  console.error(`stamp-sw: no CACHE_VERSION line in ${SW}`);
  process.exit(1);
}
const have = match[1];

if (have === want) {
  console.log(`stamp-sw: CACHE_VERSION ${have} matches assets/manifest.json`);
} else if (check) {
  console.error(`stamp-sw: CACHE_VERSION is ${have} but the manifest hashes to ${want}`);
  console.error('stamp-sw: run node tools/stamp-sw.mjs before serving or packing');
  process.exit(1);
} else {
  await writeFile(SW, sw.replace(LINE, `const CACHE_VERSION = '${want}';`), 'utf8');
  console.log(`stamp-sw: CACHE_VERSION ${have} -> ${want}`);
}
