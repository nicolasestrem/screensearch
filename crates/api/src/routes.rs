//! The v1 route handlers (`03 §7c`). Each reuses an [`ApiHost`](crate::ApiHost) method,
//! which the composition root implements over the same core paths the Tauri commands
//! use — so the API and the UI answer identically. No new retrieval code lives here.

use std::time::Duration;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use traits::{AnswerDelta, AnswerOpts, Mark, ResumeContext, SearchHit};

use crate::error::ApiError;
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::{build_search_query, now_ms, ApiState};

/// Router fallback for any unmatched path — keeps unknown/removed endpoints on the JSON
/// error contract instead of axum's default plaintext 404 (`03 §7c`).
pub async fn not_found() -> ApiError {
    ApiError::NotFound("no such endpoint".to_string())
}

/// `GET /v1/health` — liveness + a little state (`03 §7c`).
#[derive(Serialize)]
pub struct HealthResponse {
    pub version: String,
    pub uptime_secs: u64,
    pub capturing: bool,
}

pub async fn health(State(state): State<ApiState>) -> Json<HealthResponse> {
    let uptime_ms = (now_ms() - state.host.started_at_ms()).max(0);
    Json(HealthResponse {
        version: state.host.version(),
        uptime_secs: (uptime_ms / 1000) as u64,
        capturing: state.host.is_capturing().await,
    })
}

/// `GET /v1/search?q=&from=&to=&limit=&include_chrome=` — the same hybrid search as the
/// UI, content-text by default (`03 §7c`).
#[derive(Deserialize)]
pub struct SearchParams {
    q: String,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_bool")]
    include_chrome: Option<bool>,
}

/// Accepts the API's numeric and textual query-string boolean literals while rejecting
/// everything else, preserving malformed params as `400 bad_request` (usage review 2026-08-01 §7.2).
fn deserialize_optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value.as_deref() {
        None => Ok(None),
        Some("1") => Ok(Some(true)),
        Some("0") => Ok(Some(false)),
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(serde::de::Error::custom(
            "expected one of `1`, `0`, `true`, or `false`",
        )),
    }
}

pub async fn search(
    State(state): State<ApiState>,
    ApiQuery(params): ApiQuery<SearchParams>,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    let settings = state.host.settings().await;
    let query = build_search_query(
        params.q,
        params.from,
        params.to,
        params.limit,
        params.include_chrome,
        settings.text_include_chrome_default,
    )?;
    let hits = state
        .host
        .store()
        .hybrid_search(&query)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(hits))
}

/// `GET /v1/frames/{id}` — metadata + text by default; `?image=1` returns the stored
/// WebP/JPEG bytes (`03 §7c`).
#[derive(Deserialize)]
pub struct FrameParams {
    image: Option<u8>,
}

pub async fn frame(
    State(state): State<ApiState>,
    ApiPath(id): ApiPath<i64>,
    ApiQuery(params): ApiQuery<FrameParams>,
) -> Result<Response, ApiError> {
    if params.image == Some(1) {
        match state
            .host
            .frame_image(id)
            .await
            .map_err(ApiError::Internal)?
        {
            Some(image) => {
                Ok(([(header::CONTENT_TYPE, image.content_type)], image.bytes).into_response())
            }
            // `None` means either the frame is gone or its image was purged; disambiguate
            // so the caller gets an honest code.
            None => {
                if state
                    .host
                    .frame_detail(id)
                    .await
                    .map_err(ApiError::Internal)?
                    .is_some()
                {
                    Err(ApiError::ImagePurged)
                } else {
                    Err(ApiError::NotFound(format!("frame {id} not found")))
                }
            }
        }
    } else {
        match state
            .host
            .frame_detail(id)
            .await
            .map_err(ApiError::Internal)?
        {
            Some(detail) => Ok(Json(detail).into_response()),
            None => Err(ApiError::NotFound(format!("frame {id} not found"))),
        }
    }
}

/// `GET /v1/context/where-was-i` — the where-was-i heuristic; `null` when nothing
/// qualifies (`03 §7b`/§7c).
pub async fn where_was_i(
    State(state): State<ApiState>,
) -> Result<Json<Option<ResumeContext>>, ApiError> {
    let ctx = state.host.where_was_i().await.map_err(ApiError::Internal)?;
    Ok(Json(ctx))
}

/// `GET /v1/marks` — all marks, unresolved first then newest-first (same order as
/// `list_marks`, `03 §7`).
pub async fn list_marks(State(state): State<ApiState>) -> Result<Json<Vec<Mark>>, ApiError> {
    let marks = state
        .host
        .store()
        .list_marks()
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(marks))
}

/// `POST /v1/marks` — body sets **exactly one** of `frame_id` or `now:true` (the latter
/// captures the current screen past the diff gate first, D8). The only write surface in
/// v1 besides resolve (`03 §7c`, D11).
#[derive(Deserialize)]
pub struct CreateMarkBody {
    frame_id: Option<i64>,
    #[serde(default)]
    now: bool,
    note: Option<String>,
}

#[derive(Serialize)]
pub struct CreatedMark {
    pub mark_id: i64,
}

