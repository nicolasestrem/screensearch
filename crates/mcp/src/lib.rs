//! `screensearch-mcp` — a thin **stdio MCP server** wrapping the ScreenSearch local HTTP
//! API (0.3.0 PR8; `03 §7c`, D13).
//!
//! It speaks newline-delimited JSON-RPC 2.0 over stdin/stdout (the MCP stdio transport) and
//! is otherwise a pure HTTP client of the configured HTTP API root (loopback by default)
//! with a bearer token from args/env — **no store access, no app coupling**. Eleven tools
//! wrap the API's search / ask / frame pages + detail / coverage / where-was-i / marks /
//! sessions surface; if the API is off, every tool call returns a clear "enable the API in
//! ScreenSearch Settings" error.
//!
//! Logging goes to **stderr only**; stdout carries protocol messages exclusively.

pub mod client;
pub mod config;
pub mod rpc;
pub mod server;
pub mod sse;
pub mod tools;

use std::io::{BufRead, Write};

use crate::client::ApiClient;

/// Runs the stdio MCP loop: read newline-delimited JSON-RPC from `input`, dispatch, and
/// write responses to `output`, flushing after each. Returns when `input` reaches EOF (the
/// client closed stdin — the MCP shutdown signal) or the output pipe closes.
pub fn run<R: BufRead, W: Write>(
    input: R,
    mut output: W,
    rt: &tokio::runtime::Runtime,
    client: &ApiClient,
) {
    for (index, line) in input.lines().enumerate() {
        let mut line = match line {
            Ok(l) => l,
            // A read error (broken pipe / non-UTF-8) means the session is over.
            Err(_) => break,
        };
        // Defensive: strip a leading BOM some Windows launchers prepend to the first line.
        if index == 0 {
            if let Some(stripped) = line.strip_prefix('\u{feff}') {
                line = stripped.to_string();
            }
        }
        if line.trim().is_empty() {
            continue;
        }

        let response = match rpc::classify(&line) {
            rpc::Message::Request(req) => Some(rt.block_on(server::dispatch(client, req))),
            rpc::Message::Notification(note) => {
                server::handle_notification(&note);
                None
            }
            rpc::Message::Error(resp) => Some(resp),
            rpc::Message::Drop => None,
        };

        if let Some(resp) = response {
            let mut serialized = serde_json::to_string(&resp).unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#
                    .to_string()
            });
            serialized.push('\n');
            // A write/flush failure means the client is gone — exit the loop cleanly.
            if output.write_all(serialized.as_bytes()).is_err() || output.flush().is_err() {
                break;
            }
        }
    }
}
