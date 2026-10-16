//! The eleven MCP tools and their dispatch to the local API.
//!
//! Each tool maps to one HTTP endpoint (`docs/API.md`). Tool results are pretty-printed
//! JSON text blocks — full fidelity for the calling model — plus, for `get_moment`, an
//! optional image content block. Argument problems and API failures both surface as
//! `isError: true` tool results (never JSON-RPC protocol errors), so the calling model can
//! read the message and self-correct (MCP tools contract).

use base64::Engine as _;
use serde_json::{json, Value};

use crate::client::{ApiClient, ApiFailure};
use crate::rpc;

/// A tool-level failure — rendered as an `isError` result, not a protocol error.
enum ToolError {
    /// A bad or missing argument.
    Input(String),
    /// A failure talking to the local API.
    Api(ApiFailure),
}

impl ToolError {
    fn message(&self) -> String {
        match self {
            ToolError::Input(m) => m.clone(),
            ToolError::Api(f) => f.message(),
        }
    }
}

/// The eleven tool definitions, in listing order. One source of truth for `tools/list` and
/// dispatch, so a name can never drift between them.
pub fn definitions() -> Value {
    json!([
        {
            "name": "search_screen_history",
            "title": "Search screen history",
            "description": "Full-text + semantic search over the user's local screen-capture history. Returns matching moments (frames) with a text snippet, relevance score, timestamp, and app. Use a frame_id from the results with get_moment. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search text." },
                    "from": { "type": "integer", "description": "Start of the time window, unix epoch ms. Provide both from and to, or neither." },
                    "to": { "type": "integer", "description": "End of the time window (exclusive), unix epoch ms." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Max results (default 20)." },
                    "include_chrome": { "type": "boolean", "description": "Include repeated UI-chrome text; defaults to the app setting." }
                },
                "required": ["query"],
                "additionalProperties": false
            }
        },
        {
            "name": "ask_screen_history",
            "title": "Ask about screen history",
            "description": "Ask a natural-language question and get a grounded answer synthesized from the user's screen history, with the frame ids it cited. Runs a local model — may take a while. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The question to answer from screen history." },
                    "top_k": { "type": "integer", "minimum": 1, "description": "How many frames to ground the answer on (default follows the app setting)." },
                    "thinking": { "type": "boolean", "description": "Let the model reason before answering (default follows the app setting)." },
                    "max_tokens": { "type": "integer", "minimum": 1, "description": "Cap on the answer length." }
                },
                "required": ["query"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_moment",
            "title": "Get a moment",
            "description": "Fetch full detail (metadata + reconstructed text) for one captured moment by frame_id. Set include_image to also return the stored screenshot. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "frame_id": { "type": "integer", "description": "The frame id, e.g. from search_screen_history." },
                    "include_image": { "type": "boolean", "description": "Also return the stored screenshot as an image block (default false)." }
                },
                "required": ["frame_id"],
                "additionalProperties": false
            }
        },
        {
            "name": "list_frames",
            "title": "List captured frames",
            "description": "One chronological page of the user's captured frames over the exact half-open UTC window [from, to), with the full stored text layers (raw, filtered and vision) — never truncated. To continue, pass the returned next_cursor back with only an optional limit; the window and filters are frozen in the cursor. Each page is returned whole — reduce pages on the caller's side. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "integer", "description": "Start of the half-open window [from, to), unix epoch ms. Required for the first page." },
                    "to": { "type": "integer", "description": "End (exclusive) of the half-open window, unix epoch ms. Required for the first page." },
                    "app": { "type": "string", "description": "Optional app filter: literal, ASCII-case-insensitive exact match (null metadata never matches)." },
                    "window_title": { "type": "string", "description": "Optional window-title filter: literal, ASCII-case-insensitive substring (%, _ are literal characters)." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Max frames per page (default 100, max 1000)." },
                    "cursor": { "type": "string", "description": "Opaque next_cursor from a previous page; continues that page's frozen window and filters. Only limit may accompany it." }
                },
                "anyOf": [
                    { "required": ["from", "to"] },
                    { "required": ["cursor"] }
                ],
                "additionalProperties": false
            }
        },
        {
            "name": "get_capture_coverage",
            "title": "Get capture coverage",
            "description": "Truthful bounded coverage of the stored screen history for the exact half-open UTC window [from, to): stored-frame and text-available counts, oldest/newest frame, allocator high-water, retention settings, and capture observations with explicit unknowns (truncated observations, unavailable_before, open intervals). Counts describe stored records — they are not proof of what the user did. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "integer", "description": "Start of the half-open window [from, to), unix epoch ms." },
                    "to": { "type": "integer", "description": "End (exclusive) of the half-open window, unix epoch ms." }
                },
                "required": ["from", "to"],
                "additionalProperties": false
            }
        },
        {
            "name": "where_was_i",
            "title": "Where was I?",
            "description": "Return the last sustained context (app/window/URL and time span) the user was in before their current activity — the 'what was I doing before this detour?' answer. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "list_marks",
            "title": "List marks",
            "description": "List the user's marks (saved moments / intentions), unresolved first then newest-first. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "add_mark",
            "title": "Add a mark",
            "description": "Mark a moment for later. Omit frame_id to capture the current screen right now (bypassing the change gate) and mark it; pass frame_id to mark an existing moment. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "frame_id": { "type": "integer", "description": "The frame to mark. Omit to capture and mark the current screen now." },
                    "note": { "type": "string", "description": "An optional note to attach to the mark." }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "list_sessions",
            "title": "List sessions",
            "description": "List the user's activity sessions (derived runs of related screen activity), newest first. Filter by kind, tool, and/or a time window; sessions that overlap the window are returned. Use a session id from the results with get_session or ask_session. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["focus", "meeting", "ai", "other"], "description": "Filter by session kind." },
                    "tool": { "type": "string", "description": "Filter by recognized tool id, e.g. claude-code, codex, browser-ai." },
                    "from": { "type": "integer", "description": "Start of the time window, unix epoch ms. from and to are each optional; a session that overlaps the window is included." },
                    "to": { "type": "integer", "description": "End of the time window (exclusive), unix epoch ms." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Max results (default 1000)." }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "get_session",
            "title": "Get a session",
            "description": "Fetch one session's detail plus its extracted user/agent exchanges by session id. include_summary returns the cached summary if one exists — it never generates one. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "integer", "description": "The session id, e.g. from list_sessions." },
                    "include_summary": { "type": "boolean", "description": "Reveal the cached summary if one exists (never generates; default false)." }
                },
                "required": ["session_id"],
                "additionalProperties": false
            }
        },
        {
            "name": "ask_session",
            "title": "Ask about a session",
            "description": "Ask a natural-language question scoped to one session and get a grounded answer citing only that session's frames — e.g. 'what did I do in my last Claude Code session?'. Runs a local model — may take a while. Local data only — served by the ScreenSearch app on the configured API endpoint.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "integer", "description": "The session to scope the answer to, e.g. from list_sessions." },
                    "query": { "type": "string", "description": "The question to answer from this session's frames." },
                    "top_k": { "type": "integer", "minimum": 1, "description": "How many frames to ground the answer on (default follows the app setting)." },
                    "thinking": { "type": "boolean", "description": "Let the model reason before answering (default follows the app setting)." },
                    "max_tokens": { "type": "integer", "minimum": 1, "description": "Cap on the answer length." }
                },
                "required": ["session_id", "query"],
                "additionalProperties": false
            }
        }
    ])
}

