//! Typed UI ↔ core contract: command inputs/outputs and event payloads (`03 §7`).
//!
//! Every type here derives [`ts_rs::TS`] and exports to `ui/src/bindings/`, so the
//! UI consumes generated types only — never a hand-written duplicate (no contract
//! drift, `04` UI guardrails).
//!
//! **Convention:** every `i64`/`u64` field carries `#[ts(type = "number")]`. Tauri
//! serializes over serde_json where 64-bit ints become JS `number`, so the bindings
//! must say `number`, not ts-rs's default `bigint`. Frame ids and unix-ms timestamps
//! stay well under 2^53, so there is no precision loss. (`03 §7`.)

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::advanced::AdvancedConfig;
use crate::domain::{CaptureTrigger, TextSource, VisionAnalysis};
use crate::jobs::{JobKind, JobStats};
use crate::sessions::{Session, SessionArtifact, SessionHost, SessionKind};

/// Half-open `[start, end)` time window (start inclusive, end exclusive), unix
/// epoch milliseconds. `Store::hybrid_search` filters with `captured_at >= start
/// AND captured_at < end`, so callers must pass an exclusive upper bound — a frame
/// captured exactly at `end` is *not* included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct TimeRange {
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
}

/// Input to the `search` command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SearchQuery {
    pub text: String,
    pub limit: u32,
    pub time_range: Option<TimeRange>,
    /// Also search raw/app-chrome text, not just `content_text` (`03 §3b`). Default
    /// `false` → retrieval over content text only. `#[serde(default)]` so a client
    /// that omits it gets the safe default.
    #[serde(default)]
    pub include_chrome: bool,
}

/// One hybrid-search result row (`search` output).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SearchHit {
    #[ts(type = "number")]
    pub frame_id: i64,
    #[ts(type = "number")]
    pub captured_at: i64,
    pub snippet: String,
    pub score: f32,
    pub image_path: String,
    /// `true` once this frame's screenshot has been retention-degraded — text proof kept,
    /// image gone. Purged frames still match search (the text is preserved); the result
    /// tile shows a "screenshot expired" state instead of a broken thumbnail.
    pub image_purged: bool,
    pub app_hint: Option<String>,
}

/// A lightweight frame reference for browsing — the `get_frames` and
/// `get_nearest_frame` outputs (P5). Carries only what a tile/thumbnail needs
/// (frame id, capture time, the stored JPEG's relative path, and the foreground
/// app hint), without the OCR text / vision / tags that [`FrameDetail`] hydrates.
/// Drives the Timeline hover thumbnails, the Deck "jump back in" recents, and a
/// Moment's neighbour context; open one with `get_frame(frame_id)` for full detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct FrameMeta {
    #[ts(type = "number")]
    pub frame_id: i64,
    #[ts(type = "number")]
    pub captured_at: i64,
    pub image_path: String,
    pub app_hint: Option<String>,
    /// `true` once this frame's screenshot has been retention-degraded — the JPEG/WebP
    /// file is gone but the text proof remains (`storage.retention_days`). The UI shows a
    /// "screenshot expired, text kept" state instead of a broken thumbnail.
    pub image_purged: bool,
}

/// Filters for the Timeline/session list. The command normalizes an empty `tool`
/// to no filter and defaults/clamps `limit` at the shell boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SessionQuery {
    pub time_range: Option<TimeRange>,
    pub kind: Option<SessionKind>,
    pub tool: Option<String>,
    pub limit: Option<u32>,
}

/// Compact session identity joined onto frame and resume responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SessionReference {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub started_at: i64,
    #[ts(type = "number | null")]
    pub ended_at: Option<i64>,
    pub kind: SessionKind,
    pub tool: Option<String>,
    pub host: Option<SessionHost>,
    pub title: Option<String>,
    pub confidence: f64,
}

/// Session drill-in payload: bounded representative frames plus every extracted
/// exchange. `frame_count` remains the truthful total even when `frames` is sampled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SessionDetail {
    pub session: Session,
    pub frames: Vec<FrameMeta>,
    pub frame_count: u32,
    pub exchanges: Vec<SessionArtifact>,
}

/// Request-scoped in-app session Recap. Progress and cancellation reuse the existing
/// report request id machinery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SessionRecapRequest {
    #[ts(type = "number")]
    pub session_id: i64,
    pub request_id: Option<String>,
}

/// Input to the `ask` command. The answer streams back via request-scoped `answer_delta` events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AskRequest {
    pub request_id: Option<String>,
    pub query: String,
    pub thinking: bool,
    pub max_tokens: u32,
    /// Per-request retrieval-depth override (`03 §8` `retrieval.default_top_k`).
    /// `None` → the configured default. `#[serde(default)]` so existing callers
    /// that omit it keep working.
    #[serde(default)]
    pub top_k: Option<u32>,
}

/// Which range a recall report covers (`03 §8b`). The UI resolves the concrete
/// local `[start, end)` for every kind and sends it as `ReportRequest.time_range`;
/// `kind` selects the retrieval depth and the human range label, and gates the
/// coverage-vs-relevance retrieval path (`Custom` + a `prompt` → semantic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportKind {
    Daily,
    Weekly,
    Custom,
}

/// Input to the `generate_report` command (`03 §7`/`§8b`). Progress streams via
/// `report_progress` events scoped by `request_id`; the final value returns from
/// the awaited command. `prompt` (Custom only) steers the summary via semantic
/// retrieval; Daily/Weekly/Custom-without-prompt use temporal-coverage sampling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportRequest {
    pub kind: ReportKind,
    pub time_range: TimeRange,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

