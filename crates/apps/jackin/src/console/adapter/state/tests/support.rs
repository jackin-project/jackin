// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn refresh_instances(state: &mut ManagerState<'_>, paths: &JackinPaths) {
    const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
    let now = std::time::Instant::now();
    if let Some(last) = state.instances_last_refresh
        && now.duration_since(last) < REFRESH_INTERVAL
    {
        return;
    }
    state.instances_last_refresh = Some(now);
    match load_instance_refresh_snapshot(paths) {
        Ok(snapshot) => state.apply_instance_refresh_snapshot(snapshot),
        Err(error) => state.apply_instance_refresh_error(&error),
    }
}

pub(super) fn empty_ws(workdir: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: workdir.into(),
        ..Default::default()
    }
}

pub(super) fn editor_with_one_shared_mount() -> EditorState<'static> {
    use std::collections::BTreeMap;
    let ws = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: String::new(),
        mounts: vec![MountConfig {
            src: "/host/a".into(),
            dst: "/host/a".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        allowed_roles: vec![],
        default_role: None,
        default_agent: None,
        last_role: None,
        env: BTreeMap::default(),
        roles: BTreeMap::default(),
        keep_awake: KeepAwakeConfig::default(),
        docker: None,
        accounts: Vec::new(),
        account_bindings: BTreeMap::default(),
        github: None,
        git_pull_on_entry: false,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        default_launch: None,
    };
    let mut e = EditorState::new_edit("ws".into(), ws);
    e.active_tab = EditorTab::Mounts;
    e.active_field = FieldFocus::Row(0);
    e
}
