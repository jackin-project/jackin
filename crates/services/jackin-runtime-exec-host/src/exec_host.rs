// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, Result};
use jackin_protocol::control::frame;
use jackin_protocol::{CredReply, CredRequest, ExecBinding, ExecKind};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

#[expect(
    clippy::print_stderr,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn start(
    sock_path: PathBuf,
    allowed_bindings: Vec<ExecBinding>,
) -> tokio::task::JoinHandle<()> {
    jackin_telemetry::spawn::spawn_stream("exec_host.connection", async move {
        if run_listener(&sock_path, &allowed_bindings, CallerAuth::CapsuleDaemon)
            .await
            .is_err()
        {
            eprintln!("[jackin] warning: jackin-exec credential resolver unavailable");
        }
    })
}

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

#[expect(
    clippy::print_stderr,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn start_bound_for_container(
    jackin_home: &Path,
    container_name: &str,
    exec_bindings: &[ExecBinding],
) -> Result<tokio::task::JoinHandle<()>> {
    let sock_path = jackin_home
        .join("sockets")
        .join(container_name)
        .join("host.sock");
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let listener = match bind_listener(&sock_path) {
        Ok(listener) => listener,
        Err(error) => {
            jackin_telemetry::stream::complete_error(
                open,
                jackin_telemetry::schema::enums::ErrorType::IoError,
            );
            return Err(error);
        }
    };
    jackin_telemetry::stream::complete_success(open);
    let allowed_bindings = exec_bindings.to_vec();
    Ok(jackin_telemetry::spawn::spawn_stream(
        "exec_host.connection",
        async move {
            if run_bound_listener(listener, &allowed_bindings, CallerAuth::CapsuleDaemon)
                .await
                .is_err()
            {
                eprintln!("[jackin] warning: jackin-exec credential resolver unavailable");
            }
        },
    ))
}

#[derive(Clone, Copy, Debug)]
enum CallerAuth {
    CapsuleDaemon,
}

pub fn ensure_caller_auth_supported() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(
            "host credential relay is disabled on non-Linux hosts: capsule daemon peer authentication is unavailable"
        )
    }
}

async fn run_listener(
    sock_path: &Path,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
) -> Result<()> {
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let listener = match bind_listener(sock_path) {
        Ok(listener) => listener,
        Err(error) => {
            jackin_telemetry::stream::complete_error(
                open,
                jackin_telemetry::schema::enums::ErrorType::IoError,
            );
            return Err(error);
        }
    };
    jackin_telemetry::stream::complete_success(open);
    run_bound_listener(listener, allowed_bindings, caller_auth).await
}

