//! The fixture timeline: working days, sessions, frame instants and apps.
//!
//! **Clock.** Everything is laid out from a fixed base date, Monday 2026-09-07, in a fixed
//! UTC+02:00 offset (the maintainer's CEST in September 2026), never the machine's zone or
//! the current time: the database and its digest are byte-identical on every machine, and on
//! the maintainer's machine the history reads as ordinary working days starting around
//! 08:00. A machine in another zone sees the same instants shifted by its own offset.
//!
//! **Shape.** `weeks` weeks of five working days; each week holds exactly
//! `frames_per_week` frames, split over its days. A day with `n` frames holds
//! `min(6..=9, n / 2)` sessions of at least two frames each, separated by short gaps and a
//! lunch break, with frames a few minutes apart inside a session.
//!
//! **Coverage.** Every catalog entity must reach `content_text` in a full-size fixture, so
//! the planner spreads one *anchored* session per coverage view across the timeline (a
//! project's ripgrep listing, review page, ticket board, document folder and site; every
//! reference page, observability page and admin folder). An anchored session opens on the
//! view that carries those entities; the rest of the timeline is free-form. When a fixture
//! has fewer sessions than anchors (tiny test fixtures), the remainder is simply not placed.
//!
//! **Seen-again material.** Per project, some frames are grouped in twos to fives and each
//! group shares one distinctive line (an error signature, a commit, an action item, an
//! alert), so exact-phrase search finds the same line in 2-5 frames across sessions.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use traits::{SessionHost, SessionKind};

use super::code::{request_id, short_hash, teammate};
use super::prng::{subseed, SplitMix64};
use super::world::{ADMIN_DOCS, PROJECTS, RESOURCES};

/// The fixed first working day (a Monday).
pub(crate) const BASE_DATE: (i32, u32, u32) = (2026, 9, 7);
/// The fixed UTC offset the working day is laid out in (UTC+02:00).
pub(crate) const UTC_OFFSET_SECS: i32 = 2 * 3600;

const MINUTE_MS: i64 = 60_000;
/// Earliest start of a working day (08:00) and the jitter added to it.
const DAY_START_MIN: i64 = 8 * 60;
const DAY_START_JITTER_MIN: usize = 40;
/// The last frame of a day lands before 19:00 even for very dense fixtures.
const DAY_END_MIN: i64 = 19 * 60;
/// Lunch starts after the first session that ends past noon.
const NOON_MIN: i64 = 12 * 60;

pub(crate) fn offset() -> FixedOffset {
    FixedOffset::east_opt(UTC_OFFSET_SECS).expect("UTC+02:00 is a valid offset")
}

/// A fixture instant in the fixture's fixed offset.
pub(crate) fn local(at_ms: i64) -> DateTime<FixedOffset> {
    DateTime::from_timestamp_millis(at_ms)
        .expect("fixture timestamps are well inside chrono's range")
        .with_timezone(&offset())
}

/// Unix ms of local midnight on working day `day` (0-based, weekends skipped).
fn day_midnight_ms(day: usize) -> Result<i64> {
    let (y, m, d) = BASE_DATE;
    let base = NaiveDate::from_ymd_opt(y, m, d).context("invalid base date")?;
    let week = (day / 5) as u64;
    let weekday = (day % 5) as u64;
    let date = base
        .checked_add_days(chrono::Days::new(week * 7 + weekday))
        .context("fixture date overflow")?;
    let midnight = date.and_hms_opt(0, 0, 0).context("invalid midnight")?;
    Ok(offset()
        .from_local_datetime(&midnight)
        .single()
        .context("fixed offsets have exactly one local midnight")?
        .timestamp_millis())
}

/// Foreground applications, by the process stem the capture loop records as `app_hint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum App {
    Code,
    Terminal,
    Chrome,
    Slack,
    Outlook,
    Excel,
    Word,
    PowerPoint,
    Acrobat,
    Notion,
    Figma,
    Explorer,
    Zoom,
}

