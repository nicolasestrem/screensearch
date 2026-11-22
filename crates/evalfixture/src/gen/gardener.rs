//! The market gardener's four-week replay. Authored windows go through the normal
//! persona Store and OCR path. The score key is written beside the completed fixture.

mod story;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{ensure, Context, Result};
use chrono::{Days, FixedOffset, NaiveDate, TimeZone};
use serde::Serialize;
use traits::{SessionHost, SessionKind};

use super::layout::Screen;
use super::life::{LifeFrame, LifePlan, LifeSession};
use super::personas::Persona;
use super::prng::{subseed, SplitMix64};
use super::GenOptions;

pub(crate) const PERSONA_ID: &str = "market-gardener";
pub(crate) const WEEKS: u32 = 4;
const DAYS_PER_WEEK: u64 = 7;

pub(crate) struct Month {
    pub(crate) plan: LifePlan,
    screens: BTreeMap<i64, Screen>,
    planted: Planted,
}

struct Scene {
    day: u64,
    minute: u32,
    span_minutes: u32,
    weight: usize,
    primary: bool,
    screen: Screen,
}

#[derive(Serialize)]
struct Planted {
    persona: &'static str,
    start: String,
    storylines: Vec<Storyline>,
    distant_links: Vec<DistantLink>,
}

#[derive(Serialize)]
struct Storyline {
    id: &'static str,
    title: &'static str,
    beats: Vec<Beat>,
}

#[derive(Serialize)]
struct Beat {
    stage: &'static str,
    #[serde(flatten)]
    window: Anchor,
}

#[derive(Serialize)]
struct DistantLink {
    id: &'static str,
    first: Anchor,
    second: Anchor,
    connection: &'static str,
}

#[derive(Serialize)]
struct Anchor {
    date: String,
    app: &'static str,
    window: String,
    detail: &'static str,
    evidence: String,
}

impl Month {
    pub(crate) fn new(opts: &GenOptions, persona: &Persona) -> Result<Self> {
        let calendar = story::calendar(opts.start)?;
        let planted = story::planted(opts.start, &calendar)?;
        let offset = persona.offset()?;
        let mut rng = SplitMix64::new(subseed(opts.seed, "gardener-month"));
        let mut weeks: [Vec<Scene>; WEEKS as usize] = Default::default();
        for scene in calendar {
            weeks[(scene.day / DAYS_PER_WEEK) as usize].push(scene);
        }

        let mut sessions = Vec::new();
        let mut screens = BTreeMap::new();
        for mut week in weeks {
            // Secure every day's principal window before admitting ancillary work.
            week.sort_by_key(|scene| (!scene.primary, scene.day, scene.minute));
            let required = week.iter().filter(|scene| scene.primary).count();
            let scene_limit = opts.frames_per_week / 2;
            ensure!(
                scene_limit >= required,
                "the gardener month needs at least {} frames per week",
                required * 2
            );
            week.truncate(scene_limit);
            week.sort_by_key(|scene| (scene.day, scene.minute));
            let budgets = distribute_frames(&week, opts.frames_per_week, &mut rng);
            for (scene, frames) in week.into_iter().zip(budgets) {
                let session = scene.session(opts.start, offset, persona, frames, &mut rng)?;
                screens.insert(session.started_at, scene.screen);
                sessions.push(session);
            }
        }
        Ok(Self {
            plan: LifePlan { sessions },
            screens,
            planted,
        })
    }

    pub(crate) fn screen(&self, session_start: i64) -> Result<&Screen> {
        self.screens
            .get(&session_start)
            .with_context(|| format!("missing gardener window for session at {session_start}"))
    }

    pub(crate) fn write_planted(&self, out_dir: &Path) -> Result<()> {
        let path = out_dir.join("PLANTED.json");
        let mut json = serde_json::to_vec_pretty(&self.planted).context("encoding score key")?;
        json.push(b'\n');
        std::fs::write(&path, json)
            .with_context(|| format!("writing gardener score key {}", super::slash(&path)))
    }
}

/// Quiet scenes keep two frames. Busier scenes divide the remaining frame budget
/// by weight, with seeded rounding so the requested frame count is always exact.
fn distribute_frames(scenes: &[Scene], total: usize, rng: &mut SplitMix64) -> Vec<usize> {
    let remaining = total - 2 * scenes.len();
    let weight: usize = scenes.iter().map(|scene| scene.weight).sum();
    let mut budgets: Vec<usize> = scenes
        .iter()
        .map(|scene| 2 + remaining * scene.weight / weight)
        .collect();
    let extra = total - budgets.iter().sum::<usize>();
    let mut active: Vec<_> = (0..scenes.len())
        .filter(|&i| scenes[i].weight > 0)
        .collect();
    rng.shuffle(&mut active);
    for i in active.into_iter().take(extra) {
        budgets[i] += 1;
    }
    budgets
}

fn date(start: NaiveDate, day: u64) -> Result<NaiveDate> {
    start
        .checked_add_days(Days::new(day))
        .context("gardener fixture date overflow")
}

impl Scene {
    fn session(
        &self,
        start: NaiveDate,
        offset: FixedOffset,
        persona: &Persona,
        frame_count: usize,
        rng: &mut SplitMix64,
    ) -> Result<LifeSession> {
        let app = persona
            .apps
            .iter()
            .position(|app| app.hint == self.screen.app_hint)
            .with_context(|| format!("unknown gardener app {}", self.screen.app_hint))?;
        let minute = self.minute + rng.below(7) as u32;
        let local = date(start, self.day)?
            .and_hms_opt(minute / 60, minute % 60, 0)
            .context("invalid gardener scene start time")?;
        let started_at = offset
            .from_local_datetime(&local)
            .single()
            .context("invalid gardener scene timestamp")?
            .timestamp_millis();
        let span_ms = i64::from(self.span_minutes) * 60_000;
        let frames = (0..frame_count)
            .map(|i| LifeFrame {
                at: started_at + span_ms * i as i64 / (frame_count - 1) as i64,
                app,
                monitor: 0,
            })
            .collect();
        let host = if self.screen.url.is_some() {
            SessionHost::Browser
        } else {
            SessionHost::Desktop
        };
        Ok(LifeSession {
            context: None,
            frames,
            started_at,
            ended_at: started_at + span_ms + 60_000,
            kind: SessionKind::Focus,
            tool: None,
            host: Some(host),
            context_key: format!("focus:{}", self.screen.app_hint),
            confidence: 0.6,
        })
    }
}
