//! Durable job-queue types (the heart of *enrich-deferred*, `03 §5`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Kind of deferred work. Serializes to the DB `kind` column
/// (`'embed_text' | 'vision_tag' | 'graph_*'`, `03 §4`). The `embed_image` kind was removed
/// in 0.3.0 PR4 with the image-embedding lane; the V9 migration deletes any leftover rows.
///
/// The four `graph_*` values are the 0.4.5 P5 work-graph job kinds (`03 §7h.7`, 0.4.5
/// D11). They are additive: `embed_text` and `vision_tag` keep their persisted strings, and
/// no existing variant is reordered or renamed (`jobs.kind` stores these strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum JobKind {
    EmbedText,
    VisionTag,
    /// Heuristic entity extraction for one frame plus the session link (`03 §7h.2/.3`).
    GraphExtract,
    /// The optional LLM alias/normalization pass over recent session summaries (`03 §7h.2`).
    GraphNormalize,
    /// Thread clustering plus override precedence (`03 §7h.4/.5`).
    GraphCluster,
    /// The resumable historical pass: enqueues bounded `graph_extract` batches (`03 §7h.6`).
    GraphBackfill,
    /// Ask v2 session-vector maintenance for `session_vectors` (`03 §7i.2`, 0.4.5 D11):
    /// embeds a session's concatenated title/summary in bounded batches, most recently
    /// updated first. Runs on demand and on idle, never on the capture hot path.
    SessionEmbed,
    /// 0.4.5 P8 insights generation (`03 §7j.3`, D11): one model-free pass over the 30-day
    /// lookback window that regenerates all five insight kinds for that window and then sweeps
    /// evidence that no longer exists. Enqueued on idle and on demand, claimable only at
    /// throttle level 0, and never on the capture hot path.
    InsightGenerate,
}

/// State of a job in the queue. Serializes to the DB `state` column
/// (`pending | running | done | failed | dead`, `03 §4/§5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum JobState {
    Pending,
    Running,
    Done,
    Failed,
    Dead,
}

/// A job to enqueue.
#[derive(Debug, Clone)]
pub struct NewJob {
    pub kind: JobKind,
    pub frame_id: Option<i64>,
    pub priority: i64,
    pub max_attempts: i64,
    /// Earliest run time, unix epoch milliseconds (scheduling + backoff).
    pub not_before: i64,
}

/// A claimed/queued job row.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: i64,
    pub kind: JobKind,
    pub frame_id: Option<i64>,
    pub state: JobState,
    pub priority: i64,
    pub attempts: i64,
    pub max_attempts: i64,
    pub not_before: i64,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Aggregate queue counts surfaced to the UI (`get_job_stats`, `03 §7`).
// `#[ts(type = "number")]` on each u64: Tauri's JSON wire delivers 64-bit ints as
// JS numbers, so bindings must use `number` (counts stay well under 2^53). Same
// convention as every i64/u64 IPC field — see crates/traits/src/ipc.rs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct JobStats {
    #[ts(type = "number")]
    pub pending: u64,
    #[ts(type = "number")]
    pub running: u64,
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number")]
    pub failed: u64,
    #[ts(type = "number")]
    pub dead: u64,
    /// Pending `vision_tag` jobs specifically (subset of `pending`). Backs the tray +
    /// quick-menu "Start/Stop vision tagging" label so it tracks the queue (0.3.2 PR3,
    /// `03 §7d`) — aggregate `pending` alone can't distinguish a vision backlog.
    #[ts(type = "number")]
    pub vision_pending: u64,
    /// Running `vision_tag` jobs specifically (subset of `running`). While > 0 the
    /// tray shows "Stop vision tagging" even after the pending backlog drains, because a
    /// cancel leaves the in-flight job to finish (0.3.2 PR3).
    #[ts(type = "number")]
    pub vision_running: u64,
}
