// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

mod child_ownership;
mod spooled;

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use child_ownership::registry_contention_count;
pub(crate) use child_ownership::supports_selective_reaping;
pub use child_ownership::{
    ChildRegistration, ChildRegistrationSlot, ChildRegistry, coordinate_children,
    has_reaper_wakeup, install_reaper_wakeup, take_registry_poisoned,
};
pub use spooled::{
    SpoolCaptureObservation, SpoolCleanupObservation, SpoolError, SpoolExecResult,
    SpoolExecutionObservation, SpoolFailure, SpoolFailureKind, SpoolPipeObservation, SpoolRecovery,
    exec_spooled_sync_cancellable,
};

pub const DEFAULT_CAPTURE_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioMode {
    Capture,
    Inherit,
    Null,
}

impl RetryPolicy {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            max_retries: 0,
            delay: Duration::from_millis(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub program: PathBuf,
    pub args: Vec<std::ffi::OsString>,
    pub cwd: Option<PathBuf>,
    pub stdin: Option<Vec<u8>>,
    pub env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    pub env_remove: Vec<std::ffi::OsString>,
    pub env_clear: bool,
    pub stdin_mode: StdioMode,
    pub stdout_mode: StdioMode,
    pub stderr_mode: StdioMode,
    pub timeout: Option<Duration>,
    pub stdout_limit: Option<usize>,
    pub stderr_limit: Option<usize>,
    pub retry: RetryPolicy,
}

impl ExecRequest {
    #[must_use]
    pub fn new(
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args
                .into_iter()
                .map(|a| a.as_ref().to_os_string())
                .collect(),
            cwd: None,
            stdin: None,
            env: Vec::new(),
            env_remove: Vec::new(),
            env_clear: false,
            stdin_mode: StdioMode::Null,
            stdout_mode: StdioMode::Capture,
            stderr_mode: StdioMode::Capture,
            timeout: None,
            stdout_limit: None,
            stderr_limit: None,
            retry: RetryPolicy::none(),
        }
    }

    #[must_use]
    pub fn envs(
        mut self,
        envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> Self {
        self.env.extend(
            envs.into_iter()
                .map(|(k, v)| (k.as_ref().to_os_string(), v.as_ref().to_os_string())),
        );
        self
    }

    #[must_use]
    pub fn env_remove(mut self, keys: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        self.env_remove
            .extend(keys.into_iter().map(|key| key.as_ref().to_os_string()));
        self
    }

    #[must_use]
    pub fn env_clear(mut self) -> Self {
        self.env_clear = true;
        self
    }

    #[must_use]
    pub fn stdin_mode(mut self, mode: StdioMode) -> Self {
        self.stdin_mode = mode;
        self
    }

    #[must_use]
    pub fn stdout_mode(mut self, mode: StdioMode) -> Self {
        self.stdout_mode = mode;
        self
    }

    #[must_use]
    pub fn stderr_mode(mut self, mode: StdioMode) -> Self {
        self.stderr_mode = mode;
        self
    }

    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn output_limits(mut self, stdout: usize, stderr: usize) -> Self {
        self.stdout_limit = Some(stdout);
        self.stderr_limit = Some(stderr);
        self
    }

    #[must_use]
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    #[must_use]
    pub fn retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub code: Option<i32>,
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
    pub timed_out: bool,
}

/// The child pipe reported `BrokenPipe` while delivering requested stdin.
///
/// The process was still observed, drained, and reaped under the original
/// request deadline. Its result is available even though incomplete input
/// remains an execution error and must not be retried automatically.
pub struct StdinDeliveryError {
    source: io::Error,
    operation: StdinDeliveryOperation,
    result: ExecResult,
}

/// The stdin operation that failed after the child closed its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinDeliveryOperation {
    /// Writing request bytes to the child's stdin.
    Write,
    /// Shutting down the child's stdin after writing request bytes.
    Shutdown,
}

