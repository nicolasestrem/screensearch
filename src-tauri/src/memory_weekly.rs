//! Weekly synthesis scheduling (`03 §8e`, G5).
//!
//! This is a child of [`crate::memory`], not a parallel scheduler: the parent keeps
//! the daily lane, the report worker, file writing and delivery, and this module owns
//! only the weekly settings, activation/cursor state, catch-up planning and the
//! weekly run-now path. It is deliberately usable while the daily schedule is off —
//! every decision here is gated by the weekly fields alone.
//!
//! Persistence (no schema migration):
//! - `memory.weekly.enabled`, `memory.weekly.local_time`,
//!   `memory.weekly.timezone`, `memory.telegram.weekly_enabled` — weekly settings,
//!   stored beside the daily `memory.schedule.*` / `memory.telegram.*` keys. The
//!   consent key is deliberately in the `memory.telegram.*` namespace, matching the
//!   shared credential/chat and the parent's independent weekly opt-in read.
//! - `memory.weekly.state` — this schedule's own `schedule_id`, activation instant
//!   and oldest-first cursor. It is independent of the daily `memory.schedule.state`.
//! - `memory.weekly.gaps` — a disjoint, sorted `Vec<TimeRange>` of complete weeks
//!   skipped by the backfill cap. It is a separate key so re-activation or a time/
//!   timezone change never erases gap history, and it is never flattened into one
//!   `[min(start), max(end)]` span that would falsely include covered weeks.
//!
//! Durability order in [`schedule_due`]:
//! 1. record a newly discovered gap, then advance the cursor past it;
//! 2. for each window oldest-first, enqueue the run (the store deduplicates the
//!    weekly `(schedule_id, window)` identity) and only then persist the new cursor.
//!
//! A crash between steps re-derives the same windows on restart: the store returns
//! the existing run, then the cursor advances. The cursor never claims a week whose
//! enqueue did not happen.
//!
//! Weekly runs cannot go through the parent `enqueue` helper unchanged: that helper
//! stamps `run.timezone` from `config.timezone`, while a weekly run must record
//! `config.weekly_timezone`. Cloning the saved settings and overwriting `timezone`
//! would corrupt the run's `config_snapshot` (it would claim the daily schedule was
//! retimed). [`enqueue_weekly`] therefore builds the same persisted [`ReportRun`]
//! through the parent's `snapshot` and the store, setting only `run.timezone` to
//! the selected weekly zone; `config_snapshot.memory.timezone` keeps the daily zone.

use super::*;

/// Weekly settings keys, mirroring the daily `memory.schedule.*` names.
const WEEKLY_ENABLED: &str = "memory.weekly.enabled";
const WEEKLY_LOCAL_TIME: &str = "memory.weekly.local_time";
const WEEKLY_TIMEZONE: &str = "memory.weekly.timezone";
const WEEKLY_TELEGRAM_ENABLED: &str = "memory.telegram.weekly_enabled";
/// This schedule's activation/cursor state; never shared with the daily schedule.
pub(super) const STATE_KEY: &str = "memory.weekly.state";
/// Disjoint missed-week history, kept across activation and timing changes.
pub(super) const GAPS_KEY: &str = "memory.weekly.gaps";

/// Weekly activation and catch-up cursor. Unlike [`traits::ReportScheduleState`] this
/// carries no aggregate gap fields: gaps are disjoint ranges in [`GAPS_KEY`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct WeeklyScheduleState {
    schedule_id: String,
    activated_at: i64,
    #[serde(default)]
    last_window_end: Option<i64>,
}

/// The weekly setting pairs saved in the same batch as the daily settings.
pub(super) fn settings_values(
    settings: &MemorySettings,
) -> Result<Vec<(String, String)>, ReportError> {
    Ok(vec![
        (
            WEEKLY_ENABLED.into(),
            serde_json::to_string(&settings.weekly_enabled).map_err(storage)?,
        ),
        (
            WEEKLY_LOCAL_TIME.into(),
            serde_json::to_string(&settings.weekly_local_time).map_err(storage)?,
        ),
        (
            WEEKLY_TIMEZONE.into(),
            serde_json::to_string(&settings.weekly_timezone).map_err(storage)?,
        ),
        (
            WEEKLY_TELEGRAM_ENABLED.into(),
            serde_json::to_string(&settings.weekly_telegram_enabled).map_err(storage)?,
        ),
    ])
}

