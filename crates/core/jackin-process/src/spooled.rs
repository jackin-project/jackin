// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded strict execution that writes captured output to caller-owned files.

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
#[cfg(test)]
mod tests;

/// Failure stage for strict spooled execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpoolFailureKind {
    /// Request or descriptor contract rejected before spawn.
    Request,
    /// Target platform cannot provide an approved child-selection route.
    Unsupported,
    /// Anchor or requested command could not start.
    Spawn,
    /// Child status observation failed.
    Execution,
    /// Pipe, file write, sync, EOF, budget, or byte-accounting failure.
    Capture,
    /// Group signaling, child wait, or group-drain failure.
    Cleanup,
}

/// One classified post-spawn execution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpoolFailure {
    /// Failure category.
    pub kind: SpoolFailureKind,
    /// Human-readable detail.
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

/// Opaque, finite custody for direct children whose wait outcome is uncertain.
///
/// This object never signals by PID or process group. Drop closes owned pipe
/// descriptors and releases reservations to an installed PID 1 reaper. Without
/// that reaper, the kernel may retain the child until process exit.
#[derive(Debug, Default)]
pub struct SpoolRecovery {
    #[cfg(unix)]
    children: Vec<owned_group::RecoveryChild>,
}

impl SpoolRecovery {
    /// Make one nonblocking wait attempt for each retained direct child.
    ///
    /// A successful poll consumes that child's status and releases only its
    /// reservation. This method never signals a child or process group.
    #[cfg(unix)]
    pub fn poll_reap_nonblocking(&mut self) -> Vec<Result<bool, std::io::Error>> {
        self.children
            .iter_mut()
            .map(owned_group::RecoveryChild::poll_reap_nonblocking)
            .collect()
    }

    /// Number of direct children whose custody remains in this object.
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

/// Independent request observations made by the strict supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpoolExecutionObservation {
    /// Request deadline expired before exit observation.
    pub timed_out: bool,
    /// Cancellation was observed before return.
    pub cancelled: bool,
}

/// Independent EOF observations for the two captured output pipes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpoolPipeObservation {
    /// Standard output reached EOF.
    pub stdout_eof: bool,
    /// Standard error reached EOF.
    pub stderr_eof: bool,
}

impl SpoolPipeObservation {
    /// Whether both captured output pipes reached EOF.
    #[must_use]
    pub const fn both_reached_eof(self) -> bool {
        self.stdout_eof && self.stderr_eof
    }
}

/// Capture and byte-accounting facts from one strict spooled run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpoolCaptureObservation {
    /// Per-pipe EOF facts, kept independently for diagnostics.
    pub pipes: SpoolPipeObservation,
    /// The run reached both EOFs without timeout, cancellation, or capture errors and drained its group.
    pub complete: bool,
}

/// Process-group drain and direct-child recovery custody facts.
#[derive(Debug, Default)]
pub struct SpoolCleanupObservation {
    /// Both direct children were reaped and the original group was absent.
    pub group_drained: bool,
    /// Direct-child custody retained after finite uncertain cleanup.
    pub recovery: Option<SpoolRecovery>,
}

/// Result of one strict spooled process run.
#[derive(Debug)]
pub struct SpoolExecResult {
    /// Requested command PID.
    pub target_pid: u32,
    /// Original private process-group ID.
    pub process_group_id: i32,
    /// Normal exit code, if observed.
    pub code: Option<i32>,
    /// Terminating signal, if observed.
    pub signal: Option<i32>,
    /// True only for exit code zero and complete execution, capture, and cleanup.
    pub success: bool,
    /// Independent request deadline and cancellation observations.
    pub execution: SpoolExecutionObservation,
    /// Independent output EOF and capture-completion observations.
    pub capture: SpoolCaptureObservation,
    /// Group drain and uncertain direct-child recovery custody.
    pub cleanup: SpoolCleanupObservation,
    /// Elapsed time including spawn and cleanup.
    pub duration: Duration,
    /// Accepted stdout bytes written to the spool.
    pub stdout_bytes: u64,
    /// Accepted stderr bytes written to the spool.
    pub stderr_bytes: u64,
    /// Classified failures observed after target spawn.
    pub failures: Vec<SpoolFailure>,
    /// Original caller-owned stdout spool.
    pub stdout: File,
    /// Original caller-owned stderr spool.
    pub stderr: File,
}

/// Typed error that retains consumed files and any post-anchor child custody.
#[derive(Debug)]
pub struct SpoolError {
    /// Error category.
    pub kind: SpoolFailureKind,
    /// Error detail.
    pub message: String,
    /// Caller output files, when the operation did not return a result.
    stdout: Option<File>,
    stderr: Option<File>,
    /// Child custody retained after an anchor or target spawn failure.
    recovery: Option<SpoolRecovery>,
    /// Classified cleanup failures after a child was spawned.
    pub failures: Vec<SpoolFailure>,
    /// Bytes already accepted into the output files.
    pub stdout_bytes: u64,
    /// Bytes already accepted into the output files.
    pub stderr_bytes: u64,
}

impl SpoolError {
    /// Recover the original output handles and any uncertain child custody.
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

/// Run one command with strict file-backed output and finite cleanup.
///
/// Both output limits must be explicit and are independent spool budgets.
/// Files must be distinct, empty, regular, effective-UID-owned mode `0600`
/// descriptors opened for writing at byte zero, with one hard link and no
/// append mode. The supervisor checks descriptor metadata and access flags;
/// it does not validate ACLs or prove that another same-UID process lacks a
/// writable descriptor. The caller must create these files without following
/// symlinks and keep them exclusive.
///
/// The child and a held `cat` anchor share a private process group. The anchor
/// remains unreaped through the last destructive group signal. Cleanup uses a
/// single four-second monotonic deadline after finalization begins. That
/// bounds supervisor polling, but cannot preempt spawn, mutex acquisition,
/// blocking regular-file operations, or kernel stalls. Uncertain direct-child
/// custody is returned in `SpoolRecovery`; it never performs a delayed numeric
/// PID or process-group signal.
///
/// Calls inside a Tokio runtime, requests with nonempty stdin, retries, missing
/// output budgets, and platforms without approved selective reaping are
/// rejected before the anchor starts. Empty explicit stdin means immediate
/// EOF. Absent stdin may use Null or Inherit. The current-thread I/O pump
/// drains both output pipes while cleanup polls, and files are flushed and
/// synced before return. The caller must sync again before recording a receipt.
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
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
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
