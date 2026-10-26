// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Non-activating receipt inspection and native hook ownership.

#[cfg(unix)]
#[path = "shared/fs_util.rs"]
mod fs_util;
#[cfg(unix)]
mod precommit_install;
#[cfg(unix)]
type Environment = BTreeMap<OsString, OsString>;
#[cfg(unix)]
#[path = "custom_lints/paths.rs"]
mod cargo_paths;
#[cfg(unix)]
mod precommit_owner_security;
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
    path::PathBuf,
    process::ExitCode,
};

#[cfg(unix)]
const PROTOCOL: &str = "jackin-precommit-owner/3";

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

    if command == OsStr::new("--protocol") {
        ensure!(args.next().is_none(), "--protocol takes no arguments");
        writeln!(io::stdout().lock(), "{PROTOCOL}").context("writing protocol identifier")?;
        return Ok(ExitCode::SUCCESS);
    }

    if command == OsStr::new("inspect-install") {
        let root_path = required_path(&mut args, "physical install root")?;
        ensure!(
            args.next().is_none(),
            "inspect-install accepts exactly one physical root"
        );
        let source_digest = precommit_owner_security::inspect_install(&root_path)?;
        writeln!(
            io::stdout().lock(),
            "format3 receipt and checked artifact descriptors are internally consistent; recorded source digest={source_digest}; selected install root and fixed artifact names checked through retained descriptors; {}",
            precommit_owner_security::INSTALL_INSPECTION_LIMITATIONS
        )
        .context("writing checked installation inspection result")?;
        return Ok(ExitCode::SUCCESS);
    }

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

    if command != OsStr::new("inspect-receipt") {
        bail!("{}", usage());
    }

    let receipt_path = required_path(&mut args, "receipt path")?;
    let owner_path = required_path(&mut args, "owner artifact path")?;
    let entry_path = required_path(&mut args, "entry artifact path")?;
    let helper_path = required_path(&mut args, "helper artifact path")?;
    ensure!(
        args.next().is_none(),
        "inspect-receipt accepts exactly four paths"
    );

    let receipt = precommit_owner_security::inspect_receipt(
        &receipt_path,
        &owner_path,
        &entry_path,
        &helper_path,
    )?;
    writeln!(
        io::stdout().lock(),
        "format3 receipt fields are internally consistent; owner, entry, and helper descriptor contents match; recorded source digest={} (source closure not checked); executable trust, ownership, ACL, and ancestor policy not checked",
        receipt.source_hex()
    )
    .context("writing receipt inspection result")?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(unix)]
fn required_path(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<PathBuf> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("missing {name}"))
}

#[cfg(unix)]
fn usage() -> &'static str {
    "usage: jackin-precommit-owner --protocol | inspect-receipt <receipt> <owner> <entry> <helper> | inspect-install <physical-root> | install-hooks | run-snapshot [hook arguments...]"
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