/// Weekly-only validation. `manual` is the Run-now contract: a selected weekly
/// timezone and the shared output folder, but neither an enabled schedule nor a
/// configured fire time. Enabling additionally requires a valid local time and the
/// folder; saved values are validated even while the schedule is off so a broken
/// draft cannot be persisted silently.
pub(super) fn validate(config: &MemorySettings, manual: bool) -> Result<(), ReportError> {
    if let Some(zone) = &config.weekly_timezone {
        if zone.parse::<chrono_tz::Tz>().is_err() {
            return Err(fail(
                "INVALID_CONFIG",
                "Select a valid weekly IANA time zone.",
            ));
        }
    }
    if let Some(time) = &config.weekly_local_time {
        kernel::memory_schedule::validate(config.weekly_timezone.as_deref().unwrap_or("UTC"), time)
            .map_err(|_| {
                fail(
                    "INVALID_CONFIG",
                    "Weekly schedule time must be HH:mm in a valid time zone.",
                )
            })?;
    }
    if let Some(folder) = &config.output_folder {
        if !native_path(Path::new(folder)).is_absolute() {
            return Err(fail("INVALID_CONFIG", "Select an absolute output folder."));
        }
    }
    if config.weekly_enabled && (config.weekly_timezone.is_none() || config.output_folder.is_none())
    {
        return Err(fail(
            "INVALID_CONFIG",
            "Select a weekly time zone and the shared output folder first.",
        ));
    }
    if config.weekly_enabled && config.weekly_local_time.is_none() {
        return Err(fail("INVALID_CONFIG", "Select a weekly synthesis time."));
    }
    if manual && (config.weekly_timezone.is_none() || config.output_folder.is_none()) {
        return Err(fail(
            "INVALID_CONFIG",
            "Select and save a weekly timezone and shared output folder before running a weekly synthesis.",
        ));
    }
    if config.weekly_telegram_enabled
        && config
            .telegram_chat_id
            .as_deref()
            .is_none_or(|id| id.trim().is_empty())
    {
        return Err(fail(
            "INVALID_CONFIG",
            "Select a Telegram chat before enabling weekly delivery.",
        ));
    }
    Ok(())
}

/// A new activation resets only this schedule's identity/cursor. Enabling for the
/// first time and changing the weekly fire time or zone both restart the "first
/// complete week after activation" rule; saving unchanged weekly settings preserves
/// the existing activation. Gap history lives in [`GAPS_KEY`] and is intentionally
/// left untouched.
#[cfg(test)]
fn activation_reset(
    old: &MemorySettings,
    new: &MemorySettings,
) -> Result<Option<(String, String)>, ReportError> {
    activation_reset_at(old, new, now_ms())
}

fn activation_reset_at(
    old: &MemorySettings,
    new: &MemorySettings,
    now: i64,
) -> Result<Option<(String, String)>, ReportError> {
    let preserve = !new.weekly_enabled
        || (old.weekly_enabled
            && old.weekly_local_time == new.weekly_local_time
            && old.weekly_timezone == new.weekly_timezone);
    if preserve {
        return Ok(None);
    }
    let state = WeeklyScheduleState {
        schedule_id: uuid(),
        activated_at: now,
        last_window_end: None,
    };
    Ok(Some((
        STATE_KEY.into(),
        serde_json::to_string(&state).map_err(storage)?,
    )))
}

/// Save activation and newly skipped *closed* weeks in the same settings batch.
/// A timing change must not silently discard overdue weeks from the previous lane.
pub(super) async fn activation_values(
    store: &dyn Store,
    old: &MemorySettings,
    new: &MemorySettings,
    now: i64,
) -> Result<Vec<(String, String)>, ReportError> {
    let Some(reset) = activation_reset_at(old, new, now)? else {
        return Ok(vec![]);
    };
    let mut values = vec![reset];
    // Only an enabled prior lane can have missed weeks: a re-enable starts a future
    // activation period and never backfills or claims the disabled interval (`03 §8c`).
    if !old.weekly_enabled {
        return Ok(values);
    }
    if let (Some(previous), Some(zone), Some(time)) = (
        load_state(store).await?,
        old.weekly_timezone.as_deref(),
        old.weekly_local_time.as_deref(),
    ) {
        let first = kernel::memory_schedule::weekly_first_window(zone, time, previous.activated_at)
            .map_err(storage)?;
        let start = previous
            .last_window_end
            .unwrap_or(first.start)
            .max(first.start);
        let end = kernel::memory_schedule::weekly_previous(zone, now)
            .map_err(storage)?
            .end;
        let mut history = gaps(store).await?;
        if merge_gap(&mut history, (start, end)) {
            values.push((
                GAPS_KEY.into(),
                serde_json::to_string(&history).map_err(storage)?,
            ));
        }
    }
    Ok(values)
}

