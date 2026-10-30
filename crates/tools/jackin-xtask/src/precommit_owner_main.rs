// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Non-activating receipt inspection for the private pre-commit owner protocol.

#[cfg(unix)]
mod precommit_owner_security;
#[cfg(unix)]
mod precommit_provision;
#[cfg(unix)]
pub(crate) mod precommit_snapshot;

#[cfg(unix)]
use anyhow::{Context, Result, bail, ensure};
#[cfg(unix)]
use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

#[cfg(unix)]
const PROTOCOL: &str = "jackin-precommit-owner/3";
#[cfg(unix)]
const PROVISION_RESULT_HEADER: &str = "jackin-native-precommit-generation/1";

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

    if command == OsStr::new("run-installed-snapshot") {
        let generation = required_path(&mut args, "selected native hook generation")?;
        return precommit_snapshot::run_installed_from_hook(&generation, &args.collect::<Vec<_>>());
    }

    if command == OsStr::new("provision-reviewed-snapshot") {
        return provision_reviewed_snapshot(&mut args);
    }

    if command == OsStr::new("validate-hook-config") {
        return validate_hook_config(&mut args);
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
fn provision_reviewed_snapshot(args: &mut impl Iterator<Item = OsString>) -> Result<ExitCode> {
    let (bootstrap_root, repository, reviewed_tree) = parse_provision_arguments(args)?;
    let generation = precommit_provision::run(&bootstrap_root, &repository, &reviewed_tree)?;
    ensure!(
        generation.len() == 64
            && generation
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "provisioner returned an invalid generation id"
    );
    write!(
        io::stdout().lock(),
        "{PROVISION_RESULT_HEADER}\n{generation}\n"
    )
    .context("writing reviewed snapshot provisioning result")?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(unix)]
fn validate_hook_config(args: &mut impl Iterator<Item = OsString>) -> Result<ExitCode> {
    let repository = parse_hook_config_arguments(args)?;
    precommit_provision::validate_hook_config(&repository)?;
    writeln!(io::stdout().lock(), "validated Git hook config custody")
        .context("writing Git hook config custody result")?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(unix)]
fn parse_hook_config_arguments(args: &mut impl Iterator<Item = OsString>) -> Result<PathBuf> {
    expect_option(args, "--repository")?;
    let repository = required_path(args, "physical repository")?;
    ensure!(repository.is_absolute(), "repository must be absolute");
    ensure!(
        args.next().is_none(),
        "validate-hook-config accepts one repository"
    );
    Ok(repository)
}

#[cfg(unix)]
fn parse_provision_arguments(
    args: &mut impl Iterator<Item = OsString>,
) -> Result<(PathBuf, PathBuf, String)> {
    expect_option(args, "--bootstrap-root")?;
    let bootstrap_root = required_path(args, "bootstrap root")?;
    ensure!(
        bootstrap_root.is_absolute(),
        "bootstrap root must be absolute"
    );
    expect_option(args, "--repository")?;
    let repository = required_path(args, "physical repository")?;
    ensure!(repository.is_absolute(), "repository must be absolute");
    expect_option(args, "--reviewed-tree")?;
    let reviewed_tree = args
        .next()
        .context("missing reviewed source tree")?
        .into_string()
        .map_err(|_| anyhow::anyhow!("reviewed source tree is not UTF-8"))?;
    ensure!(
        matches!(reviewed_tree.len(), 40 | 64)
            && reviewed_tree
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "reviewed source tree must be lowercase hexadecimal"
    );
    ensure!(
        args.next().is_none(),
        "provision-reviewed-snapshot accepts exactly three options"
    );
    Ok((bootstrap_root, repository, reviewed_tree))
}

#[cfg(unix)]
fn expect_option(args: &mut impl Iterator<Item = OsString>, expected: &str) -> Result<()> {
    let actual = args.next().context(format!("missing {expected} option"))?;
    ensure!(actual == OsStr::new(expected), "expected {expected} option");
    Ok(())
}

#[cfg(unix)]
fn usage() -> &'static str {
    "usage: jackin-precommit-owner --protocol | inspect-receipt <receipt> <owner> <entry> <helper> | inspect-install <physical-root> | run-installed-snapshot <generation-dir> [hook arguments...] | provision-reviewed-snapshot --bootstrap-root <absolute-path> --repository <absolute-path> --reviewed-tree <git-tree-id> | validate-hook-config --repository <absolute-path>"
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn reviewed_snapshot_command_accepts_only_the_closed_argument_order() {
        let reviewed_tree = "a".repeat(40);
        let mut args = [
            "--bootstrap-root",
            "/private/bootstrap",
            "--repository",
            "/work/repository",
            "--reviewed-tree",
            reviewed_tree.as_str(),
        ]
        .into_iter()
        .map(OsString::from);
        let (bootstrap, repository, tree) = parse_provision_arguments(&mut args).unwrap();
        assert_eq!(bootstrap, PathBuf::from("/private/bootstrap"));
        assert_eq!(repository, PathBuf::from("/work/repository"));
        assert_eq!(tree, "a".repeat(40));
    }

    #[test]
    fn reviewed_snapshot_command_rejects_reordered_extra_and_noncanonical_values() {
        let reviewed_tree = "a".repeat(40);
        let mut wrong_order = [
            "--repository",
            "/work/repository",
            "--bootstrap-root",
            "/private/bootstrap",
            "--reviewed-tree",
            reviewed_tree.as_str(),
        ]
        .into_iter()
        .map(OsString::from);
        parse_provision_arguments(&mut wrong_order).expect_err("option order must be closed");

        let mut extra = [
            "--bootstrap-root",
            "/private/bootstrap",
            "--repository",
            "/work/repository",
            "--reviewed-tree",
            reviewed_tree.as_str(),
            "--force",
        ]
        .into_iter()
        .map(OsString::from);
        parse_provision_arguments(&mut extra).expect_err("extra options must be rejected");

        for tree in ["A".repeat(40), "a".repeat(39), "z".repeat(40)] {
            let args = [
                OsString::from("--bootstrap-root"),
                OsString::from("/private/bootstrap"),
                OsString::from("--repository"),
                OsString::from("/work/repository"),
                OsString::from("--reviewed-tree"),
                OsString::from(tree),
            ];
            let mut args = args.into_iter();
            parse_provision_arguments(&mut args).expect_err("invalid tree ids must be rejected");
        }
    }

    #[test]
    fn config_custody_command_accepts_only_one_absolute_repository() {
        let mut valid = ["--repository", "/work/repository"]
            .into_iter()
            .map(OsString::from);
        assert_eq!(
            parse_hook_config_arguments(&mut valid).unwrap(),
            PathBuf::from("/work/repository")
        );

        for values in [
            vec!["/work/repository", "--repository"],
            vec!["--repository", "relative"],
            vec!["--repository", "/work/repository", "--force"],
        ] {
            let mut args = values.into_iter().map(OsString::from);
            parse_hook_config_arguments(&mut args)
                .expect_err("invalid config arguments must be rejected");
        }
    }
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    use std::io::Write;

    let mut stderr = std::io::stderr().lock();
    if writeln!(
        stderr,
        "jackin-precommit-owner receipt inspection is unsupported on this platform"
    )
    .is_err()
    {
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::FAILURE
}
