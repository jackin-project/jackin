// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::{OsStr, OsString},
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{PermissionsExt, symlink},
    },
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};

use super::git::OwnedSnapshot;
use super::process::{CommandBudget, EnvironmentMode, SetupControls, capture_strict};
mod verify;
#[cfg(test)]
use verify::verify_provision_tool_paths;
use verify::{command_path, verify_rust_toolchain};

pub(super) fn admit_install_environment(source_root: &Path) -> Result<()> {
    verify::admit_install_environment(source_root)
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

const FIXED_SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

const HOST_INPUT_KEYS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_COLLATE",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_TIME",
    "TERM",
    "RUSTUP_HOME",
    "CARGO_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "MISE_DATA_DIR",
    "MISE_CACHE_DIR",
    "MISE_STATE_DIR",
    "MISE_TMP_DIR",
    "MISE_INSTALLS_DIR",
    "MISE_INSTALL_STORE_DIR",
    "MISE_PLUGINS_DIR",
    "MISE_DOWNLOADS_DIR",
    "MISE_SYSTEM_DATA_DIR",
    "MISE_SYSTEM_INSTALLS_DIR",
    "MISE_SYSTEM_SHIMS_DIR",
    "MBX_CACHE_DIR",
    "MBX_SESSION_PATH",
];

pub(super) struct QualifiedTools {
    pub(super) gate: PathBuf,
    pub(super) environment: BTreeMap<OsString, OsString>,
}

struct HostInputs {
    search_path: OsString,
    home: Option<OsString>,
    rustup_home: Option<OsString>,
    cargo_home: PathBuf,
    environment: BTreeMap<OsString, OsString>,
    mise_config_dir: PathBuf,
    mise_system_config_dir: PathBuf,
}

impl HostInputs {
    fn capture(reviewed_root: &Path, isolation_root: &Path) -> Result<Self> {
        let mut inherited = BTreeMap::new();
        for key in HOST_INPUT_KEYS {
            if let Some(value) = env::var_os(key) {
                inherited.insert(OsString::from(key), value);
            }
        }

        let cwd = env::current_dir().context("reading native hook working directory")?;
        let passwd_home = if inherited.contains_key(OsStr::new("HOME")) {
            None
        } else {
            crate::cargo_paths::passwd_home_for_real_uid()?
        };
        Self::from_environment(&inherited, &cwd, passwd_home, reviewed_root, isolation_root)
    }

    fn from_environment(
        inherited: &BTreeMap<OsString, OsString>,
        cwd: &Path,
        passwd_home: Option<OsString>,
        reviewed_root: &Path,
        isolation_root: &Path,
    ) -> Result<Self> {
        let reviewed_root = fs::canonicalize(reviewed_root)
            .context("resolving reviewed Cargo configuration root")?;
        let isolation_root =
            fs::canonicalize(isolation_root).context("resolving private Mise isolation root")?;
        ensure!(
            cwd.is_absolute(),
            "host input capture requires an absolute cwd"
        );

        let cargo_home = crate::cargo_paths::cargo_home(inherited, || Ok(passwd_home.clone()))?;
        let cargo_home = crate::cargo_paths::resolve(&cargo_home, cwd)?;
        ensure!(
            cargo_home.is_absolute(),
            "effective Cargo home must be absolute"
        );
        validate_cargo_configuration(&reviewed_root, &cargo_home)?;

        let home = inherited.get(OsStr::new("HOME")).cloned();
        let effective_home = match home.as_deref() {
            Some(value) if !value.is_empty() => {
                Some(normalize_host_path(value, home.as_deref(), cwd)?)
            }
            Some(_) => Some(PathBuf::from(std::path::MAIN_SEPARATOR_STR)),
            None => passwd_home
                .as_deref()
                .map(|value| normalize_host_path(value, None, cwd))
                .transpose()?,
        };

        let search_path = inherited
            .get(OsStr::new("PATH"))
            .cloned()
            .unwrap_or_default();
        let rustup_home = inherited
            .get(OsStr::new("RUSTUP_HOME"))
            .map(|value| {
                normalize_host_path(value, effective_home.as_deref().map(Path::as_os_str), cwd)
            })
            .transpose()?
            .map(PathBuf::into_os_string);

        let mise_config_dir = create_private_mise_config_dir(&isolation_root, "mise-config")?;
        let mise_system_config_dir =
            create_private_mise_config_dir(&isolation_root, "mise-system-config")?;

        let mut environment = BTreeMap::new();
        for key in [
            "HOME",
            "USER",
            "LOGNAME",
            "LANG",
            "LANGUAGE",
            "LC_ALL",
            "LC_COLLATE",
            "LC_CTYPE",
            "LC_MESSAGES",
            "LC_MONETARY",
            "LC_NUMERIC",
            "LC_TIME",
            "TERM",
        ] {
            if let Some(value) = inherited.get(OsStr::new(key)) {
                environment.insert(OsString::from(key), value.clone());
            }
        }
        if home.as_deref().is_some_and(|value| !value.is_empty())
            && let Some(effective_home) = &effective_home
        {
            environment.insert(
                OsString::from("HOME"),
                effective_home.as_os_str().to_owned(),
            );
        }
        for key in ["TMPDIR", "TMP", "TEMP"] {
            if let Some(value) = inherited.get(OsStr::new(key)) {
                environment.insert(
                    OsString::from(key),
                    normalize_host_path(
                        value,
                        effective_home.as_deref().map(Path::as_os_str),
                        cwd,
                    )?
                    .into_os_string(),
                );
            }
        }
        environment.insert(
            OsString::from("CARGO_HOME"),
            cargo_home.as_os_str().to_owned(),
        );
        if let Some(rustup_home) = &rustup_home {
            environment.insert(OsString::from("RUSTUP_HOME"), rustup_home.clone());
        }

        add_mise_storage_environment(inherited, cwd, effective_home.as_deref(), &mut environment)?;
        for key in ["MBX_CACHE_DIR", "MBX_SESSION_PATH"] {
            if let Some(value) = inherited.get(OsStr::new(key)) {
                environment.insert(
                    OsString::from(key),
                    normalize_host_path(
                        value,
                        effective_home.as_deref().map(Path::as_os_str),
                        cwd,
                    )?
                    .into_os_string(),
                );
            }
        }
        environment.insert(OsString::from("MBX_CARGO_SHIM_MODE"), OsString::from("1"));

        Ok(Self {
            search_path,
            home: effective_home
                .as_deref()
                .map(|path| path.as_os_str().to_owned()),
            rustup_home,
            cargo_home,
            environment,
            mise_config_dir,
            mise_system_config_dir,
        })
    }

