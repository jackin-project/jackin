// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings Auth tab key and modal handlers.
mod apply;
mod form;
mod keys;
mod modal;
mod persist;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use super::SettingsAuthOutcome;
#[cfg(test)]
pub(crate) use crate::tui::state::AuthForm;
#[cfg(test)]
pub(crate) use crate::tui::state::AuthFormFocus;
#[cfg(test)]
pub(crate) use crate::tui::state::SettingsModal;
pub use apply::{
    apply_op_picker_to_settings_auth_form_committed, apply_plain_text_to_settings_auth_form,
};
pub(crate) use apply::{
    apply_source_folder_to_settings_auth_form, commit_settings_auth_text,
    restore_settings_auth_form,
};
#[cfg(test)]
pub(crate) use crossterm::event::KeyCode;
#[cfg(test)]
pub(crate) use crossterm::event::KeyEvent;
pub(crate) use form::{SourceFolderValidator, open_settings_auth_form};
pub(super) use keys::handle_auth_key;
pub(crate) use keys::record_missing_auth_return_path;
pub use modal::handle_settings_auth_modal;
pub(crate) use persist::{clear_settings_auth_kind, persist_settings_auth_form};
