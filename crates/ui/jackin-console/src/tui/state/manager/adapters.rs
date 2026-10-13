// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` screen-trait adapters.

use crate::tui::model::{
    ConsoleManagerStageState, LaunchAccountPickerManagerState, LaunchAgentPromptManagerState,
    LaunchRolePromptManagerState,
};

use crate::tui::screens::workspaces::update::{
    PreviewFocusState, PreviewPaneCursorState, WorkspaceListHoverState,
    WorkspaceListSelectionState, WorkspaceTreeDisclosureState,
};

use crate::tui::update::{
    InlineAccountPickerState, InlineNewSessionPickerState, InlinePickerDismissalState,
    ListModalState, StatusOverlayState,
};

use super::super::{ManagerStage, ManagerState, Modal, MountScrollFocus};

impl WorkspaceTreeDisclosureState for ManagerState<'_> {
    fn collapse_workspace(&mut self, index: usize) {
        Self::collapse_workspace(self, index);
    }

    fn collapse_current_dir(&mut self) {
        Self::collapse_current_dir(self);
    }

    fn expand_workspace(&mut self, index: usize) {
        Self::expand_workspace(self, index);
    }

    fn expand_current_dir(&mut self) {
        Self::expand_current_dir(self);
    }
}

impl WorkspaceListSelectionState for ManagerState<'_> {
    fn clear_inline_role_picker(&mut self) {
        self.inline_role_picker = None;
    }

    fn clear_inline_agent_picker(&mut self) {
        self.inline_agent_picker = None;
    }

    fn clear_inline_new_session_picker(&mut self) {
        self.inline_new_session_picker = None;
    }

    fn clear_inline_account_picker(&mut self) {
        self.inline_account_picker = None;
    }

    fn clear_launch_account_picker(&mut self) {
        self.launch_account_picker = None;
    }

    fn reset_list_scroll(&mut self) {
        Self::reset_list_scroll(self);
    }

    fn set_selected(&mut self, selected: usize) {
        self.selected = selected;
    }
}

impl ConsoleManagerStageState<ManagerStage<'static>> for ManagerState<'_> {
    fn set_manager_stage(&mut self, stage: ManagerStage<'static>) {
        self.stage = stage;
    }
}

impl LaunchAgentPromptManagerState<jackin_core::RoleSelector, jackin_core::Agent>
    for ManagerState<'_>
{
    fn open_launch_agent_prompt(
        &mut self,
        role: jackin_core::RoleSelector,
        picker: crate::tui::components::agent_choice::AgentChoiceState<jackin_core::Agent>,
    ) {
        self.inline_agent_picker = Some((role, picker));
    }

    fn clear_launch_role_prompt(&mut self) {
        self.inline_role_picker = None;
    }
}

impl LaunchRolePromptManagerState<jackin_core::RoleSelector> for ManagerState<'_> {
    fn open_launch_role_prompt(
        &mut self,
        picker: crate::tui::components::role_picker::RolePickerState<jackin_core::RoleSelector>,
    ) {
        self.inline_role_picker = Some(picker);
    }
}

impl
    LaunchAccountPickerManagerState<
        jackin_core::RoleSelector,
        jackin_core::Agent,
        crate::services::launch::AccountChoice,
    > for ManagerState<'_>
{
    fn open_launch_account_picker(
        &mut self,
        picker: crate::tui::components::account_picker::AccountPickerState<
            jackin_core::RoleSelector,
            jackin_core::Agent,
            crate::services::launch::AccountChoice,
        >,
    ) {
        self.launch_account_picker = Some(picker);
    }
}

impl WorkspaceListHoverState for ManagerState<'_> {
    fn set_workspace_list_hover_target(
        &mut self,
        target: Option<super::super::ManagerHoverTarget>,
    ) {
        self.hover_target = target;
    }
}

impl StatusOverlayState for ManagerState<'_> {
    fn set_status_overlay(&mut self, overlay: Option<crate::tui::components::StatusPopupState>) {
        self.status_overlay = overlay;
    }
}

