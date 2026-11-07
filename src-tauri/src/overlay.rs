//! Flow overlay shell integration (0.3.0 PR5).
//!
//! This module owns the Tauri-only surface: global hotkey registration, the hidden
//! always-on-top overlay window, and cross-window events. Search and Ask still go
//! through the existing typed IPC commands; no retrieval path lives here.

use std::sync::Mutex;
use std::time::Instant;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, State, WebviewWindow,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use traits::{HotkeyStatus, MarkToast, OpenMoment, Toast, ToastLevel};

const OVERLAY_LABEL: &str = "overlay";
const MAIN_LABEL: &str = "main";
const OVERLAY_HOTKEY_ID: &str = "overlay.hotkey";
// "Ctrl+Alt+Z": the original "Ctrl+Alt+Space" collided with Claude Desktop's quick-entry
// shortcut. Must match `Settings::default().overlay_hotkey` and the UI `DEFAULT_OVERLAY_HOTKEY`.
const OVERLAY_DEFAULT_CHORD: &str = "Ctrl+Alt+Z";
const MARKS_HOTKEY_ID: &str = "marks.hotkey";
const MARKS_DEFAULT_CHORD: &str = "Ctrl+Alt+M";
const FALLBACK_WIDTH: u32 = 640;
const FALLBACK_HEIGHT: u32 = 460;

/// Which global shortcut a registration attempt is about. Both hotkeys share one
/// registration path, so they also share one set of messages (D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeySlot {
    /// The Flow overlay summon chord (0.3.0 PR5).
    Overlay,
    /// The mark-this-moment chord (0.3.0 PR6; `03 §7b`).
    Marks,
}

impl HotkeySlot {
    fn id(self) -> &'static str {
        match self {
            Self::Overlay => OVERLAY_HOTKEY_ID,
            Self::Marks => MARKS_HOTKEY_ID,
        }
    }

    /// The noun used in user-facing sentences ("Flow overlay hotkey", "mark hotkey").
    fn label(self) -> &'static str {
        match self {
            Self::Overlay => "Flow overlay",
            Self::Marks => "mark",
        }
    }

    fn default_chord(self) -> &'static str {
        match self {
            Self::Overlay => OVERLAY_DEFAULT_CHORD,
            Self::Marks => MARKS_DEFAULT_CHORD,
        }
    }
}

/// Registration bookkeeping for one hotkey.
#[derive(Clone, Default)]
struct HotkeySlotState {
    /// The chord the OS currently holds for us. Only advances on a successful
    /// registration, so `Some` is the honest "this shortcut is live" marker.
    registered: Option<String>,
    /// The chord whose most recent registration attempt failed. Repeating that same
    /// failed chord (an unrelated settings save) is retried silently: the failure stays
    /// visible in the hotkey status, and one toast is enough (D6).
    last_failed: Option<String>,
}

#[derive(Default)]
pub struct OverlayState {
    hotkeys: Mutex<Vec<HotkeyStatus>>,
    /// The Flow overlay chord currently registered (0.3.0 PR5).
    overlay_hotkey: Mutex<HotkeySlotState>,
    /// The mark-this-moment chord currently registered (0.3.0 PR6; `03 §7b`, D6).
    marks_hotkey: Mutex<HotkeySlotState>,
    summon_started: Mutex<Option<Instant>>,
}

impl OverlayState {
    fn slot(&self, slot: HotkeySlot) -> &Mutex<HotkeySlotState> {
        match slot {
            HotkeySlot::Overlay => &self.overlay_hotkey,
            HotkeySlot::Marks => &self.marks_hotkey,
        }
    }
}

/// Trims a chord, substituting `default` when empty — the shell never registers a blank
/// hotkey (`03 §8`; the D6 warning covers a bad/colliding value).
fn normalize_chord<'a>(chord: &'a str, default: &'a str) -> &'a str {
    let trimmed = chord.trim();
    if trimmed.is_empty() {
        default
    } else {
        trimmed
    }
}

#[tauri::command]
pub fn get_hotkey_status(state: State<'_, OverlayState>) -> Vec<HotkeyStatus> {
    state.hotkeys.lock().expect("hotkey status lock").clone()
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) -> Result<(), String> {
    hide_overlay_window(&app)
}

#[tauri::command]
pub fn overlay_shown_ack(state: State<'_, OverlayState>) -> Result<(), String> {
    if let Some(started) = state
        .summon_started
        .lock()
        .expect("overlay summon timer lock")
        .take()
    {
        tracing::info!(
            target: "overlay_perf",
            input_ready_ms = started.elapsed().as_millis() as u64,
            "overlay first paint acknowledged"
        );
    }
    Ok(())
}

