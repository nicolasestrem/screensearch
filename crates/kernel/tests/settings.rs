//! Settings persistence (`03 §8`, settings v2 / 0.4.5 D6).
//!
//! Two surfaces are covered:
//!
//! - the `UI` key set: `save_settings` followed by `load_settings` returns exactly what was
//!   written (this guards the key-string contract between the two — a typo in either would
//!   silently fall back to a default and never round-trip), retired keys are rewritten away,
//!   and the reserved history/allocator metadata survives every settings path;
//! - the `file-only` surface: `advanced.toml` parses into [`AdvancedConfig`], unknown
//!   sections/fields are dropped on a one-shot rewrite, a missing/unreadable/invalid file
//!   applies defaults without error, and the values reach the same consumers the typed
//!   fields used to reach.

use kernel::settings::{load_advanced_config, load_settings, save_settings};
use std::path::Path;
use store::SqliteStore;
use traits::{
    AdvancedConfig, CaptureAdvanced, EnrichAdvanced, FlashAttnSetting, KvCacheType, ModelTier,
    OverlayAdvanced, ReportsAdvanced, ResumeAdvanced, Settings, SidecarAdvanced, SidecarParams,
    Store, TextAdvanced, ThrottleAdvanced, FRAME_ID_HIGH_WATER_KEY, HISTORY_ID_KEY,
};

#[tokio::test]
async fn round_trips_defaults() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let original = Settings::default();

    save_settings(dyn_store, &original)
        .await
        .expect("save settings");
    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded, original, "defaults must round-trip");
}

#[tokio::test]
async fn round_trips_non_default_values() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    // Every field set away from its default, including composites (monitors,
    // excluded apps, model tiers) so the JSON encodings are exercised too.
    let original = Settings {
        app_run_at_startup: true,
        app_close_to_tray: false,
        overlay_hotkey: "Ctrl+Shift+F".to_string(),
        marks_hotkey: "Ctrl+Shift+M".to_string(),
        capture_interval_ms: 5000,
        capture_monitors: vec![0, 2],
        privacy_excluded_apps: vec!["Signal".to_string(), "Element".to_string()],
        privacy_pause_on_lock: false,
        enrich_vision_timer_enabled: true,
        enrich_vision_timer_interval_ms: 1_800_000,
        retrieval_default_top_k: 16,
        reports_daily_top_k: 60,
        sessions_min_len_secs: 240,
        sessions_gap_close_secs: 600,
        text_include_chrome_default: true,
        answer_thinking: false,
        models_vision_tier: ModelTier::Quality,
        models_answer_tier: ModelTier::Quality,
        storage_retention_days: 7,
        storage_max_width: 1600,
        throttle_enabled: true,
        api_enabled: true,
        api_port: 43_211,
        api_bind_address: "0.0.0.0".to_string(),
    };

    save_settings(dyn_store, &original)
        .await
        .expect("save settings");
    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded, original, "non-default values must round-trip");
}

#[tokio::test]
async fn load_settings_sanitizes_persisted_numeric_values() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    store.set_setting("capture.interval_ms", "1").await.unwrap();
    store
        .set_setting("storage.max_width", "100000")
        .await
        .unwrap();
    store
        .set_setting("storage.retention_days", "999999")
        .await
        .unwrap();
    store
        .set_setting("enrich.vision_timer_interval_ms", "1")
        .await
        .unwrap();
    store
        .set_setting("retrieval.default_top_k", "0")
        .await
        .unwrap();
    store
        .set_setting("reports.daily_top_k", "999999")
        .await
        .unwrap();
    store
        .set_setting("overlay.hotkey", "\"   \"")
        .await
        .unwrap();
    store.set_setting("api.port", "80").await.unwrap();

    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded.capture_interval_ms, 250);
    assert_eq!(loaded.storage_max_width, 7680);
    assert_eq!(loaded.storage_retention_days, 3650);
    assert_eq!(loaded.enrich_vision_timer_interval_ms, 60_000);
    assert_eq!(loaded.retrieval_default_top_k, 1);
    assert_eq!(loaded.reports_daily_top_k, 1_000);
    assert_eq!(loaded.overlay_hotkey, Settings::default().overlay_hotkey);
    assert_eq!(loaded.api_port, 1024);
}

