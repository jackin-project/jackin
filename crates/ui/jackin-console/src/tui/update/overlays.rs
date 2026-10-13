// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Status and list-modal overlay plans.

#[derive(Debug, Clone)]
pub enum StatusOverlayPlan {
    Open(crate::tui::components::StatusPopupState),
    Dismiss,
}

pub trait StatusOverlayState {
    fn set_status_overlay(&mut self, overlay: Option<crate::tui::components::StatusPopupState>);
}

pub fn apply_status_overlay_plan(state: &mut impl StatusOverlayState, plan: StatusOverlayPlan) {
    match plan {
        StatusOverlayPlan::Open(overlay) => state.set_status_overlay(Some(overlay)),
        StatusOverlayPlan::Dismiss => state.set_status_overlay(None),
    }
}

#[derive(Debug)]
pub enum ListModalPlan {
    ContainerInfo(crate::tui::components::container_info_surface::ContainerInfoState),
    ErrorPopup(crate::tui::components::ErrorPopupState),
    GithubPicker(crate::tui::components::github_picker::GithubPickerState),
    Dismiss,
}

pub trait ListModalState {
    fn open_container_info_modal(
        &mut self,
        state: crate::tui::components::container_info_surface::ContainerInfoState,
    );
    fn open_error_popup_modal(&mut self, state: crate::tui::components::ErrorPopupState);
    fn open_github_picker_modal(
        &mut self,
        state: crate::tui::components::github_picker::GithubPickerState,
    );
    fn dismiss_list_modal(&mut self);
}

pub fn apply_list_modal_plan(state: &mut impl ListModalState, plan: ListModalPlan) {
    match plan {
        ListModalPlan::ContainerInfo(info) => state.open_container_info_modal(info),
        ListModalPlan::ErrorPopup(error) => state.open_error_popup_modal(error),
        ListModalPlan::GithubPicker(picker) => state.open_github_picker_modal(picker),
        ListModalPlan::Dismiss => state.dismiss_list_modal(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlinePickerDismissal {
    NewSession,
    Role,
    Agent,
    Provider,
    LaunchAccount,
}

pub trait InlinePickerDismissalState {
    fn clear_inline_new_session_picker(&mut self);
    fn clear_inline_role_picker(&mut self);
    fn clear_inline_agent_picker(&mut self);
    fn clear_inline_account_picker(&mut self);
    fn clear_launch_account_picker(&mut self);
}

pub fn apply_inline_picker_dismissal_plan(
    state: &mut impl InlinePickerDismissalState,
    plan: InlinePickerDismissal,
) {
    match plan {
        InlinePickerDismissal::NewSession => state.clear_inline_new_session_picker(),
        InlinePickerDismissal::Role => state.clear_inline_role_picker(),
        InlinePickerDismissal::Agent => state.clear_inline_agent_picker(),
        InlinePickerDismissal::Provider => state.clear_inline_account_picker(),
        InlinePickerDismissal::LaunchAccount => state.clear_launch_account_picker(),
    }
}