#[tauri::command]
pub fn open_moment(app: AppHandle, frame_id: i64) -> Result<(), String> {
    let main = app
        .get_webview_window(MAIN_LABEL)
        .ok_or_else(|| "main window unavailable".to_string())?;
    main.show().map_err(|e| e.to_string())?;
    main.unminimize().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())?;
    app.emit_to(MAIN_LABEL, "open_moment", OpenMoment { frame_id })
        .map_err(|e| e.to_string())?;
    hide_overlay_window(&app)
}

/// The global-shortcut operations the hotkey lifecycle needs. Split out from the app
/// handle so the registration decisions can be driven without a live Tauri app or an OS
/// shortcut table (the 0.4.5 regression suite uses a fake; `03 §7b`, D6).
trait HotkeyEnv {
    fn slot_state(&self, slot: HotkeySlot) -> HotkeySlotState;
    fn set_registered_chord(&self, slot: HotkeySlot, chord: Option<String>);
    fn set_failed_chord(&self, slot: HotkeySlot, chord: Option<String>);
    fn is_registered(&self, chord: &str) -> bool;
    fn register(&self, slot: HotkeySlot, chord: &str) -> Result<(), String>;
    fn unregister(&self, chord: &str) -> Result<(), String>;
    fn set_status(&self, status: HotkeyStatus);
    fn warn(&self, message: String);
}

/// `HotkeyEnv` over the real Tauri app handle: the plugin's shortcut manager plus the
/// `OverlayState` bookkeeping and the toast channel.
struct AppHotkeys<'a, R: Runtime>(&'a AppHandle<R>);

impl<R: Runtime> HotkeyEnv for AppHotkeys<'_, R> {
    fn slot_state(&self, slot: HotkeySlot) -> HotkeySlotState {
        self.0
            .state::<OverlayState>()
            .slot(slot)
            .lock()
            .expect("registered hotkey lock")
            .clone()
    }

    fn set_registered_chord(&self, slot: HotkeySlot, chord: Option<String>) {
        self.0
            .state::<OverlayState>()
            .slot(slot)
            .lock()
            .expect("registered hotkey lock")
            .registered = chord;
    }

    fn set_failed_chord(&self, slot: HotkeySlot, chord: Option<String>) {
        self.0
            .state::<OverlayState>()
            .slot(slot)
            .lock()
            .expect("registered hotkey lock")
            .last_failed = chord;
    }

    fn is_registered(&self, chord: &str) -> bool {
        // The plugin's own registry only. It is true exactly when this process already
        // holds the shortcut through the plugin: the case a re-register must skip.
        self.0.global_shortcut().is_registered(chord)
    }

    fn register(&self, slot: HotkeySlot, chord: &str) -> Result<(), String> {
        match slot {
            HotkeySlot::Overlay => register_overlay_hotkey(self.0, chord),
            HotkeySlot::Marks => register_marks_hotkey(self.0, chord),
        }
    }

    fn unregister(&self, chord: &str) -> Result<(), String> {
        self.0
            .global_shortcut()
            .unregister(chord)
            .map_err(|e| e.to_string())
    }

    fn set_status(&self, status: HotkeyStatus) {
        set_status(self.0, status);
    }

    fn warn(&self, message: String) {
        emit_hotkey_warning(self.0, message);
    }
}

/// The one registration path for both hotkeys, used by the startup (`init_*`) and the
/// settings-save (`reregister_*`) entry points.
///
/// The guard is what the 0.4.5 QA bug needed: the OS had the chord while this process's
/// stored chord was `None` (a duplicate startup attempt failed and the old `init_*`
/// cleared the record), so every later save re-registered, failed with
/// `HotKey already registered`, and toasted. Ownership is now detected before any
/// registration attempt, `init_*` never clears a known-good chord, and a repeated
/// failure is retried silently.
fn apply_hotkey(env: &dyn HotkeyEnv, slot: HotkeySlot, requested: &str) {
    let chord = normalize_chord(requested, slot.default_chord());
    let slot_state = env.slot_state(slot);

    // Already ours: either our stored chord (canonically equal, so a spelling variant
    // cannot slip past) or the plugin's registry. Re-registering would fail as a
    // duplicate, so refresh the status to OK (this also clears a stale warning left by
    // an earlier failed save that reverted to this chord) and stop.
    if is_ours(&slot_state, env, chord) {
        env.set_registered_chord(slot, Some(chord.to_string()));
        env.set_failed_chord(slot, None);
        env.set_status(ok_status(slot.id(), chord));
        return;
    }

    let previous = slot_state.registered.clone();
    match env.register(slot, chord) {
        Ok(()) => {
            if let Some(previous_chord) = previous
                .as_deref()
                .filter(|previous_chord| !same_chord(previous_chord, chord))
            {
                // The new chord is live. Retire the old grab; a failure here only leaves
                // a stale OS shortcut behind, so it is logged, never toasted.
                if let Err(error) = env.unregister(previous_chord) {
                    tracing::warn!(
                        slot = slot.id(),
                        %previous_chord,
                        %error,
                        "failed to unregister previous hotkey"
                    );
                }
            }
            env.set_registered_chord(slot, Some(chord.to_string()));
            env.set_failed_chord(slot, None);
            env.set_status(ok_status(slot.id(), chord));
        }
        Err(error) => {
            // Keep the raw plugin text in the log (it names the Win32/parse failure);
            // only the humanized sentence goes to the status and the toast.
            tracing::warn!(
                slot = slot.id(),
                %chord,
                %error,
                "global hotkey registration failed"
            );
            let reason = humanize_hotkey_error(&error);
            // `registered=false` stays first-class UI state: a failed registration is
            // surfaceable in Settings, never a silent lie (D6).
            env.set_status(failed_status(slot.id(), chord, reason.clone()));
            let repeated = slot_state
                .last_failed
                .as_deref()
                .is_some_and(|failed| same_chord(failed, chord));
            env.set_failed_chord(slot, Some(chord.to_string()));
            if repeated {
                // Same chord that already failed: retried so a released conflict is
                // picked up without an app restart, but silently, so an unrelated
                // settings save cannot repeat the toast (D6 retry policy).
                return;
            }
            let suffix = if previous.is_some() {
                "; still using the previous combination"
            } else {
                ""
            };
            env.warn(format!(
                "Could not register {} hotkey{suffix}: {reason}",
                slot.label()
            ));
        }
    }
}

