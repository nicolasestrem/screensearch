//! Integration tests for the local API (0.3.0 PR7; `03 §7c`, DoD `§13b` item 6).
//!
//! A `TestHost` implements [`api::ApiHost`] over a real in-memory `SqliteStore` (plus a
//! scripted answer provider), so every endpoint is exercised against a populated DB. The
//! server binds an ephemeral loopback port; requests go out over `reqwest` with a bearer
//! token, exactly as a real client would.

use std::sync::Arc;

use api::{ApiHost, ApiServer, FrameImage, SharedToken};
use async_trait::async_trait;
use store::SqliteStore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::Sender;
use traits::{
    AnswerDelta, AnswerOpts, AnswerProvider, EntityKind, EvidenceKind, ExportFrameRow, FrameDetail,
    GraphSessionLinkAggregate, InsightEvidence, InsightGenerator, InsightPayload, NewFrame,
    NewInsight, NewSession, NewSessionArtifact, OcrResult, OpenLoopPayload, OpenLoopSource,
    RecurrencePayload, ResumeContext, ResumptionPayload, RetrievedChunk, SemanticNeighbourPayload,
    SessionArtifactKind, SessionArtifactRole, SessionHost, SessionKind, SessionNeighbour, Settings,
    Store, ThreadActivityPayload, ThreadDraft, ThreadMemberDraft, ThreadPlan,
};

#[path = "support/history_reads.rs"]
mod history_reads;
#[path = "support/permissions.rs"]
mod permission_cases;
#[path = "support/reports.rs"]
mod reports;

const TEST_TOKEN: &str = "test-token-000000000000000000000000000000000000000000000000000000";

/// A scripted answer provider: streams a fixed set of deltas, then `Done`.
struct ScriptedAnswer;

#[async_trait]
impl AnswerProvider for ScriptedAnswer {
    async fn answer(
        &self,
        _query: &str,
        context: &[RetrievedChunk],
        _opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> anyhow::Result<()> {
        let _ = tx
            .send(AnswerDelta::Token {
                text: "grounded answer".to_string(),
            })
            .await;
        // Cite *every* grounded frame, so a test can assert the citation set equals the
        // retrieved context (which is what session scoping restricts).
        for chunk in context {
            let _ = tx
                .send(AnswerDelta::Citation {
                    frame_id: chunk.frame_id,
                })
                .await;
        }
        let _ = tx.send(AnswerDelta::done()).await;
        Ok(())
    }
}

/// The test seam over a real store. `answer` toggles whether an answer provider is
/// available (for the 503 path).
struct TestHost {
    store: Arc<SqliteStore>,
    answer: Option<Arc<dyn AnswerProvider>>,
    started_at_ms: i64,
    capture_kernel: Option<Arc<kernel::Kernel>>,
    history_fault: Option<history_reads::HistoryFault>,
    /// When set, `report_page` returns this page verbatim — the only way to exercise the
    /// handler's internal-consistency guard (a next position without a snapshot).
    report_page_override: Option<traits::StoredReportPage>,
}

#[async_trait]
impl ApiHost for TestHost {
    fn version(&self) -> String {
        "test-0.3.0".to_string()
    }
    fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }
    async fn is_capturing(&self) -> bool {
        false
    }
    fn store(&self) -> Arc<dyn Store> {
        self.store.clone()
    }
    async fn settings(&self) -> Settings {
        Settings::default()
    }
    async fn frame_detail(&self, frame_id: i64) -> anyhow::Result<Option<FrameDetail>> {
        self.store.get_frame(frame_id).await
    }
    async fn frame_page(
        &self,
        request: traits::FramePageRequest,
    ) -> Result<traits::StoredFramePage, traits::HistoryReadError> {
        if let Some(fault) = self.history_fault {
            return Err(fault.error());
        }
        self.store.frame_page(request).await
    }
    async fn capture_coverage(
        &self,
        window: traits::EvidenceWindow,
    ) -> Result<traits::CaptureCoverage, traits::HistoryReadError> {
        if let Some(fault) = self.history_fault {
            return Err(fault.error());
        }
        history_reads::compose_coverage(self, window).await
    }
    async fn frame_image(&self, frame_id: i64) -> anyhow::Result<Option<FrameImage>> {
        // The fixture doesn't write real image files; a frame with image_purged=false and
        // a known path returns synthetic bytes, a purged one returns None.
        match self.store.get_frame(frame_id).await? {
            Some(detail) if !detail.image_purged => Ok(Some(FrameImage {
                bytes: b"webp-bytes".to_vec(),
                content_type: "image/webp",
            })),
            _ => Ok(None),
        }
    }
    async fn ask_context(
        &self,
        query: &str,
        top_k: u32,
        session_id: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>> {
        let q = traits::SearchQuery {
            text: query.to_string(),
            limit: top_k,
            time_range: None,
            include_chrome: false,
        };
        let hits = match session_id {
            Some(id) => self.store.hybrid_search_in_session(&q, id).await?,
            None => self.store.hybrid_search(&q).await?,
        };
        Ok(hits
            .into_iter()
            .map(|h| RetrievedChunk {
                frame_id: h.frame_id,
                text: h.snippet,
                score: h.score,
                captured_at: h.captured_at,
                session: None,
            })
            .collect())
    }
    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        self.answer.clone()
    }
    async fn add_mark(
        &self,
        frame_id: Option<i64>,
        capture_now: bool,
        note: Option<String>,
    ) -> anyhow::Result<i64> {
        match (frame_id, capture_now) {
            (Some(id), false) => self.store.insert_mark(id, 1_000, note).await,
            // The test host has no capture worker, so `now` is honestly unavailable (503).
            (None, true) => anyhow::bail!("capture is off — mark not saved"),
            _ => anyhow::bail!("exactly one of frame_id or capture_now"),
        }
    }
    async fn resolve_mark(&self, mark_id: i64) -> anyhow::Result<()> {
        self.store.resolve_mark(mark_id, 2_000).await
    }
    async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
        Ok(None)
    }
    async fn export_frames_page(
        &self,
        after_id: i64,
        from: Option<i64>,
        to: Option<i64>,
        limit: u32,
    ) -> anyhow::Result<Vec<ExportFrameRow>> {
        self.store
            .export_frames_page(after_id, from, to, limit)
            .await
    }
    async fn report_page(
        &self,
        request: traits::ReportPageRequest,
    ) -> Result<traits::StoredReportPage, traits::HistoryReadError> {
        if let Some(page) = &self.report_page_override {
            return Ok(page.clone());
        }
        if let Some(fault) = self.history_fault {
            return Err(fault.error());
        }
        self.store.report_page(request).await
    }
    async fn report_detail(
        &self,
        run_id: &str,
    ) -> Result<Option<traits::ReportDetail>, traits::HistoryReadError> {
        if let Some(fault) = self.history_fault {
            return Err(fault.error());
        }
        self.store.report_detail(run_id).await
    }
}

/// Live manual-verification harness (ignored by default). Serves the real API over an
/// in-memory fixture store on port 43210 with a known test token. Never opens user
/// history. It stays up so an external `curl` can exercise every endpoint — a genuine
/// out-of-process live check without disturbing the running desktop app.
/// Run: `cargo test -p api --test http_api -- --ignored --nocapture live_server_for_curl`
#[tokio::test]
#[ignore]
async fn live_server_for_curl() {
    let token: SharedToken = Arc::new(std::sync::RwLock::new(TEST_TOKEN.to_string()));
    let host = fixture_host(false).await;
    let server = ApiServer::start(host, token, 43210)
        .await
        .expect("bind 43210");
    println!(
        "LIVE_API_READY addr={} token={TEST_TOKEN}",
        server.local_addr()
    );
    tokio::time::sleep(std::time::Duration::from_secs(90)).await;
    server.stop().await;
}

/// A frame with content text at the given capture time; returns its id.
async fn seed_frame(store: &SqliteStore, captured_at: i64, text: &str) -> i64 {
    let id = store
        .insert_frame(NewFrame {
            captured_at,
            monitor_index: 0,
            width: 1920,
            height: 1080,
            image_path: format!("frames/{captured_at}.webp"),
            content_hash: format!("hash-{captured_at}"),
            app_hint: Some("VS Code".to_string()),
            window_title: Some("main.rs".to_string()),
            browser_url: None,
            capture_trigger: None,
        })
        .await
        .expect("insert frame");
    store
        .insert_ocr(
            id,
            OcrResult {
                text: text.to_string(),
                mean_confidence: 0.9,
                engine: "winrt".to_string(),
                spans: vec![],
            },
        )
        .await
        .expect("insert ocr");
    id
}

