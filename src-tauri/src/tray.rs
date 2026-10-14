//! System tray + quick actions (0.3.2 PR3, #56/#57; `03 §7d`, `docs/0.3.2.md` D3/D4/D5).
//!
//! The tray is the app's **passive lifecycle surface**: the icon *displays* live capture
//! state (capturing / paused / error) and the menu drives the same commands the UI does —
//! nothing here notifies, nudges, or counts (D4). All state is fed from the kernel event
//! bus via [`crate::run`]'s `forward_events` (`on_readiness` / `on_sidecar_status` /
//! `on_job_stats`) — there is **no poller** (`03 §7d`, the "same state that drives the
//! StatusRail" line).
//!
//! Sister-app reuse (D5, `screensearch-v2@d865c3d`) is by *pattern*, not code: retained
//! `MenuItem` handles mutated with `set_text` (never a menu rebuild); the atomic
//! pause-toggle with rollback (`toggle_capture`); `try_state` everywhere so a startup /
//! teardown race degrades to a no-op instead of panicking. Its pause/resume *notifications*
//! are excluded by D4.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use traits::{
    ComponentStatus, ModelLane, Readiness, SidecarState, SidecarStatus, Toast, ToastLevel,
    VisionTarget,
};

use crate::AppState;

/// Menu item ids (match against `MenuEvent.id`).
const ID_OPEN: &str = "tray.open";
const ID_PAUSE: &str = "tray.pause";
const ID_MODEL: &str = "tray.model";
const ID_VISION: &str = "tray.vision";
const ID_UPDATES: &str = "tray.updates";
const ID_QUIT: &str = "tray.quit";

/// Internal `settings`-table marker (outside the typed [`traits::Settings`] struct, like
/// `api.token`): set once the one-time close-to-tray toast has been shown, so it never
/// repeats across restarts.
const TOAST_DONE_KEY: &str = "app.tray_toast_done";

/// The 32×32 app icon the per-state tray variants are composed from at runtime.
const TRAY_ICON_BASE: &[u8] = include_bytes!("../icons/32x32.png");

// Status-dot colors, matching the app's functional tones (tokens.css `--ok` / `--ink-muted`
// / `--danger`). Paused uses the neutral ink tone, never a warning color — the tray is
// non-shaming (D4). These are an OS-native PNG, not the web UI, so the "tokens only" rule
// (`UI_REFERENCE §2`, D12) does not apply; the values are mirrored here only for coherence.
const DOT_CAPTURING: [u8; 4] = [0x7f, 0xa8, 0x8b, 0xff];
const DOT_PAUSED: [u8; 4] = [0x9a, 0x8f, 0x7a, 0xff];
const DOT_ERROR: [u8; 4] = [0xc5, 0x52, 0x4b, 0xff];
/// `--bg-base`, the 1px ring drawn around the dot so it reads on any wallpaper.
const DOT_RING: [u8; 4] = [0x15, 0x12, 0x0d, 0xff];

/// Which live capture state the tray glyph + tooltip show.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TrayVisual {
    Capturing,
    Paused,
    Error,
}

/// Maps capture readiness → the tray visual (`03 §7d`, usage review 2026-08-01 §3.4).
/// `Ready` = capturing; `Unavailable`/`Error` = a capture problem; everything else
/// (not-yet-probed, stopped, coming up) reads as paused. Backend boot normally crosses
/// this state only briefly, while an explicit stop remains visible until capture resumes.
fn tray_visual(status: ComponentStatus) -> TrayVisual {
    match status {
        ComponentStatus::Ready => TrayVisual::Capturing,
        ComponentStatus::Unavailable | ComponentStatus::Error => TrayVisual::Error,
        ComponentStatus::Unknown | ComponentStatus::Disabled | ComponentStatus::Initializing => {
            TrayVisual::Paused
        }
    }
}

fn pause_label(capturing: bool) -> &'static str {
    if capturing {
        "Pause capture"
    } else {
        "Resume capture"
    }
}

fn model_label(loaded: bool) -> &'static str {
    if loaded {
        "Unload answer model"
    } else {
        "Load answer model"
    }
}

fn vision_label(active: bool) -> &'static str {
    if active {
        "Stop vision tagging"
    } else {
        "Start vision tagging"
    }
}

