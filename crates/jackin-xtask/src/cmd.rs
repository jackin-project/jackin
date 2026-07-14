//! Centralized subprocess helpers for jackin-xtask.
//!
//! Every xtask spawn/status/capture path routes through this module so
//! `clippy::disallowed_methods` expects live in one place and error messages
//! share one shape.

use std::ffi::OsStr;
use std::process::{Command, Output};

use anyhow::{Context, Result, anyhow};

/// Run `cmd` for status only. Errors name the command display string.
pub(crate) fn run(cmd: &mut Command) -> Result<()> {
    let display = display_command(cmd);
    // `Command::status` is not in the disallowed set; keep the spawn surface
    // centralized here with `output`/`spawn` below.
    let status = cmd.status().with_context(|| format!("running {display}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{display} failed with {status}"))
    }
}

/// Capture stdout as bytes. On failure, error includes trimmed stderr.
///
/// Routes through [`jackin_process`] when the command is a simple program+args
/// capture with inherited env; keeps `Command::output` for complex configured
/// commands (env/cwd/stdio already set on the builder).
pub(crate) fn output(cmd: &mut Command) -> Result<Vec<u8>> {
    let display = display_command(cmd);
    // Prefer shared transport for plain captures (no pre-configured stdio).
    if let Some(request) = try_exec_request(cmd) {
        return jackin_process::capture_stdout_sync(&request)
            .with_context(|| format!("running {display}"));
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "xtask automation shells out to git, gh, cargo, and mise; centralized here"
    )]
    let output = cmd.output().with_context(|| format!("running {display}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "{display} failed with {}\n{}",
            output.status,
            stderr.trim()
        ))
    }
}

/// Build a transport request when `cmd` is a plain program+args capture.
fn try_exec_request(cmd: &Command) -> Option<jackin_process::ExecRequest> {
    let program = cmd.get_program();
    if program.is_empty() {
        return None;
    }
    let args: Vec<_> = cmd.get_args().collect();
    let mut request = jackin_process::ExecRequest::new(program, args);
    if let Some(cwd) = cmd.get_current_dir() {
        request = request.cwd(cwd);
    }
    Some(request)
}

/// Capture stdout as a lossy UTF-8 owned string.
pub(crate) fn output_string(cmd: &mut Command) -> Result<String> {
    Ok(String::from_utf8_lossy(&output(cmd)?).into_owned())
}

/// Full process `Output` (stdout+stderr+status) for callers that inspect all three.
pub(crate) fn output_raw(cmd: &mut Command) -> Result<Output> {
    let display = display_command(cmd);
    #[expect(
        clippy::disallowed_methods,
        reason = "xtask automation shells out to git, gh, cargo, and mise; centralized here"
    )]
    cmd.output().with_context(|| format!("running {display}"))
}

pub(crate) fn display_command(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy();
    let args = cmd
        .get_args()
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{program} {args}")
    }
}

pub(crate) fn shell_quote(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '+'))
    {
        value.into_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

#[cfg(test)]
mod tests;
