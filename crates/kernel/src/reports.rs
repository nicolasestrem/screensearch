//! Recall report generation — Calendar-Grid Coverage Map-Reduce (`03 §8b`,
//! `docs/0.2.0.md` PR6).
//!
//! A weekly report must cover the *whole* week, not just its most-recent or
//! most-relevant frames. So the orchestrator does **not** grow the model context
//! window (that scales KV-cache VRAM and forces a sidecar relaunch); instead the
//! model window stays pinned flat and the **number of bounded passes scales with the
//! range**: the range is split into a per-period grid (one calendar period each),
//! every active period gets its own frame budget — so a dense Monday cannot starve a
//! quiet Saturday — and is summarized by **one or more** MAP passes (a period holding
//! more content than a single window is split into pass-sized sub-batches, so a dense day
//! is never truncated to one window). The per-period summaries are then combined by a
//! bounded hierarchical REDUCE. Coverage is structural in both axes: every period with
//! frames is summarized (the per-period floor guarantees each contributes), and within a
//! period every sampled frame is read across the split passes.
//!
//! Two retrieval paths feed the same map→reduce machinery (`docs/0.2.0.md` PR6):
//! - **coverage** (Daily / Weekly / Custom *without* a prompt): one group per active
//!   period, each evenly sampled across its own time window
//!   ([`Store::sample_frames_in_range`]);
//! - **relevance** (Custom *with* a prompt): the most relevant frames in the range via
//!   [`Store::hybrid_search`], grouped into token-budget batches.
//!
//! Grounding is always `content_text` (via [`Store::ocr_texts`], never raw text); the
//! report cites the frames the model actually read; empty ranges produce honest
//! no-evidence output with **no** sidecar call.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use chrono::Datelike;
use traits::{
    AnswerOpts, AnswerProvider, ReportConfig, ReportMode, ReportOutcome, ReportOutput, ReportRange,
    ReportRun, RetrievedChunk, SearchHit, SearchQuery, SegmenterFrame, Store, TimeRange,
    TimelineBucket,
};

const DAY_MS: i64 = 86_400_000;

/// Coverage floor: every active period contributes at least this many frames, even
/// when the global cap (`reports.weekly_top_k`) would round its quota below it. Keeps
/// long ranges from collapsing coverage. A structural constant (not a user setting) —
/// bounded above by [`MAX_PERIODS`] so the floor can't blow past the global cap.
const MIN_FRAMES_PER_PERIOD: u32 = 3;
/// Children combined per parent at each hierarchical reduce level.
const REDUCE_FANOUT: usize = 6;
/// Cap on the period grid for long Custom ranges — beyond this the period widens to
/// multi-day buckets so the pass count stays bounded on weak hardware.
const MAX_PERIODS: u32 = 31;
/// Hard ceiling on total sidecar passes (map + reduce + final). If a pathological
/// density would exceed it, the report stops coarsening rather than adding calls and
/// is flagged `truncated` (honest framing).
const MAX_REPORT_PASSES: u32 = 64;
/// Cap on cited frame ids returned (the UI renders a subset anyway); the honest
/// `frames_summarized` count still reflects the full union.
const MAX_REPORT_CITATIONS: usize = 200;

/// Fallback answer context window used only when the provider doesn't report its own
/// (e.g. a test fake) — matches the answer lane's `sidecar.ctx_size` default. The live
/// budget comes from [`AnswerProvider::answer_context_budget`] so a user-lowered
/// `ctx_size` is respected. A heuristic for the reduce-fits gate and batch splitting
/// only — the inference crate's `build_summary_messages` is the hard bound that actually
/// prevents an overflowing prompt.
const REPORT_CTX_TOKENS: usize = 8192;
const REPORT_TEMPLATE_OVERHEAD: usize = 96;
const REPORT_ID_FRAMING: usize = 6;

/// MAP step: summarize one slice (period / batch) into a terse paragraph.
const MAP_SYSTEM_PROMPT: &str = "You summarize one slice of the user's screen activity \
from the provided content snippets, each tagged with its source frame id. Write a \
short, factual paragraph of what the user worked on and saw in this slice. Use ONLY \
the snippets — do not invent activity. Be terse; this will be combined with other \
slices.";

/// REDUCE step: combine several slice summaries into one, preserving time order.
const REDUCE_SYSTEM_PROMPT: &str = "You combine several section summaries of the user's \
screen activity into a single coherent, factual summary. Merge overlapping themes, \
keep the chronological order, and use ONLY the provided summaries — do not invent \
activity.";

/// FINAL step: write the user-facing report from snippets and/or section summaries.
const FINAL_SYSTEM_PROMPT: &str = "You write the user's recall report from the provided \
material (content snippets and/or dated section summaries) about their screen activity \
over a time range. Produce clear, well-structured markdown: a short overview, then the \
key activities and topics grouped sensibly and in time order. Use ONLY the provided \
material and be specific. If it shows little or no evidence, say so plainly rather \
than inventing activity.";

/// A callback the orchestrator invokes before each pass so the command can emit a
/// `report_progress` event. Kept impl-agnostic (the kernel never depends on Tauri):
/// `(stage_label, done, total)`.
pub type ReportProgress<'a> = dyn Fn(&str, u32, u32) + Send + Sync + 'a;

/// One slice of work fed to the MAP step: a human label plus its content chunks.
struct Group {
    label: String,
    chunks: Vec<RetrievedChunk>,
}

enum CoverageScope {
    TimeRange,
    Session(Vec<SegmenterFrame>),
}

#[derive(Clone, Copy)]
struct EvidenceFrame {
    frame_id: i64,
    captured_at: i64,
}

/// Generates a recall report over `[start, end)` (`03 §8b`). `range`/`prompt` select
/// the retrieval path and framing; `timezone` is the saved IANA zone for durable
/// memory reports (local calendar-day coverage grid, `03 §8c`) and `None` keeps the
/// legacy exact-UTC behavior; `cfg` carries the settings-derived knobs. Emits
/// progress via `progress` and cooperatively cancels (between passes) when `cancel`
/// is set. Returns the assembled [`ReportOutput`] (markdown + citations + auditable
/// coverage/cost metadata). Errors (a sidecar failure, or cancellation) propagate as
/// `Err`; the command maps them.
#[allow(clippy::too_many_arguments)]
pub async fn generate_report(
    store: &dyn Store,
    answer: &dyn AnswerProvider,
    range: ReportRange,
    start: i64,
    end: i64,
    prompt: Option<&str>,
    timezone: Option<&str>,
    cfg: ReportConfig,
    progress: Option<&ReportProgress<'_>>,
    cancel: &AtomicBool,
) -> Result<ReportOutput> {
    generate_report_scoped(
        store,
        answer,
        range,
        start,
        end,
        prompt,
        timezone,
        cfg,
        progress,
        cancel,
        CoverageScope::TimeRange,
    )
    .await
}

