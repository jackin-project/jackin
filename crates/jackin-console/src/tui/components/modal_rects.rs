// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared modal size and placement helpers.

use ratatui::layout::Rect;

use crate::tui::components::github_picker::GithubPickerState;
use crate::tui::components::op_picker::OpPickerRenderState;
use crate::tui::components::role_picker::{RoleChoice, RolePickerState};
use crate::tui::components::{auth_panel, confirm_save};
use crate::tui::layout::centered_rect_fixed;

pub trait ModalRolePickerState {
    fn filtered_len(&self) -> usize;
}

impl<R: RoleChoice> ModalRolePickerState for RolePickerState<R> {
    fn filtered_len(&self) -> usize {
        self.filtered.len()
    }
}

pub trait ModalConfirmState {
    fn width_pct(&self) -> u16;
    fn required_height(&self) -> u16;
}

impl ModalConfirmState for jackin_tui::components::ConfirmState {
    fn width_pct(&self) -> u16 {
        jackin_tui::components::confirm_width_pct(self)
    }

    fn required_height(&self) -> u16 {
        jackin_tui::components::confirm_required_height(self)
    }
}

pub trait ModalConfirmSaveState {
    fn required_height(&self) -> u16;
}

impl<M: Clone> ModalConfirmSaveState for confirm_save::ConfirmSaveState<M> {
    fn required_height(&self) -> u16 {
        confirm_save::required_height(self)
    }
}

pub trait ModalConfirmSavePrepareState {
    fn prepare_for_render(&mut self, area: Rect);
}

impl<M: Clone> ModalConfirmSavePrepareState for confirm_save::ConfirmSaveState<M> {
    fn prepare_for_render(&mut self, area: Rect) {
        confirm_save::prepare_for_render(area, self);
    }
}

pub trait ModalAuthFormState {
    fn required_height(&self) -> u16;
}

impl<V: auth_panel::AuthCredential> ModalAuthFormState for auth_panel::AuthForm<V> {
    fn required_height(&self) -> u16 {
        auth_panel::required_height(self)
    }
}

pub trait ModalOpPickerState {
    fn has_naming_stage_input(&self) -> bool;
}

impl<T: OpPickerRenderState> ModalOpPickerState for T {
    fn has_naming_stage_input(&self) -> bool {
        self.naming_stage_input().is_some()
    }
}

pub trait ModalGithubPickerState {
    fn choice_len(&self) -> usize;
}

impl ModalGithubPickerState for GithubPickerState {
    fn choice_len(&self) -> usize {
        self.choices.len()
    }
}

pub trait ModalErrorPopupState {
    fn required_height(&self, inner_width: u16, max_rows: u16) -> u16;
}

impl ModalErrorPopupState for jackin_tui::components::ErrorPopupState {
    fn required_height(&self, inner_width: u16, max_rows: u16) -> u16 {
        jackin_tui::components::error_dialog::required_height(self, inner_width, max_rows)
    }
}

pub trait ModalContainerInfoState {
    fn required_height(&self) -> u16;
}

