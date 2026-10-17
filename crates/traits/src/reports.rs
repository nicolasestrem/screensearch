//! Saved-report read contracts (`03 §7f.5/.6`, `03 §8c`).
//!
//! Reads serve the immutable persisted `SavedReport` whenever it exists,
//! independent of run/delivery phase (`03 §7f.10`, Nico GO). They never
//! generate, retry, deliver, repair, write or overlay current sources. Opaque
//! cursor encoding belongs to the HTTP adapter; the Store accepts validated
//! domain positions and shares one read transaction for the frozen maximum
//! ordering tuple and the candidate summaries.
//!
//! The new wire types here are HTTP projections, not the IPC `ReportRun` shape:
//! they omit `schedule_id`, `config_snapshot` and every delivery/path/credential
//! field, and they never serialize a stored `ReportError.message`.

use serde::{Deserialize, Serialize};

use crate::{
    EvidenceWindow, HistoryReadError, MemoryReportRange, ReportError, ReportOutcome, ReportRun,
    ReportRunPhase, SavedReport,
};

/// Whole-cursor cap for untrusted `GET /v1/reports` cursor input. The opaque
/// run_id inside a cursor stays a plain string with **no** per-id byte cap — ids
/// are never normalized or truncated; this whole-cursor bound is the only limit.
pub const REPORT_CURSOR_MAX_BYTES: usize = 4096;
pub const REPORT_PAGE_DEFAULT_LIMIT: u32 = 100;
pub const REPORT_PAGE_MAX_LIMIT: u32 = 1000;

/// Typed phase/outcome filters. Both are current observations, so filtered
/// pagination is not a status feed (`03 §7f.5`); there is no free-text filter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFilters {
    pub phase: Option<ReportRunPhase>,
    pub outcome: Option<ReportOutcome>,
}
impl ReportFilters {
    /// Typed enums are always valid; the HTTP layer rejects unknown spellings
    /// before a request reaches here.
    pub fn validate(&self) -> Result<(), HistoryReadError> {
        Ok(())
    }
}

/// Keyset position = the last returned ordering tuple `(created_at, run_id)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportPosition {
    pub created_at: i64,
    pub run_id: String,
}

/// Frozen maximum ordering tuple (cursor v1 high-water tuple). Absent means the
/// report table was genuinely empty when the first page was read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSnapshot {
    pub max_created_at: i64,
    pub max_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportContinuation {
    pub snapshot: ReportSnapshot,
    pub after: ReportPosition,
}

/// Initial window/filters or cursor-decoded frozen window/filters.
/// `continuation = None` selects the global maximum tuple and the first page in
/// one read transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportPageRequest {
    pub window: EvidenceWindow,
    pub filters: ReportFilters,
    pub limit: u32,
    pub continuation: Option<ReportContinuation>,
}
impl ReportPageRequest {
    pub fn validate(&self) -> Result<(), HistoryReadError> {
        self.window.validate()?;
        self.filters.validate()?;
        if !(1..=REPORT_PAGE_MAX_LIMIT).contains(&self.limit) {
            return Err(HistoryReadError::InvalidRequest);
        }
        if let Some(continuation) = &self.continuation {
            // Reports have no per-history feed identity, so there is no
            // `history_id` to compare (no `history_changed` for reports). The
            // seek tuple is validated against the frozen maximum tuple only.
            //
            // There is deliberately NO run_id byte cap (ids stay opaque; the
            // whole-cursor 4096-byte cap bounds untrusted input) and NO
            // window-membership check on `created_at` — the ordering key is
            // independent of the overlap window (`03 §7f.5`), so a run's
            // created_at need not fall in `[from, to)`.
            if continuation.after.run_id.is_empty()
                || continuation.snapshot.max_run_id.is_empty()
                || tuple_above_max(&continuation.after, &continuation.snapshot)
            {
                return Err(HistoryReadError::InvalidRequest);
            }
        }
        Ok(())
    }
}

/// True when the seek tuple sorts strictly above the frozen maximum in
/// descending `(created_at, run_id)` order. A continuation must never seek above
/// the frozen high tuple.
fn tuple_above_max(after: &ReportPosition, max: &ReportSnapshot) -> bool {
    after.created_at > max.max_created_at
        || (after.created_at == max.max_created_at && after.run_id > max.max_run_id)
}

