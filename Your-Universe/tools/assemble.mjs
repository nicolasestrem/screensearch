// Localtime assembler: src fragments -> one self-contained index.html.
//
//   node tools/assemble.mjs [--src <dir>] [--out <file>]
//
// The shell already owns the document, the head, the import map and the body.
// It declares two comment placeholders (see src/CONTRACT.md and src/shell.html):
// LOCALTIME:CSS (replaced by one inline style element) and LOCALTIME:SCRIPT
// (replaced by the single module script holding the fragment bundle).
//
// Bundle rules, from the contract:
//   - bare "three" import lines are deleted; exactly one canonical
//     "import * as THREE from 'three';" is emitted at the top;
//   - "three/addons/<file>.js" named imports are collected from every fragment,
//     deduplicated by module path with their names merged, and emitted as single
//     line imports directly after the THREE import; any other import errors;
//   - the leading "export " keyword is stripped from export function/const/class.
// After writing, the output is re-read and verified; any failure exits nonzero.

import { existsSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';

const FRAGMENTS = ['simday', 'data', 'assets', 'world', 'agents', 'camera', 'dream', 'post', 'main'];

// The single top-level identifier each fragment must declare, in contract order.
const FACTORY_NAMES = {
  simday: 'generateSimulatedDay',
  data: 'analyzeExport',
  assets: 'createAssetLoader',
  world: 'buildWorld',
  agents: 'createAgents',
  camera: 'createCameraRig',
  dream: 'createDream',
  post: 'createPost',
  main: 'main'
};

// fragments that are also allowed one documented secondary factory
const SECOND_FACTORY_NAMES = { assets: ['registerOfflineCache'] };

const THREE_IMPORT = "import * as THREE from 'three';";
const THREE_CDN = 'https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js';
const THREE_ADDONS_CDN = 'https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/';
const THREE_TAG = 'three@0.160.0';
const IMPORT_MAP_JSON =
  '{"imports":{"three":"' + THREE_CDN + '","three/addons/":"' + THREE_ADDONS_CDN + '"}}';

const CSS_TOKENS = [
  /<!--\s*LOCALTIME:CSS\s*-->/,
  /<!--\s*__LOCALTIME_CSS__\s*-->/,
  /<!--\s*__CSS__\s*-->/,
  /<link\b[^>]*href=["'](?:\.\/)?style\.css["'][^>]*>/i,
  /<style\b[^>]*\bid=["'](?:localtime-css|style)["'][^>]*>\s*<\/style>/i
];

const SCRIPT_TOKENS = [
  /<!--\s*LOCALTIME:SCRIPT\s*-->/,
  /<!--\s*__LOCALTIME_SCRIPT__\s*-->/,
  /<!--\s*__SCRIPT__\s*-->/,
  /<!--\s*__BUNDLE__\s*-->/,
  /<script\b[^>]*\bid=["'](?:localtime-bundle|bundle)["'][^>]*>\s*<\/script>/i
];

const RE_IMPORT_LINE = /^\s*import\s+(?!\()/;
const RE_MULTILINE_IMPORT_TAIL = /^\s*\}?\s*from\s*['"]/;
const RE_EXPORT_STRIPPABLE = /^\s*export\s+(?:async\s+)?(?:function|const|let|var|class)\b/;
const RE_THREE_IMPORT = /^import\s+(?:\*\s+as\s+[A-Za-z_$][\w$]*|\{[^}]*\}|[A-Za-z_$][\w$]*)\s+from\s+['"]three['"];?$/;
const RE_ADDON_IMPORT = /^import\s+\{([^}]*)\}\s+from\s+['"]three\/addons\/([^'"]+\.js)['"];?$/;
const RE_IMPORT_NAME = /^[A-Za-z_$][\w$]*(?:\s+as\s+[A-Za-z_$][\w$]*)?$/;

function usage() {
  return [
    'usage: node tools/assemble.mjs [--src <dir>] [--out <file>]',
    '  --src <dir>   directory holding shell.html, style.css and the fragments',
    '  --out <file>  output html file (default <root>/index.html)'
  ].join('\n');
}

function parseArgs(argv) {
  const out = { src: null, out: null, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--src' || arg === '--out') {
      const value = argv[i + 1];
      if (!value || value.charAt(0) === '-') {
        throw new Error('missing value for ' + arg);
      }
      if (arg === '--src') out.src = value;
      else out.out = value;
      i += 1;
    } else if (arg === '--help' || arg === '-h') {
      out.help = true;
    } else {
      throw new Error('unknown argument: ' + arg);
    }
  }
  return out;
}

function readText(file) {
  return readFileSync(file, 'utf8').replace(/^\uFEFF/, '');
}

// Rewrite one fragment into bundle-ready source. Returns diagnostics too.
// addonImports is the shared Map<modulePath, string[]> collecting hoisted
// "three/addons/<path>.js" named imports from every fragment.
function rewriteFragment(name, source, addonImports) {
  const lines = source.split(/\r?\n/);
  const kept = [];
  let importsRemoved = 0;
  let addonsCollected = 0;
  let exportsStripped = 0;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (RE_IMPORT_LINE.test(line)) {
      importsRemoved += 1;
      const trimmed = line.trim();
      const addon = RE_ADDON_IMPORT.exec(trimmed);
      if (addon) {
        const names = addon[1]
          .split(',')
          .map(function (entry) {
            return entry.trim();
          })
          .filter(Boolean);
        if (names.length === 0) {
          throw new Error(name + '.mjs line ' + (i + 1) + ': empty three/addons import list');
        }
        for (let n = 0; n < names.length; n += 1) {
          if (!RE_IMPORT_NAME.test(names[n])) {
            throw new Error(name + '.mjs line ' + (i + 1) + ': unsupported import name "' + names[n] + '"');
          }
        }
        const path = addon[2];
        const merged = addonImports.get(path) || [];
        for (let n = 0; n < names.length; n += 1) {
          if (merged.indexOf(names[n]) < 0) merged.push(names[n]);
        }
        addonImports.set(path, merged);
        addonsCollected += 1;
        continue;
      }
      if (RE_THREE_IMPORT.test(trimmed)) continue;
      throw new Error(
        name + '.mjs line ' + (i + 1) + ': only "three" and "three/addons/<file>.js" imports are allowed: ' + trimmed
      );
    }
    if (/^\s*export\b/.test(line)) {
      if (!RE_EXPORT_STRIPPABLE.test(line)) {
        throw new Error(
          name + '.mjs line ' + (i + 1) + ': unsupported export form, use "export function|const|class": ' + line.trim()
        );
      }
      kept.push(line.replace(/^(\s*)export\s+/, '$1'));
      exportsStripped += 1;
      continue;
    }
    if (RE_MULTILINE_IMPORT_TAIL.test(line)) {
      throw new Error(name + '.mjs line ' + (i + 1) + ': multi-line import, keep imports on one line');
    }
    kept.push(line.replace(/\s+$/, ''));
  }

  const text = kept.join('\n').replace(/^\n+/, '').replace(/\n{3,}/g, '\n\n');
  return {
    text: text.replace(/\s+$/, ''),
    importsRemoved: importsRemoved,
    addonsCollected: addonsCollected,
    exportsStripped: exportsStripped
  };
}

function hoistedAddonImports(addonImports) {
  const out = [];
  addonImports.forEach(function (names, path) {
    out.push('import { ' + names.join(', ') + " } from 'three/addons/" + path + "';");
  });
  return out;
}

function buildBundle(srcDir) {
  const chunks = [THREE_IMPORT];
  const addonImports = new Map();
  const stats = [];
  let importsRemoved = 0;
  let addonsCollected = 0;
  let exportsStripped = 0;

  for (let i = 0; i < FRAGMENTS.length; i += 1) {
    const name = FRAGMENTS[i];
    const file = join(srcDir, name + '.mjs');
    const source = readText(file);
    const rewritten = rewriteFragment(name, source, addonImports);
    importsRemoved += rewritten.importsRemoved;
    addonsCollected += rewritten.addonsCollected;
    exportsStripped += rewritten.exportsStripped;
    chunks.push('// ---- src/' + name + '.mjs ----');
    chunks.push(rewritten.text);
    chunks.push('');
    stats.push({
      name: name,
      bytes: Buffer.byteLength(source, 'utf8'),
      imports: rewritten.importsRemoved,
      exports: rewritten.exportsStripped
    });
  }

  const addonLines = hoistedAddonImports(addonImports);
  if (addonLines.length > 0) {
    chunks.splice(1, 0, ...addonLines);
  }
  chunks.splice(1 + addonLines.length, 0, '');

  const bundle = chunks.join('\n').replace(/\n{3,}/g, '\n\n').replace(/\s+$/, '') + '\n';
  if (bundle.indexOf('</script') >= 0) {
    throw new Error('a fragment contains a literal </script, which would end the module script early');
  }
  return {
    bundle: bundle,
    stats: stats,
    importsRemoved: importsRemoved,
    addonsCollected: addonsCollected,
    addonModules: addonLines.length,
    exportsStripped: exportsStripped
  };
}

function substitute(text, patterns, replacement) {
  for (let i = 0; i < patterns.length; i += 1) {
    if (patterns[i].test(text)) {
      return { text: text.replace(patterns[i], replacement), hit: true };
    }
  }
  return { text: text, hit: false };
}

function wrapShell(shellText, styleBlock, moduleBlock) {
  return [
    '<!doctype html>',
    '<html lang="en">',
    '<head>',
    '<meta charset="utf-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">',
    '<title>Localtime</title>',
    '<script type="importmap">',
    IMPORT_MAP_JSON,
    '</script>',
    styleBlock,
    '</head>',
    '<body>',
    shellText.trim(),
    moduleBlock,
    '</body>',
    '</html>',
    ''
  ].join('\n');
}

function injectBefore(text, needle, block) {
  const at = text.indexOf(needle);
  if (at < 0) return null;
  return text.slice(0, at) + block + '\n' + text.slice(at);
}

function buildHtml(srcDir) {
  const shellPath = join(srcDir, 'shell.html');
  const cssPath = join(srcDir, 'style.css');
  if (!existsSync(shellPath)) throw new Error('missing ' + shellPath);
  if (!existsSync(cssPath)) throw new Error('missing ' + cssPath);

  const shellText = readText(shellPath);
  const css = readText(cssPath).trim();
  if (css.indexOf('</style') >= 0) {
    throw new Error('style.css contains a literal </style, which would end the inline style element');
  }

  const built = buildBundle(srcDir);
  const styleBlock = '<style>\n' + css + '\n</style>';
  const moduleBlock = '<script type="module">\n' + built.bundle + '</script>';

  const isDocument = /<!doctype\b|<html[\s>]/i.test(shellText);
  const notes = [];

  if (!isDocument) {
    notes.push('shell is body content, wrapped in the assembler skeleton');
    return {
      html: wrapShell(shellText, styleBlock, moduleBlock),
      built: built,
      notes: notes
    };
  }

  let html = shellText;

  const cssSlot = substitute(html, CSS_TOKENS, styleBlock);
  if (cssSlot.hit) {
    html = cssSlot.text;
    notes.push('inlined style.css into the shell CSS placeholder');
  } else {
    const injected = injectBefore(html, '</head>', styleBlock);
    if (injected === null) throw new Error('shell declares no CSS placeholder and has no </head>');
    html = injected;
    notes.push('no CSS placeholder found, injected the style element before </head>');
  }

  if (!/\btype=["']importmap["']/i.test(html)) {
    const mapBlock = '<script type="importmap">\n' + IMPORT_MAP_JSON + '\n</script>';
    const injected = injectBefore(html, '</head>', mapBlock);
    if (injected === null) throw new Error('shell has no import map and no </head>');
    html = injected;
    notes.push('no import map in the shell, injected the canonical one before </head>');
  }

  const scriptSlot = substitute(html, SCRIPT_TOKENS, moduleBlock);
  if (scriptSlot.hit) {
    html = scriptSlot.text;
    notes.push('inlined the fragment bundle into the shell script placeholder');
  } else {
    const injected = injectBefore(html, '</body>', moduleBlock);
    if (injected === null) throw new Error('shell declares no script placeholder and has no </body>');
    html = injected;
    notes.push('no script placeholder found, injected the module script before </body>');
  }

  return { html: html.replace(/\s+$/, '') + '\n', built: built, notes: notes };
}

function checkSyntax(bundle) {
  const file = join(tmpdir(), 'localtime-bundle-check-' + process.pid + '.mjs');
  writeFileSync(file, bundle, 'utf8');
  try {
    const result = spawnSync(process.execPath, ['--check', file], { encoding: 'utf8' });
    if (result.status === 0) return { ok: true, detail: 'parses as an ES module' };
    const detail = ((result.stderr || '') + (result.stdout || '')).trim().split(/\r?\n/)[0];
    return { ok: false, detail: detail || 'node --check failed' };
  } finally {
    try {
      unlinkSync(file);
    } catch (err) {
      // the temp file is outside the repository; ignore cleanup failures
    }
  }
}

function verifyHtml(html, built) {
  const results = [];
  function record(ok, label, detail) {
    results.push({ ok: ok, label: label, detail: detail || '' });
  }

  const moduleTags = html.match(/<script\b[^>]*\btype=["']module["'][^>]*>/gi) || [];
  record(moduleTags.length === 1, 'exactly one <script type="module">', 'found ' + moduleTags.length);

  const mapTags = html.match(/<script\b[^>]*\btype=["']importmap["'][^>]*>/gi) || [];
  record(mapTags.length === 1, 'exactly one <script type="importmap">', 'found ' + mapTags.length);

  const threeTags = html.match(/three@[0-9][0-9A-Za-z.+-]*/g) || [];
  const wrongTags = threeTags.filter(function (tag) {
    return tag !== THREE_TAG;
  });
  record(
    threeTags.length === 2 && wrongTags.length === 0,
    'the two pinned three.js URLs both use ' + THREE_TAG,
    threeTags.length + ' reference(s): ' + (threeTags.join(', ') || 'none')
  );

  const importLines = html.match(/^\s*import\s+(?!\()[^\n]*/gm) || [];
  const canonical = importLines.length >= 1 && importLines[0].trim() === THREE_IMPORT;
  const addonLines = importLines.slice(1);
  const badAddon = addonLines.filter(function (line) {
    return !/^import\s+\{[^}]*\}\s+from\s+['"]three\/addons\/[^'"]+\.js['"];?$/.test(line.trim());
  });
  record(
    canonical && badAddon.length === 0,
    'import lines are the canonical THREE import plus hoisted three/addons imports',
    importLines.length + ' import line(s): ' + importLines.join(' | ')
  );

  const firstFragmentAt = html.indexOf('// ---- src/');
  const hoisted = importLines.every(function (line) {
    const at = html.indexOf(line.trim());
    return at >= 0 && firstFragmentAt >= 0 && at < firstFragmentAt;
  });
  record(hoisted, 'every import line sits above the fragment bundle', 'first fragment at ' + firstFragmentAt);

  const fromCount = (html.match(/\bfrom\s+['"]/g) || []).length;
  record(
    fromCount === importLines.length,
    'every module specifier belongs to an import line',
    'found ' + fromCount + ' "from" clauses for ' + importLines.length + ' import line(s)'
  );

  record(!/^\s*export\b/m.test(html), 'no export keyword left in the bundle');

  const linkCount = (html.match(/<link\b/gi) || []).length;
  record(linkCount === 0, 'no <link> element', 'found ' + linkCount);

  const srcCount = (html.match(/<script\b[^>]*\bsrc\s*=/gi) || []).length;
  record(srcCount === 0, 'no <script src> element', 'found ' + srcCount);

  const closing = (html.match(/<\/script\s*>/gi) || []).length;
  record(closing === 2, 'exactly two closing script tags', 'found ' + closing);

  const styleTags = (html.match(/<style\b[^>]*>/gi) || []).length;
  record(styleTags === 1, 'exactly one inline <style>', 'found ' + styleTags);

  const placeholderLeft = /<!--\s*(?:LOCALTIME:(?:CSS|SCRIPT)|__LOCALTIME_(?:CSS|SCRIPT)__|__CSS__|__SCRIPT__|__BUNDLE__)\s*-->/.test(
    html
  );
  record(!placeholderLeft, 'no assembler placeholder comment left in the output');

  for (let i = 0; i < FRAGMENTS.length; i += 1) {
    const name = FRAGMENTS[i];
    const factory = FACTORY_NAMES[name];
    const present = new RegExp('\\b' + factory + '\\b').test(html);
    record(present, 'factory ' + factory + ' appears in the bundle');
    const extra = SECOND_FACTORY_NAMES[name] || [];
    for (let k = 0; k < extra.length; k += 1) {
      record(new RegExp('\\b' + extra[k] + '\\b').test(html), 'factory ' + extra[k] + ' appears in the bundle');
    }
  }

  const syntax = checkSyntax(built.bundle);
  record(syntax.ok, 'bundle parses with node --check', syntax.detail);

  return results;
}

function run(argv) {
  const args = parseArgs(argv);
  if (args.help) {
    console.log(usage());
    return 0;
  }

  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const srcDir = args.src ? resolve(process.cwd(), args.src) : join(root, 'src');
  const outFile = args.out ? resolve(process.cwd(), args.out) : join(root, 'index.html');

  const missing = [];
  for (let i = 0; i < FRAGMENTS.length; i += 1) {
    if (!existsSync(join(srcDir, FRAGMENTS[i] + '.mjs'))) missing.push(FRAGMENTS[i] + '.mjs');
  }
  if (!existsSync(join(srcDir, 'shell.html'))) missing.push('shell.html');
  if (!existsSync(join(srcDir, 'style.css'))) missing.push('style.css');
  if (missing.length) {
    console.error('assemble: ' + srcDir + ' is missing: ' + missing.join(', '));
    return 1;
  }

  console.log('assemble: src ' + srcDir);
  const page = buildHtml(srcDir);
  const html = page.html;
  const built = page.built;
  const notes = page.notes;

  for (let i = 0; i < built.stats.length; i += 1) {
    const s = built.stats[i];
    console.log(
      '  ' + s.name.padEnd(7, ' ') + ' ' + String(s.bytes).padStart(8, ' ') + ' B from source, ' +
        s.imports + ' imports removed, ' + s.exports + ' exports stripped'
    );
  }
  console.log(
    '  rewrite: ' + built.importsRemoved + ' import lines removed, ' +
      built.addonsCollected + ' addon imports collected into ' + built.addonModules + ' hoisted line(s), ' +
      built.exportsStripped + ' export keywords stripped, one canonical import emitted'
  );
  for (let i = 0; i < notes.length; i += 1) console.log('  note: ' + notes[i]);

  mkdirSync(dirname(outFile), { recursive: true });
  writeFileSync(outFile, html, 'utf8');

  // Re-read what is actually on disk, not the in-memory string.
  const written = readFileSync(outFile, 'utf8');
  const lines = written.split('\n').length;
  const bytes = Buffer.byteLength(written, 'utf8');
  console.log('assemble: wrote ' + outFile);
  console.log('assemble: ' + bytes + ' bytes, ' + lines + ' lines');

  const results = verifyHtml(written, built);
  let failed = 0;
  for (let i = 0; i < results.length; i += 1) {
    const r = results[i];
    if (!r.ok) failed += 1;
    console.log('  ' + (r.ok ? 'PASS ' : 'FAIL ') + r.label + (r.detail && !r.ok ? ' [' + r.detail + ']' : ''));
  }
  console.log('assemble: ' + (results.length - failed) + '/' + results.length + ' verifications passed');

  if (failed > 0) {
    console.error('assemble: refusing a broken bundle');
    return 1;
  }
  return 0;
}

let exitCode = 0;
try {
  exitCode = run(process.argv.slice(2));
} catch (err) {
  console.error('assemble: ' + (err && err.message ? err.message : String(err)));
  exitCode = 1;
}
process.exit(exitCode);
