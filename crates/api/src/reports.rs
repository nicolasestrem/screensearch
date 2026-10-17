//! Strict saved-report read adapters (`03 §7f.5–.6`, §8c): list + detail only.
//!
//! There is no generation, retry, capture, scheduling, delivery, file write or
//! current-source overlay here. The Store selects membership and produces the safe
//! projection; this module only validates the query, decodes/encodes the opaque v1
//! cursor and forwards typed domain values to [`ApiHost`](crate::ApiHost). The strict
//! query parser and the shared `HistoryReadError` map are reused from `history.rs`
//! rather than copied.

use axum::extract::{RawQuery, State};
use axum::Json;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use traits::{
    EvidenceWindow, HistoryReadError, ReportContinuation, ReportDetail, ReportFilters,
    ReportOutcome, ReportPage, ReportPageRequest, ReportPosition, ReportRunPhase, ReportSnapshot,
    REPORT_CURSOR_MAX_BYTES, REPORT_PAGE_DEFAULT_LIMIT, REPORT_PAGE_MAX_LIMIT,
};

use crate::error::ApiError;
use crate::extract::ApiPath;
use crate::history::{bad_query, history_error, parse_integer, query_fields};
use crate::ApiState;

/// Reports carry no per-history feed identity, so a `history_changed` outcome cannot
/// arise from a report read. Map it defensively to the report 400 — the frame-style 409
/// stays the frame-cursor identity case and is never produced by these routes.
fn report_error(error: HistoryReadError) -> ApiError {
    match error {
        HistoryReadError::HistoryChanged => bad_query(),
        other => history_error(other),
    }
}

/// `GET /v1/reports?from=&to=&phase=&outcome=&limit=` (initial) or
/// `?cursor=&limit=` (continuation) — bounded overlap listing of saved runs.
pub(crate) async fn list_reports(
    State(state): State<ApiState>,
    RawQuery(raw): RawQuery,
) -> Result<Json<ReportPage>, ApiError> {
    let request = parse_report_page(raw.as_deref())?;
    let page = state
        .host
        .report_page(request)
        .await
        .map_err(report_error)?;
    // The Store alone decides membership and exhaustion. Only the frozen global max
    // tuple and the last ordering tuple cross into the cursor. A next position without
    // a snapshot is an internal inconsistency — a storage failure, never an empty 200.
    let next_cursor = match (page.snapshot, page.next_position) {
        (_, None) => None,
        (Some(snapshot), Some(after)) => {
            let request = ReportPageRequest {
                window: page.window,
                filters: page.filters.clone(),
                limit: REPORT_PAGE_DEFAULT_LIMIT,
                continuation: Some(ReportContinuation { snapshot, after }),
            };
            Some(encode_report_cursor(&request).map_err(|_| ApiError::HistoryStorage)?)
        }
        (None, Some(_)) => return Err(ApiError::HistoryStorage),
    };
    Ok(Json(ReportPage {
        items: page.items,
        next_cursor,
    }))
}

/// `GET /v1/reports/{run_id}` — the complete saved projection for one opaque run id.
/// A missing/pruned run is 404; never reconstructed or regenerated.
pub(crate) async fn report_detail(
    State(state): State<ApiState>,
    ApiPath(run_id): ApiPath<String>,
) -> Result<Json<ReportDetail>, ApiError> {
    match state
        .host
        .report_detail(&run_id)
        .await
        .map_err(report_error)?
    {
        Some(detail) => Ok(Json(detail)),
        None => Err(ApiError::NotFound("no such saved report".to_string())),
    }
}

