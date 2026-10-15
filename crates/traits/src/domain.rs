//! Core domain types shared across modules.
//!
//! Types that hold raw pixel data ([`CapturedFrame`]) are **internal only** — they
//! never cross the typed IPC boundary. Types that the UI needs derive [`ts_rs::TS`]
//! and live in [`crate::ipc`]; a few plain records here ([`VisionAnalysis`],
//! [`MonitorInfo`]) are exported because the IPC layer embeds them.

use std::sync::Arc;

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A connected monitor the capture source can see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MonitorInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// Why a frame was captured (`docs/0.2.0.md` event-driven capture; `07` #47/#57).
///
/// [`CaptureTrigger::Timer`] is the always-present 0.2.0 baseline cadence (and the
/// event-mode fallback); the other variants arrive only when the opt-in event-driven
/// capture mode is enabled. Persisted to `frames.capture_trigger` (`03 §4`) and
/// surfaced in [`crate::ipc::FrameDetail`] so the Moment view can show why a frame was
/// captured. It is a **label, never input content** — no keystrokes or clipboard text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum CaptureTrigger {
    /// Fixed-interval timer cadence (the 0.2.0 baseline; also the event-mode fallback).
    Timer,
    /// The user went idle past the configured threshold.
    Idle,
    /// The foreground window / app changed (`SetWinEventHook` `EVENT_SYSTEM_FOREGROUND`).
    ForegroundChange,
    /// **Legacy** — the clipboard changed (change event only, never contents). Retired
    /// by 0.3.0 PR2 (`docs/0.3.0.md`); no longer emitted, but the token stays readable
    /// for frames captured before the trim so the Moment "Captured via" row still renders.
    ClipboardChange,
    /// **Legacy** — typing/input paused for the quiet period. Retired by 0.3.0 PR2
    /// (redundant with `Idle`); kept only to read pre-trim frames (see `ClipboardChange`).
    TypingPause,
    /// **Legacy** — a mouse button was pressed (the *fact* of a click only, never
    /// position/button/content). Retired by 0.3.0 PR2 with the global mouse hook; kept
    /// only to read pre-trim frames (see `ClipboardChange`).
    Click,
    /// **Legacy** — scrolling stopped after a wheel burst settled (never the position or
    /// amount). Retired by 0.3.0 PR2 with the global mouse hook; kept only to read
    /// pre-trim frames (see `ClipboardChange`).
    ScrollStop,
    /// An explicit user-initiated "capture now" (reserved; no UI affordance yet).
    Manual,
}

impl CaptureTrigger {
    /// The DB token for the `frames.capture_trigger` column (`03 §4`).
    pub fn as_db_str(self) -> &'static str {
        match self {
            CaptureTrigger::Timer => "timer",
            CaptureTrigger::Idle => "idle",
            CaptureTrigger::ForegroundChange => "foreground_change",
            CaptureTrigger::ClipboardChange => "clipboard_change",
            CaptureTrigger::TypingPause => "typing_pause",
            CaptureTrigger::Click => "click",
            CaptureTrigger::ScrollStop => "scroll_stop",
            CaptureTrigger::Manual => "manual",
        }
    }

    /// Parses the DB token back into a [`CaptureTrigger`]; `None` on an unknown token
    /// (including the SQL `NULL` → "unknown/legacy frame" case the caller handles).
    pub fn from_db_str(s: &str) -> Option<Self> {
        let _: fn(Self) = |trigger| match trigger {
            Self::Timer
            | Self::Idle
            | Self::ForegroundChange
            | Self::ClipboardChange
            | Self::TypingPause
            | Self::Click
            | Self::ScrollStop
            | Self::Manual => {}
        };
        match s {
            "timer" => Some(CaptureTrigger::Timer),
            "idle" => Some(CaptureTrigger::Idle),
            "foreground_change" => Some(CaptureTrigger::ForegroundChange),
            "clipboard_change" => Some(CaptureTrigger::ClipboardChange),
            "typing_pause" => Some(CaptureTrigger::TypingPause),
            "click" => Some(CaptureTrigger::Click),
            "scroll_stop" => Some(CaptureTrigger::ScrollStop),
            "manual" => Some(CaptureTrigger::Manual),
            _ => None,
        }
    }
}