#[tokio::test]
async fn save_settings_persists_sanitized_numeric_values() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let original = Settings {
        capture_interval_ms: 1,
        storage_max_width: 100_000,
        enrich_vision_timer_interval_ms: 1,
        overlay_hotkey: "  Ctrl+Shift+F  ".to_string(),
        retrieval_default_top_k: 0,
        api_port: 1,
        ..Settings::default()
    };

    save_settings(dyn_store, &original)
        .await
        .expect("save settings");
    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded.capture_interval_ms, 250);
    assert_eq!(loaded.storage_max_width, 7680);
    assert_eq!(loaded.enrich_vision_timer_interval_ms, 60_000);
    assert_eq!(loaded.overlay_hotkey, "Ctrl+Shift+F");
    assert_eq!(loaded.retrieval_default_top_k, 1);
    assert_eq!(loaded.api_port, 1024);
}

/// A persisted `report_runs.config_snapshot` written before settings v2 carries the old
/// field set: no `api_*` keys, plus the now `file-only`/`deleted` names. It must still
/// deserialize (defaults for the three new keys, unknown names ignored), or a queued run
/// from an older build would fail with `STORE_ERROR` (`03 §8c`).
#[test]
fn pre_v2_settings_json_still_deserializes() {
    let mut value = serde_json::to_value(Settings::default()).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("api_enabled");
    object.remove("api_port");
    object.remove("api_bind_address");
    object.insert("sidecar_ngl".to_string(), 42.into());
    object.insert("storage.jpeg_quality".to_string(), 80.into());

    let parsed: Settings = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, Settings::default());
}

#[tokio::test]
async fn api_settings_round_trip_as_plain_values() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let settings = Settings {
        api_enabled: true,
        api_port: 43_210,
        api_bind_address: "::1".to_string(),
        ..Settings::default()
    };

    save_settings(dyn_store, &settings).await.expect("save");
    let loaded = load_settings(dyn_store).await;
    assert!(loaded.api_enabled);
    assert_eq!(loaded.api_port, 43_210);
    assert_eq!(loaded.api_bind_address, "::1");

    // Stored raw, exactly as `local_api` writes and reads it (never JSON-quoted), so the
    // HTTP surface and the typed struct cannot disagree about the literal.
    assert_eq!(
        store
            .get_setting("api.bind_address")
            .await
            .unwrap()
            .as_deref(),
        Some("::1")
    );
    assert_eq!(
        store.get_setting("api.enabled").await.unwrap().as_deref(),
        Some("true")
    );
}

#[tokio::test]
async fn overlay_hotkey_empty_string_resets_to_default() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let settings = Settings {
        overlay_hotkey: "   ".to_string(),
        ..Settings::default()
    };

    save_settings(dyn_store, &settings)
        .await
        .expect("save settings");
    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded.overlay_hotkey, Settings::default().overlay_hotkey);
    let expected_default = serde_json::to_string(&Settings::default().overlay_hotkey).unwrap();
    assert_eq!(
        store
            .get_setting("overlay.hotkey")
            .await
            .unwrap()
            .as_deref(),
        Some(expected_default.as_str())
    );
}

#[tokio::test]
async fn overlay_hotkey_legacy_default_remaps_once() {
    // An install upgraded from before the default changed still holds "Ctrl+Alt+Space" (which
    // collided with Claude Desktop). Loading must remap it once to the current default, persist
    // that, and never re-enter the remap branch afterwards.
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    store
        .set_setting("overlay.hotkey", "\"Ctrl+Alt+Space\"")
        .await
        .unwrap();

    let loaded = load_settings(dyn_store).await;
    assert_eq!(loaded.overlay_hotkey, Settings::default().overlay_hotkey);
    // The remap is persisted, so the stored row now holds the new default…
    let expected_default = serde_json::to_string(&Settings::default().overlay_hotkey).unwrap();
    assert_eq!(
        store
            .get_setting("overlay.hotkey")
            .await
            .unwrap()
            .as_deref(),
        Some(expected_default.as_str())
    );
    // …and a second load returns it without the legacy value ever reappearing.
    assert_eq!(
        load_settings(dyn_store).await.overlay_hotkey,
        Settings::default().overlay_hotkey
    );
}