/// Whether a tool name is explicitly classified as a history read. This is the read-only
/// allowlist: names absent here — including any tool added later — are never listed and
/// can never dispatch in read-only mode.
pub fn is_read_tool(name: &str) -> bool {
    matches!(
        name,
        "search_screen_history"
            | "get_moment"
            | "list_frames"
            | "get_capture_coverage"
            | "where_was_i"
            | "list_marks"
            | "list_sessions"
            | "get_session"
    )
}

/// Tool definitions selected for the requested mode. Unknown future tools default out.
pub fn definitions_for(read_only: bool) -> Value {
    let mut tools = definitions();
    if read_only {
        tools
            .as_array_mut()
            .expect("tool definitions array")
            .retain(|tool| tool["name"].as_str().is_some_and(is_read_tool));
    }
    tools
}

/// Handles a `tools/call` request, returning the full JSON-RPC response. An unknown tool
/// name is a protocol error (`-32602`); everything else is an in-band tool result.
pub async fn call(client: &ApiClient, id: Value, params: Value) -> Value {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    // Read-only mode is an explicit allowlist: only a classified read tool may dispatch.
    // Every other name — a known mutator, Ask, or a tool added later — is refused here,
    // before argument handling and before any HTTP client is entered (default deny).
    if client.is_read_only() && !is_read_tool(name) {
        return rpc::success(id, error_result("Tool unavailable in --read-only mode: only read tools are accepted (marks/capture and Ask are excluded). API token permissions are enforced independently."));
    }
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result = match name {
        "search_screen_history" => search(client, &args).await,
        "ask_screen_history" => ask(client, &args).await,
        "get_moment" => get_moment(client, &args).await,
        "list_frames" => list_frames(client, &args).await,
        "get_capture_coverage" => get_capture_coverage(client, &args).await,
        "where_was_i" => where_was_i(client).await,
        "list_marks" => list_marks(client).await,
        "add_mark" => add_mark(client, &args).await,
        "list_sessions" => list_sessions(client, &args).await,
        "get_session" => get_session(client, &args).await,
        "ask_session" => ask_session(client, &args).await,
        other => {
            return rpc::error(id, rpc::INVALID_PARAMS, &format!("Unknown tool: {other}"));
        }
    };

    match result {
        Ok(content) => rpc::success(id, json!({ "content": content })),
        Err(e) => rpc::success(id, error_result(&e.message())),
    }
}

