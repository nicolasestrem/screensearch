//! Typed settings loading: assemble the strongly-typed [`Settings`] from the
//! opaque key/value `settings` table (`03 §8`).
//!
//! Each `03 §8` key is read individually; a missing or unparsable value falls back
//! to the corresponding [`Settings::default`] field (never an error — capture must
//! be able to start on a fresh DB). Composite values (`capture.monitors`,
//! `privacy.excluded_apps`, model tiers) are stored as JSON.

use serde::de::DeserializeOwned;
use std::str::FromStr;
use traits::{CaptureConfig, ModelTier, Result, Settings, Store};

/// Reads every `03 §8` setting, falling back to [`Settings::default`] per key.
pub async fn load_settings(store: &dyn Store) -> Settings {
    let d = Settings::default();
    sanitize_settings(Settings {
        capture_interval_ms: num(store, "capture.interval_ms", d.capture_interval_ms).await,
        capture_monitors: json(store, "capture.monitors", d.capture_monitors).await,
        capture_diff_threshold: num(store, "capture.diff_threshold", d.capture_diff_threshold)
            .await,
        storage_max_width: num(store, "storage.max_width", d.storage_max_width).await,
        storage_retention_days: num(store, "storage.retention_days", d.storage_retention_days)
            .await,
        enrich_embed_text: boolean(store, "enrich.embed_text", d.enrich_embed_text).await,
        enrich_vision_timer_enabled: boolean(
            store,
            "enrich.vision_timer_enabled",
            d.enrich_vision_timer_enabled,
        )
        .await,
        enrich_vision_timer_interval_ms: num(
            store,
            "enrich.vision_timer_interval_ms",
            d.enrich_vision_timer_interval_ms,
        )
        .await,
        enrich_vision_idle_enabled: boolean(
            store,
            "enrich.vision_idle_enabled",
            d.enrich_vision_idle_enabled,
        )
        .await,
        enrich_vision_idle_secs: num(store, "enrich.vision_idle_secs", d.enrich_vision_idle_secs)
            .await,
        enrich_vision_batch_size: num(
            store,
            "enrich.vision_batch_size",
            d.enrich_vision_batch_size,
        )
        .await,
        enrich_worker_concurrency: num(
            store,
            "enrich.worker_concurrency",
            d.enrich_worker_concurrency,
        )
        .await,
        models_vision_tier: load_tier(store, "models.vision_tier", d.models_vision_tier).await,
        models_answer_tier: load_tier(store, "models.answer_tier", d.models_answer_tier).await,
        answer_thinking: boolean(store, "answer.thinking", d.answer_thinking).await,
        sidecar_idle_ttl_secs: num(store, "sidecar.idle_ttl_secs", d.sidecar_idle_ttl_secs).await,
        sidecar_ngl: num(store, "sidecar.ngl", d.sidecar_ngl).await,
        sidecar_device: json(store, "sidecar.device", d.sidecar_device).await,
        sidecar_ctx_size: num(store, "sidecar.ctx_size", d.sidecar_ctx_size).await,
        sidecar_kv_cache_type: json(store, "sidecar.kv_cache_type", d.sidecar_kv_cache_type).await,
        sidecar_flash_attn: json(store, "sidecar.flash_attn", d.sidecar_flash_attn).await,
        sidecar_recycle_enabled: boolean(
            store,
            "sidecar.recycle_enabled",
            d.sidecar_recycle_enabled,
        )
        .await,
        sidecar_recycle_rss_mb: num(store, "sidecar.recycle_rss_mb", d.sidecar_recycle_rss_mb)
            .await,
        privacy_excluded_apps: json(store, "privacy.excluded_apps", d.privacy_excluded_apps).await,
        privacy_pause_on_lock: boolean(store, "privacy.pause_on_lock", d.privacy_pause_on_lock)
            .await,
        text_include_chrome_default: boolean(
            store,
            "text.include_chrome_default",
            d.text_include_chrome_default,
        )
        .await,
        text_chrome_suppress_min_seen: num(
            store,
            "text.chrome_suppress_min_seen",
            d.text_chrome_suppress_min_seen,
        )
        .await,
        text_chrome_protect_min_chars: num(
            store,
            "text.chrome_protect_min_chars",
            d.text_chrome_protect_min_chars,
        )
        .await,
        text_chrome_region_buckets: num(
            store,
            "text.chrome_region_buckets",
            d.text_chrome_region_buckets,
        )
        .await,
        retrieval_default_top_k: num(store, "retrieval.default_top_k", d.retrieval_default_top_k)
            .await,
        reports_daily_top_k: num(store, "reports.daily_top_k", d.reports_daily_top_k).await,
        reports_weekly_top_k: num(store, "reports.weekly_top_k", d.reports_weekly_top_k).await,
        reports_map_reduce_min_frames: num(
            store,
            "reports.map_reduce_min_frames",
            d.reports_map_reduce_min_frames,
        )
        .await,
        capture_event_driven_enabled: boolean(
            store,
            "capture.event_driven_enabled",
            d.capture_event_driven_enabled,
        )
        .await,
        capture_event_on_foreground: boolean(
            store,
            "capture.event_on_foreground",
            d.capture_event_on_foreground,
        )
        .await,
        capture_event_on_idle: boolean(store, "capture.event_on_idle", d.capture_event_on_idle)
            .await,
        capture_event_debounce_ms: num(
            store,
            "capture.event_debounce_ms",
            d.capture_event_debounce_ms,
        )
        .await,
        capture_event_min_interval_ms: num(
            store,
            "capture.event_min_interval_ms",
            d.capture_event_min_interval_ms,
        )
        .await,
        capture_event_idle_threshold_ms: num(
            store,
            "capture.event_idle_threshold_ms",
            d.capture_event_idle_threshold_ms,
        )
        .await,
        capture_event_fallback_interval_ms: num(
            store,
            "capture.event_fallback_interval_ms",
            d.capture_event_fallback_interval_ms,
        )
        .await,
        capture_uia_text_enabled: boolean(
            store,
            "capture.uia_text_enabled",
            d.capture_uia_text_enabled,
        )
        .await,
        capture_uia_latency_budget_ms: num(
            store,
            "capture.uia_latency_budget_ms",
            d.capture_uia_latency_budget_ms,
        )
        .await,
        capture_uia_min_text_chars: num(
            store,
            "capture.uia_min_text_chars",
            d.capture_uia_min_text_chars,
        )
        .await,
        capture_uia_view_control_only: boolean(
            store,
            "capture.uia_view_control_only",
            d.capture_uia_view_control_only,
        )
        .await,
        capture_uia_max_nodes: num(store, "capture.uia_max_nodes", d.capture_uia_max_nodes).await,
        capture_uia_max_textpattern_calls: num(
            store,
            "capture.uia_max_textpattern_calls",
            d.capture_uia_max_textpattern_calls,
        )
        .await,
        capture_uia_suppress_during_input_ms: num(
            store,
            "capture.uia_suppress_during_input_ms",
            d.capture_uia_suppress_during_input_ms,
        )
        .await,
        overlay_hotkey: load_overlay_hotkey(store, d.overlay_hotkey).await,
        overlay_max_results: num(store, "overlay.max_results", d.overlay_max_results).await,
        resume_min_dwell_secs: num(store, "resume.min_dwell_secs", d.resume_min_dwell_secs).await,
        sessions_min_len_secs: num(store, "sessions.min_len_secs", d.sessions_min_len_secs).await,
        sessions_gap_close_secs: load_sessions_gap_close_secs(store, d.sessions_gap_close_secs)
            .await,
        marks_hotkey: json(store, "marks.hotkey", d.marks_hotkey).await,
        throttle_enabled: boolean(store, "throttle.enabled", d.throttle_enabled).await,
        throttle_cpu_enter_pct: num(store, "throttle.cpu_enter_pct", d.throttle_cpu_enter_pct)
            .await,
        throttle_cpu_exit_pct: num(store, "throttle.cpu_exit_pct", d.throttle_cpu_exit_pct).await,
        throttle_gpu_enter_pct: num(store, "throttle.gpu_enter_pct", d.throttle_gpu_enter_pct)
            .await,
        throttle_gpu_exit_pct: num(store, "throttle.gpu_exit_pct", d.throttle_gpu_exit_pct).await,
        throttle_enter_after_ms: num(store, "throttle.enter_after_ms", d.throttle_enter_after_ms)
            .await,
        throttle_exit_after_ms: num(store, "throttle.exit_after_ms", d.throttle_exit_after_ms)
            .await,
        throttle_sample_interval_ms: num(
            store,
            "throttle.sample_interval_ms",
            d.throttle_sample_interval_ms,
        )
        .await,
        throttle_embed_text_floor: num(
            store,
            "throttle.embed_text_floor",
            d.throttle_embed_text_floor,
        )
        .await,
        app_close_to_tray: boolean(store, "app.close_to_tray", d.app_close_to_tray).await,
        app_run_at_startup: boolean(store, "app.run_at_startup", d.app_run_at_startup).await,
    })
}

