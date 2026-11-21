//! # `ssv2c-evalfixture` - dev-only evaluation fixture for 0.4.5 P4/P5/P6
//!
//! Four subcommands, all dev-only and none reachable from the app:
//!
//! - `gen` builds a **seeded, synthetic v15 history** through the production Store writers:
//!   a working-day timeline of frames, sessions, OCR text and optional embeddings across many
//!   apps, repositories, files, domains and ticket references. The default span is six weeks
//!   starting Monday 2026-09-07; `--start`, `--weeks` and `--frames-per-week` scale it to a
//!   multi-year history (one fixture is capped at 2,000,000 frames). The seed is printed and
//!   committed ([`DEFAULT_SEED`]), so P4, P5 and P6 measure the same database.
//!   `--persona <id>` replaces the pinned default worker with one **data-driven persona**
//!   from the catalog ([`gen::personas`]): a different person, occupation, OS, language,
//!   country, timezone, monitor count, weekly rhythm, apps and content. `--persona random`
//!   picks one from the seed and `--list-personas` prints the catalog. Without `--persona`
//!   the output is exactly the pinned default fixture, byte for byte.
//! - `export` reads a fixture database and writes **one plain UTF-8 JSON file** - persona,
//!   time range, projects, sessions (apps/titles/word fragments) and per-day activity - that
//!   any app with a plain JSON parser can read. No SQLite, no vectors, no images, no full
//!   captured text. The same database always produces a byte-identical file.
//! - `serve` runs the **product's own local HTTP API** (`crates/api`: `GET /v1/search`,
//!   `POST /v1/ask`) over that fixture database, with the real embedding and answer
//!   providers, so the harness measures the shipped retrieval/answer code instead of a
//!   re-implementation. It is the fixture stand-in for "start the app and enable the API in
//!   Settings" - it never touches the maintainer's profile, database or models.
//! - `graph` runs the **production work-graph pipeline** (`kernel::graph_backfill::
//!   run_to_completion`) to completion over that fixture database, so the P5 completion
//!   line with the five table counts and the second-pass idempotence check come from the
//!   shipped backfill driver, not from a harness-side re-implementation.
//!
//! Why the fixture exists at all (`docs/kickoffs/P4.md`, extended for the threshold task):
//! there is no real history to measure on this machine, and the P5/P6 acceptance thresholds
//! must be frozen from a baseline that anybody can regenerate. The fixture is synthetic
//! *data*, never synthetic *expectations*: question/answer pairs remain derived from whatever
//! database the harness opens (D17), so pointing the harness at this fixture measures the
//! generators and the pipeline exactly as a real run does.
//!
//! ```text
//! ssv2c-evalfixture gen   --out harness-data/fixture
//! ssv2c-evalfixture serve --db harness-data/fixture/screensearch.db \
//!                         --data harness-data/fixture --embed --port 43210
//! ssv2c-evalfixture gen   --out harness-data/nurse --persona nurse-shifts --weeks 2 \
//!                         --frames-per-week 300 --no-embeddings
//! ssv2c-evalfixture export --db harness-data/nurse/screensearch.db --out nurse.json
//! ```
//!
//! A multi-year, model-free history for load or scale tests (roughly eight minutes and 1 GB for
//! 109,200 frames on the maintainer's machine):
//!
//! ```text
//! ssv2c-evalfixture gen --out harness-data/scale --weeks 520 --frames-per-week 210 \
//!                       --no-embeddings
//! ```

pub mod export;
pub mod gen;
pub mod graph;
pub mod serve;

use std::path::{Path, PathBuf};

/// The seeded baseline. Committed so every phase regenerates byte-identical content: P5 and
/// P6 must measure the same database P4 froze its thresholds against.
pub const DEFAULT_SEED: u64 = 0x5cee_55ee_4152_0004;

/// Default synthetic span: six weeks of working days, enough for several sessions per day
/// per app and for the `window recall` generator to find whole weeks of history.
pub const DEFAULT_WEEKS: u32 = 6;

/// Default frames per synthetic week (five working days of a 42-frame day).
pub const DEFAULT_FRAMES_PER_WEEK: usize = 210;

/// Database file name inside the fixture directory (the app's own name, so paths in the
/// report look like a real run).
pub const DB_FILE: &str = "screensearch.db";

/// The API port the harness defaults to (`crates/api` `DEFAULT_PORT`); kept as a literal so
/// this crate never has to depend on the constant's module path.
pub const DEFAULT_PORT: u16 = 43210;

/// The `settings` key holding the API bearer token; the harness reads it from the database
/// itself (`crates/harness/src/eval/facts.rs`), so the fixture writes it there.
pub const API_TOKEN_SETTING: &str = "api.token";

/// `<fixture dir>/screensearch.db`.
pub fn db_path(fixture_dir: &Path) -> PathBuf {
    fixture_dir.join(DB_FILE)
}
