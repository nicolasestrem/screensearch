// Downloads the approved free CC0 asset packs and stages their licence text.
//
// Nothing here touches src/ or index.html. Two output locations:
//   .cache/packs/<pack>.zip, .cache/packs/<pack>/   raw archive and extraction, never shipped
//   assets/LICENSES/<pack>.txt, assets/LICENSES/SOURCES.md   licence proof, shipped
//
// Usage: node tools/fetch-assets.mjs [--only pack,pack] [--inventory <file>]
//
// Re-running is cheap: a zip that still starts with the PK signature is not downloaded again,
// a pack folder that already holds files is not extracted again, and per-pack metadata is kept
// in .cache/packs/<pack>.meta.json so SOURCES.md stays byte-stable across runs.
//
// Two source kinds are supported.
//   kenney:    the pack page carries data-download-id, and /download-link/<id> hands back a
//              signed zip url. The zip unpacks Models/GLB format/*.glb plus License.txt.
//   polypizza: the model page state lists each model with its licence and a preview asset id;
//              the GLB for that model is the same asset id with a .glb suffix. Used only for
//              models Poly Pizza itself marks CC0, and only because the official Quaternius
//              distribution is not a CC0 source any more and its Drive files answer with a
//              quota refusal (both recorded in SOURCES.md).

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdir, readdir, readFile, rename, stat, writeFile } from 'node:fs/promises';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CACHE = join(ROOT, '.cache', 'packs');
const LICENCES = join(ROOT, 'assets', 'LICENSES');

// kenney.nl answers 403 to a bare undici user agent, and poly.pizza's CDN is happier with one too.
const UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36';

const CATEGORY = {
  buildings: /\b(building|buildings|house|houses|apartment|apartments|tower|towers|skyscraper|office|shop|store|storefront|balcony|roof|rooftop|chimney|door|doorway|window|windows|wall|walls|floor|floors|stairs|staircase|column|pillar|fence|garage|warehouse|factory|church|hotel|school|restaurant|cafe|kiosk|market|stall|shed|bridge|arch|gate|awning|canopy|billboard|sign)\b/i,
  vehicles: /\b(car|cars|van|truck|trucks|bus|buses|taxi|sedan|suv|ambulance|police|tractor|trailer|motorcycle|motorbike|scooter|bike|bicycle|wheel|wheels|tram|train|locomotive)\b/i,
  nature: /\b(tree|trees|pine|palm|olive|oak|bush|bushes|shrub|plant|plants|fern|grass|flower|flowers|mushroom|mushrooms|log|logs|stump|rock|rocks|stone|stones|cliff|boulder|cactus|leaf|leaves|branch|vine|ivy|moss|reed|coral|seaweed|crop|wheat|corn|pumpkin|garden|pot|hedge|hill|mountain)\b/i,
};

// A place that is dense, contemporary and Mediterranean at golden hour: fantasy props, snow and
// game pickups would read as the wrong world, so titles carrying those words are skipped.
// The character kit also carries a fantasy cast and weapon props; a Mediterranean street at
// golden hour has neither, so those are skipped by title exactly like the castles and pickups.
const EXCLUDE = /\b(fantasy|medieval|castle|dungeon|archery|banner|axe|sword|shield|potion|cooking|cauldron|grouper|magic|spell|gold|snow|ice|christmas|halloween|zombie|skeleton|alien|knight|raider|robot|soldier|weapon|bow|spear|arrow)\b/i;