/// Settings keys retired by past versions, purged from the DB on startup so a config
/// persisted with any of them still loads and the stale rows don't linger (`03 §8`,
/// `docs/0.3.0.md` PR2/PR4). Grows per arc: 0.3.0 PR2 removed the four extra event triggers
/// (clipboard / typing-pause / click / scroll-stop) and the typing-pause threshold; PR4
/// removed the image-embedding lane toggle (`enrich.image_embeddings`); 0.3.2 PR5 retired
/// the two provably-inert knobs — `storage.jpeg_quality` (inert since the lossless-WebP
/// storage path) and `capture.uia_run_on_interactive` (its only firing triggers were
/// removed by the 0.3.0 trigger trim, `07` #83) — per the D8 removal mechanics. The
/// 2026-08-01 usage review demoted the flat `sessions.focus_min_len_secs` sweep parameter
/// to a named constant; the already-constant density parameter is listed too so either
/// historical orphan is removed rather than silently retained (usage review 2026-08-01 §6.7).
pub const RETIRED_SETTINGS_KEYS: &[&str] = &[
    "capture.event_on_clipboard",
    "capture.event_on_typing_pause",
    "capture.event_on_click",
    "capture.event_on_scroll_stop",
    "capture.event_typing_pause_ms",
    "enrich.image_embeddings",
    "storage.jpeg_quality",
    "capture.uia_run_on_interactive",
    "sessions.focus_min_len_secs",
    "sessions.focus_min_density_fph",
];

/// Deletes any [`RETIRED_SETTINGS_KEYS`] left in the `settings` table by an older
/// version, logging **once** which keys were dropped. Called once at startup (before
/// the rest of the maintenance sweep). "Log once" falls out naturally — the keys are
/// gone by the next launch, so a subsequent run finds nothing and logs nothing. A read
/// or delete error is logged and swallowed: dropping stale keys is best-effort and must
/// never block startup or capture (`04 §4`).
pub async fn drop_retired_settings(store: &dyn Store) {
    match store.delete_settings(RETIRED_SETTINGS_KEYS).await {
        Ok(dropped) if !dropped.is_empty() => {
            tracing::warn!(keys = ?dropped, "settings: dropped retired keys");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "settings: failed to drop retired keys"),
    }
}