async fn fixture_host(with_answer: bool) -> Arc<TestHost> {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open store"));
    seed_frame(&store, 1_000, "quarterly invoice total").await;
    seed_frame(&store, 2_000, "vacation beach photos").await;
    Arc::new(TestHost {
        store,
        answer: with_answer.then(|| Arc::new(ScriptedAnswer) as Arc<dyn AnswerProvider>),
        started_at_ms: 500,
        capture_kernel: None,
        history_fault: None,
        report_page_override: None,
    })
}

/// Starts a server over the fixture host on an ephemeral loopback port; returns the base
/// URL and the running server (kept alive by the caller).
async fn start_server(host: Arc<TestHost>) -> (String, ApiServer, SharedToken) {
    let token: SharedToken = Arc::new(std::sync::RwLock::new(TEST_TOKEN.to_string()));
    let server = ApiServer::start(host, token.clone(), 0)
        .await
        .expect("bind ephemeral loopback");
    let base = format!("http://127.0.0.1:{}", server.local_addr().port());
    (base, server, token)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn bearer(token: &str) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    h
}

// ---- P1 explicit permission boundary ----

#[tokio::test]
async fn read_token_is_live_and_cannot_invoke_capture_or_inference() {
    let (base, server, _legacy) = start_server(fixture_host(true).await).await;
    server.set_read_token(Some("read-only-token".to_string()));
    let c = client();
    let health = c
        .get(format!("{base}/v1/health"))
        .bearer_auth("read-only-token")
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), 200);
    for (path, body) in [
        ("/v1/marks", serde_json::json!({"frame_id": 1})),
        ("/v1/marks", serde_json::json!({"now": true})),
        ("/v1/marks/1/resolve", serde_json::json!({})),
        ("/v1/ask", serde_json::json!({"query": "invoice"})),
        (
            "/v1/ask",
            serde_json::json!({"query": "invoice", "session_id": 999}),
        ),
    ] {
        let r = c
            .post(format!("{base}{path}"))
            .bearer_auth("read-only-token")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 403, "{path}: {body}");
        assert_eq!(
            r.json::<serde_json::Value>().await.unwrap()["error"],
            "forbidden"
        );
    }
    server.stop().await;
}

// ---- Configurable binding retains loopback by default ----

#[tokio::test]
async fn default_binds_only_loopback() {
    let (_base, server, _t) = start_server(fixture_host(false).await).await;
    assert_eq!(
        server.local_addr().ip(),
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
    );
    server.stop().await;
}

