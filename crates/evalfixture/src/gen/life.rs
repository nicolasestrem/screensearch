//! The persona timeline: calendar days, sessions, frames and the app each frame shows.
//!
//! Works for every persona from data alone: the weekly weekday patterns, the active window,
//! the session-count range, the daily nap, the holiday list and the vacation weeks all come
//! from [`Persona::rhythm`]; activities from [`Persona::activities`]; contexts from
//! [`Persona::contexts`] and their 0-based week ranges. Nothing here knows an occupation.
//!
//! **Clock.** Every instant is laid out in the persona's fixed UTC offset
//! (for example `+02:00`), never the machine's zone or the current time, so the database and
//! its digest are byte-identical everywhere. The run still writes exactly
//! `weeks x frames_per_week` frames: a week listed in `weeks_off` writes none and its budget
//! is redistributed over the other weeks, and a day without sessions simply carries no
//! frames from the frames it did not claim.
//!
//! **Shape.** Each active day holds `sessions.0..=sessions.1` sessions of at least two
//! frames, separated by short gaps and (when defined) one daily nap or lunch break. Frames
//! inside a session sit a few minutes apart. Sessions keep an activity and a context across
//! their frames the way real work does; the secondary frames of a session switch to an app
//! that belongs with the activity.

use anyhow::{ensure, Context as _, Result};
use chrono::{DateTime, Datelike, Days, FixedOffset, NaiveDate, TimeZone};
use traits::{SessionHost, SessionKind};

use super::personas::{AppKind, Persona};
use super::prng::{subseed, SplitMix64};

const MINUTE_MS: i64 = 60_000;
/// The earliest local start of a day gets this much seeded jitter, in minutes.
const START_JITTER_MIN: usize = 25;
/// Intra-session frame spacing (seconds).
const SPACING_MIN_SECS: i64 = 60;
const SPACING_SPAN_SECS: i64 = 540;
/// Gap between two sessions (minutes).
const GAP_MIN_MIN: i64 = 8;
const GAP_SPAN_MIN: i64 = 62;

/// One frame of a persona session: an instant, the app index in `Persona::apps`, and the
/// monitor index in `Persona::monitors`.
#[derive(Debug, Clone)]
pub(crate) struct LifeFrame {
    pub at: i64,
    pub app: usize,
    pub monitor: u8,
}

/// One planned session. `app` is the primary application; frames may switch to related apps.
#[derive(Debug, Clone)]
pub(crate) struct LifeSession {
    pub context: Option<usize>,
    pub frames: Vec<LifeFrame>,
    pub started_at: i64,
    pub ended_at: i64,
    pub kind: SessionKind,
    pub tool: Option<&'static str>,
    pub host: Option<SessionHost>,
    pub context_key: String,
    pub confidence: f64,
}

/// Everything the persona writer needs: sessions in chronological order.
#[derive(Debug, Clone)]
pub(crate) struct LifePlan {
    pub sessions: Vec<LifeSession>,
}

/// A persona instant in the persona's fixed offset.
pub(crate) fn local(at_ms: i64, offset: FixedOffset) -> DateTime<FixedOffset> {
    DateTime::from_timestamp_millis(at_ms)
        .expect("fixture timestamps are well inside chrono's range")
        .with_timezone(&offset)
}

/// The taskbar clock in the persona's locale: time and date.
pub(crate) fn clock(
    at: i64,
    offset: FixedOffset,
    labels: &super::personas::Labels,
) -> (String, String) {
    let local = local(at, offset);
    (
        local.format(labels.time_format).to_string(),
        local.format(labels.date_format).to_string(),
    )
}

fn weighted<T: Copy>(rng: &mut SplitMix64, items: &[(T, u64)]) -> T {
    let total: u64 = items.iter().map(|(_, weight)| weight).sum();
    let mut roll = rng.next_u64() % total.max(1);
    for (item, weight) in items {
        if roll < *weight {
            return *item;
        }
        roll -= weight;
    }
    items[items.len() - 1].0
}

