// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

pub(super) fn record_for(
    workspace: &str,
    container: &str,
    dst: &str,
    src: &str,
) -> IsolationRecord {
    IsolationRecord {
        workspace_name: Some(wn(workspace)),
        mount_dst: dst.into(),
        original_src: src.into(),
        isolation: MountIsolation::Worktree,
        worktree_path: format!("/data/{container}/isolated{dst}"),
        scratch_branch: format!("jackin/scratch/{container}"),
        base_commit: "abc".into(),
        selector_key: container
            .trim_start_matches(jackin_core::CONTAINER_PREFIX_DASH)
            .into(),
        container_name: container.into(),
        cleanup_status: CleanupStatus::Active,
    }
}

pub(super) fn paths_for(data: &std::path::Path) -> JackinPaths {
    JackinPaths {
        test_layout: true,
        home_dir: data.into(),
        jackin_home: data.into(),
        config_dir: data.into(),
        config_file: data.join("config.toml"),
        workspaces_dir: data.join("workspaces"),
        roles_dir: data.into(),
        data_dir: data.into(),
        cache_dir: data.into(),
    }
}

pub(super) fn mount(src: &str, dst: &str, iso: MountIsolation) -> jackin_config::MountConfig {
    jackin_config::MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: iso,
    }
}
