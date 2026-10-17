//! The always-on capture pipeline (`02 §5`, `03 §5`): pull a *changed* frame from
//! the [`CaptureSource`], OCR it on full resolution, lossless-WebP-encode it for storage,
//! write `frames` + `ocr_text`, enqueue an `embed_text` job, and emit a
//! `capture_tick`. This is the cheap half of the system; everything expensive is
//! deferred to the job queue (consumed in P3+).
//!
//! The loop is shell-agnostic and driven by the traits, so it is exercised
//! end-to-end by fakes (`tests/pipeline.rs`) with no Windows APIs involved.

use std::path::PathBuf;
use std::sync::Arc;

use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use tokio::sync::{broadcast, watch};
use traits::{
    CaptureSource, CaptureTick, CapturedFrame, JobKind, NewFrame, NewJob, OcrProvider, Result,
    Store, TextFilterContext,
};

use crate::capture_observations::{
    CaptureInterruptionReason, CaptureObservation, CaptureObservationBuffer,
};
use crate::events::KernelEvent;

/// Why the capture loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureLoopExit {
    /// The user or shell requested capture stop.
    StopRequested,
    /// The capture source ended without an explicit stop request.
    SourceShutdown,
}

/// Shared slot handing a demanded frame's `frame_id` back to a waiting `capture_now`
/// caller (mark-this-moment, `03 §7b`, D8). The kernel registers a fresh one-shot sender
/// before issuing a demand; [`run_capture_loop`] takes it once the demanded frame lands.
pub type PendingDemand = Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<i64>>>>;

/// Everything the capture loop needs besides the [`CaptureSource`] itself. Built
/// by the kernel on [`start_capture`](crate::Kernel::start_capture) and reused for
/// the loop's lifetime.
pub struct LoopCtx {
    pub store: Arc<dyn Store>,
    pub ocr: Arc<dyn OcrProvider>,
    /// Absolute directory under which stored captures (lossless WebP) are written
    /// (`<app-data>/frames`).
    pub frames_dir: PathBuf,
    pub events: broadcast::Sender<KernelEvent>,
    /// `enrich.embed_text` — whether each stored frame enqueues an `embed_text` job.
    pub enrich_embed_text: bool,
    /// `storage.max_width` — captures wider than this are downscaled (aspect kept);
    /// `0` = native (no downscale), keeping ultra-wide captures legible.
    pub max_width: u32,
    /// PR3 attention-filter thresholds (`03 §8`), snapshotted at capture start (a
    /// runtime change applies on the next capture start, like the other capture/storage
    /// settings — `07` #35). `text.chrome_suppress_min_seen`.
    pub chrome_suppress_min_seen: u32,
    /// `text.chrome_protect_min_chars`.
    pub chrome_protect_min_chars: u32,
    /// `text.chrome_region_buckets`.
    pub chrome_region_buckets: u32,
    /// Slot to return a demanded frame's id to a waiting `capture_now` caller (`03 §7b`,
    /// D8). Shared with the kernel; fired only for the one frame flagged `demanded`.
    pub pending_demand: PendingDemand,
}

/// Runs the capture loop until the source yields `None` (shutdown) or `stop` fires.
/// Per-frame errors are logged and the frame is skipped — capture keeps going
/// (`03 §12` "corrupt/oversized frame → mark + skip; capture continues"). Dropping
/// `capture` on return releases the underlying OS capture resources.
pub async fn run_capture_loop(
    capture: Box<dyn CaptureSource>,
    ctx: LoopCtx,
    stop: watch::Receiver<bool>,
) -> CaptureLoopExit {
    run_capture_loop_observed(capture, ctx, stop, None).await
}

