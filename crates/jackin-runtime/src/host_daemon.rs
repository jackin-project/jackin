//! Minimal host-daemon lifecycle foundation.
//!
//! This module intentionally owns only the empty daemon shell: socket binding,
//! request/response framing, status, and shutdown. Reactive adapters are added
//! by later plans.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use jackin_core::JackinPaths;
use serde::{Deserialize, Serialize};

pub const DAEMON_PROTOCOL_VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: u64 = 16 * 1024;
pub const SOCKET_FILE_NAME: &str = "jackin-daemon.sock";
pub const LOG_FILE_NAME: &str = "jackin-daemon.log";
pub const PID_FILE_NAME: &str = "jackin-daemon.pid";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonRequest {
    pub id: String,
    pub protocol_version: u16,
    pub build_id: String,
    #[serde(flatten)]
    pub kind: DaemonRequestKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequestKind {
    Hello,
    Status,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonResponse {
    pub id: String,
    #[serde(flatten)]
    pub kind: DaemonResponseKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponseKind {
    Hello {
        protocol_version: u16,
        build_id: String,
        capabilities: Vec<String>,
    },
    Status(DaemonStatus),
    Shutdown {
        accepted: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStatus {
    pub protocol_version: u16,
    pub build_id: String,
    pub pid: u32,
    pub socket_path: PathBuf,
    pub log_path: PathBuf,
    pub coredump_policy: CoredumpPolicy,
    pub adapters_enabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CoredumpPolicy {
    Disabled,
    Unsupported { residual_risk: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeOutcome {
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonLayout {
    pub run_dir: PathBuf,
    pub socket_path: PathBuf,
    pub log_path: PathBuf,
    pub pid_path: PathBuf,
}

impl DaemonLayout {
    #[must_use]
    pub fn new(paths: &JackinPaths) -> Self {
        let run_dir = paths.jackin_home.join("run");
        Self {
            socket_path: run_dir.join(SOCKET_FILE_NAME),
            log_path: run_dir.join(LOG_FILE_NAME),
            pid_path: run_dir.join(PID_FILE_NAME),
            run_dir,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitFiles {
    pub launchd_label: String,
    pub launchd_path: PathBuf,
    pub launchd_plist: String,
    pub systemd_path: PathBuf,
    pub systemd_unit: String,
}

pub fn ensure_run_dir(layout: &DaemonLayout) -> Result<()> {
    fs::create_dir_all(&layout.run_dir)
        .with_context(|| format!("creating daemon run dir {}", layout.run_dir.display()))?;
    fs::set_permissions(&layout.run_dir, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("restricting daemon run dir {}", layout.run_dir.display()))
}

pub fn bind_control_socket(layout: &DaemonLayout) -> Result<UnixListener> {
    ensure_run_dir(layout)?;
    if layout.socket_path.exists() {
        fs::remove_file(&layout.socket_path)
            .with_context(|| format!("removing stale socket {}", layout.socket_path.display()))?;
    }
    let listener = UnixListener::bind(&layout.socket_path)
        .with_context(|| format!("binding daemon socket {}", layout.socket_path.display()))?;
    fs::set_permissions(&layout.socket_path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restricting daemon socket {}", layout.socket_path.display()))?;
    Ok(listener)
}

pub fn serve(layout: &DaemonLayout, build_id: &str) -> Result<ServeOutcome> {
    let listener = bind_control_socket(layout)?;
    write_pid(layout)?;
    write_log(layout, "daemon started")?;
    let coredump_policy = disable_coredumps();
    for stream in listener.incoming() {
        let mut stream = stream.context("accepting daemon client")?;
        let response = handle_stream(&mut stream, layout, build_id, &coredump_policy)?;
        if matches!(
            response.kind,
            DaemonResponseKind::Shutdown { accepted: true }
        ) {
            cleanup_runtime_files(layout);
            return Ok(ServeOutcome::Shutdown);
        }
    }
    Ok(ServeOutcome::Shutdown)
}

pub fn request(
    socket_path: &Path,
    build_id: &str,
    kind: DaemonRequestKind,
) -> Result<DaemonResponse> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("connecting to daemon socket {}", socket_path.display()))?;
    let request = DaemonRequest {
        id: "cli".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: build_id.to_owned(),
        kind,
    };
    serde_json::to_writer(&mut stream, &request).context("writing daemon request")?;
    stream
        .write_all(b"\n")
        .context("terminating daemon request")?;
    read_response(stream)
}

pub fn render_unit_files(paths: &JackinPaths, executable: &Path) -> UnitFiles {
    let layout = DaemonLayout::new(paths);
    let launchd_label = "com.jackin.daemon".to_owned();
    let launchd_path = paths
        .home_dir
        .join("Library/LaunchAgents")
        .join(format!("{launchd_label}.plist"));
    let systemd_path = paths
        .config_dir
        .join("systemd/user")
        .join("jackin-daemon.service");
    let exe = executable.display();
    let log = layout.log_path.display();
    let launchd_plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{launchd_label}</string>
  <key>ProgramArguments</key>
  <array><string>{exe}</string><string>daemon</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#
    );
    let systemd_unit = format!(
        "[Unit]\nDescription=jackin daemon\n\n[Service]\nExecStart={exe} daemon serve\nRestart=on-failure\nStandardOutput=append:{log}\nStandardError=append:{log}\n\n[Install]\nWantedBy=default.target\n"
    );
    UnitFiles {
        launchd_label,
        launchd_path,
        launchd_plist,
        systemd_path,
        systemd_unit,
    }
}

pub fn install_units(paths: &JackinPaths, executable: &Path) -> Result<UnitFiles> {
    let units = render_unit_files(paths, executable);
    if cfg!(target_os = "macos") {
        write_parented(&units.launchd_path, &units.launchd_plist)?;
    } else {
        write_parented(&units.systemd_path, &units.systemd_unit)?;
    }
    Ok(units)
}

pub fn uninstall_units(paths: &JackinPaths, executable: &Path) -> Result<UnitFiles> {
    let units = render_unit_files(paths, executable);
    remove_if_exists(&units.launchd_path)?;
    remove_if_exists(&units.systemd_path)?;
    Ok(units)
}

pub fn read_log(layout: &DaemonLayout) -> Result<String> {
    match fs::read_to_string(&layout.log_path) {
        Ok(contents) => Ok(jackin_diagnostics::scrub_secrets(&contents).into_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("reading {}", layout.log_path.display())),
    }
}

fn handle_stream(
    stream: &mut UnixStream,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
) -> Result<DaemonResponse> {
    let mut line = String::new();
    let read = BufReader::new(stream.try_clone().context("cloning daemon stream")?)
        .take(MAX_REQUEST_BYTES + 1)
        .read_line(&mut line)
        .context("reading daemon request")?;
    let response = if read as u64 > MAX_REQUEST_BYTES {
        error_response("unknown", "daemon request exceeds 16384 byte limit")
    } else {
        handle_request_line(line.trim_end(), layout, build_id, coredump_policy)
    };
    serde_json::to_writer(&mut *stream, &response).context("writing daemon response")?;
    stream
        .write_all(b"\n")
        .context("terminating daemon response")?;
    Ok(response)
}

pub fn handle_request_line(
    line: &str,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
) -> DaemonResponse {
    if line.is_empty() {
        return error_response("unknown", "empty daemon request");
    }
    match serde_json::from_str::<DaemonRequest>(line) {
        Ok(request) => handle_request(request, layout, build_id, coredump_policy),
        Err(error) => error_response("unknown", format!("invalid daemon request: {error}")),
    }
}

fn handle_request(
    request: DaemonRequest,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
) -> DaemonResponse {
    if request.protocol_version != DAEMON_PROTOCOL_VERSION {
        return error_response(
            request.id,
            format!(
                "unsupported daemon protocol {}; expected {}",
                request.protocol_version, DAEMON_PROTOCOL_VERSION
            ),
        );
    }
    if request.build_id != build_id {
        return error_response(
            request.id,
            format!(
                "daemon build mismatch: client {}; daemon {}",
                request.build_id, build_id
            ),
        );
    }
    let id = request.id;
    match request.kind {
        DaemonRequestKind::Hello => DaemonResponse {
            id,
            kind: DaemonResponseKind::Hello {
                protocol_version: DAEMON_PROTOCOL_VERSION,
                build_id: build_id.to_owned(),
                capabilities: Vec::new(),
            },
        },
        DaemonRequestKind::Status => DaemonResponse {
            id,
            kind: DaemonResponseKind::Status(DaemonStatus {
                protocol_version: DAEMON_PROTOCOL_VERSION,
                build_id: build_id.to_owned(),
                pid: std::process::id(),
                socket_path: layout.socket_path.clone(),
                log_path: layout.log_path.clone(),
                coredump_policy: coredump_policy.clone(),
                adapters_enabled: Vec::new(),
            }),
        },
        DaemonRequestKind::Shutdown => DaemonResponse {
            id,
            kind: DaemonResponseKind::Shutdown { accepted: true },
        },
    }
}

fn read_response(stream: UnixStream) -> Result<DaemonResponse> {
    let mut line = String::new();
    BufReader::new(stream)
        .take(MAX_REQUEST_BYTES + 1)
        .read_line(&mut line)
        .context("reading daemon response")?;
    if line.len() as u64 > MAX_REQUEST_BYTES {
        bail!("daemon response exceeds {MAX_REQUEST_BYTES} byte limit");
    }
    serde_json::from_str(line.trim_end()).context("parsing daemon response")
}

fn error_response(id: impl Into<String>, message: impl Into<String>) -> DaemonResponse {
    DaemonResponse {
        id: id.into(),
        kind: DaemonResponseKind::Error {
            message: message.into(),
        },
    }
}

fn write_pid(layout: &DaemonLayout) -> Result<()> {
    ensure_run_dir(layout)?;
    fs::write(&layout.pid_path, format!("{}\n", std::process::id()))
        .with_context(|| format!("writing {}", layout.pid_path.display()))
}

fn write_log(layout: &DaemonLayout, line: &str) -> Result<()> {
    ensure_run_dir(layout)?;
    let line = jackin_diagnostics::scrub_secrets(line);
    fs::write(&layout.log_path, format!("{line}\n"))
        .with_context(|| format!("writing {}", layout.log_path.display()))
}

fn cleanup_runtime_files(layout: &DaemonLayout) {
    drop(remove_if_exists(&layout.socket_path));
    drop(remove_if_exists(&layout.pid_path));
}

fn write_parented(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("removing {}", path.display())),
    }
}

fn disable_coredumps() -> CoredumpPolicy {
    CoredumpPolicy::Unsupported {
        residual_risk: "core dump suppression is not wired for this build; daemon carries no adapters or secrets yet".to_owned(),
    }
}

#[cfg(test)]
mod tests;