async fn search(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let query = require_str(args, "query")?;
    let mut params: Vec<(&str, String)> = vec![("q", query)];
    if let Some(from) = args.get("from").and_then(|v| v.as_i64()) {
        params.push(("from", from.to_string()));
    }
    if let Some(to) = args.get("to").and_then(|v| v.as_i64()) {
        params.push(("to", to.to_string()));
    }
    if let Some(limit) = args.get("limit").and_then(|v| v.as_i64()) {
        params.push(("limit", limit.to_string()));
    }
    if let Some(inc) = args.get("include_chrome").and_then(|v| v.as_bool()) {
        params.push(("include_chrome", inc.to_string()));
    }
    let body = client
        .get_json("/v1/search", &params)
        .await
        .map_err(ToolError::Api)?;
    let count = body.as_array().map(|a| a.len()).unwrap_or(0);
    if count == 0 {
        return Ok(vec![text_block("No results.")]);
    }
    Ok(vec![text_block(pretty(&body))])
}

async fn ask(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let body = build_ask_body(args, None)?;
    run_ask(client, body).await
}

/// `ask_session` — the same grounded answer as `ask_screen_history`, but scoped to one session
/// so the answer cites only that session's frames (`POST /v1/ask` with `session_id`, D12). An
/// unknown session surfaces as an `isError` `not_found` result via the structured-error mapping.
async fn ask_session(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let session_id = require_i64(args, "session_id")?;
    let body = build_ask_body(args, Some(session_id))?;
    run_ask(client, body).await
}

/// Builds the `POST /v1/ask` body shared by `ask_screen_history` and `ask_session`.
fn build_ask_body(
    args: &Value,
    session_id: Option<i64>,
) -> Result<serde_json::Map<String, Value>, ToolError> {
    let query = require_str(args, "query")?;
    let mut body = serde_json::Map::new();
    body.insert("query".to_string(), json!(query));
    for key in ["top_k", "max_tokens"] {
        if let Some(v) = args.get(key).and_then(|v| v.as_i64()) {
            body.insert(key.to_string(), json!(v));
        }
    }
    if let Some(t) = args.get("thinking").and_then(|v| v.as_bool()) {
        body.insert("thinking".to_string(), json!(t));
    }
    if let Some(id) = session_id {
        body.insert("session_id".to_string(), json!(id));
    }
    Ok(body)
}

/// Streams the ask, aggregates the SSE, and renders the answer + cited frames.
async fn run_ask(
    client: &ApiClient,
    body: serde_json::Map<String, Value>,
) -> Result<Vec<Value>, ToolError> {
    let outcome = client
        .ask(Value::Object(body))
        .await
        .map_err(ToolError::Api)?;
    if !outcome.done && !outcome.tokens_seen {
        return Err(ToolError::Input(
            "The answer stream ended without producing a response — check that an answer \
             model is loaded in ScreenSearch."
                .to_string(),
        ));
    }
    if !outcome.done {
        eprintln!(
            "[screensearch-mcp] warning: answer stream ended before completion; returning partial answer"
        );
    }
    let mut text = if outcome.answer.is_empty() {
        "(the model returned no answer text)".to_string()
    } else {
        outcome.answer
    };
    if !outcome.citations.is_empty() {
        let cites: Vec<String> = outcome.citations.iter().map(|c| c.to_string()).collect();
        text.push_str(&format!("\n\nCited frames: {}", cites.join(", ")));
    }
    Ok(vec![text_block(text)])
}

