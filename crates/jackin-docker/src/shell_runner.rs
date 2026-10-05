// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ShellRunner`: concrete subprocess implementation of `CommandRunner`.
//!
//! The `CommandRunner` trait and `RunOptions` are re-exported from
//! `jackin-core` so consumer crates depend on the trait, not this
//! tokio-based implementation.
//!
//! Not responsible for: the async Docker daemon API (`docker_client.rs`), or
//! parsing Docker output formats (those live in the callers).

use crate::DockerError;
use jackin_telemetry::process::ProcessOperationGuard;
use std::path::Path;
use std::process::ExitStatus;
use std::time::Instant;
use tokio::io::AsyncReadExt;

pub use jackin_core::{BuildLogSink, CommandRunner, RunOptions};

// Error values retain only the closed executable vocabulary. Arbitrary argv
// and executable paths never enter Display, Debug, or error sources.
fn safe_program(program: &str) -> String {
    jackin_telemetry::process::classify_executable(Path::new(program))
        .as_str()
        .to_owned()
}

fn cmd_failed(program: &str) -> DockerError {
    DockerError::CommandFailed {
        program: safe_program(program),
    }
}

#[derive(Debug, Default)]
pub struct ShellRunner {
    pub debug: bool,
}

impl ShellRunner {
    fn build_request(
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> jackin_process::ExecRequest {
        // Every RunOptions field receives an explicit boundary disposition.
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
        let mut request = jackin_process::ExecRequest::new(program, args.iter().copied())
            .envs(extra_env.iter().map(|(key, value)| (key, value)))
            .stdin_mode(if should_null_stdin(opts) {
                jackin_process::StdioMode::Null
            } else {
                jackin_process::StdioMode::Inherit
            })
            .stdout_mode(jackin_process::StdioMode::Inherit)
            .stderr_mode(jackin_process::StdioMode::Inherit);
        request.cwd = cwd.map(Path::to_path_buf);
        #[cfg(unix)]
        {
            request.pinned_cwd = pinned_cwd.as_ref().map(std::sync::Arc::clone);
        }
        request
    }
}

fn should_null_stdin(opts: &RunOptions) -> bool {
    opts.null_stdin || (!opts.interactive && jackin_diagnostics::rich_terminal_owned())
}

#[derive(Debug, thiserror::Error)]
enum ProcessBoundaryError {
    #[error("interactive commands cannot capture standard output or error")]
    InvalidOptions,
    #[error("process spawn failed")]
    Spawn,
    #[error("process I/O failed")]
    Io,
    #[error("terminal is already owned by another active surface")]
    TerminalBusy,
    #[cfg(unix)]
    #[error("terminal restoration failed")]
    TerminalRestore,
}

fn process_io_error(error: &std::io::Error) -> ProcessBoundaryError {
    #[cfg(unix)]
    if error.get_ref().is_some_and(|error| {
        error
            .downcast_ref::<jackin_process_directory::ForegroundRestoreError>()
            .is_some()
    }) {
        return ProcessBoundaryError::TerminalRestore;
    }
    let _error = error;
    ProcessBoundaryError::Io
}

fn process_boundary_error(error: anyhow::Error) -> anyhow::Error {
    match error.downcast_ref::<jackin_process::ExecStage>() {
        Some(jackin_process::ExecStage::Setup) => anyhow::anyhow!("{}", error.root_cause()),
        Some(jackin_process::ExecStage::Spawn) => ProcessBoundaryError::Spawn.into(),
        None => ProcessBoundaryError::Io.into(),
    }
}

fn record_subprocess_done(
    operation: &jackin_telemetry::OperationGuard,
    program: &str,
    started: Instant,
    status: ExitStatus,
) {
    record_subprocess_result(operation, program, started.elapsed(), status.code());
}

fn record_subprocess_result(
    operation: &jackin_telemetry::OperationGuard,
    program: &str,
    duration: std::time::Duration,
    code: Option<i32>,
) {
    if let Some(code) = code {
        let _attribute_result = operation.set_attr(jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
            value: jackin_telemetry::Value::I64(i64::from(code)),
        });
    }
    jackin_diagnostics::active_subprocess_done(program, duration.as_millis() as u64, code);
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
    // Keep incomplete lines across reads. Neither terminal output nor the
    // retained sink may observe a fragment before redaction has its full
    // assignment/token context.
    let mut line_remainder: Vec<u8> = Vec::new();
    // Quoted credentials and PEM blocks can span lines. Hold those lines
    // until the shared redactor says their full sensitive span is available.
    let mut redaction_remainder = String::new();
    loop {
        let n = pipe.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        // Enforce the shared transport budget before either retained output or
        // an unfinished build-log line can grow. Overflow is a failure, never
        // silently truncated successful command output.
        if n > jackin_process::DEFAULT_CAPTURE_LIMIT.saturating_sub(captured.len()) {
            return Err(std::io::Error::other("process output limit exceeded"));
        }
        for &byte in &buf[..n] {
            line_remainder.push(byte);
            if byte == b'\n' {
                redaction_remainder.push_str(&String::from_utf8_lossy(&line_remainder));
                line_remainder.clear();
                let safe_len =
                    jackin_diagnostics::redact::safe_complete_prefix_len(&redaction_remainder);
                if safe_len > 0 {
                    let safe = redaction_remainder[..safe_len].to_owned();
                    redaction_remainder.drain(..safe_len);
                    emit_redacted_build_output(&safe, stream, sink, &mut output)?;
                }
            }
        }
        captured.extend_from_slice(&buf[..n]);
    }
    if !line_remainder.is_empty() {
        redaction_remainder.push_str(&String::from_utf8_lossy(&line_remainder));
    }
    if !redaction_remainder.is_empty() {
        emit_redacted_build_output(&redaction_remainder, stream, sink, &mut output)?;
    }
    if stream {
        output.flush()?;
    }
    Ok(captured)
}

