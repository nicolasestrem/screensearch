//! The kernel's typed event bus payloads (`03 §1/§7`).
//!
//! The kernel is shell-agnostic: it broadcasts [`KernelEvent`]s over a
//! [`tokio::sync::broadcast`] channel, and the composition root (`src-tauri`)
//! forwards them to the WebView2 UI as Tauri events. The kernel never knows about
//! Tauri.

use traits::{
    CaptureTick, JobCompleted, JobStats, ModelDownloadStatus, Readiness, SetupStatus,
    SidecarStatus, ThrottleStatus, Toast,
};

/// An event the kernel emits for the UI (forwarded to Tauri events by `src-tauri`).
///
/// Cloneable because `broadcast` delivers a clone to every subscriber.
#[derive(Debug, Clone)]
pub enum KernelEvent {
    /// A frame was captured, OCR'd, and stored — drives the live timeline
    /// (`capture_tick`, `03 §7`).
    CaptureTick(CaptureTick),
    /// A subsystem's readiness changed — drives the readiness strip
    /// (`readiness_changed`, `03 §7`).
    ReadinessChanged(Readiness),
    /// Queue depth changed after a worker finished a job — drives the job-queue
    /// indicator (`job_progress`, `03 §7`).
    JobProgress(JobStats),
    /// A data-changing enrichment job completed successfully — lets the UI refresh
    /// affected frame/search/insights queries without broad invalidation.
    JobCompleted(JobCompleted),
    /// The inference sidecar changed lifecycle state — drives the sidecar status
    /// indicator (`sidecar_status`, `03 §6/§7`).
    SidecarStatus(SidecarStatus),
    /// A model download advanced — drives the download progress UI (`model_download`,
    /// `03 §6/§7`).
    ModelDownload(ModelDownloadStatus),
    /// A transient user-facing notification.
    Toast(Toast),
    /// The enrichment throttle's level or latest pressure sample changed — drives the
    /// Settings pressure readout + the StatusRail "throttling" indicator
    /// (`throttle_changed`, `03 §7`). Emitted each sample tick while the throttle is
    /// enabled so the readout stays live.
    ThrottleChanged(ThrottleStatus),
    /// A sessions scheduler iteration committed session-row or ownership work. The UI
    /// uses this pull-based signal to refetch mounted session queries; it carries no
    /// user-facing notification (`sessions_changed`).
    SessionsChanged,
    /// Setup acquisition changed — the payload is the **whole** updated snapshot
    /// (`setup_changed`, `03 §7g`). The kernel applies the change to its authoritative
    /// state before sending this, so a handler can call [`Kernel::setup_status`] and
    /// observe exactly what the event delivered.
    SetupChanged(SetupStatus),
}
