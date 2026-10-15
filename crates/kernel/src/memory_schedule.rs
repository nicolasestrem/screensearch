//! Pure calendar scheduling: fixed IANA zones, explicit UTC windows, no wall-clock side effects.
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Days, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyWindow {
    pub start: i64,
    pub end: i64,
    pub local_date: String,
    pub due_at: i64,
}

#[derive(Debug, Default)]
pub struct ScheduleDue {
    pub windows: Vec<DailyWindow>,
    pub gap: Option<(i64, i64)>,
}

pub fn validate(timezone: &str, local_time: &str) -> Result<(Tz, NaiveTime)> {
    let zone = timezone.parse::<Tz>().context("unknown IANA time zone")?;
    let time = NaiveTime::parse_from_str(local_time, "%H:%M").context("time must be HH:mm")?;
    if time.format("%H:%M").to_string() != local_time {
        bail!("time must be HH:mm");
    }
    Ok((zone, time))
}

fn utc(ms: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms).context("timestamp outside supported calendar")
}

// A whole civil date can be skipped (e.g. Pacific/Apia); search up to two days.
fn resolve(zone: Tz, mut wall: NaiveDateTime) -> Result<i64> {
    for _ in 0..=2880 {
        match zone.from_local_datetime(&wall) {
            LocalResult::Single(t) => return Ok(t.timestamp_millis()),
            LocalResult::Ambiguous(a, b) => return Ok(a.min(b).timestamp_millis()),
            LocalResult::None => {
                wall = wall
                    .checked_add_signed(chrono::Duration::minutes(1))
                    .context("calendar overflow")?
            }
        }
    }
    bail!("local time cannot be resolved")
}

fn next(date: NaiveDate) -> Result<NaiveDate> {
    date.checked_add_days(Days::new(1))
        .context("calendar overflow")
}

fn window(zone: Tz, time: NaiveTime, date: NaiveDate) -> Result<DailyWindow> {
    let tomorrow = next(date)?;
    Ok(DailyWindow {
        start: resolve(zone, date.and_time(NaiveTime::MIN))?,
        end: resolve(zone, tomorrow.and_time(NaiveTime::MIN))?,
        due_at: resolve(zone, tomorrow.and_time(time))?,
        local_date: date.to_string(),
    })
}

pub fn first_window(timezone: &str, local_time: &str, activated_at: i64) -> Result<DailyWindow> {
    let (zone, time) = validate(timezone, local_time)?;
    let mut date = utc(activated_at)?.with_timezone(&zone).date_naive();
    if resolve(zone, date.and_time(NaiveTime::MIN))? < activated_at {
        date = next(date)?;
    }
    let mut result = window(zone, time, date)?;
    while result.start == result.end {
        date = next(date)?;
        result = window(zone, time, date)?;
    }
    Ok(result)
}

pub fn previous_day(timezone: &str, now: i64) -> Result<DailyWindow> {
    let zone = timezone.parse::<Tz>().context("unknown IANA time zone")?;
    let mut date = utc(now)?
        .with_timezone(&zone)
        .date_naive()
        .pred_opt()
        .context("calendar overflow")?;
    loop {
        let result = window(zone, NaiveTime::MIN, date)?;
        if result.start < result.end {
            return Ok(result);
        }
        date = date.pred_opt().context("calendar overflow")?;
    }
}

/// Strict canonical dates keep IPC, filenames and provenance unambiguous.
pub fn parse_date(value: &str) -> Result<NaiveDate> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").context("date must be YYYY-MM-DD")?;
    if value.len() != 10 || date.format("%Y-%m-%d").to_string() != value {
        bail!("date must be YYYY-MM-DD");
    }
    Ok(date)
}

