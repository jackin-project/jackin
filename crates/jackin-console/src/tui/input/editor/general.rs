// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! General tab helpers for the editor: field modal opener.

use crate::tui::screens::editor::update::{
    EditorGeneralFieldModalPlan, editor_general_field_modal_plan,
};
use crate::tui::screens::editor::view::{
    editor_name_input_state, editor_name_value, editor_workdir_pick_state,
};
use crate::tui::state::{EditorState, FieldFocus, Modal, TextInputTarget};

pub(super) fn open_editor_field_modal(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    match editor_general_field_modal_plan(editor.active_tab, n, !editor.pending.mounts.is_empty()) {
        EditorGeneralFieldModalPlan::RenameWorkspace => {
            let current = editor_name_value(&editor.mode, editor.pending_name.as_deref(), "");
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::Name,
                state: editor_name_input_state(current),
            });
        }
        EditorGeneralFieldModalPlan::PickWorkdir => {
            editor.modal = Some(Modal::WorkdirPick {
                state: editor_workdir_pick_state(&editor.pending.mounts),
            });
        }
        EditorGeneralFieldModalPlan::None => {}
    }
}
