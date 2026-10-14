// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, Result};
use jackin_core::container_paths;

pub async fn attach(container_name: &str, focus_session: Option<u64>) -> Result<Option<i32>> {
    let mut args: Vec<&str> = vec![
        "exec",
        "--user",
        jackin_runtime_identity::identity::CAPSULE_SUPERVISOR_USER,
        "-it",
        container_name,
        container_paths::CAPSULE_BIN,
    ];

    let focus_str;
    if let Some(id) = focus_session {
        focus_str = id.to_string();
        args.push("--focus");
        args.push(&focus_str);
    }

    let request = jackin_process::ExecRequest::new("container", &args)
        .stdin_mode(jackin_process::StdioMode::Inherit)
        .stdout_mode(jackin_process::StdioMode::Inherit)
        .stderr_mode(jackin_process::StdioMode::Inherit);
    let status = jackin_runtime_process_telemetry::process_telemetry::exec_async(&request)
        .await
        .context("container exec failed — is apple/container installed?")?;

    jackin_diagnostics::reassert_alt_screen();
    Ok(status.code)
}
