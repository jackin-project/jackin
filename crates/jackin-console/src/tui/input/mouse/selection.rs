//! Selection-row helpers: click-to-select in tab bars, mount rows,
//! auth rows, and settings trust rows.

use super::{
    ManagerMessage, ManagerStage, ManagerState, MouseEvent, Rect, SettingsTab, dispatch_manager,
    editor_auth_row_index_at_position, editor_mount_index_at_position, editor_scroll_area,
    editor_tab_at_position, editor_tab_hover_target_plan, settings_tab_at_position,
    settings_tab_hover_target_plan, settings_trust_row_at_position,
};

pub fn try_select_editor_tab(state: &mut ManagerState<'_>, mouse: MouseEvent) -> bool {
    let ManagerStage::Editor(editor) = &state.stage else {
        return false;
    };
    if editor.modal.is_some() {
        return false;
    }

    let Some(tab) = editor_tab_at_position(mouse.row, mouse.column) else {
        return false;
    };

    dispatch_manager(state, ManagerMessage::SelectEditorTab(tab));
    true
}

/// Repaint the hovered tab index on mouse motion so the strip lifts under the
/// pointer like the in-container multiplexer tabs. A motion off the strip
/// clears the highlight.
pub fn update_tab_hover(state: &mut ManagerState<'_>, mouse: MouseEvent) {
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.set_hover_target(editor_tab_hover_target_plan(
                editor.modal.is_some(),
                mouse.row,
                mouse.column,
            ));
        }
        ManagerStage::Settings(settings) => {
            settings.set_hover_target(settings_tab_hover_target_plan(
                settings.mounts.modal.is_some(),
                settings.env.modal.is_some(),
                mouse.row,
                mouse.column,
            ));
        }
        _ => {}
    }
}

pub fn try_select_settings_tab(state: &mut ManagerState<'_>, mouse: MouseEvent) -> bool {
    let ManagerStage::Settings(settings) = &state.stage else {
        return false;
    };
    if settings.mounts.modal.is_some() || settings.env.modal.is_some() {
        return false;
    }

    let Some(tab) = settings_tab_at_position(mouse.row, mouse.column) else {
        return false;
    };
    dispatch_manager(state, ManagerMessage::SelectSettingsTab(tab));
    true
}

/// Click inside the Trust block selects the row and activates the block for scrolling.
pub fn try_select_settings_trust_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Settings(settings) = &state.stage else {
        return false;
    };
    if settings.active_tab != SettingsTab::Trust || settings.mounts.modal.is_some() {
        return false;
    }
    let area = settings.content_area(term_size);
    if let Some(row) = settings_trust_row_at_position(
        area,
        mouse.column,
        mouse.row,
        settings.trust.scroll_y,
        settings.trust.pending.len(),
    ) {
        dispatch_manager(state, ManagerMessage::SelectSettingsTrustRow(row));
    } else {
        dispatch_manager(state, ManagerMessage::SelectSettingsTrustRow(usize::MAX));
    }
    true
}

/// Mount-row index the pointer is over on the editor Mounts tab, or `None`.
/// Pure geometry shared by the click handler and the hover hand-pointer cue so
/// they can't drift.
pub fn editor_mount_index_at(
    editor: &crate::tui::state::EditorState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    editor_mount_index_at_position(
        editor.active_tab,
        editor.modal.is_some(),
        editor_scroll_area(editor, term_size).area,
        mouse.column,
        mouse.row,
        editor.tab_scroll_y,
        editor.pending.mounts.as_slice(),
    )
}

pub fn try_select_editor_mount_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Editor(editor) = &state.stage else {
        return false;
    };
    let Some(index) = editor_mount_index_at(editor, mouse, term_size) else {
        return false;
    };
    dispatch_manager(state, ManagerMessage::SelectEditorMountRow(index));
    true
}

pub fn try_select_editor_auth_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return false;
    };
    let Some(index) = editor_auth_row_index_at(editor, config, mouse, term_size) else {
        return false;
    };
    editor.select_auth_row(index);
    true
}

pub fn editor_auth_row_index_at(
    editor: &crate::tui::state::EditorState<'_>,
    config: &jackin_config::AppConfig,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    let rows = editor.auth_flat_rows(config);
    editor_auth_row_index_at_position(
        editor.active_tab,
        editor.modal.is_some(),
        editor.content_area(term_size),
        mouse.column,
        mouse.row,
        editor.tab_scroll_y,
        &rows,
    )
}
