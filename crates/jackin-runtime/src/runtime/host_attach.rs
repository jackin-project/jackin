//! Host-owned attach client for running Capsule daemons.
//!
//! This is the host-side twin of the in-container `jackin-capsule`
//! interactive client. It owns the operator terminal and speaks the
//! shared attach protocol over either the bind-mounted Capsule socket
//! or a stdio `attach-proxy` running inside the container.

use std::io::Write;
use std::process::{Command as StdCommand, Stdio};

use anyhow::{Context, Result, bail};
use jackin_core::paths::JackinPaths;
use jackin_protocol::attach::{
    ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_client, read_server_frame,
};
use jackin_tui::host_colors::query_host_terminal_colors;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::signal::unix::{SignalKind, signal};

use super::attach::{
    HostAttachTransportPlan, attach_proxy_exec_args, select_host_attach_transport,
};
use super::host_clipboard::read_image_for_paste_trigger;

pub const JACKIN_HOST_ATTACH_ENV: &str = "JACKIN_HOST_ATTACH";

const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
const MIN_ROWS: u16 = 6;
const MIN_COLS: u16 = 3;

const OUTER_TERMINAL_RESET_BASE: &[u8] =
    b"\x1b[0m\x1b]22;default\x1b\\\x1b[?7h\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1007l\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b[<u\x1b[?25h";
const ALTERNATE_SCREEN_LEAVE: &[u8] = b"\x1b[?1049l";
const RESET_CLEAR_HOME: &[u8] = b"\x1b[0m\x1b[2J\x1b[H";
const CLIENT_OWNED_MODE_STATE: &[u8] =
    b"\x1b[?7l\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1005l\x1b[?1015l\x1b[?1007l\x1b[?1003h\x1b[?1006h\x1b[?1004h";

#[must_use]
pub fn host_attach_enabled() -> bool {
    std::env::var_os(JACKIN_HOST_ATTACH_ENV).is_some()
}

