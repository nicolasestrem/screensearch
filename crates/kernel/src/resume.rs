//! Where-was-i: the last sustained work context before the current detour
//! (`03 §7b`, D9, 0.3.0 PR6). A **pure** heuristic over recent frame context rows —
//! no new crate, no `unsafe`, unit-tested in isolation. [`where_was_i`] is the thin
//! async convenience the `where_was_i` command (and later `crates/api`) calls: it
//! reads a capped window from the store, then runs the pure [`last_sustained_context`].
//!
//! ## The heuristic (`03 §7b`)
//! The **anchor** is the *current* context — the last non-ScreenSearch foreground
//! context. ScreenSearch never captures its own windows (the self-exclude gate), so
//! the newest recent frames are already the app/domain the user was actually looking
//! at before summoning the overlay; the last of them is the anchor. The answer is the
//! **most recent sustained run** ending before the anchor's stretch that isn't the
//! anchor itself: a run of frames sharing a **context key** that persisted for at least
//! `resume.min_dwell_secs`. Brief excursions are absorbed — a run is broken only by an
//! interruption whose own context is *itself* sustained (persists ≥ the same dwell);
//! shorter excursions (a 2-second alt-tab, a notification, an unresolved-`app_hint`
//! frame) fold into the surrounding run. One knob, never two.
//!
//! Context key = `app_hint` (lowercased) refined by the browser domain from
//! `browser_url` when present (dormant today — production capture never sets
//! `browser_url` — but implemented per the contract). Excluded from candidacy: the
//! anchor context, ScreenSearch itself, and any `privacy.excluded_apps` entry.

use std::collections::HashMap;

use traits::{
    is_excluded, AdvancedConfig, FrameContextRow, Result, ResumeContext, Settings, Store,
};

/// Upper bound on frames the where-was-i query scans (an engineering cap, not a user
/// threshold — the COUNT_SCAN_CAP precedent). ~4k frames ≈ 100 min of dense
/// dual-monitor capture at the 3 s default cadence, far past any resumable run.
pub const RESUME_SCAN_CAP: u32 = 4_000;

/// Tunables for [`last_sustained_context`], assembled from [`Settings`] by
/// [`where_was_i`] so the pure function carries no settings dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeParams {
    /// `resume.min_dwell_secs` in **milliseconds** (the frame clock unit).
    pub min_dwell_ms: i64,
    /// `privacy.excluded_apps` — a context matching any entry never qualifies.
    pub excluded_apps: Vec<String>,
}

/// A frame's context identity (`03 §7b`): the foreground app, refined by browser
/// domain when a URL is present, so two tabs of different sites read as distinct
/// contexts while two windows of the same app read as one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ContextKey {
    app: String,
    domain: Option<String>,
}

/// A maximal keyed segment (consecutive frames sharing a context key; keyless frames
/// are transparent and never appear here). `rep` indexes the run's representative
/// (last) frame in the chronological frame list.
struct Segment {
    key: ContextKey,
    first_ts: i64,
    last_ts: i64,
    rep: usize,
}

/// A consolidated run of one context — one or more same-key segments merged across
/// non-sustained excursions.
struct Run {
    key: ContextKey,
    first_ts: i64,
    last_ts: i64,
    rep: usize,
}

