// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod hover;
mod modal_scroll;
mod scroll_bars;
mod scroll_pan;
mod scroll_registry;
mod selection;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use termrock::interaction::HitRegion;

use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::modal_overlay;
use crate::tui::layout::{
    LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT, MOUSE_VERTICAL_SCROLL_STEP, ScrollbarAxis,
    apply_scrollbar_drag, horizontal_split_pane_dims, is_horizontally_scrollable, point_in_rect,
    split_seam_column,
};
use crate::tui::model::{ConsoleInputDispatchPlan, ConsolePaintedInputOwner};
use crate::tui::run::{ConsoleClickStageFacts, ConsoleClickabilityFacts, console_clickable_at};
use crate::tui::screens::editor::update::{
    editor_auth_row_index_at_position, editor_mount_hover_target_at_position,
    editor_mount_index_at_position, editor_scroll_focus_plan, editor_tab_bar_focus_plan,
};
use crate::tui::screens::settings::update::{
    settings_modal_open as settings_modal_open_fact, settings_scroll_focus_plan,
    settings_tab_bar_focus_plan, settings_trust_clickable_at_position,
    settings_trust_hover_target_at_position, settings_trust_row_at_position,
};
use crate::tui::screens::workspaces::update::{
    WorkspaceListMousePlan, apply_workspace_list_hover_target,
    workspace_list_clickable_at_position, workspace_list_hover_row_at_position,
    workspace_list_mouse_plan, workspace_list_scroll_focus_plan,
};
use crate::tui::state::ManagerEffect;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    EditorState, EditorTab, ManagerListRow, ManagerStage, ManagerState, Modal, MountScrollFocus,
    SettingsModal, SettingsState, SettingsTab,
};
use crate::tui::update::{
    ConsoleMouseWheelPlan, ListModalScrollTarget, SettingsModalScrollTarget,
    SharedModalScrollTarget, console_mouse_wheel_plan,
};

pub use hover::{
    ConsoleHoverTarget, container_info_copyable_row_at, file_browser_modal_and_state,
    file_browser_url_row_at, list_row_hover_at, try_copy_container_info_value,
    update_container_info_hover, update_hover,
};
pub use modal_scroll::{
    scroll_file_browser_state_at, scroll_global_mount_modal_selection, scroll_list_modal_selection,
    scroll_modal_selection, scroll_settings_auth_modal_selection,
    scroll_settings_env_modal_selection, try_scroll_file_browser_modal, try_scroll_picker_modal,
};
pub use scroll_bars::{try_drag_horizontal_scrollbar, try_drag_vertical_scrollbar};
pub use scroll_pan::{dispatch_wheel, settings_modal_open, update_scroll_focus};
pub use scroll_registry::{ConsoleScrollBlock, ScrollBlockRegion, hit, scroll_block_registry};
pub use selection::{
    editor_auth_row_index_at, editor_mount_index_at, try_select_editor_auth_row,
    try_select_editor_mount_row, try_select_editor_tab, try_select_settings_tab,
    try_select_settings_trust_row,
};

#[cfg(test)]
mod tests;

use crate::tui::layout::list::SidebarScrollAreas;

pub(super) fn editor_tab_hit_regions(
    editor: &EditorState<'_>,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
    op_available: bool,
) -> Vec<HitRegion<usize>> {
    let Some(config) = config else {
        return Vec::new();
    };
    crate::tui::screens::editor::view::editor_tab_hit_regions(
        term_size,
        editor,
        config,
        op_available,
    )
}

pub(super) fn settings_tab_hit_regions(
    settings: &SettingsState<'_>,
    term_size: Rect,
    op_available: bool,
) -> Vec<HitRegion<usize>> {
    crate::tui::screens::settings::view::settings_tab_hit_regions(term_size, settings, op_available)
}

pub(super) fn tab_hit_id(regions: &[HitRegion<usize>], mouse: MouseEvent) -> Option<usize> {
    let position = Position::new(mouse.column, mouse.row);
    regions
        .iter()
        .find(|region| region.area.contains(position))
        .map(|region| region.id)
}