/// Whether this process already holds `chord`: our stored chord or the plugin registry.
fn is_ours(state: &HotkeySlotState, env: &dyn HotkeyEnv, chord: &str) -> bool {
    state
        .registered
        .as_deref()
        .is_some_and(|registered| same_chord(registered, chord))
        || env.is_registered(chord)
}

/// Parses a chord with the plugin's own parser. `None` only for text the plugin would
/// also reject, so callers fall back to a literal comparison.
fn parse_shortcut(chord: &str) -> Option<Shortcut> {
    chord.parse::<Shortcut>().ok()
}

/// Canonical chord equality: parses both sides and compares the values, so
/// `ctrl+alt+m`, `Alt+Ctrl+M` and ` Ctrl+Alt+M ` all name the same shortcut and cannot
/// trigger a duplicate registration attempt (0.4.5 QA).
fn same_chord(left: &str, right: &str) -> bool {
    match (parse_shortcut(left), parse_shortcut(right)) {
        (Some(left), Some(right)) => left == right,
        _ => left.trim().eq_ignore_ascii_case(right.trim()),
    }
}

/// Turns the global-shortcut plugin's error text into one short sentence a person can
/// act on. The plugin's `Display` embeds Rust `Debug` output (`HotKey { mods:
/// Modifiers(ALT | CONTROL), key: KeyM, id: 589855 }`), which must never reach a toast or
/// the Settings status row, so this classifies the stable English phrases and never
/// echoes the raw text: an unknown error degrades to a generic sentence.
fn humanize_hotkey_error(error: &str) -> String {
    let error = error.to_ascii_lowercase();
    if error.contains("already registered") {
        "another app is already using that shortcut".to_string()
    } else if error.contains("couldn't recognize") || error.contains("unsupported") {
        "that shortcut is not recognized".to_string()
    } else if error.contains("empty token") || error.contains("format") || error.contains("invalid")
    {
        "that shortcut is not valid".to_string()
    } else if error.contains("media key") {
        "Windows refused that shortcut".to_string()
    } else {
        "the shortcut could not be registered".to_string()
    }
}

/// Registers the Flow overlay chord at startup from the persisted setting (0.3.0 PR5;
/// `03 §8`). Idempotent: a chord this process already holds is adopted, not re-registered.
pub fn init_overlay_hotkey<R: Runtime>(app: &AppHandle<R>, chord: &str) {
    apply_hotkey(&AppHotkeys(app), HotkeySlot::Overlay, chord);
}

/// Re-runs the Flow overlay registration after a settings save. Called for every save,
/// so it must be a silent no-op for an unchanged chord (0.4.5 QA).
pub fn reregister_overlay_hotkey<R: Runtime>(app: &AppHandle<R>, chord: &str) {
    apply_hotkey(&AppHotkeys(app), HotkeySlot::Overlay, chord);
}

fn register_overlay_hotkey<R: Runtime>(app: &AppHandle<R>, chord: &str) -> Result<(), String> {
    let chord_for_handler = chord.to_string();
    app.global_shortcut()
        .on_shortcut(chord, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                if let Err(error) = toggle_overlay(app) {
                    tracing::warn!(
                        chord = %chord_for_handler,
                        error = %error,
                        "failed to toggle Flow overlay"
                    );
                }
            }
        })
        .map_err(|e| e.to_string())
}

