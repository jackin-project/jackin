//! Mouse event handling for the workspace manager: list/details seam drag,
//! click-to-select in the list pane, and `FileBrowser` URL-click fallthrough.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::modal_rects::{self, ModalRectMode};
use crate::tui::layout::list::{
    SidebarScrollAreas, list_names_content_width, selected_sidebar_scroll_areas,
};
use crate::tui::layout::{
    LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT, MIN_DRAGGABLE_WIDTH, MOUSE_VERTICAL_SCROLL_STEP,
    SCREEN_HEADER_HEIGHT, ScrollbarAxis, TAB_STRIP_HEIGHT, apply_horizontal_scroll,
    apply_scrollbar_drag, apply_vertical_scroll, horizontal_split_pane_dims,
    is_horizontally_scrollable, point_in_rect, scroll_selection_at_position, scroll_viewport_width,
    split_seam_column,
};
#[cfg(test)]
use crate::tui::mount_display::global_config_mounts_content_width as global_mounts_content_width;
#[cfg(test)]
use crate::tui::mount_display::workspace_config_mounts_content_width as workspace_mounts_content_width;
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
    EditorTab, GlobalMountModal, ManagerListRow, ManagerStage, ManagerState, Modal,
    MountScrollFocus, SettingsAuthModal, SettingsTab,
};
use crate::tui::update::{
    ConsoleMouseWheelPlan, GlobalMountModalScrollTarget, ListModalScrollTarget,
    SettingsAuthModalScrollTarget, SettingsEnvModalScrollTarget, SharedModalScrollTarget,
    console_mouse_wheel_plan,
};
#[cfg(test)]
use jackin_tui::components::scrollable_panel::max_offset as max_scroll_offset;

