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
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use directories::UserDirs;
use jackin_core::paths::JackinPaths;
use jackin_protocol::attach::{
    ClientFrame, ClientTerminal, ClipboardImage, ClipboardImageChunk, ClipboardImageEnd,
    ClipboardImageStart, FileExportChunk, FileExportEnd, FileExportStart,
    MAX_CLIPBOARD_IMAGE_BYTES, MAX_CLIPBOARD_IMAGE_CHUNK_BYTES, MAX_CLIPBOARD_IMAGE_ERROR_BYTES,
    MAX_HOST_NOTICE_BYTES, ServerFrame, SpawnRequest, encode_client, read_server_frame,
};
use jackin_tui::host_colors::query_host_terminal_colors;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::signal::unix::{SignalKind, signal};

use super::attach::{
    HostAttachTransportPlan, attach_proxy_exec_args, select_host_attach_transport,
};
use super::host_clipboard::{
    is_image_paste_trigger, paste_surrounding_bytes, read_image_for_paste_trigger,
    read_image_from_clipboard, read_image_from_clipboard_text_path, read_image_from_pasted_path,
};
use super::host_desktop::{open_host_file, open_host_url, reveal_host_file};

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
        export_subdir: sanitize_export_path_component(container_name, "instance"),
        diagnostics_run_dir: paths.data_dir.join("diagnostics/runs"),
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
    export_subdir: String,
    diagnostics_run_dir: PathBuf,
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
    let mut file_exports = HostFileExports::new(request.export_subdir.clone());
    let mut export_cleanup_tick = tokio::time::interval(HOST_FILE_EXPORT_CLEANUP_TICK);
    export_cleanup_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

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
                                let redacted = jackin_core::url_text::redact_url_for_log(&url);
                                jackin_diagnostics::debug_log!(
                                    "attach",
                                    "host open URL failed for {redacted:?}: {err:#}"
                                );
                                format!("Host open URL failed: {err:#}")
                            }
                        };
                        if let Err(err) = send_host_notice(&mut server_writer, &message).await {
                            jackin_diagnostics::debug_log!(
                                "attach",
                                "host open URL notice failed: {err:#}"
                            );
                        }
                    }
                    ServerFrame::HostRevealPath(path) => {
                        let message = match reveal_allowed_host_path(
                            Path::new(&path),
                            &request.diagnostics_run_dir,
                        ) {
                            Ok(()) => "Revealing diagnostics file on host".to_owned(),
                            Err(err) => {
                                jackin_diagnostics::debug_log!(
                                    "attach",
                                    "host reveal path rejected for category={} basename={:?}: {err:#}",
                                    host_reveal_path_category(Path::new(&path), &request.diagnostics_run_dir),
                                    host_file_basename(Path::new(&path))
                                );
                                format!("Host reveal rejected: {err:#}")
                            }
                        };
                        if let Err(err) = send_host_notice(&mut server_writer, &message).await {
                            jackin_diagnostics::debug_log!(
                                "attach",
                                "host reveal path notice failed: {err:#}"
                            );
                        }
                    }
                    ServerFrame::HostStageImageFromClipboardPath => {
                        write_clipboard_image_request_result(
                            &mut server_writer,
                            read_image_from_clipboard_text_path().await,
                            "host clipboard text is not an absolute readable image path or file:// image URL",
                            "host clipboard image path probe failed",
                            "host clipboard image path response failed",
                        )
                        .await;
                    }
                    ServerFrame::HostPasteImageFromClipboard => {
                        write_clipboard_image_request_result(
                            &mut server_writer,
                            read_image_from_clipboard().await,
                            "host clipboard does not contain a readable image",
                            "host clipboard image probe failed",
                            "host clipboard image response failed",
                        )
                        .await;
                    }
                    ServerFrame::HostStageImageFromClipboard => {
                        write_clipboard_image_request_result(
                            &mut server_writer,
                            read_image_from_clipboard().await,
                            "host clipboard does not contain a readable image",
                            "host clipboard image probe failed",
                            "host clipboard image response failed",
                        )
                        .await;
                    }
                    ServerFrame::FileExportStart(start) => {
                        if let Err(err) = file_exports.start(start) {
                            jackin_diagnostics::debug_log!(
                                "attach",
                                "host file export start failed: {err:#}"
                            );
                            let message = format!("File export rejected: {err:#}");
                            if let Err(notice_err) =
                                send_host_notice(&mut server_writer, &message).await
                            {
                                jackin_diagnostics::debug_log!(
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
                            jackin_diagnostics::debug_log!(
                                "attach",
                                "host file export chunk failed: {err:#}"
                            );
                            let message = format!("File export rejected: {err:#}");
                            if let Err(notice_err) =
                                send_host_notice(&mut server_writer, &message).await
                            {
                                jackin_diagnostics::debug_log!(
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
                                jackin_diagnostics::debug_log!(
                                    "attach",
                                    "host file export end failed: {err:#}"
                                );
                                format!("File export rejected: {err:#}")
                            }
                        };
                        if let Err(err) = send_host_notice(&mut server_writer, &message).await {
                            jackin_diagnostics::debug_log!(
                                "attach",
                                "host file export end notice failed: {err:#}"
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
                // The two image sources are mutually exclusive by construction:
                // Ctrl+V is the explicit clipboard trigger, while the pasted-path
                // probe only matches a bracketed paste (never the lone Ctrl+V
                // byte). Branch on the trigger so that exclusivity is structural.
                // The two image sources are mutually exclusive by construction:
                // Ctrl+V is the lone trigger byte (no surrounding bytes); the
                // pasted-path probe matches a bracketed paste and carries any
                // bytes that shared the read around the paste body, which must
                // still be forwarded so a coincident keystroke/mouse report is
                // not dropped when the body is consumed as an image.
                let staged: Option<(ClipboardImage, &[u8], &[u8])> =
                    if is_image_paste_trigger(input) {
                        log_clipboard_image_paste_trigger();
                        match read_image_for_paste_trigger(input).await {
                            Ok(Some(image)) => {
                                jackin_diagnostics::debug_log!(
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
                                jackin_diagnostics::debug_log!(
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
                            Ok(Some(image)) => {
                                jackin_diagnostics::debug_log!(
                                    "attach",
                                    "host pasted-path image: format={:?} bytes={}",
                                    image.format,
                                    image.bytes.len()
                                );
                                log_clipboard_image_pasted_path_staged();
                                let (prefix, suffix) = paste_surrounding_bytes(input);
                                Some((image, prefix, suffix))
                            }
                            Ok(None) => None,
                            Err(err) => {
                                jackin_diagnostics::debug_log!(
                                    "attach",
                                    "host pasted-path image probe failed: {err:#}"
                                );
                                None
                            }
                        }
                    };
                if let Some((image, prefix, suffix)) = staged {
                    match write_clipboard_image_frames(&mut server_writer, image).await {
                        Ok(()) => {
                            // Forward any bytes that shared the read but were not
                            // the paste body, so they are not dropped.
                            for extra in [prefix, suffix] {
                                if extra.is_empty() {
                                    continue;
                                }
                                let msg = encode_client(ClientFrame::Input(extra.to_vec()))
                                    .context("encoding surrounding Input frame")?;
                                server_writer
                                    .write_all(&msg)
                                    .await
                                    .context("attach socket write failed (surrounding input)")?;
                            }
                        }
                        Err(err) => {
                            jackin_diagnostics::debug_log!(
                                "attach",
                                "host clipboard image frame rejected; forwarding original input: {err:#}"
                            );
                            let msg = encode_client(ClientFrame::Input(input.to_vec()))
                                .context("encoding fallback Input frame")?;
                            server_writer
                                .write_all(&msg)
                                .await
                                .context("attach socket write failed (input fallback)")?;
                        }
                    }
                } else {
                    let msg = encode_client(ClientFrame::Input(input.to_vec()))
                        .context("encoding Input frame")?;
                    server_writer
                        .write_all(&msg)
                        .await
                        .context("attach socket write failed (input)")?;
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
                        jackin_diagnostics::debug_log!(
                            "attach",
                            "host file export cleanup notice failed: {err:#}"
                        );
                    }
                }
            }
        }
    }
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
        jackin_diagnostics::debug_log!(
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
            jackin_diagnostics::debug_log!(
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
        jackin_diagnostics::debug_log!(
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
            jackin_diagnostics::debug_log!(
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
    if source_path.starts_with("/jackin/run/") || source_path == "/jackin/run" {
        return "jackin-run";
    }
    if source_path.starts_with("/jackin/") || source_path == "/jackin" {
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

async fn write_clipboard_image_frames<W>(writer: &mut W, image: ClipboardImage) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    if image.bytes.len() <= MAX_CLIPBOARD_IMAGE_BYTES {
        let msg = encode_client(ClientFrame::ClipboardImage(image))
            .context("encoding ClipboardImage frame")?;
        writer
            .write_all(&msg)
            .await
            .context("attach socket write failed (clipboard image)")?;
        return Ok(());
    }

    let transfer_id = next_host_transfer_id();
    let size = u64::try_from(image.bytes.len()).context("clipboard image length overflow")?;
    let start = encode_client(ClientFrame::ClipboardImageStart(ClipboardImageStart {
        transfer_id,
        format: image.format.clone(),
        size,
    }))
    .context("encoding ClipboardImageStart frame")?;
    writer
        .write_all(&start)
        .await
        .context("attach socket write failed (clipboard image start)")?;

    let mut hasher = Sha256::new();
    let mut offset = 0u64;
    for chunk in image.bytes.chunks(MAX_CLIPBOARD_IMAGE_CHUNK_BYTES) {
        hasher.update(chunk);
        let msg = encode_client(ClientFrame::ClipboardImageChunk(ClipboardImageChunk {
            transfer_id,
            offset,
            bytes: chunk.to_vec(),
        }))
        .context("encoding ClipboardImageChunk frame")?;
        writer
            .write_all(&msg)
            .await
            .context("attach socket write failed (clipboard image chunk)")?;
        offset = offset
            .checked_add(u64::try_from(chunk.len()).context("clipboard image chunk overflow")?)
            .ok_or_else(|| anyhow::anyhow!("clipboard image offset overflow"))?;
    }

    let sha256 = hasher.finalize().into();
    let end = encode_client(ClientFrame::ClipboardImageEnd(ClipboardImageEnd {
        transfer_id,
        sha256,
    }))
    .context("encoding ClipboardImageEnd frame")?;
    writer
        .write_all(&end)
        .await
        .context("attach socket write failed (clipboard image end)")?;
    Ok(())
}

async fn send_clipboard_image_error<W>(writer: &mut W, message: &str) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let message = bounded_attach_message(message, MAX_CLIPBOARD_IMAGE_ERROR_BYTES);
    let msg = encode_client(ClientFrame::ClipboardImageError(message))
        .context("encoding ClipboardImageError frame")?;
    writer
        .write_all(&msg)
        .await
        .context("attach socket write failed (clipboard image error)")?;
    Ok(())
}

async fn write_clipboard_image_request_result<W>(
    writer: &mut W,
    image: Result<Option<ClipboardImage>>,
    empty_message: &str,
    probe_log_message: &str,
    response_log_message: &str,
) where
    W: AsyncWrite + Unpin,
{
    let result = match image {
        Ok(Some(image)) => write_clipboard_image_frames(writer, image).await,
        Ok(None) => send_clipboard_image_error(writer, empty_message).await,
        Err(err) => {
            jackin_diagnostics::debug_log!("attach", "{probe_log_message}: {err:#}");
            send_clipboard_image_error(writer, &format!("{probe_log_message}: {err:#}")).await
        }
    };
    if let Err(err) = result {
        jackin_diagnostics::debug_log!("attach", "{response_log_message}: {err:#}");
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

fn file_export_success_notice(export: &CompletedHostFileExport) -> String {
    if export.open_after_export {
        return match open_host_file(&export.final_path) {
            Ok(()) => format!(
                "File exported and opened: {} ({} bytes)",
                export.final_path.display(),
                export.bytes
            ),
            Err(err) => {
                jackin_diagnostics::debug_log!(
                    "attach",
                    "host file export open failed for destination_basename={:?}: {err:#}",
                    host_file_basename(&export.final_path)
                );
                format!(
                    "File exported; open failed: {} ({} bytes)",
                    export.final_path.display(),
                    export.bytes
                )
            }
        };
    }
    if !export.reveal_after_export {
        return format!(
            "File exported: {} ({} bytes)",
            export.final_path.display(),
            export.bytes
        );
    }
    match reveal_host_file(&export.final_path) {
        Ok(()) => format!(
            "File exported and revealed: {} ({} bytes)",
            export.final_path.display(),
            export.bytes
        ),
        Err(err) => {
            jackin_diagnostics::debug_log!(
                "attach",
                "host file export reveal failed for destination_basename={:?}: {err:#}",
                host_file_basename(&export.final_path)
            );
            format!(
                "File exported; reveal failed: {} ({} bytes)",
                export.final_path.display(),
                export.bytes
            )
        }
    }
}

fn validate_allowed_host_reveal_path(path: &Path, diagnostics_run_dir: &Path) -> Result<PathBuf> {
    let target = fs::canonicalize(path).context("resolving host reveal path")?;
    let diagnostics_run_dir =
        fs::canonicalize(diagnostics_run_dir).context("resolving diagnostics run directory")?;
    if !target.starts_with(&diagnostics_run_dir) {
        bail!("path is outside jackin diagnostics run directory");
    }
    if target.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
        bail!("path is not a diagnostics JSONL file");
    }
    Ok(target)
}

fn reveal_allowed_host_path(path: &Path, diagnostics_run_dir: &Path) -> Result<()> {
    let target = validate_allowed_host_reveal_path(path, diagnostics_run_dir)?;
    reveal_host_file(&target)
}

fn host_reveal_path_category(path: &Path, diagnostics_run_dir: &Path) -> &'static str {
    if path.starts_with(diagnostics_run_dir) {
        return "jackin-diagnostics";
    }
    if path.starts_with(std::env::temp_dir()) {
        return "host-temp";
    }
    if path.is_absolute() {
        return "host-absolute";
    }
    "host-relative"
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
    use std::sync::Mutex;

    use jackin_protocol::attach::{
        ClientFrame, ClientTerminal, ClipboardImageFormat, ServerFrame, SpawnRequest,
        encode_server, read_client_frame,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    use super::*;

    static TERMINAL_STATE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn normalize_size_substitutes_zero_and_clamps_minimums() {
        assert_eq!(normalize_size(0, 0), (DEFAULT_ROWS, DEFAULT_COLS));
        assert_eq!(normalize_size(1, 1), (MIN_ROWS, MIN_COLS));
        assert_eq!(normalize_size(40, 120), (40, 120));
    }

    #[test]
    fn clipboard_image_paste_compact_logs_are_captured_in_run_diagnostics() {
        let _lock = TERMINAL_STATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        jackin_diagnostics::set_rich_surface_active(false);
        jackin_diagnostics::set_host_screen_owned(false);

        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();

        jackin_diagnostics::set_host_screen_owned(true);
        log_clipboard_image_paste_trigger();
        log_clipboard_image_no_image_forwarded();
        jackin_diagnostics::set_host_screen_owned(false);

        let jsonl = fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("\"kind\":\"clipboard-image\""), "{jsonl}");
        assert!(
            jsonl.contains("clipboard-image: paste trigger source=clipboard"),
            "{jsonl}"
        );
        assert!(
            jsonl.contains("clipboard-image: no-image source=clipboard text-paste=forwarded"),
            "{jsonl}"
        );

        jackin_diagnostics::set_rich_surface_active(false);
        jackin_diagnostics::set_host_screen_owned(false);
    }

    #[tokio::test]
    async fn clipboard_image_writer_keeps_small_images_single_frame() {
        let (mut client, mut server) = duplex(4096);
        let image = ClipboardImage {
            format: ClipboardImageFormat::Png,
            bytes: b"\x89PNG\r\n\x1a\nsmall".to_vec(),
        };

        write_clipboard_image_frames(&mut client, image.clone())
            .await
            .unwrap();
        drop(client);

        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame, ClientFrame::ClipboardImage(image));
        assert_eq!(server.read(&mut tag).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn clipboard_image_writer_chunks_large_images_with_digest() {
        let mut bytes = vec![b'x'; MAX_CLIPBOARD_IMAGE_BYTES + 1];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        let capacity = bytes.len() + 4096;
        let (mut client, mut server) = duplex(capacity);
        let expected_digest: [u8; 32] = Sha256::digest(&bytes).into();

        write_clipboard_image_frames(
            &mut client,
            ClipboardImage {
                format: ClipboardImageFormat::Png,
                bytes: bytes.clone(),
            },
        )
        .await
        .unwrap();
        drop(client);

        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let start = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        let ClientFrame::ClipboardImageStart(start) = start else {
            panic!("expected chunked image start");
        };
        assert_eq!(start.format, ClipboardImageFormat::Png);
        assert_eq!(start.size, bytes.len() as u64);

        let mut received = Vec::new();
        loop {
            server.read_exact(&mut tag).await.unwrap();
            let frame = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            match frame {
                ClientFrame::ClipboardImageChunk(chunk) => {
                    assert_eq!(chunk.transfer_id, start.transfer_id);
                    assert_eq!(chunk.offset, received.len() as u64);
                    assert!(chunk.bytes.len() <= MAX_CLIPBOARD_IMAGE_CHUNK_BYTES);
                    received.extend(chunk.bytes);
                }
                ClientFrame::ClipboardImageEnd(end) => {
                    assert_eq!(end.transfer_id, start.transfer_id);
                    assert_eq!(end.sha256, expected_digest);
                    break;
                }
                other => panic!("unexpected frame {other:?}"),
            }
        }

        assert_eq!(received, bytes);
        assert_eq!(server.read(&mut tag).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn explicit_clipboard_image_request_returns_probe_error_to_capsule() {
        let (mut client, mut server) = duplex(4096);

        write_clipboard_image_request_result(
            &mut client,
            Err(anyhow::anyhow!(
                "Linux host clipboard image reader needs WAYLAND_DISPLAY with wl-paste or DISPLAY with xclip"
            )),
            "host clipboard does not contain a readable image",
            "host clipboard image probe failed",
            "host clipboard image response failed",
        )
        .await;
        drop(client);

        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        let ClientFrame::ClipboardImageError(message) = frame else {
            panic!("expected ClipboardImageError");
        };

        assert!(message.contains("host clipboard image probe failed"));
        assert!(message.contains("WAYLAND_DISPLAY with wl-paste or DISPLAY with xclip"));
        assert_eq!(server.read(&mut tag).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn explicit_clipboard_path_request_mentions_file_url_support() {
        let (mut client, mut server) = duplex(4096);

        write_clipboard_image_request_result(
            &mut client,
            Ok(None),
            "host clipboard text is not an absolute readable image path or file:// image URL",
            "host clipboard image path probe failed",
            "host clipboard image path response failed",
        )
        .await;
        drop(client);

        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        let ClientFrame::ClipboardImageError(message) = frame else {
            panic!("expected ClipboardImageError");
        };

        assert_eq!(
            message,
            "host clipboard text is not an absolute readable image path or file:// image URL"
        );
        assert_eq!(server.read(&mut tag).await.unwrap(), 0);
    }

    #[test]
    fn host_file_export_finalizes_after_digest_match() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"export me";
        let sha256: [u8; 32] = Sha256::digest(bytes).into();
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
        exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 99,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "report.txt".into(),
                    size: bytes.len() as u64,
                    reveal_after_export: true,
                    open_after_export: false,
                },
                root.path(),
            )
            .unwrap();
        exports
            .chunk(FileExportChunk {
                transfer_id: 99,
                offset: 0,
                bytes: bytes.to_vec(),
            })
            .unwrap();
        let completed = exports
            .end(FileExportEnd {
                transfer_id: 99,
                sha256,
            })
            .unwrap();

        assert_eq!(fs::read(root.path().join("report.txt")).unwrap(), bytes);
        assert_eq!(completed.final_path, root.path().join("report.txt"));
        assert_eq!(completed.bytes, bytes.len() as u64);
        assert!(completed.reveal_after_export);
    }

    #[test]
    fn host_file_export_rejects_digest_mismatch_and_removes_temp() {
        let root = tempfile::tempdir().unwrap();
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
        exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 100,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "../bad:name.txt".into(),
                    size: 3,
                    reveal_after_export: false,
                    open_after_export: false,
                },
                root.path(),
            )
            .unwrap();
        exports
            .chunk(FileExportChunk {
                transfer_id: 100,
                offset: 0,
                bytes: b"bad".to_vec(),
            })
            .unwrap();
        let err = exports
            .end(FileExportEnd {
                transfer_id: 100,
                sha256: [0; 32],
            })
            .expect_err("digest mismatch should reject export");

        assert!(format!("{err:#}").contains("SHA-256 mismatch"));
        assert!(!root.path().join("__bad_name.txt").exists());
        assert!(fs::read_dir(root.path()).unwrap().next().is_none());
    }

    #[test]
    fn host_file_export_drop_removes_interrupted_temp_file() {
        let root = tempfile::tempdir().unwrap();
        {
            let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
            exports
                .start_in_root(
                    FileExportStart {
                        transfer_id: 102,
                        source_path: "/workspace/report.txt".into(),
                        file_name: "report.txt".into(),
                        size: 9,
                        reveal_after_export: false,
                        open_after_export: false,
                    },
                    root.path(),
                )
                .unwrap();
            exports
                .chunk(FileExportChunk {
                    transfer_id: 102,
                    offset: 0,
                    bytes: b"partial".to_vec(),
                })
                .unwrap();

            assert!(root.path().join("report.txt.part").exists());
            assert!(!root.path().join("report.txt").exists());
        }

        assert!(!root.path().join("report.txt.part").exists());
        assert!(!root.path().join("report.txt").exists());
        assert!(fs::read_dir(root.path()).unwrap().next().is_none());
    }

    #[test]
    fn host_file_export_abort_removes_temp_and_rejects_end() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"export me";
        let sha256: [u8; 32] = Sha256::digest(bytes).into();
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
        exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 103,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "report.txt".into(),
                    size: bytes.len() as u64,
                    reveal_after_export: false,
                    open_after_export: false,
                },
                root.path(),
            )
            .unwrap();
        exports
            .chunk(FileExportChunk {
                transfer_id: 103,
                offset: 0,
                bytes: b"export".to_vec(),
            })
            .unwrap();

        let err = exports
            .chunk(FileExportChunk {
                transfer_id: 103,
                offset: 0,
                bytes: b"bad-offset".to_vec(),
            })
            .expect_err("bad offset should reject export chunk");
        assert!(format!("{err:#}").contains("did not match expected"));

        exports.abort(103);
        assert!(!root.path().join("report.txt.part").exists());
        assert!(!root.path().join("report.txt").exists());
        let err = exports
            .end(FileExportEnd {
                transfer_id: 103,
                sha256,
            })
            .expect_err("aborted transfer should not finalize");
        assert!(format!("{err:#}").contains("has no active start"));
    }

    #[test]
    fn host_file_export_idle_cleanup_removes_stale_temp_file() {
        let root = tempfile::tempdir().unwrap();
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
        exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 104,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "report.txt".into(),
                    size: 9,
                    reveal_after_export: false,
                    open_after_export: false,
                },
                root.path(),
            )
            .unwrap();
        exports
            .chunk(FileExportChunk {
                transfer_id: 104,
                offset: 0,
                bytes: b"partial".to_vec(),
            })
            .unwrap();
        exports.active.get_mut(&104).unwrap().last_activity =
            Instant::now().checked_sub(Duration::from_secs(10)).unwrap();

        assert!(root.path().join("report.txt.part").exists());
        assert_eq!(exports.abort_idle_before(Instant::now()), 1);
        assert!(!root.path().join("report.txt.part").exists());
        assert!(fs::read_dir(root.path()).unwrap().next().is_none());

        let err = exports
            .end(FileExportEnd {
                transfer_id: 104,
                sha256: [0; 32],
            })
            .expect_err("stale transfer cleanup should remove active export");
        assert!(format!("{err:#}").contains("has no active start"));
    }

    #[test]
    fn host_file_export_idle_cleanup_keeps_fresh_temp_file() {
        let root = tempfile::tempdir().unwrap();
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
        exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 105,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "report.txt".into(),
                    size: 9,
                    reveal_after_export: false,
                    open_after_export: false,
                },
                root.path(),
            )
            .unwrap();
        exports
            .chunk(FileExportChunk {
                transfer_id: 105,
                offset: 0,
                bytes: b"partial".to_vec(),
            })
            .unwrap();

        assert_eq!(
            exports.abort_idle_before(Instant::now().checked_sub(Duration::from_secs(10)).unwrap()),
            0
        );
        assert!(root.path().join("report.txt.part").exists());
    }

    #[test]
    fn unique_export_path_appends_counter() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("report.txt"), b"existing").unwrap();
        assert_eq!(
            unique_export_path(root.path(), "report.txt"),
            root.path().join("report-1.txt")
        );
    }

    #[test]
    fn host_file_export_root_uses_sanitized_instance_subdir() {
        let root = host_file_export_root("../jk:agent/smith")
            .expect("home or downloads should resolve in tests");

        assert!(root.ends_with(Path::new("jackin").join("_jk_agent_smith")));
    }

    #[test]
    fn export_source_path_category_names_supported_buckets() {
        assert_eq!(
            export_source_path_category("/jackin/run/clipboard/image.png"),
            "jackin-run"
        );
        assert_eq!(
            export_source_path_category("/jackin/state/marker"),
            "jackin-owned"
        );
        assert_eq!(
            export_source_path_category("/workspace/report.txt"),
            "container-absolute"
        );
        assert_eq!(
            export_source_path_category("relative/report.txt"),
            "container-relative"
        );
    }

    #[test]
    fn host_file_export_compact_line_omits_full_paths() {
        let line = host_file_export_compact_line("workspace", "report.md", 123);

        assert_eq!(
            line,
            "host-file-export: exported source_category=workspace basename=\"report.md\" bytes=123 destination_category=host-downloads-jackin-instance"
        );
        assert!(!line.contains("/workspace"));
        assert!(!line.contains("Downloads"));
        assert!(!line.contains("/jackin/run"));
    }

    #[test]
    fn host_file_basename_omits_parent_directories() {
        assert_eq!(
            host_file_basename(Path::new("/Users/operator/Downloads/jackin/report.md")),
            "report.md"
        );
        assert_eq!(host_file_basename(Path::new("/")), "jackin-export");
    }

    #[test]
    fn host_reveal_path_category_omits_full_paths() {
        let diagnostics_dir = Path::new("/Users/operator/.jackin/data/diagnostics/runs");

        assert_eq!(
            host_reveal_path_category(
                Path::new("/Users/operator/.jackin/data/diagnostics/runs/jk-run.jsonl"),
                diagnostics_dir,
            ),
            "jackin-diagnostics"
        );
        assert_eq!(
            host_reveal_path_category(Path::new("/Users/operator/private.jsonl"), diagnostics_dir),
            "host-absolute"
        );
        assert_eq!(
            host_reveal_path_category(Path::new("relative.jsonl"), diagnostics_dir),
            "host-relative"
        );
    }

    #[test]
    fn host_reveal_path_validation_accepts_diagnostics_jsonl() {
        let root = tempfile::tempdir().unwrap();
        let diagnostics_dir = root.path().join("data/diagnostics/runs");
        fs::create_dir_all(&diagnostics_dir).unwrap();
        let path = diagnostics_dir.join("jk-run-abc123.jsonl");
        fs::write(&path, b"{}\n").unwrap();

        assert_eq!(
            validate_allowed_host_reveal_path(&path, &diagnostics_dir).unwrap(),
            fs::canonicalize(&path).unwrap()
        );
    }

    #[test]
    fn host_reveal_path_validation_rejects_non_diagnostics_paths() {
        let root = tempfile::tempdir().unwrap();
        let diagnostics_dir = root.path().join("data/diagnostics/runs");
        let other_dir = root.path().join("data/other");
        fs::create_dir_all(&diagnostics_dir).unwrap();
        fs::create_dir_all(&other_dir).unwrap();
        let path = other_dir.join("jk-run-abc123.jsonl");
        fs::write(&path, b"{}\n").unwrap();

        let err = validate_allowed_host_reveal_path(&path, &diagnostics_dir)
            .expect_err("outside diagnostics dir should reject");
        assert!(format!("{err:#}").contains("outside jackin diagnostics"));
    }

    #[test]
    fn host_reveal_path_validation_rejects_non_jsonl_file() {
        let root = tempfile::tempdir().unwrap();
        let diagnostics_dir = root.path().join("data/diagnostics/runs");
        fs::create_dir_all(&diagnostics_dir).unwrap();
        let path = diagnostics_dir.join("jk-run-abc123.txt");
        fs::write(&path, b"{}\n").unwrap();

        let err = validate_allowed_host_reveal_path(&path, &diagnostics_dir)
            .expect_err("non-jsonl diagnostics path should reject");
        assert!(format!("{err:#}").contains("not a diagnostics JSONL"));
    }

    #[test]
    fn host_file_export_start_does_not_overwrite_stale_temp_file() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("report.txt.part"), b"stale").unwrap();
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());

        let err = exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 101,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "report.txt".into(),
                    size: 3,
                    reveal_after_export: false,
                    open_after_export: false,
                },
                root.path(),
            )
            .expect_err("stale temp file should not be overwritten");

        assert!(format!("{err:#}").contains("creating temporary host export"));
        assert_eq!(
            fs::read(root.path().join("report.txt.part")).unwrap(),
            b"stale"
        );
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
                ..ClientTerminal::default()
            },
            export_subdir: "jk-agent-smith".to_owned(),
            diagnostics_run_dir: tempfile::tempdir().unwrap().path().join("diagnostics/runs"),
        };

        let server_task = tokio::spawn(async move {
            let mut tag = [0u8; 1];
            server.read_exact(&mut tag).await.unwrap();
            let frame = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
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
                    ..ClientTerminal::default()
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
            export_subdir: "jk-agent-smith".to_owned(),
            diagnostics_run_dir: tempfile::tempdir().unwrap().path().join("diagnostics/runs"),
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
                .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
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
    async fn attach_protocol_preserves_bracketed_paste_and_mouse_bytes() {
        let (client, mut server) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client);
        let request = HostAttachRequest {
            spawn_request: None,
            focus_session: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            export_subdir: "jk-agent-smith".to_owned(),
            diagnostics_run_dir: tempfile::tempdir().unwrap().path().join("diagnostics/runs"),
        };
        let raw_input = b"\x1b[200~/tmp/example.png\x1b[201~\x1b[<0;12;5M\x1b[<0;12;5m".to_vec();

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
                .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
                .await
                .unwrap();
            input
        });

        let (mut input_writer, input_reader) = duplex(128);
        input_writer.write_all(&raw_input).await.unwrap();
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

        assert_eq!(server_task.await.unwrap(), ClientFrame::Input(raw_input));
    }

    #[tokio::test]
    async fn attach_protocol_auto_stages_bracketed_image_path_paste() {
        let temp = tempfile::tempdir().unwrap();
        let image_path = temp.path().join("shot.png");
        fs::write(&image_path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let (client, mut server) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client);
        let request = HostAttachRequest {
            spawn_request: None,
            focus_session: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            export_subdir: "jk-agent-smith".to_owned(),
            diagnostics_run_dir: tempfile::tempdir().unwrap().path().join("diagnostics/runs"),
        };
        let mut raw_input = b"\x1b[200~".to_vec();
        raw_input.extend_from_slice(image_path.display().to_string().as_bytes());
        raw_input.extend_from_slice(b"\x1b[201~");

        let server_task = tokio::spawn(async move {
            let mut tag = [0u8; 1];
            server.read_exact(&mut tag).await.unwrap();
            let _hello = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server.read_exact(&mut tag).await.unwrap();
            let frame = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
                .await
                .unwrap();
            frame
        });

        let (mut input_writer, input_reader) = duplex(128);
        input_writer.write_all(&raw_input).await.unwrap();
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

        // The pasted host image path is staged as an image frame, not forwarded
        // as the raw path text.
        match server_task.await.unwrap() {
            ClientFrame::ClipboardImage(image) => {
                assert_eq!(image.format, ClipboardImageFormat::Png);
                assert_eq!(image.bytes, b"\x89PNG\r\n\x1a\npayload");
            }
            other => panic!("expected staged ClipboardImage frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn attach_protocol_forwards_bytes_around_a_staged_paste() {
        let temp = tempfile::tempdir().unwrap();
        let image_path = temp.path().join("shot.png");
        fs::write(&image_path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let (client, mut server) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client);
        let request = HostAttachRequest {
            spawn_request: None,
            focus_session: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            export_subdir: "jk-agent-smith".to_owned(),
            diagnostics_run_dir: tempfile::tempdir().unwrap().path().join("diagnostics/runs"),
        };
        // A mouse report shares the read after the paste end marker.
        let mut raw_input = b"\x1b[200~".to_vec();
        raw_input.extend_from_slice(image_path.display().to_string().as_bytes());
        raw_input.extend_from_slice(b"\x1b[201~\x1b[<0;1;1M");

        let server_task = tokio::spawn(async move {
            let mut tag = [0u8; 1];
            server.read_exact(&mut tag).await.unwrap();
            let _hello = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server.read_exact(&mut tag).await.unwrap();
            let image = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server.read_exact(&mut tag).await.unwrap();
            let trailing = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
                .await
                .unwrap();
            (image, trailing)
        });

        let (mut input_writer, input_reader) = duplex(128);
        input_writer.write_all(&raw_input).await.unwrap();
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

        // The image stages, and the coincident mouse report is forwarded rather
        // than dropped with the consumed paste body.
        let (image, trailing) = server_task.await.unwrap();
        assert!(matches!(image, ClientFrame::ClipboardImage(_)));
        assert_eq!(trailing, ClientFrame::Input(b"\x1b[<0;1;1M".to_vec()));
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
            export_subdir: "jk-agent-smith".to_owned(),
            diagnostics_run_dir: tempfile::tempdir().unwrap().path().join("diagnostics/runs"),
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
                .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
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

    #[tokio::test]
    async fn attach_protocol_writes_osc52_output_unchanged() {
        let (client, mut server) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client);
        let mut output = Vec::new();
        let request = HostAttachRequest {
            spawn_request: None,
            focus_session: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            export_subdir: "jk-agent-smith".to_owned(),
            diagnostics_run_dir: tempfile::tempdir().unwrap().path().join("diagnostics/runs"),
        };
        let osc52 = b"\x1b]52;c;c2VsZWN0ZWQ=\x07".to_vec();

        let server_task = tokio::spawn(async move {
            let mut tag = [0u8; 1];
            server.read_exact(&mut tag).await.unwrap();
            let _hello = read_client_frame(&mut server, tag[0])
                .await
                .unwrap()
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Output(osc52)))
                .await
                .unwrap();
            server
                .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
                .await
                .unwrap();
        });

        let (_input_writer, input_reader) = duplex(64);
        let winch = signal(SignalKind::window_change()).unwrap();
        run_attach_protocol(
            client_reader,
            client_writer,
            input_reader,
            Cursor::new(&mut output),
            24,
            80,
            request,
            Vec::new(),
            winch,
        )
        .await
        .unwrap();
        server_task.await.unwrap();

        assert_eq!(output, b"\x1b]52;c;c2VsZWN0ZWQ=\x07");
    }

    #[tokio::test]
    async fn host_notice_writer_sends_typed_protocol_frame() {
        let (mut client, mut server) = duplex(4096);

        send_host_notice(&mut client, "File exported: ~/Downloads/jackin/report.txt")
            .await
            .unwrap();
        drop(client);

        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            frame,
            ClientFrame::HostNotice("File exported: ~/Downloads/jackin/report.txt".to_owned())
        );
        assert_eq!(server.read(&mut tag).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn host_notice_writer_bounds_overlong_message() {
        let (mut client, mut server) = duplex(MAX_HOST_NOTICE_BYTES + 64);
        let message = format!("{}{}", "a".repeat(MAX_HOST_NOTICE_BYTES), "é");

        send_host_notice(&mut client, &message).await.unwrap();
        drop(client);

        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();

        let ClientFrame::HostNotice(message) = frame else {
            panic!("expected HostNotice");
        };
        assert_eq!(message.len(), MAX_HOST_NOTICE_BYTES);
        assert!(message.ends_with("..."));
    }

    #[tokio::test]
    async fn clipboard_image_error_writer_bounds_empty_and_overlong_message() {
        let (mut client, mut server) = duplex(MAX_CLIPBOARD_IMAGE_ERROR_BYTES + 64);
        let message = format!("{}{}", "b".repeat(MAX_CLIPBOARD_IMAGE_ERROR_BYTES), "é");

        send_clipboard_image_error(&mut client, &message)
            .await
            .unwrap();
        send_clipboard_image_error(&mut client, "   ")
            .await
            .unwrap();
        drop(client);

        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        let ClientFrame::ClipboardImageError(message) = frame else {
            panic!("expected ClipboardImageError");
        };
        assert_eq!(message.len(), MAX_CLIPBOARD_IMAGE_ERROR_BYTES);
        assert!(message.ends_with("..."));

        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            frame,
            ClientFrame::ClipboardImageError("Host action failed".to_owned())
        );
    }
}