impl StdinDeliveryError {
    /// Return the observed result from the child after bounded capture and wait.
    #[must_use]
    pub const fn result(&self) -> &ExecResult {
        &self.result
    }

    /// Return which stdin operation observed the closed pipe.
    #[must_use]
    pub const fn operation(&self) -> StdinDeliveryOperation {
        self.operation
    }
}

impl std::fmt::Debug for StdinDeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StdinDeliveryError")
            .field("source", &self.source)
            .field("operation", &self.operation)
            .field("child_code", &self.result.code)
            .field("child_success", &self.result.success)
            .field("stdout_bytes", &self.result.stdout.len())
            .field("stderr_bytes", &self.result.stderr.len())
            .field("duration", &self.result.duration)
            .field("timed_out", &self.result.timed_out)
            .finish()
    }
}

impl std::fmt::Display for StdinDeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let operation = match self.operation {
            StdinDeliveryOperation::Write => "writing request bytes to child stdin",
            StdinDeliveryOperation::Shutdown => "shutting down child stdin",
        };
        write!(
            formatter,
            "stdin delivery failed while {operation}: {}",
            self.source
        )
    }
}

impl std::error::Error for StdinDeliveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub async fn exec_async(request: &ExecRequest) -> Result<ExecResult> {
    let attempts = request.retry.max_retries.saturating_add(1);
    let mut last: Option<ExecResult> = None;
    for attempt in 0..attempts {
        if attempt > 0 && !request.retry.delay.is_zero() {
            tokio::time::sleep(request.retry.delay).await;
        }
        let result = run_once_async(request).await?;
        if result.success || result.timed_out {
            return Ok(result);
        }
        last = Some(result);
    }
    last.ok_or_else(|| anyhow::anyhow!("jackin-process: zero attempts scheduled"))
}

pub fn spawn_async(request: &ExecRequest) -> Result<tokio::process::Child> {
    validate_request(request, true)?;
    if request.stdin.is_some() {
        bail!("spawn_async does not write request stdin bytes; use exec_async or a captured stdin");
    }
    let mut command = tokio::process::Command::new(&request.program);
    configure_async_command(&mut command, request);
    command
        .spawn()
        .with_context(|| format!("spawning {}", display_request(request)))
}

pub fn spawn_sync(request: &ExecRequest) -> Result<std::process::Child> {
    validate_request(request, true)?;
    if request.stdin.is_some() {
        bail!("spawn_sync does not write request stdin bytes; use exec_sync or a captured stdin");
    }
    let mut command = std::process::Command::new(&request.program);
    configure_sync_command(&mut command, request);
    command
        .spawn()
        .with_context(|| format!("spawning {}", display_request(request)))
}

pub fn exec_sync(request: &ExecRequest) -> Result<ExecResult> {
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|s| {
            s.spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("building jackin-process runtime")?;
                rt.block_on(exec_async(request))
            })
            .join()
            .map_err(|_| anyhow::anyhow!("jackin-process sync worker panicked"))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("building jackin-process runtime")?;
        rt.block_on(exec_async(request))
    }
}

fn validate_request(request: &ExecRequest, spawning: bool) -> Result<()> {
    for (name, mode, limit) in [
        ("stdout", request.stdout_mode, request.stdout_limit),
        ("stderr", request.stderr_mode, request.stderr_limit),
    ] {
        if limit.is_some_and(|bytes| bytes > usize::MAX / 2) {
            bail!("{name} byte limit exceeds maximum buffer size");
        }
        if limit.is_some() && mode != StdioMode::Capture {
            bail!("{name} byte limit requires Capture routing");
        }
        if spawning && limit.is_some() {
            bail!("spawn cannot enforce {name} byte limit; use exec_async or exec_sync");
        }
    }
    Ok(())
}

