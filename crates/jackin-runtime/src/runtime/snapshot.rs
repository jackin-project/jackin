// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side fetch of the in-container `jackin-capsule` daemon's
//! tab/pane snapshot.
//!
//! The daemon's socket is bind-mounted from
//! `paths.jackin_home/sockets/<container_name>/jackin.sock` so same-
//! kernel Docker hosts can talk to it directly via a `UnixStream`.
//! Docker Desktop for macOS exposes the socket inode through the bind
//! mount but cannot bridge the live Unix socket across the Linux VM
//! boundary, so the host falls back to `docker exec ... snapshot`
//! when the direct connection is absent or refused.
//!
//! Schema sharing: the protocol types live in `jackin_protocol`, a
//! small shared crate. The host CLI and in-container Capsule both
//! depend on it, so request and reply structs are imported verbatim.
//! Drift between the two surfaces is a compile error, not a
//! wire-format bug.
//!
//! Sync API by design: the only caller today is
//! `ManagerState::refresh_instances`, which runs inside the host
//! TUI's render loop. A blocking std `UnixStream` round-trip or
//! bounded `docker exec` fallback is kept behind the existing 500 ms
//! refresh throttle.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use jackin_protocol::control::{
    AccountUsageSnapshotView, ClientMsg, ControlRequest, ServerMsg, TabSnapshot,
    frame as control_frame,
};
use serde::Deserialize;

use jackin_core::JackinPaths;

// `InstanceSnapshot` lives in `jackin-protocol` so the console can use it
// without depending on `jackin-runtime`.
pub use jackin_protocol::InstanceSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotTransport {
    DirectSocket,
    DockerExecFallback,
}

/// Cap on the JSON reply read from the daemon. Must be ≥ the daemon's
/// frame cap so legitimate Status / Snapshot replies fit; oversized
/// replies are rejected to bound host memory.
const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;

/// Per-call socket timeout. The whole round-trip is "open socket,
/// write 5 + json bytes, read 4 + json bytes" — anything beyond a
/// couple seconds means the daemon is wedged or the bind-mount
/// points at a stale dir. The console's preview pane re-polls on a
/// cadence, so a short timeout here keeps a dead container from
/// stalling the UI.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Deserialize)]
struct SnapshotPayload {
    tabs: Vec<TabSnapshot>,
    active_tab: u32,
}

/// Build the host-side path of a container's daemon socket. Matches
/// the bind-mount source set up in `runtime/launch.rs`.
pub fn socket_path(paths: &JackinPaths, container_name: &str) -> PathBuf {
    paths
        .jackin_home
        .join("sockets")
        .join(container_name)
        .join("jackin.sock")
}

/// Connect to the container's daemon socket and fetch its tab/pane
/// snapshot.
///
/// Same-kernel Docker hosts can read the bind-mounted socket directly.
/// Docker Desktop for macOS cannot; in that case, or when the socket
/// is absent because the container predates the bind mount, this falls
/// back to the in-container client via `docker exec`.
pub fn fetch_snapshot(
    paths: &JackinPaths,
    container_name: &str,
) -> Result<Option<InstanceSnapshot>> {
    fetch_snapshot_with_transport(paths, container_name).map(|(snapshot, _transport)| snapshot)
}

pub fn fetch_snapshot_with_transport(
    paths: &JackinPaths,
    container_name: &str,
) -> Result<(Option<InstanceSnapshot>, SnapshotTransport)> {
    let path = socket_path(paths, container_name);
    let mut direct_error = None;
    if path.exists() {
        match request_control_inner(&path, &ClientMsg::Snapshot).and_then(snapshot_from_msg) {
            Ok(snapshot) => return Ok((Some(snapshot), SnapshotTransport::DirectSocket)),
            Err(error) => direct_error = Some(error),
        }
    }

    match fetch_snapshot_via_docker_exec(container_name) {
        Ok(snapshot) => Ok((snapshot, SnapshotTransport::DockerExecFallback)),
        Err(exec_error) => match direct_error {
            Some(error) => Err(exec_error.context(format!(
                "direct socket snapshot failed for {} ({error:#})",
                path.display()
            ))),
            None => Err(exec_error),
        },
    }
}

pub fn fetch_usage_accounts(
    paths: &JackinPaths,
    container_name: &str,
) -> Result<Option<Vec<AccountUsageSnapshotView>>> {
    let path = socket_path(paths, container_name);
    let mut direct_error = None;
    if path.exists() {
        match request_control_inner(&path, &ClientMsg::UsageAccountList).and_then(accounts_from_msg)
        {
            Ok(accounts) => return Ok(Some(accounts)),
            Err(error) => direct_error = Some(error),
        }
    }

    match fetch_usage_accounts_via_docker_exec(container_name) {
        Ok(accounts) => Ok(accounts),
        Err(exec_error) => match direct_error {
            Some(error) => Err(exec_error.context(format!(
                "direct socket usage accounts failed for {} ({error:#})",
                path.display()
            ))),
            None => Err(exec_error),
        },
    }
}

