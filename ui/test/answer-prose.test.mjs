// Answer prose (`UI_REFERENCE` §4 ask/recall, §5 inventory): an answer is model-written
// markdown, so every surface that shows one renders it through `AnswerProse`. The Ask v2
// conversation used to print the answer as pre-wrapped text, which showed `**Focused
// Work:**` and `- item` as literal characters; this fence holds the shared renderer in
// place and proves the rendering itself, without a DOM.
//
// What it checks:
//  1. one renderer - `AnswerStream` and the domain barrel expose the same component, and
//     the Ask route reaches it (twice: the persisted turn and the streaming turn) instead of
//     keeping its own markdown setup or a pre-wrapped `<p>`;
//  2. it renders markdown - the sample answer produces real elements (strong, lists, code,
//     heading, blockquote, GFM table, link) through the shipped options;
//  3. no visual drift - `AnswerStream`'s answer region is its previous container, unchanged,
//     wrapping exactly what `AnswerProse` renders;
//  4. the token map cannot drift from `tailwind.config.js`, and every text row clears WCAG
//     AA on every surface an answer can appear on, in all four themes.
//
// What it does NOT prove: that the installed app paints these tokens (the parent does the
// visual proof), nor anything about `ReportView`, which still owns a second (report) markdown
// setup outside this change.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { register } from "node:module";
import { join } from "node:path";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ts from "typescript";

import { contrast, primitiveLayer, resolve, themeLayers } from "./lib/contrastTokens.mjs";

/* global console */

const SRC = join(import.meta.dirname, "..", "src");
const TOKENS = join(SRC, "styles", "tokens.css");
const TAILWIND = join(import.meta.dirname, "..", "tailwind.config.js");
const ASK_ROUTE = join(SRC, "routes", "Ask.tsx");
const THEMES = ["pine-light", "pine-dark", "ink-light", "ink-dark"];
const AA_TEXT = 4.5;

register("./ui-loader.mjs", import.meta.url);
const { AnswerProse } = await import("../src/components/domain/AnswerProse.tsx");
const { AnswerStream } = await import("../src/components/domain/AnswerStream.tsx");
const domain = await import("../src/components/domain/index.ts");
const {
  ANSWER_PROSE_COMPONENTS,
  ANSWER_PROSE_ELEMENTS,
  ANSWER_PROSE_REMARK_PLUGINS,
  ANSWER_PROSE_SURFACES,
  ANSWER_PROSE_TYPOGRAPHY,
} = await import("../src/components/domain/answerProseMarkdown.ts");

/** One answer with every element the map claims to cover. */
const SAMPLE = [
  "# Focused Work",
  "",
  "**Focused Work:** two deep blocks, both on the search pipeline.",
  "",
  "- Shipped the markdown fix",
  "- Reviewed the contrast fence",
  "",
  "1. First finding",
  "2. Second finding",
  "",
  "Inline `AnswerProse` and an [external link](https://example.com/doc).",
  "",
  "> A quoted observation, with *emphasis*.",
  "",
  "| Surface | Token |",
  "| --- | --- |",
  "| pane | --surface-pane |",
  "",
  "```",
  "const answer = true;",
  "```",
].join("\n");

/** The tags the shared renderer must style, spelled as the token map spells them. */
const REQUIRED_ELEMENTS = [
  "strong",
  "em",
  "ul",
  "ol",
  "li",
  "code",
  "pre",
  "h1",
  "h2",
  "h3",
  "a",
  "blockquote",
  "table",
];

const render = (element) => renderToStaticMarkup(element);
const prose = (text) => render(React.createElement(AnswerProse, { text }));

// ---------------------------------------------------------------------------
// 1. One renderer
// ---------------------------------------------------------------------------

test("the domain barrel exports the component AnswerStream renders", () => {
  assert.equal(domain.AnswerProse, AnswerProse);
  assert.equal(typeof AnswerProse, "function");
});