#[tokio::test]
async fn overlay_hotkey_deliberate_legacy_survives_after_migration() {
    // The reversible escape hatch (`07` #94): after the one-shot remap has run, a user who
    // *deliberately* sets the old chord back must keep it.
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    store
        .set_setting("overlay.hotkey", "\"Ctrl+Alt+Space\"")
        .await
        .unwrap();
    assert_eq!(
        load_settings(dyn_store).await.overlay_hotkey,
        Settings::default().overlay_hotkey
    );

    store
        .set_setting("overlay.hotkey", "\"Ctrl+Alt+Space\"")
        .await
        .unwrap();

    assert_eq!(
        load_settings(dyn_store).await.overlay_hotkey,
        "Ctrl+Alt+Space"
    );
    assert_eq!(
        store
            .get_setting("overlay.hotkey")
            .await
            .unwrap()
            .as_deref(),
        Some("\"Ctrl+Alt+Space\""),
        "the deliberately re-chosen legacy chord is left untouched in the DB"
    );
}

#[tokio::test]
async fn overlay_hotkey_custom_value_survives() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    store
        .set_setting("overlay.hotkey", "\"Ctrl+Shift+P\"")
        .await
        .unwrap();

    let loaded = load_settings(dyn_store).await;
    assert_eq!(loaded.overlay_hotkey, "Ctrl+Shift+P");
}

/// A `Store` decorator that forwards to an inner `SqliteStore` but fails `set_setting` for one
/// key (or every batch). The real in-memory store never fails a write, so this is the only way
/// to drive the one-shot migration "write failed" branches.
struct FailSetSettingFor {
    inner: SqliteStore,
    fail_key: &'static str,
}

#[async_trait::async_trait]
impl Store for FailSetSettingFor {
    async fn set_setting(&self, key: &str, value: &str) -> traits::Result<()> {
        if key == self.fail_key {
            return Err(anyhow::anyhow!("injected set_setting failure for {key}"));
        }
        self.inner.set_setting(key, value).await
    }
    async fn set_settings_batch(&self, kvs: &[(String, String)]) -> traits::Result<()> {
        if self.fail_key == "__batch__" {
            return Err(anyhow::anyhow!("injected atomic settings batch failure"));
        }
        if let Some((key, _)) = kvs.iter().find(|(key, _)| key == self.fail_key) {
            return Err(anyhow::anyhow!(
                "injected set_settings_batch failure for {key}"
            ));
        }
        self.inner.set_settings_batch(kvs).await
    }
    async fn get_setting(&self, key: &str) -> traits::Result<Option<String>> {
        self.inner.get_setting(key).await
    }
    async fn insert_frame(&self, f: traits::NewFrame) -> traits::Result<i64> {
        self.inner.insert_frame(f).await
    }
    async fn insert_ocr(&self, frame_id: i64, ocr: traits::OcrResult) -> traits::Result<()> {
        self.inner.insert_ocr(frame_id, ocr).await
    }
    async fn insert_vision(&self, frame_id: i64, v: traits::VisionAnalysis) -> traits::Result<()> {
        self.inner.insert_vision(frame_id, v).await
    }
    async fn upsert_text_embedding(
        &self,
        frame_id: i64,
        chunk_index: i32,
        chunk_text: &str,
        source: traits::ChunkSource,
        emb: &traits::Embedding,
        model: &str,
    ) -> traits::Result<()> {
        self.inner
            .upsert_text_embedding(frame_id, chunk_index, chunk_text, source, emb, model)
            .await
    }
    async fn hybrid_search(
        &self,
        q: &traits::SearchQuery,
    ) -> traits::Result<Vec<traits::SearchHit>> {
        self.inner.hybrid_search(q).await
    }
    async fn get_enrichment_input(
        &self,
        frame_id: i64,
    ) -> traits::Result<Option<traits::FrameEnrichmentInput>> {
        self.inner.get_enrichment_input(frame_id).await
    }
    async fn enqueue_job(&self, job: traits::NewJob) -> traits::Result<i64> {
        self.inner.enqueue_job(job).await
    }
    async fn claim_jobs(
        &self,
        kinds: &[traits::JobKind],
        limit: u32,
        now: i64,
    ) -> traits::Result<Vec<traits::Job>> {
        self.inner.claim_jobs(kinds, limit, now).await
    }
    async fn complete_job(&self, id: i64) -> traits::Result<()> {
        self.inner.complete_job(id).await
    }
    async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> traits::Result<()> {
        self.inner.fail_job(id, err, retry_at).await
    }
    async fn job_stats(&self) -> traits::Result<traits::JobStats> {
        self.inner.job_stats().await
    }
    async fn reset_stale_running_jobs(&self, older_than_ms: i64) -> traits::Result<u64> {
        self.inner.reset_stale_running_jobs(older_than_ms).await
    }
    async fn delete_settings(&self, keys: &[&str]) -> traits::Result<Vec<String>> {
        self.inner.delete_settings(keys).await
    }
}

