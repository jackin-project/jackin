// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor-stage dispatch: tab navigation, field focus, per-tab key
//! handling, and the editor-level modal dispatcher.
pub(super) mod agents;
pub(super) mod general;
pub(super) mod modal;
pub(super) mod secrets;
#[cfg(test)]
mod tests;
pub use modal::{
    apply_text_input_to_pending, env_key_input_state, open_secrets_picker_modal,
    set_pending_env_op_ref,
};
mod apply;
mod keys;
mod modal_dispatch;
mod nav;
mod top;
pub use apply::{EditorModalOutcome, apply_file_browser_to_editor};
pub(crate) use apply::{apply_editor_confirm, apply_role_input, dispatch_editor_mount_dst_choice};
pub use keys::handle_editor_key;
pub use modal_dispatch::handle_editor_modal;

pub(crate) use nav::{
    dispatch_editor_field_selection, dispatch_editor_horizontal_scroll,
    dispatch_editor_immediate_action, dispatch_editor_navigation,
    dispatch_editor_role_header_expansion, dispatch_manager,
};
pub(crate) use top::dispatch_editor_top_level;
