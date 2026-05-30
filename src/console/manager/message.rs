//! Workspace-manager message/update boundary.
//!
//! This starts the Model/Update/View migration with state-only list messages.
//! Input handlers should increasingly translate terminal events into these
//! messages instead of mutating `ManagerState` inline.

use super::state::{
    EditorTab, FieldFocus, ManagerListRow, ManagerStage, ManagerState, SettingsTab,
};
use super::render::global_mounts::{settings_env_flat_rows, SettingsEnvRow};
use jackin_tui::runtime::Dirty;
use ratatui::layout::Rect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerMessage {
    CollapseSelectedTree,
    EnterPreview,
    FocusEditorContent,
    FocusEditorTabBar,
    FocusSettingsContent,
    FocusSettingsTabBar,
    ExitPreview,
    ExpandSelectedTree,
    ClearSettingsAuthKind,
    EnterSettingsAuthKind,
    ScrollEditorTabHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollEditorWorkspaceMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsGlobalMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsTrustHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    MoveSettingsGlobalMountsSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsEnvSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsTrustSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveEditorTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveSettingsTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveSettingsGeneralSelection {
        delta: isize,
    },
    MoveSettingsAuthSelection {
        delta: isize,
    },
    ToggleSettingsGeneralSelected,
    MoveListSelection(isize),
    MovePreviewPane {
        container: String,
        delta: isize,
    },
    ScrollListHorizontal(i16),
    ScrollFocusedListBlockVertical(i16),
}

pub fn update_manager(state: &mut ManagerState<'_>, message: ManagerMessage) -> Dirty {
    match message {
        ManagerMessage::CollapseSelectedTree => collapse_selected_tree(state),
        ManagerMessage::EnterPreview => state.preview_focused = true,
        ManagerMessage::FocusEditorContent => set_editor_tab_bar_focus(state, false),
        ManagerMessage::FocusEditorTabBar => set_editor_tab_bar_focus(state, true),
        ManagerMessage::FocusSettingsContent => set_settings_tab_bar_focus(state, false),
        ManagerMessage::FocusSettingsTabBar => set_settings_tab_bar_focus(state, true),
        ManagerMessage::ExitPreview => state.preview_focused = false,
        ManagerMessage::ExpandSelectedTree => expand_selected_tree(state),
        ManagerMessage::ClearSettingsAuthKind => clear_settings_auth_kind(state),
        ManagerMessage::EnterSettingsAuthKind => enter_settings_auth_kind(state),
        ManagerMessage::ScrollEditorTabHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_editor_tab_horizontal(state, delta, term_width, content_width),
        ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_editor_workspace_mounts_horizontal(state, delta, term_width, content_width),
        ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_settings_global_mounts_horizontal(state, delta, term_width, content_width),
        ManagerMessage::ScrollSettingsTrustHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_settings_trust_horizontal(state, delta, term_width, content_width),
        ManagerMessage::MoveSettingsGlobalMountsSelection {
            delta,
            term,
            footer_h,
        } => move_settings_global_mounts_selection(state, delta, term, footer_h),
        ManagerMessage::MoveSettingsEnvSelection {
            delta,
            term,
            footer_h,
        } => move_settings_env_selection(state, delta, term, footer_h),
        ManagerMessage::MoveSettingsTrustSelection {
            delta,
            term,
            footer_h,
        } => move_settings_trust_selection(state, delta, term, footer_h),
        ManagerMessage::MoveEditorTab {
            delta,
            focus_tab_bar,
        } => move_editor_tab(state, delta, focus_tab_bar),
        ManagerMessage::MoveSettingsTab {
            delta,
            focus_tab_bar,
        } => move_settings_tab(state, delta, focus_tab_bar),
        ManagerMessage::MoveSettingsGeneralSelection { delta } => {
            move_settings_general_selection(state, delta);
        }
        ManagerMessage::MoveSettingsAuthSelection { delta } => {
            move_settings_auth_selection(state, delta);
        }
        ManagerMessage::ToggleSettingsGeneralSelected => toggle_settings_general_selected(state),
        ManagerMessage::MoveListSelection(delta) => move_list_selection(state, delta),
        ManagerMessage::MovePreviewPane { container, delta } => {
            move_preview_pane(state, &container, delta);
        }
        ManagerMessage::ScrollListHorizontal(delta) => scroll_list_horizontal(state, delta),
        ManagerMessage::ScrollFocusedListBlockVertical(delta) => {
            scroll_focused_mount_block_vertical(state, delta);
        }
    }
    Dirty::Redraw
}

