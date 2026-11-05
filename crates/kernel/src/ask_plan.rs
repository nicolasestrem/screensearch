//! Pure Ask v2 time-intent planner (`03 §7i.1`, 0.4.5 P6).
//!
//! One job: turn a question's calendar phrasing into the half-open unix-ms window the
//! retrieval stages narrow on, in the caller-supplied local timezone. The planner is pure
//! and injectable — `now_ms` plus a [`chrono::TimeZone`] — so unit tests pin both while the
//! production path passes the system local zone ([`chrono::Local`]). Nothing is frozen to
//! an offset: every bound is resolved through the zone, so a DST transition inside a
//! window (or between "now" and a past date) still produces correct local midnights.
//!
//! Two rules are normative:
//!
//! - A phrase with no resolvable anchor (`before the release`, `recently`) yields **no**
//!   window: the planner never fabricates bounds. `None` means "the whole history", or, on
//!   a follow-up, "the previous turn's window unchanged" (`03 §7i.1`, `§7i.4`).
//! - Follow-up phrasing (`the day before`, `the week before`) moves the inherited window
//!   instead of replacing it (`03 §7i.4`).
//!
//! Calendar decisions fixed by the maintainer (P6 kickoff, decision 6): ISO weeks start on
//! Monday, `this morning` is `[06:00,12:00)` local, `this afternoon` is `[12:00,18:00)`
//! local, bare weekday names mean the most recent such day (today included), `last N days`
//! is N calendar days ending today, and an explicit `YYYY-MM-DD` is that local day.

use chrono::{
    DateTime, Datelike, Days, LocalResult, Months, NaiveDate, NaiveDateTime, TimeZone, Utc, Weekday,
};
use traits::TimeRange;

/// Follow-up phrasing that moves an inherited window instead of replacing it (`03 §7i.4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shift {
    /// "the day before" — one calendar day earlier.
    Day,
    /// "the week before" — one ISO week earlier.
    Week,
}

impl Shift {
    fn days(self) -> u64 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
        }
    }
}

/// What one question asks for, before inheritance is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Intent {
    /// An absolute calendar phrase, already resolved against `now`.
    Window(TimeRange),
    /// A follow-up phrase: move the previous turn's window back.
    ShiftPrevious(Shift),
    /// No resolvable anchor — never an error, never a fabricated window.
    Unresolved,
}

/// The window one turn runs with: the question's own intent wins, else a follow-up shift
/// moves `previous`, else the previous window is inherited unchanged (and with no previous
/// turn the window is unbounded).
pub(crate) fn plan_turn_window<Tz: TimeZone>(
    question: &str,
    now_ms: i64,
    zone: &Tz,
    previous: Option<TimeRange>,
) -> Option<TimeRange> {
    match classify(question, now_ms, zone) {
        Intent::Window(window) => Some(window),
        Intent::ShiftPrevious(shift) => {
            previous.and_then(|window| shift_window(window, shift, zone))
        }
        Intent::Unresolved => previous,
    }
}

/// Classifies one question against `now` in `zone`. Pure; the only fallible part is the
/// calendar arithmetic itself (a timestamp outside the supported range yields no intent).
fn classify<Tz: TimeZone>(question: &str, now_ms: i64, zone: &Tz) -> Intent {
    let Some(now) = DateTime::<Utc>::from_timestamp_millis(now_ms) else {
        return Intent::Unresolved;
    };
    let today = now.with_timezone(zone).date_naive();
    let words = words(question);

    // An explicit date is the least ambiguous anchor there is, so it wins outright.
    if let Some(date) = find_iso_date(question) {
        return resolved(next_day(date).and_then(|end| range_window(date, end, zone)));
    }
    // "the day before yesterday" contains "yesterday"; the longer phrase must win or the
    // question silently resolves to the wrong day.
    if contains_words(&words, &["day", "before", "yesterday"]) {
        return resolved(prev_day(today, 2).and_then(|date| day_window(date, zone)));
    }
    if contains_words(&words, &["this", "morning"]) {
        return resolved(span_window(today, 6, 12, zone));
    }
    if contains_words(&words, &["this", "afternoon"]) {
        return resolved(span_window(today, 12, 18, zone));
    }
    if contains_words(&words, &["yesterday"]) {
        return resolved(prev_day(today, 1).and_then(|date| day_window(date, zone)));
    }
    if contains_words(&words, &["today"]) {
        return resolved(day_window(today, zone));
    }
    if contains_words(&words, &["this", "week"]) {
        return resolved(week_window(today, 0, zone));
    }
    if contains_words(&words, &["last", "week"]) {
        return resolved(week_window(today, 1, zone));
    }
    if contains_words(&words, &["this", "month"]) {
        return resolved(month_window(today, 0, zone));
    }
    if contains_words(&words, &["last", "month"]) {
        return resolved(month_window(today, 1, zone));
    }
    if let Some(days) = last_days(&words) {
        return resolved(last_days_window(today, days, zone));
    }
    if let Some(weekday) = weekday_word(&words) {
        return resolved(most_recent_day(today, weekday).and_then(|date| day_window(date, zone)));
    }
    // Follow-up phrasing last: it is only meaningful with an inherited window, and the
    // caller decides what "no previous turn" means (no window at all).
    if contains_words(&words, &["week", "before"]) {
        return Intent::ShiftPrevious(Shift::Week);
    }
    if contains_words(&words, &["day", "before"]) {
        return Intent::ShiftPrevious(Shift::Day);
    }
    Intent::Unresolved
}

