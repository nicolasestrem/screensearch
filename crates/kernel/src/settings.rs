//! Typed settings loading: assemble the strongly-typed [`Settings`] from the opaque
//! key/value `settings` table (`03 §8`), and the [`AdvancedConfig`] from `advanced.toml`.
//!
//! Settings v2 (0.4.5 D6) splits every `03 §8` key into one of three dispositions:
//!
//! - `UI` keys live in [`Settings`] and ride `get_settings`/`set_settings`. Each is read
//!   individually; a missing or unparsable value falls back to the corresponding
//!   [`Settings::default`] field (never an error - capture must be able to start on a
//!   fresh DB). Composite values (`capture.monitors`, `privacy.excluded_apps`, model
//!   tiers) are stored as JSON.
//! - `file-only` keys live in [`AdvancedConfig`], loaded **once** at startup from
//!   `advanced.toml` beside `screensearch.db` ([`load_advanced_config`]). There is one
//!   source of truth per value: none of them is a field of [`Settings`].
//! - `deleted` keys are purged from the `settings` table on startup
//!   ([`drop_retired_settings`]) and are named nowhere else except that list, its doc
//!   comments and the tests.

use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use traits::{AdvancedConfig, CaptureConfig, ModelTier, Result, Settings, Store};

/// Path of the advanced (file-only) configuration inside the app data directory.
pub fn advanced_config_path(data_dir: &Path) -> PathBuf {
    data_dir.join(traits::ADVANCED_CONFIG_FILE)
}

/// Reads every `UI`-disposition `03 §8` setting, falling back to [`Settings::default`]
/// per key.
pub async fn load_settings(store: &dyn Store) -> Settings {
    let d = Settings::default();
    sanitize_settings(Settings {
        app_run_at_startup: boolean(store, "app.run_at_startup", d.app_run_at_startup).await,
        app_close_to_tray: boolean(store, "app.close_to_tray", d.app_close_to_tray).await,
        overlay_hotkey: load_overlay_hotkey(store, d.overlay_hotkey).await,
        marks_hotkey: json(store, "marks.hotkey", d.marks_hotkey).await,
        ui_appearance: json(store, "ui.appearance", d.ui_appearance).await,
        capture_interval_ms: num(store, "capture.interval_ms", d.capture_interval_ms).await,
        capture_monitors: json(store, "capture.monitors", d.capture_monitors).await,
        privacy_excluded_apps: json(store, "privacy.excluded_apps", d.privacy_excluded_apps).await,
        privacy_pause_on_lock: boolean(store, "privacy.pause_on_lock", d.privacy_pause_on_lock)
            .await,
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
        retrieval_default_top_k: num(store, "retrieval.default_top_k", d.retrieval_default_top_k)
            .await,
        reports_daily_top_k: num(store, "reports.daily_top_k", d.reports_daily_top_k).await,
        sessions_min_len_secs: num(store, "sessions.min_len_secs", d.sessions_min_len_secs).await,
        sessions_gap_close_secs: load_sessions_gap_close_secs(store, d.sessions_gap_close_secs)
            .await,
        text_include_chrome_default: boolean(
            store,
            "text.include_chrome_default",
            d.text_include_chrome_default,
        )
        .await,
        answer_thinking: boolean(store, "answer.thinking", d.answer_thinking).await,
        models_vision_tier: load_tier(store, "models.vision_tier", d.models_vision_tier).await,
        models_answer_tier: load_tier(store, "models.answer_tier", d.models_answer_tier).await,
        storage_retention_days: num(store, "storage.retention_days", d.storage_retention_days)
            .await,
        storage_max_width: num(store, "storage.max_width", d.storage_max_width).await,
        throttle_enabled: boolean(store, "throttle.enabled", d.throttle_enabled).await,
        api_enabled: boolean(store, "api.enabled", d.api_enabled).await,
        api_port: num(store, "api.port", d.api_port).await,
        api_bind_address: raw_string(store, "api.bind_address", d.api_bind_address).await,
    })
}

