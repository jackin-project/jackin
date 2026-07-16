// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-owned attach client for running Capsule daemons.
//!
//! This is the host-side twin of the in-container `jackin-capsule`
//! interactive client. It owns the operator terminal and speaks the
//! shared attach protocol over either the bind-mounted Capsule socket
//! or a stdio `attach-proxy` running inside the container.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::host_colors::query_host_terminal_colors;
use anyhow::{Context, Result, bail};
use directories::UserDirs;
use jackin_core::JackinPaths;
use jackin_core::container_paths;
use jackin_protocol::attach::{
    AttachControlOperation, AttachControlRequest, AttachControlResult, ClientFrame, ClientTerminal,
    ClipboardImage, ClipboardImageChunk, ClipboardImageEnd, ClipboardImageStart, FileExportChunk,
    FileExportEnd, FileExportStart, MAX_CLIPBOARD_IMAGE_CHUNK_BYTES,
    MAX_CLIPBOARD_IMAGE_ERROR_BYTES, MAX_CONTEXTUAL_CLIPBOARD_IMAGE_BYTES, MAX_HOST_NOTICE_BYTES,
    ServerFrame, SpawnRequest, encode_client, read_server_frame,
};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};

use super::attach::{
    HostAttachTransportPlan, attach_proxy_exec_args, select_host_attach_transport,
};
use jackin_host::host_clipboard::{
    is_image_paste_trigger, read_host_clipboard_image, read_host_clipboard_text_path_image,
    read_image_for_paste_trigger, read_image_from_pasted_path,
};
use jackin_host::host_desktop::{open_host_file, open_host_url, reveal_host_file};

pub const JACKIN_HOST_ATTACH_ENV: &str = "JACKIN_HOST_ATTACH";

const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
const MIN_ROWS: u16 = 6;
const MIN_COLS: u16 = 3;
const HOST_FILE_EXPORT_IDLE_TIMEOUT: Duration = Duration::from_mins(5);
const HOST_FILE_EXPORT_CLEANUP_TICK: Duration = Duration::from_secs(30);

const OUTER_TERMINAL_RESET_BASE: &[u8] =
    b"\x1b[0m\x1b]22;default\x1b\\\x1b[?7h\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1007l\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b[<u\x1b[?25h";
const ALTERNATE_SCREEN_LEAVE: &[u8] = b"\x1b[?1049l";
const RESET_CLEAR_HOME: &[u8] = b"\x1b[0m\x1b[2J\x1b[H";
const CLIENT_OWNED_MODE_STATE: &[u8] =
    b"\x1b[?7l\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1005l\x1b[?1015l\x1b[?1007l\x1b[?1003h\x1b[?1006h\x1b[?1004h";
const HOST_FILE_EXPORT_DESTINATION_CATEGORY: &str = "host-downloads-jackin-instance";
const RPC_ERROR: &str = jackin_telemetry::schema::enums::ErrorType::RpcError.as_str();

fn log_clipboard_image_paste_trigger() {
    jackin_diagnostics::emit_compact_line(
        "clipboard-image",
        "clipboard-image: paste trigger source=clipboard",
    );
}

fn log_clipboard_image_no_image_forwarded() {
    jackin_diagnostics::emit_compact_line(
        "clipboard-image",
        "clipboard-image: no-image source=clipboard text-paste=forwarded",
    );
}

fn log_clipboard_image_pasted_path_staged() {
    jackin_diagnostics::emit_compact_line(
        "clipboard-image",
        "clipboard-image: pasted-path staged source=paste",
    );
}

#[must_use]
pub fn host_attach_enabled() -> bool {
    #[cfg(test)]
    {
        false
    }
    #[cfg(not(test))]
    {
        std::env::var_os(JACKIN_HOST_ATTACH_ENV).is_some()
    }
}

