// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Preview pane key plans.

use super::{
    PreviewFocusPlan, PreviewPaneActionPlan, PreviewPaneKeyPlan, WorkspaceListFocusOwner,
    WorkspaceListTopLevelKeyPlan, should_enter_preview_pane, workspace_list_focus_head,
    workspace_list_focus_next, workspace_list_key_plan,
};

use crossterm::event::KeyCode;

use super::super::model::ManagerListRow;

#[must_use]
pub fn workspace_list_top_level_key_plan(
    key: KeyCode,
    preview_focused: bool,
    selected_row: ManagerListRow,
    selected_preview_pane_count: Option<usize>,
    list_scroll_focus_active: bool,
) -> WorkspaceListTopLevelKeyPlan {
    if preview_focused {
        return WorkspaceListTopLevelKeyPlan::PreviewFocused;
    }
    if let Some(pane_count) = selected_preview_pane_count
        && should_enter_preview_pane(key, selected_row, pane_count)
    {
        return WorkspaceListTopLevelKeyPlan::EnterPreview;
    }
    WorkspaceListTopLevelKeyPlan::ListKey(workspace_list_key_plan(key, list_scroll_focus_active))
}

#[must_use]
pub const fn enter_preview_focus_plan() -> PreviewFocusPlan {
    // Entering the preview walks the focus chain one step past its head.
    PreviewFocusPlan {
        focused: matches!(
            workspace_list_focus_next(workspace_list_focus_head()),
            WorkspaceListFocusOwner::Preview
        ),
    }
}

#[must_use]
pub const fn exit_preview_focus_plan() -> PreviewFocusPlan {
    // Exiting the preview returns focus to the chain head.
    PreviewFocusPlan {
        focused: matches!(
            workspace_list_focus_head(),
            WorkspaceListFocusOwner::Preview
        ),
    }
}

/// Preview-pane navigation mode: Esc / Left / `BackTab` exits, Up/Down
/// move inside the snapshot, and Enter reconnects to the selected pane.
#[must_use]
pub fn preview_pane_key_plan(key: KeyCode, pane_count: usize) -> PreviewPaneKeyPlan {
    use crate::tui::keymap::{PREVIEW_PANE_KEYMAP, PreviewPaneAction as A, bridged_keymap_action};

    if pane_count == 0 {
        return PreviewPaneKeyPlan::ExitPreview;
    }
    match bridged_keymap_action(
        &PREVIEW_PANE_KEYMAP,
        termrock::input::KeyEvent::new(
            termrock::input::KeyCode::from(key),
            termrock::input::KeyModifiers::NONE,
        ),
    ) {
        Some(A::Back) => PreviewPaneKeyPlan::ExitPreview,
        Some(A::NavigateUp) => PreviewPaneKeyPlan::Move { delta: -1 },
        Some(A::NavigateDown) => PreviewPaneKeyPlan::Move { delta: 1 },
        Some(A::Attach) => PreviewPaneKeyPlan::ReconnectSelected,
        None => PreviewPaneKeyPlan::Continue,
    }
}

#[must_use]
pub fn preview_pane_selected_index(
    pane_count: usize,
    current_cursor: Option<usize>,
) -> Option<usize> {
    if pane_count == 0 {
        return None;
    }
    Some(current_cursor.unwrap_or(0).min(pane_count - 1))
}

#[must_use]
pub fn preview_pane_cursor_plan(
    pane_count: usize,
    current_cursor: Option<usize>,
    delta: isize,
) -> Option<usize> {
    let cursor = preview_pane_selected_index(pane_count, current_cursor)?;
    Some(crate::tui::focus::collection_move_index(
        cursor, pane_count, delta,
    ))
}

#[must_use]
pub fn preview_pane_action_plan(
    key: KeyCode,
    current_cursor: Option<usize>,
    session_ids: impl IntoIterator<Item = u64>,
) -> PreviewPaneActionPlan {
    let session_ids: Vec<u64> = session_ids.into_iter().collect();
    match preview_pane_key_plan(key, session_ids.len()) {
        PreviewPaneKeyPlan::ExitPreview => PreviewPaneActionPlan::ExitPreview,
        PreviewPaneKeyPlan::Move { delta } => PreviewPaneActionPlan::Move { delta },
        PreviewPaneKeyPlan::ReconnectSelected => {
            let Some(cursor) = preview_pane_selected_index(session_ids.len(), current_cursor)
            else {
                return PreviewPaneActionPlan::Continue;
            };
            session_ids
                .get(cursor)
                .copied()
                .map_or(PreviewPaneActionPlan::Continue, |session_id| {
                    PreviewPaneActionPlan::ReconnectSelected { session_id }
                })
        }
        PreviewPaneKeyPlan::Continue => PreviewPaneActionPlan::Continue,
    }
}
