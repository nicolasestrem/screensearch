// Localtime DOM and stylesheet contract checker. Owner: G.
//
// Asserts the things that break the page silently:
//   - every id and class the DOM contract requires exists in src/shell.html
//   - every id used by a CSS selector exists in the shell (or is documented below
//     as created at runtime by a fragment)
//   - the shell loads nothing from the network except the pinned import map
//   - src/style.css and src/shell.html carry no em dash, no url(), no @import,
//     no external URL
//   - src/post.mjs keeps the contract shape and stays a single-top-level-identifier
//     fragment
// Exits nonzero when anything fails.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { CACHE_VERSION_LINE, computeCacheVersion } from './cache-version.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');
const PATHS = {
  post: join(ROOT, 'src', 'post.mjs'),
  css: join(ROOT, 'src', 'style.css'),
  shell: join(ROOT, 'src', 'shell.html'),
};

const EM_DASH = String.fromCharCode(0x2014);
const THREE_URL = 'https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js';
// The owner allowed the pinned addon directory in cycle 19 so GLTFLoader can be imported
// from the same three version. Nothing else from the network is allowed, here or elsewhere.
const THREE_ADDONS_URL = 'https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/';
// The assembler substitutes these two placeholder comments. They are matched in
// their exact comment form so that the prose explaining them in the head does not
// count as a second placeholder.
const CSS_TOKEN = 'LOCALTIME:CSS';
const SCRIPT_TOKEN = 'LOCALTIME:SCRIPT';
const CSS_PLACEHOLDER = '<!-- ' + CSS_TOKEN + ' -->';
const SCRIPT_PLACEHOLDER = '<!-- ' + SCRIPT_TOKEN + ' -->';

// ids fixed by src/CONTRACT.md "DOM contract".
const REQUIRED_IDS = [
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
  'fps',
];

// classes fixed by the contract that the shell itself must carry.
const REQUIRED_SHELL_CLASSES = ['chev', 'card-title', 'card-time', 'card-phrase'];

// classes the contract fixes but that no markup fragment other than main or
// dream creates. They are checked for a stylesheet rule instead of for markup.
const RUNTIME_CLASSES = [
  ['label', 'main creates one <div class="label"> per quarter inside #labels'],
  ['dream-card', 'dream.mjs creates one <div class="dream-card"> per hypothesis card inside #cards'],
];

let failures = 0;
let checks = 0;

function check(ok, message) {
  checks += 1;
  if (ok) {
    console.log('PASS  ' + message);
  } else {
    failures += 1;
    console.log('FAIL  ' + message);
  }
}

function readOrNull(path) {
  try {
    return readFileSync(path, 'utf8');
  } catch (err) {
    return null;
  }
}

function countOf(haystack, needle) {
  if (!needle) return 0;
  let total = 0;
  let at = 0;
  for (;;) {
    const found = haystack.indexOf(needle, at);
    if (found === -1) return total;
    total += 1;
    at = found + needle.length;
  }
}

function escapeRegex(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function openingTagById(html, id) {
  const pattern = new RegExp('<([a-zA-Z][a-zA-Z0-9-]*)\\b[^>]*\\bid="' + escapeRegex(id) + '"[^>]*>');
  const match = html.match(pattern);
  return match ? match[0] : null;
}

function innerById(html, id) {
  const tag = openingTagById(html, id);
  if (!tag) return null;
  const name = tag.slice(1).match(/^[a-zA-Z][a-zA-Z0-9-]*/)[0];
  const from = html.indexOf(tag) + tag.length;
  const to = html.indexOf('</' + name + '>', from);
  if (to === -1) return null;
  return html.slice(from, to);
}

function textOf(markup) {
  return markup
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function idsInMarkup(html) {
  const ids = new Set();
  const pattern = /\bid="([^"]+)"/g;
  let match = pattern.exec(html);
  while (match) {
    ids.add(match[1]);
    match = pattern.exec(html);
  }
  return ids;
}

// ---------------------------------------------------------------------------
// Load.

const post = readOrNull(PATHS.post);
const css = readOrNull(PATHS.css);
const shell = readOrNull(PATHS.shell);

check(post !== null && post.length > 0, 'src/post.mjs exists and is not empty');
check(css !== null && css.length > 0, 'src/style.css exists and is not empty');
check(shell !== null && shell.length > 0, 'src/shell.html exists and is not empty');