/// Output of `generate_report` (`03 §8b`). Markdown body + the frames cited, plus
/// auditable coverage/cost metadata so the UI footer states honestly what was read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportResponse {
    pub markdown: String,
    /// Frames the model actually read (deduped, in inclusion order).
    #[ts(type = "Array<number>")]
    pub cited_frame_ids: Vec<i64>,
    /// Human label for the covered range ("today", "the last 7 days", …).
    pub range_label: String,
    /// Periods in range (active + empty).
    pub periods_total: u32,
    /// Active periods that were summarized.
    pub periods_covered: u32,
    /// Frames sampled into the map step.
    pub frames_sampled: u32,
    /// Unique frames the model actually read across all map passes. May exceed
    /// `cited_frame_ids.len()` when the citation list is capped at `MAX_REPORT_CITATIONS`
    /// (e.g. a long range where the per-period floor pushes the union past the cap).
    pub frames_summarized: u32,
    /// Total sidecar passes (map + reduce + final); `0` for a no-evidence report.
    pub passes: u32,
    /// A structural bound forced coarser sampling than requested (honest framing).
    pub truncated: bool,
    /// Resolved model identifier (GGUF filename) for the footer; `None` if empty.
    pub model: Option<String>,
}

/// Progress of an in-flight `generate_report` (`report_progress` event), scoped by
/// `request_id`. `stage` is a short human label ("Summarizing day 3 of 7",
/// "Combining summaries"); `done`/`total` drive a determinate progress bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportProgress {
    pub request_id: String,
    pub stage: String,
    pub done: u32,
    pub total: u32,
}

/// Request-scoped streamed answer event (`answer_delta` payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AnswerEvent {
    pub request_id: String,
    pub delta: AnswerDelta,
}

/// One bucket of the timeline histogram (`get_timeline` output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct TimelineBucket {
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
    pub count: u32,
}

/// Aggregate activity summary over a time window (`get_insights` output).
///
/// The spec defines no Insights contract (silent gap, logged in `07`). This is the
/// chosen shape: real DB aggregates only — totals, capture density over time
/// (reusing [`TimelineBucket`]), the top foreground apps, and the vision
/// activity-type breakdown. [`Default`] is the honest-empty summary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct InsightsSummary {
    /// Frames captured in the window.
    #[ts(type = "number")]
    pub total_frames: i64,
    /// Frames in the window that carry a vision `activity_type`.
    #[ts(type = "number")]
    pub tagged_frames: i64,
    /// Capture density across the window (sparse, ascending by time).
    pub captures: Vec<TimelineBucket>,
    /// Most-captured foreground apps, descending by frame count.
    pub top_apps: Vec<AppCount>,
    /// Vision activity-type breakdown, descending by frame count.
    pub activity_breakdown: Vec<ActivityCount>,
}

/// Storage footprint shown in the StatusRail.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct StorageStats {
    #[ts(type = "number")]
    pub db_bytes: u64,
    #[ts(type = "number")]
    pub frame_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
}

/// One row of the per-app text-filter suppression metric (`get_text_filter_stats`
/// output, `03 §3b`). The guardrail that makes silent over-suppression observable:
/// `rate` = `suppressed_spans / total_spans` over frames classified by the live
/// `filter_version`. `app` (the foreground/target app) is `None` for frames with no
/// resolved foreground app. False suppression is the top risk, so this is surfaced
/// in the UI and recoverable via `include_chrome` + preserved `raw_text`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AppSuppression {
    pub app: Option<String>,
    /// Total classified spans for this app.
    #[ts(type = "number")]
    pub total_spans: i64,
    /// Spans dropped from `content_text` (role `chrome`/`system`/`background`).
    #[ts(type = "number")]
    pub suppressed_spans: i64,
    /// `suppressed_spans / total_spans`, in `[0,1]` (`0` when `total_spans == 0`).
    pub rate: f32,
}

/// One row of the [`InsightsSummary`] top-apps breakdown. `app` is `None` for
/// frames with no resolved foreground app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AppCount {
    pub app: Option<String>,
    pub count: u32,
}

/// One row of the [`InsightsSummary`] activity-type breakdown. `activity` is the
/// vision-assigned label (only tagged frames are counted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ActivityCount {
    pub activity: Option<String>,
    pub count: u32,
}

/// Full detail for a single frame (`get_frame` output).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct FrameDetail {
    #[ts(type = "number")]
    pub frame_id: i64,
    #[ts(type = "number")]
    pub captured_at: i64,
    pub monitor_index: u32,
    pub width: u32,
    pub height: u32,
    pub image_path: String,
    /// `true` once this frame's screenshot has been retention-degraded — the image file
    /// is gone but the text proof (raw/content text + `text_spans`) remains. The Moment
    /// view renders the layout reconstruction instead of the image (`storage.retention_days`).
    pub image_purged: bool,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    pub activity_type: Option<String>,
    /// Why the frame was captured (`frames.capture_trigger`, 0.2.1 event-driven
    /// capture). `None` for legacy frames or an unrecognized token; the Moment view
    /// shows it as "why captured".
    pub capture_trigger: Option<CaptureTrigger>,
    /// Full, unfiltered OCR/UIA text — always preserved (`03 §3b`). `None` when no
    /// `frame_text` row exists yet.
    pub raw_text: Option<String>,
    /// Filtered default-retrieval text (`03 §3b`). In 0.2.0 this is a passthrough
    /// copy of `raw_text` until PR3's classifier lands (`07` #51).
    pub content_text: Option<String>,
    /// Which engine produced the primary text (`ocr` in 0.2.0; `uia` from 0.2.1).
    pub text_source: TextSource,
    /// Spans dropped from `content_text` (the suppression-rate metric, `03 §3b`).
    /// Always `0` in PR2 (no filtering yet).
    pub suppressed_text_count: u32,
    pub vision: Option<VisionAnalysis>,
    pub tags: Vec<String>,
}

/// Tauri-only frame response. The flattened base preserves the shipped Moment wire
/// shape while the in-app command adds a session reference without changing HTTP/MCP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct UiFrameDetail {
    #[serde(flatten)]
    #[ts(flatten)]
    pub frame: FrameDetail,
    pub session: Option<SessionReference>,
}

