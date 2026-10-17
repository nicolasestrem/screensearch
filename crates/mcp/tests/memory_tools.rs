//! Protocol-fixture tests for the memory read tools (`list_frames`, `get_capture_coverage`).
//!
//! These drive the real `screensearch-mcp` binary over stdio against a **raw HTTP fixture**
//! that speaks the frozen `03 §7f.1`/`§7f.2` wire contract (the exact JSON shapes the parent
//! P1 API serves: `FramePage`/`CaptureCoverage`). They verify the MCP's translation and
//! forwarding, the shared read-only allowlist, and full-fidelity passthrough (no truncation,
//! no auto-drain, unknowns preserved).
//!
//! They are protocol-fixture evidence, **not** real-product acceptance: this branch's base
//! API does not have the `/v1/frames` or `/v1/coverage` routes, so the real end-to-end
//! consumer run against the P1 stack is a separate, still-pending gate.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const TEST_TOKEN: &str = "memory-fixture-token";
const RECV_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Raw HTTP fixture
// ---------------------------------------------------------------------------

/// One request as the fixture saw it (request line + query only; enough to pin forwarding).
#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    query: String,
}

type Responder = Box<dyn Fn(&str, &str, &str) -> (u16, &'static str, Vec<u8>) + Send + Sync>;

/// A minimal raw-HTTP/1.1 fixture: accepts connections on an ephemeral loopback port,
/// records every request, and answers from a caller-provided responder. `Connection: close`
/// after each response keeps the client from reusing a socket the fixture will not serve.
struct MemoryFixture {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MemoryFixture {
    fn start(responder: Responder) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("fixture addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let requests_thread = Arc::clone(&requests);
        let stop_thread = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !stop_thread.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // A listener that is nonblocking for the accept-poll loop makes
                        // macOS/BSD `accept()` hand back a socket that inherits O_NONBLOCK
                        // (Linux does not inherit it). Left as-is, a read before the client's
                        // bytes arrive — or a write larger than the send buffer, like the
                        // multi-hundred-KB frame page — fails with `WouldBlock` (EAGAIN,
                        // errno 35) instead of blocking. Put the accepted stream into
                        // blocking mode before any BufReader/read/write; the 10 s read
                        // timeout in `read_request` and the peer-driven writes keep every
                        // blocking read/write bounded, and the listener stays nonblocking so
                        // the accept-poll loop still honors `stop`.
                        stream
                            .set_nonblocking(false)
                            .expect("accepted stream blocking");
                        let (method, path, query) = read_request(&mut stream);
                        requests_thread.lock().unwrap().push(RecordedRequest {
                            method: method.clone(),
                            path: path.clone(),
                            query: query.clone(),
                        });
                        let (status, content_type, body) = responder(&method, &path, &query);
                        respond(&mut stream, status, content_type, &body);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            addr,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Waits until at least `n` requests were recorded (or the timeout elapses) and returns
    /// the snapshot — used before count assertions so a missing request fails fast instead
    /// of racing the child process.
    fn wait_for_requests(&self, n: usize, timeout: Duration) -> Vec<RecordedRequest> {
        let deadline = Instant::now() + timeout;
        loop {
            let recorded = self.requests();
            if recorded.len() >= n || Instant::now() >= deadline {
                return recorded;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for MemoryFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Reads one full request (request line + headers + any Content-Length body) so the
/// connection never closes with unread bytes (which would RST instead of a clean FIN).
fn read_request(stream: &mut TcpStream) -> (String, String, String) {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("request line");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), query.to_string()),
        None => (target, String::new()),
    };
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("header line");
        if line.trim().is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).expect("request body");
    }
    (method, path, query)
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).expect("write head");
    stream.write_all(body).expect("write body");
    stream.flush().expect("flush response");
}

fn json_body(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serialize fixture body")
}

/// Decodes one `application/x-www-form-urlencoded` component (`+` and `%XX`).
fn decode_component(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_map(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode_component(key), decode_component(value))
        })
        .collect()
}

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// MCP child driver
// ---------------------------------------------------------------------------

/// A minimal stdio driver for the compiled binary, with CLI args. The child gets only the
/// environment passed here — the process-global environment is never mutated, so tests stay
/// parallel-safe.
struct McpChild {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: i64,
}

impl McpChild {
    fn spawn(base: &str, token: Option<&str>, args: &[&str]) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_screensearch-mcp"));
        cmd.args(args)
            .env("SCREENSEARCH_API_URL", base)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        match token {
            Some(token) => {
                cmd.env("SCREENSEARCH_API_TOKEN", token);
            }
            None => {
                cmd.env_remove("SCREENSEARCH_API_TOKEN");
            }
        }
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "NO_PROXY",
            "no_proxy",
        ] {
            cmd.env_remove(key);
        }
        let mut child = cmd.spawn().expect("spawn screensearch-mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            rx,
            next_id: 0,
        }
    }

    fn send(&mut self, msg: &Value) {
        let mut line = serde_json::to_string(msg).expect("serialize request");
        line.push('\n');
        let stdin = self.stdin.as_mut().expect("child stdin still open");
        stdin.write_all(line.as_bytes()).expect("write request");
        stdin.flush().expect("flush request");
    }

    fn recv(&mut self) -> Value {
        let line = self
            .rx
            .recv_timeout(RECV_TIMEOUT)
            .expect("response line within timeout");
        serde_json::from_str(&line).expect("response is valid JSON")
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let resp = self.recv();
        assert_eq!(resp["id"], json!(id), "response id must match: {resp}");
        resp
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }));
    }

    fn handshake(&mut self) -> Value {
        let resp = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "memory-tools-test", "version": "1" }
            }),
        );
        assert!(resp.get("error").is_none(), "initialize failed: {resp}");
        self.notify("notifications/initialized");
        resp["result"].clone()
    }

    fn call_tool(&mut self, name: &str, args: Value) -> Value {
        let resp = self.request("tools/call", json!({ "name": name, "arguments": args }));
        assert!(
            resp.get("error").is_none(),
            "tools/call {name} was a protocol error: {resp}"
        );
        resp["result"].clone()
    }

    fn close_and_wait(mut self) -> i32 {
        drop(self.stdin.take());
        self.child.wait().expect("child exit").code().unwrap_or(-1)
    }
}

