//! Portable transport boundary tests using real HTTP sockets, never ambient network
//! services.
//!
//! Everything here exercises the crate's real public surface: the CLI/env/constructor
//! config path, the compiled binary over stdio child processes, and real HTTP/1.1
//! fixtures over loopback sockets. No test mutates the process-global environment.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mcp::client::{ApiClient, ApiFailure};
use mcp::config::{self, Cli};
use serde_json::{json, Value};

/// Reads one complete HTTP/1.1 request (head + `Content-Length` body). Draining the body
/// matters: a fixture that closes with unread body bytes sends RST and turns the client's
/// real redirect/status handling into a flaky transport error.
fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let length: usize = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    if length > 0 {
        stream.read_exact(&mut body).unwrap();
    }
    (head, body)
}

/// Writes one complete HTTP/1.1 response with `Connection: close`.
fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}

/// Writes a redirect response with the given status and `Location`.
fn respond_redirect(stream: &mut TcpStream, status: u16, location: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.flush().unwrap();
}

fn first_text(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn invalid_url_shapes_are_rejected_by_cli_and_constructor_without_echoing_secrets() {
    for url in [
        "",
        "not-a-url",
        "ftp://example.com",
        "FTP://EXAMPLE.COM",
        "file:///etc/passwd",
        "http:///",
        "http://",
        "http://:123",
        "http://user:secret@example.com",
        "http://user@example.com",
        "http://@example.com",
        "http://user:secret@127.0.0.1:43210",
        "HTTP://user:secret@EXAMPLE.COM",
        "http://example.com?",
        "http://example.com?q=secret",
        "http://example.com#",
        "http://example.com#frag",
        "http://example.com/path",
        "http://example.com/path/",
        "http://example.com//",
        "http://example.com/./",
        "http://example.com/path/..",
        "http://example.com/%2e/",
        "http://example.com/%2f",
        "http://0.0.0.0",
        "http://0",
        "http://[::]",
        "http://*.example.com",
        "http://*",
        "http://example.com\\path",
        " http://example.com",
        "http://example.com ",
        "http://example.com\t",
        "http:example.com",
    ] {
        for flag in ["--api-url", "--url"] {
            let cli = config::parse(&[format!("{flag}={url}")], |_| None);
            let Cli::BadUsage(message) = cli else {
                panic!("CLI accepted {url} via {flag}")
            };
            assert!(!message.contains("secret"), "{url}: {message}");
            assert!(!message.contains('@'), "{url}: {message}");
        }
        let env = config::parse(&[], |k| (k == config::ENV_URL).then(|| url.to_string()));
        assert!(
            matches!(env, Cli::BadUsage(_)),
            "env accepted {url}: {env:?}"
        );
        let Err(message) = ApiClient::new(url.to_string(), None) else {
            panic!("constructor accepted {url}")
        };
        assert!(!message.contains("secret"), "{url}: {message}");
        assert!(!message.contains('@'), "{url}: {message}");
    }
}

#[test]
fn url_roots_case_ipv4_ipv6_hostname_and_root_trailing_slash_only() {
    // Accepted: concrete-host http/https roots, any case, IPv4/IPv6/hostname, at most one
    // trailing root slash (normalized away).
    for (raw, normalized) in [
        ("http://127.0.0.1:43210", "http://127.0.0.1:43210"),
        ("http://127.0.0.1", "http://127.0.0.1"),
        ("http://localhost:43210", "http://localhost:43210"),
        ("http://[::1]:43210", "http://[::1]:43210"),
        ("http://[0:0:0:0:0:0:0:1]", "http://[::1]"),
        ("http://10.0.0.5:43210", "http://10.0.0.5:43210"),
        ("https://example.com", "https://example.com"),
        ("https://example.com/", "https://example.com"),
        ("https://[2001:db8::1]:123/", "https://[2001:db8::1]:123"),
        ("HTTP://LOCALHOST:43210/", "http://localhost:43210"),
        (
            "https://API.Example.COM:8443",
            "https://api.example.com:8443",
        ),
    ] {
        assert_eq!(
            config::validate_api_url(raw),
            Ok(normalized.to_string()),
            "{raw}"
        );
        assert!(ApiClient::new(raw.to_string(), None).is_ok(), "{raw}");
        for flag in ["--api-url", "--url"] {
            for args in [vec![format!("{flag}={raw}")], vec![flag.into(), raw.into()]] {
                let cli = config::parse(&args, |_| None);
                let Cli::Run(cfg) = cli else {
                    panic!("CLI rejected {raw} via {flag}")
                };
                assert_eq!(cfg.url, normalized, "{raw} via {flag}");
            }
        }
        assert!(matches!(
            config::parse(&[], |k| (k == config::ENV_URL).then(|| raw.to_string())),
            Cli::Run(_)
        ));
    }

    // A single root trailing slash is the only trailing slash accepted; anything that is
    // (or normalizes to) a non-root path stays rejected.
    for raw in [
        "http://example.com//",
        "http://example.com/a/",
        "http://example.com/a/..",
        "http://example.com/a/../",
    ] {
        assert!(config::validate_api_url(raw).is_err(), "{raw}");
    }
}

#[test]
fn explicit_remote_url_aliases_precedence_and_read_only_flag() {
    for url in [
        "http://10.0.0.5:43210",
        "https://example.com/",
        "https://[2001:db8::1]:123/",
        "http://LOCALHOST:43210",
    ] {
        for flag in ["--url", "--api-url"] {
            for args in [vec![flag.into(), url.into()], vec![format!("{flag}={url}")]] {
                assert!(
                    matches!(config::parse(&args, |_| None), Cli::Run(_)),
                    "{url}"
                );
            }
        }
        assert!(ApiClient::new(url.to_string(), None).is_ok());
        assert!(matches!(
            config::parse(&[], |k| (k == config::ENV_URL).then(|| url.into())),
            Cli::Run(_)
        ));
    }
    let args = [
        "--api-url=http://first.example",
        "--url=http://last.example",
        "--read-only",
    ]
    .map(String::from);
    let Cli::Run(cfg) = config::parse(&args, |k| {
        (k == config::ENV_URL).then(|| "http://env.example".into())
    }) else {
        panic!()
    };
    assert_eq!(cfg.url, "http://last.example");
    assert!(cfg.read_only);
    assert!(matches!(
        config::parse(&["--read-only=false".into()], |_| None),
        Cli::BadUsage(_)
    ));
}

#[tokio::test]
async fn redirects_never_visit_destination_on_any_transport_path() {
    for status in [301, 302, 303, 307, 308] {
        for kind in ["json_get", "json_post", "image", "sse"] {
            for cross_origin in [false, true] {
                let source = TcpListener::bind("127.0.0.1:0").unwrap();
                let destination = TcpListener::bind("127.0.0.1:0").unwrap();
                let base = format!("http://{}", source.local_addr().unwrap());
                let location = if cross_origin {
                    format!("http://{}/stolen", destination.local_addr().unwrap())
                } else {
                    format!("{base}/stolen")
                };
                let source_request = Arc::new(Mutex::new(String::new()));
                let seen = source_request.clone();
                let server = thread::spawn(move || {
                    let (mut stream, _) = source.accept().unwrap();
                    let (head, _body) = read_request(&mut stream);
                    *seen.lock().unwrap() = head;
                    respond_redirect(&mut stream, status, &location);
                    source
                });
                let client = ApiClient::new(base, Some("transport-test-secret".into())).unwrap();
                let err = match kind {
                    "json_get" => client.get_json("/source", &[]).await.err(),
                    "json_post" => client.post_json("/source", json!({})).await.err(),
                    "image" => client.get_bytes("/source", &[]).await.err(),
                    "sse" => client.ask(json!({"query":"test"})).await.err(),
                    _ => unreachable!(),
                }
                .expect("redirect must fail");
                assert!(
                    matches!(err, ApiFailure::Redirect { status: s } if s == status),
                    "{status}/{kind}/{cross_origin}: {}",
                    err.message()
                );
                let source = server.join().unwrap();
                let sent = source_request.lock().unwrap().to_lowercase();
                assert!(sent.contains("authorization: bearer transport-test-secret"));
                let method = if matches!(kind, "json_post" | "sse") {
                    "post "
                } else {
                    "get "
                };
                assert!(sent.starts_with(method));
                for listener in [&source, &destination] {
                    listener.set_nonblocking(true).unwrap();
                    assert_eq!(
                        listener.accept().unwrap_err().kind(),
                        std::io::ErrorKind::WouldBlock,
                        "redirect destination received a request: {status}/{kind}/{cross_origin}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn forbidden_is_permissions_guidance_for_json_image_post_and_sse() {
    for kind in ["json", "image", "post", "sse"] {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            read_request(&mut stream);
            respond(&mut stream, "403 Forbidden", "text/plain", b"");
        });
        let client = ApiClient::new(base, Some("read-token".into())).unwrap();
        let error = match kind {
            "json" => client.get_json("/v1/marks", &[]).await.err(),
            "image" => client.get_bytes("/v1/frames/1", &[]).await.err(),
            "post" => client.post_json("/v1/marks", json!({})).await.err(),
            "sse" => client.ask(json!({})).await.err(),
            _ => unreachable!(),
        }
        .unwrap();
        assert!(matches!(error, ApiFailure::Forbidden));
        let message = error.message();
        assert!(
            message.contains("403")
                && message.contains("permission")
                && message.contains("read-only")
        );
        assert!(!message.contains("enable the API") && !message.contains("not responding"));
        task.join().unwrap();
    }
}

#[tokio::test]
async fn read_only_mode_is_an_explicit_allowlist_at_listing_and_dispatch() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.local_addr().unwrap());
    let client = ApiClient::new(base.clone(), Some("token".into()))
        .unwrap()
        .with_read_only(true);

    // The listing is exactly the predicate-filtered definitions: every classified read
    // tool, nothing else.
    let listed = mcp::server::dispatch(
        &client,
        mcp::rpc::Request {
            id: json!(1),
            method: "tools/list".into(),
            params: json!({}),
        },
    )
    .await;
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "search_screen_history",
            "get_moment",
            "list_frames",
            "get_capture_coverage",
            "where_was_i",
            "list_marks",
            "list_sessions",
            "get_session"
        ]
    );
    let all = mcp::tools::definitions();
    let expected: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .filter(|name| mcp::tools::is_read_tool(name))
        .collect();
    assert_eq!(names, expected);
    assert_eq!(all.as_array().unwrap().len(), 11);

    // The predicate is explicit and default-deny: a name that is not classified as a read
    // (a known mutator, Ask, or a tool added later) is not a read tool.
    assert!(mcp::tools::is_read_tool("search_screen_history"));
    assert!(mcp::tools::is_read_tool("list_frames"));
    assert!(mcp::tools::is_read_tool("get_capture_coverage"));
    for name in [
        "add_mark",
        "ask_screen_history",
        "ask_session",
        "future_tool",
        "search_everything",
        // P3 report tools are not registered yet, so they are not read-allowed.
        "list_reports",
        "get_report",
    ] {
        assert!(!mcp::tools::is_read_tool(name), "{name}");
    }

    // Direct dispatch is refused for every non-allowlisted name — including a name that
    // does not exist yet — before any argument handling or HTTP client is entered.
    for name in [
        "add_mark",
        "ask_screen_history",
        "ask_session",
        "future_tool",
        "list_reports",
    ] {
        let result = mcp::tools::call(
            &client,
            json!(1),
            json!({"name": name, "arguments": {"query":"test", "session_id":1, "frame_id":1}}),
        )
        .await;
        assert_eq!(result["result"]["isError"], true, "{name}: {result}");
        assert!(
            first_text(&result["result"]).contains("read-only"),
            "{name}: {result}"
        );
    }

    // Normal mode keeps the retained behavior: unknown names are protocol errors (-32602),
    // and the full eleven-tool listing is unchanged.
    let normal = ApiClient::new(base, Some("token".into())).unwrap();
    let resp = mcp::tools::call(
        &normal,
        json!(2),
        json!({"name": "future_tool", "arguments": {}}),
    )
    .await;
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
    assert_eq!(
        mcp::tools::definitions_for(false).as_array().unwrap().len(),
        11
    );

    // No request reached the API for any call above.
    server.set_nonblocking(true).unwrap();
    assert_eq!(
        server.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

/// A real HTTP/1.1 fixture over loopback sockets that records every request it serves.
struct TargetFixture {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl TargetFixture {
    fn start() -> TargetFixture {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = requests.clone();
        let stop_flag = stop.clone();
        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // The listener polls non-blocking, and on BSD/macOS an accepted
                        // socket inherits that flag (Linux does not), which would make the
                        // blocking request/response helpers below fail with WouldBlock.
                        // Clear it so the fixture behaves identically on every platform.
                        stream.set_nonblocking(false).unwrap();
                        let (head, body) = read_request(&mut stream);
                        seen.lock()
                            .unwrap()
                            .push((head.clone(), String::from_utf8_lossy(&body).into_owned()));
                        serve_fixture(&mut stream, &head);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        TargetFixture {
            addr,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn wait_for_requests(&self, count: usize, timeout: Duration) -> Vec<(String, String)> {
        let deadline = Instant::now() + timeout;
        loop {
            let seen = self.requests.lock().unwrap().clone();
            if seen.len() >= count || Instant::now() >= deadline {
                return seen;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for TargetFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Serves the three transport paths the proxy test exercises — search JSON, frame image,
/// and the SSE ask stream — as real HTTP/1.1 responses.
fn serve_fixture(stream: &mut TcpStream, head: &str) {
    let mut parts = head.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    match (method, path) {
        ("GET", "/v1/search") => respond(
            stream,
            "200 OK",
            "application/json",
            br#"[{"frame_id":1,"snippet":"proxy fixture hit","score":0.9,"captured_at":1000,"app":"Fixture"}]"#,
        ),
        ("GET", "/v1/frames/1") if query.contains("image=1") => {
            respond(stream, "200 OK", "image/webp", b"proxy-fixture-webp")
        }
        ("GET", "/v1/frames/1") => respond(
            stream,
            "200 OK",
            "application/json",
            br#"{"frame_id":1,"image_purged":false,"text":"fixture detail"}"#,
        ),
        ("POST", "/v1/ask") => respond(
            stream,
            "200 OK",
            "text/event-stream",
            b"data: {\"type\":\"token\",\"text\":\"grounded answer\"}\n\ndata: {\"type\":\"citation\",\"frame_id\":1}\n\ndata: {\"type\":\"done\"}\n\n",
        ),
        _ => respond(
            stream,
            "404 Not Found",
            "application/json",
            br#"{"error":"not_found","message":"fixture has no such route"}"#,
        ),
    }
}

/// An observing HTTP proxy: a real TCP listener that records every connection and every
/// byte it receives. Zero connections after the child ran means nothing was proxied.
struct ProxyObserver {
    addr: SocketAddr,
    connections: Arc<AtomicUsize>,
    bytes: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ProxyObserver {
    fn start() -> ProxyObserver {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let connections = Arc::new(AtomicUsize::new(0));
        let bytes = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let conns = connections.clone();
        let counted = bytes.clone();
        let stop_flag = stop.clone();
        let handle = thread::spawn(move || {
            let mut streams: Vec<TcpStream> = Vec::new();
            let mut buf = [0u8; 4096];
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(true).unwrap();
                        conns.fetch_add(1, Ordering::SeqCst);
                        streams.push(stream);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
                streams.retain_mut(|stream| loop {
                    match stream.read(&mut buf) {
                        Ok(0) => return false,
                        Ok(n) => {
                            counted.fetch_add(n, Ordering::SeqCst);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                        Err(_) => return false,
                    }
                });
                thread::sleep(Duration::from_millis(5));
            }
        });
        ProxyObserver {
            addr,
            connections,
            bytes,
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn bytes(&self) -> usize {
        self.bytes.load(Ordering::SeqCst)
    }

    fn reset(&self) {
        self.connections.store(0, Ordering::SeqCst);
        self.bytes.store(0, Ordering::SeqCst);
    }

    fn wait_for_connections(&self, count: usize, timeout: Duration) -> usize {
        let deadline = Instant::now() + timeout;
        loop {
            let n = self.connections();
            if n >= count || Instant::now() >= deadline {
                return n;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for ProxyObserver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

const RECV_TIMEOUT: Duration = Duration::from_secs(20);

/// A minimal stdio driver for the compiled binary. The child gets only the environment
/// passed here — the process-global environment is never mutated, so tests stay
/// parallel-safe.
struct McpChild {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    next_id: i64,
}

impl McpChild {
    fn spawn(url: &str, token: &str, extra_env: &[(&str, String)]) -> McpChild {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_screensearch-mcp"));
        cmd.env("SCREENSEARCH_API_URL", url)
            .env("SCREENSEARCH_API_TOKEN", token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        // The caller controls the full proxy environment explicitly; nothing ambient
        // leaks in from the test runner.
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
        for (key, value) in extra_env {
            cmd.env(key, value);
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
        McpChild {
            child,
            stdin,
            rx,
            next_id: 0,
        }
    }

    fn send(&mut self, msg: &Value) {
        let mut line = serde_json::to_string(msg).unwrap();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .expect("write request");
        self.stdin.flush().expect("flush request");
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
                "clientInfo": { "name": "remote-transport-test", "version": "1" }
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
            "tool {name} protocol error: {resp}"
        );
        resp["result"].clone()
    }

    fn close_and_wait(mut self) -> i32 {
        drop(self.stdin);
        self.child.wait().expect("child exit").code().unwrap_or(-1)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proxy_environment_is_not_used_for_json_image_and_sse_paths() {
    let target = TargetFixture::start();
    let proxy = ProxyObserver::start();

    // Non-vacuity control: a direct connection to the observing proxy IS recorded, so a
    // zero count after the child ran means "no request was proxied", not "unobserved".
    drop(TcpStream::connect(proxy.addr).unwrap());
    assert!(
        proxy.wait_for_connections(1, Duration::from_secs(5)) >= 1,
        "observer did not record its own control connection"
    );
    proxy.reset();

    // The child gets only this explicit environment: every proxy variable points at the
    // observer and NO_PROXY is absent, so a client that honored environment proxies would
    // have to route through the observer.
    let token = "proxy-test-token";
    let mut mcp = McpChild::spawn(
        &target.base(),
        token,
        &[
            ("HTTP_PROXY", proxy.url()),
            ("HTTPS_PROXY", proxy.url()),
            ("ALL_PROXY", proxy.url()),
            ("http_proxy", proxy.url()),
            ("https_proxy", proxy.url()),
            ("all_proxy", proxy.url()),
        ],
    );
    let init = mcp.handshake();
    assert_eq!(init["protocolVersion"], "2025-06-18");
    let listed = mcp.request("tools/list", json!({}));
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 11);

    // JSON path.
    let search = mcp.call_tool("search_screen_history", json!({ "query": "proxy probe" }));
    assert_ne!(search["isError"], json!(true), "{search}");
    // Image path: frame detail JSON, then image bytes.
    let moment = mcp.call_tool(
        "get_moment",
        json!({ "frame_id": 1, "include_image": true }),
    );
    assert_ne!(moment["isError"], json!(true), "{moment}");
    assert_eq!(moment["content"][1]["type"], "image");
    assert_eq!(moment["content"][1]["mimeType"], "image/webp");
    // SSE path.
    let ask = mcp.call_tool("ask_screen_history", json!({ "query": "proxy ask" }));
    assert_ne!(ask["isError"], json!(true), "{ask}");
    assert!(first_text(&ask).contains("grounded answer"), "{ask}");

    // The four requests: search, frame detail, frame image, ask.
    let requests = target.wait_for_requests(4, Duration::from_secs(10));
    assert_eq!(requests.len(), 4, "target requests: {requests:?}");
    assert!(
        requests[0].0.starts_with("GET /v1/search?"),
        "{}",
        requests[0].0
    );
    assert!(requests[0].0.contains("q=proxy"), "{}", requests[0].0);
    assert!(
        requests[1].0.starts_with("GET /v1/frames/1 "),
        "{}",
        requests[1].0
    );
    assert!(
        requests[2].0.starts_with("GET /v1/frames/1?image=1"),
        "{}",
        requests[2].0
    );
    assert!(
        requests[3].0.starts_with("POST /v1/ask"),
        "{}",
        requests[3].0
    );
    assert!(
        requests[3].1.contains("\"query\":\"proxy ask\""),
        "{}",
        requests[3].1
    );
    let expected_auth = format!("authorization: bearer {token}");
    for (head, _) in &requests {
        assert!(head.to_lowercase().contains(&expected_auth), "{head}");
    }

    // Every request reached the configured target directly: the observing proxy saw none.
    thread::sleep(Duration::from_millis(250));
    assert_eq!(
        proxy.connections(),
        0,
        "a request was routed through the proxy"
    );
    assert_eq!(proxy.bytes(), 0, "bytes reached the proxy");
    assert_eq!(mcp.close_and_wait(), 0);
}

/// Kills its child on drop so a panicking test never leaks the TLS fixture server.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Counts the fixture's server-side records of a TLS handshake a client opened and then
/// aborted before any HTTP exchange. OpenSSL's `s_server` words this differently by
/// platform and build: a received alert (`...alert certificate unknown... SSL alert number
/// 46`) on Linux, a handshake record read cut short by the peer (`unexpected eof while
/// reading`) on Windows. Both are written only after a client's TLS ClientHello reached the
/// fixture, so a growing count is portable evidence that the client connected and was
/// rejected inside the handshake — not that the endpoint was simply absent (a dead endpoint
/// writes nothing). Matching either phrasing keeps the assertion from pinning one OpenSSL
/// build's exact log text.
fn handshake_aborts(log: &str) -> usize {
    log.lines()
        .filter(|line| {
            line.contains("alert")
                || line
                    .to_ascii_lowercase()
                    .contains("unexpected eof while reading")
        })
        .count()
}

#[tokio::test]
async fn https_verification_stays_on_for_untrusted_certificates() {
    // A real TLS endpoint (openssl s_server — a local tool, no new library dependency)
    // with a self-signed certificate: the client must refuse it. TLS verification is not
    // configurable in this client, so this pins the builder defaults staying on.
    let openssl_available = Command::new("openssl")
        .arg("version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !openssl_available {
        eprintln!("skipping https certificate test: openssl CLI unavailable");
        return;
    }

    let dir = std::env::temp_dir().join(format!("screensearch-mcp-tls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create tls fixture dir");
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    let generated = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-keyout",
            key.to_str().unwrap(),
            "-out",
            cert.to_str().unwrap(),
            "-days",
            "1",
            "-nodes",
            "-subj",
            "/CN=127.0.0.1",
            "-addext",
            "subjectAltName=IP:127.0.0.1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run openssl req");
    assert!(generated.success(), "openssl req failed");

    // A free port for the fixture (bound briefly to pick it, then released).
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let log_path = dir.join("s_server.log");
    let log = std::fs::File::create(&log_path).expect("create s_server log");
    let accept = format!("127.0.0.1:{port}");
    let cert_path = cert.to_str().unwrap().to_string();
    let key_path = key.to_str().unwrap().to_string();
    let server = Command::new("openssl")
        .args([
            "s_server",
            "-accept",
            accept.as_str(),
            "-cert",
            cert_path.as_str(),
            "-key",
            key_path.as_str(),
            "-www",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn openssl s_server");
    let _guard = ChildGuard(server);
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(accept.as_str()).is_err() {
        assert!(
            Instant::now() < deadline,
            "openssl s_server never accepted connections"
        );
        thread::sleep(Duration::from_millis(25));
    }

    // Control: a raw TLS client completes a handshake here (s_client does not abort on a
    // verify error), proving the fixture is a reachable TLS endpoint — so the client
    // failure below is certificate rejection, not an unreachable server.
    let control = Command::new("openssl")
        .args(["s_client", "-connect", accept.as_str()])
        .stdin(Stdio::null())
        .output()
        .expect("run openssl s_client");
    let control_text = format!(
        "{}{}",
        String::from_utf8_lossy(&control.stdout),
        String::from_utf8_lossy(&control.stderr)
    );
    assert!(
        control_text.contains("CONNECTED"),
        "control handshake failed: {control_text}"
    );
    let aborts_before = handshake_aborts(&std::fs::read_to_string(&log_path).unwrap_or_default());

    // The client under test must refuse the untrusted certificate on every real request
    // path (JSON, image bytes, SSE). TLS verification is not configurable in this client,
    // so this pins the builder defaults staying on across all three.
    let client = ApiClient::new(
        format!("https://127.0.0.1:{port}"),
        Some("tls-test-token".into()),
    )
    .unwrap();
    let outcomes: [(&str, Result<(), ApiFailure>); 3] = [
        ("json", client.get_json("/v1/marks", &[]).await.map(|_| ())),
        (
            "image",
            client.get_bytes("/v1/frames/1", &[]).await.map(|_| ()),
        ),
        ("sse", client.ask(json!({})).await.map(|_| ())),
    ];

    // Portable error assertion: a refused certificate is a connect-class failure, so no
    // HTTP response is ever produced — a request that reached the server would return Ok,
    // and a served rejection would surface as Forbidden/Unauthorized/Api/Redirect, never
    // NotReachable. That is the observable HTTP non-arrival: every path died before the
    // HTTP layer, so the client kept certificate verification on.
    for (path, outcome) in &outcomes {
        assert!(
            matches!(outcome, Err(ApiFailure::NotReachable { .. })),
            "{path} path did not fail as a connect-class TLS rejection: {outcome:?}"
        );
    }

    // The fixture must itself observe the client's aborted handshakes: one new
    // handshake-abort record per request. A genuinely unreachable endpoint writes nothing,
    // so this is what separates "certificate rejected" from "never connected" — matched
    // portably (TLS alert OR handshake EOF), never by one OpenSSL build's exact phrasing.
    let deadline = Instant::now() + Duration::from_secs(5);
    let log = loop {
        let log = std::fs::read_to_string(&log_path).unwrap_or_default();
        if handshake_aborts(&log) >= aborts_before + outcomes.len() {
            break log;
        }
        assert!(
            Instant::now() < deadline,
            "fixture never observed the client's aborted TLS handshakes \
             (aborts before: {aborts_before}):\n{log}"
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert!(
        handshake_aborts(&log) >= aborts_before + outcomes.len(),
        "fixture did not record one aborted handshake per request:\n{log}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
