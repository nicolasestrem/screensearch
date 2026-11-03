//! # Segmentation validation harness (0.4.0 PR2)
//!
//! A **dev-only, read-only** referee for the 0.4.0 sessions arc (`docs/0.4.0.md` §3 PR2,
//! `specs/03 §7e`). It is the arc's mandatory evidence phase: no segmenter ships untested
//! against the maintainer's real screen history. This crate:
//!
//! 1. **Exports** frame metadata + marks from the live SQLite DB, **read-only**, for a
//!    representative 5–10-day sample ([`export`]).
//! 2. Renders a human-readable **digest** per day so the maintainer can hand-label session
//!    boundaries + tool identities in a lightweight TOML format ([`digest`], [`labels`]).
//! 3. Replays a **pure candidate segmenter** — the `where_was_i` context key (`03 §7b`,
//!    `crates/kernel/src/resume.rs`) generalized with taxonomy tool identity per `03 §7e`
//!    ([`segmenter`]), then **groups** its micro-runs into task-level sessions ([`group`], the
//!    `§7e` amendment `06` #27) via [`taxonomy`] recognition.
//! 4. **Scores** boundary precision/recall (± tolerance) + tool-recognition accuracy against
//!    the labels, sweeps parameters, and confirms the freeze-lookback window ([`score`]).
//!
//! **0.4.5 P4 adds the intelligence evaluation runner** ([`eval`], `docs/kickoffs/P4.md`): it
//! generates question/answer pairs mechanically from the database (last-seen, involvement,
//! sequence, window recall, seen-again), asks them through the running app's local HTTP API, and
//! writes the five baseline metrics Gate A freezes for P5/P6. It reads the database only, never
//! ships, and needs no hand-written question, label or answer key (0.4.5 D17).
//!
//! ## Guarantees this crate upholds
//! - **Read-only against the live DB.** [`export`] opens with `SQLITE_OPEN_READ_ONLY` +
//!   `PRAGMA query_only`; the only write is [`export::backup`]'s `VACUUM INTO` to a fresh
//!   file outside the repo and the app data dir (the D5 pre-migration backup).
//! - **Nothing shipped.** Workspace crates are never bundled by the NSIS installer (only
//!   `src-tauri` + the `externalBin` MCP binary ship), so "excluded from the installer" holds
//!   by construction. No `ts-rs` derives, so the CI binding guard stays clean.
//! - **No personal data in the repo.** All exports, labels and eval runs live under the
//!   git-ignored `harness-data/`; this crate's tests use only synthetic fixtures + a tempfile DB.
//!
//! ## The referee contract (D9)
//! [`score`] operates over any [`model::SessionSpan`] producer, not just [`segmenter`]. PR4
//! re-runs its **shipped** segmenter through this same scoring code to prove it meets the
//! thresholds PR2 records — a miss is a stop-and-report, not tune-until-green.

pub mod data;
pub mod digest;
pub mod eval;
pub mod export;
pub mod group;
pub mod labels;
pub mod model;
pub mod score;
pub mod segmenter;
pub mod taxonomy;
