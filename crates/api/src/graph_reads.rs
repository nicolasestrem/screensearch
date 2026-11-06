//! P8 connection and insight read adapters (`03 §7c` P8 block, `§7h.4`, `§7j`): threads,
//! entities and insights.
//!
//! Four strictly read-only routes. Each one parses its query with the strict parser from
//! [`history`](crate::history) (unknown name, duplicate, malformed escape, non-integer
//! and `from > to` are all `400 bad_request`), calls exactly **one** `Store` read through
//! [`ApiState::host`], and returns the typed value as a bare JSON array (`ThreadDetail` for
//! the detail route). Nothing here writes, generates, schedules or starts inference — a GET
//! never creates an insight, never re-clusters a thread and never re-embeds a session
//! (`§7j.5`: insights are produced by the `insight_generate` job only).
//!
//! Log discipline (`07:447`, CodeQL cleartext class): no request path or row id is logged
//! here; a missing thread is a plain 404 body and nothing else.

use std::collections::BTreeMap;

use axum::extract::{RawQuery, State};
use axum::Json;
use traits::{
    EntityFilter, EntityKind, EntitySummary, Insight, InsightFilter, InsightKind, ThreadDetail,
    ThreadFilter, ThreadSummary,
};

use crate::error::ApiError;
use crate::extract::ApiPath;
use crate::history::{bad_query, parse_integer, query_fields};
use crate::ApiState;

/// Default `limit` for the P8 list routes. Threads, entities and insights are sparse rows, so
/// one page still covers months of history for an agent — the same default the sessions and
/// reports surfaces use (`§7c`). Maintenance constant, not a setting.
const LIST_LIMIT_DEFAULT: u32 = 1000;

/// Hard ceiling on `limit` for the P8 list routes, matching the IPC clamp. A larger value is
/// clamped, never rejected (the sessions semantics); a non-integer is still a strict `400`.
/// Maintenance constant, not a setting.
const LIST_LIMIT_MAX: u32 = 1000;

/// `GET /v1/threads?from=&to=&limit=` — threads **overlapping** the half-open window
/// `[from, to)` (`first_seen_at < to AND last_seen_at >= from`; a missing bound leaves that
/// side open). Ordered `last_seen_at DESC, id DESC` by the Store. A bare JSON array of
/// [`ThreadSummary`].
pub(crate) async fn threads(
    State(state): State<ApiState>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Vec<ThreadSummary>>, ApiError> {
    let filter = parse_thread_filter(raw.as_deref())?;
    let threads = state
        .host
        .store()
        .threads_in_range(filter)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(threads))
}

/// `GET /v1/threads/{id}` — one thread with its member sessions (`started_at` ascending) and
/// the entities those sessions touch (`confidence` descending, then `canonical_name`). A
/// non-integer id is a strict `400` (the [`ApiPath`] wrapper keeps the JSON error contract);
/// a thread that does not exist is `404 not_found`, never an empty object.
pub(crate) async fn thread_detail(
    State(state): State<ApiState>,
    ApiPath(thread_id): ApiPath<i64>,
) -> Result<Json<ThreadDetail>, ApiError> {
    match state
        .host
        .store()
        .thread_detail(thread_id)
        .await
        .map_err(ApiError::Internal)?
    {
        Some(detail) => Ok(Json(detail)),
        // The id is echoed nowhere: the response says only that the thread is absent.
        None => Err(ApiError::NotFound("no such thread".to_string())),
    }
}

/// `GET /v1/entities?kind=&q=&limit=` — entities filtered by an exact `entities.kind` token
/// and by a case-insensitive substring over `canonical_name` or any alias. Ordered
/// `last_seen_at DESC, id DESC` by the Store. A bare JSON array of [`EntitySummary`].
pub(crate) async fn entities(
    State(state): State<ApiState>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Vec<EntitySummary>>, ApiError> {
    let filter = parse_entity_filter(raw.as_deref())?;
    let entities = state
        .host
        .store()
        .entities_filtered(filter)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(entities))
}

/// `GET /v1/insights?from=&to=&kind=&limit=` — insights **overlapping** the half-open window
/// `[from, to)` (`window_start < to AND window_end > from`), each with its evidence ordered by
/// `ordinal`. Ordered `window_start DESC, kind ASC, id DESC` by the Store. A bare JSON array of
/// [`Insight`]; `stale` is carried so a caller can tell a live claim from one whose evidence
/// disappeared. Reading never generates (`§7j.5`).
pub(crate) async fn insights(
    State(state): State<ApiState>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Vec<Insight>>, ApiError> {
    let filter = parse_insight_filter(raw.as_deref())?;
    let insights = state
        .host
        .store()
        .list_insights(filter)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(insights))
}