fn tooltip_for(visual: TrayVisual) -> &'static str {
    match visual {
        TrayVisual::Capturing => "ScreenSearch — capturing",
        TrayVisual::Paused => "ScreenSearch — paused",
        TrayVisual::Error => "ScreenSearch — capture error",
    }
}

/// Managed tray state. Holds the `TrayIcon` + the three mutable `MenuItem` handles (updated
/// in place with `set_text`, never a menu rebuild) and the three pre-composed icon variants,
/// plus the atomics the menu handlers + the sync `CloseRequested` path read/flip.
pub struct TrayState {
    tray: TrayIcon<Wry>,
    pause_item: MenuItem<Wry>,
    model_item: MenuItem<Wry>,
    vision_item: MenuItem<Wry>,
    icon_capturing: Image<'static>,
    icon_paused: Image<'static>,
    icon_error: Image<'static>,
    /// Last-known capture-on state — the source for the atomic pause toggle (re-synced
    /// authoritatively from every `ReadinessChanged`).
    capture_running: AtomicBool,
    /// Whether the answer model is the resident sidecar model.
    answer_loaded: AtomicBool,
    /// Whether any `vision_tag` job is pending/running (drives the vision label).
    vision_active: AtomicBool,
    /// Cached `app.close_to_tray` for the synchronous `CloseRequested` handler.
    close_to_tray: AtomicBool,
    /// A close-to-tray hide has happened this session (gates the one-time toast so it never
    /// fires before the behavior it explains actually occurred).
    hidden_to_tray: AtomicBool,
    /// The one-time close-to-tray toast has not yet been shown (seeded from `TOAST_DONE_KEY`).
    restore_toast_pending: AtomicBool,
}

/// Builds the tray and manages it. Logs and continues on failure — a missing tray must
/// never take the app down (and `close_to_tray_enabled` then returns false, so window
/// close still quits cleanly rather than hiding into nothing).
pub fn init(
    app: &AppHandle,
    settings: &traits::Settings,
    readiness: &Readiness,
    vision_active: bool,
) {
    match build(app, settings, readiness, vision_active) {
        Ok(state) => {
            app.manage(state);
        }
        Err(e) => tracing::error!(error = %e, "tray init failed; continuing without a tray icon"),
    }
}

fn build(
    app: &AppHandle,
    settings: &traits::Settings,
    readiness: &Readiness,
    vision_active: bool,
) -> tauri::Result<TrayState> {
    let base = decode_base();
    let icon_capturing = Image::new_owned(
        compose_rgba(&base, DOT_CAPTURING),
        base.width(),
        base.height(),
    );
    let icon_paused =
        Image::new_owned(compose_rgba(&base, DOT_PAUSED), base.width(), base.height());
    let icon_error = Image::new_owned(compose_rgba(&base, DOT_ERROR), base.width(), base.height());

    let visual = tray_visual(readiness.capture.status);
    let capturing = matches!(visual, TrayVisual::Capturing);

    // Exactly the six `03 §7d` items (Open · Pause/Resume · Load/Unload answer model ·
    // Start/Stop vision tagging · Check for updates · Quit) — no status readout item.
    let open_item = MenuItem::with_id(app, ID_OPEN, "Open ScreenSearch", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, ID_PAUSE, pause_label(capturing), true, None::<&str>)?;
    let model_item = MenuItem::with_id(app, ID_MODEL, model_label(false), true, None::<&str>)?;
    let vision_item = MenuItem::with_id(
        app,
        ID_VISION,
        vision_label(vision_active),
        true,
        None::<&str>,
    )?;
    let updates_item = MenuItem::with_id(app, ID_UPDATES, "Check for updates", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, ID_QUIT, "Quit ScreenSearch", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&open_item)
        .separator()
        .item(&pause_item)
        .item(&model_item)
        .item(&vision_item)
        .separator()
        .item(&updates_item)
        .item(&quit_item)
        .build()?;

    let initial_icon = match visual {
        TrayVisual::Capturing => icon_capturing.clone(),
        TrayVisual::Paused => icon_paused.clone(),
        TrayVisual::Error => icon_error.clone(),
    };

    let tray = TrayIconBuilder::with_id("main")
        .icon(initial_icon)
        .tooltip(tooltip_for(visual))
        .menu(&menu)
        // Left-click summons the window (Open); the menu is the right-click gesture.
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_icon_event)
        .build(app)?;

    let toast_done = read_toast_done(app);

    Ok(TrayState {
        tray,
        pause_item,
        model_item,
        vision_item,
        icon_capturing,
        icon_paused,
        icon_error,
        capture_running: AtomicBool::new(capturing),
        answer_loaded: AtomicBool::new(false),
        // Seeded from the durable queue so a restart with a leftover backlog opens on
        // "Stop vision tagging" (not "Start"); kept in sync by `on_job_stats` thereafter.
        vision_active: AtomicBool::new(vision_active),
        close_to_tray: AtomicBool::new(settings.app_close_to_tray),
        hidden_to_tray: AtomicBool::new(false),
        restore_toast_pending: AtomicBool::new(!toast_done),
    })
}

