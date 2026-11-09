// Localtime launch lint.
//
//   node tools/check.mjs
//
// Asserts, over index.html, sample-export.json, README.md (when present) and
// every file under src/ and tools/:
//   - no U+2014 em dash anywhere;
//   - no plain http scheme, except a loopback base literal inside tools/ (a dev server
//     needs a base URL to parse request paths; it can never reach a remote host);
//   - no external URL other than the two pinned three.js entries: the core module URL and
//     any path under the three/addons/ prefix on the same pinned jsDelivr version. Two
//     narrow allowances, both provably unable to reach the network:
//       * RFC 2606 reserved example hosts (example.com/.org/.net and *.example) inside
//         sample-export.json, because the export format carries browser_url values and
//         OCR text, and those reserved names never resolve;
//       * loopback hosts inside tools/, for the same reason as the plain-http allowance.
//     One further deliberate, single-file allowance: tools/fetch-assets.mjs (the build-time
//     downloader for the owner-approved CC0 packs, see OWNER-NOTES.md 2026-11-09) may name
//     the approved source hosts. That file is a development tool and is never shipped; no
//     other file may reference those hosts, and index.html is checked to contain none.
//     Every other absolute URL anywhere fails.
//   - index.html declares no <link> and no <script src>, one import map, one module script;
//   - no TODO, FIXME or stub markers in src/;
//   - every fragment named in the contract exists;
//   - node --check passes for every .mjs.
// One PASS/FAIL line per assertion, a summary, and a nonzero exit on any failure.
//
// The needles below are assembled from fragments on purpose: this file is itself
// part of the scanned tree, so it must not contain the patterns it forbids.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const EM_DASH = String.fromCharCode(0x2014);
const SCHEME_PLAIN = 'http' + '://';
const SCHEME_SECURE = 'https' + '://';
const ALLOWED_URL = SCHEME_SECURE + 'cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js';
const ALLOWED_URL_PREFIX = SCHEME_SECURE + 'cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/';
const ASSET_FETCHER = 'tools/fetch-assets.mjs';
// Owner-approved CC0 sources, from OWNER-NOTES.md 2026-11-09. The single-file allowance
// below is scoped to the build-time downloader; nothing shipped may reference these hosts.
const CC0_SOURCE_HOSTS = [
  'kenney.nl',
  'www.kenney.nl',
  'quaternius.com',
  'www.quaternius.com',
  'poly.pizza',
  'api.poly.pizza',
  'static.poly.pizza',
  'drive.google.com',
  'drive.usercontent.google.com'
];
const THREE_TAG = 'three@' + '0.160.0';
const URL_RE = /https?:\/\/[^\s"'<>)\]}\\]+/g;

const STUB_MARKERS = [
  new RegExp('\\bTO' + 'DO\\b'),
  new RegExp('\\bFIX' + 'ME\\b'),
  new RegExp('\\bX' + 'XX\\b'),
  /not\s+implemented/i,
  /coming\s+soon/i,
  /lorem\s+ipsum/i
];

const TEXT_EXTENSIONS = ['.mjs', '.js', '.html', '.css', '.json', '.md', '.txt', '.svg'];
const CODE_EXTENSIONS = ['.mjs', '.js', '.html', '.css'];

const REQUIRED_SRC = [
  'simday.mjs',
  'data.mjs',
  'assets.mjs',
  'world.mjs',
  'agents.mjs',
  'camera.mjs',
  'dream.mjs',
  'post.mjs',
  'main.mjs',
  'shell.html',
  'style.css'
];

const DOM_IDS = [
  'view',
  'labels',
  'rot-left',
  'rot-right',
  'sleep',
  'card',
  'cards',
  'drop',
  'file',
  'open-file',
  'fps'
];

const results = [];

function record(ok, label, detail) {
  results.push({ ok: ok, label: label, detail: detail || '' });
}

function hostOf(url) {
  // Deliberately not new URL(): dev tooling prints a loopback base with an interpolated
  // port, which the URL parser rejects, and a rejected base must not become a false positive.
  const match = /^https?:\/\/(\[[^\]]+\]|[^/:?#]+)/.exec(url);
  return match ? match[1].toLowerCase() : '';
}

function isReservedSampleHost(host) {
  if (!host) return false;
  if (host === 'example.com' || host === 'example.org' || host === 'example.net') return true;
  return /(^|\.)example$/.test(host);
}

function isLoopbackHost(host) {
  return host === 'localhost' || host === '127.0.0.1' || host === '::1' || host === '[::1]';
}

// Template literals that append a path expression to an origin defeat full URL parsing, so the
// allowance matches the literal text: the host must be followed by a path slash, the start of a
// template expression, or the end of the match. Plain prefixes rather than a regular expression
// built out of the host strings, so a hostname is never re-parsed as a pattern.
const CC0_SOURCE_PREFIXES = CC0_SOURCE_HOSTS.map(function (host) {
  return SCHEME_SECURE + host;
});

function isCc0SourceUrl(url) {
  for (let i = 0; i < CC0_SOURCE_PREFIXES.length; i += 1) {
    const prefix = CC0_SOURCE_PREFIXES[i];
    if (url.indexOf(prefix) !== 0) continue;
    const next = url.charAt(prefix.length);
    if (next === '' || next === '/' || next === '$') return true;
  }
  return false;
}

function extensionOf(file) {
  const at = file.lastIndexOf('.');
  return at < 0 ? '' : file.slice(at).toLowerCase();
}

function walk(dir, out) {
  if (!existsSync(dir)) return out;
  const entries = readdirSync(dir, { withFileTypes: true });
  for (let i = 0; i < entries.length; i += 1) {
    const entry = entries[i];
    const full = join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (entry.isFile() && TEXT_EXTENSIONS.indexOf(extensionOf(full)) >= 0) out.push(full);
  }
  return out;
}

function lineAt(text, index) {
  let line = 1;
  for (let i = 0; i < index && i < text.length; i += 1) {
    if (text.charCodeAt(i) === 10) line += 1;
  }
  return line;
}

function readText(file) {
  return readFileSync(file, 'utf8');
}

function shortList(items, max) {
  if (items.length <= max) return items.join(', ');
  return items.slice(0, max).join(', ') + ' (+' + (items.length - max) + ' more)';
}

function run() {
  const indexFile = join(ROOT, 'index.html');
  const sampleFile = join(ROOT, 'sample-export.json');
  const readmeFile = join(ROOT, 'README.md');

  const scanned = [];
  if (existsSync(indexFile)) scanned.push(indexFile);
  if (existsSync(sampleFile)) scanned.push(sampleFile);
  if (existsSync(readmeFile)) scanned.push(readmeFile);
  walk(join(ROOT, 'src'), scanned);
  walk(join(ROOT, 'tools'), scanned);

  console.log('check: ' + ROOT);
  console.log('check: scanning ' + scanned.length + ' files');

  // ---- character and URL policy, every scanned file -----------------------
  const emDashHits = [];
  const plainSchemeHits = [];
  const foreignUrlHits = [];
  let reservedDataUrls = 0;
  let loopbackToolUrls = 0;
  let cc0SourceUrls = 0;

  for (let i = 0; i < scanned.length; i += 1) {
    const file = scanned[i];
    const label = relative(ROOT, file).replace(/\\/g, '/');
    const inTools = label.indexOf('tools/') === 0;
    const inAssetFetcher = label === ASSET_FETCHER;
    const text = readText(file);

    let at = text.indexOf(EM_DASH);
    while (at >= 0) {
      emDashHits.push(label + ':' + lineAt(text, at));
      at = text.indexOf(EM_DASH, at + 1);
    }

    let plainAt = text.indexOf(SCHEME_PLAIN);
    while (plainAt >= 0) {
      const after = text.slice(plainAt + SCHEME_PLAIN.length, plainAt + SCHEME_PLAIN.length + 48);
      const tolerated = inTools && /^(?:localhost|127\.0\.0\.1|\[::1\])/.test(after);
      if (!tolerated) plainSchemeHits.push(label + ':' + lineAt(text, plainAt) + ' ' + after.split(/[\s"'<>)\]}]+/)[0]);
      plainAt = text.indexOf(SCHEME_PLAIN, plainAt + 1);
    }

    URL_RE.lastIndex = 0;
    let match = URL_RE.exec(text);
    while (match) {
      const url = match[0].replace(/[.,;:]+$/, '');
      if (url !== ALLOWED_URL && url.indexOf(ALLOWED_URL_PREFIX) !== 0) {
        const host = hostOf(url);
        if (inAssetFetcher && isCc0SourceUrl(url)) cc0SourceUrls += 1;
        else if (isLoopbackHost(host) && inTools) loopbackToolUrls += 1;
        else if (isReservedSampleHost(host)) reservedDataUrls += 1;
        else foreignUrlHits.push(label + ':' + lineAt(text, match.index) + ' ' + url);
      }
      match = URL_RE.exec(text);
    }
  }

  record(emDashHits.length === 0, 'no U+2014 em dash in any scanned file', shortList(emDashHits, 6));
  record(
    plainSchemeHits.length === 0,
    'no plain ' + SCHEME_PLAIN + ' anywhere outside a loopback base in tools/',
    shortList(plainSchemeHits, 6)
  );
  record(
    foreignUrlHits.length === 0,
    'only the two pinned three.js CDN URLs are referenced',
    shortList(foreignUrlHits, 6)
  );
  if (reservedDataUrls > 0 || loopbackToolUrls > 0 || cc0SourceUrls > 0) {
    console.log(
      'check: tolerated ' + reservedDataUrls + ' reserved-example URLs in data, ' +
        loopbackToolUrls + ' loopback URLs in tools/, ' +
        cc0SourceUrls + ' approved CC0 source URLs in ' + ASSET_FETCHER
    );
  }

  // ---- index.html ---------------------------------------------------------
  if (!existsSync(indexFile)) {
    record(false, 'index.html exists', 'run node tools/assemble.mjs');
  } else {
    const html = readText(indexFile);

    const moduleTags = html.match(/<script\b[^>]*\btype=["']module["'][^>]*>/gi) || [];
    const mapTags = html.match(/<script\b[^>]*\btype=["']importmap["'][^>]*>/gi) || [];
    const linkTags = html.match(/<link\b/gi) || [];
    const srcTags = html.match(/<script\b[^>]*\bsrc\s*=/gi) || [];
    record(
      moduleTags.length === 1 && mapTags.length === 1 && linkTags.length === 0 && srcTags.length === 0,
      'index.html: one module script, one import map, no <link>, no <script src>',
      moduleTags.length + ' module, ' + mapTags.length + ' importmap, ' + linkTags.length + ' link, ' + srcTags.length + ' src'
    );

    const threeTags = html.match(/three@[0-9][0-9A-Za-z.+-]*/g) || [];
    const wrongTags = threeTags.filter(function (tag) {
      return tag !== THREE_TAG;
    });
    record(
      threeTags.length === 2 && wrongTags.length === 0,
      'index.html: both three.js URLs are pinned to ' + THREE_TAG,
      threeTags.length + ' reference(s): ' + (threeTags.join(', ') || 'none')
    );

    record(!/^\s*export\b/m.test(html), 'index.html: no export keyword in the bundle');

    const importLines = html.match(/^\s*import\s+(?!\()[^\n]*/gm) || [];
    const canonicalImport = importLines.length >= 1 && importLines[0].trim() === "import * as THREE from 'three';";
    const addonImportRe = /^import\s+\{[^}]*\}\s+from\s+['"]three\/addons\/[^'"]+\.js['"];?$/;
    const strayImports = importLines.slice(1).filter(function (line) {
      return !addonImportRe.test(line.trim());
    });
    record(
      canonicalImport && strayImports.length === 0,
      'index.html: imports are the canonical THREE import plus hoisted three/addons imports',
      importLines.length + ' found: ' + importLines.join(' | ')
    );

    const placeholderLeft = /<!--\s*(?:LOCALTIME:(?:CSS|SCRIPT)|__LOCALTIME_(?:CSS|SCRIPT)__|__CSS__|__SCRIPT__|__BUNDLE__)\s*-->/.test(
      html
    );
    record(!placeholderLeft, 'index.html: no assembler placeholder comment left');

    const styleTags = html.match(/<style\b[^>]*>/gi) || [];
    record(styleTags.length === 1, 'index.html: CSS is inlined once', 'found ' + styleTags.length);

    const missingIds = DOM_IDS.filter(function (id) {
      return html.indexOf('id="' + id + '"') < 0 && html.indexOf("id='" + id + "'") < 0;
    });
    record(missingIds.length === 0, 'index.html: DOM contract ids present', 'missing ' + missingIds.join(', '));

    URL_RE.lastIndex = 0;
    const shippedSourceUrls = [];
    let shippedMatch = URL_RE.exec(html);
    while (shippedMatch) {
      const url = shippedMatch[0].replace(/[.,;:]+$/, '');
      if (isCc0SourceUrl(url)) shippedSourceUrls.push(url);
      shippedMatch = URL_RE.exec(html);
    }
    record(
      shippedSourceUrls.length === 0,
      'index.html: no approved CC0 source URL is shipped',
      shortList(shippedSourceUrls, 4)
    );
  }

  // ---- src/ contract files ------------------------------------------------
  const missingSrc = REQUIRED_SRC.filter(function (name) {
    return !existsSync(join(ROOT, 'src', name));
  });
  record(missingSrc.length === 0, 'src/: every fragment named in the contract exists', 'missing ' + missingSrc.join(', '));

  // ---- stub markers in src/ ----------------------------------------------
  const srcFiles = walk(join(ROOT, 'src'), []).filter(function (file) {
    return CODE_EXTENSIONS.indexOf(extensionOf(file)) >= 0;
  });
  const markerHits = [];
  for (let i = 0; i < srcFiles.length; i += 1) {
    const file = srcFiles[i];
    const label = relative(ROOT, file).replace(/\\/g, '/');
    const text = readText(file);
    for (let k = 0; k < STUB_MARKERS.length; k += 1) {
      const match = STUB_MARKERS[k].exec(text);
      if (match) markerHits.push(label + ':' + lineAt(text, match.index) + ' ' + match[0]);
    }
  }
  record(markerHits.length === 0, 'src/: no TODO, FIXME or stub markers', shortList(markerHits, 6));

  // ---- node --check -------------------------------------------------------
  const checkFiles = walk(join(ROOT, 'src'), []).concat(walk(join(ROOT, 'tools'), []));
  const mjsFiles = checkFiles.filter(function (file) {
    return extensionOf(file) === '.mjs';
  });
  const unique = [];
  for (let i = 0; i < mjsFiles.length; i += 1) {
    if (unique.indexOf(mjsFiles[i]) < 0) unique.push(mjsFiles[i]);
  }
  for (let i = 0; i < unique.length; i += 1) {
    const file = unique[i];
    const label = relative(ROOT, file).replace(/\\/g, '/');
    const result = spawnSync(process.execPath, ['--check', file], { encoding: 'utf8' });
    const detail = ((result.stderr || '') + (result.stdout || '')).trim().split(/\r?\n/)[0];
    record(result.status === 0, 'node --check ' + label, detail || 'exit ' + result.status);
  }

  // ---- sample-export.json -------------------------------------------------
  if (!existsSync(sampleFile)) {
    record(false, 'sample-export.json: valid v1 export', 'missing');
  } else {
    let ok = false;
    let detail = 'unreadable';
    try {
      const parsed = JSON.parse(readText(sampleFile));
      if (!parsed || typeof parsed !== 'object') {
        detail = 'not an object';
      } else if (parsed.schema !== 'screensearch.export.v1') {
        detail = 'schema is ' + String(parsed.schema);
      } else if (!Array.isArray(parsed.frames)) {
        detail = 'frames is not an array';
      } else if (parsed.frames.length === 0) {
        detail = 'frames is empty';
      } else {
        ok = true;
        detail = parsed.frames.length + ' frames';
      }
    } catch (err) {
      detail = 'invalid JSON: ' + (err && err.message ? err.message : String(err));
    }
    record(ok, 'sample-export.json: valid v1 export', detail);
  }

  // ---- report ------------------------------------------------------------
  let passed = 0;
  let failed = 0;
  const failures = [];
  for (let i = 0; i < results.length; i += 1) {
    const r = results[i];
    if (r.ok) passed += 1;
    else {
      failed += 1;
      failures.push(r.label + (r.detail ? ' [' + r.detail + ']' : ''));
    }
    console.log((r.ok ? 'PASS ' : 'FAIL ') + r.label + (r.ok || !r.detail ? '' : ' [' + r.detail + ']'));
  }

  console.log('check: ' + results.length + ' checks, ' + passed + ' passed, ' + failed + ' failed');
  if (failed > 0) {
    for (let i = 0; i < failures.length; i += 1) console.log('check: failed - ' + failures[i]);
    return 1;
  }
  return 0;
}

let exitCode = 0;
try {
  exitCode = run();
} catch (err) {
  console.error('check: ' + (err && err.message ? err.message : String(err)));
  exitCode = 1;
}
process.exit(exitCode);
