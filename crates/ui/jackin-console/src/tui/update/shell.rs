// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Shell, selection, and overlay plans.

use super::{InlinePickerDismissal, ListModalPlan, StatusOverlayPlan};

#[must_use]
pub const fn drag_state_plan(
    drag: Option<crate::tui::split::DragState>,
) -> Option<crate::tui::split::DragState> {
    drag
}

#[must_use]
pub const fn list_split_pct_plan(pct: u16) -> u16 {
    crate::tui::split::clamp_split(pct)
}

pub trait ListShellState {
    fn set_drag_state(&mut self, drag: Option<crate::tui::split::DragState>);
    fn set_list_split_pct(&mut self, pct: u16);
}

pub fn apply_drag_state_plan(
    state: &mut impl ListShellState,
    plan: Option<crate::tui::split::DragState>,
) {
    state.set_drag_state(plan);
}

pub fn apply_list_split_pct_plan(state: &mut impl ListShellState, plan: u16) {
    state.set_list_split_pct(plan);
}

#[must_use]
pub fn selection_move_plan(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::tui::focus::collection_move_index(selected, row_count, delta)
}

#[must_use]
pub fn selected_index_plan(selected: usize, row_count: usize) -> usize {
    crate::tui::focus::selected_index(selected, row_count)
}

#[must_use]
pub const fn unclamped_scroll_plan(current_scroll: u16, delta: i16) -> u16 {
    let mut scroll = current_scroll;
    termrock::scroll::apply_scroll_delta_unclamped(&mut scroll, delta);
    scroll
}

#[must_use]
pub fn term_width_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> u16 {
    let mut scroll_x = current_scroll_x;
    termrock::scroll::apply_term_width_scroll_delta(
        &mut scroll_x,
        delta,
        term_width,
        content_width,
    );
    scroll_x
}

#[must_use]
pub fn open_status_overlay_plan(
    title: impl Into<String>,
    message: impl Into<String>,
) -> StatusOverlayPlan {
    StatusOverlayPlan::Open(crate::tui::components::status_popup::status_popup_state(
        title, message,
    ))
}

#[must_use]
pub fn role_resolution_status_overlay_plan(role_key: impl std::fmt::Display) -> StatusOverlayPlan {
    StatusOverlayPlan::Open(
        crate::tui::components::status_popup::role_resolution_status_popup_state(role_key),
    )
}

#[must_use]
pub const fn dismiss_status_overlay_plan() -> StatusOverlayPlan {
    StatusOverlayPlan::Dismiss
}

#[must_use]
pub fn open_container_info_modal_plan(
    state: crate::tui::components::container_info_surface::ContainerInfoState,
) -> ListModalPlan {
    ListModalPlan::ContainerInfo(state)
}

#[must_use]
pub fn open_error_popup_modal_plan(
    title: impl Into<String>,
    message: impl Into<String>,
) -> ListModalPlan {
    ListModalPlan::ErrorPopup(crate::tui::components::error_popup::error_popup_state(
        title, message,
    ))
}

#[must_use]
pub fn open_github_picker_modal_plan(
    state: crate::tui::components::github_picker::GithubPickerState,
) -> ListModalPlan {
    ListModalPlan::GithubPicker(state)
}

#[must_use]
pub const fn dismiss_list_modal_plan() -> ListModalPlan {
    ListModalPlan::Dismiss
}

#[must_use]
pub const fn inline_picker_dismissal_plan(kind: InlinePickerDismissal) -> InlinePickerDismissal {
    kind
}