/// Parse the initial (`from`/`to`/optional filters) or continuation (`cursor`/optional
/// `limit`) form. Strict: unknown names, duplicates, malformed percent escapes, invalid
/// UTF-8, non-integer numbers and unknown enum literals are all 400. `limit` is the only
/// parameter allowed alongside `cursor`.
fn parse_report_page(raw: Option<&str>) -> Result<ReportPageRequest, ApiError> {
    let mut fields = query_fields(raw, &["from", "to", "phase", "outcome", "limit", "cursor"])?;
    let limit = fields
        .remove("limit")
        .map(|value| {
            let n = parse_integer(&value, false)?;
            if !(1..=i64::from(REPORT_PAGE_MAX_LIMIT)).contains(&n) {
                return Err(bad_query());
            }
            Ok(n as u32)
        })
        .transpose()?
        .unwrap_or(REPORT_PAGE_DEFAULT_LIMIT);
    if let Some(cursor) = fields.remove("cursor") {
        // The frozen window and typed filters live inside the cursor, so nothing else
        // (from/to/phase/outcome) may accompany it.
        if !fields.is_empty() {
            return Err(bad_query());
        }
        return decode_report_cursor(&cursor, limit);
    }
    let from = fields.remove("from").ok_or_else(bad_query)?;
    let to = fields.remove("to").ok_or_else(bad_query)?;
    let window = EvidenceWindow {
        from: parse_integer(&from, true)?,
        to: parse_integer(&to, true)?,
    };
    window.validate().map_err(report_error)?;
    let filters = ReportFilters {
        phase: parse_phase(fields.remove("phase"))?,
        outcome: parse_outcome(fields.remove("outcome"))?,
    };
    // The allowlist bounded the names and duplicates already; a leftover key means the
    // request mixed an initial field into a form that no longer accepts it.
    if !fields.is_empty() {
        return Err(bad_query());
    }
    Ok(ReportPageRequest {
        window,
        filters,
        limit,
        continuation: None,
    })
}

fn parse_phase(value: Option<String>) -> Result<Option<ReportRunPhase>, ApiError> {
    value
        .map(|value| match value.as_str() {
            "queued" => Ok(ReportRunPhase::Queued),
            "running" => Ok(ReportRunPhase::Running),
            "blocked" => Ok(ReportRunPhase::Blocked),
            "finished" => Ok(ReportRunPhase::Finished),
            _ => Err(bad_query()),
        })
        .transpose()
}

fn parse_outcome(value: Option<String>) -> Result<Option<ReportOutcome>, ApiError> {
    value
        .map(|value| match value.as_str() {
            "OK" => Ok(ReportOutcome::Ok),
            "PARTIAL" => Ok(ReportOutcome::Partial),
            "NO_DATA" => Ok(ReportOutcome::NoData),
            "BLOCKED" => Ok(ReportOutcome::Blocked),
            "FAILED" => Ok(ReportOutcome::Failed),
            _ => Err(bad_query()),
        })
        .transpose()
}

fn phase_tag(value: Option<ReportRunPhase>) -> u8 {
    match value {
        None => 0,
        Some(ReportRunPhase::Queued) => 1,
        Some(ReportRunPhase::Running) => 2,
        Some(ReportRunPhase::Blocked) => 3,
        Some(ReportRunPhase::Finished) => 4,
    }
}

fn outcome_tag(value: Option<ReportOutcome>) -> u8 {
    match value {
        None => 0,
        Some(ReportOutcome::Ok) => 1,
        Some(ReportOutcome::Partial) => 2,
        Some(ReportOutcome::NoData) => 3,
        Some(ReportOutcome::Blocked) => 4,
        Some(ReportOutcome::Failed) => 5,
    }
}

// v1: version u8, four signed big-endian i64s (window.from, window.to,
// snapshot.max_created_at, after.created_at), then two u16-length-prefixed UTF-8 run
// ids (snapshot.max_run_id, after.run_id), then the phase and outcome tags. No history
// identity, JSON escapes, credentials or extension fields. Run ids stay opaque: no
// 64-byte cap, no case normalization, no truncation — only the whole-cursor 4096-byte
// bound limits untrusted input.
const CURSOR_VERSION: u8 = 1;

