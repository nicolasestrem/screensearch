//! The opt-in local HTTP API (0.3.0 PR7; `03 §7c`, D11/D12).
//!
//! A small [`axum`] server that exposes the same core queries the UI uses — search,
//! ask, frames, where-was-i, marks — plus a streamed JSON export, over
//! a configurable IPv4 or IPv6 address for scripts and agents. It is **off by default**;
//! the default address is `127.0.0.1`. Every request requires a **bearer token**.
//! This is plain HTTP: any reachable token holder can read history (`03 §7c`).
//! Clients use the computer's address, or `127.0.0.1` locally, as their destination.
//!
//! Every listener enforces the connection bounds of `03 §7c` (PR #122 review
//! amendment): at most [`MAX_ACTIVE_CONNECTIONS`] active connections admitted before
//! authentication, a [`REQUEST_HEADER_DEADLINE`] for each request's headers, and a
//! bounded shutdown that leaves no connection task behind.
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
mod history;
mod permissions;
pub mod routes;
pub mod sessions;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder as AutoBuilder;
use hyper_util::service::TowerToHyperService;
use tokio::sync::{watch, Semaphore};
use tokio::task::{JoinHandle, JoinSet};
use traits::{
    AnswerProvider, ExportFrameRow, FrameDetail, ResumeContext, RetrievedChunk, SearchQuery,
    Settings, Store,
};

/// The default port for the local API (`api.port`, configurable; `03 §8`, D11).
pub const DEFAULT_PORT: u16 = 43210;

/// Missing configuration remains loopback, including on existing enabled installs.
pub const DEFAULT_BIND_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// Every enabled listener — loopback included — admits at most this many active
/// connections, counted **before** authentication (`03 §7c`, PR #122 review
/// amendment). A socket beyond the limit is accepted and immediately closed; it never
/// becomes a request task. A fixed implementation constant, not a setting.
pub const MAX_ACTIVE_CONNECTIONS: usize = 64;

/// Each request's headers must be transmitted within this window, enforced with an
/// installed runtime timer; an idle keep-alive connection gets the same window for its
/// next request. This bounds **headers only** — long `/v1/ask` SSE and export response
/// bodies stream past it unaffected (`03 §7c`). Not a setting.
pub const REQUEST_HEADER_DEADLINE: Duration = Duration::from_secs(30);

/// Shutdown stops accepting, drains accepted connections for at most this long, then
/// cancels every remaining connection task — no connection survives server shutdown
/// (`03 §7c`).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// Beyond [`SHUTDOWN_GRACE`], `stop` waits this extra fixed margin for the serve task
/// to finish cancelling connection tasks before aborting the serve task itself as a
/// last resort (that abort still cancels the children — the serve task owns them in a
/// `JoinSet`, whose drop aborts everything left).
const SHUTDOWN_CANCEL_MARGIN: Duration = Duration::from_millis(500);

/// One server's connection budget. Production always uses [`ServerLimits::default`] —
/// the fixed constants above. Unit tests inject shorter deadlines/grace so slow paths
/// (header expiry, shutdown cancellation, streaming survival) run deterministically in
/// fast test time; this type is private and is not a setting.
#[derive(Clone, Copy)]
struct ServerLimits {
    max_active_connections: usize,
    request_header_deadline: Duration,
    shutdown_grace: Duration,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            max_active_connections: MAX_ACTIVE_CONNECTIONS,
            request_header_deadline: REQUEST_HEADER_DEADLINE,
            shutdown_grace: SHUTDOWN_GRACE,
        }
    }
}

/// The live bearer token, shared with the auth middleware. Held behind an `RwLock` and
/// read per request, so regenerating the token takes effect immediately without
/// restarting the server (`regenerate_api_token`, `03 §7c`).
pub type SharedToken = Arc<RwLock<String>>;