impl Drop for McpChild {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn first_text(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

// ---------------------------------------------------------------------------
// Contract-shaped fixture data (03 §7f.1 / §7f.2)
// ---------------------------------------------------------------------------

fn frame_item(frame_id: i64, captured_at: i64, raw: &str, content: &str, vision: &str) -> Value {
    json!({
        "frame_id": frame_id,
        "captured_at": captured_at,
        "monitor_index": 0,
        "app_hint": "VS Code",
        "window_title": "main.rs",
        "browser_url": null,
        "raw_text": raw,
        "content_text": content,
        "text_source": "ocr",
        "filter_version": 2,
        "text_state": "available",
        "vision_description": vision,
        "session_id": 7,
        "image_purged": false
    })
}

/// Serves `GET /v1/frames` from a small page table keyed by cursor (`None` = initial page);
/// an unknown cursor or missing cursor is the API's own 400, exactly like the real route.
fn frames_responder(pages: Vec<(Option<&'static str>, Value)>) -> Responder {
    Box::new(move |_method, path, query| {
        if path != "/v1/frames" {
            return (
                404,
                "application/json",
                br#"{"error":"not_found","message":"no route"}"#.to_vec(),
            );
        }
        let cursor = query_map(query).get("cursor").cloned();
        for (key, page) in &pages {
            if *key == cursor.as_deref() {
                return (200, "application/json", json_body(page));
            }
        }
        (
            400,
            "application/json",
            br#"{"error":"bad_request","message":"invalid history query or cursor"}"#.to_vec(),
        )
    })
}

/// A full `CaptureCoverage` body including explicit unknowns (truncated observations,
/// `unavailable_before`, an open interval) and two fields the frozen contract does not
/// define yet — a faithful passthrough must preserve all of them verbatim.
fn sample_coverage() -> Value {
    json!({
        "window": { "from": 1000, "to": 2000 },
        "observed_at": 1_700_000_000_000i64,
        "capture_enabled": true,
        "frame_id_high_water": 42,
        "oldest_frame": { "frame_id": 3, "captured_at": 1100 },
        "newest_frame": { "frame_id": 42, "captured_at": 1900 },
        "newest_captured_at": 1900,
        "frames_stored": 5,
        "frames_text_available": 4,
        "image_retention_days": 30,
        "retention_scope": "images_only",
        "saved_report_retention": {
            "terminal_days": 90,
            "exempt_phases": ["queued", "running", "blocked"],
            "exempt_delivery_states": ["pending", "sending", "conflict"],
            "failed_delivery_with_pending_retry_exempt": true
        },
        "capture_observations": {
            "observation_started_at": 500,
            "unavailable_before": 400,
            "intervals": [
                { "start": 600, "end": null, "reason": "capture_stopped" },
                { "start": 700, "end": 750, "reason": "worker_error" }
            ],
            "truncated": true
        },
        "future_extension": { "nested": [1, 2, 3], "note": "unknown fields survive" },
        "another_unknown": null
    })
}

fn empty_page() -> Value {
    json!({
        "items": [],
        "next_cursor": null,
        "history_id": "00000000-0000-4000-8000-000000000000",
        "window": { "from": 1000, "to": 2000 },
        "snapshot_max_frame_id": 0,
        "observed_at": 1_700_000_000_000i64,
        "consistency": "bounded_membership_live_content"
    })
}

// ---------------------------------------------------------------------------
// Report fixture data (03 §7f.5 / §7f.6) — authored from the normative wire types
// (spec + `traits::SavedReport`/`MemoryReportRange`), never captured from a running API.
// ---------------------------------------------------------------------------

/// One `ReportSummary` list row: metadata only — no saved Markdown/evidence arrays.
fn report_summary(run_id: &str, phase: &str, outcome: Value) -> Value {
    json!({
        "run_id": run_id,
        "source_id": "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa",
        "kind": "daily",
        "revision_of": null,
        "requested_range": { "start_date": "2026-10-12", "end_date": "2026-10-17" },
        "window_start": 1_000,
        "window_end": 2_000,
        "timezone": "Europe/Paris",
        "local_date": "2026-10-17",
        "created_at": 1_700_000_000_000i64,
        "updated_at": 1_700_000_100_000i64,
        "phase": phase,
        "outcome": outcome,
        "error_code": null
    })
}

/// A `ReportPage`: `{ items, next_cursor }` with an explicit (possibly null) terminal cursor.
fn report_page(items: Vec<Value>, next_cursor: Value) -> Value {
    json!({ "items": items, "next_cursor": next_cursor })
}

/// A complete `SavedReport` (spec §7f.6 / `traits::SavedReport`). `frame_ids` deliberately
/// exceeds the 200-item display cap so a substituted citation cap would be caught.
fn saved_report() -> Value {
    json!({
        "markdown": "# Weekly recall\n\nBody line one.\nBody line two with a `code` span.\n",
        "model": "qwen2.5-7b-instruct",
        "frame_ids": (1..=250).collect::<Vec<i64>>(),
        "session_ids": (1..=12).collect::<Vec<i64>>(),
        "cited_frame_ids": (1..=200).collect::<Vec<i64>>(),
        "frames_sampled": 250,
        "frames_summarized": 250,
        "periods_total": 5,
        "periods_covered": 5,
        "passes": 2,
        "truncated": false
    })
}

/// The factual saved result a stored NO_DATA run carries: a truthful note and no evidence.
fn no_data_saved_report() -> Value {
    json!({
        "markdown": "No screen activity was recorded for the requested window.",
        "model": null,
        "frame_ids": [],
        "session_ids": [],
        "cited_frame_ids": [],
        "frames_sampled": 0,
        "frames_summarized": 0,
        "periods_total": 5,
        "periods_covered": 0,
        "passes": 1,
        "truncated": false
    })
}

/// A `ReportDetail`: every `ReportSummary` field plus the optional saved result.
fn report_detail(run_id: &str, phase: &str, outcome: Value, result: Value) -> Value {
    let mut detail = report_summary(run_id, phase, outcome);
    detail["result"] = result;
    detail
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The two new read tools are listed in normal (11) and read-only (8) mode with schemas that
/// match the wire contract; read-only mode accepts them while still refusing mutators, Ask
/// and unknown names in-band — before any HTTP client is entered.
#[test]
fn new_read_tools_are_listed_with_accurate_schemas_and_read_only_allowlist() {
    let page = empty_page();
    let coverage = sample_coverage();
    let report_rows = report_page(
        vec![report_summary("r1", "finished", json!("OK"))],
        json!(null),
    );
    let report_one = report_detail("r1", "finished", json!("OK"), json!(null));
    let fixture = MemoryFixture::start(Box::new(move |_method, path, _query| match path {
        "/v1/frames" => (200, "application/json", json_body(&page)),
        "/v1/coverage" => (200, "application/json", json_body(&coverage)),
        "/v1/reports" => (200, "application/json", json_body(&report_rows)),
        "/v1/reports/r1" => (200, "application/json", json_body(&report_one)),
        _ => (
            404,
            "application/json",
            br#"{"error":"not_found","message":"no route"}"#.to_vec(),
        ),
    }));

    // Normal mode: eleven tools, accurate schemas on the two new ones.
    let mut normal = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    normal.handshake();
    let listed = normal.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec![
            "search_screen_history",
            "ask_screen_history",
            "get_moment",
            "list_frames",
            "get_capture_coverage",
            "list_reports",
            "get_report",
            "where_was_i",
            "list_marks",
            "add_mark",
            "list_sessions",
            "get_session",
            "ask_session",
        ],
        "normal mode lists every tool"
    );
    let by_name = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap();

    let frames = by_name("list_frames");
    let frames_props = &frames["inputSchema"]["properties"];
    assert_eq!(frames_props["from"]["type"], "integer");
    assert_eq!(frames_props["to"]["type"], "integer");
    assert_eq!(frames_props["limit"]["type"], "integer");
    assert_eq!(frames_props["limit"]["minimum"], 1);
    assert_eq!(frames_props["limit"]["maximum"], 1000);
    assert_eq!(frames_props["app"]["type"], "string");
    assert_eq!(frames_props["window_title"]["type"], "string");
    assert_eq!(frames_props["cursor"]["type"], "string");
    assert_eq!(
        frames["inputSchema"]["anyOf"],
        json!([
            { "required": ["from", "to"] },
            { "required": ["cursor"] }
        ]),
        "the initial/continuation combination is expressed in the schema"
    );
    assert_eq!(frames["inputSchema"]["additionalProperties"], false);

    let coverage_tool = by_name("get_capture_coverage");
    assert_eq!(
        coverage_tool["inputSchema"]["required"],
        json!(["from", "to"])
    );
    assert_eq!(
        coverage_tool["inputSchema"]["properties"]["from"]["type"],
        "integer"
    );
    assert_eq!(
        coverage_tool["inputSchema"]["properties"]["to"]["type"],
        "integer"
    );
    assert_eq!(coverage_tool["inputSchema"]["additionalProperties"], false);

    // The report read tools carry accurate schemas (03 §7f.6): list_reports mirrors the
    // frame page (initial from/to vs continuation cursor, 1..=1000 limit) with literal
    // phase/outcome strings; get_report takes an opaque string run_id.
    let list_reports = by_name("list_reports");
    let lr_props = &list_reports["inputSchema"]["properties"];
    assert_eq!(lr_props["from"]["type"], "integer");
    assert_eq!(lr_props["to"]["type"], "integer");
    assert_eq!(lr_props["limit"]["type"], "integer");
    assert_eq!(lr_props["limit"]["minimum"], 1);
    assert_eq!(lr_props["limit"]["maximum"], 1000);
    assert_eq!(lr_props["phase"]["type"], "string");
    assert_eq!(
        lr_props["phase"]["enum"],
        json!(["queued", "running", "blocked", "finished"])
    );
    assert_eq!(lr_props["outcome"]["type"], "string");
    assert_eq!(
        lr_props["outcome"]["enum"],
        json!(["OK", "PARTIAL", "NO_DATA", "BLOCKED", "FAILED"])
    );
    assert_eq!(lr_props["cursor"]["type"], "string");
    assert_eq!(
        list_reports["inputSchema"]["anyOf"],
        json!([
            { "required": ["from", "to"] },
            { "required": ["cursor"] }
        ]),
        "the initial/continuation combination is expressed in the schema"
    );
    assert_eq!(list_reports["inputSchema"]["additionalProperties"], false);

    let get_report = by_name("get_report");
    assert_eq!(get_report["inputSchema"]["required"], json!(["run_id"]));
    assert_eq!(
        get_report["inputSchema"]["properties"]["run_id"]["type"],
        "string"
    );
    assert_eq!(get_report["inputSchema"]["additionalProperties"], false);

    // Normal mode keeps the retained behavior: an unknown name is a protocol error.
    let unknown = normal.request(
        "tools/call",
        json!({ "name": "future_tool", "arguments": {} }),
    );
    assert_eq!(unknown["error"]["code"], -32602, "{unknown}");

    // Read-only mode: exactly the ten read tools, the report reads included.
    let mut read_only = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &["--read-only"]);
    read_only.handshake();
    let listed = read_only.request("tools/list", json!({}));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "search_screen_history",
            "get_moment",
            "list_frames",
            "get_capture_coverage",
            "list_reports",
            "get_report",
            "where_was_i",
            "list_marks",
            "list_sessions",
            "get_session",
        ],
        "read-only mode lists the shared allowlist"
    );

