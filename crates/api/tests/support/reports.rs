//! P3 saved-report reads (`03 §7f.5/.6`, §8c): real `SqliteStore` rows through the
//! production writers, driven over the real `ApiServer` + bearer auth. The HTTP surface
//! must serve the stored projection only — never generation, scheduling, delivery,
//! filesystem reads or the in-app current-source overlay.
use super::*;
use traits::{
    EvidenceWindow, MemoryReportRange, ReportError, ReportOutcome, ReportPosition, ReportRun,
    ReportRunKind, ReportRunPhase, ReportSummary, SavedReport, StoredReportPage,
    REPORT_CURSOR_MAX_BYTES,
};

/// Answer provider that aborts the test if any report read reaches generation.
struct PanicAnswer;

#[async_trait]
impl AnswerProvider for PanicAnswer {
    async fn answer(
        &self,
        _query: &str,
        _context: &[RetrievedChunk],
        _opts: AnswerOpts,
        _tx: Sender<AnswerDelta>,
    ) -> anyhow::Result<()> {
        panic!("saved-report reads must never invoke the answer model");
    }
}

const ABSENT: &str = "PRIVATE-diagnostic-text-must-not-leak";

fn saved(markdown: &str, frame_ids: Vec<i64>) -> SavedReport {
    SavedReport {
        markdown: markdown.to_string(),
        model: Some("fixture-model".into()),
        session_ids: vec![7, 8],
        cited_frame_ids: frame_ids.clone(),
        frames_sampled: 9,
        frames_summarized: frame_ids.len() as u32,
        periods_total: 3,
        periods_covered: 2,
        passes: 1,
        truncated: false,
        frame_ids,
        daily_run_ids: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    run_id: &str,
    created_at: i64,
    window_start: i64,
    phase: ReportRunPhase,
    outcome: Option<ReportOutcome>,
    error: Option<ReportError>,
    result: Option<SavedReport>,
) -> ReportRun {
    ReportRun {
        requested_range: None,
        run_id: run_id.into(),
        // A source id that does not correspond to any current profile/source.
        source_id: format!("source-absent-{run_id}"),
        kind: ReportRunKind::Daily,
        schedule_id: None,
        revision_of: None,
        window_start,
        window_end: window_start + 500,
        timezone: "Europe/Paris".into(),
        local_date: "2026-02-03".into(),
        phase,
        outcome,
        error,
        result,
        config_snapshot: "PRIVATE-config-snapshot".into(),
        created_at,
        updated_at: created_at + 5,
    }
}

/// Six runs covering every visibility/error-code case. Distinct windows keep the store's
/// same-window/source de-duplication out of the fixture.
fn fixture_runs() -> Vec<ReportRun> {
    let mut runs = vec![
        run(
            "run-a",
            100,
            1_000,
            ReportRunPhase::Finished,
            Some(ReportOutcome::Ok),
            None,
            Some(saved("# A markdown\nprovenance", vec![11, 12])),
        ),
        run(
            "run-b",
            200,
            1_100,
            ReportRunPhase::Finished,
            Some(ReportOutcome::NoData),
            None,
            Some(saved("NO_DATA saved result", vec![])),
        ),
        run(
            "run-c",
            300,
            1_200,
            ReportRunPhase::Queued,
            None,
            None,
            Some(saved("# queued saved output", vec![21])),
        ),
        run(
            "run-d",
            400,
            1_300,
            ReportRunPhase::Running,
            None,
            None,
            Some(saved("# running saved output", vec![31])),
        ),
        run(
            "run-e",
            500,
            1_400,
            ReportRunPhase::Blocked,
            Some(ReportOutcome::Failed),
            Some(ReportError {
                code: "MODEL_UNAVAILABLE".into(),
                message: ABSENT.into(),
            }),
            None,
        ),
        run(
            "run-f",
            600,
            1_500,
            ReportRunPhase::Finished,
            Some(ReportOutcome::Failed),
            Some(ReportError {
                code: "SECRET_LEAK".into(),
                message: ABSENT.into(),
            }),
            None,
        ),
    ];
    // The public `requested_range` is exposed; give one run a real value.
    runs[0].requested_range = Some(MemoryReportRange {
        start_date: "2026-01-01".into(),
        end_date: "2026-01-02".into(),
    });
    // A stored schedule id must never reach the wire (a scheduled run carries no manual
    // requested range, so this is the run that can legally hold one).
    runs[1].schedule_id = Some("PRIVATE-schedule-id".into());
    runs
}

async fn seeded_store() -> Arc<SqliteStore> {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open store"));
    for run in fixture_runs() {
        store.create_report_run(run).await.expect("seed report run");
    }
    store
}

fn report_host(
    store: Arc<SqliteStore>,
    fault: Option<history_reads::HistoryFault>,
) -> Arc<TestHost> {
    Arc::new(TestHost {
        store,
        answer: Some(Arc::new(PanicAnswer) as Arc<dyn AnswerProvider>),
        started_at_ms: 500,
        capture_kernel: None,
        history_fault: fault,
        report_page_override: None,
    })
}

const WINDOW: &str = "from=0&to=9223372036854775807";

async fn get(base: &str, path: &str) -> (reqwest::StatusCode, String) {
    let r = client()
        .get(format!("{base}{path}"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    let status = r.status();
    (status, r.text().await.unwrap())
}

fn summary_keys() -> Vec<&'static str> {
    vec![
        "created_at",
        "error_code",
        "kind",
        "local_date",
        "outcome",
        "phase",
        "requested_range",
        "revision_of",
        "run_id",
        "source_id",
        "timezone",
        "updated_at",
        "window_end",
        "window_start",
    ]
}

fn assert_keys(value: &serde_json::Value, mut expected: Vec<&str>) {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    expected.sort_unstable();
    assert_eq!(
        keys, expected,
        "wire projection must expose exactly the frozen fields"
    );
}

#[tokio::test]
async fn reports_list_serves_exact_saved_projection_in_descending_order() {
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(store, None)).await;
    let (status, body) = get(&base, &format!("/v1/reports?{WINDOW}")).await;
    assert_eq!(status, 200, "{body}");
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    let items = value["items"].as_array().unwrap();
    let order: Vec<&str> = items
        .iter()
        .map(|item| item["run_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        ["run-f", "run-e", "run-d", "run-c", "run-b", "run-a"]
    );
    assert!(
        value["next_cursor"].is_null(),
        "a short page terminates explicitly"
    );
    for item in items {
        assert_keys(item, summary_keys());
        // No stored private internals cross the wire at all.
        assert!(item.get("schedule_id").is_none());
        assert!(item.get("config_snapshot").is_none());
        assert!(item.get("result").is_none());
    }
    // Raw body never carries private internals or arbitrary error text.
    for leak in [
        "schedule_id",
        "config_snapshot",
        "destination_snapshot",
        "PRIVATE-schedule-id",
        "PRIVATE-config-snapshot",
        ABSENT,
    ] {
        assert!(!body.contains(leak), "body leaked {leak}");
    }
    let by_id = |id: &str| {
        items
            .iter()
            .find(|item| item["run_id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(by_id("run-e")["phase"], "blocked");
    assert_eq!(by_id("run-e")["outcome"], "FAILED");
    assert_eq!(by_id("run-e")["error_code"], "MODEL_UNAVAILABLE");
    assert!(
        by_id("run-f")["error_code"].is_null(),
        "unknown codes project null"
    );
    assert_eq!(by_id("run-c")["phase"], "queued");
    assert!(by_id("run-c")["outcome"].is_null());
    assert!(by_id("run-a")["revision_of"].is_null());
    assert_eq!(
        by_id("run-a")["requested_range"]["start_date"],
        "2026-01-01"
    );
    assert_eq!(by_id("run-a")["requested_range"]["end_date"], "2026-01-02");
    assert_eq!(by_id("run-a")["source_id"], "source-absent-run-a");
    // The persisted kind discriminator is a required projection field (`03 §8e`).
    assert_eq!(by_id("run-a")["kind"], "daily");
    assert_eq!(by_id("run-f")["kind"], "daily");
    server.stop().await;
}

#[tokio::test]
async fn reports_detail_serves_full_saved_markdown_and_provenance_with_explicit_nulls() {
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(store, None)).await;
    let (status, body) = get(&base, "/v1/reports/run-a").await;
    assert_eq!(status, 200, "{body}");
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    let mut expected = summary_keys();
    expected.push("result");
    assert_keys(&value, expected);
    assert_eq!(value["result"]["markdown"], "# A markdown\nprovenance");
    assert_eq!(value["result"]["model"], "fixture-model");
    assert_eq!(value["result"]["frame_ids"], serde_json::json!([11, 12]));
    assert_eq!(value["result"]["session_ids"], serde_json::json!([7, 8]));
    assert_eq!(
        value["result"]["cited_frame_ids"],
        serde_json::json!([11, 12])
    );
    assert_eq!(value["result"]["frames_sampled"], 9);
    assert_eq!(value["result"]["frames_summarized"], 2);
    assert_eq!(value["result"]["truncated"], false);
    assert!(!body.contains(ABSENT) && !body.contains("PRIVATE-config-snapshot"));
    server.stop().await;
}

#[tokio::test]
async fn reports_expose_saved_output_in_any_phase_and_null_only_without_output() {
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(store, None)).await;
    // Queued and running runs still expose their immutable saved result (§7f.10).
    for (id, markdown) in [
        ("run-c", "# queued saved output"),
        ("run-d", "# running saved output"),
        ("run-b", "NO_DATA saved result"),
    ] {
        let (status, body) = get(&base, &format!("/v1/reports/{id}")).await;
        assert_eq!(status, 200, "{body}");
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["result"]["markdown"], markdown, "{id}");
    }
    // A run with no saved result is an explicit null, never replacement prose.
    let (status, body) = get(&base, "/v1/reports/run-e").await;
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(value["result"].is_null());
    assert_eq!(value["error_code"], "MODEL_UNAVAILABLE");
    server.stop().await;
}

#[tokio::test]
async fn reports_continuation_cursor_freezes_window_filters_and_carries_limit() {
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(store, None)).await;
    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut mid_cursor: Option<String> = None;
    let pages = [["run-f", "run-e"], ["run-d", "run-c"], ["run-b", "run-a"]];
    for (index, expected) in pages.iter().enumerate() {
        let path = match &cursor {
            None => format!("/v1/reports?{WINDOW}&limit=2"),
            Some(cursor) => format!("/v1/reports?cursor={cursor}&limit=2"),
        };
        let (status, body) = get(&base, &path).await;
        assert_eq!(status, 200, "{body}");
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        let ids: Vec<&str> = value["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["run_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, *expected);
        seen.extend(ids.iter().map(|s| s.to_string()));
        cursor = value["next_cursor"].as_str().map(str::to_string);
        if index == 0 {
            mid_cursor = cursor.clone();
        }
        if index + 1 < pages.len() {
            assert!(cursor.is_some(), "more rows follow");
        }
    }
    assert!(
        cursor.is_none(),
        "the final full page terminates explicitly"
    );
    // No duplicates and no omissions across the traversal.
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 6);
    // Only `limit` may accompany a cursor.
    let mid = mid_cursor.expect("the first page yields a continuation cursor");
    let (status, _) = get(&base, &format!("/v1/reports?cursor={mid}&limit=2&from=0")).await;
    assert_eq!(status, 400);
    server.stop().await;
}