// ── Mark-this-moment hotkey + quiet toast (0.3.0 PR6; `03 §7b`, D6/D8) ─────────────

/// Registers the mark-this-moment chord at startup (0.3.0 PR6). Same idempotent path as
/// the Flow overlay chord, so a repeated call cannot clear a live registration.
pub fn init_marks_hotkey<R: Runtime>(app: &AppHandle<R>, chord: &str) {
    apply_hotkey(&AppHotkeys(app), HotkeySlot::Marks, chord);
}

/// Re-runs the mark chord registration after a settings save: silent for an unchanged
/// chord, humanized warning for a genuine conflict (0.4.5 QA).
pub fn reregister_marks_hotkey<R: Runtime>(app: &AppHandle<R>, chord: &str) {
    apply_hotkey(&AppHotkeys(app), HotkeySlot::Marks, chord);
}

fn register_marks_hotkey<R: Runtime>(app: &AppHandle<R>, chord: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(chord, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                handle_mark_hotkey(app);
            }
        })
        .map_err(|e| e.to_string())
}

/// Fires a `capture_now` mark on hotkey press and shows the quiet confirmation toast.
/// Runs the async `add_mark` on the tauri runtime; the result drives a non-focus-
/// stealing toast in the overlay window (`03 §7b`, D8) and a `marks_changed` refresh.
fn handle_mark_hotkey<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let kernel = {
            let state = app.state::<crate::AppState>();
            state.kernel.clone()
        };
        let Some(kernel) = kernel else {
            let _ = show_mark_toast(
                &app,
                MarkToast {
                    mark_id: None,
                    level: ToastLevel::Warning,
                    message: "Capture is off — mark not saved".to_string(),
                },
            );
            return;
        };
        match kernel.add_mark(None, true, None).await {
            Ok(mark_id) => {
                let _ = show_mark_toast(
                    &app,
                    MarkToast {
                        mark_id: Some(mark_id),
                        level: ToastLevel::Success,
                        message: "Marked".to_string(),
                    },
                );
                // The Deck's marks query lives in the main window's QueryClient; a
                // cross-window event refreshes the Intentions strip.
                let _ = app.emit("marks_changed", ());
            }
            Err(error) => {
                let _ = show_mark_toast(
                    &app,
                    MarkToast {
                        mark_id: None,
                        level: ToastLevel::Warning,
                        message: map_mark_error(&error.to_string()),
                    },
                );
            }
        }
    });
}

/// Maps a `capture_now` failure to a short, honest user message. "Capture is off" is the
/// common case (the demanded capture had no running worker — user decision D-capture-off).
fn map_mark_error(reason: &str) -> String {
    if reason.contains("capture is off") || reason.contains("capture stopped") {
        "Capture is off — mark not saved".to_string()
    } else {
        format!("Couldn't mark: {reason}")
    }
}

/// Shows the mark confirmation toast in the overlay window **without stealing focus**
/// (user decision D1): the window is made non-focusable before `show()`, so the user
/// keeps typing in their app; clicking the note field focuses it
/// ([`focus_overlay_for_note`]). The overlay is positioned on the foreground monitor,
/// reusing the search-overlay placement.
fn show_mark_toast<R: Runtime>(app: &AppHandle<R>, payload: MarkToast) -> Result<(), String> {
    let overlay = overlay_window(app)?;
    let _ = overlay.set_focusable(false);
    position_overlay(app, &overlay)?;
    overlay.show().map_err(|e| e.to_string())?;
    app.emit_to(OVERLAY_LABEL, "mark_toast", payload)
        .map_err(|e| e.to_string())
}

/// Makes the overlay focusable and focuses it — invoked when the user clicks the mark
/// toast's note field so keystrokes land in the note (the toast is otherwise
/// non-focusable, D1). From then on the normal blur-hide applies.
#[tauri::command]
pub fn focus_overlay_for_note(app: AppHandle) -> Result<(), String> {
    let overlay = overlay_window(&app)?;
    overlay.set_focusable(true).map_err(|e| e.to_string())?;
    overlay.set_focus().map_err(|e| e.to_string())
}

/// Dismisses the mark toast, restoring the overlay to focusable so the next search
/// summon behaves normally.
#[tauri::command]
pub fn dismiss_mark_toast(app: AppHandle) -> Result<(), String> {
    let overlay = overlay_window(&app)?;
    let _ = overlay.set_focusable(true);
    hide_overlay_window(&app)
}

pub fn toggle_overlay<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let overlay = overlay_window(app)?;
    if overlay.is_visible().unwrap_or(false) {
        return hide_overlay_window(app);
    }
    summon_overlay(app, &overlay)
}

