//! Composition-root wiring for the opt-in local HTTP API (0.3.0 PR7; `03 §7c`, D11/D12).
//!
//! This is the only place the concrete store/kernel meet the `api` crate. [`TauriApiHost`]
//! implements [`api::ApiHost`] over the same helpers the Tauri commands use (`ask_context`,
//! `safe_frame_path`, `kernel::resume::where_was_i`), so the HTTP surface and the UI answer
//! identically. The server lives in an [`ApiRuntime`] slot; enabling/disabling toggles it
//! at runtime, mirroring the capture handle's idempotent start/stop.
//!
//! The `api.enabled`/`api.port`/`api.token` keys are read/written directly on the `settings`
//! table here — deliberately **not** fields of the `Settings` struct — so the bearer token
//! never rides the `get_settings` response into `Settings.ts`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, RwLock};

use async_trait::async_trait;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use traits::{
    AnswerProvider, ApiStatus, ExportFrameRow, ExportRequest, ExportResult, FrameDetail,
    ResumeContext, RetrievedChunk, Settings, Store, Toast, ToastLevel,
};

use api::{ApiHost, ApiServer, FrameImage, SharedToken, DEFAULT_PORT};
use kernel::Kernel;
use store::SqliteStore;

use crate::{ask_context, now_ms, safe_frame_path, AppState};

const API_ENABLED_KEY: &str = "api.enabled";
const API_PORT_KEY: &str = "api.port";
const API_TOKEN_KEY: &str = "api.token";
const API_NETWORK_NOTICE_KEY: &str = "api.network_notice_state";
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

/// Called with `config_lock` held, before changing enabled intent. The single KV
/// write is atomic; this lock serializes startup, configuration and acknowledgement.
async fn initialize_network_notice(store: &SqliteStore) -> Result<(), String> {
    match store.get_setting(API_NETWORK_NOTICE_KEY).await {
        Ok(Some(value)) if value == "pending" || value == "acknowledged" => Ok(()),
        Ok(Some(_)) => Err("invalid API network notice state".to_string()),
        Ok(None) => {
            let enabled = store
                .get_setting(API_ENABLED_KEY)
                .await
                .map_err(|e| format!("could not read API upgrade state: {e}"))?
                .is_some_and(|value| value == "true");
            store
                .set_setting(
                    API_NETWORK_NOTICE_KEY,
                    if enabled { "pending" } else { "acknowledged" },
                )
                .await
                .map_err(|e| format!("could not persist API upgrade notice: {e}"))
        }
        Err(e) => Err(format!("could not read API upgrade notice: {e}")),
    }
}

/// The current API status: persisted intent + live running state + last bind error.
pub async fn current_status(store: &SqliteStore, runtime: &ApiRuntime) -> ApiStatus {
    let (enabled, port, token) = read_api_config(store).await;
    let running = runtime.server.lock().await.is_some();
    let mut last_error = runtime
        .last_error
        .lock()
        .expect("api last_error lock")
        .clone();
    let network_notice_pending = match store.get_setting(API_NETWORK_NOTICE_KEY).await {
        Ok(Some(value)) if value == "acknowledged" => false,
        Ok(Some(value)) if value == "pending" => true,
        Ok(None) => enabled,
        result => {
            last_error = Some(match result {
                Err(error) => format!("could not read API upgrade notice: {error}"),
                _ => "invalid API network notice state".to_string(),
            });
            true
        }
    };
    ApiStatus {
        enabled,
        running,
        port,
        token,
        last_error,
        network_notice_pending,
    }
}

