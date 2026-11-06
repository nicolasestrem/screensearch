// `localDateMidnight` (`ui/src/lib/time.ts`): the range Ask and the report picker build from a
// native date input. The half-open `[start, end)` end has to be the NEXT LOCAL MIDNIGHT, not
// `start + 24h`: a local day is 23h or 25h on the DST switch days, so adding a fixed day lands
// at 23:00 the same evening (dropping the day's last hour) or at 01:00 the next morning
// (stealing an hour from it), and the wrong bound then ships as an explicit retrieval override.
//
// The zone is pinned before anything else runs so the 23h/25h days are real on any machine; the
// expectations are then exact rather than "whatever this host's zone does".
/* global process -- the zone pin has to be set before the first Date is constructed */
process.env.TZ = "America/New_York";

import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);
const { localDateMidnight } = await import("../src/lib/time.ts");

const DAY_MS = 86_400_000;
const midnight = (y, m, d) => new Date(y, m - 1, d).getTime();

test("the zone pin is in effect, so the DST days below are 23h and 25h", () => {
  assert.equal(midnight(2026, 11, 2) - midnight(2026, 11, 1), 25 * 3_600_000);
  assert.equal(midnight(2026, 3, 9) - midnight(2026, 3, 8), 23 * 3_600_000);
});

test("the exclusive end of a range is the next local midnight across the fall-back day", () => {
  // 2026-11-01 in New York is 25 hours long; `+ DAY_MS` would end the range at 23:00 that day.
  const start = localDateMidnight("2026-11-01", 0);
  const end = localDateMidnight("2026-11-01", 1);
  assert.equal(start, midnight(2026, 11, 1));
  assert.equal(end, midnight(2026, 11, 2));
  assert.equal(end - start, 25 * 3_600_000);
  assert.notEqual(end, start + DAY_MS);
  const lastHour = new Date(end - 1);
  assert.equal(lastHour.getDate(), 1, "the picked day is included in full");
  assert.equal(lastHour.getHours(), 23);
});

test("the exclusive end is the next local midnight across the spring-forward day", () => {
  // 2026-03-08 is 23 hours long; `+ DAY_MS` would run an hour into March 9.
  const start = localDateMidnight("2026-03-08", 0);
  const end = localDateMidnight("2026-03-08", 1);
  assert.equal(end, midnight(2026, 3, 9));
  assert.equal(end - start, 23 * 3_600_000);
  assert.notEqual(end, start + DAY_MS);
  assert.equal(new Date(end - 1).getDate(), 8);
});

test("offsets roll the calendar month and year, not a millisecond count", () => {
  assert.equal(localDateMidnight("2026-12-31", 1), midnight(2027, 1, 1));
  assert.equal(localDateMidnight("2026-02-28", 1), midnight(2026, 3, 1));
  assert.equal(localDateMidnight("2028-02-28", 1), midnight(2028, 2, 29), "leap year");
});

test("empty and impossible dates are rejected instead of normalised", () => {
  assert.equal(localDateMidnight(""), null);
  assert.equal(localDateMidnight("2026-1-1"), null);
  assert.equal(localDateMidnight("not a date"), null);
  // `2026-02-31` would silently become March 3 if the round trip were not checked.
  assert.equal(localDateMidnight("2026-02-31", 0), null);
  assert.equal(localDateMidnight("2026-02-31", 1), null);
  assert.equal(localDateMidnight("2026-13-01"), null);
});
