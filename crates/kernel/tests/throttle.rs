//! Smart enrichment-throttle integration tests (`docs/0.2.0.md` former PR5, `07` #49).
//!
//! Drives the *real* worker pool through the *real* throttle governor, with a fake
//! pressure probe whose reading the test sets. Proves the end-to-end contract: under
//! sustained pressure the heavy `vision_tag` kind stops being claimed while `embed_text`
//! keeps draining, and everything resumes once pressure recovers — and that with the
//! throttle off (the default) nothing is paused. (The level-2 `embed_text` floor is
//! covered by the pure `ThrottleMachine` unit tests + the worker-gate code, unchanged
//! by 0.3.0 PR4's image-lane removal.)

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use image::RgbaImage;

use kernel::{CaptureFactory, Kernel};
use store::SqliteStore;
use traits::{
    AnswerDelta, AnswerOpts, AnswerProvider, CaptureSource, Embedding, EmbeddingProvider, JobKind,
    NewFrame, NewJob, OcrProvider, OcrResult, PressureProbe, PressureSample, Readiness, Result,
    RetrievedChunk, Settings, Store, VisionAnalysis, VisionProvider,
};

/// A pressure probe whose CPU reading the test sets live (GPU unmonitored, so this also
/// exercises the truthful CPU-only path). `sampled_at` is real wall-clock ms so the
/// governor's dwell timers advance like production.
struct FakePressureProbe {
    cpu_pct: Arc<AtomicU32>,
}

impl PressureProbe for FakePressureProbe {
    fn sample(&self) -> PressureSample {
        PressureSample {
            cpu_pct: self.cpu_pct.load(Ordering::Relaxed) as f32,
            gpu_pct: None,
            gpu_monitored: false,
            // Memory arm absent: this fake exercises the CPU-only path, and a test that
            // wants memory pressure uses the unit tests in `kernel::throttle` instead.
            mem_used_pct: 0.0,
            commit_pct: 0.0,
            avail_phys_mb: 0,
            commit_charge_mb: 0,
            commit_limit_mb: 0,
            mem_monitored: false,
            sampled_at: now_ms(),
        }
    }
    fn gpu_monitored(&self) -> bool {
        false
    }
    fn mem_monitored(&self) -> bool {
        false
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Deterministic embedder: text → fixed vector.
struct OkEmbedder;
#[async_trait]
impl EmbeddingProvider for OkEmbedder {
    fn dim(&self) -> usize {
        store::EMBEDDING_DIM
    }
    async fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Embedding>> {
        Ok(inputs.iter().map(|_| unit()).collect())
    }
    fn text_model_name(&self) -> &str {
        "fake-text"
    }
}

fn unit() -> Embedding {
    let mut v = vec![0.0_f32; store::EMBEDDING_DIM];
    v[0] = 1.0;
    Embedding(v)
}

struct FakeVision;
#[async_trait]
impl VisionProvider for FakeVision {
    async fn analyze(&self, _image: &RgbaImage) -> Result<VisionAnalysis> {
        Ok(VisionAnalysis {
            description: "fake".to_string(),
            activity_type: None,
            app_hint: None,
            confidence: 0.5,
            model: "fake-vision".to_string(),
        })
    }
}

struct NoCapture;
#[async_trait]
impl CaptureSource for NoCapture {
    fn monitors(&self) -> Vec<traits::MonitorInfo> {
        Vec::new()
    }
    async fn next_frame(&mut self) -> Result<Option<traits::CapturedFrame>> {
        Ok(None)
    }
}

struct NoOcr;
#[async_trait]
impl OcrProvider for NoOcr {
    async fn recognize(&self, _f: &traits::CapturedFrame) -> Result<OcrResult> {
        Ok(ocr(""))
    }
}

struct NoAnswer;
#[async_trait]
impl AnswerProvider for NoAnswer {
    async fn answer(
        &self,
        _q: &str,
        _c: &[RetrievedChunk],
        _o: AnswerOpts,
        _tx: tokio::sync::mpsc::Sender<AnswerDelta>,
    ) -> Result<()> {
        Ok(())
    }
}

fn new_frame(captured_at: i64) -> NewFrame {
    NewFrame {
        captured_at,
        monitor_index: 0,
        width: 64,
        height: 64,
        image_path: format!("frames/{captured_at}.jpg"),
        content_hash: format!("hash-{captured_at}"),
        app_hint: None,
        window_title: None,
        browser_url: None,
        capture_trigger: None,
    }
}

fn ocr(text: &str) -> OcrResult {
    OcrResult {
        text: text.to_string(),
        mean_confidence: -1.0,
        engine: "fake".to_string(),
        spans: Vec::new(),
    }
}

fn job(kind: JobKind, frame_id: i64) -> NewJob {
    NewJob {
        kind,
        frame_id: Some(frame_id),
        priority: 0,
        max_attempts: 3,
        not_before: 0,
    }
}

/// Writes a real 8×8 JPEG where the worker resolves `frames/{captured_at}.jpg`.
fn write_jpeg(data_dir: &std::path::Path, captured_at: i64) {
    let abs = data_dir.join("frames").join(format!("{captured_at}.jpg"));
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255])))
        .to_rgb8()
        .save(&abs)
        .unwrap();
}