fn summon_overlay<R: Runtime>(
    app: &AppHandle<R>,
    overlay: &WebviewWindow<R>,
) -> Result<(), String> {
    *app.state::<OverlayState>()
        .summon_started
        .lock()
        .expect("overlay summon timer lock") = Some(Instant::now());
    position_overlay(app, overlay)?;
    let started = Instant::now();
    // A prior mark toast may have left the window non-focusable (D1); the search overlay
    // always takes focus, so restore it before showing.
    let _ = overlay.set_focusable(true);
    overlay.show().map_err(|e| e.to_string())?;
    overlay.set_focus().map_err(|e| e.to_string())?;
    tracing::info!(
        target: "overlay_perf",
        visible_ms = started.elapsed().as_millis() as u64,
        "overlay window shown"
    );
    app.emit_to(OVERLAY_LABEL, "overlay_shown", ())
        .map_err(|e| e.to_string())
}

pub fn hide_overlay_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let overlay = overlay_window(app)?;
    overlay.hide().map_err(|e| e.to_string())?;
    app.emit_to(OVERLAY_LABEL, "overlay_hidden", ())
        .map_err(|e| e.to_string())
}

fn position_overlay<R: Runtime>(
    app: &AppHandle<R>,
    overlay: &WebviewWindow<R>,
) -> Result<(), String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let target = foreground_point()
        .and_then(|point| {
            monitors
                .iter()
                .find(|monitor| point_in_work_area(point, monitor.work_area()))
                .cloned()
        })
        .or_else(|| app.primary_monitor().ok().flatten())
        .or_else(|| monitors.first().cloned());

    let Some(monitor) = target else {
        return Ok(());
    };

    let area = monitor.work_area();
    let scale = monitor.scale_factor().max(1.0);
    let size = overlay.outer_size().unwrap_or_else(|_| {
        PhysicalSize::new(
            (f64::from(FALLBACK_WIDTH) * scale).round() as u32,
            (f64::from(FALLBACK_HEIGHT) * scale).round() as u32,
        )
    });
    let area_w = area.size.width as i32;
    let area_h = area.size.height as i32;
    let overlay_w = size.width as i32;
    let overlay_h = size.height as i32;
    let x_offset = ((area_w - overlay_w) / 2).max(0);
    let y_offset = (area_h / 6).min((area_h - overlay_h).max(0)).max(0);

    overlay
        .set_position(PhysicalPosition::new(
            area.position.x + x_offset,
            area.position.y + y_offset,
        ))
        .map_err(|e| e.to_string())
}