pub(super) async fn run_host_attach_session(
    paths: &JackinPaths,
    container_name: &str,
    spawn_request: Option<SpawnRequest>,
    focus_session: Option<u64>,
    env_overrides: &[(String, String)],
) -> Result<()> {
    let request = HostAttachRequest {
        spawn_request,
        focus_session,
        env: env_overrides.to_vec(),
        terminal: ClientTerminal::from_env(),
    };

    match select_host_attach_transport(paths, container_name) {
        HostAttachTransportPlan::DirectSocket { socket_path } => {
            jackin_diagnostics::debug_log!(
                "attach",
                "host attach using direct socket {}",
                socket_path.display()
            );
            let stream = UnixStream::connect(&socket_path)
                .await
                .with_context(|| format!("connecting to {}", socket_path.display()))?;
            let (reader, writer) = stream.into_split();
            run_terminal_attach(reader, writer, request).await
        }
        HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error,
        } => {
            jackin_diagnostics::debug_log!(
                "attach",
                "host attach using attach-proxy for {} (direct_error={:?})",
                socket_path.display(),
                direct_error
            );
            let mut child = Command::new("docker")
                .args(attach_proxy_exec_args(container_name))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .context("starting docker attach-proxy")?;
            let stdout = child
                .stdout
                .take()
                .context("attach-proxy stdout was not piped")?;
            let stdin = child
                .stdin
                .take()
                .context("attach-proxy stdin was not piped")?;
            let attach_result = run_terminal_attach(stdout, stdin, request).await;
            let status = child.wait().await.context("waiting for attach-proxy")?;
            if attach_result.is_ok() && !status.success() {
                bail!("attach-proxy exited with {status}");
            }
            attach_result
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostAttachRequest {
    spawn_request: Option<SpawnRequest>,
    focus_session: Option<u64>,
    env: Vec<(String, String)>,
    terminal: ClientTerminal,
}

async fn run_terminal_attach<R, W>(
    server_reader: R,
    server_writer: W,
    mut request: HostAttachRequest,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (rows, cols) = terminal_size();
    let mut stdout = std::io::stdout();
    let _cleanup = enter_host_attach_terminal(&mut stdout)?;
    let mut stdin = tokio::io::stdin();
    let host_colors =
        query_host_terminal_colors(request.terminal.term.as_deref(), &mut stdin, &mut stdout).await;
    request.terminal.default_fg = host_colors.fg;
    request.terminal.default_bg = host_colors.bg;
    let output = std::io::stdout();
    let winch =
        signal(SignalKind::window_change()).context("failed to install SIGWINCH handler")?;
    run_attach_protocol(
        server_reader,
        server_writer,
        stdin,
        output,
        rows,
        cols,
        request,
        host_colors.leftover_input,
        winch,
    )
    .await
}

#[expect(clippy::too_many_arguments)]
async fn run_attach_protocol<R, W, I, O>(
    mut server_reader: R,
    mut server_writer: W,
    mut terminal_input: I,
    mut terminal_output: O,
    rows: u16,
    cols: u16,
    request: HostAttachRequest,
    initial_input: Vec<u8>,
    mut winch: tokio::signal::unix::Signal,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    I: AsyncRead + Unpin,
    O: Write,
{
    let hello = encode_client(ClientFrame::Hello {
        rows,
        cols,
        env: request.env,
        spawn: request.spawn_request,
        terminal: request.terminal,
        focus_session: request.focus_session,
    })
    .context("encoding attach Hello frame")?;
    server_writer
        .write_all(&hello)
        .await
        .context("sending attach Hello frame")?;
    if !initial_input.is_empty() {
        let msg = encode_client(ClientFrame::Input(initial_input))
            .context("encoding pre-attach Input frame")?;
        server_writer
            .write_all(&msg)
            .await
            .context("attach socket write failed (pre-attach input)")?;
    }

    let mut stdin_buf = [0u8; 4096];
    let mut tag_buf = [0u8; 1];

    loop {
        tokio::select! {
            result = server_reader.read_exact(&mut tag_buf) => {
                if let Err(e) = result {
                    break Err(anyhow::anyhow!("attach socket closed unexpectedly: {e}"));
                }
                let tag = tag_buf[0];
                let frame = match read_server_frame(&mut server_reader, tag).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break Err(anyhow::anyhow!(
                        "attach socket EOF mid-frame (tag={tag:#04x})"
                    )),
                    Err(e) => break Err(anyhow::anyhow!(
                        "decoding server frame (tag={tag:#04x}): {e}"
                    )),
                };
                match frame {
                    ServerFrame::Output(bytes) => {
                        terminal_output.write_all(&bytes).with_context(|| {
                            format!("stdout closed while writing Output ({} bytes)", bytes.len())
                        })?;
                        terminal_output.flush().context("stdout flush failed")?;
                    }
                    ServerFrame::Shutdown => break Ok(()),
                    ServerFrame::Bell => {
                        terminal_output
                            .write_all(b"\x07")
                            .context("stdout closed while writing Bell")?;
                        terminal_output
                            .flush()
                            .context("stdout flush failed after Bell")?;
                    }
                    ServerFrame::HostOpenUrl(url) => {
                        if let Err(err) = open_host_url(&url) {
                            jackin_diagnostics::debug_log!(
                                "attach",
                                "host open URL failed for {url:?}: {err:#}"
                            );
                        }
                    }
                    ServerFrame::Welcome { .. } | ServerFrame::SessionList(_) => {}
                }
            }

            result = terminal_input.read(&mut stdin_buf) => {
                let n = match result {
                    Ok(0) => break Ok(()),
                    Err(e) => break Err(anyhow::anyhow!("stdin read failed: {e}")),
                    Ok(n) => n,
                };
                let input = &stdin_buf[..n];
                let frame = match read_image_for_paste_trigger(input).await {
                    Ok(Some(image)) => {
                        jackin_diagnostics::debug_log!(
                            "attach",
                            "host clipboard image paste: format={:?} bytes={}",
                            image.format,
                            image.bytes.len()
                        );
                        ClientFrame::ClipboardImage(image)
                    }
                    Ok(None) => ClientFrame::Input(input.to_vec()),
                    Err(err) => {
                        jackin_diagnostics::debug_log!(
                            "attach",
                            "host clipboard image paste probe failed: {err:#}"
                        );
                        ClientFrame::Input(input.to_vec())
                    }
                };
                let msg = match encode_client(frame) {
                    Ok(msg) => msg,
                    Err(err) => {
                        jackin_diagnostics::debug_log!(
                            "attach",
                            "host clipboard image frame rejected; forwarding original input: {err:#}"
                        );
                        encode_client(ClientFrame::Input(input.to_vec()))
                            .context("encoding fallback Input frame")?
                    }
                };
                server_writer
                    .write_all(&msg)
                    .await
                    .context("attach socket write failed (input)")?;
            }

            _ = winch.recv() => {
                let (rows, cols) = terminal_size();
                let msg = encode_client(ClientFrame::Resize { rows, cols })
                    .context("encoding Resize frame")?;
                server_writer
                    .write_all(&msg)
                    .await
                    .context("attach socket write failed (resize)")?;
            }
        }
    }
}

fn normalize_size(rows: u16, cols: u16) -> (u16, u16) {
    let rows = if rows == 0 { DEFAULT_ROWS } else { rows }.max(MIN_ROWS);
    let cols = if cols == 0 { DEFAULT_COLS } else { cols }.max(MIN_COLS);
    (rows, cols)
}

fn terminal_size() -> (u16, u16) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((DEFAULT_COLS, DEFAULT_ROWS));
    normalize_size(rows, cols)
}

fn open_host_url(url: &str) -> Result<()> {
    let (program, args) =
        host_open_command(url).ok_or_else(|| anyhow::anyhow!("unsupported URL or host OS"))?;
    StdCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("starting host URL opener for {url:?}"))?;
    Ok(())
}

