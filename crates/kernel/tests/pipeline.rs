//! End-to-end capture-pipeline tests (`03 §10` "capture(stub frames) → OCR → store
//! → embed job"). These drive the real kernel loop with **fake** `CaptureSource` /
//! `OcrProvider` and an in-memory `Store`, so they run on any platform (no Windows
//! APIs) and gate P2's core invariants: frames + OCR are stored, each frame
//! enqueues exactly one `embed_text` job, **no `vision_tag` job is ever enqueued**
//! (`13.3`), and a `capture_tick` is emitted per frame.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use image::{Rgba, RgbaImage};
use tokio::sync::{broadcast, watch};

use kernel::{run_capture_loop, CaptureFactory, Kernel, KernelEvent, LoopCtx};
use store::SqliteStore;
use traits::{
    CaptureNowReceiver, CaptureSource, CaptureTrigger, CapturedFrame, ComponentStatus, JobKind,
    MonitorInfo, OcrProvider, OcrResult, Readiness, Result, Store,
};

#[derive(Debug, Clone, Copy)]
enum AfterFrames {
    Shutdown,
    Pending,
}

/// A capture source that replays a fixed list of frames, then `None` (shutdown).
struct FakeCapture {
    frames: VecDeque<CapturedFrame>,
    after_frames: AfterFrames,
}

#[async_trait]
impl CaptureSource for FakeCapture {
    fn monitors(&self) -> Vec<MonitorInfo> {
        vec![MonitorInfo {
            index: 0,
            name: "FAKE".to_string(),
            width: 4,
            height: 4,
            is_primary: true,
        }]
    }
    async fn next_frame(&mut self) -> Result<Option<CapturedFrame>> {
        if let Some(frame) = self.frames.pop_front() {
            return Ok(Some(frame));
        }
        match self.after_frames {
            AfterFrames::Shutdown => Ok(None),
            AfterFrames::Pending => std::future::pending().await,
        }
    }
}

/// A capture source that services `capture_now` demands (`03 §7b`, D8): it waits on the
/// demand channel and, per demand, either emits one `demanded` Manual frame + acks `Ok`
/// (the happy path), or denies with a reason (`deny`). Timer/event cycles are inert — it
/// only ever produces demanded frames — so the kernel-level marks tests are deterministic.
struct DemandCapture {
    rx: CaptureNowReceiver,
    next_at: i64,
    deny: Option<&'static str>,
}

#[async_trait]
impl CaptureSource for DemandCapture {
    fn monitors(&self) -> Vec<MonitorInfo> {
        vec![MonitorInfo {
            index: 0,
            name: "FAKE".to_string(),
            width: 4,
            height: 4,
            is_primary: true,
        }]
    }
    async fn next_frame(&mut self) -> Result<Option<CapturedFrame>> {
        match self.rx.recv().await {
            Some(req) => {
                if let Some(reason) = self.deny {
                    let _ = req.ack.send(Err(anyhow::anyhow!(reason)));
                    // No frame produced; keep waiting for the next demand.
                    return std::future::pending().await;
                }
                let _ = req.ack.send(Ok(()));
                let mut f = frame(self.next_at);
                self.next_at += 1;
                f.trigger = CaptureTrigger::Manual;
                f.demanded = true;
                Ok(Some(f))
            }
            // Channel closed (capture stopped) → go idle.
            None => std::future::pending().await,
        }
    }
}

/// A deterministic OCR stand-in: text derived from the frame's timestamp.
struct FakeOcr;

#[async_trait]
impl OcrProvider for FakeOcr {
    async fn recognize(&self, frame: &CapturedFrame) -> Result<OcrResult> {
        // Emit one line of word spans (as the real WinRT engine does) so the attention
        // filter has geometry to classify and rebuild `content_text` from.
        let text = format!("ocr text for frame at {}", frame.captured_at);
        let mut spans = Vec::new();
        let mut x = 0.1_f32;
        for word in text.split_whitespace() {
            let w = 0.02 * word.chars().count() as f32;
            spans.push(traits::TextSpan {
                normalized_text: traits::normalize_text(word),
                text: word.to_string(),
                source: traits::TextSource::Ocr,
                role: traits::TextRole::Unknown,
                x,
                y: 0.45,
                w,
                h: 0.03,
                line_index: 0,
                is_searchable: true,
                suppress_reason: None,
            });
            x += w + 0.01;
        }
        Ok(OcrResult {
            text,
            mean_confidence: 0.9,
            engine: "fake".to_string(),
            spans,
        })
    }
}

