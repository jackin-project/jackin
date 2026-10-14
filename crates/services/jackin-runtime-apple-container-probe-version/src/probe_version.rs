// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
