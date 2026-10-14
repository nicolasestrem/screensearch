//! Settings persistence round-trip (`03 §8`): `save_settings` followed by
//! `load_settings` returns exactly what was written. This guards the key-string
//! contract between the two — a typo in either would silently fall back to a
//! default and never round-trip.

use kernel::settings::{load_settings, save_settings};
use store::SqliteStore;
use traits::{FlashAttnSetting, KvCacheType, ModelTier, Settings, Store};

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
        capture_interval_ms: 5000,
        capture_monitors: vec![0, 2],
        capture_diff_threshold: 0.02,
        storage_max_width: 1600,
        storage_retention_days: 30,
        enrich_embed_text: false,
        enrich_vision_timer_enabled: true,
        enrich_vision_timer_interval_ms: 1_800_000,
        enrich_vision_idle_enabled: true,
        enrich_vision_idle_secs: 120,
        enrich_vision_batch_size: 50,
        enrich_worker_concurrency: 4,
        models_vision_tier: ModelTier::Quality,
        models_answer_tier: ModelTier::Quality,
        answer_thinking: false,
        sidecar_idle_ttl_secs: 600,
        sidecar_ngl: 35,
        sidecar_device: Some("Vulkan0".to_string()),
        sidecar_ctx_size: 4096,
        sidecar_kv_cache_type: KvCacheType::F16,
        sidecar_flash_attn: FlashAttnSetting::Off,
        sidecar_recycle_enabled: false,
        sidecar_recycle_rss_mb: 8192,
        privacy_excluded_apps: vec!["Signal".to_string(), "Element".to_string()],
        privacy_pause_on_lock: false,
        text_include_chrome_default: true,
        text_chrome_suppress_min_seen: 20,
        text_chrome_protect_min_chars: 64,
        text_chrome_region_buckets: 12,
        retrieval_default_top_k: 16,
        reports_daily_top_k: 60,
        reports_weekly_top_k: 300,
        reports_map_reduce_min_frames: 30,
        // Event-driven capture — every field away from its default, within the
        // sanitize clamps, so the round-trip exercises the new load/save encodings.
        capture_event_driven_enabled: true,
        capture_event_on_foreground: false,
        capture_event_on_idle: true,
        capture_event_debounce_ms: 750,
        capture_event_min_interval_ms: 2000,
        capture_event_idle_threshold_ms: 8000,
        capture_event_fallback_interval_ms: 60_000,
        // UIA text — every field away from its default, within the sanitize clamps.
        capture_uia_text_enabled: false,
        capture_uia_latency_budget_ms: 300,
        capture_uia_min_text_chars: 32,
        // UIA hang-fix knobs (`07` #71) — each away from its default, within the clamps.
        capture_uia_view_control_only: false,
        capture_uia_max_nodes: 2000,
        capture_uia_max_textpattern_calls: 128,
        capture_uia_suppress_during_input_ms: 750,
        overlay_hotkey: "Ctrl+Shift+F".to_string(),
        overlay_max_results: 12,
        // 0.3.0 flow recall — away from defaults, within the sanitize clamps.
        resume_min_dwell_secs: 300,
        sessions_min_len_secs: 240,
        sessions_gap_close_secs: 600,
        marks_hotkey: "Ctrl+Shift+M".to_string(),
        // Enrichment throttle — every field away from its default, within the sanitize
        // clamps (each exit % kept below its enter %), so the round-trip exercises the
        // new load/save encodings.
        throttle_enabled: true,
        throttle_cpu_enter_pct: 75.0,
        throttle_cpu_exit_pct: 50.0,
        throttle_gpu_enter_pct: 80.0,
        throttle_gpu_exit_pct: 55.0,
        throttle_enter_after_ms: 3000,
        throttle_exit_after_ms: 6000,
        throttle_sample_interval_ms: 2000,
        throttle_embed_text_floor: 2,
        // 0.3.2 app lifecycle — both away from their defaults (close-to-tray defaults on,
        // run-at-startup defaults off) so the round-trip exercises their load/save encodings.
        app_close_to_tray: false,
        app_run_at_startup: true,
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
        .set_setting("capture.diff_threshold", "NaN")
        .await
        .unwrap();
    store
        .set_setting("storage.max_width", "100000")
        .await
        .unwrap();
    store
        .set_setting("enrich.worker_concurrency", "0")
        .await
        .unwrap();
    store
        .set_setting("enrich.vision_timer_interval_ms", "1")
        .await
        .unwrap();
    store
        .set_setting("enrich.vision_idle_secs", "1")
        .await
        .unwrap();
    store
        .set_setting("enrich.vision_batch_size", "9999")
        .await
        .unwrap();
    store
        .set_setting("sidecar.idle_ttl_secs", "999999")
        .await
        .unwrap();
    store.set_setting("sidecar.ngl", "10000").await.unwrap();
    store
        .set_setting("sidecar.ctx_size", "999999")
        .await
        .unwrap();
    store
        .set_setting("capture.uia_max_nodes", "50") // below floor 100
        .await
        .unwrap();
    store
        .set_setting("capture.uia_max_textpattern_calls", "0") // below floor 1
        .await
        .unwrap();
    store
        .set_setting("capture.uia_suppress_during_input_ms", "99999") // above ceiling 10_000
        .await
        .unwrap();
    store
        .set_setting("overlay.hotkey", "\"   \"")
        .await
        .unwrap();
    store
        .set_setting("overlay.max_results", "999")
        .await
        .unwrap();

    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded.capture_interval_ms, 250);
    assert_eq!(loaded.capture_uia_max_nodes, 100);
    assert_eq!(loaded.capture_uia_max_textpattern_calls, 1);
    assert_eq!(loaded.capture_uia_suppress_during_input_ms, 10_000);
    assert_eq!(loaded.overlay_hotkey, Settings::default().overlay_hotkey);
    assert_eq!(loaded.overlay_max_results, 50);
    assert_eq!(loaded.capture_diff_threshold, 0.0);
    assert_eq!(loaded.storage_max_width, 7680);
    assert_eq!(loaded.enrich_worker_concurrency, 1);
    assert_eq!(loaded.enrich_vision_timer_interval_ms, 60_000);
    assert_eq!(loaded.enrich_vision_idle_secs, 60);
    assert_eq!(loaded.enrich_vision_batch_size, 500);
    assert_eq!(loaded.sidecar_idle_ttl_secs, 86_400);
    assert_eq!(loaded.sidecar_ngl, 999);
    assert_eq!(loaded.sidecar_ctx_size, 32_768);
}