/// Target of an `enqueue_vision` request: a single frame or a time range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum VisionTarget {
    Frame {
        #[ts(type = "number")]
        frame_id: i64,
    },
    Range {
        #[ts(type = "number")]
        start: i64,
        #[ts(type = "number")]
        end: i64,
    },
}

/// One of the two inference lanes (`03 §6`, `MODEL_REGISTRY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ModelLane {
    Vision,
    Answer,
}

/// User-selectable model tier per lane (`00 §E`). 0.3.0 retired the `Beta` tier
/// (both lanes uniformly Apache-2.0); a persisted `beta` selection maps to `quality`
/// on load, logged once + persisted (`03 §8`, `docs/0.3.0.md` PR3, D3/D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ModelTier {
    Default,
    Quality,
}

/// KV-cache element type for the llama.cpp sidecar (`--cache-type-k`/`--cache-type-v`).
/// Lower precision shrinks the on-GPU KV cache (less VRAM) for a small quality cost.
/// `Q8_0` is a near-lossless default; `F16` is the no-compromise escape hatch; `Q4_0`
/// is the smallest. The `#[serde(rename)]`s pin the wire/TS strings to the exact tokens
/// `llama-server` expects, so a stored value is also the launch-arg value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum KvCacheType {
    #[serde(rename = "f16")]
    F16,
    #[serde(rename = "q8_0")]
    Q8_0,
    #[serde(rename = "q4_0")]
    Q4_0,
}

impl KvCacheType {
    /// The `--cache-type-k`/`--cache-type-v` argument value llama.cpp expects.
    pub fn as_arg(self) -> &'static str {
        match self {
            KvCacheType::F16 => "f16",
            KvCacheType::Q8_0 => "q8_0",
            KvCacheType::Q4_0 => "q4_0",
        }
    }

    /// Whether this is a quantized (non-`f16`) cache type. Quantized KV requires flash
    /// attention, so the sidecar only emits `--cache-type-*` for a quantized type when
    /// flash attention is active.
    pub fn is_quantized(self) -> bool {
        !matches!(self, KvCacheType::F16)
    }
}

/// Flash-attention mode for the llama.cpp sidecar (`--flash-attn`). `Auto` follows what
/// the bundled binary supports (it resolves to on when the flag exists, off otherwise);
/// `On`/`Off` force the choice. Flash attention reduces the attention compute buffer and
/// is a prerequisite for quantized KV cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum FlashAttnSetting {
    Auto,
    On,
    Off,
}

/// The llama.cpp launch knobs derived from [`crate::AdvancedConfig`], threaded to both
/// inference providers and on into the sidecar's argument list. Bundled into one struct so
/// adding a knob does not ripple through every provider/`resolve_spec` signature. Not an
/// IPC type (constructed in the core), so it carries no `ts-rs` derive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarParams {
    pub ngl: u32,
    pub device: Option<String>,
    /// `0` = automatic (a per-lane default chosen at model resolution); otherwise the
    /// shared context-window override for both lanes.
    pub ctx_size: u32,
    pub kv_cache_type: KvCacheType,
    pub flash_attn: FlashAttnSetting,
}

/// Input to `set_model_tier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SetModelTier {
    pub lane: ModelLane,
    pub tier: ModelTier,
}

/// Input to `capture_control`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum CaptureControl {
    Start,
    Stop,
}

/// Read-only projection of the `file-only` settings for Settings · Advanced
/// (`get_advanced_config`, `03 §8`, 0.4.5 D6).
///
/// Carries the **effective** [`AdvancedConfig`] the backend is running with (file values
/// clamped by `AdvancedConfig::sanitized`), the absolute path of `advanced.toml`, and the
/// `section.field` keys whose value the file actually supplied. There is deliberately no
/// setter and no writable surface: a `file-only` key is changed by editing that file and
/// restarting, never from the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AdvancedConfigView {
    /// The effective configuration - exactly what the backend reads.
    pub config: AdvancedConfig,
    /// Absolute path of `advanced.toml` (the file may not exist; the defaults then apply).
    pub path: String,
    /// `section.field` keys parsed from the file, e.g. `overlay.max_results`. Every key
    /// absent from this list still holds its compiled default. A listed value may have
    /// been clamped into its `03 §8` range by `AdvancedConfig::sanitized`.
    pub from_file: Vec<String>,
}

/// Theme appearance select (`ui.appearance`, `03 §8`; 0.4.5 P2 pick 2026-11-04).
///
/// The shell theme applier sets `data-theme` on the root element from this value, and the
/// `*-system` variants follow the OS light/dark preference inside the chosen accent. P7
/// renders the select and resolves `*-system` through `prefers-color-scheme`; P3 owns this
/// registry entry and its persistence only. The four concrete tokens are exactly the
/// `data-theme` ids defined in `ui/src/styles/tokens.css`; `*-system` is a stored intent,
/// not a fifth theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum UiAppearance {
    /// Default; matches the shipped pine-light theme.
    #[default]
    PineLight,
    PineDark,
    PineSystem,
    InkLight,
    InkDark,
    InkSystem,
}