/// OCR stand-in that counts `on_capture_stopped` calls, to prove the kernel notifies the
/// provider when capture stops (the UIA composite relies on this to release its client).
struct CountingOcr {
    stopped: Arc<std::sync::atomic::AtomicU32>,
}

#[async_trait]
impl OcrProvider for CountingOcr {
    async fn recognize(&self, frame: &CapturedFrame) -> Result<OcrResult> {
        FakeOcr.recognize(frame).await
    }
    fn on_capture_stopped(&self) {
        self.stopped
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// OCR provider used to model a missing WinRT OCR engine/language pack.
struct ErrorOcr;

#[async_trait]
impl OcrProvider for ErrorOcr {
    async fn recognize(&self, _frame: &CapturedFrame) -> Result<OcrResult> {
        anyhow::bail!("OCR unavailable: no recognizer language installed")
    }
}

fn frame(captured_at: i64) -> CapturedFrame {
    let pixels = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
    CapturedFrame {
        monitor_index: 0,
        width: 4,
        height: 4,
        captured_at,
        pixels: Arc::new(pixels),
        content_hash: format!("hash-{captured_at}"),
        app_hint: Some("Firefox".to_string()),
        window_title: Some("Inbox".to_string()),
        target_rect: None,
        foreground_hwnd: None,
        trigger: traits::CaptureTrigger::Timer,
        demanded: false,
    }
}

#[tokio::test]
async fn capture_loop_stores_frames_ocr_jpegs_and_enqueues_embed_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();
    let frames_dir = data_dir.join("frames");

    // keep the concrete handle for reads (`get_frame` is inherent, not on the trait)
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);
    let caps: VecDeque<CapturedFrame> = (0..3).map(|i| frame(1_000 + i)).collect();

    let (events, mut rx) = broadcast::channel(64);
    let (_stop_tx, stop_rx) = watch::channel(false);
    let ctx = LoopCtx {
        store: store.clone(),
        ocr,
        frames_dir,
        events,
        enrich_embed_text: true,
        max_width: 1280,
        chrome_suppress_min_seen: 12,
        chrome_protect_min_chars: 48,
        chrome_region_buckets: 8,
        pending_demand: Arc::new(std::sync::Mutex::new(None)),
    };

    run_capture_loop(
        Box::new(FakeCapture {
            frames: caps,
            after_frames: AfterFrames::Shutdown,
        }),
        ctx,
        stop_rx,
    )
    .await;

    // every frame: row stored with OCR text + foreground context + a WebP on disk.
    // content_text is a passthrough copy of raw_text in PR2 (03 §3b).
    for i in 0..3 {
        let id = i64::from(i) + 1;
        let detail = db.get_frame(id).await.unwrap().expect("frame stored");
        assert_eq!(detail.captured_at, 1_000 + i64::from(i));
        assert_eq!(detail.app_hint.as_deref(), Some("Firefox"));
        assert_eq!(detail.window_title.as_deref(), Some("Inbox"));
        assert!(detail.content_text.as_deref().unwrap().contains("ocr text"));
        assert_eq!(detail.raw_text, detail.content_text);
        assert!(
            !detail.image_purged,
            "freshly captured frame still has its image"
        );
        assert!(
            detail.image_path.ends_with(".webp"),
            "capture stored as WebP: {}",
            detail.image_path
        );
        assert!(
            data_dir.join(&detail.image_path).exists(),
            "capture image should be written at {}",
            detail.image_path
        );
    }

    // exactly one embed_text job per frame, all pending; NO vision_tag jobs (13.3)
    assert_eq!(store.job_stats().await.unwrap().pending, 3);
    assert!(store
        .claim_jobs(&[JobKind::VisionTag], 10, 9_999_999)
        .await
        .unwrap()
        .is_empty());
    let embed = store
        .claim_jobs(&[JobKind::EmbedText], 10, 9_999_999)
        .await
        .unwrap();
    assert_eq!(embed.len(), 3);
    assert!(embed
        .iter()
        .all(|j| j.kind == JobKind::EmbedText && j.frame_id.is_some()));

