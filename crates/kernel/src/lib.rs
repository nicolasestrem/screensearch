//! `kernel` — the orchestrator that owns the typed event bus and the capture
//! pipeline (`03 §1/§5`). It depends only on [`traits`] (the contracts), never on a
//! module's concrete impl; `src-tauri` wires impls in at startup via the capture
//! factory (the composition root, `03 §2`).
//!
//! P2 landed the always-on half: [`Kernel::start_capture`] spawns the capture loop
//! (CaptureSource → OcrProvider → Store → enrichment jobs → `capture_tick`). P3/P4
//! add the bounded enrichment worker pool that drains embedding and vision jobs as
//! their providers attach; P5 builds UI flows on top of these events and commands.

#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::{broadcast, mpsc, oneshot, watch, Mutex};
use tokio::task::JoinHandle;
use traits::{
    AnswerProvider, BackfillControl, CaptureConfig, CaptureNowReceiver, CaptureNowRequest,
    CaptureSource, ComponentReadiness, ComponentStatus, EmbeddingProvider, JobKind,
    ModelDownloadStatus, NewJob, OcrProvider, PressureProbe, Readiness, SessionSegmenter,
    SidecarState, SidecarStatus, Store, ThrottleStatus, Toast, ToastLevel, VisionProvider,
    VisionTarget,
};

mod capture_loop;
mod events;
pub mod reports;
pub mod resume;
pub mod sessions_intel;
mod sessions_scheduler;
pub mod settings;
mod throttle;
mod vision_scheduler;
mod worker_pool;

#[cfg(test)]
// These contract tests need private scheduler reconciliation helpers and the real store/provider
// dev dependencies; keeping them at the module seam avoids widening production visibility.
#[allow(clippy::items_after_test_module)]
mod sessions_scheduler_contract_tests {
    use traits::{
        NewSession, SegmenterFrame, Session, SessionDraft, SessionFilter, SessionHost, SessionKind,
    };

    use crate::sessions_scheduler::{
        match_drafts_to_existing, safe_backfill_cut, trim_drafts_against_frozen,
    };