#[derive(Debug)]
struct OwnedChild {
    child: tokio::process::Child,
    #[cfg(unix)]
    group: Option<nix::unistd::Pid>,
}

impl OwnedChild {
    fn new(child: tokio::process::Child) -> Self {
        Self {
            #[cfg(unix)]
            group: child
                .id()
                .and_then(|id| i32::try_from(id).ok())
                .map(nix::unistd::Pid::from_raw),
            child,
        }
    }

    fn kill_group(&self) -> Result<()> {
        #[cfg(unix)]
        if let Some(group) = self.group {
            match nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                Err(error) => return Err(error).context("killing child process group"),
            }
        }
        Ok(())
    }

    async fn kill_and_reap(&mut self) -> Result<()> {
        let group_result = self.kill_group();
        let kill_result = self.child.start_kill().context("killing child");
        tokio::time::timeout(Duration::from_secs(1), self.child.wait())
            .await
            .context("child did not reap within cleanup deadline")?
            .context("reaping child")?;
        group_result?;
        kill_result?;
        #[cfg(unix)]
        {
            self.group = None;
        }
        Ok(())
    }

    fn disarm(&mut self) {
        #[cfg(unix)]
        {
            self.group = None;
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        drop(self.kill_group());
    }
}

async fn read_captured(
    stream: Option<impl tokio::io::AsyncRead + Unpin>,
    limit: usize,
    name: &str,
) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let Some(mut stream) = stream else {
        return Ok(Vec::new());
    };
    let mut bytes = Vec::new();
    let mut buffer = vec![0_u8; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .with_context(|| format!("reading child {name}"))?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > limit.saturating_sub(bytes.len()) {
            bail!("captured {name} exceeded byte limit {limit}");
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

async fn run_once_async(request: &ExecRequest) -> Result<ExecResult> {
    run_once_with_gate_async(request, std::future::ready(Ok(()))).await
}

async fn run_once_with_gate_async<F>(
    request: &ExecRequest,
    before_stdin_write: F,
) -> Result<ExecResult>
where
    F: Future<Output = Result<()>>,
{
    use tokio::io::AsyncWriteExt;
    validate_request(request, false)?;
    let started = Instant::now();
    let deadline = request
        .timeout
        .map(|timeout| {
            started
                .checked_add(timeout)
                .map(tokio::time::Instant::from_std)
                .context("process timeout exceeds clock range")
        })
        .transpose()?;
    let mut cmd = tokio::process::Command::new(&request.program);
    configure_async_command(&mut cmd, request);
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.as_std_mut().process_group(0);
    }
    if request.stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut owned = OwnedChild::new(
        cmd.spawn()
            .with_context(|| format!("spawning {}", display_request(request)))?,
    );
    let stdin = owned.child.stdin.take();
    let stdout = owned.child.stdout.take();
    let stderr = owned.child.stderr.take();
    let completion = async {
        let write = async {
            if let (Some(bytes), Some(mut stdin)) = (&request.stdin, stdin) {
                before_stdin_write.await?;
                match stdin.write_all(bytes).await {
                    Err(error)
                        if deadline.is_some() && error.kind() == io::ErrorKind::BrokenPipe =>
                    {
                        return Ok(Some((error, StdinDeliveryOperation::Write)));
                    }
                    Err(error) => return Err(error).context("writing stdin to child"),
                    Ok(()) => {}
                }
                match stdin.shutdown().await {
                    Err(error)
                        if deadline.is_some() && error.kind() == io::ErrorKind::BrokenPipe =>
                    {
                        return Ok(Some((error, StdinDeliveryOperation::Shutdown)));
                    }
                    Err(error) => return Err(error).context("closing child stdin"),
                    Ok(()) => {}
                }
            }
            Ok(None)
        };
        let (delivery_error, stdout, stderr) = tokio::try_join!(
            write,
            read_captured(
                stdout,
                request.stdout_limit.unwrap_or(DEFAULT_CAPTURE_LIMIT),
                "stdout"
            ),
            read_captured(
                stderr,
                request.stderr_limit.unwrap_or(DEFAULT_CAPTURE_LIMIT),
                "stderr"
            ),
        )?;
        let status = owned.child.wait().await.context("waiting on child")?;
        Ok::<_, anyhow::Error>((delivery_error, stdout, stderr, status))
    };
    let outcome = if let Some(deadline) = deadline {
        tokio::time::timeout_at(deadline, completion).await.ok()
    } else {
        Some(completion.await)
    };
    match outcome {
        None => {
            owned.kill_and_reap().await?;
            Ok(ExecResult {
                code: None,
                success: false,
                stdout: Vec::new(),
                stderr: Vec::new(),
                duration: started.elapsed(),
                timed_out: true,
            })
        }
        Some(Err(error)) => {
            owned.kill_and_reap().await.context(error.to_string())?;
            Err(error)
        }
        Some(Ok((delivery_error, stdout, stderr, status))) => {
            owned.disarm();
            let result = ExecResult {
                code: status.code(),
                success: status.success(),
                stdout,
                stderr,
                duration: started.elapsed(),
                timed_out: false,
            };
            if let Some((source, operation)) = delivery_error {
                Err(StdinDeliveryError {
                    source,
                    operation,
                    result,
                }
                .into())
            } else {
                Ok(result)
            }
        }
    }
}

fn stdio(mode: StdioMode) -> Stdio {
    match mode {
        StdioMode::Capture => Stdio::piped(),
        StdioMode::Inherit => Stdio::inherit(),
        StdioMode::Null => Stdio::null(),
    }
}

fn configure_async_command(command: &mut tokio::process::Command, request: &ExecRequest) {
    command
        .args(&request.args)
        .stdin(stdio(request.stdin_mode))
        .stdout(stdio(request.stdout_mode))
        .stderr(stdio(request.stderr_mode));
    apply_command_options(command.as_std_mut(), request);
}

fn configure_sync_command(command: &mut std::process::Command, request: &ExecRequest) {
    command
        .args(&request.args)
        .stdin(stdio(request.stdin_mode))
        .stdout(stdio(request.stdout_mode))
        .stderr(stdio(request.stderr_mode));
    apply_command_options(command, request);
}

fn apply_command_options(command: &mut std::process::Command, request: &ExecRequest) {
    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd);
    }
    if request.env_clear {
        command.env_clear();
    }
    command.envs(request.env.iter().map(|(key, value)| (key, value)));
    for key in &request.env_remove {
        command.env_remove(key);
    }
}

