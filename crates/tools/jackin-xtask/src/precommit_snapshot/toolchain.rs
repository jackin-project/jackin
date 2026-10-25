// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};

use super::git::OwnedSnapshot;
use super::process::capture;
mod provision;
mod verify;
use verify::{command_output, verify_rust_toolchain};

pub(super) fn verify_provision_tool_paths(
    source_root: &Path,
    cargo_path: &Path,
    rustc_path: &Path,
) -> Result<()> {
    provision::verify_tool_paths(source_root, cargo_path, rustc_path)
}

const MANAGED_TOOLS: &[&str] = &[
    #[cfg(target_os = "macos")]
    "swiftlint",
    "hk",
    "jactionlint",
    "shellcheck",
    "rumdl",
    "gitleaks",
    "kingfisher",
    "cargo-deny",
    "cargo-nextest",
    "cargo-dylint",
    "dylint-link",
];

pub(super) struct QualifiedTools {
    pub(super) gate: PathBuf,
    pub(super) cargo: PathBuf,
    pub(super) rustc: PathBuf,
    pub(super) environment: BTreeMap<OsString, OsString>,
}

pub(super) fn qualify(snapshot: &OwnedSnapshot) -> Result<QualifiedTools> {
    let inherited_path = env::var_os("PATH").unwrap_or_default();
    let home = env::var_os("HOME").map(PathBuf::from);
    let guard = resolve_guard(&inherited_path, home.as_deref())?;
    let pin = rust_release(&snapshot.root.join("rust-toolchain.toml"))?;
    let mise = find_executable("mise", &inherited_path).context("missing Mise executable")?;
    let mise = fs::canonicalize(mise).context("resolving Mise executable")?;
    let mut mise_env = runtime_environment();
    mise_env.insert(OsString::from("PATH"), inherited_path.clone());
    mise_env.insert(
        OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
        snapshot.root.as_os_str().to_owned(),
    );
    mise_env.insert(OsString::from("MISE_AUTO_INSTALL"), OsString::from("0"));
    mise_env.insert(
        OsString::from("MISE_EXEC_AUTO_INSTALL"),
        OsString::from("false"),
    );
    let rustup = find_executable("rustup", &inherited_path).context("missing rustup executable")?;
    let resolved = resolve_managed_tools(&mise, &mise_env, &snapshot.root)?;
    let rust_tools = resolve_rust_tools(&rustup, &pin, &inherited_path)?;
    let cargo = effective_cargo(&mise, &mise_env, &snapshot.root)?;
    verify_rust_toolchain(&pin, &rust_tools, &mise, &mise_env, &snapshot.root)?;

    let managed_dir = snapshot.scratch_root.join("tools");
    fs::create_dir(&managed_dir).context("creating private qualified tool directory")?;
    // `mise exec` has already resolved this to the configured MBX Cargo
    // wrapper. Put it before rustup's bin directory so `cargo` cannot silently
    // resolve to rustup's Cargo and bypass the admitted wrapper.
    symlink(&cargo, managed_dir.join("cargo"))
        .context("linking the qualified MBX Cargo wrapper")?;
    for name in MANAGED_TOOLS {
        symlink(&resolved[*name], managed_dir.join(name))
            .with_context(|| format!("linking qualified Mise tool {name}"))?;
    }
    symlink(&guard, managed_dir.join("rust-build-guard"))
        .context("linking qualified rust-build-guard")?;
    symlink(&mise, managed_dir.join("mise")).context("linking qualified Mise executable")?;
    #[cfg(target_os = "macos")]
    symlink("/usr/bin/xcrun", managed_dir.join("xcrun"))
        .context("linking fixed Xcode tool resolver")?;

    let mut environment = env::vars_os().collect::<BTreeMap<_, _>>();
    environment.retain(|key, _| {
        let key = key.to_string_lossy();
        !key.starts_with("CARGO_ALIAS_") && !key.starts_with("DYLINT_")
    });
    for key in [
        "CARGO",
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTDOC",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "CLIPPY_DRIVER",
        "RUSTFMT",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_BUILD_DIR",
        "CARGO_BUILD_TARGET",
        "MISE_TRUSTED_CONFIG_PATHS",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_PREFIX",
        "GIT_SHALLOW_FILE",
    ] {
        environment.remove(OsStr::new(key));
    }
    let rust_bin = rust_tools["rustc"]
        .parent()
        .context("rustc path has no bin directory")?
        .to_path_buf();
    environment.insert(
        OsString::from("PATH"),
        qualified_path(&managed_dir, &rust_bin, &inherited_path)?,
    );
    let selected_path = environment
        .get(OsStr::new("PATH"))
        .context("qualified PATH is missing")?;
    let selected_cargo = find_executable("cargo", selected_path)?;
    ensure!(
        fs::canonicalize(selected_cargo).context("verifying PATH-selected Cargo")? == cargo,
        "qualified PATH does not resolve to the admitted MBX Cargo wrapper"
    );
    environment.insert(OsString::from("RUSTUP_TOOLCHAIN"), OsString::from(pin));
    environment.insert(
        OsString::from("RUSTC"),
        rust_tools["rustc"].as_os_str().to_owned(),
    );
    environment.insert(
        OsString::from("RUSTDOC"),
        rust_tools["rustdoc"].as_os_str().to_owned(),
    );
    environment.insert(
        OsString::from("CLIPPY_DRIVER"),
        rust_tools["clippy-driver"].as_os_str().to_owned(),
    );
    environment.insert(
        OsString::from("RUSTFMT"),
        rust_tools["rustfmt"].as_os_str().to_owned(),
    );
    environment.insert(
        OsString::from("CARGO_BUILD_RUSTC"),
        rust_tools["rustc"].as_os_str().to_owned(),
    );
    environment.insert(
        OsString::from("CARGO_BUILD_RUSTDOC"),
        rust_tools["rustdoc"].as_os_str().to_owned(),
    );
    environment.insert(OsString::from("CARGO_BUILD_RUSTC_WRAPPER"), OsString::new());
    environment.insert(
        OsString::from("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER"),
        OsString::new(),
    );
    environment.insert(OsString::from("RUSTC_WRAPPER"), OsString::new());
    environment.insert(OsString::from("RUSTC_WORKSPACE_WRAPPER"), OsString::new());
    environment.insert(OsString::from("CARGO_BUILD_JOBS"), OsString::from("2"));
    environment.insert(OsString::from("NEXTEST_TEST_THREADS"), OsString::from("4"));
    environment.insert(
        OsString::from("CARGO_TARGET_DIR"),
        snapshot.scratch_root.join("target").into_os_string(),
    );
    environment.insert(
        OsString::from("CARGO_BUILD_BUILD_DIR"),
        snapshot.scratch_root.join("build").into_os_string(),
    );
    environment.insert(
        OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
        snapshot.root.as_os_str().to_owned(),
    );
    environment.insert(OsString::from("MISE_AUTO_INSTALL"), OsString::from("0"));
    environment.insert(
        OsString::from("MISE_EXEC_AUTO_INSTALL"),
        OsString::from("false"),
    );
    Ok(QualifiedTools {
        gate: resolved["hk"].clone(),
        cargo,
        rustc: rust_tools["rustc"].clone(),
        environment,
    })
}

