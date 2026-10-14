//! `uia` — [`traits::OcrProvider`] backed by Windows **UI Automation** (`docs/0.2.0.md`
//! #48). It reads the *foreground/target window's* accessibility text — more structured
//! and higher-fidelity than full-screen OCR — and returns it through the same
//! [`OcrResult`] contract OCR uses, so the capture loop, the PR3 text filter, embeddings,
//! and retrieval all flow unchanged. Spans are tagged `source = TextSource::Uia` and
//! `role = TextRole::Unknown` (PR3 classifies roles); `engine = "uia"` lets the store set
//! `frame_text.primary_source = 'uia'`.
//!
//! ## Threading
//! UI Automation is COM and free-threaded; Microsoft recommends an **MTA**. A dedicated
//! long-lived worker thread ([`worker`]) owns the `IUIAutomation` instance under
//! `COINIT_MULTITHREADED`; [`UiaTextProvider::recognize`] dispatches plain `Send` data to
//! it over a channel (no COM pointer ever crosses a thread boundary) and enforces a hard
//! timeout against a wedged worker.
//!
//! ## Fallback (in the composition root, not here)
//! This crate is the **primary** UIA provider only. The OCR fallback is composed in
//! `src-tauri` (the only place that may wire two concrete impls, `03 §2`): when UIA errors,
//! exceeds its latency budget, or yields too little text, `recognize` returns `Err` and the
//! composite falls back to OCR for that frame.
//!
//! ## Privacy
//! UIA reads the on-screen accessibility text of the target window — the same privacy class
//! as OCR. Two UIA-specific guards in [`worker`]: password fields (`CurrentIsPassword`) are
//! never read, and offscreen/occluded elements (`CurrentIsOffscreen`) plus spans outside
//! the target-window rect are dropped, preserving OCR's "only what was visible" parity.
//!
//! Windows-only by design — no cross-platform fallback (`04` guardrails).

pub mod breaker;
pub mod classify;
mod geometry;
pub mod input;
mod monitors;
mod window;
mod worker;

pub use window::hwnd_is_chromium;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::oneshot;
use traits::{CapturedFrame, OcrProvider, OcrResult, Result};

/// Only every Nth busy-skip is warned, so a target app under sustained load (many frames
/// falling back to OCR) shows up in the log without one warn line per frame.
const SKIP_WARN_EVERY: u64 = 64;

/// Sentinel `mean_confidence`: UI Automation, like WinRT OCR, exposes no confidence, so we
/// record "unknown" rather than inventing a number (mirrors [`ocr::CONFIDENCE_UNKNOWN`]).
pub const CONFIDENCE_UNKNOWN: f32 = -1.0;

/// Per-frame gating thresholds, sourced from clamped settings (never hardcoded, `03 §3b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiaBudget {
    /// Soft wall-clock budget for one tree walk (`capture.uia_latency_budget_ms`); the walk
    /// abandons past it and `recognize` enforces a 2× hard timeout on top.
    pub latency_ms: u64,
    /// Minimum gathered text length below which the read is treated as a thin yield
    /// (`capture.uia_min_text_chars`) → `Err` → OCR fallback for that frame.
    pub min_text_chars: usize,
    /// Hard cap on accessibility nodes visited per walk (`capture.uia_max_nodes`, `07` #71).
    pub max_nodes: u32,
    /// Max live `TextPattern` visible-range reads per walk
    /// (`capture.uia_max_textpattern_calls`, `07` #71) — the one uncacheable cross-process cost.
    pub max_textpattern_calls: u32,
    /// Walk the control view (`true`) rather than the raw view (`capture.uia_view_control_only`,
    /// `07` #71). Control view collapses a Chromium page's per-text-run node explosion.
    pub control_view: bool,
}