fn set_editor_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.tab_bar_focused = focused;
}

fn set_settings_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.tab_bar_focused = focused;
}

fn clear_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.selected_kind = None;
    settings.auth.selected = 0;
}

fn enter_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    if let Some(row) = settings.auth.pending.get(settings.auth.selected) {
        settings.auth.selected_kind = Some(row.kind);
        settings.auth.selected = 0;
    }
}

fn move_editor_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let was_secrets = editor.active_tab == EditorTab::Secrets;
    editor.active_tab = if delta.is_negative() {
        previous_editor_tab(editor.active_tab)
    } else {
        next_editor_tab(editor.active_tab)
    };
    editor.tab_bar_focused = focus_tab_bar;
    editor.active_field = FieldFocus::Row(0);
    editor.tab_scroll_x = 0;
    editor.tab_scroll_y = 0;
    if editor.active_tab != EditorTab::Auth {
        editor.auth_selected_kind = None;
    }
    if was_secrets {
        editor.unmasked_rows.clear();
        editor.secrets_expanded.clear();
    }
}

fn move_settings_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.active_tab = if delta.is_negative() {
        previous_settings_tab(settings.active_tab)
    } else {
        next_settings_tab(settings.active_tab)
    };
    settings.tab_bar_focused = focus_tab_bar;
}

fn move_settings_general_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.general.selected = if delta.is_negative() {
        settings.general.selected.saturating_sub(delta.unsigned_abs())
    } else {
        settings.general.selected.saturating_add(delta as usize).min(1)
    };
}

fn toggle_settings_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match settings.general.selected {
        0 => {
            settings.general.pending_coauthor_trailer =
                !settings.general.pending_coauthor_trailer;
        }
        1 => {
            settings.general.pending_dco = !settings.general.pending_dco;
        }
        _ => {}
    }
}

fn move_settings_auth_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let max = settings.auth.row_count().saturating_sub(1);
    settings.auth.selected = if delta.is_negative() {
        settings.auth.selected.saturating_sub(delta.unsigned_abs())
    } else {
        settings.auth.selected.saturating_add(delta as usize).min(max)
    };
}

fn scroll_editor_tab_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.tab_content_scroll_focused = true;
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut editor.tab_scroll_x,
        delta,
        term_width,
        content_width,
    );
}

fn scroll_editor_workspace_mounts_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.workspace_mounts_scroll_focused = true;
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut editor.workspace_mounts_scroll_x,
        delta,
        term_width,
        content_width,
    );
}

fn scroll_settings_global_mounts_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut settings.mounts.scroll_x,
        delta,
        term_width,
        content_width,
    );
}

fn scroll_settings_trust_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut settings.trust.scroll_x,
        delta,
        term_width,
        content_width,
    );
}

fn move_settings_global_mounts_selection(
    state: &mut ManagerState<'_>,
    delta: isize,
    term: Rect,
    footer_h: u16,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let max = settings.mounts.pending.len();
    settings.mounts.selected = if delta.is_negative() {
        settings
            .mounts
            .selected
            .saturating_sub(delta.unsigned_abs())
    } else {
        settings
            .mounts
            .selected
            .saturating_add(delta as usize)
            .min(max)
    };
    settings.mounts.scroll_y = super::render::cursor_scroll_for_panel(
        settings.mounts.selected,
        settings.mounts.scroll_y,
        term,
        footer_h,
    );
}

