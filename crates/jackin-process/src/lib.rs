//! jackin-process: shared subprocess transport (capture, timeout, retry, status).
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`RetryPolicy`] — capture/timeout helpers without redaction or telemetry.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// How many times to re-run a failed command (excluding the first attempt).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetryPolicy {
    /// Extra attempts after the first failure. `0` = no retry.
    pub max_retries: u32,
    /// Delay between attempts.
    pub delay: Duration,
}

/// Child standard-stream routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioMode {
    /// Connect a pipe and return the emitted bytes from `exec_*`.
    Capture,
    /// Inherit the caller's corresponding stream.
    Inherit,
    /// Connect the stream to the null device.
    Null,
}

impl RetryPolicy {
    /// No retries.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            max_retries: 0,
            delay: Duration::from_millis(0),
        }
    }
}

/// Subprocess request (ordinary bytes + timing knobs only).
#[derive(Debug, Clone)]
pub struct ExecRequest {
    /// Program path or name on `PATH`.
    pub program: PathBuf,
    /// Arguments (not including the program).
    pub args: Vec<std::ffi::OsString>,
    /// Optional working directory.
    pub cwd: Option<PathBuf>,
    /// Optional stdin bytes.
    pub stdin: Option<Vec<u8>>,
    /// Optional extra environment entries (pass-through only — no filtering).
    pub env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    /// Environment keys removed after applying inheritance/clear policy.
    pub env_remove: Vec<std::ffi::OsString>,
    /// Start with an empty environment rather than inheriting the parent.
    pub env_clear: bool,
    /// Stdin routing when `stdin` bytes are absent.
    pub stdin_mode: StdioMode,
    /// Stdout routing.
    pub stdout_mode: StdioMode,
    /// Stderr routing.
    pub stderr_mode: StdioMode,
    /// Kill after this duration. `None` = wait indefinitely (capsule probe
    /// semantic: no read timeout).
    pub timeout: Option<Duration>,
    /// Retry policy on non-success exit (not applied on timeout).
    pub retry: RetryPolicy,
}

impl ExecRequest {
    /// Build a request for `program` with the given args.
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
            retry: RetryPolicy::none(),
        }
    }

    /// Append pass-through environment entries.
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

    /// Remove inherited environment keys.
    #[must_use]
    pub fn env_remove(mut self, keys: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        self.env_remove
            .extend(keys.into_iter().map(|key| key.as_ref().to_os_string()));
        self
    }

    /// Start the child with an empty environment.
    #[must_use]
    pub fn env_clear(mut self) -> Self {
        self.env_clear = true;
        self
    }

    /// Route stdin when no explicit bytes are supplied.
    #[must_use]
    pub fn stdin_mode(mut self, mode: StdioMode) -> Self {
        self.stdin_mode = mode;
        self
    }

    /// Route stdout.
    #[must_use]
    pub fn stdout_mode(mut self, mode: StdioMode) -> Self {
        self.stdout_mode = mode;
        self
    }

    /// Route stderr.
    #[must_use]
    pub fn stderr_mode(mut self, mode: StdioMode) -> Self {
        self.stderr_mode = mode;
        self
    }

    /// Set working directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set wall-clock timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Clear timeout (wait forever).
    #[must_use]
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Set retry policy.
    #[must_use]
    pub fn retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }
}

/// Subprocess result.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Process exit status code (`None` if killed by signal / unavailable).
    pub code: Option<i32>,
    /// Whether the process reported success (`ExitStatus::success`).
    pub success: bool,
    /// Captured stdout.
    pub stdout: Vec<u8>,
    /// Captured stderr.
    pub stderr: Vec<u8>,
    /// Wall time of the final attempt.
    pub duration: Duration,
    /// True when the run ended because `timeout` elapsed.
    pub timed_out: bool,
}

/// Async execution with optional timeout and retry.
///
/// # Errors
/// Returns when spawn fails or all retry attempts fail to start.
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

/// Spawn an async child using the same request model without waiting for it.
///
/// Retry and timeout apply only to `exec_*`; lifecycle callers own waiting,
/// cancellation, and retries after this function returns.
pub fn spawn_async(request: &ExecRequest) -> Result<tokio::process::Child> {
    if request.stdin.is_some() {
        bail!("spawn_async does not write request stdin bytes; use exec_async or a captured stdin");
    }
    let mut command = tokio::process::Command::new(&request.program);
    configure_async_command(&mut command, request);
    command
        .spawn()
        .with_context(|| format!("spawning {}", display_request(request)))
}

/// Spawn a synchronous child using the same request model without waiting.
pub fn spawn_sync(request: &ExecRequest) -> Result<std::process::Child> {
    if request.stdin.is_some() {
        bail!("spawn_sync does not write request stdin bytes; use exec_sync or a captured stdin");
    }
    let mut command = std::process::Command::new(&request.program);
    configure_sync_command(&mut command, request);
    command
        .spawn()
        .with_context(|| format!("spawning {}", display_request(request)))
}

/// Sync facade over [`exec_async`] using a current-thread runtime when needed.
///
/// # Errors
/// Propagates spawn / runtime build failures.
pub fn exec_sync(request: &ExecRequest) -> Result<ExecResult> {
    // Prefer calling from outside an existing runtime; if one exists, use
    // a nested current-thread runtime in a blocking section.
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

async fn run_once_async(request: &ExecRequest) -> Result<ExecResult> {
    let started = Instant::now();
    let mut cmd = tokio::process::Command::new(&request.program);
    configure_async_command(&mut cmd, request);
    cmd.kill_on_drop(true);
    if request.stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawning {}", display_request(request)))?;

    if let Some(bytes) = &request.stdin {
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(bytes)
                .await
                .context("writing stdin to child")?;
        }
    }

    let output = if let Some(timeout) = request.timeout {
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(result) => result.context("waiting on child")?,
            Err(_) => {
                // wait_with_output consumed the child on success; on timeout the
                // future was dropped — kill_on_drop aborts the process.
                return Ok(ExecResult {
                    code: None,
                    success: false,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    duration: started.elapsed(),
                    timed_out: true,
                });
            }
        }
    } else {
        child.wait_with_output().await.context("waiting on child")?
    };

    Ok(ExecResult {
        code: output.status.code(),
        success: output.status.success(),
        stdout: output.stdout,
        stderr: output.stderr,
        duration: started.elapsed(),
        timed_out: false,
    })
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

/// Convenience: run and require success; return stdout.
///
/// # Errors
/// Non-success exit or spawn failure.
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

/// Sync [`capture_stdout_async`].
///
/// # Errors
/// Non-success exit or spawn failure.
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
