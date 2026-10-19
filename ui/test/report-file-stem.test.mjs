// reportFileStem (`ui/src/lib/time.ts`): the local-time filename stem a saved recall
// report downloads under. Dual-kind coverage for `03 §8e` — a persisted run's durable
// `ReportRunKind` names the file too, and the scheduled `weekly_synthesis` must NEVER
// share the on-demand weekly report's `-weekly-…` stem (spec: "the on-demand report and
// the scheduled weekly synthesis never share a token, label or filename stem").
//
// The runner has no DOM framework, so this is a plain unit test over the pure function.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);
const { reportFileStem } = await import("../src/lib/time.ts");

// Fixed instant so the kind segment is the only variable under test; the date half is
// derived from the same local-clock getters `reportFileStem` uses, keeping the assertion
// timezone-independent while still proving the zero-padded local-time shape.
const MS = Date.UTC(2026, 5, 22, 12, 34);
const d = new Date(MS);
const pad = (n) => String(n).padStart(2, "0");
const STAMP = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`;

test("on-demand kinds keep their byte-identical 0.3.2 stems", () => {
  assert.equal(reportFileStem("daily", MS), `screensearch-report-daily-${STAMP}`);
  assert.equal(reportFileStem("weekly", MS), `screensearch-report-weekly-${STAMP}`);
  // A custom-range report (and an omitted kind) keeps the bare 0.3.1 D2 shape.
  assert.equal(reportFileStem("custom", MS), `screensearch-report-${STAMP}`);
  assert.equal(reportFileStem(undefined, MS), `screensearch-report-${STAMP}`);
});

test("a durable daily run reuses the on-demand daily stem (same concept)", () => {
  // `ReportRunKind` daily and `ReportKind` daily both describe a saved daily report, so
  // the file the user finds in Downloads is labelled the same way: `-daily-`, no
  // synthesis token.
  const stem = reportFileStem("daily", MS);
  assert.match(stem, /^screensearch-report-daily-\d{4}-\d{2}-\d{2}-\d{4}$/);
  assert.doesNotMatch(stem, /synthesis/);
});

test("a weekly_synthesis run carries its own -weekly-synthesis- token", () => {
  const stem = reportFileStem("weekly_synthesis", MS);
  assert.equal(stem, `screensearch-report-weekly-synthesis-${STAMP}`);
  assert.match(stem, /^screensearch-report-weekly-synthesis-\d{4}-\d{2}-\d{2}-\d{4}$/);
});

test("a weekly_synthesis stem is never the on-demand weekly stem", () => {
  const synthesis = reportFileStem("weekly_synthesis", MS);
  const onDemandWeekly = reportFileStem("weekly", MS);
  assert.notEqual(synthesis, onDemandWeekly);
  // Nor a bare date stem, nor an on-demand weekly stem with any date — the token itself
  // differs, not just the minute it was saved at.
  assert.notEqual(synthesis, reportFileStem(undefined, MS));
  assert.notEqual(synthesis, reportFileStem("custom", MS));
});