pub(super) async fn run_host_attach_session(
    paths: &JackinPaths,
    container_name: &str,
    spawn_request: Option<SpawnRequest>,
    focus_session: Option<u64>,
    env_overrides: &[(String, String)],
) -> Result<()> {
    let _session = jackin_telemetry::identity::SessionGuard::begin_or_reuse();
    let request = HostAttachRequest {
        spawn_request,
        focus_session,
        env: env_overrides.to_vec(),
        terminal: ClientTerminal::from_env(),
        export_subdir: sanitize_export_path_component(container_name, "instance"),
    };

    match select_host_attach_transport(paths, container_name) {
        HostAttachTransportPlan::DirectSocket { socket_path } => {
            jackin_diagnostics::telemetry_debug!(
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
            jackin_diagnostics::telemetry_debug!(
                "attach",
                "host attach using attach-proxy for {} (direct_error={:?})",
                socket_path.display(),
                direct_error
            );
            let process_request =
                jackin_process::ExecRequest::new("docker", attach_proxy_exec_args(container_name))
                    .stdin_mode(jackin_process::StdioMode::Capture)
                    .stdout_mode(jackin_process::StdioMode::Capture)
                    .stderr_mode(jackin_process::StdioMode::Inherit);
            let mut child = jackin_process::spawn_async(&process_request)
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
    export_subdir: String,
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

#[expect(
    clippy::too_many_lines,
    reason = "Attach-protocol async loop driving the host's request/response \
              exchange with the capsule daemon. Body extraction follows the \
              same deferred-parallel-pass plan as the launch fns — the inline \
              shape preserves captured socket + reader + writer borrows across \
              the protocol phases."
)]
#[expect(
    clippy::too_many_arguments,
    reason = "Attach-protocol call site propagates the four server/terminal stream \
              handles plus geometry, request payload, initial input, and the winch \
              signal. Named-arg reads match the per-input propagation idiom; \
              bundling into a config struct is the deferred-parallel-pass."
)]
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
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str("jackin.capsule.Attach/Handshake"),
        },
    ];
    let mut handshake_operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let mut context = jackin_protocol::TelemetryContext::v1();
    if let Some(operation) = handshake_operation.as_ref() {
        operation
            .span()
            .in_scope(|| jackin_telemetry::propagation::inject(&mut context));
    } else {
        jackin_telemetry::propagation::inject(&mut context);
    }
    let hello = encode_client(ClientFrame::Hello {
        rows,
        cols,
        env: request.env,
        spawn: request.spawn_request,
        terminal: request.terminal,
        focus_session: request.focus_session,
        context: Some(Box::new(context)),
    })
    .context("encoding attach Hello frame");
    let hello = match hello {
        Ok(hello) => hello,
        Err(error) => {
            if let Some(operation) = handshake_operation {
                operation.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Failure,
                    Some(RPC_ERROR),
                );
            }
            return Err(error);
        }
    };
    let hello_result = server_writer
        .write_all(&hello)
        .await
        .context("sending attach Hello frame");
    if let Err(error) = hello_result {
        if let Some(operation) = handshake_operation {
            operation.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(RPC_ERROR),
            );
        }
        return Err(error);
    }
    let mut stdin_buf = [0u8; 4096];
    let mut tag_buf = [0u8; 1];
    let mut file_exports = HostFileExports::new(request.export_subdir.clone());
    let mut attach_operations = HashMap::new();
    let mut detach_request = None;
    let mut export_cleanup_tick = tokio::time::interval(HOST_FILE_EXPORT_CLEANUP_TICK);
    export_cleanup_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let result: Result<()> = async {
        if !initial_input.is_empty() {
            let msg = encode_client(ClientFrame::Input(initial_input))
                .context("encoding pre-attach Input frame")?;
            server_writer
                .write_all(&msg)
                .await
                .context("attach socket write failed (pre-attach input)")?;
        }
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
                    ServerFrame::Shutdown { reason: _ } => break Ok(()),
                    ServerFrame::Bell => {
                        terminal_output
                            .write_all(b"\x07")
                            .context("stdout closed while writing Bell")?;
                        terminal_output
                            .flush()
                            .context("stdout flush failed after Bell")?;
                    }
                    ServerFrame::HostOpenUrl(url) => {
                        let message = match open_host_url(&url) {
                            Ok(()) => "Opening URL in host browser".to_owned(),
                            Err(err) => {
                                let redacted = jackin_core::redact_url_for_log(&url);
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host open URL failed for {redacted:?}: {err:#}"
                                );
                                format!("Host open URL failed: {err:#}")
                            }
                        };
                        if let Err(err) = send_host_notice(&mut server_writer, &message).await {
                            jackin_diagnostics::telemetry_debug!(
                                "attach",
                                "host open URL notice failed: {err:#}"
                            );
                        }
                    }
                    ServerFrame::HostRevealPath(_) => {
                        let message = "Local telemetry files are not supported";
                        if let Err(err) = send_host_notice(&mut server_writer, message).await {
                            jackin_diagnostics::telemetry_debug!(
                                "attach",
                                "host reveal path notice failed: {err:#}"
                            );
                        }
                    }
                    ServerFrame::HostStageImageFromClipboardPath => {
                        write_clipboard_image_request_result(
                            &mut server_writer,
                            &mut attach_operations,
                            read_host_clipboard_text_path_image().await,
                            "host clipboard text is not an absolute readable image path or file:// image URL",
                            "host clipboard image path probe failed",
                            "host clipboard image path response failed",
                        )
                        .await;
                    }
                    // Paste and Stage differ on the Capsule side; the host response
                    // (probe the clipboard for a readable image) is identical.
                    ServerFrame::HostPasteImageFromClipboard
                    | ServerFrame::HostStageImageFromClipboard => {
                        write_clipboard_image_request_result(
                            &mut server_writer,
                            &mut attach_operations,
                            read_host_clipboard_image().await,
                            "host clipboard does not contain a readable image",
                            "host clipboard image probe failed",
                            "host clipboard image response failed",
                        )
                        .await;
                    }
                    ServerFrame::FileExportStart(start) => {
                        if let Err(err) = file_exports.start(start) {
                            jackin_diagnostics::telemetry_debug!(
                                "attach",
                                "host file export start failed: {err:#}"
                            );
                            let message = format!("File export rejected: {err:#}");
                            if let Err(notice_err) =
                                send_host_notice(&mut server_writer, &message).await
                            {
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host file export start notice failed: {notice_err:#}"
                                );
                            }
                        }
                    }
                    ServerFrame::FileExportChunk(chunk) => {
                        let transfer_id = chunk.transfer_id;
                        if let Err(err) = file_exports.chunk(chunk) {
                            file_exports.abort(transfer_id);
                            jackin_diagnostics::telemetry_debug!(
                                "attach",
                                "host file export chunk failed: {err:#}"
                            );
                            let message = format!("File export rejected: {err:#}");
                            if let Err(notice_err) =
                                send_host_notice(&mut server_writer, &message).await
                            {
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host file export chunk notice failed: {notice_err:#}"
                                );
                            }
                        }
                    }
                    ServerFrame::FileExportEnd(end) => {
                        let message = match file_exports.end(end) {
                            Ok(export) => file_export_success_notice(&export),
                            Err(err) => {
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host file export end failed: {err:#}"
                                );
                                format!("File export rejected: {err:#}")
                            }
                        };
                        if let Err(err) = send_host_notice(&mut server_writer, &message).await {
                            jackin_diagnostics::telemetry_debug!(
                                "attach",
                                "host file export end notice failed: {err:#}"
                            );
                        }
                    }
                    ServerFrame::Welcome { .. } => {
                        if let Some(operation) = handshake_operation.take() {
                            operation.complete(
                                jackin_telemetry::schema::enums::OutcomeValue::Success,
                                None,
                            );
                        }
                    }
                    ServerFrame::SessionList(_) => {}
                    ServerFrame::AttachControlResponse(response) => {
                        if let Some(operation) = attach_operations.remove(&response.request_id) {
                            let succeeded = response.result == AttachControlResult::Success;
                            operation.complete(
                                if succeeded {
                                    jackin_telemetry::schema::enums::OutcomeValue::Success
                                } else {
                                    jackin_telemetry::schema::enums::OutcomeValue::Failure
                                },
                                (!succeeded).then_some(RPC_ERROR),
                            );
                        }
                        if detach_request == Some(response.request_id) {
                            break Ok(());
                        }
                    }
                }
            }

            result = terminal_input.read(&mut stdin_buf), if detach_request.is_none() => {
                let n = match result {
                    Ok(0) => {
                        let (request_id, context) = begin_attach_control(
                            &mut attach_operations,
                            "jackin.capsule.Attach/Detach",
                        );
                        write_attach_control(
                            &mut server_writer,
                            &mut attach_operations,
                            request_id,
                            &context,
                            AttachControlOperation::Detach,
                        )
                        .await?;
                        detach_request = Some(request_id);
                        continue;
                    }
                    Err(e) => break Err(anyhow::anyhow!("stdin read failed: {e}")),
                    Ok(n) => n,
                };
                let input = &stdin_buf[..n];
                if matches!(input, b"\x1b[I" | b"\x1b[O") {
                    let (request_id, context) = begin_attach_control(
                        &mut attach_operations,
                        "jackin.capsule.Attach/Focus",
                    );
                    let operation = if input == b"\x1b[I" {
                        AttachControlOperation::FocusIn
                    } else {
                        AttachControlOperation::FocusOut
                    };
                    write_attach_control(
                        &mut server_writer,
                        &mut attach_operations,
                        request_id,
                        &context,
                        operation,
                    )
                    .await?;
                    continue;
                }
                // The two image sources are mutually exclusive by construction:
                // Ctrl+V is the lone trigger byte (no surrounding bytes), while the
                // pasted-path probe matches a bracketed paste. Branch on the trigger
                // so exclusivity is structural. The pasted-path read also carries any
                // bytes sharing the read around the paste body; those are forwarded
                // below so a coincident keystroke/mouse report is not dropped when the
                // body is consumed as an image.
                let staged: Option<(ClipboardImage, &[u8], &[u8])> =
                    if is_image_paste_trigger(input) {
                        log_clipboard_image_paste_trigger();
                        match read_image_for_paste_trigger(input).await {
                            Ok(Some(image)) => {
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host clipboard image paste: format={:?} bytes={}",
                                    image.format,
                                    image.bytes.len()
                                );
                                Some((image, &[][..], &[][..]))
                            }
                            Ok(None) => {
                                log_clipboard_image_no_image_forwarded();
                                None
                            }
                            Err(err) => {
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host clipboard image paste probe failed: {err:#}"
                                );
                                log_clipboard_image_no_image_forwarded();
                                None
                            }
                        }
                    } else {
                        // Cmd+V parity: a bracketed paste whose body is a real host
                        // image file is auto-staged as an image frame instead of
                        // forwarding the raw path text (the container path is
                        // substituted downstream). Everything else forwards as text.
                        match read_image_from_pasted_path(input).await {
                            Ok(Some((image, prefix, suffix))) => {
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host pasted-path image: format={:?} bytes={}",
                                    image.format,
                                    image.bytes.len()
                                );
                                log_clipboard_image_pasted_path_staged();
                                Some((image, prefix, suffix))
                            }
                            Ok(None) => None,
                            Err(err) => {
                                jackin_diagnostics::telemetry_debug!(
                                    "attach",
                                    "host pasted-path image probe failed: {err:#}"
                                );
                                None
                            }
                        }
                    };
                if let Some((image, prefix, suffix)) = staged {
                    // Bytes typed before the paste must reach the agent ahead of the
                    // image, preserving wire order (prefix → image → suffix).
                    if !prefix.is_empty() {
                        write_input_frame(&mut server_writer, prefix, "paste prefix").await?;
                    }
                    match write_clipboard_image_frames(
                        &mut server_writer,
                        &mut attach_operations,
                        image,
                    )
                    .await
                    {
                        Ok(()) => {
                            if !suffix.is_empty() {
                                write_input_frame(&mut server_writer, suffix, "paste suffix").await?;
                            }
                        }
                        Err(err) => {
                            jackin_diagnostics::telemetry_debug!(
                                "attach",
                                "host clipboard image frame rejected; forwarding original input: {err:#}"
                            );
                            // The prefix already went out, so forward the remainder of
                            // the read verbatim (markers + body + suffix) to reconstruct
                            // the original input exactly — no double-send.
                            write_input_frame(&mut server_writer, &input[prefix.len()..], "input fallback")
                                .await?;
                        }
                    }
                } else {
                    write_input_frame(&mut server_writer, input, "input").await?;
                }
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

            _ = export_cleanup_tick.tick() => {
                let cleaned = file_exports.abort_idle_older_than(HOST_FILE_EXPORT_IDLE_TIMEOUT);
                if cleaned > 0 {
                    let message = format!(
                        "File export interrupted: cleaned up {cleaned} temporary host file{}",
                        if cleaned == 1 { "" } else { "s" }
                    );
                    if let Err(err) = send_host_notice(&mut server_writer, &message).await {
                        jackin_diagnostics::telemetry_debug!(
                            "attach",
                            "host file export cleanup notice failed: {err:#}"
                        );
                    }
                }
            }
            }
        }
    }
    .await;
    for (_, operation) in attach_operations {
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        );
    }
    if let Some(operation) = handshake_operation {
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        );
    }
    result
}

