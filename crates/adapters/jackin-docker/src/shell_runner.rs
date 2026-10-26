// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
        command.kill_on_drop(true);
        command
    }

    fn apply_run_opts(cmd: &mut Command, opts: &RunOptions) -> anyhow::Result<()> {
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
            #[cfg(unix)]
            pinned_cwd,
        } = opts;
        #[cfg(unix)]
        if let Some(directory) = pinned_cwd {
            jackin_process_directory::current_dir(
                cmd.as_std_mut(),
                std::sync::Arc::clone(directory),
            )?;
        }
        if should_null_stdin(opts) {
            cmd.stdin(std::process::Stdio::null());
        }
        if !extra_env.is_empty() {
            cmd.envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
        Ok(())
    }
}

fn should_null_stdin(opts: &RunOptions) -> bool {
    opts.null_stdin || (!opts.interactive && jackin_diagnostics::rich_terminal_owned())
}

#[derive(Debug, thiserror::Error)]
enum ProcessBoundaryError {
    #[error("process spawn failed")]
    Spawn,
    #[error("process I/O failed")]
    Io,
}

fn record_subprocess_done(
    operation: &jackin_telemetry::OperationGuard,
    program: &str,
    started: Instant,
    status: ExitStatus,
) {
    if let Some(code) = status.code() {
        let _attribute_result = operation.set_attr(jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
            value: jackin_telemetry::Value::I64(i64::from(code)),
        });
    }
    jackin_diagnostics::active_subprocess_done(
        program,
        started.elapsed().as_millis() as u64,
        status.code(),
    );
}

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
    let mut log_redactor = sink.map(|_| jackin_diagnostics::redact::StreamRedactor::default());
    loop {
        let n = pipe.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        if stream {
            output.write_all(&buf[..n])?;
        }
        if let (Some(s), Some(redactor)) = (sink, log_redactor.as_mut()) {
            for line in redactor.push_bytes(&buf[..n]) {
                s.push_line(&line);
            }
        }
        captured.extend_from_slice(&buf[..n]);
    }
    if let (Some(s), Some(redactor)) = (sink, log_redactor.as_mut()) {
        for line in redactor.finish() {
            s.push_line(&line);
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

fn summarize_build_stderr(stderr: &[u8]) -> String {
    const MAX_CHARS: usize = 500;
    let mut redactor = jackin_diagnostics::redact::StreamRedactor::default();
    let mut lines = redactor.push_bytes(stderr);
    lines.extend(redactor.finish());
    let lines: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = lines
        .iter()
        .rev()
        .take(3)
        .rev()
        .copied()
        .collect::<Vec<_>>();
    let mut summary = tail.join("; ");
    if summary.is_empty() {
        return "(no stderr captured)".to_owned();
    }
    summary = redact_local_paths(&summary);
    if summary.chars().count() > MAX_CHARS {
        summary = summary.chars().take(MAX_CHARS).collect();
        summary.push_str("...");
    }
    summary
}

fn redact_local_paths(summary: &str) -> String {
    summary
        .split_whitespace()
        .map(|token| {
            let trimmed =
                token.trim_matches(|c| matches!(c, '"' | '\'' | '(' | ')' | ',' | ';' | ':'));
            if trimmed.starts_with("/tmp/")
                || trimmed.starts_with("/private/tmp/")
                || trimmed.starts_with("/var/folders/")
                || trimmed.starts_with("~/")
            {
                "<redacted-path>"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureMode {
    Normal,
    Secret,
}

fn merge_combined_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.to_owned(),
        (true, false) => stderr.to_owned(),
        (false, false) => format!("{stdout}\n{stderr}"),
    }
}

fn captured_command_error(
    program: &str,
    args: &[&str],
    stderr: &[u8],
    mode: CaptureMode,
) -> anyhow::Error {
    if mode == CaptureMode::Secret {
        return cmd_failed(program, args).into();
    }
    let stderr = String::from_utf8_lossy(stderr).trim().to_owned();
    if stderr.is_empty() {
        cmd_failed(program, args).into()
    } else {
        DockerError::CommandFailedWithStderr {
            program: program.to_owned(),
            args: args.join(" "),
            stderr,
        }
        .into()
    }
}

async fn await_child_with_timeout(
    child: &mut tokio::process::Child,
    program: &str,
    timeout: Option<std::time::Duration>,
) -> anyhow::Result<ExitStatus> {
    match timeout {
        None => child
            .wait()
            .await
            .map_err(|_| ProcessBoundaryError::Io.into()),
        Some(dur) => match tokio::time::timeout(dur, child.wait()).await {
            Ok(status) => status.map_err(|_| ProcessBoundaryError::Io.into()),
            Err(_elapsed) => {
                drop(child.kill().await);
                drop(child.wait().await);
                Err(DockerError::CommandTimeout {
                    secs: dur.as_secs_f64(),
                    program: program.to_owned(),
                }
                .into())
            }
        },
    }
}

fn enter_process_execute(program: &str) -> jackin_telemetry::OperationGuard {
    let executable = jackin_telemetry::process::classify_executable(Path::new(program)).as_str();
    jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(executable),
        }],
    )
}

fn process_execute_completion<T>(
    result: &anyhow::Result<T>,
) -> (
    jackin_telemetry::schema::enums::OutcomeValue,
    Option<jackin_telemetry::schema::enums::ErrorType>,
) {
    match result {
        Ok(_) => (jackin_telemetry::schema::enums::OutcomeValue::Success, None),
        Err(error)
            if matches!(
                error.downcast_ref::<DockerError>(),
                Some(DockerError::CommandTimeout { .. })
            ) =>
        {
            (
                jackin_telemetry::schema::enums::OutcomeValue::Timeout,
                Some(jackin_telemetry::schema::enums::ErrorType::Timeout),
            )
        }
        Err(error)
            if matches!(
                error.downcast_ref::<DockerError>(),
                Some(
                    DockerError::CommandFailed { .. }
                        | DockerError::CommandFailedWithStderr { .. }
                        | DockerError::CommandFailedStderrSummary { .. }
                        | DockerError::CommandFailedCapturedSuppressed { .. }
                        | DockerError::CommandFailedSeeStderr { .. }
                        | DockerError::DockerBuildFailed { .. }
                )
            ) =>
        {
            (
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(jackin_telemetry::schema::enums::ErrorType::ProcessExitNonzero),
            )
        }
        Err(error)
            if matches!(
                error.downcast_ref::<ProcessBoundaryError>(),
                Some(ProcessBoundaryError::Io)
            ) =>
        {
            (
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(jackin_telemetry::schema::enums::ErrorType::IoError),
            )
        }
        Err(_) => (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::ProcessSpawnError),
        ),
    }
}

