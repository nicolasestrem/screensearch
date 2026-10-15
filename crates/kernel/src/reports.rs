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
use traits::{
    AnswerOpts, AnswerProvider, ReportConfig, ReportMode, ReportOutput, ReportRange,
    RetrievedChunk, SearchHit, SearchQuery, SegmenterFrame, Store, TimeRange, TimelineBucket,
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
/// the retrieval path and framing; `cfg` carries the settings-derived knobs. Emits
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
        let grid = grid_size(start, end, MAX_PERIODS);
        let buckets = match &scope {
            CoverageScope::TimeRange => store.timeline_buckets(start, end, grid).await?,
            CoverageScope::Session(frames) => session_buckets(frames, start, end, grid),
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
        for (b, frames) in buckets.iter().zip(periods_frames) {
            let day = ((b.start - start) / DAY_MS + 1).max(1);
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
        (groups, grid, total_in_range)
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

/// Whether all of `groups`' chunks fit a single map pass within `ctx_tokens` — the gate for
/// collapsing a small multi-period report into one final pass without truncating. Mirrors
/// [`split_chunks_into_batches`]'s per-batch budget (a conservative heuristic; the inference
/// budgeter is the hard bound).
fn chunks_fit_one_pass(groups: &[Group], reply_budget: u32, ctx_tokens: usize) -> bool {
    let reserve = reply_budget as usize + est_tokens(MAP_SYSTEM_PROMPT) + REPORT_TEMPLATE_OVERHEAD;
    let budget = ctx_tokens.saturating_sub(reserve);
    let used: usize = groups
        .iter()
        .flat_map(|g| &g.chunks)
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
    if range == ReportRange::MemoryDaily {
        let timestamp = |value| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(value)
                .map(|date| date.to_rfc3339())
                .unwrap_or_else(|| value.to_string())
        };
        return format!("Write memory for the selected window [{} inclusive, {} exclusive]. Use the headings Activity, Explicitly evidenced decisions, Open questions, and Next steps. Include only supported facts; distinguish tentative screen text from commitments. If a section has no evidence, say so. Use the exact selected bounds rather than relative claims about today.", timestamp(start), timestamp(end));
    }
    let base = match range {
        ReportRange::Daily => "Write the recall report for today.",
        ReportRange::Weekly => "Write the recall report for the last 7 days.",
        ReportRange::Custom | ReportRange::MemoryDaily => {
            "Write the recall report for the selected range."
        }
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
    use traits::{AnswerDelta, NewFrame, NewSession, OcrResult, SessionHost, SessionKind};

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
}