/// User-facing settings (`get_settings`/`set_settings`): **exactly** the `UI` rows of the
/// `03 §8` table (settings v2, 0.4.5 D6), with that table's defaults and clamps. The three
/// places where the as-built value still differs from the table are tracked in
/// `specs/06_PATCH_PLAN.md` #33, pending a maintainer ruling. Persisted as key/value rows
/// in the `settings` table.
///
/// The `file-only` keys do not live here: they are typed in [`crate::AdvancedConfig`] and
/// read once at startup from `advanced.toml`. `api.token` / `api.read_token` are
/// credentials owned by the API commands (they share one Integrations row), and
/// `app.tray_toast_done` is a direct KV marker, so none of the three is a field here.
///
/// Deferred vision tagging is never real-time (`03 §5`). On-demand tagging is always
/// available; timed enrichment is opt-in (`enrich.vision_timer_*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct Settings {
    // General
    /// Register ScreenSearch to launch at login (`app.run_at_startup`, 0.3.2 PR3;
    /// `03 §7d`, D3). Default **off** - an explicit user choice, never silently enabled.
    /// Applied register-before-persist: the OS registration is attempted before the
    /// setting is saved, so a save can't claim a launch-at-login that didn't take.
    pub app_run_at_startup: bool,
    /// Closing the main window hides it to the tray and keeps capturing
    /// (`app.close_to_tray`, 0.3.2 PR3; `03 §7d`, D3). Default **on**. A one-time toast
    /// explains it on the first restore from the tray; with this off, window close quits
    /// cleanly (the sidecar is torn down via the Job-Object lifecycle, `03 §6`).
    pub app_close_to_tray: bool,
    /// Global Flow overlay summon hotkey (`overlay.hotkey`, 0.3.0 PR5). The shell
    /// validates and registers the chord so a bad/colliding value becomes the D6
    /// Settings warning instead of being silently rewritten here.
    pub overlay_hotkey: String,
    /// Global mark-this-moment hotkey (`marks.hotkey`, 0.3.0 PR6; `03 §7b`, D6). Same
    /// shell-registered/validated posture as `overlay_hotkey`: a bad/colliding value
    /// surfaces as the D6 Settings warning + toast, never a silent rewrite.
    pub marks_hotkey: String,
    /// Theme appearance (`ui.appearance`, `03 §8`; 0.4.5 P2 pick 2026-11-04). Persisted
    /// like every other UI key; the shell theme applier and P7's Settings > General select
    /// consume it. `#[serde(default)]` keeps a `config_snapshot` written before settings v2
    /// deserializable (`03 §8c`).
    #[serde(default)]
    pub ui_appearance: UiAppearance,
    // Capture
    pub capture_interval_ms: u32,
    /// Empty = all monitors.
    pub capture_monitors: Vec<u32>,
    // Privacy
    pub privacy_excluded_apps: Vec<String>,
    pub privacy_pause_on_lock: bool,
    // Intelligence
    /// Opt-in: tag up to a batch of untagged frames every `vision_timer_interval_ms`.
    pub enrich_vision_timer_enabled: bool,
    pub enrich_vision_timer_interval_ms: u32,
    /// Default Ask retrieval depth (`03 §8` `retrieval.default_top_k`), replacing the
    /// former hardcoded `ASK_TOP_K`. The per-request `AskRequest.top_k` overrides it.
    pub retrieval_default_top_k: u32,
    /// Recall-report target sampled frames **per active period** (`03 §8`
    /// `reports.daily_top_k`). Report depth scales as this × active periods (`§8b`).
    pub reports_daily_top_k: u32,
    /// Minimum emitted session length (`sessions.min_len_secs`, 0.4.0 PR4). Final clamp
    /// 30..=3600 seconds.
    pub sessions_min_len_secs: u32,
    /// Pass-1 micro gap close (`sessions.gap_close_secs`, 0.4.0 PR4). Default 120 seconds:
    /// the 11-day sweep improved F1, positive F1, and tool accuracy versus 300 seconds
    /// (usage review 2026-08-01 §6.7). Final clamp 60..=3600 seconds.
    pub sessions_gap_close_secs: u32,
    /// Default value of the Recall search "include app chrome / raw text" toggle
    /// (`03 §8` `text.include_chrome_default`). `false` → default search uses
    /// `content_text` only; the per-query `SearchQuery.include_chrome` can still opt in.
    pub text_include_chrome_default: bool,
    /// Ask reasoning toggle; off = faster, shallower answers.
    pub answer_thinking: bool,
    // Models
    pub models_vision_tier: ModelTier,
    pub models_answer_tier: ModelTier,
    // Data
    /// Days to keep the *screenshot* before it degrades to text-only proof. The frame row
    /// and its raw/content text + spans are always kept (they are the durable proof); only
    /// the image file is removed. `0` = keep screenshots forever.
    pub storage_retention_days: u32,
    /// Max stored-image width in px; the capture is downscaled (aspect kept) above it.
    /// `0` = native (no downscale) - keeps ultra-wide captures legible.
    pub storage_max_width: u32,
    // Advanced
    /// Smart enrichment-throttle master switch (`throttle.enabled`, `docs/0.2.0.md`
    /// former PR5, `03 §8`). Opt-in, default `false`: when off the pressure-probe loop
    /// never runs and enrichment drains at full configured concurrency, exactly as
    /// before. When on, sustained CPU/GPU pressure pauses `vision_tag` and floors
    /// `embed_text` concurrency; capture/OCR/storage never throttle (`03 §5`).
    pub throttle_enabled: bool,
    // Integrations (`api.token` / `api.read_token` are credentials, not fields here).
    // These three joined the struct in settings v2 (0.4.5 D6); `#[serde(default)]` keeps a
    // `config_snapshot` written by a pre-v2 build deserializable (`03 §8c`) without making
    // every other field optional.
    /// Master switch; the API crate is not constructed unless on.
    #[serde(default)]
    pub api_enabled: bool,
    /// Port the local API binds; clamp 1024..=65535.
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    /// Canonical configured IPv4/IPv6 literal; missing defaults to `127.0.0.1`.
    #[serde(default = "default_api_bind_address")]
    pub api_bind_address: String,
}

fn default_api_port() -> u16 {
    43_210
}

