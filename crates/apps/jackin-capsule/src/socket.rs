// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::container_paths;

pub const SOCKET_PATH: &str = container_paths::CAPSULE_SOCKET;

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, mpsc};

use crate::protocol::control::{ServerMsg, frame};

type ClientPermit = tokio::sync::OwnedSemaphorePermit;
type ListenerReceiver = mpsc::UnboundedReceiver<(UnixStream, ClientPermit)>;
type ListenerWithLimiter = (ListenerReceiver, Arc<Semaphore>);

const ACCEPT_FAILURE_BAIL: u32 = 10;

const MAX_CONCURRENT_CLIENTS: usize = 16;

pub(crate) fn start_listener_at(path: &Path) -> Result<ListenerReceiver> {
    start_listener_at_with_limiter(path).map(|(rx, _limiter)| rx)
}

fn start_listener_at_with_limiter(path: &Path) -> Result<ListenerWithLimiter> {
    start_listener_at_inner(path)
}

fn start_listener_at_inner(path: &Path) -> Result<ListenerWithLimiter> {
    let operation =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let result = start_listener_at_inner_uninstrumented(path);
    if result.is_err() {
        let _error_event =
            jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
    }
    if result.is_ok() {
        jackin_telemetry::stream::complete_success(operation);
    } else {
        jackin_telemetry::stream::complete_error(
            operation,
            jackin_telemetry::schema::enums::ErrorType::IoError,
        );
    }
    result
}

fn start_listener_at_inner_uninstrumented(path: &Path) -> Result<ListenerWithLimiter> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("removing stale socket {}", path.display()));
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("locking socket parent {} to 0o700", parent.display()))?;
    }

    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("locking socket {} to 0o600", path.display()))?;
    let (tx, rx) = mpsc::unbounded_channel();
    let limiter = Arc::new(Semaphore::new(MAX_CONCURRENT_CLIENTS));
    let limiter_for_task = Arc::clone(&limiter);

    jackin_telemetry::spawn::spawn_stream("capsule.socket.accept", async move {
        let limiter = limiter_for_task;
        let mut consecutive_failures = 0u32;
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    consecutive_failures = 0;
                    let Ok(permit) = Arc::clone(&limiter).try_acquire_owned() else {
                        drop(stream);
                        continue;
                    };
                    if tx.send((stream, permit)).is_err() {
                        jackin_telemetry::stream::complete_success(
                            jackin_telemetry::stream::phase(
                                jackin_telemetry::schema::enums::StreamOperation::Close,
                            ),
                        );
                        return;
                    }
                }
                Err(_error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    let _error_event = jackin_telemetry::record_error(
                        jackin_telemetry::schema::enums::ErrorType::IoError,
                    );
                    if consecutive_failures >= ACCEPT_FAILURE_BAIL {
                        jackin_telemetry::stream::complete_error(
                            jackin_telemetry::stream::phase(
                                jackin_telemetry::schema::enums::StreamOperation::Close,
                            ),
                            jackin_telemetry::schema::enums::ErrorType::IoError,
                        );
                        return;
                    }
                    let _retry_event = jackin_telemetry::record_retry_scheduled();
                    let shift = consecutive_failures.saturating_sub(1).min(16);
                    let backoff_ms = 50u64.saturating_mul(1u64 << shift).min(5_000);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    });

    Ok((rx, limiter))
}

const CONTROL_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn read_control_msg(
    stream: &mut UnixStream,
    first_byte: u8,
) -> Result<jackin_protocol::control::ControlRequest> {
    let deadline = tokio::time::Instant::now() + CONTROL_READ_TIMEOUT;
    let mut rest = [0u8; 3];
    tokio::time::timeout_at(deadline, stream.read_exact(&mut rest))
        .await
        .context("control msg: timed out reading length suffix")?
        .context("control msg: reading length suffix")?;
    let len_buf = [first_byte, rest[0], rest[1], rest[2]];
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_CONTROL_MSG: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_MSG {
        anyhow::bail!("control msg length {len} exceeds limit {MAX_CONTROL_MSG}");
    }
    let body = read_payload_lazy(stream, len, deadline)
        .await
        .context("control msg: reading body")?;
    serde_json::from_slice(&body).context("control msg: parsing JSON body")
}

async fn read_payload_lazy(
    stream: &mut UnixStream,
    len: usize,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::with_capacity(len.min(64 * 1024));
    let mut remaining = len;
    let mut chunk = [0u8; 16 * 1024];
    while remaining > 0 {
        let n = chunk.len().min(remaining);
        tokio::time::timeout_at(deadline, stream.read_exact(&mut chunk[..n]))
            .await
            .context("read timed out")?
            .context("short read")?;
        buf.extend_from_slice(&chunk[..n]);
        remaining -= n;
    }
    Ok(buf)
}

pub async fn write_control_reply(mut stream: UnixStream, reply: &ServerMsg) -> Result<()> {
    write_control_frame(&mut stream, reply).await
}

pub async fn write_control_frame(stream: &mut UnixStream, reply: &ServerMsg) -> Result<()> {
    match tokio::time::timeout(Duration::from_secs(2), stream.write_all(&frame(reply))).await {
        Ok(result) => result.context("control reply write failed"),
        Err(_) => anyhow::bail!("control reply write timed out after 2 s"),
    }
}
