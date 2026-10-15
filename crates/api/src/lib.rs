//! The opt-in local HTTP API (0.3.0 PR7; `03 §7c`, D11/D12).
//!
//! A small [`axum`] server that exposes the same core queries the UI uses — search,
//! ask, frames, where-was-i, marks — plus a streamed JSON export, over
//! a configurable IPv4 or IPv6 address for scripts and agents. It is **off by default**;
//! the default address is `127.0.0.1`. Every request requires a **bearer token**.
//! This is plain HTTP: any reachable token holder can read history (`03 §7c`).
//! Clients use the computer's address, or `127.0.0.1` locally, as their destination.
//!
//! The crate depends on [`traits`] only (`03 §2`): everything it needs from the app is
//! behind the [`ApiHost`] trait, which the composition root implements over the concrete
//! store/kernel and wires in. When the API is disabled the server is simply never
//! constructed — but the crate (and its export code path) is still linked, so the
//! Settings "Export…" button works with the API off.

pub mod auth;
pub mod error;
pub mod export;
pub mod extract;
pub mod routes;
pub mod sessions;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::watch;
use tokio::task::{AbortHandle, JoinHandle};
use traits::{
    AnswerProvider, ExportFrameRow, FrameDetail, ResumeContext, RetrievedChunk, SearchQuery,
    Settings, Store,
};

/// The default port for the local API (`api.port`, configurable; `03 §8`, D11).
pub const DEFAULT_PORT: u16 = 43210;

/// Missing configuration remains loopback, including on existing enabled installs.
pub const DEFAULT_BIND_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// The live bearer token, shared with the auth middleware. Held behind an `RwLock` and
/// read per request, so regenerating the token takes effect immediately without
/// restarting the server (`regenerate_api_token`, `03 §7c`).
pub type SharedToken = Arc<RwLock<String>>;

/// A stored frame image ready to serve as the `?image=1` response body (`GET
/// /v1/frames/{id}`). `content_type` is the MIME derived from the stored file's
/// extension (WebP for current frames; JPEG for pre-WebP ones).
pub struct FrameImage {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

/// Everything the HTTP surface needs from the app, behind one trait so `crates/api`
/// depends on `traits` only (`03 §2`). The composition root implements it over the
/// concrete `SqliteStore` + `Kernel`, reusing the same helpers the Tauri commands use so
/// the API and the UI answer identically. Integration tests implement it over a fixture
/// store.
#[async_trait]
pub trait ApiHost: Send + Sync + 'static {
    /// The app version string for `GET /v1/health`.
    fn version(&self) -> String;
    /// When the API host was constructed, unix epoch ms — the health uptime anchor.
    fn started_at_ms(&self) -> i64;
    /// Whether the capture loop is currently running (health `capturing`).
    async fn is_capturing(&self) -> bool;

    /// The store, for the trait-level reads the API reuses verbatim (`hybrid_search`,
    /// `list_marks`).
    fn store(&self) -> Arc<dyn Store>;

    /// Effective settings (retrieval depth, chrome default, thinking) — the API mirrors
    /// the UI's defaults when a request omits a parameter.
    async fn settings(&self) -> Settings;

    /// Full detail for a frame, or `None` if it does not exist (`GET /v1/frames/{id}`).
    async fn frame_detail(&self, frame_id: i64) -> anyhow::Result<Option<FrameDetail>>;

    /// The stored image bytes for a frame, or `None` if the frame does not exist **or**
    /// its image was retention-purged (`?image=1`). The route distinguishes the two.
    async fn frame_image(&self, frame_id: i64) -> anyhow::Result<Option<FrameImage>>;