/// Apps that belong on screen next to another app's session.
fn affinity(kind: AppKind) -> &'static [(AppKind, u64)] {
    match kind {
        AppKind::Code => &[
            (AppKind::Terminal, 3),
            (AppKind::Browser, 2),
            (AppKind::Chat, 1),
            (AppKind::Files, 1),
        ],
        AppKind::Terminal => &[(AppKind::Code, 2), (AppKind::Browser, 1)],
        AppKind::Browser => &[
            (AppKind::Mail, 1),
            (AppKind::Chat, 1),
            (AppKind::Notes, 1),
            (AppKind::Document, 1),
            (AppKind::Spreadsheet, 1),
        ],
        AppKind::Mail => &[
            (AppKind::Chat, 1),
            (AppKind::Files, 1),
            (AppKind::Document, 1),
            (AppKind::Spreadsheet, 1),
        ],
        AppKind::Spreadsheet => &[
            (AppKind::Browser, 1),
            (AppKind::Pdf, 1),
            (AppKind::Files, 1),
            (AppKind::Document, 1),
        ],
        AppKind::Document => &[
            (AppKind::Browser, 1),
            (AppKind::Pdf, 1),
            (AppKind::Files, 1),
            (AppKind::Mail, 1),
        ],
        AppKind::Slides => &[
            (AppKind::Document, 1),
            (AppKind::Browser, 1),
            (AppKind::Pdf, 1),
        ],
        AppKind::Pdf => &[
            (AppKind::Browser, 1),
            (AppKind::Document, 1),
            (AppKind::Files, 1),
        ],
        AppKind::Chat => &[
            (AppKind::Browser, 1),
            (AppKind::Mail, 1),
            (AppKind::Files, 1),
        ],
        AppKind::Files => &[
            (AppKind::Document, 1),
            (AppKind::Spreadsheet, 1),
            (AppKind::Pdf, 1),
            (AppKind::Mail, 1),
        ],
        AppKind::Meeting => &[
            (AppKind::Document, 1),
            (AppKind::Notes, 1),
            (AppKind::Browser, 1),
        ],
        AppKind::Notes => &[
            (AppKind::Browser, 1),
            (AppKind::Chat, 1),
            (AppKind::Document, 1),
        ],
        AppKind::Canvas => &[
            (AppKind::Files, 1),
            (AppKind::Chat, 1),
            (AppKind::Browser, 1),
        ],
        AppKind::Game => &[(AppKind::Chat, 1), (AppKind::Browser, 1)],
    }
}

/// The persona's apps whose kind belongs next to `kind`, with the app's own usage weight.
fn related_apps(persona: &'static Persona, kind: AppKind) -> Vec<(usize, u64)> {
    let related = affinity(kind);
    let mut out = Vec::new();
    for (related_kind, weight) in related {
        for (index, app) in persona.apps_of(*related_kind) {
            out.push((index, app.weight * weight));
        }
    }
    out
}

/// Splits a day's frames into `sessions.0..=sessions.1` sessions of at least two frames.
fn split_day(rng: &mut SplitMix64, frames: usize, range: (u8, u8)) -> Vec<usize> {
    if frames < 2 {
        return Vec::new();
    }
    let wanted = range.0 as usize + rng.below(range.1 as usize - range.0 as usize + 1);
    let sessions = wanted.min(frames / 2).max(1);
    let mut sizes = vec![2usize; sessions];
    for _ in 0..frames - 2 * sessions {
        sizes[rng.below(sessions)] += 1;
    }
    sizes
}

/// Distributes one week's frame budget over the active days.
fn split_week(rng: &mut SplitMix64, frames: usize, days: &[u8]) -> Result<Vec<(u8, usize)>> {
    ensure!(!days.is_empty(), "an active week needs at least one day");
    let active = (frames / 2).clamp(1, days.len());
    let mut order = days.to_vec();
    rng.shuffle(&mut order);
    let mut chosen: Vec<u8> = order[..active].to_vec();
    chosen.sort_unstable();
    let base = frames / active;
    let mut out: Vec<(u8, usize)> = chosen
        .iter()
        .enumerate()
        .map(|(i, day)| (*day, base + usize::from(i < frames % active)))
        .collect();
    // Busier and quieter days: move a few frames between active days, never below two.
    if out.len() > 1 {
        let floor = (base - base / 4).max(2);
        for _ in 0..out.len() * 2 {
            let from = rng.below(out.len());
            let to = rng.below(out.len());
            if from == to {
                continue;
            }
            let movable = out[from].1.saturating_sub(floor).min(base / 8);
            if movable > 0 {
                let moved = 1 + rng.below(movable);
                out[from].1 -= moved;
                out[to].1 += moved;
            }
        }
    }
    Ok(out)
}

/// The local date of `day` (0-based calendar day counting from the Monday `start`).
fn day_date(start: NaiveDate, day: usize) -> Result<NaiveDate> {
    start
        .checked_add_days(Days::new(day as u64))
        .context("persona fixture date overflow")
}

fn is_holiday(date: NaiveDate, holidays: &[(u8, u8)]) -> bool {
    holidays
        .iter()
        .any(|(month, day)| date.month() as u8 == *month && date.day() as u8 == *day)
}

