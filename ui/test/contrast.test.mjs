// Semantic-token contrast fence (UI_REFERENCE 1-2). Every text/surface pair the
// design system permits must clear WCAG AA (4.5:1) in all four themes, and this
// file is parsed as text because the runner has no CSS engine: the tokens ARE
// the contract, so the test reads exactly what ships.
//
// The four semantic blocks (:root[data-theme=...]) are the only colour source a
// component may see; each value is a var() chain into the --p- primitives, so
// resolution is recursive and a chain that ends anywhere but a hex literal is a
// hard failure, never a silently skipped pair.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

/* global console */

const TOKENS = join(import.meta.dirname, "..", "src", "styles", "tokens.css");
const THEMES = ["pine-light", "pine-dark", "ink-light", "ink-dark"];
const SURFACES = [
  "--surface-app",
  "--surface-chrome",
  "--surface-pane",
  "--surface-raised",
  "--surface-inset",
  "--surface-hover",
  "--surface-selected",
];
const TEXT_ON_SURFACE = ["--text-primary", "--text-secondary", "--text-tertiary"];

/** Every (foreground, background) pair a component may actually put together. */
const PAIRS = [
  ...TEXT_ON_SURFACE.flatMap((fg) => SURFACES.map((bg) => [fg, bg])),
  ["--text-on-accent", "--accent"],
  ["--text-on-accent", "--accent-strong"],
  ["--action-primary-fg", "--action-primary-bg"],
  ["--action-primary-fg", "--action-primary-bg-hover"],
  ["--action-danger-fg", "--action-danger-bg"],
  ["--action-danger-fg", "--action-danger-bg-hover"],
  ["--status-ok-fg", "--status-ok-bg"],
  ["--status-warn-fg", "--status-warn-bg"],
  ["--status-danger-fg", "--status-danger-bg"],
  ["--mark-fg", "--mark-bg"],
];

const AA_TEXT = 4.5;

// ---------------------------------------------------------------------------
// CSS text parsing: flat blocks only (tokens.css has no nested at-rules).
// ---------------------------------------------------------------------------

/** Split a stylesheet into `selector -> body` blocks with comments removed. */
function cssBlocks(css) {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const blocks = [];
  for (const match of stripped.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    blocks.push({ selector: match[1].trim(), body: match[2] });
  }
  return blocks;
}

/** `--name: value;` declarations of one block, in source order. */
function declarations(body) {
  const map = new Map();
  for (const match of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    map.set(match[1], match[2].trim());
  }
  return map;
}

const isThemeSelector = (selector) => /^:root\[data-theme="[^"]+"\]$/.test(selector);
const themeName = (selector) => selector.slice(selector.indexOf('"') + 1, selector.lastIndexOf('"'));

/**
 * Layer 1 (the --p- primitives) plus the theme-independent scales. Both live in
 * bare `:root` blocks; a block that also names a data-theme or data-density is
 * a layer-2 theme block and must never leak into this map, otherwise a token
 * missing from one theme would silently resolve through the default theme.
 */
function primitiveLayer(css) {
  const merged = new Map();
  for (const { selector, body } of cssBlocks(css)) {
    const selectors = selector.split(",").map((part) => part.trim());
    const bare = selectors.includes(":root");
    const scoped = selectors.some((part) => part.includes("data-theme") || part.includes("data-density"));
    if (!bare || scoped) continue;
    for (const [name, value] of declarations(body)) merged.set(name, value);
  }
  return merged;
}

/** Layer 2, one map per theme. */
function themeLayers(css) {
  const themes = new Map();
  for (const { selector, body } of cssBlocks(css)) {
    for (const part of selector.split(",").map((p) => p.trim())) {
      if (!isThemeSelector(part)) continue;
      const name = themeName(part);
      const existing = themes.get(name) ?? new Map();
      for (const [token, value] of declarations(body)) existing.set(token, value);
      themes.set(name, existing);
    }
  }
  return themes;
}

// ---------------------------------------------------------------------------
// Resolution and WCAG math
// ---------------------------------------------------------------------------

/**
 * Follow `var(--x)` links until a literal is reached. Resolution order is the
 * theme first, then the primitive layer, which is what the cascade does.
 * Returns `undefined` when the token is not declared in that theme at all.
 */
function resolve(token, theme, primitives) {
  const seen = [];
  let name = token;
  let value = theme.get(name) ?? primitives.get(name);
  while (value !== undefined && value.startsWith("var(")) {
    const match = /^var\(\s*(--[\w-]+)\s*\)$/.exec(value);
    if (!match) throw new Error(`unsupported var() form for ${name}: ${value}`);
    if (seen.includes(name)) throw new Error(`var() cycle: ${[...seen, name, match[1]].join(" -> ")}`);
    seen.push(name);
    name = match[1];
    value = theme.get(name) ?? primitives.get(name);
  }
  if (value === undefined) return undefined;
  return { token: name, value, chain: [...seen, name] };
}

/** Parse `#rgb` / `#rrggbb` (alpha forms keep their colour channels). */
function rgb(value) {
  const match = /^#([0-9a-fA-F]{3,8})$/.exec(value);
  if (!match) throw new Error(`not a hex colour literal: ${value}`);
  let hex = match[1];
  if (hex.length === 4) hex = hex.slice(0, 3); // #rgba -> #rgb
  if (hex.length === 8) hex = hex.slice(0, 6); // #rrggbbaa -> #rrggbb
  if (hex.length !== 3 && hex.length !== 6) throw new Error(`not a hex colour literal: ${value}`);
  if (hex.length === 3) hex = hex.replace(/./g, (char) => char + char);
  return [0, 2, 4].map((offset) => parseInt(hex.slice(offset, offset + 2), 16));
}