struct HostFileExports {
    export_subdir: String,
    active: HashMap<u64, ActiveHostFileExport>,
}

struct ActiveHostFileExport {
    source_path: String,
    source_basename: String,
    final_path: PathBuf,
    temp_path: PathBuf,
    file: File,
    expected_size: u64,
    written: u64,
    hasher: Sha256,
    reveal_after_export: bool,
    open_after_export: bool,
    last_activity: Instant,
}

#[derive(Debug)]
struct CompletedHostFileExport {
    final_path: PathBuf,
    bytes: u64,
    reveal_after_export: bool,
    open_after_export: bool,
}

impl HostFileExports {
    fn new(export_subdir: String) -> Self {
        Self {
            export_subdir,
            active: HashMap::new(),
        }
    }

    fn start(&mut self, start: FileExportStart) -> Result<()> {
        let root = host_file_export_root(&self.export_subdir)?;
        self.start_in_root(start, &root)
    }

    fn start_in_root(&mut self, start: FileExportStart, root: &Path) -> Result<()> {
        if self.active.contains_key(&start.transfer_id) {
            bail!("file export transfer {} already active", start.transfer_id);
        }
        fs::create_dir_all(root).context("creating host export directory")?;
        let file_name = sanitize_export_file_name(&start.file_name);
        let final_path = unique_export_path(root, &file_name);
        let temp_path = final_path.with_extension(format!(
            "{}part",
            final_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| format!("{ext}."))
                .unwrap_or_default()
        ));
        #[expect(
            clippy::disallowed_methods,
            reason = "host file export writes run in the foreground host attach client, not in Capsule render code"
        )]
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .context("creating temporary host export file")?;
        jackin_diagnostics::telemetry_debug!(
            "attach",
            "host file export start transfer_id={} source_category={} basename={:?} bytes={} destination_category={} destination_basename={:?} reveal_after_export={} open_after_export={}",
            start.transfer_id,
            export_source_path_category(&start.source_path),
            file_name,
            start.size,
            HOST_FILE_EXPORT_DESTINATION_CATEGORY,
            host_file_basename(&final_path),
            start.reveal_after_export,
            start.open_after_export
        );
        self.active.insert(
            start.transfer_id,
            ActiveHostFileExport {
                source_path: start.source_path,
                final_path,
                temp_path,
                file,
                expected_size: start.size,
                source_basename: file_name,
                written: 0,
                hasher: Sha256::new(),
                reveal_after_export: start.reveal_after_export,
                open_after_export: start.open_after_export,
                last_activity: Instant::now(),
            },
        );
        Ok(())
    }

    fn chunk(&mut self, chunk: FileExportChunk) -> Result<()> {
        let Some(active) = self.active.get_mut(&chunk.transfer_id) else {
            bail!(
                "file export transfer {} has no active start",
                chunk.transfer_id
            );
        };
        if chunk.offset != active.written {
            bail!(
                "file export transfer {} offset {} did not match expected {}",
                chunk.transfer_id,
                chunk.offset,
                active.written
            );
        }
        let chunk_len = u64::try_from(chunk.bytes.len())
            .map_err(|_| anyhow::anyhow!("file export chunk length overflow"))?;
        let new_written = active
            .written
            .checked_add(chunk_len)
            .ok_or_else(|| anyhow::anyhow!("file export written byte count overflow"))?;
        if new_written > active.expected_size {
            bail!(
                "file export transfer {} wrote {new_written} bytes, expected {}",
                chunk.transfer_id,
                active.expected_size
            );
        }
        active
            .file
            .seek(SeekFrom::Start(active.written))
            .context("seeking host export temp file")?;
        active
            .file
            .write_all(&chunk.bytes)
            .context("writing host export chunk")?;
        active.hasher.update(&chunk.bytes);
        active.written = new_written;
        active.last_activity = Instant::now();
        Ok(())
    }

    fn end(&mut self, end: FileExportEnd) -> Result<CompletedHostFileExport> {
        let Some(mut active) = self.active.remove(&end.transfer_id) else {
            bail!(
                "file export transfer {} has no active start",
                end.transfer_id
            );
        };
        if active.written != active.expected_size {
            drop(fs::remove_file(&active.temp_path));
            bail!(
                "file export transfer {} ended after {} bytes, expected {}",
                end.transfer_id,
                active.written,
                active.expected_size
            );
        }
        active
            .file
            .flush()
            .context("flushing host export temp file")?;
        active
            .file
            .sync_all()
            .context("syncing host export temp file")?;
        drop(active.file);
        let actual: [u8; 32] = active.hasher.finalize().into();
        if actual != end.sha256 {
            drop(fs::remove_file(&active.temp_path));
            jackin_diagnostics::telemetry_debug!(
                "attach",
                "host file export digest mismatch transfer_id={} source_category={} bytes={} expected_sha256={} actual_sha256={} destination_category={}",
                end.transfer_id,
                export_source_path_category(&active.source_path),
                active.written,
                hex::encode(end.sha256),
                hex::encode(actual),
                HOST_FILE_EXPORT_DESTINATION_CATEGORY
            );
            bail!("file export transfer {} SHA-256 mismatch", end.transfer_id);
        }
        fs::rename(&active.temp_path, &active.final_path)
            .context("moving temporary host export into final destination")?;
        jackin_diagnostics::telemetry_debug!(
            "attach",
            "host file export committed transfer_id={} source_category={} bytes={} sha256={} destination_category={} destination_basename={:?}",
            end.transfer_id,
            export_source_path_category(&active.source_path),
            active.written,
            hex::encode(actual),
            HOST_FILE_EXPORT_DESTINATION_CATEGORY,
            host_file_basename(&active.final_path)
        );
        jackin_diagnostics::emit_compact_line(
            "host_file_export",
            &host_file_export_compact_line(
                export_source_path_category(&active.source_path),
                &active.source_basename,
                active.written,
            ),
        );
        Ok(CompletedHostFileExport {
            final_path: active.final_path,
            bytes: active.written,
            reveal_after_export: active.reveal_after_export,
            open_after_export: active.open_after_export,
        })
    }

    fn abort(&mut self, transfer_id: u64) {
        if let Some(active) = self.active.remove(&transfer_id) {
            jackin_diagnostics::telemetry_debug!(
                "attach",
                "host file export abort transfer_id={} source_category={} bytes_written={} destination_category={} destination_basename={:?}",
                transfer_id,
                export_source_path_category(&active.source_path),
                active.written,
                HOST_FILE_EXPORT_DESTINATION_CATEGORY,
                host_file_basename(&active.final_path)
            );
            drop(fs::remove_file(active.temp_path));
        }
    }

    fn abort_idle_older_than(&mut self, max_idle: Duration) -> usize {
        let cutoff = Instant::now()
            .checked_sub(max_idle)
            .unwrap_or_else(Instant::now);
        self.abort_idle_before(cutoff)
    }

    fn abort_idle_before(&mut self, cutoff: Instant) -> usize {
        let stale_ids: Vec<u64> = self
            .active
            .iter()
            .filter_map(|(transfer_id, active)| {
                (active.last_activity <= cutoff).then_some(*transfer_id)
            })
            .collect();
        let count = stale_ids.len();
        for transfer_id in stale_ids {
            self.abort(transfer_id);
        }
        count
    }
}

