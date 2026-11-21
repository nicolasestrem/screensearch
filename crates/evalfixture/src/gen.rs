//! `gen` - the seeded synthetic v15 history.
//!
//! Builds `<out>/screensearch.db` through the production Store writers only (no hand-written
//! SQL, no extra tables): [`SqliteStore::open_path`] creates the baseline, every frame is
//! written with `insert_frame` and its screen text with `insert_ocr_filtered` (so
//! `content_text` is the real attention-filter output), sessions with `insert_session` and
//! `assign_frames_session`, the API bearer token with `set_setting`, and optional text
//! embeddings with `upsert_text_embedding`.
//!
//! The world (projects, people, sites; [`world_entities`]) is a fixed catalog; the seed
//! drives every sampling decision through one SplitMix64 per purpose. Nothing reads the
//! clock: the timeline starts on the fixed Monday 2026-09-07 in a fixed UTC+02:00 offset
//! (see `gen/plan.rs`). The printed `digest` is a blake3 over a canonical dump of the stored
//! frame and session rows, so two runs with the same arguments print the same digest.
//!
//! ```text
//! ssv2c-evalfixture gen --out harness-data/fixture
//! ```

mod code;
mod layout;
mod life;
pub(crate) mod personas;
mod plan;
mod prng;
mod render;
mod screens;
mod world;

pub use world::{world_entities, WorldEntities};

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::{Datelike, NaiveDate, Weekday};
use embeddings::FastEmbedProvider;
use store::SqliteStore;
use traits::{
    CaptureTrigger, ChunkSource, EmbeddingProvider, EvidenceWindow, FrameContinuation,
    FrameFilters, FramePageRequest, NewFrame, NewSession, Session, SessionFilter,
    FRAME_PAGE_MAX_LIMIT,
};

use crate::{API_TOKEN_SETTING, DEFAULT_FRAMES_PER_WEEK, DEFAULT_SEED, DEFAULT_WEEKS};
use layout::lay_out;
use life::LifePlan;
use personas::{Persona, PERSONA_SETTING};
use plan::Plan;
use prng::{subseed, SplitMix64};
use render::{Renderer as LifeRenderer, Sticky as LifeSticky};
use screens::{clock, Renderer, Sticky};

/// Fixture size bounds. The week/density limits are generous enough for a multi-year history;
/// the total-frame ceiling is the real safety rail: at the measured ~4 ms and ~9 KB per frame,
/// two million frames is already roughly three hours and eighteen gigabytes, so a larger run is
/// not a test fixture. The day planner (which compresses spacing for dense fixtures) still
/// ends every day before 19:00 at the density limit.
const MAX_WEEKS: u32 = 1040;
const MIN_FRAMES_PER_WEEK: usize = 2;
const MAX_FRAMES_PER_WEEK: usize = 50_000;
const MAX_TOTAL_FRAMES: u64 = 2_000_000;

/// The earliest supported `--start` (a Monday). The store records Unix milliseconds and
/// `frame_page` reads history from 0 upward, so a pre-epoch fixture can be written but never
/// read back. Rejecting it up front keeps the failure honest (and leaves no unusable database
/// behind). 1970-01-05 is the first Monday at or after the Unix epoch.
const MIN_START: (i32, u32, u32) = (1970, 1, 5);

/// Embedding every frame is the default only while the fixture stays at this size. A larger
/// run must choose `--no-embeddings` or an explicit `--embed-frames` bound: embedding is a
/// model call per frame, so an unbounded default would turn a fixture run into an hours-long
/// job nobody asked for.
const EMBED_ALL_LIMIT: u64 = 20_000;

/// Display size of every synthetic capture (single 1080p monitor).
const FRAME_WIDTH: u32 = 1920;
const FRAME_HEIGHT: u32 = 1080;

const USAGE: &str = "\
USAGE:
  ssv2c-evalfixture gen --out <dir> [--seed <n>] [--start <YYYY-MM-DD>] [--weeks <n>]
                        [--frames-per-week <n>] [--no-embeddings | --embed-frames <n>]
                        [--persona <id> | --persona random]
  ssv2c-evalfixture gen --list-personas