/// Persists every `03 §8` setting back to the key/value `settings` table — the
/// exact inverse of [`load_settings`], using the **same key strings** so a saved
/// value round-trips. Numbers are written via `to_string`, bools as `"true"`/
/// `"false"`, and composite values (`capture.monitors`, `privacy.excluded_apps`,
/// model tiers) as JSON (matching how `set_model_tier` already writes a tier).
///
/// Every pair (including the fallible JSON encodings) is built **before** any write,
/// then committed in one transaction via [`Store::set_settings_batch`]. This is
/// all-or-nothing: a serialization error short-circuits with zero writes, and a
/// crash mid-commit rolls back — so [`load_settings`] never observes a mix of new
/// and stale keys (its per-key default fallback would hide such a split silently).
///
/// Backs the `set_settings` command (`03 §7`): the values are durable immediately;
/// which subsystems re-read them live vs on restart is documented in the Settings
/// UI (model tiers hot-apply; capture/storage/privacy on next capture start; the
/// rest on app restart).
pub async fn save_settings(store: &dyn Store, s: &Settings) -> Result<()> {
    let s = sanitize_settings(s.clone());
    let kvs: Vec<(String, String)> = vec![
        (
            "capture.interval_ms".into(),
            s.capture_interval_ms.to_string(),
        ),
        (
            "capture.monitors".into(),
            serde_json::to_string(&s.capture_monitors)?,
        ),
        (
            "capture.diff_threshold".into(),
            s.capture_diff_threshold.to_string(),
        ),
        ("storage.max_width".into(), s.storage_max_width.to_string()),
        (
            "storage.retention_days".into(),
            s.storage_retention_days.to_string(),
        ),
        (
            "enrich.embed_text".into(),
            bool_str(s.enrich_embed_text).into(),
        ),
        (
            "enrich.vision_timer_enabled".into(),
            bool_str(s.enrich_vision_timer_enabled).into(),
        ),
        (
            "enrich.vision_timer_interval_ms".into(),
            s.enrich_vision_timer_interval_ms.to_string(),
        ),
        (
            "enrich.vision_idle_enabled".into(),
            bool_str(s.enrich_vision_idle_enabled).into(),
        ),
        (
            "enrich.vision_idle_secs".into(),
            s.enrich_vision_idle_secs.to_string(),
        ),
        (
            "enrich.vision_batch_size".into(),
            s.enrich_vision_batch_size.to_string(),
        ),
        (
            "enrich.worker_concurrency".into(),
            s.enrich_worker_concurrency.to_string(),
        ),
        (
            "models.vision_tier".into(),
            serde_json::to_string(&s.models_vision_tier)?,
        ),
        (
            "models.answer_tier".into(),
            serde_json::to_string(&s.models_answer_tier)?,
        ),
        ("answer.thinking".into(), bool_str(s.answer_thinking).into()),
        (
            "sidecar.idle_ttl_secs".into(),
            s.sidecar_idle_ttl_secs.to_string(),
        ),
        ("sidecar.ngl".into(), s.sidecar_ngl.to_string()),
        (
            "sidecar.device".into(),
            serde_json::to_string(&s.sidecar_device)?,
        ),
        ("sidecar.ctx_size".into(), s.sidecar_ctx_size.to_string()),
        (
            "sidecar.kv_cache_type".into(),
            serde_json::to_string(&s.sidecar_kv_cache_type)?,
        ),
        (
            "sidecar.flash_attn".into(),
            serde_json::to_string(&s.sidecar_flash_attn)?,
        ),
        (
            "sidecar.recycle_enabled".into(),
            bool_str(s.sidecar_recycle_enabled).into(),
        ),
        (
            "sidecar.recycle_rss_mb".into(),
            s.sidecar_recycle_rss_mb.to_string(),
        ),
        (
            "privacy.excluded_apps".into(),
            serde_json::to_string(&s.privacy_excluded_apps)?,
        ),
        (
            "privacy.pause_on_lock".into(),
            bool_str(s.privacy_pause_on_lock).into(),
        ),
        (
            "text.include_chrome_default".into(),
            bool_str(s.text_include_chrome_default).into(),
        ),
        (
            "text.chrome_suppress_min_seen".into(),
            s.text_chrome_suppress_min_seen.to_string(),
        ),
        (
            "text.chrome_protect_min_chars".into(),
            s.text_chrome_protect_min_chars.to_string(),
        ),
        (
            "text.chrome_region_buckets".into(),
            s.text_chrome_region_buckets.to_string(),
        ),
        (
            "retrieval.default_top_k".into(),
            s.retrieval_default_top_k.to_string(),
        ),
        (
            "reports.daily_top_k".into(),
            s.reports_daily_top_k.to_string(),
        ),
        (
            "reports.weekly_top_k".into(),
            s.reports_weekly_top_k.to_string(),
        ),
        (
            "reports.map_reduce_min_frames".into(),
            s.reports_map_reduce_min_frames.to_string(),
        ),
        (
            "capture.event_driven_enabled".into(),
            bool_str(s.capture_event_driven_enabled).into(),
        ),
        (
            "capture.event_on_foreground".into(),
            bool_str(s.capture_event_on_foreground).into(),
        ),
        (
            "capture.event_on_idle".into(),
            bool_str(s.capture_event_on_idle).into(),
        ),
        (
            "capture.event_debounce_ms".into(),
            s.capture_event_debounce_ms.to_string(),
        ),
        (
            "capture.event_min_interval_ms".into(),
            s.capture_event_min_interval_ms.to_string(),
        ),
        (
            "capture.event_idle_threshold_ms".into(),
            s.capture_event_idle_threshold_ms.to_string(),
        ),
        (
            "capture.event_fallback_interval_ms".into(),
            s.capture_event_fallback_interval_ms.to_string(),
        ),
        (
            "capture.uia_text_enabled".into(),
            bool_str(s.capture_uia_text_enabled).into(),
        ),
        (
            "capture.uia_latency_budget_ms".into(),
            s.capture_uia_latency_budget_ms.to_string(),
        ),
        (
            "capture.uia_min_text_chars".into(),
            s.capture_uia_min_text_chars.to_string(),
        ),
        (
            "capture.uia_view_control_only".into(),
            bool_str(s.capture_uia_view_control_only).into(),
        ),
        (
            "capture.uia_max_nodes".into(),
            s.capture_uia_max_nodes.to_string(),
        ),
        (
            "capture.uia_max_textpattern_calls".into(),
            s.capture_uia_max_textpattern_calls.to_string(),
        ),
        (
            "capture.uia_suppress_during_input_ms".into(),
            s.capture_uia_suppress_during_input_ms.to_string(),
        ),
        (
            "overlay.hotkey".into(),
            serde_json::to_string(&s.overlay_hotkey)?,
        ),
        (
            "overlay.max_results".into(),
            s.overlay_max_results.to_string(),
        ),
        (
            "resume.min_dwell_secs".into(),
            s.resume_min_dwell_secs.to_string(),
        ),
        (
            "sessions.min_len_secs".into(),
            s.sessions_min_len_secs.to_string(),
        ),
        (
            "sessions.gap_close_secs".into(),
            s.sessions_gap_close_secs.to_string(),
        ),
        (
            "marks.hotkey".into(),
            serde_json::to_string(&s.marks_hotkey)?,
        ),
        (
            "throttle.enabled".into(),
            bool_str(s.throttle_enabled).into(),
        ),
        (
            "throttle.cpu_enter_pct".into(),
            s.throttle_cpu_enter_pct.to_string(),
        ),
        (
            "throttle.cpu_exit_pct".into(),
            s.throttle_cpu_exit_pct.to_string(),
        ),
        (
            "throttle.gpu_enter_pct".into(),
            s.throttle_gpu_enter_pct.to_string(),
        ),
        (
            "throttle.gpu_exit_pct".into(),
            s.throttle_gpu_exit_pct.to_string(),
        ),
        (
            "throttle.enter_after_ms".into(),
            s.throttle_enter_after_ms.to_string(),
        ),
        (
            "throttle.exit_after_ms".into(),
            s.throttle_exit_after_ms.to_string(),
        ),
        (
            "throttle.sample_interval_ms".into(),
            s.throttle_sample_interval_ms.to_string(),
        ),
        (
            "throttle.embed_text_floor".into(),
            s.throttle_embed_text_floor.to_string(),
        ),
        (
            "app.close_to_tray".into(),
            bool_str(s.app_close_to_tray).into(),
        ),
        (
            "app.run_at_startup".into(),
            bool_str(s.app_run_at_startup).into(),
        ),
    ];
    store.set_settings_batch(&kvs).await
}

