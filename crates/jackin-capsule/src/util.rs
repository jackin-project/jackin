// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared utilities for the capsule: bounded file reads, child-process
//! helpers, and small formatting utilities used across modules.
//!
//! Not responsible for: protocol encoding, session management, or rendering.

use std::io::Read;
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

/// Cap reads against text metadata files so a corrupt or hostile file
/// cannot pin daemon memory while parsing branch state or hostnames.
/// `label` is a static tag so governed DEBUG events traces name which call site
/// hit the cap or failed.
pub fn read_text_bounded(path: &Path, max_bytes: u64) -> Option<String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "bounded metadata reads are small, synchronous capsule-side helpers outside render emission"
    )]
    let Ok(file) = std::fs::File::open(path) else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        return None;
    };
    let mut buf = String::new();
    let read = file.take(max_bytes).read_to_string(&mut buf);
    if read.is_err() {
        let _warning = jackin_telemetry::record_recovered_degradation();
        return None;
    }
    if buf.len() as u64 == max_bytes {
        let _warning = jackin_telemetry::record_recovered_degradation();
    }
    Some(buf)
}

const COMMAND_PROBE_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Outcome of polling a spawned `Child` to completion with a deadline.
/// Callers translate this into their own result/Option/bool shape.
pub(crate) enum WaitOutcome {
    Exited(std::process::ExitStatus),
    /// The kernel reaped the child out from under us (PID 1's zombie
    /// reaper inside Capsule, or a sibling thread's `waitpid`). The
    /// exit status is lost; callers that captured stdout/stderr should
    /// trust those pipes, and presence-probes can treat the spawn
    /// itself as proof the executable exists.
    Reaped,
    /// Timed out before the child finished. The helper has already
    /// attempted `kill()` + `wait()` (best-effort) before returning.
    TimedOut,
    /// `try_wait` itself returned a non-`ECHILD` error.
    Failed,
}

/// Poll `child.try_wait()` at `COMMAND_PROBE_POLL_INTERVAL` until it
/// finishes, the kernel reaps it, the deadline fires, or `try_wait`
/// itself errors.
pub(crate) fn wait_child_with_timeout(child: &mut Child, timeout: Duration) -> WaitOutcome {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Exited(status),
            Ok(None) => {}
            Err(e) if e.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                return WaitOutcome::Reaped;
            }
            Err(_) => return WaitOutcome::Failed,
        }
        if started.elapsed() >= timeout {
            drop(child.kill());
            drop(child.wait());
            return WaitOutcome::TimedOut;
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "command probe waits on an owned child process outside the multiplexer render loop"
        )]
        std::thread::sleep(COMMAND_PROBE_POLL_INTERVAL);
    }
}

pub(crate) fn command_stdout_trimmed_with_timeout(
    request: &jackin_process::ExecRequest,
    timeout: Duration,
) -> Option<String> {
    let Ok((operation, mut child)) = crate::process_telemetry::spawn_sync(request) else {
        return None;
    };
    let Some(mut stdout) = child.stdout.take() else {
        operation.complete_io_failure();
        return None;
    };
    let stdout_reader = jackin_telemetry::spawn::thread_stream(
        "process.stdout",
        move || -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes)?;
            Ok(bytes)
        },
    );
    let status = match wait_child_with_timeout(&mut child, timeout) {
        WaitOutcome::Exited(status) => Some(status),
        // Status is lost; trust the stdout pipe (callers like the
        // Container info dialog would otherwise show empty fields for
        // healthy git/gh commands).
        WaitOutcome::Reaped => None,
        WaitOutcome::TimedOut => {
            // Joining the reader is bounded: kill() (inside the helper)
            // closed the pipe, so read_to_end returns quickly. Without
            // the join the OS-thread is leaked across every timeout
            // firing.
            drop(stdout_reader.join());
            operation.complete_timeout();
            return None;
        }
        WaitOutcome::Failed => {
            drop(stdout_reader.join());
            operation.complete_io_failure();
            return None;
        }
    };
    let status = match status {
        Some(status) if !status.success() => {
            operation.complete_status(status, &[0]);
            return None;
        }
        status => status,
    };
    let stdout = match stdout_reader.join() {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => {
            operation.complete_io_failure();
            return None;
        }
        Err(_) => {
            operation.complete_io_failure();
            return None;
        }
    };
    if let Some(status) = status {
        operation.complete_status(status, &[0]);
    } else {
        operation.complete_reaped();
    }
    let value = String::from_utf8_lossy(&stdout).trim().to_owned();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests;