fn display_request(request: &ExecRequest) -> String {
    let prog = request.program.display();
    if request.args.is_empty() {
        prog.to_string()
    } else {
        let args = request
            .args
            .iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        format!("{prog} {args}")
    }
}

pub async fn capture_stdout_async(request: &ExecRequest) -> Result<Vec<u8>> {
    let result = exec_async(request).await?;
    if result.timed_out {
        bail!(
            "{} timed out after {:?}",
            display_request(request),
            request.timeout
        );
    }
    if !result.success {
        let stderr = String::from_utf8_lossy(&result.stderr);
        bail!(
            "{} failed (code={:?}): {}",
            display_request(request),
            result.code,
            stderr.trim()
        );
    }
    Ok(result.stdout)
}

pub fn capture_stdout_sync(request: &ExecRequest) -> Result<Vec<u8>> {
    let result = exec_sync(request)?;
    if result.timed_out {
        bail!(
            "{} timed out after {:?}",
            display_request(request),
            request.timeout
        );
    }
    if !result.success {
        let stderr = String::from_utf8_lossy(&result.stderr);
        bail!(
            "{} failed (code={:?}): {}",
            display_request(request),
            result.code,
            stderr.trim()
        );
    }
    Ok(result.stdout)
}

#[cfg(test)]
mod tests;