impl Drop for HostFileExports {
    fn drop(&mut self) {
        for (_, active) in self.active.drain() {
            drop(fs::remove_file(active.temp_path));
        }
    }
}

fn host_file_export_root(export_subdir: &str) -> Result<PathBuf> {
    let export_subdir = sanitize_export_path_component(export_subdir, "instance");
    if let Some(downloads) =
        UserDirs::new().and_then(|dirs| dirs.download_dir().map(Path::to_path_buf))
    {
        return Ok(downloads.join("jackin").join(export_subdir));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set and Downloads directory is unavailable"))?;
    Ok(home.join("Downloads").join("jackin").join(export_subdir))
}

fn export_source_path_category(source_path: &str) -> &'static str {
    if container_paths::is_run_owned(source_path) {
        return "jackin-run";
    }
    if container_paths::is_jackin_owned(source_path) {
        return "jackin-owned";
    }
    if source_path.starts_with('/') {
        return "container-absolute";
    }
    "container-relative"
}

fn host_file_export_compact_line(
    source_category: &str,
    source_basename: &str,
    bytes: u64,
) -> String {
    format!(
        "host-file-export: exported source_category={source_category} basename={source_basename:?} bytes={bytes} destination_category={HOST_FILE_EXPORT_DESTINATION_CATEGORY}"
    )
}