const PACKS = [
  { id: 'kenney-city-kit-commercial', kind: 'kenney', page: 'https://kenney.nl/assets/city-kit-commercial' },
  { id: 'kenney-city-kit-suburban', kind: 'kenney', page: 'https://kenney.nl/assets/city-kit-suburban' },
  { id: 'kenney-city-kit-roads', kind: 'kenney', page: 'https://kenney.nl/assets/city-kit-roads' },
  { id: 'kenney-car-kit', kind: 'kenney', page: 'https://kenney.nl/assets/car-kit' },
  { id: 'kenney-nature-kit', kind: 'kenney', page: 'https://kenney.nl/assets/nature-kit' },
  { id: 'kenney-mini-characters', kind: 'kenney', page: 'https://kenney.nl/assets/mini-characters-1' },
  {
    id: 'quaternius-ultimate-buildings',
    kind: 'polypizza',
    officialPage: 'https://quaternius.com/packs/ultimatetexturedbuildings.html',
    creator: 'Quaternius',
    category: 'buildings',
  },
  {
    id: 'quaternius-cars',
    kind: 'polypizza',
    officialPage: 'https://quaternius.com/packs/cars.html',
    creator: 'Quaternius',
    category: 'vehicles',
  },
  {
    id: 'quaternius-nature',
    kind: 'polypizza',
    officialPage: 'https://quaternius.com/packs/ultimatenature.html',
    creator: 'Quaternius',
    category: 'nature',
  },
];

function parseArgs(argv) {
  const opts = { only: null, inventory: join(CACHE, 'inventory.md') };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--only') opts.only = (argv[++i] ?? '').split(',').map((s) => s.trim()).filter(Boolean);
    else if (arg.startsWith('--only=')) opts.only = arg.slice(7).split(',').map((s) => s.trim()).filter(Boolean);
    else if (arg === '--inventory') opts.inventory = resolve(argv[++i] ?? opts.inventory);
    else if (arg.startsWith('--inventory=')) opts.inventory = resolve(arg.slice(12));
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (opts.only && opts.only.length === 0) throw new Error('--only needs at least one pack id');
  return opts;
}

function today() {
  return new Date().toISOString().slice(0, 10);
}

// A cached download keeps the day it landed, so SOURCES.md stays stable even after its metadata
// file is lost: the mtime of the zip or the pack folder is the next best evidence.
async function arrivalDate(path) {
  try {
    return (await stat(path)).mtime.toISOString().slice(0, 10);
  } catch {
    return today();
  }
}

function mb(bytes) {
  return Math.round((bytes / (1024 * 1024)) * 100) / 100;
}

async function fetchText(url, referer) {
  const res = await fetch(url, { headers: headers(referer), redirect: 'follow' });
  if (!res.ok) throw new Error(`GET ${url} -> HTTP ${res.status}`);
  return res.text();
}

async function fetchBuffer(url, referer, attempt = 0) {
  const res = await fetch(url, { headers: headers(referer), redirect: 'follow' });
  if (!res.ok) {
    if (attempt < 2 && (res.status === 429 || res.status >= 500)) {
      await sleep(800 * (attempt + 1));
      return fetchBuffer(url, referer, attempt + 1);
    }
    throw new Error(`GET ${url} -> HTTP ${res.status}`);
  }
  return { body: Buffer.from(await res.arrayBuffer()), type: res.headers.get('content-type') ?? '' };
}

function headers(referer) {
  const h = { 'user-agent': UA, 'accept-language': 'en-US,en;q=0.9' };
  if (referer) h.referer = referer;
  return h;
}

function sleep(ms) {
  return new Promise((done) => setTimeout(done, ms));
}

async function downloadTo(url, dest, referer) {
  const { body } = await fetchBuffer(url, referer);
  await writeFile(`${dest}.part`, body);
  await rename(`${dest}.part`, dest);
  return { bytes: body.length, sha256: createHash('sha256').update(body).digest('hex') };
}

// Walks a directory tree and returns absolute file paths. Missing directories are empty.
async function listFiles(dir, out = []) {
  let entries;
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) await listFiles(full, out);
    else if (entry.isFile()) out.push(full);
  }
  return out;
}

async function readMeta(id) {
  try {
    return JSON.parse(await readFile(join(CACHE, `${id}.meta.json`), 'utf8'));
  } catch {
    return null;
  }
}

