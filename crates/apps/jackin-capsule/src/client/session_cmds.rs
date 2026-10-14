// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{connect_and_send, flag_value, read_control_reply_or_eof, request_control};
use crate::protocol::control::{ClientMsg, ServerMsg};
use anyhow::{Context, Result};

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

pub async fn run_session_events(args: &[String]) -> Result<()> {
    let session = flag_value(args, "--session")
        .map(|raw| raw.parse::<u64>().context("--session must be a u64"))
        .transpose()?;
    let (mut stream, operation) = connect_and_send(&ClientMsg::Events { session }).await?;
    let result = async {
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