/// Runs the existing coverage-first report pipeline over exactly one persisted
/// session's owned frames. Different session tracks may overlap in time, so the
/// source is `frames.session_id`, never a time-range fallback.
pub async fn generate_session_recap(
    store: &dyn Store,
    answer: &dyn AnswerProvider,
    session_id: i64,
    cfg: ReportConfig,
    progress: Option<&ReportProgress<'_>>,
    cancel: &AtomicBool,
) -> Result<ReportOutput> {
    let session = store
        .get_session(session_id)
        .await?
        .ok_or_else(|| anyhow!("session {session_id} not found"))?;
    let frames = store.session_frames_meta(session_id).await?;
    let start = frames
        .first()
        .map_or(session.started_at, |frame| frame.captured_at);
    let end = frames
        .last()
        .map(|frame| frame.captured_at.saturating_add(1))
        .or_else(|| session.ended_at.map(|ended| ended.saturating_add(1)))
        .unwrap_or_else(|| session.started_at.saturating_add(1))
        .max(start.saturating_add(1));
    generate_report_scoped(
        store,
        answer,
        ReportRange::Custom,
        start,
        end,
        None,
        None,
        cfg,
        progress,
        cancel,
        CoverageScope::Session(frames),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn generate_report_scoped(
    store: &dyn Store,
    answer: &dyn AnswerProvider,
    range: ReportRange,
    start: i64,
    end: i64,
    prompt: Option<&str>,
    timezone: Option<&str>,
    cfg: ReportConfig,
    progress: Option<&ReportProgress<'_>>,
    cancel: &AtomicBool,
    scope: CoverageScope,
) -> Result<ReportOutput> {
    let prompt = if range == ReportRange::MemoryDaily {
        None
    } else {
        prompt
    };
    let prompt = prompt.map(str::trim).filter(|p| !p.is_empty());
    let opts = AnswerOpts {
        thinking: false,
        max_tokens: cfg.reply_budget,
    };
    // Budget the batch/reduce planning against the provider's *actual* answer-lane context
    // window — the user can lower `sidecar.ctx_size`, and assuming a larger window would let
    // the planner pack summaries the sidecar then silently truncates. Falls back to the
    // pinned default for providers that don't report one (e.g. the test fake).
    let ctx_tokens = answer
        .answer_context_budget()
        .map(|c| c as usize)
        .unwrap_or(REPORT_CTX_TOKENS);

    // --- Grouping: relevance batches (prompt) vs coverage grid (time) ---
    let (groups, periods_total, total_in_range) = if let Some(p) = prompt {
        let hits = store
            .hybrid_search(&SearchQuery {
                text: p.to_string(),
                limit: cfg.weekly_top_k,
                time_range: Some(TimeRange { start, end }),
                include_chrome: false,
            })
            .await?;
        let retrieved = hits.len() as u64;
        let chunks = hydrate_hits(store, hits).await?;
        let batches = split_chunks_into_batches(chunks, cfg.reply_budget, ctx_tokens);
        let groups: Vec<Group> = batches
            .into_iter()
            .enumerate()
            .map(|(i, chunks)| Group {
                label: format!("part {}", i + 1),
                chunks,
            })
            .collect();
        let periods_total = groups.len() as u32;
        (groups, periods_total, retrieved)
    } else {
        // Coverage grid: legacy Daily/Weekly/Custom (and any window without a saved
        // zone) keep the uniform UTC grid. A durable memory report that carries its
        // saved IANA zone uses local calendar-day periods instead (`03 §8c`) so a
        // DST-transition day counts once (25 h day → one period) and N local days →
        // N periods; long spans group whole local days to stay under `MAX_PERIODS`.
        let (buckets, periods_total, local_days) = match (range, timezone, &scope) {
            (ReportRange::MemoryDaily, Some(zone), CoverageScope::TimeRange) => {
                let periods =
                    crate::memory_schedule::local_day_grid(zone, start, end, MAX_PERIODS)?;
                let mut buckets = Vec::with_capacity(periods.len());
                for (bucket_start, bucket_end) in periods {
                    // One count query per local-day period (`bucket_count = 1`
                    // returns the whole period as a single sparse bucket), mirroring
                    // the existing per-period sampling query below.
                    let count = store
                        .timeline_buckets(bucket_start, bucket_end, 1)
                        .await?
                        .first()
                        .map_or(0, |bucket| bucket.count);
                    buckets.push(TimelineBucket {
                        start: bucket_start,
                        end: bucket_end,
                        count,
                    });
                }
                let total = buckets.len() as u32;
                (buckets, total, true)
            }
            _ => {
                let grid = grid_size(start, end, MAX_PERIODS);
                let buckets = match &scope {
                    CoverageScope::TimeRange => store.timeline_buckets(start, end, grid).await?,
                    CoverageScope::Session(frames) => session_buckets(frames, start, end, grid),
                };
                (buckets, grid, false)
            }
        };
        let total_in_range: u64 = buckets.iter().map(|b| u64::from(b.count)).sum();
        let counts: Vec<u32> = buckets.iter().map(|b| b.count).collect();
        let quotas = plan_depth(&counts, cfg.daily_top_k, cfg.weekly_top_k);
        // Sample each active period's frames (one windowed query per period — inherent to
        // per-period coverage), then hydrate **all** of them with a single bulk `ocr_texts`
        // read rather than one query per period (avoids an N+1 over the grid).
        let mut periods_frames: Vec<Vec<EvidenceFrame>> = Vec::with_capacity(buckets.len());
        for (b, quota) in buckets.iter().zip(quotas) {
            let frames = match &scope {
                CoverageScope::TimeRange => store
                    .sample_frames_in_range(b.start, b.end, quota)
                    .await?
                    .into_iter()
                    .map(|frame| EvidenceFrame {
                        frame_id: frame.frame_id,
                        captured_at: frame.captured_at,
                    })
                    .collect(),
                CoverageScope::Session(frames) => {
                    sample_session_bucket(frames, b.start, b.end, quota)
                }
            };
            periods_frames.push(frames);
        }
        let all_ids: Vec<i64> = periods_frames
            .iter()
            .flatten()
            .map(|f| f.frame_id)
            .collect();
        let texts = store.ocr_texts(&all_ids).await?;
        let mut groups = Vec::new();
        for (index, (b, frames)) in buckets.iter().zip(periods_frames).enumerate() {
            let day = if local_days {
                // Local calendar periods are consecutive; the index is the local day
                // ordinal (25 h/23 h days still count once).
                index as i64 + 1
            } else {
                ((b.start - start) / DAY_MS + 1).max(1)
            };
            let chunks: Vec<RetrievedChunk> = frames
                .into_iter()
                .filter_map(|f| {
                    texts
                        .get(&f.frame_id)
                        .filter(|t| !t.trim().is_empty())
                        .map(|t| RetrievedChunk {
                            frame_id: f.frame_id,
                            text: t.clone(),
                            score: 0.0,
                            captured_at: f.captured_at,
                        })
                })
                .collect();
            if !chunks.is_empty() {
                groups.push(Group {
                    label: format!("day {day}"),
                    chunks,
                });
            }
        }
        (groups, periods_total, total_in_range)
    };

    let frames_sampled: usize = groups.iter().map(|g| g.chunks.len()).sum();
    if frames_sampled == 0 {
        return Ok(empty_output(periods_total));
    }
    // "Trimmed to fit" means evidence was left unsummarized, not merely filtered.
    // - Coverage path: `total_in_range` is the real in-range frame count, so a sample
    //   below it genuinely dropped frames.
    // - Relevance path: `total_in_range` is the *pre-hydration* hit count; `hydrate_hits`
    //   drops empty-text hits (no evidence), so `frames_sampled < total_in_range` fires
    //   even when nothing useful was lost. The honest signal there is "did we hit the
    //   search cap?" — i.e. more relevant frames likely existed beyond what we asked for.
    let mut truncated = if prompt.is_some() {
        total_in_range >= cfg.weekly_top_k as u64
    } else {
        (frames_sampled as u64) < total_in_range
    };

    // Bound every group to a single map pass. The map step summarizes each group in exactly
    // one pass, and a dense period — or a whole single-period Daily range, which is one
    // group — can hold far more content than one 8192 window. Without this it would be
    // summarized in one truncated pass, silently dropping frames despite the per-period
    // coverage guarantee (the daily-report "more was captured than summarized" bug). Splitting
    // an over-large group into pass-sized sub-batches lets the map fan out *within* the period
    // and the reduce fold the parts back, so coverage is complete, not merely per-period.
    // Preserve period membership before splitting; count only periods with actually read frames.
    let period_frames: Vec<std::collections::HashSet<i64>> = groups
        .iter()
        .map(|group| group.chunks.iter().map(|chunk| chunk.frame_id).collect())
        .collect();
    let covered_periods = |ids: &[i64]| {
        period_frames
            .iter()
            .filter(|period| ids.iter().any(|id| period.contains(id)))
            .count() as u32
    };
    let groups = split_groups_to_fit(groups, cfg.reply_budget, ctx_tokens);

    // --- Single-pass fast path (small ranges that fit one window) ---
    // Collapse to a single final pass only when the whole report genuinely fits one window:
    // a lone fitted group always qualifies, and a small report (<= `map_reduce_min_frames`
    // frames) collapses too *iff* its combined content still fits — never truncating, since a
    // dense report falls through to map → reduce where every pass is already bounded above.
    let collapse = groups.len() == 1
        || (frames_sampled <= cfg.map_reduce_min_frames as usize
            && chunks_fit_one_pass(&groups, cfg.reply_budget, ctx_tokens));
    if collapse {
        check_cancel(cancel)?;
        emit(progress, "Writing report", 1, 1);
        let mut all: Vec<RetrievedChunk> = groups.into_iter().flat_map(|g| g.chunks).collect();
        if prompt.is_none() {
            all.sort_by_key(|c| c.captured_at); // coverage report narrates a timeline
        }
        let (markdown, cited) = answer
            .summarize(
                FINAL_SYSTEM_PROMPT,
                &final_instruction(range, prompt, start, end),
                &all,
                opts,
            )
            .await?;
        let (cited, frame_ids) = report_citations(cited);
        let summarized = frame_ids.len() as u32;
        let periods_covered = covered_periods(&frame_ids);
        let truncated = truncated || summarized < frames_sampled as u32;
        return Ok(ReportOutput {
            markdown,
            cited_frame_ids: cited,
            frame_ids,
            mode: ReportMode::SinglePass,
            periods_total,
            periods_covered,
            frames_sampled: frames_sampled as u32,
            frames_summarized: summarized,
            passes: 1,
            truncated,
        });
    }

    // --- MAP: one summarize pass per group ---
    let group_count = groups.len() as u32;
    let mut nodes: Vec<String> = Vec::new();
    let mut union: Vec<i64> = Vec::new();
    let mut passes: u32 = 0;
    for (i, g) in groups.into_iter().enumerate() {
        check_cancel(cancel)?;
        emit(
            progress,
            &format!("Summarizing {} of {}", i + 1, group_count),
            i as u32 + 1,
            group_count,
        );
        let instruction = format!("Summarize {} of this range.", g.label);
        let (text, cited) = answer
            .summarize(MAP_SYSTEM_PROMPT, &instruction, &g.chunks, opts)
            .await?;
        passes += 1;
        if !text.trim().is_empty() {
            for id in cited {
                if !union.contains(&id) {
                    union.push(id);
                }
            }
            nodes.push(format!("{}: {}", g.label, text.trim()));
        }
        if passes >= MAX_REPORT_PASSES {
            truncated = true;
            break;
        }
    }
    if nodes.is_empty() {
        return Ok(empty_output(periods_total));
    }

    // --- REDUCE: bounded hierarchical fan-in, time order preserved ---
    while nodes.len() > 1 && !fits_single_pass(&nodes, cfg.reply_budget, ctx_tokens) {
        if passes >= MAX_REPORT_PASSES {
            truncated = true;
            break;
        }
        let mut next = Vec::with_capacity(nodes.len() / REDUCE_FANOUT + 1);
        for chunk_group in nodes.chunks(REDUCE_FANOUT) {
            // A trailing group of one needs no combining — pass it through untouched
            // rather than spend a model call summarizing a single summary.
            if chunk_group.len() == 1 {
                next.push(chunk_group[0].clone());
                continue;
            }
            check_cancel(cancel)?;
            emit(progress, "Combining summaries", passes, MAX_REPORT_PASSES);
            let (text, _) = answer
                .summarize(
                    REDUCE_SYSTEM_PROMPT,
                    "",
                    &nodes_as_chunks(chunk_group),
                    opts,
                )
                .await?;
            passes += 1;
            next.push(text.trim().to_string());
            if passes >= MAX_REPORT_PASSES {
                truncated = true;
                break;
            }
        }
        nodes = next;
        if truncated {
            break;
        }
    }

    // --- FINAL: the user-facing report ---
    check_cancel(cancel)?;
    emit(progress, "Writing report", group_count, group_count);
    let (markdown, _) = answer
        .summarize(
            FINAL_SYSTEM_PROMPT,
            &final_instruction(range, prompt, start, end),
            &nodes_as_chunks(&nodes),
            opts,
        )
        .await?;
    passes += 1;

    let (cited, frame_ids) = report_citations(union);
    let summarized = frame_ids.len() as u32;
    let periods_covered = covered_periods(&frame_ids);
    let truncated = truncated || summarized < frames_sampled as u32;
    Ok(ReportOutput {
        markdown,
        cited_frame_ids: cited,
        frame_ids,
        mode: ReportMode::MapReduce,
        periods_total,
        periods_covered,
        frames_sampled: frames_sampled as u32,
        frames_summarized: summarized,
        passes,
        truncated,
    })
}

/// One day of weekly synthesis material (`03 §8e`): either a group to summarize from that
/// day's recorded evidence, or — when none of it is still readable — the saved daily
/// prose carried as a pre-computed section node.
enum WeeklyMaterial {
    Group(Group),
    Saved { date: String, text: String },
}

/// Weekly synthesis (`03 §8e`): one report over a week's saved daily reports, running
/// through the same map → reduce → final machinery and the same prompts as every other
/// report — no second engine, no second model path. Inputs are exactly the saved dailies
/// that own a local day of the week in this run's saved zone (via
/// `Store::saved_daily_runs` and [`weekly_consumed_with_dates`]: a multi-day or explicit
/// range run and a daily saved under another zone are never consumed, so a date is never
/// credited by a run that does not own it): their recorded evidence IDs are re-read in one
/// bulk hydration (`Store::ocr_texts`) — never a fresh sample of the raw window — and a
/// day whose recorded evidence is gone (retention) contributes its saved prose as a
/// pre-computed section node, so a pruned frame can never silently drop a day. Each day is
/// labelled with its own persisted local date, its daily run ID and its recorded session
/// IDs, and the consumed manifest rides the collapse/final framing, so every item the
/// model writes can cite the daily run, frame and session IDs the framing requires — on
/// the single-pass collapse too, where day labels never reach the model as nodes. A window
/// whose saved dailies yield no material at all returns the honest empty output; the
/// service layer decides that a positive probe with no material is a generation failure.
#[allow(clippy::too_many_arguments)]
pub async fn generate_weekly_synthesis(
    store: &dyn Store,
    answer: &dyn AnswerProvider,
    dailies: &[ReportRun],
    start: i64,
    end: i64,
    timezone: &str,
    cfg: ReportConfig,
    progress: Option<&ReportProgress<'_>>,
    cancel: &AtomicBool,
) -> Result<ReportOutput> {
    let opts = AnswerOpts {
        thinking: false,
        max_tokens: cfg.reply_budget,
    };
    let ctx_tokens = answer
        .answer_context_budget()
        .map(|c| c as usize)
        .unwrap_or(REPORT_CTX_TOKENS);
    let zone: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| anyhow!("unknown IANA time zone"))?;
    let days = weekly_day_periods(timezone, start, end)?;
    let periods_total = days.len() as u32;
    let week = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(start)
        .ok_or_else(|| anyhow!("timestamp outside supported calendar"))?
        .with_timezone(&zone)
        .date_naive()
        .iso_week();
    let week_label = format!("{}-W{:02}", week.year(), week.week());

    // --- Weekly inputs (`03 §8e`): the saved dailies that own a local day of this week in
    // this run's own zone, paired with the date they own and kept in persisted (day) order.
    let consumed = weekly_consumed_with_dates(dailies, start, end, timezone)?;

    // One bulk hydration of every consumed daily's recorded evidence IDs (deduplicated,
    // first occurrence wins). This is the only evidence read — the weekly never re-samples
    // the raw window.
    let mut evidence_ids: Vec<i64> = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for (run, _) in &consumed {
        if let Some(result) = &run.result {
            for id in &result.frame_ids {
                if seen_ids.insert(*id) {
                    evidence_ids.push(*id);
                }
            }
        }
    }
    let texts = store.ocr_texts(&evidence_ids).await?;

    // Day materials in persisted order (the store returns runs oldest first); every day is
    // labelled by its own persisted provenance, and the same labels form the consumed
    // manifest the collapse/final framing carries.
    let mut materials: Vec<WeeklyMaterial> = Vec::new();
    let mut manifest: Vec<String> = Vec::new();
    let mut frame_dates: std::collections::BTreeMap<i64, String> =
        std::collections::BTreeMap::new();
    for (run, date) in &consumed {
        let Some(result) = &run.result else { continue };
        let label = weekly_day_label(date, &run.run_id, &result.session_ids);
        manifest.push(label.clone());
        let chunks: Vec<RetrievedChunk> = result
            .frame_ids
            .iter()
            .filter_map(|id| {
                texts
                    .get(id)
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| RetrievedChunk {
                        frame_id: *id,
                        text: text.clone(),
                        score: 0.0,
                        captured_at: run.window_start,
                    })
            })
            .collect();
        if chunks.is_empty() {
            let prose = result.markdown.trim();
            if !prose.is_empty() {
                materials.push(WeeklyMaterial::Saved {
                    date: date.clone(),
                    text: format!("{label}: {prose}"),
                });
            }
        } else {
            for chunk in &chunks {
                frame_dates.insert(chunk.frame_id, date.clone());
            }
            materials.push(WeeklyMaterial::Group(Group { label, chunks }));
        }
    }
    // Sampled evidence is measured in distinct frame IDs — the same unit as
    // `frames_summarized` — so a base daily and its regenerate revision consuming the same
    // frames neither inflates the recorded total nor fakes a sampling shortfall.
    let frames_sampled: usize = {
        let mut sampled = std::collections::BTreeSet::new();
        for material in &materials {
            if let WeeklyMaterial::Group(group) = material {
                for chunk in &group.chunks {
                    sampled.insert(chunk.frame_id);
                }
            }
        }
        sampled.len()
    };
    if materials.is_empty() {
        return Ok(empty_output(periods_total));
    }
    // Recorded evidence that could not be read back (retention) or carried no text is
    // unsummarized evidence, not a mere filter — the same honest truncation rule as the
    // daily coverage path, over distinct frame IDs on both sides.
    let mut truncated = (frames_sampled as u64) < (evidence_ids.len() as u64);

    // The consumed manifest rides the collapse/final framing: the citation demand names the
    // daily run of every day described, and the manifest makes it satisfiable on both paths
    // (the collapse flattens day labels away; the reduce path is where the user-facing
    // prose is written).
    let final_prompt = format!(
        "{}\n\n{}",
        weekly_final_instruction(&week_label, start, end),
        weekly_manifest(&manifest)
    );

    // --- Single-pass fast path (mirrors the daily coverage path) ---
    // Collapse only when every day contributes live evidence: a lone group always
    // qualifies, and a small week collapses too iff its evidence still fits one window.
    // With saved-prose nodes in play the reduce/final path runs so days keep their order.
    let has_saved_nodes = materials
        .iter()
        .any(|material| matches!(material, WeeklyMaterial::Saved { .. }));
    let groups_only: Vec<&Group> = materials
        .iter()
        .filter_map(|material| match material {
            WeeklyMaterial::Group(group) => Some(group),
            WeeklyMaterial::Saved { .. } => None,
        })
        .collect();
    let collapse = !has_saved_nodes
        && (groups_only.len() == 1
            || (frames_sampled <= cfg.map_reduce_min_frames as usize
                && chunks_fit(
                    groups_only.iter().flat_map(|group| &group.chunks),
                    cfg.reply_budget,
                    ctx_tokens,
                )));
    if collapse {
        check_cancel(cancel)?;
        emit(progress, "Writing report", 1, 1);
        let mut all: Vec<RetrievedChunk> = materials
            .into_iter()
            .flat_map(|material| match material {
                WeeklyMaterial::Group(group) => group.chunks,
                WeeklyMaterial::Saved { .. } => Vec::new(),
            })
            .collect();
        all.sort_by_key(|chunk| chunk.captured_at); // the week narrates in day order
        let (markdown, cited) = answer
            .summarize(FINAL_SYSTEM_PROMPT, &final_prompt, &all, opts)
            .await?;
        let (cited, frame_ids) = report_citations(cited);
        let summarized = frame_ids.len() as u32;
        let truncated = truncated || summarized < frames_sampled as u32;
        let periods_covered =
            covered_local_days(&frame_dates, &std::collections::BTreeSet::new(), &frame_ids);
        return Ok(ReportOutput {
            markdown,
            cited_frame_ids: cited,
            frame_ids,
            mode: ReportMode::SinglePass,
            periods_total,
            periods_covered,
            frames_sampled: frames_sampled as u32,
            frames_summarized: summarized,
            passes: 1,
            truncated,
        });
    }

    // --- MAP: one pass per evidence day; saved-prose days pass through untouched ---
    let group_count = groups_only.len() as u32;
    let mut nodes: Vec<String> = Vec::new();
    let mut consumed_saved_dates: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut union: Vec<i64> = Vec::new();
    let mut passes: u32 = 0;
    let mut done: u32 = 0;
    for material in materials {
        match material {
            WeeklyMaterial::Saved { date, text } => {
                consumed_saved_dates.insert(date);
                nodes.push(text);
            }
            WeeklyMaterial::Group(group) => {
                check_cancel(cancel)?;
                done += 1;
                emit(
                    progress,
                    &format!("Summarizing {done} of {group_count}"),
                    done,
                    group_count,
                );
                let instruction = format!("Summarize {} of this week.", group.label);
                let (text, cited) = answer
                    .summarize(MAP_SYSTEM_PROMPT, &instruction, &group.chunks, opts)
                    .await?;
                passes += 1;
                if !text.trim().is_empty() {
                    for id in cited {
                        if !union.contains(&id) {
                            union.push(id);
                        }
                    }
                    nodes.push(format!("{}: {}", group.label, text.trim()));
                }
                if passes >= MAX_REPORT_PASSES {
                    truncated = true;
                    break;
                }
            }
        }
    }
    if nodes.is_empty() {
        return Ok(empty_output(periods_total));
    }

    // --- REDUCE: bounded hierarchical fan-in, day order preserved (mirrors the daily path) ---
    while nodes.len() > 1 && !fits_single_pass(&nodes, cfg.reply_budget, ctx_tokens) {
        if passes >= MAX_REPORT_PASSES {
            truncated = true;
            break;
        }
        let mut next = Vec::with_capacity(nodes.len() / REDUCE_FANOUT + 1);
        for chunk_group in nodes.chunks(REDUCE_FANOUT) {
            // A trailing group of one needs no combining — pass it through untouched
            // rather than spend a model call summarizing a single summary.
            if chunk_group.len() == 1 {
                next.push(chunk_group[0].clone());
                continue;
            }
            check_cancel(cancel)?;
            emit(progress, "Combining summaries", passes, MAX_REPORT_PASSES);
            let (text, _) = answer
                .summarize(
                    REDUCE_SYSTEM_PROMPT,
                    "",
                    &nodes_as_chunks(chunk_group),
                    opts,
                )
                .await?;
            passes += 1;
            next.push(text.trim().to_string());
            if passes >= MAX_REPORT_PASSES {
                truncated = true;
                break;
            }
        }
        nodes = next;
        if truncated {
            break;
        }
    }

    // --- FINAL: the user-facing weekly synthesis ---
    check_cancel(cancel)?;
    let sections = nodes.len() as u32;
    emit(progress, "Writing report", sections, sections);
    let (markdown, _) = answer
        .summarize(
            FINAL_SYSTEM_PROMPT,
            &final_prompt,
            &nodes_as_chunks(&nodes),
            opts,
        )
        .await?;
    passes += 1;

    let (cited, frame_ids) = report_citations(union);
    let summarized = frame_ids.len() as u32;
    let periods_covered = covered_local_days(&frame_dates, &consumed_saved_dates, &frame_ids);
    let truncated = truncated || summarized < frames_sampled as u32;
    Ok(ReportOutput {
        markdown,
        cited_frame_ids: cited,
        frame_ids,
        mode: ReportMode::MapReduce,
        periods_total,
        periods_covered,
        frames_sampled: frames_sampled as u32,
        frames_summarized: summarized,
        passes,
        truncated,
    })
}

