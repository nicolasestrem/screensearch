//! `ssv2c-evalfixture` CLI - dev-only 0.4.5 evaluation fixture.
//!
//! ```text
//! ssv2c-evalfixture gen    --out <dir> [--seed <n>] [--start <YYYY-MM-DD>] [--weeks <n>]
//!                          [--frames-per-week <n>] [--no-embeddings | --embed-frames <n>]
//!                          [--persona <id> | --persona random]
//! ssv2c-evalfixture gen    --list-personas
//! ssv2c-evalfixture export --db <path> --out <file.json>
//! ssv2c-evalfixture serve  --db <path> --data <dir> [--port <n>] [--embed] [--no-answer]
//! ssv2c-evalfixture graph  --db <path>
//! ```
//!
//! Exit code 0 on success, 1 when generation, export, serving or the graph backfill fails.

use std::process::ExitCode;

use anyhow::{bail, Result};
use evalfixture::{dream, export, gen, graph, serve};

const USAGE: &str = "\
ssv2c-evalfixture - dev-only 0.4.5 evaluation fixture (never shipped)

USAGE:
  ssv2c-evalfixture gen    --out <dir> [--seed <n>] [--start <YYYY-MM-DD>] [--weeks <n>]
                           [--frames-per-week <n>] [--no-embeddings | --embed-frames <n>]
                           [--persona <id> | --persona random]
  ssv2c-evalfixture gen    --list-personas
  ssv2c-evalfixture export --db <path> --out <file.json>
  ssv2c-evalfixture serve  --db <path> --data <dir> [--port <n>] [--embed] [--no-answer]
  ssv2c-evalfixture graph  --db <path>
  ssv2c-evalfixture dream  --db <path> --out <dir> [--min-gap-mins <n>] [--no-model]

COMMANDS:
  gen     Build the seeded synthetic v15 history (frames, sessions, text, embeddings,
          settings incl. api.token) in <dir>, printing its counts and content digest.
          The default is six weeks from Monday 2026-09-07; --start, --weeks and
          --frames-per-week scale it to a multi-year history (at most 2,000,000 frames),
          and --no-embeddings / --embed-frames bound the model work. Re-running with the
          same arguments and seed prints the same digest. --persona generates one catalog
          persona's life instead of the pinned default fixture; --list-personas prints the
          catalog (`id<TAB>description`, one line each).
  export  Read a fixture database and write one plain UTF-8 JSON file (persona, range,
          projects, sessions, days) any app can parse; same database, byte-identical file.
  serve   Run the product's local HTTP API (/v1/search, /v1/ask) over that database.
          --embed attaches the real text embedder (vector arm); --no-answer skips the
          answer sidecar so /v1/ask answers 503. Both default to off/on respectively:
          the model-free default is what tests use.
  graph   Run the production work-graph backfill, extraction and clustering to completion
          over the fixture database (read-write), printing the completion log line with the
          five table counts (P5 acceptance evidence). Run it twice over the same database:
          the second pass must print identical counts.
  dream   Make a history database dream: every long absence in screen activity is one
          sweep, run Light -> REM -> Deep over it, and write <dir>/DREAMS.md (one diary
          entry per sweep, every claim with its evidence) and <dir>/MEMORY.md (what Deep
          promoted, with its score). Read-only on the database; any normal v15 database
          works. The diary is written by an OpenAI-compatible API (DREAM_API_BASE,
          DREAM_API_MODEL, DREAM_API_KEY); without one each entry is a labelled degraded
          draft.
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ssv2c-evalfixture: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("gen") => gen::run(&args[1..]),
        Some("export") => export::run(&args[1..]),
        Some("dream") => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(dream::run(&args[1..]))
        }
        Some("serve") => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(serve::run(&args[1..]))
        }
        Some("graph") => graph::run(&args[1..]),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("unknown command `{other}`\n\n{USAGE}"),
    }
}
