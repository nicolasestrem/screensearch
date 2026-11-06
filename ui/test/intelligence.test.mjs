// Intelligence components (`03 §7c` P9 block): the three seams that can be wrong
// without a browser. The evidence destination mapping is the one every claim
// depends on; the duration/count readers are pure formatting; the connected-threads
// state copy is the only place a panel state can lose its words. No React render -
// the runner has no DOM and the components' layout is reviewed, not asserted.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);

const { evidenceHref } = await import("../src/components/intelligence/evidenceLink.tsx");
const { formatCount, formatDuration, formatGap } = await import(
  "../src/components/intelligence/format.ts"
);
const { CONNECTION_STATE_COPY } = await import(
  "../src/components/intelligence/ConnectionsPanel.tsx"
);

test("every evidence kind resolves to its granted destination", () => {
  assert.equal(evidenceHref("frame", 3453), "/timeline/3453");
  assert.equal(evidenceHref("session", 12), "/timeline/session/12");
  assert.equal(evidenceHref("thread", 7), "/work/7");
  assert.equal(evidenceHref("entity", 4), "/work?entity=4");
  assert.equal(evidenceHref("mark", 9, 3453), "/timeline/3453");
  assert.equal(evidenceHref("report", 2), "/settings/data");
});

test("a mark without a frame has no destination, and keeps its own ref otherwise", () => {
  assert.equal(evidenceHref("mark", 9), null);
  assert.equal(evidenceHref("mark", 9, undefined), null);
  assert.equal(evidenceHref("mark", 9, null), null);
  // A mark's own id never becomes the moment: only the frame it flags does.
  assert.equal(evidenceHref("frame", 9, 3453), "/timeline/9");
});

test("formatDuration reads the spans a thread and an insight actually carry", () => {
  assert.equal(formatDuration(0), "<1 m");
  assert.equal(formatDuration(59_000), "<1 m");
  assert.equal(formatDuration(60_000), "1 m");
  assert.equal(formatDuration(45 * 60_000), "45 m");
  assert.equal(formatDuration(3_600_000), "1 h");
  assert.equal(formatDuration(3 * 3_600_000 + 20 * 60_000), "3 h 20 m");
  assert.equal(formatDuration(2 * 86_400_000), "2 d");
  // A negative span is a clock skew, never "-5 s".
  assert.equal(formatDuration(-5_000), "<1 m");
});

test("formatGap rounds at the hour, because a gap is a distance", () => {
  assert.equal(formatGap(30_000), "<1 m");
  assert.equal(formatGap(45 * 60_000), "45 m");
  assert.equal(formatGap(6 * 3_600_000 + 7 * 60_000), "6 h");
  assert.equal(formatGap(2 * 86_400_000), "2 d");
  assert.equal(formatGap(2 * 86_400_000 + 3 * 3_600_000), "2 d 3 h");
});

test("formatCount pluralises the caller's noun and never hides a zero", () => {
  assert.equal(formatCount(0, "session"), "0 sessions");
  assert.equal(formatCount(1, "session"), "1 session");
  assert.equal(formatCount(3, "session"), "3 sessions");
  assert.equal(formatCount(11, "occurrence"), "11 occurrences");
});

test("the connected-threads panel has copy for all five states", () => {
  const states = ["loading", "empty", "error", "partial", "populated"];
  assert.deepEqual(Object.keys(CONNECTION_STATE_COPY).sort(), [...states].sort());

  for (const state of states) {
    assert.ok(
      CONNECTION_STATE_COPY[state].title.trim().length > 0,
      `${state} must carry a title`,
    );
  }

  assert.equal(CONNECTION_STATE_COPY.loading.title, "Connecting threads");
  assert.equal(CONNECTION_STATE_COPY.empty.title, "No connected threads yet");
  assert.match(CONNECTION_STATE_COPY.empty.detail, /share an entity/);
  assert.equal(CONNECTION_STATE_COPY.error.title, "Couldn't load connected threads");
  assert.match(CONNECTION_STATE_COPY.error.detail, /Retry/);
  assert.equal(CONNECTION_STATE_COPY.partial.title, "Some connections still clustering");
  assert.equal(CONNECTION_STATE_COPY.populated.title, "Connected threads");
});