    // one capture_tick per stored frame
    let mut ticks = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, KernelEvent::CaptureTick(_)) {
            ticks += 1;
        }
    }
    assert_eq!(ticks, 3);
}

#[tokio::test]
async fn capture_loop_skips_embed_jobs_when_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);
    let caps: VecDeque<CapturedFrame> = (0..2).map(|i| frame(5_000 + i)).collect();

    let (events, _rx) = broadcast::channel(64);
    let (_stop_tx, stop_rx) = watch::channel(false);
    let ctx = LoopCtx {
        store: store.clone(),
        ocr,
        frames_dir: tmp.path().join("frames"),
        events,
        enrich_embed_text: false, // enrich.embed_text = false
        max_width: 1280,
        chrome_suppress_min_seen: 12,
        chrome_protect_min_chars: 48,
        chrome_region_buckets: 8,
        pending_demand: Arc::new(std::sync::Mutex::new(None)),
    };

    run_capture_loop(
        Box::new(FakeCapture {
            frames: caps,
            after_frames: AfterFrames::Shutdown,
        }),
        ctx,
        stop_rx,
    )
    .await;

    // frames are still stored, but no jobs are enqueued
    assert!(db.get_frame(1).await.unwrap().is_some());
    assert_eq!(store.job_stats().await.unwrap().pending, 0);
}

#[tokio::test]
async fn kernel_start_then_stop_flips_capture_readiness() {
    let tmp = tempfile::tempdir().unwrap();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);

    // factory yields a fresh fake source each Start (drains 2 frames, then ends)
    let factory: CaptureFactory = Arc::new(|_cfg, _rx| {
        let caps: VecDeque<CapturedFrame> = (0..2).map(|i| frame(2_000 + i)).collect();
        Ok(Box::new(FakeCapture {
            frames: caps,
            after_frames: AfterFrames::Pending,
        }) as Box<dyn CaptureSource>)
    });

    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );

    // off until Start (privacy-first)
    assert_eq!(kernel.readiness().capture.status, ComponentStatus::Disabled);
    assert!(!kernel.is_capturing().await);

    kernel.start_capture().await.unwrap();
    assert_eq!(kernel.readiness().capture.status, ComponentStatus::Ready);
    assert!(kernel.is_capturing().await);

    // start is idempotent — a second call is a no-op, still Ready
    kernel.start_capture().await.unwrap();
    assert_eq!(kernel.readiness().capture.status, ComponentStatus::Ready);

    // stop joins the loop and flips to Disabled; a second stop is a no-op
    kernel.stop_capture().await;
    assert_eq!(kernel.readiness().capture.status, ComponentStatus::Disabled);
    assert!(!kernel.is_capturing().await);
    kernel.stop_capture().await;
    assert_eq!(kernel.readiness().capture.status, ComponentStatus::Disabled);
}

#[tokio::test]
async fn stop_capture_notifies_ocr_provider() {
    // The kernel must tell the OCR provider when capture stops so the UIA composite can
    // disconnect its UI Automation client (Chromium/Electron apps then leave accessibility
    // mode). It must fire exactly once per real stop, and never for an idempotent no-op stop.
    let tmp = tempfile::tempdir().unwrap();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    let stopped = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let ocr: Arc<dyn OcrProvider> = Arc::new(CountingOcr {
        stopped: stopped.clone(),
    });

    let factory: CaptureFactory = Arc::new(|_cfg, _rx| {
        Ok(Box::new(FakeCapture {
            frames: VecDeque::new(),
            after_frames: AfterFrames::Pending,
        }) as Box<dyn CaptureSource>)
    });

    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );

    // Stopping capture that never started must NOT notify (no handle existed).
    kernel.stop_capture().await;
    assert_eq!(stopped.load(std::sync::atomic::Ordering::SeqCst), 0);

    kernel.start_capture().await.unwrap();
    kernel.stop_capture().await;
    assert_eq!(
        stopped.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "stop after a real start notifies exactly once"
    );

    // A second, idempotent stop (no live handle) must not re-notify.
    kernel.stop_capture().await;
    assert_eq!(
        stopped.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "idempotent stop does not re-notify"
    );
}