/// The week's local-day periods with their `YYYY-MM-DD` dates (`03 §8e`): the weekly
/// window is an ISO week, so the shared local-day grid is exactly one period per local
/// date (a DST-transition day stays one period).
fn weekly_day_periods(timezone: &str, start: i64, end: i64) -> Result<Vec<(String, i64, i64)>> {
    let zone: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| anyhow!("unknown IANA time zone"))?;
    crate::memory_schedule::local_day_grid(timezone, start, end, MAX_PERIODS)?
        .into_iter()
        .map(|(period_start, period_end)| {
            Ok((local_date(zone, period_start)?, period_start, period_end))
        })
        .collect()
}

/// Whether a saved daily owns one local date of the week (`03 §8e` weekly inputs and
/// coverage): saved in the weekly run's own zone, not an explicit range run, its
/// **persisted** `local_date` is that date, and its own window lies **entirely inside**
/// that date's local period. A manual multi-day or date-range run spans more than one
/// local day and never covers one; a daily saved under another zone is never silently
/// re-labelled into this zone — it credits nothing, so its dates stay missing/partial.
fn daily_owns_local_date(
    run: &ReportRun,
    date: &str,
    day_start: i64,
    day_end: i64,
    timezone: &str,
) -> bool {
    run.requested_range.is_none()
        && run.timezone == timezone
        && run.local_date == date
        && run.window_start >= day_start
        && run.window_end <= day_end
}

/// Every weekly-consumable saved daily paired with the local date it owns (`03 §8e`
/// inputs), in the given order. A multi-day or explicit range run, a run saved under
/// another zone, and a finished run without a saved result are not consumed — they are
/// not "the saved daily reports in the window".
fn weekly_consumed_with_dates<'a>(
    dailies: &'a [ReportRun],
    start: i64,
    end: i64,
    timezone: &str,
) -> Result<Vec<(&'a ReportRun, String)>> {
    let days = weekly_day_periods(timezone, start, end)?;
    Ok(dailies
        .iter()
        .filter(|run| run.result.is_some())
        .filter_map(|run| {
            days.iter()
                .find_map(|(date, day_start, day_end)| {
                    daily_owns_local_date(run, date, *day_start, *day_end, timezone)
                        .then(|| date.clone())
                })
                .map(|date| (run, date))
        })
        .collect())
}

/// The saved dailies a weekly synthesis may consume (`03 §8e` inputs), as owned runs:
/// exactly those that own a local date of `[start, end)` in `timezone` — see
/// [`daily_owns_local_date`]. Multi-day and explicit range runs and dailies saved under
/// another zone are excluded; their dates are never credited, so the week cannot be
/// saved as complete coverage.
pub(crate) fn weekly_consumed_dailies(
    dailies: &[ReportRun],
    start: i64,
    end: i64,
    timezone: &str,
) -> Result<Vec<ReportRun>> {
    Ok(weekly_consumed_with_dates(dailies, start, end, timezone)?
        .into_iter()
        .map(|(run, _)| run.clone())
        .collect())
}

/// `03 §8e` citation rule: a weekly report citing no daily run for a day it describes is
/// a defect. Whether the generated markdown actually cites at least one consumed daily
/// run ID — the post-generation check the service enforces as `GENERATION_FAILED`, so
/// uncited prose is never persisted as a successful synthesis. An ID counts only as a
/// whole token: a match embedded in a longer identifier is not a citation.
pub(crate) fn weekly_cites_consumed_daily(markdown: &str, consumed: &[ReportRun]) -> bool {
    let is_id_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    consumed.iter().any(|run| {
        !run.run_id.is_empty()
            && markdown.match_indices(run.run_id.as_str()).any(|(at, id)| {
                let before = markdown[..at].chars().next_back();
                let after = markdown[at + id.len()..].chars().next();
                !before.is_some_and(is_id_char) && !after.is_some_and(is_id_char)
            })
    })
}

/// Every local calendar date in `[start, end)` (`03 §8e` missing-day signal) that has no
/// weekly-consumable saved daily covering it, or whose covering dailies are all marked
/// partial/truncated (run outcome `PARTIAL`, or the saved report's `truncated` flag — a
/// run without a saved result is never complete coverage). A daily covers a date only
/// when its own window lies entirely inside that date's local period and it was saved in
/// this run's zone (see [`daily_owns_local_date`]); multi-day/range runs and dailies from
/// another zone never credit a date. Oldest-first. Base runs and revisions are consumed
/// exactly as returned: no revision precedence is defined, so the date counts as covered
/// when at least one covering saved daily is complete.
pub(crate) fn weekly_missing_local_dates(
    dailies: &[ReportRun],
    start: i64,
    end: i64,
    timezone: &str,
) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for (date, period_start, period_end) in weekly_day_periods(timezone, start, end)? {
        let complete = dailies.iter().any(|run| {
            daily_owns_local_date(run, &date, period_start, period_end, timezone)
                && !daily_is_partial(run)
        });
        if !complete {
            missing.push(date);
        }
    }
    Ok(missing)
}

