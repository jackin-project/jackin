// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::{BufRead as _, BufReader, Read as _};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use jackin_protocol::control::{
    AgentState, ClientMsg, ServerMsg, SessionEventKind, SessionEventRecord,
};

use jackin_core::{ContainerHandle, JackinPaths};

use jackin_runtime_snapshot::snapshot::{
    request_control_inner, run_docker_exec_capsule, socket_path,
};

const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlTransport {
    DirectSocket,
    DockerExecFallback,
}

pub fn send_session_text(
    paths: &JackinPaths,
    container: &ContainerHandle,
    session: u64,
    text: &str,
) -> Result<(u64, ControlTransport)> {
    let path = socket_path(paths, container.name());
    let request = ClientMsg::SessionSend {
        session,
        text: text.to_owned(),
    };
    let mut direct_error = None;
    if path.exists() {
        match request_control_inner(&path, &request).and_then(bytes_sent) {
            Ok(bytes) => return Ok((bytes, ControlTransport::DirectSocket)),
            Err(error) => direct_error = Some(error),
        }
    }
    send_session_text_via_docker_exec(container, session, text)
        .map(|bytes| (bytes, ControlTransport::DockerExecFallback))
        .map_err(|exec_error| match direct_error {
            Some(error) => exec_error.context(format!(
                "direct socket send to {} also failed: {error:#}",
                container.name()
            )),
            None => exec_error,
        })
}

fn bytes_sent(msg: ServerMsg) -> Result<u64> {
    match msg {
        ServerMsg::SessionSent { bytes, .. } => Ok(bytes),
        ServerMsg::SessionSendDenied { session, reason } => {
            bail!(
                "daemon refused the send to session {session}: {}",
                reason.label()
            )
        }
        ServerMsg::Unknown => bail!("daemon replied with an unknown ServerMsg variant"),
        other => bail!("daemon replied with {}; expected SessionSent", other.kind()),
    }
}

fn send_session_text_via_docker_exec(
    container: &ContainerHandle,
    session: u64,
    text: &str,
) -> Result<u64> {
    let output = run_docker_exec_capsule(container, &send_exec_script(session, text))?;
    if !output.status.success() {
        bail!(
            "docker exec send failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("send stdout is not UTF-8")?;
    #[derive(serde::Deserialize)]
    struct SendPayload {
        bytes: u64,
    }
    let payload: SendPayload =
        serde_json::from_str(stdout.trim()).context("parsing jackin-capsule send JSON")?;
    Ok(payload.bytes)
}

fn send_exec_script(session: u64, text: &str) -> String {
    format!(
        "exec /jackin/runtime/jackin-capsule send {session} {}",
        single_quote(text)
    )
}

fn ownership_exec_script(session: u64, action: &str) -> String {
    format!("exec /jackin/runtime/jackin-capsule {action} {session}")
}

#[derive(serde::Deserialize)]
struct OwnershipPayload {
    session: u64,
    operator_owned: bool,
}

fn ownership_confirmed(msg: ServerMsg, session: u64) -> Result<bool> {
    match msg {
        ServerMsg::Ack => Ok(true),
        ServerMsg::SessionSendDenied { reason, .. } => {
            bail!(
                "daemon refused operator ownership of session {session}: {}",
                reason.label()
            )
        }
        ServerMsg::Unknown => bail!("daemon replied with an unknown ServerMsg variant"),
        other => bail!(
            "daemon replied with {}; expected Ack for operator ownership",
            other.kind()
        ),
    }
}

/// Take exclusive operator input ownership of a session.
///
/// While ownership is held, automatic [`send_session_text`] input is rejected
/// by the daemon with [`SessionSendRejection::OperatorOwned`](jackin_protocol::control::SessionSendRejection::OperatorOwned).
/// Returns the owned flag the daemon confirmed alongside the transport used.
pub fn take_session_control(
    paths: &JackinPaths,
    container: &ContainerHandle,
    session: u64,
) -> Result<(bool, ControlTransport)> {
    ownership_roundtrip(paths, container, session, "take-control", true)
}

/// Release operator input ownership of a session, reopening automatic input.
pub fn release_session_control(
    paths: &JackinPaths,
    container: &ContainerHandle,
    session: u64,
) -> Result<(bool, ControlTransport)> {
    ownership_roundtrip(paths, container, session, "release-control", false)
}

fn ownership_roundtrip(
    paths: &JackinPaths,
    container: &ContainerHandle,
    session: u64,
    action: &str,
    expected_owned: bool,
) -> Result<(bool, ControlTransport)> {
    let path = socket_path(paths, container.name());
    let request = if action == "take-control" {
        ClientMsg::TakeControl { session }
    } else {
        ClientMsg::ReleaseControl { session }
    };
    let mut direct_error = None;
    if path.exists() {
        match request_control_inner(&path, &request)
            .and_then(|msg| ownership_confirmed(msg, session))
        {
            Ok(_) => return Ok((expected_owned, ControlTransport::DirectSocket)),
            Err(error) => direct_error = Some(error),
        }
    }
    ownership_via_docker_exec(container, session, action, expected_owned)
        .map(|owned| (owned, ControlTransport::DockerExecFallback))
        .map_err(|exec_error| match direct_error {
            Some(error) => exec_error.context(format!(
                "direct socket {action} to {} also failed: {error:#}",
                container.name()
            )),
            None => exec_error,
        })
}

fn ownership_via_docker_exec(
    container: &ContainerHandle,
    session: u64,
    action: &str,
    expected_owned: bool,
) -> Result<bool> {
    let output = run_docker_exec_capsule(container, &ownership_exec_script(session, action))?;
    if !output.status.success() {
        bail!(
            "docker exec {action} failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("{action} stdout is not UTF-8")?;
    let payload: OwnershipPayload =
        serde_json::from_str(stdout.trim()).context("parsing jackin-capsule ownership JSON")?;
    if payload.session != session || payload.operator_owned != expected_owned {
        bail!(
            "daemon confirmed unexpected ownership state for session {session}: {}",
            stdout.trim()
        );
    }
    Ok(payload.operator_owned)
}

fn single_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn events_exec_script(session: Option<u64>) -> String {
    match session {
        Some(session) => {
            format!("exec /jackin/runtime/jackin-capsule events --session {session}")
        }
        None => "exec /jackin/runtime/jackin-capsule events".to_owned(),
    }
}

pub struct SessionEvents {
    records: mpsc::Receiver<Result<SessionEventRecord>>,
    transport: ControlTransport,
    child: Option<std::process::Child>,
    operation: Option<jackin_runtime_process_telemetry::process_telemetry::ChildOperation>,
}

impl std::fmt::Debug for SessionEvents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionEvents")
            .field("transport", &self.transport)
            .finish_non_exhaustive()
    }
}

impl Drop for SessionEvents {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            drop(child.kill());
            drop(child.wait());
        }
        if let Some(operation) = self.operation.take() {
            operation.complete_success();
        }
    }
}