/// The browser domain of a URL, or `None`: strips the scheme + `www.`, keeps the host
/// up to the first `/ ? : #`, lowercased. Dormant in production (`browser_url` is
/// always `None` today) but implemented + tested per `03 §7b`.
fn browser_domain(url: Option<&str>) -> Option<String> {
    let url = url?.trim();
    if url.is_empty() {
        return None;
    }
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = after_scheme
        .split(['/', '?', ':', '#'])
        .next()
        .unwrap_or_default();
    let host = host.strip_prefix("www.").unwrap_or(host).trim();
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// The context key of a frame, or `None` when `app_hint` can't be resolved (a keyless,
/// transparent frame the run-builder absorbs).
fn context_key(row: &FrameContextRow) -> Option<ContextKey> {
    let app = row.app_hint.as_deref()?.trim();
    if app.is_empty() {
        return None;
    }
    Some(ContextKey {
        app: app.to_ascii_lowercase(),
        domain: browser_domain(row.browser_url.as_deref()),
    })
}

/// Whether the gap between two same-key segments contains a **sustained** interruption
/// — some interrupting key whose presence-span within the gap (last appearance − first
/// appearance, so a fragmented interrupter still counts) reaches `min_dwell_ms`. Such a
/// gap breaks the run; a shorter excursion is absorbed (`03 §7b`).
fn gap_has_sustained_interrupter(gap: &[Segment], min_dwell_ms: i64) -> bool {
    // (min first_ts, max last_ts) per interrupting key across the whole gap.
    let mut spans: HashMap<&ContextKey, (i64, i64)> = HashMap::new();
    for seg in gap {
        let entry = spans.entry(&seg.key).or_insert((seg.first_ts, seg.last_ts));
        entry.0 = entry.0.min(seg.first_ts);
        entry.1 = entry.1.max(seg.last_ts);
    }
    spans
        .values()
        .any(|(first, last)| last - first >= min_dwell_ms)
}

/// The last sustained context before the anchor, or `None` for "nothing to resume".
/// `rows` are **newest-first** (as [`Store::recent_frame_contexts`] returns them).
pub fn last_sustained_context(
    rows: &[FrameContextRow],
    params: &ResumeParams,
) -> Option<ResumeContext> {
    // Work chronologically (oldest → newest).
    let frames: Vec<&FrameContextRow> = rows.iter().rev().collect();

    // 1. Raw segments over keyed frames; keyless frames are transparent (absorbed).
    let mut segments: Vec<Segment> = Vec::new();
    for (idx, f) in frames.iter().enumerate() {
        let Some(key) = context_key(f) else { continue };
        match segments.last_mut() {
            Some(seg) if seg.key == key => {
                seg.last_ts = f.captured_at;
                seg.rep = idx;
            }
            _ => segments.push(Segment {
                key,
                first_ts: f.captured_at,
                last_ts: f.captured_at,
                rep: idx,
            }),
        }
    }
    // 2. Anchor = the current context (the last keyed frame's key). No keyed frames at
    //    all ⇒ nothing to resume.
    let anchor = segments.last()?.key.clone();

    // 3. Consolidate runs, absorbing non-sustained excursions. Greedy left-to-right:
    //    the earliest unconsumed segment starts a run that absorbs every later same-key
    //    segment reachable without a sustained interrupter; a same-key segment beyond a
    //    sustained break stays unconsumed and starts its own run.
    let mut consumed = vec![false; segments.len()];
    let mut runs: Vec<Run> = Vec::new();
    for start in 0..segments.len() {
        if consumed[start] {
            continue;
        }
        consumed[start] = true;
        let key = segments[start].key.clone();
        let mut run = Run {
            key: key.clone(),
            first_ts: segments[start].first_ts,
            last_ts: segments[start].last_ts,
            rep: segments[start].rep,
        };
        let mut last_seg = start;
        while let Some(next) = (last_seg + 1..segments.len()).find(|&k| segments[k].key == key) {
            if gap_has_sustained_interrupter(&segments[last_seg + 1..next], params.min_dwell_ms) {
                break;
            }
            consumed[next] = true;
            run.last_ts = segments[next].last_ts;
            run.rep = segments[next].rep;
            last_seg = next;
        }
        runs.push(run);
    }

    // 4. Candidacy + pick the latest-ending sustained run (tie: highest rep frame_id).
    let mut best: Option<Run> = None;
    for run in runs {
        if run.key == anchor || run.key.app.contains("screensearch") {
            continue;
        }
        let rep = frames[run.rep];
        if is_excluded(
            rep.app_hint.as_deref(),
            rep.window_title.as_deref(),
            &params.excluded_apps,
        ) {
            continue;
        }
        if run.last_ts - run.first_ts < params.min_dwell_ms {
            continue;
        }
        let better = match &best {
            None => true,
            Some(b) => (run.last_ts, rep.frame_id) > (b.last_ts, frames[b.rep].frame_id),
        };
        if better {
            best = Some(run);
        }
    }

    best.map(|run| {
        let rep = frames[run.rep];
        ResumeContext {
            frame_id: rep.frame_id,
            app: rep.app_hint.clone().unwrap_or_default(),
            window_title: rep.window_title.clone(),
            browser_url: rep.browser_url.clone(),
            span_start: run.first_ts,
            span_end: run.last_ts,
            image_path: rep.image_path.clone(),
            image_purged: rep.image_purged,
        }
    })
}

/// Reads the recent-frame window and runs [`last_sustained_context`] with the live
/// settings (`03 §7b`). Free function — no `Kernel` instance needed — so the
/// `where_was_i` command and the local API (`03 §7c`) share one code path. The dwell
/// threshold is `file-only` and comes from the advanced config (`03 §8`, 0.4.5 D6).
pub async fn where_was_i(
    store: &dyn Store,
    settings: &Settings,
    advanced: &AdvancedConfig,
) -> Result<Option<ResumeContext>> {
    let rows = store.recent_frame_contexts(RESUME_SCAN_CAP).await?;
    let params = ResumeParams {
        min_dwell_ms: i64::from(advanced.resume.min_dwell_secs) * 1000,
        excluded_apps: settings.privacy_excluded_apps.clone(),
    };
    Ok(last_sustained_context(&rows, &params))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DWELL_MS: i64 = 120_000; // 120 s, the default

    fn params() -> ResumeParams {
        ResumeParams {
            min_dwell_ms: DWELL_MS,
            excluded_apps: vec!["1Password".to_string()],
        }
    }

    /// Builds a newest-first `Vec<FrameContextRow>` from chronological
    /// `(secs, app, title, url)` tuples (the store returns newest-first, so we reverse).
    fn rows(seq: &[(i64, Option<&str>, &str, Option<&str>)]) -> Vec<FrameContextRow> {
        let mut v: Vec<FrameContextRow> = seq
            .iter()
            .enumerate()
            .map(|(i, (secs, app, title, url))| FrameContextRow {
                frame_id: i as i64 + 1,
                captured_at: secs * 1000,
                app_hint: app.map(|a| a.to_string()),
                window_title: Some(title.to_string()),
                browser_url: url.map(|u| u.to_string()),
                image_path: format!("frames/{i}.webp"),
                image_purged: false,
            })
            .collect();
        v.reverse(); // newest-first, as the store returns
        v
    }

    /// A sustained run of `app` from `start`..=`end` seconds, one frame per `step`.
    fn run(
        app: &str,
        start: i64,
        end: i64,
        step: i64,
    ) -> Vec<(i64, Option<&str>, &str, Option<&str>)> {
        let mut out = Vec::new();
        let mut t = start;
        while t <= end {
            out.push((t, Some(app), "win", None));
            t += step;
        }
        out
    }

    #[test]
    fn no_frames_is_none() {
        assert!(last_sustained_context(&[], &params()).is_none());
    }

    #[test]
    fn all_keyless_frames_is_none() {
        // Frames whose app_hint never resolves — no anchor, nothing to resume.
        let seq = [
            (0, None, "?", None),
            (30, None, "?", None),
            (300, None, "?", None),
        ];
        assert!(last_sustained_context(&rows(&seq), &params()).is_none());
    }

    #[test]
    fn single_context_only_is_none() {
        // Never left the work app → it's the anchor → nothing to resume (honest empty).
        let seq = run("VS Code", 0, 300, 30);
        assert!(last_sustained_context(&rows(&seq), &params()).is_none());
    }

    #[test]
    fn returns_prior_sustained_run_with_span_and_last_frame() {
        // 5 min of VS Code, then a Firefox detour (the current context / anchor).
        let mut seq = run("VS Code", 0, 300, 30);
        seq.extend(run("Firefox", 330, 360, 30));
        let got = last_sustained_context(&rows(&seq), &params()).expect("a resume context");
        assert_eq!(got.app, "VS Code");
        assert_eq!(got.span_start, 0);
        assert_eq!(got.span_end, 300_000); // the run's LAST frame, not the anchor
                                           // Representative = the last VS Code frame (captured_at 300 s).
        assert_eq!(got.span_end, got.span_end.max(300_000));
    }

    #[test]
    fn brief_excursion_is_absorbed_into_the_run() {
        // VS Code, a 30 s notification blip, more VS Code — then the Firefox anchor.
        // The blip (< 120 s) must NOT split the VS Code run.
        let mut seq = run("VS Code", 0, 120, 30);
        seq.extend(run("Toast", 150, 180, 30)); // 30 s excursion
        seq.extend(run("VS Code", 210, 360, 30));
        seq.extend(run("Firefox", 390, 420, 30)); // anchor
        let got = last_sustained_context(&rows(&seq), &params()).expect("resume ctx");
        assert_eq!(got.app, "VS Code");
        assert_eq!(got.span_start, 0);
        assert_eq!(
            got.span_end, 360_000,
            "run spans across the absorbed excursion"
        );
    }

    #[test]
    fn sustained_interruption_splits_the_run() {
        // VS Code (short, 90 s), a SUSTAINED Slack session (150 s > dwell), VS Code
        // again (short), then the Firefox anchor. The first VS Code chunk is too short
        // on its own; Slack is the only sustained non-anchor run → Slack is offered.
        let mut seq = run("VS Code", 0, 90, 30);
        seq.extend(run("Slack", 120, 270, 30)); // 150 s sustained
        seq.extend(run("VS Code", 300, 360, 30)); // short tail
        seq.extend(run("Firefox", 390, 420, 30)); // anchor
        let got = last_sustained_context(&rows(&seq), &params()).expect("resume ctx");
        assert_eq!(got.app, "Slack");
        assert_eq!(got.span_start, 120_000);
        assert_eq!(got.span_end, 270_000);
    }

    #[test]
    fn fragmented_interrupter_reaching_dwell_splits() {
        // Two short Slack blips 130 s apart bracket a brief Toast; Slack's presence-span
        // across the gap (60→190 s = 130 s > dwell) makes it a sustained interrupter, so
        // it breaks the surrounding Notes into two short chunks — even though neither Slack
        // blip alone is sustained. Proof of the split: if Notes were NOT split it would span
        // 0→250 s (> dwell) and, ending later than Slack, would win. Getting Slack back
        // (its blips consolidated across the Toast excursion) shows Notes was split.
        let seq = [
            (0, Some("Notes"), "a", None),
            (30, Some("Notes"), "a", None),
            (60, Some("Slack"), "s", None),  // blip 1
            (90, Some("Toast"), "t", None),  // brief excursion inside Slack's stretch
            (190, Some("Slack"), "s", None), // blip 2 → Slack presence-span 60→190 = 130 s
            (220, Some("Notes"), "a", None),
            (250, Some("Notes"), "a", None),
            (400, Some("Firefox"), "f", None), // anchor
        ];
        let got = last_sustained_context(&rows(&seq), &params()).expect("Slack qualifies");
        assert_eq!(
            got.app, "Slack",
            "Notes was split; the sustained Slack run wins"
        );
        assert_eq!(got.span_start, 60_000);
        assert_eq!(got.span_end, 190_000);
    }

    #[test]
    fn dwell_exactly_at_threshold_qualifies() {
        // A run of exactly 120 s (== dwell) qualifies (>=), then the anchor.
        let mut seq = run("VS Code", 0, 120, 30); // span 0..120 s == dwell
        seq.extend(run("Firefox", 150, 180, 30));
        let got = last_sustained_context(&rows(&seq), &params()).expect("exactly-dwell qualifies");
        assert_eq!(got.app, "VS Code");
    }

    #[test]
    fn single_frame_run_fails_dwell() {
        // One lone VS Code frame (span 0) before the anchor → below dwell → None.
        let seq = [
            (0, Some("VS Code"), "repo", None),
            (30, Some("Firefox"), "f", None),
            (60, Some("Firefox"), "f", None),
        ];
        assert!(last_sustained_context(&rows(&seq), &params()).is_none());
    }

    #[test]
    fn excluded_app_is_skipped_for_next_candidate() {
        // A sustained 1Password session (excluded) then a sustained VS Code session,
        // then the Firefox anchor. 1Password never qualifies; VS Code is offered.
        let mut seq = run("1Password", 0, 150, 30); // sustained but excluded
        seq.extend(run("VS Code", 180, 360, 30)); // sustained, allowed
        seq.extend(run("Firefox", 390, 420, 30)); // anchor
        let got = last_sustained_context(&rows(&seq), &params()).expect("resume ctx");
        assert_eq!(got.app, "VS Code");
    }

    #[test]
    fn screensearch_context_never_qualifies() {
        // Defensive: even if a ScreenSearch frame slipped into history, it's never a
        // candidate. Here it's sustained but must be skipped; VS Code is offered.
        let mut seq = run("VS Code", 0, 150, 30);
        seq.extend(run("ScreenSearch", 180, 360, 30));
        seq.extend(run("Firefox", 390, 420, 30)); // anchor
        let got = last_sustained_context(&rows(&seq), &params()).expect("resume ctx");
        assert_eq!(got.app, "VS Code");
    }

    #[test]
    fn distinct_browser_domains_are_distinct_contexts() {
        // Same app (Firefox) but two different domains → two contexts. A sustained
        // github.com session then a short docs.rs detour (anchor) → github is offered.
        let mut seq: Vec<(i64, Option<&str>, &str, Option<&str>)> = Vec::new();
        let mut t = 0;
        while t <= 150 {
            seq.push((t, Some("Firefox"), "gh", Some("https://github.com/foo")));
            t += 30;
        }
        seq.push((180, Some("Firefox"), "docs", Some("https://docs.rs/bar")));
        seq.push((210, Some("Firefox"), "docs", Some("https://docs.rs/bar")));
        let got = last_sustained_context(&rows(&seq), &params()).expect("resume ctx");
        assert_eq!(got.browser_url.as_deref(), Some("https://github.com/foo"));
        assert_eq!(got.span_start, 0);
        assert_eq!(got.span_end, 150_000);
    }

    #[test]
    fn equal_timestamps_are_deterministic() {
        // A dual-monitor cycle shares captured_at; the builder must not panic and must
        // stay deterministic. VS Code (sustained) then the Firefox anchor, with a pair
        // of equal-timestamp frames in the run.
        let seq = [
            (0, Some("VS Code"), "a", None),
            (0, Some("VS Code"), "b", None), // same ts (2nd monitor)
            (60, Some("VS Code"), "a", None),
            (150, Some("VS Code"), "a", None),
            (180, Some("Firefox"), "f", None),
            (210, Some("Firefox"), "f", None),
        ];
        let got = last_sustained_context(&rows(&seq), &params()).expect("resume ctx");
        assert_eq!(got.app, "VS Code");
        assert_eq!(got.span_end, 150_000);
    }

    #[test]
    fn browser_domain_parsing() {
        assert_eq!(
            browser_domain(Some("https://www.github.com/a/b")).as_deref(),
            Some("github.com")
        );
        assert_eq!(
            browser_domain(Some("http://docs.rs:8080/x?y=1")).as_deref(),
            Some("docs.rs")
        );
        assert_eq!(
            browser_domain(Some("example.com/path")).as_deref(),
            Some("example.com")
        );
        assert_eq!(browser_domain(Some("")), None);
        assert_eq!(browser_domain(None), None);
    }
}
