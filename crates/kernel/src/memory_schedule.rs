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
}
