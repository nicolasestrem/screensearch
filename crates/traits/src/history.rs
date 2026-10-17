//! Chronological, bounded-membership evidence contracts (`03 §7f`).
//! Reads never initialize metadata or invoke capture/enrichment. Opaque cursor encoding
//! belongs to the HTTP adapter; the Store accepts validated domain positions.

use crate::TextSource;
use serde::{Deserialize, Serialize};

pub const HISTORY_ID_KEY: &str = "api.history_id";
pub const FRAME_ID_HIGH_WATER_KEY: &str = "api.frame_id_high_water";
pub const FRAME_FILTER_MAX_BYTES: usize = 256;
pub const FRAME_CURSOR_MAX_BYTES: usize = 4096;
pub const FRAME_PAGE_DEFAULT_LIMIT: u32 = 100;
pub const FRAME_PAGE_MAX_LIMIT: u32 = 1000;

/// Reserved metadata must not enter user-settings import/export/reset paths.
pub fn is_history_metadata_key(key: &str) -> bool {
    matches!(key, HISTORY_ID_KEY | FRAME_ID_HIGH_WATER_KEY)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWindow {
    pub from: i64,
    pub to: i64,
}
impl EvidenceWindow {
    pub fn validate(self) -> Result<(), HistoryReadError> {
        if self.from >= self.to {
            return Err(HistoryReadError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameFilters {
    pub app: Option<String>,
    pub window_title: Option<String>,
}
impl FrameFilters {
    pub fn validate(&self) -> Result<(), HistoryReadError> {
        if self
            .app
            .iter()
            .chain(self.window_title.iter())
            .any(|s| s.len() > FRAME_FILTER_MAX_BYTES)
        {
            return Err(HistoryReadError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramePosition {
    pub captured_at: i64,
    pub frame_id: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameSnapshot {
    pub history_id: String,
    pub snapshot_max_frame_id: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameContinuation {
    pub snapshot: FrameSnapshot,
    pub after: FramePosition,
}
/// Initial time/filters or cursor-decoded frozen time/filters. `continuation=None`
/// selects the initial maximum and hydrated rows in one read transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramePageRequest {
    pub window: EvidenceWindow,
    pub filters: FrameFilters,
    pub limit: u32,
    pub continuation: Option<FrameContinuation>,
}
impl FramePageRequest {
    pub fn validate(&self) -> Result<(), HistoryReadError> {
        self.window.validate()?;
        self.filters.validate()?;
        if !(1..=FRAME_PAGE_MAX_LIMIT).contains(&self.limit) {
            return Err(HistoryReadError::InvalidRequest);
        }
        if let Some(c) = &self.continuation {
            if !valid_history_id(&c.snapshot.history_id)
                || c.snapshot.snapshot_max_frame_id <= 0
                || c.after.frame_id <= 0
                || c.after.frame_id > c.snapshot.snapshot_max_frame_id
                || c.after.captured_at < self.window.from
                || c.after.captured_at >= self.window.to
            {
                return Err(HistoryReadError::InvalidRequest);
            }
        }
        Ok(())
    }
}

/// Canonical UUID representation used for newly established histories. Kept free
/// of a UUID dependency so HTTP and Store share exactly the same validation.
pub fn valid_history_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameTextState {
    Available,
    NotAvailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameEvidence {
    pub frame_id: i64,
    pub captured_at: i64,
    pub monitor_index: u32,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    pub raw_text: Option<String>,
    pub content_text: Option<String>,
    pub text_source: Option<TextSource>,
    pub filter_version: Option<i32>,
    pub text_state: FrameTextState,
    pub vision_description: Option<String>,
    pub session_id: Option<i64>,
    pub image_purged: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameConsistency {
    BoundedMembershipLiveContent,
}
/// Public wire response: optional fields serialize explicitly as null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramePage {
    pub items: Vec<FrameEvidence>,
    pub next_cursor: Option<String>,
    pub history_id: String,
    pub window: EvidenceWindow,
    pub snapshot_max_frame_id: i64,
    pub observed_at: i64,
    pub consistency: FrameConsistency,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFramePage {
    pub items: Vec<FrameEvidence>,
    pub snapshot: FrameSnapshot,
    pub window: EvidenceWindow,
    pub filters: FrameFilters,
    pub next_position: Option<FramePosition>,
    pub observed_at: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMetadata {
    pub history_id: String,
    pub frame_id_high_water: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryUnavailableReason {
    MissingIdentity,
    MissingAllocator,
    InvalidIdentity,
    InvalidAllocator,
    UnsupportedStore,
}
#[derive(Debug)]
pub enum HistoryReadError {
    Unavailable(HistoryUnavailableReason),
    HistoryChanged,
    InvalidRequest,
    Storage(anyhow::Error),
}
impl std::fmt::Display for HistoryReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable(_) => "history feed unavailable",
            Self::HistoryChanged => "history changed",
            Self::InvalidRequest => "invalid history request",
            Self::Storage(_) => "history storage error",
        })
    }
}
impl std::error::Error for HistoryReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl From<anyhow::Error> for HistoryReadError {
    fn from(e: anyhow::Error) -> Self {
        Self::Storage(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameReference {
    pub frame_id: i64,
    pub captured_at: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCoverage {
    pub history: HistoryMetadata,
    pub window: EvidenceWindow,
    pub observed_at: i64,
    pub oldest_frame: Option<FrameReference>,
    pub newest_frame: Option<FrameReference>,
    pub newest_captured_at: Option<i64>,
    pub frames_stored: u64,
    pub frames_text_available: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureInterruptionReason {
    CaptureStopped,
    WorkerError,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureInterruption {
    pub start: i64,
    pub end: Option<i64>,
    pub reason: CaptureInterruptionReason,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureObservations {
    pub observation_started_at: i64,
    pub unavailable_before: i64,
    pub intervals: Vec<CaptureInterruption>,
    pub truncated: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionScope {
    ImagesOnly,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedReportRetention {
    pub terminal_days: u32,
    pub exempt_phases: Vec<crate::ReportRunPhase>,
    pub exempt_delivery_states: Vec<crate::ReportDeliveryState>,
    pub failed_delivery_with_pending_retry_exempt: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureCoverage {
    pub window: EvidenceWindow,
    pub observed_at: i64,
    pub capture_enabled: bool,
    pub frame_id_high_water: i64,
    pub oldest_frame: Option<FrameReference>,
    pub newest_frame: Option<FrameReference>,
    pub newest_captured_at: Option<i64>,
    pub frames_stored: u64,
    pub frames_text_available: u64,
    pub image_retention_days: u32,
    pub retention_scope: RetentionScope,
    pub saved_report_retention: SavedReportRetention,
    pub capture_observations: CaptureObservations,
}