/// A saved daily counts as partial coverage when its run outcome is `PARTIAL`
/// (in-progress range, missing retained evidence — `03 §8c`) or its saved report is
/// flagged truncated. A saved daily without a result is never complete coverage.
fn daily_is_partial(run: &ReportRun) -> bool {
    run.outcome == Some(ReportOutcome::Partial)
        || match &run.result {
            Some(result) => result.truncated,
            None => true,
        }
}

/// The local `YYYY-MM-DD` date of a UTC instant in the report's saved zone.
fn local_date(zone: chrono_tz::Tz, at: i64) -> Result<String> {
    Ok(chrono::DateTime::<chrono::Utc>::from_timestamp_millis(at)
        .ok_or_else(|| anyhow!("timestamp outside supported calendar"))?
        .with_timezone(&zone)
        .date_naive()
        .to_string())
}

/// `day 2026-10-14 (daily run <id>; sessions 3, 7)` — the day's persisted provenance, so
/// every item the model writes can cite the daily run, frame and session IDs.
fn weekly_day_label(date: &str, run_id: &str, session_ids: &[i64]) -> String {
    if session_ids.is_empty() {
        format!("day {date} (daily run {run_id})")
    } else {
        let sessions: Vec<String> = session_ids.iter().map(|id| id.to_string()).collect();
        format!(
            "day {date} (daily run {run_id}; sessions {})",
            sessions.join(", ")
        )
    }
}

/// The consumed saved dailies' provenance for the weekly framing — one
/// [`weekly_day_label`] per consumed run in day order — so the framing's citation demand
/// ("the daily run ID of each day it describes") is satisfiable from the material the
/// model actually has, on the single-pass collapse too, where day labels never reach the
/// model as nodes.
fn weekly_manifest(labels: &[String]) -> String {
    let mut out = String::from(
        "Consumed saved daily reports (the daily run of every day you describe must be \
         cited; these are the runs this week consumed):",
    );
    for label in labels {
        out.push_str("\n- ");
        out.push_str(label);
    }
    out
}

/// The weekly synthesis framing (`03 §8e`): fixed headings, exact ISO-week bounds, the
/// citation rule (the daily run of every day described; the frame and session IDs relied
/// on) and the no-over-claiming rule (tentative screen text is never a commitment).
fn weekly_final_instruction(label: &str, start: i64, end: i64) -> String {
    let timestamp = |value: i64| {
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(value)
            .map(|date| date.to_rfc3339())
            .unwrap_or_else(|| value.to_string())
    };
    format!(
        "Write the weekly synthesis for {label} [{} inclusive, {} exclusive) from the saved daily \
         reports and their evidence. Use the headings Activity, Explicitly evidenced decisions, \
         Open questions, and Next steps. Every item must cite the IDs it rests on — the daily run \
         ID of each day it describes, plus the frame and session IDs it relied on — and never \
         describe a day without citing that day's daily run. Include only supported facts; never \
         state tentative screen text as a commitment. If a section has no evidence, say so. Use \
         the exact week bounds rather than relative claims about this week.",
        timestamp(start),
        timestamp(end)
    )
}

/// Local days represented in the synthesis: days whose summarized frames the model read,
/// plus days carried by their saved prose (evidence no longer readable).
fn covered_local_days(
    frame_dates: &std::collections::BTreeMap<i64, String>,
    saved_dates: &std::collections::BTreeSet<String>,
    summarized: &[i64],
) -> u32 {
    let mut dates: std::collections::BTreeSet<&str> =
        saved_dates.iter().map(String::as_str).collect();
    for id in summarized {
        if let Some(date) = frame_dates.get(id) {
            dates.insert(date.as_str());
        }
    }
    dates.len() as u32
}

/// Hydrate relevance hits with their `content_text` in one bulk read (`Store::ocr_texts`
/// returns `content_text`, never raw), falling back to the FTS snippet when a frame's
/// text is unavailable; relevance order (best-first) is preserved.
async fn hydrate_hits(store: &dyn Store, hits: Vec<SearchHit>) -> Result<Vec<RetrievedChunk>> {
    let ids: Vec<i64> = hits.iter().map(|h| h.frame_id).collect();
    let texts = store.ocr_texts(&ids).await?;
    Ok(hits
        .into_iter()
        .filter_map(|h| {
            let text = texts.get(&h.frame_id).cloned().unwrap_or(h.snippet);
            if text.trim().is_empty() {
                None
            } else {
                Some(RetrievedChunk {
                    frame_id: h.frame_id,
                    text,
                    score: h.score,
                    captured_at: h.captured_at,
                })
            }
        })
        .collect())
}

fn session_buckets(
    frames: &[SegmenterFrame],
    start: i64,
    end: i64,
    bucket_count: u32,
) -> Vec<TimelineBucket> {
    if frames.is_empty() || end <= start || bucket_count == 0 {
        return Vec::new();
    }
    let Some(span) = end.checked_sub(start) else {
        return Vec::new();
    };
    let count = i64::from(bucket_count);
    let width = (span / count + i64::from(span % count != 0)).max(1);
    let mut counts = std::collections::BTreeMap::<i64, u32>::new();
    for frame in frames {
        let bucket =
            (frame.captured_at.saturating_sub(start) / width).clamp(0, count.saturating_sub(1));
        let entry = counts.entry(bucket).or_default();
        *entry = entry.saturating_add(1);
    }
    counts
        .into_iter()
        .map(|(bucket, count)| {
            let bucket_start = start.saturating_add(bucket.saturating_mul(width));
            TimelineBucket {
                start: bucket_start,
                end: bucket_start.saturating_add(width).min(end),
                count,
            }
        })
        .collect()
}

fn sample_session_bucket(
    frames: &[SegmenterFrame],
    start: i64,
    end: i64,
    limit: u32,
) -> Vec<EvidenceFrame> {
    if limit == 0 {
        return Vec::new();
    }
    let in_bucket: Vec<_> = frames
        .iter()
        .filter(|frame| frame.captured_at >= start && frame.captured_at < end)
        .collect();
    let wanted = usize::try_from(limit)
        .unwrap_or(usize::MAX)
        .min(in_bucket.len());
    if wanted == 0 {
        return Vec::new();
    }
    if wanted == 1 {
        let frame = in_bucket[0];
        return vec![EvidenceFrame {
            frame_id: frame.id,
            captured_at: frame.captured_at,
        }];
    }
    (0..wanted)
        .map(|index| {
            let frame = in_bucket[index * (in_bucket.len() - 1) / (wanted - 1)];
            EvidenceFrame {
                frame_id: frame.id,
                captured_at: frame.captured_at,
            }
        })
        .collect()
}

/// Per-active-period frame quota (pure — the coverage guarantee lives here). Aims for
/// `daily_top_k` frames per period, scaling down to fit the global `weekly_top_k` cap
/// when there are many periods, but never below [`MIN_FRAMES_PER_PERIOD`] (the floor
/// wins so every active period is represented). Each quota is capped at the period's
/// actual frame count.
fn plan_depth(period_counts: &[u32], daily_top_k: u32, weekly_top_k: u32) -> Vec<u32> {
    let a = period_counts.len() as u32;
    if a == 0 {
        return Vec::new();
    }
    let per_period = if daily_top_k.saturating_mul(a) <= weekly_top_k {
        daily_top_k
    } else {
        (weekly_top_k / a).max(MIN_FRAMES_PER_PERIOD)
    };
    let target = per_period.max(MIN_FRAMES_PER_PERIOD).max(1);
    period_counts.iter().map(|&c| target.min(c)).collect()
}

/// The period-grid size: one bucket per calendar day, capped at `max_periods` (a long
/// Custom range widens each period to multi-day buckets). Pure.
fn grid_size(start: i64, end: i64, max_periods: u32) -> u32 {
    if end <= start {
        return 0;
    }
    let span = end - start;
    let n_days = ((span + DAY_MS - 1) / DAY_MS) as u32; // ceil, >= 1
    n_days.clamp(1, max_periods.max(1))
}

/// Whether the section summaries fit a single (final) pass within `ctx_tokens` (the
/// provider's effective context window). Conservative heuristic; the inference budgeter
/// is the hard bound.
fn fits_single_pass(nodes: &[String], reply_budget: u32, ctx_tokens: usize) -> bool {
    let reserve =
        reply_budget as usize + est_tokens(FINAL_SYSTEM_PROMPT) + REPORT_TEMPLATE_OVERHEAD;
    let budget = ctx_tokens.saturating_sub(reserve);
    let used: usize = nodes
        .iter()
        .map(|n| est_tokens(n) + REPORT_ID_FRAMING)
        .sum();
    used <= budget
}

/// Splits relevance chunks (prompt path) into batches each fitting one map pass within
/// `ctx_tokens` (the provider's effective context window). Pure.
fn split_chunks_into_batches(
    chunks: Vec<RetrievedChunk>,
    reply_budget: u32,
    ctx_tokens: usize,
) -> Vec<Vec<RetrievedChunk>> {
    let reserve = reply_budget as usize + est_tokens(MAP_SYSTEM_PROMPT) + REPORT_TEMPLATE_OVERHEAD;
    let per_batch = ctx_tokens.saturating_sub(reserve).max(1);
    let mut batches: Vec<Vec<RetrievedChunk>> = Vec::new();
    let mut cur: Vec<RetrievedChunk> = Vec::new();
    let mut spent = 0usize;
    for c in chunks {
        if c.text.trim().is_empty() {
            continue;
        }
        let cost = est_tokens(c.text.trim()) + REPORT_ID_FRAMING;
        if !cur.is_empty() && spent + cost > per_batch {
            batches.push(std::mem::take(&mut cur));
            spent = 0;
        }
        cur.push(c);
        spent += cost;
    }
    if !cur.is_empty() {
        batches.push(cur);
    }
    batches
}

/// Bounds each group to a single map pass by splitting any whose content overflows one
/// window into pass-sized sub-batches (via [`split_chunks_into_batches`]), preserving chunk
/// order. A group that already fits is returned unchanged (its label preserved); a split
/// group's parts are suffixed "(part k of n)" so the per-period structure stays legible.
/// This is what guarantees within-period completeness — the map step summarizes each group
/// in one pass, so an unsplit dense period would be truncated to whatever fits.
fn split_groups_to_fit(groups: Vec<Group>, reply_budget: u32, ctx_tokens: usize) -> Vec<Group> {
    let mut out = Vec::with_capacity(groups.len());
    for Group { label, chunks } in groups {
        let batches = split_chunks_into_batches(chunks, reply_budget, ctx_tokens);
        let n = batches.len();
        if n <= 1 {
            for chunks in batches {
                out.push(Group {
                    label: label.clone(),
                    chunks,
                });
            }
        } else {
            for (i, chunks) in batches.into_iter().enumerate() {
                out.push(Group {
                    label: format!("{label} (part {} of {n})", i + 1),
                    chunks,
                });
            }
        }
    }
    out
}

/// Whether all of `groups`' chunks fit a single pass within `ctx_tokens` — the gate for
/// collapsing a small multi-period report into one final pass without truncating. Mirrors
/// [`split_chunks_into_batches`]'s per-batch budget (a conservative heuristic; the inference
/// budgeter is the hard bound).
fn chunks_fit_one_pass(groups: &[Group], reply_budget: u32, ctx_tokens: usize) -> bool {
    chunks_fit(
        groups.iter().flat_map(|group| &group.chunks),
        reply_budget,
        ctx_tokens,
    )
}

/// Whether the given chunks fit a single map/final pass within `ctx_tokens` (shared core
/// of [`chunks_fit_one_pass`] and the weekly synthesis's collapse gate).
fn chunks_fit<'a>(
    chunks: impl Iterator<Item = &'a RetrievedChunk>,
    reply_budget: u32,
    ctx_tokens: usize,
) -> bool {
    let reserve = reply_budget as usize + est_tokens(MAP_SYSTEM_PROMPT) + REPORT_TEMPLATE_OVERHEAD;
    let budget = ctx_tokens.saturating_sub(reserve);
    let used: usize = chunks
        .filter(|c| !c.text.trim().is_empty())
        .map(|c| est_tokens(c.text.trim()) + REPORT_ID_FRAMING)
        .sum();
    used <= budget
}

