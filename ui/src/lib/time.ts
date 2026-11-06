// Human-relative timestamps with an absolute form for hover/title (UI_REFERENCE
// §9: "Timestamps human-relative with absolute on hover"). Every input is unix
// epoch milliseconds — the `captured_at` wire format. These run only inside the
// WebView, so the platform `Date` / `Intl` APIs are always available.

import type { ReportKind } from "../bindings/ReportKind";
import type { ReportRunKind } from "../bindings/ReportRunKind";

const SECOND = 1_000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

// Built once at module load (constructing an Intl formatter per call is costly).
// 24-hour clock for the instrument-readout feel; locale otherwise honored.
const dateTimeFmt = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});
const dateFmt = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "short",
  day: "numeric",
});

// One date formatter per explicitly requested zone, built on demand. A saved report's
// window belongs to the zone persisted with the run (`03 §8c`: a later machine-timezone
// change does not move it), so its footer must not be dated in whatever zone the
// computer is in now.
const zonedDateFmts = new Map<string, Intl.DateTimeFormat>();

function dateFmtFor(timeZone?: string): Intl.DateTimeFormat {
  if (!timeZone) return dateFmt;
  const cached = zonedDateFmts.get(timeZone);
  if (cached) return cached;
  const fmt = new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    timeZone,
  });
  zonedDateFmts.set(timeZone, fmt);
  return fmt;
}

const clockFmt = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

/**
 * A short, human-relative label: "just now" · "5m ago" · "3h ago" · "yesterday" ·
 * "4d ago", falling back to an absolute date for anything older than a week. A
 * future timestamp (clock skew) reads "just now" rather than a negative span.
 */
export function relativeTime(ms: number, now: number = Date.now()): string {
  const diff = now - ms;
  if (diff < 45 * SECOND) return "just now";
  if (diff < 90 * SECOND) return "1m ago";
  if (diff < HOUR) return `${Math.round(diff / MINUTE)}m ago`;
  if (diff < 2 * HOUR) return "1h ago";
  if (diff < DAY) return `${Math.round(diff / HOUR)}h ago`;
  if (diff < 2 * DAY) return "yesterday";
  if (diff < 7 * DAY) return `${Math.round(diff / DAY)}d ago`;
  return absoluteDate(ms);
}

/** Full date + time, e.g. "Jun 22, 2026, 14:32" — the hover/title form. */
export function absoluteTime(ms: number): string {
  return dateTimeFmt.format(new Date(ms));
}

/**
 * Date only, e.g. "Jun 22, 2026". `timeZone` (an IANA identifier) dates the instant in
 * that zone instead of the browser's; omitting it keeps the local-time behaviour every
 * non-saved-report caller relies on.
 */
export function absoluteDate(ms: number, timeZone?: string): string {
  return dateFmtFor(timeZone).format(new Date(ms));
}

/** Clock time only, e.g. "14:32" — compact timeline / tile labels (mono). */
export function clockTime(ms: number): string {
  return clockFmt.format(new Date(ms));
}

/**
 * Local-time filename stem for a saved recall report:
 * `screensearch-report-YYYY-MM-DD-HHmm` (0.3.1 D2, #65). Daily and weekly reports carry
 * their kind in the stem — `screensearch-report-daily-…` / `-weekly-…` (#89) — so a
 * Downloads folder full of reports stays tellable-apart; a custom-range report keeps the
 * bare 0.3.1 D2 shape.
 *
 * A persisted run's durable kind (`ReportRunKind`, `03 §8e`) is accepted too: the
 * scheduled daily run takes the same `-daily-…` segment as the on-demand daily report
 * (same concept), while the scheduled `weekly_synthesis` takes `-weekly-synthesis-…` —
 * deliberately never the on-demand `ReportKind::Weekly` report's `-weekly-…` stem, so a
 * saved synthesis and an on-demand weekly report never collide in a Downloads folder.
 *
 * Uses the running clock (a report is saved "now", not at a captured instant) with
 * zero-padded 24-hour local time. The backend appends the `.md` extension and de-collides
 * same-minute names (`-2`, `-3`, …).
 */
export function reportFileStem(
  kind?: ReportKind | ReportRunKind,
  ms: number = Date.now(),
): string {
  const d = new Date(ms);
  const y = d.getFullYear();
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const h = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  // `weekly_synthesis` is its own token — never the on-demand weekly report's `-weekly-`
  // (`03 §8e`: the two never share a filename stem). Durable `daily` reuses `-daily-`.
  const kindSegment =
    kind === "weekly_synthesis"
      ? "weekly-synthesis-"
      : kind === "daily" || kind === "weekly"
        ? `${kind}-`
        : "";
  return `screensearch-report-${kindSegment}${y}-${mo}-${day}-${h}${mi}`;
}

/**
 * Local midnight of a `<input type="date">` value (`YYYY-MM-DD`), `offsetDays` calendar days
 * from it, or `null` when the value is empty or not a real day.
 *
 * Built from calendar components rather than arithmetic on milliseconds, because a local day is
 * not always 86,400,000 ms: on the DST switch days it is 23h or 25h, so `date + DAY_MS` lands at
 * 23:00 the same evening (losing the last hour of the day) or at 01:00 the next morning
 * (stealing an hour from it). The `Date` constructor normalises day overflow, so `offsetDays = 1`
 * rolls to the next calendar day in any zone — which is what a half-open `[start, end)` range
 * needs for its exclusive end (`03 §7f`; `ReportBuilder` documents the same trap).
 *
 * The round-trip check keeps `2026-02-31` (which `Date` would silently normalise to March 3) out,
 * so a typo cannot quietly widen a range.
 */
export function localDateMidnight(value: string, offsetDays = 0): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const base = new Date(year, month - 1, day);
  if (
    base.getFullYear() !== year ||
    base.getMonth() !== month - 1 ||
    base.getDate() !== day
  ) {
    return null;
  }
  if (offsetDays === 0) return base.getTime();
  return new Date(year, month - 1, day + offsetDays).getTime();
}