#[tokio::test]
async fn loopback_client_still_connects() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let response = client()
        .get(format!("{base}/v1/health"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    server.stop().await;
}

#[tokio::test]
async fn all_routes_require_bearer() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    for (method, path) in [
        (reqwest::Method::GET, "/v1/health"),
        (reqwest::Method::GET, "/v1/search"),
        (reqwest::Method::GET, "/v1/frames/1"),
        (reqwest::Method::GET, "/v1/context/where-was-i"),
        (reqwest::Method::GET, "/v1/sessions"),
        (reqwest::Method::GET, "/v1/sessions/1"),
        (reqwest::Method::GET, "/v1/marks"),
        (reqwest::Method::POST, "/v1/marks"),
        (reqwest::Method::POST, "/v1/marks/1/resolve"),
        (reqwest::Method::POST, "/v1/ask"),
        (reqwest::Method::GET, "/v1/export"),
        (reqwest::Method::GET, "/v1/threads"),
        (reqwest::Method::GET, "/v1/threads/1"),
        (reqwest::Method::GET, "/v1/entities"),
        (reqwest::Method::GET, "/v1/insights"),
        (reqwest::Method::GET, "/v1/does-not-exist"),
    ] {
        for token in [None, Some("wrong-token")] {
            let mut request = client().request(method.clone(), format!("{base}{path}"));
            if let Some(token) = token {
                request = request.headers(bearer(token));
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), 401, "{method} {path}");
        }
    }
    let response = client()
        .get(format!("{base}/v1/does-not-exist"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    server.stop().await;
}

// ---- auth: 401 without / with a wrong token; health reports state ----

#[tokio::test]
async fn health_requires_token_and_reports_state() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;

    // No token → 401.
    let r = client()
        .get(format!("{base}/v1/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "unauthorized");

    // Wrong token → 401.
    let r = client()
        .get(format!("{base}/v1/health"))
        .headers(bearer("nope"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    // Right token → 200 + state.
    let r = client()
        .get(format!("{base}/v1/health"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["version"], "test-0.3.0");
    assert_eq!(body["capturing"], false);
    assert!(body["uptime_secs"].is_number());

    server.stop().await;
}

/// Regenerating the token (swapping the shared value) takes effect immediately — no
/// restart. Proves the middleware reads the live token per request.
#[tokio::test]
async fn token_swap_takes_effect_without_restart() {
    let (base, server, token) = start_server(fixture_host(false).await).await;

    // Old token works.
    let r = client()
        .get(format!("{base}/v1/health"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    // Rotate the token in place.
    let new_token = "rotated-000000000000000000000000000000000000000000000000000000000";
    *token.write().unwrap() = new_token.to_string();

    // Old token now 401, new token 200 — same running server.
    let r = client()
        .get(format!("{base}/v1/health"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let r = client()
        .get(format!("{base}/v1/health"))
        .headers(bearer(new_token))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    server.stop().await;
}

// ---- every read endpoint vs the fixture DB ----

#[tokio::test]
async fn search_returns_hits_from_fixture() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let r = client()
        .get(format!("{base}/v1/search?q=invoice&limit=10"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let hits: serde_json::Value = r.json().await.unwrap();
    assert_eq!(
        hits.as_array().unwrap().len(),
        1,
        "only the invoice frame matches"
    );

    // Missing `q` → 400 on the JSON error contract, not axum's plaintext rejection.
    let r = client()
        .get(format!("{base}/v1/search?limit=10"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    assert!(r
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("application/json"));
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"].is_string());

    server.stop().await;
}

#[tokio::test]
async fn frame_detail_image_and_not_found() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;

    // Detail for a known frame (id 1 = first seeded).
    let r = client()
        .get(format!("{base}/v1/frames/1"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let detail: serde_json::Value = r.json().await.unwrap();
    assert_eq!(detail["frame_id"], 1);

    // `?image=1` returns the synthetic WebP bytes with the right content type.
    let r = client()
        .get(format!("{base}/v1/frames/1?image=1"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers().get("content-type").unwrap().to_str().unwrap(),
        "image/webp"
    );
    assert_eq!(r.bytes().await.unwrap().as_ref(), b"webp-bytes");

    // Unknown frame → 404.
    let r = client()
        .get(format!("{base}/v1/frames/9999"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "not_found");

    // A non-integer id → 400 on the JSON error contract (path rejection mapped).
    let r = client()
        .get(format!("{base}/v1/frames/not-a-number"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "bad_request");

    server.stop().await;
}

/// An unknown path answers on the JSON error contract, not axum's default plaintext 404.
#[tokio::test]
async fn unknown_route_is_json_404() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let r = client()
        .get(format!("{base}/v1/does-not-exist"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "not_found");
    server.stop().await;
}

/// An inverted `from > to` window is rejected up front (400) rather than silently empty,
/// on both search and export.
#[tokio::test]
async fn inverted_time_range_is_400() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;

    let r = client()
        .get(format!("{base}/v1/search?q=x&from=3000&to=1000"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "bad_request");

    let r = client()
        .get(format!("{base}/v1/export?from=3000&to=1000"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);

    server.stop().await;
}

#[tokio::test]
async fn where_was_i_returns_null_when_nothing_qualifies() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let r = client()
        .get(format!("{base}/v1/context/where-was-i"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().await.unwrap(), "null");
    server.stop().await;
}

// ---- marks: the only write surface (create / list / resolve / validation) ----

#[tokio::test]
async fn marks_crud_roundtrip() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let c = client();

    // Create a mark on an existing frame → 201 with a mark_id.
    let r = c
        .post(format!("{base}/v1/marks"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({ "frame_id": 1, "note": "check this" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
    let created: serde_json::Value = r.json().await.unwrap();
    let mark_id = created["mark_id"].as_i64().unwrap();

    // List → the mark is present.
    let r = c
        .get(format!("{base}/v1/marks"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let marks: serde_json::Value = r.json().await.unwrap();
    assert_eq!(marks.as_array().unwrap().len(), 1);

    // `now:true` with no capture worker → 503 (honest, not a silent no-op).
    let r = c
        .post(format!("{base}/v1/marks"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({ "now": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);

    // Both frame_id and now → 400.
    let r = c
        .post(format!("{base}/v1/marks"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({ "frame_id": 1, "now": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);

    // Create on an unknown frame → 404.
    let r = c
        .post(format!("{base}/v1/marks"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({ "frame_id": 9999 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);

    // Resolve the real mark → 200.
    let r = c
        .post(format!("{base}/v1/marks/{mark_id}/resolve"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    // Resolve an unknown mark → 404.
    let r = c
        .post(format!("{base}/v1/marks/9999/resolve"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);

    server.stop().await;
}

// ---- SSE ask ----

#[tokio::test]
async fn ask_streams_sse_deltas() {
    let (base, server, _t) = start_server(fixture_host(true).await).await;
    let r = client()
        .post(format!("{base}/v1/ask"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({ "query": "invoice" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(r
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let body = r.text().await.unwrap();
    assert!(
        body.contains("grounded answer"),
        "token delta streamed: {body}"
    );
    assert!(
        body.contains("\"type\":\"done\""),
        "terminal Done streamed: {body}"
    );
    server.stop().await;
}

#[tokio::test]
async fn ask_without_answer_model_is_503() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let r = client()
        .post(format!("{base}/v1/ask"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({ "query": "invoice" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
    server.stop().await;
}

// ---- export over HTTP + the shared file path ----

#[tokio::test]
async fn export_over_http_is_valid_json() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let r = client()
        .get(format!("{base}/v1/export?format=json"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let doc: serde_json::Value = r.json().await.unwrap();
    assert_eq!(doc["schema"], "screensearch.export.v1");
    assert_eq!(doc["frames"].as_array().unwrap().len(), 2);
    assert!(doc["marks"].is_array());

    // A non-JSON format is 400.
    let r = client()
        .get(format!("{base}/v1/export?format=csv"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);

    server.stop().await;
}

#[tokio::test]
async fn export_window_excludes_out_of_range_frames() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    // [1500, 3000) includes only the 2000 frame.
    let r = client()
        .get(format!("{base}/v1/export?from=1500&to=3000"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap();
    let doc: serde_json::Value = r.json().await.unwrap();
    let frames = doc["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["captured_at"], 2000);
    server.stop().await;
}

/// The Settings "Export…" path: streams the same document to a file **with no server** —
/// export works with the API disabled (`03 §7c`, DoD `§13b`).
#[tokio::test]
async fn export_to_file_writes_valid_json_without_a_server() {
    let host = fixture_host(false).await;
    let dir = tempfile::tempdir().unwrap();
    let result = api::export::export_to_file(host, None, None, dir.path())
        .await
        .expect("export to file");

    assert_eq!(result.frames, 2);
    let contents = std::fs::read_to_string(&result.path).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(doc["schema"], "screensearch.export.v1");
    assert_eq!(doc["frames"].as_array().unwrap().len(), 2);

    // No leftover `.partial` file.
    let partials: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".partial"))
        .collect();
    assert!(partials.is_empty(), "no leftover .partial after success");
}

// ============================================================================================
// 0.4.0 PR6: sessions API surface (`GET /v1/sessions`, `GET /v1/sessions/{id}`, D12).
// ============================================================================================

/// A `NewSession` with the arc defaults filled in.
fn new_session(
    started_at: i64,
    ended_at: Option<i64>,
    kind: SessionKind,
    tool: Option<&str>,
    host: Option<SessionHost>,
    key: &str,
) -> NewSession {
    NewSession {
        started_at,
        ended_at,
        kind,
        tool: tool.map(|t| t.to_string()),
        host,
        context_key: key.to_string(),
        confidence: 0.9,
        frozen: false,
    }
}

/// A fixture with three sessions for the PR6 endpoints. Session ids are deterministic by
/// insertion order: **1 = A** (ai/claude-code, closed `[1000,5000]`, cached title+summary, two
/// frames + user/agent exchanges + one `note`), **2 = B** (ai/codex, closed `[3000,9000]` which
/// overlaps A in wall-clock time, one frame, no cached summary), **3 = C** (focus, open, no
/// frames). A and B frames share the term "deployment pipeline" so scoped-ask tests can prove the
/// filter bites.
async fn sessions_fixture_host(with_answer: bool) -> Arc<TestHost> {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open store"));

    let a = store
        .insert_session(new_session(
            1_000,
            Some(5_000),
            SessionKind::Ai,
            Some("claude-code"),
            Some(SessionHost::Terminal),
            "ai:claude-code",
        ))
        .await
        .unwrap();
    let a1 = seed_frame(&store, 1_500, "deployment pipeline config change").await;
    let a2 = seed_frame(&store, 2_500, "deployment pipeline rollout notes").await;
    store
        .assign_frames_session(&[a1, a2], Some(a))
        .await
        .unwrap();
    store
        .insert_session_artifacts(
            a,
            &[
                NewSessionArtifact {
                    kind: SessionArtifactKind::Exchange,
                    role: Some(SessionArtifactRole::User),
                    frame_id: Some(a1),
                    content: "fix the deploy".to_string(),
                },
                NewSessionArtifact {
                    kind: SessionArtifactKind::Exchange,
                    role: Some(SessionArtifactRole::Agent),
                    frame_id: Some(a2),
                    content: "done, pipeline updated".to_string(),
                },
                NewSessionArtifact {
                    kind: SessionArtifactKind::Note,
                    role: None,
                    frame_id: None,
                    content: "a private note".to_string(),
                },
            ],
        )
        .await
        .unwrap();
    store
        .set_session_title_summary(
            a,
            "Deploy pipeline fix",
            "Reworked the deployment pipeline.",
            "test-model",
        )
        .await
        .unwrap();

    let b = store
        .insert_session(new_session(
            3_000,
            Some(9_000),
            SessionKind::Ai,
            Some("codex"),
            Some(SessionHost::Terminal),
            "ai:codex",
        ))
        .await
        .unwrap();
    let b1 = seed_frame(&store, 4_000, "deployment pipeline audit in codex").await;
    store.assign_frames_session(&[b1], Some(b)).await.unwrap();

    // C: focus, open (ended_at = None), no frames, no cached summary.
    store
        .insert_session(new_session(
            10_000,
            None,
            SessionKind::Focus,
            None,
            None,
            "focus:vscode",
        ))
        .await
        .unwrap();

    Arc::new(TestHost {
        store,
        answer: with_answer.then(|| Arc::new(ScriptedAnswer) as Arc<dyn AnswerProvider>),
        started_at_ms: 500,
        capture_kernel: None,
        history_fault: None,
        report_page_override: None,
    })
}

/// Authenticated GET against the running server.
async fn get_sessions(base: &str, path: &str) -> reqwest::Response {
    client()
        .get(format!("{base}{path}"))
        .headers(bearer(TEST_TOKEN))
        .send()
        .await
        .unwrap()
}

/// The `id` field of every element of a JSON array response, in order.
fn ids_of(v: &serde_json::Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn sessions_list_orders_newest_first_and_hides_summary() {
    let (base, server, _t) = start_server(sessions_fixture_host(false).await).await;
    let r = get_sessions(&base, "/v1/sessions").await;
    assert_eq!(r.status(), 200);
    let rows: serde_json::Value = r.json().await.unwrap();
    // started_at DESC, id DESC: C (10000), B (3000), A (1000).
    assert_eq!(ids_of(&rows), vec![3, 2, 1]);
    let arr = rows.as_array().unwrap();
    // The `open` marker is present on every row, true only for the open session C.
    assert_eq!(arr[0]["open"], true);
    assert_eq!(arr[1]["open"], false);
    assert_eq!(arr[2]["open"], false);
    // kind/tool serialize as the lowercase taxonomy strings.
    assert_eq!(arr[0]["kind"], "focus");
    assert_eq!(arr[2]["kind"], "ai");
    assert_eq!(arr[2]["tool"], "claude-code");
    // Title is always present; the cached summary is hidden on the list surface (null, not
    // absent: the field is always there).
    assert_eq!(arr[2]["title"], "Deploy pipeline fix");
    assert!(
        arr[2]["summary"].is_null(),
        "list never includes the summary"
    );
    assert!(arr[2]["summary_model"].is_null());
    server.stop().await;
}

#[tokio::test]
async fn sessions_list_overlap_predicate_and_open_session() {
    let (base, server, _t) = start_server(sessions_fixture_host(false).await).await;

    // Window straddling A and B; C starts at 10000 so it is excluded. Ordered started_at DESC.
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?from=2000&to=3500")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&arr), vec![2, 1]);

    // Window before everything → empty.
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?from=0&to=500")
        .await
        .json()
        .await
        .unwrap();
    assert!(arr.as_array().unwrap().is_empty());

    // from-only past C start: C is open, so COALESCE(ended_at, now) uses request-time now, which
    // is far greater than any fixture timestamp, keeping C visible; the closed A/B fall out.
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?from=10500")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&arr), vec![3]);

    // to-only before B/C start → only A (started 1000 < 1500).
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?to=1500")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&arr), vec![1]);

    server.stop().await;
}

#[tokio::test]
async fn sessions_list_filters_validate_and_clamp() {
    let (base, server, _t) = start_server(sessions_fixture_host(false).await).await;

    // Unknown kind → 400 naming the valid values.
    let r = get_sessions(&base, "/v1/sessions?kind=bogus").await;
    assert_eq!(r.status(), 400);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"].as_str().unwrap().contains("focus"));

    // kind=ai → A and B.
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?kind=ai")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&arr), vec![2, 1]);

    // tool filter.
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?tool=codex")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&arr), vec![2]);

    // An empty/whitespace tool is "no filter", not "match the empty-string tool" — it must
    // return every session, identical to omitting the parameter.
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?tool=")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        ids_of(&arr),
        vec![3, 2, 1],
        "empty tool filter returns all sessions"
    );

    // from > to → 400.
    let r = get_sessions(&base, "/v1/sessions?from=5000&to=1000").await;
    assert_eq!(r.status(), 400);

    // limit clamps to at least 1 row (the newest, C).
    let arr: serde_json::Value = get_sessions(&base, "/v1/sessions?limit=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&arr), vec![3]);

    server.stop().await;
}

#[tokio::test]
async fn sessions_endpoints_require_token() {
    let (base, server, _t) = start_server(sessions_fixture_host(false).await).await;
    let r = client()
        .get(format!("{base}/v1/sessions"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let r = client()
        .get(format!("{base}/v1/sessions/1"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    server.stop().await;
}

#[tokio::test]
async fn session_detail_returns_exchanges_only_and_404s_unknown() {
    let (base, server, _t) = start_server(sessions_fixture_host(false).await).await;

    let r = get_sessions(&base, "/v1/sessions/1").await;
    assert_eq!(r.status(), 200);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["session"]["id"], 1);
    assert_eq!(body["session"]["open"], false);
    let ex = body["exchanges"].as_array().unwrap();
    assert_eq!(
        ex.len(),
        2,
        "only exchange artifacts; the note is filtered out"
    );
    let roles: Vec<&str> = ex.iter().map(|e| e["role"].as_str().unwrap()).collect();
    assert!(roles.contains(&"user") && roles.contains(&"agent"));
    // Detail hides the summary unless asked.
    assert!(body["session"]["summary"].is_null());

    // Unknown id → 404 not_found.
    let r = get_sessions(&base, "/v1/sessions/9999").await;
    assert_eq!(r.status(), 404);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "not_found");

    // Non-integer path → 400 via ApiPath (JSON error contract, not axum plaintext).
    let r = get_sessions(&base, "/v1/sessions/abc").await;
    assert_eq!(r.status(), 400);

    server.stop().await;
}

#[tokio::test]
async fn session_detail_reveals_cached_summary_only_when_asked() {
    let (base, server, _t) = start_server(sessions_fixture_host(false).await).await;

    // A (id 1) has a cached summary, hidden without the flag.
    let body: serde_json::Value = get_sessions(&base, "/v1/sessions/1")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["session"]["summary"].is_null());

    // With the flag, the cached value + model are revealed.
    let body: serde_json::Value = get_sessions(&base, "/v1/sessions/1?include_summary=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["session"]["summary"],
        "Reworked the deployment pipeline."
    );
    assert_eq!(body["session"]["summary_model"], "test-model");

    server.stop().await;
}

/// D12: `include_summary=1` on an uncached session, even with an answer model available, returns
/// a null summary and never writes the row: a GET must not start inference.
#[tokio::test]
async fn session_detail_include_summary_never_generates() {
    let host = sessions_fixture_host(true).await; // answer-capable
    let store = host.store();
    let (base, server, _t) = start_server(host).await;

    // B (id 2) has no cached summary.
    let body: serde_json::Value = get_sessions(&base, "/v1/sessions/2?include_summary=1")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["session"]["summary"].is_null(),
        "the API never generates a summary (D12)"
    );

    // The row is unchanged in the store: still no summary/model.
    let s = store.get_session(2).await.unwrap().unwrap();
    assert!(
        s.summary.is_none() && s.summary_model.is_none(),
        "a GET must not write sessions.summary/summary_model (D12)"
    );

    server.stop().await;
}

// ---- P8 connection & insight reads (`03 §7c` P8 block, `§7h.4`, `§7j`) ----

/// The read credential used by every P8 case: `history.read` and nothing else.
const P8_READ_TOKEN: &str = "p8-read-credential";

const DEPLOY_CLUSTER: &str = "cluster-deploy";
const FOCUS_CLUSTER: &str = "cluster-focus";
const REPO_ENTITY_KEY: &str = "repo:screensearch";
const DOMAIN_ENTITY_KEY: &str = "domain:example.com";
/// The repo entity's seeded alias — a string that appears in no canonical name, so a query
/// matching it can only have matched through the alias table.
const REPO_ALIAS: &str = "ssv2c-repo";

/// Seeded thread-member confidences (inputs to the fixture, referenced by the assertions).
const DEPLOY_CONF_A: f64 = 0.9;
const DEPLOY_CONF_B: f64 = 0.8;
const FOCUS_CONF_C: f64 = 0.95;
const FOCUS_CONF_D: f64 = 0.7;
/// Seeded session↔entity confidences.
const REPO_CONF_A: f64 = 0.9;
const REPO_CONF_B: f64 = 0.75;
const DOMAIN_CONF_C: f64 = 0.7;
const DOMAIN_CONF_D: f64 = 0.6;

/// Everything the P8 fixture seeds. Row ids are **read back** through independent store
/// reads (threads by `cluster_key`, entities by `normalized_key`), so assertions name real
/// rows without assuming an allocation order; `insights` keeps the exact [`NewInsight`]
/// values handed to the store, which is what `/v1/insights` must project back.
struct GraphFixture {
    thread_deploy: i64,
    thread_focus: i64,
    session_a: i64,
    session_b: i64,
    session_c: i64,
    session_d: i64,
    entity_repo: i64,
    entity_domain: i64,
    insights: Vec<NewInsight>,
}

impl GraphFixture {
    /// The kind tokens `/v1/insights` must return them in: `window_start DESC, kind ASC`
    /// (`id DESC` only breaks ties this fixture does not create).
    fn insight_order(&self) -> Vec<String> {
        let mut order: Vec<&NewInsight> = self.insights.iter().collect();
        order.sort_by(|a, b| {
            b.window_start
                .cmp(&a.window_start)
                .then_with(|| a.payload.kind().as_str().cmp(b.payload.kind().as_str()))
        });
        order
            .iter()
            .map(|insight| insight.payload.kind().as_str().to_string())
            .collect()
    }

    fn insight(&self, kind: &str) -> &NewInsight {
        self.insights
            .iter()
            .find(|insight| insight.payload.kind().as_str() == kind)
            .expect("the fixture seeds one insight per kind")
    }
}

/// Two threads, four sessions, two entities (one aliased), one unresolved mark and one
/// insight per kind with real evidence — the data the four P8 read routes project.
///
/// deploy = sessions A `[1000,5000]` + B `[3000,9000]`; focus = C `[10000,12000]` +
/// D `[20000,22000]`. `repo:screensearch` (alias `ssv2c-repo`) is touched by A and B,
/// `domain:example.com` by C and D, so both entities report `session_count = 2` and the
/// entity/thread orders have two distinguishable rows each.
async fn seed_graph(store: &SqliteStore) -> GraphFixture {
    let session_a = store
        .insert_session(new_session(
            1_000,
            Some(5_000),
            SessionKind::Ai,
            Some("claude-code"),
            Some(SessionHost::Terminal),
            "ai:claude-code",
        ))
        .await
        .unwrap();
    let session_b = store
        .insert_session(new_session(
            3_000,
            Some(9_000),
            SessionKind::Ai,
            Some("codex"),
            Some(SessionHost::Terminal),
            "ai:codex",
        ))
        .await
        .unwrap();
    let session_c = store
        .insert_session(new_session(
            10_000,
            Some(12_000),
            SessionKind::Focus,
            None,
            Some(SessionHost::Desktop),
            "focus:vscode",
        ))
        .await
        .unwrap();
    let session_d = store
        .insert_session(new_session(
            20_000,
            Some(22_000),
            SessionKind::Focus,
            None,
            Some(SessionHost::Desktop),
            "focus:vscode",
        ))
        .await
        .unwrap();

    // The mark's frame belongs to session A, so `open_loop` evidence names real rows.
    let frame = seed_frame(store, 1_500, "deploy pipeline config change").await;
    store
        .assign_frames_session(&[frame], Some(session_a))
        .await
        .unwrap();
    let mark = store
        .insert_mark(frame, 4_000, Some("check the deploy".to_string()))
        .await
        .unwrap();

    store
        .graph_apply_thread_plan(&ThreadPlan {
            threads: vec![
                ThreadDraft {
                    cluster_key: DEPLOY_CLUSTER.to_string(),
                    title: "Deploy pipeline".to_string(),
                    first_seen_at: 1_000,
                    last_seen_at: 9_000,
                    sessions: vec![
                        ThreadMemberDraft {
                            session_id: session_a,
                            confidence: DEPLOY_CONF_A,
                        },
                        ThreadMemberDraft {
                            session_id: session_b,
                            confidence: DEPLOY_CONF_B,
                        },
                    ],
                },
                ThreadDraft {
                    cluster_key: FOCUS_CLUSTER.to_string(),
                    title: "Focus block".to_string(),
                    first_seen_at: 10_000,
                    last_seen_at: 22_000,
                    sessions: vec![
                        ThreadMemberDraft {
                            session_id: session_c,
                            confidence: FOCUS_CONF_C,
                        },
                        ThreadMemberDraft {
                            session_id: session_d,
                            confidence: FOCUS_CONF_D,
                        },
                    ],
                },
            ],
            ..ThreadPlan::default()
        })
        .await
        .unwrap();

    for (session, confidence, hit_count, first_seen_at, last_seen_at, aliases) in [
        (
            session_a,
            REPO_CONF_A,
            2,
            1_000,
            5_000,
            vec![REPO_ALIAS.to_string()],
        ),
        (session_b, REPO_CONF_B, 1, 3_000, 9_000, Vec::new()),
    ] {
        store
            .graph_replace_session_links(
                session,
                &[GraphSessionLinkAggregate {
                    kind: EntityKind::Repo,
                    canonical_name: "screensearch".to_string(),
                    normalized_key: REPO_ENTITY_KEY.to_string(),
                    confidence,
                    hit_count,
                    first_seen_at,
                    last_seen_at,
                    aliases,
                }],
            )
            .await
            .unwrap();
    }
    for (session, confidence, first_seen_at, last_seen_at) in [
        (session_c, DOMAIN_CONF_C, 10_000, 12_000),
        (session_d, DOMAIN_CONF_D, 20_000, 22_000),
    ] {
        store
            .graph_replace_session_links(
                session,
                &[GraphSessionLinkAggregate {
                    kind: EntityKind::Domain,
                    canonical_name: "example.com".to_string(),
                    normalized_key: DOMAIN_ENTITY_KEY.to_string(),
                    confidence,
                    hit_count: 1,
                    first_seen_at,
                    last_seen_at,
                    aliases: Vec::new(),
                }],
            )
            .await
            .unwrap();
    }

    let thread_deploy = store
        .graph_stored_threads()
        .await
        .unwrap()
        .into_iter()
        .find(|thread| thread.cluster_key == DEPLOY_CLUSTER)
        .expect("deploy thread stored")
        .id;
    let thread_focus = store
        .graph_stored_threads()
        .await
        .unwrap()
        .into_iter()
        .find(|thread| thread.cluster_key == FOCUS_CLUSTER)
        .expect("focus thread stored")
        .id;
    let catalog = store.graph_entity_catalog(50).await.unwrap();
    let entity_repo = catalog
        .iter()
        .find(|entity| entity.normalized_key == REPO_ENTITY_KEY)
        .expect("repo entity stored")
        .id;
    let entity_domain = catalog
        .iter()
        .find(|entity| entity.normalized_key == DOMAIN_ENTITY_KEY)
        .expect("domain entity stored")
        .id;

    // One insight per kind. Windows are chosen so each payload is honest for its own
    // window, and every evidence list is dense from ordinal 0 (`03 §7j.2`, which the store
    // validates): the response must project that list back ordinal-ascending.
    let insights = vec![
        NewInsight {
            window_start: 1_000,
            window_end: 9_500,
            payload: InsightPayload::ThreadActivity(ThreadActivityPayload {
                thread_id: thread_deploy,
                title: "Deploy pipeline".to_string(),
                session_count: 2,
                active_ms: (5_000 - 1_000) + (9_000 - 3_000),
                first_touched_at: 1_000,
                last_touched_at: 9_000,
            }),
            generator: InsightGenerator::Heuristic,
            evidence: vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Thread,
                    ref_id: thread_deploy,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: session_a,
                },
                InsightEvidence {
                    ordinal: 2,
                    kind: EvidenceKind::Session,
                    ref_id: session_b,
                },
            ],
        },
        NewInsight {
            window_start: 1_000,
            window_end: 9_500,
            payload: InsightPayload::OpenLoop(OpenLoopPayload {
                source: OpenLoopSource::UnresolvedMark,
                mark_id: Some(mark),
                frame_id: Some(frame),
                note: Some("check the deploy".to_string()),
                thread_id: None,
                title: None,
                opened_at: 4_000,
                idle_ms: 9_500 - 4_000,
            }),
            generator: InsightGenerator::Heuristic,
            evidence: vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Mark,
                    ref_id: mark,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Frame,
                    ref_id: frame,
                },
            ],
        },
        NewInsight {
            window_start: 3_000,
            window_end: 9_500,
            payload: InsightPayload::Recurrence(RecurrencePayload {
                entity_id: entity_repo,
                entity_kind: EntityKind::Repo.as_str().to_string(),
                entity_name: "screensearch".to_string(),
                first_session_id: session_a,
                first_seen_at: 1_000,
                second_session_id: session_b,
                second_seen_at: 3_000,
                gap_ms: 3_000 - 1_000,
                occurrence_count: 2,
            }),
            generator: InsightGenerator::Heuristic,
            evidence: vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Entity,
                    ref_id: entity_repo,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: session_a,
                },
                InsightEvidence {
                    ordinal: 2,
                    kind: EvidenceKind::Session,
                    ref_id: session_b,
                },
            ],
        },
        NewInsight {
            window_start: 3_000,
            window_end: 9_500,
            payload: InsightPayload::SemanticNeighbour(SemanticNeighbourPayload {
                session_id: session_b,
                session_title: None,
                session_started_at: 3_000,
                neighbours: vec![SessionNeighbour {
                    session_id: session_c,
                    title: None,
                    started_at: 10_000,
                    similarity: 0.91,
                }],
            }),
            generator: InsightGenerator::Heuristic,
            evidence: vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Session,
                    ref_id: session_b,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: session_c,
                },
            ],
        },
        NewInsight {
            window_start: 10_000,
            window_end: 22_500,
            payload: InsightPayload::Resumption(ResumptionPayload {
                thread_id: thread_focus,
                title: "Focus block".to_string(),
                previous_session_id: session_c,
                previous_ended_at: 12_000,
                resumed_session_id: session_d,
                resumed_at: 20_000,
                gap_ms: 20_000 - 12_000,
                last_doing: None,
            }),
            generator: InsightGenerator::Heuristic,
            evidence: vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Thread,
                    ref_id: thread_focus,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: session_c,
                },
                InsightEvidence {
                    ordinal: 2,
                    kind: EvidenceKind::Session,
                    ref_id: session_d,
                },
            ],
        },
    ];
    for insight in &insights {
        store.insert_insight(insight.clone()).await.unwrap();
    }

    GraphFixture {
        thread_deploy,
        thread_focus,
        session_a,
        session_b,
        session_c,
        session_d,
        entity_repo,
        entity_domain,
        insights,
    }
}

/// The P8 fixture host over an already-seeded store (no answer provider: these routes never
/// infer).
fn graph_host(store: Arc<SqliteStore>) -> Arc<TestHost> {
    Arc::new(TestHost {
        store,
        answer: None,
        started_at_ms: 500,
        capture_kernel: None,
        history_fault: None,
        report_page_override: None,
    })
}

/// An in-memory P8 fixture plus its running server with the read credential installed.
async fn start_graph_server() -> (String, ApiServer, Arc<SqliteStore>, GraphFixture) {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open store"));
    let fixture = seed_graph(&store).await;
    let (base, server, _t) = start_server(graph_host(store.clone())).await;
    server.set_read_token(Some(P8_READ_TOKEN.to_string()));
    (base, server, store, fixture)
}

/// Authenticated GET with the read credential.
async fn get_read(base: &str, path: &str) -> reqwest::Response {
    client()
        .get(format!("{base}{path}"))
        .bearer_auth(P8_READ_TOKEN)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn threads_list_is_ordered_and_window_filtered() {
    let (base, server, _store, fixture) = start_graph_server().await;

    let body: serde_json::Value = get_read(&base, "/v1/threads").await.json().await.unwrap();
    assert_eq!(
        ids_of(&body),
        vec![fixture.thread_focus, fixture.thread_deploy],
        "last_seen_at DESC, id DESC"
    );
    assert_eq!(body[0]["title"], "Focus block");
    assert_eq!(body[0]["first_seen_at"], 10_000);
    assert_eq!(body[0]["last_seen_at"], 22_000);
    assert_eq!(body[0]["session_count"], 2);
    assert_eq!(body[1]["title"], "Deploy pipeline");
    assert_eq!(body[1]["first_seen_at"], 1_000);
    assert_eq!(body[1]["last_seen_at"], 9_000);
    assert_eq!(body[1]["session_count"], 2);

    // Overlap predicate `first_seen_at < to AND last_seen_at >= from`: `to=10_000` keeps
    // deploy (last_seen 9_000) and drops focus, whose first_seen is exactly 10_000.
    let body: serde_json::Value = get_read(&base, "/v1/threads?to=10000")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&body), vec![fixture.thread_deploy]);
    // A single lower bound keeps only threads still live at/after it.
    let body: serde_json::Value = get_read(&base, "/v1/threads?from=10000")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&body), vec![fixture.thread_focus]);
    // A window no thread overlaps is an empty array, never an error.
    let body: serde_json::Value = get_read(&base, "/v1/threads?from=9001&to=9999")
        .await
        .json()
        .await
        .unwrap();
    assert!(body.as_array().unwrap().is_empty());

    server.stop().await;
}

#[tokio::test]
async fn thread_detail_returns_ordered_sessions_and_entities() {
    let (base, server, _store, fixture) = start_graph_server().await;

    let body: serde_json::Value =
        get_read(&base, &format!("/v1/threads/{}", fixture.thread_deploy))
            .await
            .json()
            .await
            .unwrap();
    assert_eq!(body["thread"]["id"], fixture.thread_deploy);
    assert_eq!(body["thread"]["title"], "Deploy pipeline");
    assert_eq!(body["thread"]["session_count"], 2);
    let sessions = body["sessions"].as_array().unwrap();
    assert_eq!(
        sessions
            .iter()
            .map(|s| s["session_id"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![fixture.session_a, fixture.session_b],
        "member sessions ascend by started_at"
    );
    assert_eq!(sessions[0]["started_at"], 1_000);
    assert_eq!(sessions[1]["started_at"], 3_000);
    assert_eq!(sessions[0]["ended_at"], 5_000);
    assert_eq!(sessions[0]["kind"], "ai");
    assert!(sessions[0]["title"].is_null());
    assert!((sessions[0]["confidence"].as_f64().unwrap() - DEPLOY_CONF_A).abs() < 1e-9);
    assert!((sessions[1]["confidence"].as_f64().unwrap() - DEPLOY_CONF_B).abs() < 1e-9);

    let entities = body["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["entity_id"], fixture.entity_repo);
    assert_eq!(entities[0]["kind"], "repo");
    assert_eq!(entities[0]["canonical_name"], "screensearch");
    assert_eq!(entities[0]["session_count"], 2);
    assert!((entities[0]["confidence"].as_f64().unwrap() - REPO_CONF_A).abs() < 1e-9);

    // The other thread projects its own members and entity.
    let body: serde_json::Value = get_read(&base, &format!("/v1/threads/{}", fixture.thread_focus))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["session_id"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![fixture.session_c, fixture.session_d]
    );
    let entities = body["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["entity_id"], fixture.entity_domain);
    assert_eq!(entities[0]["kind"], "domain");
    assert_eq!(entities[0]["session_count"], 2);
    assert!((entities[0]["confidence"].as_f64().unwrap() - DOMAIN_CONF_C).abs() < 1e-9);

    server.stop().await;
}

#[tokio::test]
async fn entities_list_filters_by_kind_and_alias_substring() {
    let (base, server, _store, fixture) = start_graph_server().await;

    let body: serde_json::Value = get_read(&base, "/v1/entities").await.json().await.unwrap();
    assert_eq!(
        ids_of(&body),
        vec![fixture.entity_domain, fixture.entity_repo],
        "last_seen_at DESC, id DESC"
    );
    assert_eq!(body[0]["kind"], "domain");
    assert_eq!(body[0]["canonical_name"], "example.com");
    assert_eq!(body[0]["first_seen_at"], 10_000);
    assert_eq!(body[0]["last_seen_at"], 22_000);
    assert_eq!(body[0]["session_count"], 2);
    assert_eq!(body[0]["aliases"], serde_json::json!([]));
    assert_eq!(body[1]["kind"], "repo");
    assert_eq!(body[1]["first_seen_at"], 1_000);
    assert_eq!(body[1]["last_seen_at"], 9_000);
    assert_eq!(body[1]["session_count"], 2);
    assert_eq!(
        body[1]["aliases"],
        serde_json::json!([REPO_ALIAS]),
        "the seeded alias is carried"
    );

    let body: serde_json::Value = get_read(&base, "/v1/entities?kind=repo")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&body), vec![fixture.entity_repo]);
    // `q` is a case-insensitive substring over the canonical name…
    let body: serde_json::Value = get_read(&base, "/v1/entities?q=SCREEN")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&body), vec![fixture.entity_repo]);
    // …and over any alias: the alias is the only place this substring exists, and the
    // match is ASCII-case-insensitive.
    let body: serde_json::Value = get_read(&base, "/v1/entities?q=SSV2C")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&body), vec![fixture.entity_repo]);
    let body: serde_json::Value = get_read(&base, "/v1/entities?kind=domain&q=screensearch")
        .await
        .json()
        .await
        .unwrap();
    assert!(body.as_array().unwrap().is_empty());
    let body: serde_json::Value = get_read(&base, "/v1/entities?q=example")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&body), vec![fixture.entity_domain]);

    server.stop().await;
}

#[tokio::test]
async fn insights_list_carries_typed_payloads_and_evidence() {
    let (base, server, _store, fixture) = start_graph_server().await;

    let body: serde_json::Value = get_read(&base, "/v1/insights").await.json().await.unwrap();
    let items = body.as_array().unwrap();
    assert_eq!(items.len(), fixture.insights.len());
    assert_eq!(
        items
            .iter()
            .map(|insight| insight["kind"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        fixture.insight_order(),
        "window_start DESC, kind ASC, id DESC"
    );

    for item in items {
        let kind = item["kind"].as_str().unwrap();
        let seeded = fixture.insight(kind);
        assert_eq!(item["window_start"], seeded.window_start);
        assert_eq!(item["window_end"], seeded.window_end);
        assert_eq!(item["generator"], "heuristic");
        assert_eq!(item["stale"], false);
        assert!(
            item["generated_at"].as_i64().unwrap() > 0,
            "the store assigns generated_at"
        );
        // The payload is the exact typed value handed to the store, tagged by kind.
        assert_eq!(
            item["payload"],
            serde_json::to_value(&seeded.payload).unwrap(),
            "{kind} payload round-trips"
        );
        // Evidence comes back per insight, ordinal-ascending starting at 0 and identical to
        // what the store was given — the row is never served with evidence stripped.
        let mut expected: Vec<serde_json::Value> = seeded
            .evidence
            .iter()
            .map(|evidence| {
                serde_json::json!({
                    "ordinal": evidence.ordinal,
                    "kind": evidence.kind.as_str(),
                    "ref_id": evidence.ref_id,
                })
            })
            .collect();
        expected.sort_by_key(|evidence| evidence["ordinal"].as_u64().unwrap());
        assert_eq!(
            item["evidence"],
            serde_json::Value::Array(expected),
            "{kind}"
        );
    }

    // `kind` narrows to the single row of that kind.
    let body: serde_json::Value = get_read(&base, "/v1/insights?kind=recurrence")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["kind"], "recurrence");

    // Window overlap `window_start < to AND window_end > from`: `to=9000` drops only the
    // resumption insight, which starts at 10_000.
    let body: serde_json::Value = get_read(&base, "/v1/insights?from=0&to=9000")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body.as_array()
            .unwrap()
            .iter()
            .map(|insight| insight["kind"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        vec![
            "recurrence",
            "semantic_neighbour",
            "open_loop",
            "thread_activity"
        ]
    );
    // A lower bound only keeps insights still in range at/after it.
    let body: serde_json::Value = get_read(&base, "/v1/insights?from=19000")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["kind"], "resumption");

    server.stop().await;
}

#[tokio::test]
async fn p8_reads_validate_queries_and_missing_threads() {
    let (base, server, _store, fixture) = start_graph_server().await;

    for path in [
        "/v1/threads?bogus=1",
        "/v1/threads?from=abc",
        "/v1/threads?from=1.5",
        "/v1/threads?from=10&to=5",
        "/v1/threads?from=0&from=1",
        "/v1/threads?limit=one",
        "/v1/threads?limit=-1",
        "/v1/entities?kind=bogus",
        "/v1/entities?unknown=",
        "/v1/entities?limit=1.0",
        "/v1/entities?q=%FF",
        "/v1/insights?kind=bogus",
        "/v1/insights?from=10&to=5",
        "/v1/insights?limit=+1",
        "/v1/insights?kind=open_loop&kind=open_loop",
        "/v1/insights?q=deploy",
    ] {
        let r = get_read(&base, path).await;
        assert_eq!(r.status(), 400, "{path}");
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["error"], "bad_request", "{path}");
    }

    // A non-integer path segment is a strict 400, not a 404.
    let r = get_read(&base, "/v1/threads/not-an-id").await;
    assert_eq!(r.status(), 400);
    assert_eq!(
        r.json::<serde_json::Value>().await.unwrap()["error"],
        "bad_request"
    );
    // An absent thread is a typed 404.
    let r = get_read(&base, "/v1/threads/999999").await;
    assert_eq!(r.status(), 404);
    assert_eq!(
        r.json::<serde_json::Value>().await.unwrap()["error"],
        "not_found"
    );

    // `limit` clamps rather than rejecting: 0 → the newest row only, an oversized value →
    // the whole fixture, and the default page already covers it.
    let body: serde_json::Value = get_read(&base, "/v1/insights?limit=0")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["kind"], fixture.insight_order()[0].as_str());
    let body: serde_json::Value = get_read(&base, "/v1/insights?limit=5000")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body.as_array().unwrap().len(), fixture.insights.len());
    let body: serde_json::Value = get_read(&base, "/v1/threads?limit=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ids_of(&body), vec![fixture.thread_focus]);

    server.stop().await;
}

/// A successful read of all four P8 routes must leave `insights` byte-identical in row count
/// and newest `generated_at` — a GET can never generate or replace an insight (`§7j.5`).
#[tokio::test]
async fn reads_never_write_insights() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph-reads.db");
    let store = Arc::new(SqliteStore::open_path(&db).expect("open fixture store"));
    let fixture = seed_graph(&store).await;

    // Fixture-only direct SQL against the disposable file database (the same labeled class
    // as `examples/memory_consumer_fixture.rs`), because the assertion is about rows the
    // store's own read API cannot report: the API crate itself never does this.
    let insight_state = |db: &std::path::Path| -> (i64, Option<i64>) {
        let conn = rusqlite::Connection::open(db).expect("open the fixture db for a read");
        conn.query_row(
            "SELECT COUNT(*), MAX(generated_at) FROM insights",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read the insight state")
    };
    let before = insight_state(&db);
    assert_eq!(
        before.0,
        i64::try_from(fixture.insights.len()).unwrap(),
        "the fixture seeds one insight per kind"
    );
    assert!(before.1.is_some(), "seeded insights carry a generated_at");

    let (base, server, _t) = start_server(graph_host(store)).await;
    server.set_read_token(Some(P8_READ_TOKEN.to_string()));
    for path in [
        "/v1/insights".to_string(),
        "/v1/threads".to_string(),
        format!("/v1/threads/{}", fixture.thread_deploy),
        "/v1/entities".to_string(),
    ] {
        let r = get_read(&base, &path).await;
        assert_eq!(r.status(), 200, "{path}");
        assert!(!r.bytes().await.unwrap().is_empty(), "{path} body");
    }

    assert_eq!(
        insight_state(&db),
        before,
        "a GET must not insert, regenerate or restamp an insight"
    );

    server.stop().await;
}

// ---- POST /v1/ask session_id scope (0.4.0 PR6, D12) ----

/// POSTs an ask and returns the sorted frame ids the answer cited (parsed from the SSE stream).
/// `ScriptedAnswer` cites every grounded chunk, so the citation set equals the retrieved
/// context — which is exactly what `session_id` scoping restricts.
async fn scoped_ask_citations(base: &str, payload: serde_json::Value) -> Vec<i64> {
    let r = client()
        .post(format!("{base}/v1/ask"))
        .headers(bearer(TEST_TOKEN))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "ask should stream a 200");
    let text = r.text().await.unwrap();
    let mut ids: Vec<i64> = text
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d.trim()).ok())
        .filter(|v| v["type"] == "citation")
        .filter_map(|v| v["frame_id"].as_i64())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn ask_scoped_cites_only_in_session_frames() {
    let (base, server, _t) = start_server(sessions_fixture_host(true).await).await;

    // Unscoped: the shared term hits every frame (A: 1, 2; B: 3).
    let all = scoped_ask_citations(
        &base,
        serde_json::json!({"query":"deployment pipeline","top_k":10}),
    )
    .await;
    assert_eq!(
        all,
        vec![1, 2, 3],
        "unscoped ask cites frames across both sessions"
    );

    // Scoped to A (session 1): only A frames.
    let a = scoped_ask_citations(
        &base,
        serde_json::json!({"query":"deployment pipeline","top_k":10,"session_id":1}),
    )
    .await;
    assert_eq!(a, vec![1, 2], "scoped ask cites only session A frames");

    // Scoped to B (session 2): only B frame.
    let b = scoped_ask_citations(
        &base,
        serde_json::json!({"query":"deployment pipeline","top_k":10,"session_id":2}),
    )
    .await;
    assert_eq!(b, vec![3], "scoped ask cites only session B frame");

    server.stop().await;
}

#[tokio::test]
async fn ask_unknown_session_is_404_before_stream() {
    // With an answer model available, a bad session_id is a JSON 404, not a stream and not a 200.
    let (base, server, _t) = start_server(sessions_fixture_host(true).await).await;
    let r = client()
        .post(format!("{base}/v1/ask"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({"query":"x","session_id":9999}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    let ct = r
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        ct.contains("application/json"),
        "a 404 must be JSON, never an SSE stream: {ct}"
    );
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["error"], "not_found");
    server.stop().await;

    // Even with no answer model loaded, the 404 for a bad session precedes the 503 for no model.
    let (base, server, _t) = start_server(sessions_fixture_host(false).await).await;
    let r = client()
        .post(format!("{base}/v1/ask"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({"query":"x","session_id":9999}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        404,
        "404 for a bad session takes precedence over the 503 for no model"
    );
    // A known session with no model still 503s.
    let r = client()
        .post(format!("{base}/v1/ask"))
        .headers(bearer(TEST_TOKEN))
        .json(&serde_json::json!({"query":"x","session_id":1}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
    server.stop().await;
}

#[tokio::test]
async fn explicit_ipv4_and_ipv6_listeners_require_authentication() {
    for (bind, destination) in [("0.0.0.0", "127.0.0.1"), ("::1", "[::1]"), ("::", "[::1]")] {
        let token = Arc::new(std::sync::RwLock::new(TEST_TOKEN.to_string()));
        let server =
            ApiServer::start_on(fixture_host(false).await, token, bind.parse().unwrap(), 0)
                .await
                .unwrap();
        assert_eq!(server.local_addr().ip().to_string(), bind);
        let url = format!(
            "http://{destination}:{}/v1/health",
            server.local_addr().port()
        );
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
        assert_eq!(
            client
                .get(&url)
                .headers(bearer(TEST_TOKEN))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        server.stop().await;
    }
}

// ============================================================================================
// PR #122 connection bounds (`03 §7c` review amendment): raw-socket cap/recovery, the
// production header deadline, and shutdown of held connections.
// ============================================================================================

/// Streams `deltas` token events `tick` apart, then `Done`; exits early when the receiver
/// goes away — exactly what a shutdown-cancelled stream looks like. Holds `/v1/ask` open
/// across the shutdown grace.
struct SlowAnswer {
    tick: std::time::Duration,
    deltas: usize,
}

#[async_trait]
impl AnswerProvider for SlowAnswer {
    async fn answer(
        &self,
        _query: &str,
        _context: &[RetrievedChunk],
        _opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> anyhow::Result<()> {
        for index in 0..self.deltas {
            tokio::time::sleep(self.tick).await;
            let delta = AnswerDelta::Token {
                text: format!("chunk-{index}"),
            };
            if tx.send(delta).await.is_err() {
                return Ok(());
            }
        }
        let _ = tx.send(AnswerDelta::done()).await;
        Ok(())
    }
}

/// The fixture host with a slow streaming answer provider (~15 s if never cancelled).
async fn fixture_host_with_slow_answer() -> Arc<TestHost> {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open store"));
    seed_frame(&store, 1_000, "quarterly invoice total").await;
    Arc::new(TestHost {
        store,
        answer: Some(Arc::new(SlowAnswer {
            tick: std::time::Duration::from_millis(200),
            deltas: 75,
        })),
        started_at_ms: 500,
        capture_kernel: None,
        history_fault: None,
        report_page_override: None,
    })
}

/// A complete `GET /v1/health` request with the bearer token, for the raw-socket tests.
fn health_request() -> Vec<u8> {
    format!(
        "GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TEST_TOKEN}\r\n\r\n"
    )
    .into_bytes()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// One read into `buf`; `None` on a read error or when the 5 s guard trips.
async fn read_chunk(stream: &mut tokio::net::TcpStream, buf: &mut Vec<u8>) -> Option<usize> {
    let mut chunk = [0u8; 4096];
    match tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut chunk)).await {
        Ok(Ok(n)) => {
            buf.extend_from_slice(&chunk[..n]);
            Some(n)
        }
        Ok(Err(_)) => None,
        Err(_) => None,
    }
}

/// Reads one complete HTTP/1.1 response from a keep-alive connection and returns its body;
/// `None` when the server closed the connection first (the over-capacity case).
async fn read_one_response(stream: &mut tokio::net::TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    loop {
        if let Some(head_end) = find_subslice(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            assert!(
                head.starts_with("HTTP/1.1 200"),
                "unexpected response: {head}"
            );
            let len = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if name.eq_ignore_ascii_case("content-length") {
                        value.trim().parse::<usize>().ok()
                    } else {
                        None
                    }
                })
                .expect("content-length header");
            let body_start = head_end + 4;
            while buf.len() < body_start + len {
                match read_chunk(stream, &mut buf).await {
                    Some(0) | None => return None,
                    Some(_) => {}
                }
            }
            return Some(String::from_utf8_lossy(&buf[body_start..body_start + len]).to_string());
        }
        match read_chunk(stream, &mut buf).await {
            Some(0) | None => return None,
            Some(_) => {}
        }
    }
}

/// The raw `127.0.0.1:port` socket address behind the `start_server` base URL.
fn socket_addr(base: &str) -> std::net::SocketAddr {
    base.strip_prefix("http://")
        .expect("base URL carries a scheme")
        .parse()
        .expect("socket address")
}

/// Raw-socket capacity: `MAX_ACTIVE_CONNECTIONS` established connections exhaust the
/// listener; the first connection beyond the cap is closed immediately (never served),
/// and closing an admitted connection frees its slot so the listener recovers
/// (`03 §7c`: "admits at most 64 active connections … Accept and immediately close excess
/// sockets").
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connection_cap_closes_excess_sockets_and_recovers() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let addr = socket_addr(&base);

    // Fill the cap with *established keep-alive* connections: each completes one
    // request, so it is provably admitted and holding a slot.
    let mut held = Vec::new();
    for slot in 0..api::MAX_ACTIVE_CONNECTIONS {
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        stream
            .write_all(&health_request())
            .await
            .unwrap_or_else(|error| panic!("slot {slot}: {error}"));
        stream.flush().await.expect("flush");
        let body = read_one_response(&mut stream)
            .await
            .unwrap_or_else(|| panic!("slot {slot} was not served"));
        assert!(body.contains("\"capturing\""), "slot {slot} body: {body}");
        held.push(stream);
    }

    // The first connection beyond the cap is accepted and closed immediately — no
    // response, no queued task.
    let mut excess = tokio::net::TcpStream::connect(addr).await.expect("connect");
    // Writing first is fine: a socket already closed by the server resets the write,
    // which is just another way of observing "closed".
    let _ = excess.write_all(&health_request()).await;
    let mut buf = [0u8; 64];
    match tokio::time::timeout(std::time::Duration::from_secs(10), excess.read(&mut buf)).await {
        Ok(Ok(0)) | Ok(Err(_)) => {}
        Ok(Ok(n)) => panic!("the excess connection was served {n} byte(s) instead of closed"),
        Err(_) => panic!("the excess connection was neither closed nor served"),
    }

    // Closing one admitted connection frees its slot: the listener recovers.
    drop(held.pop());
    let mut recovered = false;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        if stream.write_all(&health_request()).await.is_err() {
            continue;
        }
        let _ = stream.flush().await;
        if let Some(body) = read_one_response(&mut stream).await {
            assert!(body.contains("\"capturing\""), "recovered body: {body}");
            recovered = true;
            break;
        }
    }
    assert!(
        recovered,
        "the listener did not recover after an admitted connection closed"
    );
    server.stop().await;
}

/// Production header deadline: a raw socket that sends partial request headers — and then
/// nothing — is closed at the 30 s deadline (`03 §7c`: "Require each request's headers
/// within 30 seconds"), not earlier. This is the one real-time-long regression; the fast
/// semantics live in the crate's unit tests.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incomplete_headers_are_closed_at_the_production_header_deadline() {
    let (base, server, _t) = start_server(fixture_host(false).await).await;
    let addr = socket_addr(&base);

    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    // A header block that never terminates.
    stream
        .write_all(b"GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Partial: ")
        .await
        .expect("write partial headers");
    stream.flush().await.expect("flush");

    let started = std::time::Instant::now();
    let mut buf = [0u8; 64];
    let read = tokio::time::timeout(
        api::REQUEST_HEADER_DEADLINE + std::time::Duration::from_secs(10),
        stream.read(&mut buf),
    )
    .await;
    let elapsed = started.elapsed();
    assert!(
        read.is_ok(),
        "the connection outlived the header deadline by more than 10 s: {elapsed:?}"
    );
    assert!(
        elapsed >= api::REQUEST_HEADER_DEADLINE - std::time::Duration::from_secs(3),
        "closed before the production header deadline: {elapsed:?}"
    );
    server.stop().await;
}

/// Shutdown (`03 §7c`: "no connection may survive server shutdown"): idle held sockets
/// close at once, an in-flight SSE stream is cancelled at the bounded grace (cut, never
/// completed), the port is released, and `stop` stays bounded.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_closes_held_sockets_and_cuts_streaming_after_the_grace() {
    let (base, server, _t) = start_server(fixture_host_with_slow_answer().await).await;
    let addr = socket_addr(&base);

    // Four held idle sockets: connected, nothing sent.
    let mut idle = Vec::new();
    for _ in 0..4 {
        idle.push(tokio::net::TcpStream::connect(addr).await.expect("connect"));
    }

    // One in-flight SSE stream (~15 s untouched), so the drain must cancel it.
    let mut sse = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let payload = r#"{"query":"invoice"}"#;
    let request = format!(
        "POST /v1/ask HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TEST_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
        payload.len()
    );
    sse.write_all(request.as_bytes()).await.expect("write ask");
    sse.flush().await.expect("flush");
    // Wait for the stream to start, proving the connection is genuinely in flight.
    let mut started_bytes = Vec::new();
    while find_subslice(&started_bytes, b"chunk-0").is_none() {
        match read_chunk(&mut sse, &mut started_bytes).await {
            Some(0) | None => panic!("the SSE stream closed before it started"),
            Some(_) => {}
        }
    }

    // Keep draining the SSE socket in the background: record the bytes and the close.
    let drain = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = sse.read_to_end(&mut buf).await;
        buf
    });

    let started = std::time::Instant::now();
    server.stop().await;
    let elapsed = started.elapsed();
    assert!(
        elapsed >= api::SHUTDOWN_GRACE - std::time::Duration::from_millis(300),
        "the drain must honour the grace before cutting the stream: {elapsed:?}"
    );
    assert!(
        elapsed <= api::SHUTDOWN_GRACE + std::time::Duration::from_secs(2),
        "shutdown must stay bounded: {elapsed:?}"
    );

    // Held idle sockets are closed — nothing survives shutdown.
    for (index, mut stream) in idle.into_iter().enumerate() {
        let mut buf = [0u8; 16];
        match tokio::time::timeout(std::time::Duration::from_secs(3), stream.read(&mut buf)).await {
            Ok(Ok(0)) | Ok(Err(_)) => {}
            Ok(Ok(n)) => {
                panic!("idle socket {index} was served {n} byte(s) instead of closed")
            }
            Err(_) => panic!("idle socket {index} was still open after shutdown"),
        }
    }

    // The in-flight stream was cancelled at the grace, before the provider could finish.
    let streamed = tokio::time::timeout(std::time::Duration::from_secs(5), drain)
        .await
        .expect("the SSE socket did not close after shutdown")
        .expect("drain task");
    let text = String::from_utf8_lossy(&streamed).to_string();
    assert!(
        !text.contains("\"type\":\"done\""),
        "the stream should be cut by shutdown, not completed: {text}"
    );

    // New connections are refused once shutdown completed.
    assert!(
        tokio::net::TcpStream::connect(addr).await.is_err(),
        "the listener should be closed after shutdown"
    );
}
