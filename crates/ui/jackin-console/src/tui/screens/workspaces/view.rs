// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspaces screen view helpers.
//!
//! Composition copy-adapted from the upstream `patterns/project_launcher.rs`
//! and `patterns/session_picker.rs` recipes (composition reference, never a
//! type dependency — no `termrock::patterns` import):
//!
//! - workspace/instance sidebar list = the recipe's master list pane
//!   (`ProjectLauncherPane::Projects`),
//! - detail/preview pane = the recipe's preview region
//!   (`ProjectLauncherPane::Preview`),
//! - inline pickers (role/agent/provider) = the recipe's popover selectors
//!   (`QuickOpen`),
//! - footer action hints = the recipe's action/status strip (chrome-only).
//!
//! The focus cycle follows the recipe's `focus_order()` pattern: one ordered
//! owner chain (`workspace_list_focus_order` in `update.rs`) that enter/exit
//! transitions walk. Domain types, wording, and effects stay jackin❯-owned
//! per the copy-adapt law.
mod display_rows;
pub mod footer;
mod instance_lines;
mod instance_pane;
pub mod list;
mod list_names;
mod pickers;
mod prelude;
mod roles;
mod row_labels;
mod subpanels;
#[cfg(test)]
mod tests;
mod tree;
pub(crate) use display_rows::panel;
pub use display_rows::{
    Disclosure, WorkspaceListDisplayRow, WorkspaceListDisplayRowFacts,
    WorkspaceListDisplayRowsFacts, WorkspaceListNamesRenderFacts, WorkspaceListNamesRenderPlan,
    WorkspaceListRowTone, WorkspacePreviewPanePlan, WorkspaceSidebarFacts, WorkspaceSidebarPlan,
    workspace_preview_pane_plan, workspace_sidebar_owns_focus, workspace_sidebar_plan,
};
pub(crate) use instance_lines::{env_row_line, instance_detail_lines};
pub use instance_pane::{
    WorkspaceInstanceLivePaneFacts, WorkspaceInstanceLiveTabFacts, WorkspaceInstancePane,
    WorkspaceInstancePaneContent, WorkspaceInstanceSessionRow, WorkspaceInstanceTab,
    WorkspaceInstanceTabPane, render_instance_details_pane, workspace_instance_live_content,
    workspace_instance_pane, workspace_instance_session_content,
};
pub(crate) use list_names::row_fg;
pub use list_names::{list_name_lines, render_list_names_block, workspace_list_names_render_plan};
pub use pickers::{
    account_picker_title, render_account_picker_sidebar, render_agent_picker_sidebar,
    render_picker_sidebar, render_role_picker_sidebar,
};
pub use prelude::{
    create_prelude_mount_destination_default, create_prelude_mount_destination_input_state,
    create_prelude_mount_dst_choice_state, create_prelude_workdir_pick_state,
    create_prelude_workspace_name_default, create_prelude_workspace_name_input_state,
    render_compact_instances_summary, render_sentinel_description_pane,
};
pub use roles::{render_config_roles_subpanel, render_roles_subpanel};
pub use row_labels::{
    InstanceRowLabel, current_directory_display_row, current_directory_workspace_title,
    global_mounts_title, instance_purge_confirm_label, instance_sessions_empty_message,
    new_workspace_display_row, new_workspace_list_label, picker_sidebar_title,
    role_global_mounts_title, workspace_instance_display_row, workspace_instance_list_label,
    workspace_instance_pane_identity_label, workspace_list_display_row_for_row,
    workspace_list_display_rows,
};
pub use subpanels::{
    WorkspaceEnvRow, WorkspaceRoleRow, render_config_mounts_subpanel, render_environments_subpanel,
    render_general_subpanel, render_global_mount_rows_section, render_global_mounts_subpanel,
    render_mounts_subpanel, workspace_env_rows,
};

#[cfg(test)]
pub(crate) use crate::tui::components::editor_rows::action_row_style;
#[cfg(test)]
pub(crate) use instance_lines::live_instance_lines;
pub(crate) use tree::{push_tree_instance_line, push_tree_workspace_line};
