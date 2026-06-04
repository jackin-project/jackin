//! Mouse event handling for the workspace manager: list/details seam drag,
//! click-to-select in the list pane, and `FileBrowser` URL-click fallthrough.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::console::tui::components::modal_layout::modal_outer_rect;
#[cfg(test)]
use crate::console::tui::components::mount_display::global_mounts_content_width;
#[cfg(test)]
use crate::console::tui::components::mount_display::workspace_mounts_content_width;
use crate::console::tui::components::mount_display::{
    settings_global_mounts_content_width_with_cache, workspace_mounts_content_width_with_cache,
};
use crate::console::tui::effect::ManagerEffect;
use crate::console::tui::layout::list::{
    SidebarScrollAreas, list_names_content_width, selected_sidebar_scroll_areas,
};
use crate::console::tui::layout::settings::{
    auth_content_height, env_content_height, mounts_content_height, trust_content_height,
};
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::state::{
    DragState, EditorTab, ManagerListRow, ManagerStage, ManagerState, Modal, MountScrollFocus,
    SettingsTab, clamp_split,
};
use jackin_console::tui::components::file_browser::FileBrowserState;
use jackin_console::tui::layout::{
    LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT, MIN_DRAGGABLE_WIDTH, MOUSE_HORIZONTAL_SCROLL_STEP,
    MOUSE_VERTICAL_SCROLL_STEP, SCREEN_HEADER_HEIGHT, ScrollbarAxis, TAB_STRIP_HEIGHT,
    apply_horizontal_scroll, apply_vertical_scroll, horizontal_split_pane_dims,
    is_horizontally_scrollable, list_content_visual_index_at, near_seam, point_in_rect,
    scroll_viewport_width, scrollbar_drag_offset, split_pct_from_drag, split_seam_column,
    tab_cell_at_position, tabbed_content_area,
};
use jackin_console::tui::screens::editor::update::editor_scroll_focus_plan;
use jackin_console::tui::screens::settings::update::settings_scroll_focus_plan;
use jackin_console::tui::screens::workspaces::update::workspace_list_scroll_focus_plan;
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

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(crate) fn handle_mouse_with_config(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
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
        MouseEventKind::ScrollLeft => {
            scroll_active_panel(
                state,
                mouse,
                term_size,
                config,
                -(MOUSE_HORIZONTAL_SCROLL_STEP as i16),
            );
            return super::InputOutcome::Continue;
        }
        MouseEventKind::ScrollRight => {
            scroll_active_panel(
                state,
                mouse,
                term_size,
                config,
                MOUSE_HORIZONTAL_SCROLL_STEP as i16,
            );
            return super::InputOutcome::Continue;
        }
        MouseEventKind::ScrollUp => {
            scroll_active_panel_vertical(
                state,
                mouse,
                term_size,
                config,
                -MOUSE_VERTICAL_SCROLL_STEP,
            );
            return super::InputOutcome::Continue;
        }
        MouseEventKind::ScrollDown => {
            scroll_active_panel_vertical(
                state,
                mouse,
                term_size,
                config,
                MOUSE_VERTICAL_SCROLL_STEP,
            );
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
        MouseEventKind::Down(MouseButton::Left) => {
            let seam_x = split_seam_column(state.list_split_pct, term_size.width);
            // Seam hit always wins — a click on the seam column starts a
            // drag, never a row select. Even if the seam happens to overlap
            // a valid row position, the resize affordance takes precedence.
            if near_seam(mouse.column, seam_x) {
                dispatch_manager(
                    state,
                    ManagerMessage::SetDragState(Some(DragState {
                        anchor_pct: state.list_split_pct,
                        anchor_x: mouse.column,
                    })),
                );
                return super::InputOutcome::Continue;
            }
            // Otherwise, treat as click-to-select if the click lands inside
            // the list pane's content area (excluding borders).
            if let Some(row) = list_content_row_index(state, mouse, term_size, seam_x)
                && let Some(selected) = state.index_of_row(row)
            {
                dispatch_manager(state, ManagerMessage::SelectListRow(selected));
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(anchor) = state.drag_state {
                let new_pct = split_pct_from_drag(
                    anchor.anchor_pct,
                    anchor.anchor_x,
                    mouse.column,
                    term_size.width,
                );
                dispatch_manager(state, ManagerMessage::SetListSplitPct(clamp_split(new_pct)));
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            dispatch_manager(state, ManagerMessage::SetDragState(None));
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
pub(crate) fn clickable_at(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
) -> bool {
    let _ = config;
    if term_size.width < MIN_DRAGGABLE_WIDTH {
        return false;
    }
    // The git-prompt URL row is clickable whenever a file-browser modal with a
    // resolved URL is open, regardless of stage.
    if file_browser_url_row_at(state, mouse, term_size) {
        return true;
    }
    if container_info_copyable_row_at(state, mouse, term_size) {
        return true;
    }
    match &state.stage {
        ManagerStage::Editor(editor) if editor.modal.is_none() => {
            editor_tab_at(mouse).is_some()
                || editor_mount_index_at(editor, mouse, term_size).is_some()
        }
        ManagerStage::Settings(settings)
            if settings.mounts.modal.is_none() && settings.env.modal.is_none() =>
        {
            settings_tab_at(mouse).is_some() || settings_trust_clickable(settings, mouse, term_size)
        }
        ManagerStage::List if state.list_modal.is_none() => {
            let seam_x = split_seam_column(state.list_split_pct, term_size.width);
            if near_seam(mouse.column, seam_x) {
                return false;
            }
            list_content_row_index(state, mouse, term_size, seam_x)
                .and_then(|row| state.index_of_row(row))
                .is_some()
        }
        _ => false,
    }
}

fn try_copy_container_info_value(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_ref() else {
        return false;
    };
    let area = modal_outer_rect(
        &Modal::ContainerInfo {
            state: info.clone(),
        },
        term_size,
    );
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
    let area = modal_outer_rect(modal, term_size);
    jackin_tui::components::container_info_copy_payload_at(area, info, mouse.column, mouse.row)
        .is_some()
}

/// Brighten the hovered copyable row in the Debug info dialog (link hover cue),
/// mirroring the launch cockpit. No-op unless that modal is open.
fn update_container_info_hover(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) {
    let Some(modal @ Modal::ContainerInfo { .. }) = state.list_modal.as_ref() else {
        return;
    };
    let area = modal_outer_rect(modal, term_size);
    let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut() else {
        return;
    };
    let hovered = jackin_tui::components::container_info_copy_payload_at(
        area,
        info,
        mouse.column,
        mouse.row,
    )
    .map(|(row, _)| row);
    info.set_hovered_row(hovered);
}

/// Whether the pointer is inside the Settings → Trust content area (a click
/// there selects a row / activates scroll). Shared by the click handler and the
/// hover cue.
fn settings_trust_clickable(
    settings: &crate::console::tui::state::SettingsState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    settings.active_tab == SettingsTab::Trust
        && settings.mounts.modal.is_none()
        && point_in(mouse, settings_content_area(settings, term_size))
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
    let modal_area = modal_outer_rect(modal, term_size);
    fb_state.url_row_hit(modal_area, mouse.column, mouse.row)
}

/// Track the list row under the pointer so the renderer can lift its
/// background, mirroring the tab-hover cue. Cleared when off the list pane,
/// over the seam, or when a list modal is open.
fn update_list_row_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    state.hovered_list_row =
        if matches!(state.stage, ManagerStage::List) && state.list_modal.is_none() {
            let seam_x = split_seam_column(state.list_split_pct, term_size.width);
            if near_seam(mouse.column, seam_x) {
                None
            } else {
                list_content_row_index(state, mouse, term_size, seam_x)
                    .filter(|row| state.index_of_row(*row).is_some())
            }
        } else {
            None
        };
}

/// Track the hovered row on the editor Mounts tab and the Settings Trust tab so
/// their renderers can lift it, mirroring the tab/list hover cue. Cleared off
/// the relevant content area.
fn update_row_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.hovered_mount_row = editor_mount_index_at(editor, mouse, term_size);
        }
        ManagerStage::Settings(settings) => {
            settings.trust.hovered = settings_trust_row_at(settings, mouse, term_size);
        }
        _ => {}
    }
}

/// Trust-tab pending-entry index under the pointer, or `None`. Matches the
/// click handler's geometry: skip the column header (content line 0) and add
/// the rendered vertical scroll, same as `try_select_settings_trust_row`.
fn settings_trust_row_at(
    settings: &crate::console::tui::state::SettingsState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    if settings.active_tab != SettingsTab::Trust || settings.mounts.modal.is_some() {
        return None;
    }
    let area = settings_content_area(settings, term_size);
    if !point_in(mouse, area) {
        return None;
    }
    // Content line 0 is the column header; pending entries start at line 1.
    // Add the rendered `trust.scroll_y` so a scrolled list maps to the right
    // entry (render_scrollable_block draws header + entries scrolled together).
    let line =
        usize::from(mouse.row.saturating_sub(area.y + 1)) + usize::from(settings.trust.scroll_y);
    let row = line.checked_sub(1)?;
    (row < settings.trust.pending.len()).then_some(row)
}

fn try_select_editor_tab(state: &mut ManagerState<'_>, mouse: MouseEvent) -> bool {
    let ManagerStage::Editor(editor) = &state.stage else {
        return false;
    };
    if editor.modal.is_some() {
        return false;
    }

    let Some(tab) = editor_tab_at(mouse) else {
        return false;
    };

    dispatch_manager(state, ManagerMessage::SelectEditorTab(tab));
    true
}

fn editor_tab_at(mouse: MouseEvent) -> Option<EditorTab> {
    let labels: Vec<&str> = EditorTab::ALL.iter().map(|tab| tab.label()).collect();
    let idx = tab_cell_at(mouse, &labels)?;
    EditorTab::ALL.get(idx).copied()
}

/// Index of the tab cell under `mouse`, or `None` when the pointer is outside
/// the strip rows. Geometry comes from the shared `jackin_tui::lay_out_tabs`
/// (` label ` cell, one-column gap, from col 0) so the host console's hit-test
/// and the in-container multiplexer's stay in lock-step.
fn tab_cell_at(mouse: MouseEvent, labels: &[&str]) -> Option<usize> {
    tab_cell_at_position(mouse.row, mouse.column, labels)
}

/// Repaint the hovered tab index on mouse motion so the strip lifts under the
/// pointer like the in-container multiplexer tabs. A motion off the strip
/// clears the highlight (`tab_cell_at` returns `None`).
fn update_tab_hover(state: &mut ManagerState<'_>, mouse: MouseEvent) {
    match &mut state.stage {
        ManagerStage::Editor(editor) if editor.modal.is_none() => {
            let labels: Vec<&str> = EditorTab::ALL.iter().map(|tab| tab.label()).collect();
            editor.hovered_tab = tab_cell_at(mouse, &labels);
        }
        ManagerStage::Settings(settings)
            if settings.mounts.modal.is_none() && settings.env.modal.is_none() =>
        {
            let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
            settings.hovered_tab = tab_cell_at(mouse, &labels);
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

    let Some(tab) = settings_tab_at(mouse) else {
        return false;
    };
    dispatch_manager(state, ManagerMessage::SelectSettingsTab(tab));
    true
}

fn settings_tab_at(mouse: MouseEvent) -> Option<SettingsTab> {
    let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    let idx = tab_cell_at(mouse, &labels)?;
    SettingsTab::ALL.get(idx).copied()
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
    let area = settings_content_area(settings, term_size);
    if !point_in(mouse, area) {
        return false;
    }
    // Content line 0 is the column header; pending entries start at line 1.
    // Add the rendered `trust.scroll_y` (same offset the scrollable block was
    // drawn with) so clicks land on the entry actually under the pointer.
    let line =
        usize::from(mouse.row.saturating_sub(area.y + 1)) + usize::from(settings.trust.scroll_y);
    if let Some(row) = line.checked_sub(1)
        && row < settings.trust.pending.len()
    {
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
    editor: &crate::console::tui::state::EditorState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    if editor.active_tab != EditorTab::Mounts || editor.modal.is_some() {
        return None;
    }
    let area = editor_scroll_area(editor, term_size).area;
    if mouse.column <= area.x
        || mouse.column >= area.x.saturating_add(area.width).saturating_sub(1)
        || mouse.row <= area.y
        || mouse.row >= area.y.saturating_add(area.height).saturating_sub(1)
    {
        return None;
    }
    // The Mounts list is drawn through `render_scrollable_block` scrolled by
    // `tab_scroll_y`; convert the viewport row to a full-content visual row so
    // the lookup matches what the operator sees after scrolling.
    let row = usize::from(mouse.row.saturating_sub(area.y + 1)) + usize::from(editor.tab_scroll_y);
    editor_mount_index_at_visual_row(editor, row)
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

fn editor_mount_index_at_visual_row(
    editor: &crate::console::tui::state::EditorState<'_>,
    row: usize,
) -> Option<usize> {
    if row == 0 {
        return None;
    }

    let mut visual = 1usize;
    for (index, mount) in editor.pending.mounts.iter().enumerate() {
        if row == visual {
            return Some(index);
        }
        visual += 1;
        if mount.src != mount.dst {
            if row == visual {
                return Some(index);
            }
            visual += 1;
        }
    }

    if !editor.pending.mounts.is_empty() {
        if row == visual {
            return None;
        }
        visual += 1;
    }

    (row == visual).then_some(editor.pending.mounts.len())
}

#[allow(clippy::items_after_statements, clippy::too_many_lines)]
fn try_drag_horizontal_scrollbar(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
) -> bool {
    match &mut state.stage {
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return false;
            }
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                return false;
            };
            if drag_scrollbar(
                &mut state.list_mounts_scroll_x,
                mouse,
                areas.workspace.area,
                areas.workspace.content_width,
            ) {
                state.list_scroll_focus =
                    workspace_list_scroll_focus_plan(false, true, true, false, false, false)
                        .scroll_focus;
                return true;
            }
            if drag_scrollbar(
                &mut state.list_global_mounts_scroll_x,
                mouse,
                areas.global.area,
                areas.global.content_width,
            ) {
                state.list_scroll_focus =
                    workspace_list_scroll_focus_plan(false, true, false, true, false, false)
                        .scroll_focus;
                return true;
            }
            if let Some(role) = areas.role_global
                && drag_scrollbar(
                    &mut state.list_role_global_mounts_scroll_x,
                    mouse,
                    role.area,
                    role.content_width,
                )
            {
                state.list_scroll_focus =
                    workspace_list_scroll_focus_plan(false, true, false, false, true, false)
                        .scroll_focus;
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
                drag_scrollbar(
                    &mut editor.workspace_mounts_scroll_x,
                    mouse,
                    workspace.area,
                    workspace.content_width,
                )
            } else {
                let content_area = editor_content_area(editor, term_size);
                drag_scrollbar(
                    &mut editor.tab_scroll_x,
                    mouse,
                    content_area,
                    editor.tab_content_width,
                )
            };
            if dragged {
                let plan = editor_scroll_focus_plan(
                    editor.active_tab,
                    false,
                    editor.active_tab == EditorTab::Mounts,
                    editor.active_tab != EditorTab::Mounts,
                );
                editor.workspace_mounts_scroll_focused = plan.workspace_mounts_scroll_focused;
                editor.tab_content_scroll_focused = plan.tab_content_scroll_focused;
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
            drag_scrollbar(
                &mut settings.mounts.scroll_x,
                mouse,
                Rect {
                    x: 0,
                    y: SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT,
                    width: term_size.width,
                    height: term_size.height.saturating_sub(
                        SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT + LIST_FOOTER_HEIGHT,
                    ),
                },
                global_mount_rows_content_width(
                    &settings.mounts.pending,
                    &settings.mounts.mount_info_cache,
                ),
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
    config: Option<&crate::config::AppConfig>,
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
            let in_left_pane = point_in(mouse, left_pane_area);
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
                        point_in(mouse, areas.workspace.area),
                        point_in(mouse, areas.global.area) && areas.global.area.height > 0,
                        areas.role_global.is_some_and(|r| point_in(mouse, r.area)),
                        areas.roles.is_some_and(|r| point_in(mouse, r.area)),
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
                    point_in(mouse, area.area)
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
                    let content_area = editor_content_area(editor, term_size);
                    point_in(mouse, content_area)
                };
                editor_scroll_focus_plan(
                    editor.active_tab,
                    editor.modal.is_some(),
                    false,
                    in_tab_content,
                )
            };
            editor.workspace_mounts_scroll_focused = plan.workspace_mounts_scroll_focused;
            editor.tab_content_scroll_focused = plan.tab_content_scroll_focused;
            // Clicking the content block transfers interaction focus into it —
            // same as Tab/↓ — so the green border and ▸ appear in the same frame.
            let clicked_content =
                plan.workspace_mounts_scroll_focused || plan.tab_content_scroll_focused;
            if clicked_content && editor.tab_bar_focused {
                editor.tab_bar_focused = false;
            }
        }
        ManagerStage::Settings(settings) => {
            let modal_open = settings_modal_open(settings);
            let in_content = if modal_open {
                false
            } else {
                point_in(mouse, settings_content_area(settings, term_size))
            };
            let plan = settings_scroll_focus_plan(settings.active_tab, modal_open, in_content);
            settings.mounts.scroll_focused = plan.mounts;
            settings.env.scroll_focused = plan.env;
            settings.auth.scroll_focused = plan.auth;
            settings.trust.scroll_focused = plan.trust;
            // Clicking the content block transfers interaction focus into it —
            // same as Tab/↓ — so the green border and ▸ appear in the same frame.
            if in_content && settings.tab_bar_focused {
                settings.tab_bar_focused = false;
            }
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
}

/// The content area below the header + tab strip in Settings/Editor stages.
const fn settings_content_area(
    settings: &crate::console::tui::state::SettingsState<'_>,
    term_size: Rect,
) -> Rect {
    tabbed_content_area(term_size, settings.cached_footer_h)
}

const fn point_in(mouse: MouseEvent, area: Rect) -> bool {
    point_in_rect(mouse.column, mouse.row, area)
}

#[derive(Clone, Copy)]
struct ScrollArea {
    area: Rect,
    content_width: usize,
}

fn drag_scrollbar_axis(
    axis: ScrollbarAxis,
    value: &mut u16,
    mouse: MouseEvent,
    area: Rect,
    content_len: usize,
) -> bool {
    let Some(offset) = scrollbar_drag_offset(axis, area, content_len, mouse.column, mouse.row)
    else {
        return false;
    };
    *value = offset;
    true
}

fn drag_scrollbar(value: &mut u16, mouse: MouseEvent, area: Rect, content_width: usize) -> bool {
    drag_scrollbar_axis(ScrollbarAxis::Horizontal, value, mouse, area, content_width)
}

fn drag_vertical_scrollbar(
    value: &mut u16,
    mouse: MouseEvent,
    area: Rect,
    content_height: usize,
) -> bool {
    drag_scrollbar_axis(ScrollbarAxis::Vertical, value, mouse, area, content_height)
}

const fn settings_modal_open(settings: &crate::console::tui::state::SettingsState<'_>) -> bool {
    settings.error_popup.is_some()
        || settings.mounts.modal.is_some()
        || settings.env.modal.is_some()
        || settings.auth.modal.is_some()
}

fn try_drag_vertical_scrollbar(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
) -> bool {
    match &mut state.stage {
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return false;
            }
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                return false;
            };
            let Some(focus) = state.list_scroll_focus else {
                return false;
            };
            match focus {
                MountScrollFocus::Workspace => drag_vertical_scrollbar(
                    &mut state.list_mounts_scroll_y,
                    mouse,
                    areas.workspace.area,
                    areas.workspace.content_height,
                ),
                MountScrollFocus::Global => drag_vertical_scrollbar(
                    &mut state.list_global_mounts_scroll_y,
                    mouse,
                    areas.global.area,
                    areas.global.content_height,
                ),
                MountScrollFocus::RoleGlobal => areas.role_global.is_some_and(|area| {
                    drag_vertical_scrollbar(
                        &mut state.list_role_global_mounts_scroll_y,
                        mouse,
                        area.area,
                        area.content_height,
                    )
                }),
                MountScrollFocus::Roles => areas.roles.is_some_and(|area| {
                    drag_vertical_scrollbar(
                        &mut state.list_roles_scroll_y,
                        mouse,
                        area.area,
                        area.content_height,
                    )
                }),
            }
        }
        ManagerStage::Editor(editor) => {
            if editor.modal.is_some() {
                return false;
            }
            let area = editor_content_area(editor, term_size);
            let content_height = editor_content_height(editor);
            drag_vertical_scrollbar(&mut editor.tab_scroll_y, mouse, area, content_height)
        }
        ManagerStage::Settings(settings) => {
            if settings_modal_open(settings) {
                return false;
            }
            let area = settings_content_area(settings, term_size);
            let content_height = match settings.active_tab {
                SettingsTab::General => 0,
                SettingsTab::Mounts => mounts_content_height(settings),
                SettingsTab::Environments => env_content_height(settings),
                SettingsTab::Auth => auth_content_height(settings),
                SettingsTab::Trust => trust_content_height(settings),
            };
            match settings.active_tab {
                SettingsTab::General => false,
                SettingsTab::Mounts => drag_vertical_scrollbar(
                    &mut settings.mounts.scroll_y,
                    mouse,
                    area,
                    content_height,
                ),
                SettingsTab::Environments => {
                    drag_vertical_scrollbar(&mut settings.env.scroll_y, mouse, area, content_height)
                }
                SettingsTab::Auth => drag_vertical_scrollbar(
                    &mut settings.auth.scroll_y,
                    mouse,
                    area,
                    content_height,
                ),
                SettingsTab::Trust => drag_vertical_scrollbar(
                    &mut settings.trust.scroll_y,
                    mouse,
                    area,
                    content_height,
                ),
            }
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
fn scroll_active_panel(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
    delta: i16,
) {
    match &mut state.stage {
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return;
            }
            update_scroll_focus(state, mouse, term_size, config);
            if state.list_names_focused {
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
                apply_horizontal_scroll(&mut state.list_names_scroll_x, delta, area, content_width);
                return;
            }
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                state.list_scroll_focus =
                    workspace_list_scroll_focus_plan(false, false, false, false, false, false)
                        .scroll_focus;
                return;
            };
            let Some(focus) = state.list_scroll_focus else {
                return;
            };
            let area_info = match focus {
                MountScrollFocus::Workspace => Some(areas.workspace),
                MountScrollFocus::Global => Some(areas.global),
                MountScrollFocus::RoleGlobal => areas.role_global,
                MountScrollFocus::Roles => areas.roles,
            };
            let Some(area_info) = area_info else {
                return;
            };
            apply_horizontal_scroll(
                state.list_scroll_x_mut(focus),
                delta,
                area_info.area,
                area_info.content_width,
            );
        }
        ManagerStage::Editor(editor) => {
            if editor.modal.is_some() {
                return;
            }
            if editor.active_tab != EditorTab::Mounts {
                let area = editor_content_area(editor, term_size);
                let in_scrollable_content = point_in(mouse, area)
                    && is_horizontally_scrollable(area, editor.tab_content_width);
                let plan = editor_scroll_focus_plan(
                    editor.active_tab,
                    false,
                    false,
                    in_scrollable_content,
                );
                editor.workspace_mounts_scroll_focused = plan.workspace_mounts_scroll_focused;
                editor.tab_content_scroll_focused = plan.tab_content_scroll_focused;
                if plan.tab_content_scroll_focused {
                    apply_horizontal_scroll(
                        &mut editor.tab_scroll_x,
                        delta,
                        area,
                        editor.tab_content_width,
                    );
                }
                return;
            }
            let area = editor_scroll_area(editor, term_size);
            let in_scrollable_workspace = point_in(mouse, area.area)
                && is_horizontally_scrollable(area.area, area.content_width);
            let plan =
                editor_scroll_focus_plan(editor.active_tab, false, in_scrollable_workspace, false);
            editor.workspace_mounts_scroll_focused = plan.workspace_mounts_scroll_focused;
            editor.tab_content_scroll_focused = plan.tab_content_scroll_focused;
            if plan.workspace_mounts_scroll_focused {
                apply_horizontal_scroll(
                    &mut editor.workspace_mounts_scroll_x,
                    delta,
                    area.area,
                    area.content_width,
                );
            }
        }
        ManagerStage::Settings(settings) => {
            if settings_modal_open(settings) {
                return;
            }
            // Hover-scroll: fire on whichever block the cursor is over.
            let content_area = settings_content_area(settings, term_size);
            if !point_in(mouse, content_area) {
                return;
            }
            match settings.active_tab {
                SettingsTab::Mounts => {
                    apply_horizontal_scroll(
                        &mut settings.mounts.scroll_x,
                        delta,
                        content_area,
                        global_mount_rows_content_width(
                            &settings.mounts.pending,
                            &settings.mounts.mount_info_cache,
                        ),
                    );
                }
                SettingsTab::Trust => {
                    let cw = jackin_console::tui::screens::settings::update::trust_content_width(
                        &settings.trust,
                    );
                    apply_horizontal_scroll(&mut settings.trust.scroll_x, delta, content_area, cw);
                }
                _ => {}
            }
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
}

/// Dispatch a vertical scroll event to whichever content block the mouse is over.
/// Horizontal-only blocks (List view mounts) are silently ignored here —
/// their scroll is only driven by left/right events via `scroll_active_panel`.
#[allow(clippy::missing_const_for_fn)]
#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
fn scroll_active_panel_vertical(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
    delta: i16,
) {
    match &mut state.stage {
        ManagerStage::Settings(settings) => {
            if settings_modal_open(settings) {
                return;
            }
            let content_area = settings_content_area(settings, term_size);
            if !point_in(mouse, content_area) {
                return;
            }
            match settings.active_tab {
                // General has no scrollable content; empty arm is intentional.
                SettingsTab::General => {}
                SettingsTab::Mounts => {
                    let content_height = mounts_content_height(settings);
                    apply_vertical_scroll(
                        &mut settings.mounts.scroll_y,
                        delta,
                        content_area,
                        content_height,
                    );
                }
                SettingsTab::Environments => {
                    let content_height = env_content_height(settings);
                    apply_vertical_scroll(
                        &mut settings.env.scroll_y,
                        delta,
                        content_area,
                        content_height,
                    );
                }
                SettingsTab::Trust => {
                    let content_height = trust_content_height(settings);
                    apply_vertical_scroll(
                        &mut settings.trust.scroll_y,
                        delta,
                        content_area,
                        content_height,
                    );
                }
                SettingsTab::Auth => {
                    let content_height = auth_content_height(settings);
                    apply_vertical_scroll(
                        &mut settings.auth.scroll_y,
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
            let area = editor_content_area(editor, term_size);
            if !point_in(mouse, area) {
                return;
            }
            let content_height = editor_content_height(editor);
            apply_vertical_scroll(&mut editor.tab_scroll_y, delta, area, content_height);
        }
        ManagerStage::List => {
            if state.list_modal.is_some() {
                return;
            }
            update_scroll_focus(state, mouse, term_size, config);
            // Scroll the focused block vertically.
            match state.list_scroll_focus {
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
    config: Option<&crate::config::AppConfig>,
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

const fn editor_content_area(
    editor: &crate::console::tui::state::EditorState<'_>,
    term_size: Rect,
) -> Rect {
    tabbed_content_area(term_size, editor.cached_footer_h)
}

fn editor_scroll_area(
    editor: &crate::console::tui::state::EditorState<'_>,
    term_size: Rect,
) -> ScrollArea {
    ScrollArea {
        area: editor_content_area(editor, term_size),
        content_width: workspace_mounts_content_width_with_cache(
            editor.pending.mounts.as_slice(),
            &editor.mount_info_cache,
        ),
    }
}

const fn editor_content_height(editor: &crate::console::tui::state::EditorState<'_>) -> usize {
    editor.tab_content_height
}

fn global_mount_rows_content_width(
    rows: &[crate::config::GlobalMountRow],
    cache: &crate::console::tui::state::MountInfoCache,
) -> usize {
    // Settings mounts render Destination + Mode + Type columns, unlike the
    // sidebar's Destination + Mode variant.
    settings_global_mounts_content_width_with_cache(rows, cache)
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
    let modal_area = modal_outer_rect(modal, term_size);
    let Some(url) = fb_state.url_to_open_on_click(modal_area, mouse.column, mouse.row) else {
        return false;
    };
    state.request_effect(ManagerEffect::OpenUrl(url));
    true
}

/// Return the logical list row the mouse is over, or `None` if the click
/// falls outside the list pane's content area.
///
/// Mirrors the layout from `render::render` + `render::render_list_body`:
///   - Chrome: `[header (3 rows)][body][footer (2 rows)]`
///   - Body is horizontally split; left column hosts the workspace list.
///   - The list itself sits inside a bordered block — row 0 of list
///     items is at y = header + 1 (the +1 skips the top border).
///
/// Returns `Some(row)` only when:
///   - `mouse.column` is inside `[1, seam_x - 1]` (left pane interior,
///     i.e. excluding both the left border and the seam column itself)
///   - `mouse.row` is inside `[header + 1, body_end - 1]` (body interior,
///     excluding the top and bottom border rows)
///   - The computed index maps to a valid `ManagerListRow`. See
///     `ManagerListRow` docs for row layout.
fn list_content_row_index(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    seam_x: u16,
) -> Option<ManagerListRow> {
    let idx = list_content_visual_index_at(mouse.column, mouse.row, term_size, seam_x)?;
    state.row_at_visual_index(idx)
}

#[cfg(test)]
mod mouse_drag_tests;
