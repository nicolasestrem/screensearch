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
mod plan;
mod prng;
mod screens;
mod world;

pub use world::{world_entities, WorldEntities};

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context, Result};
use embeddings::FastEmbedProvider;
use store::SqliteStore;
use traits::{
    CaptureTrigger, ChunkSource, EmbeddingProvider, EvidenceWindow, FrameContinuation,
    FrameEvidence, FrameFilters, FramePageRequest, NewFrame, NewSession, Session, SessionFilter,
    FRAME_PAGE_MAX_LIMIT,
};

use crate::{API_TOKEN_SETTING, DEFAULT_FRAMES_PER_WEEK, DEFAULT_SEED, DEFAULT_WEEKS};
use layout::lay_out;
use plan::Plan;
use prng::{subseed, SplitMix64};
use screens::{clock, Renderer, Sticky};

/// Fixture size bounds: at most a year of weeks, and few enough frames per week that the day
/// planner (which compresses spacing for dense fixtures) still ends every day before 19:00.
const MAX_WEEKS: u32 = 52;
const MIN_FRAMES_PER_WEEK: usize = 2;
const MAX_FRAMES_PER_WEEK: usize = 10_000;

/// Display size of every synthetic capture (single 1080p monitor).
const FRAME_WIDTH: u32 = 1920;
const FRAME_HEIGHT: u32 = 1080;

const USAGE: &str = "\
USAGE:
  ssv2c-evalfixture gen --out <dir> [--seed <n>] [--weeks <n>] [--frames-per-week <n>] [--no-embeddings]

OPTIONS:
  --out <dir>              Fixture directory (required). The database is <dir>/screensearch.db;
                           it must not exist yet (a rerun never appends to an old fixture).
  --seed <n>               PRNG seed, decimal or 0x-prefixed hex (default 0x5cee55ee41520004).
  --weeks <n>              Working weeks to synthesize, 1..=52 (default 6).
  --frames-per-week <n>    Frames per week, 2..=10000 (default 210).
  --no-embeddings          Skip text embeddings: no model is loaded or downloaded, and the
                           summary reports embeddings=0. By default every frame's content
                           text is embedded with the product's fastembed model, cached under
                           <dir>/models/fastembed (downloaded on first use).
";

/// Parsed `gen` arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenOptions {
    pub out_dir: PathBuf,
    pub seed: u64,
    pub weeks: u32,
    pub frames_per_week: usize,
    pub embeddings: bool,
}

impl GenOptions {
    /// Parses the arguments after `gen`. Unknown flags and missing or malformed values are
    /// errors naming the flag, followed by the usage text.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut out_dir: Option<PathBuf> = None;
        let mut seed = DEFAULT_SEED;
        let mut weeks = DEFAULT_WEEKS;
        let mut frames_per_week = DEFAULT_FRAMES_PER_WEEK;
        let mut embeddings = true;

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
                "--weeks" => weeks = parse_number("--weeks", &value("--weeks")?)?,
                "--frames-per-week" => {
                    frames_per_week =
                        parse_number("--frames-per-week", &value("--frames-per-week")?)?
                }
                "--no-embeddings" => embeddings = false,
                other => bail!("unknown flag `{other}`\n\n{USAGE}"),
            }
        }
        let opts = Self {
            out_dir: out_dir.ok_or_else(|| anyhow!("`--out <dir>` is required\n\n{USAGE}"))?,
            seed,
            weeks,
            frames_per_week,
            embeddings,
        };
        opts.validate()?;
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

fn parse_number<T: std::str::FromStr>(flag: &str, raw: &str) -> Result<T> {
    raw.parse::<T>()
        .map_err(|_| anyhow!("`{flag}` expects a non-negative integer, got `{raw}`\n\n{USAGE}"))
}

/// What a generation run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenManifest {
    pub db: PathBuf,
    pub seed: u64,
    pub weeks: u32,
    pub frames: u64,
    pub sessions: u64,
    pub embeddings: u64,
    /// blake3 (hex) over the canonical dump of every stored frame and session row.
    pub digest: String,
}

impl GenManifest {
    /// The stable printed summary: `seed=`, `weeks=`, `frames=`, `sessions=`, `embeddings=`,
    /// `digest=`, `db=`, one per line, in that order (the path with forward slashes).
    pub fn summary(&self) -> String {
        format!(
            "seed=0x{:016x}\nweeks={}\nframes={}\nsessions={}\nembeddings={}\ndigest={}\ndb={}\n",
            self.seed,
            self.weeks,
            self.frames,
            self.sessions,
            self.embeddings,
            self.digest,
            self.db.display().to_string().replace('\\', "/")
        )
    }
}