/// Dispatch a mouse event into the workspace manager's list view. Drives
/// the mouse-draggable seam between the list pane and the details pane.
///
/// Behaviour:
/// - On `ManagerStage::List` with no list-level modal open: drives the
///   list/details seam drag (anchor + drag + release) and click-to-select.
/// - On `ManagerStage::Editor` / `CreatePrelude` with a `FileBrowser` modal
///   whose git-prompt overlay is active AND has a resolved URL: a
///   `Down(Left)` on the URL row queues a typed URL-open effect.
/// - Ignores everything when the terminal is narrower than
///   [`MIN_DRAGGABLE_WIDTH`] — drag bounds would be absurd.
/// - All other events are ignored.
///
/// The caller (run-loop in `src/console/mod.rs`) is responsible for
/// passing the current `terminal.size()?` as `term_size` so the handler
/// can compute the seam column as `term_size.width * list_split_pct / 100`.
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

    // Pointer motion only repaints the hovered tab / row; it never selects or
    // drags.
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

    // A scrollable modal (Debug info) captures the wheel so its own body scrolls
    // rather than the panel behind it. Shared handler → identical behaviour on
    // every surface; clamp to content so over-scroll cannot accumulate.
    let container_info_rect = state
        .list_modal
        .as_ref()
        .and_then(|modal| modal.container_info_rect(term_size));
    if let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut()
        && let Some(rect) = container_info_rect
        && info.scroll.on_mouse_scroll_for_axes(
            mouse.kind,
            mouse.modifiers,
            jackin_tui::components::dialog_scroll_axes(
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

    // Editor / CreatePrelude file-browser URL click: only on Down(Left),
    // only when the modal is a FileBrowser with a resolved git URL.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_open_file_browser_git_url(state, mouse, term_size)
    {
        return super::InputOutcome::Continue;
    }

    // Stage + modal gate for the list-view seam drag. Only the List view
    // participates in drag; the Editor, CreatePrelude and ConfirmDelete
    // stages only observe the URL-click path above.
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
///
/// Drives the OSC 22 hand-pointer cue (per the *Clickable targets must look
/// clickable* TUI rule). Reuses the same hit-tests as the click handlers so
/// the pointer cue and the click action can never disagree. The seam column is
/// a resize affordance, not a click target, so it is excluded here.
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
                .and_then(|config| editor_auth_row_index_at(editor, config, mouse, term_size))
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

fn try_copy_container_info_value(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let Some(modal @ Modal::ContainerInfo { state: info }) = state.list_modal.as_ref() else {
        return false;
    };
    let Some(area) = modal.container_info_rect(term_size) else {
        return false;
    };
    let Some((row, payload)) =
        jackin_tui::components::container_info_copy_payload_at(area, info, mouse.column, mouse.row)
    else {
        return false;
    };
    state.request_effect(ManagerEffect::CopyContainerInfoValue { row, payload });
    true
}

fn container_info_copyable_row_at(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let Some(modal @ Modal::ContainerInfo { state: info }) = state.list_modal.as_ref() else {
        return false;
    };
    let Some(area) = modal.container_info_rect(term_size) else {
        return false;
    };
    jackin_tui::components::container_info_copy_payload_at(area, info, mouse.column, mouse.row)
        .is_some()
}

/// Brighten the hovered copyable row in the Debug info dialog (link hover cue),
/// mirroring the launch cockpit. No-op unless that modal is open.
fn update_container_info_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    let Some(modal @ Modal::ContainerInfo { .. }) = state.list_modal.as_ref() else {
        return;
    };
    let Some(area) = modal.container_info_rect(term_size) else {
        return;
    };
    let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut() else {
        return;
    };
    let hovered =
        jackin_tui::components::container_info_copy_payload_at(area, info, mouse.column, mouse.row)
            .map(|(row, _)| row);
    info.set_hovered_row(hovered);
}

/// Resolve the active file-browser modal and its state from whichever stage
/// owns it (editor or create-prelude). Shared by the URL-row hit-test and the
/// click handler so their modal resolution can't drift out of step.
fn file_browser_modal_and_state<'a, 'b>(
    state: &'a ManagerState<'b>,
) -> Option<(&'a Modal<'b>, &'a FileBrowserState)> {
    let modal = match &state.stage {
        ManagerStage::Editor(editor) => editor.modal.as_ref(),
        ManagerStage::CreatePrelude(prelude) => prelude.modal.as_ref(),
        _ => return None,
    }?;
    match modal {
        Modal::FileBrowser { state, .. } => Some((modal, state)),
        _ => None,
    }
}

/// Whether the pointer is over a file-browser git-prompt URL row (side-effect
/// free; does not open the URL).
fn file_browser_url_row_at(state: &ManagerState<'_>, mouse: MouseEvent, term_size: Rect) -> bool {
    let Some((modal, fb_state)) = file_browser_modal_and_state(state) else {
        return false;
    };
    let modal_area = modal.rect(term_size);
    fb_state.url_row_hit(modal_area, mouse.column, mouse.row)
}

fn try_scroll_file_browser_modal(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let delta = match mouse.kind {
        MouseEventKind::ScrollUp => -MOUSE_VERTICAL_SCROLL_STEP,
        MouseEventKind::ScrollDown => MOUSE_VERTICAL_SCROLL_STEP,
        _ => return false,
    };
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let Some(modal @ Modal::FileBrowser { .. }) = editor.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let Some(Modal::FileBrowser { state, .. }) = editor.modal.as_mut() else {
                return false;
            };
            scroll_file_browser_state_at(state, area, mouse, delta)
        }
        ManagerStage::CreatePrelude(prelude) => {
            let Some(modal @ Modal::FileBrowser { .. }) = prelude.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let Some(Modal::FileBrowser { state, .. }) = prelude.modal.as_mut() else {
                return false;
            };
            scroll_file_browser_state_at(state, area, mouse, delta)
        }
        ManagerStage::Settings(settings) => {
            let area = modal_rects::modal_rect_for_mode(term_size, ModalRectMode::FileBrowser);
            if let Some(GlobalMountModal::FileBrowser { state }) = settings.mounts.modal.as_mut() {
                return scroll_file_browser_state_at(state, area, mouse, delta);
            }
            if let Some(SettingsAuthModal::SourceFolderPicker { state }) = settings.auth.modal_mut()
            {
                return scroll_file_browser_state_at(state, area, mouse, delta);
            }
            false
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
    }
}

fn scroll_file_browser_state_at(
    state: &mut FileBrowserState,
    area: Rect,
    mouse: MouseEvent,
    delta: i16,
) -> bool {
    state.scroll_selection_at(area, mouse.column, mouse.row, delta)
}

fn try_scroll_picker_modal(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let delta = match mouse.kind {
        MouseEventKind::ScrollUp => -MOUSE_VERTICAL_SCROLL_STEP,
        MouseEventKind::ScrollDown => MOUSE_VERTICAL_SCROLL_STEP,
        _ => return false,
    };

    if let Some(modal) = state.list_modal.as_ref() {
        let area = modal.rect(term_size);
        if point_in_rect(mouse.column, mouse.row, area) {
            return scroll_list_modal_selection(state, delta);
        }
    }

    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let Some(modal) = editor.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            if !point_in_rect(mouse.column, mouse.row, area) {
                return false;
            }
            scroll_modal_selection(editor.modal.as_mut(), delta)
        }
        ManagerStage::CreatePrelude(prelude) => {
            let Some(modal) = prelude.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            if !point_in_rect(mouse.column, mouse.row, area) {
                return false;
            }
            scroll_modal_selection(prelude.modal.as_mut(), delta)
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modal.as_mut() {
                return scroll_global_mount_modal_selection(modal, mouse, term_size, delta);
            }
            if let Some(modal) = settings.env.modal.as_mut() {
                return scroll_settings_env_modal_selection(modal, mouse, term_size, delta);
            }
            if let Some(modal) = settings.auth.modal_mut() {
                return scroll_settings_auth_modal_selection(modal, mouse, term_size, delta);
            }
            false
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
    }
}

