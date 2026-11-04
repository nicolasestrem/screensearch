//! The paste-ready report: one Markdown document with the run identity, the per-generator pair
//! counts, the five metrics overall and per generator, every invalid citation, every failure, the
//! exact metric definitions, and an empty Gate A threshold column for the maintainer to fill.

use super::client::HealthInfo;
use super::gen::GenOutcome;
use super::metrics::{Aggregate, Metrics, TurnRun};

/// Run identity and environment, printed at the top of the report.
pub struct RunMeta<'a> {
    pub run_id: &'a str,
    pub started_local: &'a str,
    pub seed: u64,
    pub db_path: &'a str,
    pub api_base: &'a str,
    pub health: Option<&'a HealthInfo>,
    pub schema_version: Option<i64>,
    pub history_local: Option<(String, String)>,
    pub now_ms: i64,
    pub pairs_requested: usize,
    pub max_frames: Option<usize>,
    pub duration_secs: u64,
    /// Set when the run stopped early (e.g. the answer model never became available).
    pub aborted: Option<&'a str>,
}

/// The unlabeled entity check (recovery + invention), or the honest absence of a surface.
pub struct EntityCheck {
    pub surface: bool,
    pub reference_entities: usize,
    pub reference_discovered: usize,
    pub entity_rows: usize,
    pub alias_rows: usize,
    pub recovered: usize,
    pub invention_checked: usize,
    pub invented: usize,
}

/// Everything the renderer needs.
pub struct ReportInput<'a> {
    pub meta: RunMeta<'a>,
    pub generated: &'a GenOutcome,
    pub metrics: &'a Metrics,
    pub runs: &'a [TurnRun],
    pub entity: &'a EntityCheck,
    pub command_line: &'a str,
}