#[tokio::test]
async fn overlay_hotkey_failed_remap_is_retried_not_latched() {
    // If the remap rewrite fails, the migration marker must NOT latch — otherwise the next load
    // sees `migrated` and honors the stale `Ctrl+Alt+Space` forever.
    let inner = SqliteStore::open_in_memory().expect("open in-memory store");
    inner
        .set_setting("overlay.hotkey", "\"Ctrl+Alt+Space\"")
        .await
        .unwrap();
    let failing = FailSetSettingFor {
        inner,
        fail_key: "overlay.hotkey",
    };

    let dyn_failing: &dyn Store = &failing;
    assert_eq!(
        load_settings(dyn_failing).await.overlay_hotkey,
        Settings::default().overlay_hotkey
    );
    assert_eq!(
        failing
            .inner
            .get_setting("overlay.hotkey")
            .await
            .unwrap()
            .as_deref(),
        Some("\"Ctrl+Alt+Space\""),
        "a failed remap must not rewrite the stored chord"
    );
    assert!(
        failing
            .inner
            .get_setting("overlay.hotkey_migrated")
            .await
            .unwrap()
            .is_none(),
        "a failed remap must not latch the one-shot marker"
    );

    // Recovery: a load against the now-healthy store completes the one-shot and latches.
    let dyn_ok: &dyn Store = &failing.inner;
    assert_eq!(
        load_settings(dyn_ok).await.overlay_hotkey,
        Settings::default().overlay_hotkey
    );
    let expected_default = serde_json::to_string(&Settings::default().overlay_hotkey).unwrap();
    assert_eq!(
        failing
            .inner
            .get_setting("overlay.hotkey")
            .await
            .unwrap()
            .as_deref(),
        Some(expected_default.as_str())
    );
}

#[tokio::test]
async fn load_drops_retired_keys_without_error() {
    // A config persisted by an older version still carries retired keys. `drop_retired_settings`
    // purges every one of them (so the row doesn't linger) and load must not error or be
    // perturbed by them. This is the settings-v2 rewrite: the nine `deleted` rows of `03 §8`
    // plus the two constant-candidate sweep parameters retired earlier.
    use kernel::settings::{drop_retired_settings, RETIRED_SETTINGS_KEYS};

    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    save_settings(dyn_store, &Settings::default())
        .await
        .expect("seed defaults");
    for key in RETIRED_SETTINGS_KEYS {
        store.set_setting(key, "true").await.unwrap();
    }
    // Overwrite the numeric retirees with the value shapes old installs persisted, so the
    // tolerance is proven against realistic data too.
    store
        .set_setting("storage.jpeg_quality", "80")
        .await
        .unwrap();
    store
        .set_setting("sessions.focus_min_len_secs", "600")
        .await
        .unwrap();
    store
        .set_setting("sessions.focus_min_density_fph", "90")
        .await
        .unwrap();

    drop_retired_settings(dyn_store).await;

    for key in RETIRED_SETTINGS_KEYS {
        assert_eq!(
            store.get_setting(key).await.unwrap(),
            None,
            "retired key {key} must be dropped"
        );
    }
    // A surviving key is untouched, and load still yields the defaults (no error).
    assert!(store
        .get_setting("capture.interval_ms")
        .await
        .unwrap()
        .is_some());
    assert_eq!(load_settings(dyn_store).await, Settings::default());

    // Idempotent: a second run finds nothing to drop (so it logs nothing — "once").
    drop_retired_settings(dyn_store).await;
}

#[tokio::test]
async fn save_settings_never_writes_retired_keys() {
    // The save path must not resurrect a retired key: after saving defaults, none of the
    // retired keys exist. This pins `save_settings`' key set against the retired list.
    use kernel::settings::RETIRED_SETTINGS_KEYS;

    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    save_settings(dyn_store, &Settings::default())
        .await
        .expect("save settings");

    for key in RETIRED_SETTINGS_KEYS {
        assert_eq!(
            store.get_setting(key).await.unwrap(),
            None,
            "save_settings must not write retired key {key}"
        );
    }
}

