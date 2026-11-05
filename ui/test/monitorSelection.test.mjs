// Monitor selection (P7 review finding, PR #161): an empty `capture.monitors` means
// "every monitor", so the selection may only collapse to `[]` when it covers exactly
// the connected displays. A length-only comparison turned a click that *excludes* a
// display into one that captures an extra display.
import assert from "node:assert/strict";
import test from "node:test";

import { coversExactly, toggleMonitor } from "../src/routes/settings/monitorSelection.ts";

test("empty selection means all, so clicking a display excludes it", () => {
  assert.deepEqual(toggleMonitor([], 0, [0, 1, 2]), [1, 2]);
  assert.deepEqual(toggleMonitor([], 2, [0, 1, 2]), [0, 1]);
});

test("re-including every connected display collapses to the all-sentinel", () => {
  assert.deepEqual(toggleMonitor([0], 1, [0, 1]), []);
  assert.deepEqual(toggleMonitor([1], 0, [0, 1]), []);
});

test("a disconnected saved display is never silently turned into 'all'", () => {
  // Connected [0, 1], saved [2] (display 2 was unplugged). Clicking 0 gives two
  // entries — the same COUNT as the connected displays, but display 1 is not
  // selected, so collapsing to [] would start capturing display 1 as well.
  const next = toggleMonitor([2], 0, [0, 1]);
  assert.deepEqual(next, [0, 2]);
  assert.notDeepEqual(next, []);
});

test("excluding a connected display keeps the rest", () => {
  assert.deepEqual(toggleMonitor([0, 1, 2], 1, [0, 1, 2]), [0, 2]);
});

test("unknown displays stay in the selection until removed explicitly", () => {
  assert.deepEqual(toggleMonitor([3], 1, [0, 1]), [1, 3]);
  assert.deepEqual(
    toggleMonitor([0, 3], 3, [0, 1]),
    [0],
    "clicking the disconnected chip removes exactly it",
  );
});

test("coversExactly is a set comparison, not a count", () => {
  assert.equal(coversExactly([0, 1], [0, 1]), true);
  assert.equal(coversExactly([1, 0], [0, 1]), true);
  assert.equal(coversExactly([0, 2], [0, 1]), false, "same length, different member");
  assert.equal(coversExactly([0], [0, 1]), false);
  assert.equal(coversExactly([], [0, 1]), false);
  assert.equal(coversExactly([], []), false, "no connected displays cannot mean 'all'");
});
