//! `06` #150 launch-order regression: the updater state must be managed on the `Builder`,
//! before any config-declared window (and its WebView) can run `get_update_status`.
//!
//! Tauri creates the windows from `tauri.conf.json` inside its own setup, immediately
//! before the app's setup closure (`tauri-2.11.3/src/app.rs`, `fn setup`: the
//! `WebviewWindowBuilder::from_config` loop precedes `(setup)(app)`). Both windows and their
//! WebViews therefore exist while the setup closure is still doing launch work, so a state
//! registered from there is absent when a mounting WebView invokes `get_update_status`.
//! That command takes an `AppHandle` (`src/update.rs:199`) and its `current_status` helper
//! calls `app.state::<UpdaterState>()` (`src/update.rs:72`), which is `Manager::state` and
//! panics on an unmanaged type: the shipped 0.4.5 launch crash. A command declaring
//! `State<'_, T>` returns an invoke error instead (`tauri-2.11.3/src/state.rs:60-68`). This
//! test drives the real registration seam (`manage_pre_window_state`, the function `run()`
//! applies to the builder) and proves the state is present in the manager while no WebView
//! exists.
//!
//! It lives in `tests/` rather than the lib unit tests because the crate's test-target
//! resource embedding (`cargo:rustc-link-arg-tests` in `build.rs`) applies to integration
//! test targets only. `tauri::test`'s mock runtime constructs `App`, whose tray/menu path
//! statically imports `TaskDialogIndirect` from `comctl32.dll`; that export exists only in
//! Common Controls v6, which a process gets from its embedded application manifest.

use tauri::Manager;

#[test]
fn updater_state_is_managed_before_any_webview_exists() {
    // The mock context declares no window, so `build` reproduces the moment before Tauri's
    // setup creates the config windows: no window, no WebView, and no setup closure has run.
    let app = screensearch_lib::manage_pre_window_state(tauri::test::mock_builder())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app builds");

    assert!(
        app.webview_windows().is_empty(),
        "the mock context declares no window, so no WebView can exist at this point"
    );
    // The same lookup `current_status` performs for `get_update_status`
    // (`src/update.rs:72`); `Manager::state` panics when the type is unmanaged.
    let _ = app.state::<screensearch_lib::UpdaterState>();
}
