//! Advanced (`file-only`) configuration (`03 §8`, 0.4.5 D6).
//!
//! Settings v2 splits the `03 §8` keys into three dispositions. `UI` keys live in the typed
//! [`crate::Settings`] struct and ride `get_settings`/`set_settings`; `deleted` keys no
//! longer exist; and the `file-only` keys live here, in one typed [`AdvancedConfig`] loaded
//! once at startup from `advanced.toml` in the app data directory (beside
//! `screensearch.db`). There is one source of truth per value: a field here is never also a
//! field of `Settings`.
//!
//! One `[section]` per key prefix and the suffix as the field name, so
//! `capture.diff_threshold` is `[capture]` / `diff_threshold`. Defaults and clamps mirror
//! `03 §8`; [`AdvancedConfig::sanitized`] is the backend trust boundary because the file is
//! hand-editable.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ipc::{FlashAttnSetting, KvCacheType, SidecarParams};

/// File name of the advanced configuration, in the app data directory.
pub const ADVANCED_CONFIG_FILE: &str = "advanced.toml";

/// Every `file-only` key from `03 §8`, typed and grouped by prefix.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct AdvancedConfig {
    pub capture: CaptureAdvanced,
    pub enrich: EnrichAdvanced,
    pub overlay: OverlayAdvanced,
    pub reports: ReportsAdvanced,
    pub resume: ResumeAdvanced,
    pub sidecar: SidecarAdvanced,
    pub text: TextAdvanced,
    pub throttle: ThrottleAdvanced,
}