#[cfg(test)]
use crate::tui::mount_display::global_config_mounts_content_width as global_mounts_content_width;
#[cfg(test)]
use crate::tui::mount_display::workspace_config_mounts_content_width as workspace_mounts_content_width;
#[cfg(test)]
use termrock::scroll::max_offset_u16 as max_scroll_offset;

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
    if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
        if state.drag_state.is_some() {
            dispatch_manager(state, ManagerMessage::SetDragState(None));
        }
        return super::InputOutcome::Continue;
    }

    let input_owner = state.painted_input_owner();
    if input_owner == ConsolePaintedInputOwner::Usage {
        handle_usage_pointer_event(state, mouse, term_size);
        return super::InputOutcome::Continue;
    }
    if !input_owner.allows_manager_pointer_actions() {
        state.clear_pointer_state();
        return super::InputOutcome::Continue;
    }
    let list_modal_painted = matches!(
        input_owner,
        ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::ListModal)
    );

    if matches!(mouse.kind, MouseEventKind::Moved) {
        if !term_size.contains(Position::new(mouse.column, mouse.row)) {
            clear_pointer_hover(state);
            return super::InputOutcome::Continue;
        }
        if list_modal_painted {
            state.clear_pointer_state();
            update_container_info_hover(state, mouse, term_size);
        } else if owner_is_modal(input_owner) {
            state.clear_pointer_state();
        } else {
            update_hover(state, mouse, term_size, config);
            if let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut() {
                info.set_hovered_row(None);
            }
        }
        return super::InputOutcome::Continue;
    }

    if !term_size.contains(Position::new(mouse.column, mouse.row)) {
        return super::InputOutcome::Continue;
    }

    if handle_modal_pointer_event(state, input_owner, mouse, term_size) {
        return super::InputOutcome::Continue;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_editor_tab(state, mouse, term_size, config)
    {
        return super::InputOutcome::Continue;
    }
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_settings_tab(state, mouse, term_size)
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
                    let outcome = dispatch_wheel(state, mouse, term_size, config, 0, delta);
                    if matches!(outcome, termrock::widgets::ScrollOutcome::Ignored)
                        && let Some(fallback) = vertical_fallback
                    {
                        let _ = dispatch_wheel(state, mouse, term_size, config, fallback, 0);
                    }
                }
                ConsoleMouseWheelPlan::Vertical(delta) => {
                    let _ = dispatch_wheel(state, mouse, term_size, config, delta, 0);
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

fn clear_pointer_hover(state: &mut ManagerState<'_>) {
    state.hover_target = None;
    state.hover = termrock::interaction::HoverState::default();
    match &mut state.stage {
        ManagerStage::Editor(editor) => editor.set_hover_target(None),
        ManagerStage::Settings(settings) => settings.set_hover_target(None),
        ManagerStage::List
        | ManagerStage::CreatePrelude(_)
        | ManagerStage::Workflows(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    if let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut() {
        info.set_hovered_row(None);
    }
}

fn handle_usage_pointer_event(state: &mut ManagerState<'_>, mouse: MouseEvent, area: Rect) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let _ = state
                .usage
                .select_painted_row(area, mouse.column, mouse.row);
        }
        MouseEventKind::ScrollUp => {
            let _ = state.usage.apply_painted_wheel_action(
                area,
                mouse.column,
                mouse.row,
                crate::tui::screens::usage::WheelDirection::Up,
            );
        }
        MouseEventKind::ScrollDown => {
            let _ = state.usage.apply_painted_wheel_action(
                area,
                mouse.column,
                mouse.row,
                crate::tui::screens::usage::WheelDirection::Down,
            );
        }
        MouseEventKind::Moved
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight
        | MouseEventKind::Down(_)
        | MouseEventKind::Drag(_)
        | MouseEventKind::Up(_) => {}
    }
}

fn handle_modal_pointer_event(
    state: &mut ManagerState<'_>,
    owner: ConsolePaintedInputOwner,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let list_modal_painted = matches!(
        owner,
        ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::ListModal)
    );

    if list_modal_painted
        && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_copy_container_info_value(state, mouse, term_size)
    {
        return true;
    }

    let container_info_rect = list_modal_painted
        .then(|| {
            state
                .list_modal
                .as_ref()
                .and_then(|modal| modal.container_info_rect(term_size))
        })
        .flatten();
    if let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut()
        && let Some(rect) = container_info_rect
        && info.scroll.handle_mouse(
            mouse.kind.into(),
            mouse.modifiers.into(),
            termrock::scroll::dialog_scroll_axes(info.content_width(), info.content_height(), rect),
        )
    {
        info.clamp_scroll(rect);
        return true;
    }

    if try_scroll_picker_modal(state, owner, mouse, term_size) {
        return true;
    }

    if try_scroll_file_browser_modal(state, owner, mouse, term_size) {
        return true;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_open_file_browser_git_url(state, mouse, term_size)
    {
        return true;
    }

    owner_is_modal(owner)
}

fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    update_manager(state, message);
}