    // Every read tool — including the report reads — is accepted in read-only mode and
    // reaches the fixture.
    let frames_result = read_only.call_tool("list_frames", json!({ "from": 1000, "to": 2000 }));
    assert_ne!(frames_result["isError"], json!(true), "{frames_result}");
    let coverage_result =
        read_only.call_tool("get_capture_coverage", json!({ "from": 1000, "to": 2000 }));
    assert_ne!(coverage_result["isError"], json!(true), "{coverage_result}");
    let reports_result = read_only.call_tool("list_reports", json!({ "from": 1000, "to": 2000 }));
    assert_ne!(reports_result["isError"], json!(true), "{reports_result}");
    let report_result = read_only.call_tool("get_report", json!({ "run_id": "r1" }));
    assert_ne!(report_result["isError"], json!(true), "{report_result}");
    let recorded = fixture.wait_for_requests(4, Duration::from_secs(10));
    assert_eq!(
        recorded.len(),
        4,
        "read-only calls reached the API: {recorded:?}"
    );
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/frames");
    assert_eq!(recorded[1].path, "/v1/coverage");
    assert_eq!(recorded[2].path, "/v1/reports");
    assert_eq!(recorded[3].path, "/v1/reports/r1");

    // Read-only refuses mutators, Ask and unknown names — in-band, before any HTTP client
    // is entered. (The report reads are now classified reads, so they are no longer here.)
    for name in [
        "add_mark",
        "ask_screen_history",
        "ask_session",
        "future_tool",
    ] {
        let result = read_only.call_tool(
            name,
            json!({ "query": "x", "session_id": 1, "frame_id": 1 }),
        );
        assert_eq!(result["isError"], json!(true), "{name}: {result}");
        let message = first_text(&result);
        assert!(message.contains("read-only"), "{name}: {message}");
        assert!(
            !message.contains("enable the API"),
            "{name} must be refused before HTTP: {message}"
        );
    }
    thread::sleep(Duration::from_millis(150));
    assert_eq!(
        fixture.requests().len(),
        4,
        "denied read-only calls must not reach the API"
    );

    assert_eq!(normal.close_and_wait(), 0);
    assert_eq!(read_only.close_and_wait(), 0);
}

/// The whole structured page is preserved: full raw/content/vision text (no snippet
/// truncation), structured item objects, `next_cursor` exactly as served (including the
/// terminal `null`), exact query forwarding for both the initial and continuation forms,
/// and no auto-drain (one call = one page fetched).
#[test]
fn list_frames_preserves_cursor_and_full_content() {
    let raw1 = "RAW-ONE: the full raw OCR text of the first page — never a snippet.";
    let content1 = "FILTERED-ONE: the full filtered text of the first page.";
    let vision1 = "VISION-ONE: a terminal showing cargo test output.";
    let raw2 = "RAW-TWO: the full raw text of the second page.";
    let content2 = "FILTERED-TWO: the full filtered text of the second page.";
    let vision2 = "VISION-TWO: a browser window with a documentation page.";

    let page1 = json!({
        "items": [
            frame_item(1, 1500, raw1, content1, vision1),
            frame_item(2, 1750, "raw two", "content two", "vision two")
        ],
        "next_cursor": "CURSOR-PAGE-2",
        "history_id": "11111111-2222-4333-8444-555555555555",
        "window": { "from": 1000, "to": 2000 },
        "snapshot_max_frame_id": 2,
        "observed_at": 1_700_000_000_001i64,
        "consistency": "bounded_membership_live_content"
    });
    let page2 = json!({
        "items": [frame_item(3, 1800, raw2, content2, vision2)],
        "next_cursor": null,
        "history_id": "11111111-2222-4333-8444-555555555555",
        "window": { "from": 1000, "to": 2000 },
        "snapshot_max_frame_id": 2,
        "observed_at": 1_700_000_000_002i64,
        "consistency": "bounded_membership_live_content"
    });
    let fixture = MemoryFixture::start(frames_responder(vec![
        (None, page1.clone()),
        (Some("CURSOR-PAGE-2"), page2.clone()),
    ]));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    // Initial form: from/to/app/window_title/limit are forwarded exactly — nothing else.
    let result = mcp.call_tool(
        "list_frames",
        json!({ "from": 1000, "to": 2000, "app": "VS Code", "window_title": "main.rs", "limit": 1 }),
    );
    assert_ne!(result["isError"], json!(true), "{result}");
    let recorded = fixture.wait_for_requests(1, Duration::from_secs(10));
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/frames");
    assert_eq!(
        query_map(&recorded[0].query),
        params(&[
            ("from", "1000"),
            ("to", "2000"),
            ("app", "VS Code"),
            ("window_title", "main.rs"),
            ("limit", "1"),
        ]),
        "the initial query forwards exactly the documented parameters"
    );

    let text = first_text(&result);
    let page: Value = serde_json::from_str(&text).expect("tool text is the page JSON");
    assert_eq!(page, page1, "the whole structured page is preserved");
    assert_eq!(page["next_cursor"], json!("CURSOR-PAGE-2"));
    assert!(
        text.contains("\"next_cursor\": \"CURSOR-PAGE-2\""),
        "cursor preserved verbatim: {text}"
    );
    // Full content: raw/content/vision compare exactly — no shortening for a context budget.
    assert_eq!(page["items"][0]["raw_text"], json!(raw1));
    assert_eq!(page["items"][0]["content_text"], json!(content1));
    assert_eq!(page["items"][0]["vision_description"], json!(vision1));
    // Structured objects and explicit nulls survive too.
    assert!(page["items"][0].is_object());
    assert_eq!(page["items"][0]["browser_url"], json!(null));
    assert_eq!(page["items"][0]["session_id"], json!(7));
    assert_eq!(page["window"], json!({ "from": 1000, "to": 2000 }));
    // No auto-drain: the MCP did not fetch the cursor it returned.
    assert_eq!(
        fixture.requests().len(),
        1,
        "no auto-drain after a cursor-bearing page: {:#?}",
        fixture.requests()
    );

    // Continuation form: only cursor + limit are forwarded; from/to/app/window_title absent.
    let result = mcp.call_tool(
        "list_frames",
        json!({ "cursor": "CURSOR-PAGE-2", "limit": 50 }),
    );
    assert_ne!(result["isError"], json!(true), "{result}");
    let recorded = fixture.wait_for_requests(2, Duration::from_secs(10));
    assert_eq!(recorded.len(), 2);
    assert_eq!(
        query_map(&recorded[1].query),
        params(&[("cursor", "CURSOR-PAGE-2"), ("limit", "50")]),
        "a continuation forwards the cursor and limit only"
    );

    let text = first_text(&result);
    let page: Value = serde_json::from_str(&text).expect("tool text is the page JSON");
    assert_eq!(page, page2, "the terminal page is preserved");
    assert!(
        text.contains("\"next_cursor\": null"),
        "the exact terminal null cursor is preserved: {text}"
    );
    assert_eq!(page["items"][0]["raw_text"], json!(raw2));
    assert_eq!(page["items"][0]["content_text"], json!(content2));
    assert_eq!(page["items"][0]["vision_description"], json!(vision2));

    assert_eq!(fixture.requests().len(), 2, "exactly one page per call");
    assert_eq!(mcp.close_and_wait(), 0);
}