OPTIONS:
  --out <dir>              Fixture directory (required). The database is <dir>/screensearch.db;
                           it must not exist yet (a rerun never appends to an old fixture).
  --seed <n>               PRNG seed, decimal or 0x-prefixed hex (default 0x5cee55ee41520004).
  --start <YYYY-MM-DD>     First working day; must be a Monday on or after 1970-01-05
                           (default 2026-09-07).
  --weeks <n>              Working weeks to synthesize, 1..=1040 (default 6).
  --frames-per-week <n>    Frames per week, 2..=50000 (default 210). One fixture may hold at
                           most 2000000 frames in total.
  --no-embeddings          Skip text embeddings: no model is loaded or downloaded, and the
                           summary reports embeddings=0.
  --embed-frames <n>       Embed only the first n frames that have content text (1..=the
                           run's frame count) instead of every frame. The default embeds every
                           frame only up to 20000 frames; a larger run must choose
                           --no-embeddings or a bound. Text embeddings use the product's
                           fastembed model, cached under <dir>/models/fastembed (downloaded
                           on first use).
  --persona <id>           Generate one catalog persona's life instead of the pinned default
                           fixture. `--persona random` picks one from the seed. The persona's
                           own language, country, timezone, rhythm, apps and content are used.
                           Every persona runs on Windows: the product is Windows-only, so the
                           fixture never models another desktop. The pinned default digest is
                           unaffected when the flag is absent.
  --list-personas          Print one line per catalog persona (`id<TAB>description`) and exit.
";

/// The first working day a run uses when `--start` is absent (Monday 2026-09-07).
pub fn default_start() -> NaiveDate {
    plan::default_start()
}

/// Parsed `gen` arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenOptions {
    pub out_dir: PathBuf,
    pub seed: u64,
    pub start: NaiveDate,
    pub weeks: u32,
    pub frames_per_week: usize,
    pub embeddings: bool,
    /// Embed only the first `n` frames that have content text; `None` embeds every frame
    /// (when embeddings are on).
    pub embed_frames: Option<usize>,
    /// The catalog persona to generate, or `None` for the pinned default fixture. `--persona
    /// random` is resolved to a concrete id at parse time from the seed.
    pub persona: Option<&'static str>,
}

impl GenOptions {
    /// The exact number of frames this run will write (`weeks` × `frames_per_week`).
    pub fn total_frames(&self) -> u64 {
        u64::from(self.weeks) * self.frames_per_week as u64
    }

    /// Parses the arguments after `gen`. Unknown flags and missing or malformed values are
    /// errors naming the flag, followed by the usage text.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut out_dir: Option<PathBuf> = None;
        let mut seed = DEFAULT_SEED;
        let mut start = default_start();
        let mut weeks = DEFAULT_WEEKS;
        let mut frames_per_week = DEFAULT_FRAMES_PER_WEEK;
        let mut embeddings = true;
        let mut embed_frames: Option<usize> = None;
        let mut persona_arg: Option<String> = None;

        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let mut value = |flag: &str| -> Result<String> {
                iter.next()
                    .cloned()
                    .ok_or_else(|| anyhow!("`{flag}` needs a value\n\n{USAGE}"))
            };
            match arg.as_str() {
                "--out" => out_dir = Some(PathBuf::from(value("--out")?)),
                "--seed" => seed = parse_seed(&value("--seed")?)?,
                "--start" => start = parse_start(&value("--start")?)?,
                "--weeks" => weeks = parse_number("--weeks", &value("--weeks")?)?,
                "--frames-per-week" => {
                    frames_per_week =
                        parse_number("--frames-per-week", &value("--frames-per-week")?)?
                }
                "--no-embeddings" => embeddings = false,
                "--embed-frames" => {
                    embed_frames = Some(parse_number("--embed-frames", &value("--embed-frames")?)?)
                }
                "--persona" => persona_arg = Some(value("--persona")?),
                other => bail!("unknown flag `{other}`\n\n{USAGE}"),
            }
        }
        let persona: Option<&'static str> =
            match persona_arg.as_deref() {
                None => None,
                Some("random") => Some(personas::random(seed).id),
                Some(id) => Some(personas::find(id).map(|persona| persona.id).ok_or_else(
                    || {
                        anyhow!(
                            "unknown persona `{id}`; run `ssv2c-evalfixture gen --list-personas` \
                             to see the catalog\n\n{USAGE}"
                        )
                    },
                )?),
            };
        let opts = Self {
            out_dir: out_dir.ok_or_else(|| anyhow!("`--out <dir>` is required\n\n{USAGE}"))?,
            seed,
            start,
            weeks,
            frames_per_week,
            embeddings,
            embed_frames,
            persona,
        };
        opts.validate()?;
        if opts.persona.is_some() {
            personas::validate_all()?;
        }
        Ok(opts)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_WEEKS).contains(&self.weeks),
            "`--weeks` must be within 1..={MAX_WEEKS}, got {}\n\n{USAGE}",
            self.weeks
        );
        ensure!(
            (MIN_FRAMES_PER_WEEK..=MAX_FRAMES_PER_WEEK).contains(&self.frames_per_week),
            "`--frames-per-week` must be within {MIN_FRAMES_PER_WEEK}..={MAX_FRAMES_PER_WEEK} \
             (a session needs two frames), got {}\n\n{USAGE}",
            self.frames_per_week
        );
        let total = self.total_frames();
        ensure!(
            total <= MAX_TOTAL_FRAMES,
            "`--weeks` x `--frames-per-week` would generate {total} frames, above the \
             {MAX_TOTAL_FRAMES}-frame ceiling for one fixture; shorten the span or lower the \
             density, or split it into several fixtures\n\n{USAGE}"
        );
        ensure!(
            self.start.weekday() == Weekday::Mon,
            "`--start` must be a Monday (the fixture lays out five working days per week), got \
             {} ({})\n\n{USAGE}",
            self.start,
            self.start.weekday()
        );
        let earliest = NaiveDate::from_ymd_opt(MIN_START.0, MIN_START.1, MIN_START.2)
            .expect("the earliest supported start is a valid date");
        ensure!(
            self.start >= earliest,
            "`--start` must be on or after {earliest} (the store reads history from the Unix \
             epoch, so an earlier fixture could not be read back), got {}\n\n{USAGE}",
            self.start
        );
        if !self.embeddings && self.embed_frames.is_some() {
            bail!(
                "`--embed-frames` cannot be combined with `--no-embeddings`: choose one\n\n{USAGE}"
            );
        }
        if let Some(n) = self.embed_frames {
            ensure!(
                (1..=total as usize).contains(&n),
                "`--embed-frames` must be within 1..={total} for this run, got {n}\n\n{USAGE}"
            );
        }
        if self.embeddings && self.embed_frames.is_none() {
            ensure!(
                total <= EMBED_ALL_LIMIT,
                "this run generates {total} frames; embedding every frame is limited to \
                 {EMBED_ALL_LIMIT} by default. Pass `--no-embeddings`, or `--embed-frames <n>` \
                 to embed a bounded prefix\n\n{USAGE}"
            );
        }
        Ok(())
    }
}