fn scroll_list_modal_selection(state: &mut ManagerState<'_>, delta: i16) -> bool {
    let Some(modal) = state.list_modal.as_mut() else {
        return false;
    };
    let target = modal.list_scroll_target();
    match (target, modal) {
        (ListModalScrollTarget::GithubPicker, Modal::GithubPicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (ListModalScrollTarget::RolePicker, Modal::RolePicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (ListModalScrollTarget::OpPicker, Modal::OpPicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (ListModalScrollTarget::None, _) => false,
        _ => false,
    }
}

fn scroll_modal_selection(modal: Option<&mut Modal<'_>>, delta: i16) -> bool {
    let Some(modal) = modal else {
        return false;
    };
    let target = modal.shared_scroll_target();
    match (target, modal) {
        (SharedModalScrollTarget::WorkdirPick, Modal::WorkdirPick { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::RolePicker, Modal::RolePicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::RolePicker, Modal::RoleOverridePicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::RolePicker, Modal::AuthRolePicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::OpPicker, Modal::OpPicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::None, _) => false,
        _ => false,
    }
}

fn scroll_global_mount_modal_selection(
    modal: &mut GlobalMountModal<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    delta: i16,
) -> bool {
    let target = modal.scroll_target();
    match (target, modal) {
        (GlobalMountModalScrollTarget::RolePicker, GlobalMountModal::RolePicker { state }) => {
            let area = modal_rects::role_picker_rect_for_count(term_size, state.filtered.len());
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
        }
        (GlobalMountModalScrollTarget::None, _) => false,
        _ => false,
    }
}

fn scroll_settings_env_modal_selection(
    modal: &mut crate::tui::state::SettingsEnvModal<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    delta: i16,
) -> bool {
    let target = modal.scroll_target();
    match (target, modal) {
        (
            SettingsEnvModalScrollTarget::OpPicker,
            crate::tui::state::SettingsEnvModal::OpPicker { state },
        ) => {
            let area = modal_rects::op_picker_rect(term_size);
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
        }
        (
            SettingsEnvModalScrollTarget::RolePicker,
            crate::tui::state::SettingsEnvModal::RolePicker { state },
        ) => {
            let area = modal_rects::role_picker_rect_for_count(term_size, state.filtered.len());
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
        }
        (SettingsEnvModalScrollTarget::None, _) => false,
        _ => false,
    }
}

fn scroll_settings_auth_modal_selection(
    modal: &mut SettingsAuthModal<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    delta: i16,
) -> bool {
    let target = modal.scroll_target();
    match (target, modal) {
        (SettingsAuthModalScrollTarget::OpPicker, SettingsAuthModal::OpPicker { state }) => {
            let area = modal_rects::op_picker_rect(term_size);
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
        }
        (SettingsAuthModalScrollTarget::None, _) => false,
        _ => false,
    }
}

/// Track the list row under the pointer so the renderer can lift its
/// background, mirroring the tab-hover cue. Cleared when off the list pane,
/// over the seam, or when a list modal is open.
fn update_list_row_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    apply_workspace_list_hover_target(
        state,
        list_row_hover_at(state, mouse, term_size)
            .map(crate::tui::screens::workspaces::model::ManagerHoverTarget::ListRow),
    );
}

/// Track the hovered row on the editor Mounts tab and the Settings Trust tab so
/// their renderers can lift it, mirroring the tab/list hover cue. Cleared off
/// the relevant content area.
fn update_row_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(target) = editor_mount_hover_target_at_position(
                editor.active_tab,
                editor.modal.is_some(),
                editor_scroll_area(editor, term_size).area,
                mouse.column,
                mouse.row,
                editor.tab_scroll_y,
                editor.pending.mounts.as_slice(),
            ) {
                editor.set_hover_target(Some(target));
            } else if editor.hovered_mount_row().is_some() {
                editor.set_hover_target(None);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(target) = settings_trust_hover_target_at_position(
                settings.active_tab,
                settings.mounts.modal.is_some(),
                settings.content_area(term_size),
                mouse.column,
                mouse.row,
                settings.trust.scroll_y,
                settings.trust.pending.len(),
            ) {
                settings.set_hover_target(Some(target));
            } else if settings.hovered_trust_row().is_some() {
                settings.set_hover_target(None);
            }
        }
        _ => {}
    }
}

fn list_row_hover_at(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<ManagerListRow> {
    if !matches!(state.stage, ManagerStage::List) || state.list_modal.is_some() {
        return None;
    }
    let seam_x = split_seam_column(state.list_split_pct, term_size.width);
    workspace_list_hover_row_at_position(
        state.visual_rows_vec().as_slice(),
        mouse.column,
        mouse.row,
        term_size,
        seam_x,
        |row| state.index_of_row(row).is_some(),
    )
}

fn try_select_editor_tab(state: &mut ManagerState<'_>, mouse: MouseEvent) -> bool {
    let ManagerStage::Editor(editor) = &state.stage else {
        return false;
    };
    if editor.modal.is_some() {
        return false;
    }

    let Some(tab) = editor_tab_at_position(mouse.row, mouse.column) else {
        return false;
    };

    dispatch_manager(state, ManagerMessage::SelectEditorTab(tab));
    true
}

/// Repaint the hovered tab index on mouse motion so the strip lifts under the
/// pointer like the in-container multiplexer tabs. A motion off the strip
/// clears the highlight.
fn update_tab_hover(state: &mut ManagerState<'_>, mouse: MouseEvent) {
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.set_hover_target(editor_tab_hover_target_plan(
                editor.modal.is_some(),
                mouse.row,
                mouse.column,
            ));
        }
        ManagerStage::Settings(settings) => {
            settings.set_hover_target(settings_tab_hover_target_plan(
                settings.mounts.modal.is_some(),
                settings.env.modal.is_some(),
                mouse.row,
                mouse.column,
            ));
        }
        _ => {}
    }
}

