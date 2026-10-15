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
    AnswerDelta, AnswerOpts, AnswerProvider, ExportFrameRow, FrameDetail, NewFrame, NewSession,
    NewSessionArtifact, OcrResult, ResumeContext, RetrievedChunk, SessionArtifactKind,
    SessionArtifactRole, SessionHost, SessionKind, Settings, Store,
};

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
        let _ = tx.send(AnswerDelta::Done).await;
        Ok(())
    }
}

/// The test seam over a real store. `answer` toggles whether an answer provider is
/// available (for the 503 path).
struct TestHost {
    store: Arc<SqliteStore>,
    answer: Option<Arc<dyn AnswerProvider>>,
    started_at_ms: i64,
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
        let _ = tx.send(AnswerDelta::Done).await;
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
