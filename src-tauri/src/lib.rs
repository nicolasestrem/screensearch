//! ScreenSearch V2c — Tauri 2 desktop shell and **composition root** (`03 §2`).
//!
//! This crate is the only place that wires concrete module impls into the kernel.
//! P2 wired the **capture happy path**: it opens the data spine (P1), spawns the
//! WinRT OCR worker and the WGC capture factory, builds the [`Kernel`], forwards
//! kernel events to the WebView2 UI, and exposes `capture_control` / `get_frame`.
//! P3/P4 load fastembed and the inference sidecar off the launch thread, attaching
//! provider slots that start the enrichment workers and light up search, vision
//! tagging, and grounded answers. P5 adds the full Command Deck command surface.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use tauri::{Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use traits::{
    AnswerEvent, AnswerOpts, AppSuppression, AskRequest, CaptureControl, CaptureSource,
    CapturedFrame, ComponentReadiness, ComponentStatus, FrameMeta, InsightsSummary, JobStats, Mark,
    ModelLane, MonitorInfo, OcrProvider, OcrResult, Readiness, ReportConfig, ReportKind,
    ReportMode, ReportProgress, ReportRange, ReportRequest, ReportResponse, RetrievedChunk,
    SearchHit, SearchQuery, Session, SessionArtifactKind, SessionDetail, SessionFilter,
    SessionQuery, SessionRecapRequest, SetModelTier, Settings, StorageStats, Store, TextSpan,
    ThrottleStatus, TimeRange, TimelineBucket, ToastLevel, UiFrameDetail, UiResumeContext,
    VisionTarget,
};

use embeddings::FastEmbedProvider;
use inference::{AnswerSidecar, ModelSupervisor, SupervisorConfig, VisionSidecar};
use kernel::{CaptureFactory, IdleSource, Kernel, KernelEvent};
use store::SqliteStore;
use tokio::task::JoinHandle;

mod local_api;
mod overlay;
mod tray;
mod update;

/// How long to wait for `llama-server` `/health` after a spawn (model load can be slow
/// on first run / large quants).
const SIDECAR_HEALTH_TIMEOUT: Duration = Duration::from_secs(180);
/// Reply budget (`n_predict`) for each report summarization pass. Mirrors the Ask UI's
/// 2048: reasoning answer models need room to finish a `<think>` trace and the reply,
/// and `build_summary_messages` reserves this from the pinned 8192 answer context.
const REPORT_REPLY_BUDGET: u32 = 2048;
const MAX_FRAME_LIST_LIMIT: u32 = 500;
const MAX_FRAME_CONTEXT_LIMIT_EACH: u32 = 50;
const MAX_SESSION_QUERY_LIMIT: u32 = 1000;
const DEFAULT_SESSION_QUERY_LIMIT: u32 = 1000;
const SESSION_DETAIL_FRAME_LIMIT: u32 = 24;
const MIN_TIMELINE_BUCKETS: u32 = 120;
const MAX_TIMELINE_BUCKETS: u32 = 2000;
const MIN_INSIGHTS_BUCKETS: u32 = 24;
const MAX_INSIGHTS_BUCKETS: u32 = 720;
const RETENTION_BATCH: u32 = 1000;
const RETENTION_INTERVAL: Duration = Duration::from_secs(60 * 60);
const DAY_MS: i64 = 86_400_000;
static NEXT_ASK_ID: AtomicU64 = AtomicU64::new(1);

fn next_ask_id() -> u64 {
    NEXT_ASK_ID.fetch_add(1, Ordering::Relaxed)
}

fn clamp_timeline_buckets(bucket_count: u32) -> u32 {
    bucket_count.clamp(MIN_TIMELINE_BUCKETS, MAX_TIMELINE_BUCKETS)
}

fn clamp_insights_buckets(bucket_count: u32) -> u32 {
    bucket_count.clamp(MIN_INSIGHTS_BUCKETS, MAX_INSIGHTS_BUCKETS)
}

fn clamp_frame_list_limit(limit: u32) -> u32 {
    limit.min(MAX_FRAME_LIST_LIMIT)
}

fn clamp_frame_context_limit(limit_each: u32) -> u32 {
    limit_each.min(MAX_FRAME_CONTEXT_LIMIT_EACH)
}

fn normalize_session_query(query: SessionQuery, request_now_ms: i64) -> SessionFilter {
    let (from_ms, to_ms) = query
        .time_range
        .map_or((None, None), |range| (Some(range.start), Some(range.end)));
    let tool = query.tool.and_then(|tool| {
        let tool = tool.trim();
        (!tool.is_empty()).then(|| tool.to_string())
    });
    SessionFilter {
        from_ms,
        to_ms,
        kind: query.kind,
        tool,
        limit: Some(
            query
                .limit
                .unwrap_or(DEFAULT_SESSION_QUERY_LIMIT)
                .clamp(1, MAX_SESSION_QUERY_LIMIT),
        ),
        now_ms: request_now_ms,
    }
}

async fn load_session_detail(
    store: Arc<dyn Store>,
    session_id: i64,
    include_summary: bool,
    answer: Option<Arc<dyn traits::AnswerProvider>>,
) -> Result<Option<SessionDetail>, String> {
    let Some(mut session) = store
        .get_session(session_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    // Redact a legacy invalid cache from every in-app detail response. The base detail request
    // races summary generation in the Session route, so exposing `"User"` here would keep it
    // visible while generation runs — or forever when inference is unavailable. For an
    // `include_summary` request, clear the durable cache before checking provider readiness:
    // otherwise non-UI callers can continue reading the poisoned row after the unavailable-sidecar
    // error (usage review 2026-08-01 §7.4).
    let invalid_cached_summary = session
        .summary
        .as_deref()
        .is_some_and(|summary| !kernel::sessions_intel::is_useful_session_summary(summary));
    if invalid_cached_summary {
        session.title = None;
        session.summary = None;
        session.summary_model = None;
        if include_summary
            && !store
                .clear_session_title_summary(session_id)
                .await
                .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "session {session_id} disappeared before its invalid summary cache could be cleared"
            ));
        }
    }
    let fully_cached =
        session.title.is_some() && session.summary.is_some() && session.summary_model.is_some();
    if include_summary && !fully_cached {
        let answer = answer.ok_or_else(|| "inference sidecar not ready yet".to_string())?;
        session = kernel::sessions_intel::generate_session_title_summary(
            store.clone(),
            answer,
            session_id,
        )
        .await
        .map_err(|error| error.to_string())?;
    }
    let sample = store
        .session_frame_sample(session_id, SESSION_DETAIL_FRAME_LIMIT)
        .await
        .map_err(|error| error.to_string())?;
    let exchanges = store
        .list_session_artifacts(session_id)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|artifact| artifact.kind == SessionArtifactKind::Exchange)
        .collect();
    Ok(Some(SessionDetail {
        session,
        frames: sample.frames,
        frame_count: sample.total_count,
        exchanges,
    }))
}

async fn session_recap_has_evidence(store: &dyn Store, session_id: i64) -> traits::Result<bool> {
    store.session_has_usable_content(session_id).await
}

async fn load_ui_resume_context(
    store: &dyn Store,
    settings: &Settings,
) -> traits::Result<Option<UiResumeContext>> {
    let Some(context) = kernel::resume::where_was_i(store, settings).await? else {
        return Ok(None);
    };
    let session = store.session_reference_for_frame(context.frame_id).await?;
    Ok(Some(UiResumeContext { context, session }))
}

/// A slot the composition root fills off the launch thread, shared with command
/// handlers (the sidecar supervisor + the concrete tiered providers).
type SharedSlot<T> = Arc<StdMutex<Option<T>>>;
type AskTasks = Arc<tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>>;
/// In-flight report cancel flags keyed by request id; `cancel_report` sets one to stop
/// the orchestrator at its next pass boundary (cooperative, no task abort needed since
/// `generate_report` is awaited).
type ReportTasks = Arc<tokio::sync::Mutex<HashMap<String, Arc<AtomicBool>>>>;

/// App-wide state owned by the composition root and shared with command handlers.
struct AppState {
    /// The data spine (concrete, so commands can use its inherent reads like
    /// `get_frame`). `None` only if the DB failed to open.
    store: Option<Arc<SqliteStore>>,
    /// The orchestrator; `None` when the store failed to open (no spine to drive).
    kernel: Option<Arc<Kernel>>,
    /// Readiness used only when there is no kernel (DB-open failure); otherwise the
    /// live snapshot comes from the kernel.
    fallback_readiness: Readiness,
    /// The sidecar supervisor, filled once the binary resolves off-thread (P4). Held
    /// here so app exit can shut the sidecar down cleanly.
    supervisor: SharedSlot<Arc<ModelSupervisor>>,
    /// The concrete tiered providers, for `set_model_tier` (the kernel only holds them
    /// as `dyn` traits, which can't switch tier).
    vision: SharedSlot<Arc<VisionSidecar>>,
    answer: SharedSlot<Arc<AnswerSidecar>>,
    sidecar_binary: SharedSlot<PathBuf>,
    /// In-flight answer providers keyed by client request id; `cancel_ask` aborts them.
    ask_tasks: AskTasks,
    /// In-flight report cancel flags keyed by request id; `cancel_report` sets them.
    report_tasks: ReportTasks,
    /// The UIA-text composite provider (`docs/0.2.0.md` #48), when OCR is available. Held
    /// concretely (the kernel only sees it as `dyn OcrProvider`) so `set_settings` can call
    /// `apply_settings` — which hot-applies every UIA knob by tearing down + respawning the
    /// UI Automation client. `None` only when OCR itself is unavailable.
    uia: Option<Arc<UiaWithOcrFallback>>,
    embed_models_dir: PathBuf,
    db_path: PathBuf,
    frames_dir: PathBuf,
    /// The opt-in local HTTP API runtime (0.3.0 PR7). Holds the server slot + live token;
    /// the server is constructed only when `api.enabled` (`03 §7c`).
    api_runtime: Arc<local_api::ApiRuntime>,
    /// The `ApiHost` adapter the API/export commands run against; `None` only when the DB
    /// failed to open (no store/kernel to adapt). It holds the app-start instant for the
    /// `/v1/health` uptime anchor.
    api_host: Option<Arc<local_api::TauriApiHost>>,
}

/// Liveness probe for the typed IPC bridge (P0 smoke test, retained).
#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

/// Current subsystem readiness (`03 §7`). Live `db` + `capture` come from the
/// kernel once it exists; otherwise the DB-error fallback is returned.
#[tauri::command]
fn get_readiness(state: State<'_, AppState>) -> Readiness {
    match &state.kernel {
        Some(kernel) => kernel.readiness(),
        None => state.fallback_readiness.clone(),
    }
}

/// Aggregate job-queue counts (`03 §7`). After P2 capture runs, this shows pending
/// `embed_text` jobs (consumed in P3).
#[tauri::command]
async fn get_job_stats(state: State<'_, AppState>) -> Result<JobStats, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store.job_stats().await.map_err(|e| e.to_string())
}

/// Current enrichment-throttle status (`get_throttle_status`, `03 §7`): live CPU/GPU
/// pressure, the throttle level, and whether the GPU is actually monitored (truthful
/// "GPU not monitored" state). Returns the honest disabled status when there is no kernel.
#[tauri::command]
fn get_throttle_status(state: State<'_, AppState>) -> ThrottleStatus {
    match &state.kernel {
        Some(kernel) => kernel.throttle_status(),
        None => ThrottleStatus {
            enabled: false,
            level: 0,
            sample: None,
            gpu_monitored: false,
        },
    }
}

/// Storage footprint for the StatusRail (`get_storage_stats`, P5 follow-up).
#[tauri::command]
async fn get_storage_stats(state: State<'_, AppState>) -> Result<StorageStats, String> {
    storage_stats(state.db_path.clone(), state.frames_dir.clone()).await
}

/// Connected monitor metadata for Settings.
#[tauri::command]
fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    Ok(capture::enumerate_monitors())
}

/// Per-app text-filter suppression metric (`get_text_filter_stats`, `03 §3b`): the
/// guardrail that makes silent over-suppression observable. Over frames classified by
/// the live `filter_version`.
#[tauri::command]
async fn get_text_filter_stats(state: State<'_, AppState>) -> Result<Vec<AppSuppression>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .text_filter_stats(store::FILTER_VERSION)
        .await
        .map_err(|e| e.to_string())
}

/// Lists llama.cpp device ids reported by the resolved sidecar binary (`--list-devices`).
#[tauri::command]
async fn list_sidecar_devices(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let binary = state
        .sidecar_binary
        .lock()
        .expect("sidecar binary slot")
        .clone()
        .ok_or_else(|| "llama-server binary is not resolved yet".to_string())?;
    tokio::task::spawn_blocking(move || list_devices_from_binary(&binary))
        .await
        .map_err(|e| format!("device listing task failed: {e}"))?
}

/// Full per-frame detail for the timeline (`03 §7`): base row + OCR text + vision +
/// tags. `None` if the frame id is unknown.
#[tauri::command]
async fn get_frame(
    frame_id: i64,
    state: State<'_, AppState>,
) -> Result<Option<UiFrameDetail>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let Some(frame) = store
        .get_frame(frame_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let session = store
        .session_reference_for_frame(frame_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(UiFrameDetail { frame, session }))
}

/// Lists sessions overlapping the optional half-open range. Open rows use this
/// request's clock; presentation limits and an empty tool are normalized here.
#[tauri::command]
async fn list_sessions(
    query: SessionQuery,
    state: State<'_, AppState>,
) -> Result<Vec<Session>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .sessions_in_range(normalize_session_query(query, now_ms()))
        .await
        .map_err(|error| error.to_string())
}

/// Returns a bounded session drill-in. The base read performs no inference;
/// `include_summary` lazily fills an incomplete title/summary and preserves the
/// one-call cache path in `kernel::sessions_intel`.
#[tauri::command]
async fn get_session(
    session_id: i64,
    include_summary: bool,
    state: State<'_, AppState>,
) -> Result<Option<SessionDetail>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let answer = if include_summary {
        match &state.kernel {
            Some(kernel) => kernel.answer_provider().await,
            None => None,
        }
    } else {
        None
    };
    load_session_detail(store, session_id, include_summary, answer).await
}

/// All recognized text spans for a frame — normalized `[0,1]` geometry + classified role
/// (`03 §3b`), in reading order. Backs the Moment view's text+layout **reconstruction**,
/// the durable proof shown when a frame's screenshot has been retention-degraded
/// (`storage.retention_days`). Empty when the frame has no spans.
#[tauri::command]
async fn get_frame_spans(
    frame_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<TextSpan>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store.frame_spans(frame_id).await.map_err(|e| e.to_string())
}