/// Persists every `UI`-disposition `03 §8` setting back to the key/value `settings` table -
/// the exact inverse of [`load_settings`], using the **same key strings** so a saved value
/// round-trips. Numbers are written via `to_string`, bools as `"true"`/`"false"`, and
/// composite values (`capture.monitors`, `privacy.excluded_apps`, model tiers) as JSON
/// (matching how `set_model_tier` already writes a tier). `api.bind_address` is written as a
/// raw literal, matching `local_api`'s reader.
///
/// Every pair (including the fallible JSON encodings) is built **before** any write, then
/// committed in one transaction via [`Store::set_settings_batch`]. This is all-or-nothing: a
/// serialization error short-circuits with zero writes, and a crash mid-commit rolls back -
/// so [`load_settings`] never observes a mix of new and stale keys (its per-key default
/// fallback would hide such a split silently).
///
/// Backs the `set_settings` command (`03 §7`): the values are durable immediately; which
/// subsystems re-read them live vs on restart is documented in the Settings UI (model tiers
/// hot-apply; capture/storage/privacy on next capture start; the rest on app restart).
pub async fn save_settings(store: &dyn Store, s: &Settings) -> Result<()> {
    let s = sanitize_settings(s.clone());
    let kvs: Vec<(String, String)> = vec![
        (
            "app.run_at_startup".into(),
            bool_str(s.app_run_at_startup).into(),
        ),
        (
            "app.close_to_tray".into(),
            bool_str(s.app_close_to_tray).into(),
        ),
        (
            "overlay.hotkey".into(),
            serde_json::to_string(&s.overlay_hotkey)?,
        ),
        (
            "marks.hotkey".into(),
            serde_json::to_string(&s.marks_hotkey)?,
        ),
        (
            "ui.appearance".into(),
            serde_json::to_string(&s.ui_appearance)?,
        ),
        (
            "capture.interval_ms".into(),
            s.capture_interval_ms.to_string(),
        ),
        (
            "capture.monitors".into(),
            serde_json::to_string(&s.capture_monitors)?,
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
            "enrich.vision_timer_enabled".into(),
            bool_str(s.enrich_vision_timer_enabled).into(),
        ),
        (
            "enrich.vision_timer_interval_ms".into(),
            s.enrich_vision_timer_interval_ms.to_string(),
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
            "sessions.min_len_secs".into(),
            s.sessions_min_len_secs.to_string(),
        ),
        (
            "sessions.gap_close_secs".into(),
            s.sessions_gap_close_secs.to_string(),
        ),
        (
            "text.include_chrome_default".into(),
            bool_str(s.text_include_chrome_default).into(),
        ),
        ("answer.thinking".into(), bool_str(s.answer_thinking).into()),
        (
            "models.vision_tier".into(),
            serde_json::to_string(&s.models_vision_tier)?,
        ),
        (
            "models.answer_tier".into(),
            serde_json::to_string(&s.models_answer_tier)?,
        ),
        (
            "storage.retention_days".into(),
            s.storage_retention_days.to_string(),
        ),
        ("storage.max_width".into(), s.storage_max_width.to_string()),
        (
            "throttle.enabled".into(),
            bool_str(s.throttle_enabled).into(),
        ),
        ("api.enabled".into(), bool_str(s.api_enabled).into()),
        ("api.port".into(), s.api_port.to_string()),
        ("api.bind_address".into(), s.api_bind_address.clone()),
    ];
    store.set_settings_batch(&kvs).await
}