/// A single frame whose raw/content/vision payloads are far larger than any context-fit
/// shortening threshold must arrive whole: no truncation, no elision block, no summarization.
#[test]
fn mcp_large_single_frame_is_not_silently_truncated() {
    let raw = format!("BEGIN-LARGE-RAW-{}-END-LARGE-RAW", "x".repeat(512 * 1024));
    let content = format!(
        "BEGIN-LARGE-CONTENT-{}-END-LARGE-CONTENT",
        "y".repeat(256 * 1024)
    );
    let vision = format!("BEGIN-LARGE-VISION-{}-END-LARGE-VISION", "z".repeat(4096));
    let page = json!({
        "items": [frame_item(9, 1500, &raw, &content, &vision)],
        "next_cursor": null,
        "history_id": "99999999-8888-4777-8666-555555555555",
        "window": { "from": 0, "to": 10 },
        "snapshot_max_frame_id": 9,
        "observed_at": 1_700_000_000_000i64,
        "consistency": "bounded_membership_live_content"
    });
    let fixture = MemoryFixture::start(frames_responder(vec![(None, page)]));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    let result = mcp.call_tool("list_frames", json!({ "from": 0, "to": 10 }));
    assert_ne!(result["isError"], json!(true), "{result}");

    // Exactly one full text block — no elision/truncation blocks were appended.
    let blocks = result["content"].as_array().unwrap();
    assert_eq!(blocks.len(), 1, "one whole text block, nothing elided");
    assert_eq!(blocks[0]["type"], "text");

    let text = first_text(&result);
    assert!(
        text.len() > raw.len(),
        "the tool result carries the full payload ({} bytes vs raw {} bytes)",
        text.len(),
        raw.len()
    );
    let page: Value = serde_json::from_str(&text).expect("tool text is the page JSON");
    let item = &page["items"][0];
    assert_eq!(
        item["raw_text"].as_str().unwrap(),
        raw,
        "raw_text preserved byte-for-byte"
    );
    assert_eq!(
        item["content_text"].as_str().unwrap(),
        content,
        "content_text preserved byte-for-byte"
    );
    assert_eq!(
        item["vision_description"].as_str().unwrap(),
        vision,
        "vision_description preserved byte-for-byte"
    );
    assert_eq!(item["raw_text"].as_str().unwrap().len(), raw.len());
    // Both ends of the giant string survive (a head+tail trim would drop one of them).
    assert!(text.contains("BEGIN-LARGE-RAW-"));
    assert!(text.contains("-END-LARGE-RAW"));
    assert!(text.contains("-END-LARGE-CONTENT"));
    assert!(text.contains("-END-LARGE-VISION"));

    assert_eq!(mcp.close_and_wait(), 0);
}

