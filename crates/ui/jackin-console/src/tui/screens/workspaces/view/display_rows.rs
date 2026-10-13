// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace list display rows and plans.

use ratatui::layout::Rect;

use crate::tui::screens::workspaces::model::ManagerListRow;

pub(crate) fn panel<'a>(
    theme: &'a termrock::style::DesignSystem,
    title: Option<&'a str>,
    focused: bool,
) -> termrock::widgets::Panel<'a> {
    let panel = termrock::widgets::Panel::new(theme).emphasis(if focused {
        termrock::widgets::PanelChrome::Focused
    } else {
        termrock::widgets::PanelChrome::Normal
    });
    if let Some(title) = title {
        panel.title(title)
    } else {
        panel
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disclosure {
    None,
    Collapsed,
    Expanded,
}

impl Disclosure {
    #[must_use]
    pub const fn for_instances(has_instances: bool, expanded: bool) -> Self {
        if !has_instances {
            Self::None
        } else if expanded {
            Self::Expanded
        } else {
            Self::Collapsed
        }
    }

    #[must_use]
    pub const fn glyph(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Collapsed => Some("▶"),
            Self::Expanded => Some("▼"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListRowTone {
    White,
    Workspace,
    Instance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceListDisplayRow {
    pub label: String,
    pub tone: WorkspaceListRowTone,
    pub disclosure: Disclosure,
    pub selected: bool,
    pub hovered: bool,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal UI state flags (selected, hovered, \
              current_dir_expanded, current_dir_has_instances) — each tracks an \
              independent focus / disclosure signal consumed individually by the \
              row builder. Named-field reads match the direct focus-model idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListDisplayRowFacts {
    pub row: ManagerListRow,
    pub selected: bool,
    pub hovered: bool,
    pub current_dir_expanded: bool,
    pub current_dir_has_instances: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListDisplayRowsFacts<'a> {
    pub visual_rows: &'a [Option<ManagerListRow>],
    pub visual_selected: usize,
    pub hovered_row: Option<ManagerListRow>,
    pub current_dir_expanded: bool,
    pub current_dir_has_instances: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePreviewPanePlan {
    CurrentDirectory,
    NewWorkspace,
    SavedWorkspace(usize),
    Instance {
        workspace_idx: Option<usize>,
        instance_idx: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSidebarPlan {
    InlineAccountPicker,
    LaunchAccountPicker,
    InlineNewSessionPicker,
    InlineAgentPicker,
    InlineRolePicker,
    ListNames,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Five orthogonal sidebar-picker visibility flags (provider, \
              launch_account, new_session, agent, role) — each tracks an \
              independent inline-picker open state consumed individually by the \
              sidebar planner to pick its `WorkspaceSidebarPlan` variant. Named- \
              field reads match the per-picker detection idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceSidebarFacts {
    pub inline_account_picker_open: bool,
    pub launch_account_picker_open: bool,
    pub inline_new_session_picker_open: bool,
    pub inline_agent_picker_open: bool,
    pub inline_role_picker_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListNamesRenderFacts {
    pub area: Rect,
    pub selected_index: usize,
    pub row_count: usize,
    pub scroll_y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListNamesRenderPlan {
    pub viewport_width: usize,
    pub follow_scroll_y: u16,
}

#[must_use]
pub const fn workspace_preview_pane_plan(row: ManagerListRow) -> WorkspacePreviewPanePlan {
    match row {
        ManagerListRow::CurrentDirectory => WorkspacePreviewPanePlan::CurrentDirectory,
        ManagerListRow::NewWorkspace => WorkspacePreviewPanePlan::NewWorkspace,
        ManagerListRow::SavedWorkspace(idx) => WorkspacePreviewPanePlan::SavedWorkspace(idx),
        ManagerListRow::CurrentDirectoryInstance(instance_idx) => {
            WorkspacePreviewPanePlan::Instance {
                workspace_idx: None,
                instance_idx,
            }
        }
        ManagerListRow::WorkspaceInstance(workspace_idx, instance_idx) => {
            WorkspacePreviewPanePlan::Instance {
                workspace_idx: Some(workspace_idx),
                instance_idx,
            }
        }
    }
}

#[must_use]
pub const fn workspace_sidebar_plan(facts: WorkspaceSidebarFacts) -> WorkspaceSidebarPlan {
    if facts.inline_account_picker_open {
        return WorkspaceSidebarPlan::InlineAccountPicker;
    }
    if facts.launch_account_picker_open {
        return WorkspaceSidebarPlan::LaunchAccountPicker;
    }
    if facts.inline_new_session_picker_open {
        return WorkspaceSidebarPlan::InlineNewSessionPicker;
    }
    if facts.inline_agent_picker_open {
        return WorkspaceSidebarPlan::InlineAgentPicker;
    }
    if facts.inline_role_picker_open {
        return WorkspaceSidebarPlan::InlineRolePicker;
    }
    WorkspaceSidebarPlan::ListNames
}

#[must_use]
pub const fn workspace_sidebar_owns_focus(list_names_focused: bool, list_modal_open: bool) -> bool {
    list_names_focused && !list_modal_open
}