fn parse_seed(raw: &str) -> Result<u64> {
    let clean = raw.replace('_', "");
    let parsed = match clean
        .strip_prefix("0x")
        .or_else(|| clean.strip_prefix("0X"))
    {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => clean.parse::<u64>(),
    };
    parsed.map_err(|_| anyhow!("`--seed` expects a decimal or 0x-hex u64, got `{raw}`\n\n{USAGE}"))
}

fn parse_start(raw: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| anyhow!("`--start` expects an ISO date (YYYY-MM-DD), got `{raw}`\n\n{USAGE}"))
}

fn parse_number<T: std::str::FromStr>(flag: &str, raw: &str) -> Result<T> {
    raw.parse::<T>()
        .map_err(|_| anyhow!("`{flag}` expects a non-negative integer, got `{raw}`\n\n{USAGE}"))
}

/// What a generation run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenManifest {
    pub db: PathBuf,
    pub seed: u64,
    /// First working day of the synthetic timeline.
    pub start: NaiveDate,
    pub weeks: u32,
    pub frames: u64,
    pub sessions: u64,
    pub embeddings: u64,
    /// The catalog persona this run generated, or `None` for the pinned default fixture.
    pub persona: Option<&'static str>,
    /// blake3 (hex) over the canonical dump of every stored frame and session row.
    pub digest: String,
}

impl GenManifest {
    /// The stable printed summary: `seed=`, optionally `persona=`, `start=`, `weeks=`,
    /// `frames=`, `sessions=`, `embeddings=`, `digest=`, `db=`, one per line, in that order
    /// (the path with forward slashes). The pinned default fixture's summary is unchanged.
    pub fn summary(&self) -> String {
        let mut summary = format!("seed=0x{:016x}\n", self.seed);
        if let Some(persona) = self.persona {
            summary.push_str(&format!("persona={persona}\n"));
        }
        summary.push_str(&format!(
            "start={}\nweeks={}\nframes={}\nsessions={}\nembeddings={}\ndigest={}\ndb={}\n",
            self.start,
            self.weeks,
            self.frames,
            self.sessions,
            self.embeddings,
            self.digest,
            self.db.display().to_string().replace('\\', "/")
        ));
        summary
    }
}

/// `gen` entry point: parse, generate, print the summary. `--list-personas` prints the
/// catalog without needing `--out`.
pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--list-personas") {
        print!("{}", personas::list_lines()?);
        return Ok(());
    }
    let opts = GenOptions::parse(args)?;
    let manifest = generate(&opts)?;
    print!("{}", manifest.summary());
    Ok(())
}

