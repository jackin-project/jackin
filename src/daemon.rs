use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::cli::daemon::{DaemonCommand, LogsArgs, NotifyArgs};
use crate::docker::{CommandRunner, RunOptions, ShellRunner};
use crate::paths::JackinPaths;

const PROTOCOL_VERSION: u32 = 2;
const MIN_PROTOCOL_VERSION: u32 = PROTOCOL_VERSION;
const SOCKET_FILENAME: &str = "jackin-daemon.sock";
const LOCK_FILENAME: &str = "jackin-daemon.lock";
const PID_FILENAME: &str = "jackin-daemon.pid";
const LOG_FILENAME: &str = "jackin-daemon.log";
const LAUNCHD_LABEL: &str = "com.jackin.daemon";
const SYSTEMD_UNIT: &str = "jackin-daemon.service";

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    #[serde(rename = "daemon/hello")]
    Hello {
        protocol: u32,
    },
    #[serde(rename = "workspace/list")]
    WorkspaceList {
        protocol: u32,
    },
    #[serde(rename = "session/list")]
    SessionList {
        protocol: u32,
    },
    Status {
        protocol: u32,
    },
    Shutdown {
        protocol: u32,
    },
    WarmCache {
        protocol: u32,
    },
    Notify {
        protocol: u32,
        title: String,
        body: String,
        urgency: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    Ok {
        message: String,
    },
    #[serde(rename = "daemon/hello")]
    Hello(HelloResponse),
    #[serde(rename = "workspace/list")]
    WorkspaceList(WorkspaceListResponse),
    #[serde(rename = "session/list")]
    SessionList(SessionListResponse),
    Status(StatusResponse),
    Error {
        message: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HelloResponse {
    pub version: String,
    pub protocol: u32,
    pub min_protocol: u32,
    pub max_protocol: u32,
    pub capabilities: Vec<DaemonCapability>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonCapability {
    pub method: String,
    pub since_protocol: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkspaceListResponse {
    pub workspaces: Vec<DaemonWorkspace>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonWorkspace {
    pub name: String,
    pub workdir: String,
    pub mount_count: usize,
    pub readonly_mount_count: usize,
    pub allowed_role_count: usize,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
    pub default_agent: Option<String>,
    pub keep_awake: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<DaemonSession>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonSession {
    pub container_name: String,
    pub status: String,
    pub display_name: String,
    pub workspace: Option<String>,
    pub role: Option<String>,
    pub agent: Option<String>,
    pub branch: Option<String>,
    pub primary_repo: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub pid: u32,
    pub uptime_seconds: u64,
    pub socket: String,
    pub log: String,
    pub keep_awake: String,
    pub cache_warmup: String,
    #[serde(default)]
    pub notifications: String,
}

struct DaemonState {
    paths: JackinPaths,
    started_at: SystemTime,
    shutdown: Arc<AtomicBool>,
}

pub fn exec(command: DaemonCommand) -> Result<()> {
    let paths = JackinPaths::detect()?;
    paths.ensure_base_dirs()?;
    ensure_private_run_dir(&paths.run_dir)?;

    match command {
        DaemonCommand::Install => install(&paths),
        DaemonCommand::Uninstall => {
            uninstall(&paths);
            Ok(())
        }
        DaemonCommand::Start => start(&paths),
        DaemonCommand::Stop => {
            stop(&paths);
            Ok(())
        }
        DaemonCommand::Restart => {
            stop(&paths);
            start(&paths)
        }
        DaemonCommand::Status => print_status(&paths),
        DaemonCommand::Logs(LogsArgs { lines }) => print_logs(&paths, lines),
        DaemonCommand::Serve => serve(&paths),
        DaemonCommand::Warm => {
            ensure_started(&paths)?;
            let response = send_request(
                &paths,
                &Request::WarmCache {
                    protocol: PROTOCOL_VERSION,
                },
            )?;
            print_response(response);
            Ok(())
        }
        DaemonCommand::Notify(NotifyArgs {
            title,
            body,
            urgency,
        }) => {
            ensure_started(&paths)?;
            let response = send_request(
                &paths,
                &Request::Notify {
                    protocol: PROTOCOL_VERSION,
                    title,
                    body,
                    urgency,
                },
            )?;
            print_response(response);
            Ok(())
        }
    }
}

pub fn ensure_started(paths: &JackinPaths) -> Result<()> {
    match send_request(
        paths,
        &Request::Status {
            protocol: PROTOCOL_VERSION,
        },
    ) {
        Ok(Response::Status(status)) if status.version == env!("JACKIN_VERSION") => return Ok(()),
        Ok(Response::Status(_)) => stop(paths),
        Ok(Response::Error { message }) if message.contains("protocol mismatch") => stop(paths),
        Ok(_) | Err(_) => {}
    }
    start(paths)
}

fn current_daemon_status(paths: &JackinPaths) -> Option<StatusResponse> {
    match send_request(
        paths,
        &Request::Status {
            protocol: PROTOCOL_VERSION,
        },
    ) {
        Ok(Response::Status(status)) => Some(status),
        Ok(Response::Error { message }) if message.contains("protocol mismatch") => {
            stop(paths);
            None
        }
        Ok(_) | Err(_) => None,
    }
}

fn serve(paths: &JackinPaths) -> Result<()> {
    ensure_private_run_dir(&paths.run_dir)?;
    let lock_path = paths.run_dir.join(LOCK_FILENAME);
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening {}", lock_path.display()))?;
    lock.try_lock_exclusive().with_context(|| {
        format!(
            "another jackin daemon is already running (lock {})",
            lock_path.display()
        )
    })?;

    let socket_path = socket_path(paths);
    remove_stale_socket(&socket_path)?;
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding daemon socket {}", socket_path.display()))?;
    listener
        .set_nonblocking(true)
        .context("setting daemon socket nonblocking")?;
    write_pid(paths, std::process::id())?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let state = Arc::new(DaemonState {
        paths: paths.clone(),
        started_at: SystemTime::now(),
        shutdown: Arc::clone(&shutdown),
    });

    log_line(paths, "daemon started");
    spawn_keep_awake_reconciler(&state);
    spawn_cache_warmer(&state);

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                std::thread::spawn(move || {
                    if let Err(err) = handle_client(stream, &state) {
                        log_line(&state.paths, &format!("client error: {err:#}"));
                    }
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(err).context("accepting daemon connection"),
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(paths.run_dir.join(PID_FILENAME));
    log_line(paths, "daemon stopped");
    drop(lock);
    Ok(())
}

fn handle_client(mut stream: UnixStream, state: &DaemonState) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let response = match serde_json::from_str::<Request>(&line) {
        Ok(request) => handle_request(request, state),
        Err(err) => Response::Error {
            message: format!("invalid daemon request: {err}"),
        },
    };
    serde_json::to_writer(&mut stream, &response)?;
    writeln!(&mut stream)?;
    Ok(())
}

fn handle_request(request: Request, state: &DaemonState) -> Response {
    if let Err(message) = validate_protocol(&request) {
        return Response::Error { message };
    }

    match request {
        Request::Hello { .. } => Response::Hello(hello()),
        Request::WorkspaceList { .. } => match load_daemon_config(&state.paths) {
            Ok(config) => Response::WorkspaceList(WorkspaceListResponse {
                workspaces: workspace_summaries(&config),
            }),
            Err(err) => Response::Error {
                message: format!("{err:#}"),
            },
        },
        Request::SessionList { .. } => {
            let mut runner = ShellRunner::default();
            match session_summaries(&mut runner) {
                Ok(sessions) => Response::SessionList(SessionListResponse { sessions }),
                Err(err) => Response::Error {
                    message: format!("{err:#}"),
                },
            }
        }
        Request::Status { .. } => Response::Status(status(state)),
        Request::Shutdown { .. } => {
            state.shutdown.store(true, Ordering::Relaxed);
            Response::Ok {
                message: "daemon stopping".to_string(),
            }
        }
        Request::WarmCache { .. } => match warm_cache(&state.paths) {
            Ok(summary) => Response::Ok { message: summary },
            Err(err) => Response::Error {
                message: format!("{err:#}"),
            },
        },
        Request::Notify {
            title,
            body,
            urgency,
            ..
        } => match dispatch_notification(&title, &body, &urgency) {
            Ok(()) => Response::Ok {
                message: "notification sent".to_string(),
            },
            Err(err) => Response::Error {
                message: format!("{err:#}"),
            },
        },
    }
}

fn validate_protocol(request: &Request) -> std::result::Result<(), String> {
    let protocol = match request {
        Request::Hello { protocol }
        | Request::WorkspaceList { protocol }
        | Request::SessionList { protocol }
        | Request::Status { protocol }
        | Request::Shutdown { protocol }
        | Request::WarmCache { protocol }
        | Request::Notify { protocol, .. } => *protocol,
    };
    if (MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&protocol) {
        Ok(())
    } else {
        Err(format!(
            "daemon protocol mismatch: client={protocol} daemon={MIN_PROTOCOL_VERSION}..={PROTOCOL_VERSION}; run `jackin daemon restart`"
        ))
    }
}

fn hello() -> HelloResponse {
    HelloResponse {
        version: env!("JACKIN_VERSION").to_string(),
        protocol: PROTOCOL_VERSION,
        min_protocol: MIN_PROTOCOL_VERSION,
        max_protocol: PROTOCOL_VERSION,
        capabilities: daemon_capabilities(),
    }
}

fn daemon_capabilities() -> Vec<DaemonCapability> {
    [
        "daemon/hello",
        "daemon/status",
        "daemon/shutdown",
        "daemon/warm_cache",
        "workspace/list",
        "session/list",
        "notification/send",
    ]
    .into_iter()
    .map(|method| DaemonCapability {
        method: method.to_string(),
        since_protocol: PROTOCOL_VERSION,
    })
    .collect()
}

fn load_daemon_config(paths: &JackinPaths) -> Result<crate::config::AppConfig> {
    match std::fs::read_to_string(&paths.config_file) {
        Ok(contents) => toml::from_str(&contents).context("decoding jackin config"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(crate::config::AppConfig::default())
        }
        Err(err) => Err(err).with_context(|| format!("reading {}", paths.config_file.display())),
    }
}

fn workspace_summaries(config: &crate::config::AppConfig) -> Vec<DaemonWorkspace> {
    config
        .list_workspaces()
        .into_iter()
        .map(|(name, workspace)| DaemonWorkspace {
            name: name.to_string(),
            workdir: workspace.workdir.clone(),
            mount_count: workspace.mounts.len(),
            readonly_mount_count: workspace
                .mounts
                .iter()
                .filter(|mount| mount.readonly)
                .count(),
            allowed_role_count: workspace.allowed_roles.len(),
            default_role: workspace.default_role.clone(),
            last_role: workspace.last_role.clone(),
            default_agent: workspace
                .default_agent
                .map(|agent| agent.slug().to_string()),
            keep_awake: workspace.keep_awake.enabled,
        })
        .collect()
}

fn session_summaries(runner: &mut impl CommandRunner) -> Result<Vec<DaemonSession>> {
    let output = runner.capture(
        "docker",
        &[
            "ps",
            "--filter",
            "label=jackin.kind=role",
            "--format",
            "{{.Names}}\t{{.Status}}\t{{.Label \"jackin.display_name\"}}\t{{.Label \"jackin.workspace\"}}\t{{.Label \"jackin.role_key\"}}\t{{.Label \"jackin.agent\"}}\t{{.Label \"jackin.branch\"}}\t{{.Label \"jackin.primary_repo\"}}",
        ],
        None,
    )?;
    Ok(parse_session_rows(&output))
}

fn parse_session_rows(output: &str) -> Vec<DaemonSession> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(parse_session_row)
        .collect()
}

fn parse_session_row(line: &str) -> Option<DaemonSession> {
    let mut parts = line.splitn(8, '\t');
    let container_name = parts.next()?.trim();
    if container_name.is_empty() {
        return None;
    }
    let status = parts.next().unwrap_or("").trim();
    let display_name = parts.next().unwrap_or("").trim();
    Some(DaemonSession {
        container_name: container_name.to_string(),
        status: status.to_string(),
        display_name: if display_name.is_empty() {
            container_name.to_string()
        } else {
            display_name.to_string()
        },
        workspace: non_empty_label(parts.next()),
        role: non_empty_label(parts.next()),
        agent: non_empty_label(parts.next()),
        branch: non_empty_label(parts.next()),
        primary_repo: non_empty_label(parts.next()),
    })
}

fn non_empty_label(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}

fn status(state: &DaemonState) -> StatusResponse {
    let uptime_seconds = state
        .started_at
        .elapsed()
        .map_or(0, |duration| duration.as_secs());
    StatusResponse {
        version: env!("JACKIN_VERSION").to_string(),
        pid: std::process::id(),
        uptime_seconds,
        socket: socket_path(&state.paths).display().to_string(),
        log: log_path(&state.paths).display().to_string(),
        keep_awake: if cfg!(target_os = "macos") {
            "enabled".to_string()
        } else {
            "unsupported on this host".to_string()
        },
        cache_warmup: "construct, dind, cached published images".to_string(),
        notifications: notification_adapter_status(),
    }
}

fn send_request(paths: &JackinPaths, request: &Request) -> Result<Response> {
    let mut stream =
        UnixStream::connect(socket_path(paths)).context("connecting to jackin daemon")?;
    serde_json::to_writer(&stream, request)?;
    writeln!(stream)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    serde_json::from_str(&line).context("decoding daemon response")
}

fn start(paths: &JackinPaths) -> Result<()> {
    if let Some(status) = current_daemon_status(paths) {
        if status.version == env!("JACKIN_VERSION") {
            println!(
                "jackin daemon already running (pid {}, version {})",
                status.pid, status.version
            );
            return Ok(());
        }
        stop(paths);
    }

    let exe = std::env::current_exe().context("resolving current jackin executable")?;
    let log = log_path(paths);
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .with_context(|| format!("opening {}", log.display()))?;
    let stderr = stdout.try_clone()?;
    Command::new(exe)
        .args(["daemon", "serve"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .context("starting jackin daemon")?;

    wait_until_ready(paths)?;
    println!("jackin daemon started");
    Ok(())
}

fn stop(paths: &JackinPaths) {
    match send_request(
        paths,
        &Request::Shutdown {
            protocol: PROTOCOL_VERSION,
        },
    ) {
        Ok(response @ Response::Ok { .. }) => {
            print_response(response);
            wait_until_stopped(paths);
        }
        Ok(response @ Response::Error { .. }) if is_protocol_mismatch_response(&response) => {
            stop_from_pid_file(paths);
        }
        Ok(response) => {
            print_response(response);
            stop_from_pid_file(paths);
        }
        Err(_) => stop_from_pid_file(paths),
    }
}

fn stop_from_pid_file(paths: &JackinPaths) {
    let pid_path = paths.run_dir.join(PID_FILENAME);
    if let Ok(pid) = std::fs::read_to_string(&pid_path)
        && let Ok(pid) = pid.trim().parse::<u32>()
    {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    let _ = std::fs::remove_file(socket_path(paths));
    let _ = std::fs::remove_file(pid_path);
    println!("jackin daemon stopped");
}

fn print_status(paths: &JackinPaths) -> Result<()> {
    ensure_started(paths)?;
    match send_request(
        paths,
        &Request::Status {
            protocol: PROTOCOL_VERSION,
        },
    )? {
        Response::Status(status) => {
            println!("running");
            println!("  version: {}", status.version);
            println!("  pid: {}", status.pid);
            println!("  uptime: {}s", status.uptime_seconds);
            println!("  socket: {}", status.socket);
            println!("  log: {}", status.log);
            println!("  keep_awake: {}", status.keep_awake);
            println!("  cache_warmup: {}", status.cache_warmup);
            println!("  notifications: {}", status.notifications);
        }
        other => print_response(other),
    }
    Ok(())
}

fn print_logs(paths: &JackinPaths, lines: usize) -> Result<()> {
    let path = log_path(paths);
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };
    let recent = contents.lines().rev().take(lines).collect::<Vec<_>>();
    for line in recent.into_iter().rev() {
        println!("{line}");
    }
    Ok(())
}

fn install(paths: &JackinPaths) -> Result<()> {
    let exe = std::env::current_exe().context("resolving current jackin executable")?;
    if cfg!(target_os = "macos") {
        install_launchd(paths, &exe)?;
    } else if command_exists("systemctl") {
        install_systemd(paths, &exe)?;
    } else {
        start(paths)?;
    }
    Ok(())
}

fn uninstall(paths: &JackinPaths) {
    if cfg!(target_os = "macos") {
        let plist = launchd_plist_path(paths);
        let _ = Command::new("launchctl")
            .args(["unload", plist.to_string_lossy().as_ref()])
            .status();
        let _ = std::fs::remove_file(plist);
    } else if command_exists("systemctl") {
        let unit = systemd_unit_path(paths);
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", SYSTEMD_UNIT])
            .status();
        let _ = std::fs::remove_file(unit);
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }
    stop(paths);
}

fn install_launchd(paths: &JackinPaths, exe: &Path) -> Result<()> {
    let plist = launchd_plist_path(paths);
    let parent = plist
        .parent()
        .ok_or_else(|| anyhow::anyhow!("launchd plist path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let log = log_path(paths);
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>daemon</string>
    <string>serve</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml_escape(&exe.display().to_string()),
        xml_escape(&log.display().to_string()),
        xml_escape(&log.display().to_string())
    );
    std::fs::write(&plist, content).with_context(|| format!("writing {}", plist.display()))?;
    let _ = Command::new("launchctl")
        .args(["unload", plist.to_string_lossy().as_ref()])
        .status();
    Command::new("launchctl")
        .args(["load", "-w", plist.to_string_lossy().as_ref()])
        .status()
        .context("loading launchd LaunchAgent")?;
    println!("installed launchd LaunchAgent at {}", plist.display());
    Ok(())
}

fn install_systemd(paths: &JackinPaths, exe: &Path) -> Result<()> {
    let unit = systemd_unit_path(paths);
    let parent = unit
        .parent()
        .ok_or_else(|| anyhow::anyhow!("systemd unit path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let content = format!(
        "[Unit]\nDescription=jackin daemon\n\n[Service]\nExecStart={} daemon serve\nRestart=always\n\n[Install]\nWantedBy=default.target\n",
        exe.display()
    );
    std::fs::write(&unit, content).with_context(|| format!("writing {}", unit.display()))?;
    Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .context("systemd --user daemon-reload")?;
    Command::new("systemctl")
        .args(["--user", "enable", "--now", SYSTEMD_UNIT])
        .status()
        .context("enabling systemd user service")?;
    println!("installed systemd user service at {}", unit.display());
    Ok(())
}

fn spawn_keep_awake_reconciler(state: &Arc<DaemonState>) {
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        let mut runner = ShellRunner::default();
        while !state.shutdown.load(Ordering::Relaxed) {
            crate::runtime::reconcile_keep_awake(&state.paths, &mut runner);
            sleep_or_shutdown(&state.shutdown, Duration::from_secs(5));
        }
    });
}

fn spawn_cache_warmer(state: &Arc<DaemonState>) {
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        while !state.shutdown.load(Ordering::Relaxed) {
            if let Err(err) = warm_cache(&state.paths) {
                log_line(&state.paths, &format!("cache warmup failed: {err:#}"));
            }
            sleep_or_shutdown(&state.shutdown, Duration::from_mins(15));
        }
    });
}

fn warm_cache(paths: &JackinPaths) -> Result<String> {
    let mut images = vec![
        crate::repo_contract::CONSTRUCT_IMAGE.to_string(),
        "docker:dind".to_string(),
    ];
    images.extend(cached_published_images(paths)?);
    images.sort();
    images.dedup();

    let mut runner = ShellRunner::default();
    let mut pulled = 0;
    for image in &images {
        let result = runner.run(
            "docker",
            &["pull", image],
            None,
            &RunOptions {
                quiet: true,
                ..RunOptions::default()
            },
        );
        if result.is_ok() {
            pulled += 1;
        } else if let Err(err) = result {
            log_line(paths, &format!("cache warmup skipped {image}: {err:#}"));
        }
    }
    let summary = format!(
        "cache warmup complete: {pulled}/{} images checked",
        images.len()
    );
    log_line(paths, &summary);
    Ok(summary)
}

fn cached_published_images(paths: &JackinPaths) -> Result<Vec<String>> {
    let mut images = Vec::new();
    collect_manifest_images(&paths.roles_dir, &mut images)?;
    Ok(images)
}

fn collect_manifest_images(dir: &Path, images: &mut Vec<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_manifest_images(&path, images)?;
        } else if path.file_name().and_then(|name| name.to_str()) == Some("jackin.role.toml")
            && let Ok(manifest) = crate::manifest::RoleManifest::load(
                path.parent()
                    .ok_or_else(|| anyhow::anyhow!("manifest path has no parent"))?,
            )
            && let Some(image) = manifest.published_image
        {
            images.push(image);
        }
    }
    Ok(())
}

fn dispatch_notification(title: &str, body: &str, urgency: &str) -> Result<()> {
    let notification = HostNotification::new(title, body, urgency)?;
    dispatch_macos_notification(&notification)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NotificationUrgency {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostNotification {
    title: String,
    body: String,
    urgency: NotificationUrgency,
}

impl HostNotification {
    fn new(title: &str, body: &str, urgency: &str) -> Result<Self> {
        let title = title.trim();
        let body = body.trim();
        anyhow::ensure!(!title.is_empty(), "notification title must not be empty");
        anyhow::ensure!(!body.is_empty(), "notification body must not be empty");
        let urgency = match urgency.trim() {
            "" | "normal" => NotificationUrgency::Normal,
            "low" => NotificationUrgency::Low,
            "high" => NotificationUrgency::High,
            other => anyhow::bail!(
                "invalid notification urgency {other:?}; expected low, normal, or high"
            ),
        };
        Ok(Self {
            title: title.to_string(),
            body: body.to_string(),
            urgency,
        })
    }
}

fn dispatch_macos_notification(notification: &HostNotification) -> Result<()> {
    anyhow::ensure!(
        cfg!(target_os = "macos"),
        "host notifications are currently supported on macOS only"
    );

    let script = macos_notification_script(notification);
    let status = Command::new("osascript")
        .args(["-e", &script])
        .status()
        .context("sending macOS notification through osascript")?;
    anyhow::ensure!(
        status.success(),
        "osascript failed to deliver macOS notification"
    );
    Ok(())
}

fn macos_notification_script(notification: &HostNotification) -> String {
    let mut script = format!(
        "display notification \"{}\" with title \"{}\" subtitle \"jackin\"",
        applescript_escape(&notification.body),
        applescript_escape(&notification.title)
    );
    if notification.urgency == NotificationUrgency::High {
        script.push_str(" sound name \"Glass\"");
    }
    script
}

fn notification_adapter_status() -> String {
    if cfg!(target_os = "macos") {
        "macOS Notification Center via osascript".to_string()
    } else {
        "unsupported on this host; macOS first".to_string()
    }
}

fn sleep_or_shutdown(shutdown: &AtomicBool, duration: Duration) {
    let deadline = SystemTime::now() + duration;
    while !shutdown.load(Ordering::Relaxed) && SystemTime::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn wait_until_ready(paths: &JackinPaths) -> Result<()> {
    let mut last_error = None;
    for _ in 0..50 {
        match send_request(
            paths,
            &Request::Status {
                protocol: PROTOCOL_VERSION,
            },
        ) {
            Ok(response) if is_ready_response(&response) => return Ok(()),
            Ok(response) => last_error = Some(format!("daemon not ready: {response:?}")),
            Err(err) => last_error = Some(format!("{err:#}")),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let detail = last_error.unwrap_or_else(|| "no response".to_string());
    anyhow::bail!(
        "daemon did not become ready ({detail}); see {}",
        log_path(paths).display()
    )
}

fn is_ready_response(response: &Response) -> bool {
    matches!(
        response,
        Response::Status(status) if status.version == env!("JACKIN_VERSION")
    )
}

fn is_protocol_mismatch_response(response: &Response) -> bool {
    matches!(
        response,
        Response::Error { message } if message.contains("protocol mismatch")
    )
}

fn wait_until_stopped(paths: &JackinPaths) {
    for _ in 0..50 {
        if send_request(
            paths,
            &Request::Status {
                protocol: PROTOCOL_VERSION,
            },
        )
        .is_err()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn ensure_private_run_dir(run_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(run_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(run_dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn remove_stale_socket(socket_path: &Path) -> Result<()> {
    match std::fs::remove_file(socket_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("removing {}", socket_path.display())),
    }
}

fn write_pid(paths: &JackinPaths, pid: u32) -> Result<()> {
    std::fs::write(paths.run_dir.join(PID_FILENAME), pid.to_string()).context("writing daemon pid")
}

fn print_response(response: Response) {
    match response {
        Response::Ok { message } => println!("{message}"),
        Response::Hello(hello) => println!(
            "daemon protocol {} (version {}, {} capabilities)",
            hello.protocol,
            hello.version,
            hello.capabilities.len()
        ),
        Response::WorkspaceList(response) => {
            println!("{} workspaces", response.workspaces.len());
        }
        Response::SessionList(response) => {
            println!("{} sessions", response.sessions.len());
        }
        Response::Status(status) => println!("daemon running: pid {}", status.pid),
        Response::Error { message } => eprintln!("error: {message}"),
    }
}

fn log_line(paths: &JackinPaths, message: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path(paths))
    {
        let _ = writeln!(file, "{ts} {message}");
    }
}

fn socket_path(paths: &JackinPaths) -> PathBuf {
    paths.run_dir.join(SOCKET_FILENAME)
}

fn log_path(paths: &JackinPaths) -> PathBuf {
    paths.run_dir.join(LOG_FILENAME)
}

fn launchd_plist_path(paths: &JackinPaths) -> PathBuf {
    paths
        .home_dir
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist"))
}

fn systemd_unit_path(paths: &JackinPaths) -> PathBuf {
    paths
        .home_dir
        .join(".config/systemd/user")
        .join(SYSTEMD_UNIT)
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {command} >/dev/null 2>&1")])
        .status()
        .is_ok_and(|status| status.success())
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn applescript_escape(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cached_published_images_reads_nested_role_manifests() {
        let tmp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        let role_dir = paths.roles_dir.join("agent-smith/default");
        std::fs::create_dir_all(&role_dir).unwrap();
        std::fs::write(
            role_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
published_image = "ghcr.io/example/role:latest"

[claude]
"#,
        )
        .unwrap();

        let images = cached_published_images(&paths).unwrap();

        assert_eq!(images, vec!["ghcr.io/example/role:latest"]);
    }

    #[test]
    fn validate_protocol_rejects_mismatch() {
        let error = validate_protocol(&Request::Status { protocol: 999 }).unwrap_err();
        assert!(error.contains("protocol mismatch"));
    }

    #[test]
    fn hello_response_advertises_versioned_capabilities() {
        let response = hello();
        let methods = response
            .capabilities
            .iter()
            .map(|capability| capability.method.as_str())
            .collect::<Vec<_>>();

        assert_eq!(response.version, env!("JACKIN_VERSION"));
        assert_eq!(response.protocol, PROTOCOL_VERSION);
        assert_eq!(response.min_protocol, MIN_PROTOCOL_VERSION);
        assert_eq!(response.max_protocol, PROTOCOL_VERSION);
        assert!(methods.contains(&"daemon/hello"));
        assert!(methods.contains(&"daemon/status"));
        assert!(methods.contains(&"workspace/list"));
        assert!(methods.contains(&"session/list"));
        assert!(methods.contains(&"notification/send"));
    }

    #[test]
    fn hello_request_and_response_use_versioned_wire_names() {
        let request_json = serde_json::to_value(Request::Hello {
            protocol: PROTOCOL_VERSION,
        })
        .unwrap();
        let response_json = serde_json::to_value(Response::Hello(hello())).unwrap();

        assert_eq!(request_json["type"], "daemon/hello");
        assert_eq!(request_json["protocol"], PROTOCOL_VERSION);
        assert_eq!(response_json["type"], "daemon/hello");
        assert_eq!(response_json["protocol"], PROTOCOL_VERSION);
        assert!(response_json["capabilities"].is_array());
    }

    #[test]
    fn hello_request_rejects_protocol_mismatch() {
        let tmp = tempdir().unwrap();
        let state = DaemonState {
            paths: JackinPaths::for_tests(tmp.path()),
            started_at: SystemTime::now(),
            shutdown: Arc::new(AtomicBool::new(false)),
        };

        let response = handle_request(Request::Hello { protocol: 999 }, &state);

        assert!(
            matches!(response, Response::Error { message } if message.contains("protocol mismatch"))
        );
    }

    #[test]
    fn workspace_summaries_expose_saved_workspace_metadata() {
        let mut config = crate::config::AppConfig::default();
        config.workspaces.insert(
            "project".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: "/workspace/project".to_string(),
                mounts: vec![
                    crate::workspace::MountConfig {
                        src: "/host/project".to_string(),
                        dst: "/workspace/project".to_string(),
                        readonly: false,
                        isolation: crate::isolation::MountIsolation::Shared,
                    },
                    crate::workspace::MountConfig {
                        src: "/host/docs".to_string(),
                        dst: "/workspace/docs".to_string(),
                        readonly: true,
                        isolation: crate::isolation::MountIsolation::Shared,
                    },
                ],
                allowed_roles: vec!["agent-smith".to_string()],
                default_role: Some("agent-smith".to_string()),
                default_agent: Some(crate::agent::Agent::Codex),
                last_role: Some("agent-smith".to_string()),
                keep_awake: crate::workspace::KeepAwakeConfig { enabled: true },
                ..crate::workspace::WorkspaceConfig::default()
            },
        );

        let workspaces = workspace_summaries(&config);

        assert_eq!(
            workspaces,
            vec![DaemonWorkspace {
                name: "project".to_string(),
                workdir: "/workspace/project".to_string(),
                mount_count: 2,
                readonly_mount_count: 1,
                allowed_role_count: 1,
                default_role: Some("agent-smith".to_string()),
                last_role: Some("agent-smith".to_string()),
                default_agent: Some("codex".to_string()),
                keep_awake: true,
            }]
        );
    }

    #[test]
    fn parse_session_rows_uses_launch_labels() {
        let output = "jackin-agent-smith\tUp 2 minutes\tAgent Smith\tproject\tagent-smith\tclaude\tmain\t/workspace/project\n";

        let sessions = parse_session_rows(output);

        assert_eq!(
            sessions,
            vec![DaemonSession {
                container_name: "jackin-agent-smith".to_string(),
                status: "Up 2 minutes".to_string(),
                display_name: "Agent Smith".to_string(),
                workspace: Some("project".to_string()),
                role: Some("agent-smith".to_string()),
                agent: Some("claude".to_string()),
                branch: Some("main".to_string()),
                primary_repo: Some("/workspace/project".to_string()),
            }]
        );
    }

    #[test]
    fn parse_session_rows_falls_back_for_unlabeled_sessions() {
        let sessions = parse_session_rows("jackin-old\tUp 10 seconds\t\t\t\t\t\t\n");

        assert_eq!(
            sessions,
            vec![DaemonSession {
                container_name: "jackin-old".to_string(),
                status: "Up 10 seconds".to_string(),
                display_name: "jackin-old".to_string(),
                workspace: None,
                role: None,
                agent: None,
                branch: None,
                primary_repo: None,
            }]
        );
    }

    #[test]
    fn protocol_mismatch_response_is_shutdown_fallback_signal() {
        let response = Response::Error {
            message: "daemon protocol mismatch: client=2 daemon=1..=1".to_string(),
        };

        assert!(is_protocol_mismatch_response(&response));
    }

    #[test]
    fn readiness_requires_current_version_status() {
        let status = StatusResponse {
            version: env!("JACKIN_VERSION").to_string(),
            pid: 123,
            uptime_seconds: 1,
            socket: "/tmp/jackin.sock".to_string(),
            log: "/tmp/jackin.log".to_string(),
            keep_awake: "enabled".to_string(),
            cache_warmup: "enabled".to_string(),
            notifications: "macOS Notification Center via osascript".to_string(),
        };

        assert!(is_ready_response(&Response::Status(status)));
        assert!(!is_ready_response(&Response::Error {
            message: "daemon protocol mismatch: client=2 daemon=1..=1".to_string(),
        }));
    }

    #[test]
    fn readiness_rejects_old_version_status() {
        let status = StatusResponse {
            version: "0.0.0-old".to_string(),
            pid: 123,
            uptime_seconds: 1,
            socket: "/tmp/jackin.sock".to_string(),
            log: "/tmp/jackin.log".to_string(),
            keep_awake: "enabled".to_string(),
            cache_warmup: "enabled".to_string(),
            notifications: "macOS Notification Center via osascript".to_string(),
        };

        assert!(!is_ready_response(&Response::Status(status)));
    }

    #[test]
    fn host_notification_trims_and_validates_urgency() {
        let notification = HostNotification::new(" title ", " body ", "high").unwrap();

        assert_eq!(notification.title, "title");
        assert_eq!(notification.body, "body");
        assert_eq!(notification.urgency, NotificationUrgency::High);
    }

    #[test]
    fn host_notification_rejects_unknown_urgency() {
        let error = HostNotification::new("title", "body", "urgent").unwrap_err();

        assert!(error.to_string().contains("expected low, normal, or high"));
    }

    #[test]
    fn macos_notification_script_escapes_and_adds_sound_for_high_urgency() {
        let notification =
            HostNotification::new("Jackin \"Agent\"", "Needs \\ input", "high").unwrap();

        let script = macos_notification_script(&notification);

        assert!(script.contains("with title \"Jackin \\\"Agent\\\"\""));
        assert!(script.contains("display notification \"Needs \\\\ input\""));
        assert!(script.contains("sound name \"Glass\""));
    }
}
