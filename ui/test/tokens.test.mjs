// Token fence (UI_REFERENCE 1-2). Every file under ui/src may name only a
// semantic token through a Tailwind utility: no colour literal, no rgb()/hsl(),
// no px font size, no arbitrary px value, no layer-1 --p- reference, no numeric
// px in an inline style. Sources are read as plain text on purpose, so the fence
// sees every class string exactly as the compiler does, including conditional
// ones; comments are blanked first, so prose and issue numbers (#106) are never
// violations.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

/* global console */

const SRC = join(import.meta.dirname, "..", "src");

/**
 * Scope: every hand-written `.ts`/`.tsx` under `ui/src`, recursively. Three
 * things are exempt and each for a reason: the generated `bindings` tree is not
 * ours to fix; `styles/tokens.css` is where the literals are supposed to live;
 * and an `index.ts` barrel holds re-exports only (no class string, no style),
 * so scanning it would only add noise. `index.tsx` is NOT a barrel — the icon
 * set and the settings route live in one — so it stays in scope.
 */
const EXEMPT_DIRS = new Set(["bindings"]);
/** The token file itself. A `.css` file, so the extension filter already skips
 * it; named here so the exemption is explicit in the report, not incidental. */
const TOKEN_FILE = "styles/tokens.css";
const BARREL_FILES = new Set(["index.ts"]);

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

/**
 * CSS properties whose bare number is a count, a ratio or a grid line, not a px
 * length (React's unitless set). `gridColumn: 1` and `opacity: 0.5` are honest
 * numbers; `marginTop: 7` is 7px and must come from the spacing scale.
 */
const UNITLESS_PROPERTIES = new Set([
  "animationIterationCount",
  "aspectRatio",
  "borderImageOutset",
  "borderImageSlice",
  "borderImageWidth",
  "boxFlex",
  "boxFlexGroup",
  "boxOrdinalGroup",
  "columnCount",
  "columns",
  "fillOpacity",
  "flex",
  "flexGrow",
  "flexNegative",
  "flexOrder",
  "flexPositive",
  "flexShrink",
  "floodOpacity",
  "fontWeight",
  "gridArea",
  "gridColumn",
  "gridColumnEnd",
  "gridColumnSpan",
  "gridColumnStart",
  "gridRow",
  "gridRowEnd",
  "gridRowSpan",
  "gridRowStart",
  "lineClamp",
  "lineHeight",
  "opacity",
  "order",
  "orphans",
  "stopOpacity",
  "strokeDasharray",
  "strokeDashoffset",
  "strokeMiterlimit",
  "strokeOpacity",
  "strokeWidth",
  "tabSize",
  "widows",
  "zIndex",
  "zoom",
]);

const pad = (text, width) => String(text).padEnd(width);

/**
 * Blank every comment, keeping the byte offsets and the newlines intact so a
 * reported line number is the line the compiler sees. Strings, template
 * literals and regex literals are tracked, so a `//` inside a URL and a quote
 * inside a regex are not mistaken for a comment.
 */