/// UI Automation text provider backed by a dedicated COM MTA worker thread.
///
/// `SyncSender` is itself `Sync` (when the message is `Send`), so the provider is `Sync` —
/// which the `OcrProvider` trait requires — with no lock needed; `try_send` takes `&self`.
/// The channel is bounded (capacity 1) and paired with `in_flight` so at most one walk runs
/// and at most one is queued — a slow walk can no longer let triggers pile up an unbounded
/// backlog against the target app.
pub struct UiaTextProvider {
    tx: mpsc::SyncSender<worker::Request>,
    /// Set by the worker for the duration of a walk; `recognize` reads it to skip a frame to
    /// OCR instead of queueing behind an in-progress walk.
    in_flight: Arc<AtomicBool>,
    /// Monotonic count of frames skipped because a walk was already running (rate-limited warn).
    skipped: AtomicU64,
    budget: UiaBudget,
    /// Provider-level shutdown flag shared with the worker. [`Self::shutdown`] sets it so a
    /// walk in progress bails at its next checkpoint and queued walks are refused — the client
    /// can then be released promptly (see the crate-level lifecycle note).
    shutdown: Arc<AtomicBool>,
    /// The in-flight request's cancel flag, so [`Self::shutdown`] can also abandon the *current*
    /// walk (belt-and-suspenders with `shutdown`, which the worker already checks).
    current_cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// One-shot worker-exit signal: the `Sender` half lives in the worker thread closure, so
    /// this `Receiver` reports `Disconnected` the moment the "uia-mta" thread returns (COM
    /// released). Taken once by teardown's logger / the lifecycle test.
    exit_rx: Mutex<Option<mpsc::Receiver<()>>>,
}

/// A walk's outcome plus how it ended, for the per-app circuit breaker. The composite calls
/// [`UiaTextProvider::recognize_detailed`] to get the `end`; the plain [`OcrProvider::recognize`]
/// trait method delegates to it and discards the `end`.
pub struct UiaOutcome {
    pub result: Result<OcrResult>,
    pub end: breaker::WalkEnd,
}