/// A single captured (already diff-gated as *changed*) frame.
///
/// Internal: holds the full-resolution RGBA pixels behind an [`Arc`] and never
/// crosses IPC. OCR runs on these pixels **before** any JPEG resize (`03 §8`).
///
/// `app_hint` / `window_title` carry the foreground-window context the capture
/// source already reads for the `privacy.excluded_apps` gate (`03 §8`); the kernel
/// copies them onto the stored [`NewFrame`] so the timeline has context without a
/// second OS call. `None` when the foreground window can't be resolved.
///
/// `target_rect` is the foreground (target) window's rectangle mapped into **this
/// monitor's** frame and normalized to `[0,1]` (origin top-left) — the input PR3's
/// attention filter uses to separate target-window `content` from `background`
/// (`03 §3b`). `None` when the foreground window is on another monitor, minimized, or
/// unresolved; with `None` the filter never classifies a span as `background` (the
/// safe default — false suppression is the top risk).
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub monitor_index: u32,
    pub width: u32,
    pub height: u32,
    /// Capture time, unix epoch milliseconds.
    pub captured_at: i64,
    pub pixels: Arc<RgbaImage>,
    pub content_hash: String,
    /// Foreground app/process name at capture time (privacy gate by-product).
    pub app_hint: Option<String>,
    /// Foreground window title at capture time.
    pub window_title: Option<String>,
    /// Normalized `[0,1]` foreground-window rect within this monitor's frame, or
    /// `None` (other monitor / minimized / unresolved). `[x, y, w, h]`.
    pub target_rect: Option<[f32; 4]>,
    /// Raw handle (as `i64`) of the foreground window at capture time, or `None` when
    /// unresolved/minimized. A text provider that reads the *live* foreground window
    /// (UIA) compares it against `GetForegroundWindow` at recognition time and bails to
    /// OCR if focus changed in between — otherwise a different window's text could be
    /// attributed to this frame (`07` #48). Plain integer so the frame stays `Send`.
    pub foreground_hwnd: Option<i64>,
    /// Why this frame was captured. The capture source stamps it at the moment it
    /// decides to capture; the kernel copies it onto the stored [`NewFrame`]. Always
    /// [`CaptureTrigger::Timer`] in the 0.2.0 timer/idle path.
    pub trigger: CaptureTrigger,
    /// `true` only for the one frame that satisfies a `capture_now` demand (0.3.0 PR6,
    /// mark-this-moment; `03 §7b`, D8). The kernel loop reads it to correlate "the
    /// marked frame" — after inserting it, the kernel hands its `frame_id` back to the
    /// waiting `capture_now` caller. Always `false` on the normal timer/event path (a
    /// multi-monitor `capture_now` cycle stores every changed monitor but flags only the
    /// foreground-window monitor's frame — `03 §7b`).
    pub demanded: bool,
}

/// Origin of a text span / the primary text of a frame (`03 §3b`). Serializes to
/// the DB `source` / `primary_source` columns (`'ocr' | 'uia'`, `03 §4`). UIA is
/// modelled now but only produced from 0.2.1 (`07` #48); 0.2.0 OCR is always `Ocr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum TextSource {
    Ocr,
    Uia,
}

impl TextSource {
    /// The DB token for the `source` / `primary_source` columns (`03 §4` CHECK).
    pub fn as_db_str(self) -> &'static str {
        match self {
            TextSource::Ocr => "ocr",
            TextSource::Uia => "uia",
        }
    }

    /// Parses the DB token back into a [`TextSource`]; `None` on an unknown token.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "ocr" => Some(TextSource::Ocr),
            "uia" => Some(TextSource::Uia),
            _ => None,
        }
    }
}

/// Classified role of a text span (`03 §3b`). Serializes to the DB `role` column
/// (`03 §4` CHECK). PR2 emits every span as [`TextRole::Unknown`]; PR3's classifier
/// assigns the real roles and drops non-`content` spans from `content_text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum TextRole {
    Content,
    Chrome,
    Background,
    System,
    Unknown,
}