    fn search_path(&self) -> &OsStr {
        &self.search_path
    }

    fn environment_for(&self, trusted_root: &Path, path: &OsStr) -> BTreeMap<OsString, OsString> {
        let mut environment = self.environment.clone();
        environment.insert(OsString::from("PATH"), path.to_owned());
        environment.insert(
            OsString::from("MISE_CONFIG_DIR"),
            self.mise_config_dir.as_os_str().to_owned(),
        );
        environment.insert(
            OsString::from("MISE_SYSTEM_CONFIG_DIR"),
            self.mise_system_config_dir.as_os_str().to_owned(),
        );
        environment.insert(
            OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
            trusted_root.as_os_str().to_owned(),
        );
        environment.insert(OsString::from("MISE_AUTO_INSTALL"), OsString::from("0"));
        environment.insert(
            OsString::from("MISE_EXEC_AUTO_INSTALL"),
            OsString::from("false"),
        );
        environment
    }

    fn command_environment(&self, trusted_root: &Path) -> BTreeMap<OsString, OsString> {
        self.environment_for(trusted_root, OsStr::new(FIXED_SYSTEM_PATH))
    }

    fn cargo_home(&self) -> &Path {
        &self.cargo_home
    }
}

fn add_mise_storage_environment(
    inherited: &BTreeMap<OsString, OsString>,
    cwd: &Path,
    home: Option<&Path>,
    environment: &mut BTreeMap<OsString, OsString>,
) -> Result<()> {
    for (mise_key, xdg_key, default_suffix) in [
        ("MISE_DATA_DIR", "XDG_DATA_HOME", ".local/share/mise"),
        (
            "MISE_CACHE_DIR",
            "XDG_CACHE_HOME",
            if cfg!(target_os = "macos") {
                "Library/Caches/mise"
            } else {
                ".cache/mise"
            },
        ),
        ("MISE_STATE_DIR", "XDG_STATE_HOME", ".local/state/mise"),
    ] {
        let root = selected_storage_root(inherited, mise_key, xdg_key, default_suffix, home, cwd)?;
        environment.insert(OsString::from(mise_key), root.into_os_string());
    }

    let data_root = PathBuf::from(
        environment
            .get(OsStr::new("MISE_DATA_DIR"))
            .context("effective Mise data root is missing")?,
    );
    let installs_root = if let Some(value) = inherited.get(OsStr::new("MISE_INSTALLS_DIR")) {
        normalize_host_path(value, home.map(Path::as_os_str), cwd)?
    } else {
        data_root.join("installs")
    };
    environment.insert(
        OsString::from("MISE_INSTALLS_DIR"),
        installs_root.into_os_string(),
    );

    for key in [
        "MISE_TMP_DIR",
        "MISE_INSTALL_STORE_DIR",
        "MISE_PLUGINS_DIR",
        "MISE_DOWNLOADS_DIR",
        "MISE_SYSTEM_DATA_DIR",
        "MISE_SYSTEM_INSTALLS_DIR",
        "MISE_SYSTEM_SHIMS_DIR",
    ] {
        if let Some(value) = inherited.get(OsStr::new(key)) {
            environment.insert(
                OsString::from(key),
                normalize_host_path(value, home.map(Path::as_os_str), cwd)?.into_os_string(),
            );
        }
    }
    Ok(())
}

