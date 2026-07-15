// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor screen view helpers.

use super::model::{EditorState, EditorTab};
pub use super::model::{SecretsRow, SecretsScopeTag};

pub use crate::tui::mount_display::MountDisplayRow;
pub use crate::tui::state::EditorMode;

pub use crate::tui::components::editor_rows::{
    AuthSourceDisplay, AuthSourceFolderDisplay, AuthSourceFolderKind, SecretValueDisplay,
};

#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "re-export for editor view tests")
)]
pub(crate) use crate::tui::components::editor_rows::auth_lines;

use ratatui::{layout::Rect, text::Line};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorScrollGeometry {
    pub active_mounts: bool,
    pub content_width: usize,
    pub content_height: usize,
    pub mounts_content_width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTabContentGeometry {
    pub content_width: usize,
    pub content_height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorFrameAreas {
    pub header: Rect,
    pub tabs: Rect,
    pub body: Rect,
    pub footer: Rect,
}

pub type WorkspaceEditorState<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> = EditorState<
    crate::mount_info_cache::MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>;

mod general_tab;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use general_tab::{
    editor_general_content_width, editor_row_width, general_lines, general_row_widths,
};

mod mounts_tab;
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "re-export for editor view tests")
)]
pub(crate) use mounts_tab::{editor_mount_add_row_width, mount_lines};

mod roles_tab;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use roles_tab::{
    EditorRoleRow, editor_role_load_row_width, editor_role_row_width, editor_roles_status_width,
    role_lines, role_state_geometry, role_state_lines,
};

mod secrets_tab;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use secrets_tab::{
    editor_secret_line_width, secret_key_line_width, secret_lines, secret_state_geometry,
    secret_state_lines,
};

mod auth_tab;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use auth_tab::{
    EditorAuthLineRow, auth_state_geometry, auth_state_lines, editor_auth_line_width,
};

mod modals;
pub use modals::secret_new_key_label;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use modals::{
    editor_header_title, editor_name_value, isolated_state_save_confirm_state,
    role_trust_confirm_state, secret_delete_confirm_prompt, secret_delete_confirm_state,
    secret_empty_key_label, secret_key_input_state, secret_key_input_state_from_pending,
    secret_new_key_after_picker_label, secret_new_value_input_state, secret_scope_picker_state,
    secret_source_picker_state, secret_value_current_text, secret_value_input_state,
    secrets_forbidden_label, secrets_scope_label,
};

mod frame;
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "re-export for editor view tests")
)]
pub(crate) use frame::{
    editor_body_area, editor_contextual_footer_items, editor_frame_areas,
    prepare_editor_for_render, prepare_editor_tab_for_area, render_editor_with_footer,
    render_general_tab, render_roles_tab, render_secrets_tab,
};

#[must_use]
pub fn editor_name_input_state<'a>(
    current: impl Into<String>,
) -> termrock::components::TextInputState<'a> {
    termrock::components::TextInputState::new("Rename workspace", current)
}

#[must_use]
pub fn editor_workdir_pick_state<M: crate::tui::components::workdir_pick::WorkdirMount>(
    mounts: &[M],
) -> crate::tui::components::workdir_pick::WorkdirPickState {
    crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(mounts)
}

#[must_use]
pub fn role_load_input_state<'a>(
    trusted_roles: Vec<String>,
) -> termrock::components::TextInputState<'a> {
    let mut state =
        termrock::components::TextInputState::new_with_forbidden("Load role", "", trusted_roles);
    state.forbidden_label = "trusted role registry".into();
    state
}

#[must_use]
pub fn mount_destination_input_state<'a>(
    current: impl Into<String>,
) -> termrock::components::TextInputState<'a> {
    termrock::components::TextInputState::new("Destination", current)
}

#[must_use]
pub fn mount_dst_choice_state(
    src: impl Into<String>,
) -> crate::tui::components::mount_dst_choice::MountDstChoiceState {
    crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src)
}

#[must_use]
pub fn running_isolated_state_save_block_message(affected_containers: &[String]) -> String {
    format!(
        "Cannot save: {} container(s) are running with isolated state for an affected mount: {}; eject them first.",
        affected_containers.len(),
        affected_containers.join(", "),
    )
}

#[must_use]
pub(crate) fn render_editor_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
    show_cursor: bool,
) -> Line<'static> {
    let selected = show_cursor && (row == cursor);
    crate::tui::components::editor_rows::labeled_field_line(
        selected,
        "",
        label,
        15,
        value,
        crate::tui::components::editor_rows::FieldEmphasis::SelectedValue,
    )
}

pub fn padded_width(text: &str) -> usize {
    padded_width_cols(
        text_width(text),
        text.chars().take_while(|c| *c == ' ').count(),
    )
}

pub fn padded_width_cols(width: usize, leading_spaces: usize) -> usize {
    width + leading_spaces
}

pub fn text_width(text: &str) -> usize {
    termrock::display_cols(text)
}

#[must_use]
pub fn tab_labels(active: EditorTab) -> Vec<(&'static str, bool)> {
    EditorTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[cfg(test)]
mod tests;
