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

/// Shared child ownership coordination for orphan reapers and spawn owners.
#[cfg(unix)]
pub mod child_ownership;

mod spawned_child;
pub use spawned_child::{AsyncChild, GroupChild, GroupSyncChild, SyncChild};

/// Default maximum bytes retained per stream by `exec_async` and `exec_sync`.
pub const DEFAULT_CAPTURE_LIMIT: usize = 16 * 1024 * 1024;

/// Typed context for callers that classify execution errors without inspecting
/// potentially private command paths, arguments, or operating-system messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecStage {
    /// Command setup failed before spawning a child.
    Setup,
    /// The operating system rejected spawning the configured child.
    Spawn,
}

impl std::fmt::Display for ExecStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Setup => "process setup failed",
            Self::Spawn => "process spawn failed",
        })
    }
}

impl std::error::Error for ExecStage {}

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
    /// Opened Unix directory used without resolving a pathname. Mutually
    /// exclusive with `cwd`; retained through every retry and command spawn.
    #[cfg(unix)]
    pub pinned_cwd: Option<std::sync::Arc<std::fs::File>>,
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
    /// Maximum captured stdout bytes. `None` uses [`DEFAULT_CAPTURE_LIMIT`].
    pub stdout_limit: Option<usize>,
    /// Maximum captured stderr bytes. `None` uses [`DEFAULT_CAPTURE_LIMIT`].
    pub stderr_limit: Option<usize>,
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
            #[cfg(unix)]
            pinned_cwd: None,
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

    /// Set a descriptor-pinned working directory.
    #[cfg(unix)]
    #[must_use]
    pub fn pinned_cwd(mut self, directory: std::sync::Arc<std::fs::File>) -> Self {
        self.pinned_cwd = Some(directory);
        self
    }

    /// Set wall-clock timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Bound each captured output stream independently.
    #[must_use]
    pub fn output_limits(mut self, stdout: usize, stderr: usize) -> Self {
        self.stdout_limit = Some(stdout);
        self.stderr_limit = Some(stderr);
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
/// Returns on invalid routing/limits, spawn or stream failure, output overflow,
/// or failure to kill/reap a child. Timeout results contain empty output.
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
/// cancellation, and retries after this function returns. Pipe readers and
/// their byte limits belong to the caller; explicit capture limits are rejected
/// because this function cannot enforce them. Default exec limits do not apply.
pub fn spawn_async(request: &ExecRequest) -> Result<AsyncChild> {
    spawn_async_owned(request, false)
}

/// Spawn a registered child owning a private process group.
/// Cancellation, kill, and failed output reads terminate the entire group.
/// Drain taken capture pipes before consuming finish to keep the leader PID reserved.
/// # Errors
/// Returns request validation, setup, or spawn failures.
pub fn spawn_group_async(request: &ExecRequest) -> Result<GroupChild> {
    spawn_async_owned(request, true).map(GroupChild::new)
}

/// Spawn a private group owning the inherited controlling terminal's foreground.
/// This function acquires `gate` only during synchronous setup. The release closure
/// owns its logical terminal token from before spawn through confirmed cleanup,
/// including restoration failure after failed spawn. With non-inherited input,
/// no physical foreground transfer occurs, but terminal scope remains owned.
/// # Errors
/// Returns request validation, terminal setup, command setup, or spawn failures.
pub fn spawn_foreground_async(
    request: &ExecRequest,
    gate: std::sync::Arc<std::sync::Mutex<()>>,
    release: impl FnOnce() + Send + 'static,
) -> Result<GroupChild> {
    let scope = spawned_child::ForegroundScope::new(release);
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "foreground child ownership requires Linux or macOS",
        ))
        .context(ExecStage::Setup);
    }
    #[cfg(unix)]
    let mut foreground = None;
    let outcome = {
        // Scope destruction occurs after this short gate has been released.
        // No logical-token destructor may recursively acquire its own gate.
        let _serialized = gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(unix)]
        let native_spawn = jackin_process_directory::native_spawn_guard();
        (|| {
            let mut command = configured_async_spawn(request, true)?;
            #[cfg(unix)]
            if request.stdin_mode == StdioMode::Inherit {
                foreground = jackin_process_directory::ForegroundGuard::prepare(
                    command.as_std_mut(),
                    &native_spawn,
                )
                .context(ExecStage::Setup)?;
            }
            match spawn_registered_async(
                &mut command,
                request,
                true,
                #[cfg(unix)]
                &native_spawn,
            ) {
                Ok(child) => Ok(child),
                Err(error) => {
                    #[cfg(unix)]
                    if let Some(guard) = foreground.as_mut() {
                        guard.restore().context(ExecStage::Setup)?;
                        foreground = None;
                    }
                    Err(error)
                }
            }
        })()
    };
    let child = match outcome {
        Ok(child) => child,
        Err(error) => {
            #[cfg(unix)]
            if let Some(foreground) = foreground.take() {
                spawned_child::retain_failed_foreground_restore(foreground, scope, gate);
            }
            return Err(error);
        }
    };
    let child = GroupChild::new(child);
    #[cfg(unix)]
    let child = {
        if let Some(guard) = foreground.as_mut() {
            guard.set_restore_gate(gate);
        }
        child.with_foreground(foreground)
    };
    #[cfg(not(unix))]
    let _gate = gate;
    Ok(child.with_foreground_scope(scope))
}