function relativeLuminance([r, g, b]) {
  const channel = (value) => {
    const scaled = value / 255;
    return scaled <= 0.03928 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function contrast(foreground, background) {
  const a = relativeLuminance(rgb(foreground));
  const b = relativeLuminance(rgb(background));
  const lighter = Math.max(a, b);
  const darker = Math.min(a, b);
  return (lighter + 0.05) / (darker + 0.05);
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const css = readFileSync(TOKENS, "utf8");
const primitives = primitiveLayer(css);
const themes = themeLayers(css);

/** Resolve a pair for one theme; `null` means "not declared in this theme". */
function pair(theme, [fg, bg]) {
  const foreground = resolve(fg, theme, primitives);
  const background = resolve(bg, theme, primitives);
  if (!foreground || !background) return null;
  return { foreground, background, ratio: contrast(foreground.value, background.value) };
}

const label = (token) => token.replace(/^--/, "");
const pad = (text, width) => String(text).padEnd(width);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test("the token file declares exactly the four contract themes", () => {
  assert.deepEqual([...themes.keys()].sort(), [...THEMES].sort());
  for (const name of THEMES) {
    assert.ok(themes.get(name).size > 30, `${name} declares only ${themes.get(name).size} semantic tokens`);
  }
  assert.ok(primitives.size > 60, `primitive layer declares only ${primitives.size} tokens`);
});

test("resolution follows semantic var() chains into the primitive layer", () => {
  const light = themes.get("pine-light");
  // --mark-fg -> --text-primary -> --p-mist-ink-900: two semantic hops.
  const mark = resolve("--mark-fg", light, primitives);
  assert.deepEqual(mark.chain, ["--mark-fg", "--text-primary", "--p-mist-ink-900"]);
  assert.equal(mark.value, "#181c19");
  // --status-ok-bg -> --surface-raised -> --p-mist-000 (pine success is neutral).
  const okBg = resolve("--status-ok-bg", light, primitives);
  assert.deepEqual(okBg.chain, ["--status-ok-bg", "--surface-raised", "--p-mist-000"]);
  // A token the theme never declares resolves to undefined, never to a guess.
  assert.equal(resolve("--text-not-a-token", light, primitives), undefined);
  // A cycle is a failure, not an infinite loop.
  const cyclic = new Map([
    ["--a", "var(--b)"],
    ["--b", "var(--a)"],
  ]);
  assert.throws(() => resolve("--a", cyclic, new Map()), /var\(\) cycle/);
});

test("contrast math is WCAG 2.x relative luminance", () => {
  assert.equal(Number(contrast("#000000", "#ffffff").toFixed(4)), 21);
  assert.equal(Number(contrast("#ffffff", "#ffffff").toFixed(4)), 1);
  assert.equal(Number(contrast("#ffffff", "#000000").toFixed(4)), 21);
  assert.equal(Number(contrast("#767676", "#ffffff").toFixed(4)), 4.5422);
});

test("every theme declares every text/surface pair", () => {
  const missing = [];
  for (const name of THEMES) {
    const theme = themes.get(name);
    for (const [fg, bg] of PAIRS) {
      for (const token of [fg, bg]) {
        if (!resolve(token, theme, primitives)) missing.push(`${name} ${token}`);
      }
    }
  }
  assert.deepEqual(missing, [], `tokens missing from a theme (the fence cannot guess):\n${missing.join("\n")}`);
});

test(`every semantic text pair clears WCAG AA ${AA_TEXT}:1 in all four themes`, () => {
  const failures = [];
  const perThemeMinimum = [];
  let reported = 0;

  for (const name of THEMES) {
    const theme = themes.get(name);
    let minimum = { ratio: Infinity, fg: "", bg: "" };
    const skipped = [];

    console.log(`\n${name}`);
    for (const [fg, bg] of PAIRS) {
      const result = pair(theme, [fg, bg]);
      if (!result) {
        // Only reachable if a theme drops a pair token; reported, never guessed.
        skipped.push(`${label(fg)} on ${label(bg)}`);
        continue;
      }
      const verdict = result.ratio >= AA_TEXT ? "ok  " : "FAIL";
      console.log(
        `  ${verdict} ${pad(label(fg), 20)} on ${pad(label(bg), 24)} ${result.ratio.toFixed(2)}:1`,
      );
      reported += 1;
      if (result.ratio < minimum.ratio) minimum = { ratio: result.ratio, fg: label(fg), bg: label(bg) };
      if (result.ratio < AA_TEXT) {
        failures.push(`${name}: ${label(fg)} on ${label(bg)} = ${result.ratio.toFixed(2)}:1`);
      }
    }
    if (skipped.length > 0) console.log(`  skipped (not declared in ${name}): ${skipped.join(", ")}`);
    perThemeMinimum.push(`${name} min ${minimum.ratio.toFixed(2)}:1 (${minimum.fg} on ${minimum.bg})`);
  }

  console.log(`\nchecked ${reported} pairs across ${THEMES.length} themes; minimum per theme:`);
  for (const line of perThemeMinimum) console.log(`  ${line}`);

  assert.deepEqual(failures, [], `pairs below WCAG AA ${AA_TEXT}:1:\n${failures.join("\n")}`);
});
