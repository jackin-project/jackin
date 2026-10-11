// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Command execution and process-request helpers.

use std::ffi::OsStr;

use anyhow::{Context, Result, bail};

pub(crate) fn gh_auth_status_ok() -> bool {
    let request = jackin_process::ExecRequest::new("gh", ["auth", "status"])
        .stdout_mode(jackin_process::StdioMode::Null)
        .stderr_mode(jackin_process::StdioMode::Null);
    runtime_setup_request(&request).is_ok_and(|result| result.success)
}

pub(crate) fn run_command(program: &str, args: &[&str]) -> Result<()> {
    let output = runtime_setup_output(program, args.iter().copied())
        .with_context(|| format!("failed to run {}", format_command(program, args)))?;
    if output.success {
        return Ok(());
    }
    bail!(
        "{} failed with {}: {}",
        format_command(program, args),
        output
            .code
            .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

pub(crate) fn runtime_setup_output(
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<jackin_process::ExecResult> {
    runtime_setup_request(&jackin_process::ExecRequest::new(program, args))
}

pub(crate) fn runtime_setup_request(
    request: &jackin_process::ExecRequest,
) -> Result<jackin_process::ExecResult> {
    use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue};

    let executable = jackin_telemetry::process::classify_executable(&request.program);
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(executable.as_str()),
        }],
    );
    let result = jackin_process::exec_sync(request);
    let completion = match &result {
        Ok(output) => {
            if let Some(code) = output.code {
                let _attribute = operation.set_attr(jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                    value: jackin_telemetry::Value::I64(i64::from(code)),
                });
            }
            if output.timed_out {
                (OutcomeValue::Timeout, Some(ErrorType::Timeout))
            } else if output.success {
                (OutcomeValue::Success, None)
            } else {
                (OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero))
            }
        }
        Err(_) => (OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError)),
    };
    operation.complete(completion.0, completion.1);
    result
}

pub(crate) fn run_optional_command(program: &str, args: &[&str]) -> bool {
    // Use the shared telemetry resolver rather than parsing controls privately.
    let verbose = matches!(
        jackin_diagnostics::telemetry_level(false),
        jackin_diagnostics::TelemetryLevel::Debug | jackin_diagnostics::TelemetryLevel::Trace
    );
    let mode = if verbose {
        jackin_process::StdioMode::Inherit
    } else {
        jackin_process::StdioMode::Null
    };
    let request = jackin_process::ExecRequest::new(program, args.iter().copied())
        .stdout_mode(mode)
        .stderr_mode(mode);
    match runtime_setup_request(&request) {
        Ok(result) if result.success => true,
        Ok(_) | Err(_) => false,
    }
}

pub(crate) fn format_command(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}