fn default_api_bind_address() -> String {
    "127.0.0.1".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_interval_ms: 3000,
            capture_monitors: Vec::new(),
            // 0 = capture at the monitor's native width (no downscale) — keeps ultra-wide
            // captures legible; any positive value caps the stored image width.
            storage_max_width: 0,
            // Days to keep the *screenshot* before it degrades to text-only proof (the row
            // + raw/content text + spans always survive). Non-zero by default so storage is
            // bounded out of the box; 0 = keep screenshots forever.
            storage_retention_days: 30,
            // Timed vision enrichment is opt-in (off by default); on-demand is always
            // available. Thresholds chosen with the user (07 gap #1).
            enrich_vision_timer_enabled: false,
            enrich_vision_timer_interval_ms: 3_600_000, // 60 min
            models_vision_tier: ModelTier::Default,
            models_answer_tier: ModelTier::Default,
            answer_thinking: true,
            privacy_excluded_apps: vec![
                "1Password".to_string(),
                "KeePass".to_string(),
                "Bitwarden".to_string(),
            ],
            privacy_pause_on_lock: true,
            // 0.2.x attention-first text signal (03 §8): whether default search includes
            // chrome text. The classifier thresholds themselves are `file-only`.
            text_include_chrome_default: false,
            // 0.2.x retrieval + recall reports (03 §8). default_top_k replaces the
            // hardcoded ASK_TOP_K; daily_top_k is the per-period budget for
            // temporal-coverage reports (03 §8b).
            retrieval_default_top_k: 8,
            reports_daily_top_k: 40,
            // 0.3.0 Flow overlay (docs/0.3.0.md PR5): a configurable non-OS-reserved
            // summon chord. Chord validity/conflicts are shell concerns so registration
            // failure can surface loudly in Settings. Changed from the original
            // "Ctrl+Alt+Space", which collided with Claude Desktop's global quick-entry
            // shortcut. Existing installs still holding the old default are migrated once
            // on load (`kernel::settings::load_overlay_hotkey`).
            overlay_hotkey: "Ctrl+Alt+Z".to_string(),
            // 0.3.0 flow recall (docs/0.3.0.md PR6): the mark-this-moment chord (D6,
            // non-OS-reserved, shell-registered so a conflict surfaces loudly in Settings).
            sessions_min_len_secs: 120,
            sessions_gap_close_secs: 120,
            marks_hotkey: "Ctrl+Alt+M".to_string(),
            // Four-theme design system (0.4.5 P2 pick, 2026-11-04): pine is the shipped
            // default, matching the `:root` token block in `ui/src/styles/tokens.css`.
            ui_appearance: UiAppearance::PineLight,
            // 0.2.1 smart enrichment throttle (docs/0.2.0.md former PR5, 07 #49). Opt-in
            // master OFF: flipping it on backs enrichment off under sustained load. The
            // thresholds/dwells themselves are `file-only`.
            throttle_enabled: false,
            // 0.3.2 app lifecycle (03 §7d, D3): close-to-tray ON by default (closing the
            // window keeps ScreenSearch running with capture live); run-at-startup OFF by
            // default (never silently enable launch-at-login).
            app_close_to_tray: true,
            app_run_at_startup: false,
            // Integrations: API off until enabled; loopback-only by default.
            api_enabled: false,
            api_port: 43_210,
            api_bind_address: "127.0.0.1".to_string(),
        }
    }
}

/// Readiness state of a single subsystem.
///
/// `03 §7` returns a `Readiness` but does not define this enum (07 gap #3). The
/// states below are a closed set chosen so the UI's readiness panel can show a
/// truthful, actionable status for every subsystem without inventing per-screen
/// vocabulary:
/// - `Unknown` — not yet probed (the honest pre-init value).
/// - `Disabled` — intentionally off via settings (e.g. capture stopped, image
///   embeddings disabled, vision in `on_demand` and idle). Not an error.
/// - `Initializing` — coming up (DB migrating, model downloading/loading, sidecar
///   spawning).
/// - `Ready` — operational (or, for the lazily-evicted sidecar, able to serve on
///   demand).
/// - `Unavailable` — a prerequisite is missing (model not downloaded, sidecar
///   binary absent, no capturable monitor). Actionable by the user.
/// - `Error` — a failure occurred; see `detail`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ComponentStatus {
    #[default]
    Unknown,
    Disabled,
    Initializing,
    Ready,
    Unavailable,
    Error,
}

/// Readiness of one subsystem: a [`ComponentStatus`] plus optional human-readable
/// `detail` (e.g. "model downloading 40%", "sidecar evicted (idle)", "WebView2
/// runtime missing") so the UI can explain *why* without a separate lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ComponentReadiness {
    pub status: ComponentStatus,
    pub detail: Option<String>,
}

impl ComponentReadiness {
    /// A status with no extra detail.
    pub fn of(status: ComponentStatus) -> Self {
        Self {
            status,
            detail: None,
        }
    }

    /// A status with a human-readable explanation.
    pub fn with_detail(status: ComponentStatus, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: Some(detail.into()),
        }
    }
}

/// Aggregate readiness of the four subsystems (`get_readiness` output /
/// `readiness_changed` event, `03 §7`). [`Default`] is every component `Unknown`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct Readiness {
    pub capture: ComponentReadiness,
    pub db: ComponentReadiness,
    pub embed_model: ComponentReadiness,
    pub sidecar: ComponentReadiness,
}

/// A streamed chunk of an answer (`answer_delta` event / `AnswerProvider` channel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum AnswerDelta {
    /// A token of the model's *thinking* trace.
    Thinking { text: String },
    /// A token of the final answer.
    Token { text: String },
    /// A grounding citation to a source frame.
    Citation {
        #[ts(type = "number")]
        frame_id: i64,
    },
    /// The answer is complete.
    Done,
    /// The answer failed.
    Error { message: String },
}

/// Emitted once per stored capture (`capture_tick` event).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct CaptureTick {
    #[ts(type = "number")]
    pub frame_id: i64,
    #[ts(type = "number")]
    pub captured_at: i64,
    pub monitor_index: u32,
}

/// Job-queue progress snapshot (`job_progress` event).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct JobProgress {
    pub stats: JobStats,
}

/// Data-changing enrichment job completion (`job_completed` event). Carries enough
/// identity for the UI to refresh frame/search/insights data surgically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct JobCompleted {
    #[ts(type = "number")]
    pub job_id: i64,
    pub kind: JobKind,
    #[ts(type = "number")]
    pub frame_id: i64,
    pub stats: JobStats,
}

