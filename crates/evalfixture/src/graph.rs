//! `graph` - dev-only work-graph backfill over the fixture database (0.4.5 P5 evidence).
//!
//! ```text
//! ssv2c-evalfixture graph --db <fixture>/screensearch.db
//! ```
//!
//! Opens the generated fixture read-write through the production store and runs the
//! production work-graph pipeline to completion through the public kernel driver
//! ([`kernel::graph_backfill::run_to_completion`]): the resumable backfill, every frame's
//! extraction body behind the real queue claim/complete cycle, and one clustering pass.
//! Nothing here is a second implementation; the driver reuses the production producer and
//! job bodies.
//!
//! The kernel logs the one greppable completion line with the five table counts
//! (`03 §7h.6`, the P5 acceptance evidence). This binary installs the INFO tracing
//! subscriber the app normally provides, so that line is actually printed, then prints the
//! returned counts on their own line. Running the command twice over the same database must
//! print identical counts; that pair is the idempotence evidence.
//!
//! The fixture is the only database this command may touch. `graph` never creates or
//! migrates a database, and no app, route or shipped crate can reach it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use store::SqliteStore;
use traits::{GraphTableCounts, Store};

const USAGE: &str = "\
USAGE:
  ssv2c-evalfixture graph --db <path>

OPTIONS:
  --db <path>   the fixture database written by `gen` (opened read-write; never created)

Runs the production work-graph backfill, extraction and clustering to completion over the
fixture, prints the kernel completion log line and the five returned table counts.
";

/// Parsed `graph` arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphOptions {
    pub db: PathBuf,
}

impl GraphOptions {
    /// Parses the arguments after `graph`. `--db` is required; unknown flags and a missing
    /// value are errors naming the flag, followed by the usage text.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut db: Option<PathBuf> = None;
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--db" => {
                    let value = iter
                        .next()
                        .cloned()
                        .ok_or_else(|| anyhow!("`--db` needs a value\n\n{USAGE}"))?;
                    db = Some(PathBuf::from(value));
                }
                other => bail!("unknown flag `{other}` for graph\n\n{USAGE}"),
            }
        }
        Ok(Self {
            db: db.ok_or_else(|| {
                anyhow!("--db is required (the fixture database written by gen)\n\n{USAGE}")
            })?,
        })
    }
}

/// `graph` entry point: parse, open read-write, run the production pipeline, print the
/// completion line and the returned counts.
pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let opts = GraphOptions::parse(args)?;
    init_tracing();
    if !opts.db.is_file() {
        bail!(
            "no fixture database at {} - run `ssv2c-evalfixture gen --out <dir>` first; \
             graph never creates or migrates a database",
            opts.db.display()
        );
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building the graph runtime")?;
    runtime.block_on(run_graph(&opts.db))
}

async fn run_graph(db: &Path) -> Result<()> {
    let store = SqliteStore::open_path(db)
        .with_context(|| format!("opening the fixture database {}", db.display()))?;
    let store: Arc<dyn Store> = Arc::new(store);
    let started = Instant::now();
    let counts = kernel::graph_backfill::run_to_completion(&store)
        .await
        .with_context(|| format!("running the work-graph backfill over {}", db.display()))?;
    println!("{}", counts_line(&counts));
    println!(
        "graph backfill elapsed: {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// The five returned counts on one stable, greppable line, so a second pass over the same
/// database can be compared against the first (the idempotence evidence).
pub fn counts_line(counts: &GraphTableCounts) -> String {
    format!(
        "graph backfill returned counts: entities={} entity_aliases={} session_entities={} \
         threads={} thread_sessions={}",
        counts.entities,
        counts.entity_aliases,
        counts.session_entities,
        counts.threads,
        counts.thread_sessions
    )
}

/// Installs the process-wide INFO tracing subscriber the app normally provides, so the
/// kernel's `graph backfill complete ...` acceptance line is actually emitted here. INFO is
/// the whole level this dev command needs; kernel debug chatter stays quiet. ANSI colors are
/// off so the pasted line is plain text; the target is kept so the line names the kernel
/// module that logged it.
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(std::io::stdout)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_requires_db_and_rejects_unknown_flags() {
        let opts = GraphOptions::parse(&args(&["--db", "fixture.db"])).unwrap();
        assert_eq!(opts.db, PathBuf::from("fixture.db"));

        let missing = GraphOptions::parse(&args(&[])).unwrap_err();
        assert!(
            missing.to_string().contains("--db is required"),
            "{missing}"
        );
        let dangling = GraphOptions::parse(&args(&["--db"])).unwrap_err();
        assert!(
            dangling.to_string().contains("`--db` needs a value"),
            "{dangling}"
        );
        let unknown = GraphOptions::parse(&args(&["--db", "x", "--frobnicate"])).unwrap_err();
        assert!(unknown.to_string().contains("`--frobnicate`"), "{unknown}");
    }

    #[test]
    fn counts_line_carries_every_count_as_name_value() {
        let counts = GraphTableCounts {
            entities: 1,
            entity_aliases: 2,
            session_entities: 3,
            threads: 4,
            thread_sessions: 5,
        };
        assert_eq!(
            counts_line(&counts),
            "graph backfill returned counts: entities=1 entity_aliases=2 session_entities=3 \
             threads=4 thread_sessions=5"
        );
    }
}