fn host_file_basename(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("jackin-export")
        .to_owned()
}

/// Encode and send `bytes` as one `Input` frame. `what` names the call site for
/// the socket-write error context (e.g. "paste prefix").
async fn write_input_frame<W>(writer: &mut W, bytes: &[u8], what: &str) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let msg = encode_client(ClientFrame::Input(bytes.to_vec())).context("encoding Input frame")?;
    writer
        .write_all(&msg)
        .await
        .with_context(|| format!("attach socket write failed ({what})"))?;
    Ok(())
}

fn begin_attach_control(
    operations: &mut HashMap<u64, jackin_telemetry::operation::OperationGuard>,
    method: &'static str,
) -> (u64, jackin_protocol::TelemetryContext) {
    static NEXT_REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let request_id = NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(method),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let mut context = jackin_protocol::TelemetryContext::v1();
    if let Some(operation) = operation.as_ref() {
        operation
            .span()
            .in_scope(|| jackin_telemetry::propagation::inject(&mut context));
    } else {
        jackin_telemetry::propagation::inject(&mut context);
    }
    if let Some(operation) = operation {
        operations.insert(request_id, operation);
    }
    (request_id, context)
}

async fn write_attach_control<W>(
    writer: &mut W,
    operations: &mut HashMap<u64, jackin_telemetry::operation::OperationGuard>,
    request_id: u64,
    context: &jackin_protocol::TelemetryContext,
    operation: AttachControlOperation,
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let result: Result<()> = async {
        let frame = encode_client(ClientFrame::AttachControl(AttachControlRequest {
            request_id,
            context: context.clone(),
            operation,
        }))?;
        writer.write_all(&frame).await?;
        Ok(())
    }
    .await;
    if result.is_err()
        && let Some(operation) = operations.remove(&request_id)
    {
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        );
    }
    result.context("attach control socket write failed")
}

