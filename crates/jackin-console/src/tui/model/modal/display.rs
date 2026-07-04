// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Display-side impls on `ConsoleModal`: `rect_mode` / `rect` /
//! `container_info_rect` / `prepare_for_render` / `footer_items` /
//! `footer_items_for_area`, plus the `footer_items_for_mode` helper.
//!
//! Moved out of `model/modal.rs` during the Ledger 2B decomposition so
//! the modal enum stays a thin coordinator and the per-trait dispatch
//! lives next to the trait it implements.

use ratatui::layout::Rect;

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
    ModalRectMode, ModalRolePickerState,
};

use super::ConsoleModal;
impl<
    TextInputTarget,
    TextInputState,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState,
    ErrorPopupState,
    ContainerInfoState,
    StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
>
    ConsoleModal<
        TextInputTarget,
        TextInputState,
        FileBrowserTarget,
        FileBrowserState,
        MountDstChoiceState,
        WorkdirPickState,
        ConfirmTarget,
        ConfirmState,
        SaveDiscardState,
        GithubPickerState,
        ConfirmSaveState,
        ErrorPopupState,
        ContainerInfoState,
        StatusPopupState,
        OpPickerState,
        RolePickerState,
        SourcePickerState,
        ScopePickerState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
        SecretsScopeTag,
    >
{
    #[must_use]
    pub fn rect_mode(&self, outer: Rect) -> ModalRectMode
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        match self {
            Self::TextInput { .. } => ModalRectMode::TextInput,
            Self::Confirm { state, .. } => ModalRectMode::Confirm {
                width_pct: state.width_pct(),
                height: state.required_height(),
            },
            Self::SaveDiscardCancel { .. } => ModalRectMode::SaveDiscardCancel,
            Self::FileBrowser { .. } => ModalRectMode::FileBrowser,
            Self::WorkdirPick { .. } => ModalRectMode::WorkdirPick,
            Self::MountDstChoice { .. } => ModalRectMode::MountChoice,
            Self::GithubPicker { state } => ModalRectMode::GithubPicker {
                choice_len: state.choice_len(),
            },
            Self::ConfirmSave { state } => ModalRectMode::ConfirmSave {
                required_height: state.required_height(),
            },
            Self::ErrorPopup { state } => {
                let inner_width = (outer.width * 60 / 100).saturating_sub(4);
                let max_rows = outer.height.saturating_sub(2);
                ModalRectMode::ErrorPopup {
                    required_height: state.required_height(inner_width, max_rows),
                }
            }
            Self::ContainerInfo { state } => ModalRectMode::ContainerInfo {
                required_height: state.required_height(),
            },
            Self::StatusPopup { .. } => ModalRectMode::StatusPopup,
            Self::OpPicker { state } if state.has_naming_stage_input() => ModalRectMode::TextInput,
            Self::OpPicker { .. } => ModalRectMode::OpPicker,
            Self::RolePicker { state }
            | Self::RoleOverridePicker { state }
            | Self::AuthRolePicker { state } => ModalRectMode::RolePicker {
                filtered_len: state.filtered_len(),
            },
            Self::SourcePicker { .. } | Self::AuthSourcePicker { .. } => {
                ModalRectMode::SourcePicker
            }
            Self::ScopePicker { .. } => ModalRectMode::ScopePicker,
            Self::AuthForm { state, .. } => ModalRectMode::AuthForm {
                required_height: state.required_height(),
            },
        }
    }

    #[must_use]
    pub fn rect(&self, outer: Rect) -> Rect
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        crate::tui::components::modal_rects::modal_rect_for_mode(outer, self.rect_mode(outer))
    }

    #[must_use]
    pub fn container_info_rect(&self, outer: Rect) -> Option<Rect>
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        if matches!(self, Self::ContainerInfo { .. }) {
            Some(self.rect(outer))
        } else {
            None
        }
    }

    pub fn prepare_for_render(&mut self, outer: Rect)
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState + ModalConfirmSavePrepareState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        let modal_area = self.rect(outer);
        if let Self::ConfirmSave { state } = self {
            state.prepare_for_render(modal_area);
        }
    }

    #[must_use]
    pub fn footer_items(&self, can_generate_token: bool) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
    {
        match self {
            Self::AuthForm { state, focus, .. } => {
                crate::tui::components::footer_hints::modal_footer_items(
                    state.footer_mode(*focus, can_generate_token),
                )
            }
            Self::FileBrowser { state, .. } => state.footer_items(),
            Self::TextInput { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::MountDstChoice { .. } => footer_items_for_mode(ModalFooterMode::MountDestination),
            Self::SourcePicker { .. }
            | Self::AuthSourcePicker { .. }
            | Self::ScopePicker { .. } => footer_items_for_mode(ModalFooterMode::SegmentedChoice),
            Self::WorkdirPick { .. } => footer_items_for_mode(ModalFooterMode::PickList {
                commit_label: crate::tui::components::footer_hints::pick_list_select_footer_label(),
            }),
            Self::GithubPicker { .. } => footer_items_for_mode(ModalFooterMode::PickList {
                commit_label: crate::tui::components::footer_hints::pick_list_confirm_footer_label(
                ),
            }),
            Self::ConfirmSave { state } => footer_items_for_mode(state.footer_mode()),
            Self::SaveDiscardCancel { .. } => {
                footer_items_for_mode(ModalFooterMode::SaveDiscardCancel)
            }
            Self::ErrorPopup { .. } => footer_items_for_mode(ModalFooterMode::ErrorPopup),
            Self::ContainerInfo { .. } => footer_items_for_mode(ModalFooterMode::ContainerInfo),
            Self::StatusPopup { .. } => footer_items_for_mode(ModalFooterMode::StatusPopup),
            Self::OpPicker { state } => footer_items_for_mode(state.footer_mode(true)),
            Self::RolePicker { .. }
            | Self::RoleOverridePicker { .. }
            | Self::AuthRolePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::FilteredPicker {
                    include_refresh: false,
                    include_collapse: false,
                })
            }
            Self::Confirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
        }
    }

    #[must_use]
    pub fn footer_items_for_area(
        &self,
        can_generate_token: bool,
        outer: Rect,
    ) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState + ModalContainerInfoFooterState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
        OpPickerState: ModalOpPickerState,
    {
        if let Self::ContainerInfo { state } = self {
            return crate::tui::components::footer_hints::container_info_footer_items_for_dialog(
                state.content_width(),
                state.content_height(),
                self.rect(outer),
            );
        }
        self.footer_items(can_generate_token)
    }
}

fn footer_items_for_mode(mode: ModalFooterMode) -> Vec<jackin_tui::HintSpan<'static>> {
    crate::tui::components::footer_hints::modal_footer_items(mode)
}