/// Lifecycle state of the inference sidecar (`03 §6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum SidecarState {
    Stopped,
    Starting,
    Ready,
    Evicted,
    Crashed,
    Recycled,
}

/// Sidecar status update (`sidecar_status` event). `lane` says which model is (or was
/// last) resident — vision vs. answer — so the UI can label the engine truthfully instead
/// of guessing from the filename. `None` when no model has loaded yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SidecarStatus {
    pub state: SidecarState,
    pub model: Option<String>,
    pub lane: Option<ModelLane>,
}

/// Phase of a model download (`model_download` event). Drives the progress UI so a
/// multi-GB model fetch communicates progress + completion/error instead of just opaque
/// network activity (`03 §6/§7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ModelDownloadPhase {
    Downloading,
    Done,
    Failed,
}

/// Progress of a model download for one lane. `total_bytes` is `None` when the size could
/// not be probed (the UI then shows bytes-downloaded without a percentage). `error` is set
/// only on the `Failed` phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ModelDownloadStatus {
    pub lane: ModelLane,
    pub model: Option<String>,
    pub phase: ModelDownloadPhase,
    // `#[ts(type = "number")]`: Tauri's JSON wire delivers 64-bit ints as JS numbers, and
    // byte counts stay well under 2^53 — same convention as `JobStats`.
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number | null")]
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
}

/// One independent startup/acquisition operation (`03 §7g`). Declaration order is the
/// order `SetupStatus::steps` is kept in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum SetupOperation {
    /// llama-server acquisition (`ensure_binary`).
    Engine,
    /// fastembed search-model initialization.
    Embeddings,
    /// Vision-lane GGUF download (mirrored from `model_download`).
    VisionModel,
    /// Answer-lane GGUF download (mirrored from `model_download`).
    AnswerModel,
}

/// Stage of one setup operation (`03 §7g`). `Done` means the operation completed — never
/// that a model is loaded; readiness stays authoritative for that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum SetupPhase {
    Checking,
    Downloading,
    Verifying,
    Installing,
    Preparing,
    Done,
    Failed,
}

/// Current state of one setup operation. Bytes are real received bytes of the current
/// attempt only; `total_bytes` is `None` unless the server reported a length. `error` is
/// set only on `Failed`, with URL query strings removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SetupStep {
    pub operation: SetupOperation,
    pub phase: SetupPhase,
    /// 1-based download attempt (engine); `1` for every other operation.
    pub attempt: u32,
    #[ts(type = "number | null")]
    pub downloaded_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
}

/// Authoritative current-session setup snapshot (`get_setup_status` output and the
/// whole-state `setup_changed` event payload, `03 §7g`). `revision` increases by one on
/// every change so the UI can discard an older snapshot that arrives late.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SetupStatus {
    #[ts(type = "number")]
    pub revision: u64,
    pub steps: Vec<SetupStep>,
}

/// Severity of a UI toast (`toast` event).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A transient user-facing notification (`toast` event).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct Toast {
    pub level: ToastLevel,
    pub message: String,
}

/// Registration state for a global hotkey managed by the shell (`overlay.hotkey`,
/// 0.3.0 PR5). `registered=false` is a first-class UI state so conflicts surface
/// loudly in Settings instead of silently disabling the shortcut (D6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct HotkeyStatus {
    pub id: String,
    pub chord: String,
    pub registered: bool,
    pub error: Option<String>,
}

/// Event payload telling the main window to open a captured Moment after the overlay
/// accepts a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct OpenMoment {
    #[ts(type = "number")]
    pub frame_id: i64,
}

/// The last sustained work context before the current detour (`where_was_i` output,
/// 0.3.0 PR6; `03 §7b`, D9). `None` from the command means "nothing to resume yet".
/// The pure heuristic (`kernel::resume`) picks the most recent run of frames sharing a
/// context key that persisted ≥ `resume.min_dwell_secs`, excluding the current
/// (anchor) context, ScreenSearch itself, and `privacy.excluded_apps`. `frame_id` is
/// the run's representative (last) frame — `Enter`/click opens its Moment. `image_path`
/// and `image_purged` let the overlay/Deck render a thumbnail without a second
/// `get_frame`. `browser_url` is dormant today (always `None` in production capture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ResumeContext {
    #[ts(type = "number")]
    pub frame_id: i64,
    /// The representative frame's `app_hint`, original casing (the context's app).
    pub app: String,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    /// First capture of the sustained run, unix epoch ms.
    #[ts(type = "number")]
    pub span_start: i64,
    /// Last capture of the sustained run (the representative frame), unix epoch ms.
    #[ts(type = "number")]
    pub span_end: i64,
    pub image_path: String,
    pub image_purged: bool,
}

/// Tauri-only where-was-i response. HTTP/MCP retain the shipped `ResumeContext`
/// object; the in-app command receives the same flattened fields plus session context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct UiResumeContext {
    #[serde(flatten)]
    #[ts(flatten)]
    pub context: ResumeContext,
    pub session: Option<SessionReference>,
}

/// A mark — a user-flagged moment with an optional intention note (`list_marks`
/// output, 0.3.0 PR6; `03 §4`/`§7b`). Carries the joined frame fields the Intentions
/// strip renders (thumbnail + context) so listing marks never triggers an N+1
/// `get_frame` per row; the `frames` INNER JOIN is total because `marks.frame_id`
/// CASCADEs with the frame. A retention-degraded frame keeps its row (image_purged =
/// true, "text kept"), so a mark survives image expiry (`03 §4`, D10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct Mark {
    #[ts(type = "number")]
    pub mark_id: i64,
    #[ts(type = "number")]
    pub frame_id: i64,
    /// When the mark was created, unix epoch ms.
    #[ts(type = "number")]
    pub created_at: i64,
    /// Optional one-line intention note (attached via `set_mark_note` after the fact).
    pub note: Option<String>,
    /// `null` while unresolved; set when the user resolves or dismisses the mark.
    #[ts(type = "number | null")]
    pub resolved_at: Option<i64>,
    /// The marked frame's capture time, unix epoch ms (joined for display "age").
    #[ts(type = "number")]
    pub captured_at: i64,
    pub image_path: String,
    pub image_purged: bool,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
}