/// `gen` entry point: parse, generate, print the summary.
pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
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

    let plan = plan::plan(opts.seed, opts.weeks, opts.frames_per_week)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building the generator runtime")?;
    runtime.block_on(write_fixture(opts, &db, &plan, embedder.as_ref()))
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
    let mut planned_frames = 0usize;

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
            .await?;
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
                .await?;
            store
                .insert_ocr_filtered(frame_id, laid.ocr, laid.ctx)
                .await?;
            frame_ids.push(frame_id);
        }
        let assigned = store
            .assign_frames_session(&frame_ids, Some(session_id))
            .await?;
        ensure!(
            assigned == frame_ids.len() as u64,
            "session {session_id}: assigned {assigned} of {} frames",
            frame_ids.len()
        );
        planned_frames += frame_ids.len();
    }

    // Only the token: the API's other defaults (`retrieval.default_top_k`, the answer
    // thinking toggle) are applied by the settings loader when the key is absent.
    store
        .set_setting(API_TOKEN_SETTING, &api_token(opts.seed))
        .await?;

    let frames = read_frames(&store).await?;
    let sessions = read_sessions(&store).await?;
    ensure!(
        frames.len() == planned_frames && sessions.len() == plan.sessions.len(),
        "read back {} frames / {} sessions, wrote {planned_frames} / {}",
        frames.len(),
        sessions.len(),
        plan.sessions.len()
    );
    let digest = digest(&frames, &sessions);

    let embedded = match embedder {
        Some(provider) => embed_frames(&store, provider, &frames).await?,
        None => 0,
    };

    Ok(GenManifest {
        db: db.to_path_buf(),
        seed: opts.seed,
        weeks: opts.weeks,
        frames: frames.len() as u64,
        sessions: sessions.len() as u64,
        embeddings: embedded,
        digest,
    })
}

/// Every stored frame through the production history read (`frame_page`), ascending id.
async fn read_frames(store: &SqliteStore) -> Result<Vec<FrameEvidence>> {
    let window = EvidenceWindow {
        from: 0,
        to: i64::MAX,
    };
    let mut continuation: Option<FrameContinuation> = None;
    let mut frames: Vec<FrameEvidence> = Vec::new();
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
        frames.extend(page.items);
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
    frames.sort_by_key(|f| f.frame_id);
    Ok(frames)
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

/// blake3 over one tab-separated, newline-terminated record per frame
/// (`frame id captured_at app_hint window_title browser_url content_text session_id`), then
/// per session (`session id started_at ended_at kind context_key`), each in ascending id
/// order, after a version line. Wall-clock columns and the API token are not part of it.
fn digest(frames: &[FrameEvidence], sessions: &[Session]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"evalfixture-digest v1\n");
    for f in frames {
        record(
            &mut hasher,
            &[
                "frame".to_string(),
                f.frame_id.to_string(),
                f.captured_at.to_string(),
                field(f.app_hint.as_deref()),
                field(f.window_title.as_deref()),
                field(f.browser_url.as_deref()),
                field(f.content_text.as_deref()),
                field(f.session_id.map(|id| id.to_string()).as_deref()),
            ],
        );
    }
    for s in sessions {
        record(
            &mut hasher,
            &[
                "session".to_string(),
                s.id.to_string(),
                s.started_at.to_string(),
                field(s.ended_at.map(|at| at.to_string()).as_deref()),
                s.kind.as_str().to_string(),
                field(Some(&s.context_key)),
            ],
        );
    }
    hasher.finalize().to_hex().to_string()
}

/// Embeds every frame's non-empty `content_text` (chunk 0, OCR source). Any provider or
/// store failure aborts the run; nothing falls back to a partial or fake vector.
async fn embed_frames(
    store: &SqliteStore,
    provider: &FastEmbedProvider,
    frames: &[FrameEvidence],
) -> Result<u64> {
    let model = provider.text_model_name().to_string();
    let todo = frames
        .iter()
        .filter(|f| {
            f.content_text
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty())
        })
        .count();
    eprintln!("embedding {todo} frames with {model}");
    let mut done = 0u64;
    for frame in frames {
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
            .upsert_text_embedding(frame.frame_id, 0, text, ChunkSource::Ocr, &vector, &model)
            .await?;
        done += 1;
        if done.is_multiple_of(200) {
            eprintln!("embedded {done}/{todo}");
        }
    }
    Ok(done)
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
                weeks: DEFAULT_WEEKS,
                frames_per_week: DEFAULT_FRAMES_PER_WEEK,
                embeddings: true,
            }
        );
        let opts = GenOptions::parse(&args(&[
            "--out",
            "y",
            "--seed",
            "0x5cee_55ee_4152_0004",
            "--weeks",
            "2",
            "--frames-per-week",
            "40",
            "--no-embeddings",
        ]))
        .unwrap();
        assert_eq!(opts.seed, DEFAULT_SEED);
        assert_eq!(
            (opts.weeks, opts.frames_per_week, opts.embeddings),
            (2, 40, false)
        );
        assert_eq!(
            GenOptions::parse(&args(&["--out", "z", "--seed", "42"]))
                .unwrap()
                .seed,
            42
        );
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
