// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{PrPaths, enter_lines, env_lines};

#[test]
fn paths_derive_repo_env_config_and_home_from_root() {
    let paths = PrPaths::from_root(PathBuf::from("pr-7"));
    assert_eq!(paths.repo, PathBuf::from("pr-7/jackin"));
    assert_eq!(paths.env_file, PathBuf::from("pr-7/env.sh"));
    assert_eq!(paths.config, PathBuf::from("pr-7/state/config"));
    assert_eq!(paths.home, PathBuf::from("pr-7/state/home"));
}

#[test]
fn env_lines_export_debug_path_and_config_dirs() {
    let paths = PrPaths::from_root(PathBuf::from("pr-7"));
    let lines = env_lines(&paths);
    assert!(lines[0].starts_with("export PATH=\"pr-7/jackin/target/debug:"));
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("export JACKIN_CONFIG_DIR="))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("export JACKIN_HOME_DIR="))
    );
}

#[test]
fn enter_lines_print_cd_source_and_binary_probe() {
    let paths = PrPaths::from_root(PathBuf::from("pr-7"));
    let [cd, source, which] = enter_lines(&paths);
    assert!(cd.starts_with("cd "), "{cd}");
    assert!(source.starts_with("source "), "{source}");
    assert_eq!(which, "which jackin");
}