async fn get_moment(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let frame_id = require_i64(args, "frame_id")?;
    let include_image = args
        .get("include_image")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let detail = client
        .get_json(&format!("/v1/frames/{frame_id}"), &[])
        .await
        .map_err(ToolError::Api)?;
    let mut content = vec![text_block(pretty(&detail))];

    if include_image {
        let purged = detail
            .get("image_purged")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if purged {
            content.push(text_block(PURGED_NOTE));
        } else {
            match client
                .get_bytes(
                    &format!("/v1/frames/{frame_id}"),
                    &[("image", "1".to_string())],
                )
                .await
            {
                Ok((bytes, mime)) => {
                    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    content.push(json!({ "type": "image", "data": data, "mimeType": mime }));
                }
                // The image was purged between the detail read and the image read — not an
                // error; the text reconstruction is still returned.
                Err(ApiFailure::Api { code, .. }) if code == "image_purged" => {
                    content.push(text_block(PURGED_NOTE));
                }
                Err(e) => return Err(ToolError::Api(e)),
            }
        }
    }
    Ok(content)
}

/// `list_frames` — one chronological evidence page (`GET /v1/frames`, `03 §7f.1`). The whole
/// structured page is returned unmodified: every item keeps its full raw/content/vision text,
/// `next_cursor` is preserved exactly (including the terminal `null`), and nothing is fetched
/// ahead or shortened to fit a context budget — callers reduce pages, not the server.
async fn list_frames(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let params = build_frames_query(args)?;
    let page = client
        .get_json("/v1/frames", &params)
        .await
        .map_err(ToolError::Api)?;
    Ok(vec![text_block(pretty(&page))])
}

/// `get_capture_coverage` — truthful bounded coverage (`GET /v1/coverage`, `03 §7f.2`). The
/// structured response is returned verbatim, so explicit unknowns (`truncated`,
/// `unavailable_before`, open intervals) and any fields added later survive unchanged.
async fn get_capture_coverage(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let params = build_coverage_query(args)?;
    let coverage = client
        .get_json("/v1/coverage", &params)
        .await
        .map_err(ToolError::Api)?;
    Ok(vec![text_block(pretty(&coverage))])
}

async fn where_was_i(client: &ApiClient) -> Result<Vec<Value>, ToolError> {
    let body = client
        .get_json("/v1/context/where-was-i", &[])
        .await
        .map_err(ToolError::Api)?;
    if body.is_null() {
        return Ok(vec![text_block(
            "Nothing to resume — no sustained context found before the current activity.",
        )]);
    }
    Ok(vec![text_block(pretty(&body))])
}

async fn list_marks(client: &ApiClient) -> Result<Vec<Value>, ToolError> {
    let body = client
        .get_json("/v1/marks", &[])
        .await
        .map_err(ToolError::Api)?;
    let count = body.as_array().map(|a| a.len()).unwrap_or(0);
    if count == 0 {
        return Ok(vec![text_block("No marks.")]);
    }
    Ok(vec![text_block(pretty(&body))])
}

async fn add_mark(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let body = build_add_mark_body(args)?;
    let resp = client
        .post_json("/v1/marks", body)
        .await
        .map_err(ToolError::Api)?;
    let text = match resp.get("mark_id").and_then(|v| v.as_i64()) {
        Some(id) => format!("Created mark {id}.\n{}", pretty(&resp)),
        None => pretty(&resp),
    };
    Ok(vec![text_block(text)])
}

async fn list_sessions(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(kind) = args.get("kind").and_then(|v| v.as_str()) {
        params.push(("kind", kind.to_string()));
    }
    if let Some(tool) = args.get("tool").and_then(|v| v.as_str()) {
        params.push(("tool", tool.to_string()));
    }
    if let Some(from) = args.get("from").and_then(|v| v.as_i64()) {
        params.push(("from", from.to_string()));
    }
    if let Some(to) = args.get("to").and_then(|v| v.as_i64()) {
        params.push(("to", to.to_string()));
    }
    if let Some(limit) = args.get("limit").and_then(|v| v.as_i64()) {
        params.push(("limit", limit.to_string()));
    }
    let body = client
        .get_json("/v1/sessions", &params)
        .await
        .map_err(ToolError::Api)?;
    let count = body.as_array().map(|a| a.len()).unwrap_or(0);
    if count == 0 {
        return Ok(vec![text_block("No sessions.")]);
    }
    Ok(vec![text_block(pretty(&body))])
}

