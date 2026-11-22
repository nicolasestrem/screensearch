//! `dream` - dev-only memory consolidation over a history database, after the OpenClaw
//! dreaming model (Light -> REM -> Deep, a Dream Diary and promoted long-term memory).
//!
//! ```text
//! ssv2c-evalfixture dream --db <screensearch.db> --out <dir> [--min-gap-mins <n>] [--no-model]
//! ```
//!
//! * **Sweeps are absences.** Every gap of at least `--min-gap-mins` between two captured
//!   frames is one sweep; the material staged by a sweep is everything captured since the
//!   previous absence ended. There is no fixed hour.
//! * **Light** sorts the staged sessions: what occupied the screen, what was only brushed
//!   past, plus any saved daily report or project-note text the app already produced.
//! * **REM** looks backwards for sessions close in meaning but far apart in time, using the
//!   stored frame embeddings (session centroids over `embedding_vectors`, falling back to
//!   `session_vectors`) and penalising shared window titles and lines, so keyword overlap
//!   cannot pass for an echo. Work-graph recurrences and resumptions come from the
//!   kernel's own insight derivations (`kernel::insights::derive_*`).
//! * **Deep** scores each touched theme with the six OpenClaw weights plus a Light/REM
//!   reinforcement boost, applies the three gates and promotes into `MEMORY.md`.
//!
//! The database is opened read-only and only the normal v15 schema is read, so the same
//! command runs unchanged on a real profile. `DREAMS.md`, `MEMORY.md` and `DREAMS.json` are
//! written from scratch on every run and never opened for reading: the diary is never an input.
//!
//! Narrative entries come from an OpenAI-compatible chat completion (`DREAM_API_BASE`,
//! `DREAM_API_MODEL`, `DREAM_API_KEY`, or the same keys in `--env-file`). Without one, or when
//! the reply fails validation, a narrative entry uses the labelled rule-based draft. A quiet
//! sweep gets a fixed short line without requesting a completion.

mod data;
mod diary;
mod phases;

use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};

pub use phases::Gates;

const USAGE: &str = "\
USAGE:
  ssv2c-evalfixture dream --db <path> --out <dir> [--min-gap-mins <n>] [--no-model]
                          [--env-file <path>] [--max-model-calls <n>]

OPTIONS:
  --db <path>             history database (normal v15 schema), opened read-only
  --out <dir>             output directory; DREAMS.md, MEMORY.md and DREAMS.json are (re)written there
  --min-gap-mins <n>      shortest absence that counts as a sweep (default 120)
  --no-model              write labelled rule-based drafts for narrative entries
  --env-file <path>       KEY=VALUE file with DREAM_API_BASE / DREAM_API_MODEL / DREAM_API_KEY
                          (process environment wins); the key is never printed
  --max-model-calls <n>   hard cap on completions for the whole run (default 200)
";

/// Parsed `dream` arguments.
#[derive(Debug, Clone)]
pub struct DreamOptions {
    pub db: PathBuf,
    pub out: PathBuf,
    pub min_gap_ms: i64,
    pub no_model: bool,
    pub env_file: Option<PathBuf>,
    pub max_model_calls: u32,
}

impl DreamOptions {
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut db = None;
        let mut out = None;
        let mut min_gap_mins: i64 = 120;
        let mut no_model = false;
        let mut env_file = None;
        let mut max_model_calls = 200;
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let mut value = |flag: &str| {
                iter.next()
                    .cloned()
                    .ok_or_else(|| anyhow!("`{flag}` needs a value\n\n{USAGE}"))
            };
            match arg.as_str() {
                "--db" => db = Some(PathBuf::from(value("--db")?)),
                "--out" => out = Some(PathBuf::from(value("--out")?)),
                "--min-gap-mins" => {
                    min_gap_mins = value("--min-gap-mins")?
                        .parse()
                        .context("--min-gap-mins is a whole number of minutes")?
                }
                "--no-model" => no_model = true,
                "--env-file" => env_file = Some(PathBuf::from(value("--env-file")?)),
                "--max-model-calls" => {
                    max_model_calls = value("--max-model-calls")?
                        .parse()
                        .context("--max-model-calls is a whole number")?
                }
                other => bail!("unknown flag `{other}` for dream\n\n{USAGE}"),
            }
        }
        if min_gap_mins < 10 {
            bail!("--min-gap-mins must be at least 10");
        }
        Ok(Self {
            db: db.ok_or_else(|| anyhow!("--db is required\n\n{USAGE}"))?,
            out: out.ok_or_else(|| anyhow!("--out is required\n\n{USAGE}"))?,
            min_gap_ms: min_gap_mins * 60_000,
            no_model,
            env_file,
            max_model_calls,
        })
    }
}

/// `dream` entry point.
pub async fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let opts = DreamOptions::parse(args)?;
    let history = data::History::load(&opts.db, opts.min_gap_ms)?;
    let model = if opts.no_model {
        None
    } else {
        diary::ModelConfig::discover(opts.env_file.as_deref(), opts.max_model_calls)?
    };
    match &model {
        Some(config) => eprintln!("dream: diary model {} at {}", config.model, config.base),
        None => eprintln!("dream: no diary model configured; entries will be degraded drafts"),
    }

    let sweeps = data::absences(&history, opts.min_gap_ms);
    let outcome = phases::dream(&history, &sweeps, &Gates::default());
    let written = diary::write_all(&history, &outcome, model.as_ref(), &opts.out).await?;
    println!(
        "dream: persona={} frames={} sessions={} absences={} sweeps={} entries={} model_written={} quiet_shortened={} degraded={} promoted={} first={} last={} out={}",
        history.persona.id,
        history.frames.len(),
        history.sessions.len(),
        sweeps.len(),
        outcome.sweeps.len(),
        written.entries,
        written.model_written,
        written.quiet_shortened,
        written.degraded,
        outcome.promoted_total(),
        written.first_day,
        written.last_day,
        opts.out.display().to_string().replace('\\', "/"),
    );
    Ok(())
}