/// Payload of the overlay `mark_toast` event (0.3.0 PR6; `03 §7b`). Tells the overlay
/// window to show the quiet, non-focus-stealing "Marked ✓ — note?" confirmation (or a
/// warning on failure). `mark_id` is `Some` on success (so the optional note can be
/// attached via `set_mark_note`), `None` on the failure variant (e.g. capture off, or a
/// privacy-gate denial), where only the `message` is shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MarkToast {
    #[ts(type = "number | null")]
    pub mark_id: Option<i64>,
    pub level: ToastLevel,
    pub message: String,
}

/// A point-in-time system-pressure reading from the `sysmon` probe (`03 §8`). The
/// enrichment throttle (`03 §5`) consumes this to decide whether to back enrichment off
/// under sustained load. `gpu_pct` is `None` and `gpu_monitored` is `false` when Windows
/// exposes no GPU performance counters — a truthful "GPU not monitored" state, not an
/// error; the throttle then runs on CPU pressure alone (a weak-iGPU / VM machine).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct PressureSample {
    /// Whole-machine CPU busy %, `0..=100` (not per-process — see `03 §5`).
    pub cpu_pct: f32,
    /// Summed GPU-engine utilization %, `0..=100`; `None` when GPU is unmonitored.
    pub gpu_pct: Option<f32>,
    /// Whether GPU monitoring is live; `false` → the UI shows "GPU not monitored".
    pub gpu_monitored: bool,
    /// When this sample was taken, unix epoch ms (the kernel clock unit, `03 §4`).
    #[ts(type = "number")]
    pub sampled_at: i64,
}

/// Current state of the enrichment throttle (`get_throttle_status` command + the
/// `throttle_changed` event, `03 §7`). `level` is `0` Normal / `1` High (heavy
/// enrichment paused) / `2` Sustained (text-embed concurrency floored). `sample` is
/// `None` until the first probe reading lands; `gpu_monitored` mirrors the probe so the
/// UI can show an honest "GPU not monitored" state even before a sample exists.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ThrottleStatus {
    pub enabled: bool,
    pub level: u8,
    pub sample: Option<PressureSample>,
    pub gpu_monitored: bool,
}

/// State of the opt-in local HTTP API for the Settings panel (0.3.0 PR7; `03 §7c`,
/// D11) — the return of `get_api_status` / `set_api_config` / `regenerate_api_token`.
/// `enabled` is the persisted intent; `running` is whether the socket is actually
/// bound. The two diverge in exactly one case: `enabled && !running && last_error` is
/// the **loud guided-change** state — the port was in use on enable, so the server did
/// not start, the intent is preserved, and the Settings panel shows the warning + a
/// "pick another port" retry (mirrors the D6 hotkey-conflict pattern). `token` carries
/// the bearer token so Settings can reveal/copy it (`§7c`: "shown/copyable"); it is
/// `None` only before the first enable ever generated one. The token lives in the
/// `settings` KV table, **never** in the `Settings` struct, so it cannot leak through
/// `get_settings`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ApiStatus {
    /// Configured IPv4 or IPv6 literal; missing configuration defaults to 127.0.0.1.
    pub bind_address: String,
    /// Persisted intent — the master toggle.
    pub enabled: bool,
    /// Whether the HTTP server is currently bound and serving.
    pub running: bool,
    /// The configured port (default 43210).
    pub port: u16,
    /// The bearer token (revealed/copied in Settings); `None` before first enable.
    pub token: Option<String>,
    /// Why the server is not running despite `enabled` (e.g. "port 43210 in use").
    pub last_error: Option<String>,
}

/// Manual `Debug` that never prints the bearer token bytes — a future
/// `tracing!(?status)` or `{:?}` on `ApiStatus` must not leak the secret (the same
/// discipline `ApiReadTokenSecret` keeps by omitting `Debug`). Presence is shown as a
/// redaction marker so diagnostics still distinguish `None` from `Some`; the derived
/// `Serialize`/`TS` impls (the UI contract) are unchanged.
impl std::fmt::Debug for ApiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiStatus")
            .field("bind_address", &self.bind_address)
            .field("enabled", &self.enabled)
            .field("running", &self.running)
            .field("port", &self.port)
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            .field("last_error", &self.last_error)
            .finish()
    }
}

/// Passive, nonsecret availability of the independent read credential (`03 §7f.3`).
/// Only explicit local reveal/copy actions return credential bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ApiReadTokenStatus {
    pub configured: bool,
}

/// Deliberately secret-bearing local IPC response; never a settings/event/report field.
/// No Debug implementation: accidental diagnostic formatting must not expose the token.
#[derive(Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ApiReadTokenSecret {
    pub token: String,
}

/// Auto-update lifecycle (0.3.2 PR2, #69; `03 §11b`). Output of `get_update_status` /
/// `check_for_updates` and the payload of the `update_status_changed` event. The updater
/// is **pull-based and quiet** (D1): `Idle` means no update is known — the UI renders
/// **zero** updater presence (not a "no update" badge). `Available` / `Downloading` /
/// `Ready` are the only states with any UI surface (a quiet NavRail dot + an App-section
/// line); `Ready` installs **only** on a user-initiated restart. `Error` shows a quiet
/// App-section line + retry, never a modal. Deliberately carries no byte-progress — the
/// contract asks for presence, not a meter, and keeps the event stream sparse.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum UpdateStatus {
    /// No update known: never checked, the last check found nothing newer, or a dev build.
    #[default]
    Idle,
    /// A check is in flight.
    Checking,
    /// A newer signed release was found; the background download is starting.
    Available { version: String },
    /// The verified release is downloading in the background.
    Downloading { version: String },
    /// Downloaded and signature-verified; installs on the next user-initiated restart.
    Ready { version: String },
    /// The check or download failed (includes signature rejection). Quiet App-section
    /// line + retry — never a modal, never a toast.
    Error { message: String },
}

