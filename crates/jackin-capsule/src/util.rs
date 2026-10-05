// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared utilities for the capsule: bounded file reads, child-process
//! helpers, and small formatting utilities used across modules.
//!
//! Not responsible for: protocol encoding, session management, or rendering.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// Cap reads against text metadata files so a corrupt or hostile file
/// cannot pin daemon memory while parsing branch state or hostnames.
/// `label` is a static tag so governed DEBUG events traces name which call site
/// hit the cap or failed.
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

pub(crate) fn command_stdout_trimmed_with_timeout(
    request: &jackin_process::ExecRequest,
    timeout: Duration,
) -> Option<String> {
    // The central executor owns the group and one deadline through both pipe
    // EOF and leader completion. A descendant retaining stdout cannot extend
    // this probe beyond its deadline; overflow rejects the whole response.
    let mut request = request.clone().timeout(timeout);
    request.retry = jackin_process::RetryPolicy::none();
    request.stdin = None;
    request.stdin_mode = jackin_process::StdioMode::Null;
    request.stdout_mode = jackin_process::StdioMode::Capture;
    request.stdout_limit = Some(64 * 1024);
    if request.stderr_mode == jackin_process::StdioMode::Capture {
        request.stderr_limit = Some(4 * 1024);
    } else {
        request.stderr_limit = None;
    }
    let output = crate::process_telemetry::exec_sync(&request).ok()?;
    if output.timed_out || !output.success {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests;