impl App {
    const ALL: [App; 13] = [
        App::Code,
        App::Terminal,
        App::Chrome,
        App::Slack,
        App::Outlook,
        App::Excel,
        App::Word,
        App::PowerPoint,
        App::Acrobat,
        App::Notion,
        App::Figma,
        App::Explorer,
        App::Zoom,
    ];

    /// The executable stem (`QueryFullProcessImageNameW` file stem), as production stores it.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            App::Code => "Code",
            App::Terminal => "WindowsTerminal",
            App::Chrome => "chrome",
            App::Slack => "slack",
            App::Outlook => "OUTLOOK",
            App::Excel => "EXCEL",
            App::Word => "WINWORD",
            App::PowerPoint => "POWERPNT",
            App::Acrobat => "Acrobat",
            App::Notion => "Notion",
            App::Figma => "Figma",
            App::Explorer => "explorer",
            App::Zoom => "Zoom",
        }
    }
}

/// What a session is about; it fixes the session kind and the app mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity {
    Coding,
    Shell,
    AiPairing,
    Review,
    Triage,
    Incident,
    Chat,
    Email,
    Spreadsheet,
    Writing,
    Design,
    Meeting,
    Files,
    Research,
    BrowserAi,
    Admin,
}

/// Free-form activity weights (anchored sessions are placed separately).
const ACTIVITY_WEIGHTS: [(Activity, u64); 16] = [
    (Activity::Coding, 18),
    (Activity::Shell, 6),
    (Activity::AiPairing, 7),
    (Activity::Review, 8),
    (Activity::Triage, 6),
    (Activity::Incident, 4),
    (Activity::Chat, 10),
    (Activity::Email, 8),
    (Activity::Spreadsheet, 4),
    (Activity::Writing, 6),
    (Activity::Design, 3),
    (Activity::Meeting, 7),
    (Activity::Files, 3),
    (Activity::Research, 6),
    (Activity::BrowserAi, 3),
    (Activity::Admin, 3),
];

/// Projects with a user interface, the only ones a design session opens.
const UI_PROJECTS: [usize; 5] = [2, 4, 5, 8, 10];

/// The coverage view an anchored session opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    /// Terminal: `git remote -v` plus a ripgrep listing of every source file.
    Code(usize),
    /// Browser: the project's `#n` page on its code host.
    Review(usize),
    /// Browser: the tracker board listing every ticket of the project.
    Triage(usize),
    /// Explorer: the project's document folder.
    Files(usize),
    /// Browser: the project's own site.
    Site(usize),
    /// Browser: one reference page.
    Resource(usize),
    /// Browser: one observability page (0 Sentry, 1 Grafana, 2 CI, 3 PagerDuty) for a project.
    Observability(usize, usize),
    /// Browser: a claude.ai conversation about a project.
    Claude(usize),
    /// Explorer: one admin folder (index into the distinct admin folders).
    AdminFolder(usize),
}

impl Anchor {
    fn activity(self) -> Activity {
        match self {
            Anchor::Code(_) => Activity::Shell,
            Anchor::Review(_) => Activity::Review,
            Anchor::Triage(_) => Activity::Triage,
            Anchor::Files(_) => Activity::Files,
            Anchor::Site(_) | Anchor::Resource(_) => Activity::Research,
            Anchor::Observability(..) => Activity::Incident,
            Anchor::Claude(_) => Activity::BrowserAi,
            Anchor::AdminFolder(_) => Activity::Admin,
        }
    }

    fn first_app(self) -> App {
        match self {
            Anchor::Code(_) => App::Terminal,
            Anchor::Files(_) | Anchor::AdminFolder(_) => App::Explorer,
            _ => App::Chrome,
        }
    }
}

/// The distinct folders holding admin documents, in catalog order.
pub(crate) fn admin_folders() -> Vec<&'static str> {
    let mut folders: Vec<&'static str> = Vec::new();
    for doc in &ADMIN_DOCS {
        if !folders.contains(&doc.folder) {
            folders.push(doc.folder);
        }
    }
    folders
}