/// Persists `{enabled, port}`, generates the token on first enable, then (re)starts or
/// stops the server. A bind failure leaves `enabled = true` persisted, `running = false`,
/// and `last_error` set — the loud guided-change state (`03 §7c`); the command wrapper
/// adds the warning toast (that's *data*, not an `Err`). An **`Err`** is reserved for a
/// failed persistence write: we return before touching the running server, so a disable can
/// never stop the server yet leave `enabled = true` on disk (which would autostart again).
pub async fn apply_api_config(
    store: &SqliteStore,
    runtime: &ApiRuntime,
    host: Arc<dyn ApiHost>,
    enabled: bool,
    port: Option<u16>,
) -> Result<ApiStatus, String> {
    // Serialize the whole transition so overlapping enable/disable calls can't interleave.
    let _config_guard = runtime.config_lock.lock().await;
    apply_api_config_locked(store, runtime, host, enabled, port).await
}

async fn apply_api_config_locked(
    store: &SqliteStore,
    runtime: &ApiRuntime,
    host: Arc<dyn ApiHost>,
    enabled: bool,
    port: Option<u16>,
) -> Result<ApiStatus, String> {
    initialize_network_notice(store).await?;
    let (_, current_port, _) = read_api_config(store).await;
    let target_port = clamp_api_port(port.unwrap_or(current_port));
    // Persist the intent *before* touching the running server; if the DB write fails, bail
    // now so on-disk state and the live server can't diverge.
    store
        .set_setting(API_PORT_KEY, &target_port.to_string())
        .await
        .map_err(|e| format!("could not persist api.port: {e}"))?;
    store
        .set_setting(API_ENABLED_KEY, if enabled { "true" } else { "false" })
        .await
        .map_err(|e| format!("could not persist api.enabled: {e}"))?;

    // Stop any running server first (a config change is a deterministic restart).
    let existing = runtime.server.lock().await.take();
    if let Some(server) = existing {
        server.stop().await;
    }
    *runtime.last_error.lock().expect("api last_error lock") = None;

    if enabled {
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

        match ApiServer::start(host, runtime.token.clone(), target_port).await {
            Ok(server) => {
                *runtime.server.lock().await = Some(server);
            }
            Err(e) => {
                let message = format!("port {target_port} in use ({e})");
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
            initialize_network_notice(store.as_ref()).await?;
            if !enabled {
                return Ok(current_status(store.as_ref(), &runtime).await);
            }
            let host: Arc<dyn ApiHost> = host;
            apply_api_config_locked(store.as_ref(), &runtime, host, true, None).await
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
                    message: format!("Local API: {err} — pick another port"),
                },
            );
        }
    }
}

/// Enable/disable the API (and set the port). Bind failure is loud + guided (`03 §7c`).
#[tauri::command]
pub async fn set_api_config(
    enabled: bool,
    port: Option<u16>,
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
    let status = apply_api_config(store.as_ref(), &state.api_runtime, host, enabled, port).await?;
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
    initialize_network_notice(store.as_ref()).await?;
    Ok(current_status(store.as_ref(), &state.api_runtime).await)
}

fn require_visible_main_window(label: &str, visible: bool) -> Result<(), String> {
    if label != "main" || !visible {
        return Err("acknowledge the API notice in the visible main window".to_string());
    }
    Ok(())
}

async fn persist_notice_acknowledgement(store: &SqliteStore) -> Result<(), String> {
    initialize_network_notice(store).await?;
    store
        .set_setting(API_NETWORK_NOTICE_KEY, "acknowledged")
        .await
        .map_err(|e| format!("could not acknowledge API network notice: {e}"))
}

/// Only the main window's explicit acknowledgement action may consume the notice.
#[tauri::command]
pub async fn acknowledge_api_network_notice(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<ApiStatus, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let _guard = state.api_runtime.config_lock.lock().await;
    require_visible_main_window(
        window.label(),
        window.is_visible().map_err(|e| e.to_string())?,
    )?;
    persist_notice_acknowledgement(store.as_ref()).await?;
    Ok(current_status(store.as_ref(), &state.api_runtime).await)
}

