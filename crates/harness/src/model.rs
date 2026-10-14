//! Shared data types for the harness.
//!
//! - Export records ([`ExportFrame`], [`ExportMark`], [`DayHeader`]) — the read-only export
//!   shape written to `harness-data/<day>/` (`export.rs`).
//! - Recognition enums ([`Kind`], [`Host`]) — mirror the `sessions.kind` / `sessions.host`
//!   CHECK sets in `specs/03 §4`.
//! - Segmenter I/O ([`SegParams`], [`SessionSpan`]) — the pure candidate segmenter's
//!   parameters and output span (`segmenter.rs`). `SessionSpan` is the referee's unit: PR4
//!   adapts its shipped segmenter's output to this type for the D9 re-run.
//! - Hand-label types ([`LabeledSession`], [`DayLabels`]) — the parsed per-day TOML
//!   (`labels.rs`).

use serde::{Deserialize, Serialize};

/// A `focus`/`meeting`/`ai`/`other` session kind — the `sessions.kind` CHECK set (`03 §4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Focus,
    Meeting,
    Ai,
    Other,
}

/// Where a recognized tool runs — the `sessions.host` CHECK set (`03 §4`), and the second
/// D7 recognition dimension (tool identity × host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Terminal,
    Desktop,
    Browser,
    Ide,
}

/// One exported frame's segmentation-relevant metadata — one JSONL line in
/// `harness-data/<day>/frames.jsonl`. Field names match the `frames` columns (`03 §4`,
/// `crates/store/src/schema.rs`). Only the metadata segmentation needs: never OCR/content
/// text, never the image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportFrame {
    pub frame_id: i64,
    /// Capture time, unix epoch **milliseconds** (the frame clock unit).
    pub captured_at: i64,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    /// Dormant in production capture (always `None`, gap #109) — carried for completeness.
    pub browser_url: Option<String>,
    pub capture_trigger: Option<String>,
}

/// One exported mark — one JSONL line in `harness-data/<day>/marks.jsonl`. Marks are
/// labeling **anchors**, not labels (`docs/0.4.0.md` §3 PR2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportMark {
    pub mark_id: i64,
    pub frame_id: i64,
    pub created_at: i64,
    pub note: Option<String>,
    pub resolved_at: Option<i64>,
}

/// `harness-data/<day>/day.json` — the per-day export header. `local_midnight_ms` anchors
/// label `HH:MM` → epoch arithmetic; `utc_offset_min` records the day's offset for audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayHeader {
    /// Format tag, always `"harness-day-v1"`.
    pub schema: String,
    /// Local calendar day, `"YYYY-MM-DD"`.
    pub date: String,
    /// Unix ms at local midnight starting this day.
    pub local_midnight_ms: i64,
    /// Local UTC offset in minutes on this day (e.g. 120 for Europe/Paris summer).
    pub utc_offset_min: i64,
    pub frame_count: i64,
    pub mark_count: i64,
}

/// Segmenter tunables, in the frame clock unit (ms). `min_len_ms` doubles as the single
/// dwell knob for the sustained-interrupter rule (`03 §7e` — "reusing the same dwell logic
/// rather than inventing a second knob").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegParams {
    /// `sessions.gap_close_secs` × 1000 (default 120_000). Idle-gap / context-switch close.
    pub gap_close_ms: i64,
    /// `sessions.min_len_secs` × 1000 (default 120_000). Floor + sustained-interrupter dwell.
    pub min_len_ms: i64,
}

impl Default for SegParams {
    fn default() -> Self {
        Self {
            gap_close_ms: 120_000,
            min_len_ms: 120_000,
        }
    }
}

