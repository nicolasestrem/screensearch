//! Timer + idle producers for deferred vision tagging (`03 §5`). Vision is **never**
//! real-time: `vision_tag` jobs are only enqueued on a user-configured timer or when
//! the OS reports the user idle past a threshold — both opt-in, off by default
//! (`06_PATCH_PLAN`, `07` gap #1). On-demand tagging goes through
//! [`crate::Kernel::enqueue_vision`] instead.
//!
//! Each enabled trigger enqueues up to `enrich.vision_batch_size` still-untagged frames
//! per tick. A frame that already has an in-flight (`pending`/`running`) `vision_tag`
//! job is skipped — [`Store::untagged_frame_ids`] excludes it — so a slow batch is not
//! re-enqueued on the next tick and the timer/idle lanes don't double-queue the same
//! frames. (`insert_vision` is still an idempotent upsert, so a stray re-analyze after a
//! job leaves the queue can't corrupt data.)

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use traits::{BackfillControl, EnrichAdvanced, JobKind, NewJob, Store};

use crate::settings;
use crate::IdleSource;

/// How often the timer loop re-checks settings while the timer trigger is disabled.
const DISABLED_RECHECK: Duration = Duration::from_secs(30);
/// How often the idle loop samples the platform idle time while *not* draining a backlog.
const IDLE_POLL: Duration = Duration::from_secs(30);
/// How often the idle loop tops up the queue while it *is* actively draining a backlog —
/// short so the worker pool is never starved between batches during a keep-warm drain.
const DRAIN_POLL: Duration = Duration::from_secs(5);
/// Floor on the user-set timer interval, so a misconfigured tiny value can't busy-loop.
const MIN_TIMER_INTERVAL: Duration = Duration::from_secs(60);

/// Handle to the running scheduler tasks; stop them via [`SchedulerHandle::stop`].
pub(crate) struct SchedulerHandle {
    stop: watch::Sender<bool>,
    joins: Vec<JoinHandle<()>>,
}

impl SchedulerHandle {
    /// Signals the loops to stop and waits for them to finish.
    pub(crate) async fn stop(mut self) {
        let _ = self.stop.send(true);
        for join in std::mem::take(&mut self.joins) {
            let _ = join.await;
        }
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        // Safety net: if dropped without an explicit `stop`, still end the loops.
        let _ = self.stop.send(true);
    }
}

/// Spawns the timer loop (always) and, when an idle source is available, the idle loop.
/// The timer loop re-reads its `UI` toggle each tick, so enabling/disabling it in settings
/// takes effect without a restart; the batch size and the whole idle trigger are `file-only`
/// and fixed for this run (`03 §8`, 0.4.5 D6). `backfill` lets the idle loop keep the
/// sidecar warm while it drains the backlog (`None` when no inference supervisor is wired).
pub(crate) fn spawn(
    store: Arc<dyn Store>,
    advanced: EnrichAdvanced,
    idle: Option<IdleSource>,
    backfill: Option<Arc<dyn BackfillControl>>,
) -> SchedulerHandle {
    let (stop_tx, stop_rx) = watch::channel(false);
    // Serializes the timer and idle producers' read-then-enqueue. `untagged_frame_ids`
    // only excludes jobs already committed at query time, so without this a simultaneous
    // wake of both loops could each read the same eligible frames before either inserts
    // its jobs, double-queuing them (the exact timer+idle overlap this scheduler fixes).
    // Holding the gate across the whole read+enqueue makes the two producers mutually
    // atomic; the rare on-demand-vs-scheduler overlap stays bounded by `insert_vision`'s
    // idempotent upsert.
    let gate = Arc::new(Mutex::new(()));
    let mut joins = vec![tokio::spawn(timer_loop(
        store.clone(),
        advanced.clone(),
        gate.clone(),
        stop_rx.clone(),
    ))];
    if let Some(idle) = idle {
        joins.push(tokio::spawn(idle_loop(
            store, advanced, idle, gate, backfill, stop_rx,
        )));
    }
    SchedulerHandle {
        stop: stop_tx,
        joins,
    }
}

/// Enqueues up to `enrich.vision_batch_size` untagged frames every
/// `enrich.vision_timer_interval_ms`, while the timer trigger is enabled.
async fn timer_loop(
    store: Arc<dyn Store>,
    advanced: EnrichAdvanced,
    gate: Arc<Mutex<()>>,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        let s = settings::load_settings(store.as_ref()).await;
        let wait = if s.enrich_vision_timer_enabled {
            Duration::from_millis(s.enrich_vision_timer_interval_ms as u64).max(MIN_TIMER_INTERVAL)
        } else {
            DISABLED_RECHECK
        };
        if wait_or_stop(&mut stop, wait).await {
            break;
        }
        // Re-read the UI toggle in case it changed during the wait; the batch size is
        // `file-only` and fixed for this run (`03 §8`, 0.4.5 D6).
        let s = settings::load_settings(store.as_ref()).await;
        if s.enrich_vision_timer_enabled {
            enqueue_untagged(&store, &gate, "timer", advanced.vision_batch_size).await;
        }
    }
}

