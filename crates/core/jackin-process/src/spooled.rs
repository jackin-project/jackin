// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fmt;
use std::fs::File;
#[cfg(unix)]
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::ExecRequest;

#[cfg(unix)]
mod owned_group;
#[cfg(unix)]
mod pump;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpoolFailureKind {
    Request,

    Unsupported,

    Spawn,

    Execution,

    Capture,

    Cleanup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpoolFailure {
    pub kind: SpoolFailureKind,

    pub message: String,
}

impl fmt::Display for SpoolFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "qualification spool {:?}: {}",
            self.kind, self.message
        )
    }
}

impl Error for SpoolFailure {}

#[derive(Debug, Default)]
pub struct SpoolRecovery {
    #[cfg(unix)]
    children: Vec<owned_group::RecoveryChild>,
}

impl SpoolRecovery {
    #[cfg(unix)]
    pub fn poll_reap_nonblocking(&mut self) -> Vec<Result<bool, std::io::Error>> {
        self.children
            .iter_mut()
            .map(owned_group::RecoveryChild::poll_reap_nonblocking)
            .collect()
    }

    #[must_use]
    #[cfg(unix)]
    pub fn retained_children(&self) -> usize {
        self.children
            .iter()
            .filter(|child| child.retained())
            .count()
    }

    #[cfg(all(test, unix))]
    pub(super) fn terminate_children_for_test(&mut self) -> std::io::Result<()> {
        for child in &mut self.children {
            child.terminate_and_reap_for_test()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpoolExecutionObservation {
    pub timed_out: bool,

    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpoolPipeObservation {
    pub stdout_eof: bool,

    pub stderr_eof: bool,
}

impl SpoolPipeObservation {
    #[must_use]
    pub const fn both_reached_eof(self) -> bool {
        self.stdout_eof && self.stderr_eof
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpoolCaptureObservation {
    pub pipes: SpoolPipeObservation,

    pub complete: bool,
}

#[derive(Debug, Default)]
pub struct SpoolCleanupObservation {
    pub group_drained: bool,

    pub recovery: Option<SpoolRecovery>,
}

#[derive(Debug)]
pub struct SpoolExecResult {
    pub target_pid: u32,

    pub process_group_id: i32,

    pub code: Option<i32>,

    pub signal: Option<i32>,

    pub success: bool,

    pub execution: SpoolExecutionObservation,

    pub capture: SpoolCaptureObservation,

    pub cleanup: SpoolCleanupObservation,

    pub duration: Duration,

    pub stdout_bytes: u64,

    pub stderr_bytes: u64,

    pub failures: Vec<SpoolFailure>,

    pub stdout: File,

    pub stderr: File,
}

#[derive(Debug)]
pub struct SpoolError {
    pub kind: SpoolFailureKind,

    pub message: String,

    stdout: Option<File>,
    stderr: Option<File>,

    recovery: Option<SpoolRecovery>,

    pub failures: Vec<SpoolFailure>,

    pub stdout_bytes: u64,

    pub stderr_bytes: u64,
}

impl SpoolError {
    #[must_use]
    pub fn into_parts(self) -> (Option<File>, Option<File>, Option<SpoolRecovery>) {
        (self.stdout, self.stderr, self.recovery)
    }
}

impl fmt::Display for SpoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "qualification spool {:?}: {}",
            self.kind, self.message
        )?;
        for failure in &self.failures {
            write!(formatter, "; {failure}")?;
        }
        Ok(())
    }
}

impl Error for SpoolError {}

pub fn exec_spooled_sync_cancellable(
    request: &ExecRequest,
    stdout: File,
    stderr: File,
    cancellation: &AtomicBool,
) -> anyhow::Result<SpoolExecResult> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(error(
            SpoolFailureKind::Request,
            "strict spooled execution must run outside an active Tokio runtime",
            Some(stdout),
            Some(stderr),
            None,
            0,
            0,
        )
        .into());
    }

    if !super::supports_selective_reaping() {
        return Err(error(
            SpoolFailureKind::Unsupported,
            "strict spooled execution requires an approved selective child reaper",
            Some(stdout),
            Some(stderr),
            None,
            0,
            0,
        )
        .into());
    }

    if let Err(message) = validate_request(request) {
        return Err(error(
            SpoolFailureKind::Request,
            message,
            Some(stdout),
            Some(stderr),
            None,
            0,
            0,
        )
        .into());
    }

    #[cfg(unix)]
    {
        run_unix(request, stdout, stderr, cancellation)
    }
    #[cfg(not(unix))]
    {
        Err(error(
            SpoolFailureKind::Unsupported,
            "strict spooled execution requires Unix process-group support",
            Some(stdout),
            Some(stderr),
            None,
            0,
            0,
        )
        .into())
    }
}

fn validate_request(request: &ExecRequest) -> Result<(), String> {
    if request.stdout_mode != super::StdioMode::Capture
        || request.stderr_mode != super::StdioMode::Capture
    {
        return Err("spooled execution requires Capture stdout and stderr".to_owned());
    }
    if request.stdout_limit.is_none() || request.stderr_limit.is_none() {
        return Err("spooled execution requires explicit stdout and stderr budgets".to_owned());
    }
    if request.retry.max_retries > 0 {
        return Err("spooled execution does not accept retries".to_owned());
    }
    if request
        .stdin
        .as_ref()
        .is_some_and(|input| !input.is_empty())
    {
        return Err("spooled execution does not support nonempty stdin".to_owned());
    }
    if request.stdin.is_none()
        && !matches!(
            request.stdin_mode,
            super::StdioMode::Null | super::StdioMode::Inherit
        )
    {
        return Err("absent spooled stdin requires Null or Inherit routing".to_owned());
    }
    Ok(())
}

fn error(
    kind: SpoolFailureKind,
    message: impl Into<String>,
    stdout: Option<File>,
    stderr: Option<File>,
    recovery: Option<SpoolRecovery>,
    stdout_bytes: u64,
    stderr_bytes: u64,
) -> SpoolError {
    SpoolError {
        kind,
        message: message.into(),
        stdout,
        stderr,
        recovery,
        failures: Vec::new(),
        stdout_bytes,
        stderr_bytes,
    }
}

#[cfg(unix)]
fn run_unix(
    request: &ExecRequest,
    mut stdout: File,
    mut stderr: File,
    cancellation: &AtomicBool,
) -> anyhow::Result<SpoolExecResult> {
    use std::os::unix::process::ExitStatusExt;
    use std::time::Instant;

    let identities = match validate_spools(&mut stdout, &mut stderr) {
        Ok(identities) => identities,
        Err(message) => {
            return Err(error(
                SpoolFailureKind::Request,
                message,
                Some(stdout),
                Some(stderr),
                None,
                0,
                0,
            )
            .into());
        }
    };

    let started = Instant::now();
    let deadline = request
        .timeout
        .and_then(|timeout| started.checked_add(timeout));
    if request.timeout.is_some() && deadline.is_none() {
        return Err(error(
            SpoolFailureKind::Request,
            "request timeout exceeds monotonic clock range",
            Some(stdout),
            Some(stderr),
            None,
            0,
            0,
        )
        .into());
    }

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error_value) => {
            return Err(error(
                SpoolFailureKind::Unsupported,
                format!("building caller-thread I/O pump: {error_value}"),
                Some(stdout),
                Some(stderr),
                None,
                0,
                0,
            )
            .into());
        }
    };

    let mut command = std::process::Command::new(&request.program);
    super::configure_sync_command(&mut command, request);
    if request.stdin.is_some() {
        command.stdin(std::process::Stdio::piped());
    }
    let group = match owned_group::OwnedGroup::spawn(&mut command) {
        Ok(group) => group,
        Err(spawn_error) => {
            let kind = if spawn_error.unsupported {
                SpoolFailureKind::Unsupported
            } else {
                SpoolFailureKind::Spawn
            };
            let (recovery, failures) = if let Some(group) = spawn_error.group {
                let cleanup = group.cleanup_without_pump();
                (cleanup.recovery, cleanup.failures)
            } else {
                (None, Vec::new())
            };
            let mut spool_error = error(
                kind,
                spawn_error.message,
                Some(stdout),
                Some(stderr),
                recovery,
                0,
                0,
            );
            spool_error.failures = failures;
            return Err(spool_error.into());
        }
    };

    let target_pid = group.target_pid();
    let process_group_id = group.process_group_id();
    let pump = runtime.block_on(pump::supervise(
        group,
        &mut stdout,
        &mut stderr,
        request.stdout_limit.unwrap_or(0),
        request.stderr_limit.unwrap_or(0),
        deadline,
        cancellation,
    ));

    let mut failures = pump.failures;
    let (stdout_bytes, stderr_bytes) = finish_spools(
        &mut stdout,
        &mut stderr,
        identities,
        (pump.stdout_accepted, pump.stderr_accepted),
        (
            request.stdout_limit.unwrap_or(0),
            request.stderr_limit.unwrap_or(0),
        ),
        &mut failures,
    );
    let (code, signal) = pump
        .target_status
        .map_or((None, None), |status| (status.code(), status.signal()));
    let capture_failure = failures
        .iter()
        .any(|failure| failure.kind == SpoolFailureKind::Capture);
    let output_pipes_eof = pump.pipes.both_reached_eof();
    let capture_complete = !pump.execution.timed_out
        && !pump.execution.cancelled
        && pump.cleanup.group_drained
        && output_pipes_eof
        && !capture_failure;
    let success = code == Some(0)
        && signal.is_none()
        && !pump.execution.timed_out
        && !pump.execution.cancelled
        && capture_complete
        && failures.is_empty();

    Ok(SpoolExecResult {
        target_pid,
        process_group_id,
        code,
        signal,
        success,
        execution: pump.execution,
        capture: SpoolCaptureObservation {
            pipes: pump.pipes,
            complete: capture_complete,
        },
        cleanup: pump.cleanup,
        duration: started.elapsed(),
        stdout_bytes,
        stderr_bytes,
        failures,
        stdout,
        stderr,
    })
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpoolIdentity {
    device: u64,
    inode: u64,
    uid: u32,
    links: u64,
    mode: u32,
}

