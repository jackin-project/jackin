// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace-list footer facts, mode resolver, and the matching hint-span
//! builders for the workspace-list screen.
mod facts;
mod list;
mod screen;
pub use facts::{
    WorkspaceListFooterFacts, WorkspaceListFooterInputFacts, WorkspaceListFooterMode,
    WorkspaceListFooterRowFacts, workspace_list_footer_facts, workspace_list_footer_row_facts,
    workspace_list_open_github_visible,
};
pub use list::{
    editor_save_footer_label, pick_list_confirm_footer_label, pick_list_select_footer_label,
    selected_instance_snapshot_available, settings_save_footer_label, workspace_footer_scroll_axes,
    workspace_inline_picker_content_height, workspace_list_footer_items,
    workspace_list_footer_mode_for_facts, workspace_picker_footer_items,
};
pub use screen::{
    WorkspaceFooterScrollFacts, WorkspaceInlinePickerContentFacts, WorkspaceScreenFooterFacts,
    WorkspaceScreenFooterPlan, create_prelude_footer_items, destructive_confirm_footer_items,
    workspace_screen_footer_items, workspace_screen_footer_plan,
};