fn host_open_command(url: &str) -> Option<(&'static str, Vec<String>)> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    if cfg!(target_os = "macos") {
        Some(("open", vec![url.to_owned()]))
    } else if cfg!(target_os = "linux") {
        Some(("xdg-open", vec![url.to_owned()]))
    } else if cfg!(target_os = "windows") {
        Some((
            "rundll32",
            vec!["url.dll,FileProtocolHandler".to_owned(), url.to_owned()],
        ))
    } else {
        None
    }
}

fn outer_terminal_reset_sequence() -> Vec<u8> {
    let mut seq = OUTER_TERMINAL_RESET_BASE.to_vec();
    if !jackin_diagnostics::host_screen_owned() {
        seq.extend_from_slice(ALTERNATE_SCREEN_LEAVE);
    }
    seq
}

fn enter_host_attach_terminal(stdout: &mut std::io::Stdout) -> Result<RawModeGuard> {
    crossterm::terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let cleanup = RawModeGuard;
    if jackin_diagnostics::host_screen_owned() {
        stdout.write_all(RESET_CLEAR_HOME)?;
    } else {
        stdout.write_all(b"\x1b[?1049h")?;
        stdout.write_all(RESET_CLEAR_HOME)?;
    }
    stdout.write_all(CLIENT_OWNED_MODE_STATE)?;
    stdout.flush()?;
    Ok(cleanup)
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout().lock();
        if let Err(err) = stdout
            .write_all(&outer_terminal_reset_sequence())
            .and_then(|()| stdout.flush())
        {
            tracing::warn!("host attach: failed to write terminal reset on detach: {err}");
        }
        if let Err(err) = crossterm::terminal::disable_raw_mode() {
            tracing::warn!("host attach: failed to disable raw mode on detach: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use jackin_protocol::attach::{
        ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_server, read_client_frame,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    use super::*;

    #[test]
    fn normalize_size_substitutes_zero_and_clamps_minimums() {
        assert_eq!(normalize_size(0, 0), (DEFAULT_ROWS, DEFAULT_COLS));
        assert_eq!(normalize_size(1, 1), (MIN_ROWS, MIN_COLS));
        assert_eq!(normalize_size(40, 120), (40, 120));
    }

    #[test]
    fn host_open_command_rejects_non_http_urls() {
        assert!(host_open_command("file:///tmp/report.html").is_none());
        assert!(host_open_command("javascript:alert(1)").is_none());
    }

    #[test]
    fn host_open_command_accepts_http_urls() {
        let Some((_program, args)) =
            host_open_command("https://github.com/jackin-project/jackin/actions/runs/1")
        else {
            panic!("http(s) URL should produce a host opener command on supported test platforms");
        };
        assert!(args.iter().any(|arg| arg.contains("github.com")));
    }

    #[tokio::test]
    async fn attach_protocol_sends_hello_with_spawn_focus_env_and_terminal() {
        let (client, mut server) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client);
        let mut output = Vec::new();
        let request = HostAttachRequest {
            spawn_request: Some(SpawnRequest::AgentWithProvider {
                slug: "codex".to_owned(),
                provider_label: "MiniMax".to_owned(),
            }),
            focus_session: Some(42),
            env: vec![("JACKIN_GIT_DCO".to_owned(), "1".to_owned())],
            terminal: ClientTerminal {
                term: Some("xterm-ghostty".to_owned()),
                term_program: Some("ghostty".to_owned()),
                colorterm: None,
                default_fg: None,
                default_bg: None,
            },
        };

        let server_task = tokio::spawn(async move {
            let mut tag = [0u8; 1];
            server.read_exact(&mut tag).await.unwrap();
            let frame = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Shutdown))
                .await
                .unwrap();
            frame
        });

        let (_input_writer, input_reader) = duplex(64);
        let winch = signal(SignalKind::window_change()).unwrap();
        run_attach_protocol(
            client_reader,
            client_writer,
            input_reader,
            Cursor::new(&mut output),
            30,
            100,
            request,
            Vec::new(),
            winch,
        )
        .await
        .unwrap();

        assert_eq!(
            server_task.await.unwrap(),
            ClientFrame::Hello {
                rows: 30,
                cols: 100,
                spawn: Some(SpawnRequest::AgentWithProvider {
                    slug: "codex".to_owned(),
                    provider_label: "MiniMax".to_owned(),
                }),
                env: vec![("JACKIN_GIT_DCO".to_owned(), "1".to_owned())],
                focus_session: Some(42),
                terminal: ClientTerminal {
                    term: Some("xterm-ghostty".to_owned()),
                    term_program: Some("ghostty".to_owned()),
                    colorterm: None,
                    default_fg: None,
                    default_bg: None,
                },
            }
        );
    }

    #[tokio::test]
    async fn attach_protocol_forwards_terminal_input_as_input_frames() {
        let (client, mut server) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client);
        let request = HostAttachRequest {
            spawn_request: None,
            focus_session: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
        };

        let server_task = tokio::spawn(async move {
            let mut tag = [0u8; 1];
            server.read_exact(&mut tag).await.unwrap();
            let _hello = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server.read_exact(&mut tag).await.unwrap();
            let input = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Shutdown))
                .await
                .unwrap();
            input
        });

        let (mut input_writer, input_reader) = duplex(64);
        input_writer.write_all(b"abc").await.unwrap();
        let winch = signal(SignalKind::window_change()).unwrap();
        run_attach_protocol(
            client_reader,
            client_writer,
            input_reader,
            Cursor::new(Vec::<u8>::new()),
            24,
            80,
            request,
            Vec::new(),
            winch,
        )
        .await
        .unwrap();

        assert_eq!(
            server_task.await.unwrap(),
            ClientFrame::Input(b"abc".to_vec())
        );
    }

    #[tokio::test]
    async fn attach_protocol_forwards_initial_query_leftovers_as_input() {
        let (client, mut server) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client);
        let request = HostAttachRequest {
            spawn_request: None,
            focus_session: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
        };

        let server_task = tokio::spawn(async move {
            let mut tag = [0u8; 1];
            server.read_exact(&mut tag).await.unwrap();
            let _hello = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server.read_exact(&mut tag).await.unwrap();
            let input = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Shutdown))
                .await
                .unwrap();
            input
        });

        let (_input_writer, input_reader) = duplex(64);
        let winch = signal(SignalKind::window_change()).unwrap();
        run_attach_protocol(
            client_reader,
            client_writer,
            input_reader,
            Cursor::new(Vec::<u8>::new()),
            24,
            80,
            request,
            b"typed-before-attach".to_vec(),
            winch,
        )
        .await
        .unwrap();

        assert_eq!(
            server_task.await.unwrap(),
            ClientFrame::Input(b"typed-before-attach".to_vec())
        );
    }
}