/// Generates the fixture described by `opts`.
///
/// Synchronous: it builds its own current-thread Tokio runtime for the async Store API, so it
/// must be called from synchronous code (a plain `#[test]`, `spawn_blocking`, the CLI), never
/// from inside another runtime.
///
/// The **plan** (every session and frame) is still materialized in memory before writing:
/// roughly 50 bytes per frame, about 100 MB at the 2,000,000-frame ceiling. Only the read-back
/// content text is page-bounded. Streaming the plan per week would be a separate change.
pub fn generate(opts: &GenOptions) -> Result<GenManifest> {
    opts.validate()?;
    if tokio::runtime::Handle::try_current().is_ok() {
        bail!(
            "gen::generate builds its own Tokio runtime; call it from synchronous code \
             (for example through tokio::task::spawn_blocking), not from inside a runtime"
        );
    }
    let db = crate::db_path(&opts.out_dir);
    refuse_existing(&db)?;
    std::fs::create_dir_all(&opts.out_dir)
        .with_context(|| format!("creating {}", slash(&opts.out_dir)))?;

    // Load the embedder before writing anything: a model that cannot load fails the run
    // up front instead of leaving a fixture without the vectors it was asked for.
    let embedder =
        if opts.embeddings {
            let cache = opts.out_dir.join("models").join("fastembed");
            Some(FastEmbedProvider::new(cache.clone()).with_context(|| {
                format!("loading the text embedding model into {}", slash(&cache))
            })?)
        } else {
            None
        };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building the generator runtime")?;
    match opts.persona {
        None => {
            let plan = plan::plan(opts.seed, opts.weeks, opts.frames_per_week, opts.start)?;
            runtime.block_on(write_fixture(opts, &db, &plan, embedder.as_ref()))
        }
        Some(id) => {
            let persona = personas::find(id)
                .with_context(|| format!("persona `{id}` is not in the catalog"))?;
            personas::validate(persona)?;
            let life_plan = life::plan(
                opts.seed,
                opts.weeks,
                opts.frames_per_week,
                opts.start,
                persona,
            )?;
            runtime.block_on(write_persona_fixture(
                opts,
                &db,
                &life_plan,
                persona,
                embedder.as_ref(),
            ))
        }
    }
}