pub(super) fn qualify(snapshot: &OwnedSnapshot) -> Result<QualifiedTools> {
    let controls = &snapshot.controls;
    let inputs = HostInputs::capture(&snapshot.root, &snapshot.scratch_root)?;
    let inherited_path = inputs.search_path();
    let home = inputs.home.as_deref().map(Path::new);
    let guard = resolve_guard(inherited_path, home)?;
    let pin = rust_release(&snapshot.root.join("rust-toolchain.toml"))?;
    let mise = find_executable("mise", inherited_path).context("missing Mise executable")?;
    let mise = fs::canonicalize(mise).context("resolving Mise executable")?;
    let mise_env = inputs.command_environment(&snapshot.root);
    let rustup = find_executable("rustup", inherited_path).context("missing rustup executable")?;
    let rustup = fs::canonicalize(rustup).context("resolving rustup executable")?;
    let resolved = resolve_managed_tools(&mise, &mise_env, &snapshot.root, controls)?;
    let rust_tools = resolve_rust_tools_with_controls(
        &rustup,
        &pin,
        OsStr::new(FIXED_SYSTEM_PATH),
        inputs.rustup_home.as_deref(),
        home.map(Path::as_os_str),
        controls,
    )?;
    let cargo = effective_cargo(&mise, &mise_env, &snapshot.root, controls)?;
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

    let environment =
        build_qualified_environment(snapshot, &managed_dir, &cargo, &pin, &rust_tools, &inputs)?;
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
    })
}

fn build_qualified_environment(
    snapshot: &OwnedSnapshot,
    managed_dir: &Path,
    cargo: &Path,
    pin: &str,
    rust_tools: &BTreeMap<&'static str, PathBuf>,
    inputs: &HostInputs,
) -> Result<BTreeMap<OsString, OsString>> {
    let mut environment = inputs.environment_for(&snapshot.root, OsStr::new(FIXED_SYSTEM_PATH));
    let rust_bin = rust_tools["rustc"]
        .parent()
        .context("rustc path has no bin directory")?
        .to_path_buf();
    environment.insert(
        OsString::from("PATH"),
        qualified_path(managed_dir, &rust_bin)?,
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
        OsString::from("CARGO_HOME"),
        inputs.cargo_home().as_os_str().to_owned(),
    );
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

fn qualified_path(managed_dir: &Path, rust_bin: &Path) -> Result<OsString> {
    let path_entries = std::iter::once(managed_dir.to_path_buf())
        .chain(std::iter::once(rust_bin.to_path_buf()))
        .chain(env::split_paths(OsStr::new(FIXED_SYSTEM_PATH)));
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

fn create_private_mise_config_dir(isolation_root: &Path, name: &str) -> Result<PathBuf> {
    let path = isolation_root.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "private Mise config path is not a real directory: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).with_context(|| {
                format!("creating private Mise config directory: {}", path.display())
            })?;
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "inspecting private Mise config directory: {}",
                    path.display()
                )
            });
        }
    }
    let physical = fs::canonicalize(&path).with_context(|| {
        format!(
            "resolving private Mise config directory: {}",
            path.display()
        )
    })?;
    ensure!(
        physical.starts_with(isolation_root),
        "private Mise config path escapes its isolation root: {}",
        path.display()
    );
    ensure!(
        crate::fs_util::read_dir_sorted(&physical)
            .with_context(|| format!(
                "reading private Mise config directory: {}",
                physical.display()
            ))?
            .is_empty(),
        "private Mise config directory is not empty: {}",
        physical.display()
    );
    Ok(physical)
}

