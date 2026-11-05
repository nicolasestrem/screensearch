// Monitor selection (P7 review finding, PR #161): an empty `capture.monitors` means
// "every monitor", so the selection may only collapse to `[]` when it covers exactly
// the connected displays. A length-only comparison turned a click that *excludes* a
// display into one that captures an extra display.
import assert from "node:assert/strict";
import test from "node:test";

import {
  canToggleOff,
  coversExactly,
  removeMonitor,
  toggleMonitor,
} from "../src/routes/settings/monitorSelection.ts";

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

test("the last captured display cannot be turned off into 'all monitors'", () => {
  // `[]` means every monitor in `crates/capture/src/wgc.rs`, so deselecting display 0
  // while it is the only capture would START capturing the excluded display 1.
  assert.deepEqual(toggleMonitor([0], 0, [0, 1]), [0]);
  assert.deepEqual(toggleMonitor([1], 1, [0, 1]), [1]);
  assert.equal(canToggleOff([0], 0, [0, 1]), false);
  assert.equal(canToggleOff([0, 1], 0, [0, 1, 2]), true);
  // With one connected display, "all" is that display, so the click is a no-op anyway.
  assert.deepEqual(toggleMonitor([], 0, [0]), []);
  assert.equal(canToggleOff([], 0, [0]), false);
});

test("removing a disconnected display cannot empty the selection into 'all monitors'", () => {
  // The "not connected — remove it" control is the same hole: `[2]` with 2 disconnected
  // would become `[]`, i.e. capture every display that IS connected.
  assert.deepEqual(removeMonitor([2], 2, [0, 1]), [2]);
  assert.equal(canToggleOff([2], 2, [0, 1]), false);
  // Removing one of several keeps the rest, and an exact cover becomes "all".
  assert.deepEqual(removeMonitor([2, 3], 2, [0, 1]), [3]);
  assert.deepEqual(removeMonitor([2, 0, 1], 2, [0, 1]), []);
  // [2, 0] minus 2 leaves [0], which does not cover the connected [0, 1], so it stays
  // an explicit selection rather than silently becoming "all".
  assert.deepEqual(removeMonitor([2, 0], 2, [0, 1]), [0]);
});
