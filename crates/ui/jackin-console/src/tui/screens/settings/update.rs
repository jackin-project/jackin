// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings screen update logic: handle keyboard events and produce effects
//! for the General, Mounts, Environments, Auth, and Trust tab group.
//!
//! Not responsible for: rendering (see `view`) or state definitions (see
//! `model`).
use std::collections::{BTreeMap, BTreeSet};

use super::model::{SettingsEnvConfig, SettingsEnvEnterPlan, SettingsEnvRow, SettingsEnvScope};
use jackin_core::EnvValue;

#[cfg(test)]
use super::effect::SettingsEffect;
#[cfg(test)]
use super::message::SettingsMessage;
#[cfg(test)]
use super::model::{
    ACCOUNT_KINDS, GlobalMountConfirm, GlobalMountDraft, GlobalMountTextTarget, SettingsAuthState,
    SettingsEnvTextTarget, SettingsHoverTarget, SettingsTab, SettingsTrustRow,
};
#[cfg(test)]
use crate::tui::components::scope_picker::ScopeChoice;
#[cfg(test)]
use crossterm::event::KeyCode;
#[cfg(test)]
use jackin_core::RoleSelector;
#[cfg(test)]
use jackin_oppicker::ModalOutcome;

mod auth;
mod confirm;
mod env_helpers;
mod env_plans;
mod focus;
mod keys;
mod mount_commits;
mod mount_plans;
mod selection;
mod tabs;
pub use env_helpers::*;
#[cfg(test)]
mod tests;
pub use auth::{
    SettingsAuthRowKind, reduce_account_scan_message, scanned_source_in_draft,
    settings_auth_row_kind, settings_auth_scan_row_selected, settings_auth_selected_index,
};
pub use confirm::{
    SettingsConfirmCommitPlan, SettingsConfirmPlan, settings_confirm_commit_plan,
    settings_confirm_plan,
};
pub use env_plans::{
    SettingsEnvRolePickerCommitPlan, SettingsEnvScopePickerCommitPlan,
    SettingsEnvScopePickerSelection, SettingsEnvSourcePickerCommitPlan,
    SettingsEnvSourcePickerSelection, SettingsEnvTextCommitPlan, role_picker_open_plan,
    settings_env_role_picker_commit_plan, settings_env_role_picker_open_plan,
    settings_env_role_picker_roles, settings_env_scope_picker_commit_plan,
    settings_env_source_picker_commit_plan, settings_env_text_commit_plan,
};
pub(crate) use focus::settings_focus_region_eq;
pub use focus::{
    SettingsFocusRegion, settings_focus_head, settings_focus_next, settings_focus_order,
    settings_focus_region, settings_tab_hover_plan, settings_tab_hover_target_plan,
};
pub use keys::{
    settings_auth_key_plan, settings_env_delete_key_for_row, settings_env_header_key_plan,
    settings_env_key_plan, settings_env_selected_delete_key, settings_env_selected_header_key_plan,
    settings_env_selected_is_op_ref, settings_env_selected_key_is_op_ref,
    settings_env_selected_key_matches, settings_general_key_plan, settings_shell_key_plan,
    settings_tab_at_position, settings_top_level_key_plan, settings_trust_key_plan,
};
pub use mount_commits::{
    global_mount_add_finalize_apply_plan, global_mount_add_finalize_plan,
    global_mount_add_text_apply_plan, global_mount_edit_text_apply_plan,
    global_mount_github_open_plan, global_mount_role_picker_commit_plan,
    global_mount_role_picker_open_plan, global_mount_role_picker_roles,
    global_mount_scope_picker_commit_plan, global_mount_text_commit_plan,
    set_global_mount_add_draft_destination, settings_global_mounts_key_plan,
};
pub use mount_plans::{
    GlobalMountAddFinalizeApplyPlan, GlobalMountAddFinalizePlan, GlobalMountAddTextApplyPlan,
    GlobalMountEditTextApplyPlan, GlobalMountGithubOpenPlan, GlobalMountRolePickerCommitPlan,
    GlobalMountScopePickerCommitPlan, GlobalMountTextCommitPlan, RolePickerOpenPlan,
    SettingsGlobalMountsKeyPlan,
};
pub use selection::{
    SettingsScrollFocusPlan, SettingsSelectionScrollPlan, SettingsTrustRowSelectPlan,
    settings_env_selection_plan, settings_global_mounts_add_row_selected,
    settings_global_mounts_added_index, settings_global_mounts_selected_index,
    settings_global_mounts_selection_plan, settings_horizontal_scroll_plan, settings_modal_open,
    settings_scroll_focus_plan, settings_trust_clickable_at_position,
    settings_trust_hover_target_at_position, settings_trust_row_at_position,
    settings_trust_row_select_plan, settings_trust_selection_plan, trust_content_width,
};
pub use tabs::{
    SettingsAuthKeyPlan, SettingsEnvHeaderKeyPlan, SettingsEnvKeyPlan, SettingsGeneralKeyPlan,
    SettingsShellKeyPlan, SettingsTabMovePlan, SettingsTopLevelKeyPlan, SettingsTrustKeyPlan,
    next_settings_tab, previous_settings_tab, settings_tab_bar_focus_plan, settings_tab_move_plan,
    settings_tab_select_plan,
};