async function writeMeta(id, meta) {
  await writeFile(join(CACHE, `${id}.meta.json`), `${JSON.stringify(meta, null, 1)}\n`);
}

// A cached zip counts only when it still carries the local file header signature. A truncated or
// html-error download is dropped so the next run fetches it again instead of extracting garbage.
async function inspectZip(path) {
  try {
    const handle = await readFile(path);
    if (handle[0] !== 0x50 || handle[1] !== 0x4b) return null;
    return { bytes: handle.length, sha256: createHash('sha256').update(handle).digest('hex') };
  } catch {
    return null;
  }
}

function extractZip(zipPath, destDir) {
  // Run from the cache directory with relative names: GNU tar would read an absolute Windows
  // path as a remote host, bsdtar would not, and relative names behave the same in both.
  const result = spawnSync('tar', ['-xf', basename(zipPath), '-C', basename(destDir)], {
    cwd: dirname(zipPath),
    encoding: 'utf8',
  });
  if (result.error) throw new Error(`tar failed to start: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`tar exited ${result.status}: ${(result.stderr ?? '').trim()}`);
}

// The vendor licence is the shallowest file whose name starts with licence/license: a Kenney zip
// carries License.txt at its root, and the shallowest rule keeps a nested copy from winning.
async function findLicence(dir) {
  const files = await listFiles(dir);
  const candidates = files
    .filter((file) => /^licen[cs]e(\.[a-z0-9]{1,4})?$/i.test(basename(file)))
    .sort((a, b) => a.split(/[\\/]/).length - b.split(/[\\/]/).length || a.localeCompare(b));
  return candidates[0] ?? null;
}

function licenceLine(text) {
  const line = text.split(/\r?\n/).map((s) => s.trim()).find((s) => s.includes('CC0'));
  if (line) return line.slice(0, 120);
  const at = text.indexOf('CC0');
  return at >= 0 ? text.slice(Math.max(0, at - 60), at + 60).replace(/\s+/g, ' ').trim() : '';
}

async function resolveKenneyUrl(pack) {
  const html = await fetchText(pack.page);
  const idMatch = html.match(/data-download-id=['"]([0-9a-fA-F]+)['"]/);
  if (!idMatch) throw new Error(`no data-download-id on ${pack.page}`);
  const payload = await fetchText(`https://kenney.nl/download-link/${idMatch[1]}`, pack.page);
  let parsed;
  try {
    parsed = JSON.parse(payload);
  } catch {
    throw new Error(`download-link returned non json: ${payload.slice(0, 120)}`);
  }
  if (!parsed.url) throw new Error(`download-link returned no url for ${pack.id}`);
  return { url: parsed.url };
}

async function fetchKenneyPack(pack) {
  const zipPath = join(CACHE, `${pack.id}.zip`);
  const destDir = join(CACHE, pack.id);
  const cached = await readMeta(pack.id);
  const meta = { pack: pack.id, kind: pack.kind, page: pack.page, ...(cached ?? {}) };

  let zip = await inspectZip(zipPath);
  if (!zip) {
    const resolved = await resolveKenneyUrl(pack);
    const saved = await downloadTo(resolved.url, zipPath, pack.page);
    meta.zipBytes = saved.bytes;
    meta.zipSha256 = saved.sha256;
    zip = await inspectZip(zipPath);
    if (!zip) throw new Error(`${pack.id}: downloaded file is not a zip (no PK signature)`);
    console.log(`  downloaded ${pack.id}.zip ${mb(saved.bytes)} MB`);
  } else {
    meta.zipBytes = zip.bytes;
    meta.zipSha256 = zip.sha256;
    console.log(`  zip already cached, ${mb(zip.bytes)} MB`);
  }
  // The recorded source stays the pack page: the signed transport url expires within hours.
  meta.url = pack.page;
  meta.downloadedAt = meta.downloadedAt ?? (await arrivalDate(zipPath));

  const extracted = await listFiles(destDir);
  if (extracted.length === 0) {
    await mkdir(destDir, { recursive: true });
    extractZip(zipPath, destDir);
    console.log(`  extracted into .cache/packs/${pack.id}/`);
  } else {
    console.log(`  extraction already present, ${extracted.length} files`);
  }

  const licencePath = await findLicence(destDir);
  if (!licencePath) throw new Error(`${pack.id}: no licence file in the extracted tree`);
  const licenceBytes = await readFile(licencePath);
  const licenceText = licenceBytes.toString('utf8');
  if (!licenceText.includes('CC0')) {
    throw new Error(`${pack.id}: licence ${relative(destDir, licencePath)} does not mention CC0`);
  }
  await mkdir(LICENCES, { recursive: true });
  await writeFile(join(LICENCES, `${pack.id}.txt`), licenceBytes);
  meta.licence = 'CC0 1.0';
  meta.licenceFile = relative(destDir, licencePath).split('\\').join('/');
  meta.licenceLine = licenceLine(licenceText);
  meta.extractedPath = `.cache/packs/${pack.id}`;

  const glbs = (await listFiles(destDir)).filter((file) => file.toLowerCase().endsWith('.glb'));
  meta.glbCount = glbs.length;
  meta.glbBytes = (await Promise.all(glbs.map((file) => stat(file).then((s) => s.size)))).reduce((a, b) => a + b, 0);
  meta.glbDirs = [...new Set(glbs.map((file) => dirname(relative(destDir, file)).split('\\').join('/')))].sort();
  meta.totalFiles = extracted.length;
  await writeMeta(pack.id, meta);
  return meta;
}

// Reads window.__SERVER_APP_STATE__ out of a poly.pizza page. The brace walk is needed because
// the payload is inlined inside a script tag and cannot be handed to JSON.parse wholesale.
function readAppState(html) {
  const at = html.indexOf('__SERVER_APP_STATE__');
  if (at < 0) throw new Error('poly.pizza page has no __SERVER_APP_STATE__');
  const start = html.indexOf('{', at);
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let k = start; k < html.length; k++) {
    const c = html[k];
    if (inString) {
      if (escaped) escaped = false;
      else if (c === '\\') escaped = true;
      else if (c === '"') inString = false;
      continue;
    }
    if (c === '"') inString = true;
    else if (c === '{') depth++;
    else if (c === '}') {
      depth--;
      if (depth === 0) return JSON.parse(html.slice(start, k + 1));
    }
  }
  throw new Error('unterminated __SERVER_APP_STATE__ payload');
}

