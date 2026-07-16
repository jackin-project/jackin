// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mouse event handling for the workspace manager: list/details seam drag,
//! click-to-select in the list pane, and `FileBrowser` URL-click fallthrough.
//!
//! Coordinator — declares the sibling modules under `input/mouse/` and
//! re-exports every public symbol so external callers keep their
//! existing `use crate::tui::input::mouse::*` paths.

mod hover;
mod modal_scroll;
mod scroll_bars;
mod scroll_pan;
mod selection;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::modal_rects::{self, ModalRectMode};
use crate::tui::layout::{
    LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT, MIN_DRAGGABLE_WIDTH, MOUSE_VERTICAL_SCROLL_STEP,
    SCREEN_HEADER_HEIGHT, ScrollbarAxis, TAB_STRIP_HEIGHT, apply_horizontal_scroll,
    apply_scrollbar_drag, apply_vertical_scroll, horizontal_split_pane_dims,
    is_horizontally_scrollable, point_in_rect, scroll_selection_at_position, scroll_viewport_width,
    split_seam_column,
};
use crate::tui::run::{ConsoleClickStageFacts, ConsoleClickabilityFacts, console_clickable_at};
use crate::tui::screens::editor::update::{
    editor_auth_row_index_at_position, editor_mount_hover_target_at_position,
    editor_mount_index_at_position, editor_scroll_focus_plan, editor_tab_at_position,
    editor_tab_bar_focus_plan, editor_tab_hover_target_plan,
};
use crate::tui::screens::settings::update::{
    settings_modal_open as settings_modal_open_fact, settings_scroll_focus_plan,
    settings_tab_at_position, settings_tab_bar_focus_plan, settings_tab_hover_target_plan,
    settings_trust_clickable_at_position, settings_trust_hover_target_at_position,
    settings_trust_row_at_position,
};
use crate::tui::screens::workspaces::update::{
    WorkspaceListMousePlan, apply_workspace_list_hover_target,
    workspace_list_clickable_at_position, workspace_list_hover_row_at_position,
    workspace_list_mouse_plan, workspace_list_scroll_focus_plan,
};
use crate::tui::state::ManagerEffect;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    EditorTab, ManagerListRow, ManagerStage, ManagerState, Modal, MountScrollFocus, SettingsModal,
    SettingsTab,
};
use crate::tui::update::{
    ConsoleMouseWheelPlan, ListModalScrollTarget, SettingsModalScrollTarget,
    SharedModalScrollTarget, console_mouse_wheel_plan,
};

pub use hover::{
    container_info_copyable_row_at, file_browser_modal_and_state, file_browser_url_row_at,
    list_row_hover_at, try_copy_container_info_value, update_container_info_hover,
    update_list_row_hover, update_row_hover,
};
pub use modal_scroll::{
    scroll_file_browser_state_at, scroll_global_mount_modal_selection, scroll_list_modal_selection,
    scroll_modal_selection, scroll_settings_auth_modal_selection,
    scroll_settings_env_modal_selection, try_scroll_file_browser_modal, try_scroll_picker_modal,
};
pub use scroll_bars::{try_drag_horizontal_scrollbar, try_drag_vertical_scrollbar};
pub use scroll_pan::{
    scroll_active_panel, scroll_active_panel_vertical, settings_modal_open, update_scroll_focus,
};
pub use selection::{
    editor_auth_row_index_at, editor_mount_index_at, try_select_editor_auth_row,
    try_select_editor_mount_row, try_select_editor_tab, try_select_settings_tab,
    try_select_settings_trust_row, update_tab_hover,
};

#[cfg(test)]
mod tests;

use crate::tui::layout::list::SidebarScrollAreas;

#[cfg(test)]
use crate::tui::mount_display::global_config_mounts_content_width as global_mounts_content_width;
#[cfg(test)]
use crate::tui::mount_display::workspace_config_mounts_content_width as workspace_mounts_content_width;
#[cfg(test)]
use termrock::scroll::max_offset_u16 as max_scroll_offset;

