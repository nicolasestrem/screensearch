//! The API error model: one JSON body shape for every non-2xx response (`03 §7c`).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// The JSON body of every error response: a stable machine `error` code plus a
/// human-readable `message`. Documented in `docs/API.md`.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: &'static str,
    pub message: String,
}

/// Every failure the API surface can return, mapped to a status + `ErrorBody`.
#[derive(Debug)]
pub enum ApiError {
    /// Missing or wrong bearer token → 401.
    Unauthorized,
    /// Authenticated credential lacks the explicitly required permission → 403.
    Forbidden,
    /// Malformed request (bad params/body/format) → 400.
    BadRequest(String),
    /// The requested frame/mark does not exist → 404.
    NotFound(String),
    /// The frame exists but its image was retention-purged → 404 (distinct code).
    ImagePurged,
    /// Cursor refers to a different initialized history → 409.
    HistoryChanged,
    /// New history reads deliberately never expose or log underlying private errors.
    HistoryStorage,
    /// A dependency needed to serve the request is unavailable (answer model not
    /// loaded, capture off for `capture_now`) → 503.
    Unavailable(String),
    /// An unexpected internal failure (store error) → 500. The message is included in
    /// the body and logged.
    Internal(anyhow::Error),
}

impl ApiError {
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid bearer token".to_string(),
            ),
            ApiError::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "bearer token does not grant the required permission".to_string(),
            ),
            ApiError::HistoryChanged => (
                StatusCode::CONFLICT,
                "history_changed",
                "history changed; restart the traversal".to_string(),
            ),
            ApiError::HistoryStorage => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "history storage error".to_string(),
            ),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m.clone()),
            ApiError::ImagePurged => (
                StatusCode::NOT_FOUND,
                "image_purged",
                "the frame's image was retention-purged; text is preserved".to_string(),
            ),
            ApiError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", m.clone()),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, message) = self.parts();
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(%error, %message, "local API internal error");
        }
        (status, Json(ErrorBody { error, message })).into_response()
    }
}