#[tokio::test]
async fn reports_reject_bad_queries_enums_encoding_and_oversize_with_400() {
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(store, None)).await;
    for query in [
        "",
        "from=0",
        "to=1",
        "from=1&to=1",
        "from=2&to=1",
        "from=NaN&to=1",
        "from=0&to=1&limit=0",
        "from=0&to=1&limit=1001",
        "from=0&to=1&limit=",
        "from=0&to=1&phase=bogus",
        "from=0&to=1&outcome=ok",
        "from=0&to=1&phase=queued&phase=running",
        "from=0&to=1&from=0",
        "from=0&to=1&unknown=",
        "from=0&to=1&phase=%FF",
        "from=0&to=1&phase=%",
        "cursor=abc&from=0",
        "cursor=abc&cursor=abc",
    ] {
        let (status, _) = get(&base, &format!("/v1/reports?{query}")).await;
        assert_eq!(status, 400, "{query}");
    }
    let oversize = "x".repeat(REPORT_CURSOR_MAX_BYTES + 1);
    let (status, _) = get(&base, &format!("/v1/reports?cursor={oversize}")).await;
    assert_eq!(status, 400);
    // The valid initial form is a real 200.
    let (status, _) = get(&base, &format!("/v1/reports?{WINDOW}&phase=finished")).await;
    assert_eq!(status, 200);
    server.stop().await;
}

