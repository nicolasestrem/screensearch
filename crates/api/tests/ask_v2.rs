//! Ask v2 over `POST /v1/ask` (0.4.5 P6, `03 §7i`).
//!
//! The route hands the turn to [`ApiHost::ask_turn`] and streams the returned
//! `AnswerDelta` channel through the same SSE encoding the legacy path uses; a host with
//! no Ask v2 pipeline (`Ok(None)`) falls back to the unchanged `ask_context` path. Every
//! expectation here is derived from the fixture the test builds: the session and frame
//! ids the store assigns, and the exact deltas the scripted host streams.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use api::{ApiHost, ApiServer, FrameImage, SharedToken};
use async_trait::async_trait;
use store::SqliteStore;
use tokio::sync::mpsc::Sender;
use traits::{
    AnswerDelta, AnswerOpts, AnswerProvider, AskTurnError, AskTurnRequest, AskTurnStream,
    ExportFrameRow, FrameDetail, NewFrame, NewSession, OcrResult, ResumeContext, RetrievedChunk,
    SessionKind, Settings, StageCounts, Store,
};

const TEST_TOKEN: &str =
    "ask-v2-test-token-0000000000000000000000000000000000000000000000000000000000";

/// The token text the fixture's answer provider streams on the legacy path.
const ANSWER_TEXT: &str = "grounded answer";

/// What the fixture host's Ask v2 pipeline does with a request.
enum Turn {
    /// A host without the pipeline: the route must fall back to `ask_context` unchanged.
    Absent,
    /// Stream exactly these deltas — the kernel's `Citation* , Citations, Done` tail and
    /// every pre-existing event, in the order the host emits them.
    Script(Vec<AnswerDelta>),
    /// Fail to start the turn; the route maps this before the first stream byte.
    Reject(Reject),
}

