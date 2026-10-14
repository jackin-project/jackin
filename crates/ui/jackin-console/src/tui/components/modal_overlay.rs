// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;
use termrock::interaction::{
    BackdropPolicy, DismissAction, DismissPolicy, LayerDismissPolicy, NarrowFallback, OverlayId,
    OverlayKind, OverlayPolicy, OverlaySize, OverlaySpec, OverlayStack, PlacementPrefer,
};

use crate::tui::components::github_picker::GithubPickerState;
use crate::tui::components::op_picker::OpPickerRenderState;
use crate::tui::components::role_picker::{RoleChoice, RolePickerState};
use crate::tui::components::{auth_panel, confirm_save};

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

pub const fn console_modal_policy(escape: DismissAction) -> OverlayPolicy {
    OverlayPolicy {
        esc: escape.to_layer(),

        outside: LayerDismissPolicy::Trap,
        owns_input: true,
        focus_trap: true,
        wheel_captures: true,
        backdrop: BackdropPolicy::Dim,
        prefer: PlacementPrefer::Center,
        cover_anchor: false,
        narrow_fallback: NarrowFallback::Clamp,

        narrow_cols: 0,
    }
}

pub const fn console_modal_dismiss_policy(escape: DismissAction) -> DismissPolicy {
    DismissPolicy {
        escape,
        outside: DismissAction::Trap,
        focus_leave: DismissAction::Trap,
        parent_closed: DismissAction::Dismiss,
        explicit: DismissAction::Dismiss,
    }
}

#[must_use]
pub fn fixed_dialog_size(outer: Rect, width_pct: u16, rows: u16) -> OverlaySize {
    const REFERENCE_COLS: u16 = 160;
    let preferred = REFERENCE_COLS.saturating_mul(width_pct) / 100;
    exact_dialog_size(outer, preferred.min(outer.width.saturating_sub(4)), rows)
}

#[must_use]
pub fn exact_dialog_size(outer: Rect, width: u16, height: u16) -> OverlaySize {
    OverlaySize {
        width: width.min(outer.width),
        height: height.min(outer.height),
        min_width: 0,
        min_height: 0,
        max_width: 0,
        max_height: 0,
    }
}

#[must_use]
pub fn modal_overlay_rect(
    outer: Rect,
    kind: OverlayKind,
    size: OverlaySize,
    escape: DismissAction,
) -> Rect {
    let mut stack = OverlayStack::<()>::new();
    drop(stack.open(
        outer,
        OverlaySpec {
            id: OverlayId::from_static("console-modal"),
            kind,
            parent: None,
            anchor: None,
            size,
            opener_focus: None,
            policy: Some(console_modal_policy(escape)),
        },
    ));
    stack.top().map_or_else(Rect::default, |entry| entry.rect)
}

#[must_use]
pub fn file_browser_overlay_rect(outer: Rect) -> Rect {
    modal_overlay_rect(
        outer,
        OverlayKind::Dialog,
        fixed_dialog_size(outer, 70, 22),
        DismissAction::Bubble,
    )
}

#[must_use]
pub fn role_picker_overlay_rect(outer: Rect, filtered_len: usize) -> Rect {
    let rows = (filtered_len as u16).saturating_add(6).min(15);
    modal_overlay_rect(
        outer,
        OverlayKind::Dialog,
        fixed_dialog_size(outer, 50, rows),
        DismissAction::Dismiss,
    )
}

#[must_use]
pub fn op_picker_overlay_rect(outer: Rect) -> Rect {
    modal_overlay_rect(
        outer,
        OverlayKind::Dialog,
        fixed_dialog_size(outer, 80, 22),
        DismissAction::Bubble,
    )
}
