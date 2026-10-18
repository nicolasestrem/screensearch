// Focused regressions for the pure promotion Markdown parser (`03 §8e`, G4).
// The parser and the Rust backend must read fenced code the same way: a fence
// closes only on the same marker with a run at least as long and only whitespace
// after it, so fake headings and bullets inside a fence are never selectable.
import assert from "node:assert/strict";
import test from "node:test";

import {
  parsePromotableItems,
  selectedPromotionItems,
} from "../src/lib/promotion.ts";

test("mixed fence markers: a tilde block ignores backticks, short runs and trailing text", () => {
  const markdown = [
    "## Activity",
    "- real activity",
    "~~~markdown",
    "## Next steps",
    "- fenced log",
    "```",
    "## Open questions",
    "- backtick inside a tilde block",
    "~~",
    "~~~ not a close",
    "- still fenced",
    "~~~",
    "## Activity",
    "- real log",
  ].join("\n");

  const items = parsePromotableItems(markdown);
  assert.deepEqual(
    items.map((item) => item.text),
    ["real activity", "real log"],
  );
  assert.deepEqual(
    items.map((item) => item.section),
    ["log", "log"],
  );
});

test("a four-backtick fence containing a three-backtick line stays fenced", () => {
  const markdown = [
    "## Explicitly evidenced decisions",
    "- real decision",
    "````markdown",
    "```",
    "## Next steps",
    "- fenced fake",
    "```",
    "````",
    "## Next steps",
    "- real next",
  ].join("\n");

  const items = parsePromotableItems(markdown);
  assert.deepEqual(
    items.map((item) => item.text),
    ["real decision", "real next"],
  );
  assert.deepEqual(
    items.map((item) => item.section),
    ["decisions", "next_steps"],
  );
});

test("fake and renamed headings are not promotion sections", () => {
  const markdown = [
    "## Activity archive",
    "- not a promotion bullet",
    "## Open Questions ##",
    "- real question",
    "```text",
    "## Next steps",
    "- fenced fake",
    "```",
    "## Next Steps ###",
    "- real next",
  ].join("\n");

  const items = parsePromotableItems(markdown);
  assert.deepEqual(
    items.map((item) => item.text),
    ["real question", "real next"],
  );
  assert.deepEqual(
    items.map((item) => item.section),
    ["open_questions", "next_steps"],
  );
});

test("legitimate duplicate source bullets remain separately selectable", () => {
  const markdown = ["## Activity", "- duplicate bullet", "- duplicate bullet"].join("\n");

  const items = parsePromotableItems(markdown);
  assert.equal(items.length, 2);
  assert.deepEqual(
    items.map((item) => item.text),
    ["duplicate bullet", "duplicate bullet"],
  );
  assert.notEqual(items[0].id, items[1].id);

  const wire = selectedPromotionItems(items, new Set(items.map((item) => item.id)));
  assert.deepEqual(wire, [
    { section: "log", text: "duplicate bullet" },
    { section: "log", text: "duplicate bullet" },
  ]);
});

test("CRLF and a missing final newline parse the same as LF", () => {
  const lf = ["## Activity", "- crlf bullet"].join("\n");
  const crlf = ["## Activity", "- crlf bullet"].join("\r\n");

  assert.deepEqual(parsePromotableItems(crlf), parsePromotableItems(lf));
  assert.deepEqual(parsePromotableItems(`${crlf}\r\n`), parsePromotableItems(crlf));
});

test("an unclosed fence at EOF never exposes its lines as bullets", () => {
  const markdown = ["## Activity", "- real", "```", "## Log", "- fenced"].join("\n");

  const items = parsePromotableItems(markdown);
  assert.deepEqual(
    items.map((item) => item.text),
    ["real"],
  );
});
