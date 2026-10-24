// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn mount(src: &str, dst: &str, isolation: jackin_core::MountIsolation) -> workspace::MountConfig {
    workspace::MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation,
    }
}

#[test]
fn prospective_mounts_apply_worktree_to_shared_and_clone_to_worktree_overrides() {
    let current = WorkspaceConfig {
        mounts: vec![
            mount(
                "/repo",
                "/workspace/repo",
                jackin_core::MountIsolation::Worktree,
            ),
            mount(
                "/docs",
                "/workspace/docs",
                jackin_core::MountIsolation::Clone,
            ),
        ],
        ..WorkspaceConfig::default()
    };
    let actual = prospective_mounts_for_edit(
        &current,
        &[],
        &[],
        false,
        &[
            (
                "/workspace/repo".into(),
                jackin_core::MountIsolation::Shared,
            ),
            (
                "/workspace/docs".into(),
                jackin_core::MountIsolation::Worktree,
            ),
        ],
    )
    .unwrap();

    assert_eq!(
        actual,
        vec![
            mount(
                "/repo",
                "/workspace/repo",
                jackin_core::MountIsolation::Shared,
            ),
            mount(
                "/docs",
                "/workspace/docs",
                jackin_core::MountIsolation::Worktree,
            ),
        ]
    );
}

#[test]
fn prospective_mounts_preserve_identity_without_overrides() {
    let current = WorkspaceConfig {
        mounts: vec![
            mount(
                "/repo",
                "/workspace/repo",
                jackin_core::MountIsolation::Worktree,
            ),
            mount(
                "/docs",
                "/workspace/docs",
                jackin_core::MountIsolation::Clone,
            ),
        ],
        ..WorkspaceConfig::default()
    };
    let actual = prospective_mounts_for_edit(&current, &[], &[], false, &[]).unwrap();

    assert_eq!(actual, current.mounts);
}

#[test]
fn prospective_mounts_apply_upserts_before_isolation_overrides() {
    let current = WorkspaceConfig {
        workdir: "/repo".into(),
        mounts: vec![
            mount("/repo", "/repo", jackin_core::MountIsolation::Shared),
            mount(
                "/old/docs",
                "/workspace/docs",
                jackin_core::MountIsolation::Shared,
            ),
            mount(
                "/old/cache",
                "/workspace/cache",
                jackin_core::MountIsolation::Clone,
            ),
        ],
        ..WorkspaceConfig::default()
    };
    let actual = prospective_mounts_for_edit(
        &current,
        &[
            mount(
                "/new/docs",
                "/workspace/docs",
                jackin_core::MountIsolation::Shared,
            ),
            mount(
                "/new/data",
                "/workspace/data",
                jackin_core::MountIsolation::Clone,
            ),
        ],
        &["/workspace/cache".into()],
        true,
        &[
            ("/workspace/docs".into(), jackin_core::MountIsolation::Clone),
            (
                "/workspace/data".into(),
                jackin_core::MountIsolation::Worktree,
            ),
        ],
    )
    .unwrap();

    assert_eq!(
        actual,
        vec![
            mount(
                "/new/docs",
                "/workspace/docs",
                jackin_core::MountIsolation::Clone,
            ),
            mount(
                "/new/data",
                "/workspace/data",
                jackin_core::MountIsolation::Worktree,
            ),
        ]
    );
}

#[test]
fn prospective_mounts_reject_unknown_and_removed_override_destinations() {
    let current = WorkspaceConfig {
        mounts: vec![mount(
            "/repo",
            "/workspace/repo",
            jackin_core::MountIsolation::Worktree,
        )],
        ..WorkspaceConfig::default()
    };

    let unknown = prospective_mounts_for_edit(
        &current,
        &[],
        &[],
        false,
        &[(
            "/workspace/unknown".into(),
            jackin_core::MountIsolation::Shared,
        )],
    )
    .unwrap_err();
    assert!(
        unknown
            .to_string()
            .contains("unknown destination `/workspace/unknown`")
    );

    let removed = prospective_mounts_for_edit(
        &current,
        &[],
        &["/workspace/repo".into()],
        false,
        &[(
            "/workspace/repo".into(),
            jackin_core::MountIsolation::Shared,
        )],
    )
    .unwrap_err();
    assert!(
        removed
            .to_string()
            .contains("unknown destination `/workspace/repo`")
    );
}