enum Reject {
    ThreadNotFound(i64),
    NoAnswerModel,
    Failed(&'static str),
}

/// Ids the fixture store assigned — never literals in the assertions.
#[derive(Clone, Copy)]
struct Ids {
    session: i64,
    frames: [i64; 2],
}

/// Streams one token, then cites every grounded frame, then a bare `done`: exactly the
/// legacy `/v1/ask` event shape, with no Ask v2 `citations` event.
struct ScriptedAnswer {
    calls: AtomicUsize,
}

#[async_trait]
impl AnswerProvider for ScriptedAnswer {
    async fn answer(
        &self,
        _query: &str,
        context: &[RetrievedChunk],
        _opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> anyhow::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let _ = tx
            .send(AnswerDelta::Token {
                text: ANSWER_TEXT.to_string(),
            })
            .await;
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

struct TestHost {
    store: Arc<SqliteStore>,
    ids: Ids,
    answer: Option<Arc<ScriptedAnswer>>,
    turn: Turn,
    /// Every request the route handed to `ask_turn`, in arrival order.
    turns: Mutex<Vec<AskTurnRequest>>,
    /// How often the legacy grounding path ran, so a test can prove the v2 branch does
    /// not also call `ask_context`.
    context_calls: AtomicUsize,
}

impl TestHost {
    /// The requests the route handed to `ask_turn`, in arrival order.
    fn turns(&self) -> Vec<AskTurnRequest> {
        self.turns
            .lock()
            .expect("turn log is never poisoned")
            .clone()
    }
}

#[async_trait]
impl ApiHost for TestHost {
    fn version(&self) -> String {
        "ask-v2-tests".to_string()
    }
    fn started_at_ms(&self) -> i64 {
        0
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
    async fn frame_image(&self, _frame_id: i64) -> anyhow::Result<Option<FrameImage>> {
        Ok(None)
    }
    async fn ask_context(
        &self,
        _query: &str,
        _top_k: u32,
        _session_id: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>> {
        self.context_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .ids
            .frames
            .iter()
            .enumerate()
            .map(|(index, frame_id)| RetrievedChunk {
                frame_id: *frame_id,
                text: format!("grounded frame {frame_id}"),
                score: 1.0 - index as f32 * 0.1,
                captured_at: 1_000 + index as i64,
                session: None,
            })
            .collect())
    }
    async fn ask_turn(
        &self,
        request: AskTurnRequest,
    ) -> Result<Option<AskTurnStream>, AskTurnError> {
        // Ask v2 ids are the host's business; the route forwards the stream as-is.
        let thread_id = request.thread_id.unwrap_or(NEW_THREAD);
        let turn_id = FIRST_TURN;
        self.turns
            .lock()
            .expect("turn log is never poisoned")
            .push(request);
        match &self.turn {
            Turn::Absent => Ok(None),
            Turn::Script(deltas) => {
                let (tx, rx) = tokio::sync::mpsc::channel(deltas.len().max(1));
                for delta in deltas {
                    tx.send(delta.clone())
                        .await
                        .expect("script buffer has room");
                }
                Ok(Some(AskTurnStream {
                    thread_id,
                    turn_id,
                    deltas: rx,
                }))
            }
            Turn::Reject(Reject::ThreadNotFound(id)) => Err(AskTurnError::ThreadNotFound(*id)),
            Turn::Reject(Reject::NoAnswerModel) => Err(AskTurnError::NoAnswerModel),
            Turn::Reject(Reject::Failed(message)) => {
                Err(AskTurnError::Failed(anyhow::anyhow!("{message}")))
            }
        }
    }
    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        self.answer.clone().map(|a| a as Arc<dyn AnswerProvider>)
    }
    async fn add_mark(
        &self,
        _frame_id: Option<i64>,
        _capture_now: bool,
        _note: Option<String>,
    ) -> anyhow::Result<i64> {
        anyhow::bail!("marks are not part of the ask v2 tests")
    }
    async fn resolve_mark(&self, _mark_id: i64) -> anyhow::Result<()> {
        Ok(())
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

/// The thread id the fixture host invents for a conversation the request did not name,
/// and the ordinal of its first turn.
const NEW_THREAD: i64 = 91;
const FIRST_TURN: i64 = 1;

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

/// One session holding two frames; the store assigns every id.
async fn seeded() -> (Arc<SqliteStore>, Ids) {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open store"));
    let session = store
        .insert_session(NewSession {
            started_at: 1_000,
            ended_at: Some(5_000),
            kind: SessionKind::Focus,
            tool: None,
            host: None,
            context_key: "focus:ask-v2".to_string(),
            confidence: 0.9,
            frozen: false,
        })
        .await
        .expect("insert session");
    let first = seed_frame(&store, 1_500, "deployment pipeline config").await;
    let second = seed_frame(&store, 2_500, "deployment pipeline rollback").await;
    store
        .assign_frames_session(&[first, second], Some(session))
        .await
        .expect("assign frames to the session");
    (
        store,
        Ids {
            session,
            frames: [first, second],
        },
    )
}

fn host(store: Arc<SqliteStore>, ids: Ids, turn: Turn, with_answer: bool) -> Arc<TestHost> {
    Arc::new(TestHost {
        store,
        ids,
        answer: with_answer.then(|| {
            Arc::new(ScriptedAnswer {
                calls: AtomicUsize::new(0),
            })
        }),
        turn,
        turns: Mutex::new(Vec::new()),
        context_calls: AtomicUsize::new(0),
    })
}

/// The `Citation* , Citations, Done` tail the kernel emits, plus the pre-existing events,
/// built from the fixture's ids and the conversation the test names.
fn scripted_turn(ids: Ids, thread_id: i64, turn_id: i64) -> Vec<AnswerDelta> {
    let stage_counts = StageCounts {
        threads: 1,
        sessions: 1,
        frames: 2,
    };
    let tokens_assembled = 42;
    vec![
        AnswerDelta::Thinking {
            text: "plan the turn".to_string(),
        },
        AnswerDelta::Token {
            text: ANSWER_TEXT.to_string(),
        },
        AnswerDelta::Citation {
            frame_id: ids.frames[0],
        },
        AnswerDelta::Citation {
            frame_id: ids.frames[1],
        },
        AnswerDelta::Citations {
            sessions: vec![ids.session],
            frames: vec![ids.frames[0], ids.frames[1]],
            thread_id,
            turn_id,
            stage_counts,
            tokens_assembled,
        },
        AnswerDelta::Done {
            stage_counts: Some(stage_counts),
            tokens_assembled: Some(tokens_assembled),
        },
    ]
}

async fn start(host: Arc<TestHost>) -> (String, ApiServer) {
    let token: SharedToken = Arc::new(std::sync::RwLock::new(TEST_TOKEN.to_string()));
    let server = ApiServer::start(host, token, 0)
        .await
        .expect("bind ephemeral loopback");
    let base = format!("http://127.0.0.1:{}", server.local_addr().port());
    (base, server)
}

async fn post_ask(base: &str, body: serde_json::Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base}/v1/ask"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {TEST_TOKEN}"),
        )
        .json(&body)
        .send()
        .await
        .expect("send ask")
}

/// The `data:` payloads of an SSE body, in order. Keep-alive comments (`:`) and the blank
/// separators between events are not payloads.
fn payloads(body: &str) -> Vec<serde_json::Value> {
    body.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|line| serde_json::from_str(line.trim_start()).expect("event data is JSON"))
        .collect()
}

/// The streamed deltas, decoded through the same type the route serialized.
fn deltas(body: &str) -> Vec<AnswerDelta> {
    payloads(body)
        .into_iter()
        .map(|value| serde_json::from_value(value).expect("payload is an AnswerDelta"))
        .collect()
}

#[tokio::test]
async fn ask_v2_streams_every_delta_and_one_citations_before_done() {
    let thread_id = 4_242;
    let turn_id = 7;
    let (store, ids) = seeded().await;
    let script = scripted_turn(ids, thread_id, turn_id);
    let host = host(store, ids, Turn::Script(script.clone()), true);

    let (base, server) = start(host.clone()).await;
    let response = post_ask(
        &base,
        serde_json::json!({
            "query": "what changed in the deployment pipeline",
            "thread_id": thread_id,
        }),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/event-stream"),
        "the turn streams as SSE"
    );
    let body = response.text().await.unwrap();

    let streamed = deltas(&body);
    assert_eq!(
        streamed, script,
        "every scripted delta reaches the wire unchanged and in order"
    );

    // Exactly one citations event, immediately before done (the kernel's terminal order).
    let citations = streamed
        .iter()
        .filter(|delta| matches!(delta, AnswerDelta::Citations { .. }))
        .count();
    assert_eq!(citations, 1, "one citations event: {body}");
    assert!(matches!(
        streamed[streamed.len() - 2],
        AnswerDelta::Citations { .. }
    ));
    let AnswerDelta::Done {
        stage_counts,
        tokens_assembled,
    } = streamed[streamed.len() - 1].clone()
    else {
        panic!("the turn ends with done: {body}");
    };
    let AnswerDelta::Citations {
        stage_counts: cited_counts,
        tokens_assembled: cited_tokens,
        ..
    } = streamed[streamed.len() - 2].clone()
    else {
        unreachable!("checked above");
    };
    assert_eq!(stage_counts, Some(cited_counts));
    assert_eq!(tokens_assembled, Some(cited_tokens));

    // The v2 branch answers from the turn, never from the legacy grounding path.
    assert_eq!(host.context_calls.load(Ordering::SeqCst), 0);
    let provider = host.answer.as_ref().expect("fixture provider");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);