fn encode_report_cursor(request: &ReportPageRequest) -> Result<String, ApiError> {
    request.validate().map_err(report_error)?;
    let c = request.continuation.as_ref().ok_or_else(bad_query)?;
    let mut bytes = Vec::with_capacity(1 + 4 * 8 + 2 * (2 + 36) + 2);
    bytes.push(CURSOR_VERSION);
    for value in [
        request.window.from,
        request.window.to,
        c.snapshot.max_created_at,
        c.after.created_at,
    ] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    push_string(&mut bytes, &c.snapshot.max_run_id)?;
    push_string(&mut bytes, &c.after.run_id)?;
    bytes.push(phase_tag(request.filters.phase));
    bytes.push(outcome_tag(request.filters.outcome));
    let cursor = URL_SAFE_NO_PAD.encode(bytes);
    if cursor.len() > REPORT_CURSOR_MAX_BYTES {
        return Err(bad_query());
    }
    Ok(cursor)
}

/// Length-prefixed UTF-8 with a `u16` length. This is a mechanical encoding bound, not a
/// policy cap on run-id spelling: production ids are short UUIDs.
fn push_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), ApiError> {
    let length = u16::try_from(value.len()).map_err(|_| bad_query())?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

struct CursorReader<'a>(&'a [u8]);
impl<'a> CursorReader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ApiError> {
        if self.0.len() < n {
            return Err(bad_query());
        }
        let (value, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, ApiError> {
        Ok(self.take(1)?[0])
    }
    fn integer(&mut self) -> Result<i64, ApiError> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| bad_query())?,
        ))
    }
    /// Bound the read against the remaining bytes before allocating; reject invalid UTF-8.
    fn string(&mut self) -> Result<String, ApiError> {
        let length = usize::from(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| bad_query())?,
        ));
        if length > self.0.len() {
            return Err(bad_query());
        }
        Ok(std::str::from_utf8(self.take(length)?)
            .map_err(|_| bad_query())?
            .to_string())
    }
}

fn decode_report_cursor(cursor: &str, limit: u32) -> Result<ReportPageRequest, ApiError> {
    // Pre-decode bound so an oversized untrusted cursor never allocates.
    if cursor.is_empty() || cursor.len() > REPORT_CURSOR_MAX_BYTES {
        return Err(bad_query());
    }
    let bytes = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| bad_query())?;
    let mut reader = CursorReader(&bytes);
    if reader.byte()? != CURSOR_VERSION {
        return Err(bad_query());
    }
    let window = EvidenceWindow {
        from: reader.integer()?,
        to: reader.integer()?,
    };
    let max_created_at = reader.integer()?;
    let after_created_at = reader.integer()?;
    let max_run_id = reader.string()?;
    let after_run_id = reader.string()?;
    let phase = match reader.byte()? {
        0 => None,
        1 => Some(ReportRunPhase::Queued),
        2 => Some(ReportRunPhase::Running),
        3 => Some(ReportRunPhase::Blocked),
        4 => Some(ReportRunPhase::Finished),
        _ => return Err(bad_query()),
    };
    let outcome = match reader.byte()? {
        0 => None,
        1 => Some(ReportOutcome::Ok),
        2 => Some(ReportOutcome::Partial),
        3 => Some(ReportOutcome::NoData),
        4 => Some(ReportOutcome::Blocked),
        5 => Some(ReportOutcome::Failed),
        _ => return Err(bad_query()),
    };
    // Exact consumption: no trailing bytes and no invented extension fields.
    if !reader.0.is_empty() {
        return Err(bad_query());
    }
    let request = ReportPageRequest {
        window,
        filters: ReportFilters { phase, outcome },
        limit,
        continuation: Some(ReportContinuation {
            snapshot: ReportSnapshot {
                max_created_at,
                max_run_id,
            },
            after: ReportPosition {
                created_at: after_created_at,
                run_id: after_run_id,
            },
        }),
    };
    request.validate().map_err(report_error)?;
    Ok(request)
}

#[cfg(test)]
mod tests;
