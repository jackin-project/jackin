// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor mount and role mutation plans.

use super::super::model::EditorTab;

use jackin_config::MountConfig;

#[must_use]
pub const fn editor_max_row_for_tab(
    tab: EditorTab,
    mount_count: usize,
    role_count: usize,
    secrets_row_count: usize,
    auth_row_count: usize,
) -> usize {
    match tab {
        EditorTab::General => 3,
        EditorTab::Mounts => mount_count,
        EditorTab::Roles => role_count,
        EditorTab::Secrets => secrets_row_count.saturating_sub(1),
        EditorTab::Auth => auth_row_count.saturating_sub(1),
    }
}

#[must_use]
pub const fn editor_mount_add_row_selected(selected_row: usize, mount_count: usize) -> bool {
    selected_row == mount_count
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorGeneralFieldModalPlan {
    RenameWorkspace,
    PickWorkdir,
    None,
}

#[must_use]
pub const fn editor_general_field_modal_plan(
    active_tab: EditorTab,
    selected_row: usize,
    has_mounts: bool,
) -> EditorGeneralFieldModalPlan {
    if !matches!(active_tab, EditorTab::General) {
        return EditorGeneralFieldModalPlan::None;
    }
    match selected_row {
        0 => EditorGeneralFieldModalPlan::RenameWorkspace,
        1 if has_mounts => EditorGeneralFieldModalPlan::PickWorkdir,
        _ => EditorGeneralFieldModalPlan::None,
    }
}

#[must_use]
pub const fn editor_role_add_row_selected(selected_row: usize, role_count: usize) -> bool {
    selected_row == role_count
}

pub fn cycle_mount_isolation_at(mounts: &mut [MountConfig], index: usize) {
    use jackin_config::MountIsolation::{Clone, Shared, Worktree};

    if let Some(mount) = mounts.get_mut(index) {
        mount.isolation = match mount.isolation {
            Shared => Worktree,
            Worktree => Clone,
            Clone => Shared,
        };
    }
}

pub fn toggle_allowed_role_at(
    allowed_roles: &mut Vec<String>,
    default_role: &mut Option<String>,
    role_names: &[String],
    index: usize,
) {
    let Some(role) = role_names.get(index) else {
        return;
    };
    let is_all_mode = allowed_roles.is_empty();
    let in_list = allowed_roles.iter().position(|allowed| allowed == role);

    if is_all_mode {
        *allowed_roles = role_names
            .iter()
            .filter(|allowed| allowed.as_str() != role.as_str())
            .cloned()
            .collect();
        if default_role.as_deref() == Some(role.as_str()) {
            *default_role = None;
        }
    } else if let Some(pos) = in_list {
        allowed_roles.remove(pos);
        if default_role.as_deref() == Some(role.as_str()) {
            *default_role = None;
        }
    } else {
        allowed_roles.push(role.clone());
        if allowed_roles.len() == role_names.len()
            && role_names.iter().all(|role| allowed_roles.contains(role))
        {
            allowed_roles.clear();
        }
    }
}

#[must_use]
pub fn add_role_to_workspace_editor<'a>(
    allowed_roles: &mut Vec<String>,
    mut role_names: impl Iterator<Item = &'a String> + 'a,
    key: &str,
) -> Option<usize> {
    if !allowed_roles.is_empty() && !allowed_roles.iter().any(|role| role == key) {
        allowed_roles.push(key.to_owned());
    }

    role_names.position(|role| role == key)
}

pub fn toggle_default_role_at(
    allowed_roles: &[String],
    default_role: &mut Option<String>,
    role_names: &[String],
    index: usize,
) {
    let Some(role) = role_names.get(index) else {
        return;
    };

    if default_role.as_deref() == Some(role.as_str()) {
        *default_role = None;
        return;
    }

    let role_allowed =
        allowed_roles.is_empty() || allowed_roles.iter().any(|allowed| allowed == role);
    if role_allowed {
        *default_role = Some(role.clone());
    }
}
