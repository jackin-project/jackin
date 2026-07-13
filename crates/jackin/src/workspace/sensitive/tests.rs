// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `sensitive`.
use super::*;

fn mount(src: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: "/container/path".to_owned(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    }
}

#[test]
fn detects_ssh_mount() {
    let mounts = vec![mount("/home/user/.ssh")];
    let hits = find_sensitive_mounts(&mounts);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].src, "/home/user/.ssh");
    assert!(hits[0].reason.contains("SSH"));
}

#[test]
fn detects_aws_mount() {
    let hits = find_sensitive_mounts(&[mount("/home/user/.aws")]);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].reason.contains("AWS"));
}

#[test]
fn detects_gnupg_mount() {
    let hits = find_sensitive_mounts(&[mount("/home/user/.gnupg")]);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].reason.contains("GPG"));
}

#[test]
fn detects_gcloud_mount() {
    let hits = find_sensitive_mounts(&[mount("/home/user/.config/gcloud")]);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].reason.contains("Google Cloud"));
}

#[test]
fn detects_kube_mount() {
    let hits = find_sensitive_mounts(&[mount("/home/user/.kube")]);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].reason.contains("Kubernetes"));
}

#[test]
fn detects_docker_mount() {
    let hits = find_sensitive_mounts(&[mount("/home/user/.docker")]);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].reason.contains("Docker"));
}

#[test]
fn ignores_safe_mounts() {
    let mounts = vec![
        mount("/home/user/projects"),
        mount("/tmp/workspace"),
        mount("/var/data"),
    ];
    assert!(find_sensitive_mounts(&mounts).is_empty());
}

#[test]
fn detects_multiple_sensitive_mounts() {
    let mounts = vec![
        mount("/home/user/.ssh"),
        mount("/home/user/projects"),
        mount("/home/user/.aws"),
    ];
    let hits = find_sensitive_mounts(&mounts);
    assert_eq!(hits.len(), 2);
}

#[test]
fn handles_trailing_slash_on_sensitive_mount() {
    let hits = find_sensitive_mounts(&[mount("/home/user/.ssh/")]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn does_not_match_partial_name() {
    // ".sshd" should NOT match ".ssh"
    let hits = find_sensitive_mounts(&[mount("/home/user/.sshd")]);
    assert!(hits.is_empty());
}