/// Inclusive civil dates become a half-open UTC window; today stops at enqueue time.
pub fn selected_range(timezone: &str, start: &str, end: &str, now: i64) -> Result<DailyWindow> {
    let zone = timezone.parse::<Tz>().context("unknown IANA time zone")?;
    let first = parse_date(start)?;
    let last = parse_date(end)?;
    if first > last {
        bail!("start date must not follow end date");
    }
    let today = utc(now)?.with_timezone(&zone).date_naive();
    if last > today {
        bail!("future report dates are not allowed");
    }
    let start_ms = resolve(zone, first.and_time(NaiveTime::MIN))?;
    let end_ms = resolve(zone, next(last)?.and_time(NaiveTime::MIN))?.min(now);
    if end_ms <= start_ms {
        bail!("selected dates contain no elapsed time");
    }
    Ok(DailyWindow {
        start: start_ms,
        end: end_ms,
        local_date: start.into(),
        due_at: now,
    })
}

/// Uses the saved cutoff, never today's date, so regenerated ranges retain their as-of scope.
pub fn range_is_in_progress(timezone: &str, end_date: &str, window_end: i64) -> Result<bool> {
    let zone = timezone.parse::<Tz>().context("unknown IANA time zone")?;
    let end = resolve(zone, next(parse_date(end_date)?)?.and_time(NaiveTime::MIN))?;
    Ok(window_end < end)
}

/// Local calendar-day periods for a memory report window (`03 §8c`): boundaries are
/// consecutive local midnights in `timezone`, resolved with the same DST rules as
/// scheduled windows (a missing wall time resolves to the first valid instant; an
/// ambiguous one uses the earlier instant once). The first period opens at `start`
/// and the last closes exactly at `end`, so a 25 h fall-back day is one period, a
/// 23 h spring-forward day is one period, N local days are N periods and a partial
/// day is one period. When the window spans more than `max_periods` local days,
/// whole local days are grouped so a long selected range keeps a bounded, evenly
/// spread period count (no unlimited inference).
pub fn local_day_grid(
    timezone: &str,
    start: i64,
    end: i64,
    max_periods: u32,
) -> Result<Vec<(i64, i64)>> {
    if end <= start {
        return Ok(Vec::new());
    }
    let zone = timezone.parse::<Tz>().context("unknown IANA time zone")?;
    let first = utc(start)?.with_timezone(&zone).date_naive();
    let last = utc(end - 1)?.with_timezone(&zone).date_naive();
    let days = last.signed_duration_since(first).num_days() + 1;
    let cap = i64::from(max_periods.max(1));
    let group = ((days + cap - 1) / cap).max(1);
    let mut periods = Vec::new();
    let mut offset = 0;
    while offset < days {
        let period_start = if offset == 0 {
            start
        } else {
            midnight(zone, first, offset)?
        };
        let next = offset + group;
        let period_end = if next >= days {
            end
        } else {
            midnight(zone, first, next)?
        };
        // A whole civil date can be skipped (e.g. Pacific/Apia); that day has no
        // period and the surrounding periods stay contiguous.
        if period_end > period_start {
            periods.push((period_start, period_end));
        }
        offset = next;
    }
    Ok(periods)
}

/// Local midnight for `first + offset` days in `zone`, via the shared DST resolve.
fn midnight(zone: Tz, first: NaiveDate, offset: i64) -> Result<i64> {
    let offset = u64::try_from(offset).context("calendar overflow")?;
    let date = first
        .checked_add_days(Days::new(offset))
        .context("calendar overflow")?;
    resolve(zone, date.and_time(NaiveTime::MIN))
}