fn emit_redacted_build_output<W: std::io::Write>(
    text: &str,
    stream: bool,
    sink: Option<&dyn BuildLogSink>,
    output: &mut W,
) -> std::io::Result<()> {
    let redacted = jackin_diagnostics::redact::redact_text(text);
    if stream {
        output.write_all(redacted.as_bytes())?;
    }
    if let Some(sink) = sink {
        for line in redacted.split_inclusive('\n') {
            let line = line.strip_suffix('\n').unwrap_or(line);
            sink.push_line(line.trim_end_matches('\r'));
        }
    }
    Ok(())
}

/// Strip request data that a child may echo before applying pattern redaction.
/// These values are already known to the boundary; unknown credential formats
/// need no heuristic when they came from argv or the explicit environment.
fn sanitize_error_stderr(
    stderr: &[u8],
    program: &str,
    args: &[&str],
    opts: &RunOptions,
    cwd: Option<&Path>,
) -> Vec<u8> {
    let text = String::from_utf8_lossy(stderr);
    // Preserve assignment and PEM structure for pattern recognition before
    // request values can replace credential names or delimiters.
    let mut text = jackin_diagnostics::redact::redact_text(&text).into_owned();
    let mut private_values = args.iter().copied().collect::<Vec<_>>();
    private_values.push(program);
    private_values.extend(
        args.iter()
            .filter_map(|arg| arg.split_once('=').map(|(_, value)| value)),
    );
    private_values.extend(opts.extra_env.iter().map(|(_, value)| value.as_str()));
    private_values.extend(cwd.and_then(Path::to_str));
    // Longest first prevents one argument from exposing a suffix of another.
    private_values.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
    for value in private_values {
        if !value.is_empty() {
            text = text.replace(value, "<redacted>");
        }
    }
    text.into_bytes()
}

fn summarize_stderr(stderr: &[u8]) -> Option<String> {
    const MAX_CHARS: usize = 500;
    let text = String::from_utf8_lossy(stderr);
    // Redact before line selection and truncation: those operations can sever
    // a credential assignment or a multiline private key.
    let stderr = jackin_diagnostics::redact::redact_text(&text);
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
    summary = redact_local_paths(&summary);
    if summary.chars().count() > MAX_CHARS {
        summary = summary.chars().take(MAX_CHARS).collect();
        summary.push_str("...");
    }
    Some(summary)
}

