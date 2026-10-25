// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context, Result, bail, ensure};

use super::super::process::{CommandBudget, EnvironmentMode, SetupControls, capture_strict};

pub(super) fn verify_rust_toolchain(
    pin: &str,
    tools: &BTreeMap<&str, PathBuf>,
    mise: &Path,
    mise_environment: &BTreeMap<OsString, OsString>,
    root: &Path,
    controls: &SetupControls,
) -> Result<()> {
    let bin = tools["rustc"]
        .parent()
        .context("rustc path has no bin directory")?;
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        ensure!(
            tools[name].parent() == Some(bin),
            "hook Rust tools do not belong to one toolchain bin directory"
        );
    }
    // Cargo is deliberately resolved through Mise's configured MBX wrapper.
    // `mise which cargo` points at rustup's shim and would bypass that policy.
    let mut command = Command::new(mise);
    command
        .args(["exec", "--locked", "--", "cargo", "--version"])
        .current_dir(root)
        .env_clear()
        .envs(mise_environment);
    let cargo = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Clear,
        super::super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
        CommandBudget::Probe,
    )
    .context("running locked MBX Cargo version check")?;
    let cargo = command_output(cargo, "running locked MBX Cargo version check")?;
    ensure!(
        cargo.starts_with(&format!("cargo {pin} ")),
        "hook Cargo version does not match Rust {pin}"
    );
    let rustc = run_version(&tools["rustc"], &["--version", "--verbose"], controls)?;
    let rustdoc = run_version(&tools["rustdoc"], &["--version", "--verbose"], controls)?;
    for (name, output) in [("rustc", &rustc), ("rustdoc", &rustdoc)] {
        ensure!(
            output.lines().any(|line| line == format!("release: {pin}")),
            "hook {name} version does not match Rust {pin}"
        );
    }
    let commit = field(&rustc, "commit-hash: ").context("rustc reports no compiler commit")?;
    ensure!(
        commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "rustc compiler commit is invalid"
    );
    ensure!(
        field(&rustdoc, "commit-hash: ") == Some(commit),
        "hook rustdoc compiler identity mismatch"
    );
    let clippy = run_version(&tools["clippy-driver"], &["--version"], controls)?;
    let rustfmt = run_version(&tools["rustfmt"], &["--version"], controls)?;
    ensure!(
        version_commit(&clippy).is_some_and(|value| commit.starts_with(value)),
        "hook Clippy compiler identity mismatch"
    );
    ensure!(
        version_commit(&rustfmt).is_some_and(|value| commit.starts_with(value)),
        "hook rustfmt compiler identity mismatch"
    );
    Ok(())
}

fn run_version(program: &Path, args: &[&str], controls: &SetupControls) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args).env_clear();
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Clear,
        super::super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
        CommandBudget::Probe,
    )
    .with_context(|| {
        format!(
            "running qualified {} version check",
            program.file_name().unwrap_or_default().to_string_lossy()
        )
    })?;
    command_output(output, "running qualified tool version check")
}

fn field<'a>(output: &'a str, prefix: &str) -> Option<&'a str> {
    output.lines().find_map(|line| line.strip_prefix(prefix))
}

fn version_commit(output: &str) -> Option<&str> {
    let open = output.find('(')? + 1;
    let suffix = output.get(open..)?;
    let close = suffix.find(' ')?;
    let value = suffix.get(..close)?;
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(value)
}

pub(super) fn command_output(output: Output, operation: &str) -> Result<String> {
    let stdout = command_stdout(output, operation)?;
    String::from_utf8(stdout).with_context(|| format!("decoding {operation} output"))
}

pub(super) fn command_path_output(output: Output, operation: &str) -> Result<PathBuf> {
    let stdout = command_stdout(output, operation)?;
    let path = stdout
        .strip_suffix(b"\n")
        .with_context(|| format!("{operation} output is missing its path record terminator"))?;
    ensure!(
        !path.is_empty(),
        "{operation} returned an empty path record"
    );
    ensure!(
        !path.contains(&0),
        "{operation} returned an invalid path record"
    );
    Ok(PathBuf::from(OsString::from_vec(path.to_vec())))
}

fn command_stdout(output: Output, operation: &str) -> Result<Vec<u8>> {
    if !output.status.success() {
        bail!("{operation} failed with {}", output.status);
    }
    Ok(output.stdout)
}
