//! Settings carry policy for the rebuild (`03 §8`, 0.4.5 D3/D6).
//!
//! The tool copies exactly the keys the settings contract still declares, plus the
//! reserved history identity and allocator high-water (`03 §7f.8`) and the
//! credentials/companion settings that live outside the counted table:
//!
//! - `api.read_token` shares the `api.token` UI row (`03 §8`) and is a `settings`
//!   KV row in its own right (`src-tauri/src/local_api.rs`), so it is a credential
//!   to carry, not a key to drop.
//! - The whole `memory.*` namespace is owned by `03 §8c`/`§8d` and is explicitly
//!   "not counted in this table" (`03 §8` preamble). It is still live user
//!   configuration (Telegram delivery, the Obsidian folder, schedule state, weekly
//!   synthesis and per-note promotion receipts), so dropping it on a rebuild would
//!   silently break those features. It is carried by prefix because some keys are
//!   dynamic (`memory.promotion.pending.<sha256(note)>`).
//!
//! The `UI`/`file-only`/`deleted` split is read from the `03 §8` Markdown table at
//! build time (`include_str!`), not restated here: a hand-maintained duplicate would
//! drift from the contract, and the table already is the closed set D6 defines.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use traits::{is_history_metadata_key, FRAME_ID_HIGH_WATER_KEY, HISTORY_ID_KEY};

/// The production spec, embedded at build time. The carrier crate is dev-only and
/// always built from this repository, so the file is present; if the table is
/// malformed the policy below fails loudly instead of silently carrying nothing.
const SPEC: &str = include_str!("../../../specs/03_MASTER_PRODUCTION_SPEC.md");

/// The read-only API credential: a KV row that shares the `api.token` UI row and is
/// therefore absent from the key table (`03 §8`, `03 §7f`).
pub const API_READ_TOKEN_KEY: &str = "api.read_token";

/// The settings/KV namespace owned by `03 §8c`/`§8d`, carried as a prefix because
/// it contains dynamic subkeys (`memory.promotion.pending.<sha256(note)>`).
pub const MEMORY_SETTINGS_PREFIX: &str = "memory.";

/// The parsed settings-v2 dispositions (`03 §8`, 0.4.5 D6).
pub struct SettingsPolicy {
    ui: BTreeSet<&'static str>,
    file_only: BTreeSet<&'static str>,
    deleted: BTreeSet<&'static str>,
}

impl SettingsPolicy {
    /// The process-wide policy, parsed once from the embedded spec.
    pub fn load() -> &'static Self {
        static POLICY: LazyLock<SettingsPolicy> = LazyLock::new(|| parse(SettingsPolicy::empty()));
        &POLICY
    }

    fn empty() -> Self {
        Self {
            ui: BTreeSet::new(),
            file_only: BTreeSet::new(),
            deleted: BTreeSet::new(),
        }
    }

    /// True when a source row must be copied into the rebuilt database.
    ///
    /// Retired keys — including the nine `deleted` rows — are *not* carried: D6's
    /// load rule removes a row that names a key outside the table, so the tool applies
    /// the same rule up front instead of writing rows the app would delete on boot.
    pub fn carries(&self, key: &str) -> bool {
        (self.ui.contains(key) || self.file_only.contains(key))
            || key.starts_with(MEMORY_SETTINGS_PREFIX)
            || key == API_READ_TOKEN_KEY
            || is_history_metadata_key(key)
    }

    /// True when `key` is one of the nine settings-v2 `deleted` rows.
    pub fn is_deleted(&self, key: &str) -> bool {
        self.deleted.contains(key)
    }

    /// Number of `UI` keys: the D6 ceiling (25) in the committed table.
    pub fn ui_count(&self) -> usize {
        self.ui.len()
    }

    /// Number of `file-only` keys in the committed table.
    pub fn file_only_count(&self) -> usize {
        self.file_only.len()
    }

    /// Number of `deleted` rows in the committed table.
    pub fn deleted_count(&self) -> usize {
        self.deleted.len()
    }

    /// The reserved identity keys, for callers that validate the source up front.
    pub fn reserved_keys() -> [&'static str; 2] {
        [HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY]
    }
}

