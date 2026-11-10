//! TODO-6 settings-defaults contract: the IPC payload must be the same
//! `Settings::default()` the store falls back to, and the UI registry's fallback
//! table must agree with it for every mapped field.
//!
//! The values are Rust-owned: this test calls the actual `get_settings_defaults`
//! command function, serializes the result, and pipes it to the Node helper that
//! imports the real `ui/src/routes/settings/registry.ts`. There is no snapshot, no
//! generated payload and no second defaults table: a change to `Default for Settings`
//! either flows through or fails here naming the key.
//!
//! The helper lives outside the `*.test.mjs` auto-discovery glob because it reads the
//! real payload from stdin. `node` is required by the repository gate, so a missing
//! interpreter fails this test with a clear message rather than skipping it.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn ui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri lives under the repository root")
        .join("ui")
}

#[test]
fn ipc_settings_defaults_match_the_ui_registry() {
    let defaults = screensearch_lib::settings_defaults::get_settings_defaults();
    let payload = serde_json::to_vec(&defaults).expect("serialize the IPC defaults");

    let ui = ui_dir();
    let helper = ui.join("test").join("settingsDefaultsContract.mjs");
    assert!(
        helper.is_file(),
        "missing the settings-defaults contract helper at {}",
        helper.display()
    );

    let mut child = Command::new("node")
        .arg(&helper)
        .current_dir(&ui)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| {
            panic!(
                "the settings-defaults contract test needs `node` on PATH \
                 (the repository gate requires Node 22): {error}"
            )
        });
    child
        .stdin
        .take()
        .expect("node stdin is piped")
        .write_all(&payload)
        .expect("write the defaults payload to node");

    let output = child.wait_with_output().expect("wait for the node helper");
    assert!(
        output.status.success(),
        "the UI registry disagrees with the Rust Settings::default() (node status {:?})\n\
         stdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
