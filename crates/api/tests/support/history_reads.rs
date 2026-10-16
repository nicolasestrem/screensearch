//! P1 chronological reads use real SQLite and real controller observations.
//! Public production Store operations model insertion/deletion/enrichment.
//! Typed fault injection exercises only the unavailable/error HTTP boundary.
use super::*;
use traits::{CaptureCoverage, EvidenceWindow, FramePage, HistoryReadError};

#[derive(Clone, Copy)]
pub(super) enum HistoryFault {
    Unavailable,
    Storage,
}
impl HistoryFault {
    pub(super) fn error(self) -> HistoryReadError {
        match self {
            Self::Unavailable => {
                HistoryReadError::Unavailable(traits::HistoryUnavailableReason::MissingAllocator)
            }
            Self::Storage => HistoryReadError::Storage(anyhow::anyhow!(
                "PRIVATE C:\\Users\\secret\\screen.db: arbitrary underlying SQL diagnostic"
            )),
        }
    }
}

struct NeverOcr;
#[async_trait]
impl traits::OcrProvider for NeverOcr {
    async fn recognize(&self, _: &traits::CapturedFrame) -> anyhow::Result<OcrResult> {
        panic!("history reads must not invoke OCR")
    }
}
struct IdleCapture;
#[async_trait]
impl traits::CaptureSource for IdleCapture {
    fn monitors(&self) -> Vec<traits::MonitorInfo> {
        vec![]
    }
    async fn next_frame(&mut self) -> anyhow::Result<Option<traits::CapturedFrame>> {
        std::future::pending().await
    }
}

pub(super) fn history_host(store: Arc<SqliteStore>) -> Arc<TestHost> {
    let controller = kernel::Kernel::new(
        store.clone(),
        Arc::new(NeverOcr),
        Arc::new(|_, _| Ok(Box::new(IdleCapture))),
        std::env::temp_dir().join("history-api-fixture-no-frame-files"),
        traits::Readiness::default(),
    );
    Arc::new(TestHost {
        store,
        answer: None,
        started_at_ms: 500,
        capture_kernel: Some(Arc::new(controller)),
        history_fault: None,
    })
}

pub(super) async fn compose_coverage(
    host: &TestHost,
    window: EvidenceWindow,
) -> Result<CaptureCoverage, HistoryReadError> {
    let facts = host.store.stored_coverage(window).await?;
    let controller = host
        .capture_kernel
        .as_ref()
        .ok_or(HistoryReadError::Unavailable(
            traits::HistoryUnavailableReason::UnsupportedStore,
        ))?;
    let (capture_enabled, observations) = controller
        .capture_observations(window.from, window.to)
        .await?;
    let settings = kernel::settings::load_settings(host.store.as_ref()).await;
    Ok(CaptureCoverage {
        window: facts.window,
        observed_at: facts.observed_at,
        capture_enabled,
        frame_id_high_water: facts.history.frame_id_high_water,
        oldest_frame: facts.oldest_frame,
        newest_frame: facts.newest_frame,
        newest_captured_at: facts.newest_captured_at,
        frames_stored: facts.frames_stored,
        frames_text_available: facts.frames_text_available,
        image_retention_days: settings.storage_retention_days,
        retention_scope: traits::RetentionScope::ImagesOnly,
        saved_report_retention: traits::SavedReportRetention {
            terminal_days: 90,
            exempt_phases: vec![
                traits::ReportRunPhase::Queued,
                traits::ReportRunPhase::Running,
                traits::ReportRunPhase::Blocked,
            ],
            exempt_delivery_states: vec![
                traits::ReportDeliveryState::Pending,
                traits::ReportDeliveryState::Sending,
                traits::ReportDeliveryState::UnknownDelivery,
                traits::ReportDeliveryState::Conflict,
            ],
            failed_delivery_with_pending_retry_exempt: true,
        },
        capture_observations: traits::CaptureObservations {
            observation_started_at: observations.observation_started_at,
            unavailable_before: observations.unavailable_before,
            truncated: observations.truncated,
            intervals: observations
                .intervals
                .into_iter()
                .map(|i| traits::CaptureInterruption {
                    start: i.start,
                    end: i.end,
                    reason: match i.reason {
                        kernel::CaptureInterruptionReason::CaptureStopped => {
                            traits::CaptureInterruptionReason::CaptureStopped
                        }
                        kernel::CaptureInterruptionReason::WorkerError => {
                            traits::CaptureInterruptionReason::WorkerError
                        }
                    },
                })
                .collect(),
        },
    })
}

