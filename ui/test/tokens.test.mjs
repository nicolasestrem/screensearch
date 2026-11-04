// Token fence (UI_REFERENCE 1-2). A primitive may name only a semantic token
// through a Tailwind utility: no colour literal, no rgb()/hsl(), no px font
// size, no arbitrary px value, no layer-1 --p- reference, no inline numeric
// style. The primitives are read as plain text on purpose, so the fence sees
// every class string exactly as the compiler does, including conditional ones.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

/* global console */

const DIR = join(import.meta.dirname, "..", "src", "components", "primitives");

/** One rule per way a component could escape the token layer. */
const RULES = [
  {
    id: "colour-literal",
    why: "a raw hex colour belongs in the --p- primitive layer, not in a component",
    pattern: /#[0-9a-fA-F]{3,8}\b/g,
  },
  {
    id: "colour-function",
    why: "rgb()/rgba()/hsl()/hsla() bypass the semantic layer",
    pattern: /\b(?:rgba?|hsla?)\(/g,
  },
  {
    id: "font-size-literal",
    why: "font sizes come from the type scale (text-t-*), never from a declaration",
    pattern: /font-size:\s*[0-9]/g,
  },
  {
    id: "arbitrary-font-size",
    why: "arbitrary font sizes skip the 12px floor and the scale",
    pattern: /text-\[[0-9.]+(?:px|rem)\]/g,
  },
  {
    id: "arbitrary-px",
    why: "arbitrary px values skip the density-aware spacing scale",
    // `text-[12px]` is reported by the font-size rule; this one is everything else.
    pattern: /(?<!text-)\[[0-9.]+px\]/g,
  },
  {
    id: "primitive-token",
    why: "components speak semantic tokens; --p-* is layer 1 only",
    pattern: /var\(--p-/g,
  },
];

const STYLE_RULE = {
  id: "inline-style-number",
  why: "an inline style with a numeric px value escapes the density scale",
};

const pad = (text, width) => String(text).padEnd(width);

function lineStarts(source) {
  const starts = [0];
  for (let index = 0; index < source.length; index += 1) {
    if (source[index] === "\n") starts.push(index + 1);
  }
  return starts;
}

/** Materialise one violation with the file, line and offending source text. */
function violation(file, source, index, rule, snippet) {
  const starts = lineStarts(source);
  let line = 1;
  for (let i = 0; i < starts.length && starts[i] <= index; i += 1) line = i + 1;
  const end = snippet === undefined ? source.indexOf("\n", index) : index + snippet.length;
  const text = source.slice(index, end === -1 ? source.length : end).replace(/\s+/g, " ").trim();
  return { file, line, rule: rule.id, why: rule.why, text };
}

/**
 * Find `style={{ ... }}` blocks whose value is a px measurement. A bare number
 * (`height: 12`) and a px string (`height: "12px"`) both count; a token
 * reference (`animationDuration: SPIN_DURATION`) does not.
 */
function styleViolations(file, source) {
  const hits = [];
  for (const match of source.matchAll(/style=\{\{/g)) {
    const start = match.index ?? 0;
    const close = source.indexOf("}}", start + 8);
    const body = source.slice(start, close === -1 ? source.length : close + 2);
    const numeric = /:\s*[0-9]+(?:\.[0-9]+)?\s*(?:px)?\s*[,}]/.test(body);
    const pxString = /[0-9]+(?:\.[0-9]+)?px/.test(body);
    if (numeric || pxString) hits.push(violation(file, source, start, STYLE_RULE, body));
  }
  return hits;
}

/** Files the fence owns: the primitive components, not the barrel or icons. */
function fenceFiles() {
  return readdirSync(DIR, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".tsx"))
    .map((entry) => entry.name)
    .sort();
}

function scan() {
  const files = fenceFiles();
  const hits = [];
  const perRule = new Map([...RULES.map((rule) => rule.id), STYLE_RULE.id].map((id) => [id, 0]));

  for (const name of files) {
    const source = readFileSync(join(DIR, name), "utf8");
    for (const rule of RULES) {
      for (const match of source.matchAll(rule.pattern)) {
        hits.push(violation(name, source, match.index ?? 0, rule));
        perRule.set(rule.id, perRule.get(rule.id) + 1);
      }
    }
    for (const entry of styleViolations(name, source)) {
      hits.push(entry);
      perRule.set(STYLE_RULE.id, perRule.get(STYLE_RULE.id) + 1);
    }
  }

  hits.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line);
  return { files, hits, perRule };
}

function report({ files, hits, perRule }) {
  console.log(`\ntoken fence: ${files.length} primitives x ${perRule.size} rules`);
  for (const [id, count] of perRule) console.log(`  ${pad(id, 20)} ${count} hit${count === 1 ? "" : "s"}`);
  if (hits.length === 0) {
    console.log(`  scanned: ${files.join(", ")}`);
    return;
  }
  console.log("");
  for (const entry of hits) {
    console.log(`  ${entry.file}:${entry.line}  ${entry.rule}: ${entry.text}`);
    console.log(`    ${entry.why}`);
  }
}

test("the fence covers the primitive components and nothing else", () => {
  const { files } = scan();
  assert.ok(files.length >= 25, `expected every primitive, found ${files.length}`);
  assert.ok(files.includes("Button.tsx"), "Button.tsx missing from the fence");
  assert.ok(files.includes("Table.tsx"), "Table.tsx missing from the fence");
  // icons/index.tsx holds SVG paths, not UI styling; the barrel has no JSX.
  assert.ok(!files.includes("index.tsx"), "index.tsx must not be scanned");
  assert.ok(!files.includes("index.ts"), "index.ts must not be scanned");
  assert.ok(!files.some((name) => name.startsWith("Kitchen")), "the kitchen must not be scanned");
});

test("every fence rule and the inline-style scan catch a real violation", () => {
  const samples = {
    "colour-literal": 'className="bg-[#fff]"',
    "colour-function": 'className="bg-[rgba(0,0,0,0.5)]"',
    "font-size-literal": "style={{ font-size: 13 }}",
    "arbitrary-font-size": 'className="text-[13px]"',
    "arbitrary-px": 'className="w-[240px]"',
    "primitive-token": 'className="bg-[var(--p-mist-000)]"',
  };
  for (const rule of RULES) {
    const sample = samples[rule.id];
    assert.ok(sample, `missing sample for ${rule.id}`);
    assert.ok([...sample.matchAll(rule.pattern)].length > 0, `${rule.id} did not match: ${sample}`);
  }
  // The inline-style rule is structural: it catches both spellings and leaves a
  // token-valued style alone (Spinner animates with a constant).
  assert.ok(styleViolations("Sample.tsx", "style={{ height: 12 }}").length > 0);
  assert.ok(styleViolations("Sample.tsx", 'style={{ height: "12px" }}').length > 0);
  assert.equal(styleViolations("Sample.tsx", "style={{ animationDuration: SPIN_DURATION }}").length, 0);
});

test("no primitive escapes the token layer", () => {
  const result = scan();
  report(result);
  const detail = result.hits
    .map((entry) => `${entry.file}:${entry.line} [${entry.rule}] ${entry.text} (${entry.why})`)
    .join("\n");
  assert.equal(result.hits.length, 0, `token-fence violations:\n${detail}`);
});
