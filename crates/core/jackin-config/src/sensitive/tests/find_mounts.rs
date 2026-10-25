// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::MountIsolation;

fn mount(src: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: "/workspace/data".to_owned(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}

#[test]
fn flags_ssh_mount_with_reason() {
    let hits = find_sensitive_mounts(&[mount("/home/operator/.ssh")]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].src, "/home/operator/.ssh");
    assert!(
        hits[0].reason.contains("SSH"),
        "unexpected reason: {}",
        hits[0].reason
    );
}

#[test]
fn flags_nested_github_config() {
    let hits = find_sensitive_mounts(&[mount("/home/operator/.config/gh")]);
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0].reason.contains("GitHub"),
        "unexpected reason: {}",
        hits[0].reason
    );
}

#[test]
fn ignores_benign_and_lookalike_paths() {
    let mounts = [
        mount("/data/work"),
        mount("/home/operator/.ssh-backup"),
        mount("/home/operator/projects"),
    ];
    assert!(find_sensitive_mounts(&mounts).is_empty());
}

#[test]
fn normalize_prefers_canonicalized_source() {
    let mounts = [mount("/data/work")];
    let canonicalized =
        normalize_sensitive_mount_sources(&mounts, |_| Some(PathBuf::from("/Volumes/real")));
    assert_eq!(canonicalized[0].src, "/Volumes/real");
    assert_eq!(canonicalized[0].dst, "/workspace/data");
}

#[test]
fn normalize_keeps_resolved_source_without_canonicalization() {
    let mounts = [mount("/data/work")];
    let kept = normalize_sensitive_mount_sources(&mounts, |_| None);
    assert_eq!(kept[0].src, "/data/work");
}