#[tokio::test]
async fn source_shutdown_notifies_ocr_provider() {
    // When the WGC capture source self-terminates (monitor disconnect, driver reset) the
    // kernel must still call on_capture_stopped() so the UIA composite releases its client.
    let tmp = tempfile::tempdir().unwrap();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    let stopped = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let ocr: Arc<dyn OcrProvider> = Arc::new(CountingOcr {
        stopped: stopped.clone(),
    });

    let factory: CaptureFactory = Arc::new(|_cfg, _rx| {
        Ok(Box::new(FakeCapture {
            frames: VecDeque::new(),
            after_frames: AfterFrames::Shutdown,
        }) as Box<dyn CaptureSource>)
    });

    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );

    kernel.start_capture().await.unwrap();

    // Wait for the source to self-terminate.
    for _ in 0..50 {
        if !kernel.is_capturing().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(!kernel.is_capturing().await);
    assert_eq!(
        stopped.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "SourceShutdown must notify on_capture_stopped exactly once"
    );
}

#[tokio::test]
async fn kernel_clears_capture_and_marks_error_when_source_shuts_down() {
    let tmp = tempfile::tempdir().unwrap();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);

    let factory: CaptureFactory = Arc::new(|_cfg, _rx| {
        Ok(Box::new(FakeCapture {
            frames: VecDeque::new(),
            after_frames: AfterFrames::Shutdown,
        }) as Box<dyn CaptureSource>)
    });

    let kernel = Kernel::new(
        store,
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );

    kernel.start_capture().await.unwrap();

    for _ in 0..50 {
        if !kernel.is_capturing().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(!kernel.is_capturing().await);
    let (enabled, observations) = kernel
        .capture_observations(i64::MIN, i64::MAX)
        .await
        .unwrap();
    assert!(!enabled);
    let interruption = observations
        .intervals
        .last()
        .expect("source shutdown observed");
    assert_eq!(
        interruption.reason,
        kernel::CaptureInterruptionReason::WorkerError
    );
    assert_eq!(interruption.end, None);
    let readiness = kernel.readiness().capture;
    assert_eq!(readiness.status, ComponentStatus::Error);
    assert!(readiness
        .detail
        .as_deref()
        .unwrap_or_default()
        .contains("source shut down"));
}

#[tokio::test]
async fn reload_capture_restarts_loop_with_fresh_config() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);

    // Record the CaptureConfig the factory is built with on each (re)start.
    let configs: Arc<std::sync::Mutex<Vec<traits::CaptureConfig>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = configs.clone();
    let factory: CaptureFactory = Arc::new(move |cfg, _rx| {
        seen.lock().unwrap().push(cfg);
        Ok(Box::new(FakeCapture {
            frames: VecDeque::new(),
            after_frames: AfterFrames::Pending,
        }) as Box<dyn CaptureSource>)
    });

    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );

    // Reload before any start is a no-op (it never starts capture the user hasn't).
    kernel.reload_capture().await.unwrap();
    assert!(!kernel.is_capturing().await);
    assert!(configs.lock().unwrap().is_empty());

    kernel.start_capture().await.unwrap();
    assert_eq!(configs.lock().unwrap().len(), 1);

    // A newly excluded app is persisted, then reloaded: the loop restarts and the rebuilt
    // CaptureConfig carries the new value — the fix for "Excluded Apps never applies".
    store
        .set_setting("privacy.excluded_apps", "[\"SecretApp\"]")
        .await
        .unwrap();
    kernel.reload_capture().await.unwrap();
    assert!(kernel.is_capturing().await);

    let cfgs = configs.lock().unwrap();
    assert_eq!(cfgs.len(), 2, "reload restarted the capture loop");
    assert!(
        cfgs[1].excluded_apps.iter().any(|a| a == "SecretApp"),
        "rebuilt config picks up the newly excluded app"
    );
}

#[tokio::test]
async fn kernel_refuses_to_start_capture_when_ocr_is_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(ErrorOcr);
    let factory: CaptureFactory =
        Arc::new(|_cfg, _rx| panic!("capture factory should not run without OCR"));

    let kernel = Kernel::new_with_ocr_unavailable(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
        "no OCR recognizer languages are installed".to_string(),
    );

    let err = kernel
        .start_capture()
        .await
        .expect_err("capture start fails");

    assert!(err.to_string().contains("OCR unavailable"));
    assert_eq!(
        kernel.readiness().capture.status,
        ComponentStatus::Unavailable
    );
    assert!(!kernel.is_capturing().await);
    assert!(db.get_frame(1).await.unwrap().is_none());
    assert_eq!(store.job_stats().await.unwrap().pending, 0);
}

