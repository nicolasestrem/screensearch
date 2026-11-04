//! The P4 automatic intelligence evaluation (`docs/kickoffs/P4.md`).
//!
//! Dev-only, read-only, **zero maintainer input** (0.4.5 D17). One command generates several
//! hundred question/answer pairs mechanically from the database, asks them through the running
//! app's local HTTP API, and writes a report plus the two JSONL artifacts later phases read.
//!
//! ```text
//! harness eval --db <screensearch.db> --api http://127.0.0.1:43210 \
//!              [--pairs 300] [--seed <n>] [--out harness-data/eval-<run-id>] \
//!              [--top-k <n>] [--timeout-secs 180] [--max-frames <n>]
//! ```
//!
//! Guarantees:
//! - **Read-only.** [`facts::Db`] wraps [`crate::export::open_readonly`]; a test asserts an
//!   attempted write through the runner's handle fails with a read-only error.
//! - **Deterministic.** The pair set is a function of `(db, seed, pairs, max_frames, now_ms)`;
//!   the seed is printed in the report and `--seed` pins it.
//! - **No stored questions.** Every pair comes from a database query at run time.
//! - **Fixture-proof.** Tests build synthetic databases in temp directories and stub the HTTP
//!   surface; no real database, model, profile or token is ever touched.

pub mod client;
pub mod facts;
pub mod gen;
pub mod metrics;
pub mod prng;
pub mod refs;
pub mod report;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Local;
use serde::Serialize;
use serde_json::Value;

pub use client::{ApiClient, FailureClass, HealthInfo, TurnFailure};
pub use facts::Db;
pub use gen::{
    generate_pairs, GenOpts, GenOutcome, GenStat, Generator, Pair, Window, DEFAULT_PAIRS,
};
pub use metrics::{Aggregate, GeneratorMetrics, Metrics, TurnRun};
pub use refs::Reference;

/// Consecutive `api_unavailable` turns after which the run stops asking (the report still lands).
const ABORT_AFTER_UNAVAILABLE: usize = 5;

/// CLI options for `harness eval`.
#[derive(Debug, Clone)]
pub struct EvalOpts {
    /// `--db <path>`; defaults to `%APPDATA%\app.screensearchv2c.desktop\screensearch.db`.
    pub db: PathBuf,
    /// `--api <url>`; defaults to `http://127.0.0.1:43210`.
    pub api: String,
    /// `--pairs <n>`; default [`DEFAULT_PAIRS`].
    pub pairs: usize,
    /// `--seed <n>`; default = run start in unix ms (printed in the report).
    pub seed: Option<u64>,
    /// `--out <dir>`; default `harness-data/eval-<run-id>`.
    pub out: Option<PathBuf>,
    /// `--top-k <n>`; absent means the server's `retrieval.default_top_k`.
    pub top_k: Option<u32>,
    /// `--timeout-secs <n>` per turn.
    pub timeout_secs: u64,
    /// `--max-frames <n>` caps the exact reference pass; `0`/absent means the whole table.
    pub max_frames: Option<usize>,
}

impl EvalOpts {
    /// Parse the flags from the full argv (the subcommand token is ignored).
    pub fn parse(args: &[String]) -> Result<Self> {
        let db = match flag(args, "--db") {
            Some(path) => PathBuf::from(path),
            None => crate::export::default_db_path()
                .context("APPDATA is unset; pass `--db <path>` to the live database")?,
        };
        let pairs = number(args, "--pairs")?.unwrap_or(DEFAULT_PAIRS);
        if pairs == 0 {
            bail!("`--pairs` must be at least 1");
        }
        let timeout_secs = number(args, "--timeout-secs")?.unwrap_or(180);
        if timeout_secs == 0 {
            bail!("`--timeout-secs` must be at least 1");
        }
        let max_frames = match number::<usize>(args, "--max-frames")? {
            Some(0) | None => None,
            Some(n) => Some(n),
        };
        Ok(Self {
            db,
            api: flag(args, "--api")
                .unwrap_or("http://127.0.0.1:43210")
                .to_string(),
            pairs,
            seed: number(args, "--seed")?,
            out: flag(args, "--out").map(PathBuf::from),
            top_k: number(args, "--top-k")?,
            timeout_secs,
            max_frames,
        })
    }
}