fn resolve_managed_tools(
    mise: &Path,
    environment: &BTreeMap<OsString, OsString>,
    root: &Path,
) -> Result<BTreeMap<&'static str, PathBuf>> {
    resolve_tool_paths(MANAGED_TOOLS, |name| {
        let mut command = Command::new(mise);
        command
            .args([OsStr::new("which"), OsStr::new(name)])
            .current_dir(root)
            .env_clear()
            .envs(environment);
        let output =
            capture(&mut command).with_context(|| format!("resolving staged Mise tool {name}"))?;
        let value = command_output(output, &format!("resolving staged Mise tool {name}"))?;
        let path = PathBuf::from(value.trim());
        let path = fs::canonicalize(&path)
            .with_context(|| format!("resolving staged Mise tool path for {name}"))?;
        ensure!(
            path.is_file(),
            "staged Mise tool is not a regular file: {name}"
        );
        Ok(path)
    })
}

fn resolve_tool_paths<F>(
    names: &[&'static str],
    mut resolve: F,
) -> Result<BTreeMap<&'static str, PathBuf>>
where
    F: FnMut(&'static str) -> Result<PathBuf>,
{
    let mut resolved = BTreeMap::new();
    for name in names.iter().copied() {
        resolved.insert(name, resolve(name)?);
    }
    Ok(resolved)
}

fn qualified_path(managed_dir: &Path, rust_bin: &Path, inherited_path: &OsStr) -> Result<OsString> {
    let path_entries = std::iter::once(managed_dir.to_path_buf())
        .chain(std::iter::once(rust_bin.to_path_buf()))
        .chain(env::split_paths(inherited_path));
    env::join_paths(path_entries).context("building qualified tool path")
}

fn resolve_rust_tools(
    rustup: &Path,
    pin: &str,
    inherited_path: &OsStr,
) -> Result<BTreeMap<&'static str, PathBuf>> {
    let mut rust_tools = BTreeMap::new();
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        let mut command = Command::new(rustup);
        command
            .args([
                OsStr::new("which"),
                OsStr::new("--toolchain"),
                OsStr::new(pin),
                OsStr::new(name),
            ])
            .env_clear()
            .env("PATH", inherited_path);
        let output = capture(&mut command)
            .with_context(|| format!("resolving pinned rustup tool {name}"))?;
        let value = command_output(output, &format!("resolving pinned rustup tool {name}"))?;
        let path = fs::canonicalize(value.trim())
            .with_context(|| format!("resolving pinned rustup tool path for {name}"))?;
        ensure!(
            path.is_file(),
            "pinned rustup tool is not a regular file: {name}"
        );
        rust_tools.insert(name, path);
    }
    Ok(rust_tools)
}

