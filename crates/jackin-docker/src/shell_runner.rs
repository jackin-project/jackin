//! `ShellRunner`: concrete subprocess implementation of `CommandRunner`.
//!
//! The `CommandRunner` trait and `RunOptions` are re-exported from
//! `jackin-core` so consumer crates depend on the trait, not this
//! tokio-based implementation.
//!
//! Not responsible for: the async Docker daemon API (`docker_client.rs`), or
//! parsing Docker output formats (those live in the callers).

use crate::DockerError;
use std::path::Path;
use std::process::ExitStatus;
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub use jackin_core::{BuildLogSink, CommandRunner, RunOptions};

fn cmd_failed(program: &str, args: &[&str]) -> DockerError {
    DockerError::CommandFailed {
        program: program.to_owned(),
        args: args.join(" "),
    }
}

#[derive(Debug, Default)]
pub struct ShellRunner {
    pub debug: bool,
}

impl ShellRunner {
    fn build_command(program: &str, args: &[&str], cwd: Option<&Path>) -> Command {
        let mut command = Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        // Kill the child if its awaiting future is dropped. The launch cancel
        // path (`while_waiting` losing the `select!` on Ctrl+C) drops the run
        // future mid-flight; without this the spawned process — notably a slow
        // `docker build` — keeps running detached, holding the daemon, so the
        // cancel-driven `LoadCleanup` then blocks on that same busy daemon and
        // the terminal appears frozen. `kill_on_drop` only fires when the
        // future is dropped before the child exits, so normal awaited runs are
        // unaffected.
        command.kill_on_drop(true);
        command
    }

    fn apply_run_opts(cmd: &mut Command, opts: &RunOptions) {
        // Destructure so a new RunOptions field forces a maintainer to
        // decide whether it belongs here (applied to every `run` arm)
        // or stays the responsibility of an arm-specific branch.
        let RunOptions {
            capture_stderr: _,
            capture_stdout: _,
            quiet: _,
            extra_env,
            null_stdin: _,
            stream_captured_output: _,
            interactive: _,
            tee_to_build_log: _,
            build_log_sink: _,
            timeout: _,
        } = opts;
        if should_null_stdin(opts) {
            cmd.stdin(std::process::Stdio::null());
        }
        if !extra_env.is_empty() {
            cmd.envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
    }

    fn log_command(&self, program: &str, args: &[&str], cwd: Option<&Path>) {
        if self.debug {
            let redacted = redact_env_args(args);
            let cmd = format!("{} {}", program, redacted.join(" "));
            if let Some(dir) = cwd {
                jackin_diagnostics::emit_debug_line(
                    "cmd",
                    &format!("cd {} && {cmd}", dir.display()),
                );
            } else {
                jackin_diagnostics::emit_debug_line("cmd", &cmd);
            }
        }
    }
}

fn should_null_stdin(opts: &RunOptions) -> bool {
    opts.null_stdin || (!opts.interactive && jackin_diagnostics::rich_terminal_owned())
}

fn record_subprocess_done(program: &str, started: Instant, status: ExitStatus) {
    jackin_diagnostics::operation_record_exit_code(status.code());
    jackin_diagnostics::active_subprocess_done(
        program,
        started.elapsed().as_millis() as u64,
        status.code(),
    );
}

/// Mask the value portion of env/build args and token-shaped freeform args.
pub fn redact_env_args(args: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if (arg == "-e" || arg == "--env" || arg == "--build-arg") && i + 1 < args.len() {
            out.push(arg.to_owned());
            let next = args[i + 1];
            match next.find('=') {
                Some(eq) => out.push(format!("{}=<redacted>", &next[..eq])),
                None => out.push(redact_arg(next)),
            }
            i += 2;
        } else if let Some(value) = arg.strip_prefix("--build-arg=") {
            match value.find('=') {
                Some(eq) => out.push(format!("--build-arg={}{}", &value[..=eq], "<redacted>")),
                None => out.push(redact_arg(arg)),
            }
            i += 1;
        } else {
            out.push(redact_arg(arg));
            i += 1;
        }
    }
    out
}

fn redact_arg(arg: &str) -> String {
    if let Some((key, _value)) = arg.split_once('=')
        && is_sensitive_arg_key(key)
    {
        return format!("{key}=<redacted>");
    }
    jackin_diagnostics::redact::redact_text(arg).into_owned()
}