impl TextRole {
    /// The DB token for the `role` column (`03 §4` CHECK).
    pub fn as_db_str(self) -> &'static str {
        match self {
            TextRole::Content => "content",
            TextRole::Chrome => "chrome",
            TextRole::Background => "background",
            TextRole::System => "system",
            TextRole::Unknown => "unknown",
        }
    }

    /// Parses the DB token back into a [`TextRole`]; `None` on an unknown token.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "content" => Some(TextRole::Content),
            "chrome" => Some(TextRole::Chrome),
            "background" => Some(TextRole::Background),
            "system" => Some(TextRole::System),
            "unknown" => Some(TextRole::Unknown),
            _ => None,
        }
    }
}

/// Why a span was excluded from `content_text` (`03 §3b`). `Option<SuppressReason>`
/// maps to the nullable `text_spans.suppress_reason` column — `None` = a searchable,
/// non-suppressed span (no redundant in-enum `None` variant, `03 §4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum SuppressReason {
    StaticChrome,
    SystemUi,
    BackgroundWindow,
}

impl SuppressReason {
    /// The DB token for the `suppress_reason` column (`03 §4` CHECK).
    pub fn as_db_str(self) -> &'static str {
        match self {
            SuppressReason::StaticChrome => "static_chrome",
            SuppressReason::SystemUi => "system_ui",
            SuppressReason::BackgroundWindow => "background_window",
        }
    }

    /// Parses the DB token back into a [`SuppressReason`]; `None` on an unknown token
    /// (including the SQL `NULL` → "not suppressed" case the caller handles).
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "static_chrome" => Some(SuppressReason::StaticChrome),
            "system_ui" => Some(SuppressReason::SystemUi),
            "background_window" => Some(SuppressReason::BackgroundWindow),
            _ => None,
        }
    }
}

/// One OCR/UIA text span with normalized `[0,1]` geometry (`03 §3b`). Carried on
/// [`OcrResult::spans`] and persisted to `text_spans` (`03 §4`). Also surfaced over the
/// typed IPC boundary by `get_frame_spans` so the UI can render a crisp text+layout
/// **reconstruction** of a frame whose screenshot has been retention-degraded (the
/// stored geometry + role is enough to redraw what was on screen — "text is enough" as
/// durable proof).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct TextSpan {
    /// The recognized text, verbatim.
    pub text: String,
    /// Normalized form used for chrome-signature matching (`03 §3b`,
    /// [`normalize_text`]).
    pub normalized_text: String,
    pub source: TextSource,
    pub role: TextRole,
    /// Normalized `[0,1]` bounding box (origin top-left), relative to the
    /// full-resolution frame the OCR ran on.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Zero-based index of the OCR line this word belongs to (`Lines()` order). The
    /// engine groups words into lines; carrying the index lets PR3's classifier group
    /// spans back into lines exactly (no geometry heuristic) for line-level chrome
    /// signatures and `content_text` reassembly (`03 §3b`).
    pub line_index: u32,
    /// Whether the span is included in searchable text. PR2 marks every span
    /// searchable; PR3 sets this from the classified role.
    pub is_searchable: bool,
    pub suppress_reason: Option<SuppressReason>,
}

/// Normalizes span text for chrome-signature matching and dedup (`03 §3b`):
/// lowercased, internal whitespace collapsed to single spaces, ends trimmed. Shared
/// so the OCR producer and PR3's classifier derive identical signatures.
pub fn normalize_text(s: &str) -> String {
    // Single output allocation (capacity-hinted): collapse internal whitespace by
    // joining words with single spaces, then lowercase. Hot path — one span per OCR
    // word — so avoid the intermediate Vec + join allocations.
    let mut result = String::with_capacity(s.len());
    let mut words = s.split_whitespace();
    if let Some(first) = words.next() {
        result.push_str(first);
        for word in words {
            result.push(' ');
            result.push_str(word);
        }
    }
    result.to_lowercase()
}

