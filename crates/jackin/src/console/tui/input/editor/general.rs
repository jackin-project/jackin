//! General tab helpers for the editor: field modal opener.

use crate::console::tui::state::{EditorState, EditorTab, FieldFocus, Modal, TextInputTarget};
use jackin_console::tui::screens::editor::view::{
    editor_name_input_state, editor_name_value, editor_workdir_pick_state,
};

pub(super) fn open_editor_field_modal(editor: &mut EditorState<'_>) {
    if editor.active_tab == EditorTab::General {
        let FieldFocus::Row(n) = editor.active_field;
        match n {
            0 => {
                let current = editor_name_value(&editor.mode, editor.pending_name.as_deref(), "");
                editor.modal = Some(Modal::TextInput {
                    target: TextInputTarget::Name,
                    state: editor_name_input_state(current),
                });
            }
            1 if !editor.pending.mounts.is_empty() => {
                editor.modal = Some(Modal::WorkdirPick {
                    state: editor_workdir_pick_state(&editor.pending.mounts),
                });
            }
            _ => {}
        }
    }
}