fn try_select_settings_tab(state: &mut ManagerState<'_>, mouse: MouseEvent) -> bool {
    let ManagerStage::Settings(settings) = &state.stage else {
        return false;
    };
    if settings.mounts.modal.is_some() || settings.env.modal.is_some() {
        return false;
    }

    let Some(tab) = settings_tab_at_position(mouse.row, mouse.column) else {
        return false;
    };
    dispatch_manager(state, ManagerMessage::SelectSettingsTab(tab));
    true
}

/// Click inside the Trust block selects the row and activates the block for scrolling.
fn try_select_settings_trust_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Settings(settings) = &state.stage else {
        return false;
    };
    if settings.active_tab != SettingsTab::Trust || settings.mounts.modal.is_some() {
        return false;
    }
    let area = settings.content_area(term_size);
    if let Some(row) = settings_trust_row_at_position(
        area,
        mouse.column,
        mouse.row,
        settings.trust.scroll_y,
        settings.trust.pending.len(),
    ) {
        dispatch_manager(state, ManagerMessage::SelectSettingsTrustRow(row));
    } else {
        dispatch_manager(state, ManagerMessage::SelectSettingsTrustRow(usize::MAX));
    }
    true
}

/// Mount-row index the pointer is over on the editor Mounts tab, or `None`.
/// Pure geometry shared by the click handler and the hover hand-pointer cue so
/// they can't drift.
fn editor_mount_index_at(
    editor: &crate::tui::state::EditorState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    editor_mount_index_at_position(
        editor.active_tab,
        editor.modal.is_some(),
        editor_scroll_area(editor, term_size).area,
        mouse.column,
        mouse.row,
        editor.tab_scroll_y,
        editor.pending.mounts.as_slice(),
    )
}

fn try_select_editor_mount_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Editor(editor) = &state.stage else {
        return false;
    };
    let Some(index) = editor_mount_index_at(editor, mouse, term_size) else {
        return false;
    };
    dispatch_manager(state, ManagerMessage::SelectEditorMountRow(index));
    true
}

fn try_select_editor_auth_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return false;
    };
    let Some(index) = editor_auth_row_index_at(editor, config, mouse, term_size) else {
        return false;
    };
    editor.select_auth_row(index);
    true
}

fn editor_auth_row_index_at(
    editor: &crate::tui::state::EditorState<'_>,
    config: &jackin_config::AppConfig,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    let rows = editor.auth_flat_rows(config);
    editor_auth_row_index_at_position(
        editor.active_tab,
        editor.modal.is_some(),
        editor.content_area(term_size),
        mouse.column,
        mouse.row,
        editor.tab_scroll_y,
        &rows,
    )
}