/// Result of running OCR over a [`CapturedFrame`]. `spans` carry per-word geometry
/// for the 0.2.x text-signal pipeline (`03 §3/§3b`); empty when the engine produced
/// no words.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub mean_confidence: f32,
    pub engine: String,
    pub spans: Vec<TextSpan>,
}

/// A dense embedding vector. Length always equals the provider's
/// [`EmbeddingProvider::dim`](crate::EmbeddingProvider::dim) (768).
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    /// Number of dimensions in the vector.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the vector has no dimensions (an invalid/empty embedding).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Deferred vision-tagging output for a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct VisionAnalysis {
    pub description: String,
    pub activity_type: Option<String>,
    pub app_hint: Option<String>,
    pub confidence: f32,
    pub model: String,
}

/// A new frame row to insert into the [`Store`](crate::Store).
#[derive(Debug, Clone)]
pub struct NewFrame {
    /// Capture time, unix epoch milliseconds.
    pub captured_at: i64,
    pub monitor_index: u32,
    pub width: u32,
    pub height: u32,
    /// Relative path to the stored capture image on disk (lossless WebP; legacy rows may
    /// be `.jpg`).
    pub image_path: String,
    pub content_hash: String,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    /// Why the frame was captured (`frames.capture_trigger`, `03 §4`). `None` for
    /// legacy frames captured before this column existed; new captures always set it.
    pub capture_trigger: Option<CaptureTrigger>,
}

/// The minimal frame inputs an enrichment worker needs to embed a frame: the
/// stored JPEG's relative path and the OCR text (if any was recognized). One cheap
/// read for the embedding worker (`03 §5`), vs the full [`FrameDetail`] the
/// `get_frame` command assembles. `ocr_text` is `None` when no OCR row exists yet.
#[derive(Debug, Clone)]
pub struct FrameEnrichmentInput {
    pub image_path: String,
    pub ocr_text: Option<String>,
}

/// One frame's context row for the where-was-i heuristic (`03 §7b`, D9). Internal —
/// it never crosses IPC; the heuristic's *output* is [`crate::ipc::ResumeContext`].
/// The store returns these newest-first (`Store::recent_frame_contexts`); the pure
/// `kernel::resume` function walks them to find the last sustained context. Carries
/// `window_title`/`browser_url` (which [`crate::ipc::FrameMeta`] omits) because the
/// context key is `app_hint` refined by browser domain, and the representative frame
/// needs the title/URL for its display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameContextRow {
    pub frame_id: i64,
    /// Capture time, unix epoch milliseconds.
    pub captured_at: i64,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    pub image_path: String,
    pub image_purged: bool,
}

/// One `capture_now` demand handed to the **capture worker** that drives
/// [`CaptureSource::next_frame`](crate::CaptureSource) (0.3.0 PR6, mark-this-moment;
/// `03 §7b`, D8). Deliberately **not** a [`CaptureSource`](crate::CaptureSource)
/// method — the diff gate lives in the worker, and a demanded frame must bypass it
/// (a demanded frame must never be dropped as "unchanged"). The worker acks once the
/// demanded frame is queued for the kernel loop (or on a privacy-gate denial / device
/// failure): `Ok(())` guarantees the loop will observe exactly one `demanded` frame;
/// `Err(reason)` is surfaced to the user honestly (e.g. "capture is off", "the focused
/// app is excluded from capture") — never a silent no-op.
#[derive(Debug)]
pub struct CaptureNowRequest {
    pub ack: tokio::sync::oneshot::Sender<crate::Result<()>>,
}

/// The receiving half of the `capture_now` demand channel, handed to the capture
/// source at construction so its wait loop can select on demands alongside the timer
/// tick / event trigger (`03 §7b`). The kernel holds the [`tokio::sync::mpsc::Sender`]
/// on the live capture handle.
pub type CaptureNowReceiver = tokio::sync::mpsc::Receiver<CaptureNowRequest>;

