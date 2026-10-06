// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Display-side impls on `ConsoleModal`: `overlay_kind` / `dismiss_policy`
//! / `overlay_size` / `rect` / `container_info_rect` /
//! `prepare_for_render` / `footer_items` / `footer_items_for_area`, plus
//! the `footer_items_for_mode` helper.
//!
//! Moved out of `model/modal.rs` during the Ledger 2B decomposition so
//! the modal enum stays a thin coordinator and the per-trait dispatch
//! lives next to the trait it implements.

use ratatui::layout::Rect;
use termrock::interaction::{DismissAction, DismissPolicy, OverlayKind, OverlaySize};

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_overlay::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
    ModalRolePickerState,
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
    /// Overlay kind carried on the stack entry: the confirm-class blockers
    /// are alert dialogs; everything else is a plain dialog. Behavior comes
    /// from the explicit policy, not the kind's preset.
    #[must_use]
    pub const fn overlay_kind(&self) -> OverlayKind {
        match self {
            Self::Confirm { .. } | Self::SaveDiscardCancel { .. } => OverlayKind::AlertDialog,
            _ => OverlayKind::Dialog,
        }
    }

    /// Per-variant dismiss policy from CURRENT behavior (plan 009 step 4
    /// mapping): Esc cancels every variant outright except the file browser
    /// and op-picker, whose components spend Esc on internal
    /// back-navigation first; outside clicks are inert for all variants.
    #[must_use]
    pub const fn dismiss_policy(&self) -> DismissPolicy {
        let escape = match self {
            Self::FileBrowser { .. } | Self::OpPicker { .. } => DismissAction::Bubble,
            _ => DismissAction::Dismiss,
        };
        crate::tui::components::modal_overlay::console_modal_dismiss_policy(escape)
    }

    /// Preferred overlay size: the retired `ModalRectMode` → spec → rect-fn
    /// chain's numbers, kept byte-identical per variant.
    #[must_use]
    pub fn overlay_size(&self, outer: Rect) -> OverlaySize
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
        use crate::tui::components::modal_overlay::{exact_dialog_size, fixed_dialog_size};
        match self {
            Self::TextInput { .. } => fixed_dialog_size(outer, 60, 5),
            Self::Confirm { state, .. } => {
                fixed_dialog_size(outer, state.width_pct(), state.required_height())
            }
            Self::SaveDiscardCancel { .. } => fixed_dialog_size(outer, 70, 7),
            Self::FileBrowser { .. } => fixed_dialog_size(outer, 70, 22),
            Self::WorkdirPick { .. } => fixed_dialog_size(outer, 60, 12),
            Self::MountDstChoice { .. } => exact_dialog_size(outer, 80, 8),
            Self::GithubPicker { state } => {
                let rows = (state.choice_len() as u16).saturating_add(5).min(15);
                fixed_dialog_size(outer, 60, rows)
            }
            Self::ConfirmSave { state } => {
                fixed_dialog_size(outer, 80, state.required_height().min(outer.height))
            }
            Self::ErrorPopup { state } => {
                let inner_width = (outer.width * 60 / 100).saturating_sub(4);
                let max_rows = outer.height.saturating_sub(2);
                fixed_dialog_size(outer, 60, state.required_height(inner_width, max_rows))
            }
            Self::ContainerInfo { state } => fixed_dialog_size(outer, 60, state.required_height()),
            Self::StatusPopup { .. } => fixed_dialog_size(outer, 50, 7),
            Self::OpPicker { state, .. } if state.has_naming_stage_input() => {
                fixed_dialog_size(outer, 60, 5)
            }
            Self::OpPicker { .. } => fixed_dialog_size(outer, 80, 22),
            Self::RolePicker { state } | Self::RoleOverridePicker { state } => {
                let rows = (state.filtered_len() as u16).saturating_add(6).min(15);
                fixed_dialog_size(outer, 50, rows)
            }
            Self::SourcePicker { .. } | Self::AuthSourcePicker { .. } => {
                fixed_dialog_size(outer, 50, 5)
            }
            Self::ScopePicker { .. } => fixed_dialog_size(outer, 50, 5),
            Self::AuthForm { state, .. } => fixed_dialog_size(outer, 80, state.required_height()),
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
        crate::tui::components::modal_overlay::modal_overlay_rect(
            outer,
            self.overlay_kind(),
            self.overlay_size(outer),
            self.dismiss_policy().escape,
        )
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
    pub fn footer_items(
        &self,
        can_generate_token: bool,
    ) -> Vec<termrock::widgets::HintSpan<'static>>
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
            Self::OpPicker { state, .. } => footer_items_for_mode(state.footer_mode(true)),
            Self::RolePicker { .. } | Self::RoleOverridePicker { .. } => {
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
    ) -> Vec<termrock::widgets::HintSpan<'static>>
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

fn footer_items_for_mode(mode: ModalFooterMode) -> Vec<termrock::widgets::HintSpan<'static>> {
    crate::tui::components::footer_hints::modal_footer_items(mode)
}