fn complete_process_execute<T>(
    operation: jackin_telemetry::OperationGuard,
    result: &anyhow::Result<T>,
) {
    let (outcome, error_type) = process_execute_completion(result);
    operation.complete(outcome, error_type);
}

impl CommandRunner for ShellRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let op_guard = enter_process_execute(program);
        let result = async {
            debug_assert!(
                !(opts.interactive && (opts.capture_stdout || opts.capture_stderr)),
                "RunOptions::interactive is mutually exclusive with capture_stdout/stderr"
            );

            if opts.interactive {
                let mut cmd = Self::build_command(program, args, cwd);
                Self::apply_run_opts(&mut cmd, opts)?;
                let started = Instant::now();
                let mut child = cmd.spawn().map_err(|_| ProcessBoundaryError::Spawn)?;
                let status = await_child_with_timeout(&mut child, program, opts.timeout).await?;
                record_subprocess_done(&op_guard, program, started, status);
                if !status.success() {
                    return Err(cmd_failed(program, args).into());
                }
            } else if opts.quiet {
                let mut cmd = Self::build_command(program, args, cwd);
                Self::apply_run_opts(&mut cmd, opts)?;
                let started = Instant::now();
                cmd.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                let mut child = cmd.spawn().map_err(|_| ProcessBoundaryError::Spawn)?;
                let status = await_child_with_timeout(&mut child, program, opts.timeout).await?;
                record_subprocess_done(&op_guard, program, started, status);
                if !status.success() {
                    return Err(cmd_failed(program, args).into());
                }
            } else if opts.capture_stderr || opts.capture_stdout {
                Box::pin(self.run_captured(&op_guard, program, args, cwd, opts)).await?;
            } else if self.debug || jackin_diagnostics::rich_terminal_owned() {
                let captured = RunOptions {
                    capture_stdout: true,
                    capture_stderr: true,
                    ..opts.clone()
                };
                Box::pin(self.run_captured(&op_guard, program, args, cwd, &captured)).await?;
            } else {
                let mut cmd = Self::build_command(program, args, cwd);
                Self::apply_run_opts(&mut cmd, opts)?;
                let started = Instant::now();
                let mut child = cmd.spawn().map_err(|_| ProcessBoundaryError::Spawn)?;
                let status = await_child_with_timeout(&mut child, program, opts.timeout).await?;
                record_subprocess_done(&op_guard, program, started, status);
                if !status.success() {
                    return Err(cmd_failed(program, args).into());
                }
            }
            Ok(())
        }
        .await;
        complete_process_execute(op_guard, &result);
        result
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(
            program,
            args,
            cwd,
            &RunOptions::default(),
            CaptureMode::Normal,
            false,
        )
        .await
    }

    async fn capture_with_options(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<String> {
        self.do_capture(program, args, cwd, opts, CaptureMode::Normal, false)
            .await
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(
            program,
            args,
            cwd,
            &RunOptions::default(),
            CaptureMode::Secret,
            false,
        )
        .await
    }

    async fn capture_combined(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.do_capture(
            program,
            args,
            cwd,
            &RunOptions::default(),
            CaptureMode::Normal,
            true,
        )
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
        op_guard: &jackin_telemetry::OperationGuard,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let mut cmd = Self::build_command(program, args, cwd);
        Self::apply_run_opts(&mut cmd, opts)?;
        if opts.capture_stdout {
            cmd.stdout(std::process::Stdio::piped());
        }
        if opts.capture_stderr {
            cmd.stderr(std::process::Stdio::piped());
        }
        let started = Instant::now();
        let Ok(mut child) = cmd.spawn() else {
            return Err(ProcessBoundaryError::Spawn.into());
        };
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
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
        stdout_result.map_err(|_| ProcessBoundaryError::Io)?;
        let stderr_buf = stderr_result.map_err(|_| ProcessBoundaryError::Io)?;
        let status = status.map_err(|_| ProcessBoundaryError::Io)?;
        record_subprocess_done(op_guard, program, started, status);
        if !status.success() {
            if opts.tee_to_build_log {
                let stderr = summarize_build_stderr(&stderr_buf);
                return Err(DockerError::DockerBuildFailed { stderr }.into());
            }
            if String::from_utf8_lossy(&stderr_buf).trim().is_empty() {
                return Err(cmd_failed(program, args).into());
            }
            if !stream {
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

    async fn do_capture(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
        mode: CaptureMode,
        combined: bool,
    ) -> anyhow::Result<String> {
        let operation = enter_process_execute(program);
        let result = async {
            let mut command = Self::build_command(program, args, cwd);
            Self::apply_run_opts(&mut command, opts)?;
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            if jackin_diagnostics::rich_terminal_owned() {
                command.stdin(std::process::Stdio::null());
            }
            let started = Instant::now();
            let child = command.spawn().map_err(|_| ProcessBoundaryError::Spawn)?;
            let output = match opts.timeout {
                Some(duration) => tokio::time::timeout(duration, child.wait_with_output())
                    .await
                    .map_err(|_| DockerError::CommandTimeout {
                        secs: duration.as_secs_f64(),
                        program: program.to_owned(),
                    })?
                    .map_err(|_| ProcessBoundaryError::Io)?,
                None => child
                    .wait_with_output()
                    .await
                    .map_err(|_| ProcessBoundaryError::Io)?,
            };
            record_subprocess_done(&operation, program, started, output.status);
            if !output.status.success() {
                return Err(captured_command_error(program, args, &output.stderr, mode));
            }
            if combined {
                Ok(merge_combined_output(&output.stdout, &output.stderr))
            } else {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
            }
        }
        .await;
        complete_process_execute(operation, &result);
        result
    }
}
