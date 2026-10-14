// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{connect_and_send, flag_value, read_control_reply, request_control};
use anyhow::{Context, Result};

use crate::protocol::control::{ClientMsg, ServerMsg};

pub async fn run_status() -> Result<()> {
    let msg = request_control(&ClientMsg::Status).await?;
    let sessions = match msg {
        ServerMsg::SessionList { sessions } => sessions,
        ServerMsg::Ack | ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Status — peer is newer than this CLI"
            )
        }
        other => anyhow::bail!("daemon replied with {} for Status request", other.kind()),
    };
    crate::output::stdout_line(format_args!("Sessions: {}", sessions.len()));
    for s in &sessions {
        crate::output::stdout_line(format_args!(
            "  [{}] {} ({}) state={} active={}",
            s.id,
            s.label,
            s.agent.as_deref().unwrap_or("shell"),
            s.state.label(),
            s.active,
        ));
    }

    Ok(())
}

pub async fn run_status_explain(args: &[String]) -> Result<()> {
    let session_id: u64 = flag_value(args, "explain")
        .context("usage: jackin-capsule status explain <session_id>")?
        .parse()
        .context("session_id must be a u64")?;

    let ServerMsg::Snapshot { tabs, .. } = request_control(&ClientMsg::Snapshot).await? else {
        anyhow::bail!("daemon did not reply with a snapshot");
    };
    let pane = tabs
        .iter()
        .flat_map(|tab| tab.panes.iter())
        .find(|pane| pane.session_id == session_id)
        .with_context(|| format!("no session {session_id} in the current snapshot"))?;
    let payload = serde_json::json!({
        "session_id": pane.session_id,
        "label": pane.label,
        "agent": pane.agent,
        "effective_state": pane.state.label(),
        "report": pane.agent_status_report,
    });
    crate::output::stdout_line(format_args!("{}", serde_json::to_string_pretty(&payload)?));
    Ok(())
}

pub async fn run_status_capture(args: &[String]) -> Result<()> {
    let session_id: u64 = flag_value(args, "capture")
        .context("usage: jackin-capsule status capture <session_id>")?
        .parse()
        .context("session_id must be a u64")?;

    let (mut stream, operation) =
        connect_and_send(&ClientMsg::StatusCapture { session_id }).await?;
    let read_result = async {
        let reply = read_control_reply(&mut stream).await?;
        anyhow::ensure!(
            matches!(reply, ServerMsg::Ack),
            "daemon did not acknowledge capture"
        );
        Ok(())
    }
    .await;
    if let Some(operation) = operation {
        operation.complete(
            if read_result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            read_result
                .as_ref()
                .err()
                .map(|_| jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
    read_result?;
    crate::output::stdout_line(format_args!(
        "capture requested for session {session_id}; \
         see /jackin/state/agent-status/captures/"
    ));
    Ok(())
}

pub async fn run_token_usage(args: &[String]) -> Result<()> {
    let session_id: u64 = flag_value(args, "token-usage")
        .context("usage: jackin-capsule token-usage <session_id>")?
        .parse()
        .context("session_id must be a u64")?;
    match request_control(&ClientMsg::TokenUsage { session_id }).await? {
        ServerMsg::TokenUsage {
            summary: Some(summary),
        } => {
            crate::output::stdout_line(format_args!("{}", serde_json::to_string_pretty(&summary)?));
        }
        ServerMsg::TokenUsage { summary: None } => {
            crate::output::stdout_line(format_args!(
                "no token data for session {session_id} (not an agent session, or already exited)"
            ));
        }
        other => anyhow::bail!(
            "daemon replied with {} for TokenUsage request",
            other.kind()
        ),
    }
    Ok(())
}

pub async fn run_snapshot() -> Result<()> {
    let msg = request_control(&ClientMsg::Snapshot).await?;
    let (tabs, active_tab) = match msg {
        ServerMsg::Snapshot { tabs, active_tab } => (tabs, active_tab),
        ServerMsg::Ack | ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Snapshot — peer is newer than this CLI"
            )
        }
        other => anyhow::bail!("daemon replied with {} for Snapshot request", other.kind()),
    };
    let payload = serde_json::json!({
        "tabs": tabs,
        "active_tab": active_tab,
    });
    crate::output::stdout_line(format_args!("{}", serde_json::to_string_pretty(&payload)?));
    Ok(())
}