#[tokio::test]
async fn reserved_history_metadata_survives_every_settings_path() {
    // `03 §7f.8`: the history identity and the frame allocator high-water are reserved
    // metadata. The settings loader/saver, the retired-key purge and a reset-to-default save
    // must never touch them (the store's generic seams refuse them outright; this pins the
    // kernel-side paths on top of that).
    use kernel::settings::drop_retired_settings;

    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let history = store.get_setting(HISTORY_ID_KEY).await.unwrap();
    let allocator = store.get_setting(FRAME_ID_HIGH_WATER_KEY).await.unwrap();
    assert!(
        history.is_some(),
        "opening a store initializes history identity"
    );
    assert!(
        allocator.is_some(),
        "opening a store initializes the allocator"
    );

    save_settings(dyn_store, &Settings::default())
        .await
        .expect("save defaults");
    load_settings(dyn_store).await;
    drop_retired_settings(dyn_store).await;
    save_settings(dyn_store, &Settings::default())
        .await
        .expect("save defaults again");

    assert_eq!(store.get_setting(HISTORY_ID_KEY).await.unwrap(), history);
    assert_eq!(
        store.get_setting(FRAME_ID_HIGH_WATER_KEY).await.unwrap(),
        allocator
    );
}

#[tokio::test]
async fn legacy_session_gap_close_default_migrates_once() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    store
        .set_setting("sessions.gap_close_secs", "300")
        .await
        .unwrap();

    let loaded = load_settings(dyn_store).await;
    assert_eq!(loaded.sessions_gap_close_secs, 120);
    assert_eq!(
        store
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("120"),
        "the old persisted default must be rewritten"
    );
    assert!(
        store
            .get_setting("sessions.gap_close_secs_migrated")
            .await
            .unwrap()
            .is_some(),
        "a successful remap must latch the one-shot marker"
    );

    // After the one-shot, 300 is a deliberate user choice and must survive future loads.
    store
        .set_setting("sessions.gap_close_secs", "300")
        .await
        .unwrap();
    assert_eq!(load_settings(dyn_store).await.sessions_gap_close_secs, 300);
}

#[tokio::test]
async fn fresh_session_gap_close_default_latches_value_and_marker() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");

    assert_eq!(
        load_settings(&store).await.sessions_gap_close_secs,
        Settings::default().sessions_gap_close_secs
    );
    assert_eq!(
        store
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("120"),
        "a fresh install must persist the default alongside its migration marker"
    );
    assert!(
        store
            .get_setting("sessions.gap_close_secs_migrated")
            .await
            .unwrap()
            .is_some(),
        "a fresh install must latch the one-shot marker with its default"
    );
}

#[tokio::test]
async fn failed_session_gap_close_remap_is_retried_not_latched() {
    let inner = SqliteStore::open_in_memory().expect("open in-memory store");
    inner
        .set_setting("sessions.gap_close_secs", "300")
        .await
        .unwrap();
    let failing = FailSetSettingFor {
        inner,
        fail_key: "sessions.gap_close_secs",
    };

    assert_eq!(
        load_settings(&failing).await.sessions_gap_close_secs,
        Settings::default().sessions_gap_close_secs
    );
    assert_eq!(
        failing
            .inner
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("300")
    );
    assert!(
        failing
            .inner
            .get_setting("sessions.gap_close_secs_migrated")
            .await
            .unwrap()
            .is_none(),
        "a failed remap must remain unlatched for retry"
    );

    assert_eq!(
        load_settings(&failing.inner).await.sessions_gap_close_secs,
        Settings::default().sessions_gap_close_secs
    );
    assert_eq!(
        failing
            .inner
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("120")
    );
}

#[tokio::test]
async fn session_gap_close_remap_batch_failure_leaves_no_partial_migration() {
    let inner = SqliteStore::open_in_memory().expect("open in-memory store");
    inner
        .set_setting("sessions.gap_close_secs", "300")
        .await
        .unwrap();
    let failing = FailSetSettingFor {
        inner,
        fail_key: "__batch__",
    };

    assert_eq!(
        load_settings(&failing).await.sessions_gap_close_secs,
        Settings::default().sessions_gap_close_secs
    );
    assert_eq!(
        failing
            .inner
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("300"),
        "an atomic migration failure must leave the legacy value untouched"
    );
    assert!(
        failing
            .inner
            .get_setting("sessions.gap_close_secs_migrated")
            .await
            .unwrap()
            .is_none(),
        "an atomic migration failure must leave the marker absent"
    );
}