/// `GET /v1/threads` query → [`ThreadFilter`]. Strict: only `from`/`to`/`limit` are accepted.
fn parse_thread_filter(raw: Option<&str>) -> Result<ThreadFilter, ApiError> {
    let mut fields = query_fields(raw, &["from", "to", "limit"])?;
    let (from_ms, to_ms) = take_window(&mut fields)?;
    let limit = take_limit(&mut fields)?;
    take_nothing(&fields)?;
    Ok(ThreadFilter {
        from_ms,
        to_ms,
        limit,
    })
}

/// `GET /v1/entities` query → [`EntityFilter`]. `kind` must be a token of the closed
/// `entities.kind` set (`§4`); anything else is a `400` rather than a silent empty list.
/// An empty `q` means "no substring filter" — the same result every string would match.
fn parse_entity_filter(raw: Option<&str>) -> Result<EntityFilter, ApiError> {
    let mut fields = query_fields(raw, &["kind", "q", "limit"])?;
    let kind = fields
        .remove("kind")
        .map(|token| match EntityKind::from_token(&token) {
            Some(kind) => Ok(kind.as_str().to_string()),
            None => Err(bad_query()),
        })
        .transpose()?;
    let query = fields.remove("q").filter(|q| !q.is_empty());
    let limit = take_limit(&mut fields)?;
    take_nothing(&fields)?;
    Ok(EntityFilter { kind, query, limit })
}

/// `GET /v1/insights` query → [`InsightFilter`]. `kind` must be a token of the closed
/// `insights.kind` set (`§4`); an unknown token is a `400`.
fn parse_insight_filter(raw: Option<&str>) -> Result<InsightFilter, ApiError> {
    let mut fields = query_fields(raw, &["from", "to", "kind", "limit"])?;
    let (from_ms, to_ms) = take_window(&mut fields)?;
    let kind = fields
        .remove("kind")
        .map(|token| match InsightKind::from_token(&token) {
            Some(kind) => Ok(kind),
            None => Err(bad_query()),
        })
        .transpose()?;
    let limit = take_limit(&mut fields)?;
    take_nothing(&fields)?;
    Ok(InsightFilter {
        from_ms,
        to_ms,
        kind,
        limit,
    })
}

/// Removes and parses the optional half-open window bounds. Each side is independently
/// optional (unlike `/v1/search`'s both-or-neither pairing); when both are present an
/// inverted window (`from > to`) is a strict `400`, matching the store predicate's contract.
fn take_window(
    fields: &mut BTreeMap<String, String>,
) -> Result<(Option<i64>, Option<i64>), ApiError> {
    let from_ms = fields
        .remove("from")
        .map(|value| parse_integer(&value, true))
        .transpose()?;
    let to_ms = fields
        .remove("to")
        .map(|value| parse_integer(&value, true))
        .transpose()?;
    if let (Some(from), Some(to)) = (from_ms, to_ms) {
        if from > to {
            return Err(bad_query());
        }
    }
    Ok((from_ms, to_ms))
}

/// Removes and resolves `limit`: absent means [`LIST_LIMIT_DEFAULT`], and a value outside
/// `1..=LIST_LIMIT_MAX` is clamped (never rejected), exactly like `/v1/sessions`. A
/// non-integer — including a negative one — stays a strict `400`.
fn take_limit(fields: &mut BTreeMap<String, String>) -> Result<u32, ApiError> {
    fields
        .remove("limit")
        .map(|value| {
            let n = parse_integer(&value, false)?;
            Ok(n.clamp(1, i64::from(LIST_LIMIT_MAX)) as u32)
        })
        .transpose()
        .map(|limit| limit.unwrap_or(LIST_LIMIT_DEFAULT))
}

/// The allowlist already bounded the names and rejected duplicates; a key left over means a
/// registered name was accepted but never consumed — an empty 200 would be a silent contract
/// break, so it is a `400`.
fn take_nothing(fields: &BTreeMap<String, String>) -> Result<(), ApiError> {
    if fields.is_empty() {
        Ok(())
    } else {
        Err(bad_query())
    }
}
