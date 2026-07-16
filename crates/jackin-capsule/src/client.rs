// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side capsule client: connects to the daemon socket, forwards
//! stdin/stdout, and handles terminal window resize events.
//!
//! Not responsible for: daemon session management, PTY allocation, or
//! in-container rendering.

use anyhow::{Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::protocol::attach::SpawnRequest;
use crate::protocol::control::{
    AccountUsageSnapshotView, ClientMsg, ControlRequest, ServerMsg, frame as control_frame,
};
use crate::socket::SOCKET_PATH;

/// Connect to the running daemon and run the interactive attach client.
///
/// `spawn_request` is set by `docker exec ... jackin-capsule new`;
/// the first Hello frame asks the daemon to create that session before
/// completing attach. Plain attach (operator-initiated reattach)
/// passes `None`.
pub async fn run_client(
    spawn_request: Option<SpawnRequest>,
    focus_session: Option<u64>,
) -> Result<()> {
    crate::tui::run::run_client(spawn_request, focus_session).await
}

/// Forward a runtime hook/plugin event to the daemon for the current session.
///
/// Invoked as `jackin-capsule report-event --event <name> [--payload-stdin]`
/// from a container-local hook/plugin. Reads `JACKIN_SESSION_ID`,
/// `JACKIN_STATUS_SOURCE`, `JACKIN_AGENT_RUNTIME` from the spawn env. Always
/// exits 0 — a reporter must never break the agent's hook — so all failures are
/// logged and swallowed.
pub async fn run_report_event(args: &[String]) -> Result<()> {
    if let Err(e) = try_report_event(args).await {
        jackin_diagnostics::telemetry_info!("capsule", "report-event: {e:#}");
    }
    Ok(())
}

/// Relay attach-protocol bytes between stdio and the daemon socket.
///
/// This is the fallback transport for hosts that can run `docker exec -i` but
/// cannot open the bind-mounted Unix socket directly. The proxy is deliberately
/// byte-blind: the host-side attach client still owns terminal mode, protocol
/// encoding, frame caps, and validation.
pub async fn run_attach_proxy() -> Result<()> {
    run_attach_proxy_at(SOCKET_PATH, tokio::io::stdin(), tokio::io::stdout()).await
}

async fn run_attach_proxy_at<R, W>(socket_path: &str, input: R, output: W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("cannot connect to jackin-capsule daemon at {socket_path}"))?;
    let (mut socket_read, mut socket_write) = stream.into_split();
    let mut input = input;
    let mut output = output;

    let input_to_socket = async {
        tokio::io::copy(&mut input, &mut socket_write).await?;
        socket_write.shutdown().await?;
        Ok::<(), std::io::Error>(())
    };
    let socket_to_output = async {
        tokio::io::copy(&mut socket_read, &mut output).await?;
        output.shutdown().await?;
        Ok::<(), std::io::Error>(())
    };

    tokio::pin!(input_to_socket);
    tokio::pin!(socket_to_output);
    tokio::select! {
        result = &mut input_to_socket => {
            result.context("relaying stdin to attach socket")?;
            socket_to_output.await.context("relaying attach socket to stdout")?;
        }
        result = &mut socket_to_output => {
            result.context("relaying attach socket to stdout")?;
        }
    }
    Ok(())
}

async fn try_report_event(args: &[String]) -> Result<()> {
    let event = flag_value(args, "--event").context("report-event requires --event <name>")?;
    let session_id: u64 = std::env::var("JACKIN_SESSION_ID")
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

    let mut stream = connect_and_send(&ClientMsg::ReportRuntimeEvent {
        session_id,
        source_id,
        runtime,
        event,
        payload,
    })
    .await?;
    // Read the bounded Ack so the daemon ingests the event before we exit; the
    // content is irrelevant.
    let mut len_buf = [0u8; 4];
    let _ack = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.read_exact(&mut len_buf),
    )
    .await;
    Ok(())
}

/// The first arg after `flag` — works for both `--flag value` and a positional
/// marker like the `<session_id>` after `explain`.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Query the daemon for current session list and print it.
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

/// `jackin-capsule status explain <session_id>` — dump the agent-status
/// evidence bundle (the arbitration report: raw state, winning source,
/// confidence, visible flags, foreground pgid, subagent count, revisions) for
/// one session, as pretty JSON. Reads the same `Snapshot` the console consumes,
/// so it needs no extra protocol surface.
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

/// `jackin-capsule status capture <session_id>` — ask the daemon to write a
/// capture fixture (live grid + evidence) for one session. The daemon owns the
/// grid, so it does the write; the client triggers and waits for the Ack.
pub async fn run_status_capture(args: &[String]) -> Result<()> {
    let session_id: u64 = flag_value(args, "capture")
        .context("usage: jackin-capsule status capture <session_id>")?
        .parse()
        .context("session_id must be a u64")?;

    let mut stream = connect_and_send(&ClientMsg::StatusCapture { session_id }).await?;
    // The daemon writes the fixture synchronously before replying, so reading the
    // reply's length prefix is enough to know the capture ran; the small Ack body
    // is intentionally not drained (the connection closes next).
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    crate::output::stdout_line(format_args!(
        "capture requested for session {session_id}; \
         see /jackin/state/agent-status/captures/"
    ));
    Ok(())
}

