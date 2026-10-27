// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod apply;
mod form;
mod keys;
mod modal;
mod persist;
pub use apply::{
    apply_op_picker_to_settings_auth_form_committed, apply_plain_text_to_settings_auth_form,
};
pub(crate) use apply::{
    apply_source_folder_to_settings_auth_form, commit_settings_auth_text,
    restore_settings_auth_form,
};
pub(crate) use form::{SourceFolderValidator, open_settings_auth_form};
pub(super) use keys::handle_auth_key;
pub(crate) use keys::record_missing_auth_return_path;
pub use modal::handle_settings_auth_modal;
pub(crate) use persist::{clear_settings_auth_kind, persist_settings_auth_form};