/// Hybrid search over OCR text + (once embeddings exist) vectors, fused via RRF
/// (`search`, `03 §7/§13.4`). The vector arm is live once the embedder has loaded;
/// before that it degrades to FTS-only.
#[tauri::command]
async fn search(query: SearchQuery, state: State<'_, AppState>) -> Result<Vec<SearchHit>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store.hybrid_search(&query).await.map_err(|e| e.to_string())
}

/// Starts or stops capture explicitly. Capture starts once backend boot wires a usable kernel
/// (the 2026-08-01 production-usage hardening decision); Stop remains an immediate session-local
/// privacy control (`capture_control`, `03 §7`).
#[tauri::command]
async fn capture_control(
    control: CaptureControl,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| "capture unavailable (database not open)".to_string())?;
    match control {
        CaptureControl::Start => kernel.start_capture().await.map_err(|e| e.to_string()),
        CaptureControl::Stop => {
            kernel.stop_capture().await;
            Ok(())
        }
    }
}

/// Enqueue deferred vision tagging for a frame or a time range (`enqueue_vision`,
/// `03 §7`). Returns the number of `vision_tag` jobs enqueued. This is the on-demand
/// trigger; results land asynchronously and surface via `job_progress` + `get_frame`.
#[tauri::command]
async fn enqueue_vision(target: VisionTarget, state: State<'_, AppState>) -> Result<u64, String> {
    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| "kernel unavailable (database not open)".to_string())?;
    kernel
        .enqueue_vision(target)
        .await
        .map_err(|e| e.to_string())
}

/// Cancels all pending `vision_tag` jobs (the tray + quick-menu "Stop vision tagging"
/// action, 0.3.2 PR3; `03 §7d`). Returns how many were cancelled; a `running` job finishes
/// (no lease to revoke). Refreshes queue counts via `job_progress`.
#[tauri::command]
async fn cancel_vision(state: State<'_, AppState>) -> Result<u64, String> {
    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| "kernel unavailable (database not open)".to_string())?;
    kernel.cancel_vision().await.map_err(|e| e.to_string())
}

// ── Flow recall: where-was-i + marks (0.3.0 PR6; `03 §7`/`§7b`) ────────────────────

/// The last sustained context before the current detour (`where_was_i`, `03 §7b`, D9).
/// `None` = "nothing to resume yet". Reads the live `resume.min_dwell_secs` +
/// `privacy.excluded_apps`; the heuristic itself is the pure `kernel::resume`.
#[tauri::command]
async fn where_was_i(state: State<'_, AppState>) -> Result<Option<UiResumeContext>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let settings = kernel::settings::load_settings(store.as_ref()).await;
    load_ui_resume_context(store.as_ref(), &settings)
        .await
        .map_err(|e| e.to_string())
}

/// Creates a mark (`add_mark`, `03 §7b`, D8). Exactly one of `frame_id` / `capture_now`.
/// A `capture_now` mark captures the current screen past the diff gate first. Returns the
/// new mark id and refreshes the Deck's Intentions strip via `marks_changed`.
#[tauri::command]
async fn add_mark(
    frame_id: Option<i64>,
    capture_now: bool,
    note: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| "kernel unavailable (database not open)".to_string())?;
    let mark_id = kernel
        .add_mark(frame_id, capture_now, note)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("marks_changed", ());
    Ok(mark_id)
}

/// All marks, unresolved first then newest-first within each group (`list_marks`,
/// `03 §7`). The Intentions strip renders the unresolved head.
#[tauri::command]
async fn list_marks(state: State<'_, AppState>) -> Result<Vec<Mark>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store.list_marks().await.map_err(|e| e.to_string())
}

/// Resolves a mark — done or dismiss (resolve-with-no-action), the same operation
/// (`resolve_mark`, `03 §7b`). Refreshes the strip via `marks_changed`.
#[tauri::command]
async fn resolve_mark(
    mark_id: i64,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .resolve_mark(mark_id, now_ms())
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("marks_changed", ());
    Ok(())
}

/// Attaches the optional one-line note to an existing mark after the fact (`set_mark_note`,
/// `03 §7`) — the mark is inserted at hotkey-press time; the note arrives from the toast.
#[tauri::command]
async fn set_mark_note(
    mark_id: i64,
    note: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .set_mark_note(mark_id, &note)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("marks_changed", ());
    Ok(())
}

/// Builds grounded Ask context before any answer stream starts. Full context hydration
/// is required for Ask quality: a failed `ocr_texts` bulk read means the command returns
/// a clear error instead of silently degrading every retrieved frame to snippets. Frames
/// with no stored text still fall back to their individual search snippet.
async fn ask_context<S: Store + ?Sized>(
    store: &S,
    query: &str,
    top_k: u32,
    session_id: Option<i64>,
) -> Result<Vec<RetrievedChunk>, String> {
    let query = SearchQuery {
        text: query.to_string(),
        limit: top_k,
        time_range: None,
        // Ask grounds on content text (`03 §3b`); raw/chrome stays opt-in.
        include_chrome: false,
    };
    // `session_id` (0.4.0 PR6, D12) restricts retrieval to the session's own frames so the
    // answer cites only in-session frames; without it, the frame-level path is unchanged (D10).
    let hits = match session_id {
        Some(id) => store.hybrid_search_in_session(&query, id).await,
        None => store.hybrid_search(&query).await,
    }
    .map_err(|e| e.to_string())?;
    // Hydrate context text in a single bulk query (avoid an N+1 over the hits).
    // If the bulk read fails, return before spawning the answer stream so grounded
    // Ask does not silently answer from low-quality snippets only.
    let frame_ids: Vec<i64> = hits.iter().map(|h| h.frame_id).collect();
    let ocr = store.ocr_texts(&frame_ids).await.map_err(|e| e.to_string());
    hydrate_ask_context(hits, frame_ids.len(), ocr)
}

fn hydrate_ask_context(
    hits: Vec<SearchHit>,
    frame_count: usize,
    ocr: Result<std::collections::HashMap<i64, String>, String>,
) -> Result<Vec<RetrievedChunk>, String> {
    let mut ocr = ocr
        .map_err(|e| format!("failed to hydrate Ask OCR context for {frame_count} frames: {e}"))?;
    Ok(hits
        .into_iter()
        .map(|hit| {
            // `ocr` is consumed here, so take ownership of each string instead of cloning.
            let text = ocr.remove(&hit.frame_id).unwrap_or(hit.snippet);
            RetrievedChunk {
                frame_id: hit.frame_id,
                text,
                score: hit.score,
                captured_at: hit.captured_at,
            }
        })
        .collect())
}

/// Ask a grounded question about the screen history (`ask`, `03 §7/§13.5`). Returns
/// immediately; the answer streams back as `answer_delta` events. Retrieves the top-K
/// chunks via hybrid search, supplies their full OCR text as reviewed context, and runs
/// the answer provider on a background task that forwards each delta to the UI.
#[tauri::command]
async fn ask(
    request: AskRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let request_id = request
        .request_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("legacy-{}", next_ask_id()));
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| "kernel unavailable".to_string())?;
    let answer = kernel
        .answer_provider()
        .await
        .ok_or_else(|| "inference sidecar not ready yet".to_string())?;

    // Retrieve model context: top-K hybrid hits, each with its full OCR text
    // (falling back to the search snippet if the text isn't available). Depth is the
    // configured `retrieval.default_top_k` (`03 §8`, replacing the former hardcoded
    // ASK_TOP_K), overridable per request via `AskRequest.top_k`.
    let settings = kernel::settings::load_settings(store.as_ref()).await;
    let top_k = request.top_k.unwrap_or(settings.retrieval_default_top_k);
    // The in-app Ask is always frame-level (session scoping is a PR6 API/MCP surface, D10).
    let context = ask_context(store.as_ref(), &request.query, top_k, None).await?;

    // Stream: the provider sends typed deltas on `tx`; a forwarder tags each with
    // `request_id` before emitting it as an `answer_delta` event. The provider task
    // is kept in `ask_tasks` so the UI can cancel superseded streams.
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let emitter = app.clone();
    let event_request_id = request_id.clone();
    tokio::spawn(async move {
        while let Some(delta) = rx.recv().await {
            let _ = emitter.emit(
                "answer_delta",
                AnswerEvent {
                    request_id: event_request_id.clone(),
                    delta,
                },
            );
        }
    });
    let opts = AnswerOpts {
        thinking: request.thinking,
        max_tokens: request.max_tokens,
    };
    let query = request.query;
    let mut tasks = state.ask_tasks.lock().await;
    let tasks_clone = state.ask_tasks.clone();
    let task_request_id = request_id.clone();
    let handle = tokio::spawn(async move {
        let _ = answer.answer(&query, &context, opts, tx).await;
        tasks_clone.lock().await.remove(&task_request_id);
    });
    if let Some(old) = tasks.insert(request_id, handle) {
        old.abort();
    }
    Ok(())
}

/// Cancel a streaming answer request if it is still running.
#[tauri::command]
async fn cancel_ask(request_id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(task) = state.ask_tasks.lock().await.remove(&request_id) {
        task.abort();
    }
    Ok(())
}

/// Generate a recall report over a time range (`generate_report`, `03 §8b`). Summarizes
/// `content_text` (never raw text) with temporal coverage that scales with the range —
/// one bounded pass per active day, combined by a hierarchical reduce — citing the
/// frames it read. Progress streams via request-scoped `report_progress` events; the
/// final value returns from the awaited call. Cancellable via `cancel_report`.
#[tauri::command]
async fn generate_report(
    request: ReportRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ReportResponse, String> {
    let request_id = request
        .request_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("report-{}", next_ask_id()));
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| "kernel unavailable".to_string())?;

    let settings = kernel::settings::load_settings(store.as_ref()).await;
    let cfg = ReportConfig {
        daily_top_k: settings.reports_daily_top_k,
        weekly_top_k: settings.reports_weekly_top_k,
        map_reduce_min_frames: settings.reports_map_reduce_min_frames,
        reply_budget: REPORT_REPLY_BUDGET,
    };
    let range = match request.kind {
        ReportKind::Daily => ReportRange::Daily,
        ReportKind::Weekly => ReportRange::Weekly,
        ReportKind::Custom => ReportRange::Custom,
    };
    let range_label = match request.kind {
        ReportKind::Daily => "today".to_string(),
        ReportKind::Weekly => "the last 7 days".to_string(),
        ReportKind::Custom => "the selected range".to_string(),
    };

    // An honest no-evidence report must not depend on the answer sidecar (which may still
    // be downloading/resolving right after first launch): a range with no captured frames
    // never calls the model, so short-circuit it here without acquiring the provider. A
    // probe error falls through to the normal path so the real error still surfaces.
    // (Codex review, PR #33.)
    let has_frames = store
        .frames_in_range(request.time_range.start, request.time_range.end, 1)
        .await
        .map(|f| !f.is_empty())
        .unwrap_or(true);
    if !has_frames {
        return Ok(empty_report_response(range_label));
    }

    let answer = kernel
        .answer_provider()
        .await
        .ok_or_else(|| "inference sidecar not ready yet".to_string())?;

    // Register a cancel flag so `cancel_report` can stop the orchestrator between passes.
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .report_tasks
        .lock()
        .await
        .insert(request_id.clone(), cancel.clone());

    // Stream progress as request-scoped `report_progress` events.
    let emitter = app.clone();
    let progress_id = request_id.clone();
    let progress = move |stage: &str, done: u32, total: u32| {
        let _ = emitter.emit(
            "report_progress",
            ReportProgress {
                request_id: progress_id.clone(),
                stage: stage.to_string(),
                done,
                total,
            },
        );
    };

    let result = kernel::reports::generate_report(
        store.as_ref(),
        answer.as_ref(),
        range,
        request.time_range.start,
        request.time_range.end,
        request.prompt.as_deref(),
        cfg,
        Some(&progress),
        &cancel,
    )
    .await;

    state.report_tasks.lock().await.remove(&request_id);
    let out = result.map_err(|e| e.to_string())?;
    // The footer's model provenance — only when a model actually ran.
    let model = if out.mode == ReportMode::Empty {
        None
    } else {
        answer.answer_model_label().await
    };
    Ok(ReportResponse {
        markdown: out.markdown,
        cited_frame_ids: out.cited_frame_ids,
        range_label,
        periods_total: out.periods_total,
        periods_covered: out.periods_covered,
        frames_sampled: out.frames_sampled,
        frames_summarized: out.frames_summarized,
        passes: out.passes,
        truncated: out.truncated,
        model,
    })
}

/// Generates the existing coverage-first report over exactly the named session's
/// owned frames. It shares progress/cancellation with recall reports and probes
/// filtered evidence before acquiring the answer provider.
#[tauri::command]
async fn session_recap(
    request: SessionRecapRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ReportResponse, String> {
    let request_id = request
        .request_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("session-recap-{}", next_ask_id()));
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    if store
        .get_session(request.session_id)
        .await
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(format!("session {} not found", request.session_id));
    }
    if !session_recap_has_evidence(store.as_ref(), request.session_id)
        .await
        .map_err(|error| error.to_string())?
    {
        return Ok(empty_report_response("this session".to_string()));
    }

    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| "kernel unavailable (database not open)".to_string())?;
    let settings = kernel::settings::load_settings(store.as_ref()).await;
    let cfg = ReportConfig {
        daily_top_k: settings.reports_daily_top_k,
        weekly_top_k: settings.reports_weekly_top_k,
        map_reduce_min_frames: settings.reports_map_reduce_min_frames,
        reply_budget: REPORT_REPLY_BUDGET,
    };
    let answer = kernel
        .answer_provider()
        .await
        .ok_or_else(|| "inference sidecar not ready yet".to_string())?;

    let cancel = Arc::new(AtomicBool::new(false));
    state
        .report_tasks
        .lock()
        .await
        .insert(request_id.clone(), cancel.clone());
    let emitter = app.clone();
    let progress_id = request_id.clone();
    let progress = move |stage: &str, done: u32, total: u32| {
        let _ = emitter.emit(
            "report_progress",
            ReportProgress {
                request_id: progress_id.clone(),
                stage: stage.to_string(),
                done,
                total,
            },
        );
    };

    let result = kernel::reports::generate_session_recap(
        store.as_ref(),
        answer.as_ref(),
        request.session_id,
        cfg,
        Some(&progress),
        &cancel,
    )
    .await;
    state.report_tasks.lock().await.remove(&request_id);
    let out = result.map_err(|error| error.to_string())?;
    let model = if out.mode == ReportMode::Empty {
        None
    } else {
        answer.answer_model_label().await
    };
    Ok(ReportResponse {
        markdown: out.markdown,
        cited_frame_ids: out.cited_frame_ids,
        range_label: "this session".to_string(),
        periods_total: out.periods_total,
        periods_covered: out.periods_covered,
        frames_sampled: out.frames_sampled,
        frames_summarized: out.frames_summarized,
        passes: out.passes,
        truncated: out.truncated,
        model,
    })
}