// Coverage observes controller intent/errors; it never asks capture to produce a frame.
#[tokio::test]
async fn coverage_snapshot_reads_are_inert_and_controller_transitions_are_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let starts = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let seen = starts.clone();
    let factory: CaptureFactory = Arc::new(move |_, _| {
        seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Box::new(FakeCapture {
            frames: VecDeque::new(),
            after_frames: AfterFrames::Pending,
        }))
    });
    let kernel = Kernel::new(
        db.clone(),
        Arc::new(FakeOcr),
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    let (enabled, initial) = kernel
        .capture_observations(i64::MIN, i64::MAX)
        .await
        .unwrap();
    assert!(!enabled);
    assert!(initial.truncated, "previous-process history is unknown");
    assert_eq!(initial.intervals[0].start, initial.observation_started_at);
    assert_eq!(initial.intervals[0].end, None);
    assert_eq!(
        initial.intervals[0].reason,
        kernel::CaptureInterruptionReason::CaptureStopped
    );
    kernel.stop_capture().await;
    assert_eq!(
        kernel
            .capture_observations(i64::MIN, i64::MAX)
            .await
            .unwrap()
            .1,
        initial
    );
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(kernel.capture_observations(1, 1).await.is_err());
    assert!(kernel.capture_observations(2, 1).await.is_err());
    assert!(db.get_frame(1).await.unwrap().is_none());
    assert_eq!(db.job_stats().await.unwrap().pending, 0);

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    kernel.start_capture().await.unwrap();
    let started = kernel
        .capture_observations(i64::MIN, i64::MAX)
        .await
        .unwrap();
    assert!(
        started.0,
        "enabled is intent, not proof of successful capture"
    );
    assert!(started.1.intervals[0].end.is_some());
    kernel.start_capture().await.unwrap();
    assert_eq!(
        kernel
            .capture_observations(i64::MIN, i64::MAX)
            .await
            .unwrap(),
        started
    );
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    kernel.stop_capture().await;
    let stopped = kernel
        .capture_observations(i64::MIN, i64::MAX)
        .await
        .unwrap();
    assert!(!stopped.0);
    assert_eq!(stopped.1.intervals.last().unwrap().end, None);
    kernel.stop_capture().await;
    assert_eq!(
        kernel
            .capture_observations(i64::MIN, i64::MAX)
            .await
            .unwrap(),
        stopped
    );
}

struct ErrorThenPendingCapture(bool);

#[async_trait]
impl CaptureSource for ErrorThenPendingCapture {
    fn monitors(&self) -> Vec<MonitorInfo> {
        Vec::new()
    }
    async fn next_frame(&mut self) -> Result<Option<CapturedFrame>> {
        if std::mem::take(&mut self.0) {
            anyhow::bail!("private diagnostic must never enter observations")
        }
        std::future::pending().await
    }
}

