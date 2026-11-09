// Stamp the service worker's cache version with a hash of the shipped asset manifest.
//
// The worker serves everything under assets/ cache-first, which is what makes the second visit
// instant and the offline visit possible, but it also means a changed pack is invisible to anyone
// who already loaded the page. The version therefore has to change whenever the payload changes,
// and a hand-typed version is exactly the kind of thing that gets forgotten when a pack is rebuilt.
//
// assets/manifest.json names every model, and the packs' own bytes carry the geometry and texture
// detail the manifest cannot see, so the fingerprint covers both. This writes it into assets/sw.js
// as `localtime-v<version>`; run it after pack-assets.mjs and before serving. Idempotent: a second
// run changes nothing.
//
//   node tools/stamp-sw.mjs           stamp
//   node tools/stamp-sw.mjs --check   exit 1 if the file is stale (used by check-dom.mjs)

import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CACHE_VERSION_LINE as LINE, computeCacheVersion } from './cache-version.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const SW = join(ROOT, 'assets', 'sw.js');

async function fingerprint() {
  return computeCacheVersion(ROOT);
}

const check = process.argv.includes('--check');

// computeCacheVersion already returns the stamped form (`m` + hex), so it is used verbatim.
const want = await fingerprint();
const sw = await readFile(SW, 'utf8');
const match = sw.match(LINE);
if (!match) {
  console.error(`stamp-sw: no CACHE_VERSION line in ${SW}`);
  process.exit(1);
}
const have = match[1];

if (have === want) {
  console.log(`stamp-sw: CACHE_VERSION ${have} matches the manifest and pack payload`);
} else if (check) {
  console.error(`stamp-sw: CACHE_VERSION is ${have} but the payload fingerprints to ${want}`);
  console.error('stamp-sw: run node tools/stamp-sw.mjs before serving or packing');
  process.exit(1);
} else {
  await writeFile(SW, sw.replace(LINE, `const CACHE_VERSION = '${want}';`), 'utf8');
  console.log(`stamp-sw: CACHE_VERSION ${have} -> ${want}`);
}