/// Regenerate the bearer token. A running server needs **no** restart — the auth
/// middleware reads the live token per request.
#[tauri::command]
pub async fn regenerate_api_token(state: State<'_, AppState>) -> Result<ApiStatus, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let token = api::generate_token();
    store
        .set_setting(API_TOKEN_KEY, &token)
        .await
        .map_err(|e| e.to_string())?;
    *state.api_runtime.token.write().expect("api token lock") = token;
    Ok(current_status(store.as_ref(), &state.api_runtime).await)
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
    /// survives across a disable. The server binds an ephemeral port (0) so the test never
    /// races a real port.
    #[tokio::test]
    async fn enabling_generates_token_once_and_persists() {
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();

        let status = apply_api_config(&store, &runtime, host(), true, Some(0))
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
        let status2 = apply_api_config(&store, &runtime, host(), true, Some(0))
            .await
            .unwrap();
        assert_eq!(status2.token, Some(token.clone()));

        // Disable then re-enable: still the same token (persisted, not regenerated).
        let disabled = apply_api_config(&store, &runtime, host(), false, None)
            .await
            .unwrap();
        assert!(!disabled.enabled);
        assert!(!disabled.running);
        assert_eq!(disabled.token, Some(token.clone()));

        let reenabled = apply_api_config(&store, &runtime, host(), true, Some(0))
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
        let status = apply_api_config(&store, &runtime, host(), true, Some(port))
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
    async fn enabled_upgrade_shows_bind_change_notice_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notice.sqlite");
        {
            let store = SqliteStore::open_path(&path).unwrap();
            store.set_setting(API_ENABLED_KEY, "true").await.unwrap();
            initialize_network_notice(&store).await.unwrap();
            assert!(
                current_status(&store, &ApiRuntime::new())
                    .await
                    .network_notice_pending
            );
            // Disabling must retain the warning about the prior exposure.
            let status = apply_api_config(&store, &ApiRuntime::new(), host(), false, None)
                .await
                .unwrap();
            assert!(status.network_notice_pending);
        }
        {
            let store = SqliteStore::open_path(&path).unwrap();
            initialize_network_notice(&store).await.unwrap();
            assert!(
                current_status(&store, &ApiRuntime::new())
                    .await
                    .network_notice_pending
            );
            persist_notice_acknowledgement(&store).await.unwrap();
        }
        let store = SqliteStore::open_path(&path).unwrap();
        initialize_network_notice(&store).await.unwrap();
        assert!(
            !current_status(&store, &ApiRuntime::new())
                .await
                .network_notice_pending
        );
    }

    #[test]
    fn notice_acknowledgement_requires_visible_main_window() {
        assert!(require_visible_main_window("main", true).is_ok());
        assert!(require_visible_main_window("main", false).is_err());
        assert!(require_visible_main_window("overlay", true).is_err());
    }

    #[tokio::test]
    async fn disabled_api_has_no_listener() {
        let probe = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let store = SqliteStore::open_in_memory().unwrap();
        let runtime = ApiRuntime::new();
        let status = apply_api_config(&store, &runtime, host(), false, Some(port))
            .await
            .unwrap();
        assert!(!status.enabled);
        assert!(!status.running);
        assert!(!status.network_notice_pending);
        assert!(runtime.server.lock().await.is_none());
        let available = std::net::TcpListener::bind(("0.0.0.0", port)).unwrap();
        assert_eq!(available.local_addr().unwrap().port(), port);
    }

    #[tokio::test]
    async fn invalid_notice_state_blocks_enable_and_surfaces_error() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .set_setting(API_NETWORK_NOTICE_KEY, "invalid")
            .await
            .unwrap();
        let runtime = ApiRuntime::new();
        let error = apply_api_config(&store, &runtime, host(), true, None)
            .await
            .unwrap_err();
        assert!(error.contains("invalid API network notice state"));
        assert!(runtime.server.lock().await.is_none());
        let status = current_status(&store, &runtime).await;
        assert!(status.network_notice_pending);
        assert!(status.last_error.is_some());
        assert!(!status.enabled);
    }
}
