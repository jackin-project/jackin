// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::tempdir;

use super::{ToolchainConfig, mise_toolchain_at, valid_toolchain};

fn executable(path: &std::path::Path, exit_code: u8) {
    fs::write(path, format!("#!/bin/sh\nexit {exit_code}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn toolchain_requires_both_rustc_and_cargo() {
    let config = ToolchainConfig {
        channel: "1.97.0".to_owned(),
        components: Vec::new(),
        targets: Vec::new(),
    };
    let temp = tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let rustc = bin.join("rustc");
    executable(&rustc, 0);
    assert!(!valid_toolchain(temp.path(), &config));

    let cargo = bin.join("cargo");
    executable(&cargo, 0);
    assert!(valid_toolchain(temp.path(), &config));

    executable(&cargo, 1);
    assert!(!valid_toolchain(temp.path(), &config));
}

#[test]
fn mise_storage_uses_the_exact_pinned_version() {
    let config = ToolchainConfig {
        channel: "1.97.0".to_owned(),
        components: Vec::new(),
        targets: Vec::new(),
    };
    let temp = tempdir().unwrap();
    let bin = temp.path().join("installs/rust/1.97.0/bin");
    fs::create_dir_all(&bin).unwrap();
    for binary in ["rustc", "cargo"] {
        let path = bin.join(binary);
        executable(&path, 0);
    }

    assert_eq!(
        mise_toolchain_at(temp.path(), "1.97.0", &config),
        Some(temp.path().join("installs/rust/1.97.0"))
    );
    assert_eq!(mise_toolchain_at(temp.path(), "1.96.0", &config), None);
}