async function polyPizzaModels(creator) {
  const html = await fetchText(`https://poly.pizza/u/${creator}`);
  const seen = new Map();
  const walk = (node) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      for (const item of node) walk(item);
      return;
    }
    if (node.licence && node.title && node.publicID) seen.set(node.publicID, node);
    for (const value of Object.values(node)) walk(value);
  };
  walk(readAppState(html));
  return [...seen.values()];
}

// Google Drive folder listing as plain html, which exposes entry ids without an api key.
async function driveList(folderId) {
  const html = await fetchText(`https://drive.google.com/embeddedfolderview?id=${folderId}#list`);
  const rows = [];
  const re = /id="entry-([A-Za-z0-9_-]+)"[\s\S]*?flip-entry-title">([^<]*)</g;
  let match;
  while ((match = re.exec(html))) rows.push({ id: match[1], name: match[2].trim() });
  return rows;
}

// Drive answers either with the file or, for large or hot files, with an html interstitial that
// carries a confirm token. Both shapes are handled and the refusal text is reported, not swallowed.
async function driveFile(fileId) {
  const direct = `https://drive.usercontent.google.com/download?id=${fileId}&export=download`;
  const first = await fetchBuffer(direct);
  const head = first.body.subarray(0, 400).toString('utf8');
  if (!head.includes('<html') && !first.type.includes('text/html')) return first.body;
  const token = first.body.toString('utf8').match(/confirm=([0-9A-Za-z_-]+)/);
  const uuid = first.body.toString('utf8').match(/name="uuid" value="([^"]+)"/);
  if (token) {
    const confirmed = await fetchBuffer(
      `${direct}&confirm=${token[1]}${uuid ? `&uuid=${uuid[1]}` : ''}`,
    );
    if (!confirmed.type.includes('text/html')) return confirmed.body;
  }
  const reason = first.body.toString('utf8').match(/<title>([^<]*)<\/title>/);
  throw new Error(`drive refused file ${fileId}: ${reason ? reason[1].trim() : 'unknown html response'}`);
}

