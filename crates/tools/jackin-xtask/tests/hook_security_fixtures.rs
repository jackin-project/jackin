// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use anyhow::{Context, Result, ensure};
use jackin_process::{ExecRequest, exec_sync};
use std::{
    env,
    ffi::{OsStr, OsString},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const PASSTHROUGH_HOST_KEYS: &[&str] = &[
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
    "MISE_TMP_DIR",
    "MISE_INSTALL_STORE_DIR",
    "MISE_PLUGINS_DIR",
    "MISE_DOWNLOADS_DIR",
    "MISE_SYSTEM_DATA_DIR",
    "MISE_SYSTEM_INSTALLS_DIR",
    "MISE_SYSTEM_SHIMS_DIR",
    "MBX_CACHE_DIR",
    "MBX_SESSION_PATH",
];

#[test]
fn canonical_hook_security_fixtures_run_with_prebuilt_native_binaries() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .context("resolving security fixture repository")?;
    let temporary = tempfile::Builder::new()
        .prefix("jackin-hook-security-")
        .tempdir()
        .context("creating private security fixture temporary parent")?;
    let environment = hook_fixture_environment(temporary.path())?;
    let mut audit_environment = environment.clone();
    audit_environment.push((
        OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
        root.as_os_str().to_owned(),
    ));
    let request = ExecRequest::new("sh", ["scripts/test-pre-push-security"])
        .cwd(&root)
        .env_clear()
        .envs([(
            OsStr::new("JACKIN_PRECOMMIT_OWNER_TEST_BIN"),
            OsStr::new(env!("CARGO_BIN_EXE_jackin-precommit-owner")),
        )])
        .envs(environment)
        .timeout(Duration::from_secs(120))
        .output_limits(1024 * 1024, 1024 * 1024);
    assert_fixture(&request, "pre-push pinned launcher fixture passed")?;

    let request = ExecRequest::new(
        env!("CARGO_BIN_EXE_jackin-xtask"),
        ["security-audit-fixtures"],
    )
    .cwd(&root)
    .env_clear()
    .envs(audit_environment)
    .timeout(Duration::from_secs(120))
    .output_limits(1024 * 1024, 1024 * 1024);
    assert_fixture(&request, "pinned security-audit synthetic fixtures passed")
}

fn hook_fixture_environment(temporary: &Path) -> Result<Vec<(OsString, OsString)>> {
    let invocation_cwd = env::current_dir().context("reading hook fixture invocation directory")?;
    let invocation_cwd = std::fs::canonicalize(invocation_cwd)
        .context("resolving physical hook fixture invocation directory")?;
    let host_home = env::var_os("HOME").context("reading admitted host HOME for storage roots")?;
    let host_home = PathBuf::from(host_home);
    ensure!(
        host_home.is_absolute(),
        "admitted host HOME must be absolute"
    );
    let private_home = create_private_directory(temporary, "home")?;
    let private_mise_config = create_private_directory(temporary, "mise-config")?;
    let private_mise_system_config = create_private_directory(temporary, "mise-system-config")?;
    let path = env::var_os("PATH").context("reading discovery PATH")?;
    let cargo_home = selected_host_path("CARGO_HOME", &host_home, &invocation_cwd)
        .unwrap_or_else(|| host_home.join(".cargo").into_os_string());
    let rustup_home = selected_host_path("RUSTUP_HOME", &host_home, &invocation_cwd)
        .unwrap_or_else(|| host_home.join(".rustup").into_os_string());
    let mise_data = selected_storage_path(
        "MISE_DATA_DIR",
        "XDG_DATA_HOME",
        ".local/share/mise",
        &host_home,
        &invocation_cwd,
    );
    let mise_cache = selected_storage_path(
        "MISE_CACHE_DIR",
        "XDG_CACHE_HOME",
        if cfg!(target_os = "macos") {
            "Library/Caches/mise"
        } else {
            ".cache/mise"
        },
        &host_home,
        &invocation_cwd,
    );
    let mise_state = selected_storage_path(
        "MISE_STATE_DIR",
        "XDG_STATE_HOME",
        ".local/state/mise",
        &host_home,
        &invocation_cwd,
    );
    let mise_installs = selected_host_path("MISE_INSTALLS_DIR", &host_home, &invocation_cwd)
        .unwrap_or_else(|| PathBuf::from(&mise_data).join("installs").into_os_string());

    let mut environment = Vec::new();
    for (key, value) in [
        ("PATH", path),
        ("HOME", private_home.into_os_string()),
        ("TMPDIR", temporary.as_os_str().to_owned()),
        ("CARGO_HOME", cargo_home),
        ("RUSTUP_HOME", rustup_home),
        ("MISE_DATA_DIR", mise_data),
        ("MISE_CACHE_DIR", mise_cache),
        ("MISE_STATE_DIR", mise_state),
        ("MISE_INSTALLS_DIR", mise_installs),
        ("MISE_CONFIG_DIR", private_mise_config.into_os_string()),
        (
            "MISE_SYSTEM_CONFIG_DIR",
            private_mise_system_config.into_os_string(),
        ),
        ("MISE_AUTO_INSTALL", OsString::from("0")),
        ("MISE_EXEC_AUTO_INSTALL", OsString::from("false")),
        ("MBX_CARGO_SHIM_MODE", OsString::from("1")),
        ("GIT_CONFIG_GLOBAL", OsString::from("/dev/null")),
        ("GIT_CONFIG_SYSTEM", OsString::from("/dev/null")),
        ("GIT_CONFIG_NOSYSTEM", OsString::from("1")),
    ] {
        environment.push((OsString::from(key), value));
    }

    for key in PASSTHROUGH_HOST_KEYS {
        if let Some(value) = env::var_os(key) {
            let value = if key.starts_with("MISE_") || key.starts_with("MBX_") {
                host_path_input(&value, &host_home, &invocation_cwd)
            } else {
                value
            };
            environment.push((OsString::from(key), value));
        }
    }
    for key in ["TMP", "TEMP"] {
        if let Some(value) = env::var_os(key) {
            environment.push((
                OsString::from(key),
                host_path_input(&value, &host_home, &invocation_cwd),
            ));
        }
    }
    Ok(environment)
}

