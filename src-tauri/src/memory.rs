//! Durable daily memory orchestration. This composition root wires the shared
//! report service, local files and the optional outbound adapter to persisted runs.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::memory_files::{native_path, path_string};
use crate::{now_ms, AppState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use store::SqliteStore;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{Mutex, Notify};
use traits::*;
use uuid::Uuid;

const SCHEDULE_STATE: &str = "memory.schedule.state";
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;

pub struct MemoryRuntime {
    control: Mutex<()>,
    wake: Notify,
    stopping: AtomicBool,
    cancel: Arc<AtomicBool>,
    active: AtomicBool,
    inference_settled: AtomicBool,
    join: StdMutex<Option<tokio::task::JoinHandle<()>>>,
    last_error: StdMutex<Option<ReportError>>,
}
impl MemoryRuntime {
    pub fn new() -> Self {
        Self {
            control: Mutex::new(()),
            wake: Notify::new(),
            stopping: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            active: AtomicBool::new(false),
            inference_settled: AtomicBool::new(false),
            join: StdMutex::new(None),
            last_error: StdMutex::new(None),
        }
    }
    /// True once `init_inference` has finished wiring (or failing to wire) the
    /// inference slots at startup. Generation claims stay deferred until then so a
    /// transient `answer=None` at launch cannot persist a permanent BLOCKED run.
    pub fn inference_settled(&self) -> bool {
        self.inference_settled.load(Ordering::Acquire)
    }
    /// Called by the `init_inference` task after it completes — on success and
    /// failure alike — and wakes the worker so deferred claims proceed at once.
    pub fn mark_inference_settled(&self) {
        self.inference_settled.store(true, Ordering::Release);
        self.wake.notify_one();
    }
}

fn fail(code: &str, message: &str) -> ReportError {
    ReportError {
        code: code.into(),
        message: message.into(),
    }
}
fn storage(_: impl std::fmt::Display) -> ReportError {
    fail("STORE_ERROR", "Could not read or save memory report state.")
}
fn uuid() -> String {
    Uuid::new_v4().to_string()
}
fn notify(app: &AppHandle) {
    let _ = app.emit("memory_status_changed", ());
}
fn db(state: &AppState) -> Result<Arc<SqliteStore>, ReportError> {
    state
        .store
        .clone()
        .ok_or_else(|| fail("STORE_ERROR", "Database unavailable."))
}
fn check_open(runtime: &MemoryRuntime) -> Result<(), ReportError> {
    if runtime.stopping.load(Ordering::Acquire) {
        Err(fail("BUSY", "ScreenSearch is shutting down."))
    } else {
        Ok(())
    }
}
/// Startup race guard (`03 §8c`): a queued run that still needs generation must
/// not be claimed before inference initialization settles, or a transient
/// `answer=None` at launch would persist a permanent `MODEL_UNAVAILABLE` block.
/// Runs with saved output only need delivery rework and may proceed at any time.
fn claim_allowed(run: &ReportRun, inference_settled: bool) -> bool {
    run.result.is_some() || inference_settled
}

async fn setting<T: serde::de::DeserializeOwned + Default>(
    store: &dyn Store,
    key: &str,
) -> Result<T, ReportError> {
    match store.get_setting(key).await.map_err(storage)? {
        Some(value) => serde_json::from_str(&value).map_err(storage),
        None => Ok(T::default()),
    }
}
async fn settings(store: &dyn Store) -> Result<MemorySettings, ReportError> {
    Ok(MemorySettings {
        enabled: setting(store, "memory.schedule.enabled").await?,
        local_time: setting(store, "memory.schedule.local_time").await?,
        timezone: setting(store, "memory.schedule.timezone").await?,
        output_folder: setting::<Option<String>>(store, "memory.schedule.output_folder")
            .await?
            .map(|folder| path_string(Path::new(&folder))),
        telegram_enabled: setting(store, "memory.telegram.enabled").await?,
        telegram_chat_id: setting(store, "memory.telegram.chat_id").await?,
    })
}
fn validate(config: &MemorySettings, manual: bool) -> Result<(), ReportError> {
    if let Some(zone) = &config.timezone {
        if zone.parse::<chrono_tz::Tz>().is_err() {
            return Err(fail("INVALID_CONFIG", "Select a valid IANA time zone."));
        }
    }
    if let Some(time) = &config.local_time {
        kernel::memory_schedule::validate(config.timezone.as_deref().unwrap_or("UTC"), time)
            .map_err(|_| {
                fail(
                    "INVALID_CONFIG",
                    "Schedule time must be HH:mm in a valid time zone.",
                )
            })?;
    }
    if let Some(folder) = &config.output_folder {
        if !native_path(Path::new(folder)).is_absolute() {
            return Err(fail("INVALID_CONFIG", "Select an absolute output folder."));
        }
    }
    if (config.enabled || manual) && (config.timezone.is_none() || config.output_folder.is_none()) {
        return Err(fail(
            "INVALID_CONFIG",
            "Select a time zone and output folder first.",
        ));
    }
    if config.enabled && config.local_time.is_none() {
        return Err(fail("INVALID_CONFIG", "Select a daily report time."));
    }
    if config.telegram_enabled
        && config
            .telegram_chat_id
            .as_deref()
            .is_none_or(|id| id.trim().is_empty())
    {
        return Err(fail(
            "INVALID_CONFIG",
            "Select a Telegram chat before enabling delivery.",
        ));
    }
    Ok(())
}
async fn source(store: &dyn Store) -> Result<String, ReportError> {
    store
        .ensure_report_source_id(&uuid())
        .await
        .map_err(storage)
}
async fn status(state: &AppState) -> Result<MemoryStatus, ReportError> {
    let store = db(state)?;
    let mut worker_error = state
        .memory_runtime
        .last_error
        .lock()
        .expect("memory error")
        .clone();
    let config = settings(store.as_ref()).await?;
    let schedule: Option<ReportScheduleState> = setting(store.as_ref(), SCHEDULE_STATE).await?;
    let missed_windows = schedule
        .and_then(|s| s.gap_start.zip(s.gap_end))
        .map(|(start, end)| vec![TimeRange { start, end }])
        .unwrap_or_default();
    let source_id = source(store.as_ref()).await?;
    let telegram_credential_available = match crate::memory_telegram::credential_read(&source_id) {
        Ok(token) => token.is_some(),
        Err(_) => {
            worker_error = Some(fail(
                "CREDENTIAL_UNAVAILABLE",
                "Windows Credential Manager is unavailable.",
            ));
            false
        }
    };
    Ok(MemoryStatus {
        settings: config,
        source_id: source_id.clone(),
        latest_run: store
            .list_report_runs(1, None)
            .await
            .map_err(storage)?
            .into_iter()
            .next(),
        missed_windows,
        worker_running: state.memory_runtime.active.load(Ordering::Acquire),
        telegram_credential_available,
        worker_error,
    })
}

#[tauri::command]
pub async fn get_memory_status(state: State<'_, AppState>) -> Result<MemoryStatus, ReportError> {
    status(&state).await
}

#[tauri::command]
pub async fn preview_memory_schedule(
    settings: MemorySettings,
) -> Result<Option<MemorySchedulePreview>, ReportError> {
    validate(&settings, false)?;
    let (Some(zone), Some(time)) = (settings.timezone.as_deref(), settings.local_time.as_deref())
    else {
        return Ok(None);
    };
    let window = kernel::memory_schedule::first_window(zone, time, now_ms())
        .map_err(|_| fail("INVALID_CONFIG", "The schedule date could not be resolved."))?;
    Ok(Some(MemorySchedulePreview {
        local_date: window.local_date,
        window_start: window.start,
        window_end: window.end,
        due_at: window.due_at,
    }))
}

#[tauri::command]
pub async fn set_memory_settings(
    mut settings: MemorySettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryStatus, ReportError> {
    settings.output_folder = settings
        .output_folder
        .map(|folder| path_string(Path::new(&folder)));
    validate(&settings, false)?;
    let store = db(&state)?;
    let runtime = &state.memory_runtime;
    let _guard = runtime.control.lock().await;
    check_open(runtime)?;
    let old = self::settings(store.as_ref()).await?;
    if settings.telegram_enabled {
        let profile = source(store.as_ref()).await?;
        if crate::memory_telegram::credential_read(&profile)
            .map_err(|_| {
                fail(
                    "CREDENTIAL_UNAVAILABLE",
                    "Windows Credential Manager is unavailable.",
                )
            })?
            .is_none()
        {
            return Err(fail(
                "CREDENTIAL_UNAVAILABLE",
                "Save a Telegram bot token before enabling delivery.",
            ));
        }
    }
    let mut values = vec![
        (
            "memory.schedule.enabled".into(),
            serde_json::to_string(&settings.enabled).map_err(storage)?,
        ),
        (
            "memory.schedule.local_time".into(),
            serde_json::to_string(&settings.local_time).map_err(storage)?,
        ),
        (
            "memory.schedule.timezone".into(),
            serde_json::to_string(&settings.timezone).map_err(storage)?,
        ),
        (
            "memory.schedule.output_folder".into(),
            serde_json::to_string(&settings.output_folder).map_err(storage)?,
        ),
        (
            "memory.telegram.enabled".into(),
            serde_json::to_string(&settings.telegram_enabled).map_err(storage)?,
        ),
        (
            "memory.telegram.chat_id".into(),
            serde_json::to_string(&settings.telegram_chat_id).map_err(storage)?,
        ),
    ];
    if settings.enabled
        && (!old.enabled
            || old.local_time != settings.local_time
            || old.timezone != settings.timezone)
    {
        let schedule = ReportScheduleState {
            schedule_id: uuid(),
            activated_at: now_ms(),
            last_window_end: None,
            gap_start: None,
            gap_end: None,
        };
        values.push((
            SCHEDULE_STATE.into(),
            serde_json::to_string(&schedule).map_err(storage)?,
        ));
    }
    store.set_settings_batch(&values).await.map_err(storage)?;
    *runtime.last_error.lock().expect("memory error") = None;
    runtime.wake.notify_one();
    notify(&app);
    status(&state).await
}

#[derive(Clone, Serialize, Deserialize)]
struct RunSnapshot {
    version: u32,
    memory: MemorySettings,
    report_settings: Settings,
    model_at_enqueue: Option<String>,
}
async fn snapshot(state: &AppState, config: MemorySettings) -> Result<String, ReportError> {
    let store = db(state)?;
    let report_settings = kernel::settings::load_settings(store.as_ref()).await;
    let answer = state.answer.lock().expect("answer slot").clone();
    let model_at_enqueue = match answer {
        Some(answer) => answer.answer_model_label().await,
        None => None,
    };
    serde_json::to_string(&RunSnapshot {
        version: 1,
        memory: config,
        report_settings,
        model_at_enqueue,
    })
    .map_err(storage)
}
/// Creates a run of the given persisted kind (`03 §8e`), letting the store reuse an
/// existing `(schedule_id, window)` identity; `kind` is not a deduplication dimension.
async fn enqueue(
    state: &AppState,
    config: MemorySettings,
    window: kernel::memory_schedule::DailyWindow,
    kind: ReportRunKind,
    schedule_id: Option<String>,
    prior: Option<&ReportRun>,
    requested_range: Option<MemoryReportRange>,
) -> Result<ReportRun, ReportError> {
    let store = db(state)?;
    let now = now_ms();
    let run = ReportRun {
        requested_range,
        run_id: uuid(),
        source_id: match prior {
            Some(p) => p.source_id.clone(),
            None => source(store.as_ref()).await?,
        },
        kind,
        schedule_id,
        revision_of: prior.map(|p| p.run_id.clone()),
        window_start: window.start,
        window_end: window.end,
        timezone: config
            .timezone
            .clone()
            .ok_or_else(|| fail("INVALID_CONFIG", "Select a time zone."))?,
        local_date: window.local_date,
        phase: ReportRunPhase::Queued,
        outcome: None,
        error: None,
        result: None,
        config_snapshot: snapshot(state, config).await?,
        created_at: now,
        updated_at: now,
    };
    let run = store.create_report_run(run).await.map_err(storage)?;
    tracing::info!(run_id=%run.run_id, window_start=run.window_start, window_end=run.window_end, explicit_range=run.requested_range.is_some(), "memory report enqueued");
    Ok(run)
}

#[tauri::command]
pub async fn run_memory_report(
    range: Option<MemoryReportRange>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ReportRun, ReportError> {
    let store = db(&state)?;
    let runtime = &state.memory_runtime;
    let _guard = runtime.control.lock().await;
    check_open(runtime)?;
    let config = settings(store.as_ref()).await?;
    validate(&config, true)?;
    let timezone = config.timezone.as_deref().unwrap();
    let now = now_ms();
    let window = match &range {
        Some(range) => kernel::memory_schedule::selected_range(
            timezone,
            &range.start_date,
            &range.end_date,
            now,
        ),
        None => kernel::memory_schedule::previous_day(timezone, now),
    }
    .map_err(|error| fail("INVALID_CONFIG", &error.to_string()))?;
    // Durable store uniqueness handles scheduled requests; manual base requests reuse
    // an already-active identical window under this enqueue lock.
    for run in store.list_pending_report_runs().await.map_err(storage)? {
        if range.is_none()
            && run.requested_range.is_none()
            && run.schedule_id.is_none()
            && run.revision_of.is_none()
            && run.window_start == window.start
            && run.window_end == window.end
            && matches!(run.phase, ReportRunPhase::Queued | ReportRunPhase::Running)
        {
            return Ok(run);
        }
    }
    let run = enqueue(
        &state,
        config,
        window,
        ReportRunKind::Daily,
        None,
        None,
        range,
    )
    .await?;
    runtime.wake.notify_one();
    notify(&app);
    Ok(run)
}
async fn lookup(store: &dyn Store, id: &str) -> Result<ReportRun, ReportError> {
    store
        .get_report_run(id)
        .await
        .map_err(storage)?
        .ok_or_else(|| fail("NOT_FOUND", "Report run not found."))
}
#[tauri::command]
pub async fn list_memory_runs(
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<ReportRun>, ReportError> {
    db(&state)?
        .list_report_runs(limit.unwrap_or(20).clamp(1, 100), None)
        .await
        .map_err(storage)
}
#[tauri::command]
pub async fn get_memory_run(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<ReportRun, ReportError> {
    let store = db(&state)?;
    let mut run = lookup(store.as_ref(), &run_id).await?;
    if let Some(result) = &run.result {
        if !store
            .report_evidence(result.frame_ids.clone())
            .await
            .map_err(storage)?
            .missing_frame_ids
            .is_empty()
        {
            run.error=Some(fail("SOURCE_UNAVAILABLE","Some original evidence has been deleted; the saved report retains its original provenance."));
        }
    }
    Ok(run)
}
#[tauri::command]
pub async fn get_memory_deliveries(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReportDelivery>, ReportError> {
    let mut items = db(&state)?
        .list_report_deliveries(&run_id)
        .await
        .map_err(storage)?;
    for item in &mut items {
        normalize_delivery_paths(item);
    }
    Ok(items)
}
/// `03 §8d` explicit recovery for a changed Telegram recipient: an attempt whose
/// immutable destination snapshot no longer matches the selected chat can never
/// dispatch, and changing settings must not redirect it silently. An explicit
/// retry is the user action that links a fresh attempt targeting the currently
/// selected chat while preserving the old attempt as reviewed. Creation happens
/// before the review so a crash between the two steps re-runs without loss.
///
/// The linked replacement set must cover the **complete** digest: cloning only
/// the unfinished chunks after an earlier chunk was confirmed would send the
/// new chat a broken `[i/N]` sequence missing its earlier parts. A chunk
/// confirmed to the old chat keeps its `Succeeded` receipt and is never re-sent
/// there, while its linked replacement delivers that chunk to the new chat for
/// the first time; a chunk already confirmed to the selected chat is skipped so
/// an explicit retry never repeats a confirmed delivery. `UnknownDelivery`
/// attempts are never auto-retargeted — `03 §8d` requires the user to review
/// them first, and they keep blocking dispatch until resolved.
async fn redirect_stale_telegram_deliveries(
    store: &dyn Store,
    run_id: &str,
    target_chat: Option<&str>,
) -> Result<(), ReportError> {
    let Some(target) = target_chat.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(());
    };
    let receipts = store
        .list_report_deliveries(run_id)
        .await
        .map_err(storage)?;
    for mut item in receipts
        .iter()
        .filter(|d| {
            d.destination == ReportDestination::Telegram
                && matches!(
                    d.state,
                    ReportDeliveryState::Pending
                        | ReportDeliveryState::Failed
                        | ReportDeliveryState::Succeeded
                )
        })
        .cloned()
        .collect::<Vec<_>>()
    {
        let Ok(snapshot) = serde_json::from_str::<TelegramSnapshot>(&item.destination_snapshot)
        else {
            continue;
        };
        if snapshot.chat_id == target {
            continue;
        }
        let target_confirmed = item.chunk_index.is_some()
            && receipts.iter().any(|d| {
                d.destination == ReportDestination::Telegram
                    && d.state == ReportDeliveryState::Succeeded
                    && d.chunk_index == item.chunk_index
                    && serde_json::from_str::<TelegramSnapshot>(&d.destination_snapshot)
                        .is_ok_and(|other| other.chat_id == target)
            });
        let replacement_live = receipts.iter().any(|d| {
            d.revision_of.as_deref() == Some(item.id.as_str())
                && d.destination == ReportDestination::Telegram
                && !matches!(
                    d.state,
                    ReportDeliveryState::Reviewed | ReportDeliveryState::Skipped
                )
        });
        if !replacement_live && !target_confirmed {
            let mut replacement = item.clone();
            replacement.id = uuid();
            replacement.revision_of = Some(item.id.clone());
            replacement.state = ReportDeliveryState::Pending;
            replacement.destination_snapshot = serde_json::to_string(&TelegramSnapshot {
                chat_id: target.to_owned(),
                text: snapshot.text,
            })
            .map_err(storage)?;
            replacement.message_id = None;
            replacement.error = None;
            replacement.next_attempt_at = None;
            replacement.attempt = 0;
            replacement.created_at = now_ms();
            replacement.updated_at = replacement.created_at;
            store
                .create_report_delivery(replacement)
                .await
                .map_err(storage)?;
            tracing::info!(run_id=%run_id, attempt_id=%item.id, "stale telegram destination redirected to the currently selected chat");
        }
        if item.state != ReportDeliveryState::Succeeded {
            let prior = item.state;
            item.state = ReportDeliveryState::Reviewed;
            update_delivery(store, &mut item, prior).await?;
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn retry_memory_report(
    run_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ReportRun, ReportError> {
    let store = db(&state)?;
    let runtime = &state.memory_runtime;
    let _guard = runtime.control.lock().await;
    check_open(runtime)?;
    if runtime.active.load(Ordering::Acquire) {
        return Err(fail(
            "BUSY",
            "A report or delivery is active; wait before retrying or regenerating.",
        ));
    }
    let mut run = lookup(store.as_ref(), &run_id).await?;
    if matches!(run.phase, ReportRunPhase::Queued | ReportRunPhase::Running) {
        return Err(fail("BUSY", "This report is already queued or running."));
    }
    // `03 §8d` explicit retargeting: attempts whose immutable destination snapshot
    // no longer matches the selected chat can never dispatch, and changing settings
    // must not redirect them silently. Retry links fresh attempts to the currently
    // selected recipient while preserving the old ones as reviewed.
    let current = settings(store.as_ref()).await?;
    redirect_stale_telegram_deliveries(
        store.as_ref(),
        &run_id,
        current.telegram_chat_id.as_deref(),
    )
    .await?;
    for mut item in store
        .list_report_deliveries(&run_id)
        .await
        .map_err(storage)?
    {
        if item.destination != ReportDestination::Telegram
            && item.state == ReportDeliveryState::Conflict
        {
            item.state = ReportDeliveryState::Reviewed;
            update_delivery(store.as_ref(), &mut item, ReportDeliveryState::Conflict).await?;
        }
        if item.destination == ReportDestination::Telegram
            && item.state == ReportDeliveryState::Failed
        {
            item.state = ReportDeliveryState::Pending;
            item.next_attempt_at = None;
            item.error = None;
            update_delivery(store.as_ref(), &mut item, ReportDeliveryState::Failed).await?;
        }
    }
    run.phase = ReportRunPhase::Queued;
    run.error = None;
    if run.result.is_none() {
        run.outcome = None;
    }
    run.updated_at = now_ms();
    store
        .update_report_run(run.clone())
        .await
        .map_err(storage)?;
    runtime.wake.notify_one();
    notify(&app);
    Ok(run)
}
#[tauri::command]
pub async fn regenerate_memory_report(
    run_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ReportRun, ReportError> {
    let store = db(&state)?;
    let runtime = &state.memory_runtime;
    let _guard = runtime.control.lock().await;
    check_open(runtime)?;
    if runtime.active.load(Ordering::Acquire) {
        return Err(fail(
            "BUSY",
            "A report or delivery is active; wait before retrying or regenerating.",
        ));
    }
    let prior = lookup(store.as_ref(), &run_id).await?;
    if matches!(
        prior.phase,
        ReportRunPhase::Queued | ReportRunPhase::Running
    ) {
        return Err(fail("BUSY", "This report is already active."));
    }
    let mut config = settings(store.as_ref()).await?;
    config.timezone = Some(prior.timezone.clone());
    validate(&config, true)?;
    let window = kernel::memory_schedule::DailyWindow {
        start: prior.window_start,
        end: prior.window_end,
        local_date: prior.local_date.clone(),
        due_at: now_ms(),
    };
    // Regenerate mirrors `§8c` (`03 §8e`): the linked revision keeps the prior run's
    // kind, schedule identity and explicit UTC `[start, end)` window.
    let run = enqueue(
        &state,
        config,
        window,
        prior.kind,
        prior.schedule_id.clone(),
        Some(&prior),
        prior.requested_range.clone(),
    )
    .await?;
    runtime.wake.notify_one();
    notify(&app);
    Ok(run)
}
#[tauri::command]
pub async fn reset_memory_source_identity(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryStatus, ReportError> {
    let store = db(&state)?;
    let runtime = &state.memory_runtime;
    let _guard = runtime.control.lock().await;
    check_open(runtime)?;
    let config = settings(store.as_ref()).await?;
    if config.enabled
        || config.telegram_enabled
        || runtime.active.load(Ordering::Acquire)
        || store
            .list_pending_report_runs()
            .await
            .map_err(storage)?
            .iter()
            .any(|r| matches!(r.phase, ReportRunPhase::Queued | ReportRunPhase::Running))
    {
        return Err(fail(
            "BUSY",
            "Disable schedules and Telegram and finish active runs before resetting identity.",
        ));
    }
    store
        .reset_report_source_id(&uuid())
        .await
        .map_err(storage)?;
    notify(&app);
    status(&state).await
}

async fn schedule_due(state: &AppState) -> Result<(), ReportError> {
    let store = db(state)?;
    let _guard = state.memory_runtime.control.lock().await;
    check_open(&state.memory_runtime)?;
    let config = settings(store.as_ref()).await?;
    if !config.enabled {
        return Ok(());
    }
    validate(&config, false)?;
    let mut schedule: ReportScheduleState =
        setting::<Option<ReportScheduleState>>(store.as_ref(), SCHEDULE_STATE)
            .await?
            .ok_or_else(|| {
                fail(
                    "INVALID_CONFIG",
                    "Schedule activation state is missing; disable and enable the schedule again.",
                )
            })?;
    let due = kernel::memory_schedule::due_windows(
        config.timezone.as_deref().unwrap(),
        config.local_time.as_deref().unwrap(),
        schedule.activated_at,
        schedule.last_window_end,
        now_ms(),
    )
    .map_err(|_| {
        fail(
            "INVALID_CONFIG",
            "The schedule window could not be resolved.",
        )
    })?;
    if let Some((start, end)) = due.gap {
        schedule.gap_start = Some(schedule.gap_start.map_or(start, |old| old.min(start)));
        schedule.gap_end = Some(end);
        schedule.last_window_end = Some(end);
    }
    for window in due.windows {
        let end = window.end;
        enqueue(
            state,
            config.clone(),
            window,
            ReportRunKind::Daily,
            Some(schedule.schedule_id.clone()),
            None,
            None,
        )
        .await?;
        schedule.last_window_end = Some(end);
        store
            .set_setting(
                SCHEDULE_STATE,
                &serde_json::to_string(&schedule).map_err(storage)?,
            )
            .await
            .map_err(storage)?;
    }
    store
        .set_setting(
            SCHEDULE_STATE,
            &serde_json::to_string(&schedule).map_err(storage)?,
        )
        .await
        .map_err(storage)?;
    Ok(())
}

fn requested_range_in_progress(run: &ReportRun) -> Result<bool, ReportError> {
    run.requested_range
        .as_ref()
        .map(|range| {
            kernel::memory_schedule::range_is_in_progress(
                &run.timezone,
                &range.end_date,
                run.window_end,
            )
            .map_err(|_| fail("INVALID_CONFIG", "Invalid saved report range."))
        })
        .transpose()
        .map(|value| value.unwrap_or(false))
}

/// Resolves the shared answer provider for daily and weekly runs (`03 §8c`/`§8e`):
/// an unsettled or uninstalled model is a visible `MODEL_UNAVAILABLE`, never a
/// silently empty report.
fn resolve_answer(
    answer: Option<Arc<inference::AnswerSidecar>>,
    settings: &Settings,
) -> Result<Arc<dyn AnswerProvider>, ReportError> {
    answer
        .ok_or_else(|| fail("MODEL_UNAVAILABLE", "Local answer model is not ready."))?
        .installed_provider_for(settings.models_answer_tier, SidecarParams::from(settings))
        .map_err(|_| {
            fail(
                "MODEL_UNAVAILABLE",
                "The selected answer model is not installed. Download it explicitly in Models, then retry.",
            )
        })
}

async fn generate(state: &AppState, run: &mut ReportRun) -> Result<(), ReportError> {
    if run.result.is_some() {
        return Ok(());
    }
    let store = db(state)?;
    let config: RunSnapshot = serde_json::from_str(&run.config_snapshot).map_err(storage)?;
    let settings = &config.report_settings;
    let answer = state.answer.lock().expect("answer slot").clone();
    let report_config = ReportConfig {
        daily_top_k: settings.reports_daily_top_k,
        weekly_top_k: settings.reports_weekly_top_k,
        map_reduce_min_frames: settings.reports_map_reduce_min_frames,
        reply_budget: crate::REPORT_REPLY_BUDGET,
    };
    let resolve = move || async move { resolve_answer(answer.clone(), settings) };
    match run.kind {
        ReportRunKind::Daily => {
            let execution = kernel::report_service::execute(
                store.as_ref(),
                kernel::report_service::ReportExecutionRequest {
                    range: ReportRange::MemoryDaily,
                    start: run.window_start,
                    end: run.window_end,
                    prompt: None,
                    timezone: Some(run.timezone.as_str()),
                    config: report_config,
                },
                resolve,
                None,
                &state.memory_runtime.cancel,
            )
            .await?;
            let evidence = store
                .report_evidence(execution.output.frame_ids.clone())
                .await
                .map_err(storage)?;
            save_execution(run, execution, evidence)
        }
        ReportRunKind::WeeklySynthesis => {
            let execution = kernel::report_service::execute_weekly(
                store.as_ref(),
                kernel::report_service::WeeklyExecutionRequest {
                    start: run.window_start,
                    end: run.window_end,
                    timezone: run.timezone.as_str(),
                    config: report_config,
                },
                resolve,
                None,
                &state.memory_runtime.cancel,
            )
            .await?;
            let evidence = store
                .report_evidence(execution.output.frame_ids.clone())
                .await
                .map_err(storage)?;
            save_weekly_execution(run, execution, evidence)
        }
    }
}

fn save_execution(
    run: &mut ReportRun,
    execution: kernel::report_service::ReportExecution,
    evidence: ReportEvidence,
) -> Result<(), ReportError> {
    let out = execution.output;
    let in_progress = requested_range_in_progress(run)?;
    let partial = out.truncated
        || !evidence.missing_frame_ids.is_empty()
        || (in_progress && out.mode != ReportMode::Empty);
    let as_of = if in_progress {
        format!("**In-progress coverage:** Selected dates are covered only as of {} (UTC); the final selected day was still in progress at enqueue time.\n\n", chrono::DateTime::<chrono::Utc>::from_timestamp_millis(run.window_end).ok_or_else(||fail("INVALID_CONFIG","Invalid saved cutoff."))?.to_rfc3339())
    } else {
        String::new()
    };
    run.outcome = Some(if out.mode == ReportMode::Empty {
        ReportOutcome::NoData
    } else if partial {
        ReportOutcome::Partial
    } else {
        ReportOutcome::Ok
    });
    run.result = Some(SavedReport {
        markdown: if partial {
            format!("{as_of}**Partial coverage:** This report uses sampled evidence and does not cover all requested activity.\n\n{}",out.markdown)
        } else {
            format!("{as_of}{}", out.markdown)
        },
        model: execution.model,
        frame_ids: out.frame_ids,
        session_ids: evidence.session_ids,
        cited_frame_ids: out.cited_frame_ids,
        frames_sampled: out.frames_sampled,
        frames_summarized: out.frames_summarized,
        periods_total: out.periods_total,
        periods_covered: out.periods_covered,
        passes: out.passes,
        // `truncated` carries the report engine's own truncation flag. The other
        // PARTIAL causes (in-progress range, deleted retained evidence) stay framed
        // by the run outcome and coverage fields, never by this metadata (`03 §8c`).
        truncated: out.truncated,
        daily_run_ids: Vec::new(),
    });
    Ok(())
}

/// The weekly PARTIAL lead names the concrete gaps (`03 §8e`) — days without complete
/// saved daily coverage, bounded sampling or deleted evidence — instead of reusing the
/// daily sampled-evidence sentence, which would misstate a week whose gap is specific days.
fn weekly_partial_banner(
    truncated: bool,
    missing_evidence: bool,
    missing_local_dates: &[String],
) -> String {
    let mut causes: Vec<String> = Vec::new();
    if !missing_local_dates.is_empty() {
        causes.push(format!(
            "the week is missing complete saved daily coverage for {}",
            missing_local_dates.join(", ")
        ));
    }
    if truncated {
        causes.push("sampling was bounded below the week's observed activity".into());
    }
    if missing_evidence {
        causes.push(
            "some evidence recorded by the week's daily reports has since been deleted".into(),
        );
    }
    format!("**Partial coverage:** {}.\n\n", causes.join("; "))
}

/// Saves a weekly synthesis with the `03 §8e` truthful statuses: `NO_DATA` for a week
/// with no saved dailies and no evidence, `PARTIAL` whenever a day lacks its saved
/// daily report, the output is truncated or recorded evidence is missing, and `OK`
/// only with complete measured coverage. Blocked and failed runs never reach this
/// save; `process_run` classifies them.
fn save_weekly_execution(
    run: &mut ReportRun,
    execution: kernel::report_service::WeeklyExecution,
    evidence: ReportEvidence,
) -> Result<(), ReportError> {
    let out = execution.output;
    let missing_local_dates = execution.missing_local_dates;
    let missing_evidence = !evidence.missing_frame_ids.is_empty();
    let partial = out.truncated || missing_evidence || !missing_local_dates.is_empty();
    run.outcome = Some(if out.mode == ReportMode::Empty {
        ReportOutcome::NoData
    } else if partial {
        ReportOutcome::Partial
    } else {
        ReportOutcome::Ok
    });
    let banner = if partial && out.mode != ReportMode::Empty {
        weekly_partial_banner(out.truncated, missing_evidence, &missing_local_dates)
    } else {
        String::new()
    };
    run.result = Some(SavedReport {
        markdown: format!("{banner}{}", out.markdown),
        model: execution.model,
        frame_ids: out.frame_ids,
        session_ids: evidence.session_ids,
        cited_frame_ids: out.cited_frame_ids,
        frames_sampled: out.frames_sampled,
        frames_summarized: out.frames_summarized,
        periods_total: out.periods_total,
        periods_covered: out.periods_covered,
        passes: out.passes,
        truncated: out.truncated,
        daily_run_ids: execution.daily_run_ids,
    });
    Ok(())
}

fn delivery(run: &ReportRun, destination: ReportDestination, snapshot: String) -> ReportDelivery {
    let now = now_ms();
    ReportDelivery {
        id: uuid(),
        run_id: run.run_id.clone(),
        destination,
        state: ReportDeliveryState::Pending,
        revision_of: None,
        destination_snapshot: snapshot,
        path: None,
        hash: None,
        recovery_path: None,
        chunk_index: None,
        chunk_count: None,
        message_id: None,
        error: None,
        next_attempt_at: None,
        attempt: 0,
        created_at: now,
        updated_at: now,
    }
}
async fn update_delivery(
    store: &dyn Store,
    item: &mut ReportDelivery,
    expected: ReportDeliveryState,
) -> Result<(), ReportError> {
    normalize_delivery_paths(item);
    item.updated_at = now_ms();
    if !store
        .update_report_delivery(item.clone(), expected)
        .await
        .map_err(storage)?
    {
        return Err(fail(
            "BUSY",
            "The delivery state changed; refresh before retrying.",
        ));
    }
    tracing::info!(run_id=%item.run_id, destination=?item.destination, state=?item.state, error_code=item.error.as_ref().map(|e|e.code.as_str()), "memory delivery transitioned");
    Ok(())
}

fn normalize_delivery_paths(item: &mut ReportDelivery) {
    for value in [&mut item.path, &mut item.recovery_path]
        .into_iter()
        .flatten()
    {
        *value = path_string(Path::new(value));
    }
}
/// Keep potentially blocking native reads off the cancellable report worker.
/// The closure owns only a path; cancelling its await cannot retain DB/model state.
async fn read_owned(path: &Path) -> Result<Option<Vec<u8>>, crate::memory_files::WriteFailure> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || crate::memory_files::safe_read_owned(&path))
        .await
        .map_err(|_| crate::memory_files::WriteFailure {
            message: "The file reader stopped unexpectedly.".into(),
            conflict: false,
            recovery_path: None,
        })?
}

async fn file_delivery(
    store: &dyn Store,
    run: &ReportRun,
    destination: ReportDestination,
    path: PathBuf,
    content: String,
) -> Result<ReportDelivery, ReportError> {
    let existing = store
        .list_report_deliveries(&run.run_id)
        .await
        .map_err(storage)?;
    let superseded: std::collections::HashSet<_> = existing
        .iter()
        .filter_map(|d| d.revision_of.clone())
        .collect();
    let candidate = existing
        .into_iter()
        .filter(|d| d.destination == destination && !superseded.contains(&d.id))
        .max_by_key(|d| d.created_at);
    let mut item = match candidate {
        Some(item)
            if matches!(
                item.state,
                ReportDeliveryState::Succeeded | ReportDeliveryState::Conflict
            ) =>
        {
            return Ok(item)
        }
        Some(item)
            if !matches!(
                item.state,
                ReportDeliveryState::Skipped
                    | ReportDeliveryState::Conflict
                    | ReportDeliveryState::Reviewed
            ) =>
        {
            item
        }
        previous => {
            let mut item = delivery(
                run,
                destination,
                serde_json::to_string(&path_string(&path)).map_err(storage)?,
            );
            item.revision_of = previous.map(|p| p.id);
            item.path = Some(path_string(&path));
            item.hash = Some(format!("{:x}", Sha256::digest(content.as_bytes())));
            store.create_report_delivery(item).await.map_err(storage)?
        }
    };
    let prior = item.state;
    let ownership = store
        .get_report_file(&path_string(&path))
        .await
        .map_err(storage)?;
    if ownership
        .as_ref()
        .is_some_and(|owner| owner.source_id != run.source_id)
    {
        item.state = ReportDeliveryState::Conflict;
        item.error = Some(fail(
            "FILE_CONFLICT",
            "Another source owns this generated file.",
        ));
        update_delivery(store, &mut item, prior).await?;
        return Ok(item);
    }
    // Reconcile the persisted write intent after a crash. Matching bytes alone do
    // not prove arbitrary ownership: the intent and same-source owner must agree.
    if item.attempt > 0 && item.hash.is_some() {
        let recovered = match read_owned(&path).await {
            Ok(bytes) => bytes,
            Err(error) => {
                item.state = if error.conflict {
                    ReportDeliveryState::Conflict
                } else {
                    ReportDeliveryState::Failed
                };
                item.error = Some(fail(
                    if error.conflict {
                        "FILE_CONFLICT"
                    } else {
                        "WRITE_FAILED"
                    },
                    &error.message,
                ));
                item.recovery_path = error.recovery_path;
                update_delivery(store, &mut item, prior).await?;
                return Ok(item);
            }
        };
        if let Some(bytes) = recovered {
            if Some(format!("{:x}", Sha256::digest(&bytes))) == item.hash {
                store
                    .save_report_file(ReportFileOwnership {
                        path: path_string(&path),
                        source_id: run.source_id.clone(),
                        run_id: run.run_id.clone(),
                        hash: item.hash.clone().unwrap(),
                        updated_at: now_ms(),
                    })
                    .await
                    .map_err(storage)?;
                item.state = ReportDeliveryState::Succeeded;
                item.error = None;
                update_delivery(store, &mut item, prior).await?;
                return Ok(item);
            }
        }
    }
    // Refresh an index's intended hash before a new write; a previous interrupted
    // intent was reconciled above before considering a different current index.
    item.hash = Some(format!("{:x}", Sha256::digest(content.as_bytes())));
    item.state = ReportDeliveryState::Sending;
    item.attempt += 1;
    item.error = None;
    update_delivery(store, &mut item, prior).await?;
    let expected = ownership.map(|owner| owner.hash);
    let write_path = path.clone();
    // Native filesystem I/O is not cancellable. If shutdown aborts this await,
    // the path/content-only writer may finish; its persisted Sending/hash intent
    // remains recoverable on restart. This is not an inference task.
    let receipt = tokio::task::spawn_blocking(move || {
        crate::memory_files::write_owned(&write_path, &content, expected.as_deref())
    })
    .await
    .map_err(|_| fail("WRITE_FAILED", "The file writer stopped unexpectedly."))?;
    match receipt {
        Ok(receipt) => {
            store
                .save_report_file(ReportFileOwnership {
                    path: path_string(&path),
                    source_id: run.source_id.clone(),
                    run_id: run.run_id.clone(),
                    hash: receipt.hash.clone(),
                    updated_at: now_ms(),
                })
                .await
                .map_err(storage)?;
            item.hash = Some(receipt.hash);
            item.state = ReportDeliveryState::Succeeded;
            item.error = None;
        }
        Err(error) => {
            item.state = if error.conflict {
                ReportDeliveryState::Conflict
            } else {
                ReportDeliveryState::Failed
            };
            item.error = Some(fail(
                if error.conflict {
                    "FILE_CONFLICT"
                } else {
                    "WRITE_FAILED"
                },
                &error.message,
            ));
            item.recovery_path = error.recovery_path;
        }
    }
    update_delivery(store, &mut item, ReportDeliveryState::Sending).await?;
    Ok(item)
}
fn index_topic(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let body = text
        .split_once("\n---\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    let line = body.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !line.starts_with('#')
            && !line.starts_with('>')
            && !line.starts_with("Evidence IDs")
            && !line.starts_with("**Partial")
    });
    line.map(|line| {
        format!(
            " — {}",
            line.chars()
                .filter(|c| !matches!(c, '[' | ']' | '(' | ')' | '<' | '>' | '`' | '*' | '_'))
                .take(100)
                .collect::<String>()
        )
    })
    .unwrap_or_default()
}

async fn base_report_id(store: &dyn Store, run: &ReportRun) -> Result<Option<String>, ReportError> {
    if run.requested_range.is_none() {
        return Ok(None);
    }
    let mut current = run.clone();
    let mut seen = std::collections::HashSet::new();
    while let Some(parent_id) = current.revision_of.clone() {
        if !seen.insert(parent_id.clone()) {
            return Err(fail(
                "SOURCE_UNAVAILABLE",
                "Report revision chain is invalid.",
            ));
        }
        current = store
            .get_report_run(&parent_id)
            .await
            .map_err(storage)?
            .ok_or_else(|| {
                fail(
                    "SOURCE_UNAVAILABLE",
                    "The original range report is unavailable.",
                )
            })?;
        if current.source_id != run.source_id
            || current.requested_range != run.requested_range
            || current.window_start != run.window_start
            || current.window_end != run.window_end
        {
            return Err(fail(
                "SOURCE_UNAVAILABLE",
                "The original range report does not match this revision.",
            ));
        }
    }
    Ok(Some(current.run_id))
}

async fn write_files(
    store: &dyn Store,
    run: &ReportRun,
    config: &RunSnapshot,
) -> Result<(), ReportError> {
    let folder = config
        .memory
        .output_folder
        .as_deref()
        .ok_or_else(|| fail("INVALID_CONFIG", "The saved output folder is unavailable."))?;
    let root = native_path(Path::new(folder)).join("ScreenSearch");
    if run.result.is_none() {
        for dest in [ReportDestination::Vault, ReportDestination::Index] {
            if !store
                .list_report_deliveries(&run.run_id)
                .await
                .map_err(storage)?
                .iter()
                .any(|d| d.destination == dest)
            {
                let mut item = delivery(
                    run,
                    dest,
                    serde_json::to_string(&path_string(&root)).map_err(storage)?,
                );
                item.state = ReportDeliveryState::Skipped;
                item.error = run.error.clone();
                store.create_report_delivery(item).await.map_err(storage)?;
            }
        }
        return Ok(());
    }
    if run.kind == ReportRunKind::WeeklySynthesis {
        // `03 §8e`: the weekly note and index branches belong to the weekly writer
        // (G3). A weekly synthesis must never write a daily-shaped note, so it
        // records honest skipped receipts and leaves note and index untouched.
        for dest in [ReportDestination::Vault, ReportDestination::Index] {
            if !store
                .list_report_deliveries(&run.run_id)
                .await
                .map_err(storage)?
                .iter()
                .any(|d| d.destination == dest)
            {
                let mut item = delivery(
                    run,
                    dest,
                    serde_json::to_string(&path_string(&root)).map_err(storage)?,
                );
                item.state = ReportDeliveryState::Skipped;
                item.error = Some(fail(
                    "WRITE_FAILED",
                    "This version writes no weekly synthesis note.",
                ));
                store.create_report_delivery(item).await.map_err(storage)?;
            }
        }
        return Ok(());
    }
    let base_id = base_report_id(store, run).await?;
    let relative = crate::memory_files::note_relative_path(
        &run.local_date,
        run.requested_range.as_ref(),
        base_id.as_deref(),
    )
    .map_err(|_| fail("WRITE_FAILED", "The saved report filename is invalid."))?;
    let note = crate::memory_files::daily_note(run, base_id.as_deref())
        .map_err(|_| fail("WRITE_FAILED", "Could not render the saved report note."))?;
    let receipt = file_delivery(
        store,
        run,
        ReportDestination::Vault,
        root.join(relative),
        note,
    )
    .await?;
    if receipt.state != ReportDeliveryState::Succeeded {
        if !store
            .list_report_deliveries(&run.run_id)
            .await
            .map_err(storage)?
            .iter()
            .any(|d| d.destination == ReportDestination::Index)
        {
            let mut item = delivery(
                run,
                ReportDestination::Index,
                serde_json::to_string(&path_string(&root.join("Index.md"))).map_err(storage)?,
            );
            item.state = ReportDeliveryState::Skipped;
            item.error = Some(fail(
                "WRITE_FAILED",
                "The index waits for a successfully written daily note.",
            ));
            store.create_report_delivery(item).await.map_err(storage)?;
        }
        return Ok(());
    }
    let mut entries = Vec::new();
    for owned in store.list_report_files().await.map_err(storage)? {
        let native = native_path(Path::new(&owned.path));
        let path = native.as_path();
        if ![root.join("Daily"), root.join("Reports")]
            .iter()
            .any(|folder| path.parent() == Some(folder.as_path()))
            || owned.source_id != run.source_id
        {
            continue;
        }
        let Some(bytes) = read_owned(path).await.map_err(|_| {
            fail(
                "FILE_CONFLICT",
                "An indexed output path cannot be read safely.",
            )
        })?
        else {
            continue;
        };
        if format!("{:x}", Sha256::digest(&bytes)) != owned.hash {
            continue;
        }
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        let note_text = String::from_utf8_lossy(&bytes);
        let metadata = crate::memory_files::parse_frontmatter(&note_text).map_err(|_| {
            fail(
                "FILE_CONFLICT",
                "An owned note has invalid provenance metadata.",
            )
        })?;
        if metadata.source_id != owned.source_id || metadata.run_id != owned.run_id {
            return Err(fail(
                "FILE_CONFLICT",
                "An owned note does not match its saved provenance.",
            ));
        }
        let label_date = match &metadata.requested_range {
            Some(range) if range.start_date != range.end_date => {
                format!("{} to {}", range.start_date, range.end_date)
            }
            Some(range) => range.start_date.clone(),
            None => name.strip_suffix(".md").unwrap_or_default().to_owned(),
        };
        let expected = crate::memory_files::note_relative_path(
            &label_date,
            metadata.requested_range.as_ref(),
            metadata.base_run_id.as_deref(),
        )
        .map_err(|_| {
            fail(
                "FILE_CONFLICT",
                "An owned note has invalid date or filename metadata.",
            )
        })?;
        if root.join(&expected) != path {
            return Err(fail(
                "FILE_CONFLICT",
                "An owned note filename does not match its provenance.",
            ));
        }
        let label = serde_json::to_string(&metadata.status)
            .map_err(storage)?
            .trim_matches('"')
            .to_string();
        let topic = index_topic(&bytes);
        entries.push((
            format!("{label_date}/{}", path_string(&expected)),
            format!(
                "- [{label_date}]({}) — {label}{topic}\n",
                path_string(&expected)
            ),
        ));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.dedup_by(|a, b| a.0 == b.0);
    let content = format!(
        "# ScreenSearch memory\n\n{}",
        entries
            .into_iter()
            .map(|(_, line)| line)
            .collect::<String>()
    );
    file_delivery(
        store,
        run,
        ReportDestination::Index,
        root.join("Index.md"),
        content,
    )
    .await?;
    Ok(())
}

/// Settles a failed generation on the run (`03 §8c`/`§8e`): a missing prerequisite
/// (`MODEL_UNAVAILABLE`, `SOURCE_UNAVAILABLE`) is a visible BLOCKED; any other
/// execution error is FAILED. `process_run` is the only caller, keeping this the
/// single classification point for daily and weekly runs alike.
fn settle_generation_failure(run: &mut ReportRun, error: ReportError) {
    let blocked = error.code == "MODEL_UNAVAILABLE" || error.code == "SOURCE_UNAVAILABLE";
    run.phase = if blocked {
        ReportRunPhase::Blocked
    } else {
        ReportRunPhase::Finished
    };
    run.outcome = Some(if blocked {
        ReportOutcome::Blocked
    } else {
        ReportOutcome::Failed
    });
    run.error = Some(error);
}

async fn process_run(state: &AppState, mut run: ReportRun) -> Result<(), ReportError> {
    let store = db(state)?;
    let config: RunSnapshot = serde_json::from_str(&run.config_snapshot).map_err(storage)?;
    if let Err(error) = generate(state, &mut run).await {
        settle_generation_failure(&mut run, error);
    } else {
        run.phase = ReportRunPhase::Finished;
        run.error = None;
    }
    run.updated_at = now_ms();
    store
        .update_report_run(run.clone())
        .await
        .map_err(storage)?;
    tracing::info!(run_id=%run.run_id, outcome=?run.outcome, frames_summarized=run.result.as_ref().map(|r|r.frames_summarized), error_code=run.error.as_ref().map(|e|e.code.as_str()), "memory report generation finished");
    let file_result = write_files(store.as_ref(), &run, &config).await;
    let telegram_result = deliver_telegram(state, &run, &config).await;
    file_result?;
    telegram_result?;
    Ok(())
}

pub fn start(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.store.is_none() {
        return;
    }
    let runtime = state.memory_runtime.clone();
    let handle = app.clone();
    let task=tauri::async_runtime::handle().inner().spawn(async move{
        let state=handle.state::<AppState>();
        let recovery=async{
            let store=db(&state)?;source(store.as_ref()).await?;
            store.recover_report_work(now_ms()).await.map_err(storage)?;
            Ok::<(),ReportError>(())
        }.await;
        if let Err(error)=recovery{*runtime.last_error.lock().expect("memory error")=Some(error);notify(&handle);return;}
        loop{
            if runtime.stopping.load(Ordering::Acquire){break;}
            let result=async{
                schedule_due(&state).await?;
                let store=db(&state)?;
                let mut runs=store.list_pending_report_runs().await.map_err(storage)?;
                runs.sort_by_key(|r|(r.window_start,r.created_at));
                for run in runs{
                    if runtime.stopping.load(Ordering::Acquire){break;}
                    let claimed={
                        let _guard=runtime.control.lock().await;
                        // Startup race guard: defer generation claims until inference
                        // init settles; a transient answer=None must not persist a
                        // permanent MODEL_UNAVAILABLE block (`03 §8c`).
                        if run.phase==ReportRunPhase::Queued&&claim_allowed(&run,runtime.inference_settled()) {
                            let claimed=store.claim_report_run(&run.run_id,now_ms()).await.map_err(storage)?;
                            if claimed{runtime.active.store(true,Ordering::Release);}claimed
                        }else{false}
                    };
                    if claimed{
                        runtime.cancel.store(false,Ordering::Release);notify(&handle);
                        let result=process_run(&state,run).await;
                        runtime.active.store(false,Ordering::Release);notify(&handle);result?;
                    }else if run.result.is_some(){
                        let receipts=store.list_report_deliveries(&run.run_id).await.map_err(storage)?;
                        let missing=|dest|!receipts.iter().any(|d|d.destination==dest);
                        let file_pending=receipts.iter().any(|d|d.destination!=ReportDestination::Telegram&&d.state==ReportDeliveryState::Pending);
                        let retry_telegram=receipts.iter().any(|d|d.destination==ReportDestination::Telegram&&
                            (d.state==ReportDeliveryState::Pending||d.state==ReportDeliveryState::Failed&&d.next_attempt_at.is_some_and(|at|at<=now_ms())));
                        let file_work=missing(ReportDestination::Vault)||missing(ReportDestination::Index)||file_pending;
                        if file_work||missing(ReportDestination::Telegram)||retry_telegram{
                            {let _guard=runtime.control.lock().await;check_open(&runtime)?;runtime.active.store(true,Ordering::Release);}
                            notify(&handle);
                            let recovered=async{
                                let config:RunSnapshot=serde_json::from_str(&run.config_snapshot).map_err(storage)?;
                                let file_result=if file_work{write_files(store.as_ref(),&run,&config).await}else{Ok(())};
                                let telegram_result=deliver_telegram(&state,&run,&config).await;
                                file_result?;telegram_result?;Ok::<(),ReportError>(())
                            }.await;
                            runtime.active.store(false,Ordering::Release);notify(&handle);recovered?;
                        }
                    }
                }
                store.prune_report_runs(now_ms()-RETENTION_MS).await.map_err(storage)?;
                Ok::<(),ReportError>(())
            }.await;
            *runtime.last_error.lock().expect("memory error")=result.err();notify(&handle);
            tokio::select!{_=runtime.wake.notified()=>{},_=tokio::time::sleep(Duration::from_secs(30))=>{}}
        }
        runtime.active.store(false,Ordering::Release);notify(&handle);
    });
    *state.memory_runtime.join.lock().expect("memory worker") = Some(task);
}

async fn stop_join(mut task: tokio::task::JoinHandle<()>, timeout: Duration) {
    if tokio::time::timeout(timeout, &mut task).await.is_err() {
        task.abort();
        let _ = task.await;
    }
}
pub async fn stop(runtime: &MemoryRuntime) {
    runtime.stopping.store(true, Ordering::Release);
    runtime.cancel.store(true, Ordering::Release);
    runtime.wake.notify_one();
    let task = runtime.join.lock().expect("memory worker").take();
    if let Some(task) = task {
        stop_join(task, SHUTDOWN_TIMEOUT).await;
    }
    runtime.active.store(false, Ordering::Release);
}

#[derive(Serialize, Deserialize)]
struct TelegramSnapshot {
    chat_id: String,
    text: String,
}
fn retry_delay(attempt: u32, server_secs: u64) -> Option<Duration> {
    [30, 120, 600]
        .get(attempt.saturating_sub(1) as usize)
        .map(|base| Duration::from_secs((*base).max(server_secs)))
}
fn family_delivery_block(receipts: &[ReportDelivery], run_id: &str) -> Option<&'static str> {
    if receipts.iter().any(|d| {
        d.run_id != run_id
            && d.destination == ReportDestination::Telegram
            && d.state == ReportDeliveryState::Succeeded
    }) {
        Some("A related report revision already has confirmed Telegram delivery.")
    } else if receipts.iter().any(|d| {
        d.run_id != run_id
            && d.destination == ReportDestination::Telegram
            && d.state == ReportDeliveryState::UnknownDelivery
    }) {
        Some("A related report revision has unknown Telegram delivery; review it first.")
    } else {
        None
    }
}
async fn ancestor_delivery_block(
    store: &dyn Store,
    run: &ReportRun,
) -> Result<Option<&'static str>, ReportError> {
    let family = store
        .list_report_family_deliveries(&run.run_id)
        .await
        .map_err(storage)?;
    if let Some(reason) = family_delivery_block(&family, &run.run_id) {
        return Ok(Some(reason));
    }
    let mut ancestor = run.revision_of.clone();
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = ancestor {
        if !visited.insert(id.clone()) {
            return Ok(Some(
                "Revision history is inconsistent; inspect earlier delivery before sending.",
            ));
        }
        let Some(previous) = store.get_report_run(&id).await.map_err(storage)? else {
            return Ok(Some(
                "Earlier revision history is unavailable; delivery cannot be safely repeated.",
            ));
        };
        let receipts = store.list_report_deliveries(&id).await.map_err(storage)?;
        if receipts.iter().any(|d| {
            d.destination == ReportDestination::Telegram
                && d.state == ReportDeliveryState::Succeeded
        }) {
            return Ok(Some(
                "An earlier revision already has confirmed Telegram delivery.",
            ));
        }
        if receipts.iter().any(|d| {
            d.destination == ReportDestination::Telegram
                && d.state == ReportDeliveryState::UnknownDelivery
        }) {
            return Ok(Some(
                "An earlier revision has unknown delivery; review that attempt first.",
            ));
        }
        if receipts.iter().any(|d| {
            d.destination == ReportDestination::Telegram
                && matches!(
                    d.state,
                    ReportDeliveryState::Pending | ReportDeliveryState::Sending
                )
        }) {
            return Ok(Some("An earlier revision still has pending delivery; resolve it before sending a new revision."));
        }
        ancestor = previous.revision_of;
    }
    Ok(None)
}
async fn deliver_telegram(
    state: &AppState,
    run: &ReportRun,
    config: &RunSnapshot,
) -> Result<(), ReportError> {
    let store = db(state)?;
    let mut receipts = store
        .list_report_deliveries(&run.run_id)
        .await
        .map_err(storage)?;
    let has_chunks = receipts
        .iter()
        .any(|d| d.destination == ReportDestination::Telegram && d.chunk_index.is_some());
    let eligible = config.memory.telegram_enabled
        && run.kind == ReportRunKind::Daily
        && matches!(
            run.outcome,
            Some(ReportOutcome::Ok | ReportOutcome::Partial)
        );
    if !has_chunks
        && (eligible
            || !receipts
                .iter()
                .any(|d| d.destination == ReportDestination::Telegram))
    {
        let skip = if !config.memory.telegram_enabled {
            Some("Telegram was disabled for this report.")
        } else if run.kind != ReportRunKind::Daily {
            // `03 §8e`: the weekly digest format is a separate increment (G6); a
            // weekly synthesis must never be sent as a daily digest.
            Some("This version sends no Telegram digest for weekly syntheses.")
        } else if !matches!(
            run.outcome,
            Some(ReportOutcome::Ok | ReportOutcome::Partial)
        ) {
            Some("No digest is sent for a no-data, blocked or failed report.")
        } else {
            ancestor_delivery_block(store.as_ref(), run).await?
        };
        if let Some(reason) = skip {
            let mut item = delivery(
                run,
                ReportDestination::Telegram,
                serde_json::to_string(&config.memory.telegram_chat_id).map_err(storage)?,
            );
            item.state = ReportDeliveryState::Skipped;
            item.error = Some(fail("DELIVERY_FAILED", reason));
            store.create_report_delivery(item).await.map_err(storage)?;
            tracing::info!(run_id=%run.run_id, destination="telegram", state="skipped", outcome=?run.outcome, "memory delivery skipped");
            return Ok(());
        }
        let chat_id = config
            .memory
            .telegram_chat_id
            .clone()
            .ok_or_else(|| fail("INVALID_CONFIG", "Saved Telegram destination is missing."))?;
        let chunks = crate::memory_telegram::digest_chunks(run).map_err(|_| {
            fail(
                "DELIVERY_FAILED",
                "Could not render the saved report digest.",
            )
        })?;
        let count = chunks.len() as u32;
        // One transaction persists the full immutable digest before any send.
        let mut intents = Vec::new();
        for (index, text) in chunks.into_iter().enumerate() {
            let mut item = delivery(
                run,
                ReportDestination::Telegram,
                serde_json::to_string(&TelegramSnapshot {
                    chat_id: chat_id.clone(),
                    text,
                })
                .map_err(storage)?,
            );

            item.chunk_index = Some(index as u32);
            item.chunk_count = Some(count);
            intents.push(item);
        }
        store
            .create_report_deliveries(intents)
            .await
            .map_err(storage)?;
        receipts = store
            .list_report_deliveries(&run.run_id)
            .await
            .map_err(storage)?;
    }
    let current = settings(store.as_ref()).await?;
    if !current.telegram_enabled || state.memory_runtime.stopping.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut telegram: Vec<_> = receipts
        .into_iter()
        .filter(|d| d.destination == ReportDestination::Telegram)
        .collect();
    telegram.sort_by_key(|d| (d.chunk_index, d.created_at));
    if telegram
        .iter()
        .any(|d| d.state == ReportDeliveryState::UnknownDelivery)
    {
        return Ok(());
    }
    for mut item in telegram {
        if matches!(
            item.state,
            ReportDeliveryState::Succeeded
                | ReportDeliveryState::Skipped
                | ReportDeliveryState::Reviewed
        ) {
            continue;
        }
        if item.state == ReportDeliveryState::Failed && item.next_attempt_at.is_none() {
            break;
        }
        if item.next_attempt_at.is_some_and(|at| at > now_ms()) {
            break;
        }
        let payload: TelegramSnapshot =
            serde_json::from_str(&item.destination_snapshot).map_err(storage)?;
        // A changed recipient never redirects an old intent. Re-enabling the same
        // destination resumes only its previously pending immutable chunks.
        let current = settings(store.as_ref()).await?;
        if !current.telegram_enabled
            || current.telegram_chat_id.as_deref() != Some(&payload.chat_id)
            || state.memory_runtime.stopping.load(Ordering::Acquire)
        {
            break;
        }
        if item.revision_of.is_none()
            && ancestor_delivery_block(store.as_ref(), run)
                .await?
                .is_some()
        {
            break;
        }
        let token = match crate::memory_telegram::credential_read(&run.source_id) {
            Ok(Some(token)) => token,
            _ => {
                let prior = item.state;
                item.state = ReportDeliveryState::Failed;
                item.next_attempt_at = None;
                item.error = Some(fail(
                    "CREDENTIAL_UNAVAILABLE",
                    "Configure an available Telegram credential, then retry.",
                ));
                update_delivery(store.as_ref(), &mut item, prior).await?;
                break;
            }
        };
        let dispatch_guard = state.memory_runtime.control.lock().await;
        let latest = settings(store.as_ref()).await?;
        if !latest.telegram_enabled
            || latest.telegram_chat_id.as_deref() != Some(payload.chat_id.as_str())
            || state.memory_runtime.stopping.load(Ordering::Acquire)
        {
            break;
        }
        let prior = item.state;
        item.state = ReportDeliveryState::Sending;
        item.attempt += 1;
        item.next_attempt_at = None;
        item.error = None;
        update_delivery(store.as_ref(), &mut item, prior).await?;
        drop(dispatch_guard);
        let outcome =
            crate::memory_telegram::send_chunk(&token, &payload.chat_id, &payload.text).await;
        match outcome {
            crate::memory_telegram::SendOutcome::Confirmed(id) => {
                item.state = ReportDeliveryState::Succeeded;
                item.message_id = Some(id);
            }
            crate::memory_telegram::SendOutcome::Unknown => {
                item.state = ReportDeliveryState::UnknownDelivery;
                item.error=Some(fail("DELIVERY_FAILED","Telegram acceptance is unknown. Inspect the conversation before deciding whether to resend."));
            }
            crate::memory_telegram::SendOutcome::Permanent(message) => {
                item.state = ReportDeliveryState::Failed;
                item.error = Some(fail("DELIVERY_FAILED", &message));
            }
            crate::memory_telegram::SendOutcome::Retryable { after_secs } => {
                item.state = ReportDeliveryState::Failed;
                item.error = Some(fail(
                    "DELIVERY_FAILED",
                    "Telegram explicitly rejected the request temporarily.",
                ));
                item.next_attempt_at = retry_delay(item.attempt, after_secs).map(|delay| {
                    now_ms().saturating_add(i64::try_from(delay.as_millis()).unwrap_or(i64::MAX))
                });
            }
        }
        let succeeded = item.state == ReportDeliveryState::Succeeded;
        update_delivery(store.as_ref(), &mut item, ReportDeliveryState::Sending).await?;
        if !succeeded {
            break;
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn set_telegram_token(
    token: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryStatus, ReportError> {
    if token.trim().is_empty() {
        return Err(fail("INVALID_CONFIG", "Enter a Telegram bot token."));
    }
    let store = db(&state)?;
    let source = source(store.as_ref()).await?;
    crate::memory_telegram::credential_write(&source, &token).map_err(|_| {
        fail(
            "CREDENTIAL_UNAVAILABLE",
            "Could not save the Telegram credential in Windows Credential Manager.",
        )
    })?;
    notify(&app);
    status(&state).await
}
#[tauri::command]
pub async fn clear_telegram_token(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryStatus, ReportError> {
    let store = db(&state)?;
    let source = source(store.as_ref()).await?;
    crate::memory_telegram::credential_clear(&source).map_err(|_| {
        fail(
            "CREDENTIAL_UNAVAILABLE",
            "Could not clear the Telegram credential in Windows Credential Manager.",
        )
    })?;
    notify(&app);
    status(&state).await
}
#[tauri::command]
pub async fn resolve_telegram_delivery(
    attempt_id: String,
    resolution: TelegramDeliveryResolution,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ReportRun, ReportError> {
    let store = db(&state)?;
    let runtime = &state.memory_runtime;
    let _guard = runtime.control.lock().await;
    check_open(runtime)?;
    let mut old = store
        .get_report_delivery(&attempt_id)
        .await
        .map_err(storage)?
        .ok_or_else(|| fail("NOT_FOUND", "Delivery attempt not found."))?;
    let run = lookup(store.as_ref(), &old.run_id).await?;
    if old.destination != ReportDestination::Telegram
        || old.state != ReportDeliveryState::UnknownDelivery
    {
        return Err(fail(
            "BUSY",
            "This attempt no longer needs an unknown-delivery decision.",
        ));
    }
    let replacement = if resolution == TelegramDeliveryResolution::Resend {
        let mut new = old.clone();
        new.id = uuid();
        new.revision_of = Some(old.id.clone());
        new.state = ReportDeliveryState::Pending;
        new.message_id = None;
        new.error = None;
        new.attempt = 0;
        new.next_attempt_at = None;
        new.created_at = now_ms();
        new.updated_at = new.created_at;
        Some(new)
    } else {
        None
    };
    old.state = ReportDeliveryState::Reviewed;
    old.updated_at = now_ms();
    if !store
        .resolve_report_delivery(old, replacement)
        .await
        .map_err(storage)?
    {
        return Err(fail(
            "BUSY",
            "Another action already resolved this delivery.",
        ));
    }
    runtime.wake.notify_one();
    notify(&app);
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transient_retries_are_bounded_and_honor_server_delay() {
        assert_eq!(retry_delay(1, 0), Some(Duration::from_secs(30)));
        assert_eq!(retry_delay(2, 200), Some(Duration::from_secs(200)));
        assert_eq!(retry_delay(3, 0), Some(Duration::from_secs(600)));
        assert_eq!(retry_delay(4, 0), None);
    }
    #[test]
    fn queued_generation_defers_until_inference_settles_and_delivery_resume_is_unaffected() {
        let mut run = run_fixture();
        run.phase = ReportRunPhase::Queued;
        run.result = None;
        assert!(
            !claim_allowed(&run, false),
            "a startup catch-up run must not be claimed before inference settles"
        );
        assert!(claim_allowed(&run, true));
        run.result = Some(run_fixture().result.expect("fixture result"));
        assert!(
            claim_allowed(&run, false),
            "a saved-output retry only needs delivery rework"
        );
    }
    #[tokio::test]
    async fn inference_settlement_opens_the_claim_gate_and_wakes_the_worker() {
        let runtime = MemoryRuntime::new();
        assert!(!runtime.inference_settled());
        runtime.mark_inference_settled();
        assert!(runtime.inference_settled());
        assert!(
            tokio::time::timeout(Duration::from_millis(100), runtime.wake.notified())
                .await
                .is_ok(),
            "settlement must leave a stored wake permit for the worker loop"
        );
    }
    #[tokio::test]
    async fn retry_redirects_stale_telegram_attempts_to_the_selected_chat() {
        let store = SqliteStore::open_in_memory().unwrap();
        let run = run_fixture();
        store.create_report_run(run.clone()).await.unwrap();
        let snapshot = |chat: &str, text: &str| {
            serde_json::to_string(&TelegramSnapshot {
                chat_id: chat.into(),
                text: text.into(),
            })
            .unwrap()
        };
        let mut first = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "chunk one"),
        );
        first.chunk_index = Some(0);
        first.chunk_count = Some(3);
        let mut second = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "chunk two"),
        );
        second.chunk_index = Some(1);
        second.chunk_count = Some(3);
        let mut failed = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "third chunk"),
        );
        failed.chunk_index = Some(2);
        failed.chunk_count = Some(3);
        failed.state = ReportDeliveryState::Failed;
        store
            .create_report_deliveries(vec![first.clone(), second.clone(), failed.clone()])
            .await
            .unwrap();
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("new-chat"))
            .await
            .unwrap();
        let items = store.list_report_deliveries(&run.run_id).await.unwrap();
        for old in [&first, &second, &failed] {
            let stored = items.iter().find(|d| d.id == old.id).unwrap();
            assert_eq!(
                stored.state,
                ReportDeliveryState::Reviewed,
                "the old attempt is preserved as reviewed, never retargeted in place"
            );
            assert_eq!(stored.destination_snapshot, old.destination_snapshot);
        }
        let expected = ["chunk one", "chunk two", "third chunk"];
        let linked: Vec<_> = items
            .iter()
            .filter(|d| d.state == ReportDeliveryState::Pending)
            .collect();
        assert_eq!(linked.len(), 3);
        for d in &linked {
            let snap: TelegramSnapshot = serde_json::from_str(&d.destination_snapshot).unwrap();
            assert_eq!(snap.chat_id, "new-chat");
            assert_eq!(snap.text, expected[d.chunk_index.unwrap() as usize]);
            assert!(d.revision_of.is_some());
            assert_eq!(d.attempt, 0);
        }
        // A second explicit retry creates no further attempts.
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("new-chat"))
            .await
            .unwrap();
        assert_eq!(
            store
                .list_report_deliveries(&run.run_id)
                .await
                .unwrap()
                .len(),
            6
        );
        // A matching-chat failure is left for the ordinary retry reset, and an
        // absent selected chat cannot redirect anything.
        let mut same = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("new-chat", "same chat"),
        );
        same.state = ReportDeliveryState::Failed;
        store.create_report_delivery(same.clone()).await.unwrap();
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("new-chat"))
            .await
            .unwrap();
        assert_eq!(
            store
                .get_report_delivery(&same.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            ReportDeliveryState::Failed
        );
        redirect_stale_telegram_deliveries(&store, &run.run_id, None)
            .await
            .unwrap();
        assert_eq!(
            store
                .list_report_deliveries(&run.run_id)
                .await
                .unwrap()
                .len(),
            7
        );
    }
    #[tokio::test]
    async fn retarget_after_confirmed_chunk_recreates_the_complete_digest_for_the_new_chat() {
        let store = SqliteStore::open_in_memory().unwrap();
        let run = run_fixture();
        store.create_report_run(run.clone()).await.unwrap();
        let snapshot = |chat: &str, text: &str| {
            serde_json::to_string(&TelegramSnapshot {
                chat_id: chat.into(),
                text: text.into(),
            })
            .unwrap()
        };
        let mut confirmed = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "[1/3] chunk one"),
        );
        confirmed.chunk_index = Some(0);
        confirmed.chunk_count = Some(3);
        confirmed.state = ReportDeliveryState::Succeeded;
        confirmed.message_id = Some(41);
        confirmed.attempt = 1;
        let mut failed = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "[2/3] chunk two"),
        );
        failed.chunk_index = Some(1);
        failed.chunk_count = Some(3);
        failed.state = ReportDeliveryState::Failed;
        failed.attempt = 1;
        failed.error = Some(fail("DELIVERY_FAILED", "explicit rejection"));
        let mut leftover = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "[3/3] chunk three"),
        );
        leftover.chunk_index = Some(2);
        leftover.chunk_count = Some(3);
        store
            .create_report_deliveries(vec![confirmed.clone(), failed.clone(), leftover.clone()])
            .await
            .unwrap();
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("new-chat"))
            .await
            .unwrap();
        let items = store.list_report_deliveries(&run.run_id).await.unwrap();
        let stored = items.iter().find(|d| d.id == confirmed.id).unwrap();
        assert_eq!(
            stored.state,
            ReportDeliveryState::Succeeded,
            "a confirmed chunk keeps its receipt instead of being flagged reviewed"
        );
        assert_eq!(stored.message_id, Some(41));
        assert_eq!(stored.destination_snapshot, confirmed.destination_snapshot);
        for old in [&failed, &leftover] {
            assert_eq!(
                items.iter().find(|d| d.id == old.id).unwrap().state,
                ReportDeliveryState::Reviewed
            );
        }
        let texts = ["[1/3] chunk one", "[2/3] chunk two", "[3/3] chunk three"];
        let linked: Vec<_> = items
            .iter()
            .filter(|d| d.state == ReportDeliveryState::Pending)
            .collect();
        assert_eq!(
            linked.len(),
            3,
            "the new recipient must receive every digest chunk, including the confirmed one"
        );
        for d in &linked {
            let snap: TelegramSnapshot = serde_json::from_str(&d.destination_snapshot).unwrap();
            assert_eq!(snap.chat_id, "new-chat");
            assert_eq!(snap.text, texts[d.chunk_index.unwrap() as usize]);
            assert_eq!(d.chunk_count, Some(3));
            assert_eq!(d.attempt, 0);
            assert!(d.revision_of.is_some());
        }
        let first = linked
            .iter()
            .find(|d| d.chunk_index == Some(0))
            .expect("the confirmed chunk has a linked replacement");
        assert_eq!(first.revision_of.as_deref(), Some(confirmed.id.as_str()));
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("new-chat"))
            .await
            .unwrap();
        let items = store.list_report_deliveries(&run.run_id).await.unwrap();
        assert_eq!(
            items.len(),
            6,
            "a repeated explicit retry adds no further replacements"
        );
        assert_eq!(
            items.iter().find(|d| d.id == confirmed.id).unwrap().state,
            ReportDeliveryState::Succeeded
        );
    }
    #[tokio::test]
    async fn retargeting_back_to_a_settled_chat_never_repeats_confirmed_chunks() {
        let store = SqliteStore::open_in_memory().unwrap();
        let run = run_fixture();
        store.create_report_run(run.clone()).await.unwrap();
        let snapshot = |chat: &str, text: &str| {
            serde_json::to_string(&TelegramSnapshot {
                chat_id: chat.into(),
                text: text.into(),
            })
            .unwrap()
        };
        let mut settled = Vec::new();
        for index in 0..2u32 {
            let mut item = delivery(
                &run,
                ReportDestination::Telegram,
                snapshot("chat-a", &format!("chunk {index}")),
            );
            item.chunk_index = Some(index);
            item.chunk_count = Some(2);
            item.state = ReportDeliveryState::Succeeded;
            item.message_id = Some(i64::from(index) + 1);
            settled.push(item);
        }
        store
            .create_report_deliveries(settled.clone())
            .await
            .unwrap();
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("chat-b"))
            .await
            .unwrap();
        let items = store.list_report_deliveries(&run.run_id).await.unwrap();
        for mut item in items
            .iter()
            .filter(|d| d.state == ReportDeliveryState::Pending)
            .cloned()
        {
            let prior = item.state;
            item.state = ReportDeliveryState::Succeeded;
            update_delivery(&store, &mut item, prior).await.unwrap();
        }
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("chat-a"))
            .await
            .unwrap();
        let items = store.list_report_deliveries(&run.run_id).await.unwrap();
        assert_eq!(
            items.len(),
            4,
            "a settled chat must not receive a second copy of its confirmed chunks"
        );
        assert!(items
            .iter()
            .all(|d| d.state == ReportDeliveryState::Succeeded));
        for item in &settled {
            let stored = items.iter().find(|d| d.id == item.id).unwrap();
            assert_eq!(stored.state, ReportDeliveryState::Succeeded);
            assert_eq!(stored.message_id, item.message_id);
        }
    }
    #[tokio::test]
    async fn retarget_never_touches_unknown_delivery_attempts() {
        let store = SqliteStore::open_in_memory().unwrap();
        let run = run_fixture();
        store.create_report_run(run.clone()).await.unwrap();
        let snapshot = |chat: &str, text: &str| {
            serde_json::to_string(&TelegramSnapshot {
                chat_id: chat.into(),
                text: text.into(),
            })
            .unwrap()
        };
        let mut unknown = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "[1/2] chunk one"),
        );
        unknown.chunk_index = Some(0);
        unknown.chunk_count = Some(2);
        unknown.state = ReportDeliveryState::UnknownDelivery;
        unknown.attempt = 1;
        unknown.error = Some(fail("DELIVERY_FAILED", "acceptance unknown"));
        let mut failed = delivery(
            &run,
            ReportDestination::Telegram,
            snapshot("old-chat", "[2/2] chunk two"),
        );
        failed.chunk_index = Some(1);
        failed.chunk_count = Some(2);
        failed.state = ReportDeliveryState::Failed;
        store
            .create_report_deliveries(vec![unknown.clone(), failed.clone()])
            .await
            .unwrap();
        redirect_stale_telegram_deliveries(&store, &run.run_id, Some("new-chat"))
            .await
            .unwrap();
        let items = store.list_report_deliveries(&run.run_id).await.unwrap();
        let stored = items.iter().find(|d| d.id == unknown.id).unwrap();
        assert_eq!(stored.state, ReportDeliveryState::UnknownDelivery);
        assert_eq!(stored.destination_snapshot, unknown.destination_snapshot);
        assert!(
            items
                .iter()
                .all(|d| d.revision_of.as_deref() != Some(unknown.id.as_str())),
            "an unknown delivery is resolved by the user, never auto-retargeted"
        );
        assert_eq!(
            items.iter().find(|d| d.id == failed.id).unwrap().state,
            ReportDeliveryState::Reviewed
        );
        let retargeted: Vec<_> = items
            .iter()
            .filter(|d| d.state == ReportDeliveryState::Pending)
            .collect();
        assert_eq!(retargeted.len(), 1);
        assert_eq!(retargeted[0].chunk_index, Some(1));
    }
    #[tokio::test]
    async fn shutdown_cancels_active_report_and_joins_worker() {
        let runtime = MemoryRuntime::new();
        let cancel = runtime.cancel.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = started.send(());
            while !cancel.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        });
        *runtime.join.lock().unwrap() = Some(task);
        ready.await.unwrap();
        stop(&runtime).await;
        assert!(runtime.cancel.load(Ordering::Acquire));
        assert!(runtime.join.lock().unwrap().is_none());
        assert!(runtime.stopping.load(Ordering::Acquire));
    }
    #[tokio::test]
    async fn shutdown_bound_aborts_and_joins_unresponsive_worker() {
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _sender = sender;
            std::future::pending::<()>().await;
        });
        stop_join(task, Duration::from_millis(10)).await;
        assert!(
            receiver.await.is_err(),
            "joined worker dropped its owned resources"
        );
    }
    fn run_fixture() -> ReportRun {
        ReportRun {
            requested_range: None,
            run_id: "report-fixture".into(),
            source_id: "source-fixture".into(),
            kind: ReportRunKind::Daily,
            schedule_id: None,
            revision_of: None,
            window_start: 0,
            window_end: 86400000,
            timezone: "UTC".into(),
            local_date: "1970-01-01".into(),
            phase: ReportRunPhase::Finished,
            outcome: Some(ReportOutcome::NoData),
            error: None,
            result: Some(SavedReport {
                markdown: "No captured content in this time range.".into(),
                model: None,
                frame_ids: vec![],
                session_ids: vec![],
                cited_frame_ids: vec![],
                frames_sampled: 0,
                frames_summarized: 0,
                periods_total: 0,
                periods_covered: 0,
                passes: 0,
                truncated: false,
                daily_run_ids: Vec::new(),
            }),
            config_snapshot: "{}".into(),
            created_at: 1,
            updated_at: 1,
        }
    }
    #[tokio::test]
    async fn interrupted_write_reconciles_durable_intent_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        let store = SqliteStore::open_in_memory().unwrap();
        let run = run_fixture();
        store.create_report_run(run.clone()).await.unwrap();
        let content = "Durable generated note";
        let mut item = delivery(
            &run,
            ReportDestination::Vault,
            serde_json::to_string(&path_string(&path)).unwrap(),
        );
        item.path = Some(path_string(&path));
        item.hash = Some(format!("{:x}", Sha256::digest(content.as_bytes())));
        item.state = ReportDeliveryState::Sending;
        item.attempt = 1;
        store.create_report_delivery(item.clone()).await.unwrap();
        crate::memory_files::write_owned(&path, content, None).unwrap();
        store.recover_report_work(now_ms()).await.unwrap();
        let recovered = file_delivery(
            &store,
            &run,
            ReportDestination::Vault,
            path.clone(),
            content.into(),
        )
        .await
        .unwrap();
        assert_eq!(recovered.state, ReportDeliveryState::Succeeded);
        assert_eq!(recovered.id, item.id);
        assert_eq!(std::fs::read_to_string(path).unwrap(), content);
        assert_eq!(
            store
                .list_report_deliveries(&run.run_id)
                .await
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn confirmed_and_unknown_revision_deliveries_block_automatic_repeat() {
        let run = run_fixture();
        let mut receipt = delivery(&run, ReportDestination::Telegram, "{}".into());
        receipt.state = ReportDeliveryState::Succeeded;
        assert!(family_delivery_block(&[receipt.clone()], "other-revision").is_some());
        assert!(family_delivery_block(&[receipt.clone()], &run.run_id).is_none());
        receipt.state = ReportDeliveryState::UnknownDelivery;
        assert!(family_delivery_block(&[receipt.clone()], "other-revision").is_some());
        receipt.state = ReportDeliveryState::Reviewed;
        assert!(family_delivery_block(&[receipt], "other-revision").is_none());
    }
    #[test]
    fn current_day_real_evidence_is_partial_but_empty_is_still_no_data() {
        let mut run = run_fixture();
        run.requested_range = Some(MemoryReportRange {
            start_date: "1970-01-01".into(),
            end_date: "1970-01-01".into(),
        });
        run.window_end = 43_200_000;
        let output = |mode, truncated| kernel::report_service::ReportExecution {
            output: ReportOutput {
                markdown: "Observed activity".into(),
                frame_ids: vec![1],
                cited_frame_ids: vec![1],
                mode,
                periods_total: 1,
                periods_covered: 1,
                frames_sampled: 1,
                frames_summarized: 1,
                passes: 1,
                truncated,
            },
            model: Some("local-model".into()),
        };
        save_execution(
            &mut run,
            output(ReportMode::SinglePass, false),
            ReportEvidence {
                session_ids: vec![],
                missing_frame_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(run.outcome, Some(ReportOutcome::Partial));
        let saved = run.result.as_ref().unwrap();
        assert!(
            !saved.truncated,
            "complete elapsed coverage of an in-progress range is partial, not truncated"
        );
        assert!(saved.markdown.contains("1970-01-01T12:00:00+00:00"));
        assert!(saved.markdown.contains("In-progress coverage"));
        // Deleted retained evidence is the other non-engine PARTIAL cause and must
        // not claim the engine bounded the output either.
        save_execution(
            &mut run,
            output(ReportMode::SinglePass, false),
            ReportEvidence {
                session_ids: vec![],
                missing_frame_ids: vec![1],
            },
        )
        .unwrap();
        assert_eq!(run.outcome, Some(ReportOutcome::Partial));
        assert!(!run.result.as_ref().unwrap().truncated);
        // The engine's own truncation flag is what the saved metadata preserves.
        save_execution(
            &mut run,
            output(ReportMode::SinglePass, true),
            ReportEvidence {
                session_ids: vec![],
                missing_frame_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(run.outcome, Some(ReportOutcome::Partial));
        assert!(run.result.as_ref().unwrap().truncated);
        let mut empty = output(ReportMode::Empty, false);
        empty.output.frame_ids.clear();
        empty.output.cited_frame_ids.clear();
        empty.output.frames_summarized = 0;
        empty.output.frames_sampled = 0;
        empty.model = None;
        save_execution(
            &mut run,
            empty,
            ReportEvidence {
                session_ids: vec![],
                missing_frame_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(run.outcome, Some(ReportOutcome::NoData));
        assert!(!run.result.as_ref().unwrap().truncated);
        assert_eq!(run.window_end, 43_200_000);
    }

    #[tokio::test]
    async fn selected_notes_are_isolated_and_revisions_reuse_original_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let config = RunSnapshot {
            version: 1,
            memory: MemorySettings {
                output_folder: Some(path_string(dir.path())),
                ..Default::default()
            },
            report_settings: Settings::default(),
            model_at_enqueue: None,
        };
        let mut first = run_fixture();
        first.run_id = uuid();
        first.requested_range = Some(MemoryReportRange {
            start_date: "1970-01-01".into(),
            end_date: "1970-01-01".into(),
        });
        let mut second = first.clone();
        second.run_id = uuid();
        let mut daily = run_fixture();
        daily.run_id = uuid();
        for run in [&first, &second, &daily] {
            store.create_report_run(run.clone()).await.unwrap();
            write_files(&store, run, &config).await.unwrap();
        }
        let relative = crate::memory_files::note_relative_path(
            &first.local_date,
            first.requested_range.as_ref(),
            Some(&first.run_id),
        )
        .unwrap();
        let path = dir.path().join("ScreenSearch").join(relative);
        let original = std::fs::read_to_string(&path).unwrap();
        let mut revision = first.clone();
        revision.run_id = uuid();
        revision.revision_of = Some(first.run_id.clone());
        store.create_report_run(revision.clone()).await.unwrap();
        assert_eq!(
            base_report_id(&store, &revision).await.unwrap(),
            Some(first.run_id.clone())
        );
        write_files(&store, &revision, &config).await.unwrap();
        let revised = std::fs::read_to_string(&path).unwrap();
        assert_ne!(original, revised);
        let metadata = crate::memory_files::parse_frontmatter(&revised).unwrap();
        assert_eq!(metadata.run_id, revision.run_id);
        assert_eq!(metadata.base_run_id, Some(first.run_id.clone()));
        assert_eq!(metadata.requested_range, first.requested_range);
        let index = std::fs::read_to_string(dir.path().join("ScreenSearch/Index.md")).unwrap();
        assert!(index.contains("Daily/1970-01-01.md"));
        assert_eq!(index.matches("](Reports/").count(), 2);
        assert_eq!(store.list_report_files().await.unwrap().len(), 4);
        revision.revision_of = Some(uuid());
        assert!(base_report_id(&store, &revision).await.is_err());
    }

    fn stamp(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .expect("fixture timestamp")
            .timestamp_millis()
    }

    /// A weekly synthesis run fixture (`03 §8e`): ISO week 2026-W42 in Europe/Paris,
    /// scheduled, with a saved result so delivery paths can be exercised.
    fn weekly_run_fixture() -> ReportRun {
        let mut run = run_fixture();
        run.run_id = "weekly-report-fixture".into();
        run.kind = ReportRunKind::WeeklySynthesis;
        run.schedule_id = Some("weekly-schedule-fixture".into());
        run.window_start = stamp("2026-10-11T22:00:00Z");
        run.window_end = stamp("2026-10-18T22:00:00Z");
        run.timezone = "Europe/Paris".into();
        run.local_date = "2026-W42".into();
        run
    }

    fn weekly_execution(
        mode: ReportMode,
        truncated: bool,
        missing_local_dates: Vec<String>,
    ) -> kernel::report_service::WeeklyExecution {
        kernel::report_service::WeeklyExecution {
            output: ReportOutput {
                markdown: "Weekly synthesis body".into(),
                frame_ids: vec![11, 12],
                cited_frame_ids: vec![11],
                mode,
                periods_total: 7,
                periods_covered: 7,
                frames_sampled: 2,
                frames_summarized: 2,
                passes: 2,
                truncated,
            },
            model: Some("local-model".into()),
            daily_run_ids: vec!["daily-run-a".into(), "daily-run-b".into()],
            missing_local_dates,
        }
    }

    #[tokio::test]
    async fn weekly_requires_saved_dailies() {
        // `03 §8e`: a week with no saved dailies and no evidence is NO_DATA, decided
        // before any model is acquired — the §8c inspect-before-model rule applies.
        let store = SqliteStore::open_in_memory().unwrap();
        let cancel = AtomicBool::new(false);
        let execution = kernel::report_service::execute_weekly(
            &store,
            kernel::report_service::WeeklyExecutionRequest {
                start: stamp("2026-10-11T22:00:00Z"),
                end: stamp("2026-10-18T22:00:00Z"),
                timezone: "Europe/Paris",
                config: ReportConfig {
                    daily_top_k: 24,
                    weekly_top_k: 200,
                    map_reduce_min_frames: 24,
                    reply_budget: crate::REPORT_REPLY_BUDGET,
                },
            },
            || async {
                Err(fail(
                    "MODEL_UNAVAILABLE",
                    "a no-dailies week must not acquire a model",
                ))
            },
            None,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(execution.output.mode, ReportMode::Empty);
        assert!(execution.daily_run_ids.is_empty());
        let mut run = weekly_run_fixture();
        let evidence = store
            .report_evidence(execution.output.frame_ids.clone())
            .await
            .unwrap();
        save_weekly_execution(&mut run, execution, evidence).unwrap();
        assert_eq!(run.outcome, Some(ReportOutcome::NoData));
        let saved = run.result.as_ref().unwrap();
        assert!(saved.model.is_none());
        assert!(!saved.markdown.contains("Partial coverage"));
    }

    #[test]
    fn partial_week_reports_missing_days() {
        // `03 §8e`: any day in the window without a saved daily report makes the
        // whole week PARTIAL, and the banner names the missing local dates.
        let mut run = weekly_run_fixture();
        save_weekly_execution(
            &mut run,
            weekly_execution(
                ReportMode::MapReduce,
                false,
                vec!["2026-10-14".into(), "2026-10-15".into()],
            ),
            ReportEvidence {
                session_ids: vec![],
                missing_frame_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(run.outcome, Some(ReportOutcome::Partial));
        let saved = run.result.as_ref().unwrap();
        assert!(saved
            .markdown
            .contains("missing complete saved daily coverage"));
        assert!(saved.markdown.contains("2026-10-14"));
        assert!(saved.markdown.contains("2026-10-15"));
        assert!(
            !saved.markdown.contains("uses sampled evidence"),
            "the weekly banner must not reuse the daily sampled-evidence wording"
        );
    }

    #[test]
    fn weekly_no_model_is_blocked() {
        // A missing model surfaces as MODEL_UNAVAILABLE and settles as a visible
        // BLOCKED — never FAILED and never a silently saved empty report (`03 §8e`).
        let Err(error) = resolve_answer(None, &Settings::default()) else {
            panic!("a missing model must not resolve a provider");
        };
        assert_eq!(error.code, "MODEL_UNAVAILABLE");
        let mut run = weekly_run_fixture();
        run.phase = ReportRunPhase::Running;
        run.outcome = None;
        run.result = None;
        settle_generation_failure(&mut run, error);
        assert_eq!(run.phase, ReportRunPhase::Blocked);
        assert_eq!(run.outcome, Some(ReportOutcome::Blocked));
        assert!(
            run.result.is_none(),
            "a blocked weekly run must not carry an empty report"
        );
        assert_eq!(
            run.error.as_ref().map(|error| error.code.as_str()),
            Some("MODEL_UNAVAILABLE")
        );
        // Other execution errors stay FAILED.
        let mut failed = weekly_run_fixture();
        failed.phase = ReportRunPhase::Running;
        failed.result = None;
        settle_generation_failure(&mut failed, fail("GENERATION_FAILED", "no usable text"));
        assert_eq!(failed.phase, ReportRunPhase::Finished);
        assert_eq!(failed.outcome, Some(ReportOutcome::Failed));
    }

    #[test]
    fn weekly_statuses_are_truthful() {
        let complete_evidence = || ReportEvidence {
            session_ids: vec![7],
            missing_frame_ids: vec![],
        };
        // Truncated output is PARTIAL with a bounded-sampling banner naming no dates.
        let mut truncated = weekly_run_fixture();
        save_weekly_execution(
            &mut truncated,
            weekly_execution(ReportMode::MapReduce, true, vec![]),
            complete_evidence(),
        )
        .unwrap();
        assert_eq!(truncated.outcome, Some(ReportOutcome::Partial));
        let saved = truncated.result.as_ref().unwrap();
        assert!(saved.markdown.contains("sampling was bounded"));
        assert!(!saved
            .markdown
            .contains("missing complete saved daily coverage"));
        // Deleted recorded evidence is PARTIAL, framed as deleted evidence.
        let mut deleted = weekly_run_fixture();
        save_weekly_execution(
            &mut deleted,
            weekly_execution(ReportMode::MapReduce, false, vec![]),
            ReportEvidence {
                session_ids: vec![],
                missing_frame_ids: vec![11],
            },
        )
        .unwrap();
        assert_eq!(deleted.outcome, Some(ReportOutcome::Partial));
        assert!(deleted
            .result
            .as_ref()
            .unwrap()
            .markdown
            .contains("has since been deleted"));
        // Complete measured coverage is OK, with no banner at all.
        let mut complete = weekly_run_fixture();
        save_weekly_execution(
            &mut complete,
            weekly_execution(ReportMode::MapReduce, false, vec![]),
            complete_evidence(),
        )
        .unwrap();
        assert_eq!(complete.outcome, Some(ReportOutcome::Ok));
        let saved = complete.result.as_ref().unwrap();
        assert_eq!(saved.markdown, "Weekly synthesis body");
        assert_eq!(saved.model.as_deref(), Some("local-model"));
        assert_eq!(saved.periods_total, 7);
        assert_eq!(saved.periods_covered, 7);
        assert_eq!(saved.frames_summarized, 2);
    }

    #[test]
    fn daily_run_partial_wording_is_unchanged() {
        // The daily save path keeps its exact sampled-evidence wording (`03 §8c`);
        // the weekly banner change must not leak into daily output.
        let mut run = run_fixture();
        save_execution(
            &mut run,
            kernel::report_service::ReportExecution {
                output: ReportOutput {
                    markdown: "Observed activity".into(),
                    frame_ids: vec![1],
                    cited_frame_ids: vec![1],
                    mode: ReportMode::SinglePass,
                    periods_total: 1,
                    periods_covered: 1,
                    frames_sampled: 1,
                    frames_summarized: 1,
                    passes: 1,
                    truncated: true,
                },
                model: Some("local-model".into()),
            },
            ReportEvidence {
                session_ids: vec![],
                missing_frame_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(run.outcome, Some(ReportOutcome::Partial));
        assert_eq!(
            run.result.as_ref().unwrap().markdown,
            "**Partial coverage:** This report uses sampled evidence and does not cover all requested activity.\n\nObserved activity"
        );
    }

    #[tokio::test]
    async fn weekly_runs_skip_note_and_index_with_receipts() {
        // `03 §8e`: weekly notes are the weekly writer's (G3) surface. Until they
        // exist, a weekly run records skipped vault/index receipts and touches no
        // files — it must never write a daily-shaped note.
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let config = RunSnapshot {
            version: 1,
            memory: MemorySettings {
                output_folder: Some(path_string(dir.path())),
                ..Default::default()
            },
            report_settings: Settings::default(),
            model_at_enqueue: None,
        };
        let run = weekly_run_fixture();
        store.create_report_run(run.clone()).await.unwrap();
        write_files(&store, &run, &config).await.unwrap();
        let receipts = store.list_report_deliveries(&run.run_id).await.unwrap();
        assert_eq!(receipts.len(), 2);
        assert!(receipts
            .iter()
            .all(|item| item.state == ReportDeliveryState::Skipped));
        assert!(receipts
            .iter()
            .any(|item| item.destination == ReportDestination::Vault));
        assert!(receipts
            .iter()
            .any(|item| item.destination == ReportDestination::Index));
        assert!(
            !dir.path().join("ScreenSearch").exists(),
            "a weekly run must not write note files"
        );
        assert!(store.list_report_files().await.unwrap().is_empty());
        // A second recovery pass stays idempotent.
        write_files(&store, &run, &config).await.unwrap();
        assert_eq!(
            store
                .list_report_deliveries(&run.run_id)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn weekly_regenerate_linked_revision_keeps_kind() {
        // Regenerate mirrors daily (`03 §8e`): the linked revision keeps kind, schedule
        // identity and the explicit UTC window, and `(schedule_id, window)` still
        // reuses the stored run for retries — `kind` is not a dedup dimension.
        let store = SqliteStore::open_in_memory().unwrap();
        let base = weekly_run_fixture();
        store.create_report_run(base.clone()).await.unwrap();
        let mut revision = base.clone();
        revision.run_id = uuid();
        revision.revision_of = Some(base.run_id.clone());
        revision.phase = ReportRunPhase::Queued;
        revision.outcome = None;
        revision.result = None;
        let stored = store.create_report_run(revision.clone()).await.unwrap();
        assert_eq!(stored.run_id, revision.run_id);
        assert_eq!(stored.kind, ReportRunKind::WeeklySynthesis);
        assert_eq!(stored.schedule_id, base.schedule_id);
        assert_eq!(stored.window_start, base.window_start);
        assert_eq!(stored.window_end, base.window_end);
        assert_eq!(stored.timezone, "Europe/Paris");
        let mut duplicate = base.clone();
        duplicate.run_id = uuid();
        let reused = store.create_report_run(duplicate).await.unwrap();
        assert_eq!(reused.run_id, base.run_id);
    }
}