/// Dispatch a mouse event into the workspace manager's list view. Drives
/// the mouse-draggable seam between the list pane and the details pane.
#[cfg(test)]
fn handle_mouse(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> super::InputOutcome {
    handle_mouse_with_config(state, mouse, term_size, None)
}

pub fn handle_mouse_with_config(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> super::InputOutcome {
    if term_size.width < MIN_DRAGGABLE_WIDTH {
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Moved) {
        update_tab_hover(state, mouse);
        update_list_row_hover(state, mouse, term_size);
        update_row_hover(state, mouse, term_size);
        update_container_info_hover(state, mouse, term_size);
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_copy_container_info_value(state, mouse, term_size)
    {
        return super::InputOutcome::Continue;
    }

    let container_info_rect = state
        .list_modal
        .as_ref()
        .and_then(|modal| modal.container_info_rect(term_size));
    if let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut()
        && let Some(rect) = container_info_rect
        && info.scroll.on_mouse_scroll_for_axes(
            mouse.kind.into(),
            mouse.modifiers.into(),
            termrock::components::dialog_scroll_axes(
                info.content_width(),
                info.content_height(),
                rect,
            ),
        )
    {
        info.clamp_scroll(rect);
        return super::InputOutcome::Continue;
    }

    if try_scroll_picker_modal(state, mouse, term_size) {
        return super::InputOutcome::Continue;
    }

    if try_scroll_file_browser_modal(state, mouse, term_size) {
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_editor_tab(state, mouse)
    {
        return super::InputOutcome::Continue;
    }
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_settings_tab(state, mouse)
    {
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        update_scroll_focus(state, mouse, term_size, config);
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            if try_drag_horizontal_scrollbar(state, mouse, term_size, config) =>
        {
            return super::InputOutcome::Continue;
        }
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            if try_drag_vertical_scrollbar(state, mouse, term_size, config) =>
        {
            return super::InputOutcome::Continue;
        }
        kind @ (MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight
        | MouseEventKind::ScrollUp
        | MouseEventKind::ScrollDown) => {
            match console_mouse_wheel_plan(kind, mouse.modifiers) {
                ConsoleMouseWheelPlan::Horizontal {
                    delta,
                    vertical_fallback,
                } => {
                    if !scroll_active_panel(state, mouse, term_size, config, delta)
                        && let Some(fallback) = vertical_fallback
                    {
                        scroll_active_panel_vertical(state, mouse, term_size, config, fallback);
                    }
                }
                ConsoleMouseWheelPlan::Vertical(delta) => {
                    scroll_active_panel_vertical(state, mouse, term_size, config, delta);
                }
                ConsoleMouseWheelPlan::None => {}
            }
            return super::InputOutcome::Continue;
        }
        _ => {}
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_editor_mount_row(state, mouse, term_size)
    {
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_editor_auth_row(state, mouse, term_size, config)
    {
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_settings_trust_row(state, mouse, term_size)
    {
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_open_file_browser_git_url(state, mouse, term_size)
    {
        return super::InputOutcome::Continue;
    }

    if !matches!(state.stage, ManagerStage::List) {
        return super::InputOutcome::Continue;
    }
    if state.list_modal.is_some() {
        return super::InputOutcome::Continue;
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left)
        | MouseEventKind::Drag(MouseButton::Left)
        | MouseEventKind::Up(MouseButton::Left) => {
            match workspace_list_mouse_plan(
                mouse,
                term_size,
                state.list_split_pct,
                state.drag_state,
                state.list_modal.is_some(),
                state.visual_rows_vec().as_slice(),
                |row| state.index_of_row(row).is_some(),
            ) {
                WorkspaceListMousePlan::StartDrag(drag) => {
                    dispatch_manager(state, ManagerMessage::SetDragState(Some(drag)));
                }
                WorkspaceListMousePlan::UpdateSplit(pct) => {
                    dispatch_manager(state, ManagerMessage::SetListSplitPct(pct));
                }
                WorkspaceListMousePlan::EndDrag => {
                    dispatch_manager(state, ManagerMessage::SetDragState(None));
                }
                WorkspaceListMousePlan::SelectRow(row) => {
                    if let Some(selected) = state.index_of_row(row) {
                        dispatch_manager(state, ManagerMessage::SelectListRow(selected));
                    }
                }
                WorkspaceListMousePlan::Continue => {}
            }
        }
        _ => {}
    }
    super::InputOutcome::Continue
}

fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let _dirty = update_manager(state, message);
}

/// Whether a left-click at the pointer would act on a clickable element.
#[must_use]
pub fn clickable_at(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    let stage = match &state.stage {
        ManagerStage::Editor(editor) => ConsoleClickStageFacts::Editor {
            modal_open: editor.modal.is_some(),
            tab_target: editor_tab_at_position(mouse.row, mouse.column).is_some(),
            mount_row_target: editor_mount_index_at(editor, mouse, term_size).is_some(),
            auth_row_target: config
                .and_then(|cfg| editor_auth_row_index_at(editor, cfg, mouse, term_size))
                .is_some(),
        },
        ManagerStage::Settings(settings) => ConsoleClickStageFacts::Settings {
            mounts_modal_open: settings.mounts.modal.is_some(),
            env_modal_open: settings.env.modal.is_some(),
            tab_target: settings_tab_at_position(mouse.row, mouse.column).is_some(),
            trust_target: settings_trust_clickable_at_position(
                settings.active_tab,
                settings.mounts.modal.is_some(),
                settings.content_area(term_size),
                mouse.column,
                mouse.row,
            ),
        },
        ManagerStage::List => ConsoleClickStageFacts::List {
            list_modal_open: state.list_modal.is_some(),
            workspace_list_target: workspace_list_clickable_at_position(
                mouse.column,
                mouse.row,
                term_size,
                state.list_split_pct,
                state.list_modal.is_some(),
                state.visual_rows_vec().as_slice(),
                |row| state.index_of_row(row).is_some(),
            ),
        },
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => ConsoleClickStageFacts::Other,
    };

    console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: term_size.width >= MIN_DRAGGABLE_WIDTH,
        file_browser_url_target: file_browser_url_row_at(state, mouse, term_size),
        container_info_copy_target: container_info_copyable_row_at(state, mouse, term_size),
        stage,
    })
}

fn try_open_file_browser_git_url(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let Some((modal, fb_state)) = file_browser_modal_and_state(state) else {
        return false;
    };
    let modal_area = modal.rect(term_size);
    let Some(url) = fb_state.url_to_open_on_click(modal_area, mouse.column, mouse.row) else {
        return false;
    };
    state.request_effect(ManagerEffect::OpenUrl(url));
    true
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollArea {
    area: Rect,
    content_width: usize,
}

#[must_use]
pub fn list_scroll_areas(
    state: &ManagerState<'_>,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> Option<SidebarScrollAreas> {
    let config = config?;
    let (_, _, right_x, right_w) =
        horizontal_split_pane_dims(state.list_split_pct, term_size.width);
    let body_y = LIST_HEADER_HEIGHT;
    let pane_area = Rect {
        x: right_x,
        y: body_y,
        width: right_w,
        height: term_size
            .height
            .saturating_sub(LIST_HEADER_HEIGHT + LIST_FOOTER_HEIGHT),
    };

    crate::tui::layout::list::selected_sidebar_scroll_areas(
        pane_area,
        state,
        config,
        std::path::Path::new(&state.current_dir),
    )
}

#[must_use]
pub fn editor_scroll_area(
    editor: &crate::tui::state::EditorState<'_>,
    term_size: Rect,
) -> ScrollArea {
    ScrollArea {
        area: editor.content_area(term_size),
        content_width: editor.workspace_mounts_content_width(),
    }
}
