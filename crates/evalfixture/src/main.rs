//! `ssv2c-evalfixture` CLI - dev-only 0.4.5 evaluation fixture.
//!
//! ```text
//! ssv2c-evalfixture gen   --out <dir> [--seed <n>] [--weeks <n>] [--frames-per-week <n>]
//! ssv2c-evalfixture serve --db <path> --data <dir> [--port <n>] [--embed] [--no-answer]
//! ```
//!
//! Exit code 0 on success, 1 when generation or serving fails.

use std::process::ExitCode;

use anyhow::{bail, Result};
use evalfixture::{gen, serve};

const USAGE: &str = "\
ssv2c-evalfixture - dev-only 0.4.5 evaluation fixture (never shipped)

USAGE:
  ssv2c-evalfixture gen   --out <dir> [--seed <n>] [--weeks <n>] [--frames-per-week <n>]
  ssv2c-evalfixture serve --db <path> --data <dir> [--port <n>] [--embed] [--no-answer]

COMMANDS:
  gen     Build the seeded synthetic v14 history (frames, sessions, text, embeddings,
          settings incl. api.token) in <dir>, printing its counts and content digest.
          Re-running with the same --seed prints the same digest.
  serve   Run the product's local HTTP API (/v1/search, /v1/ask) over that database.
          --embed attaches the real text embedder (vector arm); --no-answer skips the
          answer sidecar so /v1/ask answers 503. Both default to off/on respectively:
          the model-free default is what tests use.
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
        Some("serve") => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(serve::run(&args[1..]))
        }
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("unknown command `{other}`\n\n{USAGE}"),
    }
}