    fn row(id: i64, start: i64, end: Option<i64>, key: &str, frozen: bool) -> Session {
        Session {
            id,
            started_at: start,
            ended_at: end,
            kind: if key.starts_with("ai:") {
                SessionKind::Ai
            } else {
                SessionKind::Focus
            },
            tool: key.strip_prefix("ai:").map(str::to_string),
            host: key.starts_with("ai:").then_some(SessionHost::Terminal),
            context_key: key.to_string(),
            title: None,
            summary: None,
            summary_model: None,
            confidence: 0.9,
            frozen,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn draft(start: i64, end: i64, key: &str, ids: &[i64]) -> SessionDraft {
        SessionDraft {
            started_at: start,
            ended_at: end,
            kind: if key.starts_with("ai:") {
                SessionKind::Ai
            } else {
                SessionKind::Focus
            },
            tool: key.strip_prefix("ai:").map(str::to_string),
            host: None,
            context_key: key.to_string(),
            confidence: 0.8,
            frame_ids: ids.to_vec(),
            open: false,
        }
    }

    fn frame(id: i64, at: i64) -> SegmenterFrame {
        SegmenterFrame {
            id,
            captured_at: at,
            app_hint: Some("app".to_string()),
            window_title: None,
            browser_url: None,
        }
    }

    async fn seed_continuous_backfill_track(store: &dyn traits::Store) -> Vec<(i64, i64)> {
        const HOUR: i64 = 3_600_000;
        let mut inserted = Vec::new();
        let mut index = 0_i64;
        // Keep this scheduler fixture continuous under the shipped 120-second micro-gap.
        for at in (0..=6 * HOUR + 30 * 60_000).step_by(60_000) {
            let id = store
                .insert_frame(traits::NewFrame {
                    captured_at: at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{at}.webp"),
                    content_hash: format!("backfill-{index}"),
                    app_hint: Some("codex".to_string()),
                    window_title: Some("Codex".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
            inserted.push((id, at));
            index += 1;
        }
        let after_gap = 7 * HOUR + 30 * 60_000;
        let id = store
            .insert_frame(traits::NewFrame {
                captured_at: after_gap,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: format!("frames/{after_gap}.webp"),
                content_hash: format!("backfill-{index}"),
                app_hint: Some("codex".to_string()),
                window_title: Some("Codex".to_string()),
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .unwrap();
        inserted.push((id, after_gap));
        inserted
    }

    #[test]
    fn frozen_guard_trims_only_the_same_identity_track() {
        let drafts = vec![
            draft(50, 200, "focus:notepad", &[1, 2, 3, 4]),
            draft(50, 200, "ai:codex", &[5, 6, 7, 8]),
        ];
        let frozen = vec![row(10, 0, Some(100), "focus:browser", true)];
        let frames = vec![
            frame(1, 50),
            frame(2, 99),
            frame(3, 100),
            frame(4, 200),
            frame(5, 50),
            frame(6, 99),
            frame(7, 100),
            frame(8, 200),
        ];

        let got = trim_drafts_against_frozen(drafts, &frozen, &frames);
        let focus = got
            .iter()
            .find(|draft| draft.kind == SessionKind::Focus)
            .unwrap();
        assert_eq!(focus.started_at, 200);
        assert_eq!(focus.frame_ids, vec![4]);
        let ai = got
            .iter()
            .find(|draft| draft.kind == SessionKind::Ai)
            .unwrap();
        assert_eq!(ai.started_at, 50, "cross-track overlap stays allowed");
    }

    #[test]
    fn frozen_guard_treats_equal_last_frame_timestamp_as_overlap() {
        let drafts = vec![draft(100, 200, "focus:notepad", &[1, 2])];
        let frozen = vec![row(10, 0, Some(100), "focus:browser", true)];
        let frames = vec![frame(1, 100), frame(2, 200)];

        let got = trim_drafts_against_frozen(drafts, &frozen, &frames);

        assert_eq!(got.len(), 1);
        assert_eq!(got[0].started_at, 200);
        assert_eq!(got[0].ended_at, 200);
        assert_eq!(got[0].frame_ids, vec![2]);
    }

    #[test]
    fn overlap_matching_reuses_one_unfrozen_id_per_draft() {
        let existing = vec![
            row(7, 0, None, "ai:codex", false),
            row(8, 500, Some(700), "ai:codex", false),
        ];
        let drafts = vec![
            draft(50, 400, "ai:codex", &[1]),
            draft(550, 750, "ai:codex", &[2]),
        ];
        let matched = match_drafts_to_existing(&drafts, &existing, 1_000);
        assert_eq!(matched, vec![Some(7), Some(8)]);
    }

    #[test]
    fn overlap_matching_does_not_reuse_a_merely_touching_session() {
        let existing = vec![row(7, 0, Some(100), "ai:codex", false)];
        let drafts = vec![draft(100, 200, "ai:codex", &[2])];

        assert_eq!(
            match_drafts_to_existing(&drafts, &existing, 1_000),
            vec![None]
        );
    }

    #[test]
    fn historical_cut_extends_to_the_next_global_merge_gap() {
        let frames = vec![frame(1, 0), frame(2, 100), frame(3, 200), frame(4, 1_000)];
        assert_eq!(safe_backfill_cut(&frames, 150, 500, 2_000), Some(1_000));
        assert_eq!(safe_backfill_cut(&frames[..3], 150, 500, 250), None);
    }

    #[test]
    fn historical_cut_never_skips_unscanned_future_frames() {
        assert_eq!(safe_backfill_cut(&[], 500, 100, 5_000), Some(500));
        assert_eq!(
            safe_backfill_cut(&[frame(1, 0), frame(2, 100)], 500, 100, 5_000),
            Some(500)
        );
    }

    #[tokio::test]
    async fn historical_backfill_retries_past_a_fixed_target_cutting_a_live_track() {
        const HOUR: i64 = 3_600_000;
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("backfill-restart.db");
        let store = store::SqliteStore::open_path(&db).unwrap();
        seed_continuous_backfill_track(&store).await;
        let engine = sessions::SessionEngine::new().unwrap();
        let settings = traits::Settings::default();
        let lookback = traits::SESSION_FREEZE_LOOKBACK_MS;
        crate::sessions_scheduler::run_backfill_chunk(
            &store,
            &engine,
            lookback + 6 * HOUR,
            &settings,
        )
        .await
        .unwrap();
        assert!(store
            .sessions_in_range(SessionFilter {
                now_ms: lookback + 6 * HOUR,
                ..SessionFilter::default()
            })
            .await
            .unwrap()
            .is_empty());

        drop(store);
        let store = store::SqliteStore::open_path(&db).unwrap();

        crate::sessions_scheduler::run_backfill_chunk(
            &store,
            &engine,
            lookback + 8 * HOUR,
            &settings,
        )
        .await
        .unwrap();
        let sessions = store
            .sessions_in_range(SessionFilter {
                now_ms: lookback + 8 * HOUR,
                ..SessionFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(sessions.len(), 1, "retry must finish without duplicates");
        assert!(sessions[0].frozen);

        crate::sessions_scheduler::run_backfill_chunk(
            &store,
            &engine,
            lookback + 8 * HOUR,
            &settings,
        )
        .await
        .unwrap();
        assert_eq!(
            store
                .sessions_in_range(SessionFilter {
                    now_ms: lookback + 8 * HOUR,
                    ..SessionFilter::default()
                })
                .await
                .unwrap()
                .len(),
            1,
            "completed retry must be idempotent"
        );
    }

    #[tokio::test]
    async fn historical_backfill_reuses_an_exact_unfrozen_partial_row() {
        const HOUR: i64 = 3_600_000;
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("backfill-partial-row.db");
        let store = store::SqliteStore::open_path(&db).unwrap();
        let inserted = seed_continuous_backfill_track(&store).await;
        let historical: Vec<_> = inserted
            .iter()
            .copied()
            .filter(|(_, at)| *at < 7 * HOUR)
            .collect();
        let partial_id = store
            .insert_session(NewSession {
                started_at: historical.first().unwrap().1,
                ended_at: Some(historical.last().unwrap().1),
                kind: SessionKind::Ai,
                tool: Some("codex".to_string()),
                host: Some(SessionHost::Desktop),
                context_key: "ai:codex".to_string(),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .unwrap();
        let already_assigned: Vec<i64> = historical
            .iter()
            .take(historical.len() / 2)
            .map(|(id, _)| *id)
            .collect();
        store
            .assign_frames_session(&already_assigned, Some(partial_id))
            .await
            .unwrap();
        drop(store);

        let store = store::SqliteStore::open_path(&db).unwrap();
        let engine = sessions::SessionEngine::new().unwrap();
        crate::sessions_scheduler::run_backfill_chunk(
            &store,
            &engine,
            traits::SESSION_FREEZE_LOOKBACK_MS + 8 * HOUR,
            &traits::Settings::default(),
        )
        .await
        .unwrap();

        let rows = store
            .sessions_in_range(SessionFilter {
                now_ms: traits::SESSION_FREEZE_LOOKBACK_MS + 8 * HOUR,
                ..SessionFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "retry must not create a ghost duplicate");
        assert_eq!(rows[0].id, partial_id, "the partial row id remains stable");
        assert!(rows[0].frozen);
        assert_eq!(
            store.session_frames_meta(partial_id).await.unwrap().len(),
            historical.len()
        );
    }

    #[tokio::test]
    async fn delayed_backfill_trims_before_frozen_incremental_overlap() {
        const HOUR: i64 = 3_600_000;
        let store = store::SqliteStore::open_in_memory().unwrap();
        let inserted = seed_continuous_backfill_track(&store).await;
        let engine = sessions::SessionEngine::new().unwrap();
        let settings = traits::Settings::default();
        let lookback = traits::SESSION_FREEZE_LOOKBACK_MS;

        crate::sessions_scheduler::run_backfill_chunk(
            &store,
            &engine,
            lookback + 6 * HOUR,
            &settings,
        )
        .await
        .unwrap();

        let frozen_start = 6 * HOUR;
        let frozen_ids: Vec<i64> = inserted
            .iter()
            .filter_map(|(id, at)| (*at >= frozen_start && *at < 7 * HOUR).then_some(*id))
            .collect();
        let frozen_end = inserted
            .iter()
            .filter_map(|(_, at)| (*at >= frozen_start && *at < 7 * HOUR).then_some(*at))
            .max()
            .unwrap();
        let frozen_id = store
            .insert_session(NewSession {
                started_at: frozen_start,
                ended_at: Some(frozen_end),
                kind: SessionKind::Ai,
                tool: Some("codex".to_string()),
                host: Some(SessionHost::Desktop),
                context_key: "ai:codex".to_string(),
                confidence: 0.9,
                frozen: true,
            })
            .await
            .unwrap();
        assert_eq!(
            store
                .assign_frames_session(&frozen_ids, Some(frozen_id))
                .await
                .unwrap(),
            frozen_ids.len() as u64
        );

        crate::sessions_scheduler::run_backfill_chunk(
            &store,
            &engine,
            lookback + 8 * HOUR,
            &settings,
        )
        .await
        .unwrap();
        let rows = store
            .sessions_in_range(SessionFilter {
                now_ms: lookback + 8 * HOUR,
                ..SessionFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 2, "one prefix plus the immutable frozen tail");
        let prefix = rows.iter().find(|row| row.id != frozen_id).unwrap();
        assert!(prefix.frozen);
        assert!(prefix.ended_at.unwrap() < frozen_start);

        let mut owned = store
            .session_frames_meta(prefix.id)
            .await
            .unwrap()
            .into_iter()
            .chain(store.session_frames_meta(frozen_id).await.unwrap())
            .map(|frame| frame.id)
            .collect::<Vec<_>>();
        owned.sort_unstable();
        let mut expected = inserted
            .iter()
            .filter_map(|(id, at)| (*at < 7 * HOUR).then_some(*id))
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(
            owned, expected,
            "every pre-gap frame has one assignable owner"
        );
    }

    #[test]
    fn new_session_projection_keeps_open_rows_null_ended() {
        let mut value = draft(0, 100, "ai:codex", &[1]);
        value.open = true;
        assert_eq!(value.as_new_session(false).ended_at, None);
        let _: SessionFilter = SessionFilter::default();
        let _: NewSession = value.as_new_session(false);
    }

    #[tokio::test]
    async fn incremental_reconciliation_keeps_the_unfrozen_id_stable() {
        let store = store::SqliteStore::open_in_memory().unwrap();
        for (index, at) in (0..=180_000).step_by(30_000).enumerate() {
            store
                .insert_frame(traits::NewFrame {
                    captured_at: at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{at}.webp"),
                    content_hash: format!("h{index}"),
                    app_hint: Some("codex".to_string()),
                    window_title: Some("Codex".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
        }
        let engine = sessions::SessionEngine::new().unwrap();
        crate::sessions_scheduler::run_incremental_pass(
            &store,
            &engine,
            200_000,
            &traits::Settings::default(),
        )
        .await
        .unwrap();
        let first = store.unfrozen_sessions().await.unwrap();
        assert_eq!(first.len(), 1);
        let stable_id = first[0].id;

        crate::sessions_scheduler::run_incremental_pass(
            &store,
            &engine,
            250_000,
            &traits::Settings::default(),
        )
        .await
        .unwrap();
        let second = store.unfrozen_sessions().await.unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].id, stable_id);
        assert_eq!(store.session_frames_meta(stable_id).await.unwrap().len(), 7);
    }

    #[tokio::test]
    async fn successful_scheduler_pass_emits_sessions_changed_after_rows_commit() {
        let store = store::SqliteStore::open_in_memory().unwrap();
        for (index, captured_at) in (0_i64..=6).map(|index| (index, index * 30_000)) {
            store
                .insert_frame(traits::NewFrame {
                    captured_at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{index}.webp"),
                    content_hash: format!("sessions-changed-{index}"),
                    app_hint: Some("codex".to_string()),
                    window_title: Some("Codex".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
        }
        let engine = sessions::SessionEngine::new().unwrap();
        let (events, mut receiver) = tokio::sync::broadcast::channel(8);

        crate::sessions_scheduler::run_scheduler_pass(
            &store,
            &engine,
            200_000,
            &traits::Settings::default(),
            false,
            &events,
        )
        .await;

        assert!(!store.unfrozen_sessions().await.unwrap().is_empty());
        assert!(matches!(
            receiver.try_recv(),
            Ok(crate::KernelEvent::SessionsChanged)
        ));
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn no_op_scheduler_pass_does_not_emit_sessions_changed() {
        let store = store::SqliteStore::open_in_memory().unwrap();
        for (index, captured_at) in (0_i64..=6).map(|index| (index, index * 30_000)) {
            store
                .insert_frame(traits::NewFrame {
                    captured_at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{index}.webp"),
                    content_hash: format!("sessions-no-op-{index}"),
                    app_hint: Some("codex".to_string()),
                    window_title: Some("Codex".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
        }
        let engine = sessions::SessionEngine::new().unwrap();
        let (events, mut receiver) = tokio::sync::broadcast::channel(8);

        crate::sessions_scheduler::run_scheduler_pass(
            &store,
            &engine,
            200_000,
            &traits::Settings::default(),
            false,
            &events,
        )
        .await;
        assert!(matches!(
            receiver.try_recv(),
            Ok(crate::KernelEvent::SessionsChanged)
        ));

        crate::sessions_scheduler::run_scheduler_pass(
            &store,
            &engine,
            260_000,
            &traits::Settings::default(),
            false,
            &events,
        )
        .await;

        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }
}

pub use capture_loop::{run_capture_loop, CaptureLoopExit, LoopCtx, PendingDemand};
pub use events::KernelEvent;
pub use worker_pool::process_job;

/// Timeout for one `capture_now` demand end-to-end (the worker ack, then the kernel
/// loop inserting the demanded frame and returning its id). An engineering bound, not a
/// user knob — a mark that can't be serviced this fast is surfaced as an honest failure.
const CAPTURE_NOW_TIMEOUT_MS: u64 = 5_000;

/// A platform idle-time source, injected by the composition root (the kernel forbids
/// `unsafe`, so it can't query Win32 itself). Returns milliseconds since the last user
/// input, or `None` if it can't be determined. Drives idle-triggered vision tagging
/// (`03 §5`).
pub type IdleSource = Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// On-demand vision jobs outrank the background embedding backlog so a user's explicit
/// "tag this" is serviced first (`03 §5`).
const VISION_ONDEMAND_PRIORITY: i64 = 10;
/// Upper bound on frames enqueued for one `enqueue_vision` range request.
const VISION_RANGE_CAP: u32 = 1000;

/// Builds a fresh [`CaptureSource`] from the current [`CaptureConfig`]. Supplied by
/// the composition root so the kernel stays impl-agnostic; called on every
/// `start_capture` so settings changes take effect and `stop` can fully release the
/// OS capture resources by dropping the source.
pub type CaptureFactory = Arc<
    dyn Fn(CaptureConfig, CaptureNowReceiver) -> anyhow::Result<Box<dyn CaptureSource>>
        + Send
        + Sync,
>;

struct CaptureHandle {
    id: u64,
    stop: watch::Sender<bool>,
    join: JoinHandle<()>,
    /// Sender for mark-this-moment `capture_now` demands into this capture session's
    /// worker (`03 §7b`, D8). Dropped with the handle on stop, closing the channel so the
    /// capture source's demand arm goes inert.
    capture_now: mpsc::Sender<CaptureNowRequest>,
}

/// Clears the kernel's `pending_demand` slot on drop, so every `capture_now` exit path
/// (including an early `?`/`bail!`) leaves no stale frame-id sender behind to misroute
/// the next demand's frame (`03 §7b`, D8).
struct ClearPendingDemand<'a>(&'a PendingDemand);
impl Drop for ClearPendingDemand<'_> {
    fn drop(&mut self) {
        *self.0.lock().expect("pending demand lock poisoned") = None;
    }
}

/// The orchestrator. Holds the long-lived providers (store, OCR), the capture
/// factory, the event bus, and the live readiness snapshot; starts/stops the
/// capture loop on demand (`capture_control`, `03 §7`).
pub struct Kernel {
    store: Arc<dyn Store>,
    ocr: Arc<dyn OcrProvider>,
    /// Set when the composition root could not create a real OCR provider. Capture
    /// refuses to start rather than silently storing empty text rows.
    ocr_unavailable: Option<String>,
    capture_factory: CaptureFactory,
    frames_dir: PathBuf,
    events: broadcast::Sender<KernelEvent>,
    readiness: Arc<RwLock<Readiness>>,
    capture: Arc<Mutex<Option<CaptureHandle>>>,
    capture_generation: AtomicU64,
    /// The loaded embedding provider, attached after it finishes loading off the
    /// launch thread (`03 §5`); `None` until [`Kernel::attach_embedder`]. Shared live
    /// with the worker pool so inference can start vision workers first.
    embedder: worker_pool::EmbedderSlot,
    /// The running enrichment worker pool; `None` until workers are started.
    workers: Mutex<Option<worker_pool::WorkerPool>>,
    /// The vision provider, shared live with the worker pool so it can be attached
    /// after the pool is running (`03 §6`). `None` until [`Kernel::attach_inference`].
    vision: worker_pool::VisionSlot,
    /// The answer provider (backs the `ask` command); `None` until attached.
    answer: Mutex<Option<Arc<dyn AnswerProvider>>>,
    /// The running timer/idle vision scheduler; `None` until inference is attached.
    scheduler: Mutex<Option<vision_scheduler::SchedulerHandle>>,
    /// The session segmenter supplied by the composition root. The kernel owns only
    /// the contract; the concrete `sessions` crate remains outside this crate.
    session_segmenter: RwLock<Option<Arc<dyn SessionSegmenter>>>,
    /// The incremental + resumable historical session scheduler. Independent of
    /// capture and enrichment so a failure degrades to no sessions, never lost frames.
    sessions_scheduler: Mutex<Option<sessions_scheduler::SchedulerHandle>>,
    /// System-pressure probe for the enrichment throttle (`03 §5/§8`); injected by the
    /// composition root (the kernel forbids `unsafe`). `None` if the platform probe
    /// couldn't be built — the throttle then stays off.
    pressure_probe: RwLock<Option<Arc<dyn PressureProbe>>>,
    /// Current throttle level (`0` Normal / `1` High / `2` Sustained), shared live with
    /// the worker pool so a level change reaches running workers without a pool restart.
    /// Stays `0` whenever the throttle is disabled.
    throttle_level: Arc<AtomicU8>,
    /// The level-2 `embed_text` concurrency floor (clamped `>= 1`), shared live with both
    /// the worker pool (which reads it) and the governor (which rewrites it from settings
    /// each tick) so a floor edit hot-applies seamlessly, like the thresholds/dwells.
    throttle_embed_text_floor: Arc<AtomicUsize>,
    /// Latest throttle status (level + last pressure sample), cached for the
    /// `get_throttle_status` command and the `throttle_changed` event.
    throttle_status: Arc<RwLock<ThrottleStatus>>,
    /// The running throttle governor loop; `None` when the throttle is disabled.
    throttle: Mutex<Option<throttle::ThrottleHandle>>,
    /// One-shot slot handing a demanded frame's `frame_id` back to a waiting
    /// `capture_now` caller; shared into every capture loop's `LoopCtx` (`03 §7b`, D8).
    pending_demand: PendingDemand,
    /// Serializes `capture_now` demands so two rapid mark presses each complete fully
    /// (one frame + one mark each) instead of racing the single `pending_demand` slot.
    capture_now_gate: Mutex<()>,
}

impl Kernel {
    /// Wires the orchestrator. `initial_readiness` carries the `db` status the
    /// composition root already resolved; `capture` starts `Disabled` (off until
    /// the user starts it — privacy-first, recorded in `07`).
    pub fn new(
        store: Arc<dyn Store>,
        ocr: Arc<dyn OcrProvider>,
        capture_factory: CaptureFactory,
        frames_dir: PathBuf,
        initial_readiness: Readiness,
    ) -> Self {
        Self::new_inner(
            store,
            ocr,
            None,
            capture_factory,
            frames_dir,
            initial_readiness,
        )
    }

    /// Builds a kernel whose OCR dependency is known unavailable. The app can keep
    /// running, but `start_capture` fails before opening WGC so it never creates
    /// misleading empty OCR rows.
    pub fn new_with_ocr_unavailable(
        store: Arc<dyn Store>,
        ocr: Arc<dyn OcrProvider>,
        capture_factory: CaptureFactory,
        frames_dir: PathBuf,
        initial_readiness: Readiness,
        reason: String,
    ) -> Self {
        Self::new_inner(
            store,
            ocr,
            Some(reason),
            capture_factory,
            frames_dir,
            initial_readiness,
        )
    }

    fn new_inner(
        store: Arc<dyn Store>,
        ocr: Arc<dyn OcrProvider>,
        ocr_unavailable: Option<String>,
        capture_factory: CaptureFactory,
        frames_dir: PathBuf,
        mut initial_readiness: Readiness,
    ) -> Self {
        initial_readiness.capture = ComponentReadiness {
            status: ComponentStatus::Disabled,
            detail: Some("not started".to_string()),
        };
        let (events, _rx) = broadcast::channel(256);
        Self {
            store,
            ocr,
            ocr_unavailable,
            capture_factory,
            frames_dir,
            events,
            readiness: Arc::new(RwLock::new(initial_readiness)),
            capture: Arc::new(Mutex::new(None)),
            capture_generation: AtomicU64::new(1),
            embedder: Arc::new(RwLock::new(None)),
            workers: Mutex::new(None),
            vision: Arc::new(RwLock::new(None)),
            answer: Mutex::new(None),
            scheduler: Mutex::new(None),
            session_segmenter: RwLock::new(None),
            sessions_scheduler: Mutex::new(None),
            pressure_probe: RwLock::new(None),
            throttle_level: Arc::new(AtomicU8::new(0)),
            // Min clamp; the real value is written from settings by `start_workers` and the
            // governor before it's ever consulted (only at level 2).
            throttle_embed_text_floor: Arc::new(AtomicUsize::new(1)),
            throttle_status: Arc::new(RwLock::new(ThrottleStatus {
                enabled: false,
                level: 0,
                sample: None,
                gpu_monitored: false,
            })),
            throttle: Mutex::new(None),
            pending_demand: Arc::new(std::sync::Mutex::new(None)),
            capture_now_gate: Mutex::new(()),
        }
    }

    /// Subscribe to the kernel event bus (the composition root forwards these to
    /// Tauri events).
    pub fn subscribe(&self) -> broadcast::Receiver<KernelEvent> {
        self.events.subscribe()
    }

    /// The current readiness snapshot (backs the `get_readiness` command).
    pub fn readiness(&self) -> Readiness {
        self.readiness
            .read()
            .expect("readiness lock poisoned")
            .clone()
    }

    /// Whether the capture loop is currently running.
    pub async fn is_capturing(&self) -> bool {
        self.capture.lock().await.is_some()
    }

    /// Starts the capture loop (idempotent — a no-op if already running). Loads
    /// settings, builds the capture source via the factory, and spawns the loop;
    /// flips `capture` readiness Initializing → Ready (or Unavailable on failure).
    pub async fn start_capture(&self) -> anyhow::Result<()> {
        let mut guard = self.capture.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        if let Some(reason) = &self.ocr_unavailable {
            let detail = format!("OCR unavailable: {reason}");
            self.set_capture_readiness(ComponentStatus::Unavailable, Some(detail.clone()));
            return Err(anyhow::anyhow!(detail));
        }
        self.set_capture_readiness(ComponentStatus::Initializing, None);

        let settings = settings::load_settings(self.store.as_ref()).await;
        let cfg = settings::capture_config(&settings);
        // The mark-this-moment demand channel for this capture session (`03 §7b`, D8):
        // the source holds the receiver, the handle keeps the sender. Bounded + small —
        // demands are serialized by `capture_now_gate`, so one is ever in flight.
        let (capture_now_tx, capture_now_rx) = mpsc::channel::<CaptureNowRequest>(4);
        let capture = match (self.capture_factory)(cfg, capture_now_rx) {
            Ok(c) => c,
            Err(e) => {
                self.set_capture_readiness(ComponentStatus::Unavailable, Some(e.to_string()));
                return Err(e);
            }
        };

        let (stop_tx, stop_rx) = watch::channel(false);
        let ctx = LoopCtx {
            store: self.store.clone(),
            ocr: self.ocr.clone(),
            frames_dir: self.frames_dir.clone(),
            events: self.events.clone(),
            enrich_embed_text: settings.enrich_embed_text,
            max_width: settings.storage_max_width,
            chrome_suppress_min_seen: settings.text_chrome_suppress_min_seen,
            chrome_protect_min_chars: settings.text_chrome_protect_min_chars,
            chrome_region_buckets: settings.text_chrome_region_buckets,
            pending_demand: self.pending_demand.clone(),
        };
        let id = self.capture_generation.fetch_add(1, Ordering::Relaxed);
        let capture_slot = self.capture.clone();
        let readiness = self.readiness.clone();
        let events = self.events.clone();
        let ocr_for_shutdown = self.ocr.clone();
        let join = tokio::spawn(async move {
            let exit = run_capture_loop(capture, ctx, stop_rx).await;
            if exit == CaptureLoopExit::SourceShutdown {
                let mut guard = capture_slot.lock().await;
                if guard.as_ref().is_some_and(|h| h.id == id) {
                    *guard = None;
                    set_capture_readiness(
                        &readiness,
                        &events,
                        ComponentStatus::Error,
                        Some("capture source shut down unexpectedly".to_string()),
                    );
                    let _ = events.send(KernelEvent::Toast(Toast {
                        level: ToastLevel::Error,
                        message: "Capture source shut down unexpectedly".to_string(),
                    }));
                    ocr_for_shutdown.on_capture_stopped();
                }
            }
        });
        *guard = Some(CaptureHandle {
            id,
            stop: stop_tx,
            join,
            capture_now: capture_now_tx,
        });
        self.set_capture_readiness(ComponentStatus::Ready, None);
        drop(guard);

        tracing::info!("capture started");
        Ok(())
    }

    /// Stops the capture loop (idempotent). Signals the loop, waits for it to drop
    /// the capture source (releasing OS resources), and flips `capture` → Disabled.
    pub async fn stop_capture(&self) {
        let handle = self.capture.lock().await.take();
        if let Some(h) = handle {
            let _ = h.stop.send(true);
            if let Err(e) = h.join.await {
                tracing::warn!(error = %e, "capture task join failed");
            }
            // The loop has joined — no `recognize` is in flight. Notify the OCR provider so
            // the UIA composite can disconnect its client and release Chromium/Electron apps
            // from accessibility mode. Only fires when a handle existed, so an idempotent stop
            // of already-stopped capture doesn't churn a respawn.
            self.ocr.on_capture_stopped();
        }
        self.set_capture_readiness(ComponentStatus::Disabled, None);
        tracing::info!("capture stopped");
    }

    /// Re-applies capture/privacy/storage settings to a **running** capture loop by
    /// restarting it. `start_capture` snapshots `CaptureConfig` (incl.
    /// `privacy.excluded_apps`) into the capture source at start, so without this a
    /// settings change silently waits for the next manual capture start; `set_settings`
    /// calls this whenever the derived [`CaptureConfig`] changes so a newly excluded app,
    /// changed monitor set, interval, or pause-on-lock takes effect immediately. A no-op
    /// when capture isn't running (the next `start_capture` reads the new settings anyway)
    /// — it never *starts* capture the user had stopped.
    ///
    /// The stop+start is **not atomic**: it releases the `capture` lock between the two
    /// halves, and Tauri 2 async commands run concurrently (there is no global command
    /// serialization), so a user-initiated `stop_capture` IPC call landing in that gap
    /// would be undone by the trailing `start_capture` — restarting capture against the
    /// user's intent. Each half is individually idempotent, which bounds the damage to an
    /// unwanted restart (never corruption), but it does not make the combined outcome
    /// neutral. The residual risk is **accepted**: the window is sub-millisecond and the
    /// only caller is `set_settings`, while the UI surfaces no affordance to fire a save
    /// and a Stop simultaneously. Closing it would mean splitting
    /// `stop_capture`/`start_capture` into lock-held inner variants — a refactor of the
    /// core capture lifecycle judged higher-risk than the race it removes.
    pub async fn reload_capture(&self) -> anyhow::Result<()> {
        if !self.is_capturing().await {
            return Ok(());
        }
        self.stop_capture().await;
        self.start_capture().await
    }

    /// Demands one capture **now**, bypassing the diff gate, and returns the stored
    /// frame's id (mark-this-moment, `03 §7b`, D8). The privacy gate still applies in the
    /// capture source (a locked screen / our own window / an excluded app is refused with
    /// an honest reason). Errors — surfaced to the user, never a silent no-op — if capture
    /// is off, the worker is gone, or the demand times out.
    ///
    /// `capture_now_gate` serializes demands so two rapid mark presses each complete
    /// fully (one guaranteed frame each) rather than racing the single `pending_demand`
    /// slot. The capture slot lock is released before awaiting, so a demand never blocks
    /// `stop_capture`.
    pub async fn capture_now(&self) -> anyhow::Result<i64> {
        let _serialize = self.capture_now_gate.lock().await;

        // Clone the live session's demand sender, then drop the capture lock before await.
        let sender = {
            let guard = self.capture.lock().await;
            match guard.as_ref() {
                Some(h) => h.capture_now.clone(),
                None => anyhow::bail!("capture is off"),
            }
        };

        // Register the frame-id return slot before issuing the demand so the loop can fire
        // it the instant the demanded frame is inserted.
        let (frame_tx, frame_rx) = oneshot::channel::<i64>();
        *self
            .pending_demand
            .lock()
            .expect("pending demand lock poisoned") = Some(frame_tx);

        // Guard so every early return clears the slot (a stale sender would misroute the
        // next demand's frame id).
        let clear_on_drop = ClearPendingDemand(&self.pending_demand);

        let (ack_tx, ack_rx) = oneshot::channel::<anyhow::Result<()>>();
        if sender
            .send(CaptureNowRequest { ack: ack_tx })
            .await
            .is_err()
        {
            anyhow::bail!("capture stopped");
        }

        let timeout = std::time::Duration::from_millis(CAPTURE_NOW_TIMEOUT_MS);
        // 1. The worker acks once the demanded frame is queued (or denies with a reason).
        match tokio::time::timeout(timeout, ack_rx).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(reason))) => return Err(reason),
            // Sender dropped (capture stopped mid-flight) or timed out → honest failure.
            Ok(Err(_)) => anyhow::bail!("capture stopped"),
            Err(_) => anyhow::bail!("capture timed out"),
        }
        // 2. Then the kernel loop hands back the inserted frame's id.
        let frame_id = match tokio::time::timeout(timeout, frame_rx).await {
            Ok(Ok(id)) => id,
            Ok(Err(_)) => anyhow::bail!("captured frame was dropped before it could be marked"),
            Err(_) => anyhow::bail!("capture timed out"),
        };
        drop(clear_on_drop); // slot already emptied by the loop's take(); keep it tidy
        Ok(frame_id)
    }

    /// Creates a mark (`03 §7b`, D8). Exactly one source: an existing `frame_id`, or
    /// `capture_now = true` to capture the current screen past the diff gate first. A
    /// `capture_now` failure inserts **nothing** (no orphan mark). One demand → one mark.
    pub async fn add_mark(
        &self,
        frame_id: Option<i64>,
        capture_now: bool,
        note: Option<String>,
    ) -> anyhow::Result<i64> {
        let target = match (frame_id, capture_now) {
            (Some(id), false) => id,
            (None, true) => self.capture_now().await?,
            _ => anyhow::bail!("add_mark needs exactly one of frame_id or capture_now"),
        };
        self.store
            .insert_mark(target, worker_pool::now_ms(), note)
            .await
    }

    fn set_capture_readiness(&self, status: ComponentStatus, detail: Option<String>) {
        set_capture_readiness(&self.readiness, &self.events, status, detail);
    }

    /// Attaches the loaded embedding provider: lights up the store's vector arm and
    /// starts the worker pool (`03 §5`). The composition root calls this once the
    /// fastembed model has loaded off the launch thread — independent of capture, so
    /// the queue's backlog drains in the background (`02 §5`).
    pub async fn attach_embedder(&self, embedder: Arc<dyn EmbeddingProvider>) {
        self.store.set_embedder(embedder.clone());
        *self.embedder.write().expect("embedder slot lock") = Some(embedder);
        self.set_embed_readiness(ComponentStatus::Ready, None);
        self.reconfigure_enrichment().await;
    }

    /// Starts the bounded enrichment worker pool (idempotent — a no-op if already
    /// running or if no provider-backed lane is available). Runs the startup stale-job
    /// sweep first so a prior run's interrupted jobs are requeued (`03 §6`).
    pub async fn start_workers(&self) {
        let mut guard = self.workers.lock().await;
        if guard.is_some() {
            return;
        }
        let settings = settings::load_settings(self.store.as_ref()).await;
        let embedder_ready = self.embedder.read().expect("embedder slot lock").is_some();
        let vision_ready = self.vision.read().expect("vision slot lock").is_some();
        let has_embed_lane = embedder_ready && settings.enrich_embed_text;
        if !has_embed_lane && !vision_ready {
            tracing::debug!("start_workers: no provider-backed worker lanes available");
            return;
        }
        // With no worker live yet, every `running` job is stale (`03 §6`).
        match self.store.reset_stale_running_jobs(0).await {
            Ok(n) if n > 0 => tracing::info!(requeued = n, "startup sweep requeued stale jobs"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "startup sweep failed"),
        }
        // `frames.image_path` is stored relative to the app-data root (`frames/…`).
        let data_dir = self
            .frames_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.frames_dir.clone());
        let pool = worker_pool::WorkerPool::spawn(worker_pool::Shared {
            store: self.store.clone(),
            embedder: self.embedder.clone(),
            vision: self.vision.clone(),
            enable_embed_text: settings.enrich_embed_text,
            active_jobs: Arc::new(std::sync::Mutex::new(HashSet::new())),
            data_dir,
            events: self.events.clone(),
            concurrency: settings.enrich_worker_concurrency.max(1) as usize,
            throttle_level: self.throttle_level.clone(),
            embed_text_floor: {
                // Seed the shared floor from settings; the governor keeps it current per tick.
                self.throttle_embed_text_floor.store(
                    settings.throttle_embed_text_floor.max(1) as usize,
                    Ordering::Relaxed,
                );
                self.throttle_embed_text_floor.clone()
            },
            active_embed_text: Arc::new(AtomicUsize::new(0)),
        });
        *guard = Some(pool);
        tracing::info!("enrichment workers started");
    }

    /// Stops the worker pool (idempotent), waiting for in-flight jobs to finish.
    pub async fn stop_workers(&self) {
        let pool = self.workers.lock().await.take();
        if let Some(pool) = pool {
            pool.shutdown().await;
            tracing::info!("enrichment workers stopped");
        }
    }

    /// Re-reads enrichment settings and restarts the worker pool so lane toggles and
    /// concurrency changes take effect without an app restart.
    pub async fn reconfigure_enrichment(&self) {
        self.stop_workers().await;
        self.start_workers().await;
    }

    /// Sets the `embed_model` readiness and broadcasts the change (`03 §7`).
    pub fn set_embed_readiness(&self, status: ComponentStatus, detail: Option<String>) {
        let snapshot = {
            let mut r = self.readiness.write().expect("readiness lock poisoned");
            r.embed_model = ComponentReadiness { status, detail };
            r.clone()
        };
        let _ = self.events.send(KernelEvent::ReadinessChanged(snapshot));
    }

    /// Attaches the inference providers (`03 §6`): the vision provider lights up the
    /// `vision_tag` worker path via the shared slot (no pool restart needed), the
    /// answer provider backs the `ask` command, and the timer/idle vision scheduler
    /// starts. The composition root calls this once the sidecar binary + default model
    /// are resolved off the launch thread.
    pub async fn attach_inference(
        &self,
        vision: Arc<dyn VisionProvider>,
        answer: Arc<dyn AnswerProvider>,
        idle: Option<IdleSource>,
        backfill: Option<Arc<dyn BackfillControl>>,
    ) {
        *self.vision.write().expect("vision slot lock") = Some(vision);
        *self.answer.lock().await = Some(answer);
        self.start_workers().await;
        self.start_vision_scheduler(idle, backfill).await;
        tracing::info!("inference providers attached");
    }

    /// The attached answer provider, if any (backs the `ask` command).
    pub async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        self.answer.lock().await.clone()
    }

    /// Attaches the pure session engine and starts its independent background
    /// scheduler. Idempotent: the first provider remains authoritative for the run.
    pub async fn attach_segmenter(&self, segmenter: Arc<dyn SessionSegmenter>) {
        {
            let mut slot = self
                .session_segmenter
                .write()
                .expect("session segmenter slot lock poisoned");
            if slot.is_some() {
                return;
            }
            *slot = Some(segmenter.clone());
        }

        let mut scheduler = self.sessions_scheduler.lock().await;
        if scheduler.is_none() {
            *scheduler = Some(sessions_scheduler::spawn(
                self.store.clone(),
                segmenter,
                self.throttle_level.clone(),
                self.events.clone(),
            ));
            tracing::info!("sessions scheduler started");
        }
    }

    /// Stops the sessions scheduler (idempotent). Called on application shutdown.
    pub async fn stop_sessions_scheduler(&self) {
        if let Some(handle) = self.sessions_scheduler.lock().await.take() {
            handle.stop().await;
            tracing::info!("sessions scheduler stopped");
        }
    }

    /// Enqueues deferred vision tagging for a frame or a time range (`enqueue_vision`,
    /// `03 §5/§7`). On-demand, so it is the one path that may tag without a timer/idle
    /// trigger. Returns how many `vision_tag` jobs were enqueued. A single frame is
    /// always (re)tagged; a range tags only its still-untagged frames.
    pub async fn enqueue_vision(&self, target: VisionTarget) -> anyhow::Result<u64> {
        let ids = match target {
            VisionTarget::Frame { frame_id } => vec![frame_id],
            VisionTarget::Range { start, end } => {
                self.store
                    .untagged_frame_ids(VISION_RANGE_CAP, Some((start, end)))
                    .await?
            }
        };
        let mut count = 0u64;
        for frame_id in ids {
            let job = NewJob {
                kind: JobKind::VisionTag,
                frame_id: Some(frame_id),
                priority: VISION_ONDEMAND_PRIORITY,
                max_attempts: 3,
                not_before: 0,
            };
            match self.store.enqueue_job(job).await {
                Ok(_) => count += 1,
                Err(e) => tracing::warn!(frame_id, error = %e, "enqueue_vision: enqueue failed"),
            }
        }
        tracing::info!(count, "enqueue_vision enqueued vision_tag jobs");
        // Push fresh queue counts so the tray/quick-menu "Start/Stop vision tagging" label
        // flips immediately (0.3.2 PR3) instead of waiting for a worker to complete a job —
        // the only other JobProgress source is `worker_pool::emit_progress`.
        self.emit_job_progress().await;
        Ok(count)
    }

    /// Cancels all `pending` `vision_tag` jobs — the tray/quick-menu "Stop vision tagging"
    /// action (0.3.2 PR3; `03 §7d`). A `running` job is left to finish (no lease to
    /// revoke), so the label honestly stays "Stop" until it drains. Returns how many were
    /// cancelled and broadcasts refreshed queue counts.
    pub async fn cancel_vision(&self) -> anyhow::Result<u64> {
        let count = self.store.cancel_pending_vision_jobs().await?;
        tracing::info!(count, "cancel_vision removed pending vision_tag jobs");
        self.emit_job_progress().await;
        Ok(count)
    }

    /// Broadcasts current queue counts as [`KernelEvent::JobProgress`] (best-effort: a
    /// failed stats read logs and skips the emit). Shared by the on-demand vision
    /// enqueue/cancel paths so their label state stays live without a worker tick.
    async fn emit_job_progress(&self) {
        match self.store.job_stats().await {
            Ok(stats) => {
                let _ = self.events.send(KernelEvent::JobProgress(stats));
            }
            Err(e) => tracing::warn!(error = %e, "emit_job_progress: job_stats read failed"),
        }
    }

    /// Records a sidecar lifecycle transition: maps it onto `sidecar` readiness and
    /// re-broadcasts both the readiness change and the raw status (`sidecar_status`,
    /// `03 §6/§7`). The composition root bridges the supervisor's status channel here.
    pub fn emit_sidecar_status(&self, status: SidecarStatus) {
        let (component, detail) = sidecar_component(&status);
        let state = status.state;
        let snapshot = {
            let mut r = self.readiness.write().expect("readiness lock poisoned");
            r.sidecar = ComponentReadiness {
                status: component,
                detail,
            };
            r.clone()
        };
        let _ = self.events.send(KernelEvent::ReadinessChanged(snapshot));
        let _ = self.events.send(KernelEvent::SidecarStatus(status));
        if state == SidecarState::Crashed {
            self.emit_toast(
                ToastLevel::Error,
                "Inference sidecar crashed; the next request will try to restart it",
            );
        }
    }

    /// Re-broadcasts a model-download progress update for the UI (`model_download`). The
    /// composition root bridges the downloader's progress channel here.
    pub fn emit_model_download(&self, status: ModelDownloadStatus) {
        let _ = self.events.send(KernelEvent::ModelDownload(status));
    }

    /// Emits a transient user-facing notification through the shell event bridge.
    pub fn emit_toast(&self, level: ToastLevel, message: impl Into<String>) {
        let _ = self.events.send(KernelEvent::Toast(Toast {
            level,
            message: message.into(),
        }));
    }

    /// Sets the `sidecar` readiness directly and broadcasts the change. Used by the
    /// composition root while resolving the binary/model before the supervisor exists
    /// (Initializing → Ready / Unavailable, `03 §7`).
    pub fn set_sidecar_readiness(&self, status: ComponentStatus, detail: Option<String>) {
        let snapshot = {
            let mut r = self.readiness.write().expect("readiness lock poisoned");
            r.sidecar = ComponentReadiness { status, detail };
            r.clone()
        };
        let _ = self.events.send(KernelEvent::ReadinessChanged(snapshot));
    }

    /// Starts the timer/idle vision scheduler (idempotent). `idle` is `None` when no
    /// platform idle source is available (timer-only); `backfill` lets the idle loop keep
    /// the sidecar warm while it drains the backlog.
    async fn start_vision_scheduler(
        &self,
        idle: Option<IdleSource>,
        backfill: Option<Arc<dyn BackfillControl>>,
    ) {
        let mut guard = self.scheduler.lock().await;
        if guard.is_some() {
            return;
        }
        *guard = Some(vision_scheduler::spawn(self.store.clone(), idle, backfill));
        tracing::info!("vision scheduler started");
    }

    /// Stops the vision scheduler (idempotent). Called on shutdown.
    pub async fn stop_vision_scheduler(&self) {
        if let Some(handle) = self.scheduler.lock().await.take() {
            handle.stop().await;
            tracing::info!("vision scheduler stopped");
        }
    }

    /// Injects the system-pressure probe for the enrichment throttle (`03 §5/§8`). The
    /// composition root builds it (the only `unsafe`, in `sysmon`) and calls this once at
    /// startup, then [`Kernel::reconfigure_throttle`] to honor the master-enable setting.
    /// Seeds the cached status' `gpu_monitored` so the UI is truthful before the first
    /// sample lands.
    pub fn set_pressure_probe(&self, probe: Arc<dyn PressureProbe>) {
        let monitored = probe.gpu_monitored();
        *self
            .pressure_probe
            .write()
            .expect("pressure probe lock poisoned") = Some(probe);
        self.throttle_status
            .write()
            .expect("throttle status lock poisoned")
            .gpu_monitored = monitored;
    }

    /// Starts or stops the throttle governor to match the `throttle.enabled` setting,
    /// without an app restart. Called once at startup (after [`Kernel::set_pressure_probe`])
    /// and from the settings-save path (peer of [`Kernel::reconfigure_enrichment`]). When
    /// disabled it resets the shared level to `0` so the worker pool resumes full draining.
    pub async fn reconfigure_throttle(&self) {
        let enabled = settings::load_settings(self.store.as_ref())
            .await
            .throttle_enabled;
        let mut guard = self.throttle.lock().await;
        if enabled {
            if guard.is_some() {
                return;
            }
            let probe = self
                .pressure_probe
                .read()
                .expect("pressure probe lock poisoned")
                .clone();
            match probe {
                Some(probe) => {
                    *guard = Some(throttle::spawn(
                        self.store.clone(),
                        probe,
                        self.throttle_level.clone(),
                        self.throttle_embed_text_floor.clone(),
                        self.throttle_status.clone(),
                        self.events.clone(),
                    ));
                    tracing::info!("enrichment throttle governor started");
                }
                None => tracing::warn!(
                    "throttle enabled but no pressure probe injected; throttle stays off"
                ),
            }
        } else if let Some(handle) = guard.take() {
            handle.stop().await;
            self.reset_throttle_baseline();
            tracing::info!("enrichment throttle governor stopped");
        }
    }

    /// Stops the throttle governor (idempotent). Called on shutdown.
    pub async fn stop_throttle(&self) {
        if let Some(handle) = self.throttle.lock().await.take() {
            handle.stop().await;
            self.reset_throttle_baseline();
            tracing::info!("enrichment throttle governor stopped");
        }
    }

    /// Resets the shared level to `0` and the cached status to the honest disabled state
    /// (preserving the truthful `gpu_monitored`), then broadcasts the change.
    fn reset_throttle_baseline(&self) {
        self.throttle_level.store(0, Ordering::Relaxed);
        let snapshot = {
            let mut st = self
                .throttle_status
                .write()
                .expect("throttle status lock poisoned");
            st.enabled = false;
            st.level = 0;
            st.sample = None;
            *st
        };
        let _ = self.events.send(KernelEvent::ThrottleChanged(snapshot));
    }

    /// The latest throttle status (backs the `get_throttle_status` command).
    pub fn throttle_status(&self) -> ThrottleStatus {
        *self
            .throttle_status
            .read()
            .expect("throttle status lock poisoned")
    }
}

/// Maps a sidecar lifecycle state to a `sidecar` readiness component (`03 §6/§7`). An
/// evicted (idle-unloaded) sidecar maps to `Disabled`, **not** `Ready`: the process is not
/// running, so claiming "Ready" lied to the user (the panel reads the raw `SidecarState`
/// for the precise "Idle — unloaded (loads on demand)" label). It still respawns on the
/// next request — `Disabled` is the neutral, non-alarming "not currently loaded".
fn sidecar_component(status: &SidecarStatus) -> (ComponentStatus, Option<String>) {
    match status.state {
        SidecarState::Starting => (ComponentStatus::Initializing, status.model.clone()),
        SidecarState::Ready => (ComponentStatus::Ready, status.model.clone()),
        SidecarState::Evicted => (
            ComponentStatus::Disabled,
            Some("idle — unloaded (loads on demand)".to_string()),
        ),
        SidecarState::Recycled => (ComponentStatus::Initializing, status.model.clone()),
        SidecarState::Crashed => (ComponentStatus::Error, Some("sidecar crashed".to_string())),
        SidecarState::Stopped => (ComponentStatus::Disabled, Some("stopped".to_string())),
    }
}

fn set_capture_readiness(
    readiness: &RwLock<Readiness>,
    events: &broadcast::Sender<KernelEvent>,
    status: ComponentStatus,
    detail: Option<String>,
) {
    let snapshot = {
        let mut r = readiness.write().expect("readiness lock poisoned");
        r.capture = ComponentReadiness { status, detail };
        r.clone()
    };
    let _ = events.send(KernelEvent::ReadinessChanged(snapshot));
}