/// The controller installs its observer directly, rather than relying on the
/// lossy broadcast event bus. Keep the public LoopCtx/direct-loop seam unchanged.
pub(crate) async fn run_capture_loop_observed(
    mut capture: Box<dyn CaptureSource>,
    ctx: LoopCtx,
    mut stop: watch::Receiver<bool>,
    observations: Option<Arc<std::sync::Mutex<CaptureObservationBuffer>>>,
) -> CaptureLoopExit {
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => {
                tracing::info!("capture loop: stop requested");
                return CaptureLoopExit::StopRequested;
            }
            res = capture.next_frame() => match res {
                Ok(Some(frame)) => {
                    let monitor = frame.monitor_index;
                    if let Err(e) = process_frame(&ctx, frame).await {
                        // No frame content in the log (privacy, 03 §9) — just the error.
                        tracing::warn!(monitor, error = %e, "capture loop: dropping frame");
                        observe_worker_error(&observations);
                    }
                }
                Ok(None) => {
                    tracing::info!("capture loop: source signaled shutdown");
                    return CaptureLoopExit::SourceShutdown;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "capture loop: source error");
                    observe_worker_error(&observations);
                }
            }
        }
    }
}

fn observe_worker_error(observations: &Option<Arc<std::sync::Mutex<CaptureObservationBuffer>>>) {
    if let Some(observations) = observations {
        observations
            .lock()
            .expect("capture observations lock poisoned")
            .record(
                crate::worker_pool::now_ms(),
                CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
            );
    }
}

/// OCR → encode → store one frame, then enqueue enrichment and emit the tick.
/// OCR runs on the **full-resolution** pixels before the storage downscale (`03 §8`).
async fn process_frame(ctx: &LoopCtx, frame: CapturedFrame) -> Result<()> {
    let ocr = ctx.ocr.recognize(&frame).await?;

    let (rel_db_path, abs_path) =
        image_paths(&ctx.frames_dir, frame.captured_at, frame.monitor_index);
    let encode_started = std::time::Instant::now();
    let encoded_bytes = write_webp(frame.pixels.clone(), abs_path, ctx.max_width).await?;
    // Per-frame encode timing: how long the awaited storage encode holds the capture
    // loop, and the stored file size. Privacy-safe: durations + dimensions + byte
    // counts only — no screen content.
    tracing::info!(
        monitor = frame.monitor_index,
        encode_ms = encode_started.elapsed().as_millis() as u64,
        encoded_bytes,
        src_width = frame.width,
        src_height = frame.height,
        "capture encode"
    );

    let frame_id = ctx
        .store
        .insert_frame(NewFrame {
            captured_at: frame.captured_at,
            monitor_index: frame.monitor_index,
            width: frame.width,
            height: frame.height,
            image_path: rel_db_path,
            content_hash: frame.content_hash.clone(),
            app_hint: frame.app_hint.clone(),
            window_title: frame.window_title.clone(),
            browser_url: None, // needs UI Automation — deferred past P2
            capture_trigger: Some(frame.trigger), // why this frame was captured (03 §4)
        })
        .await?;

    // Mark-this-moment (`03 §7b`, D8): hand the demanded frame's id back to the waiting
    // `capture_now` caller as soon as the `frames` row exists — before the (fallible)
    // OCR/enrichment below, so a demanded frame is always markable even if OCR later
    // fails. Only the one demanded monitor's frame carries the flag.
    if frame.demanded {
        if let Some(tx) = ctx
            .pending_demand
            .lock()
            .expect("pending demand lock poisoned")
            .take()
        {
            let _ = tx.send(frame_id);
        }
    }

    // Apply PR3's attention filter in the same write (`03 §3b`): classify spans into
    // roles, store the filtered `content_text` (so embeddings + default search use
    // filtered text), and bump the chrome catalog. `target_rect` came from this frame's
    // monitor at capture time; thresholds are the capture-start snapshot.
    ctx.store
        .insert_ocr_filtered(
            frame_id,
            ocr,
            TextFilterContext {
                target_rect: frame.target_rect,
                chrome_suppress_min_seen: ctx.chrome_suppress_min_seen,
                chrome_protect_min_chars: ctx.chrome_protect_min_chars,
                chrome_region_buckets: ctx.chrome_region_buckets,
            },
        )
        .await?;

    // After the text is stored → enqueue embed_text (priority normal). vision_tag is
    // NEVER auto-enqueued here (03 §5, 13.3).
    if ctx.enrich_embed_text {
        ctx.store
            .enqueue_job(NewJob {
                kind: JobKind::EmbedText,
                frame_id: Some(frame_id),
                priority: 0,
                max_attempts: 3,
                not_before: 0,
            })
            .await?;
    }

    let _ = ctx.events.send(KernelEvent::CaptureTick(CaptureTick {
        frame_id,
        captured_at: frame.captured_at,
        monitor_index: frame.monitor_index,
    }));
    Ok(())
}