    server.stop().await;
}

#[tokio::test]
async fn ask_v2_host_without_a_pipeline_keeps_the_legacy_event_sequence() {
    let (store, ids) = seeded().await;
    let host = host(store, ids, Turn::Absent, true);

    let (base, server) = start(host.clone()).await;
    let response = post_ask(&base, serde_json::json!({ "query": "deployment pipeline" })).await;
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();

    // Today's shape: one token, one citation per grounded frame, then a bare done — no
    // `citations` event and no diagnostics on done.
    let mut expected = vec![AnswerDelta::Token {
        text: ANSWER_TEXT.to_string(),
    }];
    for frame_id in ids.frames {
        expected.push(AnswerDelta::Citation { frame_id });
    }
    expected.push(AnswerDelta::done());
    assert_eq!(deltas(&body), expected, "legacy stream unchanged: {body}");

    assert_eq!(host.context_calls.load(Ordering::SeqCst), 1);
    let provider = host.answer.as_ref().expect("fixture provider");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);

    // The host still received the turn request (the route always offers it), and the
    // absent thread stays absent.
    let turns = host.turns();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].thread_id, None);

    server.stop().await;
}

#[tokio::test]
async fn ask_v2_thread_and_options_reach_the_host_verbatim() {
    let (store, ids) = seeded().await;
    let host = host(store, ids, Turn::Absent, true);

    let (base, server) = start(host.clone()).await;
    let response = post_ask(
        &base,
        serde_json::json!({
            "query": "and the rollback?",
            "thread_id": 4_242,
            "session_id": ids.session,
            "top_k": 7,
            "thinking": false,
            "max_tokens": 111,
        }),
    )
    .await;
    assert_eq!(response.status(), 200);
    let _ = response.text().await.unwrap();

    let turns = host.turns();
    assert_eq!(turns.len(), 1);
    let request = &turns[0];
    assert_eq!(request.query, "and the rollback?");
    assert_eq!(
        request.thread_id,
        Some(4_242),
        "thread_id forwards verbatim"
    );
    assert_eq!(request.session_id, Some(ids.session));
    assert_eq!(request.top_k, 7);
    assert!(!request.opts.thinking);
    assert_eq!(request.opts.max_tokens, 111);

    server.stop().await;
}