/// Every coverage anchor, in a fixed order (shuffled by the planner).
fn all_anchors() -> Vec<Anchor> {
    let mut anchors = Vec::new();
    for p in 0..PROJECTS.len() {
        anchors.extend([
            Anchor::Code(p),
            Anchor::Review(p),
            Anchor::Triage(p),
            Anchor::Files(p),
            Anchor::Site(p),
        ]);
    }
    anchors.extend((0..RESOURCES.len()).map(Anchor::Resource));
    for which in 0..4 {
        anchors.push(Anchor::Observability(which, which * 3 % PROJECTS.len()));
    }
    anchors.push(Anchor::Claude(0));
    for i in 0..admin_folders().len() {
        anchors.push(Anchor::AdminFolder(i));
    }
    anchors
}

/// The number of coverage anchors; a fixture with at least this many sessions places all.
#[cfg(test)]
fn anchor_count() -> usize {
    all_anchors().len()
}

#[derive(Debug, Clone)]
pub(crate) struct FramePlan {
    pub at: i64,
    pub app: App,
    /// Set on the first frame of an anchored session.
    pub anchor: Option<Anchor>,
    /// Index into [`Plan::echoes`].
    pub echo: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionPlan {
    pub activity: Activity,
    pub project: Option<usize>,
    pub frames: Vec<FramePlan>,
    pub started_at: i64,
    pub ended_at: i64,
    pub kind: SessionKind,
    pub tool: Option<&'static str>,
    pub host: Option<SessionHost>,
    pub context_key: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub sessions: Vec<SessionPlan>,
    pub echoes: Vec<String>,
}

/// Splits one week's frames over its working days. Each active day gets at least two
/// frames (a session needs two); with fewer than ten frames some days stay empty.
fn split_week(rng: &mut SplitMix64, frames: usize) -> [usize; 5] {
    let active = (frames / 2).clamp(1, 5);
    let mut days: Vec<usize> = (0..5).collect();
    rng.shuffle(&mut days);
    let mut chosen: Vec<usize> = days[..active].to_vec();
    chosen.sort_unstable();
    let mut counts = [0usize; 5];
    let base = frames / active;
    for (i, day) in chosen.iter().enumerate() {
        counts[*day] = base + usize::from(i < frames % active);
    }
    // Busier and quieter days: move a few frames between active days. A day never drops
    // below three quarters of the even share (nor below two frames), so a full-size day
    // keeps room for its six to nine sessions.
    let floor = (base - base / 4).max(2);
    for _ in 0..active * 2 {
        let from = chosen[rng.below(active)];
        let to = chosen[rng.below(active)];
        let movable = counts[from].saturating_sub(floor).min(base / 8);
        if from != to && movable > 0 {
            let moved = 1 + rng.below(movable);
            counts[from] -= moved;
            counts[to] += moved;
        }
    }
    counts
}

/// Splits a day's frames into sessions of at least two frames each.
fn split_day(rng: &mut SplitMix64, frames: usize) -> Vec<usize> {
    if frames < 2 {
        return Vec::new();
    }
    let wanted = 6 + rng.below(4);
    let sessions = wanted.min(frames / 2).max(1);
    let mut sizes = vec![2usize; sessions];
    for _ in 0..frames - 2 * sessions {
        sizes[rng.below(sessions)] += 1;
    }
    sizes
}

fn weighted<T: Copy>(rng: &mut SplitMix64, items: &[(T, u64)]) -> T {
    let total: u64 = items.iter().map(|(_, w)| w).sum();
    let mut roll = rng.next_u64() % total.max(1);
    for (item, weight) in items {
        if roll < *weight {
            return *item;
        }
        roll -= weight;
    }
    items[items.len() - 1].0
}

fn primary_app(activity: Activity, rng: &mut SplitMix64) -> App {
    match activity {
        Activity::Coding => App::Code,
        Activity::Shell | Activity::AiPairing => App::Terminal,
        Activity::Review
        | Activity::Triage
        | Activity::Incident
        | Activity::Research
        | Activity::BrowserAi => App::Chrome,
        Activity::Chat => App::Slack,
        Activity::Email => App::Outlook,
        Activity::Spreadsheet => App::Excel,
        Activity::Writing => {
            if rng.chance(1, 2) {
                App::Notion
            } else {
                App::Word
            }
        }
        Activity::Design => App::Figma,
        Activity::Meeting => App::Zoom,
        Activity::Files => App::Explorer,
        Activity::Admin => weighted(
            rng,
            &[
                (App::Excel, 3),
                (App::Word, 2),
                (App::Acrobat, 2),
                (App::PowerPoint, 1),
            ],
        ),
    }
}

fn secondary_apps(activity: Activity) -> &'static [(App, u64)] {
    match activity {
        Activity::Coding => &[(App::Terminal, 3), (App::Chrome, 2), (App::Slack, 1)],
        Activity::Shell => &[(App::Code, 2), (App::Chrome, 1)],
        Activity::AiPairing => &[(App::Code, 2), (App::Chrome, 1)],
        Activity::Review => &[(App::Slack, 1), (App::Code, 1)],
        Activity::Triage => &[(App::Slack, 1), (App::Outlook, 1)],
        Activity::Incident => &[(App::Terminal, 2), (App::Slack, 2)],
        Activity::Chat => &[(App::Chrome, 1), (App::Outlook, 1)],
        Activity::Email => &[(App::Acrobat, 1), (App::Word, 1), (App::Excel, 1)],
        Activity::Spreadsheet => &[(App::Outlook, 1), (App::Chrome, 1)],
        Activity::Writing => &[(App::Chrome, 1), (App::Slack, 1)],
        Activity::Design => &[(App::Slack, 1), (App::Chrome, 1)],
        Activity::Meeting => &[(App::PowerPoint, 1), (App::Notion, 1), (App::Excel, 1)],
        Activity::Files => &[
            (App::Excel, 1),
            (App::Word, 1),
            (App::Acrobat, 1),
            (App::PowerPoint, 1),
        ],
        Activity::Research => &[(App::Code, 1), (App::Notion, 1)],
        Activity::BrowserAi => &[(App::Code, 1)],
        Activity::Admin => &[(App::Outlook, 1), (App::Explorer, 1)],
    }
}

