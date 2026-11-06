// Scope chip labels, kept out of the component module so the label can be unit-tested and so the
// file exports only components (react-refresh).
import type { TimeRange } from "../bindings/TimeRange";
import { absoluteDate } from "./time";

const DAY = 86_400_000;
const HOUR = 3_600_000;

/**
 * "Last 7 days" for a window that ends now and spans whole days, else the dated
 * range: `Last 7 days` is only ever said about a window that really is seven days
 * ending today.
 *
 * "Closing today" is a two-sided test, and it has to be, because the planner's rolling windows end
 * at the *next* local midnight (`ask_plan.rs::last_days_window`: `last 3 days` on Wednesday runs
 * Monday 00:00 → Thursday 00:00) while `this week` ends a week after it started. Without the upper
 * bound, "this week" on a Wednesday and any custom range ending in the future satisfied
 * `end >= now - HOUR` and were labelled "Last 7 days" — a chip claiming a different period from the
 * one actually searched. The ceiling is one day plus DST slack, which admits next-midnight endings
 * and nothing further; anything else falls through to its dated range.
 */
export function windowLabel(range: TimeRange, now: number): string {
  const span = Math.max(0, range.end - range.start);
  const days = Math.round(span / DAY);
  const closesToday = range.end >= now - HOUR && range.end <= now + DAY + HOUR;
  if (days >= 1 && Math.abs(span - days * DAY) < HOUR && closesToday) {
    return days === 1 ? "Last 24 hours" : `Last ${days} days`;
  }
  return `${absoluteDate(range.start)} – ${absoluteDate(range.end)}`;
}