#[allow(clippy::items_after_statements, clippy::too_many_lines)]
fn try_drag_horizontal_scrollbar(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    match &mut state.stage {
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return false;
            }
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                return false;
            };
            if apply_scrollbar_drag(
                ScrollbarAxis::Horizontal,
                &mut state.list_mounts_scroll_x,
                areas.workspace.area,
                areas.workspace.content_width,
                mouse.column,
                mouse.row,
            ) {
                state.set_list_scroll_focus(
                    workspace_list_scroll_focus_plan(false, true, true, false, false, false)
                        .scroll_focus,
                );
                return true;
            }
            if apply_scrollbar_drag(
                ScrollbarAxis::Horizontal,
                &mut state.list_global_mounts_scroll_x,
                areas.global.area,
                areas.global.content_width,
                mouse.column,
                mouse.row,
            ) {
                state.set_list_scroll_focus(
                    workspace_list_scroll_focus_plan(false, true, false, true, false, false)
                        .scroll_focus,
                );
                return true;
            }
            if let Some(role) = areas.role_global
                && apply_scrollbar_drag(
                    ScrollbarAxis::Horizontal,
                    &mut state.list_role_global_mounts_scroll_x,
                    role.area,
                    role.content_width,
                    mouse.column,
                    mouse.row,
                )
            {
                state.set_list_scroll_focus(
                    workspace_list_scroll_focus_plan(false, true, false, false, true, false)
                        .scroll_focus,
                );
                return true;
            }
            false
        }
        ManagerStage::Editor(editor) => {
            if editor.modal.is_some() {
                return false;
            }
            let dragged = if editor.active_tab == EditorTab::Mounts {
                let workspace = editor_scroll_area(editor, term_size);
                apply_scrollbar_drag(
                    ScrollbarAxis::Horizontal,
                    &mut editor.workspace_mounts_scroll_x,
                    workspace.area,
                    workspace.content_width,
                    mouse.column,
                    mouse.row,
                )
            } else {
                let content_area = editor.content_area(term_size);
                apply_scrollbar_drag(
                    ScrollbarAxis::Horizontal,
                    &mut editor.tab_scroll_x,
                    content_area,
                    editor.tab_content_width,
                    mouse.column,
                    mouse.row,
                )
            };
            if dragged {
                let plan = editor_scroll_focus_plan(
                    editor.active_tab,
                    false,
                    editor.active_tab == EditorTab::Mounts,
                    editor.active_tab != EditorTab::Mounts,
                );
                editor.apply_scroll_focus_plan(plan);
            }
            dragged
        }
        ManagerStage::Settings(settings) => {
            if settings_modal_open(settings) {
                return false;
            }
            if settings.active_tab != SettingsTab::Mounts {
                return false;
            }
            let content_width = settings.mounts.content_width();
            apply_scrollbar_drag(
                ScrollbarAxis::Horizontal,
                &mut settings.mounts.scroll_x,
                Rect {
                    x: 0,
                    y: SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT,
                    width: term_size.width,
                    height: term_size.height.saturating_sub(
                        SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT + LIST_FOOTER_HEIGHT,
                    ),
                },
                content_width,
                mouse.column,
                mouse.row,
            )
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
    }
}

