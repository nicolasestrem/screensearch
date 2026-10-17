//! Composition-root wiring for the opt-in local HTTP API (0.3.0 PR7; `03 §7c`, D11/D12).
//!
//! This is the only place the concrete store/kernel meet the `api` crate. [`TauriApiHost`]
//! implements [`api::ApiHost`] over the same helpers the Tauri commands use (`ask_context`,
//! `safe_frame_path`, `kernel::resume::where_was_i`), so the HTTP surface and the UI answer
//! identically. The server lives in an [`ApiRuntime`] slot; enabling/disabling toggles it
//! at runtime, mirroring the capture handle's idempotent start/stop.
//!
//! The `api.enabled`/`api.port`/`api.bind_address`/`api.token` keys are read/written directly on the `settings`
//! table here — deliberately **not** fields of the `Settings` struct — so the bearer token
//! never rides the `get_settings` response into `Settings.ts`.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, RwLock};

use async_trait::async_trait;
use tauri::{AppHandle, Emitter, Manager, State};
use traits::{
    AnswerProvider, ApiReadTokenSecret, ApiReadTokenStatus, ApiStatus, CaptureCoverage,
    CaptureInterruption, CaptureInterruptionReason, CaptureObservations, EvidenceWindow,
    ExportFrameRow, ExportRequest, ExportResult, FrameDetail, FramePageRequest, HistoryReadError,
    ReportDeliveryState, ReportDetail, ReportPageRequest, ReportRunKind, ReportRunPhase,
    ResumeContext, RetentionScope, RetrievedChunk, SavedReportRetention, Settings, Store,
    StoredFramePage, StoredReportPage, Toast, ToastLevel,
};

use api::{ApiHost, ApiServer, FrameImage, SharedToken, DEFAULT_BIND_ADDRESS, DEFAULT_PORT};
use kernel::Kernel;
use store::SqliteStore;

use crate::{ask_context, now_ms, safe_frame_path, AppState};

const API_ENABLED_KEY: &str = "api.enabled";
const API_PORT_KEY: &str = "api.port";
const API_TOKEN_KEY: &str = "api.token";
const API_READ_TOKEN_KEY: &str = "api.read_token";
const API_BIND_ADDRESS_KEY: &str = "api.bind_address";
/// Lower bound for `api.port` (`03 §8`; the upper bound is `u16::MAX`).
const PORT_MIN: u16 = 1024;

fn clamp_api_port(port: u16) -> u16 {
    port.max(PORT_MIN)
}

/// The runtime slot for the server + its live token + the last bind error. Held in
/// `AppState` behind an `Arc`. The `Option<ApiServer>` slot makes start/stop idempotent.
pub struct ApiRuntime {
    server: tokio::sync::Mutex<Option<ApiServer>>,
    token: SharedToken,
    last_error: StdMutex<Option<String>>,
    /// Serializes a whole config transition (persist → stop → start). Without it, two
    /// overlapping `set_api_config` calls could interleave — e.g. a disable seeing no
    /// server while an enable is still binding, then the enable storing a running server
    /// after `api.enabled=false`, leaving the API serving against the disabled intent.
    config_lock: tokio::sync::Mutex<()>,
}

impl ApiRuntime {
    pub fn new() -> Self {
        Self {
            server: tokio::sync::Mutex::new(None),
            token: Arc::new(RwLock::new(String::new())),
            last_error: StdMutex::new(None),
            config_lock: tokio::sync::Mutex::new(()),
        }
    }
}

impl Default for ApiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// The concrete [`ApiHost`], adapting the composition root's store/kernel/helpers.
pub struct TauriApiHost {
    store: Arc<SqliteStore>,
    kernel: Arc<Kernel>,
    app: AppHandle,
    data_dir: PathBuf,
    started_at_ms: i64,
}

impl TauriApiHost {
    pub fn new(
        store: Arc<SqliteStore>,
        kernel: Arc<Kernel>,
        app: AppHandle,
        data_dir: PathBuf,
        started_at_ms: i64,
    ) -> Self {
        Self {
            store,
            kernel,
            app,
            data_dir,
            started_at_ms,
        }
    }
}

#[async_trait]
impl ApiHost for TauriApiHost {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    async fn is_capturing(&self) -> bool {
        self.kernel.is_capturing().await
    }

    fn store(&self) -> Arc<dyn Store> {
        self.store.clone()
    }

    async fn settings(&self) -> Settings {
        kernel::settings::load_settings(self.store.as_ref()).await
    }

    async fn frame_detail(&self, frame_id: i64) -> anyhow::Result<Option<FrameDetail>> {
        self.store.get_frame(frame_id).await
    }

    /// Chronological evidence reads forward straight to the Store's transactional page
    /// adapter (dyn `Store` dispatch — the same seam the HTTP crate reuses verbatim);
    /// never search, ranking, hydration beyond the page read, generation or capture.
    async fn frame_page(
        &self,
        request: FramePageRequest,
    ) -> Result<StoredFramePage, HistoryReadError> {
        compose_frame_page(self.store.as_ref(), request).await
    }

    /// Coverage facts composed from one Store snapshot, current controller intent and
    /// bounded observations, and persisted retention settings (`03 §7f.2`). Never
    /// inspects files, starts capture, or invokes OCR/inference/generation.
    async fn capture_coverage(
        &self,
        window: EvidenceWindow,
    ) -> Result<CaptureCoverage, HistoryReadError> {
        compose_capture_coverage(self.store.as_ref(), self.kernel.as_ref(), window).await
    }

    async fn frame_image(&self, frame_id: i64) -> anyhow::Result<Option<FrameImage>> {
        let Some(detail) = self.store.get_frame(frame_id).await? else {
            return Ok(None);
        };
        // A retention-purged image is gone; the route maps `None`-with-detail to 404
        // `image_purged`.
        if detail.image_purged {
            return Ok(None);
        }
        let Some(path) = safe_frame_path(&self.data_dir, &detail.image_path) else {
            return Ok(None);
        };
        // A file missing on disk (manual delete, sync gap) is not a server fault — return
        // `None` so the route answers a clean 404 rather than a 500.
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        // Pre-WebP frames are JPEG; current frames are WebP (`07` #73). Extension decides.
        let content_type = if detail.image_path.to_ascii_lowercase().ends_with(".webp") {
            "image/webp"
        } else {
            "image/jpeg"
        };
        Ok(Some(FrameImage {
            bytes,
            content_type,
        }))
    }

    async fn ask_context(
        &self,
        query: &str,
        top_k: u32,
        session_id: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>> {
        ask_context(self.store.as_ref(), query, top_k, session_id)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        self.kernel.answer_provider().await
    }

    async fn add_mark(
        &self,
        frame_id: Option<i64>,
        capture_now: bool,
        note: Option<String>,
    ) -> anyhow::Result<i64> {
        let mark_id = self.kernel.add_mark(frame_id, capture_now, note).await?;
        let _ = self.app.emit("marks_changed", ());
        Ok(mark_id)
    }

    async fn resolve_mark(&self, mark_id: i64) -> anyhow::Result<()> {
        self.store.resolve_mark(mark_id, now_ms()).await?;
        let _ = self.app.emit("marks_changed", ());
        Ok(())
    }

    async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
        let settings = kernel::settings::load_settings(self.store.as_ref()).await;
        kernel::resume::where_was_i(self.store.as_ref(), &settings).await
    }

    async fn export_frames_page(
        &self,
        after_id: i64,
        from: Option<i64>,
        to: Option<i64>,
        limit: u32,
    ) -> anyhow::Result<Vec<ExportFrameRow>> {
        self.store
            .export_frames_page(after_id, from, to, limit)
            .await
    }

    /// Saved-report reads forward straight to the concrete Store's transactional page
    /// adapter (`03 §7f.5/.6`) — the same `dyn Store` seam the HTTP crate reuses. Never
    /// generation, retry, capture, scheduling, delivery, file write or current-source
    /// overlay.
    async fn report_page(
        &self,
        request: ReportPageRequest,
    ) -> Result<StoredReportPage, HistoryReadError> {
        compose_report_page(self.store.as_ref(), request).await
    }

    /// One saved run's complete public projection, forwarded to the concrete Store; a
    /// missing/pruned run is `None` (→404), never reconstructed or regenerated.
    async fn report_detail(&self, run_id: &str) -> Result<Option<ReportDetail>, HistoryReadError> {
        compose_report_detail(self.store.as_ref(), run_id).await
    }
}

/// The production frame-page adapter (`03 §7f.1`): one validated, bounded-membership
/// Store page (chronological, literal filters) forwarded through the `dyn Store` seam.
/// Reads never search, rank, hydrate beyond the page read, or touch capture/OCR/
/// inference.
async fn compose_frame_page(
    store: &SqliteStore,
    request: FramePageRequest,
) -> Result<StoredFramePage, HistoryReadError> {
    let store: &dyn Store = store;
    store.frame_page(request).await
}