#[tokio::test]
async fn fresh_session_gap_close_batch_failure_leaves_no_partial_migration() {
    let failing = FailSetSettingFor {
        inner: SqliteStore::open_in_memory().expect("open in-memory store"),
        fail_key: "__batch__",
    };

    assert_eq!(
        load_settings(&failing).await.sessions_gap_close_secs,
        Settings::default().sessions_gap_close_secs
    );
    assert!(
        failing
            .inner
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .is_none(),
        "a failed fresh-install batch must not persist the default alone"
    );
    assert!(
        failing
            .inner
            .get_setting("sessions.gap_close_secs_migrated")
            .await
            .unwrap()
            .is_none(),
        "a failed fresh-install batch must not latch the marker alone"
    );
}

#[tokio::test]
async fn custom_session_gap_close_survives_default_retune() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    store
        .set_setting("sessions.gap_close_secs", "600")
        .await
        .unwrap();

    assert_eq!(load_settings(dyn_store).await.sessions_gap_close_secs, 600);
    assert_eq!(
        store
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("600"),
        "a non-default persisted choice must not be rewritten"
    );
    assert!(
        store
            .get_setting("sessions.gap_close_secs_migrated")
            .await
            .unwrap()
            .is_some(),
        "a custom persisted choice must atomically latch its migration state"
    );
}

#[tokio::test]
async fn persisted_beta_tier_remaps_to_quality_and_persists() {
    // 0.3.0 PR3 (D3): the Beta tier is retired. A config persisted by an older version
    // still carries `"beta"` for either lane; load must map it to Quality (never fall
    // back to Default) and persist the mapping — so the retired token leaves the DB and
    // a later load is clean ("logged once", the `drop_retired_settings` mechanism).
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    store
        .set_setting("models.vision_tier", "\"beta\"")
        .await
        .unwrap();
    store
        .set_setting("models.answer_tier", "\"beta\"")
        .await
        .unwrap();

    let loaded = load_settings(dyn_store).await;
    assert_eq!(loaded.models_vision_tier, ModelTier::Quality);
    assert_eq!(loaded.models_answer_tier, ModelTier::Quality);

    // The mapping is persisted: the retired token is gone, replaced by canonical JSON.
    assert_eq!(
        store
            .get_setting("models.vision_tier")
            .await
            .unwrap()
            .as_deref(),
        Some("\"quality\"")
    );
    assert_eq!(
        store
            .get_setting("models.answer_tier")
            .await
            .unwrap()
            .as_deref(),
        Some("\"quality\"")
    );

    // Idempotent: a second load reads `"quality"` and neither remaps nor rewrites.
    let reloaded = load_settings(dyn_store).await;
    assert_eq!(reloaded.models_vision_tier, ModelTier::Quality);
    assert_eq!(reloaded.models_answer_tier, ModelTier::Quality);
}

#[tokio::test]
async fn unknown_tier_falls_back_to_default_without_rewrite() {
    // A genuinely unparsable tier value is not the retired Beta token: it falls back to
    // the field default (Default), like any unparsable JSON, and — unlike Beta — is NOT
    // rewritten. Only the known-legacy `"beta"` value is migrated.
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    store
        .set_setting("models.vision_tier", "\"turbo\"")
        .await
        .unwrap();

    let loaded = load_settings(dyn_store).await;
    assert_eq!(loaded.models_vision_tier, ModelTier::Default);
    // The unknown value is left as-is (not migrated).
    assert_eq!(
        store
            .get_setting("models.vision_tier")
            .await
            .unwrap()
            .as_deref(),
        Some("\"turbo\"")
    );
}

#[tokio::test]
async fn session_settings_round_trip_and_clamp_to_the_final_contract() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let settings = Settings {
        sessions_min_len_secs: 3_601,
        sessions_gap_close_secs: 1,
        ..Settings::default()
    };

    save_settings(dyn_store, &settings).await.unwrap();
    let loaded = load_settings(dyn_store).await;
    assert_eq!(loaded.sessions_min_len_secs, 3_600);
    assert_eq!(loaded.sessions_gap_close_secs, 60);
    assert_eq!(
        store
            .get_setting("sessions.min_len_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("3600")
    );
    assert_eq!(
        store
            .get_setting("sessions.gap_close_secs")
            .await
            .unwrap()
            .as_deref(),
        Some("60")
    );

    let low = kernel::settings::sanitize_settings(Settings {
        sessions_min_len_secs: 0,
        sessions_gap_close_secs: 99_999,
        ..Settings::default()
    });
    assert_eq!(low.sessions_min_len_secs, 30);
    assert_eq!(low.sessions_gap_close_secs, 3_600);
    assert_eq!(Settings::default().sessions_min_len_secs, 120);
    assert_eq!(Settings::default().sessions_gap_close_secs, 120);
}

