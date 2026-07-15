// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side credential resolver for `jackin-exec`.
//!
//! Listens on a Unix socket at `~/.jackin/sockets/<container>/host.sock`
//! which is bind-mounted into the role container at `/jackin/run/host.sock`.
//! When the capsule daemon confirms an `ExecCommand` and the operator has
//! selected credentials in the picker, the capsule connects here to resolve
//! the on-demand env vars before running the command.
//!
//! The `jackin load` process stays alive for the session and this listener
//! runs as a `tokio::spawn` task alongside the blocking interactive attach.
//! Future work: migrate to the jackin❯ daemon so all running containers share
//! one host-side resolver.
//!
//! # Security
//!
//! The listener validates every incoming resolution request against the
//! `allowed_bindings` set configured at session start. Only (name, kind,
//! source) triples that exactly match an operator-configured binding are
//! resolved. Unknown refs are rejected with a `CredReply::Error` without
//! calling `op` or reading any host env var. This prevents a compromised in-container
//! process from requesting arbitrary secret resolution via the host socket.
//!
//! For `kind = "op"`, `source` must start with `op://` and the `--`
//! end-of-options sentinel is inserted before passing to `op read` to prevent
//! argument injection via crafted op:// values.
//!
//! On Linux, the listener also authenticates the socket peer with safe
//! `SO_PEERCRED` (`UnixStream::peer_cred`) and accepts only the container's
//! init process (`NSpid` innermost PID 1), which is the capsule daemon. That
//! binds credential resolution to the daemon path that already enforces the
//! operator picker. Non-Linux hosts do not expose this same-kernel
//! identity check here; Docker Desktop and the future reactive daemon remain
//! the tracked residuals.

use anyhow::{Context as _, Result};
use jackin_protocol::control::frame;
use jackin_protocol::{CredReply, CredRequest, ExecBinding, ExecKind};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// Start the host.sock listener.
///
/// Returns a `JoinHandle` the caller can cancel or await. The socket file is
/// created at `sock_path`; the caller is responsible for ensuring the parent
/// directory is already bind-mounted into the container.
///
/// `allowed_bindings` is the exhaustive set of credential refs the operator
/// configured for this session. Only refs in this set are resolved; any
/// incoming request that references an unknown (name, kind, source) triple
/// is rejected, preventing escalation from a compromised in-container process.
#[expect(
    clippy::print_stderr,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn start(
    sock_path: PathBuf,
    allowed_bindings: Vec<ExecBinding>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run_listener(&sock_path, &allowed_bindings, CallerAuth::CapsuleDaemon).await
        {
            // A returned error is a startup failure (bind/chmod/mkdir) — the
            // accept loop never returns otherwise. It means jackin-exec
            // credential resolution is unavailable for the whole session, so
            // surface it on the always-on tier rather than only under --debug.
            eprintln!("[jackin] warning: jackin-exec credential resolver unavailable: {e:#}");
            jackin_diagnostics::debug_log!("exec_host", "listener error: {e:#}");
        }
    })
}

/// Start the host.sock listener for a named container.
///
/// Resolves the per-container socket path under
/// `<jackin_home>/sockets/<container>/host.sock` — the directory the launch
/// path bind-mounts to `/jackin/run` — maps the operator's `exec_bindings`
/// to the allowed-resolution set, and spawns the listener. Shared by both the
/// Docker and apple-container launch paths.
pub fn start_for_container(
    jackin_home: &Path,
    container_name: &str,
    exec_bindings: &[ExecBinding],
) -> tokio::task::JoinHandle<()> {
    let sock_path = jackin_home
        .join("sockets")
        .join(container_name)
        .join("host.sock");
    start(sock_path, exec_bindings.to_vec())
}

#[derive(Clone, Copy, Debug)]
enum CallerAuth {
    CapsuleDaemon,
    #[cfg(all(test, target_os = "linux"))]
    PeerPid(u32),
}