/// The app with the most frames; ties go to the earlier first appearance.
fn dominant_app(apps: &[App]) -> App {
    let mut best = apps[0];
    let mut best_count = 0;
    for app in App::ALL {
        let count = apps.iter().filter(|a| **a == app).count();
        let first = apps.iter().position(|a| *a == app);
        let best_first = apps.iter().position(|a| *a == best);
        if count > best_count || (count == best_count && count > 0 && first < best_first) {
            best = app;
            best_count = count;
        }
    }
    best
}

/// A score in `[lo, lo + span]` with three decimals.
fn confidence(rng: &mut SplitMix64, lo: u64, span: u64) -> f64 {
    (lo + rng.below(span as usize + 1) as u64) as f64 / 1000.0
}

fn pick_activity(
    rng: &mut SplitMix64,
    first_of_day: bool,
    previous: Option<Activity>,
    meetings: usize,
) -> Activity {
    if first_of_day {
        let roll = rng.below(100);
        if roll < 60 {
            return Activity::Chat;
        }
        if roll < 85 {
            return Activity::Email;
        }
    }
    for _ in 0..4 {
        let activity = weighted(rng, &ACTIVITY_WEIGHTS);
        let repeat = Some(activity) == previous;
        let too_many_meetings = activity == Activity::Meeting && meetings >= 2;
        if !repeat && !too_many_meetings {
            return activity;
        }
    }
    Activity::Coding
}

fn pick_project(rng: &mut SplitMix64, activity: Activity, hot: &[usize]) -> Option<usize> {
    match activity {
        Activity::Admin => None,
        Activity::Design => Some(UI_PROJECTS[rng.below(UI_PROJECTS.len())]),
        _ => {
            if rng.chance(7, 10) {
                Some(hot[rng.below(hot.len())])
            } else {
                Some(rng.below(PROJECTS.len()))
            }
        }
    }
}

