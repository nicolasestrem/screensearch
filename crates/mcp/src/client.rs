//! The HTTP client for the ScreenSearch local API and its error mapping.
//!
//! A pure HTTP client (loopback by default; D13: no store, no app coupling). Every failure is
//! mapped to a guided, human-actionable message — the not-reachable and no-token cases both
//! carry the exact phrase **"enable the API in ScreenSearch Settings"** (`03 §7c`), because
//! those are the states a first-time user hits.

use std::time::Duration;

use serde_json::Value;

use crate::sse::{AskAggregator, Flow, SseLineBuffer};

/// Per-request timeout for the small reads (search, frames, marks, where-was-i). Generous
/// for a local SQLite query so a briefly-busy app doesn't fail a call.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Ceiling for `/v1/ask`: local generation with thinking can legitimately run minutes, but
/// an app that dies mid-stream must not wedge the MCP client forever. reqwest's timeout
/// spans the whole body read and the API's 15 s keep-alives don't reset it, so this is a
/// true worst-case cap.
const ASK_TIMEOUT: Duration = Duration::from_secs(600);

/// A failure talking to the local API, ready to render as a tool error.
#[derive(Debug)]
pub enum ApiFailure {
    /// The API is not reachable (connection refused / timed out) — almost always "the API
    /// is off".
    NotReachable { url: String },
    /// No bearer token was configured.
    NoToken,
    /// The token was rejected (HTTP 401).
    Unauthorized,
    /// Valid credential without the permission required by this route.
    Forbidden,
    /// Redirect refused before inspecting/following Location, even on the same origin.
    Redirect { status: u16 },
    /// The API returned a structured error body (`{error, message}`) — e.g. 404/503/400/500.
    Api { code: String, message: String },
    /// A transport/parse failure that isn't one of the above.
    Transport(String),
}

impl ApiFailure {
    /// The guided, human-facing message shown to the calling model as a tool error.
    pub fn message(&self) -> String {
        match self {
            ApiFailure::NotReachable { url } => format!(
                "ScreenSearch's local API is not responding at {url} — enable the API in \
                 ScreenSearch Settings (Settings -> Local API), then set \
                 SCREENSEARCH_API_TOKEN to the token shown there."
            ),
            ApiFailure::NoToken => {
                "No API token is configured. Copy the token from ScreenSearch Settings -> \
                 Local API and set SCREENSEARCH_API_TOKEN (or pass --token). If the API is \
                 off, enable the API in ScreenSearch Settings first."
                    .to_string()
            }
            ApiFailure::Unauthorized => {
                "The API token was rejected (401). Copy the current token from ScreenSearch \
                 Settings -> Local API — it may have been regenerated — and update \
                 SCREENSEARCH_API_TOKEN."
                    .to_string()
            }
            ApiFailure::Forbidden => "Permission denied (403): this API token lacks permission for the requested operation. A read-only token cannot create marks, request capture or run Ask. Use read tools (--read-only hides unsupported tools), or explicitly configure an appropriately authorized token from ScreenSearch Settings -> Local API.".to_string(),
            ApiFailure::Redirect { status } => format!("API redirect refused (HTTP {status}); no redirect destination was visited. Configure --api-url with the final HTTP/HTTPS API root. The bearer token was not forwarded."),
            ApiFailure::Api { code, message } => format!("{code}: {message}"),
            ApiFailure::Transport(e) => format!("Local API request failed: {e}"),
        }
    }
}

/// The aggregated outcome of an SSE `/v1/ask` stream.
pub struct AskOutcome {
    pub answer: String,
    pub citations: Vec<i64>,
    /// Whether a terminal `done` delta was seen (vs the stream ending early).
    pub done: bool,
    /// Whether any `token` delta arrived.
    pub tokens_seen: bool,
}

/// An async HTTP client bound to one base URL + optional bearer token.
pub struct ApiClient {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
    read_only: bool,
}