#[tokio::test]
async fn sidecar_ctx_size_zero_is_preserved_as_auto_sentinel() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    // 0 must survive sanitization (it means "automatic per-lane default"), not get
    // clamped up to the 512 floor.
    store.set_setting("sidecar.ctx_size", "0").await.unwrap();
    assert_eq!(load_settings(dyn_store).await.sidecar_ctx_size, 0);

    // A small non-zero value below the floor is clamped up.
    store.set_setting("sidecar.ctx_size", "100").await.unwrap();
    assert_eq!(load_settings(dyn_store).await.sidecar_ctx_size, 512);
}

#[tokio::test]
async fn save_settings_persists_sanitized_numeric_values() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let original = Settings {
        capture_interval_ms: 1,
        capture_diff_threshold: f32::NAN,
        storage_max_width: 100_000,
        enrich_worker_concurrency: 0,
        enrich_vision_timer_interval_ms: 1,
        enrich_vision_idle_secs: 1,
        enrich_vision_batch_size: 9_999,
        sidecar_idle_ttl_secs: 999_999,
        sidecar_ngl: 10_000,
        sidecar_ctx_size: 999_999,
        capture_event_debounce_ms: 1,           // below floor 100
        capture_event_min_interval_ms: 999_999, // above ceiling 60_000
        capture_event_fallback_interval_ms: 1,  // below floor 1_000
        overlay_hotkey: "  Ctrl+Shift+F  ".to_string(),
        overlay_max_results: 0,
        ..Settings::default()
    };

    save_settings(dyn_store, &original)
        .await
        .expect("save settings");
    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded.capture_interval_ms, 250);
    assert_eq!(loaded.capture_diff_threshold, 0.0);
    assert_eq!(loaded.storage_max_width, 7680);
    assert_eq!(loaded.enrich_worker_concurrency, 1);
    assert_eq!(loaded.enrich_vision_timer_interval_ms, 60_000);
    assert_eq!(loaded.enrich_vision_idle_secs, 60);
    assert_eq!(loaded.enrich_vision_batch_size, 500);
    assert_eq!(loaded.sidecar_idle_ttl_secs, 86_400);
    assert_eq!(loaded.sidecar_ngl, 999);
    assert_eq!(loaded.sidecar_ctx_size, 32_768);
    assert_eq!(loaded.capture_event_debounce_ms, 100);
    assert_eq!(loaded.capture_event_min_interval_ms, 60_000);
    assert_eq!(loaded.capture_event_fallback_interval_ms, 1_000);
    assert_eq!(loaded.overlay_hotkey, "Ctrl+Shift+F");
    assert_eq!(loaded.overlay_max_results, 1);

    assert_eq!(
        store
            .get_setting("capture.diff_threshold")
            .await
            .unwrap()
            .as_deref(),
        Some("0")
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
    // *deliberately* sets the old chord back must keep it — the value-only remap used to clobber
    // it again on the very next load (codex PR #80 P2). The persisted migration marker makes the
    // remap fire at most once, so the deliberate choice is now durable across loads.
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    // Upgraded install: the retired default is remapped once on first load.
    store
        .set_setting("overlay.hotkey", "\"Ctrl+Alt+Space\"")
        .await
        .unwrap();
    assert_eq!(
        load_settings(dyn_store).await.overlay_hotkey,
        Settings::default().overlay_hotkey
    );

    // The user deliberately sets the old chord back…
    store
        .set_setting("overlay.hotkey", "\"Ctrl+Alt+Space\"")
        .await
        .unwrap();

    // …and it now survives every subsequent load instead of being re-remapped.
    assert_eq!(
        load_settings(dyn_store).await.overlay_hotkey,
        "Ctrl+Alt+Space"
    );
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

/// A `Store` decorator that forwards to an inner `SqliteStore` but fails `set_setting` for one
/// key. The real in-memory store never fails a write, so this is the only way to drive
/// `load_overlay_hotkey`'s "remap write failed" branch. All other methods delegate unchanged.
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
    // sees `migrated` and honors the stale `Ctrl+Alt+Space` forever (PR #80 codex/claude P2).
    // Instead the one-shot retries, and once the write can succeed the chord is remapped.
    let inner = SqliteStore::open_in_memory().expect("open in-memory store");
    inner
        .set_setting("overlay.hotkey", "\"Ctrl+Alt+Space\"")
        .await
        .unwrap();
    let failing = FailSetSettingFor {
        inner,
        fail_key: "overlay.hotkey",
    };

    // First load: the rewrite fails, so this session still uses the new default…
    let dyn_failing: &dyn Store = &failing;
    assert_eq!(
        load_settings(dyn_failing).await.overlay_hotkey,
        Settings::default().overlay_hotkey
    );
    // …but nothing latched — the DB still holds the legacy chord and no migration marker exists.
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
        Some(expected_default.as_str()),
        "once the write can succeed the retired chord is remapped"
    );
}