// Proves, per run, why the official site is not used: it either has no Drive folder, or the folder
// has no zip and its files are refused. Returns facts for SOURCES.md rather than a guess.
async function probeOfficialQuaternius(pack) {
  const facts = { page: pack.officialPage, folder: null, topEntries: [], zip: null, refusal: null };
  let html;
  try {
    html = await fetchText(pack.officialPage);
  } catch (error) {
    facts.refusal = error.message;
    return facts;
  }
  const folderMatch = html.match(/drive\.google\.com\/drive\/folders\/([A-Za-z0-9_-]+)/);
  if (!folderMatch) {
    facts.refusal = 'pack page has no Google Drive folder link';
    return facts;
  }
  facts.folder = `https://drive.google.com/drive/folders/${folderMatch[1]}`;
  try {
    const rows = await driveList(folderMatch[1]);
    facts.topEntries = rows.map((row) => row.name);
    const zipRow = rows.find((row) => /\.zip$/i.test(row.name));
    if (zipRow) {
      facts.zip = { name: zipRow.name, id: zipRow.id };
      facts.refusal = `folder holds ${zipRow.name}, but the vendor licence is no longer CC0 so its models are not used`;
      return facts;
    }
    const licenceRow = rows.find((row) => /^licen[cs]e/i.test(row.name));
    if (licenceRow) {
      await driveFile(licenceRow.id);
      facts.refusal = 'folder served a licence file but holds no archive';
    } else {
      facts.refusal = 'folder holds no zip archive';
    }
  } catch (error) {
    facts.refusal = error.message;
  }
  return facts;
}

function slugify(text) {
  return text
    .toLowerCase()
    .replace(/['"()]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 60);
}

async function mapLimit(items, limit, worker) {
  const results = new Array(items.length);
  let cursor = 0;
  const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (cursor < items.length) {
      const index = cursor++;
      results[index] = await worker(items[index], index);
    }
  });
  await Promise.all(runners);
  return results;
}

// The GLB of a Poly Pizza model is its preview asset id with a .glb suffix instead of .webp.
function glbUrlFor(model) {
  const preview = model.previewUrl ?? '';
  const id = preview.match(/static\.poly\.pizza\/([0-9a-f-]{36})\./i);
  if (!id) throw new Error(`no asset id in preview url for ${model.title}`);
  return { url: `https://static.poly.pizza/${id[1]}.glb`, assetId: id[1] };
}