impl ApiClient {
    /// Validates the base regardless of whether it came from the CLI or a library caller.
    pub fn new(base: String, token: Option<String>) -> Result<Self, String> {
        let base = crate::config::validate_api_url(&base)?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .no_proxy() // no environment proxy, for local AND explicit remote destinations
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "could not construct API HTTP client".to_string())?;
        Ok(Self {
            base,
            token,
            http,
            read_only: false,
        })
    }

    /// Restricts the MCP registry/dispatcher, not raw HTTP authorization.
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    fn require_token(&self) -> Result<&str, ApiFailure> {
        self.token.as_deref().ok_or(ApiFailure::NoToken)
    }

    fn map_reqwest(&self, e: reqwest::Error) -> ApiFailure {
        if e.is_connect() || e.is_timeout() {
            ApiFailure::NotReachable {
                url: self.base.clone(),
            }
        } else {
            ApiFailure::Transport(e.to_string())
        }
    }

    /// Maps a non-2xx response to an [`ApiFailure`], parsing the `{error, message}` contract
    /// body. On success the response is returned unchanged.
    async fn check_status(&self, resp: reqwest::Response) -> Result<reqwest::Response, ApiFailure> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        if status.as_u16() == 401 {
            return Err(ApiFailure::Unauthorized);
        }
        if status.as_u16() == 403 {
            return Err(ApiFailure::Forbidden);
        }
        if status.is_redirection() {
            return Err(ApiFailure::Redirect {
                status: status.as_u16(),
            });
        }
        let fallback = status
            .canonical_reason()
            .unwrap_or("error")
            .to_ascii_lowercase();
        match resp.json::<Value>().await {
            Ok(v) => {
                let code = v
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or(&fallback)
                    .to_string();
                let message = v
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();
                Err(ApiFailure::Api { code, message })
            }
            Err(_) => Err(ApiFailure::Api {
                code: fallback,
                message: format!("HTTP {}", status.as_u16()),
            }),
        }
    }

    /// `GET {path}?{query}` → parsed JSON body (`null` is a valid `Value`).
    pub async fn get_json(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Value, ApiFailure> {
        let token = self.require_token()?;
        let resp = self
            .http
            .get(format!("{}{}", self.base, path))
            .query(query)
            .timeout(READ_TIMEOUT)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| self.map_reqwest(e))?;
        let resp = self.check_status(resp).await?;
        resp.json::<Value>()
            .await
            .map_err(|e| ApiFailure::Transport(e.to_string()))
    }

    /// `GET {path}?{query}` → raw bytes + the response `Content-Type` (for `?image=1`).
    pub async fn get_bytes(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<(Vec<u8>, String), ApiFailure> {
        let token = self.require_token()?;
        let resp = self
            .http
            .get(format!("{}{}", self.base, path))
            .query(query)
            .timeout(READ_TIMEOUT)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| self.map_reqwest(e))?;
        let resp = self.check_status(resp).await?;
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/webp")
            .to_string();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ApiFailure::Transport(e.to_string()))?;
        Ok((bytes.to_vec(), content_type))
    }

    /// `POST {path}` with a JSON body → parsed JSON body.
    pub async fn post_json(&self, path: &str, body: Value) -> Result<Value, ApiFailure> {
        let token = self.require_token()?;
        let resp = self
            .http
            .post(format!("{}{}", self.base, path))
            .timeout(READ_TIMEOUT)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| self.map_reqwest(e))?;
        let resp = self.check_status(resp).await?;
        resp.json::<Value>()
            .await
            .map_err(|e| ApiFailure::Transport(e.to_string()))
    }

    /// `POST /v1/ask` → the aggregated SSE stream. A terminal `error` delta becomes an
    /// [`ApiFailure::Api`]; a clean `done` or an early end returns an [`AskOutcome`].
    pub async fn ask(&self, body: Value) -> Result<AskOutcome, ApiFailure> {
        use futures_util::StreamExt;

        let token = self.require_token()?;
        let resp = self
            .http
            .post(format!("{}/v1/ask", self.base))
            .timeout(ASK_TIMEOUT)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| self.map_reqwest(e))?;
        let resp = self.check_status(resp).await?;

        let mut stream = resp.bytes_stream();
        let mut buffer = SseLineBuffer::new();
        let mut agg = AskAggregator::new();
        let mut done = false;

        'outer: while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ApiFailure::Transport(e.to_string()))?;
            for line in buffer.push(&chunk) {
                match agg.feed(&line) {
                    Flow::Continue => {}
                    Flow::Done => {
                        done = true;
                        break 'outer;
                    }
                    Flow::Failed(msg) => {
                        return Err(ApiFailure::Api {
                            code: "answer_error".to_string(),
                            message: msg,
                        })
                    }
                }
            }
        }
        if !done {
            if let Some(last) = buffer.finish() {
                match agg.feed(&last) {
                    Flow::Done => done = true,
                    Flow::Failed(msg) => {
                        return Err(ApiFailure::Api {
                            code: "answer_error".to_string(),
                            message: msg,
                        })
                    }
                    Flow::Continue => {}
                }
            }
        }

        Ok(AskOutcome {
            answer: agg.answer,
            citations: agg.citations,
            done,
            tokens_seen: agg.tokens_seen,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT_PHRASE: &str = "enable the API in ScreenSearch Settings";

    #[test]
    fn not_reachable_and_no_token_carry_the_contract_phrase() {
        let nr = ApiFailure::NotReachable {
            url: "http://127.0.0.1:43210".to_string(),
        }
        .message();
        assert!(nr.contains(CONTRACT_PHRASE), "not-reachable: {nr}");

        let nt = ApiFailure::NoToken.message();
        assert!(nt.contains(CONTRACT_PHRASE), "no-token: {nt}");
    }

    #[test]
    fn unauthorized_mentions_401_and_regeneration() {
        let m = ApiFailure::Unauthorized.message();
        assert!(m.contains("401"));
        assert!(m.to_lowercase().contains("regenerat"));
    }

    #[test]
    fn api_error_is_code_colon_message() {
        let m = ApiFailure::Api {
            code: "unavailable".to_string(),
            message: "answer model not loaded".to_string(),
        }
        .message();
        assert_eq!(m, "unavailable: answer model not loaded");
    }
}