/// The project an anchor belongs to (resource anchors borrow a project of their language).
fn anchor_project(anchor: Anchor, rng: &mut SplitMix64) -> Option<usize> {
    match anchor {
        Anchor::Code(p)
        | Anchor::Review(p)
        | Anchor::Triage(p)
        | Anchor::Files(p)
        | Anchor::Site(p)
        | Anchor::Observability(_, p)
        | Anchor::Claude(p) => Some(p),
        Anchor::Resource(i) => {
            let matching: Vec<usize> = match RESOURCES[i].lang {
                Some(lang) => (0..PROJECTS.len())
                    .filter(|p| PROJECTS[*p].lang == lang)
                    .collect(),
                None => Vec::new(),
            };
            if matching.is_empty() {
                Some(rng.below(PROJECTS.len()))
            } else {
                Some(matching[rng.below(matching.len())])
            }
        }
        Anchor::AdminFolder(_) => None,
    }
}

/// Raw intra-session spacings, inter-session gaps and lunch, scaled to fit the day.
struct DayClock {
    spacings: Vec<Vec<i64>>,
    gaps: Vec<i64>,
    lunch: i64,
    start: i64,
}

fn day_clock(rng: &mut SplitMix64, midnight: i64, sizes: &[usize]) -> DayClock {
    let start = midnight
        + (DAY_START_MIN + rng.below(DAY_START_JITTER_MIN + 1) as i64) * MINUTE_MS
        + rng.below(60) as i64 * 1000;
    let mut spacings: Vec<Vec<i64>> = sizes
        .iter()
        .map(|n| {
            (1..*n)
                .map(|_| (90 + rng.below(451) as i64) * 1000 + rng.below(1000) as i64)
                .collect()
        })
        .collect();
    let mut gaps: Vec<i64> = (1..sizes.len())
        .map(|_| (240 + rng.below(1561) as i64) * 1000 + rng.below(1000) as i64)
        .collect();
    let mut lunch = (40 + rng.below(36) as i64) * MINUTE_MS;

    let total: i64 = spacings.iter().flatten().sum::<i64>() + gaps.iter().sum::<i64>() + lunch;
    let available = midnight + DAY_END_MIN * MINUTE_MS - start;
    if total > available {
        let scale =
            |v: i64| ((i128::from(v) * i128::from(available) / i128::from(total)) as i64).max(1000);
        for s in spacings.iter_mut().flatten() {
            *s = scale(*s);
        }
        for g in gaps.iter_mut() {
            *g = scale(*g);
        }
        lunch = scale(lunch);
    }
    DayClock {
        spacings,
        gaps,
        lunch,
        start,
    }
}