async fn write_clipboard_image_frames<W>(
    writer: &mut W,
    operations: &mut HashMap<u64, jackin_telemetry::operation::OperationGuard>,
    image: ClipboardImage,
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let (request_id, context) =
        begin_attach_control(operations, "jackin.capsule.Attach/ClipboardImageTransfer");
    if image.bytes.len() <= MAX_CONTEXTUAL_CLIPBOARD_IMAGE_BYTES {
        return write_attach_control(
            writer,
            operations,
            request_id,
            &context,
            AttachControlOperation::ClipboardImage(image),
        )
        .await;
    }

    let transfer_id = next_host_transfer_id();
    let size = u64::try_from(image.bytes.len()).context("clipboard image length overflow")?;
    write_attach_control(
        writer,
        operations,
        request_id,
        &context,
        AttachControlOperation::ClipboardImageStart(ClipboardImageStart {
            transfer_id,
            format: image.format.clone(),
            size,
        }),
    )
    .await?;

    let mut hasher = Sha256::new();
    let mut offset = 0u64;
    for chunk in image.bytes.chunks(MAX_CLIPBOARD_IMAGE_CHUNK_BYTES) {
        hasher.update(chunk);
        write_attach_control(
            writer,
            operations,
            request_id,
            &context,
            AttachControlOperation::ClipboardImageChunk(ClipboardImageChunk {
                transfer_id,
                offset,
                bytes: chunk.to_vec(),
            }),
        )
        .await?;
        offset = offset
            .checked_add(u64::try_from(chunk.len()).context("clipboard image chunk overflow")?)
            .ok_or_else(|| anyhow::anyhow!("clipboard image offset overflow"))?;
    }

    let sha256 = hasher.finalize().into();
    write_attach_control(
        writer,
        operations,
        request_id,
        &context,
        AttachControlOperation::ClipboardImageEnd(ClipboardImageEnd {
            transfer_id,
            sha256,
        }),
    )
    .await
}