/// First eligible week preview. The label is the weekly `local_date` contract
/// (`YYYY-Www`, matching the G2 engine fixture and `memory_files::weekly_note_*`),
/// never a Monday date.
pub(super) fn preview(
    config: &MemorySettings,
    now: i64,
) -> Result<Option<MemorySchedulePreview>, ReportError> {
    validate(config, false)?;
    let (Some(zone), Some(time)) = (
        config.weekly_timezone.as_deref(),
        config.weekly_local_time.as_deref(),
    ) else {
        return Ok(None);
    };
    let window = kernel::memory_schedule::weekly_first_window(zone, time, now).map_err(|_| {
        fail(
            "INVALID_CONFIG",
            "The weekly schedule date could not be resolved.",
        )
    })?;
    Ok(Some(MemorySchedulePreview {
        local_date: window.label,
        window_start: window.start,
        window_end: window.end,
        due_at: window.due_at,
    }))
}

/// Run the most recent complete ISO week now, independent of the schedule's enabled
/// flag. Requires the saved weekly timezone and shared output folder, not a fire
/// time. A queued/running manual weekly run for the same window is reused so a
/// double click does not duplicate work.
pub(super) async fn run_now(state: &AppState, app: &AppHandle) -> Result<ReportRun, ReportError> {
    let store = db(state)?;
    let runtime = &state.memory_runtime;
    let _guard = runtime.control.lock().await;
    check_open(runtime)?;
    let config = settings(store.as_ref()).await?;
    validate(&config, true)?;
    let timezone = config
        .weekly_timezone
        .as_deref()
        .ok_or_else(|| fail("INVALID_CONFIG", "Select a weekly time zone first."))?;
    let window = kernel::memory_schedule::weekly_previous(timezone, now_ms()).map_err(|_| {
        fail(
            "INVALID_CONFIG",
            "The most recent complete week could not be resolved.",
        )
    })?;
    for run in store.list_pending_report_runs().await.map_err(storage)? {
        if run.kind == ReportRunKind::WeeklySynthesis
            && run.schedule_id.is_none()
            && run.revision_of.is_none()
            && run.requested_range.is_none()
            && run.window_start == window.start
            && run.window_end == window.end
            && matches!(run.phase, ReportRunPhase::Queued | ReportRunPhase::Running)
        {
            return Ok(run);
        }
    }
    let run = enqueue_weekly(state, &config, &window, None).await?;
    runtime.wake.notify_one();
    notify(app);
    Ok(run)
}

/// Catch up due weeks oldest-first, recording skipped weeks as disjoint gap ranges
/// and advancing the cursor only after each enqueue succeeds. The parent calls this
/// every worker turn, whether or not the daily schedule is enabled.
pub(super) async fn schedule_due(state: &AppState) -> Result<(), ReportError> {
    let store = db(state)?;
    let _guard = state.memory_runtime.control.lock().await;
    check_open(&state.memory_runtime)?;
    let config = settings(store.as_ref()).await?;
    if !config.weekly_enabled {
        return Ok(());
    }
    let config_snapshot = snapshot(state, config.clone()).await?;
    catch_up(store.as_ref(), &config, &config_snapshot, now_ms()).await
}

// The caller holds the control lock. This real Store path queues but never generates or sends.
async fn catch_up(
    store: &dyn Store,
    config: &MemorySettings,
    config_snapshot: &str,
    now: i64,
) -> Result<(), ReportError> {
    if !config.weekly_enabled {
        return Ok(());
    }
    validate(config, false)?;
    let timezone = config
        .weekly_timezone
        .as_deref()
        .ok_or_else(|| fail("INVALID_CONFIG", "Select a weekly time zone first."))?;
    let local_time = config
        .weekly_local_time
        .as_deref()
        .ok_or_else(|| fail("INVALID_CONFIG", "Select a weekly synthesis time first."))?;
    let mut weekly = load_state(store).await?.ok_or_else(|| {
        fail(
            "INVALID_CONFIG",
            "Weekly schedule activation state is missing; disable and enable the weekly schedule again.",
        )
    })?;
    let due = kernel::memory_schedule::weekly_due_windows(
        timezone,
        local_time,
        weekly.activated_at,
        weekly.last_window_end,
        now,
    )
    .map_err(|error| {
        // Keep the kernel's distinct diagnostics (e.g. the future-cursor clock fault)
        // visible instead of flattening every window failure into one generic message.
        fail(
            "INVALID_CONFIG",
            &format!("The weekly schedule window could not be resolved: {error}"),
        )
    })?;
    if let Some(gap) = due.gap {
        // Make the miss visible before the cursor skips it. If the process stops
        // now, the next pass recomputes and de-duplicates the same range.
        record_gap(store, gap).await?;
        weekly.last_window_end = Some(gap.1);
        save_state(store, &weekly).await?;
    }
    for window in due.windows {
        let schedule_id = weekly.schedule_id.clone();
        let end = window.end;
        let run = enqueue_with_snapshot(store, config, &window, Some(schedule_id), config_snapshot)
            .await?;
        // Enqueue-before-cursor: the store's weekly `(schedule_id, window)` identity
        // makes a retry after a crash idempotent; the cursor advances only now.
        weekly.last_window_end = Some(end);
        save_state(store, &weekly).await?;
        tracing::info!(
            run_id = %run.run_id,
            window_start = run.window_start,
            window_end = run.window_end,
            label = %run.local_date,
            "weekly synthesis enqueued"
        );
    }
    Ok(())
}

