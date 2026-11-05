//! `graph` - the 0.4.5 work-graph engine's pure rules (`03 §7h`, `docs/kickoffs/P5.md`).
//!
//! The crate is **pure**: it depends on `traits` only (plus `anyhow`). No database, no
//! filesystem, no clock, no network and no model. The kernel owns I/O, the store owns SQL,
//! and this crate owns the rules, so the determinism tests can run without a database.
//!
//! Responsibilities, one module each:
//!
//! - [`extraction`] - the closed heuristic rule table: every hit carries its rule's own
//!   confidence and the shared `normalized_key` dedup identity (`03 §7h.2`).
//! - [`linking`] - the session interval rule (`03 §7h.3`), as a pure helper the store's SQL
//!   and the tests both express.
//! - [`clustering`] - deterministic thread components and titles (`03 §7h.4`).
//! - [`overrides`] - rename/merge/split precedence applied after clustering (`03 §7h.5`).
//! - [`normalize`] - validation for the optional LLM normalization proposals (`03 §7h.2`);
//!   the model call itself lives in the kernel job and is never required for a useful graph.
//!
//! Every entry point takes plain input structs and returns plain output; time values are
//! passed in as unix epoch milliseconds. Nothing here reads the clock.

#![forbid(unsafe_code)]

pub mod clustering;
pub mod extraction;
pub mod linking;
pub mod normalize;
pub mod overrides;

pub use extraction::{extract, normalized_key, ExtractionInput};