/// Builds the whole timeline for `(seed, weeks, frames_per_week)`.
pub(crate) fn plan(seed: u64, weeks: u32, frames_per_week: usize) -> Result<Plan> {
    let mut rng = SplitMix64::new(subseed(seed, "timeline"));
    let days = weeks as usize * 5;

    let mut day_sizes: Vec<Vec<usize>> = Vec::with_capacity(days);
    for _ in 0..weeks {
        for frames in split_week(&mut rng, frames_per_week) {
            day_sizes.push(split_day(&mut rng, frames));
        }
    }
    let total_sessions: usize = day_sizes.iter().map(Vec::len).sum();

    let mut anchors = all_anchors();
    rng.shuffle(&mut anchors);
    let mut anchors = std::collections::VecDeque::from(anchors);

    let mut sessions: Vec<SessionPlan> = Vec::with_capacity(total_sessions);
    let mut hot: Vec<usize> = Vec::new();
    for (day, sizes) in day_sizes.iter().enumerate() {
        if day % 5 == 0 {
            let mut order: Vec<usize> = (0..PROJECTS.len()).collect();
            rng.shuffle(&mut order);
            hot = order[..4].to_vec();
        }
        if sizes.is_empty() {
            continue;
        }
        let midnight = day_midnight_ms(day)?;
        let clock = day_clock(&mut rng, midnight, sizes);
        let mut cursor = clock.start;
        let mut lunched = false;
        let mut previous: Option<Activity> = None;
        let mut meetings = 0usize;

        for (s, size) in sizes.iter().enumerate() {
            if s > 0 {
                cursor += clock.gaps[s - 1];
                if !lunched && cursor >= midnight + NOON_MIN * MINUTE_MS {
                    cursor += clock.lunch;
                    lunched = true;
                }
            }
            let placed = sessions.len();
            let remaining = total_sessions - placed;
            let take_anchor = !anchors.is_empty()
                && (anchors.len() >= remaining || rng.below(remaining) < anchors.len());
            let (activity, project, anchor) = if take_anchor {
                let anchor = anchors.pop_front().expect("checked non-empty");
                (
                    anchor.activity(),
                    anchor_project(anchor, &mut rng),
                    Some(anchor),
                )
            } else {
                let activity = pick_activity(&mut rng, s == 0, previous, meetings);
                (activity, pick_project(&mut rng, activity, &hot), None)
            };
            if activity == Activity::Meeting {
                meetings += 1;
            }
            previous = Some(activity);

            let primary = primary_app(activity, &mut rng);
            let mut frames: Vec<FramePlan> = Vec::with_capacity(*size);
            for i in 0..*size {
                if i > 0 {
                    cursor += clock.spacings[s][i - 1];
                }
                let app = if i == 0 {
                    anchor.map_or(primary, Anchor::first_app)
                } else if rng.chance(65, 100) {
                    primary
                } else {
                    weighted(&mut rng, secondary_apps(activity))
                };
                frames.push(FramePlan {
                    at: cursor,
                    app,
                    anchor: if i == 0 { anchor } else { None },
                    echo: None,
                });
            }
            let last = cursor;
            let next_gap = clock.gaps.get(s).copied().unwrap_or(10 * MINUTE_MS);
            let tail = ((20 + rng.below(131) as i64) * 1000).min(next_gap / 2);

            let apps: Vec<App> = frames.iter().map(|f| f.app).collect();
            let (kind, tool, host, context_key, conf) = match activity {
                Activity::AiPairing => (
                    SessionKind::Ai,
                    Some("claude-code"),
                    Some(SessionHost::Terminal),
                    "ai:claude-code".to_string(),
                    confidence(&mut rng, 700, 250),
                ),
                Activity::BrowserAi => (
                    SessionKind::Ai,
                    Some("browser-ai"),
                    Some(SessionHost::Browser),
                    "ai:browser-ai".to_string(),
                    confidence(&mut rng, 700, 250),
                ),
                Activity::Meeting => (
                    SessionKind::Meeting,
                    None,
                    Some(SessionHost::Desktop),
                    "meeting:zoom".to_string(),
                    confidence(&mut rng, 700, 250),
                ),
                _ => (
                    SessionKind::Focus,
                    None,
                    None,
                    format!("focus:{}", dominant_app(&apps).hint().to_ascii_lowercase()),
                    confidence(&mut rng, 450, 200),
                ),
            };
            sessions.push(SessionPlan {
                activity,
                project,
                started_at: frames[0].at,
                ended_at: last + tail,
                frames,
                kind,
                tool,
                host,
                context_key,
                confidence: conf,
            });
        }
    }

    let echoes = assign_echoes(seed, &mut sessions);
    Ok(Plan { sessions, echoes })
}

/// Groups each project's frames into twos to fives sharing one distinctive line.
fn assign_echoes(seed: u64, sessions: &mut [SessionPlan]) -> Vec<String> {
    let mut rng = SplitMix64::new(subseed(seed, "echoes"));
    let mut echoes: Vec<String> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for p in 0..PROJECTS.len() {
        let mut slots: Vec<(usize, usize)> = Vec::new();
        for (s, session) in sessions.iter().enumerate() {
            if session.project == Some(p) {
                for f in 0..session.frames.len() {
                    if session.frames[f].anchor.is_none() {
                        slots.push((s, f));
                    }
                }
            }
        }
        rng.shuffle(&mut slots);
        let selected: Vec<(usize, usize)> =
            slots.into_iter().filter(|_| rng.chance(2, 3)).collect();
        let mut rest = selected.as_slice();
        while rest.len() >= 2 {
            let k = (2 + rng.below(4)).min(rest.len());
            let (group, tail) = rest.split_at(k);
            rest = tail;
            let text = loop {
                let candidate = echo_text(p, &mut rng);
                if used.insert(candidate.clone()) {
                    break candidate;
                }
            };
            let index = echoes.len();
            echoes.push(text);
            for (s, f) in group {
                sessions[*s].frames[*f].echo = Some(index);
            }
        }
    }
    echoes
}