impl AdvancedConfig {
    /// Applies the `03 §8` clamps to every field, mirroring the typed-settings sanitizer:
    /// the TOML file is hand-editable, so it is not a trust boundary.
    pub fn sanitized(mut self) -> Self {
        self.capture.diff_threshold = clamp_f32(self.capture.diff_threshold, 0.0, 1.0);
        self.capture.event_debounce_ms = clamp_u32(self.capture.event_debounce_ms, 100, 10_000);
        self.capture.event_fallback_interval_ms =
            clamp_u32(self.capture.event_fallback_interval_ms, 1_000, 3_600_000);
        self.capture.event_idle_threshold_ms =
            clamp_u32(self.capture.event_idle_threshold_ms, 1_000, 60_000);
        self.capture.event_min_interval_ms =
            clamp_u32(self.capture.event_min_interval_ms, 250, 60_000);
        self.capture.uia_latency_budget_ms =
            clamp_u32(self.capture.uia_latency_budget_ms, 20, 2_000);
        self.capture.uia_max_nodes = clamp_u32(self.capture.uia_max_nodes, 100, 20_000);
        self.capture.uia_max_textpattern_calls =
            clamp_u32(self.capture.uia_max_textpattern_calls, 1, 4_096);
        self.capture.uia_min_text_chars = clamp_u32(self.capture.uia_min_text_chars, 0, 10_000);
        self.capture.uia_suppress_during_input_ms =
            clamp_u32(self.capture.uia_suppress_during_input_ms, 0, 10_000);

        self.enrich.vision_batch_size = clamp_u32(self.enrich.vision_batch_size, 1, 500);
        self.enrich.vision_idle_secs = clamp_u32(self.enrich.vision_idle_secs, 60, 86_400);
        self.enrich.worker_concurrency = clamp_u32(self.enrich.worker_concurrency, 1, 16);

        self.overlay.max_results = clamp_u32(self.overlay.max_results, 1, 50);

        self.reports.map_reduce_min_frames =
            clamp_u32(self.reports.map_reduce_min_frames, 1, 1_000);
        self.reports.weekly_top_k = clamp_u32(self.reports.weekly_top_k, 1, 2_000);

        self.resume.min_dwell_secs = clamp_u32(self.resume.min_dwell_secs, 10, 86_400);

        self.sidecar.idle_ttl_secs = clamp_u32(self.sidecar.idle_ttl_secs, 0, 86_400);
        self.sidecar.ngl = clamp_u32(self.sidecar.ngl, 0, 999);
        // 0 is the "automatic" sentinel (a per-lane default chosen at resolution); any other
        // value is a real context window, clamped to a sane band.
        self.sidecar.ctx_size = if self.sidecar.ctx_size == 0 {
            0
        } else {
            clamp_u32(self.sidecar.ctx_size, 512, 32_768)
        };
        // 0 = automatic (derived from RAM); an explicit ceiling is floored at 8 GiB so it
        // clears the vision model's ~6.8 GB warmup baseline instead of triggering a recycle
        // loop.
        self.sidecar.recycle_rss_mb = if self.sidecar.recycle_rss_mb == 0 {
            0
        } else {
            clamp_u32(self.sidecar.recycle_rss_mb, 8192, 131_072)
        };
        self.sidecar.device = self
            .sidecar
            .device
            .and_then(|device| (!device.trim().is_empty()).then(|| device.trim().to_string()));

        self.text.chrome_protect_min_chars =
            clamp_u32(self.text.chrome_protect_min_chars, 1, 4_096);
        self.text.chrome_region_buckets = clamp_u32(self.text.chrome_region_buckets, 1, 32);
        self.text.chrome_suppress_min_seen =
            clamp_u32(self.text.chrome_suppress_min_seen, 2, 100_000);

        // Each exit threshold stays strictly below its enter threshold so the hysteresis
        // invariant holds even against a hand-edited file.
        self.throttle.cpu_enter_pct = clamp_f32(self.throttle.cpu_enter_pct, 1.0, 100.0);
        self.throttle.cpu_exit_pct = clamp_f32(
            self.throttle.cpu_exit_pct,
            0.0,
            self.throttle.cpu_enter_pct - 1.0,
        );
        self.throttle.gpu_enter_pct = clamp_f32(self.throttle.gpu_enter_pct, 1.0, 100.0);
        self.throttle.gpu_exit_pct = clamp_f32(
            self.throttle.gpu_exit_pct,
            0.0,
            self.throttle.gpu_enter_pct - 1.0,
        );
        self.throttle.enter_after_ms = clamp_u32(self.throttle.enter_after_ms, 500, 120_000);
        self.throttle.exit_after_ms = clamp_u32(self.throttle.exit_after_ms, 500, 300_000);
        self.throttle.sample_interval_ms = clamp_u32(self.throttle.sample_interval_ms, 250, 10_000);
        self.throttle.embed_text_floor = clamp_u32(self.throttle.embed_text_floor, 1, 16);
        self
    }
}

/// `[capture]` — event-driven capture and UI Automation tuning (`capture.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct CaptureAdvanced {
    /// `capture.diff_threshold` — frame-diff gate sensitivity.
    pub diff_threshold: f32,
    /// `capture.event_debounce_ms` — collapse a trigger burst, clamp 100..=10000.
    pub event_debounce_ms: u32,
    /// `capture.event_driven_enabled` — master switch for the opt-in trigger source.
    pub event_driven_enabled: bool,
    /// `capture.event_fallback_interval_ms` — static-screen sampling floor.
    pub event_fallback_interval_ms: u32,
    /// `capture.event_idle_threshold_ms` — idle dwell that counts as "gone idle".
    pub event_idle_threshold_ms: u32,
    /// `capture.event_min_interval_ms` — the rate ceiling.
    pub event_min_interval_ms: u32,
    /// `capture.event_on_foreground` — capture on foreground/app switch.
    pub event_on_foreground: bool,
    /// `capture.event_on_idle` — capture when the user goes idle past the threshold.
    pub event_on_idle: bool,
    /// `capture.uia_latency_budget_ms` — soft per-walk budget.
    pub uia_latency_budget_ms: u32,
    /// `capture.uia_max_nodes` — hard node cap per walk.
    pub uia_max_nodes: u32,
    /// `capture.uia_max_textpattern_calls` — live `TextPattern` reads per walk.
    pub uia_max_textpattern_calls: u32,
    /// `capture.uia_min_text_chars` — thin-yield floor below which OCR is preferred.
    pub uia_min_text_chars: u32,
    /// `capture.uia_suppress_during_input_ms` — skip UIA right after keyboard/mouse input.
    pub uia_suppress_during_input_ms: u32,
    /// `capture.uia_text_enabled` — UIA text with OCR fallback.
    pub uia_text_enabled: bool,
    /// `capture.uia_view_control_only` — walk the lighter control view.
    pub uia_view_control_only: bool,
}

