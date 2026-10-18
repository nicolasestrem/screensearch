// A saved run's exported footer must date its window in the run's persisted IANA zone
// (`03 §8c`: the selected zone is persisted and a later machine-timezone change does not
// move it). `MemoryPanel` suppresses the on-screen footer for a saved report, so the date
// span ships only in the copied/downloaded markdown — a Tokyo daily window that reads as
// two UTC-local dates is a silent export defect. The tests below are deterministic on any
// machine because every expected label is built with an explicit zone; the negative
// assertions catch a dropped zone only where the browser zone differs from the run's,
// which is exactly when the defect is visible to a user.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);
const { buildReportFooter, savedRunFooterScope } = await import(
  "../src/lib/reportFooter.ts"
);
const { absoluteDate } = await import("../src/lib/time.ts");

const dateLabel = (ms, timeZone) =>
  new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    timeZone,
  }).format(new Date(ms));

/** A `YYYY-MM-DD` calendar date is the persisted truth; label it without relying on
 *  either the browser zone or the conversion under test. */
const calendarLabel = (isoDate) => dateLabel(Date.parse(`${isoDate}T00:00:00Z`), "UTC");

const isoDateIn = (ms, timeZone) => {
  const parts = new Intl.DateTimeFormat("en", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(new Date(ms));
  const part = (type) => parts.find((value) => value.type === type)?.value;
  return `${part("year")}-${part("month")}-${part("day")}`;
};

const TOKYO_DAY = {
  run_id: "run-tokyo-day",
  source_id: "source-1",
  kind: "daily",
  schedule_id: "schedule-1",
  revision_of: null,
  // 2026-10-18 00:00 → 2026-10-19 00:00 Asia/Tokyo (UTC+9, no DST).
  window_start: Date.UTC(2026, 9, 17, 15),
  window_end: Date.UTC(2026, 9, 18, 15),
  timezone: "Asia/Tokyo",
  local_date: "2026-10-18",
  phase: "finished",
  outcome: "OK",
  error: null,
  result: {
    markdown: "# Daily report — 2026-10-18\n\nA Tokyo day.\n",
    cited_frame_ids: [],
    periods_total: 1,
    periods_covered: 1,
    frames_sampled: 4,
    frames_summarized: 4,
    passes: 2,
    truncated: false,
    model: "qwen3-vl-4b.gguf",
  },
  config_snapshot: "{}",
  created_at: 0,
  updated_at: 0,
};

const reportResponse = (run) => ({
  markdown: run.result.markdown,
  cited_frame_ids: run.result.cited_frame_ids,
  range_label: run.local_date,
  periods_total: run.result.periods_total,
  periods_covered: run.result.periods_covered,
  frames_sampled: run.result.frames_sampled,
  frames_summarized: run.result.frames_summarized,
  passes: run.result.passes,
  truncated: run.result.truncated,
  model: run.result.model,
});

test("a saved Tokyo daily run exports its Tokyo-local date, not the UTC-local span", () => {
  const run = TOKYO_DAY;
  // The fixture really is one Tokyo-local day and agrees with the persisted local_date.
  assert.equal(isoDateIn(run.window_start, run.timezone), run.local_date);
  assert.equal(isoDateIn(run.window_end - 1, run.timezone), run.local_date);

  const footer = buildReportFooter(
    reportResponse(run),
    null,
    "0.4.1",
    savedRunFooterScope(run),
  );
  const expected = calendarLabel(run.local_date);
  assert.ok(footer, "a report with passes has a footer");
  assert.ok(
    footer.split(" · ").includes(expected),
    `the span should be ${expected}: ${footer}`,
  );
  // One covered day is one date — never a two-date span joined by an en dash.
  assert.doesNotMatch(footer, /–/, footer);

  // The dates this export would carry if the browser zone were used are the UTC ones.
  const utcStart = dateLabel(run.window_start, "UTC");
  const utcEnd = dateLabel(run.window_end - 1, "UTC");
  assert.notEqual(utcStart, expected);
  assert.ok(
    !footer.includes(`${utcStart} – ${utcEnd}`),
    `the UTC-local span leaked into the export: ${footer}`,
  );
});

test("a saved multi-day Tokyo range keeps both Tokyo-local dates", () => {
  const run = {
    ...TOKYO_DAY,
    run_id: "run-tokyo-range",
    // Strict local dates 2026-10-16…2026-10-18 inclusive, in the saved zone.
    requested_range: { start_date: "2026-10-16", end_date: "2026-10-18" },
    window_start: Date.UTC(2026, 9, 15, 15),
    window_end: Date.UTC(2026, 9, 18, 15),
    local_date: "2026-10-18",
  };
  assert.equal(isoDateIn(run.window_start, run.timezone), run.requested_range.start_date);
  assert.equal(
    isoDateIn(run.window_end - 1, run.timezone),
    run.requested_range.end_date,
  );

  const footer = buildReportFooter(
    reportResponse(run),
    null,
    "0.4.1",
    savedRunFooterScope(run),
  );
  const expected = `${calendarLabel(run.requested_range.start_date)} – ${calendarLabel(
    run.requested_range.end_date,
  )}`;
  assert.ok(footer.includes(expected), `expected ${expected}: ${footer}`);
  assert.ok(
    !footer.includes(`${dateLabel(run.window_start, "UTC")} – ${dateLabel(
      run.window_end - 1,
      "UTC",
    )}`),
    footer,
  );
});

test("a saved weekly span stays in the run's zone across a DST week", () => {
  // 2026-W43 in Europe/Paris: Monday 2026-10-19 00:00 CEST (UTC+2) → Monday
  // 2026-10-26 00:00 CET (UTC+1). The week is 169 hours, so the UTC window's first
  // instant falls on the previous UTC date.
  const run = {
    ...TOKYO_DAY,
    run_id: "run-paris-week",
    kind: "weekly_synthesis",
    timezone: "Europe/Paris",
    local_date: "2026-10-19",
    window_start: Date.UTC(2026, 9, 18, 22),
    window_end: Date.UTC(2026, 9, 25, 23),
  };
  assert.equal(isoDateIn(run.window_start, run.timezone), "2026-10-19");
  assert.equal(isoDateIn(run.window_end - 1, run.timezone), "2026-10-25");

  const footer = buildReportFooter(
    reportResponse(run),
    null,
    "0.4.1",
    savedRunFooterScope(run),
  );
  assert.ok(
    footer.includes(`${calendarLabel("2026-10-19")} – ${calendarLabel("2026-10-25")}`),
    footer,
  );
  assert.match(footer, /weekly synthesis/);
});

test("on-demand and session footers keep the browser-local span", () => {
  const start = TOKYO_DAY.window_start;
  const end = TOKYO_DAY.window_end;
  const browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const expected = `${dateLabel(start, browserZone)} – ${dateLabel(
    end - 1,
    browserZone,
  )}`;

  // A recall request carries no persisted run zone, so nothing changes there.
  const requestFooter = buildReportFooter(
    reportResponse(TOKYO_DAY),
    { kind: "custom", time_range: { start, end }, prompt: "ship notes" },
    "0.4.1",
  );
  assert.ok(requestFooter.includes(expected), requestFooter);
  assert.match(requestFooter, /custom \(focus: ship notes\)/);

  // Session recap passes a scope with no saved zone: the same local-time behaviour.
  const sessionFooter = buildReportFooter(
    reportResponse(TOKYO_DAY),
    null,
    "0.4.1",
    { timeRange: { start, end }, activeFilter: "session id: s-1" },
  );
  assert.ok(sessionFooter.includes(expected), sessionFooter);
  assert.match(sessionFooter, /session id: s-1/);
});

test("savedRunFooterScope carries the run's persisted zone and window", () => {
  const daily = savedRunFooterScope(TOKYO_DAY);
  assert.deepEqual(daily.timeRange, {
    start: TOKYO_DAY.window_start,
    end: TOKYO_DAY.window_end,
  });
  assert.equal(daily.timeZone, "Asia/Tokyo");
  assert.equal(daily.activeFilter, "daily report");

  const weekly = savedRunFooterScope({ ...TOKYO_DAY, kind: "weekly_synthesis" });
  assert.equal(weekly.timeZone, "Asia/Tokyo");
  assert.equal(weekly.activeFilter, "weekly synthesis");

  // The formatter: an explicit zone dates the instant there; omitting it stays
  // browser-local for every existing caller.
  assert.equal(
    absoluteDate(TOKYO_DAY.window_start, "Asia/Tokyo"),
    calendarLabel("2026-10-18"),
  );
  assert.equal(
    absoluteDate(TOKYO_DAY.window_start),
    dateLabel(
      TOKYO_DAY.window_start,
      Intl.DateTimeFormat().resolvedOptions().timeZone,
    ),
  );
});
