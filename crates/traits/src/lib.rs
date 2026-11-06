//! `traits` — the module contracts and shared domain types for ScreenSearch V2c.
//!
//! This crate holds **no implementations** (`03 §2`). `kernel` and the module
//! crates depend on the traits here; `src-tauri` (the composition root) wires the
//! concrete impls together. That is the modularity guarantee.
//!
//! Layout:
//! - [`contracts`] — the async module traits (`03 §3`).
//! - [`domain`] — internal value types (frames, OCR, embeddings, …).
//! - [`ipc`] — the typed UI ↔ core contract (`ts-rs`-exported, `03 §7`).
//! - [`graph_ipc`] — the P9 work-graph + Ask-conversation wire contract (`03 §7c` P8 block,
//!   `§7h.4`–`§7j`), exported the same way.
//! - [`ask_ipc`] — the Ask v2 scope overrides the kernel takes (`03 §7i.4`; not wire types).
//! - [`jobs`] — durable job-queue types (`03 §5`).
//! - [`privacy`] — the pure excluded-apps matcher shared by `capture` + `kernel`.

#![forbid(unsafe_code)]

pub mod advanced;
pub mod ask_ipc;
pub mod contracts;
pub mod domain;
pub mod graph;
pub mod graph_ipc;
pub mod history;
pub mod insights;
pub mod ipc;
pub mod jobs;
pub mod memory;
pub mod privacy;
pub mod promotion;
pub mod reports;
pub mod sessions;

pub use advanced::*;
pub use ask_ipc::*;
pub use contracts::{
    AnswerProvider, BackfillControl, CaptureSource, EmbeddingProvider, OcrProvider, PressureProbe,
    SessionSegmenter, Store, VisionProvider,
};
pub use domain::*;
pub use graph::*;
pub use graph_ipc::*;
pub use history::*;
pub use insights::*;
pub use ipc::*;
pub use jobs::*;
pub use memory::*;
pub use privacy::is_excluded;
pub use promotion::*;
pub use reports::*;
pub use sessions::*;

/// Workspace-wide fallible result type (`03 §3`).
///
/// Defaults to [`anyhow::Error`]; a module may substitute its own error enum at
/// the boundary as long as it converts into `anyhow::Error`.
pub type Result<T, E = anyhow::Error> = std::result::Result<T, E>;
