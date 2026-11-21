//! `export` - the dev-only JSON export of a fixture database.
//!
//! `ssv2c-evalfixture export --db <path> --out <file.json>` reads a fixture produced by
//! `ssv2c-evalfixture gen` and writes one UTF-8 JSON file that any app with a plain JSON
//! parser can read: no SQLite, no vectors, no images, no full captured text. The shape is
//! documented in `docs/TESTING.md`.
//!
//! Not implemented yet on this commit; the next commit adds the exporter.

use anyhow::{bail, Result};

const USAGE: &str = "\
USAGE:
  ssv2c-evalfixture export --db <path> --out <file.json>

OPTIONS:
  --db <path>     Fixture database written by `ssv2c-evalfixture gen` (required).
  --out <file>    Destination JSON file (required). An existing file is overwritten; the same
                  database always produces a byte-identical file.
";

/// `export` entry point.
pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    bail!("export is not implemented yet")
}