/// Summarize a failed build's stderr for the CLI error: the LAST non-empty
/// lines (`BuildKit` reports the cause at the end, unlike the preamble
/// `summarize_stderr` takes from the front), with local temp paths
/// redacted so launch errors stay free of machine-specific paths.
fn summarize_build_stderr(stderr: &[u8]) -> String {
    const MAX_CHARS: usize = 500;
    let text = String::from_utf8_lossy(stderr);
    let text = jackin_diagnostics::redact::redact_text(&text);
    let lines: Vec<&str> = text
        .lines()
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

/// Scrub whitespace-delimited tokens containing path separators. Docker
/// build errors routinely name the ephemeral context directory; the
/// operator's terminal may show the failure, but the error value itself
/// must not carry machine-specific paths.
fn redact_local_paths(summary: &str) -> String {
    summary
        .split_whitespace()
        .map(|token| {
            let trimmed =
                token.trim_matches(|c| matches!(c, '"' | '\'' | '(' | ')' | ',' | ';' | ':'));
            // Paths can be embedded after assignment keys, URI prefixes,
            // quotes, or punctuation. Omit the whole path-bearing token.
            if trimmed.contains('/') || trimmed.contains('\\') {
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

/// Merge stdout+stderr with `2>&1` semantics: each stream trimmed, joined
/// with a newline when both are non-empty. `wait_with_output` cannot
/// recover chronological interleaving, so stdout leads; empty streams
/// contribute nothing (no stray blank line).
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

fn captured_command_error(program: &str, stderr: &[u8], mode: CaptureMode) -> anyhow::Error {
    if mode == CaptureMode::Secret {
        return cmd_failed(program).into();
    }
    let Some(stderr) = summarize_stderr(stderr) else {
        return cmd_failed(program).into();
    };
    if stderr.is_empty() {
        cmd_failed(program).into()
    } else {
        DockerError::CommandFailedWithStderr {
            program: safe_program(program),
            stderr,
        }
        .into()
    }
}

async fn await_group_with_timeout(
    child: jackin_process::GroupChild,
    program: &str,
    timeout: Option<std::time::Duration>,
) -> anyhow::Result<ExitStatus> {
    let status = match timeout {
        None => Some(
            child
                .finish()
                .await
                .map_err(|error| process_io_error(&error))?,
        ),
        Some(duration) => child
            .finish_with_timeout(duration)
            .await
            .map_err(|error| process_io_error(&error))?,
    };
    status.ok_or_else(|| {
        DockerError::CommandTimeout {
            secs: timeout.map_or(0.0, |duration| duration.as_secs_f64()),
            program: safe_program(program),
        }
        .into()
    })
}

fn enter_process_execute(program: &str) -> ProcessOperationGuard {
    let executable = jackin_telemetry::process::classify_executable(Path::new(program)).as_str();
    ProcessOperationGuard::new(jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(executable),
        }],
    ))
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

fn complete_process_execute<T>(operation: ProcessOperationGuard, result: &anyhow::Result<T>) {
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
            // Reject contradictory ownership before claiming the terminal or
            // starting a child in every build configuration.
            if opts.interactive && (opts.capture_stdout || opts.capture_stderr) {
                return Err(ProcessBoundaryError::InvalidOptions.into());
            }

            if opts.interactive {
                // Interactive commands (the `docker exec -it` multiplexer / shell
                // client) must inherit the real terminal. The --debug and
                // rich-surface arms below would otherwise capture this output,
                // denying the client its TTY and blocking forever on the
                // long-lived session — so inherit stdio directly and never capture.
                let request = Self::build_request(program, args, cwd, opts);
                let started = Instant::now();
                let external = jackin_diagnostics::claim_external_terminal()
                    .map_err(|_| ProcessBoundaryError::TerminalBusy)?;
                let activity = external.activity();
                let child = jackin_process::spawn_foreground_async(
                    &request,
                    activity.serialization_gate(),
                    move || drop(external),
                )
                .map_err(process_boundary_error)?;
                let status = await_group_with_timeout(child, program, opts.timeout).await?;
                record_subprocess_done(&op_guard, program, started, status);
                if !status.success() {
                    return Err(cmd_failed(program).into());
                }
            } else if opts.quiet {
                let mut request = Self::build_request(program, args, cwd, opts);
                let started = Instant::now();
                request.stdout_mode = jackin_process::StdioMode::Null;
                request.stderr_mode = jackin_process::StdioMode::Null;
                let child =
                    jackin_process::spawn_group_async(&request).map_err(process_boundary_error)?;
                let status = await_group_with_timeout(child, program, opts.timeout).await?;
                record_subprocess_done(&op_guard, program, started, status);
                if !status.success() {
                    return Err(cmd_failed(program).into());
                }
            } else if opts.capture_stderr || opts.capture_stdout {
                Box::pin(self.run_captured(&op_guard, program, args, cwd, opts)).await?;
            } else if self.debug || jackin_diagnostics::rich_terminal_owned() {
                // This arm would otherwise inherit the terminal and stream raw
                // command output straight to the screen — which floods a rich TUI
                // and a --debug run. Capture both streams instead so raw output
                // never corrupts the screen or enters telemetry.
                let captured = RunOptions {
                    capture_stdout: true,
                    capture_stderr: true,
                    ..opts.clone()
                };
                Box::pin(self.run_captured(&op_guard, program, args, cwd, &captured)).await?;
            } else {
                let request = Self::build_request(program, args, cwd, opts);
                let started = Instant::now();
                let child =
                    jackin_process::spawn_group_async(&request).map_err(process_boundary_error)?;
                let status = await_group_with_timeout(child, program, opts.timeout).await?;
                record_subprocess_done(&op_guard, program, started, status);
                if !status.success() {
                    return Err(cmd_failed(program).into());
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
        let mut request = Self::build_request(program, args, cwd, opts);
        if opts.capture_stdout {
            request.stdout_mode = jackin_process::StdioMode::Capture;
        }
        if opts.capture_stderr {
            request.stderr_mode = jackin_process::StdioMode::Capture;
        }
        let started = Instant::now();
        let mut child =
            jackin_process::spawn_group_async(&request).map_err(process_boundary_error)?;
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        // Never stream child output while debug handling or a rich full-screen
        // TUI owns the terminal because it would corrupt the frame. Captured
        // output is deliberately not emitted as telemetry:
        // command output and arguments may contain user or provider data.
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
        let read_output = async {
            // Keep the direct-child PID reserved until descendants release
            // captured pipes. Central group ownership stays armed throughout.
            tokio::try_join!(read_stdout, read_stderr)
        };
        let output_result = if let Some(dur) = opts.timeout {
            match tokio::time::timeout(dur, read_output).await {
                Ok(output) => output,
                Err(_elapsed) => {
                    child
                        .kill_and_reap()
                        .await
                        .map_err(|_| ProcessBoundaryError::Io)?;
                    return Err(DockerError::CommandTimeout {
                        secs: dur.as_secs_f64(),
                        program: safe_program(program),
                    }
                    .into());
                }
            }
        } else {
            read_output.await
        };
        let (_stdout, stderr_buf) = match output_result {
            Ok(output) => output,
            Err(_) => {
                // try_join drops the other readers immediately. Reap the
                // exact spawned child before returning transport failure.
                child
                    .kill_and_reap()
                    .await
                    .map_err(|_| ProcessBoundaryError::Io)?;
                return Err(ProcessBoundaryError::Io.into());
            }
        };
        let stderr_buf = sanitize_error_stderr(&stderr_buf, program, args, opts, cwd);
        let remaining_timeout = opts
            .timeout
            .map(|duration| duration.saturating_sub(started.elapsed()));
        let status = await_group_with_timeout(child, program, remaining_timeout)
            .await
            .map_err(|error| {
                if matches!(
                    error.downcast_ref::<DockerError>(),
                    Some(DockerError::CommandTimeout { .. })
                ) {
                    DockerError::CommandTimeout {
                        secs: opts.timeout.map_or(0.0, |duration| duration.as_secs_f64()),
                        program: safe_program(program),
                    }
                    .into()
                } else {
                    error
                }
            })?;
        record_subprocess_done(op_guard, program, started, status);
        if !status.success() {
            if opts.tee_to_build_log {
                // The full output went to the in-memory build log (visible
                // in the cockpit while it lives), but a fatal error must be
                // self-describing: the cockpit is gone by the time the
                // operator reads it. Carry the redacted tail.
                let stderr = summarize_build_stderr(&stderr_buf);
                return Err(DockerError::DockerBuildFailed { stderr }.into());
            }
            if String::from_utf8_lossy(&stderr_buf).trim().is_empty() {
                return Err(cmd_failed(program).into());
            }
            if !stream {
                if let Some(stderr) = summarize_stderr(&stderr_buf) {
                    return Err(DockerError::CommandFailedStderrSummary {
                        program: safe_program(program),
                        stderr,
                    }
                    .into());
                }
                return Err(DockerError::CommandFailedCapturedSuppressed {
                    program: safe_program(program),
                }
                .into());
            }
            return Err(DockerError::CommandFailedSeeStderr {
                program: safe_program(program),
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
            let mut request = jackin_process::ExecRequest::new(program, args.iter().copied())
                .envs(opts.extra_env.iter().map(|(key, value)| (key, value)))
                .stdin_mode(
                    if should_null_stdin(opts) || jackin_diagnostics::rich_terminal_owned() {
                        jackin_process::StdioMode::Null
                    } else {
                        jackin_process::StdioMode::Inherit
                    },
                );
            request.cwd = cwd.map(Path::to_path_buf);
            request.timeout = opts.timeout;
            #[cfg(unix)]
            {
                request.pinned_cwd = opts.pinned_cwd.as_ref().map(std::sync::Arc::clone);
            }
            let output = jackin_process::exec_async(&request)
                .await
                .map_err(process_boundary_error)?;
            if output.timed_out {
                return Err(DockerError::CommandTimeout {
                    secs: opts.timeout.map_or(0.0, |duration| duration.as_secs_f64()),
                    program: safe_program(program),
                }
                .into());
            }
            record_subprocess_result(&operation, program, output.duration, output.code);
            if !output.success {
                return Err(captured_command_error(
                    program,
                    &sanitize_error_stderr(&output.stderr, program, args, opts, cwd),
                    mode,
                ));
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

#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
mod foreground_tests;