impl UiaTextProvider {
    /// Spawns the MTA worker and creates the `IUIAutomation` object once. This **is** the
    /// capability probe: it returns `Err` when UI Automation cannot be initialized on this
    /// session, so the caller can compose without a UIA arm (OCR carries every frame).
    pub fn spawn(budget: UiaBudget) -> Result<Self> {
        // Bounded (capacity 1): combined with the worker-owned `in_flight` flag, the worker
        // can have at most one walk running and one queued, never an unbounded backlog.
        let (tx, rx) = mpsc::sync_channel::<worker::Request>(1);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        // Exit signal: the worker closure owns `exit_tx` and drops it on return, so `exit_rx`
        // reports `Disconnected` exactly when the "uia-mta" thread exits (COM released) — on
        // every path, including a panic. No value is ever sent.
        let (exit_tx, exit_rx) = mpsc::channel::<()>();
        let in_flight = Arc::new(AtomicBool::new(false));
        let worker_in_flight = in_flight.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = shutdown.clone();

        std::thread::Builder::new()
            .name("uia-mta".to_string())
            .spawn(move || {
                let _exit = exit_tx; // dropped on thread return → signals `exit_rx`
                worker::worker_main(rx, ready_tx, budget, worker_in_flight, worker_shutdown);
            })
            .map_err(|e| anyhow::anyhow!("failed to spawn UIA MTA thread: {e}"))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                tx,
                in_flight,
                skipped: AtomicU64::new(0),
                budget,
                shutdown,
                current_cancel: Mutex::new(None),
                exit_rx: Mutex::new(Some(exit_rx)),
            }),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(anyhow::anyhow!("UIA worker exited during init: {e}")),
        }
    }

    /// Recognize a frame and report how the walk ended (for the circuit breaker). An
    /// over-budget walk that still produced text returns `Ok` in `result` **and**
    /// `WalkEnd::Completed { over_budget: true }` in `end`.
    pub async fn recognize_detailed(&self, frame: &CapturedFrame) -> UiaOutcome {
        // A walk is already running: skip this frame to OCR rather than queue behind it. This
        // is the core backlog guard — without it, every trigger that fired during a slow walk
        // (and after the hard timeout below abandoned its future) piled another walk onto the
        // worker, so the target app was hammered continuously and never recovered.
        if self.in_flight.load(Ordering::Acquire) {
            let n = self.skipped.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(SKIP_WARN_EVERY) {
                tracing::warn!(
                    skipped = n,
                    "UIA busy; frames falling back to OCR (target app under load, rate-limited)"
                );
            }
            return UiaOutcome {
                result: Err(anyhow::anyhow!("UIA worker busy — fall back to OCR")),
                end: breaker::WalkEnd::Busy,
            };
        }

        // Hard ceiling against a wedged worker, on top of the worker's own soft budget.
        let timeout = Duration::from_millis(self.budget.latency_ms.saturating_mul(2).max(1));
        // Per-request cancel flag: on the hard timeout below we set it so the worker abandons
        // the wedged walk at its next checkpoint instead of hammering the target to completion.
        let cancel = Arc::new(AtomicBool::new(false));
        *self.current_cancel.lock().expect("uia current_cancel lock") = Some(cancel.clone());

        // `try_send` on the bounded channel never blocks the executor: if the single slot is
        // already occupied (a request is queued) it errors → OCR fallback for this frame. The
        // reply is a `tokio` oneshot awaited directly — no `spawn_blocking` task per frame. On
        // a hard timeout the receiver is dropped here and the worker's later `send` no-ops.
        let (resp_tx, resp_rx) = oneshot::channel();
        if self
            .tx
            .try_send(worker::Request {
                width: frame.width,
                height: frame.height,
                target_rect: frame.target_rect,
                monitor_index: frame.monitor_index,
                foreground_hwnd: frame.foreground_hwnd,
                cancel: cancel.clone(),
                resp: resp_tx,
            })
            .is_err()
        {
            return UiaOutcome {
                result: Err(anyhow::anyhow!(
                    "UIA worker busy or gone — fall back to OCR"
                )),
                end: breaker::WalkEnd::Busy,
            };
        }

        match tokio::time::timeout(timeout, resp_rx).await {
            Ok(Ok(reply)) => {
                let ok = reply.outcome.is_ok();
                UiaOutcome {
                    result: reply.outcome,
                    end: breaker::WalkEnd::Completed {
                        ok,
                        over_budget: reply.over_budget,
                    },
                }
            }
            // Worker dropped the response (panicked mid-walk): not the target's fault → Busy
            // (Neutral to the breaker), fall back to OCR for this frame.
            Ok(Err(_)) => UiaOutcome {
                result: Err(anyhow::anyhow!("UIA worker dropped the response")),
                end: breaker::WalkEnd::Busy,
            },
            Err(_) => {
                // Abandon the wedged walk: it stops issuing calls into the (struggling) target
                // at its next checkpoint. Can't abort a call already blocked in the target, but
                // the worker's per-call COM timeouts bound that to ~one latency budget.
                cancel.store(true, Ordering::Release);
                UiaOutcome {
                    result: Err(anyhow::anyhow!(
                        "UIA recognize exceeded {} ms hard timeout",
                        timeout.as_millis()
                    )),
                    end: breaker::WalkEnd::HardTimeout,
                }
            }
        }
    }

    /// Ask the worker to abandon any in-flight walk and refuse queued ones, so the UI
    /// Automation client can be released promptly. Idempotent. The worker thread itself exits
    /// only when the last `SyncSender` clone drops (all provider `Arc`s gone) → the channel
    /// closes → `CoUninitialize`; this just makes that exit prompt under a heavy walk.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(cancel) = self
            .current_cancel
            .lock()
            .expect("uia current_cancel lock")
            .as_ref()
        {
            cancel.store(true, Ordering::Release);
        }
    }

    /// Takes the one-shot worker-exit `Receiver` (see [`Self::exit_rx`]). Returns `None` after
    /// the first call. Its `recv`/`recv_timeout` completes (as `Disconnected`) when the
    /// "uia-mta" thread has returned and the COM client is released.
    pub fn take_exit_signal(&self) -> Option<mpsc::Receiver<()>> {
        self.exit_rx.lock().expect("uia exit_rx lock").take()
    }
}