/// Input to the `export_data` command (0.3.0 PR7; `03 §7c`, D12). Bounds the export
/// to an optional half-open `[from, to)` window (unix epoch ms), matching the
/// `GET /v1/export?from=&to=` query. Both `None` (the Settings "Export…" button) =
/// full history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ExportRequest {
    #[ts(type = "number | null")]
    pub from: Option<i64>,
    #[ts(type = "number | null")]
    pub to: Option<i64>,
}

/// Result of the `export_data` command (0.3.0 PR7; `03 §7c`, D12) — the streamed JSON
/// file the Settings "Export…" button produced. `path` is the absolute file path (in
/// the user's Downloads folder); `frames`/`marks` are the counts written; `bytes` is
/// the file size. The same streaming code path backs `GET /v1/export`, so the export
/// works even with the API disabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ExportResult {
    pub path: String,
    pub frames: u32,
    pub marks: u32,
    #[ts(type = "number")]
    pub bytes: u64,
}

#[cfg(test)]
mod ts_number_guard {
    use super::*;
    use crate::jobs::JobStats;
    use ts_rs::TS;

    /// Every i64/u64 IPC field must export as TS `number`, not `bigint` — Tauri's
    /// JSON wire delivers 64-bit ints as JS numbers. This guards the
    /// `#[ts(type = "number")]` convention against regressions (deterministic, no
    /// file IO). When adding a 64-bit field to an IPC type, list that type here.
    #[test]
    fn no_bigint_in_ipc_types() {
        let decls = [
            ("TimeRange", TimeRange::inline()),
            ("SearchHit", SearchHit::inline()),
            ("FrameMeta", FrameMeta::inline()),
            ("SessionQuery", SessionQuery::inline()),
            ("SessionReference", SessionReference::inline()),
            ("SessionDetail", SessionDetail::inline()),
            ("SessionRecapRequest", SessionRecapRequest::inline()),
            ("TimelineBucket", TimelineBucket::inline()),
            ("InsightsSummary", InsightsSummary::inline()),
            ("StorageStats", StorageStats::inline()),
            ("AppSuppression", AppSuppression::inline()),
            ("FrameDetail", FrameDetail::inline()),
            ("UiFrameDetail", UiFrameDetail::inline()),
            ("VisionTarget", VisionTarget::inline()),
            ("CaptureTick", CaptureTick::inline()),
            ("AnswerEvent", AnswerEvent::inline()),
            ("AnswerDelta", AnswerDelta::inline()),
            ("JobCompleted", JobCompleted::inline()),
            ("JobStats", JobStats::inline()),
            ("ReportRequest", ReportRequest::inline()),
            ("ReportResponse", ReportResponse::inline()),
            ("OpenMoment", OpenMoment::inline()),
            ("ResumeContext", ResumeContext::inline()),
            ("UiResumeContext", UiResumeContext::inline()),
            ("Mark", Mark::inline()),
            ("MarkToast", MarkToast::inline()),
            ("PressureSample", PressureSample::inline()),
            ("ThrottleStatus", ThrottleStatus::inline()),
            ("ApiStatus", ApiStatus::inline()),
            ("ExportRequest", ExportRequest::inline()),
            ("ExportResult", ExportResult::inline()),
        ];
        for (name, decl) in decls {
            assert!(
                !decl.contains("bigint"),
                "{name} exports a `bigint` field — add #[ts(type = \"number\")]: {decl}"
            );
        }
    }
}

#[cfg(test)]
mod api_status_redaction {
    use super::ApiStatus;
    use ts_rs::TS;

    /// A distinctive secret so a leak is unambiguous even by prefix.
    const CANARY: &str = "canary-legacy-bearer-3f9a1c0d7e2b4856";

    fn status_with(token: Option<&str>) -> ApiStatus {
        ApiStatus {
            bind_address: "127.0.0.1".to_string(),
            enabled: true,
            running: true,
            port: 43210,
            token: token.map(str::to_string),
            last_error: None,
        }
    }

    /// Guard-rail: `ApiStatus` carries the legacy bearer token, so diagnostic
    /// formatting must never print its bytes. `ApiReadTokenSecret` already omits
    /// `Debug`; this pins the same discipline for `ApiStatus`.
    #[test]
    fn debug_never_exposes_the_bearer_token() {
        let debug = format!("{:?}", status_with(Some(CANARY)));
        assert!(
            !debug.contains(CANARY),
            "Debug leaked the bearer token: {debug}"
        );
        assert!(
            !debug.contains("3f9a1c0d"),
            "Debug leaked token bytes: {debug}"
        );
        assert!(
            debug.contains("[redacted]"),
            "Debug should mark the redaction: {debug}"
        );
        assert!(
            debug.contains("Some"),
            "Debug should still show credential presence: {debug}"
        );
        // A `None` credential formats as absent, never as a fabricated secret.
        let absent = format!("{:?}", status_with(None));
        assert!(absent.contains("token: None"), "{absent}");
    }

    /// The redaction must not change the serialization/UI contract: `ApiStatus` is still
    /// `Serialize` (so `token` rides the JSON wire for Settings reveal/copy) and the
    /// generated binding still declares the token field. The repo's binding guard
    /// (`git diff --exit-code -- ui/src/bindings`) pins the exact exported bytes.
    #[test]
    fn serialization_and_binding_contract_are_unchanged() {
        fn assert_serialize<T: serde::Serialize>() {}
        assert_serialize::<ApiStatus>();

        let decl = ApiStatus::decl();
        assert!(
            decl.contains("token: string | null"),
            "binding lost the token field: {decl}"
        );
        for field in ["bind_address", "enabled", "running", "port", "last_error"] {
            assert!(decl.contains(field), "binding lost {field}: {decl}");
        }
    }
}