/// Drains the untagged-frame backlog while the user is idle, keeping the sidecar warm for
/// the whole drain instead of tagging one batch and going dormant. Each tick (faster while
/// draining): if the user is idle and a backlog remains, it tops the queue up to a batch
/// whenever in-flight vision work has fallen below a low watermark, and tells the sidecar
/// to stay loaded (`set_backfill_active(true)`). When the backlog is empty or the user
/// resumes, it clears keep-warm so the normal idle-TTL eviction frees the VRAM (`03 §5`).
async fn idle_loop(
    store: Arc<dyn Store>,
    advanced: EnrichAdvanced,
    idle: IdleSource,
    gate: Arc<Mutex<()>>,
    backfill: Option<Arc<dyn BackfillControl>>,
    mut stop: watch::Receiver<bool>,
) {
    let mut keep_warm = false;
    loop {
        // Poll faster while draining so the worker pool is never starved between batches.
        let poll = if keep_warm { DRAIN_POLL } else { IDLE_POLL };
        if wait_or_stop(&mut stop, poll).await {
            break;
        }
        if !advanced.vision_idle_enabled {
            set_keep_warm(&backfill, &mut keep_warm, false);
            continue;
        }
        let threshold_ms = u64::from(advanced.vision_idle_secs).saturating_mul(1000);
        let now_idle = idle().is_some_and(|idle_ms| idle_ms >= threshold_ms);
        if !now_idle {
            // User is back: stop holding the model warm; let the idle-TTL reclaim VRAM.
            set_keep_warm(&backfill, &mut keep_warm, false);
            continue;
        }
        // Idle. Throttle on in-flight vision work so we top the queue up rather than pile
        // batch on batch; while work is draining, keep the model warm.
        let pending = store.pending_vision_job_count().await.unwrap_or(0);
        let watermark = u64::from(advanced.vision_batch_size / 4).max(1);
        if pending >= watermark {
            set_keep_warm(&backfill, &mut keep_warm, true);
            continue;
        }
        // Queue is low: enqueue the next batch if any backlog remains.
        let found = enqueue_untagged(&store, &gate, "idle", advanced.vision_batch_size).await;
        // Backlog remains → keep warm; drained → release so the TTL frees VRAM.
        set_keep_warm(&backfill, &mut keep_warm, found > 0);
    }
    // On shutdown, never leave the sidecar pinned warm.
    set_keep_warm(&backfill, &mut keep_warm, false);
}

/// Flips the sidecar keep-warm hint, but only on a real change (the supervisor stores an
/// atomic, so this just avoids redundant churn / log noise).
fn set_keep_warm(backfill: &Option<Arc<dyn BackfillControl>>, keep_warm: &mut bool, active: bool) {
    if *keep_warm == active {
        return;
    }
    *keep_warm = active;
    if let Some(b) = backfill {
        b.set_backfill_active(active);
    }
}

/// Enqueues up to `batch` still-untagged frames (oldest first, excluding any with an
/// in-flight `vision_tag` job) as `vision_tag` jobs. Holds `gate` across the read and the
/// enqueue so a concurrent producer (the other of the timer/idle pair) can't read the same
/// frames before these jobs are committed. Returns how many candidate frames the read
/// found — the idle backfill uses a non-zero result as "backlog remains" to decide whether
/// to keep the model warm.
async fn enqueue_untagged(
    store: &Arc<dyn Store>,
    gate: &Mutex<()>,
    trigger: &str,
    batch: u32,
) -> usize {
    let _guard = gate.lock().await;
    let ids = match store.untagged_frame_ids(batch, None).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(trigger, error = %e, "vision scheduler: list untagged failed");
            return 0;
        }
    };
    let found = ids.len();
    let mut count = 0u64;
    for frame_id in ids {
        let job = NewJob {
            kind: JobKind::VisionTag,
            frame_id: Some(frame_id),
            priority: 0,
            max_attempts: 3,
            not_before: 0,
        };
        match store.enqueue_job(job).await {
            Ok(_) => count += 1,
            Err(e) => tracing::warn!(frame_id, error = %e, "vision scheduler: enqueue failed"),
        }
    }
    if count > 0 {
        tracing::info!(trigger, count, "vision scheduler enqueued vision_tag jobs");
    }
    found
}

/// Sleeps `dur`, returning early with `true` if a stop was signalled (so the caller
/// breaks its loop). Returns `true` if it should stop, `false` to continue.
async fn wait_or_stop(stop: &mut watch::Receiver<bool>, dur: Duration) -> bool {
    tokio::select! {
        biased;
        _ = stop.changed() => true,
        _ = tokio::time::sleep(dur) => *stop.borrow(),
    }
}