/// Backend-side numeric bounds matching the Settings UI's save-time sanitizer. The
/// UI is not a trust boundary: persisted settings may be hand-edited, migrated from
/// older builds, or sent directly over IPC, so the kernel clamps before use/write.
pub fn sanitize_settings(mut s: Settings) -> Settings {
    s.capture_interval_ms = clamp_u32(s.capture_interval_ms, 250, 3_600_000);
    s.capture_diff_threshold = clamp_f32(s.capture_diff_threshold, 0.0, 1.0);
    // 0 = native (no downscale) — the capture path keeps the frame at the monitor's full
    // width so ultra-wide captures stay legible. Any positive value is a real width ceiling
    // clamped to a sane band (mirrors the `0 = auto` sentinels for sidecar ctx/recycle).
    s.storage_max_width = if s.storage_max_width == 0 {
        0
    } else {
        clamp_u32(s.storage_max_width, 320, 7680)
    };
    s.storage_retention_days = clamp_u32(s.storage_retention_days, 0, 3650);
    s.enrich_worker_concurrency = clamp_u32(s.enrich_worker_concurrency, 1, 16);
    s.enrich_vision_timer_interval_ms =
        clamp_u32(s.enrich_vision_timer_interval_ms, 60_000, 86_400_000);
    s.enrich_vision_idle_secs = clamp_u32(s.enrich_vision_idle_secs, 60, 86_400);
    s.enrich_vision_batch_size = clamp_u32(s.enrich_vision_batch_size, 1, 500);
    s.sidecar_idle_ttl_secs = clamp_u32(s.sidecar_idle_ttl_secs, 0, 86_400);
    s.sidecar_ngl = clamp_u32(s.sidecar_ngl, 0, 999);
    // 0 is the "automatic" sentinel (per-lane default chosen at resolution); any other
    // value is a real context window, clamped to a sane band.
    s.sidecar_ctx_size = if s.sidecar_ctx_size == 0 {
        0
    } else {
        clamp_u32(s.sidecar_ctx_size, 512, 32_768)
    };
    // 0 = automatic (derived from RAM); any explicit value is a real MiB ceiling clamped
    // to a sane band. The 8 GiB floor matches `auto_recycle_ceiling`'s `lo`: the vision model
    // loads to ~6.8 GB *before* the first inference, so any ceiling below that would fire
    // `over_recycle_ceiling` immediately after every warmup → recycle → reload → over again,
    // a silent continuous loop. 8 GiB clears the baseline with headroom to accumulate.
    s.sidecar_recycle_rss_mb = if s.sidecar_recycle_rss_mb == 0 {
        0
    } else {
        clamp_u32(s.sidecar_recycle_rss_mb, 8192, 131_072)
    };
    s.sidecar_device = s
        .sidecar_device
        .and_then(|d| (!d.trim().is_empty()).then(|| d.trim().to_string()));
    // PR3 attention-filter thresholds (03 §8). Floors keep the classifier sane: at
    // least two appearances before suppression; a positive protect length; a non-zero
    // region grid. Ceilings are generous guards against hand-edited extremes.
    s.text_chrome_suppress_min_seen = clamp_u32(s.text_chrome_suppress_min_seen, 2, 100_000);
    s.text_chrome_protect_min_chars = clamp_u32(s.text_chrome_protect_min_chars, 1, 4_096);
    s.text_chrome_region_buckets = clamp_u32(s.text_chrome_region_buckets, 1, 32);
    // 0.2.x retrieval + recall reports (03 §8). At least 1 of everything; generous
    // ceilings guard hand-edited extremes (the weekly cap also bounds report passes).
    s.retrieval_default_top_k = clamp_u32(s.retrieval_default_top_k, 1, 100);
    s.reports_daily_top_k = clamp_u32(s.reports_daily_top_k, 1, 1_000);
    s.reports_weekly_top_k = clamp_u32(s.reports_weekly_top_k, 1, 2_000);
    s.reports_map_reduce_min_frames = clamp_u32(s.reports_map_reduce_min_frames, 1, 1_000);
    // 0.2.1 event-driven capture (docs/0.2.0.md, 07 #47). Thresholds are settings, not
    // hardcoded; floors keep the trigger machine sane (a positive debounce/quiet window,
    // a real rate ceiling) and ceilings guard hand-edited extremes.
    s.capture_event_debounce_ms = clamp_u32(s.capture_event_debounce_ms, 100, 10_000);
    s.capture_event_min_interval_ms = clamp_u32(s.capture_event_min_interval_ms, 250, 60_000);
    s.capture_event_idle_threshold_ms = clamp_u32(s.capture_event_idle_threshold_ms, 1_000, 60_000);
    s.capture_event_fallback_interval_ms =
        clamp_u32(s.capture_event_fallback_interval_ms, 1_000, 3_600_000);
    // UIA text (docs/0.2.0.md #48). Latency budget floored well above a trivial walk and
    // ceilinged so a hand-edited extreme can't stall capture; min-text-chars is a free
    // count (0 disables the thin-yield fallback). Thresholds, not hardcoded.
    s.capture_uia_latency_budget_ms = clamp_u32(s.capture_uia_latency_budget_ms, 20, 2_000);
    s.capture_uia_min_text_chars = clamp_u32(s.capture_uia_min_text_chars, 0, 10_000);
    // 0.2.1 UIA hang fix (`07` #71). Node cap floored well above a trivial walk and ceilinged
    // so a hand-edited extreme can't reopen the freeze; live TextPattern reads floored at 1
    // (a doc page still gets some body text) and ceilinged. `view_control_only` is a bool
    // (no clamp). Thresholds, not hardcoded (`03 §3b`).
    s.capture_uia_max_nodes = clamp_u32(s.capture_uia_max_nodes, 100, 20_000);
    s.capture_uia_max_textpattern_calls = clamp_u32(s.capture_uia_max_textpattern_calls, 1, 4_096);
    // Input-suppression window: 0 (off) up to 10 s (a hand-edited extreme can't keep UIA off
    // forever — capture still samples via OCR meanwhile). Threshold, not hardcoded.
    s.capture_uia_suppress_during_input_ms =
        clamp_u32(s.capture_uia_suppress_during_input_ms, 0, 10_000);
    let default_overlay_hotkey = Settings::default().overlay_hotkey;
    s.overlay_hotkey = match s.overlay_hotkey.trim() {
        "" => default_overlay_hotkey,
        hotkey => hotkey.to_string(),
    };
    s.overlay_max_results = clamp_u32(s.overlay_max_results, 1, 50);
    // 0.3.0 flow recall (docs/0.3.0.md PR6). The where-was-i dwell is floored at 10 s (a
    // shorter "sustained" context is noise) and ceilinged at a day; the mark hotkey empty
    // → default, same shell-registered posture as the overlay hotkey. Thresholds/chords are
    // settings, never hardcoded (`03 §3b` stance); the shell surfaces a bad chord loudly (D6).
    s.resume_min_dwell_secs = clamp_u32(s.resume_min_dwell_secs, 10, 86_400);
    // 0.4.0 PR4 final clamps. These are the arc's only two user-tunable segmentation knobs;
    // merge/absorb/meeting/focus-density/freeze values are named correctness constants.
    s.sessions_min_len_secs = clamp_u32(s.sessions_min_len_secs, 30, 3_600);
    s.sessions_gap_close_secs = clamp_u32(s.sessions_gap_close_secs, 60, 3_600);
    let default_marks_hotkey = Settings::default().marks_hotkey;
    s.marks_hotkey = match s.marks_hotkey.trim() {
        "" => default_marks_hotkey,
        hotkey => hotkey.to_string(),
    };
    // 0.2.1 smart enrichment throttle (docs/0.2.0.md former PR5, 07 #49). Thresholds are
    // settings, never hardcoded. Each `*_exit_pct` is clamped strictly below its
    // `*_enter_pct` so the hysteresis invariant holds even against hand-edited DB values
    // (the kernel is not a trust boundary); the embed_text floor is clamped >= 1 so text
    // indexing never fully stalls under sustained pressure.
    s.throttle_cpu_enter_pct = clamp_f32(s.throttle_cpu_enter_pct, 1.0, 100.0);
    s.throttle_cpu_exit_pct =
        clamp_f32(s.throttle_cpu_exit_pct, 0.0, s.throttle_cpu_enter_pct - 1.0);
    s.throttle_gpu_enter_pct = clamp_f32(s.throttle_gpu_enter_pct, 1.0, 100.0);
    s.throttle_gpu_exit_pct =
        clamp_f32(s.throttle_gpu_exit_pct, 0.0, s.throttle_gpu_enter_pct - 1.0);
    s.throttle_enter_after_ms = clamp_u32(s.throttle_enter_after_ms, 500, 120_000);
    s.throttle_exit_after_ms = clamp_u32(s.throttle_exit_after_ms, 500, 300_000);
    s.throttle_sample_interval_ms = clamp_u32(s.throttle_sample_interval_ms, 250, 10_000);
    s.throttle_embed_text_floor = clamp_u32(s.throttle_embed_text_floor, 1, 16);
    s
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

/// The canonical string form for a persisted bool (parsed back by [`boolean`]).
fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

/// The capture-relevant slice of [`Settings`], handed to the capture impl (`03 §8`).
pub fn capture_config(s: &Settings) -> CaptureConfig {
    CaptureConfig {
        interval_ms: s.capture_interval_ms,
        monitors: s.capture_monitors.clone(),
        diff_threshold: s.capture_diff_threshold,
        excluded_apps: s.privacy_excluded_apps.clone(),
        pause_on_lock: s.privacy_pause_on_lock,
        event_driven_enabled: s.capture_event_driven_enabled,
        event_on_foreground: s.capture_event_on_foreground,
        event_on_idle: s.capture_event_on_idle,
        event_debounce_ms: s.capture_event_debounce_ms,
        event_min_interval_ms: s.capture_event_min_interval_ms,
        event_idle_threshold_ms: s.capture_event_idle_threshold_ms,
        event_fallback_interval_ms: s.capture_event_fallback_interval_ms,
    }
}

async fn num<T: FromStr>(store: &dyn Store, key: &str, default: T) -> T {
    match store.get_setting(key).await {
        Ok(Some(raw)) => match raw.parse() {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(key, raw = %raw, "settings: unparsable number; using default");
                default
            }
        },
        Ok(None) => default,
        Err(e) => {
            tracing::warn!(key, error = %e, "settings: read failed; using default");
            default
        }
    }
}
/// The pre-retune micro-gap default selected from the original two-day sample.
const LEGACY_SESSIONS_GAP_CLOSE_SECS: u32 = 300;

