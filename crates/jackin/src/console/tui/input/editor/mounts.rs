//! Mounts tab helpers for the editor: add, remove, readonly toggle.

use crate::console::tui::state::{EditorState, FieldFocus};

pub(super) fn remove_mount_at_cursor(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    if n < editor.pending.mounts.len() {
        editor.pending.mounts.remove(n);
    }
}