/// The production saved-report page adapter (`03 §7f.5/.6`): one validated, bounded
/// overlap/status Store page forwarded through the `dyn Store` seam. Reads never
/// generate, retry, capture, schedule, deliver, write files or overlay current sources.
async fn compose_report_page(
    store: &SqliteStore,
    request: ReportPageRequest,
) -> Result<StoredReportPage, HistoryReadError> {
    let store: &dyn Store = store;
    store.report_page(request).await
}

/// The production saved-report detail adapter (`03 §7f.6`): the complete stored
/// projection for one opaque run id, or `None` (→404). Never reconstructs a missing run
/// or regenerates prose.
async fn compose_report_detail(
    store: &SqliteStore,
    run_id: &str,
) -> Result<Option<ReportDetail>, HistoryReadError> {
    let store: &dyn Store = store;
    store.report_detail(run_id).await
}

/// The production coverage adapter (`03 §7f.2`): factual Store aggregates + observed
/// controller intent/interruptions + persisted retention policy. Storage failures stay
/// errors (never zeros); unknown history is surfaced via `truncated`/`unavailable_before`,
/// never reconstructed. Reads do not write settings/frames/files or emit events, and
/// never start capture or invoke OCR/inference/generation.
async fn compose_capture_coverage(
    store: &SqliteStore,
    kernel: &Kernel,
    window: EvidenceWindow,
) -> Result<CaptureCoverage, HistoryReadError> {
    let store: &dyn Store = store;
    // One read-only transaction: persisted high-water, global chronological extrema,
    // in-window stored/text-available counts and the actual observation time.
    let facts = store.stored_coverage(window).await?;
    // Controller intent (never health) + bounded observed interruptions; the same
    // half-open window the Store read just validated.
    let (capture_enabled, observations) =
        kernel.capture_observations(window.from, window.to).await?;
    // Real persisted image retention (`storage.retention_days`, 0 = keep) read through
    // the app's canonical settings loader — never repaired or defaulted here.
    let settings = kernel::settings::load_settings(store).await;
    Ok(CaptureCoverage {
        window: facts.window,
        observed_at: facts.observed_at,
        capture_enabled,
        frame_id_high_water: facts.history.frame_id_high_water,
        oldest_frame: facts.oldest_frame,
        newest_frame: facts.newest_frame,
        newest_captured_at: facts.newest_captured_at,
        frames_stored: facts.frames_stored,
        frames_text_available: facts.frames_text_available,
        image_retention_days: settings.storage_retention_days,
        retention_scope: RetentionScope::ImagesOnly,
        saved_report_retention: saved_report_retention_policy(),
        capture_observations: map_capture_observations(observations),
    })
}

/// The existing saved-report run retention policy (`03 §7f.5/.6`): terminal runs expire
/// 90 days after their last update (`src-tauri/src/memory.rs` `RETENTION_MS`), and
/// `SqliteStore::prune_report_runs` never deletes queued/running/blocked runs or runs
/// with pending/sending/unknown/conflict deliveries or a failed delivery with a pending
/// retry. Reported as policy, not a per-run expiry guarantee.
fn saved_report_retention_policy() -> SavedReportRetention {
    SavedReportRetention {
        terminal_days: 90,
        exempt_phases: vec![
            ReportRunPhase::Queued,
            ReportRunPhase::Running,
            ReportRunPhase::Blocked,
        ],
        exempt_delivery_states: vec![
            ReportDeliveryState::Pending,
            ReportDeliveryState::Sending,
            ReportDeliveryState::UnknownDelivery,
            ReportDeliveryState::Conflict,
        ],
        failed_delivery_with_pending_retry_exempt: true,
    }
}

/// Kernel observation fields map one-to-one onto the wire DTO; the reason match is
/// exhaustive on purpose so a new kernel reason cannot silently fold into an existing
/// wire variant.
fn map_capture_observations(observations: kernel::CaptureObservations) -> CaptureObservations {
    CaptureObservations {
        observation_started_at: observations.observation_started_at,
        unavailable_before: observations.unavailable_before,
        truncated: observations.truncated,
        intervals: observations
            .intervals
            .into_iter()
            .map(|interval| CaptureInterruption {
                start: interval.start,
                end: interval.end,
                reason: match interval.reason {
                    kernel::CaptureInterruptionReason::CaptureStopped => {
                        CaptureInterruptionReason::CaptureStopped
                    }
                    kernel::CaptureInterruptionReason::WorkerError => {
                        CaptureInterruptionReason::WorkerError
                    }
                },
            })
            .collect(),
    }
}

/// Reads the persisted API config, applying the port clamp and treating a blank/absent
/// token as `None`. Unknown/malformed values fall back to defaults (tolerant load, `03 §8`).
async fn read_api_config(store: &SqliteStore) -> (bool, u16, Option<String>) {
    let enabled = store
        .get_setting(API_ENABLED_KEY)
        .await
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    let port = store
        .get_setting(API_PORT_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u16>().ok())
        .map(clamp_api_port)
        .unwrap_or(DEFAULT_PORT);
    let token = store
        .get_setting(API_TOKEN_KEY)
        .await
        .ok()
        .flatten()
        .filter(|t| !t.is_empty());
    (enabled, port, token)
}

async fn read_bind_address(store: &SqliteStore) -> Result<String, String> {
    store
        .get_setting(API_BIND_ADDRESS_KEY)
        .await
        .map(|value| value.unwrap_or_else(|| DEFAULT_BIND_ADDRESS.to_string()))
        .map_err(|e| format!("could not read api.bind_address: {e}"))
}

fn parse_bind_address(value: &str) -> Result<IpAddr, String> {
    value
        .trim()
        .parse()
        .map_err(|_| "API bind address must be an IPv4 or IPv6 literal".to_string())
}

/// The current API status: persisted intent + live running state + last bind error.
async fn current_status(store: &SqliteStore, runtime: &ApiRuntime) -> ApiStatus {
    let (enabled, port, token) = read_api_config(store).await;
    let running = runtime.server.lock().await.is_some();
    let mut last_error = runtime
        .last_error
        .lock()
        .expect("api last_error lock")
        .clone();
    let bind_address = match read_bind_address(store).await {
        Ok(value) => {
            if let Err(error) = parse_bind_address(&value) {
                last_error = Some(error);
            }
            value
        }
        Err(error) => {
            last_error = Some(error);
            DEFAULT_BIND_ADDRESS.to_string()
        }
    };
    ApiStatus {
        enabled,
        running,
        port,
        token,
        last_error,
        bind_address,
    }
}

/// Validates the address and atomically persists `{enabled, port, bind_address}`, generates the token on first enable, then (re)starts or
/// stops the server. A bind failure leaves `enabled = true` persisted, `running = false`,
/// and `last_error` set — the loud guided-change state (`03 §7c`); the command wrapper
/// adds the warning toast (that's *data*, not an `Err`). An **`Err`** is reserved for a
/// invalid address or failed persistence write: we return before touching the running server, so a disable can
/// never stop the server yet leave `enabled = true` on disk (which would autostart again).
pub async fn apply_api_config(
    store: &SqliteStore,
    runtime: &ApiRuntime,
    host: Arc<dyn ApiHost>,
    enabled: bool,
    port: Option<u16>,
    bind_address: Option<String>,
) -> Result<ApiStatus, String> {
    // Serialize the whole transition so overlapping enable/disable calls can't interleave.
    let _config_guard = runtime.config_lock.lock().await;
    apply_api_config_locked(store, runtime, host, enabled, port, bind_address).await
}

