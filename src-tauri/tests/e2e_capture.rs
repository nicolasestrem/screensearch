//! End-to-end P2 verification (`03 §13.1`): the **real** capture happy path —
//! WGC capture → WinRT OCR → Store → `embed_text` job — with no fakes. Windows- and
//! desktop-gated, so `#[ignore]`d in CI; run locally with
//! `cargo test -p screensearch --test e2e_capture -- --ignored`.
//!
//! This is the "observed running" proof for P2: it drives the same `Kernel` the app
//! wires, against a temp on-disk store, and asserts frames + OCR rows + a JPEG land
//! and an `embed_text` job is enqueued — while **no `vision_tag` job ever is**
//! (`13.3`).

use std::sync::Arc;
use std::time::Duration;

use capture::WgcCapture;
use kernel::{CaptureFactory, Kernel};
use ocr::WinRtOcr;
use store::SqliteStore;
use traits::{CaptureSource, JobKind, OcrProvider, Readiness, Store};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "real WGC + WinRT capture; requires a desktop session, run locally"]
async fn capture_pipeline_stores_frames_ocr_and_enqueues_embed_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("screensearch.db");
    let frames_dir = tmp.path().join("frames");

    let store = Arc::new(SqliteStore::open_path(&db_path).expect("open store"));
    let dyn_store: Arc<dyn Store> = store.clone();

    // Capture quickly and keep every frame so the test doesn't depend on the screen
    // actually changing; disable the lock pause for headless CI runners.
    store
        .set_setting("capture.interval_ms", "300")
        .await
        .unwrap();
    store
        .set_setting("privacy.pause_on_lock", "false")
        .await
        .unwrap();
    // `capture.diff_threshold` is a file-only key (settings v2, `03 §8`): the kernel reads it
    // from `advanced.toml`, not the `settings` table, so it is set the way the app does,
    // through the file and the loader. A `settings` row here would be silently ignored and
    // the test would run at the default threshold.
    std::fs::write(
        tmp.path().join(traits::ADVANCED_CONFIG_FILE),
        "[capture]\ndiff_threshold = 0.0\n",
    )
    .unwrap();
    let advanced = kernel::settings::load_advanced_config(tmp.path());
    assert_eq!(
        advanced.capture.diff_threshold, 0.0,
        "advanced.toml must take effect"
    );

    let ocr: Arc<dyn OcrProvider> = Arc::new(WinRtOcr::spawn().expect("spawn OCR"));
    let factory: CaptureFactory =
        Arc::new(|cfg, rx| Ok(Box::new(WgcCapture::new(cfg, rx)?) as Box<dyn CaptureSource>));

    let kernel = Kernel::new(
        dyn_store,
        ocr,
        factory,
        frames_dir.clone(),
        Readiness::default(),
    );
    kernel.set_advanced_config(advanced);

    kernel.start_capture().await.expect("start capture");
    tokio::time::sleep(Duration::from_secs(3)).await;
    kernel.stop_capture().await;

    // A frame was captured, OCR'd, and stored, with a WebP on disk.
    let frame = store
        .get_frame(1)
        .await
        .unwrap()
        .expect("at least one frame stored");
    assert!(frame.width > 0 && frame.height > 0);
    assert!(
        !frame.image_purged,
        "a freshly captured frame keeps its image"
    );
    let abs = tmp.path().join(&frame.image_path);
    assert!(
        frame.image_path.ends_with(".webp"),
        "capture stored as WebP, got {}",
        frame.image_path
    );
    assert!(abs.exists(), "webp written at {}", frame.image_path);

    // The stored file is a real, decodable WebP — and at NATIVE resolution (default
    // storage.max_width = 0 → no downscale), so an ultra-wide capture stays legible. The
    // decoded dimensions must equal the captured frame's.
    let decoded = image::open(&abs).expect("stored capture decodes as a valid image");
    eprintln!(
        "captured + stored native WebP: {}x{} at {}",
        decoded.width(),
        decoded.height(),
        frame.image_path
    );
    assert_eq!(
        (decoded.width(), decoded.height()),
        (frame.width, frame.height),
        "native capture is stored without downscaling"
    );

    // The frame_text row exists (raw/content text may be empty on a blank screen —
    // that's fine); content_text is a passthrough copy of raw_text in PR2 (03 §3b).
    assert!(
        frame.raw_text.is_some(),
        "frame_text row present for the frame"
    );
    assert_eq!(frame.raw_text, frame.content_text);

    // An embed_text job was enqueued; NO vision_tag job ever is (13.3).
    let stats = store.job_stats().await.unwrap();
    assert!(stats.pending >= 1, "embed_text enqueued, got {stats:?}");
    assert!(store
        .claim_jobs(&[JobKind::VisionTag], 16, i64::MAX)
        .await
        .unwrap()
        .is_empty());
}
