// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::too_many_lines,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
//! Key handler for the Settings → Global Mounts tab and its modals.
//!
//! Dispatches keyboard events to the add/edit/delete flow for global mount
//! entries and for the auth/env panels that share the Settings screen.
//! Produces `ManagerEffect` values the event loop applies; does not write
//! config directly.
//!
//! Not responsible for: rendering (`jackin-console` settings view) or the
//! save commit path (`console/tui/input/save.rs`).
mod auth;
mod builders;
mod commits;
mod confirm_modal;
mod env_keys;
mod env_modal;
mod env_modals;
mod mount_add;
mod mounts_keys;
mod panels;
mod shell;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use crate::tui::state::SettingsEnvScope;
pub(crate) use auth::apply_source_folder_to_settings_auth_form;
use auth::handle_auth_key;
pub use auth::handle_settings_auth_modal;
pub use auth::{
    apply_op_picker_to_settings_auth_form_committed, apply_plain_text_to_settings_auth_form,
};
pub use builders::after_settings_event;
pub(crate) use builders::{
    commit_add_scope_choice, confirm_modal, env_text_modal, scope_picker_modal, text_modal,
    text_modal_for_target,
};
pub(crate) use commits::{
    commit_env_text, commit_settings_confirm, commit_settings_env_scope_picker,
    commit_settings_env_source_picker, commit_text, open_settings_save_preview,
    request_settings_save,
};
pub use confirm_modal::handle_settings_confirm_modal;
#[cfg(test)]
pub(crate) use crossterm::event::KeyCode;
#[cfg(test)]
pub(crate) use crossterm::event::KeyEvent;
pub(crate) use env_keys::handle_env_key;
pub use env_modal::handle_settings_env_modal;
pub(crate) use env_modals::{
    delete_selected_settings_env, open_settings_env_add_modal, open_settings_env_delete_confirm,
    open_settings_env_enter_modal, open_settings_env_picker_modal, set_settings_env_value_typed,
    toggle_settings_env_mask,
};
pub(crate) use mount_add::{
    apply_global_mount_add_text, finalize_global_mount_add, open_edit_text,
    open_global_mount_scope_picker,
};
pub(crate) use mounts_keys::handle_global_mounts_key;
pub(crate) use panels::{handle_general_key, handle_trust_key};
pub(crate) use shell::dispatch_manager;
#[cfg(test)]
pub use shell::handle_settings_key;
pub use shell::{SettingsAuthOutcome, SettingsModalOutcome, handle_settings_key_with_effects};