#[must_use]
pub fn clickable_at(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    let pointer_supported = term_size.contains(Position::new(mouse.column, mouse.row));
    if !pointer_supported {
        return false;
    }
    let input_owner = state.painted_input_owner();
    if input_owner == ConsolePaintedInputOwner::Usage {
        return state
            .usage
            .painted_row_target(term_size, mouse.column, mouse.row)
            .is_some();
    }
    if !input_owner.allows_manager_pointer_actions() {
        return false;
    }
    let list_modal_painted = matches!(
        input_owner,
        ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::ListModal)
    );
    let settings_modal_painted = matches!(
        input_owner,
        ConsolePaintedInputOwner::Dispatch(
            ConsoleInputDispatchPlan::SettingsErrorPopup
                | ConsoleInputDispatchPlan::SettingsMountsModal
                | ConsoleInputDispatchPlan::SettingsEnvDialog
                | ConsoleInputDispatchPlan::SettingsAuthDialog
        )
    );

    let stage = match &state.stage {
        ManagerStage::Editor(editor) => ConsoleClickStageFacts::Editor {
            modal_open: editor.modal.is_some(),
            tab_target: tab_hit_id(
                &editor_tab_hit_regions(editor, term_size, config, state.op_available),
                mouse,
            )
            .is_some(),
            mount_row_target: editor_mount_index_at(editor, mouse, term_size).is_some(),
            auth_row_target: config
                .and_then(|cfg| editor_auth_row_index_at(editor, cfg, mouse, term_size))
                .is_some(),
        },
        ManagerStage::Settings(settings) => ConsoleClickStageFacts::Settings {
            mounts_modal_open: settings_modal_painted || settings.mounts.modals.is_open(),
            env_modal_open: settings_modal_painted || settings.env.modals.is_open(),
            tab_target: tab_hit_id(
                &settings_tab_hit_regions(settings, term_size, state.op_available),
                mouse,
            )
            .is_some(),
            trust_target: settings_trust_clickable_at_position(
                settings.active_tab,
                settings.mounts.modals.is_open(),
                settings.content_area(term_size),
                mouse.column,
                mouse.row,
            ),
        },
        ManagerStage::List => ConsoleClickStageFacts::List {
            list_modal_open: list_modal_painted,
            workspace_list_target: !list_modal_painted
                && workspace_list_clickable_at_position(
                    mouse.column,
                    mouse.row,
                    term_size,
                    state.list_split_pct,
                    list_modal_painted,
                    state.visual_rows_vec().as_slice(),
                    |row| state.index_of_row(row).is_some(),
                ),
        },
        ManagerStage::CreatePrelude(_)
        | ManagerStage::Workflows(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => ConsoleClickStageFacts::Other,
    };

    console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported,
        file_browser_url_target: file_browser_url_row_at(state, mouse, term_size),
        container_info_copy_target: list_modal_painted
            && container_info_copyable_row_at(state, mouse, term_size),
        stage,
    })
}

fn owner_is_modal(owner: ConsolePaintedInputOwner) -> bool {
    matches!(
        owner,
        ConsolePaintedInputOwner::Dispatch(
            ConsoleInputDispatchPlan::KeyboardHelp
                | ConsoleInputDispatchPlan::ListModal
                | ConsoleInputDispatchPlan::EditorModal
                | ConsoleInputDispatchPlan::SettingsErrorPopup
                | ConsoleInputDispatchPlan::SettingsMountsModal
                | ConsoleInputDispatchPlan::SettingsEnvDialog
                | ConsoleInputDispatchPlan::SettingsAuthDialog
                | ConsoleInputDispatchPlan::CreatePreludeModal
        )
    )
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
    pub area: Rect,
    pub content_width: usize,
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
pub fn editor_scroll_area(editor: &EditorState<'_>, term_size: Rect) -> ScrollArea {
    ScrollArea {
        area: editor.content_area(term_size),
        content_width: editor.workspace_mounts_content_width(),
    }
}
