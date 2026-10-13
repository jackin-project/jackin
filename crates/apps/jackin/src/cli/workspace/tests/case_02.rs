// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parse_mount_isolation_accepts_clone() {
    let (dst, mode) = parse_mount_isolation("/workspace/jackin=clone").unwrap();
    assert_eq!(dst, "/workspace/jackin");
    assert_eq!(mode, MountIsolation::Clone);
}

#[test]
fn parse_mount_isolation_rejects_missing_equals() {
    let err = parse_mount_isolation("/workspace/jackin").unwrap_err();
    assert!(err.to_string().contains("expected DST=TYPE"));
}

#[test]
fn parse_mount_isolation_rejects_empty_dst() {
    let err = parse_mount_isolation("=worktree").unwrap_err();
    assert!(err.to_string().contains("destination cannot be empty"));
}