/// `jackin-capsule token-usage <session_id>` — print the per-session token-spend
/// summary as JSON, or a no-data line when the session is unknown to the monitor.
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

/// Query the daemon for the tab/pane snapshot and print as JSON.
/// Output shape is `ServerMsg::Snapshot` verbatim so the host
/// console can deserialize the same struct it shares with the
/// daemon — no second schema to keep in sync.
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

/// Format for `jackin-capsule agents` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentsFormat {
    Human,
    Json,
}

/// Query the daemon for the agent registry and render it.
///
/// `--format json` emits the registry as a JSON array.
/// Human format renders a table with a `← you` annotation on the caller's row.
pub async fn run_agents(format: AgentsFormat) -> Result<()> {
    let msg = request_control(&ClientMsg::Agents).await?;
    let records = match msg {
        ServerMsg::AgentRegistry { records } => records,
        ServerMsg::Ack | ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Agents — peer is newer than this CLI"
            )
        }
        other => anyhow::bail!("daemon replied with {} for Agents request", other.kind()),
    };

    // Determine caller's own codename and annotate matching records.
    let my_codename = std::env::var("JACKIN_AGENT_CODENAME").unwrap_or_default();
    let mut records = records;
    if !my_codename.is_empty() {
        for r in &mut records {
            r.is_self = r.codename == my_codename;
        }
    }

    if format == AgentsFormat::Json {
        crate::output::stdout_line(format_args!("{}", serde_json::to_string_pretty(&records)?));
        return Ok(());
    }

    crate::output::stdout_fragment(format_args!("{}", crate::tui::ansi::BRAND_BANNER));
    crate::output::stdout_line(format_args!("agent registry"));
    if let Some(r) = records.iter().find(|r| r.is_self) {
        crate::output::stdout_line(format_args!(
            "\nYou are: {} ({} · {})",
            r.codename,
            r.agent.as_deref().unwrap_or("shell"),
            r.provider.as_deref().unwrap_or("—"),
        ));
    }

    // Split active first, then exited — within each group sort by started_at.
    let mut active: Vec<_> = records.iter().filter(|r| r.status == "active").collect();
    let mut exited: Vec<_> = records.iter().filter(|r| r.status != "active").collect();
    active.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    exited.sort_by(|a, b| a.started_at.cmp(&b.started_at));

    crate::output::stdout_empty_line();
    crate::output::stdout_line(format_args!(
        "  {:<12} {:<10} {:<14} {:<20} {:<20} status",
        "codename", "agent", "provider", "started", "exited"
    ));
    crate::output::stdout_line(format_args!("  {}", "─".repeat(83)));

    for r in active.iter().chain(exited.iter()) {
        let you = if r.is_self { "  ← you" } else { "" };
        crate::output::stdout_line(format_args!(
            "  {:<12} {:<10} {:<14} {:<20} {:<20} {}{}",
            r.codename,
            r.agent.as_deref().unwrap_or("shell"),
            r.provider.as_deref().unwrap_or("—"),
            // Trim the trailing 'Z' and 'T' for compact display
            r.started_at.trim_end_matches('Z').replace('T', " "),
            r.exited_at
                .as_deref()
                .unwrap_or("—")
                .trim_end_matches('Z')
                .replace('T', " "),
            r.status,
            you,
        ));
    }

    Ok(())
}

pub async fn run_usage_accounts() -> Result<()> {
    let accounts = usage_accounts().await?;
    crate::output::stdout_line(format_args!("{}", serde_json::to_string_pretty(&accounts)?));
    Ok(())
}

pub async fn run_usage_verify() -> Result<()> {
    let accounts = usage_accounts().await?;
    let checks = verify_usage_accounts(&accounts);
    for check in &checks {
        crate::output::stdout_line(format_args!(
            "{:<9} {}",
            check.label,
            check.detail.as_deref().unwrap_or(check.status)
        ));
    }
    let failures = checks
        .iter()
        .filter(|check| check.status != "ok")
        .map(|check| format!("{}: {}", check.label, check.status))
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        anyhow::bail!("usage verification failed: {}", failures.join(", "));
    }
    crate::output::stdout_line(format_args!("usage verification passed"));
    Ok(())
}

pub fn run_usage_claude_cli() -> Result<()> {
    let diagnostic = crate::usage::run_claude_usage_diagnostic()
        .map_err(|error| anyhow::anyhow!("Claude CLI usage diagnostic failed: {error}"))?;
    crate::output::stdout_line(format_args!(
        "{}",
        serde_json::to_string_pretty(&diagnostic)?
    ));
    Ok(())
}

