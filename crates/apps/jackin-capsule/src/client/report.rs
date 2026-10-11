// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Runtime hook event reporting command.

use super::{connect_and_send, flag_value, read_control_reply};
use anyhow::{Context, Result};
use jackin_telemetry::ResultTelemetryExt as _;
use tokio::io::AsyncReadExt;

use crate::protocol::control::{ClientMsg, ServerMsg};

/// Forward a runtime hook/plugin event to the daemon for the current session.
///
/// Invoked as `jackin-capsule report-event --event <name> [--payload-stdin]`
/// from a container-local hook/plugin. Reads the agent-only
/// `JACKIN_SESSION_ID`,
/// `JACKIN_STATUS_SOURCE`, `JACKIN_AGENT_RUNTIME` from the spawn env. Always
/// exits 0 — a reporter must never break the agent's hook — so all failures are
/// logged and swallowed.
/// # Errors
///
/// This function currently returns \`Ok(())\`; reporter failures are recorded
/// and swallowed so the agent hook cannot be interrupted.
pub async fn run_report_event(args: &[String]) -> Result<()> {
    drop(
        try_report_event(args)
            .await
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::RpcError),
    );
    Ok(())
}

pub(crate) async fn try_report_event(args: &[String]) -> Result<()> {
    let event = flag_value(args, "--event").context("report-event requires --event <name>")?;
    let session_id: u64 = std::env::var(jackin_protocol::SESSION_ID_ENV)
        .context("JACKIN_SESSION_ID unset")?
        .parse()
        .context("JACKIN_SESSION_ID not a u64")?;
    let source_id = std::env::var("JACKIN_STATUS_SOURCE").context("JACKIN_STATUS_SOURCE unset")?;
    let runtime = std::env::var("JACKIN_AGENT_RUNTIME").context("JACKIN_AGENT_RUNTIME unset")?;

    // Drain stdin when asked so the hook's pipe never breaks. The daemon uses
    // the payload to enrich bare Claude `Notification` events into typed
    // `Notification:<subtype>` keys for gating (plan 009b).
    let payload = if args.iter().any(|a| a == "--payload-stdin") {
        let mut buf = String::new();
        let _read = tokio::io::stdin().read_to_string(&mut buf).await;
        (!buf.is_empty()).then_some(buf)
    } else {
        None
    };

    let (mut stream, operation) = connect_and_send(&ClientMsg::ReportRuntimeEvent {
        session_id,
        source_id,
        runtime,
        event,
        payload,
    })
    .await?;
    let result = async {
        let reply = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            read_control_reply(&mut stream),
        )
        .await
        .context("daemon Ack timed out")??;
        anyhow::ensure!(
            matches!(reply, ServerMsg::Ack),
            "daemon did not acknowledge event"
        );
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
