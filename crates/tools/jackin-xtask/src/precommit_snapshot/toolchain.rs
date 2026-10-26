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
use super::process::{CommandBudget, EnvironmentMode, SetupControls, capture_strict};
mod verify;
use verify::{command_path, verify_rust_toolchain};

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
    pub(super) environment: BTreeMap<OsString, OsString>,
    pub(super) _path_aliases: Option<tempfile::TempDir>,
}

pub(super) fn qualify(snapshot: &OwnedSnapshot) -> Result<QualifiedTools> {
    let controls = &snapshot.controls;
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
    let rustup_home = env::var_os("RUSTUP_HOME");
    let resolved = resolve_managed_tools(&mise, &mise_env, &snapshot.root, controls)?;
    let rust_tools = resolve_rust_tools_with_controls(
        &rustup,
        &pin,
        &inherited_path,
        rustup_home.as_deref(),
        home.as_deref().map(Path::as_os_str),
        controls,
    )?;
    let cargo = effective_cargo(&mise, &mise_env, &snapshot.root, controls)?;
    let managed_dir = snapshot.scratch_root.join("tools");
    fs::create_dir(&managed_dir).context("creating private qualified tool directory")?;
    // `mise exec` has already resolved this to the configured MBX Cargo
    // wrapper. Put it in the first PATH projection so `cargo` cannot silently
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

    let path_aliases = create_path_aliases(&env::temp_dir(), Path::new("/tmp"))?;
    let environment = build_qualified_environment(
        snapshot,
        &managed_dir,
        path_aliases.path(),
        &cargo,
        &pin,
        &rust_tools,
        &inherited_path,
    )?;
    verify_managed_projection(&mise, &environment, &snapshot.root, controls)?;
    verify_rust_toolchain(
        &pin,
        &rust_tools,
        &mise,
        &environment,
        &snapshot.root,
        controls,
    )?;
    Ok(QualifiedTools {
        gate: resolved["hk"].clone(),
        environment,
        _path_aliases: Some(path_aliases),
    })
}