fn selected_storage_path(
    mise_key: &str,
    xdg_key: &str,
    default_suffix: &str,
    host_home: &Path,
    invocation_cwd: &Path,
) -> OsString {
    if let Some(value) = env::var_os(mise_key) {
        return host_path_input(&value, host_home, invocation_cwd);
    }
    if let Some(value) = env::var_os(xdg_key).filter(|value| !value.is_empty()) {
        return PathBuf::from(host_path_input(&value, host_home, invocation_cwd))
            .join("mise")
            .into_os_string();
    }
    host_home.join(default_suffix).into_os_string()
}

fn selected_host_path(key: &str, host_home: &Path, invocation_cwd: &Path) -> Option<OsString> {
    env::var_os(key).map(|value| host_path_input(&value, host_home, invocation_cwd))
}

fn host_path_input(value: &OsStr, host_home: &Path, invocation_cwd: &Path) -> OsString {
    let bytes = value.as_bytes();
    let path = if bytes == b"~" || bytes.starts_with(b"~/") {
        let suffix = if bytes == b"~" { &[][..] } else { &bytes[2..] };
        let mut path = host_home.to_path_buf();
        if !suffix.is_empty() {
            path.push(OsStr::from_bytes(suffix));
        }
        path
    } else {
        PathBuf::from(value)
    };
    if path.is_absolute() {
        path.into_os_string()
    } else {
        invocation_cwd.join(path).into_os_string()
    }
}

#[test]
fn host_path_input_anchors_relative_roots_and_preserves_raw_bytes() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let host_home = temporary.path().join("private host home");
    let invocation_cwd = temporary.path().join("original invocation");
    let relative = OsStr::from_bytes(b"cargo home/registry-\xff");
    assert_eq!(
        PathBuf::from(host_path_input(relative, &host_home, &invocation_cwd)),
        invocation_cwd.join(relative)
    );

    let home_relative = OsStr::from_bytes(b"~/rustup home/toolchain-\xff");
    assert_eq!(
        PathBuf::from(host_path_input(home_relative, &host_home, &invocation_cwd)),
        host_home.join(OsStr::from_bytes(b"rustup home/toolchain-\xff"))
    );

    let absolute = host_home.join("mise data");
    assert_eq!(
        host_path_input(absolute.as_os_str(), &host_home, &invocation_cwd),
        absolute.into_os_string()
    );
    Ok(())
}

fn create_private_directory(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = parent.join(name);
    std::fs::create_dir(&path).with_context(|| format!("creating private {name}"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("protecting private {name}"))?;
    Ok(path)
}

fn assert_fixture(request: &ExecRequest, marker: &str) -> Result<()> {
    let output = exec_sync(request).context("running genuine hook security fixture")?;
    ensure!(
        output.success && !output.timed_out,
        "hook security fixture failed (status {:?}): {}{}",
        output.code,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        String::from_utf8_lossy(&output.stdout).contains(marker),
        "hook security fixture omitted its completion marker: {marker}"
    );
    Ok(())
}