fn foreground_point() -> Option<(i32, i32)> {
    #[cfg(windows)]
    {
        capture::foreground_window_rect()
            .map(|(left, top, right, bottom)| ((left + right) / 2, (top + bottom) / 2))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn point_in_work_area((x, y): (i32, i32), area: &tauri::PhysicalRect<i32, u32>) -> bool {
    let left = area.position.x;
    let top = area.position.y;
    x >= left && x < left + area.size.width as i32 && y >= top && y < top + area.size.height as i32
}

fn overlay_window<R: Runtime>(app: &AppHandle<R>) -> Result<WebviewWindow<R>, String> {
    app.get_webview_window(OVERLAY_LABEL)
        .ok_or_else(|| "overlay window unavailable".to_string())
}

fn set_status<R: Runtime>(app: &AppHandle<R>, status: HotkeyStatus) {
    let state = app.state::<OverlayState>();
    let mut hotkeys = state.hotkeys.lock().expect("hotkey status lock");
    if let Some(existing) = hotkeys.iter_mut().find(|h| h.id == status.id) {
        *existing = status;
    } else {
        hotkeys.push(status);
    }
}

fn ok_status(id: &str, chord: &str) -> HotkeyStatus {
    HotkeyStatus {
        id: id.to_string(),
        chord: chord.to_string(),
        registered: true,
        error: None,
    }
}

fn failed_status(id: &str, chord: &str, error: String) -> HotkeyStatus {
    HotkeyStatus {
        id: id.to_string(),
        chord: chord.to_string(),
        registered: false,
        error: Some(error),
    }
}

fn emit_hotkey_warning<R: Runtime>(app: &AppHandle<R>, message: String) {
    let _ = app.emit(
        "toast",
        Toast {
            level: ToastLevel::Warning,
            message,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The `Display` text the plugin produces when the OS refuses a chord with Win32
    /// 1409 (`ERROR_HOTKEY_ALREADY_REGISTERED`). Reproduced verbatim, including the Rust
    /// `Debug` payload the 0.4.5 toast leaked.
    fn already_registered_error(chord: &str) -> String {
        let shortcut = parse_shortcut(chord).expect("test chord parses");
        format!("HotKey already registered: {shortcut:?}")
    }

    /// In-memory `HotkeyEnv`. `held` is the plugin registry (this process owns the
    /// shortcut), `foreign` is a chord another app owns, `warnings` collects the toasts
    /// a save would show.
    #[derive(Default)]
    struct FakeHotkeys {
        overlay: RefCell<HotkeySlotState>,
        marks: RefCell<HotkeySlotState>,
        held: RefCell<Vec<Shortcut>>,
        foreign: RefCell<Vec<Shortcut>>,
        attempts: RefCell<Vec<String>>,
        warnings: RefCell<Vec<String>>,
        statuses: RefCell<Vec<HotkeyStatus>>,
    }

    impl FakeHotkeys {
        fn new() -> Self {
            Self::default()
        }

        fn slot_ref(&self, slot: HotkeySlot) -> &RefCell<HotkeySlotState> {
            match slot {
                HotkeySlot::Overlay => &self.overlay,
                HotkeySlot::Marks => &self.marks,
            }
        }

        fn stored(&self, slot: HotkeySlot) -> HotkeySlotState {
            self.slot_ref(slot).borrow().clone()
        }

        fn hold_ours(&self, chord: &str) {
            let shortcut = parse_shortcut(chord).expect("test chord parses");
            self.held.borrow_mut().push(shortcut);
        }

        fn hold_forever(&self, chord: &str) {
            let shortcut = parse_shortcut(chord).expect("test chord parses");
            self.foreign.borrow_mut().push(shortcut);
        }

        fn release_foreign(&self, chord: &str) {
            let shortcut = parse_shortcut(chord).expect("test chord parses");
            self.foreign.borrow_mut().retain(|held| *held != shortcut);
        }

        fn holds(&self, chord: &str) -> bool {
            let shortcut = parse_shortcut(chord).expect("test chord parses");
            self.held.borrow().contains(&shortcut)
        }

        fn attempts(&self) -> Vec<String> {
            self.attempts.borrow().clone()
        }

        fn warnings(&self) -> Vec<String> {
            self.warnings.borrow().clone()
        }

        fn status(&self, slot: HotkeySlot) -> HotkeyStatus {
            self.statuses
                .borrow()
                .iter()
                .find(|status| status.id == slot.id())
                .cloned()
                .expect("status was set")
        }
    }

    impl HotkeyEnv for FakeHotkeys {
        fn slot_state(&self, slot: HotkeySlot) -> HotkeySlotState {
            self.stored(slot)
        }

        fn set_registered_chord(&self, slot: HotkeySlot, chord: Option<String>) {
            self.slot_ref(slot).borrow_mut().registered = chord;
        }

        fn set_failed_chord(&self, slot: HotkeySlot, chord: Option<String>) {
            self.slot_ref(slot).borrow_mut().last_failed = chord;
        }

        fn is_registered(&self, chord: &str) -> bool {
            parse_shortcut(chord).is_some_and(|shortcut| self.held.borrow().contains(&shortcut))
        }

        fn register(&self, slot: HotkeySlot, chord: &str) -> Result<(), String> {
            self.attempts
                .borrow_mut()
                .push(format!("{}:{chord}", slot.id()));
            let Some(shortcut) = parse_shortcut(chord) else {
                return Err("Couldn't recognize the shortcut".to_string());
            };
            let taken =
                self.held.borrow().contains(&shortcut) || self.foreign.borrow().contains(&shortcut);
            if taken {
                return Err(already_registered_error(chord));
            }
            self.held.borrow_mut().push(shortcut);
            Ok(())
        }

        fn unregister(&self, chord: &str) -> Result<(), String> {
            let Some(shortcut) = parse_shortcut(chord) else {
                return Ok(());
            };
            self.held.borrow_mut().retain(|held| *held != shortcut);
            Ok(())
        }

        fn set_status(&self, status: HotkeyStatus) {
            let mut statuses = self.statuses.borrow_mut();
            statuses.retain(|existing| existing.id != status.id);
            statuses.push(status);
        }

        fn warn(&self, message: String) {
            self.warnings.borrow_mut().push(message);
        }
    }

    /// No user-facing hotkey text may carry Rust `Debug` output.
    fn assert_no_debug_noise(message: &str) {
        for token in ["HotKey {", "Modifiers(", "Some(", "Err(", "id:"] {
            assert!(
                !message.contains(token),
                "leaked {token:?} in user-facing text {message:?}"
            );
        }
    }

    // (a) THE regression: every settings save runs the reregister path. With unchanged
    // chords it must not attempt a registration, warn, or drop the stored chord.
    #[test]
    fn unchanged_chord_after_startup_registration_never_warns() {
        for slot in [HotkeySlot::Marks, HotkeySlot::Overlay] {
            let env = FakeHotkeys::new();
            let chord = slot.default_chord();
            apply_hotkey(&env, slot, chord);
            assert_eq!(
                env.attempts().len(),
                1,
                "{slot:?} did not register at startup"
            );

            apply_hotkey(&env, slot, chord);

            assert_eq!(
                env.attempts().len(),
                1,
                "{slot:?} re-registered an unchanged chord: {:?}",
                env.attempts()
            );
            assert!(
                env.warnings().is_empty(),
                "{slot:?} emitted a hotkey toast: {:?}",
                env.warnings()
            );
            assert_eq!(
                env.stored(slot).registered.as_deref(),
                Some(chord),
                "{slot:?} lost its stored chord"
            );
            let status = env.status(slot);
            assert!(status.registered, "{slot:?} status is not OK");
            assert_eq!(status.error, None, "{slot:?} status carries an error");
        }
    }

    // (b) The observed failure shape: the manager already holds the chord while our
    // stored state is `None`. The save must adopt it silently instead of registering
    // again (which is what produced `HotKey already registered` on every save).
    #[test]
    fn manager_owned_chord_with_empty_stored_state_is_adopted_silently() {
        for slot in [HotkeySlot::Marks, HotkeySlot::Overlay] {
            let env = FakeHotkeys::new();
            let chord = slot.default_chord();
            env.hold_ours(chord);

            apply_hotkey(&env, slot, chord);

            assert!(
                env.attempts().is_empty(),
                "{slot:?} attempted a duplicate registration: {:?}",
                env.attempts()
            );
            assert!(
                env.warnings().is_empty(),
                "{slot:?} emitted a hotkey toast: {:?}",
                env.warnings()
            );
            assert_eq!(env.stored(slot).registered.as_deref(), Some(chord));
            let status = env.status(slot);
            assert!(status.registered);
            assert_eq!(status.error, None);
        }
    }

    // A spelling variant of the same shortcut is not a change (requirement: canonical
    // comparison, so the UI variant cannot trigger a duplicate registration attempt).
    #[test]
    fn chord_spelling_variants_do_not_reregister() {
        let env = FakeHotkeys::new();
        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+M");

        for variant in ["ctrl+alt+m", "Alt+Ctrl+M", " Ctrl+Alt+M ", "CONTROL+ALT+M"] {
            apply_hotkey(&env, HotkeySlot::Marks, variant);
        }

        assert_eq!(
            env.attempts().len(),
            1,
            "a spelling variant was registered again: {:?}",
            env.attempts()
        );
        assert!(env.warnings().is_empty(), "{:?}", env.warnings());
        assert_eq!(
            env.stored(HotkeySlot::Marks).registered.as_deref(),
            Some("CONTROL+ALT+M")
        );
    }

    // (c) A changed chord that genuinely conflicts: exactly one humanized warning, the
    // previous chord stays live, and the status reports the failure.
    #[test]
    fn genuine_conflict_warns_once_with_human_wording() {
        let env = FakeHotkeys::new();
        env.hold_forever("Ctrl+Alt+K");
        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+M");

        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+K");

        let warnings = env.warnings();
        assert_eq!(warnings.len(), 1, "expected one warning: {warnings:?}");
        let message = &warnings[0];
        assert_eq!(
            message,
            "Could not register mark hotkey; still using the previous combination: \
             another app is already using that shortcut"
        );
        assert_no_debug_noise(message);
        assert!(
            message.contains("another app is already using that shortcut"),
            "{message}"
        );
        assert!(
            message.contains("still using the previous combination"),
            "{message}"
        );
        assert_eq!(
            env.stored(HotkeySlot::Marks).registered.as_deref(),
            Some("Ctrl+Alt+M"),
            "the previous chord must stay stored and live"
        );
        assert!(env.holds("Ctrl+Alt+M"));
        let status = env.status(HotkeySlot::Marks);
        assert!(!status.registered);
        let error = status
            .error
            .expect("failed status carries the human reason");
        assert_no_debug_noise(&error);
        assert_eq!(error, "another app is already using that shortcut");
    }

    // (e) Same flaw, Flow overlay hotkey: a conflict with no previous chord is humanized
    // and leaves the stored chord empty rather than falsely claiming a registration.
    #[test]
    fn overlay_conflict_without_previous_chord_is_humanized() {
        let env = FakeHotkeys::new();
        env.hold_forever("Ctrl+Alt+Z");

        apply_hotkey(&env, HotkeySlot::Overlay, "Ctrl+Alt+Z");

        let warnings = env.warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let message = &warnings[0];
        assert_eq!(
            message,
            "Could not register Flow overlay hotkey: another app is already using that shortcut"
        );
        assert_no_debug_noise(message);
        assert_eq!(env.stored(HotkeySlot::Overlay).registered, None);
        assert!(!env.status(HotkeySlot::Overlay).registered);
    }

    // (d) A chord that already failed is retried on the next save (so a released
    // conflict is picked up without an app restart) but silently, and it registers with
    // no toast at all once the conflict is gone.
    #[test]
    fn repeated_failure_retries_silently() {
        let env = FakeHotkeys::new();
        env.hold_forever("Ctrl+Alt+M");

        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+M");
        assert_eq!(env.warnings().len(), 1, "the first failure must be visible");
        assert_eq!(env.attempts().len(), 1);

        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+M");
        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+M");

        assert_eq!(
            env.attempts().len(),
            3,
            "the unchanged failed chord is still retried: {:?}",
            env.attempts()
        );
        assert_eq!(
            env.warnings().len(),
            1,
            "an unrelated save repeated the toast: {:?}",
            env.warnings()
        );
        assert!(!env.status(HotkeySlot::Marks).registered);

        env.release_foreign("Ctrl+Alt+M");
        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+M");

        assert_eq!(
            env.warnings().len(),
            1,
            "recovering the chord must be silent: {:?}",
            env.warnings()
        );
        assert_eq!(
            env.stored(HotkeySlot::Marks).registered.as_deref(),
            Some("Ctrl+Alt+M")
        );
        assert!(env.status(HotkeySlot::Marks).registered);
    }

    // A changed, free chord replaces the previous registration exactly once (one handler
    // per chord, no duplicate registration, old shortcut released).
    #[test]
    fn changed_chord_replaces_the_previous_registration_once() {
        let env = FakeHotkeys::new();
        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+M");

        apply_hotkey(&env, HotkeySlot::Marks, "Ctrl+Alt+K");

        assert_eq!(env.attempts().len(), 2);
        assert!(env.warnings().is_empty(), "{:?}", env.warnings());
        assert!(env.holds("Ctrl+Alt+K"));
        assert!(!env.holds("Ctrl+Alt+M"), "old chord was not released");
        assert_eq!(env.held.borrow().len(), 1, "duplicate handlers registered");
        assert_eq!(
            env.stored(HotkeySlot::Marks).registered.as_deref(),
            Some("Ctrl+Alt+K")
        );
        assert!(env.status(HotkeySlot::Marks).registered);
    }

    // A blank saved chord falls back to the default instead of registering nothing.
    #[test]
    fn blank_chord_falls_back_to_the_default() {
        let env = FakeHotkeys::new();

        apply_hotkey(&env, HotkeySlot::Marks, "   ");

        assert_eq!(
            env.attempts(),
            vec![format!("{}:Ctrl+Alt+M", MARKS_HOTKEY_ID)]
        );
        assert_eq!(
            env.stored(HotkeySlot::Marks).registered.as_deref(),
            Some("Ctrl+Alt+M")
        );
    }

    // (4) Every error shape the plugin/global-hotkey pair can produce maps to one human
    // sentence, and no `Debug` payload can leak for any input.
    #[test]
    fn humanize_never_leaks_debug_output_for_any_plugin_error() {
        let cases = [
            (
                already_registered_error("Ctrl+Alt+M"),
                "another app is already using that shortcut",
            ),
            (
                "HotKey already registered: HotKey { mods: Modifiers(ALT | CONTROL), key: KeyM, id: 589855 }"
                    .to_string(),
                "another app is already using that shortcut",
            ),
            (
                "Couldn't recognize \"Fkey\" as a valid HotKey Code, if you feel like it should be, please report this to https://github.com/tauri-apps/global-hotkey"
                    .to_string(),
                "that shortcut is not recognized",
            ),
            (
                "Unexpected empty token while parsing hotkey: \"Ctrl++M\"".to_string(),
                "that shortcut is not valid",
            ),
            (
                "Unexpected hotkey string format: \"Ctrl+M+Alt\", a hotkey should have the modifiers first and only contain one main key"
                    .to_string(),
                "that shortcut is not valid",
            ),
            (
                "Failed to watch media key event".to_string(),
                "Windows refused that shortcut",
            ),
            (
                "Unable to register hotkey: Unknown VKCode for KeyF24".to_string(),
                "the shortcut could not be registered",
            ),
            (
                "Failed to unregister hotkey: HotKey { mods: Modifiers(ALT), key: KeyM, id: 589856 }"
                    .to_string(),
                "the shortcut could not be registered",
            ),
            (
                "The operation completed successfully. (os error 0)".to_string(),
                "the shortcut could not be registered",
            ),
            (
                "Some(Err(HotKey { mods: Modifiers(SUPER), key: KeyZ, id: 7 }))".to_string(),
                "the shortcut could not be registered",
            ),
            (
                "unexpected error with id: 589855".to_string(),
                "the shortcut could not be registered",
            ),
            (String::new(), "the shortcut could not be registered"),
        ];

        for (input, expected) in cases {
            let message = humanize_hotkey_error(&input);
            assert_eq!(message, expected, "input {input:?}");
            assert_no_debug_noise(&message);
        }
    }
}
