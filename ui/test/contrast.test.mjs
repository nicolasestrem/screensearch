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

import { contrast, primitiveLayer, resolve, themeLayers } from "./lib/contrastTokens.mjs";

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

// The CSS parsing and the WCAG math live in `lib/contrastTokens.mjs`, so the markdown
// fences in `answer-prose.test.mjs` check the same implementation this fence does.

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
