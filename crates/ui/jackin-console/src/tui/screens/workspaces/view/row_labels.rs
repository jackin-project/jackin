// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace row labels and titles.

use super::{
    Disclosure, WorkspaceListDisplayRow, WorkspaceListDisplayRowFacts,
    WorkspaceListDisplayRowsFacts, WorkspaceListRowTone,
};

use crate::tui::screens::workspaces::model::ManagerListRow;

#[must_use]
pub fn current_directory_display_row(
    disclosure: Disclosure,
    selected: bool,
    hovered: bool,
) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: "Current directory".to_owned(),
        tone: WorkspaceListRowTone::White,
        disclosure,
        selected,
        hovered,
    }
}

#[must_use]
pub fn new_workspace_display_row(selected: bool, hovered: bool) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: new_workspace_list_label().to_owned(),
        tone: WorkspaceListRowTone::White,
        disclosure: Disclosure::None,
        selected,
        hovered,
    }
}

/// Backing data a tree instance row needs to render its label: the id, role,
/// and status (status drives the compact `[state]` tag for failed instances).
#[derive(Debug, Clone)]
pub struct InstanceRowLabel {
    pub instance_id: String,
    pub role_key: String,
    pub status: jackin_core::InstanceStatus,
}

#[must_use]
pub fn workspace_instance_list_label(
    instance_id: &str,
    role_key: &str,
    status: jackin_core::InstanceStatus,
) -> String {
    use jackin_core::InstanceStatus as S;
    match status {
        // Live instances read as today; failed/stopped ones carry a compact
        // state tag so the operator can tell them apart in the tree (D15).
        S::Active | S::Running => format!("{instance_id}  {role_key}"),
        other => format!("{instance_id}  {role_key}  [{}]", other.short_label()),
    }
}

#[must_use]
pub fn workspace_instance_display_row(
    instance_id: &str,
    role_key: &str,
    status: jackin_core::InstanceStatus,
    selected: bool,
    hovered: bool,
) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: workspace_instance_list_label(instance_id, role_key, status),
        tone: WorkspaceListRowTone::Instance,
        disclosure: Disclosure::None,
        selected,
        hovered,
    }
}

#[must_use]
pub fn workspace_list_display_row_for_row(
    facts: WorkspaceListDisplayRowFacts,
    current_dir_instance: impl FnOnce(usize) -> Option<InstanceRowLabel>,
    saved_workspace: impl FnOnce(usize) -> Option<(String, bool, bool)>,
    workspace_instance: impl FnOnce(usize, usize) -> Option<InstanceRowLabel>,
) -> Option<WorkspaceListDisplayRow> {
    match facts.row {
        ManagerListRow::CurrentDirectory => Some(current_directory_display_row(
            Disclosure::for_instances(facts.current_dir_has_instances, facts.current_dir_expanded),
            facts.selected,
            facts.hovered,
        )),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => {
            current_dir_instance(inst_idx).map(|row| {
                workspace_instance_display_row(
                    &row.instance_id,
                    &row.role_key,
                    row.status,
                    facts.selected,
                    facts.hovered,
                )
            })
        }
        ManagerListRow::SavedWorkspace(idx) => {
            saved_workspace(idx).map(|(name, expanded, has_instances)| WorkspaceListDisplayRow {
                label: name,
                tone: WorkspaceListRowTone::Workspace,
                disclosure: Disclosure::for_instances(has_instances, expanded),
                selected: facts.selected,
                hovered: facts.hovered,
            })
        }
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => workspace_instance(ws_idx, inst_idx)
            .map(|row| {
                workspace_instance_display_row(
                    &row.instance_id,
                    &row.role_key,
                    row.status,
                    facts.selected,
                    facts.hovered,
                )
            }),
        ManagerListRow::NewWorkspace => {
            Some(new_workspace_display_row(facts.selected, facts.hovered))
        }
    }
}

#[must_use]
pub fn workspace_list_display_rows(
    facts: WorkspaceListDisplayRowsFacts<'_>,
    mut current_dir_instance: impl FnMut(usize) -> Option<InstanceRowLabel>,
    mut saved_workspace: impl FnMut(usize) -> Option<(String, bool, bool)>,
    mut workspace_instance: impl FnMut(usize, usize) -> Option<InstanceRowLabel>,
) -> Vec<Option<WorkspaceListDisplayRow>> {
    facts
        .visual_rows
        .iter()
        .enumerate()
        .map(|(idx, visual_row)| {
            visual_row.as_ref().and_then(|row| {
                workspace_list_display_row_for_row(
                    WorkspaceListDisplayRowFacts {
                        row: *row,
                        selected: idx == facts.visual_selected,
                        hovered: facts.hovered_row == Some(*row),
                        current_dir_expanded: facts.current_dir_expanded,
                        current_dir_has_instances: facts.current_dir_has_instances,
                    },
                    &mut current_dir_instance,
                    &mut saved_workspace,
                    &mut workspace_instance,
                )
            })
        })
        .collect()
}

#[must_use]
pub fn instance_purge_confirm_label(container_base: &str, role_key: Option<&str>) -> String {
    role_key.map_or_else(
        || container_base.to_owned(),
        |role_key| format!("{container_base} ({role_key})"),
    )
}

#[must_use]
pub fn workspace_instance_pane_identity_label(
    account_id: Option<&str>,
    config_id: Option<&str>,
) -> String {
    match (
        account_id.filter(|id| !id.is_empty()),
        config_id.filter(|id| !id.is_empty()),
    ) {
        (Some(account_id), Some(config_id)) => format!("{account_id} · {config_id}"),
        (Some(account_id), None) => account_id.to_owned(),
        (None, Some(config_id)) => config_id.to_owned(),
        (None, None) => "shell".to_owned(),
    }
}

#[must_use]
pub const fn current_directory_workspace_title() -> &'static str {
    "Current directory"
}

#[must_use]
pub const fn new_workspace_list_label() -> &'static str {
    "+ New workspace"
}

#[must_use]
pub fn picker_sidebar_title(label: &str) -> String {
    format!(" {label} ")
}

#[must_use]
pub fn role_global_mounts_title(role_label: &str) -> String {
    format!(" Role global mounts · {role_label} ")
}

#[must_use]
pub const fn global_mounts_title() -> &'static str {
    " Global mounts "
}

#[must_use]
pub const fn instance_sessions_empty_message(session_load_error: bool) -> &'static str {
    if session_load_error {
        "Sessions unavailable (manifest read error)"
    } else {
        "No sessions recorded"
    }
}