impl ListModalState for ManagerState<'_> {
    fn open_container_info_modal(
        &mut self,
        state: crate::tui::components::container_info_surface::ContainerInfoState,
    ) {
        self.list_modal = Some(Modal::ContainerInfo { state });
    }

    fn open_error_popup_modal(&mut self, state: crate::tui::components::ErrorPopupState) {
        self.list_modal = Some(Modal::ErrorPopup { state });
    }

    fn open_github_picker_modal(
        &mut self,
        state: crate::tui::components::github_picker::GithubPickerState,
    ) {
        self.list_modal = Some(Modal::GithubPicker { state });
    }

    fn dismiss_list_modal(&mut self) {
        self.list_modal = None;
    }
}

impl InlinePickerDismissalState for ManagerState<'_> {
    fn clear_inline_new_session_picker(&mut self) {
        self.inline_new_session_picker = None;
    }

    fn clear_inline_role_picker(&mut self) {
        self.inline_role_picker = None;
    }

    fn clear_inline_agent_picker(&mut self) {
        self.inline_agent_picker = None;
    }

    fn clear_inline_account_picker(&mut self) {
        self.inline_account_picker = None;
    }

    fn clear_launch_account_picker(&mut self) {
        self.launch_account_picker = None;
    }
}

impl InlineNewSessionPickerState<String, jackin_core::Agent, crate::services::launch::AccountChoice>
    for ManagerState<'_>
{
    fn set_inline_new_session_picker(
        &mut self,
        context: String,
        picker: crate::tui::components::agent_choice::AgentChoiceState<jackin_core::Agent>,
        providers: Vec<crate::services::launch::AccountChoice>,
    ) {
        self.inline_new_session_picker = Some((context, picker, providers));
    }
}

impl InlineAccountPickerState<String, jackin_core::Agent, crate::services::launch::AccountChoice>
    for ManagerState<'_>
{
    fn set_inline_account_picker(
        &mut self,
        picker: crate::tui::components::account_picker::AccountPickerState<
            String,
            jackin_core::Agent,
            crate::services::launch::AccountChoice,
        >,
    ) {
        self.inline_account_picker = Some(picker);
    }
}

impl PreviewFocusState for ManagerState<'_> {
    fn set_preview_focused(&mut self, focused: bool) {
        self.preview_focused = focused;
    }
}

impl PreviewPaneCursorState for ManagerState<'_> {
    fn set_preview_pane_cursor(&mut self, container: &str, cursor: usize) {
        self.preview_pane_cursor
            .insert(container.to_owned(), cursor);
    }
}

impl crate::tui::screens::workspaces::update::WorkspaceListScrollState for ManagerState<'_> {
    fn list_names_scroll_x(&self) -> u16 {
        self.list_names_scroll.offset_x()
    }

    fn set_list_names_scroll_x(&mut self, value: u16) {
        crate::tui::scroll_block::scroll_area_set_x(&mut self.list_names_scroll, value);
    }

    fn block_scroll_x(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.list_mounts_scroll.offset_x(),
            MountScrollFocus::Global => self.list_global_mounts_scroll.offset_x(),
            MountScrollFocus::RoleGlobal => self.list_role_global_mounts_scroll.offset_x(),
            MountScrollFocus::Roles => self.list_roles_scroll.offset_x(),
        }
    }

    fn set_block_scroll_x(&mut self, focus: MountScrollFocus, value: u16) {
        crate::tui::scroll_block::scroll_area_set_x(self.list_scroll_state_mut(focus), value);
    }

    fn block_scroll_y(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.list_mounts_scroll.offset_y(),
            MountScrollFocus::Global => self.list_global_mounts_scroll.offset_y(),
            MountScrollFocus::RoleGlobal => self.list_role_global_mounts_scroll.offset_y(),
            MountScrollFocus::Roles => self.list_roles_scroll.offset_y(),
        }
    }

    fn set_block_scroll_y(&mut self, focus: MountScrollFocus, value: u16) {
        crate::tui::scroll_block::scroll_area_set_y(self.list_scroll_state_mut(focus), value);
    }
}