function stripComments(source) {
  const out = [];
  const significant = () =>
    out.length === 0 ? "" : out.slice().reverse().find((character) => !/\s/.test(character)) ?? "";
  // `<` is deliberately absent: it precedes the `/` of a JSX closing tag
  // (`</p>`), and mistaking that for a regex would swallow the next comment.
  const REGEX_PRECEDERS = new Set(["", "(", ",", "=", ":", "[", "!", "&", "|", "?", "{", "}", ";", "+", "-", "*", "%", ">", "~", "^"]);
  const WORD = /[A-Za-z0-9_$]/;
  let state = "code";
  let inClass = false;

  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    const next = source[index + 1];

    if (state === "code") {
      if (character === "/" && next === "/") {
        state = "line";
        out.push(" ", " ");
        index += 1;
        continue;
      }
      if (character === "/" && next === "*") {
        state = "block";
        out.push(" ", " ");
        index += 1;
        continue;
      }
      if (character === "/" && REGEX_PRECEDERS.has(significant())) {
        state = "regex";
        inClass = false;
        out.push(character);
        continue;
      }
      // An apostrophe directly after a word character is prose ("Don't"), not
      // the opening quote of a string, so it must not swallow the rest of the
      // file — comments after that line would stop being stripped.
      if (character === "'" && !WORD.test(source[index - 1] ?? "")) state = "single";
      else if (character === '"') state = "double";
      else if (character === "`") state = "template";
      out.push(character);
      continue;
    }

    if (state === "line") {
      if (character === "\n") {
        state = "code";
        out.push(character);
      } else {
        out.push(" ");
      }
      continue;
    }

    if (state === "block") {
      if (character === "*" && next === "/") {
        state = "code";
        out.push(" ", " ");
        index += 1;
        continue;
      }
      out.push(character === "\n" ? "\n" : " ");
      continue;
    }

    if (state === "regex") {
      if (character === "\\") {
        out.push(character, next ?? "");
        index += 1;
        continue;
      }
      if (character === "[") inClass = true;
      else if (character === "]") inClass = false;
      else if (character === "/" && !inClass) state = "code";
      out.push(character);
      continue;
    }

    // String and template states: copy verbatim, honour escapes, leave on the
    // matching delimiter.
    if (character === "\\") {
      out.push(character, next ?? "");
      index += 1;
      continue;
    }
    if (
      (state === "single" && character === "'") ||
      (state === "double" && character === '"') ||
      (state === "template" && character === "`")
    ) {
      state = "code";
    }
    out.push(character);
  }

  return out.join("");
}

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
 * Report one `style={{ ... }}` declaration whose value is a px measurement. A
 * bare number (`height: 12`) and a px string (`height: "12px"`) both count; a
 * token reference (`animationDuration: SPIN_DURATION`) and a unitless number
 * (`gridColumn: 1`) do not.
 */
function pxMeasurement(body) {
  for (const match of body.matchAll(/([A-Za-z][A-Za-z0-9-]*)\s*:\s*([^,}\n]+)/g)) {
    const property = match[1];
    const value = match[2].trim();
    if (/[0-9]+(?:\.[0-9]+)?px/.test(value)) return true;
    if (/^[0-9]+(?:\.[0-9]+)?$/.test(value) && !UNITLESS_PROPERTIES.has(property)) return true;
  }
  return false;
}

