// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{capsule_broad_reason, construct_reason, parse_changed_files};

#[test]
fn construct_reason_fires_for_construct_inputs() {
    assert!(construct_reason("docker/construct/Dockerfile").is_some());
    assert!(construct_reason("docker-bake.hcl").is_some());
    assert!(construct_reason("mise.toml").is_some());
    assert!(construct_reason("crates/tools/jackin-xtask/src/construct/build.rs").is_some());
}

#[test]
fn construct_reason_ignores_unrelated_files() {
    assert_eq!(construct_reason("crates/ui/jackin-tui/src/lib.rs"), None);
    assert_eq!(construct_reason("README.md"), None);
}

#[test]
fn capsule_broad_reason_fires_for_workspace_inputs() {
    assert!(capsule_broad_reason("Cargo.lock").is_some());
    assert!(capsule_broad_reason("Cargo.toml").is_some());
    assert!(capsule_broad_reason(".cargo/config.toml").is_some());
}

#[test]
fn capsule_broad_reason_ignores_crate_files() {
    assert_eq!(
        capsule_broad_reason("crates/ui/jackin-tui/src/lib.rs"),
        None
    );
    assert_eq!(capsule_broad_reason("docker/construct/Dockerfile"), None);
}

#[test]
fn changed_files_parsing_trims_blanks() {
    let files = parse_changed_files("a/b.rs\n\n  c/d.rs\n").expect("parses");
    assert_eq!(files, ["a/b.rs".to_owned(), "c/d.rs".to_owned()]);
}

#[test]
fn changed_files_parsing_rejects_empty_diff() {
    parse_changed_files("\n  \n").unwrap_err();
}