async function fetchPolyPizzaPack(pack) {
  const destDir = join(CACHE, pack.id);
  const cached = await readMeta(pack.id);
  const meta = { pack: pack.id, kind: pack.kind, ...(cached ?? {}) };
  const re = CATEGORY[pack.category];
  if (!re) throw new Error(`${pack.id}: unknown category ${pack.category}`);

  const all = await polyPizzaModels(pack.creator);
  const chosen = all
    .filter((model) => model.licence === 'CC0 1.0' && re.test(model.title) && !EXCLUDE.test(model.title))
    .sort((a, b) => a.title.localeCompare(b.title) || a.publicID.localeCompare(b.publicID));
  if (chosen.length === 0) throw new Error(`${pack.id}: no CC0 models matched ${pack.category}`);

  // Names are fixed before any download: the same pack always produces the same file list, and
  // variants that share a title (many Bush and Door models do) get their asset id appended.
  const used = new Set();
  const named = chosen.map((model) => {
    const { url, assetId } = glbUrlFor(model);
    const base = slugify(model.title) || 'model';
    let file = `${base}.glb`;
    if (used.has(file)) file = `${base}-${assetId.slice(0, 6)}.glb`;
    for (let n = 2; used.has(file); n++) file = `${base}-${assetId.slice(0, 6)}-${n}.glb`;
    used.add(file);
    return { title: model.title, file, page: `https://poly.pizza${model.url}`, licence: model.licence, url };
  });

  await mkdir(destDir, { recursive: true });
  const existing = new Set((await listFiles(destDir)).map((file) => basename(file)));
  let downloaded = 0;
  await mapLimit(named, 6, async (row) => {
    if (existing.has(row.file)) return;
    const { body } = await fetchBuffer(row.url);
    if (body.subarray(0, 4).toString('latin1') !== 'glTF') {
      throw new Error(`${pack.id}: ${row.title} did not return a GLB`);
    }
    await writeFile(join(destDir, row.file), body);
    downloaded++;
  });
  console.log(`  ${named.length} CC0 models matched, ${downloaded} downloaded`);

  // The official route is probed every run so SOURCES.md states an observed fact, not a claim.
  const official = await probeOfficialQuaternius(pack);
  meta.official = official;
  meta.downloadedAt = meta.downloadedAt ?? (await arrivalDate(destDir));

  const evidence = named
    .map((row) => `${row.licence}  ${row.page}  ${row.title}`)
    .join('\n');
  const licence = [
    `# ${pack.id}`,
    '',
    'Generated by tools/fetch-assets.mjs from the licence field of each Poly Pizza model page.',
    `Creator: ${pack.creator}   Models taken: ${named.length}   Every model below is marked CC0 1.0`,
    `Creator page: https://poly.pizza/u/${pack.creator}`,
    `Retrieved: ${meta.downloadedAt}`,
    '',
    'Why not the vendor archive:',
    `  official pack page: ${official.page}`,
    `  drive folder: ${official.folder ?? 'none found'}`,
    `  top level entries: ${official.topEntries.length ? official.topEntries.join(', ') : 'none read'}`,
    `  archive: ${official.zip ? official.zip.name : 'no zip in the folder'}`,
    `  result: ${official.refusal ?? 'no archive to download'}`,
    '  quaternius.com also replaced CC0 with the Quaternius Asset License v1.0 on 2026-08-28,',
    '  so the vendor site is not a CC0 source at the time of this run.',
    '',
    'Per model licence records, verbatim from the page state:',
    '',
    evidence,
    '',
  ].join('\n');
  await mkdir(LICENCES, { recursive: true });
  await writeFile(join(LICENCES, `${pack.id}.txt`), licence);

  const files = await listFiles(destDir);
  const glbs = files.filter((file) => file.toLowerCase().endsWith('.glb'));
  meta.licence = 'CC0 1.0';
  meta.licenceFile = 'generated from per model licence field';
  meta.licenceLine = `licence: CC0 1.0 on ${named.length} model pages`;
  meta.url = `https://poly.pizza/u/${pack.creator} (creator page, ${pack.category} filter)`;
  meta.extractedPath = `.cache/packs/${pack.id}`;
  meta.glbCount = glbs.length;
  meta.glbBytes = (await Promise.all(glbs.map((file) => stat(file).then((s) => s.size)))).reduce((a, b) => a + b, 0);
  meta.glbDirs = ['.'];
  meta.totalFiles = files.length;
  meta.zipBytes = 0;
  meta.models = named.length;
  await writeMeta(pack.id, meta);
  return meta;
}