#[tokio::test]
async fn reports_get_and_head_use_history_read_for_both_credentials() {
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(store, None)).await;
    server.set_read_token(Some("reports-read-token".into()));
    let c = client();
    for token in [TEST_TOKEN, "reports-read-token"] {
        for path in [
            format!("/v1/reports?{WINDOW}"),
            "/v1/reports/run-a".to_string(),
        ] {
            let get = c
                .get(format!("{base}{path}"))
                .bearer_auth(token)
                .send()
                .await
                .unwrap();
            assert_eq!(get.status(), 200, "GET {path} as {token}");
            let head = c
                .head(format!("{base}{path}"))
                .bearer_auth(token)
                .send()
                .await
                .unwrap();
            assert_eq!(head.status(), 200, "HEAD {path} as {token}");
        }
        // Read tokens never gain a mutation route: POST is a terminal 405.
        for path in [
            format!("/v1/reports?{WINDOW}"),
            "/v1/reports/run-a".to_string(),
        ] {
            let post = c
                .post(format!("{base}{path}"))
                .bearer_auth(token)
                .send()
                .await
                .unwrap();
            assert_eq!(post.status(), 405, "POST {path} as {token}");
        }
    }
    // No credential → 401, and never a body-dependent grant.
    let unauth = c
        .get(format!("{base}/v1/reports?{WINDOW}"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), 401);
    server.stop().await;
}