/// Independent live read credential. `None` (including upgrades) grants no access.
/// Never serialized or logged; persistence belongs to the composition root.
pub type SharedReadToken = Arc<RwLock<Option<String>>>;

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

    /// Bounded chronological evidence; never search, inference, capture or detail enumeration.
    /// Composition overrides this with the Store's transactional page adapter.
    async fn frame_page(
        &self,
        _request: traits::FramePageRequest,
    ) -> Result<traits::StoredFramePage, traits::HistoryReadError> {
        Err(traits::HistoryReadError::Unavailable(
            traits::HistoryUnavailableReason::UnsupportedStore,
        ))
    }

    /// Coverage facts composed from Store, current controller intent/observations and
    /// retention policy. This read must not inspect files or start capture.
    async fn capture_coverage(
        &self,
        _window: traits::EvidenceWindow,
    ) -> Result<traits::CaptureCoverage, traits::HistoryReadError> {
        Err(traits::HistoryReadError::Unavailable(
            traits::HistoryUnavailableReason::UnsupportedStore,
        ))
    }

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
    pub read_token: SharedReadToken,
}

impl ApiState {
    /// Legacy-compatible state, with no read credential until explicitly configured.
    pub fn new(host: Arc<dyn ApiHost>, token: SharedToken) -> Self {
        Self {
            host,
            token,
            read_token: Arc::new(RwLock::new(None)),
        }
    }
}

/// A running local API server. [`ApiServer::stop`] shuts it down, bounded. Dropping
/// the handle without calling `stop` also ends the server — the serve task treats the
/// closed signal channel as stop and runs the same bounded drain; it just cannot be
/// awaited. The composition root keeps it in an `Option` slot so start/stop on the
/// settings toggle is idempotent (mirrors the capture handle, `03 §6`).
pub struct ApiServer {
    stop: watch::Sender<bool>,
    join: JoinHandle<()>,
    local_addr: SocketAddr,
    token: SharedToken,
    read_token: SharedReadToken,
}

/// Server-handle spelling for integrations; the existing `ApiServer` name remains valid.
pub type ServerHandle = ApiServer;

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
        Self::start_on_with(host, token, bind_address, port, ServerLimits::default()).await
    }

    /// [`ApiServer::start_on`] with an injectable connection budget — production always
    /// passes [`ServerLimits::default`], the fixed constants.
    async fn start_on_with(
        host: Arc<dyn ApiHost>,
        token: SharedToken,
        bind_address: IpAddr,
        port: u16,
        limits: ServerLimits,
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
        let state = ApiState::new(host, token.clone());
        let read_token = state.read_token.clone();
        let router = build_router(state);
        let (stop, stop_rx) = watch::channel(false);
        let join = tokio::spawn(serve(listener, router, stop_rx, limits));
        tracing::info!(%local_addr, "local API listening");
        Ok(ApiServer {
            stop,
            join,
            local_addr,
            token,
            read_token,
        })
    }

    /// Replace only the live legacy token. Existing `SharedToken` writers keep working.
    /// The caller must persist first under its API configuration lock.
    pub fn set_token(&self, token: String) {
        *self.token.write().expect("api token lock poisoned") = token;
    }

    /// Replace or revoke only the live read credential without changing the listener.
    /// Empty values are disabled too. Persist first under the composition config lock.
    /// Subsequent requests observe the change; admitted streams are not cancelled.
    pub fn set_read_token(&self, token: Option<String>) {
        *self
            .read_token
            .write()
            .expect("api read token lock poisoned") = token;
    }

    /// The actual bound IPv4 or IPv6 socket address. Used to report the live port;
    /// clients must use a reachable host address rather than this listen address.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Bounded graceful shutdown (`03 §7c`). Signals the serve task to stop accepting;
    /// idle connections close at once and in-flight ones get up to [`SHUTDOWN_GRACE`],
    /// after which the serve task cancels **every** remaining connection task and awaits
    /// it — none survives shutdown. This waits for that sequence (the grace plus a small
    /// fixed cancellation margin) and aborts the serve task only as a last resort, so
    /// app exit can never wedge. Every wait is timer-driven, so held sockets cannot
    /// starve it.
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let mut join = self.join;
        if tokio::time::timeout(SHUTDOWN_GRACE + SHUTDOWN_CANCEL_MARGIN, &mut join)
            .await
            .is_err()
        {
            tracing::warn!("local API shutdown did not finish; aborting the serve task");
            // Dropping the aborted task drops the JoinSet it owns, which cancels every
            // connection task still left. Await the abort so the task has actually
            // dropped its owned resources before stop returns.
            join.abort();
            let _ = join.await;
        }
    }
}