pub fn render(input: &ReportInput<'_>) -> String {
    let m = &input.meta;
    let gen = input.generated;
    let mut out = String::new();

    out.push_str("# ScreenSearch P4 automatic intelligence evaluation\n\n");
    out.push_str(&format!("- Run id: `{}`\n", m.run_id));
    out.push_str(&format!("- Started: {}\n", m.started_local));
    out.push_str(&format!(
        "- Seed: `{}` (`--seed {}` reproduces this exact pair set against this database)\n",
        m.seed, m.seed
    ));
    out.push_str(&format!(
        "- Database: `{}` (opened read-only: `SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only`)\n",
        m.db_path
    ));
    out.push_str(&format!(
        "- Schema version: {}\n",
        m.schema_version
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown (no `schema_version` row)".to_string())
    ));
    match m.health {
        Some(h) => out.push_str(&format!(
            "- API: `{}` (version {}, uptime {} s, capturing: {})\n",
            m.api_base,
            h.version,
            h.uptime_secs,
            if h.capturing { "yes" } else { "no" }
        )),
        None => out.push_str(&format!("- API: `{}`\n", m.api_base)),
    }
    out.push_str(&format!(
        "- Pairs requested: {} \u{b7} generated: {}\n",
        m.pairs_requested,
        gen.pairs.len()
    ));
    out.push_str(&format!(
        "- Frames: {} scanned for reference discovery (of {} total) \u{b7} {} in the exact reference pass{}\n",
        gen.discovery_frames,
        gen.frames_total,
        gen.reference_pass_frames,
        match m.max_frames {
            Some(cap) => format!(" (capped at {cap}; involvement is exact within that cap)"),
            None => String::new(),
        }
    ));
    out.push_str(&format!("- Sessions: {}\n", gen.session_count));
    match (&m.history_local, gen.history) {
        (Some((start, end)), _) => {
            out.push_str(&format!("- History: {} \u{2192} {} (local)\n", start, end));
        }
        (None, _) => out.push_str("- History: empty\n"),
    }
    out.push_str(&format!(
        "- Open-session overlap basis (`now_ms`): {}\n",
        m.now_ms
    ));
    out.push_str(&format!("- Duration: {} s\n", m.duration_secs));
    if let Some(reason) = m.aborted {
        out.push_str(&format!(
            "\n> **Run aborted early.** {reason}. Metrics below cover only the turns that ran.\n"
        ));
    }
    out.push('\n');

    if gen.pairs.len() < m.pairs_requested {
        out.push_str(&format!(
            "> **Pair shortfall.** {} of {} requested pairs were generated. Every generator emits \
             only what the database actually holds, so a fixture (or a young history) yields fewer \
             pairs than the several hundred the roadmap expects from real history. The metrics below \
             are computed over the pairs that exist; no pair is invented to fill the gap.\n\n",
            gen.pairs.len(),
            m.pairs_requested
        ));
    }

    out.push_str("## Pair generation\n\n");
    out.push_str("| Generator | Eligible | Target | Generated | Notes |\n");
    out.push_str("|---|---:|---:|---:|---|\n");
    for stat in &gen.stats {
        let eligible = if stat.truncated {
            format!("\u{2265} {}", stat.eligible)
        } else {
            stat.eligible.to_string()
        };
        let note = match (&stat.note, stat.truncated) {
            (Some(note), true) => format!("{note}; enumeration stopped at its cap"),
            (Some(note), false) => note.clone(),
            (None, true) => "enumeration stopped at its cap".to_string(),
            (None, false) => String::from("\u{2014}"),
        };
        out.push_str(&format!(
            "| {} (`{}`) | {} | {} | {} | {} |\n",
            stat.generator.label(),
            stat.generator.tag(),
            eligible,
            stat.target,
            stat.generated,
            cell(&note)
        ));
    }
    out.push('\n');
    out.push_str(
        "`Eligible` counts the pairs the generator could derive from this database (a lower bound \
         when marked `\u{2265}`). `Target` is the generator's even share of the requested total; a \
         deficit is handed to the other generators in fixed order (`last_seen`, `involvement`, \
         `sequence`, `window_recall`, `seen_again`), one pair at a time.\n\n",
    );

    out.push_str("## Metrics\n\n");
    out.push_str(
        "| Scope | Pairs | Evidence recall@10 | Retrieval recall@10 (diagnostic) | Citation validity | Known fact present | Latency p50 (ms) | Latency p95 (ms) | Gate A threshold |\n",
    );
    out.push_str("|---|---:|---:|---:|---:|---:|---:|---:|---|\n");
    row(&mut out, "overall", &input.metrics.overall);
    for gm in &input.metrics.per_generator {
        row(
            &mut out,
            &format!("{} (`{}`)", gm.generator.label(), gm.generator.tag()),
            &gm.aggregate,
        );
    }
    out.push('\n');

    let overall = &input.metrics.overall;
    out.push_str("### Turns\n\n");
    out.push_str(&format!(
        "- successful: {} / {}\n",
        overall.turns_ok, overall.pairs
    ));
    out.push_str(&format!(
        "- failures: api_unavailable {}, timeout {}, empty_answer {}, http_error {}\n\n",
        count(
            &overall.failures,
            super::client::FailureClass::ApiUnavailable
        ),
        count(&overall.failures, super::client::FailureClass::Timeout),
        count(&overall.failures, super::client::FailureClass::EmptyAnswer),
        count(&overall.failures, super::client::FailureClass::HttpError),
    ));

    out.push_str("## Failures\n\n");
    let failed: Vec<&TurnRun> = input.runs.iter().filter(|r| r.failure.is_some()).collect();
    if failed.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        out.push_str("| Pair | Generator | Class | Detail |\n|---|---|---|---|\n");
        for run in failed {
            let failure = run.failure.as_ref().expect("filtered");
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {} |\n",
                run.pair_id,
                run.generator.label(),
                failure.class.label(),
                cell(&failure.detail)
            ));
        }
        out.push('\n');
    }

    out.push_str("## Invalid citations\n\n");
    if input.metrics.invalid.is_empty() {
        out.push_str("_none \u{2014} every cited id exists and lies inside its pair's window_\n\n");
    } else {
        out.push_str("| Pair | Generator | Cited kind | Id | Reason |\n|---|---|---|---:|---|\n");
        for invalid in &input.metrics.invalid {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                invalid.pair_id,
                invalid.generator.label(),
                invalid.kind.as_str(),
                invalid.id,
                invalid.reason
            ));
        }
        out.push('\n');
    }

    out.push_str("## Entity check (independent reference extractor vs. the work graph)\n\n");
    let e = input.entity;
    if !e.surface {
        out.push_str("- Work-graph surface: absent (`entities` table not present or empty)\n");
        out.push_str(&format!(
            "- Reference entities sampled from the discovery frames: {} of {}\n",
            e.reference_entities, e.reference_discovered
        ));
        out.push_str("- Recovery: n/a (no entity surface)\n");
        out.push_str("- Invention: n/a (no entity surface)\n\n");
    } else {
        out.push_str(&format!(
            "- Work-graph surface: present ({} entities, {} aliases)\n",
            e.entity_rows, e.alias_rows
        ));
        out.push_str(&format!(
            "- Reference entities sampled from the discovery frames: {} of {}\n",
            e.reference_entities, e.reference_discovered
        ));
        out.push_str(&format!(
            "- Recovery: {}\n",
            rate_counts(e.recovered, e.reference_entities)
        ));
        out.push_str(&format!(
            "- Invention: {} invented of {} checked\n\n",
            e.invented, e.invention_checked
        ));
    }

    out.push_str("## Metric definitions (the comparator Gate A freezes)\n\n");
    out.push_str(METRIC_DEFINITIONS);

    out.push_str(
        "## Files written beside this report\n\n`report.md` (this document), `report.json` (the \
         same numbers, machine-readable), `pairs.jsonl` (every pair with its question, expected ids \
         and expected strings) and `runs.jsonl` (one line per turn: timing, cited arrays, terminal \
         event verbatim).\n\n",
    );
    out.push_str("## Reproduce\n\n```\n");
    out.push_str(input.command_line);
    out.push_str("\n```\n\n");
    out.push_str(
        "Nothing in this report was typed by hand: the question/answer pairs are regenerated from \
         the database on every run (0.4.5 D17), so no stored question set, label file or answer key \
         exists in the repository.\n",
    );
    out
}