/// A fixture directory is written once. Any existing database file (or its WAL/journal
/// sidecars, which SQLite would replay) means an earlier fixture lives here.
fn refuse_existing(db: &Path) -> Result<()> {
    let name = db
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    for candidate in [
        db.to_path_buf(),
        db.with_file_name(format!("{name}-wal")),
        db.with_file_name(format!("{name}-journal")),
    ] {
        if candidate.exists() {
            bail!(
                "{} already exists: pass a fresh --out directory (a rerun never appends to an \
                 existing fixture)",
                slash(&candidate)
            );
        }
    }
    Ok(())
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// The production capture loop's day-sharded image path (`kernel/src/capture_loop.rs`).
/// The fixture writes rows only; no image file exists behind the path.
fn image_path(captured_at: i64) -> String {
    let day = captured_at.div_euclid(86_400_000);
    format!("frames/day-{day}/{captured_at}-0.webp")
}

/// The same layout for a persona frame on `monitor`; multi-monitor personas write the
/// second display's index into the file name the way the capture loop does.
fn image_path_on(captured_at: i64, monitor: u8) -> String {
    let day = captured_at.div_euclid(86_400_000);
    format!("frames/day-{day}/{captured_at}-{monitor}.webp")
}

/// A synthetic API bearer token: 32 bytes of the seeded stream as hex, the shape the product
/// generates (`api::generate_token`). Never a real credential.
fn api_token(seed: u64) -> String {
    let mut rng = SplitMix64::new(subseed(seed, API_TOKEN_SETTING));
    (0..4).map(|_| format!("{:016x}", rng.next_u64())).collect()
}

async fn write_fixture(
    opts: &GenOptions,
    db: &Path,
    plan: &Plan,
    embedder: Option<&FastEmbedProvider>,
) -> Result<GenManifest> {
    let store = SqliteStore::open_path(db).with_context(|| format!("creating {}", slash(db)))?;
    let mut renderer = Renderer::new(opts.seed);
    let total_frames = opts.total_frames() as usize;
    let mut written = 0usize;

    for session in &plan.sessions {
        let session_id = store
            .insert_session(NewSession {
                started_at: session.started_at,
                ended_at: Some(session.ended_at),
                kind: session.kind,
                tool: session.tool.map(str::to_string),
                host: session.host,
                context_key: session.context_key.clone(),
                confidence: session.confidence,
                // History weeks old: the segmenter's freeze rule would have frozen it.
                frozen: true,
            })
            .await
            .with_context(|| format!("inserting session starting at {}", session.started_at))?;
        let mut sticky = Sticky::default();
        let mut frame_ids = Vec::with_capacity(session.frames.len());
        let mut previous_app = None;
        for frame in &session.frames {
            let echo = frame.echo.map(|e| plan.echoes[e].as_str());
            let screen = renderer.render(session, frame, echo, &mut sticky);
            let (time, date) = clock(frame.at);
            let laid = lay_out(&screen, (&time, &date))?;
            let trigger = if previous_app == Some(frame.app) {
                CaptureTrigger::Timer
            } else {
                CaptureTrigger::ForegroundChange
            };
            previous_app = Some(frame.app);
            let frame_id = store
                .insert_frame(NewFrame {
                    captured_at: frame.at,
                    monitor_index: 0,
                    width: FRAME_WIDTH,
                    height: FRAME_HEIGHT,
                    image_path: image_path(frame.at),
                    content_hash: blake3::hash(laid.ocr.text.as_bytes()).to_hex().to_string(),
                    app_hint: Some(screen.app_hint.to_string()),
                    window_title: Some(screen.title.clone()),
                    browser_url: screen.url.clone(),
                    capture_trigger: Some(trigger),
                })
                .await
                .with_context(|| format!("inserting frame {} of session {session_id}", frame.at))?;
            store
                .insert_ocr_filtered(frame_id, laid.ocr, laid.ctx)
                .await
                .with_context(|| format!("inserting text for frame {frame_id}"))?;
            frame_ids.push(frame_id);
            written += 1;
            if written.is_multiple_of(10_000) {
                eprintln!("wrote {written}/{total_frames} frames");
            }
        }
        let assigned = store
            .assign_frames_session(&frame_ids, Some(session_id))
            .await
            .with_context(|| {
                format!(
                    "assigning {} frames to session {session_id}",
                    frame_ids.len()
                )
            })?;
        ensure!(
            assigned == frame_ids.len() as u64,
            "session {session_id}: assigned {assigned} of {} frames",
            frame_ids.len()
        );
    }
    ensure!(
        written == total_frames,
        "wrote {written} frames, the plan expected {total_frames}"
    );

    // Only the token: the API's other defaults (`retrieval.default_top_k`, the answer
    // thinking toggle) are applied by the settings loader when the key is absent.
    store
        .set_setting(API_TOKEN_SETTING, &api_token(opts.seed))
        .await?;

    let embed_limit = if opts.embeddings {
        opts.embed_frames.unwrap_or(total_frames)
    } else {
        0
    };
    let (frames, digest, embedded) = finish_fixture(&store, embedder, embed_limit).await?;
    let sessions = read_sessions(&store).await?;
    ensure!(
        frames == total_frames as u64 && sessions.len() == plan.sessions.len(),
        "read back {frames} frames / {} sessions, wrote {total_frames} / {}",
        sessions.len(),
        plan.sessions.len()
    );

    Ok(GenManifest {
        db: db.to_path_buf(),
        seed: opts.seed,
        start: opts.start,
        weeks: opts.weeks,
        frames,
        sessions: sessions.len() as u64,
        embeddings: embedded,
        persona: None,
        digest,
    })
}

/// Writes a persona run: the same production Store writers, the same digest and the same
/// read-back, but every frame is rendered from the persona's data by [`life::LifePlan`] and
/// `gen::render`. The persona's identity is written to the `evalfixture.persona` setting so
/// `ssv2c-evalfixture export` can report it; the pinned default fixture writes no such key and
/// stays byte-identical. Persona runs also refuse a frame budget that cannot give every day
/// of the persona's widest weekly pattern its sessions, and suppress edge chrome from its
/// first sighting: the run places every short edge line itself, so there is no honest
/// cold-start window to honour.
async fn write_persona_fixture(
    opts: &GenOptions,
    db: &Path,
    plan: &LifePlan,
    persona: &'static Persona,
    embedder: Option<&FastEmbedProvider>,
) -> Result<GenManifest> {
    // `life::split_week` activates only `(frames / 2)` of a week's pattern days, so a
    // run that wants every weekday of the widest pattern live every week needs at least
    // two frames for each of those days. Refuse the quietly lossy run instead of
    // generating a persona whose story silently drops days.
    let widest = persona
        .rhythm
        .patterns
        .iter()
        .map(|pattern| pattern.len())
        .max()
        .unwrap_or(0);
    ensure!(
        opts.frames_per_week >= 2 * widest,
        "persona `{}` needs --frames-per-week >= {} so every day of its widest weekly \
         pattern holds sessions, got {}",
        persona.id,
        2 * widest,
        opts.frames_per_week
    );
    let store = SqliteStore::open_path(db).with_context(|| format!("creating {}", slash(db)))?;
    let mut renderer = LifeRenderer::new(opts.seed, persona);
    let total_frames = opts.total_frames() as usize;
    let mut written = 0usize;
    let offset = persona.offset()?;

    for session in &plan.sessions {
        let session_id = store
            .insert_session(NewSession {
                started_at: session.started_at,
                ended_at: Some(session.ended_at),
                kind: session.kind,
                tool: session.tool.map(str::to_string),
                host: session.host,
                context_key: session.context_key.clone(),
                confidence: session.confidence,
                frozen: true,
            })
            .await
            .with_context(|| format!("inserting session starting at {}", session.started_at))?;
        let mut sticky = LifeSticky::default();
        let mut frame_ids = Vec::with_capacity(session.frames.len());
        let mut previous_app = None;
        for frame in &session.frames {
            let mut screen = render::render(&mut renderer, session, frame, &mut sticky);
            // A body line that repeats the window title would be dropped by the attention
            // filter as a title echo; generated prose must not silently lose content.
            let title = traits::normalize_text(&screen.title);
            screen
                .body
                .retain(|line| traits::normalize_text(line) != title);
            let (time, date) = life::clock(frame.at, offset, persona.labels);
            let mut laid = lay_out(&screen, (&time, &date))?;
            // The fixture is written in one seeded pass and every short edge line is
            // static chrome the generator itself placed, so there is no honest
            // cold-start window to honour: suppress from the first sighting (the live
            // app defaults to 9). Long edge lines are never suppressible, which is why
            // the renderer keeps every edge line under `chrome_protect_min_chars`.
            laid.ctx.chrome_suppress_min_seen = 1;
            let trigger = if previous_app == Some(frame.app) {
                CaptureTrigger::Timer
            } else {
                CaptureTrigger::ForegroundChange
            };
            previous_app = Some(frame.app);
            let monitor = persona
                .monitors
                .get(frame.monitor as usize)
                .copied()
                .unwrap_or(persona.monitors[0]);
            let frame_id = store
                .insert_frame(NewFrame {
                    captured_at: frame.at,
                    monitor_index: u32::from(frame.monitor),
                    width: monitor.width,
                    height: monitor.height,
                    image_path: image_path_on(frame.at, frame.monitor),
                    content_hash: blake3::hash(laid.ocr.text.as_bytes()).to_hex().to_string(),
                    app_hint: Some(screen.app_hint.to_string()),
                    window_title: Some(screen.title.clone()),
                    browser_url: screen.url.clone(),
                    capture_trigger: Some(trigger),
                })
                .await
                .with_context(|| {
                    format!(
                        "inserting persona frame {} of session {session_id}",
                        frame.at
                    )
                })?;
            store
                .insert_ocr_filtered(frame_id, laid.ocr, laid.ctx)
                .await
                .with_context(|| format!("inserting text for frame {frame_id}"))?;
            frame_ids.push(frame_id);
            written += 1;
            if written.is_multiple_of(10_000) {
                eprintln!("wrote {written}/{total_frames} frames");
            }
        }
        let assigned = store
            .assign_frames_session(&frame_ids, Some(session_id))
            .await
            .with_context(|| {
                format!(
                    "assigning {} frames to session {session_id}",
                    frame_ids.len()
                )
            })?;
        ensure!(
            assigned == frame_ids.len() as u64,
            "session {session_id}: assigned {assigned} of {} frames",
            frame_ids.len()
        );
    }
    ensure!(
        written == total_frames,
        "wrote {written} frames, the plan expected {total_frames}"
    );

    store
        .set_setting(API_TOKEN_SETTING, &api_token(opts.seed))
        .await?;
    let metadata =
        serde_json::to_string(&persona.metadata()).context("serializing the persona identity")?;
    store.set_setting(PERSONA_SETTING, &metadata).await?;

    let embed_limit = if opts.embeddings {
        opts.embed_frames.unwrap_or(total_frames)
    } else {
        0
    };
    let (frames, digest, embedded) = finish_fixture(&store, embedder, embed_limit).await?;
    let sessions = read_sessions(&store).await?;
    ensure!(
        frames == total_frames as u64 && sessions.len() == plan.sessions.len(),
        "read back {frames} frames / {} sessions, wrote {total_frames} / {}",
        sessions.len(),
        plan.sessions.len()
    );

    Ok(GenManifest {
        db: db.to_path_buf(),
        seed: opts.seed,
        start: opts.start,
        weeks: opts.weeks,
        frames,
        sessions: sessions.len() as u64,
        embeddings: embedded,
        persona: Some(persona.id),
        digest,
    })
}

/// Reads the fixture back page by page: hashes every stored frame and session with the digest
/// algorithm, and embeds up to `embed_limit` non-empty frames. Returns
/// `(frames, digest, embeddings)`.
///
/// Streaming bounds the **read-back** memory by one page, so a multi-million-frame fixture
/// does not hold its own content text in RAM. The plan is still materialized in full (see
/// [`generate`]). `frame_page` returns frames in `(captured_at, id)` order and the fixture
/// writes them chronologically, so that is also ascending id — the order the digest's previous
/// read-all implementation sorted into. Any other order is a bug, not a reshuffle: the digest
/// would silently change, so the loop refuses it.
async fn finish_fixture(
    store: &SqliteStore,
    embedder: Option<&FastEmbedProvider>,
    embed_limit: usize,
) -> Result<(u64, String, u64)> {
    let model = embedder.map(|provider| provider.text_model_name().to_string());
    if let Some(model) = &model {
        eprintln!("embedding up to {embed_limit} frames with {model}");
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"evalfixture-digest v1\n");
    let window = EvidenceWindow {
        from: 0,
        to: i64::MAX,
    };
    let mut continuation: Option<FrameContinuation> = None;
    let mut frames = 0u64;
    let mut embedded = 0u64;
    let mut last_id = 0i64;
    loop {
        let page = store
            .frame_page(FramePageRequest {
                window,
                filters: FrameFilters::default(),
                limit: FRAME_PAGE_MAX_LIMIT,
                continuation: continuation.take(),
            })
            .await
            .context("reading back the fixture frames")?;
        for frame in &page.items {
            ensure!(
                frame.frame_id > last_id,
                "frame {} follows {last_id}: the fixture's frame ids must ascend across pages",
                frame.frame_id
            );
            last_id = frame.frame_id;
            record(
                &mut hasher,
                &[
                    "frame".to_string(),
                    frame.frame_id.to_string(),
                    frame.captured_at.to_string(),
                    field(frame.app_hint.as_deref()),
                    field(frame.window_title.as_deref()),
                    field(frame.browser_url.as_deref()),
                    field(frame.content_text.as_deref()),
                    field(frame.session_id.map(|id| id.to_string()).as_deref()),
                ],
            );
            frames += 1;

            // Embedding writes only the `embeddings` table; the page's keyset position and
            // snapshot describe the frames read, so a write here cannot disturb pagination.
            let (Some(provider), Some(model)) = (embedder, model.as_deref()) else {
                continue;
            };
            if embedded >= embed_limit as u64 {
                continue;
            }
            let Some(text) = frame
                .content_text
                .as_deref()
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let mut vectors = provider
                .embed_texts(&[text.to_string()])
                .await
                .with_context(|| format!("embedding frame {}", frame.frame_id))?;
            let vector = vectors.pop().ok_or_else(|| {
                anyhow!(
                    "the embedder returned no vector for frame {}",
                    frame.frame_id
                )
            })?;
            store
                .upsert_text_embedding(frame.frame_id, 0, text, ChunkSource::Ocr, &vector, model)
                .await?;
            embedded += 1;
            if embedded.is_multiple_of(200) {
                eprintln!("embedded {embedded} (limit {embed_limit})");
            }
        }
        match page.next_position {
            Some(after) => {
                continuation = Some(FrameContinuation {
                    snapshot: page.snapshot,
                    after,
                })
            }
            None => break,
        }
    }

    let sessions = read_sessions(store).await?;
    for session in &sessions {
        record(
            &mut hasher,
            &[
                "session".to_string(),
                session.id.to_string(),
                session.started_at.to_string(),
                field(session.ended_at.map(|at| at.to_string()).as_deref()),
                session.kind.as_str().to_string(),
                field(Some(&session.context_key)),
            ],
        );
    }
    Ok((frames, hasher.finalize().to_hex().to_string(), embedded))
}

/// Every stored session, ascending id.
async fn read_sessions(store: &SqliteStore) -> Result<Vec<Session>> {
    let mut sessions = store
        .sessions_in_range(SessionFilter::default())
        .await
        .context("reading back the fixture sessions")?;
    sessions.sort_by_key(|s| s.id);
    Ok(sessions)
}

/// One canonical field: `\N` for NULL, otherwise the value with `\`, tab, LF and CR escaped.
fn field(value: Option<&str>) -> String {
    match value {
        None => "\\N".to_string(),
        Some(v) => {
            let mut out = String::with_capacity(v.len());
            for ch in v.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '\t' => out.push_str("\\t"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    other => out.push(other),
                }
            }
            out
        }
    }
}