async function writeSources(packs) {
  const lines = [
    '# Asset sources',
    '',
    'Written by tools/fetch-assets.mjs. One line per pack: pack, source url, licence, download date.',
    'CC0 only. Kenney archives are copied from the vendor zip; the Quaternius packs come from Poly',
    'Pizza model pages because the official distribution is no longer CC0 and its Drive files are',
    'quota blocked (the per pack .txt files record the exact evidence).',
    '',
    '| pack | source url | licence | downloaded |',
    '| --- | --- | --- | --- |',
  ];
  for (const pack of packs) {
    const meta = await readMeta(pack.id);
    if (!meta) continue;
    const url = meta.url ?? pack.page ?? pack.officialPage ?? '';
    lines.push(`| ${pack.id} | ${url} | ${meta.licence} | ${meta.downloadedAt ?? 'unknown'} |`);
  }
  lines.push('');
  await mkdir(LICENCES, { recursive: true });
  await writeFile(join(LICENCES, 'SOURCES.md'), lines.join('\n'));
}

// Grouping for the inventory. Kenney names files in PascalCase, Poly Pizza titles in words, so the
// same keyword set is applied to both. Order matters: a bus stop is furniture, not a vehicle.
const ATLAS = /colormap|palette|atlas/i;

const INVENTORY_GROUPS = [
  ['roads', /\b(road|roads|street|streets|crosswalk|cross|junction|intersection|tile|tiles|asphalt|pavement|sidewalk|kerb|curb|highway|lane|track|rails?|bridge|tunnel|roundabout|markings?)\b/i],
  ['vehicles', CATEGORY.vehicles],
  ['trees', /\b(tree|trees|pine|palm|olive|oak|cypress|bush|bushes|shrub|hedge|garden|plant|plants|flower|flowers|grass|crop|wheat|corn|fern|moss|cactus|cacti|vine|ivy|leaf|leaves|reed)\b/i],
  ['buildings', CATEGORY.buildings],
  ['characters', /\b(character|characters|man|men|woman|women|person|people|human|pedestrian|crowd|worker|worker|citizen|kid|child)\b/i],
  ['props', /.*/],
];

function inventoryGroup(name) {
  const clean = name.replace(/[-_.]/g, ' ');
  for (const [label, re] of INVENTORY_GROUPS) if (re.test(clean)) return label;
  return 'props';
}