async fn run_listener(
    sock_path: &Path,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
) -> Result<()> {
    // Remove stale socket from a previous session.
    drop(std::fs::remove_file(sock_path));
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
        // host.sock is the credential-resolution boundary: any process that can
        // connect and send an allow-listed (name,kind,source) triple gets the
        // secret resolved. Neither launch path tightens this dir — both create
        // it under the default umask via `prepare_socket_dir` — so the listener
        // is the single choke point that locks it to 0o700, restricting the
        // socket to the operator's UID.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let listener = UnixListener::bind(sock_path)
        .with_context(|| format!("binding host.sock at {}", sock_path.display()))?;

    jackin_diagnostics::debug_log!(
        "exec_host",
        "listening at {} with {} allowed bindings",
        sock_path.display(),
        allowed_bindings.len()
    );

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                if let Err(e) = handle_connection(stream, allowed_bindings, caller_auth).await {
                    jackin_diagnostics::debug_log!("exec_host", "connection error: {e:#}");
                }
            }
            Err(e) => {
                jackin_diagnostics::debug_log!("exec_host", "accept error: {e:#}");
                // Brief back-off to avoid tight loop on persistent errors.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
) -> Result<()> {
    const MAX_REQ: usize = 512 * 1024;
    if let Err(error) = authenticate_caller(&stream, caller_auth) {
        jackin_diagnostics::debug_log!("exec_host", "rejected unauthenticated caller: {error:#}");
        return Ok(());
    }

    // Read 4-byte BE length + JSON body (same framing as control channel).
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    anyhow::ensure!(len <= MAX_REQ, "request too large: {len}");

    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let req: CredRequest = serde_json::from_slice(&body).context("parsing CredRequest")?;
    if matches!(
        jackin_telemetry::propagation::extract(&req.ctx),
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        stream
            .write_all(&frame(&CredReply::Error {
                error: "invalid correlation".to_owned(),
            }))
            .await?;
        return Ok(());
    }

    // Validate every requested ref against the operator-approved bindings.
    // Reject any ref that wasn't explicitly configured — this prevents a
    // compromised in-container process from escalating privileges by requesting
    // arbitrary op:// URIs or host env vars.
    for r in &req.refs {
        let approved = allowed_bindings
            .iter()
            .any(|b| b.name == r.name && b.kind == r.kind && b.source == r.source);
        if !approved {
            jackin_diagnostics::debug_log!(
                "exec_host",
                "rejected unauthorized ref: name={:?} kind={:?} source={:?}",
                r.name,
                r.kind,
                r.source
            );
            let reply = CredReply::Error {
                error: format!(
                    "credential {:?} is not in the approved binding set for this session",
                    r.name
                ),
            };
            stream.write_all(&frame(&reply)).await?;
            return Ok(());
        }
    }

    jackin_diagnostics::debug_log!(
        "exec_host",
        "resolving {} approved credential(s)",
        req.refs.len()
    );

    let reply = match resolve_all(&req.refs).await {
        Ok(values) => CredReply::Ok { values },
        Err(e) => CredReply::Error {
            error: format!("{e:#}"),
        },
    };
    // Reuse the canonical control-socket encoder so both ends of host.sock
    // frame identically.
    stream.write_all(&frame(&reply)).await?;
    Ok(())
}