/// One-shot marker for the 300 → 120 default migration. It is deliberately outside [`Settings`]
/// so ordinary saves neither expose nor reset migration state.
const SESSIONS_GAP_CLOSE_MIGRATED_KEY: &str = "sessions.gap_close_secs_migrated";

/// Loads `sessions.gap_close_secs`, remapping only the retired 300-second default to the 11-day
/// sweep's 120-second winner (usage review 2026-08-01 §6.7).
///
/// The marker makes this a true one-shot: a user who deliberately chooses 300 after upgrade keeps
/// it. A fresh install latches the default and marker together; custom stored values latch without
/// rewriting. The value and marker commit atomically, so a failed migration has no partial state
/// and the next load retries; this load still returns the safe new default.
async fn load_sessions_gap_close_secs(store: &dyn Store, default: u32) -> u32 {
    let migrated = matches!(
        store.get_setting(SESSIONS_GAP_CLOSE_MIGRATED_KEY).await,
        Ok(Some(_))
    );
    let raw = match store.get_setting("sessions.gap_close_secs").await {
        Ok(Some(raw)) => raw,
        Ok(None) => {
            if !migrated {
                persist_sessions_gap_close_migration(store, default).await;
            }
            return default;
        }
        Err(e) => {
            tracing::warn!(
                key = "sessions.gap_close_secs",
                error = %e,
                "settings: read failed; using default"
            );
            return default;
        }
    };
    let stored = match raw.parse::<u32>() {
        Ok(value) => value,
        Err(_) => {
            tracing::warn!(
                key = "sessions.gap_close_secs",
                raw = %raw,
                "settings: unparsable number; using default"
            );
            if !migrated {
                mark_sessions_gap_close_migrated(store).await;
            }
            return default;
        }
    };
    if migrated {
        return stored;
    }
    if stored == LEGACY_SESSIONS_GAP_CLOSE_SECS {
        tracing::warn!(
            old = LEGACY_SESSIONS_GAP_CLOSE_SECS,
            new = default,
            "settings: retired session gap-close default remapped"
        );
        persist_sessions_gap_close_migration(store, default).await;
        default
    } else {
        // The custom value is already durable. Do not rewrite a stale read with the marker:
        // a concurrent explicit change must win rather than be clobbered by this load.
        mark_sessions_gap_close_migrated(store).await;
        stored
    }
}