fn move_settings_env_selection(
    state: &mut ManagerState<'_>,
    delta: isize,
    term: Rect,
    footer_h: u16,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let rows = settings_env_flat_rows(settings);
    let max = rows.len().saturating_sub(1);
    let candidate = if delta.is_negative() {
        settings.env.selected.saturating_sub(delta.unsigned_abs())
    } else {
        settings.env.selected.saturating_add(delta as usize).min(max)
    };
    settings.env.selected = if delta.is_negative() {
        step_settings_env_cursor_up(&rows, candidate)
    } else {
        step_settings_env_cursor_down(&rows, candidate, max)
    };
    settings.env.scroll_y = super::render::cursor_scroll_for_panel(
        settings.env.selected,
        settings.env.scroll_y,
        term,
        footer_h,
    );
}

fn step_settings_env_cursor_down(rows: &[SettingsEnvRow], candidate: usize, max: usize) -> usize {
    let mut idx = candidate;
    while idx <= max {
        match rows.get(idx) {
            Some(SettingsEnvRow::SectionSpacer) => idx += 1,
            _ => return idx,
        }
    }
    candidate
}

fn step_settings_env_cursor_up(rows: &[SettingsEnvRow], candidate: usize) -> usize {
    let mut idx = candidate;
    loop {
        match rows.get(idx) {
            Some(SettingsEnvRow::SectionSpacer) => {
                if idx == 0 {
                    return 0;
                }
                idx -= 1;
            }
            _ => return idx,
        }
    }
}

fn move_settings_trust_selection(
    state: &mut ManagerState<'_>,
    delta: isize,
    term: Rect,
    footer_h: u16,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let max = settings.trust.pending.len().saturating_sub(1);
    settings.trust.selected = if delta.is_negative() {
        settings
            .trust
            .selected
            .saturating_sub(delta.unsigned_abs())
    } else {
        settings
            .trust
            .selected
            .saturating_add(delta as usize)
            .min(max)
    };
    settings.trust.scroll_y = super::render::cursor_scroll_for_panel(
        settings.trust.selected,
        settings.trust.scroll_y,
        term,
        footer_h,
    );
}

const fn previous_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Auth,
        EditorTab::Mounts => EditorTab::General,
        EditorTab::Roles => EditorTab::Mounts,
        EditorTab::Secrets => EditorTab::Roles,
        EditorTab::Auth => EditorTab::Secrets,
    }
}

const fn next_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Mounts,
        EditorTab::Mounts => EditorTab::Roles,
        EditorTab::Roles => EditorTab::Secrets,
        EditorTab::Secrets => EditorTab::Auth,
        EditorTab::Auth => EditorTab::General,
    }
}

const fn previous_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Trust,
        SettingsTab::Mounts => SettingsTab::General,
        SettingsTab::Environments => SettingsTab::Mounts,
        SettingsTab::Auth => SettingsTab::Environments,
        SettingsTab::Trust => SettingsTab::Auth,
    }
}

const fn next_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Mounts,
        SettingsTab::Mounts => SettingsTab::Environments,
        SettingsTab::Environments => SettingsTab::Auth,
        SettingsTab::Auth => SettingsTab::Trust,
        SettingsTab::Trust => SettingsTab::General,
    }
}

fn collapse_selected_tree(state: &mut ManagerState<'_>) {
    state.inline_new_session_picker = None;
    match state.selected_row() {
        ManagerListRow::SavedWorkspace(i) => {
            state.collapse_workspace(i);
        }
        ManagerListRow::WorkspaceInstance(ws_idx, _) => {
            state.collapse_workspace(ws_idx);
        }
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            state.collapse_current_dir();
        }
        ManagerListRow::NewWorkspace => {}
    }
}

fn expand_selected_tree(state: &mut ManagerState<'_>) {
    state.inline_new_session_picker = None;
    match state.selected_row() {
        ManagerListRow::SavedWorkspace(i) => state.expand_workspace(i),
        ManagerListRow::CurrentDirectory => state.expand_current_dir(),
        ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => {}
    }
}

fn move_list_selection(state: &mut ManagerState<'_>, delta: isize) {
    state.inline_role_picker = None;
    state.inline_agent_picker = None;
    state.inline_new_session_picker = None;
    let last = state.row_count().saturating_sub(1);
    let selected = if delta.is_negative() {
        state.selected.saturating_sub(delta.unsigned_abs())
    } else {
        state.selected.saturating_add(delta as usize).min(last)
    };
    if selected != state.selected {
        state.reset_list_scroll();
        state.selected = selected;
    }
}

