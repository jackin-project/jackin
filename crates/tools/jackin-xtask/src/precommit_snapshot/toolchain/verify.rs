// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};

use super::super::process::{CommandBudget, EnvironmentMode, SetupControls, capture_strict};

struct ProvisionToolPaths {
    cargo: PathBuf,
    rustc: PathBuf,
}

pub(super) fn admit_install_environment(source_root: &Path) -> Result<()> {
    let ProvisionToolPaths { cargo, rustc } = resolve_provision_tool_paths(source_root)?;
    drop((cargo, rustc));
    Ok(())
}

fn resolve_provision_tool_paths(source_root: &Path) -> Result<ProvisionToolPaths> {
    let source_root = fs::canonicalize(source_root).context("resolving bootstrap source root")?;
    let temp_parent = super::super::process::canonical_temporary_parent(&std::env::temp_dir())
        .context("validating physical provision tool-qualification directory")?;
    let isolation = tempfile::Builder::new()
        .prefix("jackin-provision-mise-")
        .tempdir_in(temp_parent)
        .context("creating private tool qualification directory")?;
    fs::set_permissions(isolation.path(), fs::Permissions::from_mode(0o700))
        .context("protecting private tool qualification directory")?;
    let controls = SetupControls::new(Arc::new(AtomicBool::new(false)), Duration::from_mins(10));
    let inputs = super::HostInputs::capture(&source_root, isolation.path(), &controls)?;
    let pin = super::rust_release(&source_root.join("rust-toolchain.toml"), &controls)?;
    let inherited_path = inputs.search_path();
    let home = inputs.home.as_deref().map(Path::new);
    let mise = super::find_executable("mise", inherited_path).context("missing Mise executable")?;
    let mise = fs::canonicalize(mise).context("resolving Mise executable")?;
    let mise_environment = inputs.command_environment(&source_root);
    let rustup =
        super::resolve_mise_tool(&mise, &mise_environment, &source_root, "rustup", &controls)?;
    let cargo = super::effective_cargo(&mise, &mise_environment, &source_root, &controls)?;
    let rust_tools = super::resolve_rust_tools_with_controls(
        &rustup,
        &pin,
        std::ffi::OsStr::new(super::FIXED_SYSTEM_PATH),
        inputs.rustup_home.as_deref(),
        home.map(Path::as_os_str),
        &controls,
    )
    .context("resolving pinned Rust tools")?;
    verify_rust_toolchain(
        &pin,
        &rust_tools,
        &mise,
        &mise_environment,
        &source_root,
        &controls,
    )?;
    Ok(ProvisionToolPaths {
        cargo,
        rustc: rust_tools["rustc"].clone(),
    })
}

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
    for name in ["cargo", "rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        ensure!(
            tools[name].parent() == Some(bin),
            "hook Rust tools do not belong to one toolchain bin directory"
        );
    }
    let mut cargo_environment = mise_environment.clone();
    cargo_environment.insert(
        OsString::from("CARGO"),
        tools["cargo"].as_os_str().to_owned(),
    );
    let mut command = Command::new(mise);
    command
        .args(["exec", "--locked", "--", "cargo", "--version"])
        .current_dir(root)
        .env_clear()
        .envs(&cargo_environment);
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
    let rust_cargo = run_version(&tools["cargo"], &["--version"], controls)?;
    ensure!(
        rust_cargo.starts_with(&format!("cargo {pin} ")),
        "pinned rustup Cargo version does not match Rust {pin}"
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

fn command_stdout(output: Output, operation: &str) -> Result<Vec<u8>> {
    if !output.status.success() {
        bail!("{operation} failed with {}", output.status);
    }
    Ok(output.stdout)
}

pub(super) fn command_path(output: Output, operation: &str) -> Result<PathBuf> {
    let mut bytes = command_stdout(output, operation)?;
    ensure!(
        bytes.last() == Some(&b'\n'),
        "{operation} output is missing its record terminator"
    );
    bytes.pop();
    ensure!(
        !bytes.is_empty(),
        "{operation} returned an empty path record"
    );
    ensure!(!bytes.contains(&0), "{operation} output contains NUL");
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}