/// Wraps section-summary strings as pseudo-chunks for a reduce/final pass (the frame
/// tag is a harmless list marker; real citations are tracked separately).
fn nodes_as_chunks(nodes: &[String]) -> Vec<RetrievedChunk> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, t)| RetrievedChunk {
            frame_id: i as i64,
            text: t.clone(),
            score: 0.0,
            captured_at: i as i64,
        })
        .collect()
}

/// Preserve full distinct provenance independently of the presentation citation cap.
fn report_citations(union: Vec<i64>) -> (Vec<i64>, Vec<i64>) {
    let mut seen = std::collections::HashSet::new();
    let full: Vec<_> = union.into_iter().filter(|id| seen.insert(*id)).collect();
    let cited = full.iter().take(MAX_REPORT_CITATIONS).copied().collect();
    (cited, full)
}

#[cfg(test)]
mod provenance_tests {
    #[test]
    fn full_provenance_survives_citation_cap_and_deduplicates() {
        let mut ids: Vec<i64> = (1..=350).collect();
        ids.push(1);
        let (cited, full) = super::report_citations(ids);
        assert_eq!(cited.len(), 200);
        assert_eq!(full.len(), 350);
        assert_eq!(full.last(), Some(&350));
    }
}

fn final_instruction(range: ReportRange, prompt: Option<&str>, start: i64, end: i64) -> String {
    let base = match range {
        // Durable memory framing (`03 §8c`): exact selected bounds and fixed headings;
        // the prompt is deliberately ignored (there is no user prompt for memory).
        ReportRange::MemoryDaily => {
            let timestamp = |value| {
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(value)
                    .map(|date| date.to_rfc3339())
                    .unwrap_or_else(|| value.to_string())
            };
            return format!("Write memory for the selected window [{} inclusive, {} exclusive]. Use the headings Activity, Explicitly evidenced decisions, Open questions, and Next steps. Include only supported facts; distinguish tentative screen text from commitments. If a section has no evidence, say so. Use the exact selected bounds rather than relative claims about today.", timestamp(start), timestamp(end));
        }
        ReportRange::Daily => "Write the recall report for today.",
        ReportRange::Weekly => "Write the recall report for the last 7 days.",
        ReportRange::Custom => "Write the recall report for the selected range.",
    };
    match prompt {
        Some(p) => format!("{base} Focus on: {p}"),
        None => base.to_string(),
    }
}

fn empty_output(periods_total: u32) -> ReportOutput {
    ReportOutput {
        markdown: "No screen activity was captured for this range — there is nothing to summarize."
            .to_string(),
        cited_frame_ids: Vec::new(),
        frame_ids: Vec::new(),
        mode: ReportMode::Empty,
        periods_total,
        periods_covered: 0,
        frames_sampled: 0,
        frames_summarized: 0,
        passes: 0,
        truncated: false,
    }
}

/// Conservative bytes-per-token estimate (mirrors the inference budgeter's upper
/// bound) — only a heuristic for the reduce-fits gate; the real bound is enforced in
/// the inference crate.
fn est_tokens(text: &str) -> usize {
    text.len() / 2 + 1
}