/// Configuration handed to a [`CaptureSource`](crate::CaptureSource) at
/// construction. The kernel derives it from [`Settings`](crate::Settings) when
/// capture starts (`03 §8`); it is shared here so the kernel can build it and the
/// capture impl can consume it without either depending on the other (`03 §2`).
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureConfig {
    /// Delay between capture cycles, ms (`capture.interval_ms`).
    pub interval_ms: u32,
    /// Monitor indices to capture; empty = all (`capture.monitors`).
    pub monitors: Vec<u32>,
    /// Normalized [0,1] change ratio above which a frame is kept
    /// (`capture.diff_threshold`).
    pub diff_threshold: f32,
    /// Foreground app/process names whose frames are skipped — case-insensitive
    /// substring match (`privacy.excluded_apps`).
    pub excluded_apps: Vec<String>,
    /// Pause capture entirely while the workstation is locked
    /// (`privacy.pause_on_lock`).
    pub pause_on_lock: bool,
    /// Event-driven capture master switch (`capture.event_driven_enabled`). When
    /// `false` (default), capture is the 0.2.0 timer/idle cadence only and **no input
    /// hooks are installed**. When `true`, the capture source also fires on the enabled
    /// user-activity triggers below, with a long fallback timer so a static screen is
    /// still sampled (`docs/0.2.0.md` event-driven capture; `07` #47).
    pub event_driven_enabled: bool,
    /// Capture on foreground/app switch (`capture.event_on_foreground`). *(0.3.0 PR2
    /// trimmed the six event triggers to foreground + idle — `docs/0.3.0.md`.)*
    pub event_on_foreground: bool,
    /// Capture when the user goes idle past the threshold (`capture.event_on_idle`).
    pub event_on_idle: bool,
    /// Collapse a burst of triggers within this window into one capture, ms
    /// (`capture.event_debounce_ms`).
    pub event_debounce_ms: u32,
    /// Minimum gap between any two event-driven captures, ms — the rate ceiling that
    /// stops a trigger storm (`capture.event_min_interval_ms`).
    pub event_min_interval_ms: u32,
    /// Idle time that counts as "gone idle", ms (`capture.event_idle_threshold_ms`).
    pub event_idle_threshold_ms: u32,
    /// Fallback capture interval in event mode, ms — guarantees a static screen is
    /// still sampled even with no input activity (`capture.event_fallback_interval_ms`).
    pub event_fallback_interval_ms: u32,
}

/// Per-frame inputs for PR3's attention filter that aren't already on the stored
/// frame (`03 §3b`). The store reads `target_app_hint`/`target_window_title` from the
/// `frames` row itself; this carries the foreground rect (a capture-time OS fact) plus
/// the configurable suppression thresholds (`03 §8`). Kept in `traits` (plain fields)
/// so the `Store` trait can name it without depending on the `textfilter` crate (which
/// depends on `traits`, not the reverse — `03 §2`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextFilterContext {
    /// Normalized `[0,1]` foreground-window rect within the frame, or `None`
    /// (other monitor / minimized / unresolved → no positional suppression).
    pub target_rect: Option<[f32; 4]>,
    /// `text.chrome_suppress_min_seen` (`03 §8`).
    pub chrome_suppress_min_seen: u32,
    /// `text.chrome_protect_min_chars` (`03 §8`).
    pub chrome_protect_min_chars: u32,
    /// `text.chrome_region_buckets` (`03 §8`).
    pub chrome_region_buckets: u32,
}

/// Origin of an embedded text chunk. Serializes to the DB `source` column
/// (`'ocr' | 'vision_description'`, `03 §4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkSource {
    Ocr,
    VisionDescription,
}

/// A retrieved context chunk handed to the answer provider for grounding.
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub frame_id: i64,
    pub text: String,
    pub score: f32,
    /// Capture time of the source frame, unix epoch milliseconds.
    pub captured_at: i64,
}

/// Options controlling a single `answer` call.
#[derive(Debug, Clone, Copy)]
pub struct AnswerOpts {
    pub thinking: bool,
    pub max_tokens: u32,
}

