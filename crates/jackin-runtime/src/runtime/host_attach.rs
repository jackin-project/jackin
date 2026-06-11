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
use std::process::{Command as StdCommand, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use directories::UserDirs;
use jackin_core::paths::JackinPaths;
use jackin_protocol::attach::{
    ClientFrame, ClientTerminal, ClipboardImage, ClipboardImageChunk, ClipboardImageEnd,
    ClipboardImageStart, FileExportChunk, FileExportEnd, FileExportStart,
    MAX_CLIPBOARD_IMAGE_BYTES, MAX_CLIPBOARD_IMAGE_CHUNK_BYTES, ServerFrame, SpawnRequest,
    encode_client, read_server_frame,
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
    read_image_for_paste_trigger, read_image_from_clipboard, read_image_from_clipboard_text_path,
};

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
        export_subdir: sanitize_export_path_component(container_name, "instance"),
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
                        let message = match open_host_url(&url) {
                            Ok(()) => "Opening URL in host browser".to_owned(),
                            Err(err) => {
                                jackin_diagnostics::debug_log!(
                                    "attach",
                                    "host open URL failed for {url:?}: {err:#}"
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
                    ServerFrame::HostStageImageFromClipboardPath => {
                        write_clipboard_image_request_result(
                            &mut server_writer,
                            read_image_from_clipboard_text_path().await,
                            "host clipboard text is not an absolute readable image path",
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
                        if let Err(err) = file_exports.chunk(chunk) {
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
                            Ok(path) => format!("File exported: {}", path.display()),
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
                let image = match read_image_for_paste_trigger(input).await {
                    Ok(Some(image)) => {
                        jackin_diagnostics::debug_log!(
                            "attach",
                            "host clipboard image paste: format={:?} bytes={}",
                            image.format,
                            image.bytes.len()
                        );
                        Some(image)
                    }
                    Ok(None) => None,
                    Err(err) => {
                        jackin_diagnostics::debug_log!(
                            "attach",
                            "host clipboard image paste probe failed: {err:#}"
                        );
                        None
                    }
                };
                if let Some(image) = image {
                    if let Err(err) = write_clipboard_image_frames(&mut server_writer, image).await {
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
        }
    }
}

struct HostFileExports {
    export_subdir: String,
    active: HashMap<u64, ActiveHostFileExport>,
}

struct ActiveHostFileExport {
    source_path: String,
    final_path: PathBuf,
    temp_path: PathBuf,
    file: File,
    expected_size: u64,
    written: u64,
    hasher: Sha256,
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
        fs::create_dir_all(&root)
            .with_context(|| format!("creating host export directory {}", root.display()))?;
        let file_name = sanitize_export_file_name(&start.file_name);
        let final_path = unique_export_path(&root, &file_name);
        let temp_path = final_path.with_extension(format!(
            "{}part",
            final_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| format!("{ext}."))
                .unwrap_or_default()
        ));
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| format!("creating temporary host export {}", temp_path.display()))?;
        self.active.insert(
            start.transfer_id,
            ActiveHostFileExport {
                source_path: start.source_path,
                final_path,
                temp_path,
                file,
                expected_size: start.size,
                written: 0,
                hasher: Sha256::new(),
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
        Ok(())
    }

    fn end(&mut self, end: FileExportEnd) -> Result<PathBuf> {
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
            bail!("file export transfer {} SHA-256 mismatch", end.transfer_id);
        }
        fs::rename(&active.temp_path, &active.final_path).with_context(|| {
            format!(
                "moving host export {} to {}",
                active.temp_path.display(),
                active.final_path.display()
            )
        })?;
        jackin_diagnostics::emit_compact_line(
            "host_file_export",
            &format!(
                "exported {} to {}",
                active.source_path,
                active.final_path.display()
            ),
        );
        Ok(active.final_path)
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
    let msg = encode_client(ClientFrame::ClipboardImageError(message.to_owned()))
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
            send_clipboard_image_error(writer, probe_log_message).await
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
    let msg = encode_client(ClientFrame::HostNotice(message.to_owned()))
        .context("encoding HostNotice frame")?;
    writer
        .write_all(&msg)
        .await
        .context("attach socket write failed (host notice)")?;
    Ok(())
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
    let open_links = std::env::var(jackin_core::env_model::JACKIN_OPEN_LINKS_ENV_NAME).ok();
    host_open_command_with_policy(url, open_links.as_deref())
}

fn host_open_command_with_policy(
    url: &str,
    open_links: Option<&str>,
) -> Option<(&'static str, Vec<String>)> {
    if !jackin_core::env_model::open_links_allowed(open_links) {
        return None;
    }
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
        ClientFrame, ClientTerminal, ClipboardImageFormat, ServerFrame, SpawnRequest,
        encode_server, read_client_frame,
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
        let Some((_program, args)) = host_open_command_with_policy(
            "https://github.com/jackin-project/jackin/actions/runs/1",
            None,
        ) else {
            panic!("http(s) URL should produce a host opener command on supported test platforms");
        };
        assert!(args.iter().any(|arg| arg.contains("github.com")));
    }

    #[test]
    fn host_open_command_honors_open_links_opt_out() {
        assert!(
            host_open_command_with_policy(
                "https://github.com/jackin-project/jackin/actions/runs/1",
                Some("deny"),
            )
            .is_none()
        );
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
        exports
            .end(FileExportEnd {
                transfer_id: 99,
                sha256,
            })
            .unwrap();

        assert_eq!(fs::read(root.path().join("report.txt")).unwrap(), bytes);
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
            },
            export_subdir: "jk-agent-smith".to_owned(),
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
            export_subdir: "jk-agent-smith".to_owned(),
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
            export_subdir: "jk-agent-smith".to_owned(),
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
}