async fn get_session(client: &ApiClient, args: &Value) -> Result<Vec<Value>, ToolError> {
    let session_id = require_i64(args, "session_id")?;
    let mut params: Vec<(&str, String)> = Vec::new();
    // The API reveals the cached summary only when asked, and never generates one (D12).
    if args
        .get("include_summary")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        params.push(("include_summary", "1".to_string()));
    }
    let body = client
        .get_json(&format!("/v1/sessions/{session_id}"), &params)
        .await
        .map_err(ToolError::Api)?;
    Ok(vec![text_block(pretty(&body))])
}

/// Builds the `POST /v1/marks` body from the tool arguments.
///
/// `frame_id` absent (or JSON `null`) means "capture the current screen now, past the diff
/// gate" (`03 §7b`, D8). A `frame_id` that is *present but not an integer* — e.g. a client
/// that stringifies ids as `{"frame_id":"1"}` — is a client error, not a capture-now request:
/// silently marking the current screen would turn a bad argument into a surprising write, so
/// reject it and let the calling model correct the type.
fn build_add_mark_body(args: &Value) -> Result<Value, ToolError> {
    let mut body = serde_json::Map::new();
    match args.get("frame_id") {
        None | Some(Value::Null) => {
            body.insert("now".to_string(), json!(true));
        }
        Some(v) => match v.as_i64() {
            Some(id) => {
                body.insert("frame_id".to_string(), json!(id));
            }
            None => {
                return Err(ToolError::Input(
                    "`frame_id` must be an integer (omit it to mark the current screen)"
                        .to_string(),
                ));
            }
        },
    }
    if let Some(note) = args.get("note").and_then(|v| v.as_str()) {
        body.insert("note".to_string(), json!(note));
    }
    Ok(Value::Object(body))
}

/// The `limit` bound documented by the frame-page contract (`03 §7f.1`, mirroring
/// `traits::FRAME_PAGE_MAX_LIMIT`). The MCP validates JSON argument types, the cursor/initial
/// combination, and this existing bound only; bounds ordering, filter caps and cursor decoding
/// stay with the API, which remains authoritative.
const FRAMES_MAX_LIMIT: i64 = 1000;

/// Builds the `GET /v1/frames` query (`03 §7f.1`). The initial form is required `from`+`to`
/// with optional `app`/`window_title`/`limit`; the continuation form is `cursor` with an
/// optional `limit` only — window and filters are frozen inside the cursor, and mixing a
/// cursor with them is a client error.
fn build_frames_query(args: &Value) -> Result<Vec<(&'static str, String)>, ToolError> {
    let limit = optional_integer(args, "limit")?;
    if let Some(limit) = limit {
        if !(1..=FRAMES_MAX_LIMIT).contains(&limit) {
            return Err(ToolError::Input(format!(
                "`limit` must be between 1 and {FRAMES_MAX_LIMIT}"
            )));
        }
    }
    if let Some(cursor) = optional_string(args, "cursor")? {
        for key in ["from", "to", "app", "window_title"] {
            if args.get(key).is_some_and(|v| !v.is_null()) {
                return Err(ToolError::Input(format!(
                    "`{key}` cannot be combined with `cursor` — a continuation carries the \
                     frozen window and filters; pass only `limit` alongside `cursor`"
                )));
            }
        }
        let mut params: Vec<(&'static str, String)> = vec![("cursor", cursor)];
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        return Ok(params);
    }
    let from = require_integer(args, "from")?;
    let to = require_integer(args, "to")?;
    let mut params: Vec<(&'static str, String)> =
        vec![("from", from.to_string()), ("to", to.to_string())];
    if let Some(app) = optional_string(args, "app")? {
        params.push(("app", app));
    }
    if let Some(window_title) = optional_string(args, "window_title")? {
        params.push(("window_title", window_title));
    }
    if let Some(limit) = limit {
        params.push(("limit", limit.to_string()));
    }
    Ok(params)
}

/// Builds the `GET /v1/coverage` query (`03 §7f.2`): both half-open bounds are required and
/// nothing else is forwarded.
fn build_coverage_query(args: &Value) -> Result<Vec<(&'static str, String)>, ToolError> {
    let from = require_integer(args, "from")?;
    let to = require_integer(args, "to")?;
    Ok(vec![("from", from.to_string()), ("to", to.to_string())])
}

/// A required JSON integer argument (UTC ms). Absent, `null` or a non-integer value — e.g. a
/// numeric string or a float — is a client error; the value is never coerced.
fn require_integer(args: &Value, key: &str) -> Result<i64, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Err(ToolError::Input(format!(
            "missing required integer argument `{key}`"
        ))),
        Some(v) => v
            .as_i64()
            .ok_or_else(|| ToolError::Input(format!("`{key}` must be an integer"))),
    }
}

