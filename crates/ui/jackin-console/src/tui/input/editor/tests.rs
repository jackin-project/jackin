// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::dispatch_editor_top_level;
use crate::tui::screens::editor::model::{EditorNavigationKeyPlan, EditorTopLevelKeyPlan};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn dispatch_editor_top_level_preserves_precedence() {
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('s')), false),
        EditorTopLevelKeyPlan::Save
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Esc), false),
        EditorTopLevelKeyPlan::Escape
    );

    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Left), true),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
            delta: -1,
            focus_tab_bar: true,
        })
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Right), true),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        })
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Down), true),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusContent)
    );

    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::BackTab), false),
        EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusTabBar)
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('h')), false),
        EditorTopLevelKeyPlan::ScrollHorizontal { delta: -8 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('L')), false),
        EditorTopLevelKeyPlan::ScrollHorizontal { delta: 8 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('k')), false),
        EditorTopLevelKeyPlan::MoveField { delta: -1 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Down), false),
        EditorTopLevelKeyPlan::MoveField { delta: 1 }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Right), false),
        EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: true }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Left), false),
        EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: false }
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Enter), false),
        EditorTopLevelKeyPlan::CheckImmediateAction
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('z')), false),
        EditorTopLevelKeyPlan::CheckImmediateAction
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::PageDown), false),
        EditorTopLevelKeyPlan::ContinueToTabActions
    );
}