async fn usage_accounts() -> Result<Vec<AccountUsageSnapshotView>> {
    let msg = request_control(&ClientMsg::UsageAccountList).await?;
    match msg {
        ServerMsg::UsageAccounts { accounts } => Ok(accounts),
        other => anyhow::bail!(
            "daemon replied with {} for UsageAccountList request",
            other.kind()
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UsageVerifyCheck {
    label: &'static str,
    status: &'static str,
    detail: Option<String>,
}

fn verify_usage_accounts(accounts: &[AccountUsageSnapshotView]) -> Vec<UsageVerifyCheck> {
    usage_verify_provider_aliases()
        .iter()
        .map(|(label, aliases)| verify_usage_provider(label, aliases, accounts))
        .collect()
}

fn usage_verify_provider_aliases() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        ("OpenAI", &["Codex", "OpenAI / Codex"]),
        ("Anthropic", &["Claude", "Anthropic / Claude"]),
        ("Amp", &["Amp"]),
        ("xAI", &["Grok Build", "xAI / Grok"]),
        ("Z.AI", &["GLM / Z.AI"]),
        ("Kimi", &["Kimi"]),
        ("MiniMax", &["MiniMax"]),
    ]
}

fn verify_usage_provider(
    label: &'static str,
    aliases: &[&str],
    accounts: &[AccountUsageSnapshotView],
) -> UsageVerifyCheck {
    let rows = accounts
        .iter()
        .filter(|account| {
            aliases
                .iter()
                .any(|alias| usage_provider_matches(alias, &account.provider))
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return UsageVerifyCheck {
            label,
            status: "missing",
            detail: None,
        };
    }
    let ok = rows.iter().any(|row| usage_row_proves_live_quota(row));
    if ok {
        let Some(latest) = rows.iter().max_by_key(|row| row.fetched_at) else {
            return UsageVerifyCheck {
                label,
                status: "missing",
                detail: None,
            };
        };
        return UsageVerifyCheck {
            label,
            status: "ok",
            detail: Some(format!(
                "ok: {} {} {} {} row(s)",
                latest.status,
                latest.source,
                latest.confidence,
                rows.len()
            )),
        };
    }
    let Some(latest) = rows.iter().max_by_key(|row| row.fetched_at) else {
        return UsageVerifyCheck {
            label,
            status: "missing",
            detail: None,
        };
    };
    UsageVerifyCheck {
        label,
        status: "untrusted",
        detail: Some(format!(
            "untrusted: latest status={} source={} confidence={} error={}",
            latest.status,
            latest.source,
            latest.confidence,
            latest.last_error.as_deref().unwrap_or("none")
        )),
    }
}

fn usage_row_proves_live_quota(row: &AccountUsageSnapshotView) -> bool {
    row.status == "fresh"
        && row.confidence == "authoritative"
        && matches!(row.source.as_str(), "provider_api" | "cli")
        && !row.window_kind.trim().is_empty()
        && !row.account_label.trim().is_empty()
        && !row.account_label.to_ascii_lowercase().contains("needs")
}

fn usage_provider_matches(needle: &str, provider: &str) -> bool {
    // Interchangeable provider/agent labels: a match needs one member of a group
    // on each side. Bidirectional and extensible — add a group, not two arms.
    const SYNONYMS: &[&[&str]] = &[
        &["openai", "codex"],
        &["anthropic", "claude"],
        &["xai", "grok"],
        &["zai", "glm"],
    ];
    let needle = normalize_usage_provider_label(needle);
    let provider = normalize_usage_provider_label(provider);
    provider.contains(&needle)
        || needle.contains(&provider)
        || SYNONYMS.iter().any(|group| {
            group.iter().any(|m| needle.contains(m)) && group.iter().any(|m| provider.contains(m))
        })
}

fn normalize_usage_provider_label(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Connect to the daemon control socket and send one length-prefixed request,
/// returning the open stream so the caller can read (or ignore) the reply.
async fn connect_and_send(request: &ClientMsg) -> Result<UnixStream> {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(request.rpc_method()),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;
    let mut ctx = jackin_protocol::TelemetryContext::v1();
    {
        let _entered = operation.as_ref().map(|guard| guard.span().enter());
        jackin_telemetry::propagation::inject(&mut ctx);
    }
    let result = stream
        .write_all(&control_frame(&ControlRequest {
            ctx,
            msg: request.clone(),
        }))
        .await;
    if let Some(operation) = operation {
        operation.complete(
            if result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            result.as_ref().err().map(|_| "rpc_error"),
        );
    }
    result?;
    Ok(stream)
}

async fn request_control(request: &ClientMsg) -> Result<ServerMsg> {
    let mut stream = connect_and_send(request).await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    // Mirror the daemon-side cap in `socket::read_control_msg`. A
    // buggy or wedged daemon (or a peer that won the socket race
    // inside the container) could otherwise send `0xFFFFFFFF` and
    // force a 4 GiB allocation attempt in the client.
    const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_REPLY {
        anyhow::bail!("daemon control reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests;
