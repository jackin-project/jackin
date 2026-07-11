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
    #[expect(
        clippy::disallowed_methods,
        reason = "xtask automation shells out to git, gh, cargo, and mise; centralized here"
    )]
    let status = cmd.status().with_context(|| format!("running {display}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{display} failed with {status}"))
    }
}

/// Capture stdout as bytes. On failure, error includes trimmed stderr.
pub(crate) fn output(cmd: &mut Command) -> Result<Vec<u8>> {
    let display = display_command(cmd);
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