#[tokio::test]
async fn coverage_records_observed_worker_errors_without_claiming_capture_health() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let factory: CaptureFactory = Arc::new(|_, _| Ok(Box::new(ErrorThenPendingCapture(true))));
    let kernel = Kernel::new(
        db.clone(),
        Arc::new(FakeOcr),
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    kernel.start_capture().await.unwrap();
    let observations = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let snapshot = kernel
                .capture_observations(i64::MIN, i64::MAX)
                .await
                .unwrap();
            if snapshot.1.intervals.last().is_some_and(|interval| {
                interval.reason == kernel::CaptureInterruptionReason::WorkerError
            }) {
                break snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("source error observed by actual capture loop");
    assert!(
        observations.0,
        "nonfatal worker error leaves controller intent enabled"
    );
    assert_eq!(observations.1.intervals.last().unwrap().end, None);
    kernel.start_capture().await.unwrap();
    assert_eq!(
        kernel
            .capture_observations(i64::MIN, i64::MAX)
            .await
            .unwrap(),
        observations,
        "an idempotent start is not a health recovery observation"
    );
    assert!(!format!("{:?}", observations.1).contains("private diagnostic"));
    assert!(db.get_frame(1).await.unwrap().is_none());
    kernel.stop_capture().await;
}

#[tokio::test]
async fn stop_capture_after_source_shutdown_does_not_relabel_worker_error() {
    // A source that self-terminates (monitor disconnect, driver reset) clears the capture
    // slot and records an open-ended worker error. A later user stop (capture_control(Stop),
    // tray stop, quit) has no handle to stop, so it must not close that error interval and
    // relabel it as a deliberate `capture_stopped` observation (03 §7f.2/§7f.6: reasons are
    // observed, never inferred; actual observed endpoints are preserved).
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let factory: CaptureFactory = Arc::new(|_cfg, _rx| {
        Ok(Box::new(FakeCapture {
            frames: VecDeque::new(),
            after_frames: AfterFrames::Shutdown,
        }) as Box<dyn CaptureSource>)
    });
    let kernel = Kernel::new(
        db.clone(),
        Arc::new(FakeOcr),
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    kernel.start_capture().await.unwrap();

    // Wait until the shutdown watcher has cleared the capture slot and recorded the
    // worker error; both happen under the capture lock, so observing the error proves
    // the slot is already empty.
    let before = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let snapshot = kernel
                .capture_observations(i64::MIN, i64::MAX)
                .await
                .unwrap();
            if snapshot.1.intervals.last().is_some_and(|interval| {
                interval.reason == kernel::CaptureInterruptionReason::WorkerError
            }) {
                break snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("source shutdown observed by the watcher");
    assert!(!kernel.is_capturing().await);
    assert_eq!(before.1.intervals.last().unwrap().end, None);

    kernel.stop_capture().await;
    let after = kernel
        .capture_observations(i64::MIN, i64::MAX)
        .await
        .unwrap();
    assert_eq!(
        after, before,
        "a no-handle stop must not fabricate a capture_stopped observation"
    );
    let last = after.1.intervals.last().unwrap();
    assert_eq!(last.end, None);
    assert_eq!(last.reason, kernel::CaptureInterruptionReason::WorkerError);
}

#[tokio::test]
async fn coverage_observes_capture_worker_startup_failure_without_private_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    let factory: CaptureFactory =
        Arc::new(|_, _| anyhow::bail!("private worker startup diagnostic"));
    let kernel = Kernel::new(
        store,
        Arc::new(FakeOcr),
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    assert!(kernel.start_capture().await.is_err());
    let (enabled, observations) = kernel
        .capture_observations(i64::MIN, i64::MAX)
        .await
        .unwrap();
    assert!(!enabled);
    let interruption = observations.intervals.last().unwrap();
    assert_eq!(
        interruption.reason,
        kernel::CaptureInterruptionReason::WorkerError
    );
    assert_eq!(interruption.end, None);
    assert!(!format!("{observations:?}").contains("private worker startup diagnostic"));
}

#[tokio::test]
async fn coverage_observes_frame_processing_errors_without_changing_capture_behavior() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let factory: CaptureFactory = Arc::new(|_, _| {
        Ok(Box::new(FakeCapture {
            // Backdated fixture frame time must never become the error observation time.
            frames: VecDeque::from([frame(-1_000_000)]),
            after_frames: AfterFrames::Pending,
        }))
    });
    let kernel = Kernel::new(
        db.clone(),
        Arc::new(ErrorOcr),
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    kernel.start_capture().await.unwrap();
    let (enabled, observations) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let snapshot = kernel
                .capture_observations(i64::MIN, i64::MAX)
                .await
                .unwrap();
            if snapshot.1.intervals.last().is_some_and(|interval| {
                interval.reason == kernel::CaptureInterruptionReason::WorkerError
            }) {
                break snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("OCR failure observed in actual processing loop");
    assert!(
        enabled,
        "per-frame errors do not stop the existing pipeline"
    );
    let interruption = observations.intervals.last().unwrap();
    assert!(interruption.start >= observations.observation_started_at);
    assert_ne!(interruption.start, -1_000_000);
    assert_eq!(interruption.end, None);
    assert!(db.get_frame(1).await.unwrap().is_none());
    assert_eq!(db.job_stats().await.unwrap().pending, 0);
    kernel.stop_capture().await;
}

// ── Mark-this-moment: capture_now + add_mark (03 §7b, D8, 0.3.0 PR6) ──────────────

/// A `capture_now` mark from a running session inserts a `manual` frame past the diff
/// gate and a mark referencing it — one demand, one frame, one mark.
#[tokio::test]
async fn add_mark_capture_now_inserts_manual_frame_and_mark() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);
    let factory: CaptureFactory = Arc::new(|_cfg, rx| {
        Ok(Box::new(DemandCapture {
            rx,
            next_at: 7_000,
            deny: None,
        }) as Box<dyn CaptureSource>)
    });
    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    kernel.start_capture().await.unwrap();

    let mark_id = kernel
        .add_mark(None, true, Some("call the plumber".to_string()))
        .await
        .expect("capture_now mark succeeds");

    // The demanded frame is stored with capture_trigger = 'manual'.
    let marks = store.list_marks().await.unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].mark_id, mark_id);
    assert_eq!(marks[0].note.as_deref(), Some("call the plumber"));
    let detail = db
        .get_frame(marks[0].frame_id)
        .await
        .unwrap()
        .expect("frame stored");
    assert_eq!(detail.capture_trigger, Some(CaptureTrigger::Manual));

    kernel.stop_capture().await;
}