/// An optional JSON integer argument with the same strict typing as [`require_integer`].
fn optional_integer(args: &Value, key: &str) -> Result<Option<i64>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| ToolError::Input(format!("`{key}` must be an integer"))),
    }
}

/// An optional JSON string argument. Absent and `null` mean "not given"; any other non-string
/// value is a client error (a stringified number is never silently accepted).
fn optional_string(args: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(ToolError::Input(format!("`{key}` must be a string"))),
    }
}

const PURGED_NOTE: &str = "(screenshot purged by retention — the text reconstruction is preserved)";

fn require_str(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::Input(format!("missing required string argument `{key}`")))
}

fn require_i64(args: &Value, key: &str) -> Result<i64, ToolError> {
    args.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ToolError::Input(format!("missing required integer argument `{key}`")))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn text_block(text: impl Into<String>) -> Value {
    json!({ "type": "text", "text": text.into() })
}

fn error_result(message: &str) -> Value {
    json!({ "content": [text_block(message)], "isError": true })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_eleven_tools_with_object_schemas() {
        let defs = definitions();
        let arr = defs.as_array().unwrap();
        let names: Vec<&str> = arr.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec![
                "search_screen_history",
                "ask_screen_history",
                "get_moment",
                "list_frames",
                "get_capture_coverage",
                "where_was_i",
                "list_marks",
                "add_mark",
                "list_sessions",
                "get_session",
                "ask_session",
            ]
        );
        for t in arr {
            assert_eq!(t["inputSchema"]["type"], "object", "tool {}", t["name"]);
            assert!(t["description"].as_str().unwrap().len() > 10);
        }
    }

    #[test]
    fn required_fields_are_declared() {
        let defs = definitions();
        let arr = defs.as_array().unwrap();
        let by_name = |n: &str| arr.iter().find(|t| t["name"] == n).unwrap().clone();

        assert_eq!(
            by_name("search_screen_history")["inputSchema"]["required"],
            json!(["query"])
        );
        assert_eq!(
            by_name("ask_screen_history")["inputSchema"]["required"],
            json!(["query"])
        );
        assert_eq!(
            by_name("get_moment")["inputSchema"]["required"],
            json!(["frame_id"])
        );
        // list_frames has no unconditional required set: from+to are required for the first
        // page, a cursor for a continuation — expressed via anyOf (see the wire-type test).
        assert!(by_name("list_frames")["inputSchema"]
            .get("required")
            .is_none());
        assert_eq!(
            by_name("list_frames")["inputSchema"]["anyOf"],
            json!([
                { "required": ["from", "to"] },
                { "required": ["cursor"] }
            ])
        );
        assert_eq!(
            by_name("get_capture_coverage")["inputSchema"]["required"],
            json!(["from", "to"])
        );
        // add_mark has no required field (frame_id omitted => capture now).
        assert!(by_name("add_mark")["inputSchema"].get("required").is_none());
        // list_sessions has no required field (all filters optional).
        assert!(by_name("list_sessions")["inputSchema"]
            .get("required")
            .is_none());
        assert_eq!(
            by_name("get_session")["inputSchema"]["required"],
            json!(["session_id"])
        );
        assert_eq!(
            by_name("ask_session")["inputSchema"]["required"],
            json!(["session_id", "query"])
        );
    }

    #[test]
    fn error_result_is_flagged() {
        let r = error_result("boom");
        assert_eq!(r["isError"], true);
        assert_eq!(r["content"][0]["type"], "text");
        assert_eq!(r["content"][0]["text"], "boom");
    }

    fn ok_body(args: Value) -> Value {
        match build_add_mark_body(&args) {
            Ok(v) => v,
            Err(e) => panic!("expected Ok, got error: {}", e.message()),
        }
    }

    #[test]
    fn add_mark_body_captures_now_when_frame_id_absent_or_null() {
        assert_eq!(ok_body(json!({})), json!({ "now": true }));
        assert_eq!(
            ok_body(json!({ "frame_id": null, "note": "x" })),
            json!({ "now": true, "note": "x" })
        );
    }

    #[test]
    fn add_mark_body_uses_integer_frame_id() {
        assert_eq!(
            ok_body(json!({ "frame_id": 42 })),
            json!({ "frame_id": 42 })
        );
    }

    #[test]
    fn add_mark_body_rejects_non_integer_frame_id() {
        // A stringified id must NOT silently fall through to a capture-now write.
        for bad in [json!({ "frame_id": "1" }), json!({ "frame_id": 1.5 })] {
            assert!(
                matches!(build_add_mark_body(&bad), Err(ToolError::Input(_))),
                "expected Input error for {bad}"
            );
        }
    }

    #[test]
    fn read_only_allowlist_is_explicit_and_defaults_future_tools_out() {
        let read_names = [
            "search_screen_history",
            "get_moment",
            "list_frames",
            "get_capture_coverage",
            "where_was_i",
            "list_marks",
            "list_sessions",
            "get_session",
        ];
        for name in read_names {
            assert!(is_read_tool(name), "{name}");
        }
        for name in [
            "add_mark",
            "ask_screen_history",
            "ask_session",
            "future_tool",
            // P3 report tools are deliberately not registered yet (and so not read-allowed).
            "list_reports",
            "get_report",
            "",
        ] {
            assert!(!is_read_tool(name), "{name}");
        }
        // The read-only listing is exactly the allowlist-filtered definitions, and every
        // listed tool passes the predicate (the filter never admits an unclassified name).
        let read_only_defs = definitions_for(true);
        let listed: Vec<&str> = read_only_defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(listed, read_names);
        for tool in read_only_defs.as_array().unwrap() {
            assert!(is_read_tool(tool["name"].as_str().unwrap()));
        }
        // Normal mode lists every tool: all eleven.
        assert_eq!(definitions_for(false).as_array().unwrap().len(), 11);
    }

    #[test]
    fn new_read_tools_declare_accurate_wire_types() {
        let defs = definitions();
        let arr = defs.as_array().unwrap();
        let by_name = |n: &str| arr.iter().find(|t| t["name"] == n).unwrap().clone();

        let frames = by_name("list_frames");
        let props = &frames["inputSchema"]["properties"];
        for key in ["from", "to", "limit"] {
            assert_eq!(props[key]["type"], "integer", "list_frames.{key}");
        }
        for key in ["app", "window_title", "cursor"] {
            assert_eq!(props[key]["type"], "string", "list_frames.{key}");
        }
        assert_eq!(props["limit"]["minimum"], 1);
        assert_eq!(props["limit"]["maximum"], 1000);
        assert_eq!(frames["inputSchema"]["additionalProperties"], false);

        let coverage = by_name("get_capture_coverage");
        assert_eq!(coverage["inputSchema"]["required"], json!(["from", "to"]));
        assert_eq!(
            coverage["inputSchema"]["properties"]["from"]["type"],
            "integer"
        );
        assert_eq!(
            coverage["inputSchema"]["properties"]["to"]["type"],
            "integer"
        );
        assert_eq!(coverage["inputSchema"]["additionalProperties"], false);
    }

    fn frames_params(args: Value) -> Vec<(&'static str, String)> {
        match build_frames_query(&args) {
            Ok(params) => params,
            Err(e) => panic!("expected Ok for {args}, got: {}", e.message()),
        }
    }

    fn frames_error(args: Value) -> String {
        match build_frames_query(&args) {
            Err(ToolError::Input(message)) => message,
            Err(other) => panic!(
                "expected an input error for {args}, got: {}",
                other.message()
            ),
            Ok(params) => panic!("expected an input error for {args}, got {params:?}"),
        }
    }

    #[test]
    fn frames_query_builds_initial_and_continuation_forms() {
        // Initial form: from+to required; app/window_title/limit forwarded only when set.
        assert_eq!(
            frames_params(json!({ "from": 1000, "to": 2000 })),
            vec![("from", "1000".to_string()), ("to", "2000".to_string())]
        );
        assert_eq!(
            frames_params(json!({
                "from": 1000,
                "to": 2000,
                "app": "VS Code",
                "window_title": "main.rs",
                "limit": 250
            })),
            vec![
                ("from", "1000".to_string()),
                ("to", "2000".to_string()),
                ("app", "VS Code".to_string()),
                ("window_title", "main.rs".to_string()),
                ("limit", "250".to_string()),
            ]
        );
        // Null filters mean "not given" — never a forwarded empty string.
        assert_eq!(
            frames_params(json!({ "from": 1, "to": 2, "app": null, "window_title": null })),
            vec![("from", "1".to_string()), ("to", "2".to_string())]
        );
        // Continuation form: the cursor alone, or cursor + limit only.
        assert_eq!(
            frames_params(json!({ "cursor": "opaque-1" })),
            vec![("cursor", "opaque-1".to_string())]
        );
        assert_eq!(
            frames_params(json!({ "cursor": "opaque-1", "limit": 1 })),
            vec![
                ("cursor", "opaque-1".to_string()),
                ("limit", "1".to_string())
            ]
        );
    }

    #[test]
    fn frames_query_rejects_bad_types_and_mixed_forms() {
        // Missing bounds on the initial form.
        assert!(frames_error(json!({})).contains("`from`"));
        assert!(frames_error(json!({ "from": 1 })).contains("`to`"));
        // The cursor freezes window + filters: mixing them is rejected before any HTTP.
        for mixed in [
            json!({ "cursor": "c", "from": 1 }),
            json!({ "cursor": "c", "to": 2 }),
            json!({ "cursor": "c", "app": "x" }),
            json!({ "cursor": "c", "window_title": "x" }),
        ] {
            assert!(
                frames_error(mixed.clone()).contains("cannot be combined with `cursor`"),
                "{mixed}"
            );
        }
        // Strict JSON types: numeric strings and floats are never coerced.
        assert!(frames_error(json!({ "from": "1000", "to": 2000 }))
            .contains("`from` must be an integer"));
        assert!(
            frames_error(json!({ "from": 1000, "to": 2000.5 })).contains("`to` must be an integer")
        );
        assert!(
            frames_error(json!({ "from": 1000, "to": 2000, "limit": "10" }))
                .contains("`limit` must be an integer")
        );
        assert!(frames_error(json!({ "from": 1000, "to": 2000, "app": 7 }))
            .contains("`app` must be a string"));
        assert!(
            frames_error(json!({ "from": 1000, "to": 2000, "cursor": 7 }))
                .contains("`cursor` must be a string")
        );
    }

    #[test]
    fn frames_query_limit_matches_the_api_bound() {
        // 1..=1000 is the existing frame-page bound (`03 §7f.1`) — validated locally as a
        // convenience, never widened or narrowed; the API stays authoritative for the rest.
        for ok in [1, 100, 1000] {
            assert_eq!(
                frames_params(json!({ "from": 1, "to": 2, "limit": ok })),
                vec![
                    ("from", "1".to_string()),
                    ("to", "2".to_string()),
                    ("limit", ok.to_string()),
                ]
            );
        }
        for bad in [0, -1, 1001, i64::MAX] {
            assert!(
                frames_error(json!({ "from": 1, "to": 2, "limit": bad }))
                    .contains("between 1 and 1000"),
                "limit {bad}"
            );
        }
    }

    #[test]
    fn coverage_query_requires_two_integer_bounds() {
        match build_coverage_query(&json!({ "from": 1000, "to": 2000 })) {
            Ok(params) => assert_eq!(
                params,
                vec![("from", "1000".to_string()), ("to", "2000".to_string())]
            ),
            Err(e) => panic!("expected Ok, got: {}", e.message()),
        }
        for bad in [
            json!({}),
            json!({ "from": 1 }),
            json!({ "to": 2 }),
            json!({ "from": "1", "to": 2 }),
            json!({ "from": 1, "to": 2.5 }),
        ] {
            assert!(
                matches!(build_coverage_query(&bad), Err(ToolError::Input(_))),
                "expected an input error for {bad}"
            );
        }
    }
}
