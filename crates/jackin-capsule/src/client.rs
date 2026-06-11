//! Host-side capsule client: connects to the daemon socket, forwards
//! stdin/stdout, and handles terminal window resize events.
//!
//! Not responsible for: daemon session management, PTY allocation, or
//! in-container rendering.

use anyhow::{Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn attach_proxy_relays_binary_bytes_without_interpreting_frames() {
        let tmp = TempDir::new().unwrap();
        let socket_path = short_socket_path(&tmp, "proxy.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();

        let client_frame = vec![0x01, 0x00, 0x00, 0x00, 0x02, 0xff, 0x00];
        let server_frame = vec![0x82, 0x00, 0x00, 0x00, 0x03, b'o', b'u', b't'];
        let expected_client_frame = client_frame.clone();
        let server_frame_for_task = server_frame.clone();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut received = vec![0u8; expected_client_frame.len()];
            stream.read_exact(&mut received).await.unwrap();
            assert_eq!(received, expected_client_frame);
            stream.write_all(&server_frame_for_task).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let input = tokio::io::duplex(1024);
        let output = tokio::io::duplex(1024);
        let (mut input_writer, input_reader) = input;
        let (output_writer, mut output_reader) = output;

        input_writer.write_all(&client_frame).await.unwrap();
        input_writer.shutdown().await.unwrap();

        run_attach_proxy_at(socket_path.to_str().unwrap(), input_reader, output_writer)
            .await
            .unwrap();

        let mut received = Vec::new();
        output_reader.read_to_end(&mut received).await.unwrap();
        assert_eq!(received, server_frame);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn attach_proxy_exits_when_socket_closes_before_stdin() {
        let tmp = TempDir::new().unwrap();
        let socket_path = short_socket_path(&tmp, "proxy.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server_frame = vec![0x84, 0x00, 0x00, 0x00, 0x00];
        let server_frame_for_task = server_frame.clone();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(&server_frame_for_task).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let (_input_writer, input_reader) = tokio::io::duplex(1024);
        let (output_writer, mut output_reader) = tokio::io::duplex(1024);

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            run_attach_proxy_at(socket_path.to_str().unwrap(), input_reader, output_writer),
        )
        .await
        .expect("proxy should exit after socket EOF")
        .unwrap();

        let mut received = Vec::new();
        output_reader.read_to_end(&mut received).await.unwrap();
        assert_eq!(received, server_frame);
        server.await.unwrap();
    }

    fn short_socket_path(tmp: &TempDir, file_name: &str) -> PathBuf {
        tmp.path().join(file_name)
    }
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
        ServerMsg::Unknown => {
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
