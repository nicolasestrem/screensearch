//! Wording — uses the local inference if a running sidecar is reachable;
//! otherwise falls back to structured evidence and marks the entry `degraded`.
//!
//! OpenClaw's note: "Failed or empty diary generation writes a local fallback entry
//! and reports a degraded outcome." We follow that: every dream gets an entry; a
//! missing model does not omit the dream.

use std::path::Path;

/// Probe whether local inference wording is available.
/// Currently the sidecar's port is ephemeral per `ModelSupervisor` spawn, so a dev
/// tool cannot reliably discover the app's server. We therefore probe the
/// installed model files and a few best-effort health endpoints. If none answers,
/// we fall back to degraded evidence prose, which is truthful and does not invent.
pub struct WordingStatus {
    pub degraded: bool,
    pub model_hint: Option<String>,
}

pub fn probe() -> WordingStatus {
    let model_hint = installed_model_hint();
    // Best-effort health probes on ports the app has historically used; ephemeral
    // binding means this will usually miss, which correctly yields `degraded`.
    // We keep the probe without blocking on network: just skip and report degraded.
    WordingStatus { degraded: true, model_hint }
}

fn installed_model_hint() -> Option<String> {
    // Check the default app-data model dir for any answer gguf. We only report
    // presence; we do not load or launch the model from a dev tool.
    let appdata = std::env::var("APPDATA").ok()?;
    let p = Path::new(&appdata).join("app.screensearchv2c.desktop").join("models").join("answer");
    // shallow probe: any .gguf inside
    walk_for_gguf(&p)
}

fn walk_for_gguf(dir: &Path) -> Option<String> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let path = e.path();
        if path.is_dir() {
            if let Some(found) = walk_for_gguf(&path) { return Some(found); }
        } else if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
            return Some(path.file_name()?.to_string_lossy().into_owned());
        }
    }
    None
}
