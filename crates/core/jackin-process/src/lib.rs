// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

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
                stdin
                    .write_all(bytes)
                    .await
                    .context("writing stdin to child")?;
                stdin.shutdown().await.context("closing child stdin")?;
            }
            Ok::<_, anyhow::Error>(())
        };

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
        let status = owned.child.wait().await.context("waiting on child")?;
        Ok::<_, anyhow::Error>(((), stdout, stderr, status))
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
        Some(Ok(((), stdout, stderr, status))) => {
            owned.disarm();
            Ok(ExecResult {
                code: status.code(),
                success: status.success(),
                stdout,
                stderr,
                duration: started.elapsed(),
                timed_out: false,
            })
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