#[tokio::test]
async fn reports_missing_is_404_and_unavailable_and_storage_are_fixed_safe_errors() {
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(store, None)).await;
    let (status, body) = get(&base, "/v1/reports/does-not-exist").await;
    assert_eq!(status, 404);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"],
        "not_found"
    );
    server.stop().await;

    // Readiness unavailable → fixed 503, never empty 200/NO_DATA.
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(
        store,
        Some(history_reads::HistoryFault::Unavailable),
    ))
    .await;
    for path in [
        format!("/v1/reports?{WINDOW}"),
        "/v1/reports/run-a".to_string(),
    ] {
        let (status, body) = get(&base, &path).await;
        assert_eq!(status, 503, "{path}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"error": "unavailable", "message": "history feed unavailable"})
        );
    }
    server.stop().await;

    // Storage/decode failure → fixed, redacted 500.
    let store = seeded_store().await;
    let (base, server, _) = start_server(report_host(
        store,
        Some(history_reads::HistoryFault::Storage),
    ))
    .await;
    for path in [
        format!("/v1/reports?{WINDOW}"),
        "/v1/reports/run-a".to_string(),
    ] {
        let (status, body) = get(&base, &path).await;
        assert_eq!(status, 500, "{path}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"error": "internal", "message": "history storage error"})
        );
        assert!(!body.contains("PRIVATE") && !body.contains("screen.db"));
    }
    server.stop().await;
}

#[tokio::test]
async fn reports_reads_never_mutate_stored_state_or_invoke_the_provider() {
    let store = seeded_store().await;
    let before: Vec<ReportRun> = store.list_report_runs(1000, None).await.unwrap();
    let (base, server, _) = start_server(report_host(store.clone(), None)).await;
    let (status, _) = get(&base, &format!("/v1/reports?{WINDOW}&limit=2")).await;
    assert_eq!(status, 200);
    let (status, _) = get(&base, "/v1/reports/run-a").await;
    assert_eq!(status, 200);
    server.stop().await;
    // The stored runs are byte-for-byte unchanged: no phase/updated_at/result writes and
    // no new delivery rows. The panicking provider was never reached.
    let after: Vec<ReportRun> = store.list_report_runs(1000, None).await.unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn reports_next_position_without_snapshot_is_a_storage_failure_not_empty_success() {
    // A page that claims a continuation but carries no frozen max tuple is internally
    // inconsistent — the handler must fail closed (500), never emit an empty 200.
    let summary = ReportSummary {
        run_id: "run-a".into(),
        source_id: "source-absent-run-a".into(),
        kind: ReportRunKind::Daily,
        revision_of: None,
        requested_range: None,
        window_start: 1_000,
        window_end: 1_500,
        timezone: "Europe/Paris".into(),
        local_date: "2026-02-03".into(),
        created_at: 100,
        updated_at: 105,
        phase: ReportRunPhase::Finished,
        outcome: None,
        error_code: None,
    };
    let page = StoredReportPage {
        items: vec![summary],
        snapshot: None,
        window: EvidenceWindow {
            from: 0,
            to: i64::MAX,
        },
        filters: traits::ReportFilters::default(),
        next_position: Some(ReportPosition {
            created_at: 100,
            run_id: "run-a".into(),
        }),
    };
    let store = seeded_store().await;
    let host = Arc::new(TestHost {
        store,
        answer: Some(Arc::new(PanicAnswer) as Arc<dyn AnswerProvider>),
        started_at_ms: 500,
        capture_kernel: None,
        history_fault: None,
        report_page_override: Some(page),
    });
    let (base, server, _) = start_server(host).await;
    let (status, body) = get(&base, &format!("/v1/reports?{WINDOW}")).await;
    assert_eq!(status, 500, "{body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"error": "internal", "message": "history storage error"})
    );
    server.stop().await;
}