fn spawn_async_owned(request: &ExecRequest, owns_group: bool) -> Result<AsyncChild> {
    let mut command = configured_async_spawn(request, owns_group)?;
    #[cfg(unix)]
    let native_spawn = jackin_process_directory::native_spawn_guard();
    spawn_registered_async(
        &mut command,
        request,
        owns_group,
        #[cfg(unix)]
        &native_spawn,
    )
}

fn configured_async_spawn(
    request: &ExecRequest,
    owns_group: bool,
) -> Result<tokio::process::Command> {
    #[cfg(not(unix))]
    let _owns_group = owns_group;
    validate_request(request, true)?;
    if request.stdin.is_some() {
        bail!("spawn_async does not write request stdin bytes; use exec_async or a captured stdin");
    }
    let mut command = tokio::process::Command::new(&request.program);
    configure_async_command(&mut command, request).context(ExecStage::Setup)?;
    command.kill_on_drop(false);
    #[cfg(unix)]
    if owns_group {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    Ok(command)
}

fn spawn_registered_async(
    command: &mut tokio::process::Command,
    request: &ExecRequest,
    owns_group: bool,
    #[cfg(unix)] _native_spawn: &jackin_process_directory::NativeSpawnGuard,
) -> Result<AsyncChild> {
    #[cfg(unix)]
    return child_ownership::coordinate(|registry| {
        let child = command
            .spawn()
            .with_context(|| format!("spawning {}", display_request(request)))
            .context(ExecStage::Spawn)?;
        let registration = child.id().map(|pid| registry.register(pid));
        Ok(AsyncChild::new(child, registration, owns_group))
    });
    #[cfg(not(unix))]
    Ok(AsyncChild::new(
        command.spawn().context(ExecStage::Spawn)?,
        owns_group,
    ))
}

/// Spawn a synchronous child using the same request model without waiting.
///
/// The caller owns pipe limits and the entire child lifecycle, as in
/// [`spawn_async`]. Explicit capture limits are rejected.
pub fn spawn_sync(request: &ExecRequest) -> Result<SyncChild> {
    spawn_sync_owned(request, false)
}

/// Spawn a synchronous child owning a private process group.
/// # Errors
/// Returns request validation, setup, or spawn failures.
pub fn spawn_group_sync(request: &ExecRequest) -> Result<GroupSyncChild> {
    spawn_sync_owned(request, true).map(GroupSyncChild::new)
}

fn spawn_sync_owned(request: &ExecRequest, owns_group: bool) -> Result<SyncChild> {
    #[cfg(not(unix))]
    let _owns_group = owns_group;
    validate_request(request, true)?;
    if request.stdin.is_some() {
        bail!("spawn_sync does not write request stdin bytes; use exec_sync or a captured stdin");
    }
    let mut command = std::process::Command::new(&request.program);
    configure_sync_command(&mut command, request).context(ExecStage::Setup)?;
    #[cfg(unix)]
    if owns_group {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(unix)]
    let _native_spawn = jackin_process_directory::native_spawn_guard();
    #[cfg(unix)]
    return child_ownership::coordinate(|registry| {
        let child = command
            .spawn()
            .with_context(|| format!("spawning {}", display_request(request)))
            .context(ExecStage::Spawn)?;
        let registration = Some(registry.register(child.id()));
        Ok(SyncChild::new(child, registration, owns_group))
    });
    #[cfg(not(unix))]
    Ok(SyncChild::new(
        command.spawn().context(ExecStage::Spawn)?,
        owns_group,
    ))
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
    let mut buffer = [0; 8192];
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
    configure_async_command(&mut cmd, request).context(ExecStage::Setup)?;
    cmd.kill_on_drop(false);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.as_std_mut().process_group(0);
    }
    if request.stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    #[cfg(unix)]
    let owned = {
        let _native_spawn = jackin_process_directory::native_spawn_guard();
        child_ownership::coordinate(|registry| {
            let child = cmd
                .spawn()
                .with_context(|| format!("spawning {}", display_request(request)))
                .context(ExecStage::Spawn)?;
            let registration = child.id().map(|pid| registry.register(pid));
            Ok::<_, anyhow::Error>(AsyncChild::new(child, registration, true))
        })?
    };
    #[cfg(not(unix))]
    let owned = AsyncChild::new(
        cmd.spawn()
            .with_context(|| format!("spawning {}", display_request(request)))
            .context(ExecStage::Spawn)?,
        true,
    );
    let mut owned = GroupChild::new(owned);
    let stdin = owned.stdin.take();
    let stdout = owned.stdout.take();
    let stderr = owned.stderr.take();
    let completion = async {
        let write = async {
            if let (Some(bytes), Some(mut stdin)) = (&request.stdin, stdin) {
                stdin
                    .write_all(bytes)
                    .await
                    .context("writing stdin to child")?;
                stdin.shutdown().await.context("closing child stdin")?;
            }
            Ok::<_, anyhow::Error>(())
        };
        // Keep the direct child unreaped while descendants can retain pipes.
        // Its reserved PID prevents the process-group ID from being reused.
        let ((), stdout, stderr) = tokio::try_join!(
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
        Ok::<_, anyhow::Error>((stdout, stderr))
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
        Some(Ok((stdout, stderr))) => {
            let status = if let Some(deadline) = deadline {
                owned
                    .finish_with_timeout(
                        deadline.saturating_duration_since(tokio::time::Instant::now()),
                    )
                    .await
                    .context("waiting on child")?
            } else {
                Some(owned.finish().await.context("waiting on child")?)
            };
            match status {
                Some(status) => Ok(ExecResult {
                    code: status.code(),
                    success: status.success(),
                    stdout,
                    stderr,
                    duration: started.elapsed(),
                    timed_out: false,
                }),
                None => Ok(ExecResult {
                    code: None,
                    success: false,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    duration: started.elapsed(),
                    timed_out: true,
                }),
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

fn configure_async_command(
    command: &mut tokio::process::Command,
    request: &ExecRequest,
) -> Result<()> {
    command
        .args(&request.args)
        .stdin(stdio(request.stdin_mode))
        .stdout(stdio(request.stdout_mode))
        .stderr(stdio(request.stderr_mode));
    apply_command_options(command.as_std_mut(), request)
}

fn configure_sync_command(
    command: &mut std::process::Command,
    request: &ExecRequest,
) -> Result<()> {
    command
        .args(&request.args)
        .stdin(stdio(request.stdin_mode))
        .stdout(stdio(request.stdout_mode))
        .stderr(stdio(request.stderr_mode));
    apply_command_options(command, request)
}

fn apply_command_options(command: &mut std::process::Command, request: &ExecRequest) -> Result<()> {
    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd);
    }
    #[cfg(unix)]
    if let Some(directory) = &request.pinned_cwd {
        jackin_process_directory::current_dir(command, std::sync::Arc::clone(directory))?;
    }
    if request.env_clear {
        command.env_clear();
    }
    command.envs(request.env.iter().map(|(key, value)| (key, value)));
    for key in &request.env_remove {
        command.env_remove(key);
    }
    Ok(())
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
