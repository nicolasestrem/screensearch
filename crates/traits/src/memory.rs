//! Durable report and delivery contracts (`03 Â§8c`). No credentials are carried here.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportRunPhase {
    Queued,
    Running,
    Blocked,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportOutcome {
    Ok,
    Partial,
    NoData,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportDestination {
    Vault,
    Index,
    Telegram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportDeliveryState {
    Pending,
    Sending,
    Succeeded,
    Conflict,
    Failed,
    Skipped,
    UnknownDelivery,
    Reviewed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MemorySettings {
    pub enabled: bool,
    pub local_time: Option<String>,
    pub timezone: Option<String>,
    pub output_folder: Option<String>,
    pub telegram_enabled: bool,
    pub telegram_chat_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportScheduleState {
    pub schedule_id: String,
    #[ts(type = "number")]
    pub activated_at: i64,
    #[ts(type = "number | null")]
    pub last_window_end: Option<i64>,
    #[ts(type = "number | null")]
    pub gap_start: Option<i64>,
    #[ts(type = "number | null")]
    pub gap_end: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SavedReport {
    pub markdown: String,
    pub model: Option<String>,
    #[ts(type = "number[]")]
    pub frame_ids: Vec<i64>,
    #[ts(type = "number[]")]
    pub session_ids: Vec<i64>,
    #[ts(type = "number[]")]
    pub cited_frame_ids: Vec<i64>,
    pub frames_sampled: u32,
    pub frames_summarized: u32,
    pub periods_total: u32,
    pub periods_covered: u32,
    pub passes: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportRun {
    pub run_id: String,
    pub source_id: String,
    pub schedule_id: Option<String>,
    pub revision_of: Option<String>,
    #[ts(type = "number")]
    pub window_start: i64,
    #[ts(type = "number")]
    pub window_end: i64,
    pub timezone: String,
    pub local_date: String,
    pub phase: ReportRunPhase,
    pub outcome: Option<ReportOutcome>,
    pub error: Option<ReportError>,
    pub result: Option<SavedReport>,
    pub config_snapshot: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportDelivery {
    pub id: String,
    pub run_id: String,
    pub destination: ReportDestination,
    pub state: ReportDeliveryState,
    pub revision_of: Option<String>,
    pub destination_snapshot: String,
    pub path: Option<String>,
    pub hash: Option<String>,
    pub recovery_path: Option<String>,
    pub chunk_index: Option<u32>,
    pub chunk_count: Option<u32>,
    #[ts(type = "number | null")]
    pub message_id: Option<i64>,
    pub error: Option<ReportError>,
    #[ts(type = "number | null")]
    pub next_attempt_at: Option<i64>,
    pub attempt: u32,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportFileOwnership {
    pub path: String,
    pub source_id: String,
    pub run_id: String,
    pub hash: String,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportEvidence {
    #[ts(type = "number[]")]
    pub session_ids: Vec<i64>,
    #[ts(type = "number[]")]
    pub missing_frame_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MemoryStatus {
    pub settings: MemorySettings,
    pub source_id: String,
    pub latest_run: Option<ReportRun>,
    pub missed_windows: Vec<crate::TimeRange>,
    pub worker_running: bool,
    pub worker_error: Option<ReportError>,
    pub telegram_credential_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MemorySchedulePreview {
    pub local_date: String,
    #[ts(type = "number")]
    pub window_start: i64,
    #[ts(type = "number")]
    pub window_end: i64,
    #[ts(type = "number")]
    pub due_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum TelegramDeliveryResolution {
    Reviewed,
    Resend,
}