/// What one run produced, returned so tests can assert on it without re-reading files.
#[derive(Debug)]
pub struct EvalRun {
    pub run_id: String,
    pub out_dir: PathBuf,
    pub report_path: PathBuf,
    /// Machine-readable numbers beside `report.md` (later phases read this, not the Markdown).
    pub report_json_path: PathBuf,
    pub pairs_path: PathBuf,
    pub runs_path: PathBuf,
    pub report: String,
    pub metrics: Metrics,
    pub pairs: Vec<Pair>,
    pub runs: Vec<TurnRun>,
    pub generated: GenOutcome,
}

/// Run the whole evaluation: open read-only, generate pairs, ask each one, measure, report.
pub async fn run_eval(opts: &EvalOpts) -> Result<EvalRun> {
    let started_at = Instant::now();
    let started_local = Local::now();
    let run_id = started_local.format("%Y%m%d-%H%M%S").to_string();
    let now_ms = started_local.timestamp_millis();
    let seed = opts.seed.unwrap_or(now_ms as u64);
    let out_dir = opts
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("harness-data").join(format!("eval-{run_id}")));

    // Nothing is written before the database opens read-only, yields its token and produces at
    // least one pair: a bad invocation must not leave an empty run directory behind.
    let db = Db::open(&opts.db)?;
    // Fail before any pair is generated with the actionable token message, not a 401 later.
    let token = db.api_token()?;
    let schema_version = db.schema_version()?;

    let generated = generate_pairs(
        &db,
        &GenOpts {
            seed,
            pairs: opts.pairs,
            max_frames: opts.max_frames,
            now_ms,
        },
    )?;
    if generated.pairs.is_empty() {
        bail!(
            "no question/answer pairs could be generated from {}: {} frames, {} sessions. The \
             generators derive every pair from captured text, so an empty or reference-free \
             history has nothing to evaluate (0.4.5 D17 forbids seeding questions by hand).",
            opts.db.display(),
            generated.frames_total,
            generated.session_count
        );
    }
    fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let pairs_path = out_dir.join("pairs.jsonl");
    write_pairs(&pairs_path, &generated.pairs)?;

    let client = ApiClient::new(&opts.api, &token, Duration::from_secs(opts.timeout_secs))
        .map_err(anyhow::Error::msg)?;
    let health = client.health().await.map_err(anyhow::Error::msg)?;

    let mut runs: Vec<TurnRun> = Vec::with_capacity(generated.pairs.len());
    let mut consecutive_unavailable = 0usize;
    let mut aborted: Option<String> = None;
    for pair in &generated.pairs {
        let retrieval = client.search(&pair.question, 10).await;
        match client.ask(&pair.question, opts.top_k).await {
            Ok(turn) => {
                consecutive_unavailable = 0;
                let cited_frames = db.cited_frames(&turn.cited_frames)?;
                let cited_sessions = db.cited_sessions(&turn.cited_sessions)?;
                let mut run = TurnRun::success(
                    pair,
                    turn.answer,
                    cited_frames,
                    cited_sessions,
                    turn.latency_ms,
                    turn.terminal,
                );
                apply_retrieval(&mut run, retrieval);
                runs.push(run);
            }
            Err(failure) => {
                if failure.class == FailureClass::ApiUnavailable {
                    consecutive_unavailable += 1;
                } else {
                    consecutive_unavailable = 0;
                }
                let mut run = TurnRun::failed(pair, failure);
                apply_retrieval(&mut run, retrieval);
                runs.push(run);
                if consecutive_unavailable >= ABORT_AFTER_UNAVAILABLE {
                    aborted = Some(format!(
                        "stopped after {consecutive_unavailable} consecutive `api_unavailable` \
                         failures (the answer model is not loaded yet, or the app is not \
                         reachable); the remaining pairs were not asked"
                    ));
                    break;
                }
            }
        }
    }

    let runs_path = out_dir.join("runs.jsonl");
    write_runs(&runs_path, &runs)?;

    let metrics = metrics::compute(&generated.pairs, &runs);
    let entity = entity_check(&db, &generated)?;

    let history_local = match generated.history {
        Some((start, end)) => Some((db.local_datetime(start)?, db.local_datetime(end)?)),
        None => None,
    };
    let command_line = format!(
        "cargo run -p harness --bin harness -- eval --db \"{}\" --api {} --seed {} --pairs {} --out \"{}\"",
        opts.db.display(),
        opts.api,
        seed,
        opts.pairs,
        out_dir.display()
    );
    let report = report::render(&report::ReportInput {
        meta: report::RunMeta {
            run_id: &run_id,
            started_local: &started_local.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
            seed,
            db_path: &opts.db.display().to_string(),
            api_base: client.base(),
            health: Some(&health),
            schema_version,
            history_local,
            now_ms,
            pairs_requested: opts.pairs,
            max_frames: opts.max_frames,
            duration_secs: started_at.elapsed().as_secs(),
            aborted: aborted.as_deref(),
        },
        generated: &generated,
        metrics: &metrics,
        runs: &runs,
        entity: &entity,
        command_line: &command_line,
    });
    let report_path = out_dir.join("report.md");
    fs::write(&report_path, &report)
        .with_context(|| format!("writing {}", report_path.display()))?;
    let report_json_path = out_dir.join("report.json");
    let report_json = serde_json::to_string_pretty(&ReportJson {
        run_id: &run_id,
        started_local: &started_local.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
        seed,
        db: &opts.db.display().to_string(),
        api: client.base(),
        schema_version,
        now_ms,
        pairs_requested: opts.pairs,
        pairs_generated: generated.pairs.len(),
        duration_secs: started_at.elapsed().as_secs(),
        generators: &generated.stats,
        metrics: &metrics,
        entity: entity_json(&entity),
        aborted: aborted.as_deref(),
    })
    .context("serializing report.json")?;
    fs::write(&report_json_path, &report_json)
        .with_context(|| format!("writing {}", report_json_path.display()))?;

    if let Some(reason) = &aborted {
        println!("warning: run aborted early: {reason}");
    }
    print!("{report}");

    Ok(EvalRun {
        run_id,
        out_dir,
        report_path,
        report_json_path,
        pairs_path,
        runs_path,
        report,
        metrics,
        pairs: generated.pairs.clone(),
        runs,
        generated,
    })
}