impl SessionEvents {
    pub fn subscribe(
        paths: &JackinPaths,
        container: &ContainerHandle,
        session: Option<u64>,
    ) -> Result<Self> {
        let path = socket_path(paths, container.name());
        let mut direct_error = None;
        if path.exists() {
            match Self::subscribe_direct(&path, session) {
                Ok(events) => return Ok(events),
                Err(error) => direct_error = Some(error),
            }
        }
        Self::subscribe_via_docker_exec(container, session).map_err(|exec_error| match direct_error
        {
            Some(error) => exec_error.context(format!(
                "direct socket subscribe to {} also failed: {error:#}",
                container.name()
            )),
            None => exec_error,
        })
    }

    #[must_use]
    pub const fn transport(&self) -> ControlTransport {
        self.transport
    }

    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<SessionEventRecord>> {
        match self.records.recv_timeout(timeout) {
            Ok(record) => record.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("capsule event stream ended")
            }
        }
    }

    pub fn wait_for(
        &mut self,
        deadline: Instant,
        mut predicate: impl FnMut(&SessionEventRecord) -> bool,
    ) -> Result<SessionEventRecord> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("timed out waiting for a matching capsule session event");
            }
            if let Some(record) = self.next_event(remaining)?
                && predicate(&record)
            {
                return Ok(record);
            }
        }
    }

    pub fn wait_for_state(
        &mut self,
        session: u64,
        state: AgentState,
        deadline: Instant,
    ) -> Result<SessionEventRecord> {
        let mut exited = None;
        let record = self.wait_for(deadline, |record| {
            if record.session != session {
                return false;
            }
            if let SessionEventKind::Exited { reason } = &record.kind {
                exited = Some(reason.clone());
                return true;
            }
            record.state == state
                && matches!(
                    record.kind,
                    SessionEventKind::StateChanged { .. } | SessionEventKind::Subscribed
                )
        })?;
        if let Some(reason) = exited {
            bail!(
                "session {session} exited before reaching {}{}",
                state.label(),
                reason.map_or_else(String::new, |reason| format!(": {reason}"))
            );
        }
        Ok(record)
    }

    fn subscribe_direct(path: &std::path::Path, session: Option<u64>) -> Result<Self> {
        let mut stream = jackin_diagnostics::operation::connection_attempt_sync(
            jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleControl,
            || std::os::unix::net::UnixStream::connect(path),
        )
        .with_context(|| format!("connecting to daemon socket {}", path.display()))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .context("setting write timeout")?;
        stream
            .set_read_timeout(Some(jackin_protocol::capsule_transport::HANDSHAKE_TIMEOUT))
            .context("setting handshake read timeout")?;
        jackin_protocol::capsule_transport::client_handshake(
            &mut stream,
            jackin_protocol::capsule_transport::HANDSHAKE_TIMEOUT,
        )
        .context("negotiating Capsule control transport")?;
        stream
            .set_read_timeout(None)
            .context("clearing handshake read timeout")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .context("restoring write timeout")?;
        write_control_request(&mut stream, &ClientMsg::Events { session })?;

        let (tx, rx) = mpsc::channel();
        jackin_telemetry::spawn::thread_stream_named(
            "jackin-capsule-events".to_owned(),
            move || read_framed_events(stream, &tx),
        )
        .context("spawning the capsule event reader")?;
        Ok(Self {
            records: rx,
            transport: ControlTransport::DirectSocket,
            child: None,
            operation: None,
        })
    }

    fn subscribe_via_docker_exec(
        container: &ContainerHandle,
        session: Option<u64>,
    ) -> Result<Self> {
        jackin_runtime_snapshot::snapshot::ensure_capsule_protocol_via_docker_exec(container)?;
        let args = docker_events_exec_args(container, session);
        let request = jackin_process::ExecRequest::new("docker", &args);
        let (operation, mut child) =
            jackin_runtime_process_telemetry::process_telemetry::spawn_sync(&request)
                .context("starting the docker exec event stream")?;
        let stdout = child
            .stdout
            .take()
            .context("docker exec event stream has no stdout")?;

        let (tx, rx) = mpsc::channel();
        jackin_telemetry::spawn::thread_stream_named(
            "jackin-capsule-events".to_owned(),
            move || read_ndjson_events(stdout, &tx),
        )
        .context("spawning the capsule event reader")?;
        Ok(Self {
            records: rx,
            transport: ControlTransport::DockerExecFallback,
            child: Some(child),
            operation: Some(operation),
        })
    }
}

