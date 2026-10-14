// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

mod accounts;
mod core;
mod inventory;
mod modals;
mod prelude_modals;
mod stages;
#[cfg(test)]
mod tests;
pub(crate) use accounts::{
    account_picker, settings_account_api_form, settings_account_profile_form,
    settings_accounts_populated, workspace_accounts_assigned,
};
pub(super) use core::{BaselineCase, render_manager_buffer};
pub(crate) use core::{plain, populated_config, render_case, test_cwd};
pub(crate) use inventory::{EXPECTED_INVENTORY, baseline_path, check_case};
pub(super) use inventory::{baselines_dir, inventory, stage_views};
pub(crate) use modals::{
    modal_auth_form, modal_auth_source_picker, modal_confirm, modal_confirm_save,
    modal_container_info, modal_error_popup, modal_file_browser, modal_github_picker,
    modal_mount_dst_choice, modal_op_picker, modal_role_override_picker, modal_role_picker,
    modal_save_discard_cancel, modal_scope_picker, modal_source_picker, modal_status_popup,
    modal_text_input, modal_workdir_pick,
};
pub(crate) use prelude_modals::{
    create_prelude_file_browser, create_prelude_mount_dst_choice, create_prelude_name_input,
    create_prelude_workdir_pick,
};
pub(crate) use stages::{
    confirm_delete, confirm_instance_purge, create_prelude, editor_auth, editor_general,
    editor_mounts, editor_roles, editor_secrets, keyboard_help, populated_then, settings_auth,
    settings_environments, settings_general, settings_mounts, settings_trust,
    workspaces_list_empty, workspaces_list_populated,
};
#[cfg(test)]
pub(crate) use std::fs;
#[cfg(test)]
pub(crate) use std::path::PathBuf;