fn record(hasher: &mut blake3::Hasher, fields: &[String]) {
    hasher.update(fields.join("\t").as_bytes());
    hasher.update(b"\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_applies_defaults_and_flags() {
        let opts = GenOptions::parse(&args(&["--out", "x"])).unwrap();
        assert_eq!(
            opts,
            GenOptions {
                out_dir: PathBuf::from("x"),
                seed: DEFAULT_SEED,
                start: default_start(),
                weeks: DEFAULT_WEEKS,
                frames_per_week: DEFAULT_FRAMES_PER_WEEK,
                embeddings: true,
                embed_frames: None,
                persona: None,
            }
        );
        let opts = GenOptions::parse(&args(&[
            "--out",
            "y",
            "--seed",
            "0x5cee_55ee_4152_0004",
            "--start",
            "2027-01-04",
            "--weeks",
            "2",
            "--frames-per-week",
            "40",
            "--no-embeddings",
        ]))
        .unwrap();
        assert_eq!(opts.seed, DEFAULT_SEED);
        assert_eq!(
            (
                opts.start,
                opts.weeks,
                opts.frames_per_week,
                opts.embeddings,
                opts.embed_frames,
            ),
            (
                NaiveDate::from_ymd_opt(2027, 1, 4).unwrap(),
                2,
                40,
                false,
                None,
            )
        );
        assert_eq!(
            GenOptions::parse(&args(&["--out", "z", "--seed", "42"]))
                .unwrap()
                .seed,
            42
        );
        let bounded = GenOptions::parse(&args(&[
            "--out",
            "b",
            "--weeks",
            "52",
            "--frames-per-week",
            "1000",
            "--embed-frames",
            "500",
        ]))
        .unwrap();
        assert_eq!(bounded.embed_frames, Some(500));
        assert_eq!(bounded.total_frames(), 52_000);
        let multi_year = GenOptions::parse(&args(&[
            "--out",
            "m",
            "--weeks",
            "520",
            "--frames-per-week",
            "210",
            "--no-embeddings",
        ]))
        .unwrap();
        assert_eq!(multi_year.total_frames(), 109_200);
    }

    #[test]
    fn parse_names_bad_flags_and_values() {
        let unknown = GenOptions::parse(&args(&["--out", "x", "--frobnicate"])).unwrap_err();
        assert!(unknown.to_string().contains("`--frobnicate`"), "{unknown}");
        assert!(
            unknown.to_string().contains("--frames-per-week <n>"),
            "{unknown}"
        );
        let missing = GenOptions::parse(&args(&["--seed", "1"])).unwrap_err();
        assert!(missing.to_string().contains("--out <dir>"), "{missing}");
        let dangling = GenOptions::parse(&args(&["--out"])).unwrap_err();
        assert!(
            dangling.to_string().contains("`--out` needs a value"),
            "{dangling}"
        );
        let weeks = GenOptions::parse(&args(&["--out", "x", "--weeks", "0"])).unwrap_err();
        assert!(weeks.to_string().contains("--weeks"), "{weeks}");
        let frames =
            GenOptions::parse(&args(&["--out", "x", "--frames-per-week", "1"])).unwrap_err();
        assert!(frames.to_string().contains("--frames-per-week"), "{frames}");
        let seed = GenOptions::parse(&args(&["--out", "x", "--seed", "0xzz"])).unwrap_err();
        assert!(seed.to_string().contains("--seed"), "{seed}");
        let start_format =
            GenOptions::parse(&args(&["--out", "x", "--start", "2026-09-07T00:00"])).unwrap_err();
        assert!(
            start_format.to_string().contains("--start"),
            "{start_format}"
        );
        let start_weekday =
            GenOptions::parse(&args(&["--out", "x", "--start", "2026-09-08"])).unwrap_err();
        assert!(
            start_weekday.to_string().contains("Monday"),
            "{start_weekday}"
        );
        let start_epoch =
            GenOptions::parse(&args(&["--out", "x", "--start", "1969-12-22"])).unwrap_err();
        assert!(
            start_epoch.to_string().contains("1970-01-05"),
            "{start_epoch}"
        );
        let earliest = GenOptions::parse(&args(&[
            "--out",
            "x",
            "--start",
            "1970-01-05",
            "--weeks",
            "1",
            "--frames-per-week",
            "2",
            "--no-embeddings",
        ]))
        .unwrap();
        assert_eq!(earliest.start, NaiveDate::from_ymd_opt(1970, 1, 5).unwrap());
        let weeks_cap = GenOptions::parse(&args(&["--out", "x", "--weeks", "1041"])).unwrap_err();
        assert!(weeks_cap.to_string().contains("--weeks"), "{weeks_cap}");
        let frames_cap =
            GenOptions::parse(&args(&["--out", "x", "--frames-per-week", "50001"])).unwrap_err();
        assert!(
            frames_cap.to_string().contains("--frames-per-week"),
            "{frames_cap}"
        );
        let total_cap = GenOptions::parse(&args(&[
            "--out",
            "x",
            "--weeks",
            "52",
            "--frames-per-week",
            "50000",
            "--no-embeddings",
        ]))
        .unwrap_err();
        assert!(total_cap.to_string().contains("2000000"), "{total_cap}");
        let embed_conflict = GenOptions::parse(&args(&[
            "--out",
            "x",
            "--weeks",
            "2",
            "--frames-per-week",
            "40",
            "--no-embeddings",
            "--embed-frames",
            "5",
        ]))
        .unwrap_err();
        assert!(
            embed_conflict.to_string().contains("--embed-frames"),
            "{embed_conflict}"
        );
        let embed_zero = GenOptions::parse(&args(&[
            "--out",
            "x",
            "--weeks",
            "2",
            "--frames-per-week",
            "40",
            "--embed-frames",
            "0",
        ]))
        .unwrap_err();
        assert!(
            embed_zero.to_string().contains("--embed-frames"),
            "{embed_zero}"
        );
        let embed_too_many = GenOptions::parse(&args(&[
            "--out",
            "x",
            "--weeks",
            "2",
            "--frames-per-week",
            "40",
            "--embed-frames",
            "81",
        ]))
        .unwrap_err();
        assert!(
            embed_too_many.to_string().contains("1..=80"),
            "{embed_too_many}"
        );
        let embed_all_cap = GenOptions::parse(&args(&[
            "--out",
            "x",
            "--weeks",
            "52",
            "--frames-per-week",
            "1000",
        ]))
        .unwrap_err();
        assert!(
            embed_all_cap.to_string().contains("--no-embeddings"),
            "{embed_all_cap}"
        );
    }

    #[test]
    fn digest_fields_escape_separators_and_mark_null() {
        assert_eq!(field(None), "\\N");
        assert_eq!(field(Some("a\tb\nc\\d\r")), "a\\tb\\nc\\\\d\\r");
        assert_ne!(field(Some("")), field(None));
    }

    #[test]
    fn api_token_has_the_product_shape_and_follows_the_seed() {
        let token = api_token(DEFAULT_SEED);
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(token, api_token(DEFAULT_SEED));
        assert_ne!(token, api_token(DEFAULT_SEED ^ 1));
    }

    #[test]
    fn image_paths_follow_the_capture_loop_layout() {
        assert_eq!(
            image_path(1_788_765_600_123),
            "frames/day-20703/1788765600123-0.webp"
        );
    }
}