/** Read `ui/src/routes/Ask.tsx` once, with parent pointers, for the structural checks. */
function parseAsk() {
  const source = readFileSync(ASK_ROUTE, "utf8");
  return {
    source,
    file: ts.createSourceFile("Ask.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX),
  };
}

/** The tag name of a JSX element node, or `null` for any other node. */
function tagName(node, file) {
  if (ts.isJsxElement(node)) return node.openingElement.tagName.getText(file);
  if (ts.isJsxSelfClosingElement(node)) return node.tagName.getText(file);
  return null;
}

/** The name of the nearest enclosing function declaration (the component that owns it). */
function owner(node) {
  for (let parent = node.parent; parent; parent = parent.parent) {
    if (ts.isFunctionDeclaration(parent) && parent.name) return parent.name.text;
  }
  return null;
}

/** The JSX attributes of an element node, over its opening element. */
function attributes(node) {
  if (ts.isJsxElement(node)) return node.openingElement.attributes.properties;
  if (ts.isJsxSelfClosingElement(node)) return node.attributes.properties;
  return [];
}

test("the Ask route renders both answer positions through the shared renderer", () => {
  const { source, file } = parseAsk();

  const specifiers = [];
  const namedImports = new Map();
  const visitImport = (node) => {
    if (!ts.isImportDeclaration(node)) return;
    const from = node.moduleSpecifier.getText(file).replace(/['"]/g, "");
    specifiers.push(from);
    const clause = node.importClause;
    if (!clause?.namedBindings || !ts.isNamedImports(clause.namedBindings)) return;
    for (const element of clause.namedBindings.elements) {
      namedImports.set(element.name.text, from);
    }
  };
  ts.forEachChild(file, visitImport);

  // No second markdown setup in the route: no react-markdown, no remark plugin import.
  assert.ok(!specifiers.includes("react-markdown"), "Ask.tsx imports react-markdown directly");
  assert.ok(!specifiers.includes("remark-gfm"), "Ask.tsx imports remark-gfm directly");
  assert.equal(
    namedImports.get("AnswerProse"),
    "../components/domain/AnswerProse",
    "Ask.tsx does not import the shared renderer",
  );
  assert.equal(
    namedImports.get("ANSWER_PROSE_TYPOGRAPHY"),
    "../components/domain/answerProseMarkdown",
    "Ask.tsx does not use the shared prose typography",
  );

  // Exactly two answer renderings, one per turn component.
  const uses = [];
  const visit = (node) => {
    if (tagName(node, file) === "AnswerProse") uses.push(node);
    ts.forEachChild(node, visit);
  };
  visit(file);
  assert.deepEqual(uses.map(owner).sort(), ["StreamingTurn", "TurnBlock"]);

  for (const use of uses) {
    // The answer string is handed over as text, never printed as a child expression.
    const text = attributes(use).find(
      (attribute) => ts.isJsxAttribute(attribute) && attribute.name.getText(file) === "text",
    );
    assert.ok(text, "an AnswerProse call site does not receive the answer");
    assert.equal(text.initializer.expression.getText(file), "answer");

    // Its container carries the shared prose typography, so the deck tokens apply.
    const container = attributes(use.parent).find(
      (attribute) => ts.isJsxAttribute(attribute) && attribute.name.getText(file) === "className",
    );
    assert.ok(container, "the answer container has no className");
    assert.ok(
      container.initializer.expression.getText(file).includes("${ANSWER_PROSE_TYPOGRAPHY}"),
      "the answer container does not use ANSWER_PROSE_TYPOGRAPHY",
    );
  }

  // The regression itself: a pre-wrapped paragraph prints markdown literally.
  assert.ok(!source.includes("whitespace-pre-wrap"), "Ask.tsx still pre-wraps text");
});

// ---------------------------------------------------------------------------
// 2. It renders markdown
// ---------------------------------------------------------------------------

test("the shared options are the markdown setup: GFM plus the one link override", () => {
  assert.deepEqual(Object.keys(ANSWER_PROSE_COMPONENTS), ["a"]);
  assert.equal(typeof ANSWER_PROSE_COMPONENTS.a, "function");
  assert.equal(ANSWER_PROSE_REMARK_PLUGINS.length, 1);
  assert.equal(ANSWER_PROSE_TYPOGRAPHY, "prose prose-deck");
});

test("an answer renders as elements, not as literal markdown characters", () => {
  const html = prose(SAMPLE);

  assert.ok(!html.includes("**"), "the answer still contains literal emphasis markers");
  assert.ok(html.includes("<h1>Focused Work</h1>"), html);
  assert.ok(html.includes("<strong>Focused Work:</strong>"), html);
  assert.ok(html.includes("<em>emphasis</em>"), html);
  assert.ok(html.includes("<ul>") && html.includes("<li>Shipped the markdown fix</li>"), html);
  assert.ok(html.includes("<ol>") && html.includes("<li>First finding</li>"), html);
  assert.ok(html.includes("<code>AnswerProse</code>"), html);
  assert.ok(html.includes("<pre><code>"), html);
  assert.ok(html.includes("<blockquote>"), html);
  // GFM only: a table without remark-gfm would render as pipe text.
  assert.ok(html.includes("<table>") && html.includes("<th>Surface</th>"), html);
  // The one override: model-output links leave the WebView.
  assert.ok(
    html.includes(
      '<a href="https://example.com/doc" target="_blank" rel="noopener noreferrer">external link</a>',
    ),
    html,
  );
});

// ---------------------------------------------------------------------------
// 3. No visual or behavioural drift in AnswerStream
// ---------------------------------------------------------------------------

test("AnswerStream keeps its answer region and renders exactly the shared prose", () => {
  const proseHtml = prose(SAMPLE);
  const streamHtml = render(
    React.createElement(AnswerStream, {
      phase: "done",
      thinking: "Looked at three frames.",
      answer: SAMPLE,
      citations: [],
      error: null,
      onOpenFrame: () => {},
    }),
  );

  // The focusable answer region is unchanged, so the reported answer lands where it did.
  const region =
    `<div tabindex="-1" role="region" aria-label="Answer" ` +
    `class="${ANSWER_PROSE_TYPOGRAPHY} max-w-none focus-visible:outline-none">` +
    `${proseHtml}</div>`;
  assert.ok(streamHtml.includes(region), streamHtml);

  // The collapsible thinking trace and the auto-follow sentinel are untouched.
  assert.ok(streamHtml.includes("<details"), streamHtml);
  assert.ok(streamHtml.includes(">Thinking</summary>"), streamHtml);
  assert.ok(streamHtml.includes("Looked at three frames."), streamHtml);
  assert.ok(streamHtml.trimEnd().endsWith('<div aria-hidden="true"></div></div>'), streamHtml);
});

// ---------------------------------------------------------------------------
// 4. Tokens: no drift from the shipped CSS, and AA on every surface
// ---------------------------------------------------------------------------

const css = readFileSync(TOKENS, "utf8");
const primitives = primitiveLayer(css);
const themes = themeLayers(css);

/** The `deck` typography variant's `--tw-prose-*` mapping, read from the real config. */
function deckVariables() {
  const config = readFileSync(TAILWIND, "utf8");
  const map = new Map();
  for (const match of config.matchAll(/"(--tw-prose-[\w-]+)":\s*"var\((--[\w-]+)\)"/g)) {
    map.set(match[1], match[2]);
  }
  return map;
}

test("the element map is exactly the deck typography variant in tailwind.config.js", () => {
  const deck = deckVariables();
  assert.ok(deck.size >= 16, `only ${deck.size} deck variables parsed`);
  const rows = Object.values(ANSWER_PROSE_ELEMENTS);
  assert.deepEqual(
    [...new Set(rows.map((row) => row.from))].sort(),
    [...deck.keys()].sort(),
    "the token map and tailwind.config.js disagree about which prose variables exist",
  );
  for (const row of rows) {
    assert.equal(row.token, deck.get(row.from), `${row.from} does not resolve to ${row.token}`);
  }
});

test("the element map covers every markdown element an answer emits", () => {
  const styled = new Set(Object.values(ANSWER_PROSE_ELEMENTS).flatMap((row) => row.elements));
  const missing = REQUIRED_ELEMENTS.filter((element) => !styled.has(element));
  assert.deepEqual(missing, [], `elements with no token row:\n${missing.join("\n")}`);
});

test("every answer text token clears WCAG AA on every surface, in all four themes", () => {
  assert.deepEqual([...themes.keys()].sort(), [...THEMES].sort());

  const failures = [];
  const missing = [];
  const perThemeMinimum = [];
  let checked = 0;

  for (const name of THEMES) {
    const theme = themes.get(name);
    let minimum = { ratio: Infinity, row: "", surface: "" };
    console.log(`\n${name}`);

    for (const [row, spec] of Object.entries(ANSWER_PROSE_ELEMENTS)) {
      // `text` is fenced against every answer surface; `surface` must simply exist as a
      // surface token; `border` is a decorative rule WCAG does not grade.
      const foreground = resolve(spec.token, theme, primitives);
      if (!foreground) {
        missing.push(`${name} ${row} ${spec.token}`);
        continue;
      }
      const surfaces = spec.kind === "text" ? ANSWER_PROSE_SURFACES : [];
      if (spec.kind === "border") {
        console.log(`  rule ${row.padEnd(16)} ${spec.token}`);
        continue;
      }
      if (spec.kind === "surface") {
        assert.match(spec.token, /^--surface-/, `${row} is not a surface token`);
        console.log(`  fill ${row.padEnd(16)} ${spec.token}`);
        continue;
      }
      for (const surfaceToken of surfaces) {
        const background = resolve(surfaceToken, theme, primitives);
        if (!background) {
          missing.push(`${name} ${row} ${surfaceToken}`);
          continue;
        }
        const ratio = contrast(foreground.value, background.value);
        checked += 1;
        const verdict = ratio >= AA_TEXT ? "ok  " : "FAIL";
        console.log(
          `  ${verdict} ${row.padEnd(16)} on ${surfaceToken.padEnd(20)} ${ratio.toFixed(2)}:1`,
        );
        if (ratio < minimum.ratio) {
          minimum = { ratio, row, surface: surfaceToken };
        }
        if (ratio < AA_TEXT) {
          failures.push(`${name}: ${row} (${spec.token}) on ${surfaceToken} = ${ratio.toFixed(2)}:1`);
        }
      }
    }
    perThemeMinimum.push(
      `${name} min ${minimum.ratio.toFixed(2)}:1 (${minimum.row} on ${minimum.surface})`,
    );
  }

  console.log(`\nchecked ${checked} element/surface pairs across ${THEMES.length} themes:`);
  for (const line of perThemeMinimum) console.log(`  ${line}`);

  assert.deepEqual(missing, [], `tokens missing from a theme:\n${missing.join("\n")}`);
  assert.deepEqual(failures, [], `answer prose below WCAG AA ${AA_TEXT}:1:\n${failures.join("\n")}`);
});
