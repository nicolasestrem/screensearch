//! `inference` — `VisionProvider` + `AnswerProvider`, both backed by the single
//! supervised, model-agnostic **llama.cpp sidecar** (OpenAI-compatible HTTP), plus
//! the `ModelSupervisor` that owns its lifecycle (`03 §6`).
//!
//! The sidecar is bound to the app via a Windows **Job Object**
//! (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) so it can never orphan after a crash —
//! a hard requirement; P4 does not ship until the no-orphan test passes (`03 §6`).
//!
//! **Status (P4):** the Job-Object lifecycle lands first (`02 §5`, `04 §3`). The
//! [`job_object`] + [`process`] modules implement the no-orphan binding (assign a
//! suspended child to a `KILL_ON_JOB_CLOSE` job, then resume); real inference wiring
//! (supervisor, HTTP client, providers) builds on top of it.
//!
//! (No `forbid(unsafe_code)` here: the Job-Object path uses `unsafe` Win32 FFI.)

/// The contracts this crate implements.
pub use traits::{AnswerProvider, VisionProvider};

pub mod client;
pub mod download;
pub mod flags;
pub mod models;

#[cfg(windows)]
pub mod answer;
#[cfg(windows)]
pub mod job_object;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod supervisor;
#[cfg(windows)]
pub mod vision;

/// Sidecar flag-capability probe (used by the composition root to tune `build_args`).
pub use flags::{probe_caps, FlashAttnKind, SidecarCaps};

/// The model downloader (coordinated, progress-reporting). Shared by both lane providers.
pub use download::{EngineProgress, ModelDownloader};

// The composition root (`src-tauri`) wires these concrete impls (`03 §2`).
#[cfg(windows)]
pub use answer::AnswerSidecar;
#[cfg(windows)]
pub use supervisor::{
    auto_recycle_ceiling, resolve_recycle_ceiling, should_recycle, ModelSupervisor,
    SupervisorConfig,
};
#[cfg(windows)]
pub use vision::VisionSidecar;