/// The honest no-evidence [`ReportResponse`] — no model ran. Mirrors the kernel's
/// `empty_output` body so a range with no captures returns identical output whether the
/// command short-circuits it (before the sidecar is ready) or the orchestrator does.
/// `passes == 0` tells the UI to render the message alone (no chips/footer).
fn empty_report_response(range_label: String) -> ReportResponse {
    ReportResponse {
        markdown: "No screen activity was captured for this range — there is nothing to summarize."
            .to_string(),
        cited_frame_ids: Vec::new(),
        range_label,
        periods_total: 0,
        periods_covered: 0,
        frames_sampled: 0,
        frames_summarized: 0,
        passes: 0,
        truncated: false,
        model: None,
    }
}

/// Cancel an in-flight `generate_report` (cooperative: stops at the next pass boundary).
#[tauri::command]
async fn cancel_report(request_id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(flag) = state.report_tasks.lock().await.get(&request_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Save a recall-report markdown body to the user's Downloads folder under a
/// collision-safe, date-stamped name (`save_report_markdown`, 0.3.1 D2, #65). The UI
/// builds the local-time stem (`screensearch-report-YYYY-MM-DD-HHmm`) — the WebView owns
/// the user's clock, so no timezone math happens here; this command only sanitizes the
/// stem to a safe leaf name, resolves Downloads (same `download_dir()` path as
/// `export_data`), never clobbers an existing file (`<stem>.md`, `<stem>-2.md`,
/// `<stem>-3.md`, …), and writes atomically through a `.partial` rename so a failed write
/// leaves no half-file. Returns the final path for the success toast.
#[tauri::command]
async fn save_report_markdown(
    stem: String,
    markdown: String,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| format!("cannot resolve the Downloads folder: {e}"))?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let final_path =
        unique_markdown_path(&dir, &sanitize_report_stem(&stem)).map_err(|e| e.to_string())?;

    // Write through a sibling `.partial` name and rename on success, so an interrupted or
    // failed write never leaves a plausible-looking report file (mirrors the export path,
    // `api::export::export_to_file`).
    let mut partial_os = final_path.clone().into_os_string();
    partial_os.push(".partial");
    let partial_path = PathBuf::from(partial_os);
    if let Err(e) = tokio::fs::write(&partial_path, markdown.as_bytes()).await {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(e.to_string());
    }
    if let Err(e) = tokio::fs::rename(&partial_path, &final_path).await {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(e.to_string());
    }
    Ok(final_path.to_string_lossy().into_owned())
}

/// Sanitizes a UI-supplied report filename stem to a single safe leaf name: keeps ASCII
/// alphanumerics, collapses every other run of characters to a single `-`, and trims
/// leading/trailing `-`. No path separator, drive letter, or `..` can survive, so the
/// result is always a direct child of the target dir; an empty result falls back to a
/// fixed stem. Pure (no IO) — unit-tested.
fn sanitize_report_stem(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    let mut prev_dash = false;
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "screensearch-report".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Resolves the first non-colliding path: `<dir>/<stem>.md`, then `<stem>-2.md`,
/// `<stem>-3.md`, … (0.3.1 D2 same-minute collision rule). `try_exists` surfaces a real
/// IO error rather than silently treating an unreadable entry as free. Unit-tested against
/// a temp dir.
fn unique_markdown_path(dir: &Path, stem: &str) -> std::io::Result<PathBuf> {
    let mut n: u32 = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}.md")
        } else {
            format!("{stem}-{n}.md")
        };
        let candidate = dir.join(name);
        if !candidate.try_exists()? {
            return Ok(candidate);
        }
        n += 1;
    }
}

/// Change the active model tier for a lane (`set_model_tier`, `03 §7`). Persists the
/// choice to settings and applies it to the live provider; the sidecar switches to the
/// new GGUF on the lane's next request (`03 §6`).
#[tauri::command]
async fn set_model_tier(request: SetModelTier, state: State<'_, AppState>) -> Result<(), String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    let value = serde_json::to_string(&request.tier).map_err(|e| e.to_string())?;
    let key = match request.lane {
        ModelLane::Vision => "models.vision_tier",
        ModelLane::Answer => "models.answer_tier",
    };
    store
        .set_setting(key, &value)
        .await
        .map_err(|e| e.to_string())?;

    match request.lane {
        ModelLane::Vision => {
            if let Some(v) = state.vision.lock().expect("vision slot").clone() {
                v.set_tier(request.tier);
            }
        }
        ModelLane::Answer => {
            if let Some(a) = state.answer.lock().expect("answer slot").clone() {
                a.set_tier(request.tier);
            }
        }
    }
    Ok(())
}

/// Eagerly load a lane's model into the sidecar (the "Load model" control). Spawns or
/// switches the sidecar so the next request on that lane is instant; surfaces any
/// download/spawn error so the panel can show it.
#[tauri::command]
async fn load_model(lane: ModelLane, state: State<'_, AppState>) -> Result<(), String> {
    match lane {
        ModelLane::Vision => {
            let v = state
                .vision
                .lock()
                .expect("vision slot")
                .clone()
                .ok_or_else(|| "inference sidecar not ready yet".to_string())?;
            v.preload().await.map_err(|e| e.to_string())
        }
        ModelLane::Answer => {
            let a = state
                .answer
                .lock()
                .expect("answer slot")
                .clone()
                .ok_or_else(|| "inference sidecar not ready yet".to_string())?;
            a.preload().await.map_err(|e| e.to_string())
        }
    }
}

/// Unload the resident sidecar model now, freeing its VRAM/RAM (the "Unload" control).
/// No-op if nothing is loaded; the next request lazily respawns the model.
#[tauri::command]
async fn unload_model(state: State<'_, AppState>) -> Result<(), String> {
    let supervisor = state
        .supervisor
        .lock()
        .expect("supervisor slot")
        .clone()
        .ok_or_else(|| "inference sidecar not ready yet".to_string())?;
    supervisor.unload().await;
    Ok(())
}

/// Frame-count density buckets over `[start, end)` for the Scanline Timeline
/// (`get_timeline`, `03 §7`). `bucket_count` is presentation-driven (the UI passes a
/// count derived from the ribbon width); the store returns sparse, occupied buckets.
#[tauri::command]
async fn get_timeline(
    range: TimeRange,
    bucket_count: u32,
    state: State<'_, AppState>,
) -> Result<Vec<TimelineBucket>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .timeline_buckets(range.start, range.end, clamp_timeline_buckets(bucket_count))
        .await
        .map_err(|e| e.to_string())
}

/// Lightweight frame list over `[start, end)` for browsing (`get_frames`, P5): the
/// Timeline's hover thumbnails, the Deck's "jump back in" recents, and a Moment's
/// neighbour context. Most-recent-first, capped at `limit`. Each row is a
/// [`FrameMeta`]; open one with `get_frame` for the full detail.
#[tauri::command]
async fn get_frames(
    range: TimeRange,
    limit: u32,
    state: State<'_, AppState>,
) -> Result<Vec<FrameMeta>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .frames_in_range(range.start, range.end, clamp_frame_list_limit(limit))
        .await
        .map_err(|e| e.to_string())
}

/// The frame whose capture time is nearest `at` (unix ms), or `null` if the DB has
/// no frames (`get_nearest_frame`, P5). Resolves the Timeline scan-head position to a
/// concrete frame id so "Enter opens the moment under the head" lands on a real frame.
#[tauri::command]
async fn get_nearest_frame(
    at: i64,
    range: Option<TimeRange>,
    state: State<'_, AppState>,
) -> Result<Option<FrameMeta>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    match range {
        Some(range) => store
            .nearest_frame_in_range(at, range.start, range.end)
            .await
            .map_err(|e| e.to_string()),
        None => store.nearest_frame(at).await.map_err(|e| e.to_string()),
    }
}

/// The captures bracketing `at` (unix ms) for a Moment's prev/next + context strip
/// (`get_frame_context`, P5): the `limit_each` closest frames on each side within
/// `±half_window_ms`, ascending by capture time, excluding the anchor. Unlike
/// `get_frames` (newest-first, capped), this guarantees the *adjacent* captures, so
/// prev/next always point at the true neighbours even in a busy session.
#[tauri::command]
async fn get_frame_context(
    at: i64,
    half_window_ms: i64,
    limit_each: u32,
    state: State<'_, AppState>,
) -> Result<Vec<FrameMeta>, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .neighbour_frames(at, half_window_ms, clamp_frame_context_limit(limit_each))
        .await
        .map_err(|e| e.to_string())
}

/// Truthful activity aggregates over `[start, end)` for the Insights screen
/// (`get_insights`, P5). Real DB counts only — honest-empty when the window is bare.
#[tauri::command]
async fn get_insights(
    range: TimeRange,
    bucket_count: u32,
    state: State<'_, AppState>,
) -> Result<InsightsSummary, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    store
        .insights_summary(range.start, range.end, clamp_insights_buckets(bucket_count))
        .await
        .map_err(|e| e.to_string())
}

/// Read the persisted user settings (`get_settings`, `03 §7/§8`). Missing keys fall
/// back to defaults (a fresh DB returns [`Settings::default`]).
#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    Ok(kernel::settings::load_settings(store.as_ref()).await)
}

/// Persist user settings (`set_settings`, `03 §7/§8`). All keys are written durably.
/// Model tiers **hot-apply** to the live providers here (same path as
/// `set_model_tier`); `answer.thinking` is read per-`ask`; capture/storage/privacy
/// take effect on the next capture start; enrichment workers reconfigure after save
/// and the image embedder reloads when the image lane is newly enabled.
#[tauri::command]
async fn set_settings(
    settings: Settings,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let store = state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())?;
    // Snapshot the capture-relevant config (interval, monitors, diff threshold, excluded
    // apps, pause-on-lock) BEFORE persisting, so we can tell whether a running capture loop
    // must pick up new values. Without this, capture/privacy changes silently waited for the
    // next manual capture start — the user-reported "Excluded Apps never applies" bug.
    let prev_settings = kernel::settings::load_settings(store.as_ref()).await;
    let prev_capture_cfg = kernel::settings::capture_config(&prev_settings);
    // Clamp once up front so the values handed to the live providers below are exactly what
    // gets persisted. `save_settings` sanitizes internally too, but a direct IPC call could
    // pass out-of-range values (e.g. a huge `sidecar_ctx_size`); without this, the DB would
    // store the clamped value while the next sidecar spawn ran the raw one until restart.
    let settings = kernel::settings::sanitize_settings(settings);

    // Register-before-persist for launch-at-login (0.3.2 PR3, `03 §7d`; sister-app B1
    // pattern): when `app.run_at_startup` changed, apply the OS registration BEFORE saving,
    // and abort the save if it fails. So the persisted setting can never claim a
    // launch-at-login that didn't take — the UI's optimistic mutation then rolls back and
    // shows the failure inline (`UI_REFERENCE §4`, Settings·App row).
    if settings.app_run_at_startup != prev_settings.app_run_at_startup {
        set_autostart(&app, settings.app_run_at_startup).map_err(|e| {
            format!(
                "couldn't {} run-at-startup: {e}",
                if settings.app_run_at_startup {
                    "enable"
                } else {
                    "disable"
                }
            )
        })?;
    }

    // Persist. If the save fails *after* the autostart registration was changed above, undo
    // that registration so the OS launch-at-login state can't outlive a setting that never
    // landed (otherwise run-at-startup stays flipped while the UI mutation rolls back). The
    // boot-time `reconcile_autostart` is the backstop; this keeps the live session honest.
    if let Err(e) = kernel::settings::save_settings(store.as_ref(), &settings).await {
        if settings.app_run_at_startup != prev_settings.app_run_at_startup {
            let _ = set_autostart(&app, prev_settings.app_run_at_startup);
        }
        return Err(e.to_string());
    }

    // Refresh the tray's cached close-to-tray flag so the (synchronous) CloseRequested
    // handler sees the new value immediately (0.3.2 PR3).
    tray::set_close_to_tray(&app, settings.app_close_to_tray);

    // Re-run the shell registration check after every save. Each helper no-ops when the
    // persisted chord is already active, but retries a previously failed registration for
    // the same saved chord after the user releases an OS/app conflict (D6).
    overlay::reregister_overlay_hotkey(&app, &settings.overlay_hotkey);
    overlay::reregister_marks_hotkey(&app, &settings.marks_hotkey);

    // Hot-apply every UIA knob: the composite swaps its config and, if anything changed, tears
    // down + lazily respawns its UI Automation client so the new values (enable, budget, node
    // caps, view, input-suppression) all take effect immediately — no capture restart, no app
    // restart. Disabling UIA here also releases the client, so Chromium/Electron apps leave
    // accessibility mode (`docs/0.2.0.md` #48; the "disable didn't stop the hangs" fix).
    if let Some(uia) = &state.uia {
        uia.apply_settings(&settings);
    }

    // Hot-apply sidecar lane settings; embedding workers are handled below. The launch
    // params (ngl/device/ctx/KV/flash) are all launch args, so they take effect when the
    // supervisor relaunches on the next request (a changed param makes `needs_restart`
    // true) — no immediate process kill here.
    let sidecar_params = traits::SidecarParams::from(&settings);
    if let Some(v) = state.vision.lock().expect("vision slot").clone() {
        v.set_tier(settings.models_vision_tier);
        v.set_launch_options(sidecar_params.clone());
    }
    if let Some(a) = state.answer.lock().expect("answer slot").clone() {
        a.set_tier(settings.models_answer_tier);
        a.set_launch_options(sidecar_params);
    }
    if let Some(kernel) = state.kernel.clone() {
        let embeddings_enabled = settings.enrich_embed_text;
        let embed_status = kernel.readiness().embed_model.status;
        if embeddings_enabled
            && matches!(
                embed_status,
                ComponentStatus::Disabled | ComponentStatus::Unavailable | ComponentStatus::Unknown
            )
        {
            kernel.set_embed_readiness(
                ComponentStatus::Initializing,
                Some("loading embedding model".to_string()),
            );
            let dyn_store: Arc<dyn Store> = store.clone();
            tauri::async_runtime::spawn(init_embeddings(
                kernel,
                dyn_store,
                state.embed_models_dir.clone(),
            ));
        } else {
            kernel.reconfigure_enrichment().await;
        }
    }
    // Hot-apply the enrichment-throttle master switch: start/stop the governor to match
    // `throttle.enabled` (docs/0.2.0.md former PR5). Threshold + cadence edits are picked up
    // by the running loop each sample tick, so only the on/off toggle needs this.
    if let Some(kernel) = state.kernel.clone() {
        kernel.reconfigure_throttle().await;
    }
    // Hot-apply capture/privacy changes to a running capture loop (restarts it so the new
    // CaptureConfig — e.g. a freshly excluded app — takes effect now, not on the next manual
    // start). No-op when nothing capture-relevant changed or capture isn't running.
    if let Some(kernel) = state.kernel.clone() {
        if kernel::settings::capture_config(&settings) != prev_capture_cfg {
            if let Err(e) = kernel.reload_capture().await {
                tracing::warn!(error = %e, "capture reload after settings change failed");
            }
        }
    }
    Ok(())
}