fn resolved(window: Option<TimeRange>) -> Intent {
    window.map_or(Intent::Unresolved, Intent::Window)
}

/// Lowercase word tokens; any non-alphanumeric character separates words, so `today's`
/// still contains `today`.
fn words(question: &str) -> Vec<String> {
    question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether `phrase` occurs as a contiguous run of whole words in `words`.
fn contains_words(words: &[String], phrase: &[&str]) -> bool {
    !phrase.is_empty()
        && words
            .windows(phrase.len())
            .any(|window| window.iter().zip(phrase).all(|(word, part)| word == part))
}

/// A canonical `YYYY-MM-DD` anywhere in the question. Digit-bounded so a longer number
/// (`12026-11-03`) cannot masquerade as a date, and validated by `NaiveDate`'s parser.
fn find_iso_date(question: &str) -> Option<NaiveDate> {
    let bytes = question.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    for start in 0..=bytes.len() - 10 {
        let candidate = &bytes[start..start + 10];
        if !iso_shape(candidate) {
            continue;
        }
        let bounded_before = start == 0 || !bytes[start - 1].is_ascii_digit();
        let bounded_after = start + 10 == bytes.len() || !bytes[start + 10].is_ascii_digit();
        if !(bounded_before && bounded_after) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(candidate) else {
            continue;
        };
        if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
            return Some(date);
        }
    }
    None
}

fn iso_shape(candidate: &[u8]) -> bool {
    candidate.len() == 10
        && candidate[..4].iter().all(u8::is_ascii_digit)
        && candidate[4] == b'-'
        && candidate[5..7].iter().all(u8::is_ascii_digit)
        && candidate[7] == b'-'
        && candidate[8..].iter().all(u8::is_ascii_digit)
}

/// `last <n> days` → `n`, bounded to a sane range (0 is not a window; a century is).
fn last_days(words: &[String]) -> Option<u64> {
    words.windows(3).find_map(|window| {
        if window[0] != "last" || window[2] != "days" {
            return None;
        }
        let days = window[1].parse::<u64>().ok()?;
        (1..=36_600).contains(&days).then_some(days)
    })
}

fn weekday_word(words: &[String]) -> Option<Weekday> {
    words.iter().find_map(|word| match word.as_str() {
        "monday" => Some(Weekday::Mon),
        "tuesday" => Some(Weekday::Tue),
        "wednesday" => Some(Weekday::Wed),
        "thursday" => Some(Weekday::Thu),
        "friday" => Some(Weekday::Fri),
        "saturday" => Some(Weekday::Sat),
        "sunday" => Some(Weekday::Sun),
        _ => None,
    })
}

fn prev_day(date: NaiveDate, days: u64) -> Option<NaiveDate> {
    date.checked_sub_days(Days::new(days))
}

fn next_day(date: NaiveDate) -> Option<NaiveDate> {
    date.checked_add_days(Days::new(1))
}

/// The most recent `weekday` at or before `date` (today included).
fn most_recent_day(date: NaiveDate, weekday: Weekday) -> Option<NaiveDate> {
    let back = (date.weekday().num_days_from_monday() + 7 - weekday.num_days_from_monday()) % 7;
    prev_day(date, u64::from(back))
}

fn day_window(date: NaiveDate, zone: &impl TimeZone) -> Option<TimeRange> {
    range_window(date, next_day(date)?, zone)
}

/// `[06:00, 12:00)` (or any other fixed local hour pair) of one local day.
fn span_window(
    date: NaiveDate,
    from_hour: u32,
    to_hour: u32,
    zone: &impl TimeZone,
) -> Option<TimeRange> {
    Some(TimeRange {
        start: local_ms(date.and_hms_opt(from_hour, 0, 0)?, zone)?,
        end: local_ms(date.and_hms_opt(to_hour, 0, 0)?, zone)?,
    })
}