/// Decodes the bundled base icon to RGBA8; an unreadable asset degrades to a transparent
/// 32×32 canvas (the dot still renders) rather than failing tray construction.
fn decode_base() -> image::RgbaImage {
    match image::load_from_memory(TRAY_ICON_BASE) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            tracing::warn!(error = %e, "tray base icon decode failed; using blank canvas");
            image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 0, 0, 0]))
        }
    }
}

/// Overlays an opaque, ringed status dot on the bottom-right of the base icon and returns
/// the raw RGBA bytes. Pure (no Tauri types) so the state → pixels mapping is unit-testable.
fn compose_rgba(base: &image::RgbaImage, dot: [u8; 4]) -> Vec<u8> {
    let (w, h) = base.dimensions();
    let mut img = base.clone();
    let r = ((w as f32) * 0.30).round() as i32;
    let cx = w as i32 - r - 1;
    let cy = h as i32 - r - 1;
    for y in (cy - r - 1)..=(cy + r + 1) {
        for x in (cx - r - 1)..=(cx + r + 1) {
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                continue;
            }
            let d2 = (x - cx) * (x - cx) + (y - cy) * (y - cy);
            if d2 <= r * r {
                img.put_pixel(x as u32, y as u32, image::Rgba(dot));
            } else if d2 <= (r + 1) * (r + 1) {
                img.put_pixel(x as u32, y as u32, image::Rgba(DOT_RING));
            }
        }
    }
    img.into_raw()
}

// ── State feed (called from `forward_events` and boot sync; no poller) ─────────────────

/// Re-syncs the icon, tooltip, pause/resume label and the authoritative `capture_running`
/// flag from a readiness snapshot (`03 §7d`, usage review 2026-08-01 §3.4).
pub fn on_readiness(app: &AppHandle, readiness: &Readiness) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let visual = tray_visual(readiness.capture.status);
    let capturing = matches!(visual, TrayVisual::Capturing);
    state.capture_running.store(capturing, Ordering::SeqCst);
    let icon = match visual {
        TrayVisual::Capturing => &state.icon_capturing,
        TrayVisual::Paused => &state.icon_paused,
        TrayVisual::Error => &state.icon_error,
    };
    let _ = state.tray.set_icon(Some(icon.clone()));
    let _ = state.tray.set_tooltip(Some(tooltip_for(visual)));
    let _ = state.pause_item.set_text(pause_label(capturing));
}

/// Updates the Load/Unload answer-model label. The answer model is "loaded" only when it is
/// the resident sidecar model (single sidecar, one model at a time) — a resident *vision*
/// model correctly reads as "Load answer model".
pub fn on_sidecar_status(app: &AppHandle, status: &SidecarStatus) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let loaded = status.state == SidecarState::Ready && status.lane == Some(ModelLane::Answer);
    state.answer_loaded.store(loaded, Ordering::SeqCst);
    let _ = state.model_item.set_text(model_label(loaded));
}

/// Updates the Start/Stop vision-tagging label from the queue's vision counts. Fed by
/// `JobProgress` **and** `JobCompleted` so the label flips back when the backlog drains.
pub fn on_job_stats(app: &AppHandle, stats: &traits::JobStats) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let active = stats.vision_pending + stats.vision_running > 0;
    state.vision_active.store(active, Ordering::SeqCst);
    let _ = state.vision_item.set_text(vision_label(active));
}

// ── Close-to-tray helpers ───────────────────────────────────────────────────────────