fn row(out: &mut String, scope: &str, agg: &Aggregate) {
    out.push_str(&format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} |  |\n",
        scope,
        agg.pairs,
        agg.evidence_recall10().map(percent).unwrap_or_else(n_a),
        agg.retrieval_recall10()
            .map(percent)
            .unwrap_or_else(|| "n/a".to_string()),
        match agg.citation_validity() {
            Some(v) => format!(
                "{} ({}/{})",
                percent(v),
                agg.cited_ids - agg.invalid_ids,
                agg.cited_ids
            ),
            None => "n/a (no citations)".to_string(),
        },
        agg.known_fact_rate().map(percent).unwrap_or_else(n_a),
        agg.p50.map(|v| v.to_string()).unwrap_or_else(n_a),
        agg.p95.map(|v| v.to_string()).unwrap_or_else(n_a),
    ));
}

fn n_a() -> String {
    "n/a".to_string()
}

fn percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn rate_counts(hit: usize, total: usize) -> String {
    if total == 0 {
        return "n/a (no reference entities)".to_string();
    }
    format!(
        "{:.1}% ({}/{})",
        hit as f64 / total as f64 * 100.0,
        hit,
        total
    )
}

fn count(
    failures: &std::collections::BTreeMap<super::client::FailureClass, usize>,
    class: super::client::FailureClass,
) -> usize {
    failures.get(&class).copied().unwrap_or(0)
}

/// A table cell: pipes escaped, whitespace collapsed, length bounded.
fn cell(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let escaped = collapsed.replace('|', "\\|");
    if escaped.chars().count() <= 240 {
        return escaped;
    }
    let mut out: String = escaped.chars().take(240).collect();
    out.push('\u{2026}');
    out
}

