// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

#[cfg(unix)]
#[path = "custom_lints/paths.rs"]
mod cargo_paths;
#[path = "shared/fs_util.rs"]
mod fs_util;
#[cfg(unix)]
mod precommit_install;
#[cfg(unix)]
pub(crate) mod precommit_snapshot;
#[cfg(test)]
mod tests;

#[cfg(unix)]
use anyhow::{Context, Result, bail, ensure};
#[cfg(unix)]
use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    io::{self, Write},
    process::ExitCode,
};

#[cfg(unix)]
type Environment = BTreeMap<OsString, OsString>;

#[cfg(unix)]
fn main() -> ExitCode {
    match run() {
        Ok(status) => status,
        Err(error) => {
            let mut stderr = io::stderr().lock();
            if writeln!(stderr, "error: {error:#}").is_err() {
                return ExitCode::FAILURE;
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(unix)]
fn run() -> Result<ExitCode> {
    let mut args = env::args_os().skip(1);
    let command = args.next().ok_or_else(|| anyhow::anyhow!("{}", usage()))?;

    if command == OsStr::new("install-hooks") {
        ensure!(args.next().is_none(), "install-hooks takes no arguments");
        if let Err(error) = precommit_install::install_hooks() {
            if precommit_install::activation_failed_after_rename(&error) {
                drop(writeln!(io::stderr().lock(), "error: {error:#}"));
                return Ok(ExitCode::from(2));
            }
            return Err(error);
        }
        writeln!(
            io::stdout().lock(),
            "installed Jackin staged-snapshot pre-commit and pinned pre-push hooks"
        )
        .context("writing hook installation result")?;
        return Ok(ExitCode::SUCCESS);
    }

    if command == OsStr::new("run-snapshot") {
        let status = precommit_snapshot::run_from_hook()?;
        let status = u8::try_from(status).context("hook runner returned an invalid status")?;
        return Ok(ExitCode::from(status));
    }

    bail!("{}", usage())
}

#[cfg(unix)]
fn usage() -> &'static str {
    "usage: jackin-precommit-owner install-hooks | run-snapshot [hook arguments...]"
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    use std::io::Write;

    let mut stderr = std::io::stderr().lock();
    if writeln!(
        stderr,
        "jackin-precommit-owner is unsupported on this platform"
    )
    .is_err()
    {
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::FAILURE
}