/// Whether closing the main window should hide to tray. Defaults to **false** when the tray
/// failed to build, so window close still quits (never hides into an absent tray).
pub fn close_to_tray_enabled(app: &AppHandle) -> bool {
    app.try_state::<TrayState>()
        .map(|s| s.close_to_tray.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// Caches the `app.close_to_tray` setting for the synchronous `CloseRequested` handler
/// (called after every settings save).
pub fn set_close_to_tray(app: &AppHandle, on: bool) {
    if let Some(state) = app.try_state::<TrayState>() {
        state.close_to_tray.store(on, Ordering::SeqCst);
    }
}

/// Records that a close-to-tray hide happened this session (arms the one-time toast).
pub fn note_hidden_to_tray(app: &AppHandle) {
    if let Some(state) = app.try_state::<TrayState>() {
        state.hidden_to_tray.store(true, Ordering::SeqCst);
    }
}

// ── Window restore + one-time toast ─────────────────────────────────────────────────

/// Shows and focuses the main window (tray Open / left-click / single-instance second
/// launch). Identical show → unminimize → set_focus sequence to the single-instance
/// callback — `show()` first so a tray-hidden window re-appears. Then maybe fires the
/// one-time close-to-tray toast.
pub fn restore_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    maybe_first_restore_toast(app);
}

/// Fires the one-time, non-shaming close-to-tray toast — but only on the first *restore*
/// after a hide actually happened, so the message ("we kept running in the tray") is true
/// and visible (a toast fired at hide-time, when the window is vanishing, would never be
/// seen). Persisted via [`TOAST_DONE_KEY`] so it never repeats across restarts.
fn maybe_first_restore_toast(app: &AppHandle) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if !state.hidden_to_tray.load(Ordering::SeqCst) {
        return;
    }
    if !state.restore_toast_pending.swap(false, Ordering::SeqCst) {
        return;
    }
    let _ = app.emit(
        "toast",
        Toast {
            level: ToastLevel::Info,
            message: "ScreenSearch kept running in the tray while the window was closed \
                      — you can turn this off in Settings → App."
                .to_string(),
        },
    );
    persist_toast_done(app);
}

/// Persists the "toast shown" marker (best-effort, off-thread — a write failure just risks
/// the toast showing once more on the next restore, never a crash).
fn persist_toast_done(app: &AppHandle) {
    let store = app.state::<AppState>().store.clone();
    if let Some(store) = store {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = store.set_setting(TOAST_DONE_KEY, "true").await {
                tracing::warn!(error = %e, "failed to persist tray toast marker");
            }
        });
    }
}

/// Reads the persisted "toast shown" marker at init (a brief synchronous settings read on
/// the setup thread, like the hotkey-chord load).
fn read_toast_done(app: &AppHandle) -> bool {
    let store = app.state::<AppState>().store.clone();
    let Some(store) = store else {
        return false;
    };
    tauri::async_runtime::block_on(async move {
        matches!(store.get_setting(TOAST_DONE_KEY).await, Ok(Some(v)) if v == "true")
    })
}

// ── Event handlers ──────────────────────────────────────────────────────────────────

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        ID_OPEN => restore_main_window(app),
        ID_PAUSE => toggle_capture(app),
        ID_MODEL => toggle_answer_model(app),
        ID_VISION => toggle_vision(app),
        ID_UPDATES => {
            // The single-flight guard in `run_check` makes a double-fire a no-op.
            let app = app.clone();
            tauri::async_runtime::spawn(async move { crate::update::run_check(&app).await });
        }
        // Routes through `RunEvent::ExitRequested` → `graceful_shutdown` (capture stopped,
        // sidecar torn down via the Job-Object lifecycle — no orphaned `llama-server`).
        ID_QUIT => app.exit(0),
        _ => {}
    }
}

fn on_tray_icon_event(tray: &TrayIcon<Wry>, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        restore_main_window(tray.app_handle());
    }
}

/// Atomic pause toggle (sister-app B3 pattern): `fetch_xor(true)` flips `capture_running`
/// and returns the prior value in one op, so two rapid clicks derive opposite targets
/// instead of both reading a stale value. The label flips optimistically; the actual
/// start/stop runs off-thread and rolls the flip + label back on failure. `ReadinessChanged`
/// re-syncs authoritatively regardless.
fn toggle_capture(app: &AppHandle) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let prior = state.capture_running.fetch_xor(true, Ordering::SeqCst);
    let target_running = !prior;
    let _ = state.pause_item.set_text(pause_label(target_running));

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let kernel = app.state::<AppState>().kernel.clone();
        let outcome = match kernel {
            Some(kernel) if target_running => {
                kernel.start_capture().await.map_err(|e| e.to_string())
            }
            Some(kernel) => {
                kernel.stop_capture().await;
                Ok(())
            }
            None => Err("capture unavailable (database not open)".to_string()),
        };
        if let Err(e) = outcome {
            tracing::warn!(error = %e, "tray capture toggle failed; rolling back");
            if let Some(state) = app.try_state::<TrayState>() {
                state.capture_running.store(prior, Ordering::SeqCst);
                let _ = state.pause_item.set_text(pause_label(prior));
            }
        }
    });
}