fn update_scroll_focus(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) {
    match &mut state.stage {
        ManagerStage::List => {
            // Determine whether the click is in the left pane.
            let seam_x = split_seam_column(state.list_split_pct, term_size.width);
            let left_pane_area = Rect {
                x: 0,
                y: LIST_HEADER_HEIGHT,
                width: seam_x,
                height: term_size
                    .height
                    .saturating_sub(LIST_HEADER_HEIGHT + LIST_FOOTER_HEIGHT),
            };
            let in_left_pane = point_in_rect(mouse.column, mouse.row, left_pane_area);
            let areas = list_scroll_areas(state, term_size, config);
            let plan = areas.map_or_else(
                || {
                    workspace_list_scroll_focus_plan(
                        in_left_pane,
                        false,
                        false,
                        false,
                        false,
                        false,
                    )
                },
                |areas| {
                    workspace_list_scroll_focus_plan(
                        in_left_pane,
                        true,
                        point_in_rect(mouse.column, mouse.row, areas.workspace.area),
                        point_in_rect(mouse.column, mouse.row, areas.global.area)
                            && areas.global.area.height > 0,
                        areas
                            .role_global
                            .is_some_and(|r| point_in_rect(mouse.column, mouse.row, r.area)),
                        areas
                            .roles
                            .is_some_and(|r| point_in_rect(mouse.column, mouse.row, r.area)),
                    )
                },
            );
            dispatch_manager(
                state,
                ManagerMessage::SetListNamesFocused(plan.list_names_focused),
            );
            dispatch_manager(state, ManagerMessage::SetListScrollFocus(plan.scroll_focus));
        }
        ManagerStage::Editor(editor) => {
            let plan = if editor.active_tab == EditorTab::Mounts {
                let in_workspace_mounts = if editor.modal.is_some() {
                    false
                } else {
                    let area = editor_scroll_area(editor, term_size);
                    point_in_rect(mouse.column, mouse.row, area.area)
                };
                editor_scroll_focus_plan(
                    editor.active_tab,
                    editor.modal.is_some(),
                    in_workspace_mounts,
                    false,
                )
            } else {
                let in_tab_content = if editor.modal.is_some() {
                    false
                } else {
                    let content_area = editor.content_area(term_size);
                    point_in_rect(mouse.column, mouse.row, content_area)
                };
                editor_scroll_focus_plan(
                    editor.active_tab,
                    editor.modal.is_some(),
                    false,
                    in_tab_content,
                )
            };
            editor.apply_scroll_focus_plan(plan);
            // Clicking the content block transfers interaction focus into it —
            // same as Tab/↓ — so the green border and ▸ appear in the same frame.
            let clicked_content =
                plan.workspace_mounts_scroll_focused || plan.tab_content_scroll_focused;
            if clicked_content && editor.tab_bar_focused() {
                editor.apply_tab_bar_focus_plan(editor_tab_bar_focus_plan(false));
            }
        }
        ManagerStage::Settings(settings) => {
            let modal_open = settings_modal_open(settings);
            let in_content = if modal_open {
                false
            } else {
                point_in_rect(mouse.column, mouse.row, settings.content_area(term_size))
            };
            let plan = settings_scroll_focus_plan(settings.active_tab, modal_open, in_content);
            settings.apply_scroll_focus_plan(plan);
            // Clicking the content block transfers interaction focus into it —
            // same as Tab/↓ — so the green border and ▸ appear in the same frame.
            if in_content && settings.tab_bar_focused() {
                settings.apply_tab_bar_focus_plan(settings_tab_bar_focus_plan(false));
            }
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
}

#[derive(Clone, Copy)]
struct ScrollArea {
    area: Rect,
    content_width: usize,
}

const fn settings_modal_open(settings: &crate::tui::state::SettingsState<'_>) -> bool {
    settings_modal_open_fact(
        settings.error_popup.is_some(),
        settings.mounts.modal.is_some(),
        settings.env.modal.is_some(),
        settings.auth.has_modal(),
    )
}

fn try_drag_vertical_scrollbar(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    match &mut state.stage {
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return false;
            }
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                return false;
            };
            let Some(focus) = state.list_scroll_focus() else {
                return false;
            };
            match focus {
                MountScrollFocus::Workspace => apply_scrollbar_drag(
                    ScrollbarAxis::Vertical,
                    &mut state.list_mounts_scroll_y,
                    areas.workspace.area,
                    areas.workspace.content_height,
                    mouse.column,
                    mouse.row,
                ),
                MountScrollFocus::Global => apply_scrollbar_drag(
                    ScrollbarAxis::Vertical,
                    &mut state.list_global_mounts_scroll_y,
                    areas.global.area,
                    areas.global.content_height,
                    mouse.column,
                    mouse.row,
                ),
                MountScrollFocus::RoleGlobal => areas.role_global.is_some_and(|area| {
                    apply_scrollbar_drag(
                        ScrollbarAxis::Vertical,
                        &mut state.list_role_global_mounts_scroll_y,
                        area.area,
                        area.content_height,
                        mouse.column,
                        mouse.row,
                    )
                }),
                MountScrollFocus::Roles => areas.roles.is_some_and(|area| {
                    apply_scrollbar_drag(
                        ScrollbarAxis::Vertical,
                        &mut state.list_roles_scroll_y,
                        area.area,
                        area.content_height,
                        mouse.column,
                        mouse.row,
                    )
                }),
            }
        }
        ManagerStage::Editor(editor) => {
            if editor.modal.is_some() {
                return false;
            }
            let area = editor.content_area(term_size);
            let content_height = editor.tab_content_height;
            apply_scrollbar_drag(
                ScrollbarAxis::Vertical,
                &mut editor.tab_scroll_y,
                area,
                content_height,
                mouse.column,
                mouse.row,
            )
        }
        ManagerStage::Settings(settings) => {
            if settings_modal_open(settings) {
                return false;
            }
            let area = settings.content_area(term_size);
            let content_height = match settings.active_tab {
                SettingsTab::General => 0,
                SettingsTab::Mounts => settings.mounts_content_height(),
                SettingsTab::Environments => settings.env_content_height(),
                SettingsTab::Auth => settings.auth_content_height(),
                SettingsTab::Trust => settings.trust_content_height(),
            };
            match settings.active_tab {
                SettingsTab::General => false,
                SettingsTab::Mounts => apply_scrollbar_drag(
                    ScrollbarAxis::Vertical,
                    &mut settings.mounts.scroll_y,
                    area,
                    content_height,
                    mouse.column,
                    mouse.row,
                ),
                SettingsTab::Environments => apply_scrollbar_drag(
                    ScrollbarAxis::Vertical,
                    &mut settings.env.scroll_y,
                    area,
                    content_height,
                    mouse.column,
                    mouse.row,
                ),
                SettingsTab::Auth => apply_scrollbar_drag(
                    ScrollbarAxis::Vertical,
                    settings.auth.scroll_y_mut(),
                    area,
                    content_height,
                    mouse.column,
                    mouse.row,
                ),
                SettingsTab::Trust => apply_scrollbar_drag(
                    ScrollbarAxis::Vertical,
                    &mut settings.trust.scroll_y,
                    area,
                    content_height,
                    mouse.column,
                    mouse.row,
                ),
            }
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
    }
}