/// Coverage passthrough is verbatim: explicit unknowns (`truncated`, `unavailable_before`,
/// open intervals), structured sub-objects, explicit nulls, and fields added after the
/// frozen contract all survive byte-for-byte.
#[test]
fn coverage_tool_preserves_unknowns() {
    let expected = sample_coverage();
    let served = expected.clone();
    let fixture = MemoryFixture::start(Box::new(move |_method, path, _query| match path {
        "/v1/coverage" => (200, "application/json", json_body(&served)),
        _ => (
            404,
            "application/json",
            br#"{"error":"not_found","message":"no route"}"#.to_vec(),
        ),
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    let result = mcp.call_tool("get_capture_coverage", json!({ "from": 1000, "to": 2000 }));
    assert_ne!(result["isError"], json!(true), "{result}");
    let recorded = fixture.wait_for_requests(1, Duration::from_secs(10));
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/coverage");
    assert_eq!(
        query_map(&recorded[0].query),
        params(&[("from", "1000"), ("to", "2000")]),
        "coverage forwards exactly from+to"
    );

    let text = first_text(&result);
    let coverage: Value = serde_json::from_str(&text).expect("tool text is the coverage JSON");
    assert_eq!(
        coverage,
        sample_coverage(),
        "every field — known and unknown — is preserved verbatim"
    );

    // Explicit unknowns, spelled out so a lossy reserialization cannot slip through.
    let observations = &coverage["capture_observations"];
    assert_eq!(observations["truncated"], json!(true));
    assert_eq!(observations["unavailable_before"], json!(400));
    assert_eq!(
        observations["intervals"][0]["end"],
        json!(null),
        "an open interval keeps end: null"
    );
    assert_eq!(observations["intervals"][0]["reason"], "capture_stopped");
    assert_eq!(observations["intervals"][1]["reason"], "worker_error");
    assert_eq!(observations["intervals"][1]["end"], json!(750));
    // Structured sub-objects stay objects (not stringified).
    assert_eq!(
        coverage["oldest_frame"],
        json!({ "frame_id": 3, "captured_at": 1100 })
    );
    assert_eq!(
        coverage["newest_frame"],
        json!({ "frame_id": 42, "captured_at": 1900 })
    );
    assert_eq!(coverage["retention_scope"], "images_only");
    assert_eq!(
        coverage["saved_report_retention"]["exempt_phases"],
        json!(["queued", "running", "blocked"])
    );
    // Unknown fields (and an unknown null) survive.
    assert_eq!(
        coverage["future_extension"],
        json!({ "nested": [1, 2, 3], "note": "unknown fields survive" })
    );
    assert!(coverage.get("another_unknown").is_some());
    assert_eq!(coverage["another_unknown"], json!(null));
    assert!(text.contains("\"truncated\": true"));
    assert!(text.contains("\"end\": null"));

    assert_eq!(mcp.close_and_wait(), 0);
}

/// A 403 on the new read routes is permissions guidance — it must never be rendered as the
/// API-off message ("enable the API…"/"not responding").
#[test]
fn forbidden_on_new_read_tools_is_not_api_off() {
    let fixture = MemoryFixture::start(Box::new(|_method, _path, _query| {
        (
            403,
            "application/json",
            br#"{"error":"forbidden","message":"bearer token does not grant the required permission"}"#
                .to_vec(),
        )
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    for (tool, args) in [
        ("list_frames", json!({ "from": 1000, "to": 2000 })),
        ("get_capture_coverage", json!({ "from": 1000, "to": 2000 })),
    ] {
        let result = mcp.call_tool(tool, args);
        assert_eq!(result["isError"], json!(true), "{tool}: {result}");
        let message = first_text(&result);
        assert!(message.contains("403"), "{tool}: {message}");
        assert!(message.contains("permission"), "{tool}: {message}");
        assert!(message.contains("read-only"), "{tool}: {message}");
        assert!(
            !message.contains("enable the API"),
            "{tool}: a 403 must not be rendered as API-off: {message}"
        );
        assert!(
            !message.contains("not responding"),
            "{tool}: a 403 must not be rendered as unreachable: {message}"
        );
    }

    assert_eq!(mcp.close_and_wait(), 0);
}

/// JSON argument types, the cursor/initial combination and the limit bound are validated
/// before any HTTP; everything the MCP does not validate (window ordering, cursor validity)
/// is forwarded so the API stays authoritative — its 400 surfaces as the tool error.
#[test]
fn invalid_arguments_fail_before_http_and_api_validation_stays_authoritative() {
    let fixture = MemoryFixture::start(Box::new(|_method, path, query| {
        if path == "/v1/frames" {
            let recorded = query_map(query);
            if recorded.get("from").map(String::as_str) == Some("5") {
                return (
                    400,
                    "application/json",
                    br#"{"error":"bad_request","message":"invalid history query or cursor"}"#
                        .to_vec(),
                );
            }
            return (200, "application/json", json_body(&empty_page()));
        }
        (
            400,
            "application/json",
            br#"{"error":"bad_request","message":"invalid history query or cursor"}"#.to_vec(),
        )
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    // Local client errors — none of these may reach the API.
    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "list_frames",
            json!({}),
            "missing required integer argument `from`",
        ),
        (
            "list_frames",
            json!({ "from": 1 }),
            "missing required integer argument `to`",
        ),
        (
            "list_frames",
            json!({ "from": 1, "to": 2, "cursor": "c" }),
            "cannot be combined with `cursor`",
        ),
        (
            "list_frames",
            json!({ "from": 1, "to": 2, "limit": 0 }),
            "between 1 and 1000",
        ),
        (
            "list_frames",
            json!({ "from": 1, "to": 2, "limit": 1001 }),
            "between 1 and 1000",
        ),
        (
            "list_frames",
            json!({ "from": "1", "to": 2 }),
            "`from` must be an integer",
        ),
        (
            "list_frames",
            json!({ "from": 1, "to": 2, "app": 3 }),
            "`app` must be a string",
        ),
        (
            "get_capture_coverage",
            json!({ "from": 1 }),
            "missing required integer argument `to`",
        ),
        (
            "get_capture_coverage",
            json!({ "from": 1, "to": "2" }),
            "`to` must be an integer",
        ),
    ];
    for (tool, args, needle) in cases {
        let result = mcp.call_tool(tool, args.clone());
        assert_eq!(result["isError"], json!(true), "{tool} {args}: {result}");
        let message = first_text(&result);
        assert!(message.contains(needle), "{tool} {args}: {message}");
        assert!(
            !message.contains("bad_request"),
            "{tool} {args}: local validation must not call the API: {message}"
        );
    }
    thread::sleep(Duration::from_millis(150));
    assert_eq!(
        fixture.requests().len(),
        0,
        "locally rejected calls must not reach the API"
    );

    // Window ordering is not a local bound: the MCP forwards from >= to and surfaces the
    // API's own 400 — the API remains authoritative for query validation.
    let result = mcp.call_tool("list_frames", json!({ "from": 5, "to": 1 }));
    assert_eq!(result["isError"], json!(true), "{result}");
    assert!(
        first_text(&result).contains("bad_request"),
        "{}",
        first_text(&result)
    );
    let recorded = fixture.wait_for_requests(1, Duration::from_secs(10));
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        query_map(&recorded[0].query),
        params(&[("from", "5"), ("to", "1")]),
        "the rejected window was forwarded unmodified"
    );

    assert_eq!(mcp.close_and_wait(), 0);
}

/// Regression for the macOS inherited-nonblocking fixture bug. `accept()` on a nonblocking
/// listener hands back a socket that inherits `O_NONBLOCK` on macOS/BSD (Linux does not), so
/// a read issued before the client's request bytes arrive fails with `WouldBlock` (EAGAIN,
/// errno 35) instead of blocking — the exact panic the hosted macOS job hit at
/// `read_request`'s `read_line`. The fixture must configure the accepted stream for blocking
/// I/O. Here a raw client connects and deliberately withholds its request line, so the
/// fixture is forced to enter `read_request` with an empty socket; the delayed request must
/// still be read and answered.
///
/// A real-socket test with no OS faking. It can only fail where the platform inherits
/// `O_NONBLOCK` (macOS/BSD) — on Linux the accepted socket is already blocking, so it passes
/// with or without the fix; the hosted macOS rerun is the true proof.
#[test]
fn accepted_connection_blocks_until_a_delayed_request_arrives() {
    let fixture = MemoryFixture::start(Box::new(|_method, path, _query| match path {
        "/v1/coverage" => (200, "application/json", json_body(&sample_coverage())),
        _ => (
            404,
            "application/json",
            br#"{"error":"not_found","message":"no route"}"#.to_vec(),
        ),
    }));

    // Connect first, then withhold the request bytes: the fixture's `accept` returns at
    // once, but its read must block until these bytes arrive rather than failing with
    // `WouldBlock`.
    let mut client = TcpStream::connect(fixture.addr).expect("connect fixture");
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("client read timeout");
    thread::sleep(Duration::from_millis(200));
    client
        .write_all(
            b"GET /v1/coverage?from=1000&to=2000 HTTP/1.1\r\nHost: fixture\r\n\
              Connection: close\r\n\r\n",
        )
        .expect("send delayed request");
    client.flush().expect("flush request");

    // Read the response as a Content-Length-framed HTTP message instead of waiting for
    // connection close. `Content-Length` is the message boundary, so a client must not
    // depend on a graceful EOF: Windows can surface the fixture's post-response close as
    // WSAECONNRESET (10054) on the extra read `read_to_end` issues after the body, which
    // would fail a complete, correctly-framed response. Reading exactly the header block
    // plus the declared body never issues that extra read, and asserting the parsed JSON
    // equals `sample_coverage()` is stronger than a substring check — every field,
    // including `retention_scope`, must match.
    let mut reader = BufReader::new(client);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).expect("status line");
    assert_eq!(
        status_line.trim_end(),
        "HTTP/1.1 200 OK",
        "fixture must answer the delayed request"
    );
    let mut content_length = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("response header line");
        if line.trim().is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = Some(value.trim().parse::<usize>().expect("content-length"));
        }
    }
    let content_length = content_length.expect("response declares Content-Length");
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).expect("response body");
    let body: Value = serde_json::from_slice(&body).expect("coverage body json");
    assert_eq!(
        body,
        sample_coverage(),
        "fixture must serve the full coverage body"
    );

    let recorded = fixture.wait_for_requests(1, Duration::from_secs(10));
    assert_eq!(recorded.len(), 1, "the delayed request must be recorded");
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/coverage");
    assert_eq!(
        query_map(&recorded[0].query),
        params(&[("from", "1000"), ("to", "2000")])
    );
}

// ---------------------------------------------------------------------------
// Report read tools (03 §7f.5) — protocol-fixture evidence over the raw HTTP fixture.
// These prove the MCP's translation/forwarding and full-fidelity passthrough against a
// fixture that speaks the frozen wire contract. They are NOT producer/cross-machine
// acceptance: the real `/v1/reports` routes are a separate, still-pending gate.
// ---------------------------------------------------------------------------

/// `list_reports` forwards the initial and continuation forms exactly, preserves the opaque
/// cursor (including the terminal `null`), and returns list rows that carry only the summary
/// — never the saved Markdown/evidence arrays (those belong to `get_report`).
#[test]
fn list_reports_forwards_query_and_cursor_and_rows_omit_saved_result() {
    let page1 = report_page(
        vec![
            report_summary("run-a", "finished", json!("OK")),
            report_summary("run-b", "running", json!(null)),
        ],
        json!("REPORT-CURSOR-2"),
    );
    let page2 = report_page(
        vec![report_summary("run-c", "blocked", json!("BLOCKED"))],
        json!(null),
    );
    let served1 = page1.clone();
    let served2 = page2.clone();
    let fixture = MemoryFixture::start(Box::new(move |_method, path, query| {
        if path != "/v1/reports" {
            return (
                404,
                "application/json",
                br#"{"error":"not_found","message":"no route"}"#.to_vec(),
            );
        }
        match query_map(query).get("cursor").cloned().as_deref() {
            None => (200, "application/json", json_body(&served1)),
            Some("REPORT-CURSOR-2") => (200, "application/json", json_body(&served2)),
            _ => (
                400,
                "application/json",
                br#"{"error":"bad_request","message":"invalid report query or cursor"}"#.to_vec(),
            ),
        }
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    // Initial form: from/to/phase/outcome/limit forwarded exactly — nothing else.
    let result = mcp.call_tool(
        "list_reports",
        json!({ "from": 1000, "to": 2000, "phase": "finished", "outcome": "OK", "limit": 2 }),
    );
    assert_ne!(result["isError"], json!(true), "{result}");
    let recorded = fixture.wait_for_requests(1, Duration::from_secs(10));
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/reports");
    assert_eq!(
        query_map(&recorded[0].query),
        params(&[
            ("from", "1000"),
            ("to", "2000"),
            ("phase", "finished"),
            ("outcome", "OK"),
            ("limit", "2"),
        ]),
        "the initial query forwards exactly the documented parameters"
    );

    let text = first_text(&result);
    let page: Value = serde_json::from_str(&text).expect("tool text is the page JSON");
    assert_eq!(page, page1, "the whole structured page is preserved");
    assert_eq!(page["next_cursor"], json!("REPORT-CURSOR-2"));
    assert!(text.contains("\"next_cursor\": \"REPORT-CURSOR-2\""));
    // List rows carry the summary only — no saved Markdown/evidence arrays.
    assert!(
        page["items"][0].get("result").is_none(),
        "list rows omit result: {text}"
    );
    assert!(page["items"][0].get("markdown").is_none());
    assert!(page["items"][0].get("frame_ids").is_none());
    assert_eq!(page["items"][0]["phase"], "finished");
    assert_eq!(page["items"][0]["outcome"], json!("OK"));
    assert_eq!(page["items"][1]["outcome"], json!(null));
    assert_eq!(page["items"][0]["revision_of"], json!(null));
    assert_eq!(
        fixture.requests().len(),
        1,
        "no auto-drain after a cursor-bearing page"
    );

    // Continuation form: only cursor + limit are forwarded.
    let result = mcp.call_tool(
        "list_reports",
        json!({ "cursor": "REPORT-CURSOR-2", "limit": 50 }),
    );
    assert_ne!(result["isError"], json!(true), "{result}");
    let recorded = fixture.wait_for_requests(2, Duration::from_secs(10));
    assert_eq!(recorded.len(), 2);
    assert_eq!(
        query_map(&recorded[1].query),
        params(&[("cursor", "REPORT-CURSOR-2"), ("limit", "50")]),
        "a continuation forwards the cursor and limit only"
    );
    let text = first_text(&result);
    let page: Value = serde_json::from_str(&text).expect("tool text is the page JSON");
    assert_eq!(page, page2, "the terminal page is preserved");
    assert!(text.contains("\"next_cursor\": null"));
    assert_eq!(fixture.requests().len(), 2, "exactly one page per call");

    assert_eq!(mcp.close_and_wait(), 0);
}

/// `get_report` returns the full public detail unmodified: the saved Markdown byte-for-byte,
/// the full summarized frame-id union (>200, never a citation-cap substitute), generation-time
/// session provenance, a saved result exposed while the run is still queued, an explicit
/// `result: null` when nothing is saved, and a factual stored NO_DATA result.
#[test]
fn get_report_preserves_full_saved_result_and_provenance() {
    let saved = saved_report();
    let expected_finished =
        report_detail("run-finished", "finished", json!("PARTIAL"), saved.clone());
    // A saved result is exposed immediately even while the run/delivery is still queued.
    let expected_queued = report_detail("run-queued", "queued", json!(null), saved.clone());
    // No saved result -> an explicit, present `result: null` (never generated prose).
    let expected_null = report_detail("run-null", "running", json!(null), json!(null));
    // A stored NO_DATA run keeps its factual saved result.
    let expected_no_data = report_detail(
        "run-nodata",
        "finished",
        json!("NO_DATA"),
        no_data_saved_report(),
    );
    let served_finished = expected_finished.clone();
    let served_queued = expected_queued.clone();
    let served_null = expected_null.clone();
    let served_no_data = expected_no_data.clone();
    let fixture = MemoryFixture::start(Box::new(move |_method, path, _query| {
        let body = match path {
            "/v1/reports/run-finished" => Some(served_finished.clone()),
            "/v1/reports/run-queued" => Some(served_queued.clone()),
            "/v1/reports/run-null" => Some(served_null.clone()),
            "/v1/reports/run-nodata" => Some(served_no_data.clone()),
            _ => None,
        };
        match body {
            Some(body) => (200, "application/json", json_body(&body)),
            None => (
                404,
                "application/json",
                br#"{"error":"not_found","message":"no saved report for run"}"#.to_vec(),
            ),
        }
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    // Full saved result: Markdown byte-exact, >200 evidence ids, provenance, counts, nulls.
    let result = mcp.call_tool("get_report", json!({ "run_id": "run-finished" }));
    assert_ne!(result["isError"], json!(true), "{result}");
    let recorded = fixture.wait_for_requests(1, Duration::from_secs(10));
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/reports/run-finished");
    let text = first_text(&result);
    let detail: Value = serde_json::from_str(&text).expect("tool text is the detail JSON");
    assert_eq!(detail, expected_finished, "the whole detail is preserved");
    assert_eq!(
        detail["result"]["markdown"],
        json!("# Weekly recall\n\nBody line one.\nBody line two with a `code` span.\n"),
        "Markdown preserved byte-for-byte"
    );
    assert_eq!(detail["result"]["frame_ids"].as_array().unwrap().len(), 250);
    assert!(
        detail["result"]["frame_ids"].as_array().unwrap().len() > 200,
        "the full summarized frame-id union is kept, not a 200-item citation cap"
    );
    assert_eq!(
        detail["result"]["session_ids"].as_array().unwrap().len(),
        12
    );
    assert_eq!(
        detail["result"]["cited_frame_ids"]
            .as_array()
            .unwrap()
            .len(),
        200
    );
    assert_eq!(detail["result"]["model"], "qwen2.5-7b-instruct");
    assert_eq!(detail["result"]["frames_sampled"], json!(250));
    assert_eq!(detail["result"]["passes"], json!(2));
    assert_eq!(detail["result"]["truncated"], json!(false));
    assert_eq!(detail["revision_of"], json!(null));
    assert_eq!(detail["error_code"], json!(null));
    assert!(text.contains("\"markdown\""));

    // A saved result is exposed while the run is still queued (delivery phase never hides it).
    let queued = mcp.call_tool("get_report", json!({ "run_id": "run-queued" }));
    assert_ne!(queued["isError"], json!(true), "{queued}");
    let queued_detail: Value =
        serde_json::from_str(&first_text(&queued)).expect("queued detail JSON");
    assert_eq!(queued_detail["phase"], "queued");
    assert!(
        queued_detail["result"].is_object(),
        "a queued run exposes its saved result: {queued}"
    );
    assert_eq!(
        queued_detail["result"]["frame_ids"]
            .as_array()
            .unwrap()
            .len(),
        250
    );

    // No saved result -> an explicit, present `result: null`.
    let null_result = mcp.call_tool("get_report", json!({ "run_id": "run-null" }));
    assert_ne!(null_result["isError"], json!(true), "{null_result}");
    let null_text = first_text(&null_result);
    let null_detail: Value = serde_json::from_str(&null_text).expect("null detail JSON");
    assert!(
        null_detail.get("result").is_some(),
        "the result key is present"
    );
    assert_eq!(null_detail["result"], json!(null));
    assert!(null_text.contains("\"result\": null"));

    // A stored NO_DATA run keeps its factual saved result.
    let no_data = mcp.call_tool("get_report", json!({ "run_id": "run-nodata" }));
    assert_ne!(no_data["isError"], json!(true), "{no_data}");
    let no_data_detail: Value =
        serde_json::from_str(&first_text(&no_data)).expect("no_data detail JSON");
    assert_eq!(no_data_detail["outcome"], "NO_DATA");
    assert!(
        no_data_detail["result"].is_object(),
        "NO_DATA keeps its factual saved result, not null: {no_data}"
    );
    assert_eq!(no_data_detail["result"]["frame_ids"], json!([]));
    assert!(no_data_detail["result"]["markdown"]
        .as_str()
        .unwrap()
        .contains("No screen activity"));

    assert_eq!(mcp.close_and_wait(), 0);
}

/// A `run_id` is an opaque string and must be encoded as exactly one URL path segment: a
/// slash, question mark, hash or percent (or space/backslash) can never add a path segment,
/// start a query string, start a fragment, or introduce a percent-escape.
#[test]
fn get_report_encodes_run_id_as_one_url_segment() {
    let served = report_detail("opaque", "finished", json!("OK"), json!(null));
    let fixture = MemoryFixture::start(Box::new(move |_method, path, _query| {
        if path.starts_with("/v1/reports/") {
            (200, "application/json", json_body(&served))
        } else {
            (
                404,
                "application/json",
                br#"{"error":"not_found","message":"no route"}"#.to_vec(),
            )
        }
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    let cases = [
        ("a/b", "/v1/reports/a%2Fb"),
        ("a?b", "/v1/reports/a%3Fb"),
        ("a#b", "/v1/reports/a%23b"),
        ("a%b", "/v1/reports/a%25b"),
        ("a b", "/v1/reports/a%20b"),
        ("a\\b", "/v1/reports/a%5Cb"),
    ];
    for (run_id, expected_path) in cases {
        let result = mcp.call_tool("get_report", json!({ "run_id": run_id }));
        assert_ne!(result["isError"], json!(true), "{run_id}: {result}");
        let recorded = fixture.requests();
        let last = recorded.last().expect("a request was recorded");
        assert_eq!(last.method, "GET", "{run_id}");
        assert_eq!(
            last.path, expected_path,
            "run_id {run_id:?} must be one encoded path segment"
        );
        assert_eq!(
            last.query, "",
            "run_id {run_id:?} must not introduce a query"
        );
    }

    // A multi-character hostile id stays one segment end to end.
    let hostile = "run/with?special#chars%2Fand space";
    let result = mcp.call_tool("get_report", json!({ "run_id": hostile }));
    assert_ne!(result["isError"], json!(true), "{result}");
    let recorded = fixture.requests();
    let last = recorded.last().unwrap();
    assert_eq!(
        last.path,
        "/v1/reports/run%2Fwith%3Fspecial%23chars%252Fand%20space"
    );
    assert_eq!(last.query, "");
    assert_eq!(recorded.len(), cases.len() + 1);

    assert_eq!(mcp.close_and_wait(), 0);
}

/// The `run_id` is carried as exactly one URL path segment. The degenerate ids that cannot be
/// represented as one segment — `.`, `..` and the empty string — are rejected locally with
/// ZERO HTTP requests: percent-encoding their dots does not survive `reqwest`'s URL re-parse
/// (it normalizes `%2E` back to a dot segment and then drops it), so any such workaround would
/// silently retarget the request at `/v1/reports/` or `/v1/` — a different endpoint. Ids that
/// merely *contain* dots stay literal single segments, and a valid id's served error is
/// preserved (never masked by the local rejection).
#[test]
fn get_report_rejects_unrepresentable_dot_ids_before_http() {
    let served = report_detail("a.b", "finished", json!("OK"), json!(null));
    let fixture = MemoryFixture::start(Box::new(move |_method, path, _query| {
        if path == "/v1/reports/broken" {
            return (
                500,
                "application/json",
                br#"{"error":"internal","message":"report store read failed"}"#.to_vec(),
            );
        }
        if path.starts_with("/v1/reports/") {
            (200, "application/json", json_body(&served))
        } else {
            (
                404,
                "application/json",
                br#"{"error":"not_found","message":"no route"}"#.to_vec(),
            )
        }
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    // Unrepresentable ids: a local isError and no HTTP request at all.
    for run_id in [".", "..", ""] {
        let result = mcp.call_tool("get_report", json!({ "run_id": run_id }));
        assert_eq!(result["isError"], json!(true), "{run_id:?}: {result}");
        let message = first_text(&result);
        assert!(
            message.contains("run_id"),
            "{run_id:?}: the message names the argument: {message}"
        );
        assert!(
            !message.contains("enable the API") && !message.contains("not responding"),
            "{run_id:?}: a local rejection is not an API-off error: {message}"
        );
    }
    thread::sleep(Duration::from_millis(150));
    assert_eq!(
        fixture.requests().len(),
        0,
        "unrepresentable ids must be rejected before any HTTP: {:#?}",
        fixture.requests()
    );

    // Ids that merely contain dots are representable and stay literal single segments.
    for (run_id, expected_path) in [
        ("a.b", "/v1/reports/a.b"),
        ("...", "/v1/reports/..."),
        (".hidden", "/v1/reports/.hidden"),
        ("a/../b", "/v1/reports/a%2F..%2Fb"),
    ] {
        let result = mcp.call_tool("get_report", json!({ "run_id": run_id }));
        assert_ne!(result["isError"], json!(true), "{run_id:?}: {result}");
        let recorded = fixture.requests();
        let last = recorded.last().expect("a request was recorded");
        assert_eq!(last.method, "GET", "{run_id:?}");
        assert_eq!(
            last.path, expected_path,
            "run_id {run_id:?} must reach one literal encoded segment"
        );
        assert_eq!(last.query, "", "{run_id:?} must not introduce a query");
    }

    // A valid id's served failure is preserved, not masked by the local rejection.
    let result = mcp.call_tool("get_report", json!({ "run_id": "broken" }));
    assert_eq!(result["isError"], json!(true), "{result}");
    assert!(
        first_text(&result).contains("internal"),
        "{}",
        first_text(&result)
    );
    let recorded = fixture.requests();
    assert_eq!(recorded.last().unwrap().path, "/v1/reports/broken");

    assert_eq!(mcp.close_and_wait(), 0);
}

/// Local validation (JSON types, the initial/continuation combination, the limit bound and
/// the 4096-byte cursor cap) fails before any HTTP; enum membership and window ordering stay
/// the API's to judge, so a bogus phase/outcome or an inverted window is forwarded and its
/// own 400 surfaces as the tool error.
#[test]
fn list_reports_local_validation_and_api_authoritative_enum_and_interval() {
    let fixture = MemoryFixture::start(Box::new(|_method, path, query| {
        if path == "/v1/reports" {
            let recorded = query_map(query);
            let bogus = recorded.get("phase").map(String::as_str) == Some("bogus")
                || recorded.get("outcome").map(String::as_str) == Some("bogus")
                || recorded.get("from").map(String::as_str) == Some("5");
            if bogus {
                return (
                    400,
                    "application/json",
                    br#"{"error":"bad_request","message":"invalid report query or cursor"}"#
                        .to_vec(),
                );
            }
            return (
                200,
                "application/json",
                json_body(&report_page(vec![], json!(null))),
            );
        }
        (
            400,
            "application/json",
            br#"{"error":"bad_request","message":"invalid report query or cursor"}"#.to_vec(),
        )
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    // Local client errors — none of these may reach the API.
    let local: Vec<(Value, &str)> = vec![
        (json!({}), "missing required integer argument `from`"),
        (
            json!({ "from": 1 }),
            "missing required integer argument `to`",
        ),
        (
            json!({ "from": 1, "to": 2, "cursor": "c" }),
            "cannot be combined with `cursor`",
        ),
        (
            json!({ "cursor": "c", "phase": "finished" }),
            "cannot be combined with `cursor`",
        ),
        (
            json!({ "cursor": "c", "outcome": "OK" }),
            "cannot be combined with `cursor`",
        ),
        (
            json!({ "from": 1, "to": 2, "limit": 0 }),
            "between 1 and 1000",
        ),
        (
            json!({ "from": 1, "to": 2, "limit": 1001 }),
            "between 1 and 1000",
        ),
        (json!({ "from": "1", "to": 2 }), "`from` must be an integer"),
        (
            json!({ "from": 1, "to": 2, "limit": "10" }),
            "`limit` must be an integer",
        ),
        (
            json!({ "from": 1, "to": 2, "phase": 7 }),
            "`phase` must be a string",
        ),
        (
            json!({ "from": 1, "to": 2, "outcome": 7 }),
            "`outcome` must be a string",
        ),
        (
            json!({ "from": 1, "to": 2, "cursor": 7 }),
            "`cursor` must be a string",
        ),
    ];
    for (args, needle) in local {
        let result = mcp.call_tool("list_reports", args.clone());
        assert_eq!(result["isError"], json!(true), "{args}: {result}");
        let message = first_text(&result);
        assert!(message.contains(needle), "{args}: {message}");
        assert!(
            !message.contains("bad_request"),
            "{args}: local validation must not call the API: {message}"
        );
    }

    // The 4096-byte cursor cap is validated locally, before any HTTP.
    let oversized = "x".repeat(4097);
    let result = mcp.call_tool("list_reports", json!({ "cursor": oversized }));
    assert_eq!(result["isError"], json!(true), "{result}");
    assert!(
        first_text(&result).contains("4096"),
        "{}",
        first_text(&result)
    );

    thread::sleep(Duration::from_millis(150));
    assert_eq!(
        fixture.requests().len(),
        0,
        "locally rejected calls must not reach the API: {:#?}",
        fixture.requests()
    );

    // Enum membership and window ordering are the API's to judge: a bogus phase/outcome or
    // an inverted window is forwarded, and the API's own 400 surfaces as the tool error.
    for args in [
        json!({ "from": 1, "to": 2, "phase": "bogus" }),
        json!({ "from": 1, "to": 2, "outcome": "bogus" }),
        json!({ "from": 5, "to": 1 }),
    ] {
        let result = mcp.call_tool("list_reports", args.clone());
        assert_eq!(result["isError"], json!(true), "{args}: {result}");
        assert!(
            first_text(&result).contains("bad_request"),
            "{args}: {}",
            first_text(&result)
        );
    }
    let recorded = fixture.wait_for_requests(3, Duration::from_secs(10));
    assert_eq!(recorded.len(), 3, "{recorded:?}");
    assert_eq!(
        query_map(&recorded[0].query),
        params(&[("from", "1"), ("to", "2"), ("phase", "bogus")]),
        "the rejected phase was forwarded unmodified"
    );
    assert_eq!(
        query_map(&recorded[1].query),
        params(&[("from", "1"), ("to", "2"), ("outcome", "bogus")])
    );
    assert_eq!(
        query_map(&recorded[2].query),
        params(&[("from", "5"), ("to", "1")]),
        "the inverted window was forwarded unmodified"
    );

    assert_eq!(mcp.close_and_wait(), 0);
}

/// The report read routes surface served failures as guided tool errors — a missing/pruned
/// detail (404) and store/feed failures (500/503) — never as empty data, NO_DATA or the
/// API-off guidance.
#[test]
fn report_read_errors_404_500_503_surface_as_tool_errors() {
    let fixture = MemoryFixture::start(Box::new(|_method, path, _query| match path {
        "/v1/reports" => (
            503,
            "application/json",
            br#"{"error":"unavailable","message":"report feed not ready"}"#.to_vec(),
        ),
        "/v1/reports/missing" => (
            404,
            "application/json",
            br#"{"error":"not_found","message":"no saved report for run"}"#.to_vec(),
        ),
        "/v1/reports/broken" => (
            500,
            "application/json",
            br#"{"error":"internal","message":"report store read failed"}"#.to_vec(),
        ),
        _ => (
            404,
            "application/json",
            br#"{"error":"not_found","message":"no route"}"#.to_vec(),
        ),
    }));

    let mut mcp = McpChild::spawn(&fixture.base(), Some(TEST_TOKEN), &[]);
    mcp.handshake();

    let cases = [
        (
            "list_reports",
            json!({ "from": 1, "to": 2 }),
            "503",
            "unavailable",
        ),
        (
            "get_report",
            json!({ "run_id": "missing" }),
            "404",
            "not_found",
        ),
        (
            "get_report",
            json!({ "run_id": "broken" }),
            "500",
            "internal",
        ),
    ];
    for (tool, args, status, code) in cases {
        let result = mcp.call_tool(tool, args.clone());
        assert_eq!(result["isError"], json!(true), "{tool} {args}: {result}");
        let message = first_text(&result);
        assert!(message.contains(code), "{tool} ({status}): {message}");
        assert!(
            !message.contains("enable the API"),
            "{tool} ({status}): a served error is not API-off: {message}"
        );
        assert!(
            !message.contains("not responding"),
            "{tool} ({status}): a served error is not unreachable: {message}"
        );
    }

    assert_eq!(mcp.close_and_wait(), 0);
}