/// Macro grouping tunables for pass 2 of the two-pass segmenter (`crate::group`, the `03 §7e`
/// amendment `06` #27). In the frame clock unit (ms) except the density gate (frames/hour). The
/// key names are PR1-style **proposals** for the `sessions.*` settings keys — PR4 owns the final
/// keys, PR5 the Settings UI (the 0.3.2 `app.*` precedent). Defaults sit mid-plateau, above the
/// physical cliffs measured on the ground truth; PR2's sweep confirms their placement, and any
/// knob whose 1-D response is flat is proposed to PR4 as a named constant rather than a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupParams {
    /// `sessions.merge_gap_secs` × 1000 (default 1_500_000). An inactivity gap this long closes
    /// the open group (the macro close rule; labeled sessions hold internal lulls up to ~969 s).
    pub merge_gap_ms: i64,
    /// `sessions.absorb_max_secs` × 1000 (default 1_200_000). The absorb budget: a single foreign
    /// micro-run (a different AI identity, or unrecognized activity while an AI anchor is active)
    /// up to this long is absorbed into the group instead of closing it; longer = a real switch.
    pub absorb_max_ms: i64,
    /// `sessions.meeting_gap_secs` × 1000 (default 480_000). Meeting-band chaining gap (must stay
    /// below the observed post-meeting straggler gap so a band ends at the real presence stop).
    pub meeting_gap_ms: i64,
    /// `sessions.focus_min_len_secs` × 1000 (default 600_000). Floor for anchorless focus sessions
    /// AND the qualification extent for meeting bands (a sub-floor meeting chain never anchors).
    pub focus_min_len_ms: i64,
    /// `sessions.focus_min_density_fph` (default 30; 0 disables). Minimum frames/hour for an
    /// **anchorless** focus session; AI/meeting sessions are exempt. Suppresses fragmented
    /// low-density blocks that the maintainer labels no-session (gap #113).
    pub focus_min_density_fph: i64,
}

impl Default for GroupParams {
    fn default() -> Self {
        Self {
            merge_gap_ms: 1_500_000,
            absorb_max_ms: 1_200_000,
            meeting_gap_ms: 480_000,
            focus_min_len_ms: 600_000,
            focus_min_density_fph: 30,
        }
    }
}

/// One emitted candidate session. The referee's unit ([`score`](crate::score)). `tool` is
/// `Some` only when `kind == Ai`; `host` is set when recognition resolved one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSpan {
    pub start_ms: i64,
    pub end_ms: i64,
    /// The context key. From the ungrouped `segment` this is the **micro-run** key
    /// (`"app"` / `"app|domain"` / `"app|domain|tool"`; domain dormant, gap #109). From the
    /// grouped `segment_grouped` it is the **task-level** anchor key
    /// (`"ai:<tool-id>"` / `"meeting:<taxonomy-id>"` / `"focus:<dominant-stem>"`, the `03 §7e`
    /// amendment `06` #27 stored-key grammar — what PR3 re-normalizes the `§4` column comment to).
    pub context_key: String,
    pub kind: Kind,
    pub tool: Option<String>,
    pub host: Option<Host>,
    pub frame_count: usize,
    pub first_frame_id: i64,
    pub last_frame_id: i64,
}

/// One hand-labeled session from a per-day `labels.toml`. Times are local `"HH:MM"` as
/// written by the maintainer; [`labels`](crate::labels) validates + converts them to epoch ms
/// against the day's `local_midnight_ms`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LabeledSession {
    pub start: String,
    pub end: String,
    pub kind: Kind,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub host: Option<Host>,
    #[serde(default)]
    pub note: Option<String>,
}

/// A parsed per-day label file (`harness-data/<day>/labels.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DayLabels {
    pub date: String,
    #[serde(default, rename = "session")]
    pub sessions: Vec<LabeledSession>,
}

/// Render a unix-ms instant as a local `"HH:MM"` given the day's `local_midnight_ms` (no
/// timezone library needed — all callers work within one exported local day). An instant in
/// the next day reads as `"24:MM"`+ and one before midnight carries a `-` sign, so a stray
/// value is visible rather than silently wrong.
pub fn local_hhmm(ms: i64, local_midnight_ms: i64) -> String {
    let min = (ms - local_midnight_ms).div_euclid(60_000);
    let (sign, m) = if min < 0 { ("-", -min) } else { ("", min) };
    format!("{sign}{:02}:{:02}", m / 60, m % 60)
}