/// Backend-side bounds matching the Settings UI's save-time sanitizer. The UI is not a
/// trust boundary: persisted settings may be hand-edited, migrated from older builds, or
/// sent directly over IPC, so the kernel clamps before use/write.
pub fn sanitize_settings(mut s: Settings) -> Settings {
    s.capture_interval_ms = clamp_u32(s.capture_interval_ms, 250, 3_600_000);
    // 0 = native (no downscale) — the capture path keeps the frame at the monitor's full
    // width so ultra-wide captures stay legible. Any positive value is a real width ceiling
    // clamped to a sane band (mirrors the `0 = auto` sentinels for sidecar ctx/recycle).
    s.storage_max_width = if s.storage_max_width == 0 {
        0
    } else {
        clamp_u32(s.storage_max_width, 320, 7680)
    };
    s.storage_retention_days = clamp_u32(s.storage_retention_days, 0, 3650);
    s.enrich_vision_timer_interval_ms =
        clamp_u32(s.enrich_vision_timer_interval_ms, 60_000, 86_400_000);
    s.retrieval_default_top_k = clamp_u32(s.retrieval_default_top_k, 1, 100);
    s.reports_daily_top_k = clamp_u32(s.reports_daily_top_k, 1, 1_000);
    s.sessions_min_len_secs = clamp_u32(s.sessions_min_len_secs, 30, 3_600);
    s.sessions_gap_close_secs = clamp_u32(s.sessions_gap_close_secs, 60, 3_600);
    let default_overlay_hotkey = Settings::default().overlay_hotkey;
    s.overlay_hotkey = match s.overlay_hotkey.trim() {
        "" => default_overlay_hotkey,
        hotkey => hotkey.to_string(),
    };
    let default_marks_hotkey = Settings::default().marks_hotkey;
    s.marks_hotkey = match s.marks_hotkey.trim() {
        "" => default_marks_hotkey,
        hotkey => hotkey.to_string(),
    };
    // `03 §8`: the port clamp is 1024..=65535 (`u16` already caps the top); an empty bind
    // address means "missing" and falls back to loopback. A non-empty non-literal is left
    // alone here so the API surfaces it as a loud guided-change error rather than a silent
    // rewrite.
    s.api_port = s.api_port.max(1024);
    s.api_bind_address = match s.api_bind_address.trim() {
        "" => Settings::default().api_bind_address,
        address => address.to_string(),
    };
    s
}

/// The capture-relevant slice of [`Settings`] plus the `file-only` capture knobs
/// ([`AdvancedConfig`]), handed to the capture impl (`03 §8`).
pub fn capture_config(s: &Settings, advanced: &AdvancedConfig) -> CaptureConfig {
    CaptureConfig {
        interval_ms: s.capture_interval_ms,
        monitors: s.capture_monitors.clone(),
        diff_threshold: advanced.capture.diff_threshold,
        excluded_apps: s.privacy_excluded_apps.clone(),
        pause_on_lock: s.privacy_pause_on_lock,
        event_driven_enabled: advanced.capture.event_driven_enabled,
        event_on_foreground: advanced.capture.event_on_foreground,
        event_on_idle: advanced.capture.event_on_idle,
        event_debounce_ms: advanced.capture.event_debounce_ms,
        event_min_interval_ms: advanced.capture.event_min_interval_ms,
        event_idle_threshold_ms: advanced.capture.event_idle_threshold_ms,
        event_fallback_interval_ms: advanced.capture.event_fallback_interval_ms,
    }
}

/// Settings keys retired by past versions, purged from the DB on startup so a config
/// persisted with any of them still loads and the stale rows don't linger (`03 §8`).
///
/// The first nine are the settings-v2 `deleted` rows (0.4.5 D6): each has no live consumer
/// after the 0.4.5 changes, and no replacement key was promoted into the `UI` ceiling to
/// refill a slot. They were removed by earlier arcs - the four extra event triggers and the
/// typing-pause threshold by 0.3.0 PR2, the image-embedding lane toggle by 0.3.0 PR4, the
/// superseded API notice state by the API arc, and `storage.jpeg_quality` +
/// `capture.uia_run_on_interactive` by 0.3.2 PR5 (both provably inert). The last two are
/// sweep parameters the 2026-08-01 usage review demoted to named constants. This list is the
/// only place those key names remain, besides these comments and the retirement test.
pub const RETIRED_SETTINGS_KEYS: &[&str] = &[
    "api.network_notice_state",
    "capture.event_on_click",
    "capture.event_on_clipboard",
    "capture.event_on_scroll_stop",
    "capture.event_on_typing_pause",
    "capture.event_typing_pause_ms",
    "capture.uia_run_on_interactive",
    "enrich.image_embeddings",
    "storage.jpeg_quality",
    "sessions.focus_min_len_secs",
    "sessions.focus_min_density_fph",
];