/// Return the seven most recent eligible overdue windows oldest first, exposing older gaps.
/// Caller durably enqueues each unique window before advancing its persisted cursor.
pub fn due_windows(
    timezone: &str,
    local_time: &str,
    activated_at: i64,
    cursor_end: Option<i64>,
    now: i64,
) -> Result<ScheduleDue> {
    let (zone, time) = validate(timezone, local_time)?;
    let first = first_window(timezone, local_time, activated_at)?;
    let floor = cursor_end.unwrap_or(first.start).max(first.start);
    let mut date = utc(now)?
        .with_timezone(&zone)
        .date_naive()
        .pred_opt()
        .context("calendar overflow")?;
    let mut windows = Vec::new();
    // Only inspect recent dates, not every day since a potentially decades-old activation.
    for _ in 0..10 {
        let w = window(zone, time, date)?;
        if w.end <= floor || w.start < first.start {
            break;
        }
        if w.start >= floor && w.start < w.end && w.due_at <= now {
            windows.push(w);
            if windows.len() == 7 {
                break;
            }
        }
        date = date.pred_opt().context("calendar overflow")?;
    }
    windows.reverse();
    let gap = windows
        .first()
        .and_then(|w| (w.start > floor).then_some((floor, w.start)));
    Ok(ScheduleDue { windows, gap })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stamp(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    #[test]
    fn daily_window_handles_dst() {
        let short = previous_day("Europe/Paris", stamp("2026-03-30T12:00:00Z")).unwrap();
        let long = previous_day("Europe/Paris", stamp("2026-10-26T12:00:00Z")).unwrap();
        assert_eq!(short.end - short.start, 23 * 3_600_000);
        assert_eq!(long.end - long.start, 25 * 3_600_000);
        let gap = window(
            "Europe/Paris".parse().unwrap(),
            NaiveTime::from_hms_opt(2, 30, 0).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 28).unwrap(),
        )
        .unwrap();
        assert_eq!(gap.due_at, stamp("2026-03-29T01:00:00Z"));
        let overlap = window(
            "Europe/Paris".parse().unwrap(),
            NaiveTime::from_hms_opt(2, 30, 0).unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 24).unwrap(),
        )
        .unwrap();
        assert_eq!(overlap.due_at, stamp("2026-10-25T00:30:00Z"));
    }
    #[test]
    fn restart_backfills_once_and_cap_reports_gap() {
        let activation = stamp("2026-01-01T12:00:00Z");
        let now = stamp("2026-01-20T12:00:00Z");
        let due = due_windows("Etc/UTC", "08:00", activation, None, now).unwrap();
        assert_eq!(due.windows.len(), 7);
        assert_eq!(due.windows[0].local_date, "2026-01-13");
        assert_eq!(
            due.gap,
            Some((stamp("2026-01-02T00:00:00Z"), stamp("2026-01-13T00:00:00Z")))
        );
        assert!(due_windows(
            "Etc/UTC",
            "08:00",
            activation,
            Some(due.windows.last().unwrap().end),
            now
        )
        .unwrap()
        .windows
        .is_empty());
    }
    #[test]
    fn first_enable_excludes_partial_day_and_has_no_early_run() {
        let a = stamp("2026-01-01T12:00:00Z");
        assert_eq!(
            first_window("Etc/UTC", "08:00", a).unwrap().local_date,
            "2026-01-02"
        );
        assert!(
            due_windows("Etc/UTC", "08:00", a, None, stamp("2026-01-03T07:59:00Z"))
                .unwrap()
                .windows
                .is_empty()
        );
        assert_eq!(
            due_windows("Etc/UTC", "08:00", a, None, stamp("2026-01-03T08:00:00Z"))
                .unwrap()
                .windows
                .len(),
            1
        );
    }
    #[test]
    fn selected_dates_are_inclusive_dst_aware_and_today_is_capped() {
        let now = stamp("2026-03-30T10:15:00Z");
        let range = selected_range("Europe/Paris", "2026-03-28", "2026-03-29", now).unwrap();
        assert_eq!(range.start, stamp("2026-03-27T23:00:00Z"));
        assert_eq!(range.end, stamp("2026-03-29T22:00:00Z"));
        assert_eq!(range.end - range.start, 47 * 3_600_000);
        let today = selected_range("Europe/Paris", "2026-03-30", "2026-03-30", now).unwrap();
        assert_eq!(today.end, now);
        assert!(range_is_in_progress("Europe/Paris", "2026-03-30", today.end).unwrap());
        assert!(!range_is_in_progress("Europe/Paris", "2026-03-29", range.end).unwrap());
        let long = selected_range(
            "Europe/Paris",
            "2026-10-25",
            "2026-10-25",
            stamp("2026-10-26T12:00:00Z"),
        )
        .unwrap();
        assert_eq!(long.end - long.start, 25 * 3_600_000);
    }

    #[test]
    fn selected_dates_reject_invalid_future_reversed_and_skipped_days() {
        let now = stamp("2026-01-02T00:30:00Z");
        for (start, end) in [
            ("2026-1-01", "2026-01-01"),
            ("2026-02-30", "2026-02-30"),
            ("2026-01-02", "2026-01-01"),
            ("2026-01-03", "2026-01-03"),
        ] {
            assert!(selected_range("UTC", start, end, now).is_err());
        }
        assert!(selected_range("America/Los_Angeles", "2026-01-02", "2026-01-02", now).is_err());
        assert!(selected_range("Pacific/Apia", "2011-12-30", "2011-12-30", now).is_err());
    }

    #[test]
    fn local_day_grid_is_one_period_per_local_day_through_dst() {
        // Fall-back day (Europe/Paris 2026-10-25) lasts 25 h and is still one period.
        let long = local_day_grid(
            "Europe/Paris",
            stamp("2026-10-24T22:00:00Z"),
            stamp("2026-10-25T23:00:00Z"),
            31,
        )
        .unwrap();
        assert_eq!(
            long,
            vec![(stamp("2026-10-24T22:00:00Z"), stamp("2026-10-25T23:00:00Z"))]
        );
        // Spring-forward day (2026-03-29) lasts 23 h and is one period.
        let short = local_day_grid(
            "Europe/Paris",
            stamp("2026-03-28T23:00:00Z"),
            stamp("2026-03-29T22:00:00Z"),
            31,
        )
        .unwrap();
        assert_eq!(short.len(), 1);
        // Seven local days are seven periods even with the 25 h day inside, and the
        // boundaries land exactly on local midnights (whole local-day buckets).
        let week = local_day_grid(
            "Europe/Paris",
            stamp("2026-10-20T22:00:00Z"),
            stamp("2026-10-27T23:00:00Z"),
            31,
        )
        .unwrap();
        assert_eq!(week.len(), 7);
        assert_eq!(week[0].0, stamp("2026-10-20T22:00:00Z"));
        assert_eq!(week[6].1, stamp("2026-10-27T23:00:00Z"));
        assert_eq!(week[4].1 - week[4].0, 25 * 3_600_000);
        assert_eq!(week[5].1 - week[5].0, 24 * 3_600_000);
        let zone: Tz = "Europe/Paris".parse().unwrap();
        for (_, end) in week.iter().take(6) {
            assert_eq!(
                utc(*end).unwrap().with_timezone(&zone).time(),
                NaiveTime::MIN
            );
        }
        // A partial day (window frozen mid-day) is one period.
        let partial = local_day_grid(
            "Europe/Paris",
            stamp("2026-03-29T22:00:00Z"),
            stamp("2026-03-30T08:00:00Z"),
            31,
        )
        .unwrap();
        assert_eq!(
            partial,
            vec![(stamp("2026-03-29T22:00:00Z"), stamp("2026-03-30T08:00:00Z"))]
        );
    }

    #[test]
    fn local_day_grid_caps_long_spans_by_grouping_whole_local_days() {
        let zone: Tz = "Europe/Paris".parse().unwrap();
        let start = stamp("2026-01-01T23:00:00Z"); // 2026-01-02 00:00 local
        let end = resolve(
            zone,
            NaiveDate::from_ymd_opt(2026, 3, 3)
                .unwrap()
                .and_time(NaiveTime::MIN),
        )
        .unwrap(); // 60 local days later
        let grid = local_day_grid("Europe/Paris", start, end, 31).unwrap();
        assert_eq!(grid.len(), 30, "60 local days grouped 2-at-a-time under 31");
        assert!(grid.len() <= 31);
        assert_eq!(grid[0].0, start);
        assert_eq!(grid[29].1, end);
        for (_, boundary) in grid.iter().take(29) {
            assert_eq!(
                utc(*boundary).unwrap().with_timezone(&zone).time(),
                NaiveTime::MIN,
                "group boundaries stay on local midnights"
            );
        }
        // Degenerate and invalid inputs stay explicit, never a panic.
        assert!(local_day_grid("Europe/Paris", end, start, 31)
            .unwrap()
            .is_empty());
        assert!(local_day_grid("Not/AZone", start, end, 31).is_err());
    }
}