#[tokio::test]
async fn overlay_hotkey_custom_value_survives() {
    // A chord the user deliberately set (anything but the exact retired default) is never
    // touched by the one-shot remap.
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    store
        .set_setting("overlay.hotkey", "\"Ctrl+Shift+P\"")
        .await
        .unwrap();

    let loaded = load_settings(dyn_store).await;
    assert_eq!(loaded.overlay_hotkey, "Ctrl+Shift+P");
    assert_eq!(
        store
            .get_setting("overlay.hotkey")
            .await
            .unwrap()
            .as_deref(),
        Some("\"Ctrl+Shift+P\""),
        "a custom chord is left untouched in the DB"
    );
}

#[tokio::test]
async fn sidecar_device_round_trips_empty_as_none() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;
    let settings = Settings {
        sidecar_device: Some("   ".to_string()),
        ..Settings::default()
    };

    save_settings(dyn_store, &settings)
        .await
        .expect("save settings");
    let loaded = load_settings(dyn_store).await;

    assert_eq!(loaded.sidecar_device, None);
    assert_eq!(
        store
            .get_setting("sidecar.device")
            .await
            .unwrap()
            .as_deref(),
        Some("null")
    );
}

#[tokio::test]
async fn load_drops_retired_event_keys_without_error() {
    // 0.3.0 PR2: a config persisted by an older version still carries the four extra
    // event-trigger keys + the typing-pause threshold. `drop_retired_settings` purges
    // them (so the row doesn't linger) and load must not error or be perturbed by them.
    // 0.3.2 PR5 (D8): `storage.jpeg_quality` + `capture.uia_run_on_interactive` joined the
    // retired list. The 2026-08-01 usage review adds the now-constant focus length/density
    // parameters. This same test proves a persisted config drops every removed key without
    // perturbing surviving settings (usage review 2026-08-01 §6.7).
    use kernel::settings::{drop_retired_settings, RETIRED_SETTINGS_KEYS};

    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    let dyn_store: &dyn Store = &store;

    // A real (surviving) key alongside the retired ones, to prove only the retired go.
    save_settings(dyn_store, &Settings::default())
        .await
        .expect("seed defaults");
    for key in RETIRED_SETTINGS_KEYS {
        store.set_setting(key, "true").await.unwrap();
    }
    // Overwrite the numeric retiree with the value shape a pre-0.3.2 install persisted
    // (`80`, not `true`) so the tolerance is proven against realistic data too.
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
        .get_setting("capture.event_on_foreground")
        .await
        .unwrap()
        .is_some());
    assert_eq!(load_settings(dyn_store).await, Settings::default());

    // Idempotent: a second run finds nothing to drop (so it logs nothing — "once").
    drop_retired_settings(dyn_store).await;
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