/// Deletes any [`RETIRED_SETTINGS_KEYS`] left in the `settings` table by an older version,
/// logging **once** which keys were dropped. Called once at startup (before the rest of the
/// maintenance sweep). "Log once" falls out naturally - the keys are gone by the next launch,
/// so a subsequent run finds nothing and logs nothing. A read or delete error is logged and
/// swallowed: dropping stale keys is best-effort and must never block startup or capture
/// (`04 §4`). The reserved history/allocator keys (`03 §7f.8`) are not on this list and the
/// store's generic delete refuses them regardless.
pub async fn drop_retired_settings(store: &dyn Store) {
    match store.delete_settings(RETIRED_SETTINGS_KEYS).await {
        Ok(dropped) if !dropped.is_empty() => {
            tracing::warn!(keys = ?dropped, "settings: dropped retired keys");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "settings: failed to drop retired keys"),
    }
}

/// One `[section]` of `advanced.toml`: pulls the known fields out of the table, dropping
/// (and flagging) anything unknown or unparsable, and records the `section.field` name of
/// every value that actually parsed from the file.
macro_rules! advanced_section {
    ($table:ident, $dirty:ident, $from_file:ident, $name:literal, $default:expr, {
        $($field:ident => $key:literal),* $(,)?
    }) => {{
        let mut section = take_advanced_section(&mut $table, $name, &mut $dirty);
        let mut out = $default;
        $(
            if let Some(value) = section.remove($key) {
                match value.try_into() {
                    Ok(parsed) => {
                        out.$field = parsed;
                        $from_file.push(concat!($name, ".", $key).to_string());
                    }
                    Err(error) => {
                        tracing::warn!(
                            section = $name,
                            field = $key,
                            error = %error,
                            "advanced.toml: unparsable value; using default",
                        );
                        $dirty = true;
                    }
                }
            }
        )*
        if !section.is_empty() {
            let unknown: Vec<&String> = section.keys().collect();
            tracing::warn!(
                section = $name,
                fields = ?unknown,
                "advanced.toml: dropping unknown fields",
            );
            $dirty = true;
        }
        out
    }};
}

/// One [`load_advanced_config_with_provenance`] result: the effective config plus the
/// provenance the read-only Settings · Advanced projection reports
/// ([`traits::AdvancedConfigView`], `03 §8`, 0.4.5 D6).
#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedConfigLoad {
    /// The effective configuration, clamped by [`AdvancedConfig::sanitized`].
    pub config: AdvancedConfig,
    /// `section.field` keys whose value was parsed from the file, e.g.
    /// `overlay.max_results`. A key absent from this list still holds its compiled
    /// default. A listed value may have been clamped into its `03 §8` range.
    pub from_file: Vec<String>,
}

/// Parses `advanced.toml` once at startup into the typed [`AdvancedConfig`] (`03 §8`,
/// 0.4.5 D6). The config without its provenance, for callers that do not report it.
pub fn load_advanced_config(data_dir: &Path) -> AdvancedConfig {
    load_advanced_config_with_provenance(data_dir).config
}