/// One frame row of the v1 JSON export (0.3.0 PR7; `03 §7c`, D12): metadata +
/// `content_text` only — never `raw_text`, never image paths/bytes. Produced by the
/// store's `export_frames_page` cursor and serialized incrementally by
/// `crates/api::export` (the same code path the HTTP `GET /v1/export` route and the
/// Settings "Export…" command share). Serialize-only and *not* a `ts_rs` type: it is
/// an external wire record, never part of the UI IPC contract, and lives here in
/// `traits` so both `store` (producer) and `api` (consumer) reference it without a
/// module→module dependency.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportFrameRow {
    pub frame_id: i64,
    /// Capture time, unix epoch milliseconds.
    pub captured_at: i64,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    pub activity_type: Option<String>,
    /// The attention-filtered content text (never the raw full-screen OCR).
    pub content_text: Option<String>,
}

/// Which calendar range a recall report covers (`03 §8b`). Carried for honest
/// framing (the range label + prose); the concrete `[start, end)` is resolved by
/// the command (in local time) before the orchestrator runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportRange {
    Daily,
    /// Durable daily memory uses coverage retrieval and explicit dated framing.
    MemoryDaily,
    Weekly,
    Custom,
}

/// How a report was generated — drives the honest footer and is observable in
/// tests without a live sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportMode {
    /// Zero non-empty content text in range: honest no-evidence output, no sidecar
    /// call was made.
    Empty,
    /// One summarize pass over the packed content text (small range / fast path).
    SinglePass,
    /// Per-period map then (possibly hierarchical) reduce (`03 §8b`).
    MapReduce,
}

/// Tunable report knobs assembled from [`crate::ipc::Settings`] by the command and
/// passed to the kernel orchestrator (`03 §8`/`§8b`), so the orchestrator carries no
/// settings-loading dependency. Structural safety bounds (fan-out, max periods/
/// passes) live as constants in the orchestrator, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportConfig {
    /// `reports.daily_top_k` — target sampled frames per active period.
    pub daily_top_k: u32,
    /// `reports.weekly_top_k` — global cap on frames summarized across all periods.
    pub weekly_top_k: u32,
    /// `reports.map_reduce_min_frames` — at/below ⇒ single pass; above ⇒ map-reduce.
    pub map_reduce_min_frames: u32,
    /// Reply token budget (`n_predict`) for each summarize pass.
    pub reply_budget: u32,
}

/// The result of a report generation (`generate_report`, `03 §8b`). Carries the
/// markdown body, the frames cited, and auditable coverage/cost metadata so the UI
/// footer states honestly what was read ("retrieve up to N, summarize what fits").
#[derive(Debug, Clone)]
pub struct ReportOutput {
    pub markdown: String,
    /// Frames the model actually read (deduped, in inclusion order).
    pub cited_frame_ids: Vec<i64>,
    /// Full distinct frame IDs actually summarized, without the UI citation cap.
    pub frame_ids: Vec<i64>,
    /// How the report was produced.
    pub mode: ReportMode,
    /// Periods in range (active + empty).
    pub periods_total: u32,
    /// Active periods that were summarized.
    pub periods_covered: u32,
    /// Frames sampled into the map step.
    pub frames_sampled: u32,
    /// Frames actually summarized (== `frame_ids.len()`).
    pub frames_summarized: u32,
    /// Total sidecar passes (map + reduce + final); `0` for a no-evidence report.
    pub passes: u32,
    /// A structural bound forced coarser sampling than requested (honest framing).
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::CaptureTrigger;

    #[test]
    fn capture_trigger_db_str_round_trips() {
        let covered = [
            CaptureTrigger::Timer,
            CaptureTrigger::Idle,
            CaptureTrigger::ForegroundChange,
            CaptureTrigger::ClipboardChange,
            CaptureTrigger::TypingPause,
            CaptureTrigger::Click,
            CaptureTrigger::ScrollStop,
            CaptureTrigger::Manual,
        ];
        for t in covered {
            assert_eq!(
                CaptureTrigger::from_db_str(t.as_db_str()),
                Some(t),
                "round-trip failed for {t:?}"
            );
        }
    }

    #[test]
    fn capture_trigger_from_unknown_db_str_is_none() {
        assert_eq!(CaptureTrigger::from_db_str("nope"), None);
        assert_eq!(CaptureTrigger::from_db_str(""), None);
    }
}