async fn apply_api_config_locked(
    store: &SqliteStore,
    runtime: &ApiRuntime,
    host: Arc<dyn ApiHost>,
    enabled: bool,
    port: Option<u16>,
    bind_address: Option<String>,
) -> Result<ApiStatus, String> {
    let explicit_address = bind_address.is_some();
    let stored_address = match bind_address {
        Some(value) => value,
        None => read_bind_address(store).await?,
    };
    // A damaged saved address must not prevent turning a running API off.
    let target_address = match parse_bind_address(&stored_address) {
        Ok(address) => Some(address),
        Err(_) if !enabled && !explicit_address => None,
        Err(error) => return Err(error),
    };
    let persisted_address = target_address
        .map(|address| address.to_string())
        .unwrap_or(stored_address);
    // Fail closed before stopping/rebinding: a failed credential read must not silently
    // drop a previously configured read credential. The same config lock covers edits.
    let read_token = if enabled {
        read_read_token(store).await?
    } else {
        None
    };
    let (_, current_port, _) = read_api_config(store).await;
    let target_port = clamp_api_port(port.unwrap_or(current_port));
    // Persist the intent *before* touching the running server; if the DB write fails, bail
    // now so on-disk state and the live server can't diverge.
    store
        .set_settings_batch(&[
            (API_PORT_KEY.to_string(), target_port.to_string()),
            (API_ENABLED_KEY.to_string(), enabled.to_string()),
            (API_BIND_ADDRESS_KEY.to_string(), persisted_address),
        ])
        .await
        .map_err(|e| format!("could not persist API configuration: {e}"))?;

    // Stop any running server first (a config change is a deterministic restart).
    let existing = runtime.server.lock().await.take();
    if let Some(server) = existing {
        server.stop().await;
    }
    *runtime.last_error.lock().expect("api last_error lock") = None;

    if enabled {
        let target_address = target_address.expect("enabled configuration has a validated address");
        // The live token is authoritative once populated — `regenerate_api_token` keeps the
        // DB and `runtime.token` in sync. Only read/populate it on first enable (empty live
        // token, e.g. a fresh boot's autostart); re-writing it here from a stale DB read
        // could clobber a concurrent regenerate.
        if runtime.token.read().expect("api token lock").is_empty() {
            let token = match store
                .get_setting(API_TOKEN_KEY)
                .await
                .ok()
                .flatten()
                .filter(|t| !t.is_empty())
            {
                Some(t) => t,
                None => {
                    let t = api::generate_token();
                    store
                        .set_setting(API_TOKEN_KEY, &t)
                        .await
                        .map_err(|e| format!("could not persist api.token: {e}"))?;
                    t
                }
            };
            *runtime.token.write().expect("api token lock") = token;
        }

        match ApiServer::start_on_with_read_token(
            host,
            runtime.token.clone(),
            read_token,
            target_address,
            target_port,
        )
        .await
        {
            Ok(server) => {
                // The configured read credential is installed inside
                // `start_on_with_read_token`, before the accepting serve task is spawned
                // (startup, enable, and rebind): there is no fail-closed 401 window.
                // A `None` credential still fails closed.
                *runtime.server.lock().await = Some(server);
            }
            Err(e) => {
                let message = format!(
                    "could not listen on {}: {e}",
                    SocketAddr::new(target_address, target_port)
                );
                tracing::warn!(port = target_port, error = %e, "local API failed to bind");
                *runtime.last_error.lock().expect("api last_error lock") = Some(message);
            }
        }
    }

    Ok(current_status(store, runtime).await)
}

/// Stops the server if running (bounded graceful shutdown). Called on app exit.
pub async fn stop_server(runtime: &ApiRuntime) {
    let existing = runtime.server.lock().await.take();
    if let Some(server) = existing {
        server.stop().await;
    }
}

/// On launch (after `AppState` is managed), start the server if `api.enabled` — loud on a
/// bind failure, exactly like the boot-time hotkey registration (D6).
pub fn autostart(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (Some(store), Some(host)) = (state.store.clone(), state.api_host.clone()) else {
        return;
    };
    let runtime = state.api_runtime.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _config_guard = runtime.config_lock.lock().await;
        // Startup must not turn a failed read into a persisted disabled intent.
        // Unlike tolerant status rendering, this read controls a configuration write.
        let result = async {
            let enabled = store
                .get_setting(API_ENABLED_KEY)
                .await
                .map_err(|e| format!("could not read API startup configuration: {e}"))?
                .is_some_and(|value| value == "true");
            if !enabled {
                return Ok(current_status(store.as_ref(), &runtime).await);
            }
            let host: Arc<dyn ApiHost> = host;
            apply_api_config_locked(store.as_ref(), &runtime, host, true, None, None).await
        }
        .await;
        match result {
            Ok(status) => warn_on_bind_failure(&app, &status),
            Err(e) => {
                tracing::error!(error = %e, "local API autostart failed");
                *runtime.last_error.lock().expect("api last_error lock") = Some(e);
            }
        }
        // Invalidate early UI reads after both successful and failed startup. Never
        // broadcast ApiStatus itself: it contains the bearer token.
        let _ = app.emit("api_status_changed", ());
    });
}

/// Fires the loud port-in-use warning toast when the API is enabled but not running
/// (mirrors the D6 hotkey-conflict toast).
fn warn_on_bind_failure(app: &AppHandle, status: &ApiStatus) {
    if status.enabled && !status.running {
        if let Some(err) = &status.last_error {
            let _ = app.emit(
                "toast",
                Toast {
                    level: ToastLevel::Warning,
                    message: format!("Local API: {err} — check the bind address and port"),
                },
            );
        }
    }
}

/// Enable/disable the API and optionally set its port and literal bind address. Bind failure is loud + guided (`03 §7c`).
#[tauri::command]
pub async fn set_api_config(
    enabled: bool,
    port: Option<u16>,
    bind_address: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ApiStatus, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let host = state
        .api_host
        .clone()
        .ok_or_else(|| "api host unavailable".to_string())?;
    let status = apply_api_config(
        store.as_ref(),
        &state.api_runtime,
        host,
        enabled,
        port,
        bind_address,
    )
    .await?;
    warn_on_bind_failure(&app, &status);
    Ok(status)
}

/// The current API status (enabled, running, port, token, last error).
#[tauri::command]
pub async fn get_api_status(state: State<'_, AppState>) -> Result<ApiStatus, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let _guard = state.api_runtime.config_lock.lock().await;
    Ok(current_status(store.as_ref(), &state.api_runtime).await)
}

/// Regenerate the bearer token. A running server needs **no** restart — the auth
/// middleware reads the live token per request.
#[tauri::command]
pub async fn regenerate_api_token(state: State<'_, AppState>) -> Result<ApiStatus, String> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| "database unavailable".to_string())?;
    rotate_legacy_token(store, &state.api_runtime).await
}

async fn rotate_legacy_token(
    store: &SqliteStore,
    runtime: &ApiRuntime,
) -> Result<ApiStatus, String> {
    let _guard = runtime.config_lock.lock().await;
    let token = api::generate_token();
    store
        .set_setting(API_TOKEN_KEY, &token)
        .await
        .map_err(|_| "Could not save the API token; access was not changed.".to_string())?;
    *runtime.token.write().expect("api token lock") = token;
    Ok(current_status(store, runtime).await)
}

/// Read errors are not absence: neither status nor a mutation may generate a credential
/// after a failed read. The error is intentionally fixed, with no secret-bearing cause.
async fn read_read_token(store: &SqliteStore) -> Result<Option<String>, String> {
    store
        .get_setting(API_READ_TOKEN_KEY)
        .await
        .map(|value| value.filter(|token| !token.is_empty()))
        .map_err(|_| "Could not read the API read credential.".to_string())
}

#[derive(Clone, Copy)]
enum ReadTokenChange {
    Create,
    Rotate,
    Revoke,
}

/// The caller holds config_lock throughout persistence and live install. Keeping the
/// persist future explicit lets a fault test exercise this exact ordering seam.
async fn persist_then_install_read_token(
    runtime: &ApiRuntime,
    token: Option<String>,
    persist: impl std::future::Future<Output = anyhow::Result<()>>,
) -> Result<ApiReadTokenStatus, String> {
    persist.await.map_err(|_| {
        "Could not save the API read credential; access was not changed.".to_string()
    })?;
    if let Some(server) = runtime.server.lock().await.as_ref() {
        server.set_read_token(token.clone());
    }
    Ok(ApiReadTokenStatus {
        configured: token.is_some(),
    })
}

async fn change_read_token(
    store: &SqliteStore,
    runtime: &ApiRuntime,
    change: ReadTokenChange,
) -> Result<ApiReadTokenStatus, String> {
    let _guard = runtime.config_lock.lock().await;
    let existing = read_read_token(store).await?;
    match change {
        ReadTokenChange::Create if existing.is_some() => {
            return Err("A read token already exists. Use Rotate to replace it.".into())
        }
        ReadTokenChange::Rotate if existing.is_none() => {
            return Err("No read token is configured. Use Create first.".into())
        }
        _ => {}
    }
    let token = match change {
        ReadTokenChange::Create | ReadTokenChange::Rotate => Some(api::generate_token()),
        ReadTokenChange::Revoke => None,
    };
    // An empty saved value is the existing KV mechanism's disabled credential, not a
    // newly generated secret. No API enable/port/address keys are touched here.
    let saved = token.clone().unwrap_or_default();
    persist_then_install_read_token(
        runtime,
        token,
        store.set_setting(API_READ_TOKEN_KEY, &saved),
    )
    .await
}

/// Availability only. Credential bytes are never returned by passive status reads.
#[tauri::command]
pub async fn get_api_read_token_status(
    state: State<'_, AppState>,
) -> Result<ApiReadTokenStatus, String> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| "database unavailable".to_string())?;
    let _guard = state.api_runtime.config_lock.lock().await;
    Ok(ApiReadTokenStatus {
        configured: read_read_token(store).await?.is_some(),
    })
}

#[tauri::command]
pub async fn create_api_read_token(
    state: State<'_, AppState>,
) -> Result<ApiReadTokenStatus, String> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| "database unavailable".to_string())?;
    change_read_token(store, &state.api_runtime, ReadTokenChange::Create).await
}

#[tauri::command]
pub async fn rotate_api_read_token(
    state: State<'_, AppState>,
) -> Result<ApiReadTokenStatus, String> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| "database unavailable".to_string())?;
    change_read_token(store, &state.api_runtime, ReadTokenChange::Rotate).await
}

