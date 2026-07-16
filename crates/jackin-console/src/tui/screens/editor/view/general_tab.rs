// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! General tab content lines, widths, and geometry extracted from the view
//! coordinator. All items re-exported from parent to preserve `super::` call
//! sites in `frame.rs` (via `render_general_tab`) and `view/tests.rs`.

use ratatui::text::Line;

use crate::tui::screens::editor::model::FieldFocus;
use crate::tui::screens::form_model::{FieldRow, FormSection};

use super::WorkspaceEditorState;
use super::render_editor_row;
use super::{editor_name_value, padded_width};

pub(crate) fn editor_row_width(label: &str, value: &str) -> usize {
    padded_width(&format!("  {label:15}{value}"))
}

#[must_use]
pub(crate) fn editor_general_content_width(
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> usize {
    general_row_widths(
        name_value,
        workdir_display,
        keep_awake_enabled,
        git_pull_on_entry,
    )
    .into_iter()
    .max()
    .unwrap_or(0)
}

/// Build the shared form section for the editor general tab.
#[must_use]
pub(crate) fn general_form_section(
    cursor: usize,
    show_cursor: bool,
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> FormSection {
    let keep_awake_display = if keep_awake_enabled {
        "enabled (macOS only)"
    } else {
        "disabled"
    };
    let git_pull_display = if git_pull_on_entry {
        "enabled"
    } else {
        "disabled"
    };
    FormSection::new(
        vec![
            FieldRow::new("Name", name_value),
            FieldRow::new("Working dir", workdir_display),
            FieldRow::new("Keep awake", keep_awake_display),
            FieldRow::new("Git pull", git_pull_display),
        ],
        cursor,
        show_cursor,
        15,
    )
}

#[must_use]
pub(crate) fn general_state_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) -> super::EditorTabContentGeometry {
    let name_value = editor_name_value(&state.mode, state.pending_name.as_deref(), "(new)");
    let workdir_display = jackin_core::shorten_home(&state.pending.workdir);
    super::EditorTabContentGeometry {
        content_width: editor_general_content_width(
            &name_value,
            &workdir_display,
            state.pending.keep_awake.enabled,
            state.pending.git_pull_on_entry,
        ),
        content_height: 4,
    }
}

#[must_use]
pub(crate) fn general_lines(
    cursor: usize,
    show_cursor: bool,
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> Vec<Line<'static>> {
    let section = general_form_section(
        cursor,
        show_cursor,
        name_value,
        workdir_display,
        keep_awake_enabled,
        git_pull_on_entry,
    );
    // Keep render_editor_row path for byte-identical editor snapshots; FormSection
    // carries the shared row model for geometry/settings reuse.
    section
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            render_editor_row(
                i,
                section.cursor,
                &row.label,
                &row.value,
                section.show_cursor,
            )
        })
        .collect()
}

#[must_use]
pub(crate) fn general_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let name_value = editor_name_value(&state.mode, state.pending_name.as_deref(), "(new)");
    let workdir_display = jackin_core::shorten_home(&state.pending.workdir);

    general_lines(
        cursor,
        show_cursor,
        &name_value,
        &workdir_display,
        state.pending.keep_awake.enabled,
        state.pending.git_pull_on_entry,
    )
}

pub(crate) fn general_row_widths(
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> [usize; 4] {
    let keep_awake_display = if keep_awake_enabled {
        "enabled (macOS only)"
    } else {
        "disabled"
    };
    let git_pull_display = if git_pull_on_entry {
        "enabled"
    } else {
        "disabled"
    };
    [
        editor_row_width("Name", name_value),
        editor_row_width("Working dir", workdir_display),
        editor_row_width("Keep awake", keep_awake_display),
        editor_row_width("Git pull", git_pull_display),
    ]
}
