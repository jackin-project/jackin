//! Host-side capsule client: connects to the daemon socket, forwards
//! stdin/stdout, and handles terminal window resize events.
//!
//! Not responsible for: daemon session management, PTY allocation, or
//! in-container rendering.

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::protocol::attach::SpawnRequest;
use crate::protocol::control::{ClientMsg, ServerMsg, frame as control_frame};
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
        crate::clog!("report-event: {e:#}");
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

    // Drain stdin when asked so the hook's pipe never breaks; the payload is
    // forwarded but unused by gating today.
    let payload = if args.iter().any(|a| a == "--payload-stdin") {
        let mut buf = String::new();
        let _read = tokio::io::stdin().read_to_string(&mut buf).await;
        (!buf.is_empty()).then_some(buf)
    } else {
        None
    };

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;
    let msg = control_frame(&ClientMsg::ReportRuntimeEvent {
        session_id,
        source_id,
        runtime,
        event,
        payload,
    });
    stream.write_all(&msg).await?;
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

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Query the daemon for current session list and print it.
pub async fn run_status() -> Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;

    let msg = control_frame(&ClientMsg::Status);
    stream.write_all(&msg).await?;

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

    let msg: ServerMsg = serde_json::from_slice(&body)?;
    let sessions = match msg {
        ServerMsg::SessionList { sessions } => sessions,
        ServerMsg::Ack | ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Status — peer is newer than this CLI"
            )
        }
        ServerMsg::Snapshot { .. } => {
            anyhow::bail!("daemon replied with Snapshot for Status request")
        }
        ServerMsg::AgentRegistry { .. } => {
            anyhow::bail!("daemon replied with AgentRegistry for Status request")
        }
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

/// Query the daemon for the tab/pane snapshot and print as JSON.
/// Output shape is `ServerMsg::Snapshot` verbatim so the host
/// console can deserialize the same struct it shares with the
/// daemon — no second schema to keep in sync.
pub async fn run_snapshot() -> Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;

    stream
        .write_all(&control_frame(&ClientMsg::Snapshot))
        .await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_REPLY {
        anyhow::bail!("daemon control reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let msg: ServerMsg = serde_json::from_slice(&body)?;
    let (tabs, active_tab) = match msg {
        ServerMsg::Snapshot { tabs, active_tab } => (tabs, active_tab),
        ServerMsg::Ack | ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Snapshot — peer is newer than this CLI"
            )
        }
        ServerMsg::SessionList { .. } => {
            anyhow::bail!("daemon replied with SessionList for Snapshot request")
        }
        ServerMsg::AgentRegistry { .. } => {
            anyhow::bail!("daemon replied with AgentRegistry for Snapshot request")
        }
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
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;

    stream.write_all(&control_frame(&ClientMsg::Agents)).await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_REPLY {
        anyhow::bail!("daemon control reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let msg: ServerMsg = serde_json::from_slice(&body)?;
    let records = match msg {
        ServerMsg::AgentRegistry { records } => records,
        ServerMsg::Ack | ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Agents — peer is newer than this CLI"
            )
        }
        ServerMsg::SessionList { .. } => {
            anyhow::bail!("daemon replied with SessionList for Agents request")
        }
        ServerMsg::Snapshot { .. } => {
            anyhow::bail!("daemon replied with Snapshot for Agents request")
        }
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

    crate::output::stdout_fragment(format_args!("{}", jackin_tui::ansi::BRAND_BANNER));
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