const METRIC_DEFINITIONS: &str = "\
**1. Evidence recall@10 (the comparator).** For one pair: the pair's expected ids with sessions \
and frames interleaved, a session first (s1, f1, s2, f2, and so on), first 10 taken, score = matched / \
min(10, expected count). A cited id set is the turn's cited frame ids plus its cited session ids; \
an expected **session** id also counts as matched when a cited frame's `session_id` equals it. The \
aggregate is the arithmetic mean of the per-pair scores, overall and per generator. Citations are \
the only evidence the product exposes to a client (`03 §7i.3`), so this is what Gate A freezes.

**2. Retrieval recall@10 (diagnostic, never mixed into 1).** Per pair, whether the pair's first \
expected frame id appears among the first 10 `frame_id`s of `GET /v1/search?q=<question>&limit=10&include_chrome=false`. \
Pairs without expected frames, or whose search call failed, are not measured and do not enter the \
denominator.

**3. Citation validity.** `valid / total` over every cited id of every turn (the kickoff states \
its complement, `invalid / total`; both counts appear next to the rate). A cited frame is valid \
when it exists and its `captured_at` lies inside the pair's window; a pair with no window means the \
whole history. A cited session is valid when it exists and its interval overlaps the window (a \
session with `ended_at` NULL is open-ended). Every invalid id is printed above with its pair and \
the reason.

**4. Known fact present.** The answer, lowercased and whitespace-collapsed, contains at least one \
of the pair's expected strings (normalized the same way), or for `last_seen` the expected local \
`YYYY-MM-DD`. The rate is over all pairs of the scope: a failed turn has no answer and counts as a \
miss.

**5. Latency p50 / p95.** Per turn, wall clock milliseconds from the `POST /v1/ask` request to the \
terminal `done` or `error` event, over successful turns only, nearest-rank percentile \
(`ceil(p*n)`). Failures are counted separately by class: `api_unavailable`, `timeout`, \
`empty_answer`, `http_error`.

**Generator ground truth (the field scope each family matches).** `last_seen` matches the reference, ASCII-case-insensitively, in `frames.window_title`, `frames.browser_url` or content text (`frame_text.content_text`, falling back to legacy `ocr_text.text`), newest first. `involvement` matches `window_title`/`browser_url` only - the fields a repository or domain appears in per `03 §7h.2` - and orders the owning sessions by `started_at`. `sequence` takes the session with the greatest `started_at` strictly below the target session's, and skips the pair when that is not unique (another session starts at the same instant as either one). `window_recall` takes the sessions whose interval overlaps the local `[06:00, 12:00)` window of that date, and of their frames only those captured inside that window, so expected evidence is never an out-of-window citation. `seen_again` takes the exact FTS5 match set (`frame_text_fts`, porter tokenizer) of a whitespace-collapsed line of 24 to 160 characters that occurs in at least two and at most 50 frames. Each pair keeps at most ten expected ids of each kind, because the comparator is `@10`.

An expected string for `seen_again` is the quoted line itself, so that family's known-fact check can be satisfied by echoing the question; it is reported per generator for exactly that reason, never only in the overall mean.

**Entity recovery and invention.** The reference entities are the distinct references the narrow \
independent extractor found in the sampled frames (`file`: an absolute Windows path with a drive \
letter; `domain`: the host of a full URL; `repo`: `<host>/<owner>/<repo>` on a known code host, \
from a URL or a bare token; `document`: a bare file name with a document extension; `ref`: `#123` \
or `ABC-123`; `app` is not extracted because nothing in captured text names an application \
unambiguously). Recovery is the fraction of those references matched by `entities.canonical_name`, \
`entities.normalized_key` or an `entity_aliases.alias` (case-insensitive). Invention counts \
`entities` rows whose `canonical_name` and every alias appear in no frame's `window_title`, \
`browser_url` or content text. On a database whose graph tables are empty the section says \
`n/a (no entity surface)` instead of a fabricated zero.

";