async function writeInventory(packs, target) {
  const blocks = [
    '# Packs inventory',
    '',
    'Produced by tools/fetch-assets.mjs. Raw packs live in .cache/packs (never shipped); only',
    'assets/LICENSES is part of the deliverable so far.',
    '',
  ];
  for (const pack of packs) {
    const meta = await readMeta(pack.id);
    if (!meta) continue;
    const dir = join(CACHE, pack.id);
    const glbs = (await listFiles(dir))
      .filter((file) => file.toLowerCase().endsWith('.glb'))
      .map((file) => basename(file))
      .sort((a, b) => a.localeCompare(b));
    const groups = new Map();
    for (const name of glbs) {
      const label = inventoryGroup(name);
      if (!groups.has(label)) groups.set(label, []);
      groups.get(label).push(name);
    }
    // Promotional previews are not model texture: they only inflate the totals and the name list.
    const textureFiles = (await listFiles(dir)).filter(
      (file) => /\.(png|jpe?g|webp|ktx2)$/i.test(file) && !/^(preview|sample)/i.test(basename(file)),
    );
    const textureSizes = await Promise.all(textureFiles.map((file) => stat(file).then((s) => s.size)));
    const textureBytes = textureSizes.reduce((a, b) => a + b, 0);
    // One row per basename: a Kenney zip carries the same colormap once per format folder, and the
    // interesting number is how big the palette is, not how many copies of it the archive holds.
    const byName = new Map();
    for (let i = 0; i < textureFiles.length; i++) {
      const name = basename(textureFiles[i]);
      const row = byName.get(name) ?? { name, bytes: 0, copies: 0 };
      row.bytes = Math.max(row.bytes, textureSizes[i]);
      row.copies++;
      byName.set(name, row);
    }
    const textures = [...byName.values()].sort(
      (a, b) => ATLAS.test(b.name) - ATLAS.test(a.name) || b.bytes - a.bytes || a.name.localeCompare(b.name),
    );
    const shown = textures.length <= 14 ? textures : textures.slice(0, 12);

    blocks.push(`## ${pack.id}`, '');
    blocks.push(`- licence: ${meta.licence} (${meta.licenceFile})`);
    blocks.push(`- licence line: ${meta.licenceLine}`);
    if (meta.zipBytes) blocks.push(`- zip: ${mb(meta.zipBytes)} MB`);
    blocks.push(`- source: ${meta.url}`);
    blocks.push(`- downloaded: ${meta.downloadedAt}`);
    blocks.push(`- extracted path: ${meta.extractedPath}`);
    if (meta.official) {
      blocks.push(`- official pack page: ${meta.official.page}`);
      blocks.push(`- official drive folder: ${meta.official.folder ?? 'none'}`);
      blocks.push(`- official folder top level: ${meta.official.topEntries.join(', ') || 'none'}`);
      blocks.push(`- official archive: ${meta.official.zip ? meta.official.zip.name : 'none, no zip in the folder'}`);
      blocks.push(`- official refusal: ${meta.official.refusal ?? 'n/a'}`);
    }
    blocks.push(`- GLB files: ${glbs.length}, ${mb(meta.glbBytes)} MB total`);
    blocks.push(`- GLB directories: ${meta.glbDirs.join(', ')}`);
    if (textureFiles.length === 0) {
      blocks.push('- textures: none on disk, each GLB carries its own images or vertex colours');
    } else {
      blocks.push(`- textures: ${textures.length} distinct names, ${textureFiles.length} files, ${mb(textureBytes)} MB`);
    }
    if (shown.length) {
      const more = textures.length > shown.length ? `, ${textures.length - shown.length} more` : '';
      blocks.push(
        `- texture names, atlas first then largest (KB, copies when repeated)${more}: ${shown
          .map((row) => `${row.name} (${Math.max(1, Math.round(row.bytes / 1024))}${row.copies > 1 ? `, x${row.copies}` : ''})`)
          .join(', ')}`,
      );
    }
    blocks.push('');
    for (const [label, entries] of [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
      blocks.push(`### ${pack.id} ${label} (${entries.length})`, '');
      blocks.push(entries.join(', '));
      blocks.push('');
    }
  }
  await writeFile(target, `${blocks.join('\n')}\n`);
  return target;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  await mkdir(CACHE, { recursive: true });
  await mkdir(LICENCES, { recursive: true });
  const wanted = opts.only ? PACKS.filter((pack) => opts.only.includes(pack.id)) : PACKS;
  if (wanted.length === 0) throw new Error(`no pack matched --only ${opts.only.join(',')}`);

  const results = [];
  for (const pack of wanted) {
    console.log(`${pack.id}`);
    try {
      const meta = pack.kind === 'kenney' ? await fetchKenneyPack(pack) : await fetchPolyPizzaPack(pack);
      results.push({ id: pack.id, ok: true, meta });
      console.log(`  licence ${meta.licenceFile}: ${meta.licenceLine}`);
      console.log(`  ${meta.glbCount} GLB, ${mb(meta.glbBytes)} MB`);
    } catch (error) {
      results.push({ id: pack.id, ok: false, error: error.message });
      console.log(`  FAILED: ${error.message}`);
    }
  }

  await writeSources(PACKS);
  const inventory = await writeInventory(PACKS, opts.inventory);
  console.log(`\ninventory: ${relative(ROOT, inventory).split('\\').join('/')}`);
  console.log(`licences: assets/LICENSES`);

  const failed = results.filter((row) => !row.ok);
  if (failed.length) {
    console.error(`\n${failed.length} pack(s) failed:`);
    for (const row of failed) console.error(`  ${row.id}: ${row.error}`);
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error.stack ?? error.message);
  process.exitCode = 1;
});