fn emit(progress: Option<&ReportProgress<'_>>, stage: &str, done: u32, total: u32) {
    if let Some(p) = progress {
        p(stage, done, total);
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(anyhow!("report cancelled"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use store::SqliteStore;
    use traits::{
        AnswerDelta, MemoryReportRange, NewFrame, NewSession, OcrResult, ReportError,
        ReportRunKind, ReportRunPhase, SavedReport, SessionHost, SessionKind,
    };

    use crate::report_service::{execute_weekly, WeeklyExecutionRequest};

    // ---- Pure-function tests (no store / no sidecar) ----

    #[test]
    fn grid_size_is_one_per_day_capped_at_max() {
        assert_eq!(grid_size(0, DAY_MS, MAX_PERIODS), 1); // daily
        assert_eq!(grid_size(0, 7 * DAY_MS, MAX_PERIODS), 7); // weekly
        assert_eq!(grid_size(0, 60 * DAY_MS, MAX_PERIODS), MAX_PERIODS); // widened
        assert_eq!(grid_size(10, 10, MAX_PERIODS), 0); // degenerate
    }

    #[test]
    fn plan_depth_gives_every_active_period_its_budget() {
        // 7 days, budget fits: each active period gets daily_top_k (capped at count).
        let counts = [50u32, 4, 30, 8, 12, 20, 40];
        let q = plan_depth(&counts, 40, 200);
        for (c, quota) in counts.iter().zip(&q) {
            assert!(*quota >= MIN_FRAMES_PER_PERIOD.min(*c));
            assert!(*quota <= *c, "quota {quota} must not exceed count {c}");
        }
        // A dense day cannot consume a quiet day's allotment (independent quotas).
        assert_eq!(q[1], 4); // quiet day capped at its 4 frames
    }

    #[test]
    fn plan_depth_floor_wins_over_global_cap_on_long_ranges() {
        // Many active periods, tight global cap: floor still guarantees >= 1 each.
        let counts = vec![10u32; 90];
        let q = plan_depth(&counts, 40, 200);
        assert_eq!(q.len(), 90);
        assert!(
            q.iter().all(|&x| x >= 1),
            "every active period must get at least one frame"
        );
        // The floor (3) wins even though 90 * 3 > the 200 global cap.
        assert!(q.iter().all(|&x| x >= MIN_FRAMES_PER_PERIOD));
    }

    #[test]
    fn fits_single_pass_detects_overflow() {
        let small = vec!["short summary".to_string(); 3];
        assert!(fits_single_pass(&small, 512, REPORT_CTX_TOKENS));
        let huge = vec!["x".repeat(8_000); 7]; // ~28k tokens >> 8192
        assert!(!fits_single_pass(&huge, 512, REPORT_CTX_TOKENS));
        // A smaller provider context tightens the gate: summaries that fit 8192 no longer
        // fit a lowered window (mirrors a user-lowered `sidecar.ctx_size`).
        let medium = vec!["lorem ipsum dolor ".repeat(60); 4];
        assert!(fits_single_pass(&medium, 512, REPORT_CTX_TOKENS));
        assert!(!fits_single_pass(&medium, 512, 1_024));
    }

    #[test]
    fn split_chunks_batches_every_chunk_without_dropping() {
        let chunks: Vec<RetrievedChunk> = (0..50)
            .map(|i| RetrievedChunk {
                frame_id: i,
                text: "lorem ipsum ".repeat(40),
                score: 0.0,
                captured_at: i,
            })
            .collect();
        let batches = split_chunks_into_batches(chunks, 512, REPORT_CTX_TOKENS);
        let total: usize = batches.iter().map(|b| b.len()).sum();
        assert_eq!(total, 50, "no chunk dropped");
        assert!(
            batches.len() > 1,
            "large input splits into multiple batches"
        );
    }

    // ---- Orchestrator tests (real in-memory store + fake answer provider) ----

    /// Records its calls and echoes a configurable-size summary, citing exactly the
    /// frames it was given — so the test can assert coverage + citation propagation
    /// without a real model. `big` forces large summaries to exercise the reduce tree.
    struct FakeAnswer {
        calls: Arc<AtomicUsize>,
        big: bool,
    }

    #[async_trait]
    impl AnswerProvider for FakeAnswer {
        async fn answer(
            &self,
            _query: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
            _tx: tokio::sync::mpsc::Sender<AnswerDelta>,
        ) -> Result<()> {
            Ok(())
        }
        async fn summarize(
            &self,
            _system: &str,
            instruction: &str,
            context: &[RetrievedChunk],
            _opts: AnswerOpts,
        ) -> Result<(String, Vec<i64>)> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let cited: Vec<i64> = context.iter().map(|c| c.frame_id).collect();
            let body = if self.big {
                "lorem ".repeat(700) // ~4.2k chars → ~2.1k tokens per node
            } else {
                format!("[{} frames]", context.len())
            };
            Ok((format!("{instruction} {body}"), cited))
        }
    }

    fn cfg() -> ReportConfig {
        ReportConfig {
            daily_top_k: 40,
            weekly_top_k: 200,
            map_reduce_min_frames: 20,
            reply_budget: 512,
        }
    }

    async fn seed_day(store: &SqliteStore, day_start: i64, n: usize) -> Vec<i64> {
        let mut ids = Vec::new();
        for i in 0..n {
            let at = day_start + (i as i64) * 1000;
            let fid = store
                .insert_frame(NewFrame {
                    captured_at: at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{at}.jpg"),
                    content_hash: format!("h{at}"),
                    app_hint: None,
                    window_title: None,
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
            store
                .insert_ocr(
                    fid,
                    OcrResult {
                        text: format!("content for frame {fid} at {at}"),
                        mean_confidence: -1.0,
                        engine: "test".to_string(),
                        spans: Vec::new(),
                    },
                )
                .await
                .unwrap();
            ids.push(fid);
        }
        ids
    }

    /// Like [`seed_day`] but with caller-supplied (large) `content_text`, to push a single
    /// period past one map pass and exercise within-period splitting. `insert_ocr` is the
    /// passthrough populator, so the text is stored verbatim.
    async fn seed_day_text(store: &SqliteStore, day_start: i64, n: usize, text: &str) -> Vec<i64> {
        let mut ids = Vec::new();
        for i in 0..n {
            let at = day_start + (i as i64) * 1000;
            let fid = store
                .insert_frame(NewFrame {
                    captured_at: at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{at}.jpg"),
                    content_hash: format!("h{at}"),
                    app_hint: None,
                    window_title: None,
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
            store
                .insert_ocr(
                    fid,
                    OcrResult {
                        text: text.to_string(),
                        mean_confidence: -1.0,
                        engine: "test".to_string(),
                        spans: Vec::new(),
                    },
                )
                .await
                .unwrap();
            ids.push(fid);
        }
        ids
    }

    #[tokio::test]
    async fn empty_range_is_honest_with_no_sidecar_call() {
        let store = SqliteStore::open_in_memory().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer {
            calls: calls.clone(),
            big: false,
        };
        let cancel = AtomicBool::new(false);
        let out = generate_report(
            &store,
            &answer,
            ReportRange::Daily,
            0,
            DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(out.mode, ReportMode::Empty);
        assert_eq!(out.passes, 0);
        assert!(out.cited_frame_ids.is_empty());
        assert_eq!(
            calls.load(Ordering::Relaxed),
            0,
            "no model call on empty range"
        );
    }

    struct FirstFrameOnly(FakeAnswer);
    #[async_trait]
    impl AnswerProvider for FirstFrameOnly {
        async fn answer(
            &self,
            query: &str,
            context: &[RetrievedChunk],
            opts: AnswerOpts,
            tx: tokio::sync::mpsc::Sender<AnswerDelta>,
        ) -> Result<()> {
            self.0.answer(query, context, opts, tx).await
        }
        async fn summarize(
            &self,
            system: &str,
            instruction: &str,
            context: &[RetrievedChunk],
            opts: AnswerOpts,
        ) -> Result<(String, Vec<i64>)> {
            self.0
                .summarize(system, instruction, &context[..context.len().min(1)], opts)
                .await
        }
    }

    #[tokio::test]
    async fn provider_context_loss_is_partial_and_provenance_matches_read_frames() {
        let store = SqliteStore::open_in_memory().unwrap();
        let expected = seed_day(&store, 0, 5).await;
        let answer = FirstFrameOnly(FakeAnswer {
            calls: Arc::new(AtomicUsize::new(0)),
            big: false,
        });
        let output = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            0,
            DAY_MS,
            Some("framing must never turn this into a relevance query"),
            None,
            cfg(),
            None,
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert_eq!(output.frames_sampled, 5);
        assert_eq!(output.frame_ids, vec![expected[0]]);
        assert_eq!(output.frames_summarized, 1);
        assert_eq!(output.periods_covered, 1);
        assert!(output.truncated);
    }

    #[tokio::test]
    async fn memory_daily_preserves_coverage_and_uses_selected_window_framing() {
        let store = SqliteStore::open_in_memory().unwrap();
        let expected = seed_day(&store, DAY_MS, 5).await;
        let answer = FakeAnswer {
            calls: Arc::new(AtomicUsize::new(0)),
            big: false,
        };
        let cancel = AtomicBool::new(false);
        let manual = generate_report(
            &store,
            &answer,
            ReportRange::Custom,
            DAY_MS,
            2 * DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        let memory = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            DAY_MS,
            2 * DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(memory.frame_ids, expected);
        assert_eq!(memory.frame_ids, manual.frame_ids);
        assert_eq!(memory.frames_sampled, manual.frames_sampled);
        assert_eq!(memory.periods_covered, manual.periods_covered);
        assert!(memory.markdown.contains("1970-01-02T00:00:00+00:00"));
        assert!(memory.markdown.contains("Explicitly evidenced decisions"));
    }

    #[tokio::test]
    async fn report_retains_full_evidence_beyond_display_cap() {
        let store = SqliteStore::open_in_memory().unwrap();
        let ids = seed_day(&store, 0, 350).await;
        let answer = FakeAnswer {
            calls: Arc::new(AtomicUsize::new(0)),
            big: false,
        };
        let mut config = cfg();
        config.daily_top_k = 350;
        config.weekly_top_k = 350;
        let output = generate_report(
            &store,
            &answer,
            ReportRange::Daily,
            0,
            DAY_MS,
            None,
            None,
            config,
            None,
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert_eq!(output.frame_ids, ids);
        assert_eq!(output.frames_summarized, 350);
        assert_eq!(output.cited_frame_ids.len(), MAX_REPORT_CITATIONS);
    }

    #[tokio::test]
    async fn daily_small_range_uses_single_pass() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_day(&store, 0, 5).await; // 5 frames in one day, < map_reduce_min_frames
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer {
            calls: calls.clone(),
            big: false,
        };
        let cancel = AtomicBool::new(false);
        let out = generate_report(
            &store,
            &answer,
            ReportRange::Daily,
            0,
            DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(out.mode, ReportMode::SinglePass);
        assert_eq!(out.passes, 1);
        assert_eq!(out.frames_summarized, 5);
    }

    #[tokio::test]
    async fn dense_single_period_splits_into_passes_without_truncating() {
        let store = SqliteStore::open_in_memory().unwrap();
        // One day, 40 frames each carrying large content_text → far more than one 8192 pass.
        // The single calendar period must fan out into several map passes (the daily-report
        // "more was captured than summarized" bug) instead of collapsing to one truncated pass.
        let big_text = "lorem ipsum dolor sit amet ".repeat(40); // ~1080 chars → ~540 tokens
        let ids = seed_day_text(&store, 0, 40, &big_text).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer {
            calls: calls.clone(),
            big: false,
        };
        let cancel = AtomicBool::new(false);
        let out = generate_report(
            &store,
            &answer,
            ReportRange::Daily,
            0,
            DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(
            out.mode,
            ReportMode::MapReduce,
            "a dense single day must fan out, not run one truncated pass"
        );
        assert!(
            out.passes >= 3,
            ">= 2 map passes + final (within-period split): {} passes",
            out.passes
        );
        assert_eq!(out.periods_total, 1);
        assert_eq!(
            out.periods_covered, 1,
            "still one calendar period — the split is internal"
        );
        // No truncation: every sampled frame is summarized across the split passes.
        assert_eq!(out.frames_summarized, ids.len() as u32);
        assert!(!out.truncated, "splitting covers all sampled frames");
        // The day's span survives: earliest and latest frames are both cited.
        assert!(out.cited_frame_ids.contains(ids.first().unwrap()));
        assert!(out.cited_frame_ids.contains(ids.last().unwrap()));
    }

    #[tokio::test]
    async fn weekly_covers_every_active_day_and_cites_first_and_last() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 7 days; a dense Monday and a quiet Saturday, each day active.
        let mut first_day = Vec::new();
        let mut last_day = Vec::new();
        for d in 0..7i64 {
            let n = if d == 0 {
                60
            } else if d == 5 {
                4
            } else {
                15
            };
            let ids = seed_day(&store, d * DAY_MS, n).await;
            match d {
                0 => first_day = ids,
                6 => last_day = ids,
                _ => {}
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer {
            calls: calls.clone(),
            big: false,
        };
        let cancel = AtomicBool::new(false);
        let out = generate_report(
            &store,
            &answer,
            ReportRange::Weekly,
            0,
            7 * DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(out.mode, ReportMode::MapReduce);
        assert_eq!(out.periods_total, 7);
        assert_eq!(out.periods_covered, 7, "every active day summarized");
        // Citation propagation: frames from BOTH the first and last day survive.
        assert!(
            first_day.iter().any(|id| out.cited_frame_ids.contains(id)),
            "first day must be cited"
        );
        assert!(
            last_day.iter().any(|id| out.cited_frame_ids.contains(id)),
            "last day must be cited (no trailing-day drop)"
        );
        // A dense Monday cannot starve a quiet Saturday: the dense day is sampled
        // down (capped near its share) while the quiet day keeps all 4 frames.
        assert!(out.frames_sampled <= cfg().weekly_top_k);
    }

    #[tokio::test]
    async fn reduce_overflow_preserves_all_days_via_hierarchical_reduce() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut first_day = Vec::new();
        let mut last_day = Vec::new();
        for d in 0..7i64 {
            let ids = seed_day(&store, d * DAY_MS, 15).await;
            match d {
                0 => first_day = ids,
                6 => last_day = ids,
                _ => {}
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        // `big` summaries (7 × ~2k tokens) overflow one reduce pass → hierarchical.
        let answer = FakeAnswer {
            calls: calls.clone(),
            big: true,
        };
        let cancel = AtomicBool::new(false);
        let out = generate_report(
            &store,
            &answer,
            ReportRange::Weekly,
            0,
            7 * DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(out.mode, ReportMode::MapReduce);
        // 7 map + 1 reduce (the 6-node group; the trailing single node passes through with
        // no call) + 1 final = 9 passes (hierarchical reduce engaged).
        assert!(
            out.passes >= 9,
            "hierarchical reduce ran: {} passes",
            out.passes
        );
        assert!(first_day.iter().any(|id| out.cited_frame_ids.contains(id)));
        assert!(
            last_day.iter().any(|id| out.cited_frame_ids.contains(id)),
            "no day dropped through the reduce tree"
        );
    }

    #[tokio::test]
    async fn cancellation_between_passes_returns_err() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_day(&store, 0, 5).await;
        let answer = FakeAnswer {
            calls: Arc::new(AtomicUsize::new(0)),
            big: false,
        };
        let cancel = AtomicBool::new(true); // already cancelled
        let out = generate_report(
            &store,
            &answer,
            ReportRange::Daily,
            0,
            DAY_MS,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await;
        assert!(out.is_err(), "a set cancel flag aborts before the pass");
    }

    #[tokio::test]
    async fn session_recap_with_overlapping_spans_cites_only_the_named_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        let wanted_session = store
            .insert_session(NewSession {
                started_at: 0,
                ended_at: Some(10_000),
                kind: SessionKind::Ai,
                tool: Some("codex".to_string()),
                host: Some(SessionHost::Desktop),
                context_key: "ai:codex".to_string(),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .unwrap();
        let other_session = store
            .insert_session(NewSession {
                started_at: 0,
                ended_at: Some(10_000),
                kind: SessionKind::Meeting,
                tool: None,
                host: Some(SessionHost::Desktop),
                context_key: "meeting:teams".to_string(),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .unwrap();
        let wanted_ids = seed_day(&store, 0, 4).await;
        let other_ids = seed_day(&store, 500, 4).await;
        store
            .assign_frames_session(&wanted_ids, Some(wanted_session))
            .await
            .unwrap();
        store
            .assign_frames_session(&other_ids, Some(other_session))
            .await
            .unwrap();
        let answer = FakeAnswer {
            calls: Arc::new(AtomicUsize::new(0)),
            big: false,
        };
        let cancel = AtomicBool::new(false);

        let out = generate_session_recap(&store, &answer, wanted_session, cfg(), None, &cancel)
            .await
            .unwrap();

        assert!(!out.cited_frame_ids.is_empty());
        assert!(out.cited_frame_ids.iter().all(|id| wanted_ids.contains(id)));
        assert!(out.cited_frame_ids.iter().all(|id| !other_ids.contains(id)));
    }

    #[tokio::test]
    async fn session_recap_without_usable_content_makes_zero_model_calls() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(NewSession {
                started_at: 0,
                ended_at: Some(1_000),
                kind: SessionKind::Focus,
                tool: None,
                host: Some(SessionHost::Ide),
                context_key: "focus:code".to_string(),
                confidence: 0.7,
                frozen: false,
            })
            .await
            .unwrap();
        let frame_id = store
            .insert_frame(NewFrame {
                captured_at: 100,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: "frames/empty.webp".to_string(),
                content_hash: "empty".to_string(),
                app_hint: Some("Code".to_string()),
                window_title: Some("Code".to_string()),
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .unwrap();
        store
            .assign_frames_session(&[frame_id], Some(session_id))
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer {
            calls: calls.clone(),
            big: false,
        };
        let cancel = AtomicBool::new(false);

        let out = generate_session_recap(&store, &answer, session_id, cfg(), None, &cancel)
            .await
            .unwrap();

        assert_eq!(out.mode, ReportMode::Empty);
        assert_eq!(out.passes, 0);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    // ---- Local calendar-day memory grid (`03 §8c`) ----

    fn stamp(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis()
    }

    #[tokio::test]
    async fn memory_grid_counts_a_25h_dst_day_as_one_period() {
        let store = SqliteStore::open_in_memory().unwrap();
        // Europe/Paris fall-back day 2026-10-25: local 00:00 +02:00 → next local 00:00
        // +01:00 is a 25 h span that the legacy UTC grid ceil-divides into 2 periods.
        let start = stamp("2026-10-24T22:00:00Z");
        let end = stamp("2026-10-25T23:00:00Z");
        assert_eq!(end - start, 25 * 3_600_000);
        seed_day(&store, start + 3_600_000, 2).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer { calls, big: false };
        let cancel = AtomicBool::new(false);

        let memory = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            start,
            end,
            None,
            Some("Europe/Paris"),
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(memory.periods_total, 1, "a 25 h local day is one period");
        assert_eq!(memory.periods_covered, 1);
        assert_eq!(memory.frames_sampled, 2);

        // Legacy default (no saved zone) keeps the uniform UTC grid: ceil(25 h / 24 h) = 2.
        let legacy = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            start,
            end,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(
            legacy.periods_total, 2,
            "legacy UTC grid behavior is unchanged"
        );
    }

    #[tokio::test]
    async fn memory_week_has_exactly_seven_local_day_periods_across_dst() {
        let store = SqliteStore::open_in_memory().unwrap();
        // Seven Europe/Paris local days 2026-10-21 00:00 → 2026-10-28 00:00, one of
        // them the 25 h fall-back day: 7 local periods (legacy UTC grid: ceil(169 h/24 h)=8).
        let start = stamp("2026-10-20T22:00:00Z");
        let end = stamp("2026-10-27T23:00:00Z");
        let periods =
            crate::memory_schedule::local_day_grid("Europe/Paris", start, end, 31).unwrap();
        assert_eq!(periods.len(), 7);
        for (period_start, _) in &periods {
            seed_day(&store, period_start + 3_600_000, 1).await;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer { calls, big: false };
        let cancel = AtomicBool::new(false);

        let memory = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            start,
            end,
            None,
            Some("Europe/Paris"),
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(
            memory.periods_total, 7,
            "seven local days are seven periods"
        );
        assert_eq!(memory.periods_covered, 7);
        assert_eq!(memory.frames_sampled, 7);

        let legacy = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            start,
            end,
            None,
            None,
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(
            legacy.periods_total, 8,
            "legacy UTC grid behavior is unchanged"
        );
    }

    #[tokio::test]
    async fn memory_grid_handles_spring_forward_and_a_partial_local_day() {
        let store = SqliteStore::open_in_memory().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = FakeAnswer { calls, big: false };
        let cancel = AtomicBool::new(false);

        // 2026-03-29 (Europe/Paris) is a 23 h spring-forward day → one period.
        let short_start = stamp("2026-03-28T23:00:00Z");
        let short_end = stamp("2026-03-29T22:00:00Z");
        assert_eq!(short_end - short_start, 23 * 3_600_000);
        seed_day(&store, short_start + 3_600_000, 1).await;
        let short = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            short_start,
            short_end,
            None,
            Some("Europe/Paris"),
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(short.periods_total, 1, "a 23 h local day is one period");
        assert_eq!(short.periods_covered, 1);

        // A partial local day (window frozen mid-day) is still exactly one period.
        let partial_start = stamp("2026-03-29T22:00:00Z"); // 2026-03-30 00:00 +02:00
        let partial_end = stamp("2026-03-30T08:00:00Z"); // 10 h into the local day
        seed_day(&store, partial_start + 3_600_000, 1).await;
        let partial = generate_report(
            &store,
            &answer,
            ReportRange::MemoryDaily,
            partial_start,
            partial_end,
            None,
            Some("Europe/Paris"),
            cfg(),
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(
            partial.periods_total, 1,
            "a partial local day is one period"
        );
        assert_eq!(partial.periods_covered, 1);
    }

    // ---- Weekly synthesis engine (`03 §8e`) ----

    /// A saved daily run fixture: finished, with a result whose provenance is `frame_ids`.
    #[test]
    fn weekly_citation_check_matches_whole_run_ids_only() {
        let run = |id: &str| {
            weekly_run(
                id,
                0,
                DAY_MS,
                "1970-01-01",
                vec![],
                false,
                ReportOutcome::Ok,
            )
        };
        let consumed = vec![run("daily-7")];
        assert!(weekly_cites_consumed_daily(
            "Shipped the parser (daily run daily-7).",
            &consumed
        ));
        assert!(weekly_cites_consumed_daily("daily-7", &consumed));
        assert!(!weekly_cites_consumed_daily(
            "See daily-70 and xdaily-7.",
            &consumed
        ));
        assert!(!weekly_cites_consumed_daily("daily-7_b", &consumed));
        assert!(!weekly_cites_consumed_daily("", &consumed));
        assert!(!weekly_cites_consumed_daily("anything", &[run("")]));
    }

    fn weekly_run(
        run_id: &str,
        start: i64,
        end: i64,
        local_date: &str,
        frame_ids: Vec<i64>,
        truncated: bool,
        outcome: ReportOutcome,
    ) -> ReportRun {
        ReportRun {
            requested_range: None,
            run_id: run_id.to_string(),
            source_id: "test-source".to_string(),
            kind: ReportRunKind::Daily,
            schedule_id: Some("memory.daily".to_string()),
            revision_of: None,
            window_start: start,
            window_end: end,
            timezone: "Etc/UTC".to_string(),
            local_date: local_date.to_string(),
            phase: ReportRunPhase::Finished,
            outcome: Some(outcome),
            error: None,
            result: Some(SavedReport {
                markdown: format!("Daily report for {local_date}."),
                model: Some("test-model".to_string()),
                frame_ids: frame_ids.clone(),
                session_ids: Vec::new(),
                cited_frame_ids: frame_ids.clone(),
                frames_sampled: frame_ids.len() as u32,
                frames_summarized: frame_ids.len() as u32,
                periods_total: 1,
                periods_covered: 1,
                passes: 1,
                truncated,
                daily_run_ids: Vec::new(),
            }),
            config_snapshot: "{}".to_string(),
            created_at: start,
            updated_at: start,
        }
    }

    /// The Monday-anchored ISO week 2026-10-12 … 2026-10-19 (`Etc/UTC`, seven dates).
    fn week_bounds() -> (i64, i64) {
        (stamp("2026-10-12T00:00:00Z"), stamp("2026-10-19T00:00:00Z"))
    }

    /// The seven local calendar dates of [`week_bounds`], computed independently of the
    /// engine under test.
    fn week_dates() -> Vec<String> {
        (0..7)
            .map(|offset| {
                (chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
                    + chrono::Duration::days(offset))
                .to_string()
            })
            .collect()
    }

    #[tokio::test]
    async fn weekly_requires_saved_dailies() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        // No saved dailies in the week and no captured evidence: the frozen NO_DATA
        // shape — Empty, no model, every local date reported missing — and the resolver
        // must never be asked for a provider.
        let execution = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            || async { panic!("a week without saved dailies must not resolve a model") },
            None,
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert_eq!(execution.output.mode, ReportMode::Empty);
        assert!(execution.model.is_none());
        assert!(execution.daily_run_ids.is_empty());
        assert_eq!(execution.missing_local_dates, week_dates());
        assert!(execution.output.markdown.contains("No captured content"));
    }

    #[tokio::test]
    async fn weekly_evidence_without_saved_dailies_is_refused() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        // Captured frames exist in the week, but no saved daily report covers any date.
        // `03 §8e` defines neither NO_DATA (that needs no evidence too) nor a synthesis
        // for this, so the run must stop instead of guessing.
        seed_day(&store, start + 3_600_000, 2).await;
        let result = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            || async { panic!("no saved dailies must not resolve a model") },
            None,
            &AtomicBool::new(false),
        )
        .await;
        let Err(err) = result else {
            panic!("captured frames without saved dailies must not become Empty/NO_DATA or a synthesis");
        };
        assert_eq!(err.code, "GENERATION_FAILED");
        assert!(err.message.contains("saved daily reports"));
    }

    #[tokio::test]
    async fn partial_week_reports_missing_days() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        // Five of the seven local dates have saved dailies; 2026-10-15 and 2026-10-16 do
        // not (offsets 3 and 4 of the Monday-anchored week).
        let seeded = [0usize, 1, 2, 5, 6];
        let mut consumed_ids = Vec::new();
        for offset in seeded {
            let day_start = start + offset as i64 * DAY_MS;
            let date = week_dates()[offset].clone();
            let frames = seed_day(&store, day_start + 1_000, 2).await;
            let run = weekly_run(
                &format!("daily-{date}"),
                day_start,
                day_start + DAY_MS,
                &date,
                frames,
                false,
                ReportOutcome::Ok,
            );
            store.create_report_run(run).await.unwrap();
            consumed_ids.push(format!("daily-{date}"));
        }
        // Expected missing dates computed in the test from the fixture, not by calling the
        // engine's helper.
        let expected_missing: Vec<String> = week_dates()
            .into_iter()
            .filter(|date| {
                !seeded
                    .iter()
                    .any(|offset| week_date_of(*offset) == date.as_str())
            })
            .collect();
        assert_eq!(expected_missing, vec!["2026-10-15", "2026-10-16"]);

        let calls = Arc::new(AtomicUsize::new(0));
        let provider_calls = calls.clone();
        let execution =
            execute_weekly(
                &store,
                WeeklyExecutionRequest {
                    start,
                    end,
                    timezone: "Etc/UTC",
                    config: cfg(),
                },
                || {
                    let calls = provider_calls.clone();
                    async move {
                        Ok(Arc::new(FakeAnswer { calls, big: false }) as Arc<dyn AnswerProvider>)
                    }
                },
                None,
                &AtomicBool::new(false),
            )
            .await
            .unwrap();
        assert_eq!(execution.missing_local_dates, expected_missing);
        let mut got = execution.daily_run_ids.clone();
        got.sort();
        let mut want = consumed_ids.clone();
        want.sort();
        assert_eq!(got, want, "exactly the consumed saved dailies are recorded");
        assert_ne!(execution.output.mode, ReportMode::Empty);
        assert_eq!(execution.output.periods_total, 7);
        assert_eq!(execution.output.periods_covered, 5);
        assert_eq!(execution.output.frames_summarized, 10);
        assert!(!execution.output.truncated);
        for heading in [
            "Activity",
            "Explicitly evidenced decisions",
            "Open questions",
            "Next steps",
        ] {
            assert!(
                execution.output.markdown.contains(heading),
                "weekly framing missing heading {heading}"
            );
        }
        assert!(
            calls.load(Ordering::Relaxed) > 0,
            "the shared model path ran"
        );
    }

    #[tokio::test]
    async fn partial_or_truncated_dailies_mark_their_dates_missing() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        // Every date is saved, but 2026-10-15's saved report is flagged truncated and
        // 2026-10-17's run outcome is PARTIAL: both are partial coverage signals.
        let fixtures = [
            (0, ReportOutcome::Ok, false),
            (1, ReportOutcome::Ok, false),
            (2, ReportOutcome::Ok, false),
            (3, ReportOutcome::Ok, true),
            (4, ReportOutcome::Ok, false),
            (5, ReportOutcome::Partial, false),
            (6, ReportOutcome::Ok, false),
        ];
        for (offset, outcome, truncated) in fixtures {
            let day_start = start + offset as i64 * DAY_MS;
            let date = week_dates()[offset].clone();
            let run = weekly_run(
                &format!("daily-{date}"),
                day_start,
                day_start + DAY_MS,
                &date,
                Vec::new(),
                truncated,
                outcome,
            );
            store.create_report_run(run).await.unwrap();
        }
        let provider_calls = Arc::new(AtomicUsize::new(0));
        let execution =
            execute_weekly(
                &store,
                WeeklyExecutionRequest {
                    start,
                    end,
                    timezone: "Etc/UTC",
                    config: cfg(),
                },
                || {
                    let calls = provider_calls.clone();
                    async move {
                        Ok(Arc::new(FakeAnswer { calls, big: false }) as Arc<dyn AnswerProvider>)
                    }
                },
                None,
                &AtomicBool::new(false),
            )
            .await
            .unwrap();
        assert_eq!(
            execution.missing_local_dates,
            vec!["2026-10-15", "2026-10-17"]
        );
    }

    #[tokio::test]
    async fn weekly_no_model_is_blocked() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        let frames = seed_day(&store, start + 1_000, 2).await;
        let run = weekly_run(
            "daily-2026-10-12",
            start,
            start + DAY_MS,
            "2026-10-12",
            frames,
            false,
            ReportOutcome::Ok,
        );
        store.create_report_run(run).await.unwrap();
        // With saved dailies present the run needs a model: a resolver failure must
        // surface unchanged, with no partial success.
        let result = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            || async {
                Err(ReportError {
                    code: "MODEL_UNAVAILABLE".into(),
                    message: "The selected answer model is not installed.".into(),
                })
            },
            None,
            &AtomicBool::new(false),
        )
        .await;
        let Err(err) = result else {
            panic!("a resolver failure must not produce a partial success");
        };
        assert_eq!(err.code, "MODEL_UNAVAILABLE");
        assert_eq!(err.message, "The selected answer model is not installed.");
    }

    /// Returns empty prose with no citations — the blank-prose failure path.
    struct BlankAnswer;

    #[async_trait]
    impl AnswerProvider for BlankAnswer {
        async fn answer(
            &self,
            _query: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
            _tx: tokio::sync::mpsc::Sender<AnswerDelta>,
        ) -> Result<()> {
            Ok(())
        }
        async fn summarize(
            &self,
            _system: &str,
            _instruction: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
        ) -> Result<(String, Vec<i64>)> {
            Ok((String::new(), Vec::new()))
        }
    }

    #[tokio::test]
    async fn weekly_blank_prose_is_generation_failed() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        let frames = seed_day(&store, start + 1_000, 2).await;
        store
            .create_report_run(weekly_run(
                "daily-2026-10-12",
                start,
                start + DAY_MS,
                "2026-10-12",
                frames,
                false,
                ReportOutcome::Ok,
            ))
            .await
            .unwrap();
        // Saved dailies were found, so blank model prose is a generation failure — never
        // a successful note and never a NO_DATA claim.
        let result = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            || async { Ok(Arc::new(BlankAnswer) as Arc<dyn AnswerProvider>) },
            None,
            &AtomicBool::new(false),
        )
        .await;
        let Err(err) = result else {
            panic!("blank prose after a positive probe must fail");
        };
        assert_eq!(err.code, "GENERATION_FAILED");
        assert!(err.message.contains("no usable text"));
    }

    /// Records every summarize call's full model input (system + instruction + context
    /// texts/ids) and answers with prose that cites the fixture daily runs it was given —
    /// a stand-in for a model that follows the weekly citation rule.
    struct WeeklyCaptureAnswer {
        seen: std::sync::Mutex<Vec<String>>,
        cite: Vec<String>,
    }

    impl WeeklyCaptureAnswer {
        /// Everything the provider received across its calls, joined.
        fn input(&self) -> String {
            self.seen.lock().unwrap().join("\n---\n")
        }
    }

    #[async_trait]
    impl AnswerProvider for WeeklyCaptureAnswer {
        async fn answer(
            &self,
            _query: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
            _tx: tokio::sync::mpsc::Sender<AnswerDelta>,
        ) -> Result<()> {
            Ok(())
        }
        async fn summarize(
            &self,
            system: &str,
            instruction: &str,
            context: &[RetrievedChunk],
            _opts: AnswerOpts,
        ) -> Result<(String, Vec<i64>)> {
            let cited: Vec<i64> = context.iter().map(|c| c.frame_id).collect();
            let rendered: Vec<String> = context
                .iter()
                .map(|c| format!("[{}] {}", c.frame_id, c.text))
                .collect();
            self.seen
                .lock()
                .unwrap()
                .push(format!("{system}\n{instruction}\n{}", rendered.join("\n")));
            Ok((
                format!("Weekly synthesis citing {}.", self.cite.join(", ")),
                cited,
            ))
        }
    }

    /// Adversarial prose: cites no consumed daily run at all.
    struct UncitedAnswer;

    #[async_trait]
    impl AnswerProvider for UncitedAnswer {
        async fn answer(
            &self,
            _query: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
            _tx: tokio::sync::mpsc::Sender<AnswerDelta>,
        ) -> Result<()> {
            Ok(())
        }
        async fn summarize(
            &self,
            _system: &str,
            _instruction: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
        ) -> Result<(String, Vec<i64>)> {
            Ok((
                "The week had activity and next steps, supported by no cited run.".to_string(),
                Vec::new(),
            ))
        }
    }

    #[tokio::test]
    async fn weekly_collapse_path_carries_daily_run_provenance() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        // A light week — two frames a day, 14 total, below `map_reduce_min_frames` — takes
        // the single-pass collapse, where per-day labels are never sent as model nodes.
        let mut run_ids = Vec::new();
        for offset in 0..7usize {
            let day_start = start + offset as i64 * DAY_MS;
            let date = week_dates()[offset].clone();
            let frames = seed_day(&store, day_start + 1_000, 2).await;
            let run_id = format!("daily-{date}");
            store
                .create_report_run(weekly_run(
                    &run_id,
                    day_start,
                    day_start + DAY_MS,
                    &date,
                    frames,
                    false,
                    ReportOutcome::Ok,
                ))
                .await
                .unwrap();
            run_ids.push(run_id);
        }
        let capture = Arc::new(WeeklyCaptureAnswer {
            seen: std::sync::Mutex::new(Vec::new()),
            cite: vec![run_ids[0].clone(), run_ids[6].clone()],
        });
        let provider = capture.clone();
        let execution = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            move || {
                let provider = provider.clone();
                async move { Ok(provider as Arc<dyn AnswerProvider>) }
            },
            None,
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        // The collapse path ran (one pass, no map nodes)...
        assert_eq!(execution.output.mode, ReportMode::SinglePass);
        // ...and the model input the provider received carries every consumed day's local
        // date and daily run id, so the framing's citation demand is satisfiable.
        let input = capture.input();
        for (offset, run_id) in run_ids.iter().enumerate() {
            assert!(
                input.contains(&week_dates()[offset]),
                "model input must carry consumed day {offset}"
            );
            assert!(
                input.contains(run_id.as_str()),
                "model input must carry consumed daily run id {run_id}"
            );
        }
        // A synthesis citing no consumed daily run (adversarial prose) is refused on the
        // §8c generation-failure path — never persisted as OK.
        let result = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            || async { Ok(Arc::new(UncitedAnswer) as Arc<dyn AnswerProvider>) },
            None,
            &AtomicBool::new(false),
        )
        .await;
        let Err(err) = result else {
            panic!("uncited weekly prose must not be saved as a successful synthesis");
        };
        assert_eq!(err.code, "GENERATION_FAILED");
        assert!(err.message.contains("did not cite"));
    }

    #[tokio::test]
    async fn weekly_multiday_range_run_does_not_cover_days() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        // Captured evidence exists (a positive probe), and one manual multi-day range run
        // is saved over the whole week — enqueued as kind Daily, but spanning every local
        // day, so it is no day's own saved daily.
        seed_day(&store, start + 3_600_000, 2).await;
        let mut range_run = weekly_run(
            "range-2026-10-12_to_2026-10-18",
            start,
            end,
            "2026-10-12",
            Vec::new(),
            false,
            ReportOutcome::Ok,
        );
        range_run.requested_range = Some(MemoryReportRange {
            start_date: "2026-10-12".to_string(),
            end_date: "2026-10-18".to_string(),
        });
        // Manual range runs are not scheduled runs (`03 §8c`).
        range_run.schedule_id = None;
        store.create_report_run(range_run.clone()).await.unwrap();

        // The range run overlaps every local date of the week — the old overlap predicate
        // would have credited all seven — yet it owns none of them (computed here from the
        // seeded window and the week bounds, independently of the engine).
        for offset in 0..7usize {
            let day_start = start + offset as i64 * DAY_MS;
            let day_end = day_start + DAY_MS;
            assert!(
                range_run.window_start < day_end && range_run.window_end > day_start,
                "fixture must overlap day {offset} to exercise the defect"
            );
        }
        // Every date is missing: none has its own single-day saved daily.
        let expected_missing = week_dates();
        assert_eq!(
            weekly_missing_local_dates(&[range_run], start, end, "Etc/UTC").unwrap(),
            expected_missing
        );
        // The week is not complete coverage: the execution refuses (after the positive
        // probe) instead of synthesizing prose about days with no consumed daily run.
        let result = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            || async { panic!("a week without a consumable saved daily must not resolve a model") },
            None,
            &AtomicBool::new(false),
        )
        .await;
        let Err(err) = result else {
            panic!("a range run must not let the week be saved as complete coverage");
        };
        assert_eq!(err.code, "GENERATION_FAILED");
    }

    #[tokio::test]
    async fn weekly_base_and_revision_do_not_inflate_truncation() {
        let (start, end) = week_bounds();
        let day = start; // the 2026-10-12 local day (Etc/UTC)
        for pruned in [None, Some(9_000_001i64)] {
            let store = SqliteStore::open_in_memory().unwrap();
            let frames = seed_day(&store, day + 1_000, 5).await;
            // The base daily and its Regenerate revision are both consumed (no revision
            // precedence) and their recorded evidence overlaps; the pruned variant records
            // one frame id whose evidence is gone (retention).
            let revision_ids: Vec<i64> = frames[2..].iter().copied().chain(pruned).collect();
            store
                .create_report_run(weekly_run(
                    "daily-base",
                    day,
                    day + DAY_MS,
                    "2026-10-12",
                    frames.clone(),
                    false,
                    ReportOutcome::Ok,
                ))
                .await
                .unwrap();
            let mut revision = weekly_run(
                "daily-rev",
                day,
                day + DAY_MS,
                "2026-10-12",
                revision_ids.clone(),
                false,
                ReportOutcome::Ok,
            );
            revision.revision_of = Some("daily-base".to_string());
            store.create_report_run(revision).await.unwrap();

            // Expected counts computed from the fixture: the distinct readable union of
            // both runs' recorded ids, and the distinct recorded ids including the pruned
            // one.
            let expected_readable: usize = {
                let mut ids: Vec<i64> = frames.clone();
                for id in &revision_ids {
                    if !ids.contains(id) {
                        ids.push(*id);
                    }
                }
                ids.retain(|id| frames.contains(id));
                ids.len()
            };
            let expected_recorded: usize = frames.len() + usize::from(pruned.is_some());

            let execution = execute_weekly(
                &store,
                WeeklyExecutionRequest {
                    start,
                    end,
                    timezone: "Etc/UTC",
                    config: cfg(),
                },
                || async {
                    Ok(Arc::new(FakeAnswer {
                        calls: Arc::new(AtomicUsize::new(0)),
                        big: false,
                    }) as Arc<dyn AnswerProvider>)
                },
                None,
                &AtomicBool::new(false),
            )
            .await
            .unwrap();
            // Both runs were consumed as inputs (per-run consumption stays intact).
            let mut consumed = execution.daily_run_ids.clone();
            consumed.sort();
            assert_eq!(
                consumed,
                vec!["daily-base".to_string(), "daily-rev".to_string()]
            );
            // Duplicate overlap does not inflate the recorded sampled count (it is a
            // distinct-frame count, the same unit as frames_summarized) ...
            assert_eq!(execution.output.frames_sampled, expected_readable as u32);
            assert_eq!(execution.output.frames_summarized, expected_readable as u32);
            // ... and truncation tracks recorded-but-unreadable evidence only.
            assert_eq!(
                execution.output.truncated,
                expected_readable < expected_recorded,
                "truncation must track unreadable recorded evidence, not base+revision duplicates"
            );
            if pruned.is_none() {
                assert!(
                    !execution.output.truncated,
                    "a fully read week with base+revision overlap is not truncated"
                );
            } else {
                assert!(
                    execution.output.truncated,
                    "a pruned recorded frame id must still mark the week truncated"
                );
            }
        }
    }

    #[tokio::test]
    async fn weekly_attributes_days_by_their_own_local_date() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (start, end) = week_bounds();
        // One normal UTC daily owns 2026-10-14, so the week still has a consumable daily
        // and the execution completes.
        let owned = "2026-10-14";
        let frames = seed_day(&store, stamp(&format!("{owned}T01:00:00Z")), 2).await;
        store
            .create_report_run(weekly_run(
                &format!("daily-{owned}"),
                stamp(&format!("{owned}T00:00:00Z")),
                stamp("2026-10-15T00:00:00Z"),
                owned,
                frames,
                false,
                ReportOutcome::Ok,
            ))
            .await
            .unwrap();
        // A daily saved under another zone (a supported zone edit happened since) whose
        // persisted local date is 2026-10-13, while its window lies inside the UTC day
        // 2026-10-12 — re-computing from the window in the weekly zone would silently
        // relabel it to the neighbouring date.
        let mut foreign = weekly_run(
            "daily-tokyo-2026-10-13",
            stamp("2026-10-12T16:00:00Z"),
            stamp("2026-10-12T23:00:00Z"),
            "2026-10-13",
            Vec::new(),
            false,
            ReportOutcome::Ok,
        );
        foreign.timezone = "Asia/Tokyo".to_string();
        store.create_report_run(foreign.clone()).await.unwrap();
        // Fixture check, independent of the engine: the foreign window lies entirely inside
        // the weekly zone's 2026-10-12, so containment alone would credit the wrong date.
        assert!(foreign.window_start >= start && foreign.window_end <= start + DAY_MS);

        let execution = execute_weekly(
            &store,
            WeeklyExecutionRequest {
                start,
                end,
                timezone: "Etc/UTC",
                config: cfg(),
            },
            || async {
                Ok(Arc::new(FakeAnswer {
                    calls: Arc::new(AtomicUsize::new(0)),
                    big: false,
                }) as Arc<dyn AnswerProvider>)
            },
            None,
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        // Expected dates computed in the test: only the UTC daily owns its own date.
        let expected_missing: Vec<String> = week_dates()
            .into_iter()
            .filter(|date| date != owned)
            .collect();
        assert_eq!(execution.missing_local_dates, expected_missing);
        assert!(
            execution
                .missing_local_dates
                .contains(&"2026-10-12".to_string()),
            "the neighbouring date the foreign daily would be relabelled to must stay missing"
        );
        assert!(
            execution
                .missing_local_dates
                .contains(&"2026-10-13".to_string()),
            "the foreign daily's own persisted date is missing, not silently credited"
        );
        // The other-zone daily is not consumed: it is not this week's saved daily report.
        assert_eq!(execution.daily_run_ids, vec![format!("daily-{owned}")]);
    }

    /// The `Etc/UTC` local date for a day offset of [`week_bounds`] — test-side only.
    fn week_date_of(offset: usize) -> String {
        (chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
            + chrono::Duration::days(offset as i64))
        .to_string()
    }
}