/// `(image_path stored in the DB, absolute path to write)`. The DB path is relative
/// to the app-data dir; frames shard into per-day buckets (`frames/day-<n>/…`) by
/// `captured_at` (unix-ms / 86_400_000) — no calendar dependency (recorded in `07`).
fn image_paths(frames_dir: &std::path::Path, captured_at: i64, monitor: u32) -> (String, PathBuf) {
    let day = captured_at.div_euclid(86_400_000);
    let rel_within = format!("day-{day}/{captured_at}-{monitor}.webp");
    let abs = frames_dir.join(&rel_within);
    (format!("frames/{rel_within}"), abs)
}

/// Downscale to `max_width` (aspect-preserved; `0` = native, no downscale) and encode as
/// **lossless WebP**, creating parent dirs. Lossless keeps text crisp (no JPEG ringing on
/// an ultra-wide capture) and compresses flat UI well; the constant alpha of an opaque
/// screen is dropped to RGB. Runs on the blocking pool (CPU + file IO). Returns the
/// encoded file size in bytes.
async fn write_webp(pixels: Arc<RgbaImage>, path: PathBuf, max_width: u32) -> Result<u64> {
    tokio::task::spawn_blocking(move || -> Result<u64> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let resized = maybe_resize(&pixels, max_width);
        let rgb = image::DynamicImage::ImageRgba8(resized).into_rgb8();
        let file = std::io::BufWriter::new(std::fs::File::create(&path)?);
        image::codecs::webp::WebPEncoder::new_lossless(file).write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )?;
        // The byte count is observational (timing log only) — a post-write metadata
        // failure (AV scan holding the file, transient ACL) must not fail the frame,
        // which is already safely on disk. `0` in the log flags the anomaly.
        Ok(std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0))
    })
    .await?
}

/// Clone-and-resize to `max_width` if wider; otherwise clone as-is. `max_width == 0` means
/// native resolution (no downscale).
fn maybe_resize(pixels: &RgbaImage, max_width: u32) -> RgbaImage {
    if max_width == 0 || pixels.width() <= max_width {
        return pixels.clone();
    }
    let ratio = f64::from(max_width) / f64::from(pixels.width());
    let new_h = ((f64::from(pixels.height()) * ratio).round() as u32).max(1);
    image::imageops::resize(
        pixels,
        max_width,
        new_h,
        image::imageops::FilterType::Triangle,
    )
}

#[cfg(test)]
mod tests {
    use super::{image_paths, maybe_resize};
    use image::RgbaImage;
    use std::path::Path;

    /// Stored captures are WebP, sharded into per-day buckets by `captured_at`.
    #[test]
    fn image_paths_are_webp_and_day_sharded() {
        let (rel, abs) = image_paths(Path::new("/data/frames"), 1_700_000_000_000, 0);
        assert!(rel.starts_with("frames/day-"), "day-sharded: {rel}");
        assert!(rel.ends_with(".webp"), "stored as webp: {rel}");
        assert_eq!(abs.extension().and_then(|s| s.to_str()), Some("webp"));
    }

    /// `max_width == 0` is the native sentinel: an ultra-wide frame is stored at full
    /// width so its text stays legible (the readability fix).
    #[test]
    fn native_max_width_zero_does_not_downscale() {
        let img = RgbaImage::new(3440, 1440);
        let out = maybe_resize(&img, 0);
        assert_eq!((out.width(), out.height()), (3440, 1440));
    }

    /// A positive `max_width` below native downscales, preserving aspect ratio.
    #[test]
    fn positive_max_width_downscales_keeping_aspect() {
        let img = RgbaImage::new(3440, 1440);
        let out = maybe_resize(&img, 1720);
        assert_eq!(out.width(), 1720);
        assert_eq!(out.height(), 720, "aspect preserved (1720/3440 * 1440)");
    }

    /// A `max_width` at or above the frame width is a no-op (never upscales).
    #[test]
    fn max_width_at_or_above_native_is_noop() {
        let img = RgbaImage::new(1280, 720);
        let out = maybe_resize(&img, 4000);
        assert_eq!((out.width(), out.height()), (1280, 720));
    }
}
