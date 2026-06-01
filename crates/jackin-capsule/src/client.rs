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
        ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Status — peer is newer than this CLI"
            )
        }
        ServerMsg::Snapshot { .. } => {
            anyhow::bail!("daemon replied with Snapshot for Status request")
        }
    };
    println!("Sessions: {}", sessions.len());
    for s in &sessions {
        println!(
            "  [{}] {} ({}) state={} active={}",
            s.id,
            s.label,
            s.agent.as_deref().unwrap_or("shell"),
            s.state.label(),
            s.active,
        );
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
        ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Snapshot — peer is newer than this CLI"
            )
        }
        ServerMsg::SessionList { .. } => {
            anyhow::bail!("daemon replied with SessionList for Snapshot request")
        }
    };
    let payload = serde_json::json!({
        "tabs": tabs,
        "active_tab": active_tab,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
    );
    Ok(())
}