/// Atomically persists the effective gap-close value and one-shot migration marker.
async fn persist_sessions_gap_close_migration(store: &dyn Store, value: u32) {
    if let Err(e) = store
        .set_settings_batch(&[
            ("sessions.gap_close_secs".to_string(), value.to_string()),
            (SESSIONS_GAP_CLOSE_MIGRATED_KEY.to_string(), "1".to_string()),
        ])
        .await
    {
        tracing::warn!(
            error = %e,
            "settings: failed to atomically persist session gap-close migration; retrying next load"
        );
    }
}

/// Best-effort latch for [`load_sessions_gap_close_secs`] (`04 §4`: load never errors).
async fn mark_sessions_gap_close_migrated(store: &dyn Store) {
    if let Err(e) = store
        .set_setting(SESSIONS_GAP_CLOSE_MIGRATED_KEY, "1")
        .await
    {
        tracing::warn!(
            error = %e,
            "settings: failed to persist session gap-close migration marker"
        );
    }
}

async fn boolean(store: &dyn Store, key: &str, default: bool) -> bool {
    match store.get_setting(key).await {
        Ok(Some(raw)) => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => {
                tracing::warn!(key, raw = %raw, "settings: unparsable bool; using default");
                default
            }
        },
        Ok(None) => default,
        Err(e) => {
            tracing::warn!(key, error = %e, "settings: read failed; using default");
            default
        }
    }
}

