// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Managed CLI execution and output capture.

use std::io::Read;

use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::super::process_telemetry;
use super::PROCESS_OUTPUT_MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOutput {
    pub(crate) success: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) fn run_cli_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    let output = run_cli_with_timeout_full(command, args, timeout)?;
    if !output.success {
        return Err(format!(
            "{command} exited with status {:?}",
            output.exit_code
        ));
    }
    Ok(output.stdout)
}

#[expect(
    clippy::disallowed_methods,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn run_cli_with_timeout_full(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<CliOutput, String> {
    let operation = process_telemetry::ChildOperation::begin(command);
    let Ok(mut child) = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        operation.spawn_failed();
        return Err("usage command failed to start".to_owned());
    };
    let Some(stdout) = child.stdout.take() else {
        drop(child.kill());
        drop(child.wait());
        operation.io_failed();
        return Err("usage command output unavailable".to_owned());
    };
    let Some(stderr) = child.stderr.take() else {
        drop(child.kill());
        drop(child.wait());
        operation.io_failed();
        return Err("usage command output unavailable".to_owned());
    };
    let stdout_reader =
        jackin_telemetry::spawn::thread_stream("usage.stdout", move || read_process_pipe(stdout));
    let stderr_reader =
        jackin_telemetry::spawn::thread_stream("usage.stderr", move || read_process_pipe(stderr));
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output =
                    collect_cli_output(command, Some(status), stdout_reader, stderr_reader);
                match &output {
                    Ok(output) => operation.complete_status(output.exit_code, output.success),
                    Err(_) => operation.io_failed(),
                }
                return output;
            }
            Ok(None) if started.elapsed() >= timeout => {
                drop(child.kill());
                drop(child.wait());
                drop(stdout_reader.join());
                drop(stderr_reader.join());
                operation.timed_out();
                return Err("usage command timed out".to_owned());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(err) if err.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                drop(stdout_reader.join());
                drop(stderr_reader.join());
                operation.io_failed();
                return Err("usage command status unavailable".to_owned());
            }
            Err(_) => {
                drop(child.kill());
                drop(child.wait());
                drop(stdout_reader.join());
                drop(stderr_reader.join());
                operation.io_failed();
                return Err("usage command status failed".to_owned());
            }
        }
    }
}

pub(crate) fn collect_cli_output(
    command: &str,
    status: Option<ExitStatus>,
    stdout_reader: thread::JoinHandle<Result<String, String>>,
    stderr_reader: thread::JoinHandle<Result<String, String>>,
) -> Result<CliOutput, String> {
    let stdout = stdout_reader
        .join()
        .map_err(|_| format!("{command} stdout reader panicked"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| format!("{command} stderr reader panicked"))?;
    Ok(CliOutput {
        success: status.is_none_or(|status| status.success()),
        exit_code: status.and_then(|status| status.code()),
        stdout: stdout?,
        stderr: stderr?,
    })
}

pub(crate) fn read_process_pipe(mut pipe: impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut exceeded = false;
    loop {
        let count = pipe
            .read(&mut chunk)
            .map_err(|_| "process output read failed".to_owned())?;
        if count == 0 {
            break;
        }
        let remaining = PROCESS_OUTPUT_MAX.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..count.min(remaining)]);
        exceeded |= count > remaining;
    }
    if exceeded {
        return Err("process output exceeded limit".to_owned());
    }
    String::from_utf8(bytes).map_err(|_| "process output was not UTF-8".to_owned())
}
