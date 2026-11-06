// The window chip's one claim: it says "Last 7 days" only about a window that really is seven days
// ending today. The planner's rolling windows end at the *next* local midnight, while `this week`
// ends a week after it started — so a one-sided "not too old" test labels a Monday-to-next-Monday
// window "Last 7 days" on a Wednesday, and any custom range ending in the future with it.
//
// Asserted by classification ("does it say a rolling window?") rather than by rendering the dates,
// so the test does not depend on the runner's zone.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);

const { windowLabel } = await import("../src/lib/scopeWindow.ts");

const DAY = 86_400_000;
const HOUR = 3_600_000;
// A fixed "now" does not need to be a real weekday: the predicate compares against this number and
// the ranges below are built from it, which is exactly the arithmetic the planner produces.
const NOW = 1_800_000_000_000;
const midnight = (at) => Math.floor(at / DAY) * DAY;
const tomorrow = midnight(NOW) + DAY;

const rolling = (label) => label.startsWith("Last ");

test("a rolling window ending at tomorrow's local midnight reads as a rolling window", () => {
  // `last 7 days` on this day: start is six whole days before today, end is tomorrow 00:00.
  const range = { start: tomorrow - 7 * DAY, end: tomorrow };
  assert.equal(windowLabel(range, NOW), "Last 7 days");
  assert.equal(windowLabel({ start: tomorrow - DAY, end: tomorrow }, NOW), "Last 24 hours");
});

test("a window ending now still reads as a rolling window", () => {
  assert.equal(rolling(windowLabel({ start: NOW - 7 * DAY, end: NOW }, NOW)), true);
});

test("this week's Monday-to-next-Monday window is dated, not 'Last 7 days'", () => {
  // 7 whole days, end five days out — the shape `this week` has from Wednesday on.
  const range = { start: midnight(NOW) - 2 * DAY, end: midnight(NOW) + 5 * DAY };
  const label = windowLabel(range, NOW);
  assert.equal(rolling(label), false, `got ${label}`);
  assert.match(label, / – /);
});

test("a custom range ending in the future is dated, however whole its days", () => {
  const far = { start: midnight(NOW) + 30 * DAY, end: midnight(NOW) + 37 * DAY };
  assert.equal(rolling(windowLabel(far, NOW)), false);
});

test("an inherited window that ended long ago is dated, not a rolling window", () => {
  // A "last 7 days" window from yesterday: whole days, but the end is 14 hours behind now.
  const stale = { start: midnight(NOW) - 8 * DAY, end: midnight(NOW) };
  assert.equal(rolling(windowLabel(stale, NOW)), false);
});

test("a span that is not whole days is dated", () => {
  const ragged = { start: tomorrow - 3 * DAY - HOUR * 5, end: tomorrow };
  assert.equal(rolling(windowLabel(ragged, NOW)), false);
});