fn authenticate_caller(stream: &UnixStream, caller_auth: CallerAuth) -> Result<()> {
    match caller_auth {
        CallerAuth::CapsuleDaemon => authenticate_capsule_daemon_peer(stream),
        #[cfg(all(test, target_os = "linux"))]
        CallerAuth::PeerPid(expected) => {
            let actual = peer_pid(stream)?;
            anyhow::ensure!(
                actual == expected,
                "peer pid {actual} did not match expected pid {expected}"
            );
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Result<u32> {
    let cred = stream.peer_cred().context("reading peer credentials")?;
    let pid = cred
        .pid()
        .ok_or_else(|| anyhow::anyhow!("peer credentials did not include a pid"))?;
    u32::try_from(pid).context("peer pid was negative")
}

#[cfg(target_os = "linux")]
fn authenticate_capsule_daemon_peer(stream: &UnixStream) -> Result<()> {
    let pid = peer_pid(stream)?;
    anyhow::ensure!(
        peer_is_container_init_process(pid)?,
        "peer pid {pid} is not the capsule daemon container init process"
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn authenticate_capsule_daemon_peer(_stream: &UnixStream) -> Result<()> {
    jackin_diagnostics::debug_log!(
        "exec_host",
        "peer credential daemon authentication unavailable on this host OS"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn peer_is_container_init_process(pid: u32) -> Result<bool> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .with_context(|| format!("reading /proc/{pid}/status"))?;
    Ok(peer_is_container_init_process_status(&status))
}

#[cfg(target_os = "linux")]
fn peer_is_container_init_process_status(status: &str) -> bool {
    status
        .lines()
        .find_map(|line| line.strip_prefix("NSpid:"))
        .and_then(|value| value.split_whitespace().last())
        == Some("1")
}

async fn resolve_all(refs: &[ExecBinding]) -> Result<std::collections::BTreeMap<String, String>> {
    // Resolve concurrently: each `op` kind spawns an `op read` subprocess
    // (network + a possible Touch ID prompt, ~1-3s each), so serial resolution
    // would make interactive `jackin-exec` latency scale with the number of
    // selected credentials. Mirrors the parallel launch-time resolver.
    let resolved = futures_util::future::try_join_all(refs.iter().map(|r| async move {
        let value = resolve_one(r)
            .await
            .with_context(|| format!("resolving credential {:?}", r.name))?;
        Ok::<_, anyhow::Error>((r.name.clone(), value))
    }))
    .await?;
    Ok(resolved.into_iter().collect())
}

fn validate_op_source(source: &str) -> Result<()> {
    anyhow::ensure!(
        source.starts_with("op://"),
        "invalid op:// reference {source:?}: must start with op://"
    );
    // Reject segments that look like CLI flags (start with -) to prevent arg injection.
    let path = &source["op://".len()..];
    anyhow::ensure!(
        !path.split('/').any(|s| s.starts_with('-')),
        "invalid op:// reference: segment looks like a flag in {source:?}"
    );
    Ok(())
}

async fn resolve_one(r: &ExecBinding) -> Result<String> {
    match r.kind {
        ExecKind::Op => {
            validate_op_source(&r.source).with_context(|| format!("credential {:?}", r.name))?;
            resolve_op(&r.source).await
        }
        ExecKind::Env => {
            // Reuse the canonical `$VAR` / `${VAR}` parser the binding collector
            // used to classify this source, so producer and consumer can't drift
            // on the host-ref grammar.
            let var_name = jackin_env::parse_host_ref(&r.source).ok_or_else(|| {
                anyhow::anyhow!(
                    "env credential {:?}: source {:?} is not a $VAR host reference",
                    r.name,
                    r.source
                )
            })?;
            std::env::var(var_name).with_context(|| format!("host env var {var_name:?} is not set"))
        }
        ExecKind::Literal => Ok(r.source.clone()),
    }
}

async fn resolve_op(op_ref: &str) -> Result<String> {
    // Insert -- end-of-options sentinel to prevent argument injection
    // via crafted op:// values containing flags. No timeout: Touch ID
    // prompts may block arbitrarily long (same semantic as pre-transport).
    let request = jackin_process::ExecRequest::new("op", ["read", "--", op_ref]).no_timeout();
    let output = jackin_process::exec_async(&request)
        .await
        .context("running `op read`")?;

    if output.success {
        let raw = String::from_utf8_lossy(&output.stdout);
        Ok(raw.trim_end_matches('\n').to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`op read` failed: {}", stderr.trim())
    }
}

#[cfg(test)]
mod tests;