function styleViolations(file, source) {
  const hits = [];
  for (const match of source.matchAll(/style=\{\{/g)) {
    const start = match.index ?? 0;
    const close = source.indexOf("}}", start + 8);
    const body = source.slice(start, close === -1 ? source.length : close + 2);
    if (pxMeasurement(body)) hits.push(violation(file, source, start, STYLE_RULE, body));
  }
  return hits;
}

/** Every hand-written source file, walked recursively from ui/src. */
function fenceFiles(dir = SRC, prefix = "") {
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const name = prefix === "" ? entry.name : `${prefix}/${entry.name}`;
    if (entry.isDirectory()) {
      if (EXEMPT_DIRS.has(name)) continue;
      files.push(...fenceFiles(join(dir, entry.name), name));
      continue;
    }
    if (!/\.tsx?$/.test(entry.name)) continue;
    if (BARREL_FILES.has(entry.name)) continue;
    files.push(name);
  }
  return files.sort();
}

function scan() {
  const files = fenceFiles();
  const hits = [];
  const perRule = new Map([...RULES.map((rule) => rule.id), STYLE_RULE.id].map((id) => [id, 0]));

  for (const name of files) {
    const source = stripComments(readFileSync(join(SRC, name), "utf8"));
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
  const directories = [...new Set(files.map((name) => name.split("/")[0]).filter((name) => !name.includes(".")))].sort();
  const root = files.filter((name) => !name.includes("/")).length;
  console.log(`\ntoken fence: ${files.length} files x ${perRule.size} rules`);
  console.log(`  scope: src/**/*.{ts,tsx} recursive; exempt ${[...EXEMPT_DIRS].join(", ")}/**, ${TOKEN_FILE} (css), ${[...BARREL_FILES].join(", ")} barrels`);
  for (const [id, count] of perRule) console.log(`  ${pad(id, 20)} ${count} hit${count === 1 ? "" : "s"}`);
  if (hits.length === 0) {
    console.log(`  scanned: ${directories.join(", ")}${root === 0 ? "" : ` + ${root} root file${root === 1 ? "" : "s"}`}`);
    return;
  }
  console.log("");
  for (const entry of hits) {
    console.log(`  ${entry.file}:${entry.line}  ${entry.rule}: ${entry.text}`);
    console.log(`    ${entry.why}`);
  }
}

test("the fence covers all of ui/src except the generated, token-owning and barrel files", () => {
  const { files } = scan();
  assert.ok(files.length >= 150, `expected the whole source tree, found ${files.length}`);
  assert.ok(
    files.includes("components/domain/AnswerStream.tsx"),
    "AnswerStream.tsx missing from the fence",
  );
  assert.ok(files.includes("routes/Deck.tsx"), "routes/Deck.tsx missing from the fence");
  // A recursive walk reaches files three directories down, not just src/*.
  assert.ok(
    files.includes("components/primitives/icons/index.tsx"),
    "the recursive walk must reach components/primitives/icons/index.tsx",
  );
  assert.ok(new Set(files.map((name) => name.split("/").length)).size >= 3, "the walk must recurse");
  assert.ok(
    files.every((name) => name.endsWith(".ts") || name.endsWith(".tsx")),
    "only TypeScript sources belong to the fence",
  );
  // Exemptions: generated bindings, the token file, and re-export barrels.
  assert.ok(!files.some((name) => name.startsWith("bindings/")), "generated bindings must not be scanned");
  assert.ok(!files.includes(TOKEN_FILE), "the token file must not be scanned");
  assert.ok(!files.some((name) => name.endsWith("/index.ts")), "index.ts barrels must not be scanned");
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
  // The inline-style rule is structural: it catches both spellings, leaves a
  // token-valued style alone (Spinner animates with a constant) and leaves a
  // unitless grid line alone (SessionBands packs lanes into grid rows).
  assert.ok(pxMeasurement("style={{ height: 12 }}"), "a bare number must be a px measurement");
  assert.ok(pxMeasurement('style={{ height: "12px" }}'), "a px string must be a px measurement");
  assert.ok(pxMeasurement("style={{ marginTop: 7 }}"), "a bare number must be a px measurement");
  assert.ok(!pxMeasurement("style={{ animationDuration: SPIN_DURATION }}"));
  assert.ok(!pxMeasurement("style={{ gridColumn: 1 }}"), "a grid line is not a px length");
  // Comment prose is blanked before the rules run, so issue numbers are safe.
  const prose = stripComments('// #106 fixed in [13px] with var(--p-mist-000)\n<div className="bg-base" />');
  assert.equal(prose.includes("#106"), false, "a comment must not survive");
  assert.equal([...prose.matchAll(RULES[0].pattern)].length, 0, "an issue number is not a colour");
  assert.equal(prose.includes('className="bg-base"'), true, "code on the next line must survive");
  // An apostrophe in JSX copy is prose, not a string opener: if it opened one,
  // every comment after that line would stop being stripped.
  const contraction = stripComments("<p>Don't shift</p>\n// #161 fixed\n<span className='x' />");
  assert.equal(contraction.includes("#161"), false, "comment stripping must survive an apostrophe");
});

test("no source file escapes the token layer", () => {
  const result = scan();
  report(result);
  const detail = result.hits
    .map((entry) => `${entry.file}:${entry.line} [${entry.rule}] ${entry.text} (${entry.why})`)
    .join("\n");
  assert.equal(result.hits.length, 0, `token-fence violations:\n${detail}`);
});
