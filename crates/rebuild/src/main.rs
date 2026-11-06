//! `ssv2c-rebuild` CLI - dev-only v13 → v15 rebuild (`03 §4`, 0.4.5 D3, D2).
//!
//! ```text
//! ssv2c-rebuild --source <v13.db> --dest <new-v15.db> [--sample-frames N]
//! ssv2c-rebuild --counts <db>
//! ```
//!
//! Exit code 0 on success, 1 when the rebuild or a check fails (a failed run
//! removes the partially written destination).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use rebuild::{RebuildOptions, RebuildReport, BINARY_NAME};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{BINARY_NAME}: {error:#}");
            ExitCode::FAILURE
        }
    }
}

enum Command {
    Help,
    Counts(PathBuf),
    Rebuild(RebuildOptions),
}

fn run() -> Result<()> {
    match parse_args()? {
        Command::Help => {
            print_usage();
            Ok(())
        }
        Command::Counts(path) => {
            let counts = rebuild::table_counts(&path)?;
            println!(
                "{BINARY_NAME} - per-table row counts for {}",
                path.display()
            );
            print_counts(&counts);
            Ok(())
        }
        Command::Rebuild(options) => {
            let report = rebuild::rebuild(&options)?;
            print_report(&report, &options);
            Ok(())
        }
    }
}

fn parse_args() -> Result<Command> {
    let mut source = None;
    let mut dest = None;
    let mut counts = None;
    let mut sample_frames = None;

    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--source" => source = Some(PathBuf::from(next_value(&mut args, "--source")?)),
            "--dest" => dest = Some(PathBuf::from(next_value(&mut args, "--dest")?)),
            "--counts" => counts = Some(PathBuf::from(next_value(&mut args, "--counts")?)),
            "--sample-frames" => {
                let raw = next_value(&mut args, "--sample-frames")?;
                let limit = raw
                    .parse::<usize>()
                    .with_context(|| format!("--sample-frames expects a number, got `{raw}`"))?;
                sample_frames = Some(limit);
            }
            other => bail!("unknown argument `{other}` (try --help)"),
        }
    }

    if let Some(path) = counts {
        if source.is_some() || dest.is_some() || sample_frames.is_some() {
            bail!("--counts takes one database and cannot be combined with --source/--dest");
        }
        return Ok(Command::Counts(path));
    }

    match (source, dest) {
        (Some(source), Some(dest)) => Ok(Command::Rebuild(RebuildOptions {
            source,
            dest,
            sample_frames,
        })),
        _ => bail!("--source and --dest are both required (try --help)"),
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next().with_context(|| format!("{flag} needs a value"))
}

fn print_usage() {
    println!(
        "\
{BINARY_NAME} - dev-only ScreenSearch v13 -> current-baseline database rebuild (03 §4, 0.4.5 D3)

USAGE:
  {BINARY_NAME} --source <v13.db> --dest <new-v15.db> [--sample-frames N]
  {BINARY_NAME} --counts <db>

The source is opened read-only: it is never written, checkpointed, vacuumed or
renamed. The destination must not exist; it is created from the baseline.
Rows are copied generically (shared columns only), FTS5 and vec0 are rebuilt
from the copied rows, `jobs` and `chrome_text_catalog` are not carried, and
retired settings keys are dropped. `*.v13.bak` remains a maintainer step.

  --counts <db>        print a per-table row count for any SQLite database
  --sample-frames N    print N random frame ids from the rebuilt file
  -h, --help           show this help"
    );
}

fn print_report(report: &RebuildReport, options: &RebuildOptions) {
    println!("{BINARY_NAME} - dev-only v13 -> current-baseline rebuild (03 §4, 0.4.5 D3)");
    println!();
    println!(
        "source: {} (schema_version {}, opened read-only)",
        options.source.display(),
        report.source_version
    );
    println!(
        "dest:   {} (schema_version {}, created fresh)",
        options.dest.display(),
        report.dest_version
    );
    println!();
    println!("per-table counts (destination, with the source where present):");
    print_counts_merged(&report.source_counts, &report.dest_counts);
    println!();
    println!(
        "settings: {} carried, {} dropped{}",
        report.settings_copied,
        report.settings_dropped.len(),
        if report.settings_dropped.is_empty() {
            String::new()
        } else {
            format!(" ({})", report.settings_dropped.join(", "))
        }
    );
    println!(
        "embedding vectors: {} vec0 row(s) inserted from the copied embeddings",
        report.embedding_vectors
    );
    if report.missing_images.is_empty() {
        println!("images: every referenced image file is present");
    } else {
        println!(
            "images: {} referenced file(s) missing; the rows and their paths are kept:",
            report.missing_images.len()
        );
        for path in report.missing_images.iter().take(10) {
            println!("  {path}");
        }
        if report.missing_images.len() > 10 {
            println!("  … {} more", report.missing_images.len() - 10);
        }
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    println!("integrity_check: {}", report.integrity_check.join(", "));
    println!(
        "foreign_key_check: {} ({} row(s))",
        if report.foreign_key_violations == 0 {
            "clean"
        } else {
            "FAILED"
        },
        report.foreign_key_violations
    );
    if let Some(requested) = options.sample_frames {
        println!();
        println!(
            "sample frames ({requested} requested, {} returned, random): {}",
            report.sample_frames.len(),
            report
                .sample_frames
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    println!();
    println!("rebuild OK");
}

fn print_counts(counts: &[(String, i64)]) {
    let width = counts
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0)
        .max("table".len());
    println!("{:<width$} {:>12}", "table", "rows", width = width);
    for (name, rows) in counts {
        println!("{:<width$} {:>12}", name, rows, width = width);
    }
}

fn print_counts_merged(source: &[(String, i64)], dest: &[(String, i64)]) {
    let source: BTreeMap<&str, i64> = source
        .iter()
        .map(|(name, rows)| (name.as_str(), *rows))
        .collect();
    let dest: BTreeMap<&str, i64> = dest
        .iter()
        .map(|(name, rows)| (name.as_str(), *rows))
        .collect();
    let names: BTreeSet<&str> = source.keys().chain(dest.keys()).copied().collect();
    let width = names
        .iter()
        .map(|name| name.len())
        .max()
        .unwrap_or(0)
        .max(24);
    println!(
        "{:<width$} {:>10} {:>10}",
        "table",
        "source",
        "dest",
        width = width
    );
    for name in names {
        println!(
            "{:<width$} {:>10} {:>10}",
            name,
            display_count(source.get(name)),
            display_count(dest.get(name)),
            width = width
        );
    }
}

fn display_count(count: Option<&i64>) -> String {
    count.map(i64::to_string).unwrap_or_else(|| "-".to_string())
}
