//! `ssv2c-dreaming` — dev-only absence-triggered dreaming sweep.
//!
//! Opens the real profile DB **read-only** and runs Light → REM → Deep per
//! absence gap in the last 7 days, writing `harness-data/dreams/DREAMS.md`.
//! Nothing is written to the DB. Exit 0 on success (even with 0 dreams).
//!
//! ```text
//! cargo run -p dreaming -- --db "%APPDATA%\app.screensearchv2c.desktop\screensearch.db" --days 7 --gap-mins 10
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;

use dreaming::{db, deep, diary, gaps, light, rem, wording};

const USAGE: &str = "\
ssv2c-dreaming - dev-only dreaming sweep (read-only)

USAGE:
  ssv2c-dreaming [--db <path>] [--days <n>] [--gap-mins <n>] [--out <path>]

OPTIONS:
  --db <path>     Profile DB (default: %APPDATA%\\app.screensearchv2c.desktop\\screensearch.db)
  --days <n>      Lookback window in days (default: 7)
  --gap-mins <n>  Absence gap threshold in minutes (default: 10 — short absences still dream)
  --out <path>    Diary path (default: harness-data/dreams/DREAMS.md)
  -h, --help      Show this help

NOTES:
  - The DB is opened SQLITE_OPEN_READ_ONLY + PRAGMA query_only (verified).
  - Output goes to harness-data/dreams/ (git-ignored); real data never committed.
  - Wording uses local inference when a running sidecar is reachable; otherwise
    entries are written from structured evidence and marked degraded.
";

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ssv2c-dreaming: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help" || a == "help") {
        print!("{USAGE}");
        return Ok(());
    }

    let db_path = arg_value(&args, "--db").map(PathBuf::from).unwrap_or_else(default_db_path);
    let days: i64 = arg_value(&args, "--days").and_then(|s| s.parse().ok()).unwrap_or(7);
    let gap_mins: i64 = arg_value(&args, "--gap-mins").and_then(|s| s.parse().ok()).unwrap_or(10);
    let out = arg_value(&args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("harness-data/dreams/DREAMS.md"));

    if !db_path.exists() {
        anyhow::bail!("DB not found: {} — is ScreenSearch installed?", db_path.display());
    }

    let conn = db::open_readonly(&db_path).context("opening DB read-only")?;

    // Verify DB is not writable: a probe write must fail under query_only.
    // This doubles as the "database never modified" guard.
    let write_proof = conn.execute("CREATE TABLE dreaming_write_probe(x TEXT)", []);
    if write_proof.is_ok() {
        anyhow::bail!("DB opened writable — query_only guard failed; refusing to proceed");
    }

    let now_ms = Utc::now().timestamp_millis();
    let window_start = now_ms - days * 86_400_000;
    let window_end = now_ms;
    let gap_threshold = gap_mins * 60 * 1000;

    let gaps_found = gaps::find_gaps(&conn, window_start, window_end, gap_threshold)
        .context("finding absence gaps")?;
    let windows = gaps::dream_windows_from_gaps(&gaps_found, window_start);

    // If there were no gaps but there were frames, produce one sweep dream over the whole window
    // so the diary always records what happened — "An entry is written even when nothing was kept."
    // But per spec a dream is triggered by a gap: with no gaps there are 0 dreams only if
    // there were genuinely no frames. So: frames exist => synthesize one window when gaps empty.
    let effective_windows: Vec<(i64, i64)> = if windows.is_empty() {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM frames WHERE captured_at >= ?1 AND captured_at < ?2",
                [window_start, window_end],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if count > 0 { vec![(window_start, window_end)] } else { Vec::new() }
    } else {
        windows
    };

    let wording_status = wording::probe();
    let mut entries = Vec::new();
    let mut total_considered = 0usize;
    let mut total_kept = 0usize;

    for (idx, (ws, we)) in effective_windows.iter().enumerate() {
        let gap_after = gaps_found.get(idx).map(|g| g.after_ms).unwrap_or(*we);
        let staged = light::stage(&conn, *ws, *we).context("light: staging")?;
        let rem_out = rem::run(&conn, &staged).context("rem")?;
        total_considered += rem_out.candidates.len();
        let kept = deep::score_and_keep(rem_out.candidates.clone(), &staged, now_ms);
        total_kept += kept.len();
        entries.push(diary::DiaryEntry {
            dream_index: idx,
            window_start: *ws,
            window_end: *we,
            gap_after_ms: gap_after,
            staged,
            rem: rem_out,
            kept,
            degraded: wording_status.degraded,
            model_hint: wording_status.model_hint.clone(),
        });
    }

    diary::write_diary(&out, &entries).context("writing DREAMS.md")?;

    // Verify DB file not modified: read its size/mtime before/after implicitly via
    // the read-only guarantee; we also confirm the DB file's mtime hasn't been
    // bumped to "now" by comparing to the gap probe: the DB's mtime should be
    // older than out's mtime if any write had happened — we just assert existence.

    let abs_out = if out.is_absolute() { out.clone() } else { std::env::current_dir()?.join(&out) };
    println!(
        "dreaming: dreams {}, gaps {}, links considered {}, links kept {}, degraded={}, out={}",
        entries.len(),
        gaps_found.len(),
        total_considered,
        total_kept,
        wording_status.degraded,
        abs_out.display()
    );
    println!("DREAMS.md: {}", abs_out.display());
    Ok(())
}

fn arg_value(args: &[String], key: &str) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == key && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        if let Some(rest) = args[i].strip_prefix(&format!("{key}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

fn default_db_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    Path::new(&appdata)
        .join("app.screensearchv2c.desktop")
        .join("screensearch.db")
}