const TENANTS: [&str; 6] = [
    "acme-eu",
    "globex-us",
    "initech-uk",
    "umbrella-de",
    "hooli-fr",
    "stark-nl",
];
const WEEKDAYS: [&str; 5] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"];

/// One distinctive line about project `p` (24..=160 chars, no double quote).
fn echo_text(p: usize, rng: &mut SplitMix64) -> String {
    let project = &PROJECTS[p];
    let v = project.vocab;
    let ticket = &project.tickets[rng.below(3)];
    match rng.below(6) {
        0 => format!(
            "ERROR {} {} {} exhausted tenant={} req={}",
            project.service,
            v[1],
            v[0],
            TENANTS[rng.below(TENANTS.len())],
            request_id(rng)
        ),
        1 => format!(
            "{} fix({}): reset the {} window on deploy ({})",
            short_hash(rng),
            v[0],
            v[2],
            ticket.key
        ),
        2 => format!(
            "Action item: {} to review {} before {} ({})",
            teammate(project, rng),
            project.docs[rng.below(3)],
            WEEKDAYS[rng.below(5)],
            ticket.key
        ),
        3 => format!(
            "[FIRING:1] High{}Rate {} prod value={}.{:03}",
            super::code::camel(v[1]),
            project.service,
            rng.below(2),
            rng.below(1000)
        ),
        4 => format!(
            "{} still reproduces on build {} with {} {} enabled",
            ticket.key,
            4000 + rng.below(6000),
            v[2],
            v[3]
        ),
        _ => format!(
            "panic: {} {} mismatch in {} line {} trace={}",
            v[0],
            v[3],
            project.file_name(rng.below(4)),
            10 + rng.below(90),
            request_id(rng)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_hold_exactly_their_frames_and_sessions_hold_two_or_more() {
        for frames in [2usize, 8, 9, 10, 37, 210, 1000] {
            let mut rng = SplitMix64::new(frames as u64);
            let split = split_week(&mut rng, frames);
            assert_eq!(split.iter().sum::<usize>(), frames, "{frames}");
            for day in split {
                let sizes = split_day(&mut rng, day);
                assert_eq!(sizes.iter().sum::<usize>(), if day < 2 { 0 } else { day });
                assert!(sizes.iter().all(|s| *s >= 2));
                if frames >= 210 {
                    assert!((6..=9).contains(&sizes.len()), "{day}: {}", sizes.len());
                }
            }
        }
    }

    #[test]
    fn the_timeline_is_ordered_and_echoes_recur_two_to_five_times() {
        let plan = plan(7, 2, 210).unwrap();
        let mut last = i64::MIN;
        for s in &plan.sessions {
            assert!(s.frames.len() >= 2);
            assert!(s.started_at > last, "sessions start strictly in order");
            for f in &s.frames {
                assert!(f.at > last);
                last = f.at;
            }
            assert!(s.ended_at >= last);
            let t = local(s.started_at);
            assert!(chrono::Timelike::hour(&t) >= 8 && chrono::Timelike::hour(&t) < 19);
        }
        let mut counts = vec![0usize; plan.echoes.len()];
        for s in &plan.sessions {
            for f in &s.frames {
                if let Some(e) = f.echo {
                    counts[e] += 1;
                }
            }
        }
        assert!(!counts.is_empty());
        assert!(counts.iter().all(|c| (2..=5).contains(c)), "{counts:?}");
        for e in &plan.echoes {
            assert!(
                (24..=160).contains(&e.chars().count()) && !e.contains('"'),
                "{e}"
            );
        }
    }

    #[test]
    fn a_full_size_fixture_places_every_anchor() {
        let plan = plan(crate::DEFAULT_SEED, 6, 210).unwrap();
        let placed = plan
            .sessions
            .iter()
            .filter(|s| s.frames[0].anchor.is_some())
            .count();
        assert_eq!(placed, anchor_count());
    }
}
