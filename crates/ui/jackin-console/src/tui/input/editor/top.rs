// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crossterm::event::KeyEvent;

use crate::tui::screens::editor::model::{EditorNavigationKeyPlan, EditorTopLevelKeyPlan};

use crate::tui::keymap::{
    EDITOR_CONTENT_KEYMAP, EDITOR_GLOBAL_KEYMAP, EDITOR_TAB_BAR_KEYMAP, EditorContentAction,
    EditorGlobalAction, EditorTabBarAction, bridged_keymap_action,
};

pub(crate) fn dispatch_editor_top_level(
    key: KeyEvent,
    tab_bar_focused: bool,
) -> EditorTopLevelKeyPlan {
    use crossterm::event::KeyCode;

    let event = termrock::input::KeyEvent::from(key);

    if let Some(action) = bridged_keymap_action(&EDITOR_GLOBAL_KEYMAP, event) {
        return match action {
            EditorGlobalAction::Save => EditorTopLevelKeyPlan::Save,
            EditorGlobalAction::Escape => EditorTopLevelKeyPlan::Escape,
        };
    }

    if tab_bar_focused && let Some(action) = bridged_keymap_action(&EDITOR_TAB_BAR_KEYMAP, event) {
        return match action {
            EditorTabBarAction::PrevTab => {
                EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
                    delta: -1,
                    focus_tab_bar: true,
                })
            }
            EditorTabBarAction::NextTab => {
                EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
                    delta: 1,
                    focus_tab_bar: true,
                })
            }
            EditorTabBarAction::FocusContent => {
                EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusContent)
            }
        };
    }

    match bridged_keymap_action(&EDITOR_CONTENT_KEYMAP, event) {
        Some(EditorContentAction::MoveUp) => EditorTopLevelKeyPlan::MoveField { delta: -1 },
        Some(EditorContentAction::MoveDown) => EditorTopLevelKeyPlan::MoveField { delta: 1 },
        Some(EditorContentAction::ScrollLeft) => {
            EditorTopLevelKeyPlan::ScrollHorizontal { delta: -8 }
        }
        Some(EditorContentAction::ScrollRight) => {
            EditorTopLevelKeyPlan::ScrollHorizontal { delta: 8 }
        }
        Some(EditorContentAction::CollapseHeader) => {
            EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: false }
        }
        Some(EditorContentAction::ExpandHeader) => {
            EditorTopLevelKeyPlan::SetRoleHeaderExpanded { expanded: true }
        }
        Some(EditorContentAction::NextTab) => {
            EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            })
        }
        Some(EditorContentAction::FocusTabBar) => {
            EditorTopLevelKeyPlan::Navigation(EditorNavigationKeyPlan::FocusTabBar)
        }
        Some(EditorContentAction::CheckImmediate) => EditorTopLevelKeyPlan::CheckImmediateAction,
        None => {
            if matches!(key.code, KeyCode::Char(_)) {
                EditorTopLevelKeyPlan::CheckImmediateAction
            } else {
                EditorTopLevelKeyPlan::ContinueToTabActions
            }
        }
    }
}
