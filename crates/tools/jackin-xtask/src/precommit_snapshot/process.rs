// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Output},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use jackin_process::{
    ExecRequest, SpoolError, SpoolExecResult, SpoolFailureKind, exec_spooled_sync_cancellable,
    exec_spooled_sync_cancellable_with_stdin_file,
};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::{Mode, fchmod},
};
use tempfile::TempDir;

mod temporary_parent;
pub(super) use temporary_parent::canonical_temporary_parent;

const PROBE_LIMIT: Duration = Duration::from_secs(120);
pub(super) const METADATA_OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EnvironmentMode {
    Inherit,
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommandBudget {
    Probe,
    PhaseRemainder,
}

#[derive(Clone, Debug)]
pub(super) struct SetupControls {
    cancellation: Arc<AtomicBool>,
    phase_deadline: Instant,
}

impl SetupControls {
    pub(super) fn new(cancellation: Arc<AtomicBool>, phase_budget: Duration) -> Self {
        Self {
            cancellation,
            phase_deadline: Instant::now() + phase_budget,
        }
    }

    pub(in crate::precommit_snapshot) fn cancellation(&self) -> &AtomicBool {
        &self.cancellation
    }

    pub(super) fn check_start(&self) -> Result<()> {
        if self.cancellation.load(Ordering::SeqCst) {
            return Err(CaptureFailure::new(
                CaptureFailureKind::Cancelled,
                "native hook setup was cancelled before child spawn",
                None,
            )
            .into());
        }
        let remaining = self
            .phase_deadline
            .saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero(),
            CaptureFailure::new(
                CaptureFailureKind::Deadline,
                "native hook setup phase deadline expired before child spawn",
                None,
            )
        );
        Ok(())
    }

    pub(in crate::precommit_snapshot) fn child_timeout(
        &self,
        budget: CommandBudget,
    ) -> Result<Duration> {
        self.check_start()?;
        let remaining = self
            .phase_deadline
            .saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero(),
            CaptureFailure::new(
                CaptureFailureKind::Deadline,
                "native hook setup phase deadline expired before child spawn",
                None,
            )
        );
        Ok(match budget {
            CommandBudget::Probe => remaining.min(PROBE_LIMIT),
            CommandBudget::PhaseRemainder => remaining,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CaptureFailureKind {
    Cancelled,
    Deadline,
    Timeout,
    UncertainCleanup,
    Capture,
}

#[derive(Debug)]
pub(super) struct CaptureFailure {
    pub(super) kind: CaptureFailureKind,
    detail: String,
    diagnostics: Option<PathBuf>,
}

impl CaptureFailure {
    fn new(
        kind: CaptureFailureKind,
        detail: impl Into<String>,
        diagnostics: Option<PathBuf>,
    ) -> Self {
        Self {
            kind,
            detail: detail.into(),
            diagnostics,
        }
    }
}

impl std::fmt::Display for CaptureFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.detail)?;
        if let Some(path) = &self.diagnostics {
            write!(
                formatter,
                "; diagnostic spools retained at {}",
                path.display()
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for CaptureFailure {}

pub(super) fn is_clean_cancellation(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<CaptureFailure>()
        .is_some_and(|failure| failure.kind == CaptureFailureKind::Cancelled)
}

pub(super) struct SpoolOutput {
    status: ExitStatus,
    stdout: File,
    stderr: File,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    scratch_root: PathBuf,
    scratch: Option<TempDir>,
}

impl SpoolOutput {
    pub(super) fn status(&self) -> ExitStatus {
        self.status
    }

    pub(super) fn stdout_mut(&mut self) -> &mut File {
        &mut self.stdout
    }

    pub(super) fn read_output(
        mut self,
        stdout_limit: usize,
        stderr_limit: usize,
    ) -> Result<Output> {
        let stdout = match read_spool(&mut self.stdout, stdout_limit) {
            Ok(output) => output,
            Err(error) => return Err(self.retain_error(error)),
        };
        let stderr = match read_spool(&mut self.stderr, stderr_limit) {
            Ok(output) => output,
            Err(error) => return Err(self.retain_error(error)),
        };
        Ok(Output {
            status: self.status,
            stdout,
            stderr,
        })
    }

    fn retain_error(&mut self, error: anyhow::Error) -> anyhow::Error {
        let path = self.retain_scratch();
        CaptureFailure::new(
            CaptureFailureKind::Capture,
            format!(
                "reading completed private process output: {error:#}; stdout={}, stderr={}",
                self.stdout_path.display(),
                self.stderr_path.display()
            ),
            Some(path),
        )
        .into()
    }

    fn retain_scratch(&mut self) -> PathBuf {
        self.scratch
            .take()
            .map_or_else(|| self.scratch_root.clone(), TempDir::keep)
    }
}

pub(super) fn capture_strict(
    command: &mut Command,
    controls: &SetupControls,
    environment: EnvironmentMode,
    stdout_limit: usize,
    stderr_limit: usize,
    budget: CommandBudget,
) -> Result<Output> {
    capture_spooled(
        command,
        controls,
        environment,
        stdout_limit,
        stderr_limit,
        budget,
    )?
    .read_output(stdout_limit, stderr_limit)
}

pub(super) fn capture_spooled(
    command: &mut Command,
    controls: &SetupControls,
    environment: EnvironmentMode,
    stdout_limit: usize,
    stderr_limit: usize,
    budget: CommandBudget,
) -> Result<SpoolOutput> {
    capture_spooled_inner(
        command,
        None,
        controls,
        environment,
        stdout_limit,
        stderr_limit,
        budget,
    )
}

pub(super) fn capture_spooled_with_stdin_file(
    command: &mut Command,
    input: (File, usize),
    controls: &SetupControls,
    environment: EnvironmentMode,
    stdout_limit: usize,
    stderr_limit: usize,
    budget: CommandBudget,
) -> Result<SpoolOutput> {
    capture_spooled_inner(
        command,
        Some(input),
        controls,
        environment,
        stdout_limit,
        stderr_limit,
        budget,
    )
}

fn capture_spooled_inner(
    command: &mut Command,
    stdin_file: Option<(File, usize)>,
    controls: &SetupControls,
    environment: EnvironmentMode,
    stdout_limit: usize,
    stderr_limit: usize,
    budget: CommandBudget,
) -> Result<SpoolOutput> {
    let timeout = controls.child_timeout(budget)?;
    let scratch = private_capture_dir()?;
    let scratch_root = scratch.path().to_path_buf();
    let stdout_path = scratch_root.join("stdout.log");
    let stderr_path = scratch_root.join("stderr.log");
    let stdout = create_private_spool(&stdout_path).with_context(|| {
        format!(
            "creating private process stdout spool {}",
            stdout_path.display()
        )
    })?;
    let stderr = create_private_spool(&stderr_path).with_context(|| {
        format!(
            "creating private process stderr spool {}",
            stderr_path.display()
        )
    })?;
    let request = request_from_command(command, environment, stdout_limit, stderr_limit, timeout);

    let execution = match stdin_file {
        Some((input, input_limit)) => exec_spooled_sync_cancellable_with_stdin_file(
            &request,
            input,
            input_limit,
            stdout,
            stderr,
            controls.cancellation(),
        ),
        None => exec_spooled_sync_cancellable(&request, stdout, stderr, controls.cancellation()),
    };
    let (result, scratch) = match_execution(execution, scratch)?;

    let mut stdout = result.stdout;
    let mut stderr = result.stderr;
    let files_synced = stdout.sync_all().and_then(|()| stderr.sync_all());
    let failure_kind = if !result.cleanup.group_drained || result.cleanup.recovery.is_some() {
        Some(CaptureFailureKind::UncertainCleanup)
    } else if result.execution.timed_out {
        Some(CaptureFailureKind::Timeout)
    } else if !result.capture.pipes.both_reached_eof()
        || !result.failures.is_empty()
        || files_synced.is_err()
    {
        Some(CaptureFailureKind::Capture)
    } else if result.execution.cancelled {
        Some(CaptureFailureKind::Cancelled)
    } else if !result.capture.complete {
        Some(CaptureFailureKind::Capture)
    } else {
        None
    };
    if let Some(kind) = failure_kind {
        let detail = format!(
            "strict process capture incomplete: code={:?}, signal={:?}, timed_out={}, cancelled={}, capture_complete={}, stdout_eof={}, stderr_eof={}, group_drained={}, recovery={}, failures={:?}{}",
            result.code,
            result.signal,
            result.execution.timed_out,
            result.execution.cancelled,
            result.capture.complete,
            result.capture.pipes.stdout_eof,
            result.capture.pipes.stderr_eof,
            result.cleanup.group_drained,
            result.cleanup.recovery.is_some(),
            result.failures,
            files_synced
                .err()
                .map(|error| format!(", spool_sync_error={error}"))
                .unwrap_or_default(),
        );
        let retained = scratch.keep();
        drop(stdout);
        drop(stderr);
        drop(result.cleanup.recovery);
        return Err(CaptureFailure::new(kind, detail, Some(retained)).into());
    }

    let status = match (result.code, result.signal) {
        (Some(code), None) if (0..=255).contains(&code) => ExitStatus::from_raw(code << 8),
        (None, Some(signal)) if (1..=127).contains(&signal) => ExitStatus::from_raw(signal),
        _ => {
            let retained = scratch.keep();
            drop(stdout);
            drop(stderr);
            drop(result.cleanup.recovery);
            return Err(CaptureFailure::new(
                CaptureFailureKind::Capture,
                format!(
                    "strict process capture returned invalid status code={:?}, signal={:?}",
                    result.code, result.signal
                ),
                Some(retained),
            )
            .into());
        }
    };
    if let Err(error) = stdout
        .seek(SeekFrom::Start(0))
        .and_then(|_| stderr.seek(SeekFrom::Start(0)))
    {
        let retained = scratch.keep();
        drop(stdout);
        drop(stderr);
        drop(result.cleanup.recovery);
        return Err(CaptureFailure::new(
            CaptureFailureKind::Capture,
            format!("rewinding completed private process output: {error}"),
            Some(retained),
        )
        .into());
    }
    Ok(SpoolOutput {
        status,
        stdout,
        stderr,
        stdout_path,
        stderr_path,
        scratch_root,
        scratch: Some(scratch),
    })
}

fn match_execution(
    execution: Result<SpoolExecResult>,
    scratch: TempDir,
) -> Result<(SpoolExecResult, TempDir)> {
    match execution {
        Ok(result) => Ok((result, scratch)),
        Err(error) => {
            let detail = format!("{error:#}");
            let (mut stdout, mut stderr, recovery, spool_kind) =
                match error.downcast::<SpoolError>() {
                    Ok(spool) => {
                        let kind = spool.kind;
                        let (stdout, stderr, recovery) = spool.into_parts();
                        (stdout, stderr, recovery, Some(kind))
                    }
                    Err(error) => {
                        let root = scratch.keep();
                        return Err(error.context(format!(
                            "process capture failed; private diagnostic directory retained at {}",
                            root.display()
                        )));
                    }
                };
            let stdout_sync = stdout.as_mut().map(|file| file.sync_all()).transpose();
            let stderr_sync = stderr.as_mut().map(|file| file.sync_all()).transpose();
            let kind = if recovery.is_some() || spool_kind == Some(SpoolFailureKind::Cleanup) {
                CaptureFailureKind::UncertainCleanup
            } else {
                CaptureFailureKind::Capture
            };
            let retained = scratch.keep();
            drop(stdout);
            drop(stderr);
            drop(recovery);
            let detail = format!(
                "{detail}{}{}",
                stdout_sync
                    .err()
                    .map(|error| format!("; stdout spool sync failed: {error}"))
                    .unwrap_or_default(),
                stderr_sync
                    .err()
                    .map(|error| format!("; stderr spool sync failed: {error}"))
                    .unwrap_or_default(),
            );
            Err(CaptureFailure::new(kind, detail, Some(retained)).into())
        }
    }
}

fn private_capture_dir() -> Result<TempDir> {
    let parent = canonical_temporary_parent(&std::env::temp_dir())?;
    let scratch = tempfile::Builder::new()
        .prefix("jackin-native-capture-")
        .tempdir_in(parent)
        .context("creating private process capture directory")?;
    fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o700))
        .context("protecting private process capture directory")?;
    validate_confidential_directory(scratch.path())
        .context("validating private process capture directory")?;
    Ok(scratch)
}

pub(super) fn validate_confidential_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let before = fs::symlink_metadata(path).context("inspecting private scratch directory")?;
    ensure!(before.is_dir(), "private scratch path is not a directory");
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .context("opening private scratch directory")?;
    let file = File::from(descriptor);
    let opened = file
        .metadata()
        .context("reading private scratch directory descriptor")?;
    ensure!(
        before.dev() == opened.dev() && before.ino() == opened.ino(),
        "private scratch directory changed while opening"
    );
    jackin_process_directory::CheckedInstallRoot::validate_confidential_directory(&file)
        .context("checking private scratch directory ownership, mode, and ACL")?;
    let after = fs::symlink_metadata(path).context("restating private scratch directory")?;
    ensure!(
        after.is_dir() && before.dev() == after.dev() && before.ino() == after.ino(),
        "private scratch directory changed during confidentiality validation"
    );
    Ok(())
}

fn request_from_command(
    command: &Command,
    environment: EnvironmentMode,
    stdout_limit: usize,
    stderr_limit: usize,
    timeout: Duration,
) -> ExecRequest {
    let mut request = ExecRequest::new(
        PathBuf::from(command.get_program()),
        command.get_args().map(OsStr::to_os_string),
    );
    request.cwd = command.get_current_dir().map(Path::to_path_buf);
    request.env_clear = environment == EnvironmentMode::Clear;
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => request.env.push((key.to_os_string(), value.to_os_string())),
            None => request.env_remove.push(key.to_os_string()),
        }
    }
    request.stdin_mode = jackin_process::StdioMode::Null;
    request.stdout_mode = jackin_process::StdioMode::Capture;
    request.stderr_mode = jackin_process::StdioMode::Capture;
    request.timeout = Some(timeout);
    request.stdout_limit = Some(stdout_limit);
    request.stderr_limit = Some(stderr_limit);
    request.retry = jackin_process::RetryPolicy::none();
    request
}

fn read_spool(file: &mut File, limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(16 * 1024));
    file.take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut output)
        .context("reading private process output spool")?;
    ensure!(
        output.len() <= limit,
        "private process output exceeded its byte limit"
    );
    Ok(output)
}

#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, process::ExitStatusExt};

pub(super) fn open_readonly(path: &Path) -> io::Result<File> {
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    Ok(File::from(descriptor))
}

pub(super) fn create_private_new(path: &Path) -> io::Result<File> {
    let descriptor = open(
        path,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(io::Error::from)?;
    fchmod(&descriptor, Mode::from_bits_truncate(0o600)).map_err(io::Error::from)?;
    Ok(File::from(descriptor))
}

fn create_private_spool(path: &Path) -> io::Result<File> {
    let descriptor = open(
        path,
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(io::Error::from)?;
    fchmod(&descriptor, Mode::from_bits_truncate(0o600)).map_err(io::Error::from)?;
    Ok(File::from(descriptor))
}