async fn json<T: DeserializeOwned>(store: &dyn Store, key: &str, default: T) -> T {
    match store.get_setting(key).await {
        Ok(Some(raw)) => match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(key, error = %e, "settings: unparsable JSON; using default");
                default
            }
        },
        Ok(None) => default,
        Err(e) => {
            tracing::warn!(key, error = %e, "settings: read failed; using default");
            default
        }
    }
}

/// Reads a `models.*_tier` setting, migrating the retired `beta` value to `quality`.
///
/// 0.3.0 retired the Beta tier (D3): a config persisted by an older version still holds
/// `"beta"` for a lane. Rather than let the generic [`json`] fallback silently drop it to
/// the field default (`Default`), this maps `beta` → `Quality`, **persists** the mapping,
/// and returns `Quality`. Persisting means the retired token leaves the DB, so the warn
/// logs **once** — the next load reads `"quality"` and this branch never fires again (the
/// same "log once" mechanism as [`drop_retired_settings`]). The write is best-effort: a
/// failure is logged and swallowed (load must never error — `04 §4`) and the next load
/// simply retries. Any *other* unparsable value falls back to `default` and is left
/// untouched, exactly like [`json`] — only the known-legacy `beta` token is migrated.
///
/// The remap lives here in the load path (not the startup-maintenance sweep like
/// [`drop_retired_settings`]) because the composition root builds the sidecars straight
/// from `load_settings`' output; a sweep-side remap would race it and the first
/// post-upgrade session would run `Default` instead of `Quality`.
async fn load_tier(store: &dyn Store, key: &str, default: ModelTier) -> ModelTier {
    let raw = match store.get_setting(key).await {
        Ok(Some(raw)) => raw,
        Ok(None) => return default,
        Err(e) => {
            tracing::warn!(key, error = %e, "settings: read failed; using default");
            return default;
        }
    };
    if let Ok(tier) = serde_json::from_str::<ModelTier>(&raw) {
        return tier;
    }
    if serde_json::from_str::<String>(&raw).is_ok_and(|s| s == "beta") {
        tracing::warn!(key, "settings: retired `beta` tier mapped to `quality`");
        let quality = serde_json::to_string(&ModelTier::Quality)
            .expect("ModelTier always serializes to JSON");
        if let Err(e) = store.set_setting(key, &quality).await {
            tracing::warn!(key, error = %e, "settings: failed to persist beta→quality mapping");
        }
        return ModelTier::Quality;
    }
    tracing::warn!(key, raw = %raw, "settings: unparsable tier; using default");
    default
}

/// The pre-remap Flow-overlay default, retired because it collided with Claude Desktop's
/// global quick-entry shortcut.
const LEGACY_OVERLAY_HOTKEY: &str = "Ctrl+Alt+Space";

/// Marker key latched once the one-shot overlay-hotkey remap has run. Its presence (any value)
/// means "the new-default code has already seen this install — honor `overlay.hotkey` verbatim
/// from here on." Without it the value-only remap re-fired on **every** load, so a user who
/// deliberately set `Ctrl+Alt+Space` back had it silently clobbered again on the next load,
/// breaking the reversible escape hatch (`07` #94 / CHANGELOG). Not part of [`Settings`], so
/// `save_settings` never touches it.
const OVERLAY_HOTKEY_MIGRATED_KEY: &str = "overlay.hotkey_migrated";

