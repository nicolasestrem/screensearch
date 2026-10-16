//! MCP method dispatch: `initialize`, `ping`, `tools/list`, `tools/call`, and the
//! notification sink. Everything is lenient — a well-behaved client is not required for the
//! read-only handshake to work.

use serde_json::{json, Value};

use crate::client::ApiClient;
use crate::rpc::{self, Notification, Request};
use crate::tools;

/// The MCP protocol revisions this server understands. On `initialize` we echo the client's
/// requested revision if it's here, else we downgrade to [`PREFERRED_VERSION`] (spec-
/// sanctioned): the tool surface is identical across these revisions.
pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// The revision advertised when the client asks for one we don't recognize.
pub const PREFERRED_VERSION: &str = "2025-06-18";

/// Negotiates the protocol version to advertise in the `initialize` result.
pub fn negotiate_version(requested: Option<&str>) -> &'static str {
    match requested {
        Some(v) => SUPPORTED_VERSIONS
            .iter()
            .copied()
            .find(|&s| s == v)
            .unwrap_or(PREFERRED_VERSION),
        None => PREFERRED_VERSION,
    }
}

/// Dispatches one request to its handler, returning the JSON-RPC response to send.
pub async fn dispatch(client: &ApiClient, req: Request) -> Value {
    match req.method.as_str() {
        "initialize" => {
            let mut result = initialize_result(&req.params);
            if client.is_read_only() {
                result["instructions"] = json!("Read the user's ScreenSearch history through the configured HTTP API. Read-only tool mode excludes marks/capture and Ask; the API independently enforces token permissions.");
            }
            rpc::success(req.id, result)
        }
        "ping" => rpc::success(req.id, json!({})),
        "tools/list" => rpc::success(
            req.id,
            json!({ "tools": tools::definitions_for(client.is_read_only()) }),
        ),
        "tools/call" => tools::call(client, req.id, req.params).await,
        other => rpc::error(
            req.id,
            rpc::METHOD_NOT_FOUND,
            &format!("Method not found: {other}"),
        ),
    }
}

/// Handles a notification. `initialized` and `cancelled` are no-ops (we process requests
/// sequentially, so there is nothing in flight to cancel); anything else is logged to
/// stderr and ignored.
pub fn handle_notification(note: &Notification) {
    match note.method.as_str() {
        "notifications/initialized" | "notifications/cancelled" => {}
        other => eprintln!("[screensearch-mcp] ignoring notification: {other}"),
    }
}

fn initialize_result(params: &Value) -> Value {
    let version = negotiate_version(params.get("protocolVersion").and_then(|v| v.as_str()));
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "screensearch-mcp",
            "title": "ScreenSearch screen history",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Search, ask about, and mark the user's ScreenSearch screen-capture history through the configured HTTP API (loopback by default). Requires the API enabled in ScreenSearch Settings -> HTTP API."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echoes_each_supported_version() {
        for v in SUPPORTED_VERSIONS {
            assert_eq!(negotiate_version(Some(v)), *v);
        }
    }

    #[test]
    fn downgrades_unknown_or_absent_version() {
        assert_eq!(negotiate_version(Some("2025-11-25")), PREFERRED_VERSION);
        assert_eq!(negotiate_version(Some("banana")), PREFERRED_VERSION);
        assert_eq!(negotiate_version(None), PREFERRED_VERSION);
    }

    #[test]
    fn initialize_result_shape() {
        let r = initialize_result(&json!({ "protocolVersion": "2025-06-18" }));
        assert_eq!(r["protocolVersion"], "2025-06-18");
        assert!(r["capabilities"]["tools"].is_object());
        assert_eq!(r["serverInfo"]["name"], "screensearch-mcp");
        assert_eq!(r["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    }
}