// ---------------------------------------------------------------------------
// `advanced.toml` (`file-only` keys, 0.4.5 D6)
// ---------------------------------------------------------------------------

fn write_advanced(dir: &Path, body: &str) -> std::path::PathBuf {
    let path = kernel::settings::advanced_config_path(dir);
    std::fs::write(&path, body).expect("write advanced.toml");
    path
}

#[test]
fn advanced_defaults_apply_when_the_file_is_absent() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = load_advanced_config(dir.path());
    assert_eq!(loaded, AdvancedConfig::default());
    assert!(
        !kernel::settings::advanced_config_path(dir.path()).exists(),
        "a missing file is not created merely by reading it"
    );
}

#[test]
fn advanced_defaults_apply_when_the_file_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    // A directory at the file path is unreadable as a text file on every platform.
    std::fs::create_dir(kernel::settings::advanced_config_path(dir.path())).unwrap();
    assert_eq!(load_advanced_config(dir.path()), AdvancedConfig::default());
}

#[test]
fn advanced_defaults_apply_when_the_file_is_invalid_and_it_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let body = "not = [valid";
    let path = write_advanced(dir.path(), body);

    assert_eq!(load_advanced_config(dir.path()), AdvancedConfig::default());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        body,
        "a syntax error must not be destroyed by a rewrite"
    );
}

#[test]
fn advanced_parses_known_values_and_clamps_them() {
    let dir = tempfile::tempdir().unwrap();
    write_advanced(
        dir.path(),
        r#"
[capture]
diff_threshold = 9.0
event_on_idle = true
event_debounce_ms = 1

[sidecar]
ngl = 100000
ctx_size = 100
device = "  Vulkan0  "

[throttle]
cpu_enter_pct = 50.0
cpu_exit_pct = 90.0
embed_text_floor = 0

[overlay]
max_results = 1
"#,
    );

    let cfg = load_advanced_config(dir.path());
    assert_eq!(cfg.capture.diff_threshold, 1.0, "clamped to the 0..=1 band");
    assert!(cfg.capture.event_on_idle);
    assert_eq!(cfg.capture.event_debounce_ms, 100, "floor is 100 ms");
    assert_eq!(cfg.sidecar.ngl, 999);
    assert_eq!(cfg.sidecar.ctx_size, 512, "non-zero ctx floors at 512");
    assert_eq!(cfg.sidecar.device.as_deref(), Some("Vulkan0"), "trimmed");
    assert_eq!(cfg.throttle.cpu_enter_pct, 50.0);
    assert_eq!(
        cfg.throttle.cpu_exit_pct, 49.0,
        "exit must stay strictly below enter"
    );
    assert_eq!(cfg.throttle.embed_text_floor, 1);
    assert_eq!(cfg.overlay.max_results, 1);
}

#[test]
fn advanced_reports_which_keys_came_from_the_file_not_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    write_advanced(
        dir.path(),
        r#"
[capture]
diff_threshold = 9.0
mystery_field = 1

[overlay]
max_results = 12

[reports]
weekly_top_k = 300
"#,
    );

    let loaded = kernel::settings::load_advanced_config_with_provenance(dir.path());
    assert_eq!(loaded.config.overlay.max_results, 12);
    assert_eq!(
        loaded.from_file,
        [
            "capture.diff_threshold",
            "overlay.max_results",
            "reports.weekly_top_k"
        ],
        "exactly the keys the file supplied, in section order"
    );
    assert_eq!(
        loaded.config.capture.diff_threshold, 1.0,
        "a clamped value still came from the file"
    );
    assert!(
        !loaded.from_file.iter().any(|key| key.starts_with("text.")),
        "an untouched section holds defaults and is not claimed"
    );

    // Nothing comes from a file that does not exist.
    let empty = tempfile::tempdir().unwrap();
    let defaults = kernel::settings::load_advanced_config_with_provenance(empty.path());
    assert_eq!(defaults.config, AdvancedConfig::default());
    assert!(defaults.from_file.is_empty());
}

#[test]
fn advanced_sidecar_device_blank_round_trips_as_none() {
    let dir = tempfile::tempdir().unwrap();
    write_advanced(
        dir.path(),
        r#"
[sidecar]
device = "   "
"#,
    );
    assert_eq!(load_advanced_config(dir.path()).sidecar.device, None);
}

