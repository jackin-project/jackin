// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! MCP server for `jackin_exec` tool.
//!
//! Implements the Model Context Protocol (MCP) stdio transport so Claude Code
//! can invoke `jackin-exec` commands as a structured tool rather than relying
//! on system prompt injection alone.
//!
//! Registered via `claude mcp add jackin-exec -- /jackin/runtime/jackin-capsule mcp-server`
//! in `runtime_setup.rs` (Claude Code setup). The server reads JSON-RPC from
//! stdin and writes responses to stdout using the MCP stdio protocol.
//!
//! # Tool: `jackin_exec`
//!
//! Allows Claude Code to call `jackin-exec <command> [args...]` with operator
//! approval for on-demand credential injection. The tool description lists the
//! available on-demand bindings from `JACKIN_EXEC_BINDINGS` so the model knows
//! which commands need to go through `jackin-exec`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "jackin-exec";
const SERVER_VERSION: &str = env!("JACKIN_CAPSULE_VERSION");

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    // JSON-RPC 2.0 required field; `id` may be null for notifications.
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// Build the `jackin_exec` tool schema.
/// Uses `JACKIN_EXEC_BINDINGS` to populate the description with available bindings.
fn tool_schema() -> Value {
    let bindings = std::env::var("JACKIN_EXEC_BINDINGS").unwrap_or_default();
    let description = if bindings.is_empty() {
        "Execute a command via jackin-exec with secure credential injection. \
         No on-demand credential bindings are currently configured for this workspace."
            .to_owned()
    } else {
        format!(
            "Execute a command via jackin-exec with secure credential injection. \
             The operator will be prompted to select which on-demand credentials to inject \
             before the command runs. Raw credentials are never exposed to the agent.\n\n\
             Available on-demand credential bindings for this workspace:\n{}",
            bindings
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|b| format!("  - {b}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    json!({
        "name": "jackin_exec",
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute (e.g. 'ssh', 'gh', 'aws', 'docker')"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Arguments to pass to the command",
                    "default": []
                }
            },
            "required": ["command"]
        }
    })
}

/// Handle a `tools/call` request for the `jackin_exec` tool.
async fn handle_tool_call(params: &Value) -> Value {
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");

    if tool_name != "jackin_exec" {
        return json!({
            "content": [{
                "type": "text",
                "text": format!("Unknown tool: {tool_name}")
            }],
            "isError": true
        });
    }

    let args = params.get("arguments").cloned().unwrap_or_default();
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_owned(),
        None => {
            return json!({
                "content": [{"type": "text", "text": "missing required argument: command"}],
                "isError": true
            });
        }
    };

    let cmd_args: Vec<String> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    // Delegate to run_capture which sends ExecCommand via the control socket,
    // shows the operator picker dialog for on-demand credentials, and returns
    // the captured stdout/stderr rather than calling process::exit.
    let mut all_args = vec![command];
    all_args.extend(cmd_args);

    match crate::exec::run_capture(&all_args).await {
        Err(e) => {
            json!({
                "content": [{"type": "text", "text": format!("jackin-exec error: {e:#}")}],
                "isError": true
            })
        }
        Ok(capture) => {
            if let Some(reason) = capture.denied {
                json!({
                    "content": [{"type": "text", "text": format!("[jackin-exec] denied: {reason}")}],
                    "isError": true
                })
            } else {
                let mut text = String::new();
                if !capture.stdout.is_empty() {
                    text.push_str(&capture.stdout);
                }
                if !capture.stderr.is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str("[stderr]\n");
                    text.push_str(&capture.stderr);
                }
                if capture.redacted_count > 0 {
                    text.push_str(&format!(
                        "\n[{} secret pattern(s) redacted from output by jackin']",
                        capture.redacted_count
                    ));
                }
                if text.is_empty() {
                    text = format!("(command completed with exit code {})", capture.exit_code);
                }
                let is_error = capture.exit_code != 0;
                json!({
                    "content": [{"type": "text", "text": text}],
                    "isError": is_error
                })
            }
        }
    }
}

/// Run the MCP stdio server. Reads JSON-RPC from stdin, writes to stdout.
pub async fn run() -> Result<()> {
    run_with_io(tokio::io::stdin(), tokio::io::stdout()).await
}

async fn run_with_io<R, W>(stdin: R, mut stdout: W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();
    jackin_telemetry::stream::complete_success(open);
    let close = jackin_telemetry::stream::close_on_drop();

    let result = async {
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                // EOF — client closed the connection.
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
                Err(e) => Some(JsonRpcResponse::err(
                    Value::Null,
                    -32700,
                    format!("parse error: {e}"),
                )),
                Ok(req) => dispatch(req).await,
            };

            if let Some(resp) = response {
                let mut out = serde_json::to_string(&resp)?;
                out.push('\n');
                stdout.write_all(out.as_bytes()).await?;
                stdout.flush().await?;
            }
        }
        Ok(())
    }
    .await;

    match &result {
        Ok(()) => close.complete_success(),
        Err(_) => close.complete_error(jackin_telemetry::schema::enums::ErrorType::IoError),
    }
    result
}

/// Returns `None` for JSON-RPC notifications (no response required by spec).
async fn dispatch(req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    match req.method.as_str() {
        "initialize" => Some(JsonRpcResponse::ok(
            req.id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            }),
        )),
        "notifications/initialized" | "notifications/cancelled" => {
            // Notifications must not receive a response per JSON-RPC 2.0 spec.
            None
        }
        "tools/list" => Some(JsonRpcResponse::ok(
            req.id,
            json!({
                "tools": [tool_schema()]
            }),
        )),
        "tools/call" => {
            let result = handle_tool_call(&req.params).await;
            Some(JsonRpcResponse::ok(req.id, result))
        }
        "ping" => Some(JsonRpcResponse::ok(req.id, json!({}))),
        other => Some(JsonRpcResponse::err(
            req.id,
            -32601,
            format!("method not found: {other}"),
        )),
    }
}

#[cfg(test)]
mod tests;
