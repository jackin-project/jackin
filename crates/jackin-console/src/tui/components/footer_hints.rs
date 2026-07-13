// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared footer hint fragments for modal pickers and confirmations.
//!
//! Coordinator — declares the sibling modules and re-exports every public
//! type and builder so external callers keep their existing `use` paths.
//! Sibling layout follows the worklist split: workspace / editor /
//! settings / modals / common.

mod common;
mod editor;
mod modals;
mod settings;
mod workspace;

// Workspace-list footer types + builders.
pub use self::workspace::{
    WorkspaceFooterScrollFacts, WorkspaceInlinePickerContentFacts, WorkspaceListFooterFacts,
    WorkspaceListFooterInputFacts, WorkspaceListFooterMode, WorkspaceListFooterRowFacts,
    WorkspaceScreenFooterFacts, WorkspaceScreenFooterPlan, create_prelude_footer_items,
    destructive_confirm_footer_items, editor_save_footer_label, pick_list_confirm_footer_label,
    pick_list_select_footer_label, selected_instance_snapshot_available,
    settings_save_footer_label, workspace_footer_scroll_axes,
    workspace_inline_picker_content_height, workspace_list_footer_facts,
    workspace_list_footer_items, workspace_list_footer_mode_for_facts,
    workspace_list_footer_row_facts, workspace_list_open_github_visible,
    workspace_picker_footer_items, workspace_screen_footer_items, workspace_screen_footer_plan,
};

// Editor-screen footer types + builders.
pub use self::editor::{
    AuthRowFooterMode, EditorContextFooterMode, EditorScreenFooterFacts, auth_row_footer_items,
    editor_contextual_row_footer_items, editor_footer_items, editor_general_row_footer_items,
    editor_role_row_footer_items, editor_screen_footer_items,
};

// Settings-screen footer types + builders.
pub use self::settings::{
    SettingsContextFooterMode, add_row_footer_items, append_generate_token_footer_item,
    filtered_picker_footer_items, global_mount_row_footer_items, mount_destination_footer_items,
    op_section_footer_items, pick_list_footer_items, secret_add_row_footer_items,
    secret_op_ref_row_footer_items, secret_plain_row_footer_items, secret_role_header_footer_items,
    segmented_choice_footer_items, settings_contextual_row_footer_items,
    settings_general_row_footer_items, settings_trust_row_footer_items,
    workspace_mount_row_footer_items,
};

// `SettingsScreenFooterFacts` lives next to the editor's
// `EditorScreenFooterFacts` because both screen orchestrators consume it.
pub use self::editor::{SettingsScreenFooterFacts, settings_screen_footer_items};

// Modal footer types + dispatch.
pub use self::modals::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState, auth_form_footer_items,
    confirm_save_footer_items, container_info_footer_items, container_info_footer_items_for_dialog,
    error_popup_footer_items, modal_footer_items, op_picker_modal_footer_mode,
    save_discard_cancel_footer_items, status_popup_footer_items,
};

// Common fragments used by tab-bar / content / save-and-escape footers.
pub use self::common::{content_footer_items, tab_bar_footer_items};