/// Application entry point (called from `main.rs`).
pub fn run() {
    tauri::Builder::default()
        // MUST be the first plugin. Users double-click or relaunch when nothing seems to be
        // happening; without this a second instance runs against the *same* app-data dir and
        // races the first — corrupting the SQLite DB and colliding on hf-hub's per-model cache
        // lock (the ~5 s `LockAcquisition` download failure + retry storm we diagnosed). Instead,
        // focus the existing window and let the second process exit.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch behaves as tray "Open" (`03 §7d`): restore a hidden /
            // tray-minimized window (show *before* unminimize + focus — unminimize alone
            // won't re-show a `hide()`d window) and, if this is the first restore after a
            // close-to-tray hide, fire the one-time explanatory toast.
            tray::restore_main_window(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Opens external URLs (the NavRail version link + report/answer markdown links) in
        // the OS default browser; Tauri v2 does not open plain `target="_blank"` anchors
        // (0.3.1 D4, #57). Scoped by the `opener:allow-open-url` capability.
        .plugin(tauri_plugin_opener::init())
        // Auto-update (0.3.2 PR2, #69; `03 §11b`). The public key + GitHub-Releases
        // `latest.json` endpoint come from `tauri.conf.json` (`plugins.updater`); the
        // check/download/install flow is driven from `update.rs` via our own typed
        // commands (no `updater:*` capability needed — D1).
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Launch-at-login for the "Run at startup" setting (0.3.2 PR3, #56; `03 §7d`,
        // default OFF). Registration is driven from Rust (`set_settings` + the boot
        // reconcile below) so the UI needs no `autostart:*` capability. The macOS launcher
        // arg is required by the plugin API but inert on Windows.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // Resolve the per-user app data dir from the bundle identifier and make
            // sure it (and the log dir) exist before we open anything in them.
            let data_dir = app.path().app_data_dir()?;
            let log_dir = data_dir.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            init_tracing(&log_dir);

            let db_path = data_dir.join("screensearch.db");
            let frames_dir = data_dir.join("frames");
            let embed_models_dir = data_dir.join("models").join("fastembed");

            let (store, db_readiness) = open_store(&db_path);
            app.manage(overlay::OverlayState::default());
            // Auto-update state (0.3.2 PR2, #69). Check on launch in **release builds
            // only** — a debug build (`npm run dev`) skips the check so development
            // never hits the live GitHub-Releases endpoint on every start. The manual
            // "Check for updates" command works in every build. Pull-based + quiet (D1):
            // a found update downloads in the background and installs only on a
            // user-initiated restart.
            app.manage(update::UpdaterState::default());
            if !cfg!(debug_assertions) {
                tauri::async_runtime::spawn(update::launch_check(app.handle().clone()));
            }
            // Load the persisted settings once, up front: the hotkey chords seed the global
            // shortcuts (below) and the whole struct seeds the tray (`app.close_to_tray`
            // for the CloseRequested handler) + the boot autostart reconcile (`03 §7d`).
            let startup_settings = match &store {
                Some(store) => {
                    tauri::async_runtime::block_on(kernel::settings::load_settings(store.as_ref()))
                }
                None => Settings::default(),
            };
            // Register both global hotkeys (Flow overlay + mark-this-moment, 0.3.0 PR5/PR6)
            // from the persisted chords; a failed registration surfaces as a loud Settings
            // warning + toast (D6), never a silent no-op.
            overlay::init_overlay_hotkey(app.handle(), &startup_settings.overlay_hotkey);
            overlay::init_marks_hotkey(app.handle(), &startup_settings.marks_hotkey);
            // Sync launch-at-login to the persisted `app.run_at_startup` (0.3.2 PR3): if the
            // OS registration drifted from the setting (manual change, failed prior save),
            // reconcile it. Best-effort — a failure just logs; capture is unaffected.
            reconcile_autostart(app.handle(), startup_settings.app_run_at_startup);
            // PR3 audit fix: backfill the attention filter_version once at startup. When the
            // version bumped since the last run, re-clean every sub-current frame's
            // content_text against the now-warm chrome catalog (`textfilter::reconcile`) so
            // the live classifier's cold-start window no longer leaves app/nav chrome
            // permanently searchable (docs/AUDIT_0.2.0_PR3_2026-06-26.md). Spawned in the
            // background so launch never blocks; batched + idempotent + resumable, and
            // capture is user-triggered (much later), so there is no capture contention.
            if let Some(store) = &store {
                let store = store.clone();
                let purge_dir = data_dir.clone();
                tauri::async_runtime::spawn(async move {
                    // Purge any settings keys retired by a past version (0.3.0 PR2 event
                    // triggers) so a config persisted with them still loads and the stale
                    // rows don't linger; logs once (the keys are gone next launch).
                    kernel::settings::drop_retired_settings(store.as_ref()).await;
                    // Then sweep out any pre-existing own-window captures (PR3 self-
                    // exclude), then re-clean the remaining frames against the warm
                    // chrome catalog.
                    purge_self_captures(store.clone(), purge_dir).await;
                    let s = kernel::settings::load_settings(store.as_ref()).await;
                    match store
                        .backfill_filter_version(
                            store::FILTER_VERSION,
                            s.text_chrome_suppress_min_seen,
                            s.text_chrome_protect_min_chars,
                            s.text_chrome_region_buckets,
                            s.enrich_embed_text,
                        )
                        .await
                    {
                        Ok(0) => {}
                        Ok(n) => {
                            tracing::info!(frames = n, "filter_version backfill re-cleaned frames")
                        }
                        Err(e) => tracing::warn!(error = %e, "filter_version backfill failed"),
                    }
                    // Then reclaim DB rows for frames already purged before per-line span
                    // merging shipped (`07` #73a). After the backfill above, so any purged
                    // frame still gets accurate content_text from its per-word spans first.
                    merge_purged_spans_once(store.clone()).await;
                });
            }
            let readiness = Readiness {
                db: db_readiness,
                ..Default::default()
            };

            // Build the kernel only if the spine opened. It owns the live readiness
            // (capture starts Disabled) and the event bus. `spawn_ocr` also hands back the
            // concrete UIA composite (when OCR is available) so `set_settings` can hot-apply
            // UIA knobs; the kernel only sees it as `dyn OcrProvider`.
            let mut uia_composite: Option<Arc<UiaWithOcrFallback>> = None;
            let kernel = store.as_ref().map(|store| {
                let dyn_store: Arc<dyn Store> = store.clone();
                let (ocr, uia, ocr_unavailable) = spawn_ocr(store);
                uia_composite = uia;
                match ocr_unavailable {
                    Some(reason) => Arc::new(Kernel::new_with_ocr_unavailable(
                        dyn_store,
                        ocr,
                        capture_factory(),
                        frames_dir.clone(),
                        readiness.clone(),
                        reason,
                    )),
                    None => Arc::new(Kernel::new(
                        dyn_store,
                        ocr,
                        capture_factory(),
                        frames_dir.clone(),
                        readiness.clone(),
                    )),
                }
            });

            // Slots filled off the launch thread (P4 inference), shared with command
            // handlers and the exit path.
            let supervisor_slot: SharedSlot<Arc<ModelSupervisor>> = Arc::new(StdMutex::new(None));
            let vision_slot: SharedSlot<Arc<VisionSidecar>> = Arc::new(StdMutex::new(None));
            let answer_slot: SharedSlot<Arc<AnswerSidecar>> = Arc::new(StdMutex::new(None));
            let sidecar_binary_slot: SharedSlot<PathBuf> = Arc::new(StdMutex::new(None));

            // Forward kernel events to the UI, and kick off the off-thread model loads
            // (P3 embeddings + P4 inference) — app launch is never blocked on the
            // first-run downloads.
            if let (Some(kernel), Some(store)) = (&kernel, &store) {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(forward_events(kernel.clone(), handle));

                // The pure sessions engine lives in its own module crate; the kernel
                // sees only `SessionSegmenter`. A bundled-taxonomy parse failure is a
                // startup wiring error, while runtime pass failures remain isolated
                // inside the scheduler and never gate capture (0.4.0 D10).
                let segmenter = Arc::new(sessions::SessionEngine::new()?);
                {
                    let kernel = kernel.clone();
                    tauri::async_runtime::spawn(async move {
                        kernel.attach_segmenter(segmenter).await;
                    });
                }

                // Inject the system-pressure probe (the only `unsafe`, in `sysmon`) and
                // start the enrichment-throttle governor if `throttle.enabled` (docs/0.2.0.md
                // former PR5). Independent of inference — it also governs embed_text.
                kernel.set_pressure_probe(sysmon::native_probe());
                {
                    let kernel = kernel.clone();
                    tauri::async_runtime::spawn(async move { kernel.reconfigure_throttle().await });
                }

                kernel.set_embed_readiness(
                    ComponentStatus::Initializing,
                    Some("loading embedding model".to_string()),
                );
                let dyn_store: Arc<dyn Store> = store.clone();
                tauri::async_runtime::spawn(init_embeddings(
                    kernel.clone(),
                    dyn_store,
                    embed_models_dir.clone(),
                ));

                // Resolve the sidecar binary + wire the inference providers off-thread.
                kernel.set_sidecar_readiness(
                    ComponentStatus::Initializing,
                    Some("resolving llama-server".to_string()),
                );
                let dyn_store: Arc<dyn Store> = store.clone();
                tauri::async_runtime::spawn(init_inference(
                    kernel.clone(),
                    dyn_store,
                    data_dir.clone(),
                    supervisor_slot.clone(),
                    vision_slot.clone(),
                    answer_slot.clone(),
                    sidecar_binary_slot.clone(),
                ));
                tauri::async_runtime::spawn(retention_sweeper(
                    store.clone(),
                    kernel.clone(),
                    data_dir.clone(),
                ));
            }

            // Build the local-API host adapter (0.3.0 PR7) when the spine is up. The
            // server itself is not started here — `local_api::autostart` starts it only
            // when `api.enabled`, after `AppState` is managed.
            let started_at_ms = now_ms();
            let api_host = match (&store, &kernel) {
                (Some(store), Some(kernel)) => Some(Arc::new(local_api::TauriApiHost::new(
                    store.clone(),
                    kernel.clone(),
                    app.handle().clone(),
                    data_dir.clone(),
                    started_at_ms,
                ))),
                _ => None,
            };

            app.manage(AppState {
                store,
                kernel,
                fallback_readiness: readiness,
                supervisor: supervisor_slot,
                vision: vision_slot,
                answer: answer_slot,
                sidecar_binary: sidecar_binary_slot,
                ask_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                report_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                uia: uia_composite,
                embed_models_dir,
                db_path,
                frames_dir,
                api_runtime: Arc::new(local_api::ApiRuntime::new()),
                api_host,
            });

            // Build the system tray now that `AppState` is managed (its menu handlers reach
            // the kernel/sidecar slots through it). Seed the icon from the live readiness
            // snapshot and the vision label from the durable queue (a restart with pending
            // `vision_tag` jobs must open on "Stop vision tagging", not "Start" — no
            // `JobProgress` is emitted until a worker settles). `forward_events` keeps both
            // in sync from here on (no poller, `03 §7d`).
            let (tray_readiness, tray_vision_active) = {
                let state = app.state::<AppState>();
                let readiness = match &state.kernel {
                    Some(kernel) => kernel.readiness(),
                    None => state.fallback_readiness.clone(),
                };
                let vision_active = match &state.store {
                    Some(store) => tauri::async_runtime::block_on(store.job_stats())
                        .map(|s| s.vision_pending + s.vision_running > 0)
                        .unwrap_or(false),
                    None => false,
                };
                (readiness, vision_active)
            };
            tray::init(
                app.handle(),
                &startup_settings,
                &tray_readiness,
                tray_vision_active,
            );

            // Capture is a backend lifecycle invariant, not a WebView mount side effect: 8 of
            // 75 measured launches never reached the frontend start command and captured
            // nothing (usage review 2026-08-01 §3.4). Start synchronously after the kernel,
            // managed state, and tray are wired: a detached task could first run after
            // `graceful_shutdown` stops capture during an immediate quit. `start_capture` is
            // idempotent, so the frontend's later Start command remains a harmless no-op.
            // OCR/source failures are loud but non-fatal; re-syncing from the resulting readiness
            // snapshot also closes the startup event-subscription race for the tray without a
            // poller.
            if let Some(kernel) = app.state::<AppState>().kernel.clone() {
                let handle = app.handle().clone();
                if let Err(e) = tauri::async_runtime::block_on(kernel.start_capture()) {
                    tracing::warn!(
                        error = %e,
                        "capture autostart failed; app will continue without capture"
                    );
                }
                tray::on_readiness(&handle, &kernel.readiness());
            }

            // Start the local API if it was left enabled (loud on a bind failure, D6).
            local_api::autostart(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            get_readiness,
            get_job_stats,
            get_storage_stats,
            get_monitors,
            list_sidecar_devices,
            get_frame,
            get_frame_spans,
            list_sessions,
            get_session,
            search,
            capture_control,
            enqueue_vision,
            cancel_vision,
            ask,
            cancel_ask,
            generate_report,
            session_recap,
            cancel_report,
            save_report_markdown,
            set_model_tier,
            load_model,
            unload_model,
            get_timeline,
            get_frames,
            get_nearest_frame,
            get_frame_context,
            get_insights,
            get_settings,
            set_settings,
            get_text_filter_stats,
            get_throttle_status,
            overlay::get_hotkey_status,
            overlay::hide_overlay,
            overlay::overlay_shown_ack,
            overlay::open_moment,
            overlay::focus_overlay_for_note,
            overlay::dismiss_mark_toast,
            where_was_i,
            add_mark,
            list_marks,
            resolve_mark,
            set_mark_note,
            local_api::set_api_config,
            local_api::get_api_status,
            local_api::regenerate_api_token,
            local_api::export_data,
            update::get_update_status,
            update::check_for_updates,
            update::restart_to_apply_update
        ])
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                    // Close-to-tray (0.3.2 PR3, `03 §7d`): with `app.close_to_tray` on (the
                    // default), hide the window and keep capturing — the hidden overlay
                    // window already keeps the process alive, so nothing extra is needed to
                    // stay resident. A close-to-tray hide arms the one-time explanatory toast
                    // (shown on the next restore). With the setting off — or if the tray
                    // failed to build (`close_to_tray_enabled` then returns false) — window
                    // close quits cleanly (→ `RunEvent::ExitRequested` → `graceful_shutdown`).
                    api.prevent_close();
                    let app = window.app_handle();
                    if tray::close_to_tray_enabled(app) {
                        let _ = window.hide();
                        tray::note_hidden_to_tray(app);
                    } else {
                        app.exit(0);
                    }
                }
                tauri::WindowEvent::Focused(false)
                    if window.label() == "overlay"
                        && std::env::var_os("SCREENSEARCH_OVERLAY_STICKY").is_none() =>
                {
                    let _ = window.hide();
                    let _ = window.emit("overlay_hidden", ());
                }
                tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "overlay" => {
                    api.prevent_close();
                    let _ = window.hide();
                    let _ = window.emit("overlay_hidden", ());
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // On exit, stop capture before anything that can block, so no new frames are
            // captured or persisted once quit begins (#84). The local API's graceful
            // shutdown below can wait up to 3 s draining an open `/v1/ask` SSE, and timer/
            // event capture must not keep firing during that window. Then stop the local
            // API, drain the vision scheduler + worker pool so in-flight work finishes
            // cleanly, and shut the sidecar down. Best-effort: a job left `running` is
            // requeued by the startup stale-job sweep, and the Job Object would terminate
            // the sidecar anyway (`03 §6`).
            if let tauri::RunEvent::ExitRequested { .. } = event {
                tauri::async_runtime::block_on(graceful_shutdown(app_handle));
            }
        });
}