impl Default for CaptureAdvanced {
    fn default() -> Self {
        Self {
            diff_threshold: 0.006,
            event_debounce_ms: 500,
            event_driven_enabled: false,
            event_fallback_interval_ms: 30_000,
            event_idle_threshold_ms: 5_000,
            event_min_interval_ms: 1_000,
            event_on_foreground: true,
            event_on_idle: false,
            uia_latency_budget_ms: 150,
            uia_max_nodes: 4_000,
            uia_max_textpattern_calls: 64,
            uia_min_text_chars: 16,
            uia_suppress_during_input_ms: 500,
            uia_text_enabled: true,
            uia_view_control_only: true,
        }
    }
}

/// `[enrich]` — enrichment lanes and worker tuning (`enrich.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct EnrichAdvanced {
    /// `enrich.embed_text` — text-embedding lane toggle.
    pub embed_text: bool,
    /// `enrich.vision_batch_size` — frames a timer/idle tick enqueues.
    pub vision_batch_size: u32,
    /// `enrich.vision_idle_enabled` — idle-triggered vision enrichment.
    pub vision_idle_enabled: bool,
    /// `enrich.vision_idle_secs` — idle dwell required before an idle vision tick.
    pub vision_idle_secs: u32,
    /// `enrich.worker_concurrency` — enrichment worker count.
    pub worker_concurrency: u32,
}

impl Default for EnrichAdvanced {
    fn default() -> Self {
        Self {
            embed_text: true,
            vision_batch_size: 20,
            vision_idle_enabled: false,
            vision_idle_secs: 300,
            worker_concurrency: 2,
        }
    }
}

/// `[overlay]` — Flow overlay tuning (`overlay.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct OverlayAdvanced {
    /// `overlay.max_results` — top-N results in the overlay.
    pub max_results: u32,
}

impl Default for OverlayAdvanced {
    fn default() -> Self {
        Self { max_results: 8 }
    }
}

/// `[reports]` — report depth beyond the daily UI knob (`reports.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct ReportsAdvanced {
    /// `reports.map_reduce_min_frames` — frame count above which a range uses map-reduce.
    pub map_reduce_min_frames: u32,
    /// `reports.weekly_top_k` — weekly report frame cap.
    pub weekly_top_k: u32,
}

impl Default for ReportsAdvanced {
    fn default() -> Self {
        Self {
            map_reduce_min_frames: 20,
            weekly_top_k: 200,
        }
    }
}

/// `[resume]` — where-was-i tuning (`resume.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct ResumeAdvanced {
    /// `resume.min_dwell_secs` — min sustained-context dwell for where-was-i.
    pub min_dwell_secs: u32,
}

impl Default for ResumeAdvanced {
    fn default() -> Self {
        Self {
            min_dwell_secs: 120,
        }
    }
}

/// `[sidecar]` — llama.cpp launch and lifecycle knobs (`sidecar.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct SidecarAdvanced {
    /// `sidecar.ctx_size` — `0` = automatic per-lane default; else 512..=32768.
    pub ctx_size: u32,
    /// `sidecar.device` — optional llama.cpp `--device` selector; blank round-trips as `None`.
    pub device: Option<String>,
    /// `sidecar.flash_attn` — flash-attention mode.
    pub flash_attn: FlashAttnSetting,
    /// `sidecar.idle_ttl_secs` — idle TTL, resolved once at supervisor construction.
    pub idle_ttl_secs: u32,
    /// `sidecar.kv_cache_type` — KV-cache precision.
    pub kv_cache_type: KvCacheType,
    /// `sidecar.ngl` — GPU layers offloaded to the sidecar.
    pub ngl: u32,
    /// `sidecar.recycle_enabled` — recycle on the committed-RAM ceiling.
    pub recycle_enabled: bool,
    /// `sidecar.recycle_rss_mb` — `0` = automatic ceiling; else 8192..=131072.
    pub recycle_rss_mb: u32,
}