#[test]
fn advanced_drops_unknown_section_and_field_and_rewrites_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_advanced(
        dir.path(),
        r#"
[capture]
diff_threshold = 0.25
mystery_field = 1

[bogus]
x = 1
"#,
    );

    let cfg = load_advanced_config(dir.path());
    assert_eq!(cfg.capture.diff_threshold, 0.25, "known value survives");
    assert_eq!(cfg.enrich, AdvancedConfig::default().enrich);

    let rewritten = std::fs::read_to_string(&path).unwrap();
    assert!(
        !rewritten.contains("mystery_field"),
        "unknown field must be dropped from the file: {rewritten}"
    );
    assert!(
        !rewritten.contains("bogus"),
        "unknown section must be dropped from the file: {rewritten}"
    );
    assert!(
        rewritten.contains("diff_threshold = 0.25"),
        "the surviving value is kept: {rewritten}"
    );
    // Atomic rewrite: the temp file is renamed into place, never left behind, and the
    // result is complete, parseable TOML rather than a truncated write.
    assert!(
        !path.with_extension("toml.tmp").exists(),
        "the rewrite must not leave its temp file behind"
    );
    rewritten
        .parse::<toml::Table>()
        .expect("the rewritten file is complete, valid TOML");

    // Second load is clean: same values, and the file is not rewritten again.
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(load_advanced_config(dir.path()), cfg);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before,
        "a clean file is not rewritten"
    );
}

#[test]
fn advanced_values_drive_the_same_consumers_the_typed_fields_did() {
    let dir = tempfile::tempdir().unwrap();
    write_advanced(
        dir.path(),
        r#"
[capture]
diff_threshold = 0.5
event_driven_enabled = true
event_on_idle = true
event_min_interval_ms = 2000

[sidecar]
ngl = 42
ctx_size = 4096
device = "Vulkan0"
kv_cache_type = "f16"
flash_attn = "off"
idle_ttl_secs = 600
recycle_rss_mb = 16384
recycle_enabled = false

[text]
chrome_suppress_min_seen = 9
chrome_protect_min_chars = 64
chrome_region_buckets = 12
"#,
    );

    let cfg = load_advanced_config(dir.path());

    // Capture config is fed the same fields it used to read off `Settings`.
    let capture = kernel::settings::capture_config(&Settings::default(), &cfg);
    assert_eq!(capture.diff_threshold, 0.5);
    assert!(capture.event_driven_enabled);
    assert!(capture.event_on_idle);
    assert_eq!(capture.event_min_interval_ms, 2_000);
    assert_eq!(
        capture.interval_ms, 3000,
        "UI key still comes from Settings"
    );

    // Sidecar launch params mirror the old `From<&Settings>` conversion exactly.
    let params = SidecarParams::from(&cfg);
    assert_eq!(params.ngl, 42);
    assert_eq!(params.ctx_size, 4096);
    assert_eq!(params.device.as_deref(), Some("Vulkan0"));
    assert_eq!(params.kv_cache_type, KvCacheType::F16);
    assert_eq!(params.flash_attn, FlashAttnSetting::Off);

    // Text-chrome thresholds reach the capture loop through the same snapshot.
    assert_eq!(cfg.text.chrome_suppress_min_seen, 9);
    assert_eq!(cfg.text.chrome_protect_min_chars, 64);
    assert_eq!(cfg.text.chrome_region_buckets, 12);
}

/// The `file-only` sections are constructed here so a rename or a dropped field is a compile
/// error rather than a silent default (the defaults are asserted by `AdvancedConfig::default`
/// in the tests above; this pins the shapes the consumers destructure).
#[test]
fn advanced_config_sections_are_complete() {
    let cfg = AdvancedConfig {
        capture: CaptureAdvanced::default(),
        enrich: EnrichAdvanced::default(),
        overlay: OverlayAdvanced::default(),
        reports: ReportsAdvanced::default(),
        resume: ResumeAdvanced::default(),
        sidecar: SidecarAdvanced::default(),
        text: TextAdvanced::default(),
        throttle: ThrottleAdvanced::default(),
    };
    assert!(cfg.enrich.embed_text);
    assert_eq!(cfg.enrich.vision_batch_size, 20);
    assert_eq!(cfg.reports.weekly_top_k, 200);
    assert_eq!(cfg.reports.map_reduce_min_frames, 20);
    assert_eq!(cfg.resume.min_dwell_secs, 120);
    assert_eq!(cfg.throttle.sample_interval_ms, 1_000);
    assert_eq!(cfg.overlay.max_results, 8);
}