/// Ordered, best-effort graceful shutdown of every runtime subsystem, shared by normal
/// quit (`RunEvent::ExitRequested`) and install-on-restart (`update::restart_to_apply_update`)
/// so the two can never drift. Stop capture first, so no new frames are captured or
/// persisted once quit begins (#84) and the local API's bounded drain (below, up to ~3 s
/// on an open `/v1/ask` SSE, `03 §7c`) can't race timer/event capture. Then stop the local
/// API, drain the vision scheduler + worker pool so in-flight work finishes cleanly, and
/// shut the sidecar down. Every step is idempotent, so a second invocation (e.g. the
/// installer's own exit after `restart_to_apply_update` already ran this) is harmless: a
/// job left `running` is requeued by the startup stale-job sweep, and the Job Object would
/// terminate the sidecar anyway (`03 §6`).
pub(crate) async fn graceful_shutdown(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let kernel = state.kernel.clone();
    if let Some(kernel) = &kernel {
        kernel.stop_capture().await;
    }
    let api_runtime = state.api_runtime.clone();
    local_api::stop_server(&api_runtime).await;
    if let Some(kernel) = kernel {
        kernel.stop_throttle().await;
        kernel.stop_sessions_scheduler().await;
        kernel.stop_vision_scheduler().await;
        kernel.stop_workers().await;
    }
    let supervisor = state.supervisor.lock().expect("supervisor slot").clone();
    if let Some(supervisor) = supervisor {
        supervisor.shutdown().await;
    }
}

/// Loads the fastembed model off the launch thread and attaches it to the kernel —
/// which lights up the store's vector arm and starts the worker pool that drains the
/// `embed_text` backlog (`03 §5`). App start never blocks on the multi-hundred-MB
/// first-run download; `embed_model` readiness reflects progress
/// (Initializing → Ready / Unavailable / Disabled). Skips loading entirely when text
/// embeddings are off.
async fn init_embeddings(kernel: Arc<Kernel>, store: Arc<dyn Store>, models_dir: PathBuf) {
    let settings = kernel::settings::load_settings(store.as_ref()).await;
    if !settings.enrich_embed_text {
        kernel.set_embed_readiness(
            ComponentStatus::Disabled,
            Some("embeddings disabled in settings".to_string()),
        );
        return;
    }
    match tokio::task::spawn_blocking(move || FastEmbedProvider::new(models_dir)).await {
        Ok(Ok(provider)) => {
            tracing::info!("embedding model loaded; attaching to kernel");
            kernel.attach_embedder(Arc::new(provider)).await;
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "embedding model load failed");
            kernel.set_embed_readiness(ComponentStatus::Unavailable, Some(e.to_string()));
        }
        Err(e) => {
            tracing::error!(error = %e, "embedding model load task panicked");
            kernel.set_embed_readiness(
                ComponentStatus::Unavailable,
                Some("model load task panicked".to_string()),
            );
        }
    }
}

/// Resolves the `llama-server` binary off the launch thread, builds the
/// [`ModelSupervisor`] + the tiered vision/answer providers, and attaches them to the
/// kernel (P4, `03 §6`). Lazy by design: the sidecar binary is fetched now, but models
/// download — and the process spawns — only on the first real request. `sidecar`
/// readiness reflects progress (Initializing → Ready / Unavailable), and the
/// supervisor's lifecycle transitions are bridged into the kernel's event bus.
async fn init_inference(
    kernel: Arc<Kernel>,
    store: Arc<dyn Store>,
    data_dir: PathBuf,
    supervisor_slot: SharedSlot<Arc<ModelSupervisor>>,
    vision_slot: SharedSlot<Arc<VisionSidecar>>,
    answer_slot: SharedSlot<Arc<AnswerSidecar>>,
    sidecar_binary_slot: SharedSlot<PathBuf>,
) {
    let sidecar_dir = data_dir.join("sidecar");
    let models_root = data_dir.join("models");
    if let Err(e) = std::fs::create_dir_all(&sidecar_dir) {
        kernel.set_sidecar_readiness(ComponentStatus::Unavailable, Some(e.to_string()));
        return;
    }
    let pidfile = sidecar_dir.join("llama-server.pid");
    let mut reap_binaries = inference::download::installed_binary_candidates(&sidecar_dir);
    if inference::supervisor::reap_stray_any(&pidfile, &reap_binaries) {
        tracing::warn!("startup reap terminated a stray sidecar before binary resolution");
    }

    // Fetch the prebuilt Vulkan llama-server (idempotent; skipped if already present).
    let binary = match inference::download::ensure_binary(&sidecar_dir).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "llama-server unavailable");
            kernel.set_sidecar_readiness(
                ComponentStatus::Unavailable,
                Some(format!("llama-server unavailable: {e}")),
            );
            return;
        }
    };

    let settings = kernel::settings::load_settings(store.as_ref()).await;
    *sidecar_binary_slot.lock().expect("sidecar binary slot") = Some(binary.clone());
    reap_binaries.push(binary.clone());
    // Probe the bundled binary once so `build_args` only emits memory-tuning flags it
    // actually accepts. The probe spawns `llama-server --help` (a blocking syscall), so it
    // runs on a blocking pool rather than parking this async worker; on failure we fall
    // back to the conservative caps (context-only) the probe itself would return.
    // Probed once per process: the binary download path is idempotent (skip-if-present),
    // so a running app keeps the same binary — and thus the same caps — until restart.
    let binary_for_probe = binary.clone();
    let caps = tokio::task::spawn_blocking(move || inference::probe_caps(&binary_for_probe))
        .await
        .unwrap_or_else(|_| inference::SidecarCaps::conservative());
    let config = SupervisorConfig {
        binary,
        reap_binaries,
        pidfile,
        sidecar_log: Some(sidecar_dir.join("llama-server.log")),
        idle_ttl: Duration::from_secs(settings.sidecar_idle_ttl_secs as u64),
        health_timeout: SIDECAR_HEALTH_TIMEOUT,
        caps,
        recycle_ceiling_bytes: inference::resolve_recycle_ceiling(
            settings.sidecar_recycle_enabled,
            settings.sidecar_recycle_rss_mb,
        ),
    };
    let supervisor = match ModelSupervisor::new(config) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "model supervisor init failed");
            kernel.set_sidecar_readiness(ComponentStatus::Unavailable, Some(e.to_string()));
            return;
        }
    };

    let sidecar_params = traits::SidecarParams::from(&settings);
    // One downloader, shared by both lanes: it serializes per-lane fetches (no concurrent
    // races) and broadcasts progress for the UI.
    let downloader = inference::ModelDownloader::new(models_root.clone());
    let vision = Arc::new(VisionSidecar::new(
        supervisor.clone(),
        downloader.clone(),
        models_root.clone(),
        settings.models_vision_tier,
        sidecar_params.clone(),
    ));
    let answer = Arc::new(AnswerSidecar::new(
        supervisor.clone(),
        downloader.clone(),
        models_root,
        settings.models_answer_tier,
        sidecar_params,
    ));
    *vision_slot.lock().expect("vision slot") = Some(vision.clone());
    *answer_slot.lock().expect("answer slot") = Some(answer.clone());
    *supervisor_slot.lock().expect("supervisor slot") = Some(supervisor.clone());

    // Bridge the supervisor's lifecycle transitions into the kernel (updates `sidecar`
    // readiness + re-broadcasts as `sidecar_status`).
    {
        let kernel = kernel.clone();
        let mut rx = supervisor.subscribe();
        tauri::async_runtime::spawn(async move {
            // A `Lagged` (receiver fell behind a burst) must NOT end the bridge — that
            // would silently freeze the StatusRail for the rest of the session. Skip the
            // missed events and keep going; only stop when the sender is gone (`Closed`).
            loop {
                match rx.recv().await {
                    Ok(status) => kernel.emit_sidecar_status(status),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Bridge model-download progress into the kernel (re-broadcast as `model_download`).
    {
        let kernel = kernel.clone();
        let mut rx = downloader.subscribe();
        tauri::async_runtime::spawn(async move {
            // Same as above: a lagging receiver must keep bridging progress, not die and
            // leave the download chip frozen for the rest of the run.
            loop {
                match rx.recv().await {
                    Ok(status) => kernel.emit_model_download(status),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Idle source for idle-triggered tagging (the kernel forbids `unsafe`).
    let idle: IdleSource = Arc::new(capture::user_idle_ms);
    // The supervisor is the keep-warm authority: while idle backfill drains the backlog it
    // suppresses idle-TTL eviction so the model stays loaded for the whole drain (`03 §5`).
    let backfill = supervisor.clone() as Arc<dyn traits::BackfillControl>;
    kernel
        .attach_inference(
            vision as Arc<dyn traits::VisionProvider>,
            answer as Arc<dyn traits::AnswerProvider>,
            Some(idle),
            Some(backfill),
        )
        .await;
    kernel.set_sidecar_readiness(
        ComponentStatus::Ready,
        Some("ready (model downloads + spawns on first use)".to_string()),
    );
    tracing::info!("inference attached; sidecar ready (lazy spawn)");
}

/// Forwards [`KernelEvent`]s onto the Tauri event bus for the WebView2 UI (`03 §7`).
async fn forward_events(kernel: Arc<Kernel>, app: tauri::AppHandle) {
    use tokio::sync::broadcast::error::RecvError;
    let mut rx = kernel.subscribe();
    loop {
        match rx.recv().await {
            Ok(KernelEvent::CaptureTick(tick)) => {
                let _ = app.emit("capture_tick", tick);
            }
            Ok(KernelEvent::ReadinessChanged(readiness)) => {
                // The tray icon/tooltip/pause-label track live capture state (`03 §7d`) —
                // this is the "same state that drives the StatusRail, no separate poller".
                tray::on_readiness(&app, &readiness);
                let _ = app.emit("readiness_changed", readiness);
            }
            Ok(KernelEvent::JobProgress(stats)) => {
                tray::on_job_stats(&app, &stats);
                let _ = app.emit("job_progress", stats);
            }
            Ok(KernelEvent::JobCompleted(completed)) => {
                // Carries fresh stats too, so the tray's vision label flips back when the
                // backlog drains (JobProgress alone fires on enqueue/cancel).
                tray::on_job_stats(&app, &completed.stats);
                let _ = app.emit("job_completed", completed);
            }
            Ok(KernelEvent::SidecarStatus(status)) => {
                tray::on_sidecar_status(&app, &status);
                let _ = app.emit("sidecar_status", status);
            }
            Ok(KernelEvent::ModelDownload(status)) => {
                let _ = app.emit("model_download", status);
            }
            Ok(KernelEvent::Toast(toast)) => {
                let _ = app.emit("toast", toast);
            }
            Ok(KernelEvent::ThrottleChanged(status)) => {
                let _ = app.emit("throttle_changed", status);
            }
            Ok(KernelEvent::SessionsChanged) => {
                let _ = app.emit("sessions_changed", ());
            }
            Err(RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "event bus lagged; some ticks dropped")
            }
            Err(RecvError::Closed) => break,
        }
    }
}

async fn storage_stats(db_path: PathBuf, frames_dir: PathBuf) -> Result<StorageStats, String> {
    tokio::task::spawn_blocking(move || {
        let db_bytes = db_file_family_size(&db_path);
        let frame_bytes = dir_size(&frames_dir).map_err(|e| e.to_string())?;
        Ok(StorageStats {
            db_bytes,
            frame_bytes,
            total_bytes: db_bytes.saturating_add(frame_bytes),
        })
    })
    .await
    .map_err(|e| format!("storage stats task failed: {e}"))?
}

fn db_file_family_size(db_path: &Path) -> u64 {
    let mut total = file_size(db_path);
    for suffix in ["-wal", "-shm"] {
        let mut os = db_path.as_os_str().to_os_string();
        os.push(suffix);
        total = total.saturating_add(file_size(Path::new(&os)));
    }
    total
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn dir_size(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0_u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(0);
    };
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            total = total.saturating_add(dir_size(&entry.path())?);
        } else if meta.is_file() {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

fn list_devices_from_binary(binary: &Path) -> Result<Vec<String>, String> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: enumerating GPU devices for the Settings panel runs
    // `llama-server --list-devices`, a console exe; without this flag it flashes a terminal
    // window every time Settings opens. Mirrors `inference::flags`/`inference::process`.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new(binary)
        .arg("--list-devices")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("run {} --list-devices: {e}", binary.display()))?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(combined.trim().to_string());
    }
    Ok(parse_sidecar_device_ids(&combined))
}

fn parse_sidecar_device_ids(output: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim().trim_start_matches(['-', '*', ' ']).trim();
        let token = trimmed
            .split([':', ' ', '\t'])
            .next()
            .unwrap_or_default()
            .trim();
        // llama.cpp device ids are at least four characters (`CUDA0`,
        // `Vulkan0`, `Metal`, etc.); shorter tokens are usually log noise.
        if token.len() >= 4
            && token.chars().any(|c| c.is_ascii_digit())
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !out.iter().any(|d| d == token)
        {
            out.push(token.to_string());
        }
    }
    out
}

async fn retention_sweeper(store: Arc<SqliteStore>, kernel: Arc<Kernel>, data_dir: PathBuf) {
    loop {
        match run_retention_once(store.clone(), kernel.clone(), &data_dir).await {
            Ok(n) if n > 0 => {
                tracing::info!(
                    degraded = n,
                    "retention sweep expired screenshots (text kept)"
                )
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "retention sweep failed"),
        }
        tokio::time::sleep(RETENTION_INTERVAL).await;
    }
}

async fn run_retention_once(
    store: Arc<SqliteStore>,
    kernel: Arc<Kernel>,
    data_dir: &Path,
) -> Result<u64, String> {
    let settings = kernel::settings::load_settings(store.as_ref()).await;
    if settings.storage_retention_days == 0 {
        return Ok(0);
    }
    let retention_ms = i64::from(settings.storage_retention_days).saturating_mul(DAY_MS);
    let cutoff = now_ms().saturating_sub(retention_ms);
    // Degrade-to-text retention: past the window we delete the screenshot file but KEEP the
    // row + raw/content text + embeddings as durable, searchable proof, and collapse the
    // per-word `text_spans` to per-line (`07` #73a) so the DB shrinks too — the Moment view
    // still renders a line-level text+layout reconstruction. Candidates exclude already-
    // degraded frames so the sweep converges instead of re-listing text-only rows every hour.
    let candidates = store
        .frames_with_image_older_than(cutoff, RETENTION_BATCH)
        .await
        .map_err(|e| e.to_string())?;
    let mut degraded = 0_u64;
    for frame in candidates {
        if let Some(path) = safe_frame_path(data_dir, &frame.image_path) {
            if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    // Leave image_purged=0 so a transient failure (AV/indexer sharing
                    // violation) is retried next sweep instead of marking the row text-only
                    // while its file lingers.
                    tracing::warn!(path = %path.display(), error = %e, "retention could not delete frame image");
                    continue;
                }
            }
        }
        // Degrade the frame in the DB atomically: collapse the now-permanent per-word
        // `text_spans` to per-line AND mark `image_purged = 1` in one transaction (`07` #73a).
        // Atomic so a transient failure never leaves the row flagged purged with its per-word rows
        // stranded — the sweep only re-lists `image_purged = 0`, so on error we `continue` with the
        // flag unset and the whole frame is retried next sweep (the file is already gone; the next
        // `remove_file` tolerates NotFound). The FrameReconstruction stays intact (line-level).
        if let Err(e) = store.degrade_frame_to_text(frame.frame_id).await {
            tracing::error!(frame_id = frame.frame_id, error = %e, "retention could not degrade frame to text");
            continue;
        }
        degraded += 1;
    }
    if degraded > 0 {
        // Keep "retention" in the message — useLiveEvents refreshes storage stats on it.
        kernel.emit_toast(
            ToastLevel::Info,
            format!("Retention expired {degraded} old screenshots (text kept)"),
        );
    }
    Ok(degraded)
}

/// Settings watermark marking that the one-time self-capture purge has run.
const SELF_PURGE_KEY: &str = "maintenance.self_capture_purged";

/// Settings watermark marking that the one-time per-word→per-line span merge for
/// already-purged frames has run.
const PURGED_SPANS_MERGED_KEY: &str = "maintenance.purged_spans_merged";

/// One-time backfill (`07` #73a): collapse the per-word `text_spans` of frames that were
/// retention-purged *before* per-line merging shipped down to per-line spans, reclaiming DB
/// rows while keeping the `FrameReconstruction` intact. New purges are merged inline by the
/// retention sweep; this drains the pre-existing backlog. Gated by [`PURGED_SPANS_MERGED_KEY`]
/// so it runs once (a clean/fresh DB finds no purged frames and no-ops); cursor-batched so the
/// connection is released between batches. The watermark is recorded only on a **clean** full
/// drain — i.e. every listed frame was merged with no error — so any failure (a list error *or*
/// a per-frame merge error, e.g. the DB briefly busy) leaves the watermark unset and the whole
/// backfill retries next launch, mirroring `purge_self_captures`. The merge is idempotent, so
/// re-running over already-merged frames is a harmless no-op-equivalent — only the still-per-word
/// stragglers actually change — until the pass finally completes without a hiccup.
async fn merge_purged_spans_once(store: Arc<SqliteStore>) {
    if matches!(
        store.get_setting(PURGED_SPANS_MERGED_KEY).await,
        Ok(Some(_))
    ) {
        return;
    }
    let mut cursor = 0_i64;
    let mut merged = 0_u64;
    let mut clean_drain = true;
    loop {
        let batch = match store.purged_frame_ids(cursor, 256).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "purged-span merge: list failed");
                clean_drain = false;
                break;
            }
        };
        if batch.is_empty() {
            break; // drained the backlog; `clean_drain` still reflects per-frame outcomes
        }
        for id in batch {
            cursor = id; // advance past every listed frame so the scan always terminates
            if let Err(e) = store.merge_frame_spans_to_lines(id).await {
                tracing::warn!(frame_id = id, error = %e, "purged-span merge: frame failed (kept per-word)");
                // A per-frame failure (not just a list failure) withholds the watermark so this
                // frame is retried next launch instead of being permanently skipped (`07` #73a).
                clean_drain = false;
                continue;
            }
            merged += 1;
        }
    }
    // Only watermark a fully clean drain: a list *or* per-frame failure retries the whole
    // (idempotent) backfill next launch rather than stranding a straggler's per-word rows.
    if clean_drain {
        if let Err(e) = store.set_setting(PURGED_SPANS_MERGED_KEY, "1").await {
            tracing::warn!(error = %e, "purged-span merge: could not record watermark");
        }
    }
    if merged > 0 {
        tracing::info!(
            merged,
            "merged per-word spans to per-line for purged frames (`07` #73a)"
        );
    }
}