fn move_preview_pane(state: &mut ManagerState<'_>, container: &str, delta: isize) {
    let len = state.flattened_preview_panes(container).len();
    if len == 0 {
        state.preview_focused = false;
        return;
    }
    let cursor = state
        .preview_pane_cursor
        .get(container)
        .copied()
        .unwrap_or(0)
        .min(len - 1);
    let next = if delta.is_negative() {
        cursor.saturating_sub(delta.unsigned_abs())
    } else {
        cursor.saturating_add(delta as usize).min(len - 1)
    };
    state.preview_pane_cursor.insert(container.to_owned(), next);
}

const fn scroll_list_horizontal(state: &mut ManagerState<'_>, delta: i16) {
    if state.list_names_focused {
        jackin_tui::components::apply_scroll_delta_unclamped(&mut state.list_names_scroll_x, delta);
    } else {
        scroll_focused_mount_block(state, delta);
    }
}

const fn scroll_focused_mount_block(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_x_mut(focus);
    jackin_tui::components::apply_scroll_delta_unclamped(value, delta);
}

const fn scroll_focused_mount_block_vertical(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_y_mut(focus);
    jackin_tui::components::apply_scroll_delta_unclamped(value, delta);
}

#[cfg(test)]
mod tests {
    use super::{ManagerMessage, update_manager};
    use crate::console::manager::state::{
        EditorState, EditorTab, FieldFocus, ManagerStage, ManagerState, MountScrollFocus,
        SettingsState, SettingsTab,
    };

