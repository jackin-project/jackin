// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace-list footer facts and mode resolver.

use termrock::scroll::ScrollAxes;

use crate::tui::screens::workspaces::model::ManagerListRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListFooterMode {
    AgentPicker {
        scroll_axes: ScrollAxes,
    },
    RolePicker {
        scroll_axes: ScrollAxes,
    },
    PreviewPane,
    InstanceRow {
        has_snapshot: bool,
        is_live: bool,
    },
    WorkspaceRow {
        scroll_axes: ScrollAxes,
        enter_label: &'static str,
        is_saved: bool,
        show_prewarm: bool,
        show_expand: bool,
        show_collapse: bool,
        show_open_in_github: bool,
    },
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Twelve orthogonal footer-state flags (inline-agent/role-picker, \
              selected row / preview focus, snapshot+live markers, saved vs new \
              workspace, show prewarm/expand/collapse/github) — each tracks an \
              independent UI hint visibility consumed individually by the footer \
              item builder. Named-field reads match the per-hint gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterFacts {
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub selected_instance: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
    pub selected_instance_is_live: bool,
    pub selected_saved_workspace: bool,
    pub selected_new_workspace: bool,
    pub show_prewarm: bool,
    pub show_expand: bool,
    pub show_collapse: bool,
    pub workspace_scroll_axes: ScrollAxes,
    pub show_open_in_github: bool,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Eight orthogonal footer-input flags (selected row, inline-agent/role \
              pickers, preview focus, snapshot+live markers, show_expand/collapse, \
              scroll axes, open-in-github) — each is an independent input the \
              workspace-list-footer mode resolver reads individually. Named-field \
              reads match the per-input gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterInputFacts {
    pub selected_row: ManagerListRow,
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
    pub selected_instance_is_live: bool,
    pub show_expand: bool,
    pub show_collapse: bool,
    pub workspace_scroll_axes: ScrollAxes,
    pub show_open_in_github: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterRowFacts {
    pub selected_instance: bool,
    pub selected_saved_workspace: bool,
    pub selected_new_workspace: bool,
}

#[must_use]
pub const fn workspace_list_footer_row_facts(row: ManagerListRow) -> WorkspaceListFooterRowFacts {
    match row {
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceListFooterRowFacts {
                selected_instance: true,
                selected_saved_workspace: false,
                selected_new_workspace: false,
            }
        }
        ManagerListRow::SavedWorkspace(_) => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: true,
            selected_new_workspace: false,
        },
        ManagerListRow::NewWorkspace => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: false,
            selected_new_workspace: true,
        },
        ManagerListRow::CurrentDirectory => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: false,
            selected_new_workspace: false,
        },
    }
}

#[must_use]
pub const fn workspace_list_open_github_visible(
    row: ManagerListRow,
    selected_workspace_has_github_mounts: bool,
) -> bool {
    matches!(row, ManagerListRow::SavedWorkspace(_)) && selected_workspace_has_github_mounts
}

#[must_use]
pub const fn workspace_list_footer_facts(
    facts: WorkspaceListFooterInputFacts,
) -> WorkspaceListFooterFacts {
    let row_facts = workspace_list_footer_row_facts(facts.selected_row);
    WorkspaceListFooterFacts {
        inline_agent_picker: facts.inline_agent_picker,
        inline_role_picker: facts.inline_role_picker,
        selected_instance: row_facts.selected_instance,
        preview_focused: facts.preview_focused,
        selected_instance_has_snapshot: facts.selected_instance_has_snapshot,
        selected_instance_is_live: facts.selected_instance_is_live,
        selected_saved_workspace: row_facts.selected_saved_workspace,
        selected_new_workspace: row_facts.selected_new_workspace,
        // Surface the `W` prewarm hint exactly when a saved workspace is
        // selected — the only row for which `W` dispatches PrewarmNamed.
        show_prewarm: row_facts.selected_saved_workspace,
        show_expand: facts.show_expand,
        show_collapse: facts.show_collapse,
        workspace_scroll_axes: facts.workspace_scroll_axes,
        show_open_in_github: facts.show_open_in_github,
    }
}