/// One-time purge of the app's own previously-captured frames (PR3 audit,
/// `docs/AUDIT_0.2.0_PR3_2026-06-26.md`). Before the self-window capture skip existed,
/// foreground frames of the ScreenSearch window itself were stored; they hold only app
/// chrome (sidebar nav, command palette, a results pane echoing other captures) and
/// dominate `Deck`/`Recall` default search. Gated by [`SELF_PURGE_KEY`] so it runs once;
/// a clean-DB install finds none and no-ops. Identifies own frames by the running exe's
/// file stem (the same value the capture path recorded as `app_hint`). Deletes each
/// frame's JPEG then its row (CASCADE clears frame_text / text_spans / embeddings /
/// jobs). Batched, and bails if a batch makes no progress so a persistently failing
/// delete can't spin forever — and the [`SELF_PURGE_KEY`] watermark is recorded only on a
/// full drain, so a transient failure that leaves frames behind retries next launch
/// instead of marking the purge permanently complete.
async fn purge_self_captures(store: Arc<SqliteStore>, data_dir: PathBuf) {
    if matches!(store.get_setting(SELF_PURGE_KEY).await, Ok(Some(_))) {
        return;
    }
    let Some(own_hint) = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .filter(|s| !s.is_empty())
    else {
        tracing::warn!("self-capture purge skipped: could not resolve own exe name");
        return;
    };
    let mut purged = 0_u64;
    // Only watermark the purge as done once every own-window frame is actually gone. A
    // transient delete failure (Windows sharing violation, a DB error) trips the
    // no-progress break with frames still present — leaving the watermark unset so the
    // next launch retries, instead of permanently skipping the purge and leaving that
    // chrome searchable.
    let mut fully_drained = false;
    loop {
        let batch = match store.frames_with_app_hint(&own_hint, 256).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "self-capture purge: list failed");
                break;
            }
        };
        if batch.is_empty() {
            fully_drained = true;
            break;
        }
        let before = purged;
        for frame in batch {
            if let Some(path) = safe_frame_path(&data_dir, &frame.image_path) {
                if let Err(e) = std::fs::remove_file(&path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        // Skip the row delete so the JPEG isn't orphaned on a transient
                        // failure (e.g. an AV/indexer sharing violation), mirroring the
                        // retention sweeper. The no-progress guard below stops the loop
                        // if every delete in a batch keeps failing; the row stays linked
                        // to its file and the backfill still chrome-cleans its text.
                        tracing::warn!(path = %path.display(), error = %e, "self-capture purge: file delete failed");
                        continue;
                    }
                }
            }
            if let Err(e) = store.delete_frame(frame.frame_id).await {
                tracing::error!(frame_id = frame.frame_id, error = %e, "self-capture purge: db delete failed");
                continue;
            }
            purged += 1;
        }
        if purged == before {
            // No progress this batch (every delete failed) — stop rather than spin.
            break;
        }
    }
    if fully_drained {
        if let Err(e) = store.set_setting(SELF_PURGE_KEY, "1").await {
            tracing::warn!(error = %e, "self-capture purge: could not record watermark");
        }
    }
    if purged > 0 {
        tracing::info!(purged, app = %own_hint, "purged own-window captures (PR3 self-exclude)");
    }
}

