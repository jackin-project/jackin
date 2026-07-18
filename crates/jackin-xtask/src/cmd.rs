//! Centralized subprocess helpers for jackin-xtask.
//!
//! Every xtask spawn/status/capture path routes through this module so
//! `clippy::disallowed_methods` expects live in one place and error messages
//! share one shape.

use std::ffi::OsStr;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

/// Run `cmd` for status only. Errors name the command display string.
pub(crate) fn run(cmd: &mut Command) -> Result<()> {
    let display = display_command(cmd);
    let request = exec_request(cmd);
    let result =
        jackin_process::exec_sync(&request).with_context(|| format!("running {display}"))?;
    if result.success {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(anyhow!(
            "{display} failed with code {:?}\n{}",
            result.code,
            stderr.trim()
        ))
    }
}

/// Run a long-lived command with live stdout and stderr.
///
/// `BuildKit` reports cache resolution and layer progress on stderr. Capturing
/// that stream leaves GitHub Actions silent until the build exits, which makes
/// an otherwise healthy image build indistinguishable from a stalled job.
pub(crate) fn run_streaming(cmd: &mut Command) -> Result<()> {
    let display = display_command(cmd);
    let mut request = exec_request(cmd);
    request.stdout_mode = jackin_process::StdioMode::Inherit;
    request.stderr_mode = jackin_process::StdioMode::Inherit;
    let result =
        jackin_process::exec_sync(&request).with_context(|| format!("running {display}"))?;
    if result.success {
        Ok(())
    } else {
        Err(anyhow!("{display} failed with code {:?}", result.code))
    }
}

/// Capture stdout as bytes. On failure, error includes trimmed stderr.
///
/// Routes through [`jackin_process`] when the command is a simple program+args
/// capture with inherited env; keeps `Command::output` for complex configured
/// commands (env/cwd/stdio already set on the builder).
pub(crate) fn output(cmd: &mut Command) -> Result<Vec<u8>> {
    let display = display_command(cmd);
    let output = jackin_process::exec_sync(&exec_request(cmd))
        .with_context(|| format!("running {display}"))?;
    if output.success {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "{display} failed with {}\n{}",
            output
                .code
                .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
            stderr.trim()
        ))
    }
}

/// Build a transport request when `cmd` is a plain program+args capture.
fn exec_request(cmd: &Command) -> jackin_process::ExecRequest {
    let program = cmd.get_program();
    if program.is_empty() {
        return jackin_process::ExecRequest::new(program, None::<&str>);
    }
    let args: Vec<_> = cmd.get_args().collect();
    let mut request = jackin_process::ExecRequest::new(program, args);
    if let Some(cwd) = cmd.get_current_dir() {
        request = request.cwd(cwd);
    }
    for (key, value) in cmd.get_envs() {
        if let Some(value) = value {
            request.env.push((key.to_os_string(), value.to_os_string()));
        } else {
            request.env_remove.push(key.to_os_string());
        }
    }
    request
}

/// Capture stdout as a lossy UTF-8 owned string.
pub(crate) fn output_string(cmd: &mut Command) -> Result<String> {
    Ok(String::from_utf8_lossy(&output(cmd)?).into_owned())
}

/// Construct an xtask command; execution must return through this module.
pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// Full process `Output` (stdout+stderr+status) for callers that inspect all three.
pub(crate) fn output_raw(cmd: &mut Command) -> Result<jackin_process::ExecResult> {
    let display = display_command(cmd);
    jackin_process::exec_sync(&exec_request(cmd)).with_context(|| format!("running {display}"))
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
