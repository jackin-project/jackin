// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Session send and event-stream commands.

use super::{connect_and_send, flag_value, read_control_reply_or_eof, request_control};
use crate::protocol::control::{ClientMsg, ServerMsg};
use anyhow::{Context, Result};

/// `jackin-capsule send <session_id> <text>` — write text into a running
/// session's PTY and report what happened as one JSON line.
///
/// This is the in-container half of the host's `session.send`: hosts that
/// cannot open the bind-mounted Unix socket (Docker Desktop on macOS) reach the
/// daemon through `docker exec` and this command, exactly as `snapshot` does.
/// The text is passed through verbatim — an operator who wants the agent to
/// submit includes the carriage return in the argument.
/// # Errors
///
/// Returns an error when the session ID or text argument is missing or the
/// daemon request fails.
pub async fn run_session_send(args: &[String]) -> Result<()> {
    let session: u64 = args
        .get(2)
        .context("usage: jackin-capsule send <session_id> <text>")?
        .parse()
        .context("session_id must be a u64")?;
    let text = args
        .get(3)
        .context("usage: jackin-capsule send <session_id> <text>")?
        .clone();
    let reply = request_control(&ClientMsg::SessionSend { session, text }).await?;
    match reply {
        ServerMsg::SessionSent { session, bytes } => {
            crate::output::stdout_line(format_args!(
                "{}",
                serde_json::json!({ "session": session, "bytes": bytes })
            ));
            Ok(())
        }
        ServerMsg::SessionSendDenied { session, reason } => {
            anyhow::bail!("session {session}: {}", reason.label())
        }
        other => anyhow::bail!(
            "daemon replied with {} for SessionSend request",
            other.kind()
        ),
    }
}

/// `jackin-capsule events [--session <id>]` — stream the session event feed as
/// newline-delimited JSON on stdout, one [`jackin_protocol::control::SessionEventRecord`]
/// per line, until the daemon exits or the reader goes away.
///
/// NDJSON rather than the framed wire format because the consumer on this path
/// is reading a `docker exec` pipe, not a socket: a line is the only framing
/// that survives that transport unchanged. Hosts that can open the socket
/// directly read the framed form and never run this.
/// # Errors
///
/// Returns an error when the session filter is invalid, the daemon connection
/// fails, or the event stream cannot be read or serialized.
pub async fn run_session_events(args: &[String]) -> Result<()> {
    let session = flag_value(args, "--session")
        .map(|raw| raw.parse::<u64>().context("--session must be a u64"))
        .transpose()?;
    let (mut stream, operation) = connect_and_send(&ClientMsg::Events { session }).await?;
    let result = async {
        // The stream ends only when the connection does. A daemon shutdown
        // closing it is an orderly end of subscription, not a failure.
        while let Some(reply) = read_control_reply_or_eof(&mut stream).await? {
            let ServerMsg::SessionEvent { event } = reply else {
                anyhow::bail!("daemon replied with {} on the event stream", reply.kind());
            };
            crate::output::stdout_line(format_args!("{}", serde_json::to_string(&event)?));
        }
        Ok(())
    }
    .await;
    if let Some(operation) = operation {
        operation.complete(
            if result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            result
                .as_ref()
                .err()
                .map(|_| jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
    result
}