/// The `UI` master switch (`throttle.enabled`).
fn throttle_settings(enabled: bool) -> Settings {
    Settings {
        throttle_enabled: enabled,
        ..Settings::default()
    }
}

/// The `file-only` throttle knobs (`advanced.toml`, `03 §8`): fast timing so the dwell +
/// sample windows pass in ~1 s, and the text embed lane enabled so the pool claims
/// `embed_text`.
fn throttle_advanced() -> traits::AdvancedConfig {
    traits::AdvancedConfig {
        throttle: traits::ThrottleAdvanced {
            cpu_enter_pct: 80.0,
            cpu_exit_pct: 20.0,
            enter_after_ms: 500,
            exit_after_ms: 500,
            sample_interval_ms: 250,
            embed_text_floor: 1,
            ..Default::default()
        },
        enrich: traits::EnrichAdvanced {
            embed_text: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Polls `done >= want` for up to 5 s.
async fn wait_done(store: &Arc<dyn Store>, want: u64) -> bool {
    for _ in 0..100 {
        if store.job_stats().await.unwrap().done >= want {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Polls the throttle level against `pred` for up to 5 s.
async fn wait_level(kernel: &Kernel, pred: impl Fn(u8) -> bool) -> bool {
    for _ in 0..100 {
        if pred(kernel.throttle_status().level) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// The headline test: under sustained pressure `embed_text` drains while the heavy
/// `vision_tag` kind stays pending; once pressure recovers, the held job drains too. The
/// level is driven through the shared atomic mid-run, so this also proves live workers
/// react to a level change without a pool restart.
#[tokio::test]
async fn throttle_pauses_heavy_enrichment_then_resumes_on_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());

    // One embed_text job (light) and one vision_tag job (heavy). Aggregate job_stats then
    // tells the kinds apart: under throttle exactly the embed_text job (1) reaches `done`.
    let text_fid = store.insert_frame(new_frame(1_000)).await.unwrap();
    store
        .insert_ocr(text_fid, ocr("real document text"))
        .await
        .unwrap();
    store
        .enqueue_job(job(JobKind::EmbedText, text_fid))
        .await
        .unwrap();

    let vis_fid = store.insert_frame(new_frame(3_000)).await.unwrap();
    write_jpeg(&data_dir, 3_000);
    store
        .enqueue_job(job(JobKind::VisionTag, vis_fid))
        .await
        .unwrap();

    // Persist the throttle settings BEFORE the pool starts.
    kernel::settings::save_settings(store.as_ref(), &throttle_settings(true))
        .await
        .unwrap();

    let factory: CaptureFactory =
        Arc::new(|_c, _rx| Ok(Box::new(NoCapture) as Box<dyn CaptureSource>));
    let kernel = Kernel::new(
        store.clone(),
        Arc::new(NoOcr) as Arc<dyn OcrProvider>,
        factory,
        data_dir.join("frames"),
        Readiness::default(),
    );

    kernel.set_advanced_config(throttle_advanced());

    // Probe starts HIGH; start the governor and wait until it reaches level >= 1 BEFORE
    // any worker runs, so the heavy kind is never claimable in this window.
    let cpu = Arc::new(AtomicU32::new(99));
    kernel.set_pressure_probe(Arc::new(FakePressureProbe {
        cpu_pct: cpu.clone(),
    }));
    kernel.reconfigure_throttle().await;
    assert!(
        wait_level(&kernel, |l| l >= 1).await,
        "throttle did not engage under sustained high pressure"
    );

    // Now bring up the providers (pool + vision). Level is already >= 1, so vision_tag is
    // paused; embed_text drains.
    kernel.attach_embedder(Arc::new(OkEmbedder)).await;
    kernel
        .attach_inference(
            Arc::new(FakeVision) as Arc<dyn VisionProvider>,
            Arc::new(NoAnswer) as Arc<dyn AnswerProvider>,
            None,
            None,
        )
        .await;

    // embed_text drains...
    assert!(
        wait_done(&store, 1).await,
        "embed_text did not drain under throttle"
    );
    // ...while vision_tag stays held (give the pool ample time to (not) claim it).
    tokio::time::sleep(Duration::from_millis(600)).await;
    let held = store.job_stats().await.unwrap();
    assert_eq!(
        held.done, 1,
        "only embed_text should drain under throttle: {held:?}"
    );
    assert_eq!(
        held.pending, 1,
        "vision_tag must stay pending under throttle: {held:?}"
    );

    // Recover: drop pressure and wait for the governor to step back to level 0.
    cpu.store(5, Ordering::Relaxed);
    assert!(
        wait_level(&kernel, |l| l == 0).await,
        "throttle did not release after pressure recovered"
    );
    // The held heavy job now drains.
    assert!(
        wait_done(&store, 2).await,
        "vision_tag did not drain after recovery"
    );

    kernel.stop_throttle().await;
    kernel.stop_vision_scheduler().await;
    kernel.stop_workers().await;
}

/// With the throttle off (the default), both kinds drain — proving the claim gate is
/// truly inert when disabled (zero behavioral change), even with a probe injected.
#[tokio::test]
async fn throttle_disabled_drains_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());

    let text_fid = store.insert_frame(new_frame(1_000)).await.unwrap();
    store
        .insert_ocr(text_fid, ocr("real document text"))
        .await
        .unwrap();
    store
        .enqueue_job(job(JobKind::EmbedText, text_fid))
        .await
        .unwrap();
    let vis_fid = store.insert_frame(new_frame(3_000)).await.unwrap();
    write_jpeg(&data_dir, 3_000);
    store
        .enqueue_job(job(JobKind::VisionTag, vis_fid))
        .await
        .unwrap();

    // Throttle disabled, so the pool claims both kinds.
    kernel::settings::save_settings(store.as_ref(), &throttle_settings(false))
        .await
        .unwrap();

    let factory: CaptureFactory =
        Arc::new(|_c, _rx| Ok(Box::new(NoCapture) as Box<dyn CaptureSource>));
    let kernel = Kernel::new(
        store.clone(),
        Arc::new(NoOcr) as Arc<dyn OcrProvider>,
        factory,
        data_dir.join("frames"),
        Readiness::default(),
    );
    kernel.set_advanced_config(throttle_advanced());
    // Even with a probe injected, a disabled throttle never starts the governor.
    kernel.set_pressure_probe(Arc::new(FakePressureProbe {
        cpu_pct: Arc::new(AtomicU32::new(99)),
    }));
    kernel.reconfigure_throttle().await;
    kernel.attach_embedder(Arc::new(OkEmbedder)).await;
    kernel
        .attach_inference(
            Arc::new(FakeVision) as Arc<dyn VisionProvider>,
            Arc::new(NoAnswer) as Arc<dyn AnswerProvider>,
            None,
            None,
        )
        .await;

    assert!(
        wait_done(&store, 2).await,
        "both kinds should drain when the throttle is off: {:?}",
        store.job_stats().await.unwrap()
    );

    kernel.stop_vision_scheduler().await;
    kernel.stop_workers().await;
}

/// Counts the times the governor asked for memory relief. Stands in for the inference
/// supervisor, which cannot be exercised here without a real `llama-server`.
struct CountingRelief {
    calls: Arc<AtomicU32>,
}

#[async_trait]
impl traits::PressureRelief for CountingRelief {
    async fn release_memory(&self) -> bool {
        self.calls.fetch_add(1, Ordering::SeqCst);
        true
    }
}

/// Polls `calls >= want` for up to 5 s.
async fn wait_relief(calls: &Arc<AtomicU32>, want: u32) -> bool {
    for _ in 0..100 {
        if calls.load(Ordering::SeqCst) >= want {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Level 2 is where the throttle stops merely pausing lanes and asks for the resident model's
/// memory back. That request must be **edge-triggered**: once while the level is held (a long
/// starvation window cannot become a kill/reload loop), and again only after a real recovery
/// and a fresh escalation.
#[tokio::test]
async fn level_two_relief_fires_once_per_escalation() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    kernel::settings::save_settings(store.as_ref(), &throttle_settings(true))
        .await
        .unwrap();

    let factory: CaptureFactory =
        Arc::new(|_c, _rx| Ok(Box::new(NoCapture) as Box<dyn CaptureSource>));
    let kernel = Kernel::new(
        store.clone(),
        Arc::new(NoOcr) as Arc<dyn OcrProvider>,
        factory,
        data_dir.join("frames"),
        Readiness::default(),
    );
    kernel.set_advanced_config(throttle_advanced());

    let calls = Arc::new(AtomicU32::new(0));
    kernel.set_pressure_relief(Arc::new(CountingRelief {
        calls: calls.clone(),
    }));
    let cpu = Arc::new(AtomicU32::new(99));
    kernel.set_pressure_probe(Arc::new(FakePressureProbe {
        cpu_pct: cpu.clone(),
    }));
    kernel.reconfigure_throttle().await;

    assert!(
        wait_level(&kernel, |l| l >= 2).await,
        "throttle did not reach Sustained: {:?}",
        kernel.throttle_status()
    );
    assert!(
        wait_relief(&calls, 1).await,
        "reaching level 2 did not ask for memory relief"
    );
    // Hold the level for several sample intervals: the request must not repeat.
    tokio::time::sleep(Duration::from_millis(1_000)).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "relief must fire on entering level 2, not on every sample"
    );

    // Recover fully, then escalate again: that is a new emergency, so it asks again.
    cpu.store(5, Ordering::Relaxed);
    assert!(
        wait_level(&kernel, |l| l == 0).await,
        "throttle did not release after pressure recovered"
    );
    cpu.store(99, Ordering::Relaxed);
    assert!(
        wait_level(&kernel, |l| l >= 2).await,
        "throttle did not escalate again after a new pressure event"
    );
    assert!(
        wait_relief(&calls, 2).await,
        "a second escalation must ask for relief again"
    );

    kernel.stop_throttle().await;
}
