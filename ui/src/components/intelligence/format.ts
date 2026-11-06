// Coarse, locale-independent readings for the intelligence surfaces (threads,
// insights, ask scope). Three shapes only, because every figure the wire types
// carry is one of them:
//
//   formatDuration - attention spent ("3 h 20 m") from `active_ms` figures.
//   formatGap      - distance between two moments ("6 d") from `gap_ms` figures.
//   formatCount    - a pluralised noun ("3 sessions") from every `*_count`.
//
// Deliberately no `Intl` / `toLocaleString`: the same milliseconds must read the
// same in every host locale, and the unit tests pin the exact strings.
// `UI_REFERENCE §9` still owns prose dates ("Jun 22, 2026"); `lib/time.ts` renders
// those, this module never does.

const SECOND = 1_000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * How long something lasted, at the granularity a person reads it:
 * `"<1 m"` · `"45 m"` · `"3 h 20 m"` · `"2 d 4 h"`. Minutes are the floor, so
 * any span under a minute (including 0, and a negative span from a clock skew)
 * reads `"<1 m"` rather than `"0 m"`, and a whole hour drops the empty minute
 * part (`"1 h"`).
 */
export function formatDuration(ms: number): string {
  const total = Number.isFinite(ms) && ms > 0 ? ms : 0;
  if (total < MINUTE) return "<1 m";

  const minutes = Math.round(total / MINUTE);
  if (minutes < 60) return `${minutes} m`;

  if (total < DAY) {
    const hours = Math.floor(minutes / 60);
    const restMinutes = minutes % 60;
    return restMinutes === 0 ? `${hours} h` : `${hours} h ${restMinutes} m`;
  }

  const days = Math.floor(total / DAY);
  const restHours = Math.floor((total - days * DAY) / HOUR);
  return restHours === 0 ? `${days} d` : `${days} d ${restHours} h`;
}

/**
 * The distance between two moments (`RecurrencePayload.gap_ms`,
 * `ResumptionPayload.gap_ms`). Same coarse shapes as [`formatDuration`], rounded
 * at the hour instead of the minute: a gap is a distance between two events, not
 * a measurement of attention, so `"6 h"` reads better than `"6 h 7 m"`.
 */
export function formatGap(ms: number): string {
  const total = Number.isFinite(ms) && ms > 0 ? ms : 0;
  if (total < MINUTE) return "<1 m";
  if (total < HOUR) return `${Math.round(total / MINUTE)} m`;
  if (total < DAY) return `${Math.round(total / HOUR)} h`;

  const days = Math.floor(total / DAY);
  const restHours = Math.round((total - days * DAY) / HOUR);
  // A remainder that rounds up to a whole day must not read "2 d 24 h".
  if (restHours >= 24) return `${days + 1} d`;
  return restHours === 0 ? `${days} d` : `${days} d ${restHours} h`;
}

/**
 * `3 sessions` / `1 session`. Linear, never pluralised with `Intl.PluralRules`
 * (one English bundle ships), and the noun is the caller's so this helper never
 * invents a label (`0 sessions`, not `no sessions`: a count is a count).
 */
export function formatCount(count: number, singular: string): string {
  const whole = Number.isFinite(count) ? Math.max(0, Math.trunc(count)) : 0;
  return `${whole} ${whole === 1 ? singular : `${singular}s`}`;
}