/// Disjoint missed-week ranges, independent of the daily lane's `missed_windows`.
pub(super) async fn gaps(store: &dyn Store) -> Result<Vec<TimeRange>, ReportError> {
    setting(store, GAPS_KEY).await
}

/// Latest weekly run and the disjoint missed-week ranges, read independently of the
/// daily lane's `latest_run` / `missed_windows`.
pub(super) async fn status(
    store: &dyn Store,
) -> Result<(Option<ReportRun>, Vec<TimeRange>), ReportError> {
    let latest = store
        .list_report_runs(1, Some(ReportRunKind::WeeklySynthesis))
        .await
        .map_err(storage)?
        .into_iter()
        .next();
    Ok((latest, gaps(store).await?))
}

/// Identity reset remains a hard stop while either weekly consent is on, exactly as
/// the daily flags stop it: resetting the source identity under a live schedule is
/// never a silent background mutation.
pub(super) fn identity_blocked(config: &MemorySettings) -> bool {
    config.weekly_enabled || config.weekly_telegram_enabled
}

/// The weekly persisted run shape. `run.timezone` is the selected weekly zone; the
/// `config_snapshot` is the unmodified saved settings, so the daily timezone remains
/// truthful. `local_date` is the ISO week label, matching the G2 fixture and the
/// writer's `week_label` re-derivation.
async fn enqueue_weekly(
    state: &AppState,
    config: &MemorySettings,
    window: &kernel::memory_schedule::WeeklyWindow,
    schedule_id: Option<String>,
) -> Result<ReportRun, ReportError> {
    let store = db(state)?;
    let config_snapshot = snapshot(state, config.clone()).await?;
    enqueue_with_snapshot(
        store.as_ref(),
        config,
        window,
        schedule_id,
        &config_snapshot,
    )
    .await
}

async fn enqueue_with_snapshot(
    store: &dyn Store,
    config: &MemorySettings,
    window: &kernel::memory_schedule::WeeklyWindow,
    schedule_id: Option<String>,
    config_snapshot: &str,
) -> Result<ReportRun, ReportError> {
    let timezone = config.weekly_timezone.clone().ok_or_else(|| {
        fail(
            "INVALID_CONFIG",
            "Select and save a weekly timezone before running a weekly synthesis.",
        )
    })?;
    let now = now_ms();
    let run = ReportRun {
        requested_range: None,
        run_id: uuid(),
        source_id: source(store).await?,
        kind: ReportRunKind::WeeklySynthesis,
        schedule_id,
        revision_of: None,
        window_start: window.start,
        window_end: window.end,
        timezone,
        local_date: window.label.clone(),
        phase: ReportRunPhase::Queued,
        outcome: None,
        error: None,
        result: None,
        config_snapshot: config_snapshot.to_owned(),
        created_at: now,
        updated_at: now,
    };
    store.create_report_run(run).await.map_err(storage)
}

async fn load_state(store: &dyn Store) -> Result<Option<WeeklyScheduleState>, ReportError> {
    setting(store, STATE_KEY).await
}

async fn save_state(store: &dyn Store, weekly: &WeeklyScheduleState) -> Result<(), ReportError> {
    store
        .set_setting(STATE_KEY, &serde_json::to_string(weekly).map_err(storage)?)
        .await
        .map_err(storage)
}

/// Merge one missed span into the disjoint history. Touching or overlapping ranges
/// coalesce; ranges separated by a covered week never do. The history is a separate
/// key, so this survives activation and timezone changes.
async fn record_gap(store: &dyn Store, gap: (i64, i64)) -> Result<(), ReportError> {
    let mut gaps: Vec<TimeRange> = setting(store, GAPS_KEY).await?;
    if merge_gap(&mut gaps, gap) {
        store
            .set_setting(GAPS_KEY, &serde_json::to_string(&gaps).map_err(storage)?)
            .await
            .map_err(storage)?;
    }
    Ok(())
}