async fn run_bound_listener(
    listener: UnixListener,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
) -> Result<()> {
    let _close = jackin_telemetry::stream::close_on_drop();

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            if handle_connection(stream, allowed_bindings, caller_auth)
                .await
                .is_err()
            {
                let _error = jackin_telemetry::record_error(
                    jackin_telemetry::schema::enums::ErrorType::RpcError,
                );
            }
        } else {
            let _error =
                jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
            let _retry = jackin_telemetry::record_retry_scheduled();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

fn bind_listener(sock_path: &Path) -> Result<UnixListener> {
    drop(std::fs::remove_file(sock_path));
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    UnixListener::bind(sock_path)
        .with_context(|| format!("binding host.sock at {}", sock_path.display()))
}

async fn handle_connection(
    mut stream: UnixStream,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
) -> Result<()> {
    const MAX_REQ: usize = 512 * 1024;
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str("jackin.host.Credentials/Resolve"),
        },
    ];
    if authenticate_caller(&stream, caller_auth).is_err() {
        complete_local_rpc_failure(&attrs);
        return Ok(());
    }

    let request = async {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        anyhow::ensure!(len <= MAX_REQ, "request too large: {len}");
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body).await?;
        serde_json::from_slice::<CredRequest>(&body).context("parsing CredRequest")
    }
    .await;
    let Ok(req) = request else {
        complete_local_rpc_failure(&attrs);
        return Ok(());
    };
    let extracted = jackin_telemetry::propagation::extract(&req.ctx);
    if matches!(
        extracted,
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        let operation =
            jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs).ok();
        record_rpc_error(operation.as_ref());
        let write_result = stream
            .write_all(&frame(&CredReply::Error {
                error: "invalid correlation".to_owned(),
            }))
            .await;
        if let Some(operation) = operation {
            operation.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
            );
        }
        drop(write_result);
        return Ok(());
    }
    let operation = match &extracted {
        jackin_telemetry::propagation::ExtractOutcome::Parent(parent) => {
            jackin_telemetry::operation_with_remote_parent(
                &jackin_telemetry::operation::RPC_SERVER,
                &attrs,
                parent,
            )
        }
        _ => jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs),
    }
    .ok();

    let mut approved_refs = Vec::with_capacity(req.refs.len());
    for requested in &req.refs {
        let approved = allowed_bindings
            .iter()
            .find(|allowed| binding_matches_request(allowed, requested));
        let Some(approved) = approved else {
            record_rpc_error(operation.as_ref());
            let reply = CredReply::Error {
                error: "credential reference is not approved".to_owned(),
            };
            let write_result = stream.write_all(&frame(&reply)).await;
            if let Some(operation) = operation {
                operation.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Failure,
                    Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
                );
            }
            drop(write_result);
            return Ok(());
        };
        approved_refs.push(approved.clone());
    }

    let reply = if let Ok(values) = resolve_all(&approved_refs).await {
        CredReply::Ok { values }
    } else {
        CredReply::Error {
            error: "credential resolution failed".to_owned(),
        }
    };
    let succeeded = matches!(&reply, CredReply::Ok { .. });
    let write_result = stream.write_all(&frame(&reply)).await;
    if !succeeded || write_result.is_err() {
        record_rpc_error(operation.as_ref());
    }
    if let Some(operation) = operation {
        operation.complete(
            if succeeded && write_result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            (!succeeded || write_result.is_err())
                .then_some(jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
    drop(write_result);
    Ok(())
}

fn binding_matches_request(allowed: &ExecBinding, requested: &ExecBinding) -> bool {
    if allowed.name != requested.name || allowed.kind != requested.kind {
        return false;
    }
    match allowed.kind {
        ExecKind::Op | ExecKind::Env => allowed.source == requested.source,
        ExecKind::Literal => true,
    }
}

fn complete_local_rpc_failure(attrs: &[jackin_telemetry::Attr<'_>]) {
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, attrs).ok();
    record_rpc_error(operation.as_ref());
    if let Some(operation) = operation {
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
}

fn record_rpc_error(operation: Option<&jackin_telemetry::OperationGuard>) {
    let record = || {
        let _error =
            jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::RpcError);
    };
    if let Some(operation) = operation {
        operation.span().in_scope(record);
    } else {
        record();
    }
}

fn authenticate_caller(stream: &UnixStream, caller_auth: CallerAuth) -> Result<()> {
    match caller_auth {
        CallerAuth::CapsuleDaemon => authenticate_capsule_daemon_peer(stream),
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
    ensure_caller_auth_supported()
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
        .is_some_and(|value| {
            let mut ids = value.split_whitespace();
            let host_pid = ids.next().and_then(|value| value.parse::<u32>().ok());
            let container_pid = ids.next();
            host_pid.is_some_and(|pid| pid > 0)
                && container_pid == Some("1")
                && ids.next().is_none()
        })
}

async fn resolve_all(refs: &[ExecBinding]) -> Result<std::collections::BTreeMap<String, String>> {
    let resolved = futures_util::future::try_join_all(refs.iter().map(|r| async move {
        let value = resolve_one(r)
            .await
            .context("resolving approved credential")?;
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
    let request = jackin_process::ExecRequest::new("op", ["read", "--", op_ref]).no_timeout();
    let output = jackin_runtime_process_telemetry::process_telemetry::exec_async(&request)
        .await
        .context("running credential process")?;

    if output.success {
        let raw = String::from_utf8_lossy(&output.stdout);
        Ok(raw.trim_end_matches('\n').to_owned())
    } else {
        anyhow::bail!("credential process exited unsuccessfully")
    }
}