/// Loads or unloads the answer model (the `load_model(Answer)` / `unload_model` command
/// paths). Unload is supervisor-level (frees whatever is resident); the label re-syncs from
/// `SidecarStatus`.
fn toggle_answer_model(app: &AppHandle) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let loaded = state.answer_loaded.load(Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = if loaded {
            let supervisor = app
                .state::<AppState>()
                .supervisor
                .lock()
                .expect("supervisor slot")
                .clone();
            match supervisor {
                Some(supervisor) => {
                    supervisor.unload().await;
                    Ok(())
                }
                None => Err("inference sidecar not ready yet".to_string()),
            }
        } else {
            let answer = app
                .state::<AppState>()
                .answer
                .lock()
                .expect("answer slot")
                .clone();
            match answer {
                Some(answer) => answer.preload().await.map_err(|e| e.to_string()),
                None => Err("inference sidecar not ready yet".to_string()),
            }
        };
        if let Err(e) = outcome {
            tracing::warn!(error = %e, "tray answer-model toggle failed");
        }
    });
}

/// Starts or stops vision tagging (user decision, 2026-07-05): Start enqueues the untagged
/// backlog (`enqueue_vision` range, capped at `VISION_RANGE_CAP`); Stop cancels pending
/// `vision_tag` jobs (`cancel_vision`). The label re-syncs from the `JobProgress` the kernel
/// emits after each.
fn toggle_vision(app: &AppHandle) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let active = state.vision_active.load(Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let kernel = app.state::<AppState>().kernel.clone();
        let Some(kernel) = kernel else {
            tracing::warn!("tray vision toggle: kernel unavailable");
            return;
        };
        let outcome = if active {
            kernel.cancel_vision().await.map(|_| ())
        } else {
            kernel
                .enqueue_vision(VisionTarget::Range {
                    start: 0,
                    end: crate::now_ms(),
                })
                .await
                .map(|_| ())
        };
        if let Err(e) = outcome {
            tracing::warn!(error = %e, "tray vision toggle failed");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_status_maps_to_visual() {
        assert_eq!(tray_visual(ComponentStatus::Ready), TrayVisual::Capturing);
        assert_eq!(tray_visual(ComponentStatus::Unavailable), TrayVisual::Error);
        assert_eq!(tray_visual(ComponentStatus::Error), TrayVisual::Error);
        assert_eq!(tray_visual(ComponentStatus::Unknown), TrayVisual::Paused);
        assert_eq!(tray_visual(ComponentStatus::Disabled), TrayVisual::Paused);
        assert_eq!(
            tray_visual(ComponentStatus::Initializing),
            TrayVisual::Paused
        );
    }

    #[test]
    fn labels_track_state() {
        assert_eq!(pause_label(true), "Pause capture");
        assert_eq!(pause_label(false), "Resume capture");
        assert_eq!(model_label(true), "Unload answer model");
        assert_eq!(model_label(false), "Load answer model");
        assert_eq!(vision_label(true), "Stop vision tagging");
        assert_eq!(vision_label(false), "Start vision tagging");
    }

    #[test]
    fn composed_icons_differ_per_state() {
        let base = image::RgbaImage::from_pixel(32, 32, image::Rgba([10, 10, 10, 255]));
        let capturing = compose_rgba(&base, DOT_CAPTURING);
        let paused = compose_rgba(&base, DOT_PAUSED);
        let error = compose_rgba(&base, DOT_ERROR);
        // Same dimensions as the base (32×32×4).
        assert_eq!(capturing.len(), 32 * 32 * 4);
        // The status dot makes each state's pixels distinct.
        assert_ne!(capturing, paused);
        assert_ne!(capturing, error);
        assert_ne!(paused, error);
        // The dot actually changed pixels vs. the flat base.
        assert_ne!(capturing, base.into_raw());
    }
}