impl Default for SidecarAdvanced {
    fn default() -> Self {
        Self {
            ctx_size: 0,
            device: None,
            flash_attn: FlashAttnSetting::Auto,
            idle_ttl_secs: 180,
            kv_cache_type: KvCacheType::Q8_0,
            ngl: 99,
            recycle_enabled: true,
            recycle_rss_mb: 0,
        }
    }
}

/// `[text]` — chrome-suppression classifier tuning (`text.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct TextAdvanced {
    /// `text.chrome_protect_min_chars` — lines at least this long are never suppressed.
    pub chrome_protect_min_chars: u32,
    /// `text.chrome_region_buckets` — grid resolution for a span's `region_bucket`.
    pub chrome_region_buckets: u32,
    /// `text.chrome_suppress_min_seen` — appearances before a signature is marked chrome.
    pub chrome_suppress_min_seen: u32,
}

impl Default for TextAdvanced {
    fn default() -> Self {
        Self {
            chrome_protect_min_chars: 48,
            chrome_region_buckets: 8,
            // Lowered from 12 after the PR3 audit (`docs/AUDIT_0.2.0_PR3_2026-06-26.md`): a
            // repeated short edge label is caught within a few captures instead of leaking
            // through a long cold-start window.
            chrome_suppress_min_seen: 4,
        }
    }
}

/// `[throttle]` — under-pressure enrichment governor tuning (`throttle.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
#[serde(default)]
pub struct ThrottleAdvanced {
    /// `throttle.cpu_enter_pct` — CPU busy % at which pressure counts toward raising a level.
    pub cpu_enter_pct: f32,
    /// `throttle.cpu_exit_pct` — CPU busy % below which pressure counts toward lowering.
    pub cpu_exit_pct: f32,
    /// `throttle.embed_text_floor` — min concurrent `embed_text` workers at level 2.
    pub embed_text_floor: u32,
    /// `throttle.enter_after_ms` — sustained-enter dwell before stepping up.
    pub enter_after_ms: u32,
    /// `throttle.exit_after_ms` — recovered-exit dwell before stepping down.
    pub exit_after_ms: u32,
    /// `throttle.gpu_enter_pct` — GPU utilization % enter threshold.
    pub gpu_enter_pct: f32,
    /// `throttle.gpu_exit_pct` — GPU utilization % exit threshold.
    pub gpu_exit_pct: f32,
    /// `throttle.sample_interval_ms` — probe/sample cadence.
    pub sample_interval_ms: u32,
}

impl Default for ThrottleAdvanced {
    fn default() -> Self {
        Self {
            cpu_enter_pct: 85.0,
            cpu_exit_pct: 65.0,
            embed_text_floor: 1,
            enter_after_ms: 5_000,
            exit_after_ms: 8_000,
            gpu_enter_pct: 90.0,
            gpu_exit_pct: 70.0,
            sample_interval_ms: 1_000,
        }
    }
}

impl From<&AdvancedConfig> for SidecarParams {
    fn from(config: &AdvancedConfig) -> Self {
        Self {
            ngl: config.sidecar.ngl,
            device: config.sidecar.device.clone(),
            ctx_size: config.sidecar.ctx_size,
            kv_cache_type: config.sidecar.kv_cache_type,
            flash_attn: config.sidecar.flash_attn,
        }
    }
}

fn clamp_u32(value: u32, min: u32, max: u32) -> u32 {
    value.clamp(min, max)
}

fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}