fn safe_frame_path(data_dir: &Path, image_path: &str) -> Option<PathBuf> {
    use std::path::Component;
    let rel = Path::new(image_path);
    if rel.is_absolute() {
        return None;
    }
    let mut components = rel.components();
    match components.next()? {
        Component::Normal(first) if first == "frames" => {}
        _ => return None,
    }
    let mut safe = PathBuf::from("frames");
    for component in components {
        match component {
            Component::Normal(part) => safe.push(part),
            _ => return None,
        }
    }
    Some(data_dir.join(safe))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Enables/disables launch-at-login via `tauri-plugin-autostart` (0.3.2 PR3, `03 §7d`).
/// Returns the plugin error as a string so `set_settings`' register-before-persist path can
/// abort the save on failure.
fn set_autostart(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

/// Reconciles the OS launch-at-login registration to the persisted `app.run_at_startup` at
/// boot (best-effort — a mismatch is corrected, any failure is logged, capture is
/// unaffected). Covers drift from a manual change or a prior save whose registration failed.
fn reconcile_autostart(app: &tauri::AppHandle, desired: bool) {
    let manager = app.autolaunch();
    match manager.is_enabled() {
        Ok(current) if current == desired => {}
        Ok(_) => {
            let result = if desired {
                manager.enable()
            } else {
                manager.disable()
            };
            if let Err(e) = result {
                tracing::warn!(error = %e, desired, "autostart reconcile failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "autostart state probe failed"),
    }
}

/// Spawns the text recognizer and records an unavailable reason if OCR cannot be
/// initialized (capture start is blocked in that state). The recognizer is a composite:
/// **UI Automation primary, OCR fallback** (`docs/0.2.0.md` #48). OCR is the mandatory
/// floor, so if WinRT OCR is unavailable we return it alone (capture stays blocked) and do
/// not attempt UIA. Otherwise we read the UIA settings, seed the shared enable flag, and —
/// if `UiaTextProvider::spawn` succeeds (its own capability probe) — wrap UIA over OCR.
/// On any UIA spawn failure, fall back to OCR-only. The capture loop is unchanged: it still
/// calls a single `Arc<dyn OcrProvider>`.
fn spawn_ocr(
    store: &Arc<SqliteStore>,
) -> (
    Arc<dyn OcrProvider>,
    Option<Arc<UiaWithOcrFallback>>,
    Option<String>,
) {
    let (ocr, ocr_unavailable): (Arc<dyn OcrProvider>, Option<String>) =
        match ocr::WinRtOcr::spawn() {
            Ok(engine) => {
                tracing::info!("WinRT OCR ready");
                (Arc::new(engine), None)
            }
            Err(e) => {
                let reason = e.to_string();
                tracing::error!(error = %reason, "OCR unavailable; capture cannot start");
                (
                    Arc::new(UnavailableOcr {
                        reason: reason.clone(),
                    }),
                    Some(reason),
                )
            }
        };

    // No OCR floor → capture is blocked regardless; skip UIA entirely.
    if ocr_unavailable.is_some() {
        return (ocr, None, ocr_unavailable);
    }

    // Read the UIA settings (sync setup context → block on the async load). Every UIA knob now
    // hot-applies: the composite tears down + lazily respawns its UI Automation client on a
    // settings save, so nothing is "baked at spawn" anymore. The worker itself is spawned
    // lazily on the first eligible frame (its `CoCreateInstance` is the capability probe); a
    // session where UIA can't initialize just runs OCR-only from then on.
    let settings = kernel::settings::sanitize_settings(tauri::async_runtime::block_on(
        kernel::settings::load_settings(store.as_ref()),
    ));
    let config = uia_runtime_config(&settings);
    tracing::info!(
        enabled = config.enabled,
        control_view = config.budget.control_view,
        suppress_during_input_ms = config.suppress_during_input_ms,
        "UI Automation text composite ready (lazy client; OCR fallback)"
    );
    let composite = Arc::new(UiaWithOcrFallback::new(ocr, config));
    let dyn_ocr: Arc<dyn OcrProvider> = composite.clone();
    (dyn_ocr, Some(composite), None)
}

/// All the UIA-composite knobs, snapshotted from [`Settings`]. Swapped atomically on a
/// settings save; a change tears the client down so the next frame respawns with the new
/// values (so every UIA setting hot-applies — no app restart).
#[derive(Clone, Copy, PartialEq, Eq)]
struct UiaRuntimeConfig {
    enabled: bool,
    budget: uia::UiaBudget,
    suppress_during_input_ms: u32,
}

fn uia_runtime_config(s: &Settings) -> UiaRuntimeConfig {
    UiaRuntimeConfig {
        enabled: s.capture_uia_text_enabled,
        budget: uia::UiaBudget {
            latency_ms: u64::from(s.capture_uia_latency_budget_ms),
            min_text_chars: s.capture_uia_min_text_chars as usize,
            max_nodes: s.capture_uia_max_nodes,
            max_textpattern_calls: s.capture_uia_max_textpattern_calls,
            control_view: s.capture_uia_view_control_only,
        },
        suppress_during_input_ms: s.capture_uia_suppress_during_input_ms,
    }
}

/// Logs a breaker open/close exactly once per transition (the breaker returns `Some` only on
/// a state change), so a struggling app shows up in the log without per-frame spam.
fn log_breaker_transition(app: &str, transition: uia::breaker::BreakerTransition) {
    match transition {
        uia::breaker::BreakerTransition::Opened => tracing::warn!(
            app = app,
            cooldown_mins = uia::breaker::BREAKER_COOLDOWN_MS / 60_000,
            "UIA circuit breaker opened for app; routing it to OCR during cooldown \
             (repeated over-budget / timed-out walks — target app was struggling)"
        ),
        uia::breaker::BreakerTransition::Closed => tracing::info!(
            app = app,
            "UIA circuit breaker closed for app; UI Automation re-enabled"
        ),
    }
}

/// Composite [`OcrProvider`]: UI Automation primary, OCR fallback (`docs/0.2.0.md` #48).
/// It lives in the composition root because it is the one place allowed to wire two concrete
/// impls (`03 §2`).
///
/// Per frame: when UIA is enabled, the trigger runs UIA, the input gate is open, and the
/// per-app circuit breaker for this frame's app is closed, walk the tree (lazily spawning the
/// client on first use); on any `Err` (error / latency-timeout / thin yield) fall back to OCR.
/// Otherwise go straight to OCR.
///
/// Lifecycle (the fix for "disabling capture doesn't stop the Chromium hangs"): the UI
/// Automation client is held in a slot that [`teardown`](Self::teardown) empties — dropping our
/// `Arc` closes the worker's channel, which exits the "uia-mta" thread and releases the client
/// so Chromium/Electron apps leave accessibility mode. Teardown fires when UIA is disabled or
/// its settings change ([`apply_settings`](Self::apply_settings)) and when capture stops
/// ([`OcrProvider::on_capture_stopped`]); the next eligible frame lazily respawns.
struct UiaWithOcrFallback {
    ocr: Arc<dyn OcrProvider>,
    /// `Some` while the UI Automation client is connected; `None` after teardown. Lazily
    /// (re)spawned on the next eligible frame. `RwLock` because reads (the hot path) dominate.
    uia: std::sync::RwLock<Option<Arc<uia::UiaTextProvider>>>,
    /// All UIA knobs; hot-swapped by `apply_settings`.
    config: StdMutex<UiaRuntimeConfig>,
    /// Serializes lazy respawn so a burst of frames after teardown spawns exactly one client.
    respawn_gate: tokio::sync::Mutex<()>,
    /// Latched after a spawn failure so we don't retry every frame; cleared on `apply_settings`.
    spawn_failed: std::sync::atomic::AtomicBool,
    /// Per-app circuit breaker: repeated over-budget/timed-out walks against one app route it
    /// to OCR for a cooldown, so a heavy *native* app isn't re-hammered every frame. Chromium/
    /// Electron windows never reach it — they're skipped to OCR up front (`07` #93) — so it now
    /// only backstops native apps and any browser whose class lookup failed.
    breaker: StdMutex<uia::breaker::AppBreaker>,
    /// `app_hint`s for which we've already logged the "Chromium → OCR" skip, so the info line is
    /// emitted once per app rather than once per frame (`07` #93).
    chromium_skip_logged: StdMutex<HashSet<String>>,
}

impl UiaWithOcrFallback {
    fn new(ocr: Arc<dyn OcrProvider>, config: UiaRuntimeConfig) -> Self {
        Self {
            ocr,
            uia: std::sync::RwLock::new(None),
            config: StdMutex::new(config),
            respawn_gate: tokio::sync::Mutex::new(()),
            spawn_failed: std::sync::atomic::AtomicBool::new(false),
            breaker: StdMutex::new(uia::breaker::AppBreaker::new(
                uia::breaker::BREAKER_CONSECUTIVE_BAD,
                uia::breaker::BREAKER_COOLDOWN_MS,
            )),
            chromium_skip_logged: StdMutex::new(HashSet::new()),
        }
    }

    /// Whether the input-activity gate routes this frame to OCR instead of a UIA walk
    /// (`07` #71). The scroll/click trigger gate is inert in the default timer-only capture
    /// path (every frame is a `Timer`), so this reads the live system idle time and skips the
    /// walk for `Timer` frames captured while the user is actively interacting. If the OS
    /// can't report idle time the gate stays open (unknown activity must not silently disable
    /// UIA). Cheap (the decision short-circuits before the syscall when the window is `0`).
    fn input_gate_skips(&self, trigger: traits::CaptureTrigger, cfg: &UiaRuntimeConfig) -> bool {
        if cfg.suppress_during_input_ms == 0 {
            return false;
        }
        match uia::input::ms_since_last_input() {
            Some(ms) => {
                uia::classify::input_gate_skips_uia(trigger, ms, cfg.suppress_during_input_ms)
            }
            None => false,
        }
    }

    /// Returns the live UIA client, lazily spawning it if the slot is empty (and a prior spawn
    /// hasn't failed). The blocking `spawn` (a COM `CoCreateInstance` + ready handshake, ~ms)
    /// runs on `spawn_blocking` off the executor, serialized by `respawn_gate` so a burst of
    /// frames after teardown creates exactly one client.
    async fn get_or_spawn_uia(&self) -> Option<Arc<uia::UiaTextProvider>> {
        let existing = self.uia.read().expect("uia slot lock").clone();
        if let Some(p) = existing {
            return Some(p);
        }
        if self.spawn_failed.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        let _gate = self.respawn_gate.lock().await;
        // Re-check under the gate: another frame may have spawned it while we waited.
        let existing = self.uia.read().expect("uia slot lock").clone();
        if let Some(p) = existing {
            return Some(p);
        }
        if self.spawn_failed.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        // Read the budget fresh under the gate rather than trusting the caller's snapshot: a
        // settings save can land between `recognize` snapshotting its `cfg` and this spawn, and
        // while the slot is still empty that save's teardown is a no-op — so the stale snapshot
        // would otherwise get baked permanently into the worker. Reading the live config here
        // keeps "every UIA setting hot-applies" true across the capture-start → first-spawn
        // window. (A second save racing this exact in-progress spawn self-heals on the next save.)
        let budget = self.config.lock().expect("uia config lock").budget;
        match tokio::task::spawn_blocking(move || uia::UiaTextProvider::spawn(budget)).await {
            Ok(Ok(provider)) => {
                let arc = Arc::new(provider);
                *self.uia.write().expect("uia slot lock") = Some(arc.clone());
                tracing::info!(
                    budget_ms = budget.latency_ms,
                    control_view = budget.control_view,
                    "UI Automation client connected (OCR fallback)"
                );
                Some(arc)
            }
            Ok(Err(e)) => {
                self.spawn_failed
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!(error = %e, "UI Automation unavailable; using OCR only until settings change");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "UIA spawn task panicked; using OCR only for now");
                None
            }
        }
    }

    /// Disconnect the UI Automation client now (idempotent). Flags the worker to abandon a
    /// wedged walk at its next checkpoint, drops our `Arc` (closing the channel → the
    /// "uia-mta" thread exits → `CoUninitialize`), and logs the exit **without joining** — a
    /// wedged walk ends on its own (bounded by the worker's per-call COM timeouts) and must
    /// not block the caller. An in-flight `recognize` on another task holds its own `Arc`
    /// clone, so the channel actually closes ≤ one hard-timeout later — bounded and fine.
    fn teardown(&self, reason: &'static str) {
        let provider = self.uia.write().expect("uia slot lock").take();
        if let Some(provider) = provider {
            provider.shutdown();
            let exit_rx = provider.take_exit_signal();
            drop(provider); // release our Arc; the worker exits when the last clone drops
            if let Some(exit_rx) = exit_rx {
                std::thread::spawn(move || {
                    match exit_rx.recv_timeout(std::time::Duration::from_secs(5)) {
                        // Sender dropped = worker returned = client released. (No value is
                        // ever sent, so `Ok` can't happen, but treat it as exited too.)
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) | Ok(()) => {
                            tracing::info!(
                                reason,
                                "UIA worker exited; UI Automation client released"
                            );
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            tracing::warn!(
                                reason,
                                "UIA worker still finishing a walk 5s after teardown; detached (it exits when the walk ends)"
                            );
                        }
                    }
                });
            }
        }
    }

    /// Swap in fresh UIA settings and, if anything changed, tear the client down so the next
    /// frame respawns with the new values — this is what makes every UIA knob hot-apply. A
    /// no-op when the UIA config is unchanged, so unrelated settings saves don't churn the
    /// client. Called from `set_settings`.
    fn apply_settings(&self, settings: &Settings) {
        let cfg = uia_runtime_config(settings);
        let changed = {
            let mut guard = self.config.lock().expect("uia config lock");
            if *guard == cfg {
                false
            } else {
                *guard = cfg;
                true
            }
        };
        if changed {
            // Clear a prior spawn-failure latch so the fresh config gets a real attempt.
            self.spawn_failed
                .store(false, std::sync::atomic::Ordering::Relaxed);
            self.teardown("uia settings changed");
        }
    }
}

#[async_trait]
impl OcrProvider for UiaWithOcrFallback {
    async fn recognize(&self, frame: &CapturedFrame) -> traits::Result<OcrResult> {
        let cfg = *self.config.lock().expect("uia config lock");
        // Gate UIA off when disabled, off high-frequency interactive triggers (scroll/click),
        // and while the user is actively typing (`07` #71) — each walk is synchronous
        // cross-process COM against the foreground app, and on a large Chromium/Electron
        // a11y tree that freezes the target's UI thread ("Not responding").
        let run_uia = cfg.enabled
            && uia::classify::trigger_runs_uia(frame.trigger)
            && !self.input_gate_skips(frame.trigger, &cfg);
        if run_uia {
            // Chromium/Electron windows are never walked: the first UIA touch hangs their UI
            // thread and can't be aborted mid-call (`07` #93). Skip straight to OCR here, before
            // spawning/dispatching a walk, whenever capture recorded the foreground handle (the
            // common case). The worker keeps its own `Chrome_WidgetWin_*` backstop for the frames
            // where `foreground_hwnd` is `None` or focus moved between capture and recognize.
            if let Some(hwnd) = frame.foreground_hwnd {
                if uia::hwnd_is_chromium(hwnd) {
                    if let Some(app) = frame.app_hint.as_deref() {
                        // Log once per app, not once per frame — browsers are foreground often.
                        if self
                            .chromium_skip_logged
                            .lock()
                            .expect("chromium skip-log lock")
                            .insert(app.to_string())
                        {
                            tracing::info!(
                                app,
                                "UIA disabled for Chromium/Electron app; using OCR (07 #93)"
                            );
                        }
                    }
                    // A skip is neither a good nor a bad walk, so it feeds nothing to the breaker.
                    return self.ocr.recognize(frame).await;
                }
            }
            // `now_ms` is an i64 epoch-ms; the breaker only does relative comparisons, so a
            // non-negative `u64` is all it needs (clamp guards a pre-1970 clock).
            let now = now_ms().max(0) as u64;
            // Key the breaker on the frame's foreground app; `None` app can't be attributed,
            // so it bypasses the breaker (it still runs UIA, just uncounted).
            let app_key = frame.app_hint.as_deref().map(|s| s.to_ascii_lowercase());
            let breaker_open = app_key
                .as_deref()
                .is_some_and(|k| self.breaker.lock().expect("breaker lock").is_open(k, now));
            if !breaker_open {
                if let Some(provider) = self.get_or_spawn_uia().await {
                    let outcome = provider.recognize_detailed(frame).await;
                    if let Some(k) = app_key.as_deref() {
                        let signal = uia::breaker::signal(outcome.end);
                        let transition = self
                            .breaker
                            .lock()
                            .expect("breaker lock")
                            .record(k, signal, now);
                        if let Some(t) = transition {
                            log_breaker_transition(k, t);
                        }
                    }
                    match outcome.result {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            tracing::debug!(error = %e, "UIA recognize failed; falling back to OCR")
                        }
                    }
                }
            }
        }
        self.ocr.recognize(frame).await
    }

    fn on_capture_stopped(&self) {
        // Capture stopped: release the UI Automation client so Chromium/Electron apps leave
        // accessibility mode instead of staying degraded. The next capture start respawns it.
        self.teardown("capture stopped");
    }
}

/// Builds a WGC capture source from the current config — the seam that keeps the
/// kernel impl-agnostic (`03 §2`).
fn capture_factory() -> CaptureFactory {
    Arc::new(|config, capture_now_rx| {
        Ok(Box::new(capture::WgcCapture::new(config, capture_now_rx)?) as Box<dyn CaptureSource>)
    })
}

/// Defensive fallback used only when the WinRT engine can't be created. The kernel
/// refuses to start capture before this provider can be called; if that invariant is
/// broken, returning an error still prevents silently writing empty OCR rows.
struct UnavailableOcr {
    reason: String,
}

#[async_trait]
impl OcrProvider for UnavailableOcr {
    async fn recognize(&self, _frame: &CapturedFrame) -> traits::Result<OcrResult> {
        Err(std::io::Error::other(format!("OCR unavailable: {}", self.reason)).into())
    }
}

/// Opens the store and returns it with the corresponding `db` readiness. A DB error
/// at either step (open, or the schema-version probe that confirms the connection is
/// usable) surfaces as `Error` with no store — never a Ready store the UI can't query.
fn open_store(db_path: &Path) -> (Option<Arc<SqliteStore>>, ComponentReadiness) {
    let result = SqliteStore::open_path(db_path).and_then(|s| {
        let version = s.schema_version()?;
        Ok((s, version))
    });
    match result {
        Ok((s, version)) => {
            let detail = format!("schema v{version} ({})", db_path.display());
            tracing::info!(db = %db_path.display(), schema_version = version, "store opened");
            (
                Some(Arc::new(s)),
                ComponentReadiness::with_detail(ComponentStatus::Ready, detail),
            )
        }
        Err(e) => {
            tracing::error!(error = %e, db = %db_path.display(), "store unavailable");
            (
                None,
                ComponentReadiness::with_detail(ComponentStatus::Error, e.to_string()),
            )
        }
    }
}

/// Keeps the non-blocking file appender's worker alive for the process lifetime;
/// dropping the guard would stop file logging.
static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Console + daily-rotating file logging (`03 §9`). Privacy: callers must not log
/// screen content or OCR text at info level.
fn init_tracing(log_dir: &Path) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let file_appender = tracing_appender::rolling::daily(log_dir, "screensearch.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .with(fmt::layer().with_ansi(false).with_writer(non_blocking))
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wiring proof: opening the store at a real path creates the DB file on disk
    /// and reports `db = Ready` (headless P1 guarantee, still valid in P2).
    #[test]
    fn open_store_creates_db_file_and_reports_ready() {
        let dir = std::env::temp_dir().join(format!("ssv2c-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("screensearch.db");
        let _ = std::fs::remove_file(&db);

        let (store, readiness) = open_store(&db);

        assert!(store.is_some(), "store handle present");
        assert_eq!(readiness.status, ComponentStatus::Ready);
        assert!(db.exists(), "db file created at {}", db.display());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A DB that cannot be opened surfaces as `db = Error` instead of crashing.
    #[test]
    fn open_store_reports_error_when_db_cannot_open() {
        let db = std::env::temp_dir()
            .join(format!("ssv2c-missing-{}", std::process::id()))
            .join("nope")
            .join("screensearch.db");

        let (store, readiness) = open_store(&db);

        assert!(store.is_none());
        assert_eq!(readiness.status, ComponentStatus::Error);
    }

    #[test]
    fn ipc_presentation_limits_are_clamped() {
        assert_eq!(clamp_frame_list_limit(u32::MAX), MAX_FRAME_LIST_LIMIT);
        assert_eq!(
            clamp_frame_context_limit(u32::MAX),
            MAX_FRAME_CONTEXT_LIMIT_EACH
        );
        assert_eq!(clamp_timeline_buckets(1), MIN_TIMELINE_BUCKETS);
        assert_eq!(clamp_timeline_buckets(u32::MAX), MAX_TIMELINE_BUCKETS);
        assert_eq!(clamp_insights_buckets(1), MIN_INSIGHTS_BUCKETS);
        assert_eq!(clamp_insights_buckets(u32::MAX), MAX_INSIGHTS_BUCKETS);
    }

    #[test]
    fn session_query_normalizes_time_kind_tool_and_limit() {
        let filter = normalize_session_query(
            traits::SessionQuery {
                time_range: Some(TimeRange {
                    start: 100,
                    end: 200,
                }),
                kind: Some(traits::SessionKind::Ai),
                tool: Some("  codex  ".to_string()),
                limit: Some(u32::MAX),
            },
            999,
        );
        assert_eq!(filter.from_ms, Some(100));
        assert_eq!(filter.to_ms, Some(200));
        assert_eq!(filter.kind, Some(traits::SessionKind::Ai));
        assert_eq!(filter.tool.as_deref(), Some("codex"));
        assert_eq!(filter.limit, Some(1000));
        assert_eq!(filter.now_ms, 999);

        let defaults = normalize_session_query(
            traits::SessionQuery {
                time_range: None,
                kind: None,
                tool: Some("   ".to_string()),
                limit: Some(0),
            },
            123,
        );
        assert_eq!(defaults.tool, None);
        assert_eq!(defaults.limit, Some(1));

        let omitted = normalize_session_query(
            traits::SessionQuery {
                time_range: None,
                kind: None,
                tool: None,
                limit: None,
            },
            321,
        );
        assert_eq!(omitted.limit, Some(1000));
    }

    #[tokio::test]
    async fn session_detail_samples_24_frames_and_returns_only_exchanges_without_inference() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let session_id = store
            .insert_session(traits::NewSession {
                started_at: 0,
                ended_at: Some(3_000),
                kind: traits::SessionKind::Ai,
                tool: Some("codex".to_string()),
                host: Some(traits::SessionHost::Desktop),
                context_key: "ai:codex".to_string(),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .unwrap();
        let mut frame_ids = Vec::new();
        for index in 0..30_i64 {
            let frame_id = store
                .insert_frame(traits::NewFrame {
                    captured_at: index * 100,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{index}.webp"),
                    content_hash: format!("hash-{index}"),
                    app_hint: Some("codex".to_string()),
                    window_title: Some("Codex".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
            frame_ids.push(frame_id);
        }
        store
            .assign_frames_session(&frame_ids, Some(session_id))
            .await
            .unwrap();
        store
            .insert_session_artifacts(
                session_id,
                &[
                    traits::NewSessionArtifact {
                        kind: traits::SessionArtifactKind::Exchange,
                        role: Some(traits::SessionArtifactRole::User),
                        frame_id: Some(frame_ids[0]),
                        content: "please fix it".to_string(),
                    },
                    traits::NewSessionArtifact {
                        kind: traits::SessionArtifactKind::Note,
                        role: None,
                        frame_id: Some(frame_ids[1]),
                        content: "reserved note".to_string(),
                    },
                ],
            )
            .await
            .unwrap();

        let detail = load_session_detail(store, session_id, false, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.frame_count, 30);
        assert_eq!(detail.frames.len(), 24);
        assert_eq!(
            detail.frames.first().map(|frame| frame.frame_id),
            frame_ids.first().copied()
        );
        assert_eq!(
            detail.frames.last().map(|frame| frame.frame_id),
            frame_ids.last().copied()
        );
        assert_eq!(detail.exchanges.len(), 1);
        assert_eq!(
            detail.exchanges[0].kind,
            traits::SessionArtifactKind::Exchange
        );
    }

    #[tokio::test]
    async fn session_detail_redacts_invalid_cached_summary_before_regeneration() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let session_id = store
            .insert_session(traits::NewSession {
                started_at: 0,
                ended_at: Some(1),
                kind: traits::SessionKind::Ai,
                tool: Some("codex".to_string()),
                host: Some(traits::SessionHost::Desktop),
                context_key: "ai:codex".to_string(),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .unwrap();
        store
            .set_session_title_summary(session_id, "Speaker", "User", "bad-model.gguf")
            .await
            .unwrap();

        let base = load_session_detail(store.clone(), session_id, false, None)
            .await
            .unwrap()
            .unwrap();
        assert!(
            base.session.title.is_none()
                && base.session.summary.is_none()
                && base.session.summary_model.is_none(),
            "base detail responses must redact the invalid cache while summary generation runs"
        );

        let error = load_session_detail(store.clone(), session_id, true, None)
            .await
            .expect_err("the invalid summary must reach the generator, not the cached response");
        assert_eq!(error, "inference sidecar not ready yet");
        let persisted = store.get_session(session_id).await.unwrap().unwrap();
        assert!(
            persisted.title.is_none()
                && persisted.summary.is_none()
                && persisted.summary_model.is_none(),
            "an include-summary request must durably clear an invalid cache before provider readiness"
        );
    }

    #[tokio::test]
    async fn ui_resume_context_hydrates_session_without_changing_external_resume_shape() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let mut ids = Vec::new();
        for (at, app) in [
            (0, "Code"),
            (120_000, "Code"),
            (200_000, "Firefox"),
            (320_000, "Firefox"),
        ] {
            ids.push(
                store
                    .insert_frame(traits::NewFrame {
                        captured_at: at,
                        monitor_index: 0,
                        width: 1920,
                        height: 1080,
                        image_path: format!("frames/{at}.webp"),
                        content_hash: format!("resume-{at}"),
                        app_hint: Some(app.to_string()),
                        window_title: Some(app.to_string()),
                        browser_url: None,
                        capture_trigger: None,
                    })
                    .await
                    .unwrap(),
            );
        }
        let session_id = store
            .insert_session(traits::NewSession {
                started_at: 0,
                ended_at: Some(120_000),
                kind: traits::SessionKind::Focus,
                tool: None,
                host: Some(traits::SessionHost::Ide),
                context_key: "focus:code".to_string(),
                confidence: 0.75,
                frozen: false,
            })
            .await
            .unwrap();
        store
            .assign_frames_session(&ids[..2], Some(session_id))
            .await
            .unwrap();

        let joined = load_ui_resume_context(store.as_ref(), &Settings::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(joined.context.frame_id, ids[1]);
        assert_eq!(
            joined.session.as_ref().map(|session| session.id),
            Some(session_id)
        );

        assert!(store.delete_unfrozen_session(session_id).await.unwrap());
        let deleted = load_ui_resume_context(store.as_ref(), &Settings::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(deleted.context.frame_id, ids[1]);
        assert!(deleted.session.is_none());
    }

    #[tokio::test]
    async fn session_recap_evidence_probe_rejects_missing_text_before_provider_acquisition() {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let session_id = store
            .insert_session(traits::NewSession {
                started_at: 0,
                ended_at: Some(1_000),
                kind: traits::SessionKind::Focus,
                tool: None,
                host: Some(traits::SessionHost::Ide),
                context_key: "focus:code".to_string(),
                confidence: 0.7,
                frozen: false,
            })
            .await
            .unwrap();
        let frame_id = store
            .insert_frame(traits::NewFrame {
                captured_at: 100,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: "frames/100.webp".to_string(),
                content_hash: "hash-100".to_string(),
                app_hint: Some("Code".to_string()),
                window_title: Some("Code".to_string()),
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .unwrap();
        store
            .assign_frames_session(&[frame_id], Some(session_id))
            .await
            .unwrap();

        assert!(!session_recap_has_evidence(store.as_ref(), session_id)
            .await
            .unwrap());
        store
            .insert_ocr(
                frame_id,
                traits::OcrResult {
                    text: "\t\n \r".to_string(),
                    mean_confidence: 0.9,
                    engine: "test".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .unwrap();
        assert!(
            !session_recap_has_evidence(store.as_ref(), session_id)
                .await
                .unwrap(),
            "Rust str::trim whitespace must not acquire the answer provider"
        );
        store
            .insert_ocr(
                frame_id,
                traits::OcrResult {
                    text: "usable filtered content".to_string(),
                    mean_confidence: 0.9,
                    engine: "test".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .unwrap();
        assert!(session_recap_has_evidence(store.as_ref(), session_id)
            .await
            .unwrap());
    }

    #[test]
    fn safe_frame_path_accepts_only_relative_frames_children() {
        let data = PathBuf::from(r"C:\appdata");

        assert_eq!(
            safe_frame_path(&data, "frames/day-1/100.jpg"),
            Some(data.join("frames").join("day-1").join("100.jpg"))
        );
        assert!(safe_frame_path(&data, r"C:\outside\100.jpg").is_none());
        assert!(safe_frame_path(&data, "../frames/100.jpg").is_none());
        assert!(safe_frame_path(&data, "screens/100.jpg").is_none());
        assert!(safe_frame_path(&data, "frames/../100.jpg").is_none());
    }

    /// `07` #73a one-time backfill: it merges every purged frame's per-word spans to per-line,
    /// records the completion watermark on a clean drain, and short-circuits on re-run. (A fresh
    /// store with no purged frames still watermarks — a clean, empty drain.)
    #[tokio::test]
    async fn merge_purged_spans_once_merges_backlog_then_watermarks_and_is_idempotent() {
        fn word(text: &str, x: f32) -> traits::TextSpan {
            traits::TextSpan {
                normalized_text: traits::normalize_text(text),
                text: text.to_string(),
                source: traits::TextSource::Ocr,
                role: traits::TextRole::Unknown,
                x,
                y: 0.0,
                w: 0.1,
                h: 0.05,
                line_index: 0,
                is_searchable: true,
                suppress_reason: None,
            }
        }

        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let mut ids = Vec::new();
        for at in [1_000_i64, 2_000] {
            let id = store
                .insert_frame(traits::NewFrame {
                    captured_at: at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{at}.jpg"),
                    content_hash: format!("h{at}"),
                    app_hint: None,
                    window_title: None,
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
            store
                .insert_ocr(
                    id,
                    traits::OcrResult {
                        text: "hello world".to_string(),
                        mean_confidence: -1.0,
                        engine: "winrt".to_string(),
                        spans: vec![word("hello", 0.0), word("world", 0.2)],
                    },
                )
                .await
                .unwrap();
            store.purge_frame_image(id).await.unwrap();
            ids.push(id);
        }
        // Pre: two per-word spans per purged frame.
        for &id in &ids {
            assert_eq!(store.frame_spans(id).await.unwrap().len(), 2);
        }

        merge_purged_spans_once(store.clone()).await;

        // Post: one line span per frame; the clean drain recorded the watermark.
        for &id in &ids {
            assert_eq!(
                store.frame_spans(id).await.unwrap().len(),
                1,
                "merged to one line"
            );
        }
        assert_eq!(
            store
                .get_setting(PURGED_SPANS_MERGED_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some("1"),
            "clean drain sets the completion watermark"
        );

        // Idempotent: a second run short-circuits on the watermark and leaves the spans untouched.
        merge_purged_spans_once(store.clone()).await;
        for &id in &ids {
            assert_eq!(store.frame_spans(id).await.unwrap().len(), 1);
        }
    }

    #[test]
    fn parses_llama_cpp_device_ids() {
        let output = "
Available devices:
  Vulkan0: AMD Radeon
  Vulkan1: NVIDIA RTX
  CUDA0: NVIDIA RTX
";
        assert_eq!(
            parse_sidecar_device_ids(output),
            vec!["Vulkan0", "Vulkan1", "CUDA0"]
        );
    }

    #[test]
    fn hydrate_ask_context_returns_clear_error_when_ocr_texts_fails() {
        let hits = vec![SearchHit {
            frame_id: 42,
            captured_at: 1_234,
            snippet: "snippet fallback".to_string(),
            score: 0.75,
            image_path: "frames/42.webp".to_string(),
            image_purged: false,
            app_hint: Some("Editor".to_string()),
        }];

        let err = hydrate_ask_context(hits, 1, Err("simulated ocr_texts failure".to_string()))
            .unwrap_err();

        assert_eq!(
            err,
            "failed to hydrate Ask OCR context for 1 frames: simulated ocr_texts failure"
        );
    }

    #[test]
    fn sanitize_report_stem_produces_a_safe_leaf_name() {
        // The UI's own stem passes through unchanged.
        assert_eq!(
            sanitize_report_stem("screensearch-report-2026-07-04-1530"),
            "screensearch-report-2026-07-04-1530"
        );
        // Path separators / traversal / drive letters collapse to dashes — nothing that
        // could escape the target dir survives.
        assert_eq!(sanitize_report_stem("../../etc/passwd"), "etc-passwd");
        assert_eq!(
            sanitize_report_stem(r"C:\Windows\system32"),
            "C-Windows-system32"
        );
        // Leading/trailing junk is trimmed; an all-junk or empty stem falls back.
        assert_eq!(sanitize_report_stem("  !!  "), "screensearch-report");
        assert_eq!(sanitize_report_stem(""), "screensearch-report");
    }

    #[test]
    fn unique_markdown_path_appends_2_3_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let stem = "screensearch-report-2026-07-04-1530";

        let p1 = unique_markdown_path(dir.path(), stem).unwrap();
        assert_eq!(
            p1.file_name().unwrap().to_str().unwrap(),
            format!("{stem}.md").as_str()
        );
        std::fs::write(&p1, b"one").unwrap();

        let p2 = unique_markdown_path(dir.path(), stem).unwrap();
        assert_eq!(
            p2.file_name().unwrap().to_str().unwrap(),
            format!("{stem}-2.md").as_str()
        );
        std::fs::write(&p2, b"two").unwrap();

        let p3 = unique_markdown_path(dir.path(), stem).unwrap();
        assert_eq!(
            p3.file_name().unwrap().to_str().unwrap(),
            format!("{stem}-3.md").as_str()
        );
    }

    #[test]
    fn db_file_family_size_includes_wal_and_shm() {
        let dir = std::env::temp_dir().join(format!("ssv2c-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("screensearch.db");
        std::fs::write(&db, [0_u8; 3]).unwrap();
        std::fs::write(dir.join("screensearch.db-wal"), [0_u8; 5]).unwrap();
        std::fs::write(dir.join("screensearch.db-shm"), [0_u8; 7]).unwrap();

        assert_eq!(db_file_family_size(&db), 15);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