#[cfg(unix)]
fn validate_spools(
    stdout: &mut File,
    stderr: &mut File,
) -> Result<(SpoolIdentity, SpoolIdentity), String> {
    use std::io::Seek;

    let stdout_identity = validate_spool(stdout, "stdout")?;
    let stderr_identity = validate_spool(stderr, "stderr")?;
    if stdout_identity.device == stderr_identity.device
        && stdout_identity.inode == stderr_identity.inode
    {
        return Err("stdout and stderr spools must be distinct files".to_owned());
    }
    if stdout
        .stream_position()
        .map_err(|error| format!("reading stdout position: {error}"))?
        != 0
        || stderr
            .stream_position()
            .map_err(|error| format!("reading stderr position: {error}"))?
            != 0
    {
        return Err("spool descriptors must start at byte zero".to_owned());
    }
    Ok((stdout_identity, stderr_identity))
}

#[cfg(unix)]
fn validate_spool(file: &File, name: &str) -> Result<SpoolIdentity, String> {
    use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
    use std::os::unix::fs::MetadataExt;

    let metadata = file
        .metadata()
        .map_err(|error| format!("reading {name} spool metadata: {error}"))?;
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    if !metadata.is_file() {
        return Err(format!("{name} spool is not a regular file"));
    }
    if metadata.uid() != effective_uid {
        return Err(format!("{name} spool is not owned by the effective UID"));
    }
    if metadata.mode() & 0o7777 != 0o600 {
        return Err(format!("{name} spool mode must be exactly 0600"));
    }
    if metadata.nlink() != 1 {
        return Err(format!("{name} spool must have one hard link"));
    }
    if metadata.len() != 0 {
        return Err(format!("{name} spool must be empty"));
    }
    let raw_flags = fcntl(file, FcntlArg::F_GETFL)
        .map_err(|error| format!("reading {name} spool status flags: {error}"))?;
    let flags = OFlag::from_bits_truncate(raw_flags);
    if flags & OFlag::O_ACCMODE == OFlag::O_RDONLY {
        return Err(format!("{name} spool descriptor is read-only"));
    }
    if flags.contains(OFlag::O_APPEND) {
        return Err(format!("{name} spool descriptor uses append mode"));
    }
    let raw_descriptor_flags = fcntl(file, FcntlArg::F_GETFD)
        .map_err(|error| format!("reading {name} spool descriptor flags: {error}"))?;
    let descriptor_flags = FdFlag::from_bits_truncate(raw_descriptor_flags);
    if !descriptor_flags.contains(FdFlag::FD_CLOEXEC) {
        return Err(format!("{name} spool descriptor must have FD_CLOEXEC"));
    }
    Ok(SpoolIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        uid: metadata.uid(),
        links: metadata.nlink(),
        mode: metadata.mode() & 0o7777,
    })
}