fn request_control_inner(path: &Path, request: &ClientMsg) -> Result<ServerMsg> {
    let mut stream = jackin_diagnostics::operation::connection_attempt_sync(
        jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleControl,
        || UnixStream::connect(path),
    )
    .with_context(|| format!("connecting to daemon socket {}", path.display()))?;
    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .context("setting read timeout")?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .context("setting write timeout")?;

    stream
        .write_all(&control_frame(&ControlRequest {
            ctx: {
                let mut ctx = jackin_protocol::TelemetryContext::v1();
                jackin_telemetry::propagation::inject(&mut ctx);
                ctx
            },
            msg: request.clone(),
        }))
        .context("writing control request to daemon")?;

    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .context("reading reply length")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_CONTROL_REPLY {
        bail!("daemon reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).context("reading reply body")?;

    serde_json::from_slice(&body).context("parsing reply JSON")
}

fn snapshot_from_msg(msg: ServerMsg) -> Result<InstanceSnapshot> {
    match msg {
        ServerMsg::Snapshot { tabs, active_tab } => Ok(InstanceSnapshot { tabs, active_tab }),
        // `Unknown` is the `#[serde(other)]` sink for variants from a newer daemon.
        ServerMsg::Unknown => bail!("daemon replied with an unknown ServerMsg variant"),
        other => bail!("daemon replied with {}; expected Snapshot", other.kind()),
    }
}

fn accounts_from_msg(msg: ServerMsg) -> Result<Vec<AccountUsageSnapshotView>> {
    match msg {
        ServerMsg::UsageAccounts { accounts } => Ok(accounts),
        other => bail!(
            "daemon replied with {}; expected UsageAccounts",
            other.kind()
        ),
    }
}

fn fetch_snapshot_via_docker_exec(container_name: &str) -> Result<Option<InstanceSnapshot>> {
    let output = run_docker_exec_capsule(container_name, snapshot_exec_script())?;
    if !output.status.success() {
        bail!(
            "docker exec snapshot failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("snapshot stdout is not UTF-8")?;
    if stdout.trim().is_empty() {
        return Ok(None);
    }
    snapshot_from_cli_stdout(&stdout).map(Some)
}

fn fetch_usage_accounts_via_docker_exec(
    container_name: &str,
) -> Result<Option<Vec<AccountUsageSnapshotView>>> {
    let output = run_docker_exec_capsule(container_name, usage_accounts_exec_script())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if let Some(message) = stale_usage_subcommand_hint(container_name, stderr) {
            bail!("{message}");
        }
        bail!(
            "docker exec usage accounts failed with status {}: {}",
            output.status,
            stderr
        );
    }
    let stdout = String::from_utf8(output.stdout).context("usage accounts stdout is not UTF-8")?;
    if stdout.trim().is_empty() {
        return Ok(None);
    }
    usage_accounts_from_cli_stdout(&stdout).map(Some)
}

fn stale_usage_subcommand_hint(container_name: &str, stderr: &str) -> Option<String> {
    if stderr.contains("unknown jackin-capsule subcommand") && stderr.contains("\"usage\"") {
        return Some(format!(
            "docker exec usage accounts failed because container {container_name} is running a stale jackin-capsule binary without usage support; rerun `jackin-dev pr sync <PR_NUMBER>`, source the generated env.sh, relaunch the instance, then retry `jackin usage {container_name} verify`: {stderr}"
        ));
    }
    None
}

fn run_docker_exec_capsule(container_name: &str, script: &str) -> Result<std::process::Output> {
    // Match the container's run-time UID (`--user` on docker run) so fallback
    // exec reads host-UID-owned state, not as the image's baked UID 1000.
    let run_as_user = crate::runtime::identity::host_run_as_user();
    let mut args: Vec<&str> = vec!["exec"];
    if let Some(ref user) = run_as_user {
        args.push("--user");
        args.push(user.as_str());
    }
    args.extend_from_slice(&[container_name, "sh", "-lc", script]);
    let request = jackin_process::ExecRequest::new("docker", &args);
    let mut child = jackin_process::spawn_sync(&request)
        .with_context(|| format!("starting docker exec snapshot for {container_name}"))?;

    let deadline = Instant::now() + SOCKET_TIMEOUT;
    loop {
        if child
            .try_wait()
            .context("polling docker exec snapshot child")?
            .is_some()
        {
            return child
                .wait_with_output()
                .context("collecting docker exec snapshot output");
        }
        if Instant::now() >= deadline {
            drop(child.kill());
            // SIGKILL is async — bound the post-kill drain so an
            // unresponsive docker daemon does not leave us blocked
            // in `wait_with_output` while the pipe stays open. The
            // 500 ms ceiling caps how fast docker-exec children can
            // accumulate when the daemon is consistently wedged.
            let drain_deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < drain_deadline {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                #[expect(
                    clippy::disallowed_methods,
                    reason = "snapshot timeout drain runs on the caller's snapshot worker path"
                )]
                std::thread::sleep(Duration::from_millis(20));
            }
            let output = child.wait_with_output().ok();
            let stderr = output
                .as_ref()
                .map(|out| String::from_utf8_lossy(&out.stderr).trim().to_owned())
                .unwrap_or_default();
            bail!("docker exec snapshot timed out after {SOCKET_TIMEOUT:?}: {stderr}");
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "snapshot retry loop is bounded and not part of the render task"
        )]
        std::thread::sleep(Duration::from_millis(10));
    }
}

const fn snapshot_exec_script() -> &'static str {
    "exec /jackin/runtime/jackin-capsule snapshot"
}

const fn usage_accounts_exec_script() -> &'static str {
    "exec /jackin/runtime/jackin-capsule usage accounts"
}

fn snapshot_from_cli_stdout(stdout: &str) -> Result<InstanceSnapshot> {
    let payload: SnapshotPayload =
        serde_json::from_str(stdout).context("parsing jackin-capsule snapshot JSON")?;
    Ok(InstanceSnapshot {
        tabs: payload.tabs,
        active_tab: payload.active_tab,
    })
}

fn usage_accounts_from_cli_stdout(stdout: &str) -> Result<Vec<AccountUsageSnapshotView>> {
    serde_json::from_str(stdout).context("parsing jackin-capsule usage accounts JSON")
}

#[cfg(test)]
mod tests;