/// One ISO week starting on the Monday `weeks_back` weeks before `date`'s week.
fn week_window(date: NaiveDate, weeks_back: u64, zone: &impl TimeZone) -> Option<TimeRange> {
    let monday = prev_day(date, u64::from(date.weekday().num_days_from_monday()))?;
    let start = prev_day(monday, weeks_back * 7)?;
    range_window(start, start.checked_add_days(Days::new(7))?, zone)
}

/// One calendar month: the month `months_back` months before `date`'s month.
fn month_window(date: NaiveDate, months_back: u32, zone: &impl TimeZone) -> Option<TimeRange> {
    let first = date.with_day(1)?;
    let start = if months_back == 0 {
        first
    } else {
        first.checked_sub_months(Months::new(months_back))?
    };
    range_window(start, start.checked_add_months(Months::new(1))?, zone)
}

/// `n` calendar days ending today, inclusive.
fn last_days_window(date: NaiveDate, days: u64, zone: &impl TimeZone) -> Option<TimeRange> {
    let start = prev_day(date, days - 1)?;
    range_window(start, next_day(date)?, zone)
}

/// Half-open local-day range between two civil dates.
fn range_window(start: NaiveDate, end: NaiveDate, zone: &impl TimeZone) -> Option<TimeRange> {
    Some(TimeRange {
        start: local_ms(start.and_hms_opt(0, 0, 0)?, zone)?,
        end: local_ms(end.and_hms_opt(0, 0, 0)?, zone)?,
    })
}

fn shift_window(window: TimeRange, shift: Shift, zone: &impl TimeZone) -> Option<TimeRange> {
    Some(TimeRange {
        start: shift_bound(window.start, shift, zone)?,
        end: shift_bound(window.end, shift, zone)?,
    })
}

/// Moves one bound back by whole calendar days, preserving its local wall-clock time, and
/// re-resolves it through `zone` so a DST change between the two days is honored.
fn shift_bound(ms: i64, shift: Shift, zone: &impl TimeZone) -> Option<i64> {
    let local = DateTime::<Utc>::from_timestamp_millis(ms)?.with_timezone(zone);
    let wall = local
        .naive_local()
        .checked_sub_days(Days::new(shift.days()))?;
    local_ms(wall, zone)
}