fn is_sensitive_arg_key(key: &str) -> bool {
    let key = key
        .trim_start_matches('-')
        .replace(['-', '_'], "")
        .to_ascii_lowercase();
    [
        "authorization",
        "bearer",
        "token",
        "secret",
        "password",
        "passwd",
        "credential",
        "apikey",
        "accesskey",
        "privatekey",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

async fn read_process_pipe<R, W>(
    pipe: &mut R,
    stream: bool,
    sink: Option<&dyn BuildLogSink>,
    mut output: W,
) -> std::io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: std::io::Write,
{
    let mut captured = Vec::new();
    let mut buf = [0u8; 8192];
    // Partial line carried across reads so the build-log tee only ever pushes
    // complete lines (BuildKit emits CRLF; the trailing `\r` is trimmed).
    let mut line_remainder: Vec<u8> = Vec::new();
    loop {
        let n = pipe.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        if stream {
            output.write_all(&buf[..n])?;
        }
        if let Some(s) = sink {
            for &byte in &buf[..n] {
                if byte == b'\n' {
                    let line = String::from_utf8_lossy(&line_remainder);
                    s.push_line(line.trim_end_matches('\r'));
                    line_remainder.clear();
                } else {
                    line_remainder.push(byte);
                }
            }
        }
        captured.extend_from_slice(&buf[..n]);
    }
    if !line_remainder.is_empty() {
        let line = String::from_utf8_lossy(&line_remainder);
        if let Some(s) = sink {
            s.push_line(line.trim_end_matches('\r'));
        }
    }
    Ok(captured)
}

fn summarize_stderr(stderr: &[u8]) -> Option<String> {
    const MAX_CHARS: usize = 500;
    let stderr = String::from_utf8_lossy(stderr);
    let mut summary = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join("; ");
    if summary.is_empty() {
        return None;
    }
    if summary.chars().count() > MAX_CHARS {
        summary = summary.chars().take(MAX_CHARS).collect();
        summary.push_str("...");
    }
    Some(summary)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureMode {
    Normal,
    Secret,
}

async fn await_child_with_timeout(
    child: &mut tokio::process::Child,
    program: &str,
    timeout: Option<std::time::Duration>,
) -> anyhow::Result<ExitStatus> {
    match timeout {
        None => Ok(child.wait().await?),
        Some(dur) => match tokio::time::timeout(dur, child.wait()).await {
            Ok(status) => Ok(status?),
            Err(_elapsed) => {
                drop(child.kill().await);
                drop(child.wait().await);
                return Err(DockerError::CommandTimeout {
                    secs: dur.as_secs_f64(),
                    program: program.to_owned(),
                }
                .into());
            }
        },
    }
}

fn enter_process_execute(program: &str, args: &[&str]) -> jackin_diagnostics::OperationGuard {
    let redacted = redact_env_args(args).join(" ");
    jackin_diagnostics::enter_operation(
        jackin_diagnostics::otel_events::PROCESS_EXECUTE,
        &[
            (
                jackin_diagnostics::otel_keys::PROCESS_COMMAND,
                program.to_owned(),
            ),
            (
                jackin_diagnostics::otel_keys::PROCESS_ARGS_REDACTED,
                redacted,
            ),
        ],
    )
}

impl CommandRunner for ShellRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let _op_guard = enter_process_execute(program, args);
        self.log_command(program, args, cwd);

        // `interactive` must own the real terminal, so the arms below resolve it
        // before any capture arm — meaning interactive + capture silently drops
        // the capture. Catch that illegal combination in tests/debug builds.
        debug_assert!(
            !(opts.interactive && (opts.capture_stdout || opts.capture_stderr)),
            "RunOptions::interactive is mutually exclusive with capture_stdout/stderr"
        );

        if opts.interactive {
            // Interactive commands (the `docker exec -it` multiplexer / shell
            // client) must inherit the real terminal. The --debug and
            // rich-surface arms below would otherwise capture this output,
            // denying the client its TTY and blocking forever on the
            // long-lived session — so inherit stdio directly and never capture.
            let mut cmd = Self::build_command(program, args, cwd);
            Self::apply_run_opts(&mut cmd, opts);
            let started = Instant::now();
            let mut child = cmd.spawn()?;
            let status = await_child_with_timeout(&mut child, program, opts.timeout).await?;
            record_subprocess_done(program, started, status);
            if !status.success() {
                return Err(cmd_failed(program, args).into());
            }
        } else if opts.quiet {
            let mut cmd = Self::build_command(program, args, cwd);
            Self::apply_run_opts(&mut cmd, opts);
            let started = Instant::now();
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            let mut child = cmd.spawn()?;
            let status = await_child_with_timeout(&mut child, program, opts.timeout).await?;
            record_subprocess_done(program, started, status);
            if !status.success() {
                return Err(cmd_failed(program, args).into());
            }
        } else if opts.capture_stderr || opts.capture_stdout {
            Box::pin(self.run_captured(program, args, cwd, opts)).await?;
        } else if self.debug || jackin_diagnostics::rich_terminal_owned() {
            // This arm would otherwise inherit the terminal and stream raw
            // command output straight to the screen — which floods a rich TUI
            // and a --debug run. Capture both streams instead so the output
            // lands in the diagnostics file (under --debug) and never on the
            // screen.
            let captured = RunOptions {
                capture_stdout: true,
                capture_stderr: true,
                ..opts.clone()
            };
            Box::pin(self.run_captured(program, args, cwd, &captured)).await?;
        } else {
            let mut cmd = Self::build_command(program, args, cwd);
            Self::apply_run_opts(&mut cmd, opts);
            let started = Instant::now();
            let mut child = cmd.spawn()?;
            let status = await_child_with_timeout(&mut child, program, opts.timeout).await?;
            record_subprocess_done(program, started, status);
            if !status.success() {
                return Err(cmd_failed(program, args).into());
            }
        }
        Ok(())
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(program, args, cwd, CaptureMode::Normal)
            .await
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(program, args, cwd, CaptureMode::Secret)
            .await
    }
}

impl ShellRunner {
    #[expect(
        clippy::large_futures,
        reason = "ShellRunner joins wait+stdout+stderr under optional timeout; boxing adds latency without measured win"
    )]
    async fn run_captured(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let _op_guard = enter_process_execute(program, args);
        let mut cmd = Self::build_command(program, args, cwd);
        Self::apply_run_opts(&mut cmd, opts);
        if opts.capture_stdout {
            cmd.stdout(std::process::Stdio::piped());
        }
        if opts.capture_stderr {
            cmd.stderr(std::process::Stdio::piped());
        }
        let started = Instant::now();
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                jackin_diagnostics::operation_error(
                    "process_spawn_error",
                    &format!("failed to spawn {program}: {error}"),
                    &[],
                );
                return Err(error.into());
            }
        };
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        // Never stream child output to the terminal while a debug run is
        // capturing (it belongs in the diagnostics file, not the screen) or
        // while a rich full-screen TUI owns the terminal (it would corrupt
        // the frame). In both cases the output is captured and, under
        // --debug, written to the run's JSONL by `log_captured_output`.
        let stream = opts.stream_captured_output
            && !self.debug
            && !jackin_diagnostics::rich_terminal_owned();
        let (sink_out, sink_err) = (opts.build_log_sink.clone(), opts.build_log_sink.clone());
        let read_stdout = async move {
            let Some(mut stdout_pipe) = stdout_pipe else {
                return Ok::<Vec<u8>, std::io::Error>(Vec::new());
            };
            read_process_pipe(
                &mut stdout_pipe,
                stream,
                sink_out.as_deref(),
                std::io::stdout(),
            )
            .await
        };
        let read_stderr = async move {
            let Some(mut stderr_pipe) = stderr_pipe else {
                return Ok::<Vec<u8>, std::io::Error>(Vec::new());
            };
            read_process_pipe(
                &mut stderr_pipe,
                stream,
                sink_err.as_deref(),
                std::io::stderr(),
            )
            .await
        };
        let (status, stdout_result, stderr_result) = if let Some(dur) = opts.timeout {
            match tokio::time::timeout(dur, async {
                tokio::join!(child.wait(), read_stdout, read_stderr)
            })
            .await
            {
                Ok(triple) => triple,
                Err(_elapsed) => {
                    drop(child.kill().await);
                    drop(child.wait().await);
                    return Err(DockerError::CommandTimeout {
                        secs: dur.as_secs_f64(),
                        program: program.to_owned(),
                    }
                    .into());
                }
            }
        } else {
            tokio::join!(child.wait(), read_stdout, read_stderr)
        };
        let stdout_buf = stdout_result?;
        let stderr_buf = stderr_result?;
        let status = status?;
        record_subprocess_done(program, started, status);
        self.log_captured_output(program, args, &stdout_buf, &stderr_buf);
        let command = format!("{} {}", program, redact_env_args(args).join(" "));
        if opts.tee_to_build_log
            && let Some(run) = jackin_diagnostics::active_run()
        {
            let wrote = run.write_command_output(
                "docker-build",
                &command,
                cwd,
                status,
                &stdout_buf,
                &stderr_buf,
            );
            if wrote.is_none() {
                // Sidecar open failed (disk/perm/dir-gone). Land a compact
                // entry in the run jsonl so the failure is at least visible
                // to anyone reading the run afterwards.
                // `launch_failure_cli_error` itself only consults the
                // on-disk sidecar path, so without an artifact file the CLI
                // surface will still fall back to the bare error.
                run.compact(
                    "docker-build",
                    "failed to write docker-build diagnostics sidecar",
                );
            }
        }
        if !status.success() {
            if opts.tee_to_build_log {
                return Err(DockerError::DockerBuildFailed.into());
            }
            if String::from_utf8_lossy(&stderr_buf).trim().is_empty() {
                return Err(cmd_failed(program, args).into());
            }
            if !stream {
                if let Some(run) = jackin_diagnostics::active_run().filter(|_| self.debug) {
                    return Err(DockerError::CommandFailedDebugRun {
                        program: program.to_owned(),
                        args: args.join(" "),
                        run_id: run.run_id().to_owned(),
                    }
                    .into());
                }
                if let Some(run) = jackin_diagnostics::active_run() {
                    return Err(DockerError::CommandFailedSuppressed {
                        program: program.to_owned(),
                        args: args.join(" "),
                        run_id: run.run_id().to_owned(),
                    }
                    .into());
                }
                if let Some(stderr) = summarize_stderr(&stderr_buf) {
                    return Err(DockerError::CommandFailedStderrSummary {
                        program: program.to_owned(),
                        args: args.join(" "),
                        stderr,
                    }
                    .into());
                }
                return Err(DockerError::CommandFailedCapturedSuppressed {
                    program: program.to_owned(),
                    args: args.join(" "),
                }
                .into());
            }
            return Err(DockerError::CommandFailedSeeStderr {
                program: program.to_owned(),
                args: args.join(" "),
            }
            .into());
        }
        Ok(())
    }

    fn log_captured_output(&self, program: &str, args: &[&str], stdout: &[u8], stderr: &[u8]) {
        if !self.debug {
            return;
        }
        let command = format!("{} {}", program, redact_env_args(args).join(" "));
        for line in String::from_utf8_lossy(stdout).lines() {
            let line = jackin_diagnostics::scrub_secrets(line);
            jackin_diagnostics::active_debug("cmd.stdout", &format!("{command}: {line}"));
        }
        for line in String::from_utf8_lossy(stderr).lines() {
            let line = jackin_diagnostics::scrub_secrets(line);
            jackin_diagnostics::active_debug("cmd.stderr", &format!("{command}: {line}"));
        }
    }

    async fn do_capture(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        mode: CaptureMode,
    ) -> anyhow::Result<String> {
        self.log_command(program, args, cwd);
        let mut command = Self::build_command(program, args, cwd);
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if jackin_diagnostics::rich_terminal_owned() {
            command.stdin(std::process::Stdio::null());
        }
        let started = Instant::now();
        let output = command.output().await?;
        record_subprocess_done(program, started, output.status);
        if !output.status.success() {
            match mode {
                CaptureMode::Secret => {
                    return Err(cmd_failed(program, args).into());
                }
                CaptureMode::Normal => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                    if stderr.is_empty() {
                        return Err(cmd_failed(program, args).into());
                    }
                    return Err(DockerError::CommandFailedWithStderr {
                        program: program.to_owned(),
                        args: args.join(" "),
                        stderr,
                    }
                    .into());
                }
            }
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if self.debug && !stdout.is_empty() {
            match mode {
                CaptureMode::Normal => {
                    let first_line = stdout.lines().next().unwrap_or("");
                    jackin_diagnostics::emit_debug_line("cmd", &format!("-> {first_line}"));
                }
                CaptureMode::Secret => {}
            }
        }
        Ok(stdout)
    }
}

#[cfg(test)]
mod tests;