/// Store → host page (internal, no wire serialization). `snapshot = None` is
/// the honest empty-table case, not a fabricated tuple; a `next_position`
/// requires `Some(snapshot)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReportPage {
    pub items: Vec<ReportSummary>,
    pub snapshot: Option<ReportSnapshot>,
    pub window: EvidenceWindow,
    pub filters: ReportFilters,
    pub next_position: Option<ReportPosition>,
}

/// Wire summary (`03 §7f.6`) — never serialized from a stored `ReportRun`.
/// `error_code` is allowlisted only; unknown/unallowlisted codes project as null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSummary {
    pub run_id: String,
    pub source_id: String,
    pub revision_of: Option<String>,
    pub requested_range: Option<MemoryReportRange>,
    pub window_start: i64,
    pub window_end: i64,
    pub timezone: String,
    pub local_date: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub phase: ReportRunPhase,
    pub outcome: Option<ReportOutcome>,
    pub error_code: Option<String>,
}
impl ReportSummary {
    /// Public projection of a stored run. Excludes schedule id, config snapshot
    /// and the arbitrary error message (`03 §7f.5`/`§7f.6`).
    pub fn from_run(run: &ReportRun) -> Self {
        Self {
            run_id: run.run_id.clone(),
            source_id: run.source_id.clone(),
            revision_of: run.revision_of.clone(),
            requested_range: run.requested_range.clone(),
            window_start: run.window_start,
            window_end: run.window_end,
            timezone: run.timezone.clone(),
            local_date: run.local_date.clone(),
            created_at: run.created_at,
            updated_at: run.updated_at,
            phase: run.phase,
            outcome: run.outcome,
            error_code: allowlisted_error_code(run.error.as_ref()),
        }
    }
}

/// Wire detail = all summary fields plus the stored saved result (`03 §7f.6`).
/// `result` mirrors actual saved-result presence: `Some` whenever a valid
/// persisted `SavedReport` exists, in any run/delivery phase (`03 §7f.10`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportDetail {
    pub run_id: String,
    pub source_id: String,
    pub revision_of: Option<String>,
    pub requested_range: Option<MemoryReportRange>,
    pub window_start: i64,
    pub window_end: i64,
    pub timezone: String,
    pub local_date: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub phase: ReportRunPhase,
    pub outcome: Option<ReportOutcome>,
    pub error_code: Option<String>,
    pub result: Option<SavedReport>,
}
impl ReportDetail {
    /// Public projection of a stored run. `result = run.result` — the stored
    /// presence rule — and never a regenerated or phase-suppressed value.
    pub fn from_run(run: ReportRun) -> Self {
        let summary = ReportSummary::from_run(&run);
        Self {
            run_id: summary.run_id,
            source_id: summary.source_id,
            revision_of: summary.revision_of,
            requested_range: summary.requested_range,
            window_start: summary.window_start,
            window_end: summary.window_end,
            timezone: summary.timezone,
            local_date: summary.local_date,
            created_at: summary.created_at,
            updated_at: summary.updated_at,
            phase: summary.phase,
            outcome: summary.outcome,
            error_code: summary.error_code,
            result: run.result,
        }
    }
}

/// Wire page (`03 §7f.6`): items + optional terminal-null cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportPage {
    pub items: Vec<ReportSummary>,
    pub next_cursor: Option<String>,
}

/// The exact `§8c` report error-code allowlist. Any other stored code (or a
/// missing error) projects as null, never private diagnostic text.
pub const REPORT_ERROR_CODE_ALLOWLIST: &[&str] = &[
    "INVALID_CONFIG",
    "NOT_FOUND",
    "BUSY",
    "SOURCE_UNAVAILABLE",
    "MODEL_UNAVAILABLE",
    "STORE_ERROR",
    "GENERATION_FAILED",
    "FILE_CONFLICT",
    "WRITE_FAILED",
    "CREDENTIAL_UNAVAILABLE",
    "DELIVERY_FAILED",
];

/// Projects a stored `ReportError` to its allowlisted code, or `None` when the
/// error is absent or its code is not in the `§8c` allowlist.
pub fn allowlisted_error_code(error: Option<&ReportError>) -> Option<String> {
    error
        .map(|error| error.code.as_str())
        .filter(|code| REPORT_ERROR_CODE_ALLOWLIST.contains(code))
        .map(str::to_owned)
}
