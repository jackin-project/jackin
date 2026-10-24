// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Read};
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

#[must_use]
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

pub(crate) enum ChildPollOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
}

pub(crate) enum WaitOutcome {
    Exited(std::process::ExitStatus),
    Reaped,
    TimedOut,
    Failed,
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded synchronous polling supervises only this owned child outside the multiplexer render loop"
)]
pub(crate) fn poll_owned_child_until_timeout(
    child: &mut Child,
    timeout: Duration,
    poll_interval: Duration,
) -> io::Result<ChildPollOutcome> {
    if poll_interval.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "child polling interval must be nonzero",
        ));
    }

    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(ChildPollOutcome::Exited(status));
        }

        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Ok(ChildPollOutcome::TimedOut);
        }

        std::thread::sleep(poll_interval.min(remaining));
    }
}

pub(crate) fn wait_child_with_timeout(child: &mut Child, timeout: Duration) -> WaitOutcome {
    match poll_owned_child_until_timeout(child, timeout, COMMAND_PROBE_POLL_INTERVAL) {
        Ok(ChildPollOutcome::Exited(status)) => WaitOutcome::Exited(status),
        Ok(ChildPollOutcome::TimedOut) => {
            drop(child.kill());
            drop(child.wait());
            WaitOutcome::TimedOut
        }
        Err(error) if error.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
            WaitOutcome::Reaped
        }
        Err(_) => WaitOutcome::Failed,
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
    let stdout_reader =
        jackin_telemetry::spawn::thread_stream("process.stdout", move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes)?;
            Ok(bytes)
        });
    let status = match wait_child_with_timeout(&mut child, timeout) {
        WaitOutcome::Exited(status) => {
            drop(child);
            Some(status)
        }
        WaitOutcome::Reaped => {
            drop(child);
            None
        }
        WaitOutcome::TimedOut => {
            operation.complete_timeout();
            drop(child);
            drop(stdout_reader.join());
            return None;
        }
        WaitOutcome::Failed => {
            operation.complete_io_failure();
            drop(child);
            drop(stdout_reader.join());
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
