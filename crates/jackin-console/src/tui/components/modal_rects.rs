//! Shared modal size and placement helpers.

use ratatui::layout::Rect;

use crate::tui::components::github_picker::GithubPickerState;
use crate::tui::components::op_picker::OpPickerRenderState;
use crate::tui::components::role_picker::{RoleChoice, RolePickerState};
use crate::tui::components::{auth_panel, confirm_save};

pub use jackin_tui::components::modal_rects::*;

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