#[tauri::command]
pub async fn revoke_api_read_token(
    state: State<'_, AppState>,
) -> Result<ApiReadTokenStatus, String> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| "database unavailable".to_string())?;
    change_read_token(store, &state.api_runtime, ReadTokenChange::Revoke).await
}

/// Explicit local credential action only. Never cache/broadcast this response.
#[tauri::command]
pub async fn reveal_api_read_token(
    state: State<'_, AppState>,
) -> Result<ApiReadTokenSecret, String> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| "database unavailable".to_string())?;
    let _guard = state.api_runtime.config_lock.lock().await;
    let token = read_read_token(store)
        .await?
        .ok_or_else(|| "No read token is configured.".to_string())?;
    Ok(ApiReadTokenSecret { token })
}

/// Export frames + content text + marks to a JSON file in the user's Downloads folder
/// (`03 §7c`, D12). Uses the same streaming code path as `GET /v1/export`, so it works
/// with the API disabled.
#[tauri::command]
pub async fn export_data(
    request: ExportRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ExportResult, String> {
    if let (Some(from), Some(to)) = (request.from, request.to) {
        if from > to {
            return Err("`from` must be less than or equal to `to`".to_string());
        }
    }
    let host = state
        .api_host
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let host: Arc<dyn ApiHost> = host;
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| format!("cannot resolve the Downloads folder: {e}"))?;
    api::export::export_to_file(host, request.from, request.to, &dir)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `ApiHost` for config-logic tests — none of its data methods are exercised
    /// by `apply_api_config`, which only needs *something* to hand to `ApiServer::start`.
    struct StubHost;

    #[async_trait]
    impl ApiHost for StubHost {
        fn version(&self) -> String {
            "stub".to_string()
        }
        fn started_at_ms(&self) -> i64 {
            0
        }
        async fn is_capturing(&self) -> bool {
            false
        }
        fn store(&self) -> Arc<dyn Store> {
            unreachable!("config tests never touch the store via the host")
        }
        async fn settings(&self) -> Settings {
            Settings::default()
        }
        async fn frame_detail(&self, _frame_id: i64) -> anyhow::Result<Option<FrameDetail>> {
            Ok(None)
        }
        async fn frame_image(&self, _frame_id: i64) -> anyhow::Result<Option<FrameImage>> {
            Ok(None)
        }
        async fn ask_context(
            &self,
            _query: &str,
            _top_k: u32,
            _session_id: Option<i64>,
        ) -> anyhow::Result<Vec<RetrievedChunk>> {
            Ok(vec![])
        }
        async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
            None
        }
        async fn add_mark(
            &self,
            _frame_id: Option<i64>,
            _capture_now: bool,
            _note: Option<String>,
        ) -> anyhow::Result<i64> {
            Ok(0)
        }
        async fn resolve_mark(&self, _mark_id: i64) -> anyhow::Result<()> {
            Ok(())
        }
        async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
            Ok(None)
        }
        async fn export_frames_page(
            &self,
            _after_id: i64,
            _from: Option<i64>,
            _to: Option<i64>,
            _limit: u32,
        ) -> anyhow::Result<Vec<ExportFrameRow>> {
            Ok(vec![])
        }
    }

    fn host() -> Arc<dyn ApiHost> {
        Arc::new(StubHost)
    }

    #[tokio::test]
    async fn read_token_creation_does_not_enable_or_rebind_api() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        store
            .set_setting(API_BIND_ADDRESS_KEY, "192.0.2.50")
            .await
            .unwrap();
        store.set_setting(API_PORT_KEY, "45000").await.unwrap();
        store
            .set_setting(API_TOKEN_KEY, "legacy-fixture")
            .await
            .unwrap();
        assert!(
            read_read_token(&store).await.unwrap().is_none(),
            "upgrade never creates read credential"
        );
        let before = current_status(&store, &runtime).await;
        let created = change_read_token(&store, &runtime, ReadTokenChange::Create)
            .await
            .unwrap();
        assert!(created.configured);
        assert_eq!(current_status(&store, &runtime).await, before);
        assert!(runtime.server.lock().await.is_none());
        assert!(store.get_setting(API_ENABLED_KEY).await.unwrap().is_none());
        let token = read_read_token(&store).await.unwrap().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(change_read_token(&store, &runtime, ReadTokenChange::Create)
            .await
            .is_err());
        assert!(read_read_token(&store).await.unwrap().as_deref() == Some(token.as_str()));
        // Revoke is idempotent and never affects the legacy token/endpoint either.
        for _ in 0..2 {
            assert!(
                !change_read_token(&store, &runtime, ReadTokenChange::Revoke)
                    .await
                    .unwrap()
                    .configured
            );
        }
        assert_eq!(current_status(&store, &runtime).await, before);
        assert!(change_read_token(&store, &runtime, ReadTokenChange::Rotate)
            .await
            .is_err());
    }

    async fn read_health(port: u16, token: &str) -> reqwest::StatusCode {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://127.0.0.1:{port}/v1/health"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn read_token_live_rotation_revocation_and_legacy_rotation_are_independent() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let port = available_port();
        let before = apply_api_config(&store, &runtime, host(), true, Some(port), None)
            .await
            .unwrap();
        assert!(before.running);
        change_read_token(&store, &runtime, ReadTokenChange::Create)
            .await
            .unwrap();
        let first = read_read_token(&store).await.unwrap().unwrap();
        assert_eq!(read_health(port, &first).await, reqwest::StatusCode::OK);
        assert_eq!(current_status(&store, &runtime).await, before);
        change_read_token(&store, &runtime, ReadTokenChange::Rotate)
            .await
            .unwrap();
        let next = read_read_token(&store).await.unwrap().unwrap();
        assert!(first != next);
        assert_eq!(
            read_health(port, &first).await,
            reqwest::StatusCode::UNAUTHORIZED
        );
        assert_eq!(read_health(port, &next).await, reqwest::StatusCode::OK);
        let legacy = rotate_legacy_token(&store, &runtime).await.unwrap();
        assert_eq!(
            read_health(port, before.token.as_deref().unwrap()).await,
            reqwest::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            read_health(port, legacy.token.as_deref().unwrap()).await,
            reqwest::StatusCode::OK
        );
        assert_eq!(read_health(port, &next).await, reqwest::StatusCode::OK);
        assert!(read_read_token(&store).await.unwrap().as_deref() == Some(next.as_str()));
        change_read_token(&store, &runtime, ReadTokenChange::Revoke)
            .await
            .unwrap();
        assert_eq!(
            read_health(port, &next).await,
            reqwest::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            read_health(port, legacy.token.as_deref().unwrap()).await,
            reqwest::StatusCode::OK
        );
        assert_eq!(current_status(&store, &runtime).await, legacy);
        stop_server(&runtime).await;
    }

    #[tokio::test]
    async fn read_token_persistence_failure_preserves_live_credential_and_listener() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let port = available_port();
        let before = apply_api_config(&store, &runtime, host(), true, Some(port), None)
            .await
            .unwrap();
        change_read_token(&store, &runtime, ReadTokenChange::Create)
            .await
            .unwrap();
        let prior = read_read_token(&store).await.unwrap().unwrap();
        let guard = runtime.config_lock.lock().await;
        for candidate in [Some("not-saved-fixture".to_string()), None] {
            let error = persist_then_install_read_token(&runtime, candidate, async {
                anyhow::bail!("injected write failure, secret must never be echoed")
            })
            .await
            .unwrap_err();
            assert!(!error.contains("secret"));
            assert!(read_read_token(&store).await.unwrap().as_deref() == Some(prior.as_str()));
            assert_eq!(read_health(port, &prior).await, reqwest::StatusCode::OK);
            assert_eq!(
                read_health(port, "not-saved-fixture").await,
                reqwest::StatusCode::UNAUTHORIZED
            );
            assert_eq!(current_status(&store, &runtime).await, before);
        }
        drop(guard);
        stop_server(&runtime).await;
    }

    #[tokio::test]
    async fn read_token_survives_disable_enable_rebind_and_fresh_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.sqlite");
        let token;
        let port = available_port();
        {
            let store = SqliteStore::open_path(&path).unwrap();
            let runtime = ApiRuntime::new();
            change_read_token(&store, &runtime, ReadTokenChange::Create)
                .await
                .unwrap();
            token = read_read_token(&store).await.unwrap().unwrap();
            apply_api_config(&store, &runtime, host(), true, Some(port), None)
                .await
                .unwrap();
            assert_eq!(read_health(port, &token).await, reqwest::StatusCode::OK);
            apply_api_config(&store, &runtime, host(), false, None, None)
                .await
                .unwrap();
            assert!(runtime.server.lock().await.is_none());
            apply_api_config(&store, &runtime, host(), true, None, None)
                .await
                .unwrap();
            assert_eq!(read_health(port, &token).await, reqwest::StatusCode::OK);
            let rebound = available_port();
            apply_api_config(&store, &runtime, host(), true, Some(rebound), None)
                .await
                .unwrap();
            assert_eq!(read_health(rebound, &token).await, reqwest::StatusCode::OK);
            stop_server(&runtime).await;
        }
        let store = SqliteStore::open_path(&path).unwrap();
        let runtime = ApiRuntime::new();
        let status = apply_api_config(&store, &runtime, host(), true, None, None)
            .await
            .unwrap();
        assert_eq!(
            read_health(status.port, &token).await,
            reqwest::StatusCode::OK
        );
        stop_server(&runtime).await;
    }

    /// Audit P1 defect 1 (+ gap 3): a bind failure must leave the read credential
    /// persisted, and the retry/rebind must install it again *at startup* through the
    /// credential-first entry point — so it is live the moment the listener accepts.
    /// Real sockets throughout; no `set_read_token` call is made.
    #[tokio::test]
    async fn read_credential_survives_bind_failure_and_is_reinstalled_on_rebind() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        // A competing loopback listener occupies the port, so the enable bind fails.
        let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let blocked_port = occupied.local_addr().unwrap().port();
        change_read_token(&store, &runtime, ReadTokenChange::Create)
            .await
            .unwrap();
        let token = read_read_token(&store).await.unwrap().unwrap();

        let failed = apply_api_config(&store, &runtime, host(), true, Some(blocked_port), None)
            .await
            .unwrap();
        assert!(failed.enabled, "intent is preserved");
        assert!(!failed.running, "the occupied port blocks the bind");
        assert!(failed.last_error.is_some());
        assert!(runtime.server.lock().await.is_none());
        // The credential is untouched on disk and none is live yet.
        assert_eq!(
            read_read_token(&store).await.unwrap().as_deref(),
            Some(token.as_str())
        );

        drop(occupied);
        let free = available_port();
        let rebound = apply_api_config(&store, &runtime, host(), true, Some(free), None)
            .await
            .unwrap();
        assert!(rebound.running, "{:?}", rebound.last_error);
        // Installed at startup by the credential-first entry point — no setter here.
        assert_eq!(read_health(free, &token).await, reqwest::StatusCode::OK);
        stop_server(&runtime).await;
    }

    #[test]
    fn passive_credential_status_and_general_settings_do_not_serialize_secrets() {
        assert_eq!(
            serde_json::to_value(ApiReadTokenStatus { configured: true }).unwrap(),
            serde_json::json!({"configured": true})
        );
        let settings = serde_json::to_value(Settings::default()).unwrap();
        assert!(settings
            .as_object()
            .unwrap()
            .keys()
            .all(|key| !key.contains("token")));
        let status = serde_json::to_value(ApiStatus {
            enabled: false,
            running: false,
            port: DEFAULT_PORT,
            bind_address: DEFAULT_BIND_ADDRESS.to_string(),
            token: Some("legacy-fixture".into()),
            last_error: None,
        })
        .unwrap();
        assert!(
            status.get("read_token").is_none(),
            "legacy status never gains new secret fields"
        );
    }

    /// A fresh profile has the API off, the default port, and no token — nothing listens.
    #[tokio::test]
    async fn fresh_profile_defaults_off() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let status = current_status(&store, &runtime).await;
        assert!(!status.enabled);
        assert!(!status.running);
        assert_eq!(status.port, DEFAULT_PORT);
        assert_eq!(status.token, None);
    }

    /// The port clamps to the `1024..=65535` band.
    #[test]
    fn port_clamps_to_floor() {
        assert_eq!(clamp_api_port(0), PORT_MIN);
        assert_eq!(clamp_api_port(80), PORT_MIN);
        assert_eq!(clamp_api_port(43210), 43210);
        assert_eq!(clamp_api_port(u16::MAX), u16::MAX);
    }

    /// Enabling generates a token once and persists it; a later enable reuses it, and it
    /// survives across a disable. Use an available OS-selected port.
    #[tokio::test]
    async fn enabling_generates_token_once_and_persists() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();

        let port = available_port();
        let status = apply_api_config(&store, &runtime, host(), true, Some(port), None)
            .await
            .unwrap();
        assert!(status.enabled);
        assert!(status.running, "server started on the ephemeral port");
        let token = status
            .token
            .clone()
            .expect("token generated on first enable");
        assert_eq!(token.len(), 64, "32 random bytes as hex");

        // Re-enable: same token.
        let status2 = apply_api_config(&store, &runtime, host(), true, Some(port), None)
            .await
            .unwrap();
        assert_eq!(status2.token, Some(token.clone()));

        // Disable then re-enable: still the same token (persisted, not regenerated).
        let disabled = apply_api_config(&store, &runtime, host(), false, None, None)
            .await
            .unwrap();
        assert!(!disabled.enabled);
        assert!(!disabled.running);
        assert_eq!(disabled.token, Some(token.clone()));

        let reenabled = apply_api_config(&store, &runtime, host(), true, Some(port), None)
            .await
            .unwrap();
        assert_eq!(reenabled.token, Some(token));

        stop_server(&runtime).await;
    }

    /// A bind failure (a port already held by another listener) leaves `enabled = true`
    /// persisted, `running = false`, and a `last_error` — the loud guided-change state.
    #[tokio::test]
    async fn bind_failure_keeps_enabled_with_error() {
        // A competing loopback service must block the wildcard API bind too:
        // local MCP must never silently connect to a different service on this port.
        assert_bind_failure("127.0.0.1").await;
    }

    #[tokio::test]
    async fn wildcard_bind_failure_keeps_enabled_with_error() {
        assert_bind_failure("0.0.0.0").await;
    }

    async fn assert_bind_failure(address: &str) {
        let occupied = std::net::TcpListener::bind((address, 0)).unwrap();
        let port = occupied.local_addr().unwrap().port();

        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let status = apply_api_config(
            &store,
            &runtime,
            host(),
            true,
            Some(port),
            Some("0.0.0.0".into()),
        )
        .await
        .unwrap();

        assert!(status.enabled, "intent is preserved");
        assert!(!status.running, "the server did not start");
        assert!(
            status.last_error.is_some(),
            "the error is surfaced, not swallowed"
        );
        // Persisted enabled stays true so a restart re-attempts loudly.
        assert_eq!(
            store.get_setting("api.enabled").await.unwrap().as_deref(),
            Some("true")
        );
    }

    #[tokio::test]
    async fn disabled_api_has_no_listener() {
        let probe = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let status = apply_api_config(&store, &runtime, host(), false, Some(port), None)
            .await
            .unwrap();
        assert!(!status.enabled);
        assert!(!status.running);
        assert_eq!(status.bind_address, "127.0.0.1");
        assert!(runtime.server.lock().await.is_none());
        let available = std::net::TcpListener::bind(("0.0.0.0", port)).unwrap();
        assert_eq!(available.local_addr().unwrap().port(), port);
    }

    fn available_port() -> u16 {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn accepts_only_ip_literals() {
        for address in [
            "127.0.0.1",
            "0.0.0.0",
            "192.168.1.9",
            "::",
            "::1",
            "2001:db8::1",
        ] {
            assert!(parse_bind_address(address).is_ok(), "{address}");
        }
        for address in [
            "",
            "localhost",
            "https://127.0.0.1",
            "127.0.0.1:43210",
            "[::1]:43210",
            "256.0.0.1",
        ] {
            assert!(parse_bind_address(address).is_err(), "{address}");
        }
    }

    #[tokio::test]
    async fn existing_enabled_install_stays_loopback_and_explicit_address_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("api.sqlite");
        let port = available_port();
        {
            let store = SqliteStore::open_path(&path).unwrap();
            store.set_setting(API_ENABLED_KEY, "true").await.unwrap();
            store
                .set_setting(API_TOKEN_KEY, "existing-token")
                .await
                .unwrap();
            let runtime = ApiRuntime::new();
            let status = apply_api_config(&store, &runtime, host(), true, Some(port), None)
                .await
                .unwrap();
            assert!(status.running, "{:?}", status.last_error);
            assert_eq!(status.bind_address, "127.0.0.1");
            assert_eq!(
                runtime
                    .server
                    .lock()
                    .await
                    .as_ref()
                    .unwrap()
                    .local_addr()
                    .ip(),
                DEFAULT_BIND_ADDRESS
            );
            let status =
                apply_api_config(&store, &runtime, host(), true, None, Some("0.0.0.0".into()))
                    .await
                    .unwrap();
            assert!(status.running, "{:?}", status.last_error);
            assert_eq!(
                runtime
                    .server
                    .lock()
                    .await
                    .as_ref()
                    .unwrap()
                    .local_addr()
                    .ip()
                    .to_string(),
                "0.0.0.0"
            );
            assert_eq!(status.token.as_deref(), Some("existing-token"));
            stop_server(&runtime).await;
        }
        let store = SqliteStore::open_path(&path).unwrap();
        let runtime = ApiRuntime::new();
        let status = apply_api_config(&store, &runtime, host(), true, None, None)
            .await
            .unwrap();
        assert!(status.running, "{:?}", status.last_error);
        assert_eq!(status.bind_address, "0.0.0.0");
        assert_eq!(status.port, port);
        assert_eq!(status.token.as_deref(), Some("existing-token"));
        stop_server(&runtime).await;
    }

    #[tokio::test]
    async fn invalid_address_preserves_live_listener_and_persisted_configuration() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let before = apply_api_config(&store, &runtime, host(), true, Some(available_port()), None)
            .await
            .unwrap();
        assert!(before.running);
        assert!(apply_api_config(
            &store,
            &runtime,
            host(),
            false,
            Some(43211),
            Some("not-an-address".into())
        )
        .await
        .is_err());
        assert_eq!(current_status(&store, &runtime).await, before);
        stop_server(&runtime).await;
    }

    #[tokio::test]
    async fn ipv6_setting_binds_and_persists_canonical_literal() {
        let probe = std::net::TcpListener::bind(("::1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let status = apply_api_config(
            &store,
            &runtime,
            host(),
            true,
            Some(port),
            Some("0:0:0:0:0:0:0:1".into()),
        )
        .await
        .unwrap();
        assert!(status.running, "{:?}", status.last_error);
        assert_eq!(status.bind_address, "::1");
        assert_eq!(
            store
                .get_setting(API_BIND_ADDRESS_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some("::1")
        );
        assert!(runtime
            .server
            .lock()
            .await
            .as_ref()
            .unwrap()
            .local_addr()
            .is_ipv6());
        stop_server(&runtime).await;
    }
    #[tokio::test]
    async fn invalid_saved_address_can_be_disabled_without_broadening() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let initial =
            apply_api_config(&store, &runtime, host(), true, Some(available_port()), None)
                .await
                .unwrap();
        assert!(initial.running);
        store
            .set_setting(API_BIND_ADDRESS_KEY, "damaged")
            .await
            .unwrap();
        assert!(apply_api_config(&store, &runtime, host(), true, None, None)
            .await
            .is_err());
        let disabled = apply_api_config(&store, &runtime, host(), false, None, None)
            .await
            .unwrap();
        assert!(!disabled.running);
        assert!(!disabled.enabled);
        assert_eq!(disabled.bind_address, "damaged");
        assert!(disabled.last_error.is_some());
    }

    // --- P1 frame/coverage composition adapters (`03 §7f.1/.2/.6`) ------------------

    /// OCR provider that aborts the test if a frame/coverage read ever invokes it.
    struct PanicOcr;

    #[async_trait]
    impl traits::OcrProvider for PanicOcr {
        async fn recognize(
            &self,
            _frame: &traits::CapturedFrame,
        ) -> anyhow::Result<traits::OcrResult> {
            panic!("frame/coverage reads must never invoke OCR");
        }
    }

    /// A capture source that never yields frames (intent transitions only).
    struct IdleCapture;

    #[async_trait]
    impl traits::CaptureSource for IdleCapture {
        fn monitors(&self) -> Vec<traits::MonitorInfo> {
            vec![]
        }
        async fn next_frame(&mut self) -> anyhow::Result<Option<traits::CapturedFrame>> {
            std::future::pending().await
        }
    }

    /// A source whose first read fails (one observed worker error), then idles.
    struct FailOnceCapture {
        failed: bool,
    }

    #[async_trait]
    impl traits::CaptureSource for FailOnceCapture {
        fn monitors(&self) -> Vec<traits::MonitorInfo> {
            vec![]
        }
        async fn next_frame(&mut self) -> anyhow::Result<Option<traits::CapturedFrame>> {
            if !self.failed {
                self.failed = true;
                anyhow::bail!("stub capture source read failure");
            }
            std::future::pending().await
        }
    }

    /// Real store + real kernel whose OCR and capture factory panic on use: read-only
    /// composition must never start capture or run models.
    fn read_only_kernel(store: &Arc<SqliteStore>, frames_dir: &std::path::Path) -> Kernel {
        Kernel::new(
            store.clone(),
            Arc::new(PanicOcr),
            Arc::new(|_, _| -> anyhow::Result<Box<dyn traits::CaptureSource>> {
                panic!("frame/coverage reads must never construct a capture source")
            }),
            frames_dir.to_path_buf(),
            traits::Readiness::default(),
        )
    }

    /// Seeds one frame through the production insert seam; `text = Some(..)` adds a real
    /// `frame_text` row (empty strings included).
    async fn seed_frame(store: &SqliteStore, captured_at: i64, text: Option<&str>) -> i64 {
        let id = store
            .insert_frame(traits::NewFrame {
                captured_at,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: format!("frames/{captured_at}.webp"),
                content_hash: format!("hash-{captured_at}"),
                app_hint: Some("VS Code".into()),
                window_title: Some("main.rs".into()),
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .unwrap();
        if let Some(text) = text {
            store
                .insert_ocr(
                    id,
                    traits::OcrResult {
                        text: text.to_string(),
                        mean_confidence: 0.9,
                        engine: "winrt".into(),
                        spans: vec![],
                    },
                )
                .await
                .unwrap();
        }
        id
    }

    #[tokio::test]
    async fn frame_page_forwards_real_store_pages_without_search_or_generation() {
        let store = SqliteStore::open_in_memory().unwrap();
        let full = "full stored evidence\n".repeat(500);
        let first = seed_frame(&store, 1000, Some(&full)).await;
        let second = seed_frame(&store, 1001, Some("")).await;
        let third = seed_frame(&store, 1002, None).await;

        let page = compose_frame_page(
            &store,
            FramePageRequest {
                window: EvidenceWindow {
                    from: 1000,
                    to: 1003,
                },
                filters: traits::FrameFilters::default(),
                limit: 2,
                continuation: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            page.window,
            EvidenceWindow {
                from: 1000,
                to: 1003
            }
        );
        assert_eq!(page.snapshot.snapshot_max_frame_id, third);
        assert!(page.observed_at > 0);
        let ids: Vec<i64> = page.items.iter().map(|item| item.frame_id).collect();
        assert_eq!(
            ids,
            vec![first, second],
            "chronological membership, never ranked"
        );
        assert_eq!(page.items[0].raw_text.as_deref(), Some(full.as_str()));
        assert_eq!(page.items[1].text_state, traits::FrameTextState::Available);
        let next = page.next_position.expect("a third stored frame follows");

        let rest = compose_frame_page(
            &store,
            FramePageRequest {
                window: EvidenceWindow {
                    from: 1000,
                    to: 1003,
                },
                filters: traits::FrameFilters::default(),
                limit: 2,
                continuation: Some(traits::FrameContinuation {
                    snapshot: page.snapshot.clone(),
                    after: next,
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(rest.items.len(), 1);
        assert_eq!(rest.items[0].frame_id, third);
        assert_eq!(
            rest.items[0].text_state,
            traits::FrameTextState::NotAvailable
        );
        assert!(rest.items[0].raw_text.is_none() && rest.items[0].content_text.is_none());
        assert!(rest.next_position.is_none());
    }

    #[tokio::test]
    async fn frame_page_propagates_typed_request_and_history_errors() {
        let store = SqliteStore::open_in_memory().unwrap();
        let frame = seed_frame(&store, 1000, None).await;

        // The shared validator rejects a zero limit before any SQL — never a fake page.
        let invalid = compose_frame_page(
            &store,
            FramePageRequest {
                window: EvidenceWindow { from: 0, to: 2000 },
                filters: traits::FrameFilters::default(),
                limit: 0,
                continuation: None,
            },
        )
        .await;
        assert!(matches!(invalid, Err(HistoryReadError::InvalidRequest)));

        // A continuation frozen against a different history is 409 material, not a restart.
        let changed = compose_frame_page(
            &store,
            FramePageRequest {
                window: EvidenceWindow { from: 0, to: 2000 },
                filters: traits::FrameFilters::default(),
                limit: 2,
                continuation: Some(traits::FrameContinuation {
                    snapshot: traits::FrameSnapshot {
                        history_id: "00000000-0000-4000-8000-000000000000".into(),
                        snapshot_max_frame_id: frame,
                    },
                    after: traits::FramePosition {
                        captured_at: 1000,
                        frame_id: frame,
                    },
                }),
            },
        )
        .await;
        assert!(matches!(changed, Err(HistoryReadError::HistoryChanged)));
    }

    #[tokio::test]
    async fn capture_coverage_composes_real_store_facts_settings_and_policy() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        store
            .set_setting("storage.retention_days", "7")
            .await
            .unwrap();
        let frames_dir = tempfile::tempdir().unwrap();
        let kernel = read_only_kernel(&store, frames_dir.path());

        let window = EvidenceWindow {
            from: 0,
            to: i64::MAX,
        };
        let initial = compose_capture_coverage(&store, &kernel, window)
            .await
            .unwrap();
        assert!(
            !initial.capture_enabled,
            "disabled intent is not a health claim"
        );
        assert_eq!(initial.frame_id_high_water, 0);
        assert!(
            initial.oldest_frame.is_none()
                && initial.newest_frame.is_none()
                && initial.newest_captured_at.is_none()
        );
        assert_eq!(
            (initial.frames_stored, initial.frames_text_available),
            (0, 0)
        );
        assert_eq!(initial.image_retention_days, 7, "real persisted setting");
        assert_eq!(initial.retention_scope, RetentionScope::ImagesOnly);
        assert_eq!(initial.saved_report_retention.terminal_days, 90);
        assert_eq!(
            initial.saved_report_retention.exempt_phases,
            vec![
                ReportRunPhase::Queued,
                ReportRunPhase::Running,
                ReportRunPhase::Blocked
            ]
        );
        assert_eq!(
            initial.saved_report_retention.exempt_delivery_states,
            vec![
                ReportDeliveryState::Pending,
                ReportDeliveryState::Sending,
                ReportDeliveryState::UnknownDelivery,
                ReportDeliveryState::Conflict
            ]
        );
        assert!(
            initial
                .saved_report_retention
                .failed_delivery_with_pending_retry_exempt
        );
        let observation = &initial.capture_observations;
        assert!(
            observation.truncated,
            "query predates this process; unknown is explicit"
        );
        assert_eq!(
            observation.observation_started_at,
            observation.unavailable_before
        );
        assert_eq!(observation.intervals.len(), 1);
        assert_eq!(
            observation.intervals[0].start,
            observation.observation_started_at
        );
        assert!(observation.intervals[0].end.is_none());
        assert_eq!(
            observation.intervals[0].reason,
            CaptureInterruptionReason::CaptureStopped
        );

        // Real stored facts: empty stored OCR counts as available, absent OCR doesn't;
        // global extrema use chronological order, independent of the requested window.
        let newest = seed_frame(&store, 100, Some("")).await;
        let oldest = seed_frame(&store, -10, None).await;
        let tied_newest = seed_frame(&store, 100, None).await;
        let current =
            compose_capture_coverage(&store, &kernel, EvidenceWindow { from: 100, to: 101 })
                .await
                .unwrap();
        assert_eq!(current.frames_stored, 2);
        assert_eq!(current.frames_text_available, 1);
        assert_eq!(
            current.oldest_frame,
            Some(traits::FrameReference {
                frame_id: oldest,
                captured_at: -10
            })
        );
        assert_eq!(
            current.newest_frame,
            Some(traits::FrameReference {
                frame_id: tied_newest,
                captured_at: 100
            })
        );
        assert_eq!(current.newest_captured_at, Some(100));
        assert!(tied_newest > newest);
        assert_eq!(current.frame_id_high_water, tied_newest);
        // The read is inert: the setting it consumed is unchanged (the panicking OCR/
        // capture stubs above would have aborted the test on any capture/model path).
        assert_eq!(
            store
                .get_setting("storage.retention_days")
                .await
                .unwrap()
                .as_deref(),
            Some("7")
        );
    }

    #[tokio::test]
    async fn capture_coverage_tracks_controller_intent_not_capture_health() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let frames_dir = tempfile::tempdir().unwrap();
        let kernel = Kernel::new(
            store.clone(),
            Arc::new(PanicOcr),
            Arc::new(|_, _| -> anyhow::Result<Box<dyn traits::CaptureSource>> {
                Ok(Box::new(IdleCapture))
            }),
            frames_dir.path().to_path_buf(),
            traits::Readiness::default(),
        );
        let window = EvidenceWindow {
            from: 0,
            to: i64::MAX,
        };
        let before = compose_capture_coverage(&store, &kernel, window)
            .await
            .unwrap();
        assert!(!before.capture_enabled);

        kernel.start_capture().await.unwrap();
        let running = compose_capture_coverage(&store, &kernel, window)
            .await
            .unwrap();
        assert!(
            running.capture_enabled,
            "an accepted start is intent, not a health probe"
        );
        assert!(
            running
                .capture_observations
                .intervals
                .iter()
                .all(|interval| interval.end.is_some()),
            "a real start closes the known pause"
        );

        kernel.stop_capture().await;
        let stopped = compose_capture_coverage(&store, &kernel, window)
            .await
            .unwrap();
        assert!(!stopped.capture_enabled);
        assert!(
            stopped
                .capture_observations
                .intervals
                .last()
                .unwrap()
                .end
                .is_none(),
            "stop records an open observed pause"
        );
    }

    #[tokio::test]
    async fn capture_coverage_maps_observed_worker_errors_exhaustively() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let frames_dir = tempfile::tempdir().unwrap();
        let kernel = Kernel::new(
            store.clone(),
            Arc::new(PanicOcr),
            Arc::new(|_, _| -> anyhow::Result<Box<dyn traits::CaptureSource>> {
                Ok(Box::new(FailOnceCapture { failed: false }))
            }),
            frames_dir.path().to_path_buf(),
            traits::Readiness::default(),
        );
        kernel.start_capture().await.unwrap();
        // Wait for the real controller loop to observe the source read failure.
        let observed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let (_, observations) = kernel.capture_observations(0, i64::MAX).await.unwrap();
                if observations.intervals.iter().any(|interval| {
                    matches!(
                        interval.reason,
                        kernel::CaptureInterruptionReason::WorkerError
                    )
                }) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        observed.expect("the controller observes the source failure as a worker error");

        let window = EvidenceWindow {
            from: 0,
            to: i64::MAX,
        };
        let running = compose_capture_coverage(&store, &kernel, window)
            .await
            .unwrap();
        assert!(
            running.capture_enabled,
            "observed errors do not fabricate a health verdict"
        );
        let worker_error = running
            .capture_observations
            .intervals
            .iter()
            .find(|interval| interval.reason == CaptureInterruptionReason::WorkerError)
            .expect("kernel WorkerError maps to the wire worker_error reason");
        assert!(worker_error.end.is_none(), "still open at read time");

        kernel.stop_capture().await;
        let stopped = compose_capture_coverage(&store, &kernel, window)
            .await
            .unwrap();
        assert!(!stopped.capture_enabled);
        assert!(
            stopped
                .capture_observations
                .intervals
                .iter()
                .all(|interval| matches!(
                    interval.reason,
                    CaptureInterruptionReason::CaptureStopped
                        | CaptureInterruptionReason::WorkerError
                )),
            "only observed stop/error reasons cross the wire"
        );
    }

    fn report_run_fixture(
        run_id: &str,
        phase: ReportRunPhase,
        window_start: i64,
        updated_at: i64,
    ) -> traits::ReportRun {
        traits::ReportRun {
            requested_range: None,
            run_id: run_id.into(),
            source_id: "profile-fixture".into(),
            kind: ReportRunKind::Daily,
            schedule_id: None,
            revision_of: None,
            window_start,
            window_end: window_start + 500,
            timezone: "Europe/Paris".into(),
            local_date: "2026-01-01".into(),
            phase,
            outcome: None,
            error: None,
            result: None,
            config_snapshot: "{}".into(),
            created_at: 1,
            updated_at,
        }
    }

    fn report_delivery_fixture(
        id: &str,
        run_id: &str,
        state: ReportDeliveryState,
        next_attempt_at: Option<i64>,
    ) -> traits::ReportDelivery {
        traits::ReportDelivery {
            id: id.into(),
            run_id: run_id.into(),
            destination: traits::ReportDestination::Vault,
            state,
            revision_of: None,
            destination_snapshot: "{}".into(),
            path: None,
            hash: None,
            recovery_path: None,
            chunk_index: None,
            chunk_count: None,
            message_id: None,
            error: None,
            next_attempt_at,
            attempt: 0,
            created_at: 1,
            updated_at: 1,
        }
    }

    /// Pins the reported `saved_report_retention` policy to the store's real pruning:
    /// 90-day terminal expiry with exactly the queued/running/blocked phase and
    /// pending/sending/unknown/conflict (plus failed-with-pending-retry) exemptions.
    #[tokio::test]
    async fn saved_report_retention_policy_matches_real_store_pruning() {
        let store = SqliteStore::open_in_memory().unwrap();
        let now = now_ms();
        let day: i64 = 24 * 60 * 60 * 1000;
        let old = now - 100 * day;

        // (run_id, phase, delivery states, failed-delivery next_attempt_at)
        let fixtures: Vec<(&str, ReportRunPhase, Vec<ReportDeliveryState>, Option<i64>)> = vec![
            ("terminal", ReportRunPhase::Finished, vec![], None),
            ("queued", ReportRunPhase::Queued, vec![], None),
            ("running", ReportRunPhase::Running, vec![], None),
            ("blocked", ReportRunPhase::Blocked, vec![], None),
            (
                "pending",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::Pending],
                None,
            ),
            (
                "sending",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::Sending],
                None,
            ),
            (
                "unknown",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::UnknownDelivery],
                None,
            ),
            (
                "conflict",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::Conflict],
                None,
            ),
            (
                "failed-retry",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::Failed],
                Some(now + day),
            ),
            (
                "failed-terminal",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::Failed],
                None,
            ),
            (
                "succeeded",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::Succeeded],
                None,
            ),
            (
                "reviewed",
                ReportRunPhase::Finished,
                vec![ReportDeliveryState::Reviewed],
                None,
            ),
        ];
        for (index, (run_id, phase, states, failed_next)) in fixtures.iter().enumerate() {
            let window_start = 1_000_000 + (index as i64) * 1_000;
            store
                .create_report_run(report_run_fixture(run_id, *phase, window_start, old))
                .await
                .unwrap();
            for (slot, state) in states.iter().enumerate() {
                store
                    .create_report_delivery(report_delivery_fixture(
                        &format!("{run_id}-delivery-{slot}"),
                        run_id,
                        *state,
                        if *state == ReportDeliveryState::Failed {
                            *failed_next
                        } else {
                            None
                        },
                    ))
                    .await
                    .unwrap();
            }
        }

        let pruned = store.prune_report_runs(now - 90 * day).await.unwrap();
        let remaining: std::collections::BTreeSet<String> = store
            .list_report_runs(1000, None)
            .await
            .unwrap()
            .into_iter()
            .map(|run| run.run_id)
            .collect();
        let expected_kept: std::collections::BTreeSet<String> = [
            "queued",
            "running",
            "blocked",
            "pending",
            "sending",
            "unknown",
            "conflict",
            "failed-retry",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            remaining, expected_kept,
            "the real prune keeps exactly the policy's exempt runs"
        );
        assert_eq!(
            pruned, 4,
            "terminal, failed-terminal, succeeded and reviewed runs expire"
        );

        // The policy the coverage read reports must agree with the observed behavior.
        let policy = saved_report_retention_policy();
        for (run_id, phase, states, failed_next) in &fixtures {
            let policy_keeps = policy.exempt_phases.contains(phase)
                || states
                    .iter()
                    .any(|state| policy.exempt_delivery_states.contains(state))
                || (policy.failed_delivery_with_pending_retry_exempt
                    && states.contains(&ReportDeliveryState::Failed)
                    && failed_next.is_some());
            assert_eq!(
                remaining.contains(*run_id),
                policy_keeps,
                "policy disagrees with prune behavior for {run_id}"
            );
        }
    }

    // --- P3 saved-report read composition (`03 §7f.5/.6`, §8c) ---------------------

    /// Answer provider that aborts the test if a report read ever reaches generation.
    struct PanicAnswer;

    #[async_trait]
    impl AnswerProvider for PanicAnswer {
        async fn answer(
            &self,
            _query: &str,
            _context: &[RetrievedChunk],
            _opts: traits::AnswerOpts,
            _tx: tokio::sync::mpsc::Sender<traits::AnswerDelta>,
        ) -> anyhow::Result<()> {
            panic!("saved-report reads must never invoke the answer model");
        }
    }

    fn saved_report_fixture(markdown: &str) -> traits::SavedReport {
        traits::SavedReport {
            markdown: markdown.into(),
            model: Some("fixture-model".into()),
            frame_ids: vec![11, 12, 13],
            session_ids: vec![5],
            cited_frame_ids: vec![11],
            frames_sampled: 9,
            frames_summarized: 3,
            periods_total: 2,
            periods_covered: 2,
            passes: 1,
            truncated: false,
        }
    }

    fn saved_run_fixture(
        run_id: &str,
        created_at: i64,
        window_start: i64,
        phase: ReportRunPhase,
        result: Option<traits::SavedReport>,
    ) -> traits::ReportRun {
        traits::ReportRun {
            requested_range: None,
            run_id: run_id.into(),
            source_id: "profile-fixture".into(),
            kind: ReportRunKind::Daily,
            schedule_id: None,
            revision_of: None,
            window_start,
            window_end: window_start + 500,
            timezone: "Europe/Paris".into(),
            local_date: "2026-01-01".into(),
            phase,
            outcome: Some(traits::ReportOutcome::Ok),
            error: None,
            result,
            config_snapshot: "{\"PRIVATE\":true}".into(),
            created_at,
            updated_at: created_at + 1,
        }
    }

    /// A host whose report reads forward to the concrete `SqliteStore` exactly like
    /// `TauriApiHost`, with provider/search/write spies that abort if ever reached.
    struct ReportHost {
        store: Arc<SqliteStore>,
    }

    #[async_trait]
    impl ApiHost for ReportHost {
        fn version(&self) -> String {
            "report-composition".into()
        }
        fn started_at_ms(&self) -> i64 {
            0
        }
        async fn is_capturing(&self) -> bool {
            false
        }
        fn store(&self) -> Arc<dyn Store> {
            self.store.clone()
        }
        async fn settings(&self) -> Settings {
            Settings::default()
        }
        async fn frame_detail(&self, _frame_id: i64) -> anyhow::Result<Option<FrameDetail>> {
            Ok(None)
        }
        async fn frame_image(&self, _frame_id: i64) -> anyhow::Result<Option<FrameImage>> {
            Ok(None)
        }
        async fn ask_context(
            &self,
            _query: &str,
            _top_k: u32,
            _session_id: Option<i64>,
        ) -> anyhow::Result<Vec<RetrievedChunk>> {
            panic!("report reads must never search")
        }
        async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
            Some(Arc::new(PanicAnswer) as Arc<dyn AnswerProvider>)
        }
        async fn add_mark(
            &self,
            _frame_id: Option<i64>,
            _capture_now: bool,
            _note: Option<String>,
        ) -> anyhow::Result<i64> {
            panic!("report reads must never write")
        }
        async fn resolve_mark(&self, _mark_id: i64) -> anyhow::Result<()> {
            panic!("report reads must never write")
        }
        async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
            Ok(None)
        }
        async fn export_frames_page(
            &self,
            _after_id: i64,
            _from: Option<i64>,
            _to: Option<i64>,
            _limit: u32,
        ) -> anyhow::Result<Vec<ExportFrameRow>> {
            Ok(vec![])
        }
        async fn report_page(
            &self,
            request: ReportPageRequest,
        ) -> Result<StoredReportPage, HistoryReadError> {
            compose_report_page(self.store.as_ref(), request).await
        }
        async fn report_detail(
            &self,
            run_id: &str,
        ) -> Result<Option<ReportDetail>, HistoryReadError> {
            compose_report_detail(self.store.as_ref(), run_id).await
        }
    }

    #[tokio::test]
    async fn report_reads_compose_real_store_without_generation_or_mutation() {
        let store = SqliteStore::open_in_memory().unwrap();
        let saved = saved_report_fixture("# saved markdown\nprovenance");
        let run = saved_run_fixture(
            "run-a",
            100,
            1_000,
            ReportRunPhase::Queued,
            Some(saved.clone()),
        );
        store.create_report_run(run.clone()).await.unwrap();

        let page = compose_report_page(
            &store,
            ReportPageRequest {
                window: EvidenceWindow {
                    from: 0,
                    to: i64::MAX,
                },
                filters: traits::ReportFilters::default(),
                limit: 100,
                continuation: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].run_id, "run-a");
        assert!(page.next_position.is_none());
        assert!(
            page.snapshot.is_some(),
            "a non-empty page carries the frozen max tuple"
        );

        // A saved result is exposed independent of phase; a missing run is None (→404).
        let detail = compose_report_detail(&store, "run-a")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.result, Some(saved));
        assert!(compose_report_detail(&store, "absent")
            .await
            .unwrap()
            .is_none());
        // The read is side-effect free: the stored run is byte-for-byte unchanged.
        assert_eq!(store.get_report_run("run-a").await.unwrap().unwrap(), run);
    }

    #[tokio::test]
    async fn report_routes_serve_real_store_over_http_without_generation() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let saved = saved_report_fixture("# http saved markdown");
        store
            .create_report_run(saved_run_fixture(
                "run-http",
                100,
                1_000,
                ReportRunPhase::Running,
                Some(saved),
            ))
            .await
            .unwrap();
        let token: SharedToken = Arc::new(RwLock::new("report-composition-token".into()));
        let server = ApiServer::start(
            Arc::new(ReportHost {
                store: store.clone(),
            }),
            token,
            0,
        )
        .await
        .unwrap();
        let base = format!("http://127.0.0.1:{}", server.local_addr().port());
        let client = reqwest::Client::new();
        let list = client
            .get(format!("{base}/v1/reports?from=0&to={}", i64::MAX))
            .bearer_auth("report-composition-token")
            .send()
            .await
            .unwrap();
        assert_eq!(list.status(), 200);
        let body: serde_json::Value = list.json().await.unwrap();
        assert_eq!(body["items"][0]["run_id"], "run-http");
        assert!(body["items"][0].get("config_snapshot").is_none());
        let detail = client
            .get(format!("{base}/v1/reports/run-http"))
            .bearer_auth("report-composition-token")
            .send()
            .await
            .unwrap();
        assert_eq!(detail.status(), 200);
        let body: serde_json::Value = detail.json().await.unwrap();
        assert_eq!(body["result"]["markdown"], "# http saved markdown");
        assert!(body.get("config_snapshot").is_none());
        server.stop().await;
        // No state mutation: still exactly the one seeded run.
        assert_eq!(store.list_report_runs(1000, None).await.unwrap().len(), 1);
    }
}
