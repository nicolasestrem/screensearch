//! Dev-only dreaming sweep — `Light → REM → Deep`, absence-triggered.
//!
//! Mirrors the mechanics in <https://docs.openclaw.ai/concepts/dreaming>.
//! The database is opened **read-only** (`SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only`)
//! with the `sqlite-vec` (`vec0`) extension registered so `session_vectors` and
//! `embedding_vectors` remain readable. Nothing here writes the database, touches
//! settings, the schema, or any IPC surface. Never shipped.
//!
//! Reuses what exists: `crates/store`'s schema constants and query shapes,
//! `crates/graph`/`crates/kernel::insights` thresholds and KNN pattern,
//! `harness::export::open_readonly`'s busy-timeout / WAL probe.

pub mod db;
pub mod deep;
pub mod diary;
pub mod gaps;
pub mod light;
pub mod rem;
pub mod wording;

pub use db::open_readonly;
pub use diary::DiaryEntry;