/// The bounded accept loop (`03 §7c`, PR #122 review amendment). Admits at most
/// `limits.max_active_connections` sockets: the capacity permit is taken **before**
/// any read or authentication, and a socket beyond the limit is closed immediately
/// instead of being queued as a task. A finished connection releases its permit, so
/// the listener recovers by itself. On shutdown the loop stops accepting, drops the
/// listener (refusing new connections at once), lets accepted connections drain for
/// the bounded grace, then cancels and awaits every remaining connection task — none
/// survives.
async fn serve(
    listener: tokio::net::TcpListener,
    router: Router,
    mut stop_rx: watch::Receiver<bool>,
    limits: ServerLimits,
) {
    let semaphore = Arc::new(Semaphore::new(limits.max_active_connections));
    let mut connections: JoinSet<()> = JoinSet::new();
    // One receiver for every connection task, cloned before the loop future below
    // borrows the loop's own receiver.
    let connection_stop = stop_rx.clone();
    // Both this stopwatch and the drain timeout are timer-driven, so held sockets can
    // never starve them.
    let mut stop = std::pin::pin!(async {
        let _ = stop_rx.wait_for(|stopped| *stopped).await;
    });

    loop {
        tokio::select! {
            biased;
            // Stop signal — or the handle dropped, which can never be signalled again.
            _ = &mut stop => break,
            // Reap finished connection tasks so the set cannot grow without bound.
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => match semaphore.clone().try_acquire_owned() {
                    Ok(permit) => {
                        let router = router.clone();
                        let stop_rx = connection_stop.clone();
                        let header_deadline = limits.request_header_deadline;
                        // The permit is held for the connection's whole lifetime; the
                        // task ends (releasing it) when the connection closes.
                        connections.spawn(async move {
                            let _permit = permit;
                            run_connection(stream, router, stop_rx, header_deadline).await;
                        });
                    }
                    // At capacity: close the excess socket immediately — no task, no
                    // read, no authentication.
                    Err(_) => {
                        tracing::debug!(%peer, "local API connection closed: active-connection limit reached");
                        drop(stream);
                    }
                },
                // A failed accept (for example a connection the peer tore down) must
                // not kill the listener.
                Err(error) => tracing::warn!(%error, "local API accept failed"),
            },
        }
    }

    // Shutdown: refuse new connections for the rest of the drain.
    drop(listener);

    if tokio::time::timeout(limits.shutdown_grace, async {
        while connections.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        tracing::warn!("local API connections outlasted the shutdown grace; cancelling them");
        // Cancels every remaining connection task and awaits its termination — the
        // no-detached-leaks rail.
        connections.shutdown().await;
    }
}

/// Serves one admitted socket (`03 §7c`, PR #122 review amendment): HTTP/1 only, with
/// the runtime timer installed, so the header deadline covers **every** request — a
/// socket that never speaks included — while response streaming (SSE ask, export) is
/// deliberately untouched. A stop signal starts a graceful connection shutdown; the
/// task always ends, releasing its capacity permit.
async fn run_connection(
    stream: tokio::net::TcpStream,
    router: Router,
    mut stop_rx: watch::Receiver<bool>,
    header_deadline: Duration,
) {
    let mut builder = AutoBuilder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(header_deadline);
    // No version sniffing: h1-only goes straight to the HTTP/1 parser, so the header
    // deadline covers a silent socket from its first poll. The API is plain HTTP — no
    // ALPN — and never served h2 prior-knowledge connections (http2 stays off).
    let builder = builder.http1_only();
    let service = TowerToHyperService::new(router);
    let conn = builder.serve_connection(TokioIo::new(stream), service);
    tokio::pin!(conn);

    let mut stopped = std::pin::pin!(async {
        let _ = stop_rx.wait_for(|requested| *requested).await;
    });
    tokio::select! {
        result = conn.as_mut() => log_connection_end(result),
        // Stop signal, or the sender dropped: close idle keep-alive connections at
        // once and let an in-flight response finish; the serve task's bounded drain
        // cancels anything that outlasts the grace.
        _ = &mut stopped => {
            conn.as_mut().graceful_shutdown();
            log_connection_end(conn.as_mut().await);
        }
    }
}

