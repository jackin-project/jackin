// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` list scroll accessors and state.

use super::super::{ManagerState, MountScrollFocus};

impl ManagerState<'_> {
    pub fn list_scroll_state_mut(
        &mut self,
        focus: MountScrollFocus,
    ) -> &mut termrock::widgets::ScrollAreaState {
        match focus {
            MountScrollFocus::Workspace => &mut self.list_mounts_scroll,
            MountScrollFocus::Global => &mut self.list_global_mounts_scroll,
            MountScrollFocus::RoleGlobal => &mut self.list_role_global_mounts_scroll,
            MountScrollFocus::Roles => &mut self.list_roles_scroll,
        }
    }

    pub fn reset_list_scroll(&mut self) {
        self.list_mounts_scroll = crate::tui::scroll_block::console_scroll_area_state();
        self.list_global_mounts_scroll = crate::tui::scroll_block::console_scroll_area_state();
        self.list_role_global_mounts_scroll = crate::tui::scroll_block::console_scroll_area_state();
        self.list_roles_scroll = crate::tui::scroll_block::console_scroll_area_state();
        self.list_focus_owner.focus_tab_bar();
        self.list_names_scroll = crate::tui::scroll_block::console_scroll_area_state();
    }

    pub fn list_names_focused(&self) -> bool {
        self.list_focus_owner.is_tab_bar()
    }

    pub fn set_list_names_focused(&mut self, focused: bool) {
        if focused {
            self.list_focus_owner.focus_tab_bar();
        } else if self.list_names_focused() {
            self.list_focus_owner
                .focus_content(MountScrollFocus::Workspace);
        }
    }

    pub fn list_scroll_focus(&self) -> Option<MountScrollFocus> {
        self.list_focus_owner.focused_content()
    }

    pub fn set_list_scroll_focus(&mut self, focus: Option<MountScrollFocus>) {
        if let Some(focus) = focus {
            self.list_focus_owner.focus_content(focus);
        } else {
            self.list_focus_owner.focus_tab_bar();
        }
    }
}