/// Parses the `03 §8` table rows `| \`key\` | default | disposition | where | notes |`.
///
/// The scan starts at the table's `| Key | Default | Disposition |` header and stops
/// at the first non-table line, so no other Markdown table in the spec can leak in.
/// An unknown disposition is a hard error: a silently ignored row would either carry
/// or drop a key without the contract saying so.
fn parse(mut policy: SettingsPolicy) -> SettingsPolicy {
    // Scope the scan to the one key table: the spec has other Markdown tables
    // whose first cell is a backticked token, so "starts with a key" is not
    // enough. The table ends at the first line that is not a table row.
    let mut in_table = false;
    for line in SPEC.lines() {
        let line = line.trim();
        if !in_table {
            in_table = line.starts_with("| Key | Default | Disposition |");
            continue;
        }
        if !line.starts_with('|') {
            break;
        }
        if !line.starts_with("| `") {
            continue; // the `|---|` separator row
        }
        let mut fields = line.split('|');
        let _ = fields.next();
        let key = fields.next().unwrap_or("").trim().trim_matches('`');
        let _default = fields.next();
        let disposition = fields.next().unwrap_or("").trim();
        if key.is_empty() {
            continue;
        }
        match disposition {
            "UI" => {
                policy.ui.insert(key);
            }
            "file-only" => {
                policy.file_only.insert(key);
            }
            "deleted" => {
                policy.deleted.insert(key);
            }
            other => panic!("specs/03 §8 row `{key}` has unknown disposition `{other}`"),
        }
    }
    assert!(
        !policy.ui.is_empty() && !policy.deleted.is_empty(),
        "specs/03 §8 settings table was not found or parsed; the rebuild policy cannot be built"
    );
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_table_parses_to_the_committed_sets() {
        let policy = SettingsPolicy::load();
        assert_eq!(policy.ui_count(), 25, "D6's UI ceiling is 25 keys");
        assert_eq!(policy.deleted_count(), 9, "the nine deleted rows");
        assert_eq!(policy.file_only_count(), 44);
    }

    #[test]
    fn ui_and_file_only_keys_are_carried() {
        let policy = SettingsPolicy::load();
        assert!(policy.carries("capture.interval_ms"));
        assert!(policy.carries("capture.diff_threshold"));
        assert!(policy.carries("app.tray_toast_done"));
        assert!(policy.carries("api.token"));
    }

    #[test]
    fn deleted_keys_are_not_carried() {
        let policy = SettingsPolicy::load();
        for key in [
            "api.network_notice_state",
            "capture.event_on_click",
            "capture.event_on_clipboard",
            "capture.event_on_scroll_stop",
            "capture.event_on_typing_pause",
            "capture.event_typing_pause_ms",
            "capture.uia_run_on_interactive",
            "enrich.image_embeddings",
            "storage.jpeg_quality",
        ] {
            assert!(policy.is_deleted(key), "{key} must be a deleted row");
            assert!(!policy.carries(key), "{key} must not be carried");
        }
    }

    #[test]
    fn reserved_and_out_of_table_settings_are_carried() {
        let policy = SettingsPolicy::load();
        assert!(policy.carries(HISTORY_ID_KEY));
        assert!(policy.carries(FRAME_ID_HIGH_WATER_KEY));
        assert!(policy.carries(API_READ_TOKEN_KEY));
        assert!(policy.carries("memory.telegram.chat_id"));
        assert!(policy.carries("memory.schedule.output_folder"));
        assert!(policy.carries("memory.promotion.pending.abc123"));
        assert!(!policy.carries("obsolete.unknown_key"));
    }
}