pub async fn create_mark(
    State(state): State<ApiState>,
    ApiJson(body): ApiJson<CreateMarkBody>,
) -> Result<Response, ApiError> {
    match (body.frame_id, body.now) {
        (Some(frame_id), false) => {
            // Pre-check so a bad frame_id is a clean 404, not an FK-violation 500.
            if state
                .host
                .frame_detail(frame_id)
                .await
                .map_err(ApiError::Internal)?
                .is_none()
            {
                return Err(ApiError::NotFound(format!("frame {frame_id} not found")));
            }
            let mark_id = state
                .host
                .add_mark(Some(frame_id), false, body.note)
                .await
                .map_err(ApiError::Internal)?;
            Ok((StatusCode::CREATED, Json(CreatedMark { mark_id })).into_response())
        }
        (None, true) => {
            // `capture_now` fails honestly when capture is off / privacy denies — 503
            // with the reason, never a silent no-op (`03 §7b`).
            let mark_id = state
                .host
                .add_mark(None, true, body.note)
                .await
                .map_err(|e| ApiError::Unavailable(e.to_string()))?;
            Ok((StatusCode::CREATED, Json(CreatedMark { mark_id })).into_response())
        }
        _ => Err(ApiError::BadRequest(
            "body must set exactly one of `frame_id` or `now: true`".to_string(),
        )),
    }
}

/// `POST /v1/marks/{id}/resolve` — resolve = done, dismiss = resolve-with-no-action
/// (same op, `03 §7b`). Idempotent; unknown id → 404.
#[derive(Serialize)]
pub struct ResolvedMark {
    pub resolved: bool,
}

pub async fn resolve_mark(
    State(state): State<ApiState>,
    ApiPath(id): ApiPath<i64>,
) -> Result<Json<ResolvedMark>, ApiError> {
    // Marks are user-created (tens, not millions), so an O(n) existence check avoids a
    // new store method while still giving a clean 404.
    let exists = state
        .host
        .store()
        .list_marks()
        .await
        .map_err(ApiError::Internal)?
        .iter()
        .any(|m| m.mark_id == id);
    if !exists {
        return Err(ApiError::NotFound(format!("mark {id} not found")));
    }
    state
        .host
        .resolve_mark(id)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(ResolvedMark { resolved: true }))
}

/// `POST /v1/ask` — a grounded answer as an SSE stream of `AnswerDelta` events (`03 §7c`).
/// Reuses the ask pipeline; the sidecar cancellation fix means a disconnected client
/// stops generation (the dropped SSE receiver closes the delta channel).
#[derive(Deserialize)]
pub struct AskBody {
    query: String,
    top_k: Option<u32>,
    thinking: Option<bool>,
    max_tokens: Option<u32>,
    /// 0.4.0 PR6 (D12): scope retrieval to one session's frames so the answer cites only
    /// in-session frames. Absent → unchanged frame-level behavior (D10).
    session_id: Option<i64>,
}

pub async fn ask(
    State(state): State<ApiState>,
    ApiJson(body): ApiJson<AskBody>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    // A bad `session_id` is a 404 on the request itself — resolve it *before* the answer-model
    // check and before any SSE headers go out (a 404 must not arrive as a stream, and it takes
    // precedence over the 503 for a missing model).
    if let Some(id) = body.session_id {
        if state
            .host
            .store()
            .get_session(id)
            .await
            .map_err(ApiError::Internal)?
            .is_none()
        {
            return Err(ApiError::NotFound(format!("session {id} not found")));
        }
    }
    let provider = state
        .host
        .answer_provider()
        .await
        .ok_or_else(|| ApiError::Unavailable("answer model not loaded".to_string()))?;
    let settings = state.host.settings().await;
    let top_k = body.top_k.unwrap_or(settings.retrieval_default_top_k);
    // Ground before streaming; a hydration failure is a hard error (no silent
    // low-quality answer), mirroring the `ask` command.
    let context = state
        .host
        .ask_context(&body.query, top_k, body.session_id)
        .await
        .map_err(ApiError::Internal)?;
    let opts = AnswerOpts {
        thinking: body.thinking.unwrap_or(settings.answer_thinking),
        max_tokens: body.max_tokens.unwrap_or(2048),
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<AnswerDelta>(64);
    let query = body.query;
    tokio::spawn(async move {
        // The provider drops `tx` when done; on client disconnect the SSE body (and thus
        // `rx`) is dropped, closing `tx` — which the sidecar cancellation path observes.
        let _ = provider.answer(&query, &context, opts, tx).await;
    });

    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|delta| {
            // Serialize the tagged AnswerDelta as the SSE `data:` payload. Serialization
            // of a small enum cannot fail; fall back to an empty event if it ever does.
            let event = Event::default()
                .json_data(&delta)
                .unwrap_or_else(|_| Event::default().data("{}"));
            (Ok(event), rx)
        })
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;

    use super::SearchParams;

    fn parse_include_chrome(value: Option<&str>) -> Result<Option<bool>, String> {
        let suffix = value
            .map(|value| format!("&include_chrome={value}"))
            .unwrap_or_default();
        let uri: Uri = format!("/v1/search?q=test{suffix}").parse().unwrap();
        Query::<SearchParams>::try_from_uri(&uri)
            .map(|Query(params)| params.include_chrome)
            .map_err(|error| error.to_string())
    }

    #[test]
    fn include_chrome_accepts_numeric_and_textual_booleans() {
        assert_eq!(parse_include_chrome(Some("1")).unwrap(), Some(true));
        assert_eq!(parse_include_chrome(Some("0")).unwrap(), Some(false));
        assert_eq!(parse_include_chrome(Some("true")).unwrap(), Some(true));
        assert_eq!(parse_include_chrome(Some("false")).unwrap(), Some(false));
        assert_eq!(parse_include_chrome(Some("TrUe")).unwrap(), Some(true));
        assert_eq!(parse_include_chrome(Some("FaLsE")).unwrap(), Some(false));
    }

    #[test]
    fn include_chrome_absent_is_none() {
        assert_eq!(parse_include_chrome(None).unwrap(), None);
    }

    #[test]
    fn include_chrome_rejects_other_literals() {
        assert!(parse_include_chrome(Some("yes")).is_err());
    }
}