fn effective_cargo(
    mise: &Path,
    environment: &BTreeMap<OsString, OsString>,
    root: &Path,
) -> Result<PathBuf> {
    let mut command = Command::new(mise);
    command
        .args([
            "exec",
            "--locked",
            "--",
            "/bin/sh",
            "-c",
            "command -v cargo",
        ])
        .current_dir(root)
        .env_clear()
        .envs(environment);
    let output = capture(&mut command).context("resolving the configured MBX Cargo wrapper")?;
    let value = command_output(output, "resolving the configured MBX Cargo wrapper")?;
    let path = fs::canonicalize(value.trim()).context("resolving effective MBX Cargo path")?;
    ensure!(
        path.is_file(),
        "effective MBX Cargo path is not a regular file"
    );
    Ok(path)
}

fn runtime_environment() -> BTreeMap<OsString, OsString> {
    let mut environment = BTreeMap::new();
    for key in [
        "HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "LC_ALL", "TERM",
    ] {
        if let Some(value) = env::var_os(key) {
            environment.insert(OsString::from(key), value);
        }
    }
    environment
}

fn rust_release(path: &Path) -> Result<String> {
    let contents = fs::read_to_string(path).context("reading staged rust-toolchain.toml")?;
    let value: toml::Value =
        toml::from_str(&contents).context("parsing staged rust-toolchain.toml")?;
    let channel = value
        .get("toolchain")
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .context("staged rust-toolchain.toml has no toolchain.channel")?;
    ensure!(
        channel.split('.').count() == 3
            && channel
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "hook requires an exact numeric Rust release pin"
    );
    Ok(channel.to_owned())
}

fn find_executable(name: &str, path: &OsStr) -> Result<PathBuf> {
    for directory in env::split_paths(path) {
        let candidate = directory.join(name);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    bail!("tool is missing from PATH: {name}")
}

fn resolve_guard(inherited_path: &OsStr, home: Option<&Path>) -> Result<PathBuf> {
    let candidate = match find_executable("rust-build-guard", inherited_path) {
        Ok(candidate) => candidate,
        Err(_) => home
            .map(|home| home.join(".local/bin/rust-build-guard"))
            .filter(|candidate| is_executable(candidate))
            .context("rust-build-guard is not installed")?,
    };
    let physical = fs::canonicalize(candidate).context("resolving rust-build-guard executable")?;
    ensure!(
        is_executable(&physical),
        "rust-build-guard is not executable"
    );
    Ok(physical)
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests;