/// Parses `advanced.toml` once at startup: [`load_advanced_config`] plus which
/// `section.field` keys the file actually supplied.
///
/// Missing file: defaults apply and the reason is logged (the honest degraded path P7
/// shows under Settings · Advanced). Unreadable or invalid TOML: defaults apply, the reason
/// is logged, and the file is left untouched so a typo is never destroyed. In every one of
/// those cases `from_file` is empty - nothing came from the file.
///
/// Strict in the same way as the `settings` table: an unknown section or field, or a field
/// whose value does not parse, is dropped on a one-shot rewrite, so the file always matches
/// the table afterwards. Values are clamped by [`AdvancedConfig::sanitized`]; `from_file`
/// is recorded at parse time, so the one-shot rewrite that follows never makes a default
/// look like a file value.
pub fn load_advanced_config_with_provenance(data_dir: &Path) -> AdvancedConfigLoad {
    let path = advanced_config_path(data_dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(
                path = %path.display(),
                "advanced.toml absent; file-only defaults apply",
            );
            return AdvancedConfigLoad {
                config: AdvancedConfig::default(),
                from_file: Vec::new(),
            };
        }
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "advanced.toml unreadable; file-only defaults apply",
            );
            return AdvancedConfigLoad {
                config: AdvancedConfig::default(),
                from_file: Vec::new(),
            };
        }
    };
    let mut table: toml::Table = match toml::from_str(&raw) {
        Ok(table) => table,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "advanced.toml is not valid TOML; file-only defaults apply (file left untouched)",
            );
            return AdvancedConfigLoad {
                config: AdvancedConfig::default(),
                from_file: Vec::new(),
            };
        }
    };

    let mut dirty = false;
    let mut from_file: Vec<String> = Vec::new();
    let defaults = AdvancedConfig::default();
    let capture = advanced_section!(table, dirty, from_file, "capture", defaults.capture.clone(), {
        diff_threshold => "diff_threshold",
        event_debounce_ms => "event_debounce_ms",
        event_driven_enabled => "event_driven_enabled",
        event_fallback_interval_ms => "event_fallback_interval_ms",
        event_idle_threshold_ms => "event_idle_threshold_ms",
        event_min_interval_ms => "event_min_interval_ms",
        event_on_foreground => "event_on_foreground",
        event_on_idle => "event_on_idle",
        uia_latency_budget_ms => "uia_latency_budget_ms",
        uia_max_nodes => "uia_max_nodes",
        uia_max_textpattern_calls => "uia_max_textpattern_calls",
        uia_min_text_chars => "uia_min_text_chars",
        uia_suppress_during_input_ms => "uia_suppress_during_input_ms",
        uia_text_enabled => "uia_text_enabled",
        uia_view_control_only => "uia_view_control_only",
    });
    let enrich = advanced_section!(table, dirty, from_file, "enrich", defaults.enrich.clone(), {
        embed_text => "embed_text",
        vision_batch_size => "vision_batch_size",
        vision_idle_enabled => "vision_idle_enabled",
        vision_idle_secs => "vision_idle_secs",
        worker_concurrency => "worker_concurrency",
    });
    let overlay = advanced_section!(table, dirty, from_file, "overlay", defaults.overlay.clone(), {
        max_results => "max_results",
    });
    let reports = advanced_section!(table, dirty, from_file, "reports", defaults.reports.clone(), {
        map_reduce_min_frames => "map_reduce_min_frames",
        weekly_top_k => "weekly_top_k",
    });
    let resume = advanced_section!(table, dirty, from_file, "resume", defaults.resume.clone(), {
        min_dwell_secs => "min_dwell_secs",
    });
    let sidecar = advanced_section!(table, dirty, from_file, "sidecar", defaults.sidecar.clone(), {
        ctx_size => "ctx_size",
        device => "device",
        flash_attn => "flash_attn",
        idle_ttl_secs => "idle_ttl_secs",
        kv_cache_type => "kv_cache_type",
        ngl => "ngl",
        recycle_enabled => "recycle_enabled",
        recycle_rss_mb => "recycle_rss_mb",
    });
    let text = advanced_section!(table, dirty, from_file, "text", defaults.text.clone(), {
        chrome_protect_min_chars => "chrome_protect_min_chars",
        chrome_region_buckets => "chrome_region_buckets",
        chrome_suppress_min_seen => "chrome_suppress_min_seen",
    });
    let throttle = advanced_section!(table, dirty, from_file, "throttle", defaults.throttle.clone(), {
        cpu_enter_pct => "cpu_enter_pct",
        cpu_exit_pct => "cpu_exit_pct",
        embed_text_floor => "embed_text_floor",
        enter_after_ms => "enter_after_ms",
        exit_after_ms => "exit_after_ms",
        gpu_enter_pct => "gpu_enter_pct",
        gpu_exit_pct => "gpu_exit_pct",
        sample_interval_ms => "sample_interval_ms",
    });

    if !table.is_empty() {
        let unknown: Vec<&String> = table.keys().collect();
        tracing::warn!(
            sections = ?unknown,
            "advanced.toml: dropping unknown sections",
        );
        dirty = true;
    }

    let config = AdvancedConfig {
        capture,
        enrich,
        overlay,
        reports,
        resume,
        sidecar,
        text,
        throttle,
    }
    .sanitized();
    if dirty {
        rewrite_advanced_config(&path, &config);
    }
    AdvancedConfigLoad { config, from_file }
}