    /// Grounding context for `/v1/ask` — the same hybrid-search + OCR-hydrate path the
    /// `ask` command uses. `session_id = Some(id)` (0.4.0 PR6) scopes retrieval to that
    /// session's own frames so the answer cites only in-session frames; `None` is the
    /// unchanged frame-level path (D10).
    async fn ask_context(
        &self,
        query: &str,
        top_k: u32,
        session_id: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>>;

    /// The active answer provider, or `None` when no answer model is loaded (`/v1/ask`
    /// then returns 503).
    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>>;

    /// Create a mark (`POST /v1/marks`). Exactly one of `frame_id` or `capture_now`;
    /// `capture_now` bypasses the diff gate for one frame (`03 §7b`, D8).
    async fn add_mark(
        &self,
        frame_id: Option<i64>,
        capture_now: bool,
        note: Option<String>,
    ) -> anyhow::Result<i64>;

    /// Resolve (or dismiss) a mark (`POST /v1/marks/{id}/resolve`).
    async fn resolve_mark(&self, mark_id: i64) -> anyhow::Result<()>;

    /// The where-was-i heuristic result, or `None` when nothing qualifies
    /// (`GET /v1/context/where-was-i`).
    async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>>;

    /// One keyset page of the streaming export (`GET /v1/export`); see the store method
    /// of the same name.
    async fn export_frames_page(
        &self,
        after_id: i64,
        from: Option<i64>,
        to: Option<i64>,
        limit: u32,
    ) -> anyhow::Result<Vec<ExportFrameRow>>;
}

/// The axum handler state: the app seam + the live bearer token.
#[derive(Clone)]
pub struct ApiState {
    pub host: Arc<dyn ApiHost>,
    pub token: SharedToken,
}

/// A running local API server. Dropping the handle does **not** stop it — call
/// [`ApiServer::stop`]. The composition root keeps it in an `Option` slot so start/stop
/// on the settings toggle is idempotent (mirrors the capture handle, `03 §6`).
pub struct ApiServer {
    stop: watch::Sender<bool>,
    join: JoinHandle<()>,
    local_addr: SocketAddr,
}

impl ApiServer {
    /// Starts on the default loopback address. Explicit network access uses `start_on`.
    pub async fn start(
        host: Arc<dyn ApiHost>,
        token: SharedToken,
        port: u16,
    ) -> std::io::Result<ApiServer> {
        Self::start_on(host, token, DEFAULT_BIND_ADDRESS, port).await
    }

    /// Binds the selected literal address before spawning, surfacing any bind error.
    pub async fn start_on(
        host: Arc<dyn ApiHost>,
        token: SharedToken,
        bind_address: IpAddr,
        port: u16,
    ) -> std::io::Result<ApiServer> {
        let socket = match bind_address {
            IpAddr::V4(_) => tokio::net::TcpSocket::new_v4()?,
            IpAddr::V6(_) => tokio::net::TcpSocket::new_v6()?,
        };
        // Windows otherwise permits wildcard and interface-specific listeners to
        // share a port. Local MCP could reach the other service despite a successful
        // API bind. Claim the port exclusively before binding any interface.
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawSocket;
            use windows::Win32::Networking::WinSock::{
                setsockopt, WSAGetLastError, SOCKET, SOCKET_ERROR, SOL_SOCKET, SO_EXCLUSIVEADDRUSE,
            };
            let enabled = 1_i32.to_ne_bytes();
            // SAFETY: socket owns a live Winsock socket; the option byte slice has
            // the required BOOL width and remains valid for this synchronous call.
            let result = unsafe {
                setsockopt(
                    SOCKET(socket.as_raw_socket() as usize),
                    SOL_SOCKET,
                    SO_EXCLUSIVEADDRUSE,
                    Some(&enabled),
                )
            };
            if result == SOCKET_ERROR {
                // SAFETY: retrieves this thread's error from the failed Winsock call.
                return Err(std::io::Error::from_raw_os_error(unsafe {
                    WSAGetLastError().0
                }));
            }
        }
        socket.bind(SocketAddr::from((bind_address, port)))?;
        let listener = socket.listen(128)?;
        let local_addr = listener.local_addr()?;
        let router = build_router(ApiState { host, token });
        let (stop, mut stop_rx) = watch::channel(false);
        let join = tokio::spawn(async move {
            let shutdown = async move {
                // Resolve when someone flips the stop flag.
                let _ = stop_rx.wait_for(|stopped| *stopped).await;
            };
            if let Err(e) = axum::serve(listener, router.into_make_service())
                .with_graceful_shutdown(shutdown)
                .await
            {
                tracing::error!(error = %e, "local API server exited with error");
            }
        });
        tracing::info!(%local_addr, "local API listening");
        Ok(ApiServer {
            stop,
            join,
            local_addr,
        })
    }

