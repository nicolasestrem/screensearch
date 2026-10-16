//! P1 auth cases share the existing real-SQLite HTTP fixture, not canned responses.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const READ: &str = "p1-read-credential";

#[tokio::test]
async fn read_token_route_matrix_including_head_image_cached_summary_and_methods() {
    let (base, server, _) = start_server(sessions_fixture_host(false).await).await;
    server.set_read_token(Some(READ.to_string()));
    let c = client();
    for token in [TEST_TOKEN, READ] {
        for path in [
            "/v1/health",
            "/v1/search?q=invoice",
            "/v1/frames/1",
            "/v1/frames/1?image=1",
            "/v1/context/where-was-i",
            "/v1/marks",
            "/v1/sessions",
            "/v1/sessions/1",
            "/v1/sessions/1?include_summary=1",
            "/v1/export",
        ] {
            for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
                let r = c
                    .request(method.clone(), format!("{base}{path}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(r.status(), 200, "{method} {path}");
                if method == reqwest::Method::HEAD {
                    assert!(r.bytes().await.unwrap().is_empty());
                }
            }
        }
        for (path, allow) in [
            ("/v1/health", "GET,HEAD"),
            ("/v1/search", "GET,HEAD"),
            ("/v1/frames/1?image=1", "GET,HEAD"),
            ("/v1/context/where-was-i", "GET,HEAD"),
            ("/v1/sessions", "GET,HEAD"),
            ("/v1/sessions/1", "GET,HEAD"),
            ("/v1/marks", "GET,HEAD,POST"),
            ("/v1/export", "GET,HEAD"),
            ("/v1/marks/1/resolve", "POST"),
            ("/v1/ask", "POST"),
        ] {
            for method in [
                reqwest::Method::PUT,
                reqwest::Method::PATCH,
                reqwest::Method::DELETE,
                reqwest::Method::OPTIONS,
                reqwest::Method::TRACE,
                reqwest::Method::from_bytes(b"FUTURE").unwrap(),
            ] {
                let r = c
                    .request(method.clone(), format!("{base}{path}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(r.status(), 405, "{method} {path}");
                assert_eq!(r.headers()[reqwest::header::ALLOW], allow);
                assert!(r.bytes().await.unwrap().is_empty());
            }
        }
        for path in ["/v1/ask", "/v1/marks/1/resolve"] {
            for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
                let r = c
                    .request(method, format!("{base}{path}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(r.status(), 405);
                assert_eq!(r.headers()[reqwest::header::ALLOW], "POST");
            }
        }
        for path in [
            "/v1/future",
            "/outside-v1",
            "/v1/frames/1/extra",
            "/v1/reports/generate",
        ] {
            for method in [
                reqwest::Method::GET,
                reqwest::Method::HEAD,
                reqwest::Method::POST,
                reqwest::Method::PUT,
            ] {
                let r = c
                    .request(method.clone(), format!("{base}{path}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(r.status(), 404, "{method} {path}");
                if method != reqwest::Method::HEAD {
                    assert_eq!(
                        r.json::<serde_json::Value>().await.unwrap()["error"],
                        "not_found"
                    );
                }
            }
        }
    }
    // Authentication precedes method/path dispatch for every route, HEAD and future method too.
    for path in [
        "/v1/health",
        "/v1/search",
        "/v1/frames/1?image=1",
        "/v1/context/where-was-i",
        "/v1/sessions",
        "/v1/sessions/1",
        "/v1/marks",
        "/v1/marks/1/resolve",
        "/v1/ask",
        "/v1/export",
        "/outside-v1",
    ] {
        for method in [
            reqwest::Method::GET,
            reqwest::Method::HEAD,
            reqwest::Method::POST,
            reqwest::Method::PUT,
            reqwest::Method::PATCH,
            reqwest::Method::DELETE,
            reqwest::Method::OPTIONS,
        ] {
            for token in [None, Some("invalid")] {
                let mut req = c.request(method.clone(), format!("{base}{path}"));
                if let Some(token) = token {
                    req = req.bearer_auth(token);
                }
                assert_eq!(req.send().await.unwrap().status(), 401, "{method} {path}");
            }
        }
    }
    server.stop().await;
}

#[tokio::test]
async fn read_token_rotation_revocation_and_legacy_swaps_are_independent() {
    let (base, server, legacy) = start_server(fixture_host(false).await).await;
    let addr = server.local_addr();
    let c = client();
    async fn status(c: &reqwest::Client, base: &str, token: &str) -> u16 {
        c.get(format!("{base}/v1/health"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }
    assert_eq!(status(&c, &base, READ).await, 401);
    server.set_read_token(Some(READ.into()));
    assert_eq!(status(&c, &base, READ).await, 200);
    assert_eq!(*legacy.read().unwrap(), TEST_TOKEN);
    server.set_read_token(Some("read-rotated".into()));
    assert_eq!(status(&c, &base, READ).await, 401);
    assert_eq!(status(&c, &base, "read-rotated").await, 200);
    assert_eq!(status(&c, &base, TEST_TOKEN).await, 200);
    server.set_token("legacy-rotated".into());
    assert_eq!(*legacy.read().unwrap(), "legacy-rotated");
    assert_eq!(status(&c, &base, TEST_TOKEN).await, 401);
    assert_eq!(status(&c, &base, "legacy-rotated").await, 200);
    assert_eq!(status(&c, &base, "read-rotated").await, 200);
    server.set_read_token(None);
    assert_eq!(status(&c, &base, "read-rotated").await, 401);
    assert_eq!(status(&c, &base, "legacy-rotated").await, 200);
    server.set_read_token(Some(String::new()));
    assert_eq!(status(&c, &base, "").await, 401);
    server.set_read_token(Some(READ.into()));
    // Preserve existing direct SharedToken mutation compatibility, including revocation.
    *legacy.write().unwrap() = String::new();
    assert_eq!(status(&c, &base, "legacy-rotated").await, 401);
    assert_eq!(status(&c, &base, READ).await, 200);
    assert_eq!(server.local_addr(), addr);
    server.stop().await;
}

struct CountHost {
    inner: Arc<TestHost>,
    calls: AtomicUsize,
    marks: AtomicUsize,
    capture: AtomicUsize,
    resolve: AtomicUsize,
    provider: AtomicUsize,
    grounding: AtomicUsize,
}

impl CountHost {
    fn touch(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl ApiHost for CountHost {
    fn version(&self) -> String {
        self.touch();
        self.inner.version()
    }
    fn started_at_ms(&self) -> i64 {
        self.touch();
        self.inner.started_at_ms()
    }
    async fn is_capturing(&self) -> bool {
        self.touch();
        self.inner.is_capturing().await
    }
    fn store(&self) -> Arc<dyn Store> {
        self.touch();
        self.inner.store()
    }
    async fn settings(&self) -> Settings {
        self.touch();
        self.inner.settings().await
    }
    async fn frame_detail(&self, id: i64) -> anyhow::Result<Option<FrameDetail>> {
        self.touch();
        self.inner.frame_detail(id).await
    }
    async fn frame_page(
        &self,
        request: traits::FramePageRequest,
    ) -> Result<traits::StoredFramePage, traits::HistoryReadError> {
        self.touch();
        self.inner.frame_page(request).await
    }
    async fn capture_coverage(
        &self,
        window: traits::EvidenceWindow,
    ) -> Result<traits::CaptureCoverage, traits::HistoryReadError> {
        self.touch();
        self.inner.capture_coverage(window).await
    }
    async fn frame_image(&self, id: i64) -> anyhow::Result<Option<FrameImage>> {
        self.touch();
        self.inner.frame_image(id).await
    }
    async fn ask_context(
        &self,
        q: &str,
        k: u32,
        session: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>> {
        self.touch();
        self.grounding.fetch_add(1, Ordering::SeqCst);
        self.inner.ask_context(q, k, session).await
    }
    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        self.touch();
        self.provider.fetch_add(1, Ordering::SeqCst);
        self.inner.answer_provider().await
    }
    async fn add_mark(
        &self,
        id: Option<i64>,
        now: bool,
        note: Option<String>,
    ) -> anyhow::Result<i64> {
        self.touch();
        self.marks.fetch_add(1, Ordering::SeqCst);
        if now {
            self.capture.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.add_mark(id, now, note).await
    }
    async fn resolve_mark(&self, id: i64) -> anyhow::Result<()> {
        self.touch();
        self.resolve.fetch_add(1, Ordering::SeqCst);
        self.inner.resolve_mark(id).await
    }
    async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
        self.touch();
        self.inner.where_was_i().await
    }
    async fn export_frames_page(
        &self,
        after: i64,
        from: Option<i64>,
        to: Option<i64>,
        limit: u32,
    ) -> anyhow::Result<Vec<ExportFrameRow>> {
        self.touch();
        self.inner.export_frames_page(after, from, to, limit).await
    }
}

#[tokio::test]
async fn history_bad_queries_and_auth_denials_do_not_enter_any_host_adapter() {
    let inner =
        super::history_reads::history_host(Arc::new(SqliteStore::open_in_memory().unwrap()));
    let h = Arc::new(CountHost {
        inner: inner.clone(),
        calls: AtomicUsize::new(0),
        marks: AtomicUsize::new(0),
        capture: AtomicUsize::new(0),
        resolve: AtomicUsize::new(0),
        provider: AtomicUsize::new(0),
        grounding: AtomicUsize::new(0),
    });
    let before = inner.store.history_readiness().await.unwrap();
    let server = ApiServer::start(
        h.clone(),
        Arc::new(std::sync::RwLock::new(TEST_TOKEN.into())),
        0,
    )
    .await
    .unwrap();
    server.set_read_token(Some(READ.into()));
    let base = format!("http://{}", server.local_addr());
    let c = client();
    let mut frame_bad = vec![
        "".to_string(),
        "from=0".into(),
        "to=1".into(),
        "from=1&to=1".into(),
        "from=2&to=1".into(),
        "from=NaN&to=1".into(),
        "from=0.0&to=1".into(),
        "from=1e0&to=2".into(),
        "from=9223372036854775808&to=9223372036854775809".into(),
        "from=-9223372036854775809&to=1".into(),
        "from=0&to=1&limit=0".into(),
        "from=0&to=1&limit=1001".into(),
        "from=0&to=1&limit=-1".into(),
        "from=0&to=1&limit=1.0".into(),
        "from=0&to=1&limit=+1".into(),
        "from=0&to=1&limit=4294967296".into(),
        "from=0&to=1&q=search".into(),
        "from=0&to=1&wat=".into(),
        "from=0&to=1&from=0".into(),
        "from=0&to=1&%66rom=0".into(),
        "from=0&to=1&to=1".into(),
        "from=0&to=1&app=x&app=x".into(),
        "from=0&to=1&window_title=x&window_title=y".into(),
        "from=0&to=1&limit=1&limit=2".into(),
        "from=0&to=1&app=%FF".into(),
        "from=0&to=1&app=%0g".into(),
        "cursor=".into(),
        "cursor=invalid".into(),
        "cursor=invalid&cursor=invalid".into(),
        "cursor=invalid&from=0".into(),
        "cursor=invalid&to=1".into(),
        "cursor=invalid&app=".into(),
        "cursor=invalid&window_title=".into(),
    ];
    for name in ["app", "window_title"] {
        frame_bad.push(format!("from=0&to=1&{name}={}", "x".repeat(257)));
        frame_bad.push(format!("from=0&to=1&{name}={}", "%C3%89".repeat(129)));
    }
    frame_bad.push(format!("cursor={}", "x".repeat(4097)));
    let coverage_bad = [
        "",
        "from=0",
        "to=1",
        "from=1&to=1",
        "from=1&to=0",
        "from=NaN&to=1",
        "from=0&to=1&from=0",
        "from=0&to=1&limit=1",
        "from=0&to=1&app=",
        "from=0&to=1&window_title=",
        "from=0&to=1&cursor=",
        "from=0&to=1&q=",
        "from=0&to=1&unknown=",
    ];
    for token in [TEST_TOKEN, READ] {
        for (endpoint, bad) in [
            (
                "frames",
                frame_bad.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
            ("coverage", coverage_bad.to_vec()),
        ] {
            for query in bad {
                let r = c
                    .get(format!("{base}/v1/{endpoint}?{query}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(r.status(), 400, "{endpoint} {query}");
                assert_eq!(
                    r.json::<serde_json::Value>().await.unwrap(),
                    serde_json::json!({"error":"bad_request","message":"invalid history query or cursor"})
                );
                let r = c
                    .head(format!("{base}/v1/{endpoint}?{query}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(r.status(), 400);
                assert!(r.bytes().await.unwrap().is_empty());
            }
        }
    }
    for endpoint in ["frames", "coverage"] {
        for query in ["from=0&to=1", "not-a-query"] {
            for token in [None, Some("invalid")] {
                for method in [
                    reqwest::Method::GET,
                    reqwest::Method::HEAD,
                    reqwest::Method::POST,
                ] {
                    let mut r = c.request(method, format!("{base}/v1/{endpoint}?{query}"));
                    if let Some(token) = token {
                        r = r.bearer_auth(token);
                    }
                    assert_eq!(r.send().await.unwrap().status(), 401);
                }
            }
        }
    }
    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.capture.load(Ordering::SeqCst), 0);
    assert_eq!(h.provider.load(Ordering::SeqCst), 0);
    let after = inner.store.history_readiness().await.unwrap();
    assert_eq!(after, before);
    assert_eq!(
        inner
            .store
            .stored_coverage(traits::EvidenceWindow { from: 0, to: 1 })
            .await
            .unwrap()
            .frames_stored,
        0
    );
    // Sanity check the counter actually observes an admitted real adapter call.
    assert_eq!(
        c.get(format!("{base}/v1/frames?from=0&to=1"))
            .bearer_auth(READ)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    server.stop().await;
}

#[tokio::test]
async fn denials_leave_store_capture_model_and_all_host_calls_unchanged() {
    // Both available and unavailable providers: permission denial must win before either.
    for answer_available in [false, true] {
        let inner = fixture_host(answer_available).await;
        let mark = inner
            .store
            .insert_mark(1, 1000, Some("unchanged".into()))
            .await
            .unwrap();
        let before = serde_json::to_value(inner.store.list_marks().await.unwrap()).unwrap();
        let frames_before = inner
            .store
            .export_frames_page(0, None, None, 100)
            .await
            .unwrap()
            .len();
        let h = Arc::new(CountHost {
            inner: inner.clone(),
            calls: AtomicUsize::new(0),
            marks: AtomicUsize::new(0),
            capture: AtomicUsize::new(0),
            resolve: AtomicUsize::new(0),
            provider: AtomicUsize::new(0),
            grounding: AtomicUsize::new(0),
        });
        let token = Arc::new(std::sync::RwLock::new(TEST_TOKEN.into()));
        let server = ApiServer::start(h.clone(), token, 0).await.unwrap();
        server.set_read_token(Some(READ.into()));
        let base = format!("http://{}", server.local_addr());
        let c = client();
        for (path, body) in [
            ("/v1/marks".into(), r#"{"frame_id":1}"#),
            ("/v1/marks".into(), r#"{"frame_id":999999}"#),
            ("/v1/marks".into(), r#"{"now":true}"#),
            ("/v1/marks".into(), r#"{"frame_id":1,"now":true}"#),
            ("/v1/marks".into(), r#"{"frame_id":null,"now":true}"#),
            ("/v1/marks".into(), r#"{"now":"true"}"#),
            ("/v1/marks".into(), "not-json"),
            (format!("/v1/marks/{mark}/resolve"), "{}"),
            ("/v1/marks/not-an-id/resolve".into(), "{}"),
            ("/v1/ask".into(), r#"{"query":"invoice"}"#),
            (
                "/v1/ask".into(),
                r#"{"query":"invoice","session_id":999999}"#,
            ),
            ("/v1/ask".into(), "not-json"),
        ] {
            let r = c
                .post(format!("{base}{path}"))
                .bearer_auth(READ)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body)
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 403, "{path} {body}");
            assert_ne!(
                r.headers()[reqwest::header::CONTENT_TYPE],
                "text/event-stream"
            );
            assert_eq!(
                r.json::<serde_json::Value>().await.unwrap()["error"],
                "forbidden"
            );
        }
        // Missing/invalid image HEAD must not even read stored bytes or touch the host.
        for token in [None, Some("invalid")] {
            for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
                let mut r = c.request(method, format!("{base}/v1/frames/1?image=1"));
                if let Some(token) = token {
                    r = r.bearer_auth(token);
                }
                assert_eq!(r.send().await.unwrap().status(), 401);
            }
        }
        assert_eq!(
            h.calls.load(Ordering::SeqCst),
            0,
            "denied requests reached ApiHost"
        );
        for counter in [&h.marks, &h.capture, &h.resolve, &h.provider, &h.grounding] {
            assert_eq!(counter.load(Ordering::SeqCst), 0);
        }
        assert_eq!(
            serde_json::to_value(inner.store.list_marks().await.unwrap()).unwrap(),
            before
        );
        assert_eq!(
            inner
                .store
                .export_frames_page(0, None, None, 100)
                .await
                .unwrap()
                .len(),
            frames_before
        );
        // Positive control proves the counters actually observe admitted handler work.
        assert_eq!(
            c.get(format!("{base}/v1/health"))
                .bearer_auth(READ)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert!(h.calls.load(Ordering::SeqCst) > 0);
        server.stop().await;
    }
}
