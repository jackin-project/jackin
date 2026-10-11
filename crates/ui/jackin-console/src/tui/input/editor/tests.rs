// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the editor-stage keymap dispatch resolver.

use super::dispatch_editor_top_level;
use crate::tui::screens::editor::model::{EditorNavigationKeyPlan, EditorTopLevelKeyPlan};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The editor top-level resolver composes three keymaps (global → tab-bar →
/// content) in precedence order. This asserts that composition end-to-end, so
/// the ordering logic in `dispatch_editor_top_level` cannot regress. Per-keymap
/// chord coverage lives in `tui::keymap::tests`.
#[test]
fn dispatch_editor_top_level_preserves_precedence() {
    // Global keymap wins regardless of focus.
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('s')), false),
        EditorTopLevelKeyPlan::Save
    );
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Esc), false),
        EditorTopLevelKeyPlan::Escape
    );

    // Tab-bar keymap applies only when the tab bar has focus.
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

    // Content keymap (tab bar not focused, or keys that fall through).
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
    // A printable char that is no shortcut → immediate-action check.
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::Char('z')), false),
        EditorTopLevelKeyPlan::CheckImmediateAction
    );
    // A non-char, non-shortcut key → fall through to tab actions.
    assert_eq!(
        dispatch_editor_top_level(key(KeyCode::PageDown), false),
        EditorTopLevelKeyPlan::ContinueToTabActions
    );
}
