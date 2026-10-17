//! Bearer-token authentication for the local API (`03 §7c`, D11).
//!
//! Applied to **every** `/v1` route, health included — the whole surface is private. A
//! request must carry `Authorization: Bearer <token>` matching the current token exactly;
//! anything else is 401. The token is compared in constant time so a timing side channel
//! can't recover it byte by byte.

use crate::error::ApiError;
use crate::permissions::{Policies, Principal};
use crate::ApiState;
use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct AuthState {
    pub(crate) api: ApiState,
    pub(crate) policies: Arc<Policies>,
}

pub(crate) async fn require_bearer(
    State(state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Response {
    let presented = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let principal = presented.and_then(|token| {
        let legacy = state.api.token.read().expect("api token lock poisoned");
        let read = state
            .api
            .read_token
            .read()
            .expect("api read token lock poisoned");
        let legacy_match =
            !legacy.is_empty() && constant_time_eq(token.as_bytes(), legacy.as_bytes());
        let read_match = read.as_deref().is_some_and(|current| {
            !current.is_empty() && constant_time_eq(token.as_bytes(), current.as_bytes())
        });
        // Colliding configuration fails toward least privilege, never elevates the read slot.
        if read_match {
            Some(Principal::READ)
        } else if legacy_match {
            Some(Principal::LEGACY)
        } else {
            None
        }
    });
    let Some(principal) = principal else {
        return ApiError::Unauthorized.into_response();
    };
    if let Some(response) = state.policies.authorize(
        req.extensions().get::<MatchedPath>(),
        req.method(),
        &principal,
    ) {
        return response;
    }
    req.extensions_mut().insert(principal);
    next.run(req).await
}

/// Constant-time byte-slice equality — no new dependency. Accumulates the XOR of every
/// byte so the running time does not depend on where the first difference is. A length
/// mismatch returns early: the token length is fixed (64 hex chars), so length is not a
/// secret and leaking it reveals nothing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn constant_time_eq_matches_only_identical_slices() {
        assert!(constant_time_eq(b"abc123", b"abc123"));
        assert!(!constant_time_eq(b"abc123", b"abc124"));
        assert!(!constant_time_eq(b"abc123", b"abc12"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }
}
