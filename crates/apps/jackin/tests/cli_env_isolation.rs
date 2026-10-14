#![cfg(unix)]
// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

    let _unused = Command::cargo_bin("jackin")
        .unwrap()
        .env("HOME", &home)
        .env("JACKIN_HOME_DIR", &jackin_root)
        .env("JACKIN_CONFIG_DIR", &config_root)
        .args(["workspace", "list"])
        .assert()
        .success();

    assert!(
        config_root.join("config.toml").exists(),
        "config.toml not created under JACKIN_CONFIG_DIR: {}",
        config_root.display(),
    );
    assert!(
        config_root.join("workspaces").is_dir(),
        "workspaces dir not created under JACKIN_CONFIG_DIR",
    );

    assert!(
        jackin_root.join("data").is_dir(),
        "data dir not created under JACKIN_HOME_DIR: {}",
        jackin_root.display(),
    );
    assert!(
        jackin_root.join("roles").is_dir(),
        "roles dir not created under JACKIN_HOME_DIR",
    );

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