/// Adds `[start, end)` to a sorted disjoint list. Returns whether the stored history
/// changed (a fully contained span does not rewrite it).
fn merge_gap(gaps: &mut Vec<TimeRange>, gap: (i64, i64)) -> bool {
    if gap.0 >= gap.1 {
        return false;
    }
    gaps.sort_by_key(|range| (range.start, range.end));
    let before = gaps.clone();
    let mut merged = Vec::with_capacity(gaps.len() + 1);
    let (mut start, mut end) = gap;
    let mut inserted = false;
    for existing in gaps.iter().copied() {
        if existing.end < start {
            merged.push(existing);
        } else if existing.start > end {
            if !inserted {
                merged.push(TimeRange { start, end });
                inserted = true;
            }
            merged.push(existing);
        } else {
            start = start.min(existing.start);
            end = end.max(existing.end);
        }
    }
    if !inserted {
        merged.push(TimeRange { start, end });
    }
    *gaps = merged;
    *gaps != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis()
    }

    #[tokio::test]
    async fn weekly_catchup_persists_real_runs_and_replays_enqueue_before_cursor() {
        let store = SqliteStore::open_in_memory().unwrap();
        let config = weekly_config();
        assert!(
            !config.enabled,
            "weekly scheduling must work with daily disabled"
        );
        let activation = stamp("2025-01-01T12:00:00Z");
        let weekly = WeeklyScheduleState {
            schedule_id: "weekly-store-test".into(),
            activated_at: activation,
            last_window_end: None,
        };
        save_state(&store, &weekly).await.unwrap();
        let daily = ReportScheduleState {
            schedule_id: "daily-store-test".into(),
            activated_at: activation,
            last_window_end: None,
            gap_start: None,
            gap_end: None,
        };
        let daily_json = serde_json::to_string(&daily).unwrap();
        store
            .set_setting(SCHEDULE_STATE, &daily_json)
            .await
            .unwrap();
        let saved = serde_json::to_string(&RunSnapshot {
            version: 1,
            memory: config.clone(),
            report_settings: kernel::settings::load_settings(&store).await,
            model_at_enqueue: None,
        })
        .unwrap();
        let now = stamp("2026-02-02T12:00:00Z");
        catch_up(&store, &config, &saved, now).await.unwrap();
        let mut runs = store
            .list_report_runs(10, Some(ReportRunKind::WeeklySynthesis))
            .await
            .unwrap();
        runs.sort_by_key(|run| run.window_start);
        assert_eq!(runs.len(), 4);
        assert_eq!(
            runs.iter()
                .map(|run| run.local_date.as_str())
                .collect::<Vec<_>>(),
            ["2026-W02", "2026-W03", "2026-W04", "2026-W05"]
        );
        assert!(runs.iter().all(|run| run.phase == ReportRunPhase::Queued
            && run.schedule_id.as_deref() == Some("weekly-store-test")
            && run.timezone == "Europe/Paris"));
        let gaps_before = gaps(&store).await.unwrap();
        assert_eq!(gaps_before.len(), 1);
        assert_eq!(gaps_before[0].end, runs[0].window_start);
        assert_eq!(
            load_state(&store).await.unwrap().unwrap().last_window_end,
            Some(runs[3].window_end)
        );
        assert_eq!(
            store.get_setting(SCHEDULE_STATE).await.unwrap().as_deref(),
            Some(daily_json.as_str())
        );
        let ids_before: Vec<_> = runs.iter().map(|run| run.run_id.clone()).collect();
        // The enqueue committed, but its cursor checkpoint was lost on a crash.
        let rewound = WeeklyScheduleState {
            last_window_end: Some(runs[0].window_start),
            ..weekly
        };
        save_state(&store, &rewound).await.unwrap();
        catch_up(&store, &config, &saved, now).await.unwrap();
        let mut replayed = store
            .list_report_runs(10, Some(ReportRunKind::WeeklySynthesis))
            .await
            .unwrap();
        replayed.sort_by_key(|run| run.window_start);
        assert_eq!(
            replayed
                .iter()
                .map(|run| run.run_id.clone())
                .collect::<Vec<_>>(),
            ids_before
        );
        assert_eq!(gaps(&store).await.unwrap(), gaps_before);
        assert!(replayed.iter().all(|run| run.config_snapshot == saved));
    }

    #[tokio::test]
    async fn weekly_catchup_surfaces_the_future_cursor_message() {
        let store = SqliteStore::open_in_memory().unwrap();
        let config = weekly_config();
        let weekly = WeeklyScheduleState {
            schedule_id: "weekly-future-cursor".into(),
            activated_at: stamp("2025-01-01T12:00:00Z"),
            last_window_end: Some(stamp("2099-01-07T23:00:00Z")),
        };
        save_state(&store, &weekly).await.unwrap();
        let saved = serde_json::to_string(&RunSnapshot {
            version: 1,
            memory: config.clone(),
            report_settings: kernel::settings::load_settings(&store).await,
            model_at_enqueue: None,
        })
        .unwrap();
        let now = stamp("2026-02-02T12:00:00Z");
        let error = catch_up(&store, &config, &saved, now).await.unwrap_err();
        assert_eq!(error.code, "INVALID_CONFIG");
        assert!(
            error
                .message
                .contains("weekly cursor is ahead of the current clock"),
            "the distinct future-cursor diagnostic must reach the user: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn weekly_timing_change_records_skipped_closed_weeks_as_gap() {
        let store = SqliteStore::open_in_memory().unwrap();
        let old = weekly_config();
        let previous = WeeklyScheduleState {
            schedule_id: "old-weekly".into(),
            activated_at: stamp("2026-09-28T00:00:00Z"),
            last_window_end: Some(stamp("2026-10-11T22:00:00Z")),
        };
        save_state(&store, &previous).await.unwrap();
        let mut new = old.clone();
        new.weekly_local_time = Some("09:00".into());
        let now = stamp("2026-10-28T12:00:00Z");
        let mut values = settings_values(&new).unwrap();
        values.extend(activation_values(&store, &old, &new, now).await.unwrap());
        store.set_settings_batch(&values).await.unwrap();
        let missed = gaps(&store).await.unwrap();
        assert_eq!(
            missed,
            vec![TimeRange {
                start: stamp("2026-10-11T22:00:00Z"),
                end: stamp("2026-10-25T23:00:00Z"),
            }]
        );
        assert!(
            missed[0].end <= now,
            "the open activation week is not already missed"
        );
        let reset = load_state(&store).await.unwrap().unwrap();
        assert_ne!(reset.schedule_id, previous.schedule_id);
        assert_eq!(reset.activated_at, now);
        assert!(reset.last_window_end.is_none());
    }

    #[tokio::test]
    async fn weekly_reenable_does_not_record_disabled_weeks_as_missed() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut old = weekly_config();
        old.weekly_enabled = false;
        // A disable keeps the prior state, time and zone; only the enable flag changes.
        let previous = WeeklyScheduleState {
            schedule_id: "disabled-weekly".into(),
            activated_at: stamp("2026-08-03T00:00:00Z"),
            last_window_end: Some(stamp("2026-08-30T22:00:00Z")),
        };
        save_state(&store, &previous).await.unwrap();
        let new = weekly_config();
        let now = stamp("2026-10-28T12:00:00Z");
        let mut values = settings_values(&new).unwrap();
        values.extend(activation_values(&store, &old, &new, now).await.unwrap());
        store.set_settings_batch(&values).await.unwrap();
        assert!(
            gaps(&store).await.unwrap().is_empty(),
            "weeks inside an intentionally disabled period are not missed"
        );
        let reset = load_state(&store).await.unwrap().unwrap();
        assert_ne!(reset.schedule_id, previous.schedule_id);
        assert_eq!(reset.activated_at, now);
        assert!(reset.last_window_end.is_none());
    }

    fn notes_folder() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    fn weekly_config() -> MemorySettings {
        MemorySettings {
            weekly_enabled: true,
            weekly_local_time: Some("08:00".into()),
            weekly_timezone: Some("Europe/Paris".into()),
            output_folder: Some(notes_folder()),
            ..Default::default()
        }
    }

    fn run_fixture(run_id: &str, kind: ReportRunKind, created_at: i64) -> ReportRun {
        ReportRun {
            requested_range: None,
            run_id: run_id.into(),
            source_id: "source-fixture".into(),
            kind,
            schedule_id: None,
            revision_of: None,
            window_start: stamp("2026-10-11T22:00:00Z"),
            window_end: stamp("2026-10-18T22:00:00Z"),
            timezone: "Europe/Paris".into(),
            local_date: if kind == ReportRunKind::WeeklySynthesis {
                "2026-W42".into()
            } else {
                "2026-10-12".into()
            },
            phase: ReportRunPhase::Finished,
            outcome: Some(ReportOutcome::NoData),
            error: None,
            result: None,
            config_snapshot: "{}".into(),
            created_at,
            updated_at: created_at,
        }
    }

    #[tokio::test]
    async fn weekly_settings_round_trip_preserves_daily_fields() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .set_setting("memory.schedule.timezone", "\"Europe/Paris\"")
            .await
            .unwrap();
        let mut settings = weekly_config();
        settings.weekly_telegram_enabled = true;
        store
            .set_settings_batch(&settings_values(&settings).unwrap())
            .await
            .unwrap();

        let enabled: bool = setting(&store, WEEKLY_ENABLED).await.unwrap();
        let local_time: Option<String> = setting(&store, WEEKLY_LOCAL_TIME).await.unwrap();
        let timezone: Option<String> = setting(&store, WEEKLY_TIMEZONE).await.unwrap();
        let telegram: bool = setting(&store, WEEKLY_TELEGRAM_ENABLED).await.unwrap();
        assert!(enabled);
        assert_eq!(local_time.as_deref(), Some("08:00"));
        assert_eq!(timezone.as_deref(), Some("Europe/Paris"));
        assert!(telegram);
        assert_eq!(
            store.get_setting("memory.schedule.timezone").await.unwrap(),
            Some("\"Europe/Paris\"".into()),
            "the daily timezone is untouched"
        );
        assert!(store.get_setting(STATE_KEY).await.unwrap().is_none());
    }

    #[test]
    fn weekly_activation_resets_only_when_enabling_or_timing_changes() {
        let base = weekly_config();
        assert_eq!(activation_reset(&base, &base).unwrap(), None);

        let mut timed = base.clone();
        timed.weekly_local_time = Some("09:30".into());
        let (key, value) = activation_reset(&base, &timed).unwrap().unwrap();
        assert_eq!(key, STATE_KEY);
        let state: WeeklyScheduleState = serde_json::from_str(&value).unwrap();
        assert!(!state.schedule_id.is_empty());
        assert!(state.activated_at > 0);
        assert_eq!(state.last_window_end, None);

        let mut zoned = base.clone();
        zoned.weekly_timezone = Some("America/New_York".into());
        assert!(activation_reset(&base, &zoned).unwrap().is_some());

        let mut disabled = base.clone();
        disabled.weekly_enabled = false;
        assert_eq!(activation_reset(&base, &disabled).unwrap(), None);

        let fresh = weekly_config();
        assert!(activation_reset(&MemorySettings::default(), &fresh)
            .unwrap()
            .is_some());

        let mut daily_only = base.clone();
        daily_only.local_time = Some("07:00".into());
        daily_only.timezone = Some("Etc/UTC".into());
        assert_eq!(activation_reset(&base, &daily_only).unwrap(), None);
    }

    #[test]
    fn weekly_validation_is_independent_of_the_daily_schedule() {
        // Run now: weekly zone + shared folder, daily schedule off, no weekly time.
        let manual = MemorySettings {
            weekly_timezone: Some("Europe/Paris".into()),
            output_folder: Some(notes_folder()),
            ..Default::default()
        };
        assert!(validate(&manual, true).is_ok());
        assert!(validate(&manual, false).is_ok());

        let mut no_zone = manual.clone();
        no_zone.weekly_timezone = None;
        let error = validate(&no_zone, true).unwrap_err();
        assert_eq!(error.code, "INVALID_CONFIG");
        assert!(error.message.contains("weekly timezone"));

        // Enabling requires the time; run now does not.
        let mut enable_no_time = manual.clone();
        enable_no_time.weekly_enabled = true;
        assert!(validate(&enable_no_time, false)
            .unwrap_err()
            .message
            .contains("weekly synthesis time"));

        let mut enable_no_folder = manual.clone();
        enable_no_folder.weekly_enabled = true;
        enable_no_folder.weekly_local_time = Some("08:00".into());
        enable_no_folder.output_folder = None;
        assert!(validate(&enable_no_folder, false).is_err());

        let mut bad_zone = manual.clone();
        bad_zone.weekly_timezone = Some("Mars/Olympus".into());
        assert!(validate(&bad_zone, true).is_err());

        let mut bad_time = manual.clone();
        bad_time.weekly_timezone = Some("Etc/UTC".into());
        bad_time.weekly_local_time = Some("8:00".into());
        assert!(validate(&bad_time, false).is_err());

        let mut relative = manual.clone();
        relative.output_folder = Some("relative/notes".into());
        assert!(validate(&relative, true).is_err());

        // Weekly Telegram consent needs the shared chat but not daily Telegram consent.
        let mut telegram = manual.clone();
        telegram.weekly_telegram_enabled = true;
        assert!(validate(&telegram, true).is_err());
        telegram.telegram_chat_id = Some("-100123".into());
        assert!(validate(&telegram, true).is_ok());
        assert!(!telegram.telegram_enabled);
    }

    #[test]
    fn weekly_preview_uses_the_first_complete_week_label() {
        let config = weekly_config();
        let preview = preview(&config, stamp("2026-10-17T12:00:00Z"))
            .unwrap()
            .unwrap();
        assert_eq!(preview.local_date, "2026-W43");
        assert!(preview.window_start < preview.window_end);
        assert!(preview.due_at >= preview.window_end);

        let blank = MemorySettings::default();
        assert_eq!(
            super::preview(&blank, stamp("2026-10-17T12:00:00Z")).unwrap(),
            None
        );
    }

    #[test]
    fn weekly_windows_convert_to_the_iso_week_label_without_retiming_daily_config() {
        // The G2 fixture's week: Monday 2026-10-12 is ISO 2026-W42, and the weekly
        // `local_date` is that label — not the Monday daily date.
        let window = kernel::memory_schedule::weekly_window(
            "Europe/Paris",
            "08:00",
            chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap(),
        )
        .unwrap();
        assert_eq!(window.label, "2026-W42");
        assert_eq!(window.start, stamp("2026-10-11T22:00:00Z"));
        assert_eq!(
            kernel::week_label("Europe/Paris", window.start).unwrap(),
            window.label
        );
        assert_ne!(
            window.label, "2026-10-12",
            "a weekly run is not a Monday daily"
        );
    }

    #[test]
    fn weekly_gap_history_stays_disjoint_and_never_spans_a_covered_week() {
        let mut gaps = Vec::new();
        assert!(merge_gap(
            &mut gaps,
            (stamp("2026-01-05T00:00:00Z"), stamp("2026-01-12T00:00:00Z"))
        ));
        assert!(merge_gap(
            &mut gaps,
            (stamp("2026-02-02T00:00:00Z"), stamp("2026-02-09T00:00:00Z"))
        ));
        assert_eq!(gaps.len(), 2);
        assert_eq!(
            gaps[0],
            TimeRange {
                start: stamp("2026-01-05T00:00:00Z"),
                end: stamp("2026-01-12T00:00:00Z"),
            }
        );

        // Overlap coalesces inside the first range; the second stays separate.
        assert!(merge_gap(
            &mut gaps,
            (stamp("2026-01-08T00:00:00Z"), stamp("2026-01-19T00:00:00Z"))
        ));
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].end, stamp("2026-01-19T00:00:00Z"));

        // A range touching both sides is one continuous uncovered span.
        assert!(merge_gap(
            &mut gaps,
            (stamp("2026-01-19T00:00:00Z"), stamp("2026-02-02T00:00:00Z"))
        ));
        assert_eq!(
            gaps,
            vec![TimeRange {
                start: stamp("2026-01-05T00:00:00Z"),
                end: stamp("2026-02-09T00:00:00Z"),
            }]
        );

        // Re-adding a covered span is idempotent.
        assert!(!merge_gap(
            &mut gaps,
            (stamp("2026-01-06T00:00:00Z"), stamp("2026-01-07T00:00:00Z"))
        ));
    }

    #[tokio::test]
    async fn weekly_state_and_disjoint_gaps_are_durable_and_survive_reactivation() {
        let store = SqliteStore::open_in_memory().unwrap();
        let state = WeeklyScheduleState {
            schedule_id: "weekly-schedule".into(),
            activated_at: stamp("2026-10-01T12:00:00Z"),
            last_window_end: Some(stamp("2026-10-12T00:00:00Z")),
        };
        save_state(&store, &state).await.unwrap();
        assert_eq!(load_state(&store).await.unwrap(), Some(state));

        record_gap(
            &store,
            (stamp("2026-09-07T00:00:00Z"), stamp("2026-09-14T00:00:00Z")),
        )
        .await
        .unwrap();
        record_gap(
            &store,
            (stamp("2026-09-21T00:00:00Z"), stamp("2026-09-28T00:00:00Z")),
        )
        .await
        .unwrap();
        let (_, gaps) = status(&store).await.unwrap();
        assert_eq!(gaps.len(), 2, "separately missed weeks stay separate");

        // Re-activation rewrites only state; history remains visible.
        let old = weekly_config();
        let mut enabled = old.clone();
        enabled.weekly_local_time = Some("09:00".into());
        let (key, value) = activation_reset(&old, &enabled).unwrap().unwrap();
        assert_eq!(key, STATE_KEY);
        store.set_setting(&key, &value).await.unwrap();
        let (_, gaps_after) = status(&store).await.unwrap();
        assert_eq!(gaps_after, gaps);
    }

    #[tokio::test]
    async fn weekly_status_keeps_the_weekly_lane_and_gap_independent_of_daily() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .create_report_run(run_fixture("daily-run", ReportRunKind::Daily, 100))
            .await
            .unwrap();
        let (latest, gaps) = status(&store).await.unwrap();
        assert!(latest.is_none(), "a daily run is not a weekly latest");
        assert!(gaps.is_empty());

        store
            .create_report_run(run_fixture(
                "weekly-run",
                ReportRunKind::WeeklySynthesis,
                200,
            ))
            .await
            .unwrap();
        record_gap(
            &store,
            (stamp("2026-08-03T00:00:00Z"), stamp("2026-08-10T00:00:00Z")),
        )
        .await
        .unwrap();
        let (latest, gaps) = status(&store).await.unwrap();
        assert_eq!(latest.unwrap().run_id, "weekly-run");
        assert_eq!(
            gaps,
            vec![TimeRange {
                start: stamp("2026-08-03T00:00:00Z"),
                end: stamp("2026-08-10T00:00:00Z"),
            }]
        );
    }

    #[test]
    fn identity_reset_is_blocked_by_either_weekly_consent() {
        assert!(!identity_blocked(&MemorySettings::default()));
        let enabled = MemorySettings {
            weekly_enabled: true,
            ..Default::default()
        };
        assert!(identity_blocked(&enabled));
        let telegram = MemorySettings {
            weekly_telegram_enabled: true,
            ..Default::default()
        };
        assert!(identity_blocked(&telegram));
    }
}
