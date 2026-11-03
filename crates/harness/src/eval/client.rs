//! The product under measurement, reached only through the local HTTP API (`03 §7c`).
//!
//! `GET /v1/search` is the retrieval arm (diagnostic) and `POST /v1/ask` is the answer arm
//! (comparator). Nothing here re-implements retrieval: the harness reads what the product
//! exposes, and a missing surface is a stop-and-report, never a harness-side substitute.
//!
//! **[`AskStreamReader`] reads both citation shapes now.** Today the stream emits one `citation`
//! event per frame; P6 (0.4.5 D12, `03 §7i.3`) adds one terminal `citations` event carrying
//! `{"sessions":[id...],"frames":[id...]}` while keeping every existing event byte-identical.
//! When the terminal event is present its two arrays win and every other citation field is
//! ignored; otherwise the per-frame `citation` events are used. On today's stream both paths
//! produce the same frame set, so the P4 baseline is unaffected.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How long the reader waits for a trailing citations event after `done`/`error` before closing.
pub const TERMINAL_GRACE: Duration = Duration::from_secs(2);

/// The four failure classes the report counts separately (`docs/kickoffs/P4.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The API or the answer model is not reachable / not loaded (connection error, 503).
    ApiUnavailable,
    /// The turn exceeded `--timeout-secs`.
    Timeout,
    /// The stream completed but carried no answer text.
    EmptyAnswer,
    /// Any other HTTP answer, a malformed stream, or a stream `error` event.
    HttpError,
}

impl FailureClass {
    pub fn label(self) -> &'static str {
        match self {
            FailureClass::ApiUnavailable => "api_unavailable",
            FailureClass::Timeout => "timeout",
            FailureClass::EmptyAnswer => "empty_answer",
            FailureClass::HttpError => "http_error",
        }
    }
}

/// One failed turn. `detail` is short and safe to paste.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnFailure {
    pub class: FailureClass,
    pub detail: String,
    /// The terminal `error` event payload, when the failure came from the stream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<Value>,
}

impl std::fmt::Display for TurnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.class.label(), self.detail)
    }
}

/// One successful turn.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    pub answer: String,
    pub cited_frames: Vec<i64>,
    pub cited_sessions: Vec<i64>,
    pub latency_ms: u64,
    /// The terminal `done` payload, verbatim, for `runs.jsonl`.
    pub terminal: Value,
}

/// `GET /v1/health` payload (the fields the report prints).
#[derive(Debug, Clone, Deserialize)]
pub struct HealthInfo {
    pub version: String,
    #[serde(default)]
    pub uptime_secs: u64,
    #[serde(default)]
    pub capturing: bool,
}

/// The two citations arrays carried by the future terminal event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Citations {
    pub frames: Vec<i64>,
    pub sessions: Vec<i64>,
}

/// Incremental SSE `data:` reader for `POST /v1/ask`. Pure: no I/O, no clock, testable with a
/// stubbed byte stream.
#[derive(Debug, Default, Clone)]
pub struct AskStreamReader {
    answer: String,
    cited_frames: Vec<i64>,
    terminal_citations: Option<Citations>,
    terminal: Option<Value>,
    error: Option<String>,
}

impl AskStreamReader {
    /// Consume one SSE `data:` payload. Returns `true` when a terminal event arrived.
    pub fn feed(&mut self, data: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            // A non-JSON data line is not part of any known event shape; ignore it rather than
            // failing the turn on a future keep-alive payload.
            return false;
        };
        // Future terminal citations event: recognised by its arrays, whatever its `type` token is,
        // so a later phase can rename the event without breaking this reader.
        if value.get("frames").is_some() || value.get("sessions").is_some() {
            let citations = Citations {
                frames: ids_of(&value, "frames"),
                sessions: ids_of(&value, "sessions"),
            };
            if !citations.frames.is_empty() || !citations.sessions.is_empty() {
                self.terminal_citations = Some(citations);
            }
        }
        match value.get("type").and_then(Value::as_str).unwrap_or("") {
            "token" => {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    self.answer.push_str(text);
                }
                false
            }
            "thinking" => false,
            "citation" => {
                if let Some(id) = value.get("frame_id").and_then(Value::as_i64) {
                    push_unique(&mut self.cited_frames, id);
                }
                false
            }
            "done" => {
                self.terminal = Some(value);
                true
            }
            "error" => {
                self.error = Some(
                    value
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("stream error event")
                        .to_string(),
                );
                self.terminal = Some(value);
                true
            }
            _ => false,
        }
    }

    pub fn answer(&self) -> &str {
        &self.answer
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn terminal(&self) -> Option<&Value> {
        self.terminal.as_ref()
    }

    /// Cited frame ids: the terminal event's array when present, else the per-frame events.
    pub fn citation_frames(&self) -> Vec<i64> {
        match &self.terminal_citations {
            Some(c) => c.frames.clone(),
            None => self.cited_frames.clone(),
        }
    }

    /// Cited session ids: only the terminal event carries them today (P6), so otherwise empty.
    pub fn citation_sessions(&self) -> Vec<i64> {
        self.terminal_citations
            .as_ref()
            .map(|c| c.sessions.clone())
            .unwrap_or_default()
    }
}

