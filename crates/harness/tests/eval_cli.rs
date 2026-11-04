//! End-to-end proof without a real database, profile, model or token: a synthetic v14 database in
//! a temp directory plus a `wiremock` server replaying `/v1/health`, `/v1/search` and `/v1/ask`.
//!
//! The assertions are computed from the fixture (the generated pairs) and the mock's citation
//! plan, never from a stored expected answer.

mod common;

use std::path::Path;

use harness::eval::gen::{generate_pairs, GenOpts, Pair};
use harness::eval::{run_eval, Db, EvalOpts};
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SEED: u64 = 7;
const PAIRS: usize = 5;
/// A citation to a frame that does not exist, added to one turn to prove the validity path.
const BOGUS_FRAME: i64 = 999_999;

fn plan_pairs(path: &Path) -> Vec<Pair> {
    let db = Db::open(path).unwrap();
    generate_pairs(
        &db,
        &GenOpts {
            seed: SEED,
            pairs: PAIRS,
            // All fixture sessions are closed, so `now_ms` cannot change a pair.
            now_ms: 0,
            max_frames: None,
        },
    )
    .unwrap()
    .pairs
}

async fn mount_health(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/v1/health"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "version": "test-1.0",
            "uptime_secs": 12,
            "capturing": true,
        })))
        .mount(server)
        .await;
}

