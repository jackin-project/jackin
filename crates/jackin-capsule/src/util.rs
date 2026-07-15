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
/// `label` is a static tag so `cdebug!` traces name which call site
/// hit the cap or failed.
pub fn read_text_bounded(label: &'static str, path: &Path, max_bytes: u64) -> Option<String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "bounded metadata reads are small, synchronous capsule-side helpers outside render emission"
    )]
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            crate::cdebug!(
                "read_text_bounded[{label}]: open {} failed: {e} (errno={:?})",
                path.display(),
                e.raw_os_error()
            );
            return None;
        }
    };
    let mut buf = String::new();
    let read = file.take(max_bytes).read_to_string(&mut buf);
    if let Err(e) = read {
        crate::cdebug!(
            "read_text_bounded[{label}]: read {} failed: {e} (errno={:?})",
            path.display(),
            e.raw_os_error()
        );
        return None;
    }
    if buf.len() as u64 == max_bytes {
        crate::cdebug!(
            "read_text_bounded[{label}]: capped at {max_bytes} bytes; file {} likely larger and downstream parsing may fail",
            path.display()
        );
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
    Failed(std::io::Error),
}

/// Poll `child.try_wait()` at `COMMAND_PROBE_POLL_INTERVAL` until it
/// finishes, the kernel reaps it, the deadline fires, or `try_wait`
/// itself errors. `label` is only used in the "kill after timeout
/// failed" log so the line names the program that lingered.
pub(crate) fn wait_child_with_timeout(
    child: &mut Child,
    label: &str,
    timeout: Duration,
) -> WaitOutcome {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Exited(status),
            Ok(None) => {}
            Err(e) if e.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                return WaitOutcome::Reaped;
            }
            Err(e) => return WaitOutcome::Failed(e),
        }
        if started.elapsed() >= timeout {
            if let Err(e) = child.kill() {
                crate::clog!(
                    "{label}: timeout ({timeout:?}) and child.kill() failed: {e} (errno={:?})",
                    e.raw_os_error()
                );
            }
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
    let mut child = match jackin_process::spawn_sync(request) {
        Ok(child) => child,
        Err(e) => {
            crate::clog!("command spawn failed ({}): {e}", request.program.display());
            return None;
        }
    };
    let mut stdout = child.stdout.take()?;
    let stdout_reader = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    let label = request.program.display().to_string();
    let status_success: Option<bool> = match wait_child_with_timeout(&mut child, &label, timeout) {
        WaitOutcome::Exited(status) => Some(status.code() == Some(0)),
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
            return None;
        }
        WaitOutcome::Failed(e) => {
            crate::clog!(
                "command try_wait failed ({}): {e} (errno={:?})",
                request.program.display(),
                e.raw_os_error()
            );
            drop(stdout_reader.join());
            return None;
        }
    };
    if status_success == Some(false) {
        crate::cdebug!(
            "command exited non-accepted status ({}); stderr was nulled so reason is unavailable",
            request.program.display()
        );
        return None;
    }
    let stdout = match stdout_reader.join() {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => {
            crate::clog!(
                "command stdout read failed: {e} (errno={:?})",
                e.raw_os_error()
            );
            return None;
        }
        Err(_) => {
            crate::clog!("command stdout reader thread panicked");
            return None;
        }
    };
    let value = String::from_utf8_lossy(&stdout).trim().to_owned();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests;
