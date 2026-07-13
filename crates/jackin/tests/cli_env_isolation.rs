#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::manual_assert,
    clippy::duration_suboptimal_units,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    unreachable_pub,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
#![cfg(unix)]

//! Integration coverage for the three isolation env vars
//! `JACKIN_HOME_DIR`, `JACKIN_CONFIG_DIR`, and `JACKIN_CONSTRUCT_IMAGE`.
//!
//! Unit tests in `src/paths.rs` cover `resolve_with_env` directly, but a
//! regression in `JackinPaths::detect()` (e.g. accidentally reading the
//! old `JACKIN_HOME` env name) would slip past them. These tests spawn
//! the binary so a real `std::env::var_os` lookup is exercised.

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn jackin_home_dir_relocates_state_writes() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let jackin_root = temp.path().join("isolated-state");
    let config_root = temp.path().join("isolated-config");
    fs::create_dir_all(&home).unwrap();

    // `jackin workspace list` triggers `JackinPaths::detect()` +
    // `AppConfig::load_or_init`, which together materialize the
    // config + data trees under whatever the env vars resolve to.
    let _unused = Command::cargo_bin("jackin")
        .unwrap()
        .env("HOME", &home)
        .env("JACKIN_HOME_DIR", &jackin_root)
        .env("JACKIN_CONFIG_DIR", &config_root)
        .args(["workspace", "list"])
        .assert()
        .success();

    // Config tree landed under the JACKIN_CONFIG_DIR override.
    assert!(
        config_root.join("config.toml").exists(),
        "config.toml not created under JACKIN_CONFIG_DIR: {}",
        config_root.display(),
    );
    assert!(
        config_root.join("workspaces").is_dir(),
        "workspaces dir not created under JACKIN_CONFIG_DIR",
    );
    // Data tree landed under the JACKIN_HOME_DIR override.
    assert!(
        jackin_root.join("data").is_dir(),
        "data dir not created under JACKIN_HOME_DIR: {}",
        jackin_root.display(),
    );
    assert!(
        jackin_root.join("roles").is_dir(),
        "roles dir not created under JACKIN_HOME_DIR",
    );
    // HOME-relative defaults were NOT used.
    assert!(
        !home.join(".config/jackin/config.toml").exists(),
        "config.toml unexpectedly created under HOME/.config/jackin: {}",
        home.display(),
    );
    assert!(
        !home.join(".jackin/data").exists(),
        "data unexpectedly created under HOME/.jackin: {}",
        home.display(),
    );
}