async fn bare_frame(store: &SqliteStore, at: i64, app: Option<&str>, title: Option<&str>) -> i64 {
    store
        .insert_frame(NewFrame {
            captured_at: at,
            monitor_index: 2,
            width: 100,
            height: 100,
            image_path: "PRIVATE-do-not-expose.webp".into(),
            content_hash: format!("fixture-{at}"),
            app_hint: app.map(str::to_string),
            window_title: title.map(str::to_string),
            browser_url: Some("https://example.test/stored".into()),
            capture_trigger: None,
        })
        .await
        .unwrap()
}
async fn page(base: &str, query: &str) -> FramePage {
    let r = client()
        .get(format!("{base}/v1/frames?{query}"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.status());
    r.json().await.unwrap()
}
async fn coverage(base: &str, query: &str) -> CaptureCoverage {
    let r = client()
        .get(format!("{base}/v1/coverage?{query}"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    r.json().await.unwrap()
}

#[tokio::test]
async fn frames_route_serves_real_full_stored_text_and_explicit_nulls() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let full = "full-évidence\n\"\\\0😀".repeat(12_000);
    let id = seed_frame(&store, 1000, &full).await;
    let missing = bare_frame(&store, 1001, None, None).await;
    let empty = seed_frame(&store, 1002, "").await;
    let content = full.clone();
    let vision = "stored vision, not an inferred app label\n".repeat(12_000);
    store
        .insert_vision(
            id,
            traits::VisionAnalysis {
                description: vision.clone(),
                activity_type: None,
                app_hint: Some("INFERRED-LABEL-MUST-NOT-REPLACE-STORED-CONTEXT".into()),
                confidence: 1.0,
                model: "fixture".into(),
            },
        )
        .await
        .unwrap();
    let session = store
        .insert_session(new_session(
            0,
            Some(2000),
            SessionKind::Focus,
            None,
            None,
            "focus:fixture",
        ))
        .await
        .unwrap();
    store
        .assign_frames_session(&[id], Some(session))
        .await
        .unwrap();
    store.degrade_frame_to_text(missing).await.unwrap();
    let (base, server, _) = start_server(history_host(store.clone())).await;
    let r = client()
        .get(format!("{base}/v1/frames?from=1000&to=1003"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let raw: serde_json::Value = r.json().await.unwrap();
    assert_eq!(raw["items"][0]["raw_text"], full);
    assert_eq!(raw["items"][0]["content_text"], content);
    assert_eq!(raw["items"][0]["vision_description"], vision);
    assert_eq!(raw["items"][0]["session_id"], session);
    assert_eq!(raw["items"][0]["app_hint"], "VS Code");
    assert_eq!(raw["items"][0]["text_source"], "ocr");
    assert!(raw["items"][0]["filter_version"].is_number());
    assert_eq!(raw["items"][1]["text_state"], "not_available");
    for name in [
        "raw_text",
        "content_text",
        "text_source",
        "filter_version",
        "app_hint",
        "window_title",
        "vision_description",
        "session_id",
    ] {
        assert!(raw["items"][1].get(name).unwrap().is_null(), "{name}");
    }
    assert_eq!(raw["items"][1]["image_purged"], true);
    assert_eq!(raw["items"][2]["frame_id"], empty);
    assert_eq!(raw["items"][2]["text_state"], "available");
    assert_eq!(raw["items"][2]["raw_text"], "");
    assert_eq!(raw["consistency"], "bounded_membership_live_content");
    assert!(raw.get("next_cursor").unwrap().is_null());
    assert_eq!(raw["window"], serde_json::json!({"from":1000,"to":1003}));
    assert!(!raw.to_string().contains("PRIVATE-do-not-expose"));
    let typed: FramePage = serde_json::from_value(raw).unwrap();
    assert_eq!(typed.items[0].frame_id, id);
    assert_eq!(typed.items[0].raw_text.as_deref(), Some(full.as_str()));
    // Strict upper bound: no frame at to.
    assert_eq!(page(&base, "from=1000&to=1002").await.items.len(), 2);
    server.stop().await;
}

#[tokio::test]
async fn frames_drain_over_1000_equal_out_of_order_times_snapshot_late_inserts_and_live_updates() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let mut expected = Vec::new();
    for n in 0..1205 {
        let at = 100 + (n * 37 % 101);
        let id = bare_frame(&store, at, Some("Test"), Some("stable")).await;
        expected.push((at, id));
    }
    expected.sort_unstable();
    let (base, server, _) = start_server(history_host(store.clone())).await;
    let first = page(&base, "from=0&to=300&limit=1000").await;
    assert_eq!(first.items.len(), 1000);
    assert_eq!(first.snapshot_max_frame_id, 1205);
    let mut cursor = first.next_cursor.clone().unwrap();
    let first_retry = page(&base, &format!("cursor={cursor}&limit=1")).await;
    assert_eq!(
        (
            first_retry.items[0].captured_at,
            first_retry.items[0].frame_id
        ),
        expected[1000]
    );
    assert_eq!(
        page(&base, &format!("cursor={cursor}&limit=1")).await.items,
        first_retry.items
    );
    // Both backdated and equal-time production inserts are above the fixed membership bound.
    let late_backdated = bare_frame(&store, 1, Some("Test"), Some("stable")).await;
    let late_equal = bare_frame(&store, expected[1000].0, Some("Test"), Some("stable")).await;
    assert!(
        late_backdated > first.snapshot_max_frame_id && late_equal > first.snapshot_max_frame_id
    );
    let delete_after = expected[1001].1;
    let delete_consumed = expected[999].1;
    let update_id = expected[1000].1;
    store.delete_frame(delete_after).await.unwrap();
    store.delete_frame(delete_consumed).await.unwrap();
    store
        .insert_ocr(
            update_id,
            OcrResult {
                text: "late OCR, no snapshot promise".into(),
                mean_confidence: 1.0,
                engine: "fixture".into(),
                spans: vec![],
            },
        )
        .await
        .unwrap();
    let updated = page(&base, &format!("cursor={cursor}&limit=1")).await;
    assert_eq!(
        updated.items[0].raw_text.as_deref(),
        Some("late OCR, no snapshot promise")
    );
    let mut observed: Vec<_> = first
        .items
        .iter()
        .map(|i| (i.captured_at, i.frame_id))
        .collect();
    loop {
        let p = page(&base, &format!("cursor={cursor}&limit=71")).await;
        assert_eq!(p.history_id, first.history_id);
        assert_eq!(p.window, first.window);
        assert_eq!(p.snapshot_max_frame_id, first.snapshot_max_frame_id);
        observed.extend(p.items.into_iter().map(|i| (i.captured_at, i.frame_id)));
        match p.next_cursor {
            Some(c) => cursor = c,
            None => break,
        }
    }
    expected.retain(|(_, id)| *id != delete_after);
    // Deleting an already-consumed row does not revoke the fact the client observed it.
    assert_eq!(observed, expected);
    assert!(!observed
        .iter()
        .any(|(_, id)| [late_backdated, late_equal].contains(id)));
    assert!(page(&base, "from=0&to=300&limit=1")
        .await
        .items
        .iter()
        .any(|i| i.frame_id == late_backdated));
    server.stop().await;
}

#[tokio::test]
async fn cursor_progresses_when_all_remaining_members_deleted_terminal_null_and_empty_max_zero() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let (base, server, _) = start_server(history_host(store.clone())).await;
    let empty = page(&base, "from=-1&to=100").await;
    assert!(empty.items.is_empty() && empty.next_cursor.is_none());
    assert_eq!(empty.snapshot_max_frame_id, 0);
    for n in 1..=3 {
        bare_frame(&store, n, None, None).await;
    }
    let first = page(&base, "from=-1&to=100&limit=1").await;
    let cursor = first.next_cursor.unwrap();
    for id in 1..=3 {
        store.delete_frame(id).await.unwrap();
    }
    let terminal = page(&base, &format!("cursor={cursor}")).await;
    assert!(terminal.items.is_empty() && terminal.next_cursor.is_none());
    assert_eq!(terminal.snapshot_max_frame_id, 3);
    assert_eq!(terminal.history_id, empty.history_id);
    server.stop().await;
}

#[tokio::test]
async fn frame_filters_are_literal_ascii_only_untrimmed_target_metadata_and_control_safe() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let literal = bare_frame(
        &store,
        1,
        Some(" VS Code "),
        Some("prefix Lit_%\\\0É東京 suffix"),
    )
    .await;
    bare_frame(
        &store,
        2,
        Some("VS Code"),
        Some("prefix LitXYZÉ東京 suffix"),
    )
    .await;
    bare_frame(&store, 3, None, None).await;
    let target = bare_frame(&store, 4, Some("target.exe"), Some("selected_%")).await;
    let (base, server, _) = start_server(history_host(store.clone())).await;
    let c = client();
    for (app, title, expected) in [
        (" vS cODE ", "lIT_%\\\0É東京", vec![literal]),
        ("VS CODE", "LitXYZÉ東京", vec![2]),
        (" VS Code ", "é東京", vec![]), // SQLite folds ASCII, not accented Unicode.
        ("target.EXE", "selected_%", vec![target]),
        ("VS Code", "selected_%", vec![]), // No inference from unrelated app labels.
    ] {
        let r = c
            .get(format!("{base}/v1/frames"))
            .bearer_auth(TEST_TOKEN)
            .query(&[
                ("from", "0"),
                ("to", "10"),
                ("app", app),
                ("window_title", title),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let p: FramePage = r.json().await.unwrap();
        assert_eq!(
            p.items.iter().map(|i| i.frame_id).collect::<Vec<_>>(),
            expected,
            "{app:?} {title:?}"
        );
        if let Some(item) = p.items.iter().find(|i| i.frame_id == target) {
            assert_eq!(item.app_hint.as_deref(), Some("target.exe"));
            assert_eq!(item.window_title.as_deref(), Some("selected_%"));
        }
    }
    // Empty literal substring matches present titles (including empty), never NULL.
    assert_eq!(
        page(&base, "from=0&to=10&window_title=").await.items.len(),
        3
    );
    // Accepted maximum UTF8-byte filters survive a real selected page/cursor/resume.
    for filter in [
        "\0".repeat(256),
        "\"".repeat(256),
        "\\".repeat(256),
        "😀".repeat(64),
    ] {
        let a = bare_frame(&store, 20, Some("sized"), Some(&filter)).await;
        let b = bare_frame(&store, 20, Some("sized"), Some(&filter)).await;
        let r = c
            .get(format!("{base}/v1/frames"))
            .bearer_auth(TEST_TOKEN)
            .query(&[
                ("from", "20"),
                ("to", "21"),
                ("app", "SIZED"),
                ("window_title", filter.as_str()),
                ("limit", "1"),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let first: FramePage = r.json().await.unwrap();
        assert_eq!(first.items[0].frame_id, a);
        let cursor = first.next_cursor.unwrap();
        assert!(cursor.len() <= 4096);
        let last = page(&base, &format!("cursor={cursor}&limit=1000")).await;
        assert_eq!(
            last.items.iter().map(|i| i.frame_id).collect::<Vec<_>>(),
            vec![b]
        );
        assert!(last.next_cursor.is_none());
        store.delete_frame(a).await.unwrap();
        store.delete_frame(b).await.unwrap();
    }
    server.stop().await;
}

#[tokio::test]
async fn history_identity_mismatch_is_409_and_forged_max_above_allocator_is_400() {
    use base64::Engine as _;
    let store_a = Arc::new(SqliteStore::open_in_memory().unwrap());
    let store_b = Arc::new(SqliteStore::open_in_memory().unwrap());
    for store in [&store_a, &store_b] {
        for at in 1..=2 {
            bare_frame(store, at, None, None).await;
        }
    }
    let (a, server_a, _) = start_server(history_host(store_a)).await;
    let (b, server_b, _) = start_server(history_host(store_b)).await;
    let cursor = page(&a, "from=0&to=3&limit=1").await.next_cursor.unwrap();
    let r = client()
        .get(format!("{b}/v1/frames?cursor={cursor}"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 409);
    assert_eq!(
        r.json::<serde_json::Value>().await.unwrap()["error"],
        "history_changed"
    );
    let mut bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&cursor)
        .unwrap();
    bytes[53..61].copy_from_slice(&i64::MAX.to_be_bytes());
    let forged = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let r = client()
        .get(format!("{a}/v1/frames?cursor={forged}"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    server_a.stop().await;
    server_b.stop().await;
}

#[tokio::test]
async fn coverage_uses_real_store_controller_intent_policy_and_empty_is_not_inactivity() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    store
        .set_setting("storage.retention_days", "0")
        .await
        .unwrap();
    let host = history_host(store.clone());
    let controller = host.capture_kernel.as_ref().unwrap().clone();
    let (base, server, _) = start_server(host).await;
    let query = "from=0&to=9223372036854775807";
    let initial = coverage(&base, query).await;
    assert!(!initial.capture_enabled);
    assert_eq!(initial.frame_id_high_water, 0);
    assert!(
        initial.oldest_frame.is_none()
            && initial.newest_frame.is_none()
            && initial.newest_captured_at.is_none()
    );
    assert_eq!(
        (initial.frames_stored, initial.frames_text_available),
        (0, 0)
    );
    assert_eq!(initial.image_retention_days, 0);
    assert_eq!(initial.retention_scope, traits::RetentionScope::ImagesOnly);
    assert_eq!(initial.saved_report_retention.terminal_days, 90);
    assert_eq!(
        initial.saved_report_retention.exempt_phases,
        vec![
            traits::ReportRunPhase::Queued,
            traits::ReportRunPhase::Running,
            traits::ReportRunPhase::Blocked
        ]
    );
    assert_eq!(
        initial.saved_report_retention.exempt_delivery_states,
        vec![
            traits::ReportDeliveryState::Pending,
            traits::ReportDeliveryState::Sending,
            traits::ReportDeliveryState::UnknownDelivery,
            traits::ReportDeliveryState::Conflict
        ]
    );
    assert!(
        initial
            .saved_report_retention
            .failed_delivery_with_pending_retry_exempt
    );
    let observation = &initial.capture_observations;
    assert!(observation.truncated); // Query predates this process; unknown is explicit.
    assert_eq!(
        observation.observation_started_at,
        observation.unavailable_before
    );
    assert_eq!(observation.intervals.len(), 1);
    assert_eq!(
        observation.intervals[0].start,
        observation.observation_started_at
    );
    assert!(observation.intervals[0].end.is_none());
    assert_eq!(
        observation.intervals[0].reason,
        traits::CaptureInterruptionReason::CaptureStopped
    );
    controller.start_capture().await.unwrap();
    let enabled_empty = coverage(&base, query).await;
    assert!(enabled_empty.capture_enabled);
    assert_eq!(enabled_empty.frames_stored, 0); // Enabled intent never becomes an activity claim.
    let wire = serde_json::to_value(&enabled_empty).unwrap();
    for forbidden in ["activity", "inactivity", "NO_DATA", "image_path", "PRIVATE"] {
        assert!(!wire.to_string().contains(forbidden));
    }
    let newest = seed_frame(&store, 100, "").await;
    let oldest = bare_frame(&store, -10, None, None).await;
    let tied_newest = bare_frame(&store, 100, None, None).await;
    let current = coverage(&base, "from=100&to=101").await;
    assert_eq!(current.frames_stored, 2);
    assert_eq!(current.frames_text_available, 1); // Empty stored OCR counts, absent OCR doesn't.
    assert_eq!(
        current.oldest_frame,
        Some(traits::FrameReference {
            frame_id: oldest,
            captured_at: -10
        })
    );
    assert_eq!(
        current.newest_frame,
        Some(traits::FrameReference {
            frame_id: tied_newest,
            captured_at: 100
        })
    );
    assert_eq!(current.newest_captured_at, Some(100));
    assert!(tied_newest > newest);
    assert_eq!(current.frame_id_high_water, tied_newest);
    controller.stop_capture().await;
    let stopped = coverage(&base, query).await;
    assert!(!stopped.capture_enabled);
    assert_eq!(
        stopped.capture_observations.intervals.last().unwrap().end,
        None
    );
    server.stop().await;
}

#[tokio::test]
async fn typed_unavailable_and_private_storage_errors_are_safe_not_empty_data() {
    for fault in [HistoryFault::Unavailable, HistoryFault::Storage] {
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let before = store.history_readiness().await.unwrap();
        let mut host = history_host(store.clone());
        Arc::get_mut(&mut host).unwrap().history_fault = Some(fault);
        let (base, server, _) = start_server(host).await;
        for endpoint in ["frames", "coverage"] {
            let r = client()
                .get(format!("{base}/v1/{endpoint}?from=0&to=1"))
                .bearer_auth(TEST_TOKEN)
                .send()
                .await
                .unwrap();
            let expected = match fault {
                HistoryFault::Unavailable => 503,
                HistoryFault::Storage => 500,
            };
            assert_eq!(r.status(), expected);
            let value: serde_json::Value = r.json().await.unwrap();
            assert_eq!(
                value,
                if expected == 503 {
                    serde_json::json!({"error":"unavailable","message":"history feed unavailable"})
                } else {
                    serde_json::json!({"error":"internal","message":"history storage error"})
                }
            );
        }
        // Health remains compatible and cheap even while the feed cannot read storage.
        assert_eq!(
            client()
                .get(format!("{base}/v1/health"))
                .bearer_auth(TEST_TOKEN)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(store.history_readiness().await.unwrap(), before);
        server.stop().await;
    }
}

#[tokio::test]
async fn new_read_routes_require_explicit_auth_get_head_and_preserve_method_denials() {
    let host = history_host(Arc::new(SqliteStore::open_in_memory().unwrap()));
    let (base, server, _) = start_server(host).await;
    server.set_read_token(Some("history-read".into()));
    for token in [TEST_TOKEN, "history-read"] {
        for endpoint in ["frames", "coverage"] {
            for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
                let r = client()
                    .request(method.clone(), format!("{base}/v1/{endpoint}?from=0&to=1"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(r.status(), 200);
                if method == reqwest::Method::HEAD {
                    assert!(r.bytes().await.unwrap().is_empty());
                }
            }
            for method in [
                reqwest::Method::POST,
                reqwest::Method::PUT,
                reqwest::Method::DELETE,
            ] {
                assert_eq!(
                    client()
                        .request(method, format!("{base}/v1/{endpoint}?from=0&to=1"))
                        .bearer_auth(token)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    405
                );
            }
        }
    }
    server.stop().await;
}
