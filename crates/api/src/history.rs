//! Strict read adapters for `03 §7f`: no search, capture, inference or filesystem reads.
use std::collections::BTreeMap;

use axum::extract::{RawQuery, State};
use axum::Json;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use traits::{
    CaptureCoverage, EvidenceWindow, FrameConsistency, FrameContinuation, FrameFilters, FramePage,
    FramePageRequest, FramePosition, FrameSnapshot, HistoryReadError, FRAME_CURSOR_MAX_BYTES,
    FRAME_FILTER_MAX_BYTES, FRAME_PAGE_DEFAULT_LIMIT, FRAME_PAGE_MAX_LIMIT,
};

use crate::{error::ApiError, ApiState};

fn bad_query() -> ApiError {
    // Never echo a cursor, filter, query payload or underlying private diagnostic.
    ApiError::BadRequest("invalid history query or cursor".to_string())
}

fn history_error(error: HistoryReadError) -> ApiError {
    match error {
        HistoryReadError::InvalidRequest => bad_query(),
        HistoryReadError::HistoryChanged => ApiError::HistoryChanged,
        HistoryReadError::Unavailable(_) => {
            ApiError::Unavailable("history feed unavailable".into())
        }
        HistoryReadError::Storage(_) => ApiError::HistoryStorage,
    }
}

pub(crate) async fn frames(
    State(state): State<ApiState>,
    RawQuery(raw): RawQuery,
) -> Result<Json<FramePage>, ApiError> {
    let request = parse_frames(raw.as_deref())?;
    let page = state
        .host
        .frame_page(request)
        .await
        .map_err(history_error)?;
    // Only the Store selects membership and determines exhaustion. No re-enumeration,
    // remote detail calls, limit inference or text manipulation at the HTTP boundary.
    let next_cursor = page
        .next_position
        .map(|after| {
            encode_cursor(&FramePageRequest {
                window: page.window,
                filters: page.filters.clone(),
                limit: FRAME_PAGE_DEFAULT_LIMIT,
                continuation: Some(FrameContinuation {
                    snapshot: page.snapshot.clone(),
                    after,
                }),
            })
        })
        .transpose()
        .map_err(|_| ApiError::HistoryStorage)?;
    Ok(Json(FramePage {
        items: page.items,
        next_cursor,
        history_id: page.snapshot.history_id,
        window: page.window,
        snapshot_max_frame_id: page.snapshot.snapshot_max_frame_id,
        observed_at: page.observed_at,
        consistency: FrameConsistency::BoundedMembershipLiveContent,
    }))
}

pub(crate) async fn coverage(
    State(state): State<ApiState>,
    RawQuery(raw): RawQuery,
) -> Result<Json<CaptureCoverage>, ApiError> {
    let fields = query_fields(raw.as_deref(), &["from", "to"])?;
    let window = parse_window(&fields)?;
    state
        .host
        .capture_coverage(window)
        .await
        .map(Json)
        .map_err(history_error)
}

/// Parse percent escapes strictly: malformed encodings and invalid UTF-8 are 400,
/// never lossy replacement. Decode names before duplicate/allowlist checks too.
fn decode_component(input: &str) -> Result<String, ApiError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut input = input.bytes();
    while let Some(byte) = input.next() {
        bytes.push(match byte {
            b'+' => b' ',
            b'%' => {
                let hi = input
                    .next()
                    .and_then(|b| (b as char).to_digit(16))
                    .ok_or_else(bad_query)?;
                let lo = input
                    .next()
                    .and_then(|b| (b as char).to_digit(16))
                    .ok_or_else(bad_query)?;
                (hi * 16 + lo) as u8
            }
            other => other,
        });
    }
    String::from_utf8(bytes).map_err(|_| bad_query())
}

fn query_fields(raw: Option<&str>, allowed: &[&str]) -> Result<BTreeMap<String, String>, ApiError> {
    let mut fields = BTreeMap::new();
    for pair in raw.unwrap_or("").split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').ok_or_else(bad_query)?;
        let key = decode_component(key)?;
        if !allowed.contains(&key.as_str()) {
            return Err(bad_query());
        }
        let value = decode_component(value)?;
        if fields.insert(key, value).is_some() {
            return Err(bad_query());
        }
    }
    Ok(fields)
}

fn parse_integer(value: &str, signed: bool) -> Result<i64, ApiError> {
    let digits = if signed {
        value.strip_prefix('-').unwrap_or(value)
    } else {
        value
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad_query());
    }
    value.parse().map_err(|_| bad_query())
}

fn parse_window(fields: &BTreeMap<String, String>) -> Result<EvidenceWindow, ApiError> {
    let window = EvidenceWindow {
        from: parse_integer(fields.get("from").ok_or_else(bad_query)?, true)?,
        to: parse_integer(fields.get("to").ok_or_else(bad_query)?, true)?,
    };
    window.validate().map_err(history_error)?;
    Ok(window)
}