async fn send_clipboard_image_error<W>(
    writer: &mut W,
    operations: &mut HashMap<u64, jackin_telemetry::operation::OperationGuard>,
    message: &str,
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let message = bounded_attach_message(message, MAX_CLIPBOARD_IMAGE_ERROR_BYTES);
    let (request_id, context) =
        begin_attach_control(operations, "jackin.capsule.Attach/ClipboardImageTransfer");
    write_attach_control(
        writer,
        operations,
        request_id,
        &context,
        AttachControlOperation::ClipboardImageError(
            jackin_protocol::attach::ClipboardImageError::from_message(message),
        ),
    )
    .await
}

async fn write_clipboard_image_request_result<W>(
    writer: &mut W,
    operations: &mut HashMap<u64, jackin_telemetry::operation::OperationGuard>,
    image: Result<Option<ClipboardImage>>,
    empty_message: &str,
    probe_log_message: &str,
    response_log_message: &str,
) where
    W: AsyncWrite + Unpin,
{
    let result = match image {
        Ok(Some(image)) => write_clipboard_image_frames(writer, operations, image).await,
        Ok(None) => send_clipboard_image_error(writer, operations, empty_message).await,
        Err(err) => {
            jackin_diagnostics::telemetry_debug!("attach", "{probe_log_message}: {err:#}");
            send_clipboard_image_error(writer, operations, &format!("{probe_log_message}: {err:#}"))
                .await
        }
    };
    if let Err(err) = result {
        jackin_diagnostics::telemetry_debug!("attach", "{response_log_message}: {err:#}");
    }
}

async fn send_host_notice<W>(writer: &mut W, message: &str) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let message = bounded_attach_message(message, MAX_HOST_NOTICE_BYTES);
    let msg =
        encode_client(ClientFrame::HostNotice(message)).context("encoding HostNotice frame")?;
    writer
        .write_all(&msg)
        .await
        .context("attach socket write failed (host notice)")?;
    Ok(())
}