/// With capture off, a `capture_now` mark fails honestly (no mark, a clear reason) —
/// the "Capture is off — mark not saved" path (user decision D-capture-off).
#[tokio::test]
async fn add_mark_capture_now_fails_when_capture_off() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);
    let factory: CaptureFactory = Arc::new(|_cfg, rx| {
        Ok(Box::new(DemandCapture {
            rx,
            next_at: 1,
            deny: None,
        }) as Box<dyn CaptureSource>)
    });
    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    // Capture never started.
    let err = kernel.add_mark(None, true, None).await.unwrap_err();
    assert!(err.to_string().contains("capture is off"), "got: {err}");
    assert!(
        store.list_marks().await.unwrap().is_empty(),
        "no mark inserted"
    );
}

/// A privacy-gate / device denial from the capture source propagates its reason and
/// inserts **nothing** (no orphan mark).
#[tokio::test]
async fn add_mark_capture_now_propagates_denial_and_inserts_no_mark() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);
    let factory: CaptureFactory = Arc::new(|_cfg, rx| {
        Ok(Box::new(DemandCapture {
            rx,
            next_at: 1,
            deny: Some("the focused app is excluded from capture"),
        }) as Box<dyn CaptureSource>)
    });
    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );
    kernel.start_capture().await.unwrap();

    let err = kernel.add_mark(None, true, None).await.unwrap_err();
    assert!(
        err.to_string().contains("excluded from capture"),
        "got: {err}"
    );
    assert!(
        store.list_marks().await.unwrap().is_empty(),
        "denied demand inserts no mark"
    );

    kernel.stop_capture().await;
}

/// `add_mark` with an existing `frame_id` (no capture) marks that frame directly, and
/// requires exactly one source.
#[tokio::test]
async fn add_mark_by_frame_id_marks_directly_and_validates_source() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store: Arc<dyn Store> = db.clone();
    let ocr: Arc<dyn OcrProvider> = Arc::new(FakeOcr);
    let factory: CaptureFactory = Arc::new(|_cfg, rx| {
        Ok(Box::new(DemandCapture {
            rx,
            next_at: 1,
            deny: None,
        }) as Box<dyn CaptureSource>)
    });
    let kernel = Kernel::new(
        store.clone(),
        ocr,
        factory,
        tmp.path().join("frames"),
        Readiness::default(),
    );

    let fid = store
        .insert_frame(traits::NewFrame {
            captured_at: 42,
            monitor_index: 0,
            width: 4,
            height: 4,
            image_path: "frames/42.webp".to_string(),
            content_hash: "h42".to_string(),
            app_hint: Some("VS Code".to_string()),
            window_title: Some("repo".to_string()),
            browser_url: None,
            capture_trigger: Some(CaptureTrigger::Timer),
        })
        .await
        .unwrap();

    let mark_id = kernel
        .add_mark(Some(fid), false, None)
        .await
        .expect("mark by id");
    let marks = store.list_marks().await.unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!((marks[0].mark_id, marks[0].frame_id), (mark_id, fid));

    // Neither / both sources is a usage error.
    assert!(kernel.add_mark(None, false, None).await.is_err());
    assert!(kernel.add_mark(Some(fid), true, None).await.is_err());
}
