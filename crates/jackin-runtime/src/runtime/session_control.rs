// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side client for the capsule's `session.send` and `events` control
//! surface: type text into a running agent session, and watch that session's
//! state transitions, exits, and activity.
//!
//! Not responsible for: deciding what an agent is doing. The capsule's
//! terminal-observation arbitration is the sole authority for agent state;
//! this module transports its verdicts to the host and never forms its own.
//!
//! Two transports, chosen exactly as `runtime::snapshot` chooses them. A
//! same-kernel Docker host reads the bind-mounted Unix socket directly and
//! speaks the framed control protocol. Docker Desktop on macOS exposes the
//! socket inode but cannot bridge the live socket across the VM boundary, so
//! the host falls back to `docker exec … jackin-capsule send` / `… events`,
//! whose NDJSON output carries the same `jackin_protocol` types. Both paths
//! decode into the same records, so a caller never branches on transport.
//!
//! Both transports deliver records through one background reader thread and a
//! channel, so `next_event` has a real timeout regardless of which one is in
//! use — a blocking `read` on a socket and a blocking `read_line` on a pipe do
//! not otherwise share a cancellation story.

use std::io::{BufRead as _, BufReader, Read as _};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use jackin_protocol::control::{
    AgentState, ClientMsg, ServerMsg, SessionEventKind, SessionEventRecord,
};

use jackin_core::JackinPaths;

use super::snapshot::{request_control_inner, run_docker_exec_capsule, socket_path};

/// Cap on one framed reply read from the daemon socket. Mirrors the daemon's
/// own frame cap so a legitimate event never trips it.
const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;

/// Which transport carried a control call. Callers that report to an operator
/// surface this so a failure names the path it actually took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlTransport {
    /// The bind-mounted Unix socket, speaking the framed control protocol.
    DirectSocket,
    /// `docker exec … jackin-capsule`, speaking NDJSON.
    DockerExecFallback,
}