fn validate_cargo_configuration(reviewed_root: &Path, cargo_home: &Path) -> Result<()> {
    let mut staged_visited = BTreeSet::new();
    let mut external_visited = BTreeSet::new();
    inspect_staged_cargo_configuration_tree(
        reviewed_root,
        &mut staged_visited,
        &mut external_visited,
    )?;

    let mut ancestor = reviewed_root.parent();
    while let Some(path) = ancestor {
        inspect_cargo_config_directory(
            path,
            reviewed_root,
            false,
            &mut staged_visited,
            &mut external_visited,
        )?;
        ancestor = path.parent();
    }

    for name in ["config", "config.toml"] {
        let path = cargo_home.join(name);
        if cargo_home.starts_with(reviewed_root) {
            inspect_staged_cargo_config(&path, reviewed_root, &mut staged_visited)?;
        } else {
            inspect_external_cargo_config(&path, &mut external_visited)?;
        }
    }
    Ok(())
}

fn inspect_staged_cargo_configuration_tree(
    reviewed_root: &Path,
    staged_visited: &mut BTreeSet<PathBuf>,
    external_visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let mut pending = vec![reviewed_root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        inspect_cargo_config_directory(
            &directory,
            reviewed_root,
            true,
            staged_visited,
            external_visited,
        )?;

        for entry in crate::fs_util::read_dir_sorted(&directory).with_context(|| {
            format!(
                "reading staged workspace directory: {}",
                directory.display()
            )
        })? {
            let name = entry.file_name();
            // Cargo never runs from Git metadata.
            if name == OsStr::new(".git") {
                continue;
            }
            let file_type = entry
                .file_type()
                .with_context(|| format!("inspecting staged path: {}", entry.path().display()))?;
            if !file_type.is_dir() {
                continue;
            }

            let path = entry.path();
            let physical = fs::canonicalize(&path).with_context(|| {
                format!("resolving staged workspace directory: {}", path.display())
            })?;
            ensure!(
                physical.starts_with(reviewed_root),
                "staged workspace directory escapes the reviewed snapshot: {}",
                path.display()
            );
            pending.push(physical);
        }
    }
    Ok(())
}

fn inspect_cargo_config_directory(
    directory: &Path,
    reviewed_root: &Path,
    staged: bool,
    staged_visited: &mut BTreeSet<PathBuf>,
    external_visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let cargo_dir = directory.join(".cargo");
    let metadata = match fs::symlink_metadata(&cargo_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("inspecting Cargo config directory: {}", cargo_dir.display())
            });
        }
    };
    let physical = fs::canonicalize(&cargo_dir)
        .with_context(|| format!("resolving Cargo config directory: {}", cargo_dir.display()))?;
    ensure!(
        metadata.is_dir() || metadata.file_type().is_symlink(),
        "Cargo config path is not a directory: {}",
        cargo_dir.display()
    );
    ensure!(
        physical.is_dir(),
        "Cargo config path is not a directory: {}",
        cargo_dir.display()
    );
    if staged {
        ensure!(
            physical.starts_with(reviewed_root),
            "staged Cargo config directory escapes the reviewed snapshot: {}",
            cargo_dir.display()
        );
    }

    for name in ["config", "config.toml"] {
        let config = physical.join(name);
        if staged {
            inspect_staged_cargo_config(&config, reviewed_root, staged_visited)?;
        } else {
            inspect_external_cargo_config(&config, external_visited)?;
        }
    }
    Ok(())
}

fn inspect_external_cargo_config(path: &Path, visited: &mut BTreeSet<PathBuf>) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("inspecting external Cargo config path: {}", path.display())
            });
        }
    };
    ensure!(
        !metadata.is_dir(),
        "external Cargo config path is not a file: {}",
        path.display()
    );
    let physical = fs::canonicalize(path)
        .with_context(|| format!("resolving external Cargo config path: {}", path.display()))?;
    ensure!(
        fs::metadata(&physical)
            .with_context(|| format!("inspecting external Cargo config path: {}", path.display()))?
            .is_file(),
        "external Cargo config path is not a regular file: {}",
        path.display()
    );
    if !visited.insert(physical.clone()) {
        return Ok(());
    }
    let contents = fs::read_to_string(&physical)
        .with_context(|| format!("reading external Cargo config path: {}", path.display()))?;
    let value = toml::from_str::<toml::Value>(&contents).map_err(|_| {
        anyhow::anyhow!(
            "external Cargo config is not valid TOML: {}",
            path.display()
        )
    })?;
    ensure!(
        value.as_table().is_some_and(toml::map::Map::is_empty),
        "external Cargo config is nonempty and not admitted: {}",
        path.display()
    );
    Ok(())
}