fn day_midnight_ms(date: NaiveDate, offset: FixedOffset) -> Result<i64> {
    let midnight = date.and_hms_opt(0, 0, 0).context("invalid midnight")?;
    Ok(offset
        .from_local_datetime(&midnight)
        .single()
        .context("fixed offsets have exactly one local midnight")?
        .timestamp_millis())
}

/// Builds the persona timeline for `(seed, weeks, frames_per_week, start)`.
pub(crate) fn plan(
    seed: u64,
    weeks: u32,
    frames_per_week: usize,
    start: NaiveDate,
    persona: &'static Persona,
) -> Result<LifePlan> {
    let offset = persona.offset()?;
    let rhythm = persona.rhythm;
    let total = weeks as usize * frames_per_week;

    // Weekly budgets: a vacation week writes nothing and lends its budget to the others.
    let mut off = vec![false; weeks as usize];
    for week in rhythm.weeks_off {
        if let Some(slot) = off.get_mut(*week as usize) {
            *slot = true;
        }
    }
    let any_off = off.iter().any(|value| *value);
    let mut active_weeks = if any_off {
        off.iter().filter(|value| !**value).count()
    } else {
        weeks as usize
    };
    if any_off {
        // A definition whose every week is off would write no frames: ignore the list
        // rather than fail the whole run, and say so in the digest as zero sessions.
        if active_weeks == 0 {
            active_weeks = weeks as usize;
            off.iter_mut().for_each(|value| *value = false);
        }
    }
    let mut budgets = vec![0usize; weeks as usize];
    let base = total / active_weeks;
    let remainder = total % active_weeks;
    let mut seen_active = 0usize;
    for (week, budget) in budgets.iter_mut().enumerate() {
        if off[week] {
            continue;
        }
        *budget = base + usize::from(seen_active < remainder);
        seen_active += 1;
    }
    ensure!(
        budgets.iter().sum::<usize>() == total,
        "persona budget allocation wrote {} of {total} frames",
        budgets.iter().sum::<usize>()
    );

    let mut rng = SplitMix64::new(subseed(seed, "persona-timeline"));
    let mut sessions: Vec<LifeSession> = Vec::new();
    let mut context_uses = vec![0u32; persona.contexts.len()];

    for (week, budget) in budgets.iter().enumerate() {
        if *budget == 0 {
            continue;
        }
        let week = week as u32;
        let pattern = rhythm.patterns[(week as usize) % rhythm.patterns.len()];
        let mut days: Vec<u8> = pattern
            .iter()
            .copied()
            .filter(|weekday| {
                let date = day_date(start, week as usize * 7 + *weekday as usize)
                    .expect("weekday date is in range");
                !is_holiday(date, rhythm.holidays)
            })
            .collect();
        if days.is_empty() {
            // A week where every working day is a holiday still needs the frames; keep the
            // pattern's days rather than silently dropping a week's budget.
            days = pattern.to_vec();
        }
        days.sort_unstable();
        days.dedup();

        for (weekday, day_frames) in split_week(&mut rng, *budget, &days)? {
            if day_frames < 2 {
                continue;
            }
            let date = day_date(start, week as usize * 7 + weekday as usize)?;
            let midnight = day_midnight_ms(date, offset)?;
            let sizes = split_day(&mut rng, day_frames, rhythm.sessions);

            // Day clock: one seeded start, intra-session spacings, inter-session gaps and
            // the optional nap, scaled to fit before the persona's end of day.
            let day_start = midnight
                + (rhythm.start_min + rng.below(START_JITTER_MIN + 1) as i64) * MINUTE_MS
                + rng.below(60) as i64 * 1000;
            let mut spacings: Vec<Vec<i64>> = sizes
                .iter()
                .map(|count| {
                    (1..*count)
                        .map(|_| {
                            (SPACING_MIN_SECS + rng.below(SPACING_SPAN_SECS as usize) as i64) * 1000
                                + rng.below(1000) as i64
                        })
                        .collect()
                })
                .collect();
            let mut gaps: Vec<i64> = (1..sizes.len())
                .map(|_| {
                    (GAP_MIN_MIN + rng.below(GAP_SPAN_MIN as usize) as i64) * MINUTE_MS
                        + rng.below(1000) as i64
                })
                .collect();
            let nap_len = rhythm.nap.map_or(0, |(from, to)| (to - from) * MINUTE_MS);
            let raw_total: i64 =
                spacings.iter().flatten().sum::<i64>() + gaps.iter().sum::<i64>() + nap_len;
            let available = midnight + rhythm.end_min * MINUTE_MS - day_start;
            if raw_total > available {
                let scale = |value: i64| {
                    ((i128::from(value) * i128::from(available) / i128::from(raw_total)) as i64)
                        .max(1000)
                };
                for spacing in spacings.iter_mut().flatten() {
                    *spacing = scale(*spacing);
                }
                for gap in gaps.iter_mut() {
                    *gap = scale(*gap);
                }
            }
            let nap_at = rhythm
                .nap
                .map(|(_, to)| midnight + to * MINUTE_MS)
                .filter(|at| *at > day_start);

            let live: Vec<usize> = (0..persona.contexts.len())
                .filter(|index| Persona::context_live(&persona.contexts[*index], week))
                .collect();
            let mut cursor = day_start;
            let mut previous: Option<AppKind> = None;
            let mut napped = false;
            for (session, size) in sizes.iter().enumerate() {
                if session > 0 {
                    cursor += gaps[session - 1];
                    if !napped && nap_at.is_some_and(|at| cursor >= at) {
                        cursor += nap_len;
                        napped = true;
                    }
                } else if !napped {
                    if let Some(at) = nap_at {
                        if cursor >= at {
                            cursor += nap_len;
                            napped = true;
                        }
                    }
                }

                // Activity: weighted, favouring a change from the previous session.
                let activity = {
                    let mut chosen = weighted(&mut rng, persona.activities);
                    if Some(chosen) == previous {
                        for _ in 0..3 {
                            chosen = weighted(&mut rng, persona.activities);
                            if Some(chosen) != previous {
                                break;
                            }
                        }
                    }
                    chosen
                };
                let apps: Vec<(usize, u64)> = persona
                    .apps_of(activity)
                    .map(|(index, app)| (index, app.weight))
                    .collect();
                ensure!(
                    !apps.is_empty(),
                    "persona `{}`: no app for activity `{}`",
                    persona.id,
                    activity.as_str()
                );
                let primary = weighted(&mut rng, &apps);
                let context = if live.is_empty() {
                    None
                } else {
                    // Least-used context half of the time, so dormant contexts are not lost.
                    let pick = if rng.chance(1, 2) {
                        let min = live
                            .iter()
                            .map(|index| context_uses[*index])
                            .min()
                            .unwrap_or(0);
                        let tied: Vec<usize> = live
                            .iter()
                            .copied()
                            .filter(|index| context_uses[*index] == min)
                            .collect();
                        tied[rng.below(tied.len())]
                    } else {
                        live[rng.below(live.len())]
                    };
                    context_uses[pick] += 1;
                    Some(pick)
                };

                let related = related_apps(persona, activity);
                let session_monitor = if persona.monitors.len() > 1 && rng.chance(1, 4) {
                    1u8
                } else {
                    0u8
                };
                let mut frames: Vec<LifeFrame> = Vec::with_capacity(*size);
                for frame in 0..*size {
                    if frame > 0 {
                        cursor += spacings[session][frame - 1];
                    }
                    let app = if frame == 0 || rng.chance(65, 100) || related.is_empty() {
                        primary
                    } else {
                        weighted(&mut rng, &related)
                    };
                    let monitor = if frame == 0 || rng.chance(19, 20) {
                        session_monitor
                    } else {
                        u8::from(session_monitor == 0 && persona.monitors.len() > 1)
                    };
                    frames.push(LifeFrame {
                        at: cursor,
                        app,
                        monitor,
                    });
                }
                previous = Some(activity);
                let last = cursor;
                let next_gap = gaps.get(session).copied().unwrap_or(10 * MINUTE_MS);
                let tail = ((20 + rng.below(131) as i64) * 1000).min(next_gap / 2);
                let hint = persona.apps[primary].hint.to_ascii_lowercase();
                let (kind, tool, host, context_key) = match activity {
                    AppKind::Meeting => (
                        SessionKind::Meeting,
                        None,
                        Some(SessionHost::Desktop),
                        format!("meeting:{hint}"),
                    ),
                    AppKind::Terminal => (
                        SessionKind::Focus,
                        None,
                        Some(SessionHost::Terminal),
                        format!("focus:{hint}"),
                    ),
                    _ => (SessionKind::Focus, None, None, format!("focus:{hint}")),
                };
                let confidence = if kind == SessionKind::Meeting {
                    (700 + rng.below(251)) as f64 / 1000.0
                } else {
                    (450 + rng.below(201)) as f64 / 1000.0
                };
                sessions.push(LifeSession {
                    context,
                    started_at: frames[0].at,
                    frames,
                    ended_at: last + tail,
                    kind,
                    tool,
                    host,
                    context_key,
                    confidence,
                });
            }
        }
    }

    Ok(LifePlan { sessions })
}
