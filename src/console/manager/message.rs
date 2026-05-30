//! Workspace-manager message/update boundary.
//!
//! This starts the Model/Update/View migration with state-only list messages.
//! Input handlers should increasingly translate terminal events into these
//! messages instead of mutating `ManagerState` inline.

use super::state::{ManagerListRow, ManagerState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerMessage {
    CollapseSelectedTree,
    EnterPreview,
    ExitPreview,
    ExpandSelectedTree,
    MoveListSelection(isize),
    MovePreviewPane {
        container: String,
        delta: isize,
    },
    ScrollListHorizontal(i16),
    ScrollFocusedListBlockVertical(i16),
}

pub fn update_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    match message {
        ManagerMessage::CollapseSelectedTree => collapse_selected_tree(state),
        ManagerMessage::EnterPreview => state.preview_focused = true,
        ManagerMessage::ExitPreview => state.preview_focused = false,
        ManagerMessage::ExpandSelectedTree => expand_selected_tree(state),
        ManagerMessage::MoveListSelection(delta) => move_list_selection(state, delta),
        ManagerMessage::MovePreviewPane { container, delta } => {
            move_preview_pane(state, &container, delta);
        }
        ManagerMessage::ScrollListHorizontal(delta) => scroll_list_horizontal(state, delta),
        ManagerMessage::ScrollFocusedListBlockVertical(delta) => {
            scroll_focused_mount_block_vertical(state, delta);
        }
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
    use crate::console::manager::state::{ManagerState, MountScrollFocus};

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

        update_manager(&mut state, ManagerMessage::MoveListSelection(99));

        assert_eq!(state.selected, state.row_count() - 1);
    }

    #[test]
    fn scroll_focused_list_block_updates_selected_axis() {
        let mut state = state_with_saved_count(1);
        state.list_scroll_focus = Some(MountScrollFocus::Workspace);

        update_manager(
            &mut state,
            ManagerMessage::ScrollFocusedListBlockVertical(3),
        );

        assert_eq!(state.list_mounts_scroll_y, 3);
    }

    #[test]
    fn expand_and_collapse_selected_tree_updates_current_dir() {
        let mut state = state_with_saved_count(1);

        update_manager(&mut state, ManagerMessage::ExpandSelectedTree);
        assert!(state.current_dir_expanded);

        update_manager(&mut state, ManagerMessage::CollapseSelectedTree);
        assert!(!state.current_dir_expanded);
    }
}