#[async_trait]
impl OcrProvider for UiaTextProvider {
    async fn recognize(&self, frame: &CapturedFrame) -> Result<OcrResult> {
        self.recognize_detailed(frame).await.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::sync::Arc;
    use traits::{CaptureTrigger, TextSource};

    fn frame(w: u32, h: u32) -> CapturedFrame {
        CapturedFrame {
            monitor_index: 0,
            width: w,
            height: h,
            captured_at: 1,
            pixels: Arc::new(RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]))),
            content_hash: "test".to_string(),
            app_hint: None,
            window_title: None,
            // Full-screen target rect: UIA only runs when this frame's monitor holds the
            // foreground window (a `None` rect now falls back to OCR), so the live test must
            // supply one to exercise the real UIA path.
            target_rect: Some([0.0, 0.0, 1.0, 1.0]),
            // `None` ⇒ the worker skips the focus-change check (it can't know the live HWND).
            foreground_hwnd: None,
            trigger: CaptureTrigger::Timer,
            demanded: false,
        }
    }

    /// Real UI Automation against the live desktop's foreground window: must spawn, return
    /// `engine = "uia"` with the unknown-confidence sentinel, and every emitted span's bbox
    /// normalized to `[0,1]` and tagged `TextSource::Uia`. Since the `BuildUpdatedCache`
    /// cache-batched walk (`07` #71) is exercised **only** here (never in CI), this is the
    /// acceptance gate: it also asserts the walk returns well inside the hard timeout and prints
    /// the yield (span/char counts) for manual inspection. `#[ignore]`d in CI (needs a real
    /// desktop session); run locally with `cargo test -p uia -- --ignored --nocapture` **with a
    /// native (non-Chromium) window foreground** — a `Chrome_WidgetWin_*` foreground now bails to
    /// OCR by design (`07` #93), which would make the `Ok`/`engine = "uia"` assertions fail.
    #[tokio::test]
    #[ignore = "requires a real desktop (UI Automation); run locally"]
    async fn uia_provider_spawns_and_recognizes_foreground() {
        let latency_ms = 150_u64;
        let provider = UiaTextProvider::spawn(UiaBudget {
            latency_ms,
            min_text_chars: 0,
            max_nodes: 4000,
            max_textpattern_calls: 64,
            control_view: true,
        })
        .expect("spawn uia worker");
        let t0 = std::time::Instant::now();
        let result = provider
            .recognize(&frame(1920, 1080))
            .await
            .expect("uia recognize ok");
        let elapsed = t0.elapsed();
        assert_eq!(result.engine, "uia");
        assert_eq!(result.mean_confidence, CONFIDENCE_UNKNOWN);
        // The cache-batched per-node walk must complete comfortably within the hard ceiling
        // (`recognize` enforces 2× the latency budget); a generous slack keeps it non-flaky.
        assert!(
            elapsed < std::time::Duration::from_millis(latency_ms * 2 + 500),
            "uia walk took {elapsed:?}, over the hard-timeout ceiling"
        );
        for span in &result.spans {
            assert!(
                (0.0..=1.0).contains(&span.x)
                    && (0.0..=1.0).contains(&span.y)
                    && span.x + span.w <= 1.0 + 1e-6
                    && span.y + span.h <= 1.0 + 1e-6,
                "span bbox not normalized: {span:?}"
            );
            assert!(matches!(span.source, TextSource::Uia));
        }
        // The BuildUpdatedCache path must actually yield text on a normal desktop foreground
        // window (this catches a silent cached-getter regression that returns nothing).
        println!(
            "uia live walk: {} spans, {} chars, elapsed {elapsed:?}",
            result.spans.len(),
            result.text.chars().count()
        );

        // `recognize_detailed` must report a `Completed` end (the breaker input) for a walk
        // that returned — over_budget may be either way depending on the live tree.
        let detailed = provider.recognize_detailed(&frame(1920, 1080)).await;
        assert!(
            matches!(
                detailed.end,
                breaker::WalkEnd::Completed { .. } | breaker::WalkEnd::Busy
            ),
            "unexpected walk end: {:?}",
            detailed.end
        );
    }

    /// Lifecycle: a provider whose worker is `shutdown()` and then dropped must let the
    /// "uia-mta" thread exit (releasing the `IUIAutomation` client) promptly — this is the fix
    /// for "disabling capture doesn't stop the Chromium hangs". The exit-signal channel is the
    /// deterministic hook (no sleeps): its `recv` returns `Disconnected` when the thread
    /// returns. `#[ignore]`d in CI because `spawn` does `CoCreateInstance(CUIAutomation)`
    /// (needs a real desktop session); run locally with `cargo test -p uia -- --ignored`.
    #[tokio::test]
    #[ignore = "requires a real desktop (UI Automation); run locally"]
    async fn uia_worker_exits_on_shutdown() {
        let provider = UiaTextProvider::spawn(UiaBudget {
            latency_ms: 150,
            min_text_chars: 0,
            max_nodes: 4000,
            max_textpattern_calls: 64,
            control_view: true,
        })
        .expect("spawn uia worker");
        let exit = provider.take_exit_signal().expect("exit signal present");
        provider.shutdown();
        drop(provider); // last Arc/SyncSender gone → channel closes → worker returns
        let ended = exit.recv_timeout(std::time::Duration::from_secs(5));
        assert!(
            matches!(ended, Err(std::sync::mpsc::RecvTimeoutError::Disconnected)),
            "uia-mta thread did not exit within 5s after shutdown+drop: {ended:?}"
        );
    }
}