/// Reads `overlay.hotkey`, one-shot remapping the retired default `Ctrl+Alt+Space` to the
/// current default (`Ctrl+Alt+Z`) **exactly once per install**. The remap fires only on the
/// first load after upgrade — gated by [`OVERLAY_HOTKEY_MIGRATED_KEY`] — so a user who *later*
/// chooses `Ctrl+Alt+Space` on purpose keeps it (the reversible behavior recorded in `07` #94).
/// Only an **exact** match of the old default is migrated; any other chord is returned untouched.
/// The marker is latched on the first load when no rewrite is needed (fresh install / custom
/// chord), but when a legacy chord *is* rewritten the marker is latched **only after** that write
/// succeeds — so a failed rewrite is retried on the next load instead of being latched into a
/// permanently-honored stale value. All writes are best-effort (load must never error, `04 §4`).
///
/// Lives in the load path (not the startup-maintenance sweep) for the same reason as
/// [`load_tier`]: the composition root registers the overlay hotkey straight from
/// `load_settings`' output, so a sweep-side remap would race it and the first post-upgrade
/// session would still try to register the colliding chord.
async fn load_overlay_hotkey(store: &dyn Store, default: String) -> String {
    // Has the one-shot remap already run? Its marker means "honor the stored chord as-is",
    // which is what makes a deliberately re-chosen `Ctrl+Alt+Space` durable.
    let migrated = matches!(
        store.get_setting(OVERLAY_HOTKEY_MIGRATED_KEY).await,
        Ok(Some(_))
    );
    let raw = match store.get_setting("overlay.hotkey").await {
        Ok(Some(raw)) => raw,
        Ok(None) => {
            // Nothing stored (fresh install): use the default, and latch the marker so a later
            // deliberate `Ctrl+Alt+Space` is never auto-remapped.
            if !migrated {
                mark_overlay_hotkey_migrated(store).await;
            }
            return default;
        }
        Err(e) => {
            // Read error: fall back to default and retry the one-shot on a later load.
            tracing::warn!(key = "overlay.hotkey", error = %e, "settings: read failed; using default");
            return default;
        }
    };
    let stored = match serde_json::from_str::<String>(&raw) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(key = "overlay.hotkey", error = %e, "settings: unparsable JSON; using default");
            return default;
        }
    };
    // Migration already ran: honor the stored chord verbatim (including a deliberate legacy value).
    if migrated {
        return stored;
    }
    // First load under the new default: remap the retired chord if present, then latch the marker
    // so this can never fire again — but latch ONLY after any required rewrite has actually
    // landed. Latching a marker while the `overlay.hotkey` rewrite failed would honor the stale
    // `Ctrl+Alt+Space` forever (the next load sees `migrated` and returns it verbatim); leaving
    // the marker unset instead lets the next load retry the one-shot.
    if stored == LEGACY_OVERLAY_HOTKEY {
        tracing::warn!(
            old = LEGACY_OVERLAY_HOTKEY,
            new = %default,
            "settings: retired overlay hotkey remapped (old default collided with Claude Desktop)"
        );
        let encoded = serde_json::to_string(&default).expect("String always serializes to JSON");
        match store.set_setting("overlay.hotkey", &encoded).await {
            Ok(()) => mark_overlay_hotkey_migrated(store).await,
            Err(e) => tracing::warn!(
                error = %e,
                "settings: failed to persist overlay-hotkey remap; leaving migration un-latched to retry next load"
            ),
        }
        default
    } else {
        // No rewrite needed (a custom chord / new default already stored): safe to latch now so a
        // later deliberate `Ctrl+Alt+Space` is honored verbatim.
        mark_overlay_hotkey_migrated(store).await;
        stored
    }
}

/// Best-effort latch of the one-shot overlay-hotkey migration marker (`04 §4`: load never errors).
async fn mark_overlay_hotkey_migrated(store: &dyn Store) {
    if let Err(e) = store.set_setting(OVERLAY_HOTKEY_MIGRATED_KEY, "1").await {
        tracing::warn!(error = %e, "settings: failed to persist overlay-hotkey migration marker");
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_settings;
    use traits::Settings;

    #[test]
    fn recycle_rss_mb_clamps_explicit_but_keeps_auto_zero() {
        // 0 = automatic sentinel: must survive sanitization unchanged.
        let s = sanitize_settings(Settings {
            sidecar_recycle_rss_mb: 0,
            ..Settings::default()
        });
        assert_eq!(s.sidecar_recycle_rss_mb, 0, "0 stays auto");

        // Below the 8 GiB floor: clamped up (8192 = auto_recycle_ceiling's lo; clears the
        // ~6.8 GB vision warmup baseline so an explicit value can't cause a recycle loop).
        let s = sanitize_settings(Settings {
            sidecar_recycle_rss_mb: 100,
            ..Settings::default()
        });
        assert_eq!(s.sidecar_recycle_rss_mb, 8192);

        // Above 131_072 ceiling: clamped down.
        let s = sanitize_settings(Settings {
            sidecar_recycle_rss_mb: 999_999,
            ..Settings::default()
        });
        assert_eq!(s.sidecar_recycle_rss_mb, 131_072);
    }

    #[test]
    fn resume_min_dwell_secs_clamps_to_band() {
        // Below the 10 s floor: clamped up (a shorter "sustained" context is noise).
        let s = sanitize_settings(Settings {
            resume_min_dwell_secs: 0,
            ..Settings::default()
        });
        assert_eq!(s.resume_min_dwell_secs, 10);
        // Above the day ceiling: clamped down.
        let s = sanitize_settings(Settings {
            resume_min_dwell_secs: 999_999,
            ..Settings::default()
        });
        assert_eq!(s.resume_min_dwell_secs, 86_400);
        // In band: unchanged (the 120 s default).
        let s = sanitize_settings(Settings::default());
        assert_eq!(s.resume_min_dwell_secs, 120);
    }

    #[test]
    fn marks_hotkey_empty_falls_back_to_default() {
        let default = Settings::default().marks_hotkey;
        // Blank / whitespace-only → the default chord (never silently blank, D6).
        let s = sanitize_settings(Settings {
            marks_hotkey: "   ".to_string(),
            ..Settings::default()
        });
        assert_eq!(s.marks_hotkey, default);
        // A real chord is trimmed and kept.
        let s = sanitize_settings(Settings {
            marks_hotkey: "  Ctrl+Alt+K  ".to_string(),
            ..Settings::default()
        });
        assert_eq!(s.marks_hotkey, "Ctrl+Alt+K");
    }
}