/// Removes one section table, flagging a non-table value for the rewrite.
fn take_advanced_section(table: &mut toml::Table, name: &str, dirty: &mut bool) -> toml::Table {
    match table.remove(name) {
        None => toml::Table::new(),
        Some(toml::Value::Table(section)) => section,
        Some(_) => {
            tracing::warn!(
                section = name,
                "advanced.toml: section is not a table; using defaults",
            );
            *dirty = true;
            toml::Table::new()
        }
    }
}

/// Best-effort one-shot rewrite of `advanced.toml` from the effective config, so the file
/// matches the `03 §8` table after unknown/unparsable entries were dropped.
///
/// Written atomically: the body goes to a sibling temp file which then replaces the
/// original. A crash or kill mid-write leaves either the old file or the new one, never a
/// truncated file that the loader would read as invalid TOML and silently replace with
/// defaults on every later start.
fn rewrite_advanced_config(path: &Path, config: &AdvancedConfig) {
    let body = match toml::to_string_pretty(config) {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(error = %error, "advanced.toml: failed to serialize the rewrite");
            return;
        }
    };
    let tmp = path.with_extension("toml.tmp");
    let written = std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, path));
    match written {
        Ok(()) => tracing::warn!(
            path = %path.display(),
            "advanced.toml rewritten to match the settings table",
        ),
        Err(error) => {
            let _ = std::fs::remove_file(&tmp);
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "advanced.toml rewrite failed; the file is unchanged and the in-memory values still apply",
            );
        }
    }
}

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

/// The pre-retune micro-gap default selected from the original two-day sample.
const LEGACY_SESSIONS_GAP_CLOSE_SECS: u32 = 300;

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

/// Reads a plain (non-JSON) string setting. `api.bind_address` is written raw by both this
/// module and `local_api`, so it cannot use [`json`]. A blank value means "missing" and maps
/// to the default.
async fn raw_string(store: &dyn Store, key: &str, default: String) -> String {
    match store.get_setting(key).await {
        Ok(Some(raw)) => match raw.trim() {
            "" => default,
            value => value.to_string(),
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

/// The canonical string form for a persisted bool (parsed back by [`boolean`]).
fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

fn clamp_u32(value: u32, min: u32, max: u32) -> u32 {
    value.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::sanitize_settings;
    use traits::Settings;

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

    #[test]
    fn api_port_floors_at_1024_and_bind_address_blank_means_loopback() {
        let s = sanitize_settings(Settings {
            api_port: 80,
            api_bind_address: "   ".to_string(),
            ..Settings::default()
        });
        assert_eq!(s.api_port, 1024);
        assert_eq!(s.api_bind_address, "127.0.0.1");
        // A literal is trimmed and kept.
        let s = sanitize_settings(Settings {
            api_port: 43_210,
            api_bind_address: "  0.0.0.0  ".to_string(),
            ..Settings::default()
        });
        assert_eq!(s.api_port, 43_210);
        assert_eq!(s.api_bind_address, "0.0.0.0");
    }
}
