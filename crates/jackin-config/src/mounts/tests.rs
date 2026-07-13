// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_mount_spec_with_optional_readonly_suffix() {
    let mount = parse_mount_spec("/tmp/cache:/workspace/cache:ro").unwrap();

    assert_eq!(mount.src, "/tmp/cache");
    assert_eq!(mount.dst, "/workspace/cache");
    assert!(mount.readonly);
}

#[test]
fn parses_mount_spec_with_src_only() {
    let mount = parse_mount_spec("/tmp/project").unwrap();

    assert_eq!(mount.src, "/tmp/project");
    assert_eq!(mount.dst, "/tmp/project");
    assert!(!mount.readonly);
}

#[test]
fn parses_mount_spec_with_src_only_readonly() {
    let mount = parse_mount_spec("/tmp/project:ro").unwrap();

    assert_eq!(mount.src, "/tmp/project");
    assert_eq!(mount.dst, "/tmp/project");
    assert!(mount.readonly);
}

#[test]
fn parses_mount_spec_with_tilde_src_only() {
    let home = std::env::var("HOME").unwrap();
    let mount = parse_mount_spec("~/projects").unwrap();

    assert_eq!(mount.src, format!("{home}/projects"));
    assert_eq!(mount.dst, format!("{home}/projects"));
    assert!(!mount.readonly);
}

#[test]
fn parse_mount_spec_resolved_resolves_relative_src_and_dst() {
    let cwd = std::env::current_dir().unwrap();
    let mount = parse_mount_spec_resolved("my-project").unwrap();
    let expected = cwd.join("my-project").display().to_string();

    assert_eq!(mount.src, expected);
    assert_eq!(mount.dst, expected);
    assert!(!mount.readonly);
}

#[test]
fn parse_mount_spec_resolved_resolves_relative_src_with_explicit_dst() {
    let cwd = std::env::current_dir().unwrap();
    let mount = parse_mount_spec_resolved("my-project:/workspace/project").unwrap();

    assert_eq!(mount.src, cwd.join("my-project").display().to_string());
    assert_eq!(mount.dst, "/workspace/project");
    assert!(!mount.readonly);
}

#[test]
fn parse_mount_spec_resolved_normalizes_dotdot_in_relative_path() {
    let cwd = std::env::current_dir().unwrap();
    let mount = parse_mount_spec_resolved("../sibling-project").unwrap();
    let expected = cwd.parent().unwrap().join("sibling-project");

    assert_eq!(mount.src, expected.display().to_string());
    assert_eq!(mount.dst, expected.display().to_string());
    assert!(!mount.src.contains(".."));
}

#[test]
fn parse_mount_spec_resolved_normalizes_dot_path() {
    let cwd = std::env::current_dir().unwrap();
    let mount = parse_mount_spec_resolved(".").unwrap();

    assert_eq!(mount.src, cwd.display().to_string());
    assert_eq!(mount.dst, cwd.display().to_string());
}

#[test]
fn parse_mount_spec_does_not_resolve_relative_paths() {
    let mount = parse_mount_spec("my-project").unwrap();

    assert_eq!(mount.src, "my-project");
    assert_eq!(mount.dst, "my-project");
}