fn docker_events_exec_args(container: &ContainerHandle, session: Option<u64>) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "--user".to_owned(),
        jackin_runtime_identity::identity::CAPSULE_SUPERVISOR_USER.to_owned(),
        container.id().to_owned(),
        "sh".to_owned(),
        "-lc".to_owned(),
        events_exec_script(session),
    ]
}

fn write_control_request(
    stream: &mut std::os::unix::net::UnixStream,
    request: &ClientMsg,
) -> Result<()> {
    use std::io::Write as _;
    let mut ctx = jackin_protocol::TelemetryContext::v1();
    jackin_telemetry::propagation::inject(&mut ctx);
    stream
        .write_all(&jackin_protocol::control::frame(
            &jackin_protocol::control::ControlRequest {
                ctx,
                session_capability: None,
                msg: request.clone(),
            },
        ))
        .context("writing control request to daemon")
}

fn read_framed_events(
    mut stream: std::os::unix::net::UnixStream,
    tx: &mpsc::Sender<Result<SessionEventRecord>>,
) {
    loop {
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(_) => return,
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_CONTROL_REPLY {
            drop(tx.send(Err(anyhow::anyhow!(
                "daemon event frame length {len} exceeds limit {MAX_CONTROL_REPLY}"
            ))));
            return;
        }
        let mut body = vec![0u8; len];
        if stream.read_exact(&mut body).is_err() {
            return;
        }
        let decoded = serde_json::from_slice::<ServerMsg>(&body)
            .context("parsing capsule event frame")
            .and_then(|msg| match msg {
                ServerMsg::SessionEvent { event } => Ok(*event),
                other => Err(anyhow::anyhow!(
                    "daemon sent {} on the event stream",
                    other.kind()
                )),
            });
        let failed = decoded.is_err();
        if tx.send(decoded).is_err() || failed {
            return;
        }
    }
}

fn read_ndjson_events(
    stdout: std::process::ChildStdout,
    tx: &mpsc::Sender<Result<SessionEventRecord>>,
) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let decoded = serde_json::from_str::<SessionEventRecord>(&line)
            .context("parsing capsule event NDJSON line");
        let failed = decoded.is_err();
        if tx.send(decoded).is_err() || failed {
            return;
        }
    }
}

#[must_use]
pub fn control_socket_path(paths: &JackinPaths, container_name: &str) -> PathBuf {
    socket_path(paths, container_name)
}

/// Mint a workflow command ID for session input correlation.
///
/// The Capsule `SessionSend`/`SessionSent` wire shape stays ID-less
/// for backward compatibility, so correlation lives host-side in the
/// workflow service: generate the ID BEFORE sending, commit the
/// command as `dispatching`, run the transport call
/// ([`send_session_text`]), then match the returned receipt to record
/// `written` and match later session events to record `turn_ended`.
/// A lost receipt leaves `dispatching`; restart recovery moves it to
/// `delivery_unknown` and never resubmits automatically.
#[must_use]
pub fn new_command_id() -> String {
    format!("cmd-{}", uuid::Uuid::new_v4().as_simple())
}

#[cfg(test)]
mod tests;