/// A connection ending with an error is routine — a shutdown cancellation, a header
/// deadline expiry, a client gone — so it is debug-level.
fn log_connection_end(result: Result<(), Box<dyn std::error::Error + Send + Sync>>) {
    if let Err(error) = result {
        tracing::debug!(%error, "local API connection ended");
    }
}

/// Builds the v1 router with the bearer-auth layer applied to **every** route (health
/// included — the whole surface is private). The ask + export routes are added by the
/// PR7 SSE/export module.
pub fn build_router(state: ApiState) -> Router {
    use permissions::{ClassifiedRouter, Permission::*};
    ClassifiedRouter::new()
        .get("/v1/health", routes::health, HistoryRead)
        .get("/v1/search", routes::search, HistoryRead)
        .get("/v1/frames", history::frames, HistoryRead)
        .get("/v1/coverage", history::coverage, HistoryRead)
        .get("/v1/frames/{id}", routes::frame, HistoryRead)
        .get("/v1/context/where-was-i", routes::where_was_i, HistoryRead)
        .get("/v1/sessions", sessions::list_sessions, HistoryRead)
        .get("/v1/sessions/{id}", sessions::session_detail, HistoryRead)
        .get("/v1/marks", routes::list_marks, HistoryRead)
        .post("/v1/marks", routes::create_mark, MarksWrite)
        .post("/v1/marks/{id}/resolve", routes::resolve_mark, MarksWrite)
        .post("/v1/ask", routes::ask, AnswerGenerate)
        .get("/v1/export", export::export, HistoryRead)
        .finish(state)
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

/// Fast, deterministic coverage of the connection-bounds contract (`03 §7c`, PR #122
/// review amendment) over real loopback sockets. The production numbers are pinned
/// here; the slow paths — header expiry, the next-header deadline on an idle
/// keep-alive connection, streaming past the deadline, shutdown cancellation — run
/// against injected short windows through the exact same code path production uses,
/// so nothing here hardcodes a success: only the constants differ, and the
/// production-sized integration tests in `tests/http_api.rs` cover the real values.
#[cfg(test)]
mod connection_tests {
    use std::net::SocketAddr;
    use std::sync::{Arc, RwLock};
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::mpsc::Sender;
    use traits::{
        AnswerDelta, AnswerOpts, AnswerProvider, ExportFrameRow, FrameDetail, ResumeContext,
        RetrievedChunk, Settings, Store,
    };

    use super::{
        ApiHost, ApiServer, FrameImage, ServerLimits, SharedToken, DEFAULT_BIND_ADDRESS,
        MAX_ACTIVE_CONNECTIONS, REQUEST_HEADER_DEADLINE, SHUTDOWN_GRACE,
    };

    const TEST_TOKEN: &str = "pr122-connection-tests-token";

    /// A minimal host over an empty in-memory store: enough for `/v1/health` and for
    /// `/v1/ask` with empty grounding (the streaming tests supply the slow provider).
    struct Host {
        store: Arc<store::SqliteStore>,
        answer: Option<Arc<dyn AnswerProvider>>,
    }

    #[async_trait]
    impl ApiHost for Host {
        fn version(&self) -> String {
            "connection-tests".to_string()
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

        async fn frame_detail(&self, frame_id: i64) -> anyhow::Result<Option<FrameDetail>> {
            self.store.get_frame(frame_id).await
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
            Ok(Vec::new())
        }

        async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
            self.answer.clone()
        }

        async fn add_mark(
            &self,
            _frame_id: Option<i64>,
            _capture_now: bool,
            _note: Option<String>,
        ) -> anyhow::Result<i64> {
            anyhow::bail!("marks are not part of the connection tests")
        }

        async fn resolve_mark(&self, _mark_id: i64) -> anyhow::Result<()> {
            Ok(())
        }

        async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
            Ok(None)
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

    #[tokio::test]
    async fn legacy_hosts_without_new_adapters_return_explicit_unsupported_not_empty() {
        let host = Host {
            store: Arc::new(store::SqliteStore::open_in_memory().unwrap()),
            answer: None,
        };
        let window = traits::EvidenceWindow { from: 0, to: 1 };
        let request = traits::FramePageRequest {
            window,
            filters: traits::FrameFilters::default(),
            limit: 100,
            continuation: None,
        };
        assert!(matches!(
            host.frame_page(request).await,
            Err(traits::HistoryReadError::Unavailable(
                traits::HistoryUnavailableReason::UnsupportedStore
            ))
        ));
        assert!(matches!(
            host.capture_coverage(window).await,
            Err(traits::HistoryReadError::Unavailable(
                traits::HistoryUnavailableReason::UnsupportedStore
            ))
        ));
    }

    /// Streams `deltas` token events `tick` apart, then `Done`; exits early when the
    /// receiver goes away — exactly what a shutdown-cancelled stream looks like.
    struct SlowAnswer {
        tick: Duration,
        deltas: usize,
    }

    #[async_trait]
    impl AnswerProvider for SlowAnswer {
        async fn answer(
            &self,
            _query: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
            tx: Sender<AnswerDelta>,
        ) -> anyhow::Result<()> {
            for index in 0..self.deltas {
                tokio::time::sleep(self.tick).await;
                let delta = AnswerDelta::Token {
                    text: format!("chunk-{index}"),
                };
                if tx.send(delta).await.is_err() {
                    return Ok(());
                }
            }
            let _ = tx.send(AnswerDelta::Done).await;
            Ok(())
        }
    }

    /// Starts a real loopback server with the injected limits and an empty store.
    async fn start_with(
        limits: ServerLimits,
        answer: Option<Arc<dyn AnswerProvider>>,
    ) -> (SocketAddr, ApiServer) {
        let host = Arc::new(Host {
            store: Arc::new(store::SqliteStore::open_in_memory().expect("open store")),
            answer,
        });
        let token: SharedToken = Arc::new(RwLock::new(TEST_TOKEN.to_string()));
        let server = ApiServer::start_on_with(host, token, DEFAULT_BIND_ADDRESS, 0, limits)
            .await
            .expect("bind loopback");
        (server.local_addr(), server)
    }

    pub(super) fn permission_test_state() -> super::ApiState {
        super::ApiState::new(
            Arc::new(Host {
                store: Arc::new(store::SqliteStore::open_in_memory().unwrap()),
                answer: None,
            }),
            Arc::new(RwLock::new("permission-legacy".to_string())),
        )
    }

    /// A complete `GET /v1/health` request with the bearer token, over a raw socket.
    fn health_request() -> Vec<u8> {
        format!(
            "GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TEST_TOKEN}\r\n\r\n"
        )
        .into_bytes()
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    /// One read into `buf`; `None` on a read error or when the 5 s guard trips.
    async fn read_chunk(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Option<usize> {
        let mut chunk = [0u8; 4096];
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
            Ok(Ok(n)) => {
                buf.extend_from_slice(&chunk[..n]);
                Some(n)
            }
            Ok(Err(_)) => None,
            Err(_) => None,
        }
    }

    /// Reads one complete HTTP/1.1 response from a keep-alive connection and returns
    /// its body; `None` when the server closed the connection first.
    async fn read_one_response(stream: &mut TcpStream) -> Option<String> {
        let mut buf = Vec::new();
        loop {
            if let Some(head_end) = find_subslice(&buf, b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                assert!(
                    head.starts_with("HTTP/1.1 200"),
                    "unexpected response: {head}"
                );
                let len = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .expect("content-length header");
                let body_start = head_end + 4;
                while buf.len() < body_start + len {
                    match read_chunk(stream, &mut buf).await {
                        Some(0) | None => return None,
                        Some(_) => {}
                    }
                }
                return Some(
                    String::from_utf8_lossy(&buf[body_start..body_start + len]).to_string(),
                );
            }
            match read_chunk(stream, &mut buf).await {
                Some(0) | None => return None,
                Some(_) => {}
            }
        }
    }

    /// The reviewer-approved policy numbers are pinned; changing them must be
    /// deliberate, not an accident.
    #[test]
    fn production_connection_limits_are_pinned() {
        assert_eq!(MAX_ACTIVE_CONNECTIONS, 64);
        assert_eq!(REQUEST_HEADER_DEADLINE, Duration::from_secs(30));
        assert_eq!(SHUTDOWN_GRACE, Duration::from_secs(3));
        assert_eq!(
            ServerLimits::default().request_header_deadline,
            REQUEST_HEADER_DEADLINE,
            "production must use the fixed constants"
        );
    }

    /// Incomplete request headers are closed at the header deadline — and not before
    /// it. Fast variant with an injected deadline; the production 30 s value is
    /// exercised by the integration test.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn partial_request_headers_expire_at_the_header_deadline() {
        let deadline = Duration::from_millis(300);
        let limits = ServerLimits {
            request_header_deadline: deadline,
            ..ServerLimits::default()
        };
        let (addr, server) = start_with(limits, None).await;

        let mut stream = TcpStream::connect(addr).await.unwrap();
        // A header block that never terminates.
        stream
            .write_all(b"GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Partial: ")
            .await
            .unwrap();
        stream.flush().await.unwrap();

        let started = Instant::now();
        let mut buf = Vec::new();
        let closed =
            tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf)).await;
        let elapsed = started.elapsed();
        assert!(
            closed.is_ok(),
            "the connection outlived the header deadline"
        );
        assert!(
            elapsed >= deadline.saturating_sub(Duration::from_millis(200)),
            "closed before the header deadline: {elapsed:?}"
        );
        server.stop().await;
    }

    /// An idle keep-alive connection — one that completed a request and is waiting for
    /// the next — is closed at the same header deadline for that next request
    /// (`03 §7c`: "idle keep-alive connections are subject to the same next-header
    /// deadline").
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn idle_keep_alive_connection_expires_at_the_next_header_deadline() {
        let deadline = Duration::from_millis(300);
        let limits = ServerLimits {
            request_header_deadline: deadline,
            ..ServerLimits::default()
        };
        let (addr, server) = start_with(limits, None).await;

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(&health_request()).await.unwrap();
        stream.flush().await.unwrap();
        let body = read_one_response(&mut stream)
            .await
            .expect("the first request must be served");
        assert!(body.contains("\"capturing\""), "health body: {body}");

        // Now stay idle: the server must close us at the deadline for the next request.
        let started = Instant::now();
        let mut buf = Vec::new();
        let closed =
            tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf)).await;
        let elapsed = started.elapsed();
        assert!(
            closed.is_ok(),
            "the idle keep-alive connection was not closed within the guard"
        );
        assert!(
            elapsed >= deadline.saturating_sub(Duration::from_millis(200)),
            "closed before the next-header deadline: {elapsed:?}"
        );
        server.stop().await;
    }

    /// A response that streams longer than the header deadline — the `/v1/ask` SSE
    /// case — is not cut off by it: every delta and the terminal `done` arrive, well
    /// after the deadline has passed (`03 §7c`: "not a response/body deadline").
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn streaming_response_outlives_the_header_deadline() {
        let tick = Duration::from_millis(200);
        let deltas = 6; // ≈1.2 s of streaming, four times the injected 300 ms deadline
        let limits = ServerLimits {
            request_header_deadline: Duration::from_millis(300),
            ..ServerLimits::default()
        };
        let (addr, server) = start_with(limits, Some(Arc::new(SlowAnswer { tick, deltas }))).await;

        let payload = serde_json::json!({ "query": "anything" }).to_string();
        // `Connection: close` ends the socket after the response body, so EOF means the
        // stream finished (an SSE response is otherwise kept alive for a next request).
        let request = format!(
            "POST /v1/ask HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TEST_TOKEN}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{payload}",
            payload.len()
        );
        let started = Instant::now();
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();

        let mut buf = Vec::new();
        loop {
            match read_chunk(&mut stream, &mut buf).await {
                Some(0) | None => break,
                Some(_) => {}
            }
        }
        let elapsed = started.elapsed();
        let text = String::from_utf8_lossy(&buf).to_string();
        assert!(
            elapsed >= tick * u32::try_from(deltas).expect("deltas fit u32"),
            "the stream finished faster than the provider produced it: {elapsed:?}"
        );
        assert!(
            text.contains("chunk-0"),
            "the stream should have started: {text}"
        );
        assert!(
            text.contains(&format!("chunk-{}", deltas - 1)),
            "the last delta must arrive: {text}"
        );
        assert!(
            text.contains("\"type\":\"done\""),
            "the stream must terminate normally: {text}"
        );
        server.stop().await;
    }

    /// Capacity: the first connection beyond an exhausted cap is closed immediately and
    /// is never served, and closing an admitted connection frees its slot. Fast variant
    /// with an injected cap of 4; the production 64 is exercised by the integration
    /// test.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn excess_connections_are_closed_and_capacity_recovers() {
        let limits = ServerLimits {
            max_active_connections: 4,
            ..ServerLimits::default()
        };
        let (addr, server) = start_with(limits, None).await;

        // Admit four keep-alive connections, each proven by a completed request.
        let mut held = Vec::new();
        for slot in 0..limits.max_active_connections {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_all(&health_request()).await.unwrap();
            stream.flush().await.unwrap();
            let body = read_one_response(&mut stream)
                .await
                .unwrap_or_else(|| panic!("slot {slot} was not served"));
            assert!(body.contains("\"capturing\""), "slot {slot} body: {body}");
            held.push(stream);
        }

        // The next socket is over the cap: closed, never served.
        let mut excess = TcpStream::connect(addr).await.unwrap();
        let mut byte = [0u8; 1];
        match tokio::time::timeout(Duration::from_secs(5), excess.read(&mut byte)).await {
            Ok(Ok(0)) | Ok(Err(_)) => {}
            Ok(Ok(n)) => panic!("the excess connection was served {n} byte(s) instead of closed"),
            Err(_) => panic!("the excess connection was neither closed nor served"),
        }

        // Closing one admitted connection frees its slot.
        drop(held.pop());
        let mut recovered = false;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let mut stream = TcpStream::connect(addr).await.unwrap();
            if stream.write_all(&health_request()).await.is_err() {
                continue;
            }
            let _ = stream.flush().await;
            if let Some(body) = read_one_response(&mut stream).await {
                assert!(body.contains("\"capturing\""), "recovered body: {body}");
                recovered = true;
                break;
            }
        }
        assert!(
            recovered,
            "capacity did not recover after an admitted connection closed"
        );
        server.stop().await;
    }

    /// Shutdown cancels a connection that outlasts the grace: the streaming response is
    /// cut (no `done`) and `stop` returns bounded. Fast variant with an injected 250 ms
    /// grace; the production 3 s bound is exercised by the integration test.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_cancels_a_stream_that_outlasts_the_grace() {
        let grace = Duration::from_millis(250);
        let limits = ServerLimits {
            shutdown_grace: grace,
            ..ServerLimits::default()
        };
        let (addr, server) = start_with(
            limits,
            Some(Arc::new(SlowAnswer {
                tick: Duration::from_millis(100),
                deltas: 100, // ~10 s if it were never cancelled
            })),
        )
        .await;

        let payload = serde_json::json!({ "query": "anything" }).to_string();
        let request = format!(
            "POST /v1/ask HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TEST_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
            payload.len()
        );
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();

        // Wait until the stream has started, so the connection is genuinely in flight.
        let mut streamed = Vec::new();
        while find_subslice(&streamed, b"chunk-0").is_none() {
            match read_chunk(&mut stream, &mut streamed).await {
                Some(0) | None => panic!("the stream closed before it started"),
                Some(_) => {}
            }
        }

        let started = Instant::now();
        server.stop().await;
        let elapsed = started.elapsed();
        assert!(
            elapsed >= grace.saturating_sub(Duration::from_millis(100)),
            "shutdown must honour the grace before cancelling: {elapsed:?}"
        );
        assert!(
            elapsed <= grace + Duration::from_secs(2),
            "shutdown must stay bounded: {elapsed:?}"
        );

        // The stream was cancelled, not completed: read to the end and look for `done`.
        let mut rest = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut rest)).await;
        let text = String::from_utf8_lossy(&rest).to_string();
        assert!(
            !text.contains("\"type\":\"done\""),
            "the stream should be cut at shutdown, not completed: {text}"
        );
    }
}
