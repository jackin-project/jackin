// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, env, ffi::OsString, fs, path::Path};

use anyhow::{Context, Result, ensure};

use super::verify::verify_rust_toolchain;
use super::{
    effective_cargo, find_executable, resolve_rust_tools, runtime_environment, rust_release,
};

pub(super) fn verify_tool_paths(
    source_root: &Path,
    cargo_path: &Path,
    rustc_path: &Path,
) -> Result<()> {
    let source_root = fs::canonicalize(source_root).context("resolving bootstrap source root")?;
    let pin = rust_release(&source_root.join("rust-toolchain.toml"))?;
    let inherited_path = env::var_os("PATH").unwrap_or_default();
    let mise = find_executable("mise", &inherited_path).context("missing Mise executable")?;
    let mise = fs::canonicalize(mise).context("resolving Mise executable")?;
    let rustup = find_executable("rustup", &inherited_path).context("missing rustup executable")?;
    let mise_environment = mise_environment(&source_root, &inherited_path);
    let expected_cargo = effective_cargo(&mise, &mise_environment, &source_root)?;
    let expected_rust_tools = resolve_rust_tools(&rustup, &pin, &inherited_path)
        .context("resolving pinned Rust tools")?;
    verify_rust_toolchain(
        &pin,
        &expected_rust_tools,
        &mise,
        &mise_environment,
        &source_root,
    )?;
    let cargo_path = fs::canonicalize(cargo_path).context("resolving bootstrap Cargo path")?;
    let rustc_path = fs::canonicalize(rustc_path).context("resolving bootstrap rustc path")?;
    ensure!(
        cargo_path.is_file(),
        "bootstrap Cargo is not a regular file"
    );
    ensure!(
        rustc_path.is_file(),
        "bootstrap rustc is not a regular file"
    );
    ensure!(
        cargo_path == expected_cargo,
        "bootstrap Cargo differs from the configured MBX wrapper"
    );
    ensure!(
        rustc_path == expected_rust_tools["rustc"],
        "bootstrap rustc differs from the pinned Rust toolchain"
    );
    Ok(())
}

fn mise_environment(root: &Path, inherited_path: &std::ffi::OsStr) -> BTreeMap<OsString, OsString> {
    let mut environment = runtime_environment();
    environment.insert(OsString::from("PATH"), inherited_path.to_owned());
    environment.insert(
        OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
        root.as_os_str().to_owned(),
    );
    environment.insert(OsString::from("MISE_AUTO_INSTALL"), OsString::from("0"));
    environment.insert(
        OsString::from("MISE_EXEC_AUTO_INSTALL"),
        OsString::from("false"),
    );
    environment
}