fn scroll_active_panel(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
    delta: i16,
) -> bool {
    match &mut state.stage {
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return false;
            }
            update_scroll_focus(state, mouse, term_size, config);
            if state.list_names_focused() {
                let (left_x, left_w, _, _) =
                    horizontal_split_pane_dims(state.list_split_pct, term_size.width);
                let area = Rect {
                    x: left_x,
                    y: LIST_HEADER_HEIGHT,
                    width: left_w,
                    height: term_size
                        .height
                        .saturating_sub(LIST_HEADER_HEIGHT + LIST_FOOTER_HEIGHT),
                };
                let viewport = scroll_viewport_width(area);
                let content_width = list_names_content_width(state, viewport);
                return apply_horizontal_scroll(
                    &mut state.list_names_scroll_x,
                    delta,
                    area,
                    content_width,
                );
            }
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                state.set_list_scroll_focus(
                    workspace_list_scroll_focus_plan(false, false, false, false, false, false)
                        .scroll_focus,
                );
                return false;
            };
            let Some(focus) = state.list_scroll_focus() else {
                return false;
            };
            let area_info = match focus {
                MountScrollFocus::Workspace => Some(areas.workspace),
                MountScrollFocus::Global => Some(areas.global),
                MountScrollFocus::RoleGlobal => areas.role_global,
                MountScrollFocus::Roles => areas.roles,
            };
            let Some(area_info) = area_info else {
                return false;
            };
            apply_horizontal_scroll(
                state.list_scroll_x_mut(focus),
                delta,
                area_info.area,
                area_info.content_width,
            )
        }
        ManagerStage::Editor(editor) => {
            if editor.modal.is_some() {
                return false;
            }
            if editor.active_tab != EditorTab::Mounts {
                let area = editor.content_area(term_size);
                let in_scrollable_content = point_in_rect(mouse.column, mouse.row, area)
                    && is_horizontally_scrollable(area, editor.tab_content_width);
                let plan = editor_scroll_focus_plan(
                    editor.active_tab,
                    false,
                    false,
                    in_scrollable_content,
                );
                editor.apply_scroll_focus_plan(plan);
                return plan.tab_content_scroll_focused
                    && apply_horizontal_scroll(
                        &mut editor.tab_scroll_x,
                        delta,
                        area,
                        editor.tab_content_width,
                    );
            }
            let area = editor_scroll_area(editor, term_size);
            let in_scrollable_workspace = point_in_rect(mouse.column, mouse.row, area.area)
                && is_horizontally_scrollable(area.area, area.content_width);
            let plan =
                editor_scroll_focus_plan(editor.active_tab, false, in_scrollable_workspace, false);
            editor.apply_scroll_focus_plan(plan);
            plan.workspace_mounts_scroll_focused
                && apply_horizontal_scroll(
                    &mut editor.workspace_mounts_scroll_x,
                    delta,
                    area.area,
                    area.content_width,
                )
        }
        ManagerStage::Settings(settings) => {
            if settings_modal_open(settings) {
                return false;
            }
            // Hover-scroll: fire on whichever block the cursor is over.
            let content_area = settings.content_area(term_size);
            if !point_in_rect(mouse.column, mouse.row, content_area) {
                return false;
            }
            match settings.active_tab {
                SettingsTab::Mounts => {
                    let content_width = settings.mounts.content_width();
                    apply_horizontal_scroll(
                        &mut settings.mounts.scroll_x,
                        delta,
                        content_area,
                        content_width,
                    )
                }
                SettingsTab::Trust => {
                    let cw =
                        crate::tui::screens::settings::update::trust_content_width(&settings.trust);
                    apply_horizontal_scroll(&mut settings.trust.scroll_x, delta, content_area, cw)
                }
                _ => false,
            }
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
    }
}

