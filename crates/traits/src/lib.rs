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
//! - [`jobs`] — durable job-queue types (`03 §5`).
//! - [`privacy`] — the pure excluded-apps matcher shared by `capture` + `kernel`.

#![forbid(unsafe_code)]

pub mod contracts;
pub mod domain;
pub mod history;
pub mod ipc;
pub mod jobs;
pub mod memory;
pub mod privacy;
pub mod reports;
pub mod sessions;

pub use contracts::{
    AnswerProvider, BackfillControl, CaptureSource, EmbeddingProvider, OcrProvider, PressureProbe,
    SessionSegmenter, Store, VisionProvider,
};
pub use domain::*;
pub use history::*;
pub use ipc::*;
pub use jobs::*;
pub use memory::*;
pub use privacy::is_excluded;
pub use reports::*;
pub use sessions::*;

/// Workspace-wide fallible result type (`03 §3`).
///
/// Defaults to [`anyhow::Error`]; a module may substitute its own error enum at
/// the boundary as long as it converts into `anyhow::Error`.
pub type Result<T, E = anyhow::Error> = std::result::Result<T, E>;