if (post === null || css === null || shell === null) {
  console.log('');
  console.log('FAILED: cannot read the files above, stopping before markup checks');
  process.exitCode = 1;
} else {
  const cssCode = css.replace(/\/\*[\s\S]*?\*\//g, ' ');
  const shellIds = idsInMarkup(shell);

  // -------------------------------------------------------------------------
  // style.css hygiene.

  check(countOf(css, EM_DASH) === 0, 'style.css has no em dash character');
  check(countOf(css, 'url(') === 0, 'style.css has no url() reference');
  check(countOf(css, '@import') === 0, 'style.css has no @import');
  check(/http/i.test(css) === false, 'style.css has no http reference');

  // -------------------------------------------------------------------------
  // shell.html hygiene and the one allowed network dependency.

  check(countOf(shell, EM_DASH) === 0, 'shell.html has no em dash character');

  // HTML tag and attribute names are case-insensitive, so every tag match here carries the i
  // flag: a hand-edited <SCRIPT SRC=...> or <LINK ...> must fail the gate like the lowercase form.
  const scriptTags = shell.match(/<script\b[^>]*>/gi) || [];
  check(
    scriptTags.length === 1 && /type="importmap"/i.test(scriptTags[0]),
    'shell.html has exactly one script tag and it is the import map'
  );
  check(
    (shell.match(/<script\b[^>]*\ssrc=/gi) || []).length === 0,
    'shell.html has no external script element'
  );
  check((shell.match(/<link\b[^>]*>/gi) || []).length === 0, 'shell.html has no link element (no external stylesheet or font)');

  const urls = shell.match(/https?:\/\/[^\s"'<>]+/g) || [];
  const allowedUrls = (urls.length === 1 && urls[0] === THREE_URL) || (urls.length === 2 && urls[0] === THREE_URL && urls[1] === THREE_ADDONS_URL);
  check(
    allowedUrls,
    'shell.html mentions only the pinned three module and its pinned addon directory (' + urls.length + ' URL(s): ' + urls.join(', ') + ')'
  );
  const importMapMatch = shell.match(/<script[^>]*type="importmap"[^>]*>([\s\S]*?)<\/script>/i);
  let importMapOk = false;
  let importMapNote = 'no import map script found';
  if (importMapMatch) {
    try {
      const parsed = JSON.parse(importMapMatch[1]);
      const keys = parsed && parsed.imports ? Object.keys(parsed.imports) : [];
      importMapOk =
        keys.length === 2 &&
        keys.indexOf('three') >= 0 &&
        keys.indexOf('three/addons/') >= 0 &&
        parsed.imports.three === THREE_URL &&
        parsed.imports['three/addons/'] === THREE_ADDONS_URL;
      importMapNote = importMapOk ? 'two entries, three and three/addons/' : 'import map is ' + JSON.stringify(parsed);
    } catch (err) {
      importMapNote = 'import map is not valid JSON';
    }
  }
  check(importMapOk, 'the import map parses and binds "three" and "three/addons/" to the pinned version only (' + importMapNote + ')');

  const cssTokenAt = countOf(shell, CSS_PLACEHOLDER);
  const scriptTokenAt = countOf(shell, SCRIPT_PLACEHOLDER);
  check(cssTokenAt === 1, 'shell.html carries exactly one ' + CSS_TOKEN + ' placeholder (found ' + cssTokenAt + ')');
  check(scriptTokenAt === 1, 'shell.html carries exactly one ' + SCRIPT_TOKEN + ' placeholder (found ' + scriptTokenAt + ')');
  check(
    countOf(shell, CSS_TOKEN) >= 1 && countOf(shell, SCRIPT_TOKEN) >= 1,
    'both placeholder tokens are spelled out in the head notes'
  );

  const importMapAt = shell.indexOf('type="importmap"');
  const cssIndex = shell.indexOf(CSS_PLACEHOLDER);
  const scriptIndex = shell.indexOf(SCRIPT_PLACEHOLDER);
  check(
    importMapAt !== -1 && scriptIndex !== -1 && importMapAt < scriptIndex,
    'the import map appears before the script placeholder'
  );
  check(cssIndex !== -1 && cssIndex < scriptIndex, 'the stylesheet placeholder appears before the script placeholder');
  check(shell.indexOf('</head>') > cssIndex && shell.indexOf('</head>') < scriptIndex, 'the stylesheet placeholder sits inside the head');

  // -------------------------------------------------------------------------
  // Required ids, exactly once each.

  for (const id of REQUIRED_IDS) {
    const found = countOf(shell, 'id="' + id + '"');
    check(found === 1, 'shell.html declares id="' + id + '" exactly once (found ' + found + ')');
  }

  // -------------------------------------------------------------------------
  // Required classes.

  check(countOf(shell, 'class="chev"') === 2, 'shell.html marks both rotation buttons with class="chev"');
  for (const name of REQUIRED_SHELL_CLASSES) {
    check(shell.includes('class="' + name + '"'), 'shell.html declares class="' + name + '"');
  }
  for (const [name, reason] of RUNTIME_CLASSES) {
    check(
      cssCode.includes('.' + name),
      'style.css styles runtime class .' + name + ' (' + reason + ')'
    );
  }

  // -------------------------------------------------------------------------
  // Element roles and attributes fixed by the contract.

  const viewTag = openingTagById(shell, 'view');
  check(viewTag !== null && viewTag.startsWith('<canvas'), '#view is a canvas element');

  const labelsInner = innerById(shell, 'labels');
  const cardsInner = innerById(shell, 'cards');
  check(
    labelsInner !== null && textOf(labelsInner) === '',
    '#labels starts as an empty overlay container'
  );
  check(
    cardsInner !== null && textOf(cardsInner) === '',
    '#cards starts empty so #cards:empty hides it'
  );

  const fileTag = openingTagById(shell, 'file');
  check(fileTag !== null && /\stype="file"/.test(fileTag), '#file is a file input');
  check(
    fileTag !== null && /accept="application\/json,\.json"/.test(fileTag),
    '#file accepts application/json and .json'
  );
  check(fileTag !== null && /\shidden(\s|>)/.test(fileTag), '#file starts hidden');

  for (const id of ['card', 'drop', 'fps']) {
    const tag = openingTagById(shell, id);
    check(tag !== null && /\shidden(\s|>)/.test(tag), '#' + id + ' carries the hidden attribute');
  }

  const sleepTag = openingTagById(shell, 'sleep');
  const openFileTag = openingTagById(shell, 'open-file');
  check(sleepTag !== null && /aria-label="Sleep"/.test(sleepTag), '#sleep is labelled "Sleep"');
  check(openFileTag !== null && /aria-label="Load export"/.test(openFileTag), '#open-file is labelled "Load export"');

  const iconButtons = { 'rot-left': 'left chevron', 'rot-right': 'right chevron', sleep: 'moon', 'open-file': 'folder' };
  for (const id of Object.keys(iconButtons)) {
    const inner = innerById(shell, id);
    check(inner !== null && inner.includes('<svg'), '#' + id + ' carries an inline svg ' + iconButtons[id] + ' icon');
    check(inner !== null && textOf(inner) === '', '#' + id + ' has no text content');
    check(inner !== null && !/xlink:href|href=|<image/.test(inner), '#' + id + ' svg uses no external image reference');
  }

  check(
    textOf(innerById(shell, 'drop') || '') === 'Drop a ScreenSearch export',
    '#drop reads "Drop a ScreenSearch export"'
  );
  check(
    shell.includes('<h3 class="card-title">') &&
      shell.includes('<p class="card-time">') &&
      shell.includes('<p class="card-phrase">'),
    '#card holds card-title, card-time and card-phrase children'
  );

  // -------------------------------------------------------------------------
  // Every id used by a stylesheet selector must exist in the shell.

  const cssIds = new Set();
  const idPattern = /#([A-Za-z][A-Za-z0-9_-]*)/g;
  let idMatch = idPattern.exec(cssCode);
  while (idMatch) {
    const name = idMatch[1];
    // a selector id is never a bare hex colour
    if (!/^[0-9a-fA-F]{3,8}$/.test(name)) cssIds.add(name);
    idMatch = idPattern.exec(cssCode);
  }

  check(cssIds.size > 0, 'style.css uses at least one id selector (found ' + cssIds.size + ')');
  for (const id of Array.from(cssIds).sort()) {
    check(
      shellIds.has(id),
      'style.css selector #' + id + ' resolves to an element in shell.html'
    );
  }

  // -------------------------------------------------------------------------
  // Style requirements that are mechanically checkable.

  check(cssCode.includes('system-ui'), 'style.css uses the system-ui font stack and loads no font file');
  check(cssCode.includes('all-small-caps'), 'labels use small caps');
  check(cssCode.includes('var(--q'), 'labels read their swatch colour from the --q custom property');
  check(cssCode.includes('--q, var(--accent)'), 'labels fall back to the accent colour when --q is missing');
  check(/#view\s*\{[\s\S]*?touch-action:\s*none/.test(cssCode), '#view sets touch-action none for the camera gestures');
  check(/\[hidden\]\s*\{[^}]*display:\s*none/.test(cssCode), '[hidden] is forced to display none');
  check(cssCode.includes('min-height: 44px') && cssCode.includes('min-width: 44px'), 'buttons carry 44px minimum touch targets');
  check(cssCode.includes('prefers-reduced-motion'), 'style.css handles prefers-reduced-motion');
  check(/orientation:\s*portrait/.test(cssCode), 'style.css has a portrait phone layout');
  check(cssCode.includes('max-width: 520px'), 'the portrait layout is scoped to phone widths');
  check(/ui-monospace/.test(cssCode), '#fps uses a monospace stack');
  check(/#fps\s*\{[\s\S]*?white-space:\s*pre-line/.test(cssCode), '#fps renders multi-line stats');
  check(!/linear-gradient|radial-gradient/.test(cssCode), 'style.css uses no gradient chrome that reads as a HUD bar');

  // -------------------------------------------------------------------------
  // post.mjs contract shape.

  check(countOf(post, EM_DASH) === 0, 'src/post.mjs has no em dash character');
  check(
    /^export function createPost\s*\(THREE, renderer, scene, camera, opts\)/m.test(post),
    'post.mjs exports createPost(THREE, renderer, scene, camera, opts)'
  );
  const exportsInPost = (post.match(/^export\s/gm) || []).length;
  check(exportsInPost === 1, 'post.mjs declares exactly one top level identifier (found ' + exportsInPost + ' exports)');
  const strayTopLevel = post.match(/^(const|let|var|class|function|async)\b.*/gm) || [];
  check(strayTopLevel.length === 0, 'post.mjs keeps every helper inside the factory closure');
  check(!/^\s*await\s/m.test(post), 'post.mjs has no top level await');
  check(post.includes('WebGLRenderTarget'), 'post.mjs renders the scene into a render target');
  check(post.includes('ShaderMaterial'), 'post.mjs composites with ShaderMaterial');
  check(post.includes('new THREE.BufferGeometry()'), 'post.mjs uses a full screen triangle geometry');
  check(/function setSize\s*\(w, h\)/.test(post), 'post.mjs exposes setSize(w, h)');
  check(/function dispose\s*\(/.test(post), 'post.mjs exposes dispose()');
  check(post.includes('setRenderTarget(null)'), 'post.mjs can render straight to the canvas when disabled');
  const disposes = (post.match(/\.dispose\(\)/g) || []).length;
  check(disposes >= 7, 'post.mjs disposes three render targets and every material (found ' + disposes + ' dispose calls)');
  check(!/OrbitControls|EffectComposer|RGBELoader|loaders?\//.test(post), 'post.mjs imports no three addon');

  // -------------------------------------------------------------------------
  // The shipped payload and the service worker must agree, or a returning visitor keeps a stale
  // pack from the cache forever. tools/stamp-sw.mjs writes the version and both sides compute it
  // through tools/cache-version.mjs, so this fails if either the manifest or any pack byte moves
  // without restamping.

  const wantVersion = await computeCacheVersion(ROOT);
  const swText = readFileSync(join(ROOT, 'assets', 'sw.js'), 'utf8');
  const swVersion = (swText.match(CACHE_VERSION_LINE) || [])[1];
  check(swVersion === wantVersion,
    'assets/sw.js cache version matches the payload fingerprint (' + swVersion + ' vs ' + wantVersion +
    '; run node tools/stamp-sw.mjs)');

  // -------------------------------------------------------------------------
  // Result.

  console.log('');
  if (failures === 0) {
    console.log('total: ' + checks + ' checks, all PASS');
  } else {
    console.log('total: ' + checks + ' checks, ' + failures + ' FAIL');
    process.exitCode = 1;
  }
}