#[tokio::test]
async fn ask_v2_absent_thread_id_starts_a_new_conversation() {
    let (store, ids) = seeded().await;
    let host = host(store, ids, Turn::Absent, true);

    let (base, server) = start(host.clone()).await;
    let response = post_ask(&base, serde_json::json!({ "query": "invoice" })).await;
    assert_eq!(response.status(), 200);
    let _ = response.text().await.unwrap();

    let turns = host.turns();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].thread_id, None);

    server.stop().await;
}

#[tokio::test]
async fn ask_v2_thread_not_found_is_404_before_any_stream() {
    let unknown = 7_777;
    let (store, ids) = seeded().await;
    let host = host(
        store,
        ids,
        Turn::Reject(Reject::ThreadNotFound(unknown)),
        true,
    );

    let (base, server) = start(host.clone()).await;
    let response = post_ask(
        &base,
        serde_json::json!({ "query": "continue", "thread_id": unknown }),
    )
    .await;
    assert_eq!(response.status(), 404);
    assert_json_content_type(&response);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "not_found");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains(&unknown.to_string()),
        "the body names the missing conversation: {body}"
    );

    server.stop().await;
}

#[tokio::test]
async fn ask_v2_no_answer_model_is_503_before_any_stream() {
    let (store, ids) = seeded().await;
    let host = host(store, ids, Turn::Reject(Reject::NoAnswerModel), true);

    let (base, server) = start(host.clone()).await;
    let response = post_ask(&base, serde_json::json!({ "query": "continue" })).await;
    assert_eq!(response.status(), 503);
    assert_json_content_type(&response);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "unavailable");

    server.stop().await;
}

#[tokio::test]
async fn ask_v2_start_failure_is_500_before_any_stream() {
    let reason = "retrieval exploded";
    let (store, ids) = seeded().await;
    let host = host(store, ids, Turn::Reject(Reject::Failed(reason)), true);

    let (base, server) = start(host.clone()).await;
    let response = post_ask(&base, serde_json::json!({ "query": "continue" })).await;
    assert_eq!(response.status(), 500);
    assert_json_content_type(&response);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "internal");
    assert!(
        body["message"].as_str().unwrap().contains(reason),
        "the failure reason is reported: {body}"
    );

    server.stop().await;
}

#[tokio::test]
async fn ask_v2_session_check_precedes_the_model_check_and_the_turn() {
    // No answer provider at all: only the session check can turn this into a 404.
    let (store, ids) = seeded().await;
    let host = host(store, ids, Turn::Reject(Reject::NoAnswerModel), false);
    let (base, server) = start(host.clone()).await;

    let unknown = ids.session + 100;
    let response = post_ask(
        &base,
        serde_json::json!({ "query": "invoice", "session_id": unknown }),
    )
    .await;
    assert_eq!(response.status(), 404, "unknown session wins over 503");
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "not_found");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains(&format!("session {unknown}")));

    // Absent session_id keeps today's order too: the model check answers first.
    let response = post_ask(&base, serde_json::json!({ "query": "invoice" })).await;
    assert_eq!(response.status(), 503, "no model is 503 without a session");

    // Neither request may reach the turn pipeline.
    assert!(
        host.turns().is_empty(),
        "the route rejects before starting a turn"
    );

    server.stop().await;
}

fn assert_json_content_type(response: &reqwest::Response) {
    assert!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/json"),
        "a start failure is a JSON error, never a stream"
    );
}