/// Write `text` into a running session's PTY, verbatim.
///
/// Nothing is appended: a caller that wants the agent to *submit* the text
/// includes the submit key itself (`"\r"`). Returns the byte count the daemon
/// reports it wrote, plus the transport that carried the call.
///
/// # Errors
///
/// Returns an error when neither transport can reach the daemon, when the
/// session id is unknown to the container, or when the session's PTY writer
/// has already exited — a refused send is an error, never a silent no-op.
pub fn send_session_text(
    paths: &JackinPaths,
    container_name: &str,
    session: u64,
    text: &str,
) -> Result<(u64, ControlTransport)> {
    let path = socket_path(paths, container_name);
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
    send_session_text_via_docker_exec(container_name, session, text)
        .map(|bytes| (bytes, ControlTransport::DockerExecFallback))
        .map_err(|exec_error| match direct_error {
            Some(error) => exec_error.context(format!(
                "direct socket send to {container_name} also failed: {error:#}"
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
        // `Unknown` is the `#[serde(other)]` sink for a newer daemon.
        ServerMsg::Unknown => bail!("daemon replied with an unknown ServerMsg variant"),
        other => bail!("daemon replied with {}; expected SessionSent", other.kind()),
    }
}

fn send_session_text_via_docker_exec(
    container_name: &str,
    session: u64,
    text: &str,
) -> Result<u64> {
    let output = run_docker_exec_capsule(container_name, &send_exec_script(session, text))?;
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

/// Build the in-container `send` command line.
///
/// The payload is passed as one single-quoted `sh` word so a newline, a space,
/// or a `$` in the prompt reaches the PTY unchanged instead of being re-split
/// or expanded by the shell that `docker exec` starts.
fn send_exec_script(session: u64, text: &str) -> String {
    format!(
        "exec /jackin/runtime/jackin-capsule send {session} {}",
        single_quote(text)
    )
}

/// POSIX single-quote one shell word: wrap in `'…'` and replace each embedded
/// quote with `'\''`. There is no other metacharacter inside single quotes, so
/// this is total — every byte survives.
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

/// A live subscription to one container's session event stream.
///
/// Dropping it ends the subscription: the reader thread's channel closes, and
/// the transport (socket or `docker exec` child) is torn down with it.
pub struct SessionEvents {
    records: mpsc::Receiver<Result<SessionEventRecord>>,
    transport: ControlTransport,
    child: Option<std::process::Child>,
    /// Span over the `docker exec` child's whole life. Held until the
    /// subscription drops, because the subscription *is* the child's lifetime.
    operation: Option<crate::process_telemetry::ChildOperation>,
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
        // The `docker exec` child holds a pipe open on the host and a control
        // connection open in the container; neither ends on its own, so the
        // subscription's lifetime has to actually kill it.
        if let Some(child) = self.child.as_mut() {
            drop(child.kill());
            drop(child.wait());
        }
        // Terminating the reader when the caller is done with it is the normal
        // end of a subscription, not a failure — the reaped signal status is
        // ours, so it must not be read as the child's verdict.
        if let Some(operation) = self.operation.take() {
            operation.complete_success();
        }
    }
}

impl SessionEvents {
    /// Open a subscription. `session` restricts the stream to one session;
    /// `None` streams every session in the container.
    ///
    /// The daemon sends one `Subscribed` baseline record per matching live
    /// session before anything else, so the first reads describe the container
    /// as it stands rather than making the caller wait for a change.
    ///
    /// # Errors
    ///
    /// Returns an error when neither transport can open the stream.
    pub fn subscribe(
        paths: &JackinPaths,
        container_name: &str,
        session: Option<u64>,
    ) -> Result<Self> {
        let path = socket_path(paths, container_name);
        let mut direct_error = None;
        if path.exists() {
            match Self::subscribe_direct(&path, session) {
                Ok(events) => return Ok(events),
                Err(error) => direct_error = Some(error),
            }
        }
        Self::subscribe_via_docker_exec(container_name, session).map_err(|exec_error| {
            match direct_error {
                Some(error) => exec_error.context(format!(
                    "direct socket subscribe to {container_name} also failed: {error:#}"
                )),
                None => exec_error,
            }
        })
    }

    /// Which transport this subscription is reading.
    #[must_use]
    pub const fn transport(&self) -> ControlTransport {
        self.transport
    }

    /// Read the next record, waiting at most `timeout`.
    ///
    /// `Ok(None)` means the timeout elapsed with the stream still open — a
    /// quiet container, not a failure. An ended stream (daemon shut down, the
    /// `docker exec` child died) is an error, because a caller waiting for a
    /// transition must never mistake "the stream is gone" for "nothing
    /// happened yet".
    ///
    /// # Errors
    ///
    /// Returns an error when a record fails to decode or the stream has ended.
    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<SessionEventRecord>> {
        match self.records.recv_timeout(timeout) {
            Ok(record) => record.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("capsule event stream ended")
            }
        }
    }

    /// Read records until one satisfies `predicate` or `deadline` passes.
    ///
    /// # Errors
    ///
    /// Returns an error when the deadline passes without a match, or when the
    /// stream ends first.
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

    /// Wait for `session` to enter `state`.
    ///
    /// Matches the transition record *and* a baseline record that already
    /// reports the state: an agent that started working before the subscriber
    /// finished connecting has still entered the state, and a caller that only
    /// accepted transitions would hang on that race forever.
    ///
    /// # Errors
    ///
    /// Returns an error when the deadline passes first, when the stream ends,
    /// or when the session exits without ever reaching `state`.
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
        // No read timeout: the whole point of the subscription is to block
        // until the daemon has something to say. The caller's timeout lives on
        // `next_event`, and dropping the subscription closes the socket, which
        // unblocks this thread's read.
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .context("setting write timeout")?;
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

    fn subscribe_via_docker_exec(container_name: &str, session: Option<u64>) -> Result<Self> {
        let run_as_user = crate::runtime::identity::CAPSULE_SUPERVISOR_USER;
        let mut args: Vec<String> = vec!["exec".to_owned()];
        args.push("--user".to_owned());
        args.push(run_as_user.to_owned());
        args.push(container_name.to_owned());
        args.push("sh".to_owned());
        args.push("-lc".to_owned());
        args.push(events_exec_script(session));
        let request = jackin_process::ExecRequest::new("docker", &args);
        let (operation, mut child) = crate::process_telemetry::spawn_sync(&request)
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

/// Reader thread for the direct-socket transport: length-prefixed JSON frames
/// until the daemon closes the connection or the subscription is dropped.
fn read_framed_events(
    mut stream: std::os::unix::net::UnixStream,
    tx: &mpsc::Sender<Result<SessionEventRecord>>,
) {
    loop {
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => {}
            // Clean end of stream, or the subscription was dropped and closed
            // the socket under us. Either way there is nothing to report.
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

/// Reader thread for the `docker exec` transport: one JSON record per line.
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

/// Host-side path of a container's daemon socket. Re-exported so callers of
/// this module do not have to reach into `snapshot` for it.
#[must_use]
pub fn control_socket_path(paths: &JackinPaths, container_name: &str) -> PathBuf {
    socket_path(paths, container_name)
}

#[cfg(test)]
mod tests;