impl ModalContainerInfoState for jackin_tui::components::ContainerInfoState {
    fn required_height(&self) -> u16 {
        jackin_tui::components::container_info_required_height(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalRectSpec {
    TextInput,
    SourcePicker,
    ScopePicker,
    OpPicker,
    RolePicker { filtered_len: usize },
    Confirm { width_pct: u16, height: u16 },
    MountChoice,
    AuthForm { required_height: u16 },
    Fixed { width_pct: u16, height: u16 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalRectMode {
    TextInput,
    SourcePicker,
    ScopePicker,
    OpPicker,
    RolePicker { filtered_len: usize },
    Confirm { width_pct: u16, height: u16 },
    MountChoice,
    AuthForm { required_height: u16 },
    SaveDiscardCancel,
    FileBrowser,
    WorkdirPick,
    GithubPicker { choice_len: usize },
    ConfirmSave { required_height: u16 },
    ErrorPopup { required_height: u16 },
    ContainerInfo { required_height: u16 },
    StatusPopup,
}

impl ModalRectMode {
    fn spec(self, outer_height: u16) -> ModalRectSpec {
        match self {
            Self::TextInput => ModalRectSpec::TextInput,
            Self::SourcePicker => ModalRectSpec::SourcePicker,
            Self::ScopePicker => ModalRectSpec::ScopePicker,
            Self::OpPicker => ModalRectSpec::OpPicker,
            Self::RolePicker { filtered_len } => ModalRectSpec::RolePicker { filtered_len },
            Self::Confirm { width_pct, height } => ModalRectSpec::Confirm { width_pct, height },
            Self::MountChoice => ModalRectSpec::MountChoice,
            Self::AuthForm { required_height } => ModalRectSpec::AuthForm { required_height },
            Self::SaveDiscardCancel => ModalRectSpec::Fixed {
                width_pct: 70,
                height: 7,
            },
            Self::FileBrowser => ModalRectSpec::Fixed {
                width_pct: 70,
                height: 22,
            },
            Self::WorkdirPick => ModalRectSpec::Fixed {
                width_pct: 60,
                height: 12,
            },
            Self::GithubPicker { choice_len } => {
                let rows = (choice_len as u16).saturating_add(5).min(15);
                ModalRectSpec::Fixed {
                    width_pct: 60,
                    height: rows,
                }
            }
            Self::ConfirmSave { required_height } => ModalRectSpec::Fixed {
                width_pct: 80,
                height: required_height.min(outer_height),
            },
            Self::ErrorPopup { required_height } => ModalRectSpec::Fixed {
                width_pct: 60,
                height: required_height,
            },
            Self::ContainerInfo { required_height } => ModalRectSpec::Fixed {
                width_pct: 60,
                height: required_height,
            },
            Self::StatusPopup => ModalRectSpec::Fixed {
                width_pct: 50,
                height: 7,
            },
        }
    }
}

#[must_use]
pub fn modal_rect_for_mode(outer: Rect, mode: ModalRectMode) -> Rect {
    modal_rect(outer, mode.spec(outer.height))
}

#[must_use]
pub fn modal_rect(outer: Rect, spec: ModalRectSpec) -> Rect {
    // Structural exception: this console registry maps modal kinds to shared centered rects until the registry can live in `jackin-tui`.
    match spec {
        ModalRectSpec::TextInput => text_input_rect(outer),
        ModalRectSpec::SourcePicker => source_picker_rect(outer),
        ModalRectSpec::ScopePicker => scope_picker_rect(outer),
        ModalRectSpec::OpPicker => op_picker_rect(outer),
        ModalRectSpec::RolePicker { filtered_len } => {
            role_picker_rect_for_count(outer, filtered_len)
        }
        ModalRectSpec::Confirm { width_pct, height } => {
            centered_rect_fixed(outer, width_pct, height)
        }
        ModalRectSpec::MountChoice => mount_choice_rect(outer),
        ModalRectSpec::AuthForm { required_height } => {
            auth_form_rect_for_height(outer, required_height)
        }
        ModalRectSpec::Fixed { width_pct, height } => centered_rect_fixed(outer, width_pct, height),
    }
}

#[must_use]
pub fn text_input_rect(outer: Rect) -> Rect {
    // Structural exception: console mode-specific sizing data feeds the shared centered-rect primitive.
    centered_rect_fixed(outer, 60, 5)
}

#[must_use]
pub fn source_picker_rect(outer: Rect) -> Rect {
    // Structural exception: console mode-specific sizing data feeds the shared centered-rect primitive.
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn scope_picker_rect(outer: Rect) -> Rect {
    // Structural exception: console mode-specific sizing data feeds the shared centered-rect primitive.
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn op_picker_rect(outer: Rect) -> Rect {
    // Structural exception: console mode-specific sizing data feeds the shared centered-rect primitive.
    centered_rect_fixed(outer, 80, 22)
}

#[must_use]
pub fn role_picker_rect_for_count(outer: Rect, filtered_len: usize) -> Rect {
    // Structural exception: role picker height depends on filtered console state; placement still uses shared centering.
    let rows = (filtered_len as u16).saturating_add(6).min(15);
    centered_rect_fixed(outer, 50, rows)
}

#[must_use]
pub fn confirm_rect(outer: Rect, state: &jackin_tui::components::ConfirmState) -> Rect {
    // Structural exception: confirm sizing is owned by shared confirm state; this adapter keeps console modal routing centralized.
    centered_rect_fixed(
        outer,
        jackin_tui::components::confirm_width_pct(state),
        jackin_tui::components::confirm_required_height(state),
    )
}

#[must_use]
pub fn mount_choice_rect(outer: Rect) -> Rect {
    // Structural exception: mount-choice content has a fixed row contract; placement mirrors shared centered-rect behavior.
    // 2 borders + 1 leading + 1 question + 1 path + 1 spacer + 1 buttons + 1 trailing = 8
    let w = outer.width.min(80);
    let h = 8u16.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[must_use]
pub fn auth_form_rect_for_height(outer: Rect, required_height: u16) -> Rect {
    // Structural exception: auth-panel height is computed by its form state; placement still uses shared centering.
    centered_rect_fixed(outer, 80, required_height)
}
