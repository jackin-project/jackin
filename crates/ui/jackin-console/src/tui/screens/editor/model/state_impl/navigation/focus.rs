// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` content area, scroll, and hover.

use super::super::super::{
    EditorFocusTarget, EditorHoverTarget, EditorNavigationKeyPlan, EditorState, EditorTab,
};

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    #[must_use]
    pub fn navigation_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorNavigationKeyPlan {
        use crossterm::event::KeyCode;

        match key_code {
            KeyCode::Left | KeyCode::BackTab if self.tab_bar_focused() => {
                EditorNavigationKeyPlan::MoveTab {
                    delta: -1,
                    focus_tab_bar: true,
                }
            }
            KeyCode::Right if self.tab_bar_focused() => EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            },
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j' | 'J') if self.tab_bar_focused() => {
                EditorNavigationKeyPlan::FocusContent
            }
            KeyCode::Tab => EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            },
            KeyCode::BackTab => EditorNavigationKeyPlan::FocusTabBar,
            _ => EditorNavigationKeyPlan::NotNavigation,
        }
    }

    #[must_use]
    pub const fn content_area(&self, term_size: ratatui::layout::Rect) -> ratatui::layout::Rect {
        crate::tui::layout::tabbed_content_area(term_size, self.cached_footer_h)
    }

    pub fn set_tab_bar_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner.focus_tab_bar();
        } else if matches!(self.active_tab, EditorTab::Mounts) {
            self.focus_owner
                .focus_content(EditorFocusTarget::WorkspaceMounts);
        } else {
            self.focus_owner
                .focus_content(EditorFocusTarget::TabContent);
        }
    }

    pub fn apply_tab_bar_focus_plan(&mut self, focused: bool) {
        self.set_tab_bar_focused(focused);
    }

    #[must_use]
    pub fn workspace_mounts_scroll_focused(&self) -> bool {
        self.focus_owner
            .is_content(EditorFocusTarget::WorkspaceMounts)
    }

    pub fn set_workspace_mounts_scroll_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner
                .focus_content(EditorFocusTarget::WorkspaceMounts);
        } else if self.workspace_mounts_scroll_focused() {
            self.focus_owner.focus_tab_bar();
        }
    }

    #[must_use]
    pub fn tab_content_scroll_focused(&self) -> bool {
        self.focus_owner.is_content(EditorFocusTarget::TabContent)
    }

    pub fn set_tab_content_scroll_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner
                .focus_content(EditorFocusTarget::TabContent);
        } else if self.tab_content_scroll_focused() {
            self.focus_owner.focus_tab_bar();
        }
    }

    #[must_use]
    pub const fn hovered_tab(&self) -> Option<usize> {
        match self.hover_target {
            Some(EditorHoverTarget::Tab(index)) => Some(index),
            _ => None,
        }
    }

    #[must_use]
    pub const fn hovered_mount_row(&self) -> Option<usize> {
        match self.hover_target {
            Some(EditorHoverTarget::MountRow(index)) => Some(index),
            _ => None,
        }
    }

    pub fn set_hover_target(&mut self, target: Option<EditorHoverTarget>) {
        self.hover_target = target;
    }
}