    fn state_with_saved_count(count: usize) -> ManagerState<'static> {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        let mut config = crate::config::AppConfig::default();
        for idx in 0..count {
            config.workspaces.insert(
                format!("workspace-{idx}"),
                crate::workspace::WorkspaceConfig {
                    workdir: format!("/tmp/workspace-{idx}"),
                    ..crate::workspace::WorkspaceConfig::default()
                },
            );
        }
        ManagerState::from_config(&config, cwd)
    }

    #[test]
    fn move_list_selection_clamps() {
        let mut state = state_with_saved_count(2);
        state.selected = 1;

        assert!(update_manager(&mut state, ManagerMessage::MoveListSelection(99)).is_dirty());

        assert_eq!(state.selected, state.row_count() - 1);
    }

    #[test]
    fn scroll_focused_list_block_updates_selected_axis() {
        let mut state = state_with_saved_count(1);
        state.list_scroll_focus = Some(MountScrollFocus::Workspace);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollFocusedListBlockVertical(3),
            )
            .is_dirty()
        );

        assert_eq!(state.list_mounts_scroll_y, 3);
    }

    #[test]
    fn expand_and_collapse_selected_tree_updates_current_dir() {
        let mut state = state_with_saved_count(1);

        assert!(update_manager(&mut state, ManagerMessage::ExpandSelectedTree).is_dirty());
        assert!(state.current_dir_expanded);

        assert!(update_manager(&mut state, ManagerMessage::CollapseSelectedTree).is_dirty());
        assert!(!state.current_dir_expanded);
    }

    #[test]
    fn move_editor_tab_resets_tab_local_view_state() {
        let mut state = state_with_saved_count(0);
        let mut editor = EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        editor.active_tab = EditorTab::Secrets;
        editor.tab_bar_focused = false;
        editor.active_field = FieldFocus::Row(7);
        editor.tab_scroll_x = 4;
        editor.tab_scroll_y = 5;
        editor.secrets_expanded.insert("role".into());
        state.stage = ManagerStage::Editor(editor);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveEditorTab {
                    delta: 1,
                    focus_tab_bar: true,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.active_tab, EditorTab::Auth);
        assert!(editor.tab_bar_focused);
        assert_eq!(editor.active_field, FieldFocus::Row(0));
        assert_eq!(editor.tab_scroll_x, 0);
        assert_eq!(editor.tab_scroll_y, 0);
        assert!(editor.secrets_expanded.is_empty());
    }

    #[test]
    fn move_settings_tab_cycles_and_sets_focus() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.active_tab = SettingsTab::Trust;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsTab {
                    delta: 1,
                    focus_tab_bar: true,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.active_tab, SettingsTab::General);
        assert!(settings.tab_bar_focused);
    }

    #[test]
    fn settings_general_selection_and_toggle_update_state() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.general.pending_dco = false;
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsGeneralSelection { delta: 1 },
            )
            .is_dirty()
        );
        assert!(update_manager(&mut state, ManagerMessage::ToggleSettingsGeneralSelected).is_dirty());

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.general.selected, 1);
        assert!(settings.general.pending_dco);
    }

    #[test]
    fn settings_auth_selection_and_kind_entry_update_state() {
        let mut state = state_with_saved_count(0);
        state.stage =
            ManagerStage::Settings(SettingsState::from_config(&crate::config::AppConfig::default()));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsAuthSelection { delta: 99 },
            )
            .is_dirty()
        );
        assert!(update_manager(&mut state, ManagerMessage::EnterSettingsAuthKind).is_dirty());

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.auth.selected, 0);
        assert!(settings.auth.selected_kind.is_some());

        assert!(update_manager(&mut state, ManagerMessage::ClearSettingsAuthKind).is_dirty());

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.auth.selected, 0);
        assert!(settings.auth.selected_kind.is_none());
    }

    #[test]
    fn scroll_editor_tab_marks_panel_focus_and_updates_offset() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Editor(EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollEditorTabHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.tab_content_scroll_focused);
        assert_eq!(editor.tab_scroll_x, 8);
    }

    #[test]
    fn scroll_editor_workspace_mounts_marks_mounts_focus_and_updates_offset() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Editor(EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.workspace_mounts_scroll_focused);
        assert_eq!(editor.workspace_mounts_scroll_x, 8);
    }

    #[test]
    fn scroll_settings_global_mounts_updates_offset() {
        let mut state = state_with_saved_count(0);
        state.stage =
            ManagerStage::Settings(SettingsState::from_config(&crate::config::AppConfig::default()));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.mounts.scroll_x, 8);
    }

    #[test]
    fn move_settings_global_mounts_selection_clamps_to_add_row() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.mounts.pending.push(crate::config::GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: crate::workspace::MountConfig {
                src: "/tmp/cache".into(),
                dst: "/home/agent/.cache".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        });
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsGlobalMountsSelection {
                    delta: 99,
                    term: Rect::new(0, 0, 80, 24),
                    footer_h: 1,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.mounts.selected, settings.mounts.pending.len());
    }

    #[test]
    fn move_settings_env_selection_skips_section_spacers() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings
            .env
            .pending
            .env
            .insert("ALPHA".into(), crate::operator_env::EnvValue::Plain("one".into()));
        settings
            .env
            .pending
            .env
            .insert("BETA".into(), crate::operator_env::EnvValue::Plain("two".into()));
        settings.env.selected = 1;
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsEnvSelection {
                    delta: 1,
                    term: Rect::new(0, 0, 80, 24),
                    footer_h: 1,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.env.selected, 3);
    }

    #[test]
    fn scroll_settings_trust_updates_offset() {
        let mut state = state_with_saved_count(0);
        let mut config = crate::config::AppConfig::default();
        config.roles.insert(
            "chainargos/agent-smith".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/agent-smith".into(),
                trusted: true,
                ..crate::config::RoleSource::default()
            },
        );
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollSettingsTrustHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.trust.scroll_x, 8);
    }

    #[test]
    fn move_settings_trust_selection_clamps_to_role_rows() {
        let mut state = state_with_saved_count(0);
        let mut config = crate::config::AppConfig::default();
        config.roles.insert(
            "chainargos/agent-a".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/agent-a".into(),
                trusted: false,
                ..crate::config::RoleSource::default()
            },
        );
        config.roles.insert(
            "chainargos/agent-b".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/agent-b".into(),
                trusted: true,
                ..crate::config::RoleSource::default()
            },
        );
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsTrustSelection {
                    delta: 99,
                    term: Rect::new(0, 0, 80, 24),
                    footer_h: 1,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.trust.selected, settings.trust.pending.len() - 1);
    }
}
