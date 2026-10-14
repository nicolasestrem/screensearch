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

/// The llama.cpp launch knobs derived from [`Settings`], threaded to both inference
/// providers and on into the sidecar's argument list. Bundled into one struct so adding
/// a knob does not ripple through every provider/`resolve_spec` signature. Not an IPC
/// type (constructed in the core from [`Settings`]), so it carries no `ts-rs` derive.
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

impl From<&Settings> for SidecarParams {
    fn from(s: &Settings) -> Self {
        Self {
            ngl: s.sidecar_ngl,
            device: s.sidecar_device.clone(),
            ctx_size: s.sidecar_ctx_size,
            kv_cache_type: s.sidecar_kv_cache_type,
            flash_attn: s.sidecar_flash_attn,
        }
    }
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

/// User-facing settings (`get_settings`/`set_settings`). Field defaults mirror
/// `03 §8`; persisted as key/value rows in the `settings` table.
///
/// Deferred vision tagging is never real-time (`03 §5`). On-demand tagging
/// (UI-triggered) is always available; **timed** and **idle** enrichment are each
/// independent opt-in toggles, off by default, with a user-set threshold. (This
/// replaces `03 §8`'s single `enrich.vision_mode` enum — see specs/06_PATCH_PLAN.)
///
/// `sidecar_device` is the optional llama.cpp `--device` selector (for example,
/// `Vulkan0`); `None` lets llama.cpp choose its default device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct Settings {
    pub capture_interval_ms: u32,
    /// Empty = all monitors.
    pub capture_monitors: Vec<u32>,
    pub capture_diff_threshold: f32,
    /// Max stored-image width in px; the capture is downscaled (aspect kept) above it.
    /// `0` = native (no downscale) — keeps ultra-wide captures legible.
    pub storage_max_width: u32,
    /// Days to keep the *screenshot* before it degrades to text-only proof. The frame row
    /// and its raw/content text + spans are always kept (they are the durable proof); only
    /// the image file is removed. `0` = keep screenshots forever.
    pub storage_retention_days: u32,
    pub enrich_embed_text: bool,
    /// Opt-in: tag up to a batch of untagged frames every `vision_timer_interval_ms`.
    pub enrich_vision_timer_enabled: bool,
    pub enrich_vision_timer_interval_ms: u32,
    /// Opt-in: tag while the user has been idle for at least `vision_idle_secs`.
    pub enrich_vision_idle_enabled: bool,
    pub enrich_vision_idle_secs: u32,
    /// Max still-untagged frames a timer/idle tick enqueues per run (the scheduler
    /// batch size). Already-queued frames are skipped, so this caps fresh work per run.
    pub enrich_vision_batch_size: u32,
    pub enrich_worker_concurrency: u32,
    pub models_vision_tier: ModelTier,
    pub models_answer_tier: ModelTier,
    pub answer_thinking: bool,
    pub sidecar_idle_ttl_secs: u32,
    pub sidecar_ngl: u32,
    pub sidecar_device: Option<String>,
    /// Sidecar context window in tokens (`--ctx-size`). `0` = automatic: a small
    /// per-lane default (vision 4096, answer 8192). A non-zero value overrides both
    /// lanes. Lower = less VRAM (smaller KV cache); too low can truncate long answers.
    pub sidecar_ctx_size: u32,
    /// KV-cache precision (`--cache-type-k`/`--cache-type-v`). A quantized cache uses
    /// less VRAM and is applied only when flash attention is active.
    pub sidecar_kv_cache_type: KvCacheType,
    /// Flash-attention mode (`--flash-attn`). Reduces attention memory and unlocks KV
    /// quantization; `Auto` enables it when the bundled binary supports it.
    pub sidecar_flash_attn: FlashAttnSetting,
    /// Recycle (restart) the sidecar when its committed host RAM crosses the ceiling,
    /// reclaiming the upstream llama.cpp multimodal leak that otherwise grows the vision
    /// sidecar ~150 MB per frame during long tagging. On by default.
    pub sidecar_recycle_enabled: bool,
    /// Committed-RAM ceiling in MiB that triggers a sidecar recycle. `0` = automatic
    /// (derived from total system RAM). Ignored when `sidecar_recycle_enabled` is false.
    pub sidecar_recycle_rss_mb: u32,
    pub privacy_excluded_apps: Vec<String>,
    pub privacy_pause_on_lock: bool,
    /// Default value of the Recall search "include app chrome / raw text" toggle
    /// (`03 §8` `text.include_chrome_default`). `false` → default search uses
    /// `content_text` only; the per-query `SearchQuery.include_chrome` can still opt in.
    pub text_include_chrome_default: bool,
    /// Appearances of a span signature before it is marked static chrome and dropped
    /// from `content_text` (`03 §8` `text.chrome_suppress_min_seen`). A threshold, never
    /// hardcoded (`03 §3b`).
    pub text_chrome_suppress_min_seen: u32,
    /// Lines at least this many characters are never suppressed for merely repeating
    /// (`03 §8` `text.chrome_protect_min_chars`) — protects long, information-rich text.
    pub text_chrome_protect_min_chars: u32,
    /// Grid resolution for a span's `region_bucket` in the chrome signature
    /// (`03 §8` `text.chrome_region_buckets`); an N×N grid over the normalized frame.
    pub text_chrome_region_buckets: u32,
    /// Default Ask retrieval depth (`03 §8` `retrieval.default_top_k`), replacing the
    /// former hardcoded `ASK_TOP_K`. The per-request `AskRequest.top_k` overrides it.
    pub retrieval_default_top_k: u32,
    /// Recall-report target sampled frames **per active period** (`03 §8`
    /// `reports.daily_top_k`). Report depth scales as this × active periods (`§8b`).
    pub reports_daily_top_k: u32,
    /// Recall-report **global** cap on frames summarized across all periods
    /// (`03 §8` `reports.weekly_top_k`); bounds the sidecar pass count on weak HW.
    pub reports_weekly_top_k: u32,
    /// Frame count at/below which a report uses a single pass; above it, map-reduce
    /// (`03 §8` `reports.map_reduce_min_frames`, `§8b`).
    pub reports_map_reduce_min_frames: u32,
    /// Event-driven capture master switch (`capture.event_driven_enabled`,
    /// `docs/0.2.0.md`). Opt-in, default `false`: capture stays the 0.2.0 timer/idle
    /// cadence and no input hooks are installed unless this is on.
    pub capture_event_driven_enabled: bool,
    /// Capture on foreground/app switch when event-driven capture is on
    /// (`capture.event_on_foreground`). *(0.3.0 PR2 trimmed the six event triggers to
    /// foreground + idle — `docs/0.3.0.md`.)*
    pub capture_event_on_foreground: bool,
    /// Capture when the user goes idle past the threshold (`capture.event_on_idle`).
    pub capture_event_on_idle: bool,
    /// Collapse a burst of triggers within this window into one capture, ms
    /// (`capture.event_debounce_ms`). A threshold, never hardcoded.
    pub capture_event_debounce_ms: u32,
    /// Minimum gap between any two event-driven captures, ms — the rate ceiling
    /// (`capture.event_min_interval_ms`).
    pub capture_event_min_interval_ms: u32,
    /// Idle time that counts as "gone idle", ms (`capture.event_idle_threshold_ms`).
    pub capture_event_idle_threshold_ms: u32,
    /// Fallback capture interval in event mode, ms — a static screen is still sampled
    /// at least this often (`capture.event_fallback_interval_ms`).
    pub capture_event_fallback_interval_ms: u32,
    /// Use Windows UI Automation for the target window's text, with OCR fallback
    /// (`capture.uia_text_enabled`, `docs/0.2.0.md` #48). Default ON: UIA yields more
    /// structured text than OCR; on any failure/timeout/thin-yield the frame falls back to
    /// OCR. Hot-applies per frame (no capture restart).
    pub capture_uia_text_enabled: bool,
    /// Per-frame UIA latency budget, ms (`capture.uia_latency_budget_ms`). The tree walk
    /// abandons past this and a 2× hard timeout guards a wedged worker; over budget → OCR
    /// fallback. A threshold, never hardcoded. Baked into the provider at startup — applied
    /// on app restart (a capture stop/start reuses the existing provider).
    pub capture_uia_latency_budget_ms: u32,
    /// Minimum UIA text length, chars, below which the read is a thin yield → OCR fallback
    /// (`capture.uia_min_text_chars`). Catches GPU/canvas/custom-drawn windows where OCR is
    /// strictly better. Baked into the provider at startup — applied on app restart (a
    /// capture stop/start reuses the existing provider).
    pub capture_uia_min_text_chars: u32,
    /// Walk the UIA **control view** rather than the raw view
    /// (`capture.uia_view_control_only`, `07` #71). Default **ON**: control view collapses a
    /// Chromium page's per-text-run node explosion to the elements that carry text, slashing
    /// cross-process calls. Off = raw view (legacy; far heavier on browsers). Baked at startup.
    pub capture_uia_view_control_only: bool,
    /// Hard cap on accessibility nodes visited per UIA walk (`capture.uia_max_nodes`, `07`
    /// #71; replaces the former hardcoded constant). Bounds the walk on a pathological tree.
    /// A threshold, never hardcoded (`03 §3b`). Baked into the provider at startup.
    pub capture_uia_max_nodes: u32,
    /// Max live `TextPattern` visible-range reads per UIA walk
    /// (`capture.uia_max_textpattern_calls`, `07` #71). TextPattern ranges are the one
    /// uncacheable cross-process cost; bounding them stops a document-heavy page from
    /// reopening the call storm. A threshold, never hardcoded. Baked into the provider.
    pub capture_uia_max_textpattern_calls: u32,
    /// Skip the UIA walk for periodic `Timer` frames captured within this many ms of the
    /// last keyboard/mouse input, falling back to OCR (`capture.uia_suppress_during_input_ms`,
    /// `07` #71). Closes the residual freeze gap the scroll/click trigger gate leaves in the
    /// default timer-only capture path, where every frame is a `Timer` and a tick can land
    /// mid-scroll on a heavy Chromium/Electron tree. `0` disables the gate — it always
    /// applies otherwise (0.3.2 PR5 retired the former `capture.uia_run_on_interactive`
    /// bypass with the knob, `07` #83). A threshold, never hardcoded. Baked into the
    /// provider at startup — applied on app restart.
    pub capture_uia_suppress_during_input_ms: u32,
    /// Global Flow overlay summon hotkey (`overlay.hotkey`, 0.3.0 PR5). The shell
    /// validates and registers the chord so a bad/colliding value becomes the D6
    /// Settings warning instead of being silently rewritten here.
    pub overlay_hotkey: String,
    /// Top-N results in the Flow overlay (`overlay.max_results`, 0.3.0 PR5).
    pub overlay_max_results: u32,
    /// Minimum dwell for the where-was-i heuristic, seconds (`resume.min_dwell_secs`,
    /// 0.3.0 PR6; `03 §7b`, D9). A context must persist at least this long to count as a
    /// "sustained context" worth resuming, and the same threshold decides whether a
    /// brief excursion breaks a run. One knob, never hardcoded (`03 §3b` stance).
    pub resume_min_dwell_secs: u32,
    /// Minimum emitted session length (`sessions.min_len_secs`, 0.4.0 PR4). Final clamp
    /// 30..=3600 seconds; this is one of the arc's exactly two typed settings keys.
    pub sessions_min_len_secs: u32,
    /// Pass-1 micro gap close (`sessions.gap_close_secs`, 0.4.0 PR4). Default 120 seconds:
    /// the 11-day sweep improved F1, positive F1, and tool accuracy versus 300 seconds
    /// (usage review 2026-08-01 §6.7). Final clamp 60..=3600 seconds.
    pub sessions_gap_close_secs: u32,
    /// Global mark-this-moment hotkey (`marks.hotkey`, 0.3.0 PR6; `03 §7b`, D6). Same
    /// shell-registered/validated posture as `overlay_hotkey`: a bad/colliding value
    /// surfaces as the D6 Settings warning + toast, never a silent rewrite.
    pub marks_hotkey: String,
    /// Smart enrichment-throttle master switch (`throttle.enabled`, `docs/0.2.0.md`
    /// former PR5, `03 §8`). Opt-in, default `false`: when off the pressure-probe loop
    /// never runs and enrichment drains at full configured concurrency, exactly as
    /// before. When on, sustained CPU/GPU pressure pauses `vision_tag` and floors
    /// `embed_text` concurrency; capture/OCR/storage never throttle (`03 §5`).
    pub throttle_enabled: bool,
    /// CPU busy % at/above which pressure counts toward raising a throttle level
    /// (`throttle.cpu_enter_pct`). A threshold, never hardcoded (`03 §3b` stance).
    pub throttle_cpu_enter_pct: f32,
    /// CPU busy % below which pressure counts toward lowering a level
    /// (`throttle.cpu_exit_pct`). Kept strictly below the enter % for hysteresis.
    pub throttle_cpu_exit_pct: f32,
    /// GPU utilization % enter threshold (`throttle.gpu_enter_pct`). Ignored when the GPU
    /// is unmonitored (no Windows GPU perf counters) — the throttle is then CPU-only.
    pub throttle_gpu_enter_pct: f32,
    /// GPU utilization % exit threshold (`throttle.gpu_exit_pct`); strictly below enter.
    pub throttle_gpu_exit_pct: f32,
    /// How long pressure must stay above the enter threshold before stepping up one
    /// throttle level, ms (`throttle.enter_after_ms`) — the sustained-enter dwell.
    pub throttle_enter_after_ms: u32,
    /// How long pressure must stay below the exit threshold before stepping down one
    /// level, ms (`throttle.exit_after_ms`) — the recovered-exit dwell (longer = stickier).
    pub throttle_exit_after_ms: u32,
    /// Pressure sampling cadence, ms (`throttle.sample_interval_ms`). The floor keeps
    /// sampling cheap.
    pub throttle_sample_interval_ms: u32,
    /// Minimum concurrent `embed_text` jobs at the Sustained level
    /// (`throttle.embed_text_floor`). Clamped ≥ 1 so text indexing never fully stalls.
    /// Hot-applied by the governor each sample tick, like the other `throttle.*` knobs —
    /// no worker-pool restart.
    pub throttle_embed_text_floor: u32,
    /// Closing the main window hides it to the tray and keeps capturing
    /// (`app.close_to_tray`, 0.3.2 PR3; `03 §7d`, D3). Default **on**. A one-time toast
    /// explains it on the first restore from the tray; with this off, window close quits
    /// cleanly (the sidecar is torn down via the Job-Object lifecycle, `03 §6`).
    pub app_close_to_tray: bool,
    /// Register ScreenSearch to launch at login (`app.run_at_startup`, 0.3.2 PR3;
    /// `03 §7d`, D3). Default **off** — an explicit user choice, never silently enabled.
    /// Applied register-before-persist: the OS registration is attempted before the
    /// setting is saved, so a save can't claim a launch-at-login that didn't take.
    pub app_run_at_startup: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_interval_ms: 3000,
            capture_monitors: Vec::new(),
            capture_diff_threshold: 0.006,
            // 0 = capture at the monitor's native width (no downscale) — keeps ultra-wide
            // captures legible; any positive value caps the stored image width.
            storage_max_width: 0,
            // Days to keep the *screenshot* before it degrades to text-only proof (the row
            // + raw/content text + spans always survive). Non-zero by default so storage is
            // bounded out of the box; 0 = keep screenshots forever.
            storage_retention_days: 30,
            enrich_embed_text: true,
            // Timed/idle vision enrichment are opt-in (off by default); on-demand is
            // always available. Thresholds chosen with the user (07 gap #1), used only
            // when the matching toggle is enabled. All user-adjustable in settings.
            enrich_vision_timer_enabled: false,
            enrich_vision_timer_interval_ms: 3_600_000, // 60 min
            enrich_vision_idle_enabled: false,
            enrich_vision_idle_secs: 300, // 5 min
            enrich_vision_batch_size: 20, // frames per scheduled tick
            enrich_worker_concurrency: 2,
            models_vision_tier: ModelTier::Default,
            models_answer_tier: ModelTier::Default,
            answer_thinking: true,
            sidecar_idle_ttl_secs: 180,
            sidecar_ngl: 99,
            sidecar_device: None,
            // Memory-tuning defaults (balanced): pin a small per-lane context (0 =
            // auto → vision 4096 / answer 8192), quantize the KV cache to q8_0, and let
            // flash attention turn on where the bundled llama.cpp build supports it.
            // Together these cut VRAM well below the uncontrolled-context default with
            // no expected quality loss; the f16 / larger-context escape hatches let a
            // user trade memory back for quality.
            sidecar_ctx_size: 0,
            sidecar_kv_cache_type: KvCacheType::Q8_0,
            sidecar_flash_attn: FlashAttnSetting::Auto,
            sidecar_recycle_enabled: true,
            sidecar_recycle_rss_mb: 0,
            privacy_excluded_apps: vec![
                "1Password".to_string(),
                "KeePass".to_string(),
                "Bitwarden".to_string(),
            ],
            privacy_pause_on_lock: true,
            // 0.2.x attention-first text signal (03 §8). Thresholds are settings, never
            // hardcoded (03 §3b). suppress_min_seen=4 (lowered from 12 after the PR3 audit,
            // docs/AUDIT_0.2.0_PR3_2026-06-26.md): a repeated short edge label is caught
            // within a few captures instead of leaking through a long cold-start window;
            // long lines, interior body, and rect-less frames stay protected, and the
            // filter_version backfill + include_chrome/raw text recover any false positive.
            text_include_chrome_default: false,
            text_chrome_suppress_min_seen: 4,
            text_chrome_protect_min_chars: 48,
            text_chrome_region_buckets: 8,
            // 0.2.x retrieval + recall reports (03 §8). default_top_k replaces the
            // hardcoded ASK_TOP_K; daily/weekly_top_k are the per-period budget and
            // the global cap for temporal-coverage reports (03 §8b).
            retrieval_default_top_k: 8,
            reports_daily_top_k: 40,
            reports_weekly_top_k: 200,
            reports_map_reduce_min_frames: 20,
            // 0.2.1 event-driven capture (docs/0.2.0.md, 07 #47), trimmed by 0.3.0 PR2
            // (docs/0.3.0.md) to foreground + idle. Opt-in master OFF: flipping it on
            // gives a sane out-of-box set (foreground on; idle off as the noisier
            // trigger), 500 ms debounce, a 1 s rate ceiling, and a 30 s fallback so a
            // static screen is still sampled. Every threshold is a setting, never
            // hardcoded (mirrors the PR3 stance).
            capture_event_driven_enabled: false,
            capture_event_on_foreground: true,
            capture_event_on_idle: false,
            capture_event_debounce_ms: 500,
            capture_event_min_interval_ms: 1000,
            capture_event_idle_threshold_ms: 5000,
            capture_event_fallback_interval_ms: 30_000,
            // UIA text (docs/0.2.0.md #48): default ON with OCR fallback. 150 ms keeps the
            // walk well under the capture cadence; 16 chars is the thin-yield floor below
            // which OCR is preferred. Thresholds, never hardcoded (mirrors the PR3 stance).
            capture_uia_text_enabled: true,
            capture_uia_latency_budget_ms: 150,
            capture_uia_min_text_chars: 16,
            // 0.2.1 UIA hang fix (`07` #71): use the lighter control view and bound nodes +
            // live TextPattern reads. These are settings, never hardcoded (PR3 stance);
            // max_nodes/textpattern replace the former worker.rs consts. (0.3.2 PR5 retired
            // the `run_on_interactive` knob — inert since the 0.3.0 trigger trim, `07` #83.)
            capture_uia_view_control_only: true,
            capture_uia_max_nodes: 4000,
            capture_uia_max_textpattern_calls: 64,
            // 500 ms: long enough to span the gaps between wheel/scroll events so an active
            // scroll keeps UIA off the target app, short enough that a paused user's next
            // timer tick resumes UIA promptly. 0 disables. (`07` #71 residual-gap fix.)
            capture_uia_suppress_during_input_ms: 500,
            // 0.3.0 Flow overlay (docs/0.3.0.md PR5): a configurable non-OS-reserved
            // summon chord plus the top-N result cap. Chord validity/conflicts are
            // shell concerns so registration failure can surface loudly in Settings.
            // Changed from the original "Ctrl+Alt+Space", which collided with Claude Desktop's
            // global quick-entry shortcut. Existing installs still holding the old default are
            // migrated once on load (`kernel::settings::load_overlay_hotkey`).
            overlay_hotkey: "Ctrl+Alt+Z".to_string(),
            overlay_max_results: 8,
            // 0.3.0 flow recall (docs/0.3.0.md PR6): 120 s min sustained-context dwell
            // for where-was-i (D9), and the mark-this-moment chord (D6, non-OS-reserved,
            // shell-registered so a conflict surfaces loudly in Settings).
            resume_min_dwell_secs: 120,
            sessions_min_len_secs: 120,
            sessions_gap_close_secs: 120,
            marks_hotkey: "Ctrl+Alt+M".to_string(),
            // 0.2.1 smart enrichment throttle (docs/0.2.0.md former PR5, 07 #49). Opt-in
            // master OFF: flipping it on backs enrichment off under sustained load. Enter
            // above 85% CPU / 90% GPU held 5 s; exit below 65% / 70% held 8 s (exit < enter
            // = hysteresis, so a value between the bands holds the level instead of
            // flapping); sample every 1 s; keep ≥1 embed_text worker at L2 so text indexing
            // never fully stalls. Every threshold is a setting, never hardcoded (PR3 stance).
            throttle_enabled: false,
            throttle_cpu_enter_pct: 85.0,
            throttle_cpu_exit_pct: 65.0,
            throttle_gpu_enter_pct: 90.0,
            throttle_gpu_exit_pct: 70.0,
            throttle_enter_after_ms: 5000,
            throttle_exit_after_ms: 8000,
            throttle_sample_interval_ms: 1000,
            throttle_embed_text_floor: 1,
            // 0.3.2 app lifecycle (03 §7d, D3): close-to-tray ON by default (closing the
            // window keeps ScreenSearch running with capture live); run-at-startup OFF by
            // default (never silently enable launch-at-login).
            app_close_to_tray: true,
            app_run_at_startup: false,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ApiStatus {
    /// Persisted intent — the master toggle.
    pub enabled: bool,
    /// Whether the HTTP server is currently bound and serving.
    pub running: bool,
    /// The configured port (default 43210); the bind is always `127.0.0.1`.
    pub port: u16,
    /// The bearer token (revealed/copied in Settings); `None` before first enable.
    pub token: Option<String>,
    /// Why the server is not running despite `enabled` (e.g. "port 43210 in use").
    pub last_error: Option<String>,
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
