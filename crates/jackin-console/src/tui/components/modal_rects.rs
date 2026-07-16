//! Shared modal size and placement registry.
//!
//! Surfaces pass the area that is allowed to be covered by the modal. The
//! registry centers within that area unless a spec explicitly says otherwise,
//! so callers keep owning footer/status reservation while modal sizing stays in
//! one place.

use ratatui::layout::Rect;

use crate::tui::components::github_picker::GithubPickerState;
use crate::tui::components::op_picker::OpPickerRenderState;
use crate::tui::components::role_picker::{RoleChoice, RolePickerState};
use crate::tui::components::{auth_panel, confirm_save};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalRectSpec {
    TextInput,
    SourcePicker,
    ScopePicker,
    OpPicker,
    RolePicker {
        filtered_len: usize,
    },
    Confirm {
        width_pct: u16,
        height: u16,
    },
    MountChoice,
    AuthForm {
        required_height: u16,
    },
    Fixed {
        width_pct: u16,
        height: u16,
    },
    Exact {
        width: u16,
        height: u16,
    },
    MaxWidthMin {
        max_width: u16,
        min_width: u16,
        side_margin: u16,
        height: u16,
    },
    PercentClamp {
        width_pct: u16,
        min_width: u16,
        side_margin: u16,
        height: u16,
    },
    PercentClampWithMargin {
        width_pct: u16,
        min_width: u16,
        width_margin: u16,
        height_margin: u16,
        height: u16,
    },
    TopAligned {
        width: u16,
        height: u16,
    },
    TopAlignedMaxWidthMin {
        max_width: u16,
        min_width: u16,
        side_margin: u16,
        height: u16,
    },
}
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

impl ModalConfirmState for crate::tui::components::ConfirmState {
    fn width_pct(&self) -> u16 {
        self.width_pct()
    }

    fn required_height(&self) -> u16 {
        self.required_height()
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

impl ModalErrorPopupState for crate::tui::components::ErrorPopupState {
    fn required_height(&self, inner_width: u16, max_rows: u16) -> u16 {
        self.required_height(inner_width, max_rows)
    }
}

pub trait ModalContainerInfoState {
    fn required_height(&self) -> u16;
}

impl ModalContainerInfoState
    for crate::tui::components::container_info_surface::ContainerInfoState
{
    fn required_height(&self) -> u16 {
        crate::tui::components::container_info_surface::required_height(self)
    }
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
            Self::ErrorPopup { required_height } | Self::ContainerInfo { required_height } => {
                ModalRectSpec::Fixed {
                    width_pct: 60,
                    height: required_height,
                }
            }
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
        ModalRectSpec::Exact { width, height } => centered_rect_exact(outer, width, height),
        ModalRectSpec::MaxWidthMin {
            max_width,
            min_width,
            side_margin,
            height,
        } => {
            let width = max_width
                .min(outer.width.saturating_sub(side_margin))
                .max(min_width);
            centered_rect_exact(outer, width, height)
        }
        ModalRectSpec::PercentClamp {
            width_pct,
            min_width,
            side_margin,
            height,
        } => {
            let max_width = outer.width.saturating_sub(side_margin).max(min_width);
            let width = (outer.width.saturating_mul(width_pct) / 100).clamp(min_width, max_width);
            centered_rect_exact(outer, width, height)
        }
        ModalRectSpec::PercentClampWithMargin {
            width_pct,
            min_width,
            width_margin,
            height_margin,
            height,
        } => {
            let max_width = outer.width.saturating_sub(width_margin).max(min_width);
            let width = (outer.width.saturating_mul(width_pct) / 100).clamp(min_width, max_width);
            let height = height.min(outer.height.saturating_sub(height_margin));
            centered_rect_exact(outer, width, height)
        }
        ModalRectSpec::TopAligned { width, height } => {
            resolve_exact(outer, width, height, termrock::layout::Placement::Top)
        }
        ModalRectSpec::TopAlignedMaxWidthMin {
            max_width,
            min_width,
            side_margin,
            height,
        } => {
            let width = max_width
                .min(outer.width.saturating_sub(side_margin))
                .max(min_width);
            resolve_exact(outer, width, height, termrock::layout::Placement::Top)
        }
    }
}

#[must_use]
pub fn text_input_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 60, 5)
}

#[must_use]
pub fn source_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn scope_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn op_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 80, 22)
}

#[must_use]
pub fn role_picker_rect_for_count(outer: Rect, filtered_len: usize) -> Rect {
    let rows = (filtered_len as u16).saturating_add(6).min(15);
    centered_rect_fixed(outer, 50, rows)
}

#[must_use]
pub fn confirm_rect(outer: Rect, state: &crate::tui::components::ConfirmState) -> Rect {
    centered_rect_fixed(outer, state.width_pct(), state.required_height())
}

#[must_use]
pub fn mount_choice_rect(outer: Rect) -> Rect {
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
    centered_rect_fixed(outer, 80, required_height)
}

/// Center a dialog at a stable preferred width derived from `pct_w` of a
/// 160-column reference terminal.
#[must_use]
pub fn centered_rect_fixed(outer: Rect, pct_w: u16, rows: u16) -> Rect {
    const REFERENCE_COLS: u16 = 160;
    let preferred = REFERENCE_COLS.saturating_mul(pct_w) / 100;
    centered_rect_preferred(outer, preferred, rows)
}

/// Center a dialog at `preferred_w` columns, shrinking only when the outer area
/// is too narrow to fit `preferred_w` with a four-column side margin.
#[must_use]
pub fn centered_rect_preferred(outer: Rect, preferred_w: u16, rows: u16) -> Rect {
    let w = preferred_w.min(outer.width.saturating_sub(4));
    let h = rows.min(outer.height);
    centered_rect_exact(outer, w, h)
}

#[must_use]
pub fn centered_rect_exact(outer: Rect, width: u16, height: u16) -> Rect {
    resolve_exact(outer, width, height, termrock::layout::Placement::Centered)
}

fn resolve_exact(
    outer: Rect,
    width: u16,
    height: u16,
    placement: termrock::layout::Placement,
) -> Rect {
    termrock::layout::resolve_dialog(
        outer,
        termrock::layout::DialogSpec {
            min_width: width,
            preferred_width: width,
            max_width: width,
            min_height: 0,
            preferred_height: height,
            max_height: height,
            horizontal_margin: 0,
            vertical_margin: 0,
            placement,
        },
    )
}
