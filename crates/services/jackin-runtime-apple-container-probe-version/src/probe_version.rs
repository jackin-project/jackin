// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `container` CLI version probe for the apple-container backend.
//!
//! [`probe_version`] shells out to `container --version` through
//! the shared process transport and returns the trimmed version
//! string, or `None` when the CLI is missing or exits
//! unsuccessfully — the `launch` path bails with the install
//! hint on `None`.

/// Probe the `container` CLI version. Returns `None` if not installed.
pub async fn probe_version() -> Option<String> {
    let output = jackin_runtime_process_telemetry::process_telemetry::exec_async(
        &jackin_process::ExecRequest::new("container", ["--version"]),
    )
    .await
    .ok()?;
    if output.success {
        let v = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Some(v)
    } else {
        None
    }
}