fn build_qualified_environment(
    snapshot: &OwnedSnapshot,
    managed_dir: &Path,
    path_alias_dir: &Path,
    cargo: &Path,
    pin: &str,
    rust_tools: &BTreeMap<&'static str, PathBuf>,
    inherited_path: &OsStr,
) -> Result<BTreeMap<OsString, OsString>> {
    let mut environment = env::vars_os().collect::<BTreeMap<_, _>>();
    environment.retain(|key, _| {
        let key = key.to_string_lossy();
        !key.starts_with("CARGO_ALIAS_") && !key.starts_with("DYLINT_")
    });
    let mise_storage = ["MISE_DATA_DIR", "MISE_CACHE_DIR"]
        .into_iter()
        .filter_map(|key| {
            environment
                .get(OsStr::new(key))
                .cloned()
                .map(|value| (key, value))
        })
        .collect::<Vec<_>>();
    configure_managed_mise_environment(&mut environment, &snapshot.root);
    for (key, value) in mise_storage {
        environment.insert(OsString::from(key), value);
    }
    for key in [
        "CARGO",
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTC_BOOTSTRAP",
        "RUSTDOC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTDOC",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "CLIPPY_DRIVER",
        "RUSTFMT",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_BUILD_DIR",
        "CARGO_BUILD_TARGET",
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
        "BASH_ENV",
    ] {
        environment.remove(OsStr::new(key));
    }
    populate_path_aliases(path_alias_dir, managed_dir, rust_tools)?;
    environment.insert(
        OsString::from("PATH"),
        qualified_path(path_alias_dir, inherited_path)?,
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
    Ok(environment)
}

fn configure_managed_mise_environment(
    environment: &mut BTreeMap<OsString, OsString>,
    trusted_root: &Path,
) {
    let storage = ["MISE_DATA_DIR", "MISE_CACHE_DIR"]
        .into_iter()
        .filter_map(|key| {
            environment
                .get(OsStr::new(key))
                .cloned()
                .map(|value| (key, value))
        })
        .collect::<Vec<_>>();
    environment.retain(|key, _| !key.to_string_lossy().starts_with("MISE_"));
    for (key, value) in storage {
        environment.insert(OsString::from(key), value);
    }
    environment.insert(
        OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
        trusted_root.as_os_str().to_owned(),
    );
    environment.insert(OsString::from("MISE_AUTO_INSTALL"), OsString::from("0"));
    environment.insert(
        OsString::from("MISE_EXEC_AUTO_INSTALL"),
        OsString::from("false"),
    );
}

fn resolve_managed_tools(
    mise: &Path,
    environment: &BTreeMap<OsString, OsString>,
    root: &Path,
    controls: &SetupControls,
) -> Result<BTreeMap<&'static str, PathBuf>> {
    resolve_tool_paths(MANAGED_TOOLS, |name| {
        let mut command = Command::new(mise);
        command
            .args([OsStr::new("which"), OsStr::new(name)])
            .current_dir(root)
            .env_clear()
            .envs(environment);
        let output = capture_strict(
            &mut command,
            controls,
            EnvironmentMode::Clear,
            super::process::METADATA_OUTPUT_LIMIT,
            1024 * 1024,
            CommandBudget::Probe,
        )
        .with_context(|| format!("resolving staged Mise tool {name}"))?;
        let path = command_path(output, &format!("resolving staged Mise tool {name}"))?;
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

fn create_path_aliases(preferred: &Path, fallback: &Path) -> Result<tempfile::TempDir> {
    let preferred = fs::canonicalize(preferred).context("resolving preferred PATH alias root")?;
    let parent = if preferred.as_os_str().as_encoded_bytes().contains(&b':') {
        // `/tmp` is the fixed separator-free fallback when TMPDIR cannot be encoded in PATH.
        fs::canonicalize(fallback).context("resolving separator-safe PATH alias fallback")?
    } else {
        preferred
    };
    ensure!(
        !parent.as_os_str().as_encoded_bytes().contains(&b':'),
        "separator-safe PATH alias root contains the PATH delimiter"
    );
    let directory = tempfile::Builder::new()
        .prefix("jackin-path-")
        .tempdir_in(parent)
        .context("creating private separator-safe PATH aliases")?;
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
    super::process::validate_confidential_directory(directory.path())
        .context("validating private separator-safe PATH aliases")?;
    ensure!(
        !directory
            .path()
            .as_os_str()
            .as_encoded_bytes()
            .contains(&b':'),
        "private PATH alias path contains the PATH delimiter"
    );
    Ok(directory)
}

fn populate_path_aliases(
    path_alias_dir: &Path,
    managed_dir: &Path,
    rust_tools: &BTreeMap<&str, PathBuf>,
) -> Result<()> {
    for entry in
        crate::fs_util::read_dir_sorted(managed_dir).context("reading private qualified tools")?
    {
        symlink(entry.path(), path_alias_dir.join(entry.file_name()))
            .context("linking private tool into separator-safe PATH projection")?;
    }
    for (name, path) in rust_tools {
        symlink(path, path_alias_dir.join(name))
            .with_context(|| format!("linking qualified Rust tool {name}"))?;
    }
    Ok(())
}

fn qualified_path(path_alias_dir: &Path, inherited_path: &OsStr) -> Result<OsString> {
    let path_entries =
        std::iter::once(path_alias_dir.to_path_buf()).chain(env::split_paths(inherited_path));
    env::join_paths(path_entries).context("building qualified tool path")
}

#[cfg(test)]
fn resolve_rust_tools(
    rustup: &Path,
    pin: &str,
    inherited_path: &OsStr,
    rustup_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<BTreeMap<&'static str, PathBuf>> {
    let controls = SetupControls::for_test(std::time::Duration::from_secs(30));
    resolve_rust_tools_with_controls(rustup, pin, inherited_path, rustup_home, home, &controls)
}

fn resolve_rust_tools_with_controls(
    rustup: &Path,
    pin: &str,
    inherited_path: &OsStr,
    rustup_home: Option<&OsStr>,
    home: Option<&OsStr>,
    controls: &SetupControls,
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
        if let Some(rustup_home) = rustup_home {
            command.env("RUSTUP_HOME", rustup_home);
        } else if let Some(home) = home {
            command.env("HOME", home);
        }
        let output = capture_strict(
            &mut command,
            controls,
            EnvironmentMode::Clear,
            super::process::METADATA_OUTPUT_LIMIT,
            1024 * 1024,
            CommandBudget::Probe,
        )
        .with_context(|| format!("resolving pinned rustup tool {name}"))?;
        let path = command_path(output, &format!("resolving pinned rustup tool {name}"))?;
        let path = fs::canonicalize(path)
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
    controls: &SetupControls,
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
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Clear,
        super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
        CommandBudget::Probe,
    )
    .context("resolving the configured MBX Cargo wrapper")?;
    let path = command_path(output, "resolving the configured MBX Cargo wrapper")?;
    let path = fs::canonicalize(path).context("resolving effective MBX Cargo path")?;
    ensure!(
        path.is_file(),
        "effective MBX Cargo path is not a regular file"
    );
    Ok(path)
}

fn runtime_environment() -> BTreeMap<OsString, OsString> {
    let mut environment = BTreeMap::new();
    for key in [
        "HOME",
        "USER",
        "LOGNAME",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "TERM",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
    ] {
        if let Some(value) = env::var_os(key) {
            environment.insert(OsString::from(key), value);
        }
    }
    for key in ["MISE_DATA_DIR", "MISE_CACHE_DIR"] {
        if let Some(value) = env::var_os(key) {
            environment.insert(OsString::from(key), value);
        }
    }
    environment
}

fn verify_managed_projection(
    mise: &Path,
    environment: &BTreeMap<OsString, OsString>,
    root: &Path,
    controls: &SetupControls,
) -> Result<()> {
    let mut command = Command::new(mise);
    command
        .args([
            "exec",
            "--locked",
            "--",
            "/bin/sh",
            "-c",
            "test \"${RUSTC_BOOTSTRAP+x}\" != x",
        ])
        .current_dir(root)
        .env_clear()
        .envs(environment);
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Clear,
        super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
        CommandBudget::Probe,
    )
    .context("checking staged Mise environment projection")?;
    ensure!(
        output.status.success(),
        "staged Mise environment reintroduces RUSTC_BOOTSTRAP"
    );
    Ok(())
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