/// The HTTP client for one run. `base` must be an absolute `http(s)://` URL.
#[derive(Debug, Clone)]
pub struct ApiClient {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(base: &str, token: &str, timeout: Duration) -> Result<Self, String> {
        let base = base.trim_end_matches('/').to_string();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(format!(
                "API base {base:?} is not an absolute http(s) URL (e.g. http://127.0.0.1:43210)"
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(timeout)
            // Loopback only: never let a proxy environment variable reroute private history.
            .no_proxy()
            .build()
            .map_err(|e| format!("building the HTTP client: {e}"))?;
        Ok(Self {
            base,
            token: token.to_string(),
            http,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// `GET /v1/health` — the preflight, so a wrong token or a disabled API fails with a usable
    /// message before any pair is asked.
    pub async fn health(&self) -> Result<HealthInfo, String> {
        let resp = self
            .http
            .get(format!("{}/v1/health", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| connectivity_message(&self.base, &e))?;
        let status = resp.status();
        if status.as_u16() == 401 {
            return Err(format!(
                "the bearer token from `settings.api.token` was rejected (401) at {} \u{2014} \
                 enable or regenerate the API token in ScreenSearch Settings > Integrations",
                self.base
            ));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "GET {}/v1/health returned HTTP {}: {}",
                self.base,
                status.as_u16(),
                truncate(&body, 200)
            ));
        }
        resp.json::<HealthInfo>()
            .await
            .map_err(|e| format!("parsing /v1/health: {e}"))
    }

    /// `GET /v1/search?q=&limit=10&include_chrome=false` — the diagnostic retrieval arm.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<i64>, String> {
        let resp = self
            .http
            .get(format!("{}/v1/search", self.base))
            .bearer_auth(&self.token)
            .query(&[
                ("q", query),
                ("limit", &limit.to_string()),
                ("include_chrome", "false"),
            ])
            .send()
            .await
            .map_err(|e| connectivity_message(&self.base, &e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "GET /v1/search returned HTTP {}: {}",
                status.as_u16(),
                truncate(&body, 200)
            ));
        }
        let hits = resp
            .json::<Vec<SearchHitWire>>()
            .await
            .map_err(|e| format!("parsing /v1/search: {e}"))?;
        Ok(hits
            .into_iter()
            .take(limit as usize)
            .map(|h| h.frame_id)
            .collect())
    }

    /// `POST /v1/ask` — one turn, from request to terminal event.
    pub async fn ask(&self, query: &str, top_k: Option<u32>) -> Result<TurnOutcome, TurnFailure> {
        let started = Instant::now();
        let resp = self
            .http
            .post(format!("{}/v1/ask", self.base))
            .bearer_auth(&self.token)
            .json(&AskBody { query, top_k })
            .send()
            .await
            .map_err(map_request_error)?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let class = if status.as_u16() == 503 {
                FailureClass::ApiUnavailable
            } else {
                FailureClass::HttpError
            };
            return Err(TurnFailure {
                class,
                detail: format!("HTTP {}: {}", status.as_u16(), truncate(&body, 300)),
                terminal: None,
            });
        }

        let mut stream = std::pin::pin!(resp.bytes_stream());
        let mut reader = AskStreamReader::default();
        let mut buffer = String::new();
        let mut terminal_at: Option<Instant> = None;
        while let Some(chunk) = next_chunk(&mut stream, terminal_at).await {
            let chunk = chunk.map_err(map_request_error)?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buffer.find('\n') {
                let mut line = buffer.drain(..=pos).collect::<String>();
                while line.ends_with('\n') || line.ends_with('\r') {
                    line.pop();
                }
                if let Some(data) = line.strip_prefix("data:") {
                    if reader.feed(data.trim_start()) && terminal_at.is_none() {
                        terminal_at = Some(Instant::now());
                    }
                }
            }
        }

        let latency_ms = terminal_at
            .unwrap_or_else(Instant::now)
            .duration_since(started)
            .as_millis() as u64;
        let terminal_event = reader.terminal().cloned();

        if let Some(error) = reader.error() {
            return Err(TurnFailure {
                class: FailureClass::HttpError,
                detail: format!("stream `error` event: {}", truncate(error, 300)),
                terminal: terminal_event,
            });
        }
        if terminal_at.is_none() {
            return Err(TurnFailure {
                class: FailureClass::HttpError,
                detail: "the stream ended without a terminal `done` or `error` event".to_string(),
                terminal: terminal_event,
            });
        }
        if reader.answer().trim().is_empty() {
            return Err(TurnFailure {
                class: FailureClass::EmptyAnswer,
                detail: "the stream completed with no answer text".to_string(),
                terminal: terminal_event,
            });
        }
        Ok(TurnOutcome {
            answer: reader.answer().to_string(),
            cited_frames: reader.citation_frames(),
            cited_sessions: reader.citation_sessions(),
            latency_ms,
            terminal: terminal_event.unwrap_or(Value::Null),
        })
    }
}

#[derive(Serialize)]
struct AskBody<'a> {
    query: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

#[derive(Deserialize)]
struct SearchHitWire {
    frame_id: i64,
}

/// Wait for the next byte-stream chunk. After the terminal event only a short grace window runs,
/// so a server that keeps the SSE connection alive cannot hang the run.
async fn next_chunk<S>(stream: &mut S, terminal_at: Option<Instant>) -> Option<S::Item>
where
    S: futures_util::Stream + Unpin,
{
    match terminal_at {
        None => stream.next().await,
        Some(at) => {
            let grace = TERMINAL_GRACE.saturating_sub(at.elapsed());
            tokio::time::timeout(grace.max(Duration::from_millis(1)), stream.next())
                .await
                .unwrap_or_default()
        }
    }
}

fn ids_of(value: &Value, key: &str) -> Vec<i64> {
    let mut out = Vec::new();
    if let Some(items) = value.get(key).and_then(Value::as_array) {
        for item in items {
            if let Some(id) = item.as_i64() {
                push_unique(&mut out, id);
            }
        }
    }
    out
}

fn push_unique(items: &mut Vec<i64>, id: i64) {
    if !items.contains(&id) {
        items.push(id);
    }
}

fn map_request_error(error: reqwest::Error) -> TurnFailure {
    let class = if error.is_timeout() {
        FailureClass::Timeout
    } else if error.is_connect() {
        FailureClass::ApiUnavailable
    } else {
        FailureClass::HttpError
    };
    TurnFailure {
        class,
        detail: truncate(&error.to_string(), 300),
        terminal: None,
    }
}

fn connectivity_message(base: &str, error: &reqwest::Error) -> String {
    if error.is_connect() {
        format!(
            "the API is not reachable at {base} \u{2014} start ScreenSearch, enable the API in \
             Settings > Integrations, and check `api.bind_address` / `api.port` (03 \u{a7}7c)"
        )
    } else if error.is_timeout() {
        format!("request to {base} timed out")
    } else {
        format!("request to {base} failed: {error}")
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(max_chars).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(reader: &mut AskStreamReader, payloads: &[&str]) -> Vec<bool> {
        payloads.iter().map(|p| reader.feed(p)).collect()
    }

    #[test]
    fn todays_stream_reads_tokens_and_frame_citations() {
        let mut r = AskStreamReader::default();
        let done = feed_all(
            &mut r,
            &[
                r#"{"type":"thinking","text":"hmm"}"#,
                r#"{"type":"token","text":"You were in "}"#,
                r#"{"type":"token","text":"VS Code."}"#,
                r#"{"type":"citation","frame_id":12}"#,
                r#"{"type":"citation","frame_id":12}"#,
                r#"{"type":"citation","frame_id":9}"#,
                r#"{"type":"done"}"#,
            ],
        );
        assert_eq!(done, vec![false, false, false, false, false, false, true]);
        assert_eq!(r.answer(), "You were in VS Code.");
        assert_eq!(r.citation_frames(), vec![12, 9], "deduped, in stream order");
        assert!(r.citation_sessions().is_empty());
        assert_eq!(r.terminal().unwrap()["type"], "done");
    }

    #[test]
    fn future_terminal_citations_event_wins_over_per_frame_events() {
        let mut r = AskStreamReader::default();
        feed_all(
            &mut r,
            &[
                r#"{"type":"token","text":"It was Wednesday."}"#,
                r#"{"type":"citation","frame_id":5}"#,
                r#"{"type":"citation","frame_id":6}"#,
                r#"{"type":"citations","sessions":[1,2],"frames":[7,8],"stage_counts":{"threads":3},"tokens":1234}"#,
                r#"{"type":"done","stage_counts":{"threads":3}}"#,
            ],
        );
        assert_eq!(
            r.citation_frames(),
            vec![7, 8],
            "the terminal event's array replaces the per-frame citations"
        );
        assert_eq!(r.citation_sessions(), vec![1, 2]);
        assert_eq!(r.answer(), "It was Wednesday.");
        // Unknown diagnostic fields are ignored, never fatal.
        assert_eq!(r.terminal().unwrap()["stage_counts"]["threads"], 3);
    }

    #[test]
    fn a_renamed_terminal_event_is_still_recognised_by_its_arrays() {
        let mut r = AskStreamReader::default();
        feed_all(
            &mut r,
            &[r#"{"type":"answer_citations","sessions":[],"frames":[42]}"#],
        );
        assert_eq!(r.citation_frames(), vec![42]);
        assert!(r.citation_sessions().is_empty());
    }

    #[test]
    fn error_events_are_terminal_and_keep_their_payload() {
        let mut r = AskStreamReader::default();
        assert!(r.feed(r#"{"type":"error","message":"sidecar stopped"}"#));
        assert_eq!(r.error(), Some("sidecar stopped"));
        assert_eq!(r.terminal().unwrap()["message"], "sidecar stopped");
    }

    #[test]
    fn junk_data_lines_do_not_fail_the_turn() {
        let mut r = AskStreamReader::default();
        assert!(!r.feed("not json at all"));
        assert!(!r.feed("[1,2,3]"));
        assert_eq!(r.answer(), "");
        assert!(r.error().is_none());
    }
}