#[cfg(unix)]
fn checked_spool_length(
    file: &impl OutputSpool,
    expected: SpoolIdentity,
    name: &str,
    failures: &mut Vec<SpoolFailure>,
) -> u64 {
    match file.spool_metadata() {
        Ok(metadata) => {
            if !metadata.regular_file || metadata.identity != expected {
                failures.push(SpoolFailure {
                    kind: SpoolFailureKind::Capture,
                    message: format!("{name} spool identity changed during execution"),
                });
            }
            metadata.length
        }
        Err(error_value) => {
            failures.push(SpoolFailure {
                kind: SpoolFailureKind::Capture,
                message: format!("reading {name} spool metadata: {error_value}"),
            });
            0
        }
    }
}

#[cfg(unix)]
trait OutputSpool: Write {
    fn sync_spool(&mut self) -> std::io::Result<()>;
    fn spool_metadata(&self) -> std::io::Result<SpoolMetadata>;
}

#[cfg(unix)]
impl OutputSpool for File {
    fn sync_spool(&mut self) -> std::io::Result<()> {
        self.sync_all()
    }

    fn spool_metadata(&self) -> std::io::Result<SpoolMetadata> {
        use std::os::unix::fs::MetadataExt as _;

        let metadata = self.metadata()?;
        Ok(SpoolMetadata {
            regular_file: metadata.is_file(),
            identity: SpoolIdentity {
                device: metadata.dev(),
                inode: metadata.ino(),
                uid: metadata.uid(),
                links: metadata.nlink(),
                mode: metadata.mode() & 0o7777,
            },
            length: metadata.len(),
        })
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
struct SpoolMetadata {
    regular_file: bool,
    identity: SpoolIdentity,
    length: u64,
}

#[cfg(unix)]
fn finish_spools<Stdout: OutputSpool, Stderr: OutputSpool>(
    stdout: &mut Stdout,
    stderr: &mut Stderr,
    identities: (SpoolIdentity, SpoolIdentity),
    accepted: (usize, usize),
    limits: (usize, usize),
    failures: &mut Vec<SpoolFailure>,
) -> (u64, u64) {
    flush_spool("stdout", stdout, failures);
    flush_spool("stderr", stderr, failures);
    sync_spool("stdout", stdout, failures);
    sync_spool("stderr", stderr, failures);

    let stdout_bytes = checked_spool_length(stdout, identities.0, "stdout", failures);
    let stderr_bytes = checked_spool_length(stderr, identities.1, "stderr", failures);
    for (name, accepted, length, limit) in [
        ("stdout", accepted.0, stdout_bytes, limits.0),
        ("stderr", accepted.1, stderr_bytes, limits.1),
    ] {
        if !u64::try_from(accepted).is_ok_and(|accepted| accepted == length)
            || length > u64::try_from(limit).unwrap_or(u64::MAX)
        {
            failures.push(SpoolFailure {
                kind: SpoolFailureKind::Capture,
                message: format!("{name} spool byte accounting does not match accepted output"),
            });
        }
    }
    (stdout_bytes, stderr_bytes)
}

#[cfg(unix)]
fn flush_spool(spool_name: &str, spool: &mut impl OutputSpool, failures: &mut Vec<SpoolFailure>) {
    if let Err(error) = spool.flush() {
        failures.push(SpoolFailure {
            kind: SpoolFailureKind::Capture,
            message: format!("flushing {spool_name} spool: {error}"),
        });
    }
}

#[cfg(unix)]
fn sync_spool(spool_name: &str, spool: &mut impl OutputSpool, failures: &mut Vec<SpoolFailure>) {
    if let Err(error) = spool.sync_spool() {
        failures.push(SpoolFailure {
            kind: SpoolFailureKind::Capture,
            message: format!("syncing {spool_name} spool: {error}"),
        });
    }
}

#[cfg(test)]
mod tests;