    /// The actual bound IPv4 or IPv6 socket address. Used to report the live port;
    /// clients must use a reachable host address rather than this listen address.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Graceful shutdown, bounded. Signals the serve task to stop accepting and drain,
    /// then waits up to 3 s; a still-open connection (a long `/v1/ask` SSE) that outlasts
    /// the grace window is force-aborted so app exit never wedges.
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let abort: AbortHandle = self.join.abort_handle();
        if tokio::time::timeout(Duration::from_secs(3), self.join)
            .await
            .is_err()
        {
            tracing::warn!("local API graceful shutdown timed out; aborting");
            abort.abort();
        }
    }
}

/// Builds the v1 router with the bearer-auth layer applied to **every** route (health
/// included — the whole surface is private). The ask + export routes are added by the
/// PR7 SSE/export module.
pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/search", get(routes::search))
        .route("/v1/frames/{id}", get(routes::frame))
        .route("/v1/context/where-was-i", get(routes::where_was_i))
        .route("/v1/sessions", get(sessions::list_sessions))
        .route("/v1/sessions/{id}", get(sessions::session_detail))
        .route(
            "/v1/marks",
            get(routes::list_marks).post(routes::create_mark),
        )
        .route("/v1/marks/{id}/resolve", post(routes::resolve_mark))
        .route("/v1/ask", post(routes::ask))
        .route("/v1/export", get(export::export))
        // Unmatched paths still answer on the JSON error contract (not axum's default 404
        // body). Registered before the auth layer so it, too, requires a token.
        .fallback(routes::not_found)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ))
        .with_state(state)
}

/// Generates a fresh bearer token: 32 CSPRNG bytes as 64 lowercase hex chars (`03 §7c`,
/// D11). Panics only if the OS RNG is unavailable (a machine that can't seed randomness
/// can't be trusted to serve a private API anyway).
pub fn generate_token() -> String {
    use std::fmt::Write;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS RNG unavailable for API token generation");
    let mut token = String::with_capacity(64);
    for b in bytes {
        let _ = write!(token, "{b:02x}");
    }
    token
}

/// Wall-clock unix epoch ms (the kernel clock unit). Local to this crate so it doesn't
/// reach into the composition root for a one-liner.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Shared query params for search-like reads. Kept here so `routes` and any future
/// module reuse one definition.
pub(crate) fn build_search_query(
    text: String,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<u32>,
    include_chrome: Option<bool>,
    default_include_chrome: bool,
) -> Result<SearchQuery, error::ApiError> {
    let time_range = match (from, to) {
        (Some(start), Some(end)) => {
            if start > end {
                return Err(error::ApiError::BadRequest(
                    "`from` must be less than or equal to `to`".to_string(),
                ));
            }
            Some(traits::TimeRange { start, end })
        }
        (None, None) => None,
        _ => {
            return Err(error::ApiError::BadRequest(
                "`from` and `to` must be provided together".to_string(),
            ))
        }
    };
    Ok(SearchQuery {
        text,
        // Clamp to a sane ceiling; the store enforces its own hard cap too.
        limit: limit.unwrap_or(20).clamp(1, 100),
        time_range,
        include_chrome: include_chrome.unwrap_or(default_include_chrome),
    })
}
