// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn ws_with_allowed(allowed: Vec<String>) -> WorkspaceConfig {
    WorkspaceConfig {
        allowed_roles: allowed,
        ..WorkspaceConfig::default()
    }
}

pub(super) fn workspace_with_workdir_and_dst(workdir: &str, dst: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: workdir.to_owned(),
        mounts: vec![MountConfig {
            src: "/tmp/src".to_owned(),
            dst: dst.to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..Default::default()
    }
}

pub(super) fn worktree_mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Worktree,
    }
}

pub(super) fn clone_mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Clone,
    }
}

pub(super) fn shared_mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}