fn inspect_staged_cargo_config(
    path: &Path,
    reviewed_root: &Path,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("inspecting staged Cargo config path: {}", path.display())
            });
        }
    };
    ensure!(
        !metadata.is_dir(),
        "staged Cargo config path is not a file: {}",
        path.display()
    );
    let physical = fs::canonicalize(path)
        .with_context(|| format!("resolving staged Cargo config path: {}", path.display()))?;
    ensure!(
        physical.starts_with(reviewed_root),
        "staged Cargo config path escapes the reviewed snapshot: {}",
        path.display()
    );
    ensure!(
        fs::metadata(&physical)
            .with_context(|| format!("inspecting staged Cargo config path: {}", path.display()))?
            .is_file(),
        "staged Cargo config path is not a regular file: {}",
        path.display()
    );
    if !visited.insert(physical.clone()) {
        return Ok(());
    }
    let contents = fs::read_to_string(&physical)
        .with_context(|| format!("reading staged Cargo config path: {}", path.display()))?;
    let value = toml::from_str::<toml::Value>(&contents).map_err(|_| {
        anyhow::anyhow!("staged Cargo config is not valid TOML: {}", path.display())
    })?;
    let table = value.as_table().with_context(|| {
        format!(
            "staged Cargo config is not a TOML table: {}",
            path.display()
        )
    })?;
    let Some(includes) = table.get("include") else {
        return Ok(());
    };
    let includes = includes.as_array().with_context(|| {
        format!(
            "staged Cargo config include list is invalid: {}",
            path.display()
        )
    })?;
    for include in includes {
        let (include_path, optional) = if let Some(value) = include.as_str() {
            (PathBuf::from(value), false)
        } else {
            let include = include.as_table().with_context(|| {
                format!(
                    "staged Cargo config include entry is invalid: {}",
                    path.display()
                )
            })?;
            ensure!(
                include.keys().all(|key| key == "path" || key == "optional"),
                "staged Cargo config include entry has unknown fields: {}",
                path.display()
            );
            let include_path = include
                .get("path")
                .and_then(toml::Value::as_str)
                .with_context(|| {
                    format!(
                        "staged Cargo config include has no path: {}",
                        path.display()
                    )
                })?;
            let optional = include
                .get("optional")
                .map(|value| {
                    value.as_bool().with_context(|| {
                        format!(
                            "staged Cargo config include optional flag is invalid: {}",
                            path.display()
                        )
                    })
                })
                .transpose()?
                .unwrap_or(false);
            (PathBuf::from(include_path), optional)
        };
        let candidate = if include_path.is_absolute() {
            include_path
        } else {
            path.parent()
                .context("staged Cargo config path has no parent")?
                .join(include_path)
        };
        if optional && !candidate.exists() && !candidate.is_symlink() {
            continue;
        }
        ensure!(
            optional || candidate.exists() || candidate.is_symlink(),
            "staged Cargo config include is missing: {}",
            path.display()
        );
        inspect_staged_cargo_config(&candidate, reviewed_root, visited)?;
    }
    Ok(())
}

fn normalize_host_path(value: &OsStr, home: Option<&OsStr>, cwd: &Path) -> Result<PathBuf> {
    let bytes = value.as_bytes();
    let path = if bytes == b"~" || bytes.starts_with(b"~/") {
        let home = home.context("tilde path requires an admitted HOME")?;
        let suffix = if bytes == b"~" { &[][..] } else { &bytes[2..] };
        let mut expanded = PathBuf::from(home);
        if !suffix.is_empty() {
            expanded.push(OsStr::from_bytes(suffix));
        }
        expanded
    } else {
        ensure!(
            !bytes.starts_with(b"~"),
            "host path uses an unsupported named-user expansion"
        );
        PathBuf::from(value)
    };
    let resolved = crate::cargo_paths::resolve(&path, cwd)?;
    ensure!(
        resolved.is_absolute(),
        "admitted host path must be absolute"
    );
    Ok(resolved)
}

fn selected_storage_root(
    inherited: &BTreeMap<OsString, OsString>,
    mise_key: &str,
    xdg_key: &str,
    default_suffix: &str,
    home: Option<&Path>,
    cwd: &Path,
) -> Result<PathBuf> {
    if let Some(value) = inherited.get(OsStr::new(mise_key)) {
        return normalize_host_path(value, home.map(Path::as_os_str), cwd);
    }
    if let Some(value) = inherited
        .get(OsStr::new(xdg_key))
        .filter(|value| !value.is_empty())
    {
        let base = normalize_host_path(value, home.map(Path::as_os_str), cwd)?;
        return Ok(base.join("mise"));
    }
    let home = home.with_context(|| format!("{mise_key} needs an admitted home directory"))?;
    Ok(home.join(default_suffix))
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