/// Dispatch a vertical scroll event to whichever content block the mouse is over.
/// Horizontal-only blocks (List view mounts) are silently ignored here —
/// their scroll is only driven by left/right events via `scroll_active_panel`.
#[allow(clippy::missing_const_for_fn)]
fn scroll_active_panel_vertical(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
    delta: i16,
) {
    match &mut state.stage {
        ManagerStage::Settings(settings) => {
            if settings_modal_open(settings) {
                return;
            }
            let content_area = settings.content_area(term_size);
            if !point_in_rect(mouse.column, mouse.row, content_area) {
                return;
            }
            match settings.active_tab {
                // General has no scrollable content; empty arm is intentional.
                SettingsTab::General => {}
                SettingsTab::Mounts => {
                    let content_height = settings.mounts_content_height();
                    apply_vertical_scroll(
                        &mut settings.mounts.scroll_y,
                        delta,
                        content_area,
                        content_height,
                    );
                }
                SettingsTab::Environments => {
                    let content_height = settings.env_content_height();
                    apply_vertical_scroll(
                        &mut settings.env.scroll_y,
                        delta,
                        content_area,
                        content_height,
                    );
                }
                SettingsTab::Trust => {
                    let content_height = settings.trust_content_height();
                    apply_vertical_scroll(
                        &mut settings.trust.scroll_y,
                        delta,
                        content_area,
                        content_height,
                    );
                }
                SettingsTab::Auth => {
                    let content_height = settings.auth_content_height();
                    apply_vertical_scroll(
                        settings.auth.scroll_y_mut(),
                        delta,
                        content_area,
                        content_height,
                    );
                }
            }
        }
        ManagerStage::Editor(editor) => {
            if editor.modal.is_some() {
                return;
            }
            let area = editor.content_area(term_size);
            if !point_in_rect(mouse.column, mouse.row, area) {
                return;
            }
            let content_height = editor.tab_content_height;
            apply_vertical_scroll(&mut editor.tab_scroll_y, delta, area, content_height);
        }
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return;
            }
            update_scroll_focus(state, mouse, term_size, config);
            // Scroll the focused block vertically.
            match state.list_scroll_focus() {
                Some(MountScrollFocus::Workspace) => {
                    if let Some(areas) = list_scroll_areas(state, term_size, config) {
                        apply_vertical_scroll(
                            &mut state.list_mounts_scroll_y,
                            delta,
                            areas.workspace.area,
                            areas.workspace.content_height,
                        );
                    }
                }
                Some(MountScrollFocus::Global) => {
                    if let Some(areas) = list_scroll_areas(state, term_size, config) {
                        apply_vertical_scroll(
                            &mut state.list_global_mounts_scroll_y,
                            delta,
                            areas.global.area,
                            areas.global.content_height,
                        );
                    }
                }
                Some(MountScrollFocus::RoleGlobal) => {
                    if let Some(areas) = list_scroll_areas(state, term_size, config)
                        && let Some(area) = areas.role_global
                    {
                        apply_vertical_scroll(
                            &mut state.list_role_global_mounts_scroll_y,
                            delta,
                            area.area,
                            area.content_height,
                        );
                    }
                }
                Some(MountScrollFocus::Roles) => {
                    if let Some(areas) = list_scroll_areas(state, term_size, config)
                        && let Some(area) = areas.roles
                    {
                        apply_vertical_scroll(
                            &mut state.list_roles_scroll_y,
                            delta,
                            area.area,
                            area.content_height,
                        );
                    }
                }
                None => {}
            }
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
}

fn list_scroll_areas(
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

    selected_sidebar_scroll_areas(
        pane_area,
        state,
        config,
        std::path::Path::new(&state.current_dir),
    )
}

fn editor_scroll_area(editor: &crate::tui::state::EditorState<'_>, term_size: Rect) -> ScrollArea {
    ScrollArea {
        area: editor.content_area(term_size),
        content_width: editor.workspace_mounts_content_width(),
    }
}

/// If the `Editor` or `CreatePrelude` stage has an open `FileBrowser`
/// whose git-prompt is active with a resolved URL, and the click lands
/// on the URL row, request browser-open from the non-TUI service adapter.
/// Returns `true` iff the click was consumed. Non-matching stages,
/// non-click events, and clicks outside the URL row all return `false`
/// and the caller falls through to the list-view handler.
///
/// Modal geometry comes from the same helper `render_modal` uses, so mouse
/// hit-testing can never drift out of sync with what was drawn.
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

#[cfg(test)]
mod tests;
