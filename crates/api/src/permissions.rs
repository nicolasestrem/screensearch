//! Explicit route/method classification; no credential has a wildcard grant.
use std::sync::Arc;

use axum::extract::MatchedPath;
use axum::handler::Handler;
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;

use crate::{auth, error::ApiError, ApiState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Permission {
    HistoryRead,
    MarksWrite,
    CaptureRequest,
    AnswerGenerate,
    // Reserved, unsupported: deliberately absent from every credential grant.
    #[allow(dead_code)]
    ReportsGenerate,
}

#[derive(Clone)]
pub(crate) struct Principal(pub(crate) &'static [Permission]);

impl Principal {
    pub(crate) const LEGACY: Self = Self(&[
        Permission::HistoryRead,
        Permission::MarksWrite,
        Permission::CaptureRequest,
        Permission::AnswerGenerate,
    ]);
    pub(crate) const READ: Self = Self(&[Permission::HistoryRead]);

    pub(crate) fn require(&self, permission: Permission) -> Result<(), ApiError> {
        if self.0.contains(&permission) {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

#[derive(Clone)]
struct RoutePolicy {
    path: &'static str,
    method: Method,
    permission: Permission,
}

#[derive(Clone)]
pub(crate) struct Policies(Vec<RoutePolicy>);

impl Policies {
    pub(crate) fn authorize(
        &self,
        path: Option<&MatchedPath>,
        method: &Method,
        principal: &Principal,
    ) -> Option<Response> {
        let Some(path) = path else {
            return Some(ApiError::NotFound("no such endpoint".to_string()).into_response());
        };
        let entries = || self.0.iter().filter(|p| p.path == path.as_str());
        if !entries().any(|_| true) {
            return Some(ApiError::Forbidden.into_response());
        }
        if let Some(policy) = entries().find(|p| p.method == *method) {
            return principal
                .require(policy.permission)
                .err()
                .map(IntoResponse::into_response);
        }
        // Terminal 405, never dispatch a newly registered but unclassified method.
        let allow = entries()
            .map(|p| p.method.as_str())
            .collect::<Vec<_>>()
            .join(",");
        Some((StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, allow)]).into_response())
    }
}

/// Register handler and required permission together. GET explicitly classifies HEAD.
/// The global gate also rejects any raw/unclassified route added around this registry.
pub(crate) struct ClassifiedRouter {
    router: Router<ApiState>,
    policies: Policies,
}

impl ClassifiedRouter {
    pub(crate) fn new() -> Self {
        Self {
            router: Router::new(),
            policies: Policies(Vec::new()),
        }
    }
    pub(crate) fn get<H, T>(
        mut self,
        path: &'static str,
        handler: H,
        permission: Permission,
    ) -> Self
    where
        H: Handler<T, ApiState>,
        T: 'static,
    {
        self.router = self.router.route(path, get(handler));
        for method in [Method::GET, Method::HEAD] {
            self.policies.0.push(RoutePolicy {
                path,
                method,
                permission,
            });
        }
        self
    }
    pub(crate) fn post<H, T>(
        mut self,
        path: &'static str,
        handler: H,
        permission: Permission,
    ) -> Self
    where
        H: Handler<T, ApiState>,
        T: 'static,
    {
        self.router = self.router.route(path, post(handler));
        self.policies.0.push(RoutePolicy {
            path,
            method: Method::POST,
            permission,
        });
        self
    }
    pub(crate) fn finish(self, state: ApiState) -> Router {
        let auth_state = auth::AuthState {
            api: state.clone(),
            policies: Arc::new(self.policies),
        };
        self.router
            .fallback(crate::routes::not_found)
            .layer(axum::middleware::from_fn_with_state(
                auth_state,
                auth::require_bearer,
            ))
            .with_state(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn new_routes_and_methods_never_inherit_either_credential_grants() {
        let state = crate::connection_tests::permission_test_state();
        *state.read_token.write().unwrap() = Some("permission-read".into());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut routes = ClassifiedRouter::new()
            .get(
                "/known",
                || async { "explicit read" },
                Permission::HistoryRead,
            )
            .post(
                "/future-report",
                || async { "must not generate" },
                Permission::ReportsGenerate,
            );
        let c = calls.clone();
        routes.router = routes.router.route(
            "/unclassified",
            get(move || async move {
                c.fetch_add(1, Ordering::SeqCst);
                "unsafe new handler"
            }),
        );
        let c = calls.clone();
        routes.router = routes.router.route(
            "/known",
            axum::routing::put(move || async move {
                c.fetch_add(1, Ordering::SeqCst);
                "unsafe new method"
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = routes.finish(state);
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::new();
        for token in ["permission-legacy", "permission-read"] {
            for method in [
                reqwest::Method::GET,
                reqwest::Method::HEAD,
                reqwest::Method::POST,
            ] {
                assert_eq!(
                    client
                        .request(method, format!("{base}/unclassified"))
                        .bearer_auth(token)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    403
                );
            }
            assert_eq!(
                client
                    .put(format!("{base}/known"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                405
            );
            assert_eq!(
                client
                    .post(format!("{base}/future-report"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                403
            );
            assert_eq!(
                client
                    .get(format!("{base}/known"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );
        }
        assert_eq!(
            client
                .get(format!("{base}/unclassified"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn marks_write_alone_cannot_enter_capture_now() {
        let state = crate::connection_tests::permission_test_state();
        let principal = Principal(&[Permission::MarksWrite]);
        let body = serde_json::from_value(serde_json::json!({"now": true})).unwrap();
        // This host returns an error if add_mark is reached, which would map to 503.
        // 403 proves the separate body-dependent capture gate runs before that call.
        let result = crate::routes::create_mark(
            axum::extract::State(state),
            axum::Extension(principal),
            crate::extract::ApiJson(body),
        )
        .await;
        assert!(matches!(result, Err(ApiError::Forbidden)));
    }
}