fn parse_frames(raw: Option<&str>) -> Result<FramePageRequest, ApiError> {
    let mut fields = query_fields(
        raw,
        &["from", "to", "app", "window_title", "limit", "cursor"],
    )?;
    let limit = fields
        .remove("limit")
        .map(|value| {
            let n = parse_integer(&value, false)?;
            if !(1..=i64::from(FRAME_PAGE_MAX_LIMIT)).contains(&n) {
                return Err(bad_query());
            }
            Ok(n as u32)
        })
        .transpose()?
        .unwrap_or(FRAME_PAGE_DEFAULT_LIMIT);
    if let Some(cursor) = fields.remove("cursor") {
        if !fields.is_empty() {
            return Err(bad_query());
        }
        return decode_cursor(&cursor, limit);
    }
    let window = parse_window(&fields)?;
    let mut filters = FrameFilters {
        app: fields.remove("app"),
        window_title: fields.remove("window_title"),
    };
    // Byte bound before normalization; never trim whitespace or Unicode-casefold.
    filters.validate().map_err(history_error)?;
    for value in filters
        .app
        .iter_mut()
        .chain(filters.window_title.iter_mut())
    {
        value.make_ascii_lowercase();
    }
    Ok(FramePageRequest {
        window,
        filters,
        limit,
        continuation: None,
    })
}

// v1: version u8, history UUID ASCII[36], five signed big-endian i64s
// (from,to,snapshot_max,last_time,last_id), then two optional UTF-8 filters.
// Filter prefix u16: 0xffff=None; 0..=256=Some byte length (including Some("")).
// No JSON escapes, token credentials or arbitrary extension fields. Max bytes:
// 1 + 36 + 5*8 + 2*(2+256) = 593; base64url-no-pad <= 791 < 4096
// for EVERY accepted UTF-8 filter pair, independent of controls/quotes/backslashes.
const CURSOR_VERSION: u8 = 1;
const CURSOR_BINARY_MAX: usize = 1 + 36 + 5 * 8 + 2 * (2 + FRAME_FILTER_MAX_BYTES);

fn encode_cursor(request: &FramePageRequest) -> Result<String, ApiError> {
    request.validate().map_err(history_error)?;
    let c = request.continuation.as_ref().ok_or_else(bad_query)?;
    let mut bytes = Vec::with_capacity(CURSOR_BINARY_MAX);
    bytes.push(CURSOR_VERSION);
    bytes.extend_from_slice(c.snapshot.history_id.as_bytes());
    for value in [
        request.window.from,
        request.window.to,
        c.snapshot.snapshot_max_frame_id,
        c.after.captured_at,
        c.after.frame_id,
    ] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    for filter in [&request.filters.app, &request.filters.window_title] {
        match filter {
            None => bytes.extend_from_slice(&u16::MAX.to_be_bytes()),
            Some(value) => {
                // Encoding enforces the same canonical normalization as decoding.
                if value != &value.to_ascii_lowercase() {
                    return Err(bad_query());
                }
                bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
                bytes.extend_from_slice(value.as_bytes());
            }
        }
    }
    let cursor = URL_SAFE_NO_PAD.encode(bytes);
    if cursor.len() > FRAME_CURSOR_MAX_BYTES {
        return Err(bad_query());
    }
    Ok(cursor)
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
    fn integer(&mut self) -> Result<i64, ApiError> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| bad_query())?,
        ))
    }
    fn filter(&mut self) -> Result<Option<String>, ApiError> {
        let length = u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| bad_query())?);
        if length == u16::MAX {
            return Ok(None);
        }
        if usize::from(length) > FRAME_FILTER_MAX_BYTES {
            return Err(bad_query());
        }
        let value =
            std::str::from_utf8(self.take(usize::from(length))?).map_err(|_| bad_query())?;
        if value != value.to_ascii_lowercase() {
            return Err(bad_query());
        }
        Ok(Some(value.to_string()))
    }
}

fn decode_cursor(cursor: &str, limit: u32) -> Result<FramePageRequest, ApiError> {
    if cursor.is_empty() || cursor.len() > FRAME_CURSOR_MAX_BYTES {
        return Err(bad_query());
    }
    let bytes = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| bad_query())?;
    if bytes.len() > CURSOR_BINARY_MAX {
        return Err(bad_query());
    }
    let mut reader = CursorReader(&bytes);
    if reader.take(1)? != [CURSOR_VERSION] {
        return Err(bad_query());
    }
    let history_id = std::str::from_utf8(reader.take(36)?)
        .map_err(|_| bad_query())?
        .to_string();
    let window = EvidenceWindow {
        from: reader.integer()?,
        to: reader.integer()?,
    };
    let snapshot_max_frame_id = reader.integer()?;
    let after = FramePosition {
        captured_at: reader.integer()?,
        frame_id: reader.integer()?,
    };
    let filters = FrameFilters {
        app: reader.filter()?,
        window_title: reader.filter()?,
    };
    if !reader.0.is_empty() {
        return Err(bad_query());
    }
    let request = FramePageRequest {
        window,
        filters,
        limit,
        continuation: Some(FrameContinuation {
            snapshot: FrameSnapshot {
                history_id,
                snapshot_max_frame_id,
            },
            after,
        }),
    };
    request.validate().map_err(history_error)?;
    Ok(request)
}

#[cfg(test)]
mod tests;