async fn mount_pair(server: &MockServer, pair: &Pair, index: usize) {
    // Retrieval arm: the pair's first expected frame is the top hit.
    let hits = match pair.expected_frames.first() {
        Some(id) => json!([{ "frame_id": id }]),
        None => json!([]),
    };
    Mock::given(method("GET"))
        .and(path("/v1/search"))
        .and(query_param("q", pair.question.as_str()))
        .and(query_param("limit", "10"))
        .and(query_param("include_chrome", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(hits))
        .mount(server)
        .await;

    // Answer arm. Even indexes use today's stream (per-frame `citation` events only); odd indexes
    // also carry the future terminal `citations` event with both arrays.
    let answer = pair
        .expected_strings
        .first()
        .cloned()
        .or_else(|| pair.expected_local_date.clone())
        .unwrap_or_default();
    let mut sse = String::new();
    let mut push = |value: serde_json::Value| {
        sse.push_str(&format!("data: {value}\n\n"));
    };
    push(json!({"type": "thinking", "text": "considering"}));
    push(json!({"type": "token", "text": format!("I saw {answer}.")}));
    for id in &pair.expected_frames {
        push(json!({"type": "citation", "frame_id": id}));
    }
    if index % 2 == 1 {
        push(json!({
            "type": "citations",
            "sessions": pair.expected_sessions,
            "frames": pair.expected_frames,
            "stage_counts": {"threads": 1, "sessions": pair.expected_sessions.len()},
            "tokens": 128,
        }));
    }
    if index == 0 {
        // Today's shape: the per-frame list is authoritative, so this extra citation survives to
        // the validity check. (In the future shape it would be replaced by the terminal arrays.)
        push(json!({"type": "citation", "frame_id": BOGUS_FRAME}));
    }
    push(json!({"type": "done"}));

    Mock::given(method("POST"))
        .and(path("/v1/ask"))
        .and(body_partial_json(json!({"query": pair.question})))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_complete_report_is_produced_and_its_numbers_match_the_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = common::rich_with_token(tmp.path());
    let pairs = plan_pairs(&db_path);
    assert_eq!(pairs.len(), PAIRS);

    let server = MockServer::start().await;
    mount_health(&server).await;
    for (i, pair) in pairs.iter().enumerate() {
        mount_pair(&server, pair, i).await;
    }

    let out = tmp.path().join("eval-out");
    let opts = EvalOpts {
        db: db_path.clone(),
        api: server.uri(),
        pairs: PAIRS,
        seed: Some(SEED),
        out: Some(out.clone()),
        top_k: None,
        timeout_secs: 30,
        max_frames: None,
    };
    let run = run_eval(&opts).await.expect("the run completes");

    // --- artifacts
    for name in ["report.md", "report.json", "pairs.jsonl", "runs.jsonl"] {
        assert!(out.join(name).exists(), "missing {name}");
    }
    let json_report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("report.json")).unwrap()).unwrap();
    assert_eq!(json_report["seed"], SEED);
    assert_eq!(json_report["pairs_generated"], PAIRS);
    assert_eq!(
        json_report["metrics"]["overall"]["pairs"],
        run.metrics.overall.pairs
    );
    assert_eq!(json_report["metrics"]["overall"]["invalid_ids"], 1);
    assert!(
        json_report["entity"]["recovery"].is_null(),
        "no entity surface => null recovery, never a fabricated zero"
    );
    let report = std::fs::read_to_string(out.join("report.md")).unwrap();
    assert_eq!(report, run.report);

    let pairs_lines: Vec<String> = std::fs::read_to_string(out.join("pairs.jsonl"))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(pairs_lines.len(), PAIRS);
    for (line, pair) in pairs_lines.iter().zip(&pairs) {
        let parsed: Pair = serde_json::from_str(line).unwrap();
        assert_eq!(&parsed, pair, "pairs.jsonl carries the generated pair");
    }
    let runs_lines: Vec<serde_json::Value> = std::fs::read_to_string(out.join("runs.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(runs_lines.len(), PAIRS);
    for line in &runs_lines {
        assert_eq!(
            line["terminal_event"]["type"], "done",
            "terminal event passed through"
        );
        assert!(line["pair_id"].is_string());
        assert!(line["cited_frames"].is_array());
        assert!(line["cited_sessions"].is_array());
    }

    // --- metrics
    let m = &run.metrics;
    assert_eq!(m.overall.pairs, PAIRS);
    assert_eq!(m.overall.turns_ok, PAIRS);
    assert_eq!(m.overall.failure_count(), 0);
    assert!(
        m.invalid.len() == 1,
        "exactly the one bogus citation is invalid"
    );
    assert_eq!(m.invalid[0].id, BOGUS_FRAME);
    assert_eq!(m.invalid[0].reason, "frame does not exist");

    // Every pair's answer contains its expected string, so the known-fact rate is 100%.
    assert_eq!(m.overall.known_fact_rate(), Some(1.0));
    // Retrieval: the mock returns the pair's first expected frame as the top hit.
    assert_eq!(m.overall.retrieval_recall10(), Some(1.0));

    // Validity: all expected ids are in-window; the bogus frame is the only invalid one.
    let cited_total = m.overall.cited_ids;
    assert!(cited_total > 1);
    assert_eq!(m.overall.invalid_ids, 1);
    assert!(
        (m.overall.citation_validity().unwrap() - (1.0 - 1.0 / cited_total as f64)).abs() < 1e-9
    );

    // Evidence recall: computed from the fixture and the citation plan, not from the code under
    // test. Even turns cite frames only (sessions match through their frames), odd turns cite both
    // arrays; the one bogus frame does not change the expected match set.
    let db = Db::open(&db_path).unwrap();
    let mut expected_sum = 0.0f64;
    for (i, pair) in pairs.iter().enumerate() {
        let expected = pair.expected_order();
        let mut cited_frames = pair.expected_frames.clone();
        if i == 0 {
            cited_frames.push(BOGUS_FRAME);
        }
        let cited_sessions: Vec<i64> = if i % 2 == 1 {
            pair.expected_sessions.clone()
        } else {
            Vec::new()
        };
        let frame_sessions: Vec<i64> = db
            .cited_frames(&cited_frames)
            .unwrap()
            .into_iter()
            .filter_map(|f| f.session_id)
            .collect();
        let matched = expected
            .iter()
            .take(10)
            .filter(|id| match id {
                harness::eval::gen::ExpectedId::Frame(f) => cited_frames.contains(f),
                harness::eval::gen::ExpectedId::Session(s) => {
                    cited_sessions.contains(s) || frame_sessions.contains(s)
                }
            })
            .count();
        expected_sum += matched as f64 / expected.len().min(10) as f64;
    }
    let expected_mean = expected_sum / PAIRS as f64;
    assert!(
        (m.overall.evidence_recall10().unwrap() - expected_mean).abs() < 1e-9,
        "recall {} != expected {expected_mean}",
        m.overall.evidence_recall10().unwrap()
    );

    // --- report content
    for needle in [
        "## Pair generation",
        "## Metrics",
        "Evidence recall@10",
        "Retrieval recall@10 (diagnostic)",
        "Citation validity",
        "Known fact present",
        "Latency p50 (ms)",
        "Latency p95 (ms)",
        "Gate A threshold",
        "## Invalid citations",
        "frame does not exist",
        "n/a (no entity surface)",
        "test-1.0",
    ] {
        assert!(report.contains(needle), "report is missing {needle:?}");
    }

    // --- a second run with the same seed reproduces the exact pair set
    let out2 = tmp.path().join("eval-out-2");
    let opts2 = EvalOpts {
        out: Some(out2.clone()),
        ..opts.clone()
    };
    let run2 = run_eval(&opts2).await.expect("the replay completes");
    assert_eq!(run2.pairs, run.pairs);
    assert_eq!(
        std::fs::read_to_string(out2.join("pairs.jsonl")).unwrap(),
        std::fs::read_to_string(out.join("pairs.jsonl")).unwrap(),
        "same (db, seed) => byte-identical pairs.jsonl"
    );
}

#[tokio::test]
async fn a_missing_token_fails_before_any_turn_with_a_settings_message() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = common::rich(tmp.path()); // no api.token row
    let server = MockServer::start().await;
    mount_health(&server).await;
    let opts = EvalOpts {
        db: db_path,
        api: server.uri(),
        pairs: 3,
        seed: Some(1),
        out: Some(tmp.path().join("out")),
        top_k: None,
        timeout_secs: 5,
        max_frames: None,
    };
    let error = run_eval(&opts).await.expect_err("no token => no run");
    let message = error.to_string();
    assert!(message.contains("Settings > Integrations"), "{message}");
    assert!(!message.contains("test-token"));
}

#[tokio::test]
async fn an_unreachable_api_fails_the_preflight_with_an_actionable_message() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = common::rich_with_token(tmp.path());
    // A port nothing listens on: bind then drop to get a free one.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let opts = EvalOpts {
        db: db_path,
        api: format!("http://127.0.0.1:{port}"),
        pairs: 3,
        seed: Some(1),
        out: Some(tmp.path().join("out")),
        top_k: None,
        timeout_secs: 5,
        max_frames: None,
    };
    let error = run_eval(&opts).await.expect_err("nothing is listening");
    let message = error.to_string();
    assert!(
        message.contains("not reachable") && message.contains("Settings > Integrations"),
        "{message}"
    );
}