/// Resolves a local wall-clock time to unix ms. A nonexistent local time (a DST gap, or a
/// whole skipped civil date) advances minute by minute; an ambiguous one (a DST overlap)
/// takes the earlier instant, matching `crate::memory_schedule`'s rule.
fn local_ms(wall: NaiveDateTime, zone: &impl TimeZone) -> Option<i64> {
    let mut wall = wall;
    for _ in 0..=2880 {
        match zone.from_local_datetime(&wall) {
            LocalResult::Single(at) => return Some(at.timestamp_millis()),
            LocalResult::Ambiguous(first, second) => {
                return Some(first.min(second).timestamp_millis())
            }
            LocalResult::None => {
                wall = wall.checked_add_signed(chrono::Duration::minutes(1))?;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Tz;

    /// A fixed zone for every test: CET/CEST, so DST behavior is exercised for real.
    const PARIS: Tz = chrono_tz::Europe::Paris;

    fn at(zone: &Tz, year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        zone.with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("unambiguous local time")
            .timestamp_millis()
    }

    /// `now` = a Wednesday, 10:00 local.
    fn now() -> i64 {
        at(&PARIS, 2026, 3, 4, 10, 0)
    }

    fn local_day(year: i32, month: u32, day: u32) -> TimeRange {
        let date = NaiveDate::from_ymd_opt(year, month, day).expect("valid date");
        let next = date.checked_add_days(Days::new(1)).expect("next day");
        TimeRange {
            start: at(&PARIS, year, month, day, 0, 0),
            end: at(&PARIS, next.year(), next.month(), next.day(), 0, 0),
        }
    }

    fn window(question: &str, now_ms: i64) -> Option<TimeRange> {
        plan_turn_window(question, now_ms, &PARIS, None)
    }

    #[test]
    fn today_and_yesterday_are_local_calendar_days() {
        assert_eq!(
            window("what did I do today?", now()),
            Some(local_day(2026, 3, 4))
        );
        assert_eq!(
            window("summarize yesterday", now()),
            Some(local_day(2026, 3, 3))
        );
        // The half-open window is exactly one local day long in the same offset month.
        let yesterday = window("yesterday", now()).expect("yesterday resolves");
        assert_eq!(yesterday.end - yesterday.start, 24 * 60 * 60 * 1000);
    }

    #[test]
    fn morning_and_afternoon_are_fixed_local_hours() {
        let morning = window("what was I reading this morning?", now()).expect("morning");
        assert_eq!(morning.start, at(&PARIS, 2026, 3, 4, 6, 0));
        assert_eq!(morning.end, at(&PARIS, 2026, 3, 4, 12, 0));
        let afternoon = window("this afternoon", now()).expect("afternoon");
        assert_eq!(afternoon.start, at(&PARIS, 2026, 3, 4, 12, 0));
        assert_eq!(afternoon.end, at(&PARIS, 2026, 3, 4, 18, 0));
    }

    #[test]
    fn this_and_last_week_are_iso_weeks_from_monday() {
        // Wednesday 2026-03-04 → this week is Monday 03-02 .. Monday 03-09.
        let this_week = window("what happened this week?", now()).expect("this week");
        assert_eq!(this_week.start, at(&PARIS, 2026, 3, 2, 0, 0));
        assert_eq!(this_week.end, at(&PARIS, 2026, 3, 9, 0, 0));
        let last_week = window("last week", now()).expect("last week");
        assert_eq!(last_week.start, at(&PARIS, 2026, 2, 23, 0, 0));
        assert_eq!(last_week.end, at(&PARIS, 2026, 3, 2, 0, 0));
        // A Monday is its own week's first day.
        let monday_now = at(&PARIS, 2026, 3, 2, 9, 0);
        let monday_week = window("this week", monday_now).expect("this week");
        assert_eq!(monday_week.start, at(&PARIS, 2026, 3, 2, 0, 0));
    }

    #[test]
    fn this_and_last_month_are_calendar_months() {
        let this_month = window("this month", now()).expect("this month");
        assert_eq!(this_month.start, at(&PARIS, 2026, 3, 1, 0, 0));
        assert_eq!(this_month.end, at(&PARIS, 2026, 4, 1, 0, 0));
        let last_month = window("what did I ship last month?", now()).expect("last month");
        assert_eq!(last_month.start, at(&PARIS, 2026, 2, 1, 0, 0));
        assert_eq!(last_month.end, at(&PARIS, 2026, 3, 1, 0, 0));
    }

    #[test]
    fn month_end_last_month_is_the_previous_calendar_month() {
        // 31 March: the previous month is February, not "31 February" and not a month
        // shortened to 28 days in the wrong place.
        let end_of_march = at(&PARIS, 2026, 3, 31, 23, 30);
        let last_month = window("last month", end_of_march).expect("last month");
        assert_eq!(last_month.start, at(&PARIS, 2026, 2, 1, 0, 0));
        assert_eq!(last_month.end, at(&PARIS, 2026, 3, 1, 0, 0));
        // 31 January → December of the previous year.
        let end_of_january = at(&PARIS, 2026, 1, 31, 12, 0);
        let december = window("last month", end_of_january).expect("last month");
        assert_eq!(december.start, at(&PARIS, 2025, 12, 1, 0, 0));
        assert_eq!(december.end, at(&PARIS, 2026, 1, 1, 0, 0));
    }

    #[test]
    fn last_n_days_counts_calendar_days_ending_today() {
        let three = window("what did I do last 3 days?", now()).expect("last 3 days");
        assert_eq!(three.start, at(&PARIS, 2026, 3, 2, 0, 0));
        assert_eq!(three.end, at(&PARIS, 2026, 3, 5, 0, 0));
        // "last 1 days" is today, not an empty window.
        assert_eq!(window("last 1 days", now()), Some(local_day(2026, 3, 4)));
        // A bare "last days" has no anchor.
        assert_eq!(window("last days", now()), None);
    }

    #[test]
    fn bare_weekday_is_the_most_recent_such_day_today_included() {
        // Wednesday 2026-03-04.
        assert_eq!(
            window("what did I do on monday", now()),
            Some(local_day(2026, 3, 2))
        );
        assert_eq!(
            window("wednesday", now()),
            Some(local_day(2026, 3, 4)),
            "today is included"
        );
        // Sunday of the current week is in the future, so the most recent one is 03-01.
        assert_eq!(window("sunday", now()), Some(local_day(2026, 3, 1)));
        assert_eq!(window("friday", now()), Some(local_day(2026, 2, 27)));
    }

    #[test]
    fn iso_date_resolves_to_that_local_day() {
        assert_eq!(
            window("what happened on 2026-02-27?", now()),
            Some(local_day(2026, 2, 27))
        );
        assert_eq!(window("2026-03-04", now()), Some(local_day(2026, 3, 4)));
        // A date-looking run inside a longer number is not a date.
        assert_eq!(window("build 12026-02-27", now()), None);
        // An impossible date falls through to the other phrases (here: none).
        assert_eq!(window("2026-02-31", now()), None);
    }

    #[test]
    fn day_before_yesterday_wins_over_yesterday() {
        assert_eq!(
            window("the day before yesterday", now()),
            Some(local_day(2026, 3, 2))
        );
    }

    #[test]
    fn unresolvable_anchor_yields_no_window() {
        assert_eq!(window("what changed before the release?", now()), None);
        assert_eq!(window("recently", now()), None);
        assert_eq!(window("tell me about the deployment", now()), None);
        // With an inherited window, "no anchor" means "keep the previous window".
        let previous = local_day(2026, 3, 3);
        assert_eq!(
            plan_turn_window("and what else?", now(), &PARIS, Some(previous)),
            Some(previous)
        );
        // …while an explicit anchor overrides it.
        assert_eq!(
            plan_turn_window("what about today?", now(), &PARIS, Some(previous)),
            Some(local_day(2026, 3, 4))
        );
    }

    #[test]
    fn follow_up_phrases_shift_the_inherited_window() {
        let day = local_day(2026, 3, 3);
        assert_eq!(
            plan_turn_window("and the day before?", now(), &PARIS, Some(day)),
            Some(local_day(2026, 3, 2))
        );
        let week = window("this week", now()).expect("this week");
        assert_eq!(
            plan_turn_window("what about the week before?", now(), &PARIS, Some(week)),
            Some(TimeRange {
                start: at(&PARIS, 2026, 2, 23, 0, 0),
                end: at(&PARIS, 2026, 3, 2, 0, 0),
            })
        );
        // A shift with nothing to shift yields no window rather than a guess.
        assert_eq!(
            plan_turn_window("the day before?", now(), &PARIS, None),
            None
        );
        // A shift preserves a non-midnight window's local shape ("this morning" → the
        // morning before, same hours).
        let morning = window("this morning", now()).expect("morning");
        assert_eq!(
            plan_turn_window("the day before", now(), &PARIS, Some(morning)),
            Some(TimeRange {
                start: at(&PARIS, 2026, 3, 3, 6, 0),
                end: at(&PARIS, 2026, 3, 3, 12, 0),
            })
        );
    }

    #[test]
    fn dst_boundary_day_is_the_real_local_day() {
        // Europe/Paris springs forward at 02:00 on 2026-03-29: that local day is 23 hours.
        let dst_day = at(&PARIS, 2026, 3, 29, 12, 0);
        let today = window("today", dst_day).expect("today");
        assert_eq!(today.start, at(&PARIS, 2026, 3, 29, 0, 0));
        assert_eq!(today.end, at(&PARIS, 2026, 3, 30, 0, 0));
        assert_eq!(today.end - today.start, 23 * 60 * 60 * 1000);

        // The day after is 24 hours again, and "yesterday" on the DST day is the 28th.
        assert_eq!(window("yesterday", dst_day), Some(local_day(2026, 3, 28)));
        let after = window("today", at(&PARIS, 2026, 3, 30, 9, 0)).expect("today");
        assert_eq!(after.end - after.start, 24 * 60 * 60 * 1000);

        // A window that spans the transition keeps both local midnights.
        let spanning = window("last 2 days", dst_day).expect("last 2 days");
        assert_eq!(spanning.start, at(&PARIS, 2026, 3, 28, 0, 0));
        assert_eq!(spanning.end, at(&PARIS, 2026, 3, 30, 0, 0));
        assert_eq!(spanning.end - spanning.start, 47 * 60 * 60 * 1000);

        // Shifting across the transition also lands on the local midnight, not +-1 h.
        assert_eq!(
            plan_turn_window(
                "the day before",
                dst_day,
                &PARIS,
                Some(local_day(2026, 3, 29))
            ),
            Some(local_day(2026, 3, 28))
        );
    }

    #[test]
    fn autumn_transition_day_is_25_hours_long() {
        // Europe/Paris falls back at 03:00 on 2026-10-25; that local day is 25 hours.
        let today = window("today", at(&PARIS, 2026, 10, 25, 12, 0)).expect("today");
        assert_eq!(today.start, at(&PARIS, 2026, 10, 25, 0, 0));
        assert_eq!(today.end, at(&PARIS, 2026, 10, 26, 0, 0));
        assert_eq!(today.end - today.start, 25 * 60 * 60 * 1000);
    }
}