fn apply_retrieval(run: &mut TurnRun, result: Result<Vec<i64>, String>) {
    match result {
        Ok(ids) => run.retrieval_ids = ids,
        Err(error) => run.retrieval_error = Some(error),
    }
}

/// The unlabeled entity check: recovery of the reference entities and invention in the graph.
pub fn entity_check(db: &Db, generated: &GenOutcome) -> Result<report::EntityCheck> {
    let reference_entities = generated.reference_entities.len();
    let entities = db.entities()?;
    let aliases = db.aliases()?;
    if !db.has_entity_surface() || entities.is_empty() {
        return Ok(report::EntityCheck {
            surface: false,
            reference_entities,
            reference_discovered: generated.reference_discovered,
            entity_rows: entities.len(),
            alias_rows: aliases.len(),
            recovered: 0,
            invention_checked: 0,
            invented: 0,
        });
    }

    // Recovery: a reference is recovered when any entity name, normalized key or alias equals it
    // (case-insensitive). Names are compared verbatim; normalized keys cover path/URL folding.
    let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (_, _, canonical, normalized) in &entities {
        known.insert(canonical.to_lowercase());
        known.insert(normalized.to_lowercase());
    }
    for (_, alias) in &aliases {
        known.insert(alias.to_lowercase());
    }
    let recovered = generated
        .reference_entities
        .iter()
        .filter(|r| {
            known.contains(&r.text.to_lowercase()) || known.contains(&r.normalized.to_lowercase())
        })
        .count();

    // Invention: an entity is invented when its canonical name and every alias appear in no
    // frame's window title, URL or content text.
    let mut aliases_by_entity: BTreeMap<i64, Vec<&str>> = BTreeMap::new();
    for (id, alias) in &aliases {
        aliases_by_entity
            .entry(*id)
            .or_default()
            .push(alias.as_str());
    }
    let mut invented = 0usize;
    for (id, _, canonical, _) in &entities {
        let mut found = db.text_present(canonical)?;
        if !found {
            for alias in aliases_by_entity.get(id).into_iter().flatten() {
                if db.text_present(alias)? {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            invented += 1;
        }
    }

    Ok(report::EntityCheck {
        surface: true,
        reference_entities,
        reference_discovered: generated.reference_discovered,
        entity_rows: entities.len(),
        alias_rows: aliases.len(),
        recovered,
        invention_checked: entities.len(),
        invented,
    })
}

/// The `report.json` document: the same numbers as the Markdown, without the prose.
#[derive(Serialize)]
struct ReportJson<'a> {
    run_id: &'a str,
    started_local: &'a str,
    seed: u64,
    db: &'a str,
    api: &'a str,
    schema_version: Option<i64>,
    now_ms: i64,
    pairs_requested: usize,
    pairs_generated: usize,
    duration_secs: u64,
    generators: &'a [GenStat],
    metrics: &'a Metrics,
    entity: EntityJson,
    aborted: Option<&'a str>,
}

#[derive(Serialize)]
struct EntityJson {
    surface: bool,
    reference_entities: usize,
    reference_discovered: usize,
    entity_rows: usize,
    alias_rows: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invention: Option<usize>,
    invention_checked: usize,
}

fn entity_json(check: &report::EntityCheck) -> EntityJson {
    EntityJson {
        surface: check.surface,
        reference_entities: check.reference_entities,
        reference_discovered: check.reference_discovered,
        entity_rows: check.entity_rows,
        alias_rows: check.alias_rows,
        recovery: if check.surface && check.reference_entities > 0 {
            Some(check.recovered as f64 / check.reference_entities as f64)
        } else {
            None
        },
        invention: if check.surface {
            Some(check.invented)
        } else {
            None
        },
        invention_checked: check.invention_checked,
    }
}

fn write_pairs(path: &Path, pairs: &[Pair]) -> Result<()> {
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut out = BufWriter::new(file);
    for pair in pairs {
        serde_json::to_writer(&mut out, pair).context("serializing a pair")?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(())
}

/// One `runs.jsonl` line: the cited arrays and the terminal event, passed through verbatim so a
/// field a later phase adds to that event is readable without changing this crate.
#[derive(Serialize)]
struct RunLine<'a> {
    pair_id: &'a str,
    generator: Generator,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<&'a TurnFailure>,
    cited_frames: Vec<i64>,
    cited_sessions: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retrieval_error: Option<&'a str>,
    answer_chars: usize,
    terminal_event: &'a Value,
}

fn write_runs(path: &Path, runs: &[TurnRun]) -> Result<()> {
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut out = BufWriter::new(file);
    for run in runs {
        let terminal = match &run.failure {
            Some(failure) => failure.terminal.as_ref().unwrap_or(&run.terminal),
            None => &run.terminal,
        };
        let line = RunLine {
            pair_id: &run.pair_id,
            generator: run.generator,
            ok: run.failure.is_none(),
            latency_ms: run.latency_ms,
            failure: run.failure.as_ref(),
            cited_frames: run.cited_frames.iter().map(|f| f.id).collect(),
            cited_sessions: run.cited_sessions.iter().map(|s| s.id).collect(),
            retrieval_error: run.retrieval_error.as_deref(),
            answer_chars: run.answer.chars().count(),
            terminal_event: terminal,
        };
        serde_json::to_writer(&mut out, &line).context("serializing a run line")?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(())
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn number<T: std::str::FromStr>(args: &[String], name: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    match flag(args, name) {
        Some(value) => value
            .parse::<T>()
            .map(Some)
            .map_err(|e| anyhow::anyhow!("{name} expects a number, got {value:?} ({e})")),
        None => Ok(None),
    }
}