fn bounded_attach_message(message: &str, max_bytes: usize) -> String {
    const ELLIPSIS: &str = "...";

    let trimmed = message.trim();
    let message = if trimmed.is_empty() {
        "Host action failed"
    } else {
        trimmed
    };
    if message.len() <= max_bytes {
        return message.to_owned();
    }

    let keep = max_bytes.saturating_sub(ELLIPSIS.len());
    let mut boundary = keep;
    while boundary > 0 && !message.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}{}", &message[..boundary], ELLIPSIS)
}

fn next_host_transfer_id() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_nanos().try_into().unwrap_or(duration.as_secs())
        })
}

fn sanitize_export_file_name(name: &str) -> String {
    sanitize_export_path_component(name, "jackin-export.bin")
}

fn sanitize_export_path_component(name: &str, fallback: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '\0') {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(['.', ' ', '\t']).trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.chars().take(255).collect()
    }
}

fn unique_export_path(root: &Path, file_name: &str) -> PathBuf {
    let candidate = root.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("jackin-export");
    let extension = path.extension().and_then(|ext| ext.to_str());
    for idx in 1.. {
        let name = match extension {
            Some(ext) if !ext.is_empty() => format!("{stem}-{idx}.{ext}"),
            _ => format!("{stem}-{idx}"),
        };
        let candidate = root.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded unique export path search should always return");
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

/// Run a post-export desktop action (`open`/`reveal`) and build the user
/// notice. `success_verb` is the past-tense word for the OK message ("opened",
/// "revealed"); `fail_verb` is the bare action word reused in both the debug
/// log and the failure notice ("open", "reveal").
fn export_action_notice(
    export: &CompletedHostFileExport,
    action: impl FnOnce(&Path) -> Result<()>,
    success_verb: &str,
    fail_verb: &str,
) -> String {
    match action(&export.final_path) {
        Ok(()) => format!(
            "File exported and {success_verb}: {} ({} bytes)",
            export.final_path.display(),
            export.bytes
        ),
        Err(err) => {
            jackin_diagnostics::telemetry_debug!(
                "attach",
                "host file export {fail_verb} failed for destination_basename={:?}: {err:#}",
                host_file_basename(&export.final_path)
            );
            format!(
                "File exported; {fail_verb} failed: {} ({} bytes)",
                export.final_path.display(),
                export.bytes
            )
        }
    }
}

fn file_export_success_notice(export: &CompletedHostFileExport) -> String {
    if export.open_after_export {
        return export_action_notice(export, open_host_file, "opened", "open");
    }
    if !export.reveal_after_export {
        return format!(
            "File exported: {} ({} bytes)",
            export.final_path.display(),
            export.bytes
        );
    }
    export_action_notice(export, reveal_host_file, "revealed", "reveal")
}

fn outer_terminal_reset_sequence() -> Vec<u8> {
    let mut seq = OUTER_TERMINAL_RESET_BASE.to_vec();
    if !jackin_diagnostics::host_screen_owned() {
        seq.extend_from_slice(ALTERNATE_SCREEN_LEAVE);
    }
    seq
}

fn enter_host_attach_terminal(stdout: &mut std::io::Stdout) -> Result<RawModeGuard> {
    // Only enter raw mode when stdin is a real terminal. In a headless context
    // (CI, piped stdio, tests) `enable_raw_mode` fails with "Device not
    // configured (os error 6)"; skipping it lets the non-interactive path proceed
    // and the guard no-ops its raw-mode teardown on drop.
    let raw_mode = std::io::IsTerminal::is_terminal(&std::io::stdin());
    if raw_mode {
        crossterm::terminal::enable_raw_mode().context("failed to enable raw mode")?;
    }
    let cleanup = RawModeGuard { raw_mode };
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

struct RawModeGuard {
    /// Whether `enter_host_attach_terminal` actually enabled raw mode (only on a
    /// real terminal); the drop teardown disables it only when it was enabled.
    raw_mode: bool,
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout().lock();
        if let Err(err) = stdout
            .write_all(&outer_terminal_reset_sequence())
            .and_then(|()| stdout.flush())
        {
            jackin_diagnostics::telemetry_warn!(
                "attach",
                "failed to write terminal reset on detach: {err}"
            );
        }
        if self.raw_mode
            && let Err(err) = crossterm::terminal::disable_raw_mode()
        {
            jackin_diagnostics::telemetry_warn!(
                "attach",
                "failed to disable raw mode on detach: {err}"
            );
        }
    }
}

#[cfg(test)]
mod tests;
