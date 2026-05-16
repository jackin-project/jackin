//! Mouse event handling for the workspace manager: list/details seam drag,
//! click-to-select in the list pane, and `FileBrowser` URL-click fallthrough.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::super::super::widgets::file_browser::FileBrowserState;
use super::super::render::global_mounts::trust_content_width;
use super::super::render::list::{
    global_mounts_block_height, global_mounts_content_height, global_mounts_content_width,
    mount_block_height, workspace_mounts_content_height, workspace_mounts_content_width,
};
use super::super::render::{
    is_scrollable, max_scroll_offset, scroll_viewport_height, scroll_viewport_width,
};
use super::super::state::{
    DragState, EditorTab, FieldFocus, ManagerListRow, ManagerStage, ManagerState, Modal,
    MountScrollFocus, SettingsTab, clamp_split,
};

/// Minimum terminal width (in columns) at which the list/details seam is
/// draggable. Below this, the 20/80 clamp bounds leave the right pane
/// implausibly narrow for meaningful interaction — silently ignore mouse
/// events rather than produce an unusable layout.
const MIN_DRAGGABLE_WIDTH: u16 = 40;
/// Half-width of the seam hit-region. A Down event lands within ±1 column
/// of the computed seam to initiate drag. Narrow enough that operators
/// don't accidentally start a drag while clicking in either pane.
const SEAM_HIT_SLACK: u16 = 1;

/// Height of the header chunk in the list-view chrome. Mirrors
/// `Constraint::Length(3)` in `render::render`. Used by mouse hit-testing
/// to convert a terminal row into a list item index.
const LIST_HEADER_HEIGHT: u16 = 3;
/// Height of the footer chunk in the list-view chrome. Mirrors
/// `Constraint::Length(2)` in `render::render`.
const LIST_FOOTER_HEIGHT: u16 = 2;
const EDITOR_HEADER_HEIGHT: u16 = 3;
const EDITOR_TAB_STRIP_HEIGHT: u16 = 2;
const MOUSE_HORIZONTAL_SCROLL_STEP: u16 = 1;
const MOUSE_VERTICAL_SCROLL_STEP: i16 = 1;

/// Dispatch a mouse event into the workspace manager's list view. Drives
/// the mouse-draggable seam between the list pane and the details pane.
///
/// Behaviour:
/// - On `ManagerStage::List` with no list-level modal open: drives the
///   list/details seam drag (anchor + drag + release) and click-to-select.
/// - On `ManagerStage::Editor` / `CreatePrelude` with a `FileBrowser` modal
///   whose git-prompt overlay is active AND has a resolved URL: a
///   `Down(Left)` on the URL row fires `open::that_detached` best-effort.
/// - Ignores everything when the terminal is narrower than
///   [`MIN_DRAGGABLE_WIDTH`] — drag bounds would be absurd.
/// - All other events are ignored.
///
/// The caller (run-loop in `src/console/mod.rs`) is responsible for
/// passing the current `terminal.size()?` as `term_size` so the handler
/// can compute the seam column as `term_size.width * list_split_pct / 100`.
pub fn handle_mouse(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    handle_mouse_with_config(state, mouse, term_size, None);
}

#[allow(clippy::too_many_lines)]
pub fn handle_mouse_with_config(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
) {
    if term_size.width < MIN_DRAGGABLE_WIDTH {
        return;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_editor_tab(state, mouse)
    {
        return;
    }
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_settings_tab(state, mouse)
    {
        return;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        update_scroll_focus(state, mouse, term_size, config);
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            if try_drag_horizontal_scrollbar(state, mouse, term_size, config) =>
        {
            return;
        }
        MouseEventKind::ScrollLeft => {
            scroll_active_panel(
                state,
                mouse,
                term_size,
                config,
                -(MOUSE_HORIZONTAL_SCROLL_STEP as i16),
            );
            return;
        }
        MouseEventKind::ScrollRight => {
            scroll_active_panel(
                state,
                mouse,
                term_size,
                config,
                MOUSE_HORIZONTAL_SCROLL_STEP as i16,
            );
            return;
        }
        MouseEventKind::ScrollUp => {
            scroll_active_panel_vertical(state, mouse, term_size, -MOUSE_VERTICAL_SCROLL_STEP);
            return;
        }
        MouseEventKind::ScrollDown => {
            scroll_active_panel_vertical(state, mouse, term_size, MOUSE_VERTICAL_SCROLL_STEP);
            return;
        }
        _ => {}
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_editor_mount_row(state, mouse, term_size)
    {
        return;
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_select_settings_trust_row(state, mouse, term_size)
    {
        return;
    }

    // Editor / CreatePrelude file-browser URL click: only on Down(Left),
    // only when the modal is a FileBrowser with a resolved git URL.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_open_file_browser_git_url(state, mouse, term_size)
    {
        return;
    }

    // Stage + modal gate for the list-view seam drag. Only the List view
    // participates in drag; the Editor, CreatePrelude and ConfirmDelete
    // stages only observe the URL-click path above.
    if !matches!(state.stage, ManagerStage::List) {
        return;
    }
    if state.list_modal.is_some() {
        return;
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let seam_x = seam_column(state.list_split_pct, term_size.width);
            // Seam hit always wins — a click on the seam column starts a
            // drag, never a row select. Even if the seam happens to overlap
            // a valid row position, the resize affordance takes precedence.
            if near_seam(mouse.column, seam_x) {
                state.drag_state = Some(DragState {
                    anchor_pct: state.list_split_pct,
                    anchor_x: mouse.column,
                });
                return;
            }
            // Otherwise, treat as click-to-select if the click lands inside
            // the list pane's content area (excluding borders).
            if let Some(row) = list_content_row_index(state, mouse, term_size, seam_x) {
                state.inline_role_picker = None;
                let selected = row.to_screen_index(state.workspaces.len());
                if selected != state.selected {
                    state.reset_list_scroll();
                    state.selected = selected;
                    state.inline_agent_picker = None;
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(anchor) = state.drag_state {
                let new_pct = pct_from_drag(anchor, mouse.column, term_size.width);
                state.list_split_pct = clamp_split(new_pct);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            state.drag_state = None;
        }
        _ => {}
    }
}

fn try_select_editor_tab(state: &mut ManagerState<'_>, mouse: MouseEvent) -> bool {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return false;
    };
    if editor.modal.is_some() {
        return false;
    }

    let Some(tab) = editor_tab_at(mouse) else {
        return false;
    };

    let was_secrets = editor.active_tab == EditorTab::Secrets;
    editor.active_tab = tab;
    editor.active_field = FieldFocus::Row(0);
    editor.workspace_mounts_scroll_focused = false;
    if editor.active_tab != EditorTab::Auth {
        editor.auth_selected_kind = None;
    }
    if was_secrets && editor.active_tab != EditorTab::Secrets {
        editor.unmasked_rows.clear();
        editor.secrets_expanded.clear();
    }
    true
}

fn editor_tab_at(mouse: MouseEvent) -> Option<EditorTab> {
    if mouse.row < EDITOR_HEADER_HEIGHT
        || mouse.row >= EDITOR_HEADER_HEIGHT.saturating_add(EDITOR_TAB_STRIP_HEIGHT)
    {
        return None;
    }

    let mut x = 0u16;
    for &(tab, label) in super::super::render::editor::EDITOR_TAB_LABELS {
        let width = label.len() as u16 + 2;
        if mouse.column >= x && mouse.column < x.saturating_add(width) {
            return Some(tab);
        }
        x = x.saturating_add(width + 1);
    }
    None
}

fn try_select_settings_tab(state: &mut ManagerState<'_>, mouse: MouseEvent) -> bool {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return false;
    };
    if settings.mounts.modal.is_some() || settings.env.modal.is_some() {
        return false;
    }

    let Some(tab) = settings_tab_at(mouse) else {
        return false;
    };
    settings.active_tab = tab;
    true
}

fn settings_tab_at(mouse: MouseEvent) -> Option<SettingsTab> {
    if mouse.row < EDITOR_HEADER_HEIGHT
        || mouse.row >= EDITOR_HEADER_HEIGHT.saturating_add(EDITOR_TAB_STRIP_HEIGHT)
    {
        return None;
    }
    let mut x = 0u16;
    for tab in SettingsTab::ALL {
        let width = tab.label().len() as u16 + 2;
        if mouse.column >= x && mouse.column < x.saturating_add(width) {
            return Some(tab);
        }
        x = x.saturating_add(width + 1);
    }
    None
}

/// Click inside the Trust block selects the row and activates the block for scrolling.
fn try_select_settings_trust_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return false;
    };
    if settings.active_tab != SettingsTab::Trust || settings.mounts.modal.is_some() {
        return false;
    }
    let area = settings_content_area(term_size);
    if !point_in(mouse, area) {
        return false;
    }
    // Row 0 is the header; rows 1.. are trust entries.
    let clicked_row = usize::from(mouse.row.saturating_sub(area.y + 1));
    if clicked_row < settings.trust.pending.len() {
        settings.trust.selected = clicked_row;
    }
    settings.trust.scroll_focused = true;
    true
}

fn try_select_editor_mount_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return false;
    };
    if editor.active_tab != EditorTab::Mounts || editor.modal.is_some() {
        return false;
    }

    let area_info = editor_scroll_area(editor, term_size);
    let area = area_info.area;
    if mouse.column <= area.x
        || mouse.column >= area.x.saturating_add(area.width).saturating_sub(1)
        || mouse.row <= area.y
        || mouse.row >= area.y.saturating_add(area.height).saturating_sub(1)
    {
        return false;
    }

    let row = usize::from(mouse.row.saturating_sub(area.y + 1));
    let Some(index) = editor_mount_index_at_visual_row(editor, row) else {
        return false;
    };
    editor.active_field = FieldFocus::Row(index);
    editor.workspace_mounts_scroll_focused =
        is_scrollable(area_info.content_width, scroll_viewport_width(area));
    true
}

fn editor_mount_index_at_visual_row(
    editor: &super::super::state::EditorState<'_>,
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

fn try_drag_horizontal_scrollbar(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
) -> bool {
    match &mut state.stage {
        ManagerStage::List => {
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                return false;
            };
            if drag_scrollbar(
                &mut state.list_mounts_scroll_x,
                mouse,
                areas.workspace.area,
                areas.workspace.content_width,
            ) {
                state.list_scroll_focus = Some(MountScrollFocus::Workspace);
                return true;
            }
            if drag_scrollbar(
                &mut state.list_global_mounts_scroll_x,
                mouse,
                areas.global.area,
                areas.global.content_width,
            ) {
                state.list_scroll_focus = Some(MountScrollFocus::Global);
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
                state.list_scroll_focus = Some(MountScrollFocus::RoleGlobal);
                return true;
            }
            false
        }
        ManagerStage::Editor(editor) => {
            let workspace = editor_scroll_area(editor, term_size);
            let dragged = drag_scrollbar(
                &mut editor.workspace_mounts_scroll_x,
                mouse,
                workspace.area,
                workspace.content_width,
            );
            if dragged {
                editor.workspace_mounts_scroll_focused = true;
            }
            dragged
        }
        ManagerStage::Settings(settings) => {
            if settings.active_tab != SettingsTab::Mounts {
                return false;
            }
            drag_scrollbar(
                &mut settings.mounts.scroll_x,
                mouse,
                Rect {
                    x: 0,
                    y: EDITOR_HEADER_HEIGHT + EDITOR_TAB_STRIP_HEIGHT,
                    width: term_size.width,
                    height: term_size.height.saturating_sub(
                        EDITOR_HEADER_HEIGHT + EDITOR_TAB_STRIP_HEIGHT + LIST_FOOTER_HEIGHT,
                    ),
                },
                global_mount_rows_content_width(&settings.mounts.pending),
            )
        }
        ManagerStage::CreatePrelude(_) | ManagerStage::ConfirmDelete { .. } => false,
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
            let seam_x = seam_column(state.list_split_pct, term_size.width);
            let left_pane_area = Rect {
                x: 0,
                y: LIST_HEADER_HEIGHT,
                width: seam_x,
                height: term_size
                    .height
                    .saturating_sub(LIST_HEADER_HEIGHT + LIST_FOOTER_HEIGHT),
            };
            if point_in(mouse, left_pane_area) {
                // Click in left pane: activate left pane, clear right focus.
                state.list_names_focused = true;
                state.list_scroll_focus = None;
                return;
            }
            state.list_names_focused = false;

            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                state.list_scroll_focus = None;
                return;
            };
            state.list_scroll_focus = if point_in(mouse, areas.workspace.area)
                && (is_scrollable(
                    areas.workspace.content_width,
                    scroll_viewport_width(areas.workspace.area),
                ) || is_scrollable(
                    areas.workspace.content_height,
                    scroll_viewport_height(areas.workspace.area),
                )) {
                Some(MountScrollFocus::Workspace)
            } else if point_in(mouse, areas.global.area)
                && areas.global.area.height > 0
                && (is_scrollable(
                    areas.global.content_width,
                    scroll_viewport_width(areas.global.area),
                ) || is_scrollable(
                    areas.global.content_height,
                    scroll_viewport_height(areas.global.area),
                ))
            {
                Some(MountScrollFocus::Global)
            } else if areas.role_global.is_some_and(|r| {
                point_in(mouse, r.area)
                    && (is_scrollable(r.content_width, scroll_viewport_width(r.area))
                        || is_scrollable(r.content_height, scroll_viewport_height(r.area)))
            }) {
                Some(MountScrollFocus::RoleGlobal)
            } else if areas.roles.is_some_and(|r| {
                point_in(mouse, r.area)
                    && (is_scrollable(r.content_width, scroll_viewport_width(r.area))
                        || is_scrollable(r.content_height, scroll_viewport_height(r.area)))
            }) {
                Some(MountScrollFocus::Roles)
            } else {
                None
            };
        }
        ManagerStage::Editor(editor) => {
            if editor.active_tab == EditorTab::Mounts {
                if editor.modal.is_some() {
                    editor.workspace_mounts_scroll_focused = false;
                } else {
                    let area = editor_scroll_area(editor, term_size);
                    editor.workspace_mounts_scroll_focused = point_in(mouse, area.area)
                        && is_scrollable(area.content_width, scroll_viewport_width(area.area));
                }
                editor.tab_content_scroll_focused = false;
            } else {
                editor.workspace_mounts_scroll_focused = false;
                if editor.modal.is_some() {
                    editor.tab_content_scroll_focused = false;
                } else {
                    let content_area = settings_content_area(term_size);
                    let in_content = point_in(mouse, content_area);
                    let viewport_h = scroll_viewport_height(content_area);
                    editor.tab_content_scroll_focused =
                        in_content && is_scrollable(editor.tab_content_height, viewport_h);
                }
            }
        }
        ManagerStage::Settings(settings) => {
            let content_area = settings_content_area(term_size);
            let in_content = point_in(mouse, content_area);
            settings.mounts.scroll_focused =
                settings.active_tab == SettingsTab::Mounts && in_content;
            settings.trust.scroll_focused = settings.active_tab == SettingsTab::Trust && in_content;
        }
        ManagerStage::CreatePrelude(_) | ManagerStage::ConfirmDelete { .. } => {}
    }
}

/// The content area below the header + tab strip in Settings/Editor stages.
const fn settings_content_area(term_size: Rect) -> Rect {
    Rect {
        x: 0,
        y: EDITOR_HEADER_HEIGHT + EDITOR_TAB_STRIP_HEIGHT,
        width: term_size.width,
        height: term_size
            .height
            .saturating_sub(EDITOR_HEADER_HEIGHT + EDITOR_TAB_STRIP_HEIGHT),
    }
}

const fn point_in(mouse: MouseEvent, area: Rect) -> bool {
    mouse.column >= area.x
        && mouse.column < area.x.saturating_add(area.width)
        && mouse.row >= area.y
        && mouse.row < area.y.saturating_add(area.height)
}

#[derive(Clone, Copy)]
struct ScrollArea {
    area: Rect,
    content_width: usize,
    content_height: usize,
}

fn drag_scrollbar(value: &mut u16, mouse: MouseEvent, area: Rect, content_width: usize) -> bool {
    let viewport = scroll_viewport_width(area);
    if !is_scrollable(content_width, viewport) {
        return false;
    }
    let scrollbar_y = area.y + area.height.saturating_sub(1);
    let start_x = area.x + 1;
    let end_x = area.x + area.width.saturating_sub(2);
    if mouse.row != scrollbar_y || mouse.column < start_x || mouse.column > end_x {
        return false;
    }
    let max_position = usize::from(max_scroll_offset(content_width, viewport));
    let track = usize::from(end_x.saturating_sub(start_x)).max(1);
    let rel = usize::from(mouse.column.saturating_sub(start_x));
    *value = ((max_position * rel) / track).min(usize::from(u16::MAX)) as u16;
    true
}

fn scroll_active_panel(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
    delta: i16,
) {
    match &mut state.stage {
        ManagerStage::List => {
            update_scroll_focus(state, mouse, term_size, config);
            if state.list_names_focused {
                apply_scroll_delta(&mut state.list_names_scroll_x, delta);
                return;
            }
            let Some(areas) = list_scroll_areas(state, term_size, config) else {
                state.list_scroll_focus = None;
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
            if editor.active_tab != EditorTab::Mounts {
                editor.workspace_mounts_scroll_focused = false;
                return;
            }
            let area = editor_scroll_area(editor, term_size);
            if point_in(mouse, area.area)
                && is_scrollable(area.content_width, scroll_viewport_width(area.area))
            {
                editor.workspace_mounts_scroll_focused = true;
                apply_horizontal_scroll(
                    &mut editor.workspace_mounts_scroll_x,
                    delta,
                    area.area,
                    area.content_width,
                );
            } else {
                editor.workspace_mounts_scroll_focused = false;
            }
        }
        ManagerStage::Settings(settings) => {
            // Hover-scroll: fire on whichever block the cursor is over.
            let content_area = settings_content_area(term_size);
            if !point_in(mouse, content_area) {
                return;
            }
            match settings.active_tab {
                SettingsTab::Mounts => {
                    apply_horizontal_scroll(
                        &mut settings.mounts.scroll_x,
                        delta,
                        content_area,
                        global_mount_rows_content_width(&settings.mounts.pending),
                    );
                }
                SettingsTab::Trust => {
                    let cw = trust_content_width(settings);
                    apply_horizontal_scroll(&mut settings.trust.scroll_x, delta, content_area, cw);
                }
                _ => {}
            }
        }
        ManagerStage::CreatePrelude(_) | ManagerStage::ConfirmDelete { .. } => {}
    }
}

/// Dispatch a vertical scroll event to whichever content block the mouse is over.
/// Horizontal-only blocks (List view mounts) are silently ignored here —
/// their scroll is only driven by left/right events via `scroll_active_panel`.
fn scroll_active_panel_vertical(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    delta: i16,
) {
    match &mut state.stage {
        ManagerStage::Settings(settings) => {
            let content_area = settings_content_area(term_size);
            if !point_in(mouse, content_area) {
                return;
            }
            match settings.active_tab {
                SettingsTab::Mounts => {
                    apply_scroll_delta(&mut settings.mounts.scroll_y, delta);
                }
                SettingsTab::Environments => {
                    apply_scroll_delta(&mut settings.env.scroll_y, delta);
                }
                SettingsTab::Trust => {
                    apply_scroll_delta(&mut settings.trust.scroll_y, delta);
                }
                // Auth has too few rows to overflow — ignore vertical scroll.
                SettingsTab::Auth => {}
            }
        }
        ManagerStage::Editor(editor) => {
            match editor.active_tab {
                // General has 4 fixed rows — no vertical scroll needed.
                EditorTab::General => {}
                // Mounts, Roles, Secrets, Auth all use tab_scroll_y.
                EditorTab::Mounts | EditorTab::Roles | EditorTab::Secrets | EditorTab::Auth => {
                    apply_scroll_delta(&mut editor.tab_scroll_y, delta);
                }
            }
        }
        ManagerStage::List => {
            // Scroll the focused block vertically.
            match state.list_scroll_focus {
                Some(MountScrollFocus::Workspace) => {
                    apply_scroll_delta(&mut state.list_mounts_scroll_y, delta);
                }
                Some(MountScrollFocus::Global) => {
                    apply_scroll_delta(&mut state.list_global_mounts_scroll_y, delta);
                }
                Some(MountScrollFocus::RoleGlobal) => {
                    apply_scroll_delta(&mut state.list_role_global_mounts_scroll_y, delta);
                }
                Some(MountScrollFocus::Roles) => {
                    apply_scroll_delta(&mut state.list_roles_scroll_y, delta);
                }
                None => {}
            }
        }
        ManagerStage::CreatePrelude(_) | ManagerStage::ConfirmDelete { .. } => {}
    }
}

/// The render frame clamps stored values each draw, so no max is needed here.
const fn apply_scroll_delta(value: &mut u16, delta: i16) {
    *value = if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta as u16)
    };
}

fn apply_horizontal_scroll(value: &mut u16, delta: i16, area: Rect, content_width: usize) {
    let max = max_scroll_offset(content_width, scroll_viewport_width(area));
    let current = (*value).min(max);
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as u16)
    };
    *value = next.min(max);
}

struct ListScrollAreas {
    workspace: ScrollArea,
    global: ScrollArea,
    role_global: Option<ScrollArea>,
    roles: Option<ScrollArea>,
}

fn list_scroll_areas(
    state: &ManagerState<'_>,
    term_size: Rect,
    config: Option<&crate::config::AppConfig>,
) -> Option<ListScrollAreas> {
    let config = config?;
    let (right_x, right_w) = right_pane_dims(state.list_split_pct, term_size.width);
    let body_y = LIST_HEADER_HEIGHT;
    if state.is_current_dir_selected() {
        return Some(current_dir_scroll_areas(
            state, config, right_x, right_w, body_y,
        ));
    }
    let summary = state.selected_workspace_summary()?;
    let workspace = config.workspaces.get(&summary.name)?;
    let mounts_h = mount_block_height(workspace.mounts.as_slice());
    let picker_role = state
        .inline_role_picker
        .as_ref()
        .and_then(|picker| {
            picker
                .list_state
                .selected
                .and_then(|idx| picker.filtered.get(idx).cloned())
        })
        .or_else(|| {
            state
                .inline_agent_picker
                .as_ref()
                .map(|(role, _)| role.clone())
        });
    let global_rows = super::super::render::global_rows_for(config, picker_role.as_ref());
    let (global_mounts, role_global_mounts) =
        super::super::render::partition_mounts_by_scope(&global_rows);
    let global_h = if global_mounts.is_empty() {
        0
    } else {
        global_mounts_block_height(global_mounts.as_slice())
    };
    let role_global_h = if role_global_mounts.is_empty() {
        0
    } else {
        global_mounts_block_height(role_global_mounts.as_slice())
    };
    let agent_count = super::super::render::list::agents_block_agent_count(Some(workspace), config);
    let roles_h = super::super::render::list::agents_block_height(agent_count);
    let roles_y = body_y + 3 + mounts_h + global_h + role_global_h;
    let roles_content_w =
        super::super::render::list::agents_block_content_width(Some(workspace), config);
    let workspace_mounts_content_h = workspace_mounts_content_height(workspace.mounts.as_slice());
    let global_content_h = global_mounts_content_height(global_mounts.as_slice());
    let role_global_content_h = global_mounts_content_height(role_global_mounts.as_slice());
    let roles_content_h = 2 + agent_count;
    Some(ListScrollAreas {
        workspace: ScrollArea {
            area: Rect {
                x: right_x,
                y: body_y + 3,
                width: right_w,
                height: mounts_h,
            },
            content_width: workspace_mounts_content_width(workspace.mounts.as_slice()),
            content_height: workspace_mounts_content_h,
        },
        global: ScrollArea {
            area: Rect {
                x: right_x,
                y: body_y + 3 + mounts_h,
                width: right_w,
                height: global_h,
            },
            content_width: global_mounts_content_width(global_mounts.as_slice()),
            content_height: global_content_h,
        },
        role_global: (role_global_h > 0).then(|| ScrollArea {
            area: Rect {
                x: right_x,
                y: body_y + 3 + mounts_h + global_h,
                width: right_w,
                height: role_global_h,
            },
            content_width: global_mounts_content_width(role_global_mounts.as_slice()),
            content_height: role_global_content_h,
        }),
        roles: (roles_h > 0).then_some(ScrollArea {
            area: Rect {
                x: right_x,
                y: roles_y,
                width: right_w,
                height: roles_h,
            },
            content_width: roles_content_w,
            content_height: roles_content_h,
        }),
    })
}

fn current_dir_scroll_areas(
    state: &ManagerState<'_>,
    config: &crate::config::AppConfig,
    right_x: u16,
    right_w: u16,
    body_y: u16,
) -> ListScrollAreas {
    let mounts = [current_dir_mount(state)];
    let mounts_h = mount_block_height(&mounts);
    let agent_count = super::super::render::list::agents_block_agent_count(None, config);
    let roles_h = super::super::render::list::agents_block_height(agent_count);
    let roles_y = body_y + 3 + mounts_h;
    let roles_content_w = super::super::render::list::agents_block_content_width(None, config);
    let workspace_content_h = workspace_mounts_content_height(&mounts);
    let roles_content_h = 2 + agent_count;
    ListScrollAreas {
        workspace: ScrollArea {
            area: Rect {
                x: right_x,
                y: body_y + 3,
                width: right_w,
                height: mounts_h,
            },
            content_width: workspace_mounts_content_width(&mounts),
            content_height: workspace_content_h,
        },
        global: ScrollArea {
            area: Rect {
                x: right_x,
                y: roles_y,
                width: right_w,
                height: 0,
            },
            content_width: 0,
            content_height: 0,
        },
        role_global: None,
        roles: (roles_h > 0).then_some(ScrollArea {
            area: Rect {
                x: right_x,
                y: roles_y,
                width: right_w,
                height: roles_h,
            },
            content_width: roles_content_w,
            content_height: roles_content_h,
        }),
    }
}

fn current_dir_mount(state: &ManagerState<'_>) -> crate::workspace::MountConfig {
    crate::workspace::MountConfig {
        src: state.current_dir.clone(),
        dst: state.current_dir.clone(),
        readonly: false,
        isolation: crate::isolation::MountIsolation::Shared,
    }
}

fn editor_scroll_area(
    editor: &super::super::state::EditorState<'_>,
    term_size: Rect,
) -> ScrollArea {
    let body_y = 5;
    let body_h = term_size.height.saturating_sub(7);
    let rows = editor.pending.mounts.iter().fold(1usize, |acc, mount| {
        acc + if mount.src == mount.dst { 1 } else { 2 }
    }) + 2;
    ScrollArea {
        area: Rect {
            x: 0,
            y: body_y,
            width: term_size.width,
            height: (rows as u16 + 2).min(body_h.max(4)),
        },
        content_width: workspace_mounts_content_width(editor.pending.mounts.as_slice()),
        content_height: rows,
    }
}

fn global_mount_rows_content_width(rows: &[crate::config::GlobalMountRow]) -> usize {
    let mounts: Vec<crate::workspace::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    global_mounts_content_width(&mounts)
}

/// If the `Editor` or `CreatePrelude` stage has an open `FileBrowser`
/// whose git-prompt is active with a resolved URL, and the click lands
/// on the URL row, fire `open::that_detached` best-effort. Returns
/// `true` iff the click was consumed (URL opened). Non-matching stages,
/// non-click events, and clicks outside the URL row all return `false`
/// and the caller falls through to the list-view handler.
///
/// Modal geometry comes from `render::modal_outer_rect` — the same
/// helper `render_modal` uses — so mouse hit-testing can never drift
/// out of sync with what was drawn.
fn try_open_file_browser_git_url(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let (modal, fb_state): (&Modal<'_>, &FileBrowserState) = match &state.stage {
        ManagerStage::Editor(editor) => match editor.modal.as_ref() {
            Some(m @ Modal::FileBrowser { state, .. }) => (m, state),
            _ => return false,
        },
        ManagerStage::CreatePrelude(prelude) => match prelude.modal.as_ref() {
            Some(m @ Modal::FileBrowser { state, .. }) => (m, state),
            _ => return false,
        },
        _ => return false,
    };
    let modal_area = super::super::render::modal_outer_rect(modal, term_size);
    fb_state.maybe_open_url_on_click(modal_area, mouse.column, mouse.row)
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
    // Column check — strictly inside the left pane (exclude left border
    // and seam column, which is also the left pane's right border).
    if mouse.column == 0 || mouse.column >= seam_x {
        return None;
    }
    // Row check — strictly inside the bordered list block.
    let content_top = LIST_HEADER_HEIGHT + 1; // +1 skips the top border
    let body_end = term_size.height.saturating_sub(LIST_FOOTER_HEIGHT);
    // Content bottom is body_end - 1 (skip bottom border). Guard against
    // a terminal so short that the list has no interior.
    let content_bottom = body_end.saturating_sub(1);
    if mouse.row < content_top || mouse.row >= content_bottom {
        return None;
    }
    // Visual row index into the rendered list: items start at y = content_top
    // (the first row below the top border). The rendered list may contain a
    // blank spacer before "+ New workspace"; clicking that spacer selects
    // nothing.
    let idx = usize::from(mouse.row - content_top);
    state.row_at_visual_index(idx)
}

/// Compute the seam column (0-based) for a given split percentage and
/// total terminal width. Mirrors ratatui's own `Layout::split` arithmetic
/// closely enough for hit-testing purposes.
const fn seam_column(pct: u16, width: u16) -> u16 {
    // (width * pct) / 100 — saturating so a pathological width of 0 doesn't
    // panic. Under MIN_DRAGGABLE_WIDTH this arithmetic is already gated off
    // by the caller, but keep the helper safe for direct unit-testing.
    width.saturating_mul(pct) / 100
}

/// Return `(right_x, right_w)` using ratatui's own `Layout::split` arithmetic
/// so that scroll-offset clamping in mouse handlers uses the same viewport
/// width as `render_scrollable_block`. Integer division in `seam_column`
/// disagrees with ratatui's percentage rounding for some terminal widths,
/// causing touchpad scroll to stop 1 column short of the keyboard-reachable max.
fn right_pane_dims(pct: u16, total_width: u16) -> (u16, u16) {
    let right_pct = 100u16.saturating_sub(pct);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(pct),
            Constraint::Percentage(right_pct),
        ])
        .split(Rect {
            x: 0,
            y: 0,
            width: total_width,
            height: 1,
        });
    (cols[1].x, cols[1].width)
}

/// `true` when `column` is within ±`SEAM_HIT_SLACK` of `seam_x`.
const fn near_seam(column: u16, seam_x: u16) -> bool {
    let lo = seam_x.saturating_sub(SEAM_HIT_SLACK);
    let hi = seam_x.saturating_add(SEAM_HIT_SLACK);
    column >= lo && column <= hi
}

/// Derive the new split percentage from an active drag anchor and the
/// current mouse column. Handles the signed delta safely (mouse can move
/// either way along x) without underflow on u16.
fn pct_from_drag(anchor: DragState, mouse_col: u16, width: u16) -> u16 {
    // Signed delta in columns, scaled to a percentage of terminal width.
    let delta_cols = i32::from(mouse_col) - i32::from(anchor.anchor_x);
    let delta_pct = delta_cols * 100 / i32::from(width.max(1));
    let candidate = i32::from(anchor.anchor_pct) + delta_pct;
    // Clamp into [0, 100] before the narrower [MIN..=MAX] clamp so we can
    // safely cast back to u16.
    let bounded = candidate.clamp(0, 100);
    // `as u16` is safe: bounded is in [0,100].
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let narrowed = bounded as u16;
    narrowed
}

#[cfg(test)]
mod mouse_drag_tests {
    //! Unit tests for `handle_mouse`: the list/details seam is a
    //! mouse-draggable resize affordance driven entirely from `ManagerState`.
    //! These build `MouseEvent` values directly and bypass the ratatui
    //! event loop — enough to pin the seam hit-test + drag math without a
    //! real terminal.
    use super::{MOUSE_HORIZONTAL_SCROLL_STEP, handle_mouse, handle_mouse_with_config};
    use crate::console::manager::state::{
        DEFAULT_SPLIT_PCT, EditorState, EditorTab, FieldFocus, MAX_SPLIT_PCT, MIN_SPLIT_PCT,
        ManagerStage, ManagerState, Modal, MountScrollFocus, SecretsScopeTag,
    };
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    /// Build a `ManagerState` in the List stage at the default split,
    /// with no workspaces and no modal.
    fn list_state() -> ManagerState<'static> {
        let config = crate::config::AppConfig::default();
        let tmp = tempfile::tempdir().unwrap();
        ManagerState::from_config(&config, tmp.path())
    }

    /// Build a `MouseEvent` at column `col`, row 0.
    const fn mouse(kind: MouseEventKind, col: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// A 100-col-wide terminal area.
    const fn term(width: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height: 30,
        }
    }

    #[test]
    fn mouse_down_on_seam_starts_drag() {
        // Default split on a 100-col terminal => seam at column
        // `DEFAULT_SPLIT_PCT`.
        let mut state = list_state();
        assert_eq!(state.list_split_pct, DEFAULT_SPLIT_PCT);
        let e = mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT);
        handle_mouse(&mut state, e, term(100));
        assert!(
            state.drag_state.is_some(),
            "Down on seam must capture drag anchor; got {:?}",
            state.drag_state,
        );
        let drag = state.drag_state.unwrap();
        assert_eq!(drag.anchor_pct, DEFAULT_SPLIT_PCT);
        assert_eq!(drag.anchor_x, DEFAULT_SPLIT_PCT);
    }

    #[test]
    fn mouse_drag_updates_split_pct() {
        // Anchor at DEFAULT_SPLIT_PCT. Drag +10 columns on a 100-col
        // terminal ⇒ +10%.
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        let target = DEFAULT_SPLIT_PCT + 10;
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Drag(MouseButton::Left), target),
            term(100),
        );
        assert_eq!(state.list_split_pct, target);
    }

    #[test]
    fn mouse_drag_clamps_to_min_and_max() {
        // Drag far left ⇒ clamp to MIN_SPLIT_PCT.
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Drag(MouseButton::Left), 0),
            term(100),
        );
        assert_eq!(state.list_split_pct, MIN_SPLIT_PCT);

        // Drag far right ⇒ clamp to MAX_SPLIT_PCT.
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Drag(MouseButton::Left), 99),
            term(100),
        );
        assert_eq!(state.list_split_pct, MAX_SPLIT_PCT);
    }

    #[test]
    fn mouse_up_ends_drag() {
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        assert!(state.drag_state.is_some());
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Up(MouseButton::Left), 60),
            term(100),
        );
        assert!(state.drag_state.is_none(), "Up must clear drag anchor");
    }

    #[test]
    fn mouse_down_far_from_seam_does_not_start_drag() {
        // Clicks in the middle of either pane must be ignored — the
        // operator's intent is "click a row/button", not "start a resize".
        let mut state = list_state();
        // Seam at column `DEFAULT_SPLIT_PCT`; columns near either border
        // are far enough from the seam to be rejected.
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), 2),
            term(100),
        );
        assert!(state.drag_state.is_none(), "left-pane click must not drag");
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), 80),
            term(100),
        );
        assert!(state.drag_state.is_none(), "right-pane click must not drag");
    }

    #[test]
    fn drag_ignored_when_list_modal_open() {
        // GithubPicker is the only list-level modal today. Any mouse event
        // while it's up must be a silent no-op — the picker owns the
        // keyboard + (implicitly) the mouse focus.
        let mut state = list_state();
        // Use the github_mounts resolver indirectly — easier to
        // just synthesize a GithubPicker state with an arbitrary choice.
        // The picker's exact contents don't matter; only `list_modal.is_some()`.
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![MountConfig {
                src: "/w".into(),
                dst: "/w".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
        };
        // Ensure the helper signature compiles (guards against future refactors).
        let _ = crate::console::manager::github_mounts::resolve_for_workspace(&ws);
        state.list_modal = Some(Modal::GithubPicker {
            state: crate::console::widgets::github_picker::GithubPickerState::new(vec![
                crate::console::widgets::github_picker::GithubChoice {
                    src: "/w".into(),
                    branch: "main".into(),
                    url: "https://github.com/o/r".into(),
                },
            ]),
        });

        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        assert!(
            state.drag_state.is_none(),
            "Down with list_modal open must not drag",
        );
    }

    #[test]
    fn drag_ignored_on_non_list_stage() {
        // While in the Editor (or any non-List stage), mouse events are
        // ignored outright — no seam to drag.
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![],
            ..Default::default()
        };
        state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        assert!(
            state.drag_state.is_none(),
            "Down on Editor stage must not drag",
        );
    }

    #[test]
    fn drag_ignored_when_terminal_too_narrow() {
        // Terminals narrower than MIN_DRAGGABLE_WIDTH skip hit-testing
        // entirely — below that the clamp bounds already leave the right
        // pane implausibly small.
        let mut state = list_state();
        // 30-col terminal is below the 40-col threshold.
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), 13),
            term(30),
        );
        assert!(state.drag_state.is_none());
    }

    // ── File-browser URL-click integration ─────────────────────────────
    //
    // When a FileBrowser modal with a git-prompt + resolved URL is open
    // during the Editor or CreatePrelude stages, Down(Left) on the URL
    // row must be consumed by the open-URL path (best-effort; silent on
    // failure) — observable side-effect: the drag-anchor never latches.

    /// Term of 120x40 ⇒ `FileBrowser` modal at (18, 9, 84, 22); URL row at
    /// y = 17, column range ≈ 19..=100. Mirrors the reference geometry
    /// used in `file_browser::tests::manufactured_modal_area`.
    fn term_120x40() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        }
    }

    /// Mouse event at `(col, row)`, left-button Down.
    const fn mouse_down_at(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_down_on_editor_tab_selects_tab() {
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![],
            ..Default::default()
        };
        state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

        // Rendered tab spans start at x=0:
        // " General " (0..9), space, " Mounts " (10..18), space,
        // " Roles " (19..26), space, " Environments " (27..41).
        handle_mouse(&mut state, mouse_down_at(33, 3), term(100));

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.active_tab, EditorTab::Secrets);
        assert!(matches!(
            editor.active_field,
            crate::console::manager::state::FieldFocus::Row(0)
        ));
    }

    #[test]
    fn mouse_down_on_editor_tab_clears_secrets_view_when_leaving() {
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![],
            ..Default::default()
        };
        let mut editor = EditorState::new_edit("x".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor
            .unmasked_rows
            .insert((SecretsScopeTag::Workspace, "TOKEN".to_string()));
        editor.secrets_expanded.insert("agent-smith".to_string());
        state.stage = ManagerStage::Editor(editor);

        handle_mouse(&mut state, mouse_down_at(3, 3), term(100));

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.active_tab, EditorTab::General);
        assert!(editor.unmasked_rows.is_empty());
        assert!(editor.secrets_expanded.is_empty());
    }

    #[test]
    fn mouse_down_on_url_row_in_prelude_with_url_does_not_drag() {
        use crate::console::manager::state::CreatePreludeState;
        use crate::console::widgets::file_browser::FileBrowserState;
        let mut state = list_state();
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        // Build a FileBrowser at `parent`, select the repo, open git prompt,
        // and inject a URL so the URL row renders.
        let mut fb = FileBrowserState::new_at(tmp.path().to_path_buf(), parent);
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.pending_git_url = Some("file:///tmp/unreachable".to_string());

        let prelude = CreatePreludeState {
            modal: Some(Modal::FileBrowser {
                target: crate::console::manager::state::FileBrowserTarget::CreateFirstMountSrc,
                state: fb,
            }),
            ..CreatePreludeState::default()
        };
        state.stage = ManagerStage::CreatePrelude(prelude);

        // URL row at y = 17 for this term size; centre column ≈ 60.
        handle_mouse(&mut state, mouse_down_at(60, 17), term_120x40());
        // No drag latched — URL click is consumed before the seam path.
        assert!(
            state.drag_state.is_none(),
            "URL click must not start a seam drag",
        );
    }

    #[test]
    fn mouse_down_outside_url_row_in_prelude_is_silent_noop() {
        use crate::console::manager::state::CreatePreludeState;
        use crate::console::widgets::file_browser::FileBrowserState;
        let mut state = list_state();
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut fb = FileBrowserState::new_at(tmp.path().to_path_buf(), parent);
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.pending_git_url = Some("file:///tmp/unreachable".to_string());

        let prelude = CreatePreludeState {
            modal: Some(Modal::FileBrowser {
                target: crate::console::manager::state::FileBrowserTarget::CreateFirstMountSrc,
                state: fb,
            }),
            ..CreatePreludeState::default()
        };
        state.stage = ManagerStage::CreatePrelude(prelude);

        // Row 0 is well outside the URL row (17) and the modal entirely.
        handle_mouse(&mut state, mouse_down_at(60, 0), term_120x40());
        // CreatePrelude is not the List stage, so the list-drag path is
        // also inert — no drag latched regardless of the URL branch.
        assert!(state.drag_state.is_none());
    }

    // ── Click-to-select tests ──────────────────────────────────────
    //
    // Layout (100x30 terminal, header=3 footer=2 body=25):
    //   y = 0..=2   → header (chunks[0])
    //   y = 3       → body top border (list block)
    //   y = 4       → list item 0 ("Current directory")
    //   y = 5       → list item 1 (first saved workspace)
    //   ...
    //   y = 28      → body bottom border
    //   y = 29      → footer (chunks[2])
    //
    // Left pane (default split = DEFAULT_SPLIT_PCT%): x = 0..=(seam-1)
    // with x=0 = left border and x=seam-1 inclusive = last interior col.
    // The seam column itself is the drag-handle.

    /// Mouse event at `(col, row)`, left-button Down.
    const fn mouse_at(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    const fn mouse_kind_at(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Build a list state with `n` saved workspaces (row 0 + n + spacer + sentinel).
    fn list_state_with_saved(n: usize) -> ManagerState<'static> {
        let mut config = crate::config::AppConfig::default();
        for i in 0..n {
            config.workspaces.insert(
                format!("ws-{i:02}"),
                WorkspaceConfig {
                    workdir: format!("/w/{i}"),
                    mounts: vec![],
                    ..Default::default()
                },
            );
        }
        let tmp = tempfile::tempdir().unwrap();
        ManagerState::from_config(&config, tmp.path())
    }

    fn config_with_scrollable_workspace_and_global_mounts() -> crate::config::AppConfig {
        let mut config = crate::config::AppConfig::default();
        config.workspaces.insert(
            "demo".into(),
            WorkspaceConfig {
                workdir: "/workspace/demo".into(),
                mounts: vec![MountConfig {
                    src: "/host/source/with/a/very/long/path/that/forces/workspace/mount/scrolling"
                        .into(),
                    dst: "/container/destination/with/a/very/long/path/that/forces/workspace/mount/scrolling"
                        .into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );
        config.add_mount(
            "global-long",
            MountConfig {
                src: "/host/source/with/a/very/long/path/that/forces/global/mount/scrolling"
                    .into(),
                dst: "/container/destination/with/a/very/long/path/that/forces/global/mount/scrolling"
                    .into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            None,
        );
        config
    }

    fn selected_demo_state(config: &crate::config::AppConfig) -> ManagerState<'static> {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(config, tmp.path());
        state.selected = 1;
        state
    }

    fn current_dir_state_at(path: &std::path::Path) -> ManagerState<'static> {
        let config = crate::config::AppConfig::default();
        ManagerState::from_config(&config, path)
    }

    fn config_with_long_git_type_mount(source: &std::path::Path) -> crate::config::AppConfig {
        let mut config = crate::config::AppConfig::default();
        config.workspaces.insert(
            "demo".into(),
            WorkspaceConfig {
                workdir: "/workspace/demo".into(),
                mounts: vec![MountConfig {
                    src: source.display().to_string(),
                    dst: source.display().to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );
        config
    }

    #[test]
    fn click_on_first_row_sets_selected_to_zero() {
        // y=4 = first list item (index 0, "Current directory").
        let mut state = list_state_with_saved(3);
        state.selected = 2;
        handle_mouse(&mut state, mouse_at(10, 4), term(100));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn click_on_fifth_row_sets_selected_to_four() {
        // y=8 = fifth list row (index 4). Needs enough saved workspaces
        // to make index 4 a valid selection target.
        let mut state = list_state_with_saved(5);
        state.selected = 0;
        handle_mouse(&mut state, mouse_at(10, 8), term(100));
        assert_eq!(state.selected, 4);
    }

    #[test]
    fn click_on_sentinel_row_sets_selected_to_sentinel_idx() {
        // 3 saved workspaces ⇒ rows are:
        //   y=4  → index 0 ("Current directory")
        //   y=5,6,7 → indices 1, 2, 3 (saved)
        //   y=8  → visual spacer
        //   y=9  → visual index 5 (sentinel "+ New workspace")
        let mut state = list_state_with_saved(3);
        state.selected = 0;
        handle_mouse(&mut state, mouse_at(10, 9), term(100));
        assert_eq!(state.selected, 4, "sentinel_idx = saved_count + 1 = 4");
    }

    #[test]
    fn click_on_workspace_list_spacer_does_not_change_selected() {
        let mut state = list_state_with_saved(3);
        state.selected = 2;
        handle_mouse(&mut state, mouse_at(10, 8), term(100));
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn click_outside_list_rows_does_not_change_selected() {
        // Several "outside" positions must all leave selected untouched:
        //   - Click above the list (y < 4, e.g. in the header)
        //   - Click on the left border (x=0)
        //   - Click at x >= seam (right pane territory)
        //   - Click below the list content (footer)
        let mut state = list_state_with_saved(3);
        state.selected = 2;
        let initial = state.selected;

        // In the header.
        handle_mouse(&mut state, mouse_at(10, 1), term(100));
        assert_eq!(state.selected, initial, "click in header must not select");

        // On the top border of the list block.
        handle_mouse(&mut state, mouse_at(10, 3), term(100));
        assert_eq!(state.selected, initial, "click on top border");

        // On the left border column.
        handle_mouse(&mut state, mouse_at(0, 4), term(100));
        assert_eq!(state.selected, initial, "click on left border");

        // Past the sentinel row (y=9+ when we have 3 saved workspaces).
        handle_mouse(&mut state, mouse_at(10, 10), term(100));
        assert_eq!(state.selected, initial, "click below sentinel");

        // In the right pane (x=60, well clear of the default seam).
        handle_mouse(&mut state, mouse_at(60, 5), term(100));
        assert_eq!(state.selected, initial, "click in details pane");

        // In the footer.
        handle_mouse(&mut state, mouse_at(10, 29), term(100));
        assert_eq!(state.selected, initial, "click on footer row");
    }

    #[test]
    fn click_on_seam_still_starts_drag_not_selection() {
        // Regression guard for batch 14: a click on the seam column must
        // kick off a drag and NOT retarget selection, even when the y
        // coordinate happens to overlap a valid list row.
        let mut state = list_state_with_saved(3);
        state.selected = 0;
        // Default split on a 100-col terminal ⇒ seam at column
        // `DEFAULT_SPLIT_PCT`. y=5 maps to list index 1 in our layout —
        // if seam didn't win, selection would flip to 1.
        handle_mouse(&mut state, mouse_at(DEFAULT_SPLIT_PCT, 5), term(100));
        assert!(state.drag_state.is_some(), "click on seam must start drag");
        assert_eq!(
            state.selected, 0,
            "seam-click must not change selection even when y lands on a list row"
        );
    }

    #[test]
    fn click_scrollable_mount_block_focuses_it() {
        let config = config_with_scrollable_workspace_and_global_mounts();
        let mut state = selected_demo_state(&config);

        // Right pane starts at x=30 for a 100-col terminal. Workspace mounts
        // block starts at y=6 after General's 3 rows.
        handle_mouse_with_config(&mut state, mouse_at(31, 7), term(100), Some(&config));

        assert_eq!(state.list_scroll_focus, Some(MountScrollFocus::Workspace));
    }

    #[test]
    fn click_current_directory_mount_block_focuses_and_scrolls_it() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join(
            "very-long-current-directory-name-that-forces-horizontal-scrolling-in-the-preview",
        );
        std::fs::create_dir_all(&cwd).unwrap();
        let config = crate::config::AppConfig::default();
        let mut state = current_dir_state_at(&cwd);
        assert!(state.is_current_dir_selected());

        handle_mouse_with_config(&mut state, mouse_at(31, 7), term(100), Some(&config));
        assert_eq!(state.list_scroll_focus, Some(MountScrollFocus::Workspace));

        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollRight, 31, 7),
            term(100),
            Some(&config),
        );

        assert_eq!(state.list_mounts_scroll_x, MOUSE_HORIZONTAL_SCROLL_STEP);
    }

    #[test]
    fn click_non_scrollable_area_clears_mount_focus() {
        let config = config_with_scrollable_workspace_and_global_mounts();
        let mut state = selected_demo_state(&config);
        state.list_scroll_focus = Some(MountScrollFocus::Workspace);

        // y=4 is inside the General block, which is not a horizontal-scroll
        // target.
        handle_mouse_with_config(&mut state, mouse_at(31, 4), term(100), Some(&config));

        assert_eq!(state.list_scroll_focus, None);
    }

    #[test]
    fn horizontal_mouse_wheel_scrolls_block_under_pointer() {
        let config = config_with_scrollable_workspace_and_global_mounts();
        let mut state = selected_demo_state(&config);
        state.list_scroll_focus = Some(MountScrollFocus::Workspace);

        // Global mounts block starts immediately after General (3 rows) and
        // the one-mount Workspace mounts block (5 rows): y=11.
        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollRight, 31, 12),
            term(100),
            Some(&config),
        );

        assert_eq!(state.list_mounts_scroll_x, 0);
        assert_eq!(
            state.list_global_mounts_scroll_x,
            MOUSE_HORIZONTAL_SCROLL_STEP
        );
        assert_eq!(state.list_scroll_focus, Some(MountScrollFocus::Global));
    }

    #[test]
    fn vertical_mouse_wheel_does_not_scroll_horizontal_only_list_block() {
        // W3C rule: ScrollUp/Down are vertical events; horizontal-only blocks
        // (List view mounts) must ignore them. Only ScrollLeft/Right scroll them.
        let config = config_with_scrollable_workspace_and_global_mounts();
        let mut state = selected_demo_state(&config);

        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, 31, 12),
            term(100),
            Some(&config),
        );

        assert_eq!(
            state.list_global_mounts_scroll_x, 0,
            "ScrollDown must not change horizontal scroll on a horizontal-only block"
        );

        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollUp, 31, 12),
            term(100),
            Some(&config),
        );

        assert_eq!(state.list_global_mounts_scroll_x, 0);
    }

    #[test]
    fn horizontal_mouse_wheel_clamps_stored_offset_at_block_end() {
        let config = config_with_scrollable_workspace_and_global_mounts();
        let mut state = selected_demo_state(&config);

        for _ in 0..100 {
            handle_mouse_with_config(
                &mut state,
                mouse_kind_at(MouseEventKind::ScrollRight, 31, 12),
                term(100),
                Some(&config),
            );
        }

        let global_mounts: Vec<MountConfig> = config
            .list_mount_rows()
            .into_iter()
            .filter(|row| row.scope.is_none())
            .map(|row| row.mount)
            .collect();
        let global_area = Rect {
            x: 30,
            y: 11,
            width: 70,
            height: 5,
        };
        let expected_max = super::max_scroll_offset(
            super::global_mounts_content_width(global_mounts.as_slice()),
            super::scroll_viewport_width(global_area),
        );
        assert_eq!(state.list_global_mounts_scroll_x, expected_max);

        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollLeft, 31, 12),
            term(100),
            Some(&config),
        );

        assert_eq!(
            state.list_global_mounts_scroll_x,
            expected_max.saturating_sub(MOUSE_HORIZONTAL_SCROLL_STEP),
            "left-scroll after overscrolling right must move immediately, not burn hidden offset"
        );
    }

    #[test]
    fn horizontal_mouse_wheel_reaches_rendered_workspace_width() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(
            repo.join(".git").join("HEAD"),
            "ref: refs/heads/feat/backend-rust-gdpr-purge-normalization\n",
        )
        .unwrap();
        let config = config_with_long_git_type_mount(&repo);
        let mut state = selected_demo_state(&config);

        for _ in 0..100 {
            handle_mouse_with_config(
                &mut state,
                mouse_kind_at(MouseEventKind::ScrollRight, 31, 7),
                term(100),
                Some(&config),
            );
        }

        let workspace = config.workspaces.get("demo").unwrap();
        let workspace_area = Rect {
            x: 30,
            y: 6,
            width: 70,
            height: 4,
        };
        let expected_max = super::max_scroll_offset(
            super::workspace_mounts_content_width(workspace.mounts.as_slice()),
            super::scroll_viewport_width(workspace_area),
        );

        assert_eq!(
            state.list_mounts_scroll_x, expected_max,
            "mouse/touch scroll must clamp at the same rendered width keyboard scrolling reaches"
        );
    }

    #[test]
    fn horizontal_mouse_wheel_clamps_before_applying_left_delta() {
        let config = config_with_scrollable_workspace_and_global_mounts();
        let mut state = selected_demo_state(&config);
        state.list_global_mounts_scroll_x = u16::MAX;

        let global_mounts: Vec<MountConfig> = config
            .list_mount_rows()
            .into_iter()
            .filter(|row| row.scope.is_none())
            .map(|row| row.mount)
            .collect();
        let global_area = Rect {
            x: 30,
            y: 11,
            width: 70,
            height: 5,
        };
        let expected_max = super::max_scroll_offset(
            super::global_mounts_content_width(global_mounts.as_slice()),
            super::scroll_viewport_width(global_area),
        );

        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollLeft, 31, 12),
            term(100),
            Some(&config),
        );

        assert_eq!(
            state.list_global_mounts_scroll_x,
            expected_max.saturating_sub(MOUSE_HORIZONTAL_SCROLL_STEP),
            "left-scroll must first clamp stale resize/overscroll state, then move left"
        );
    }

    #[test]
    fn editor_mounts_tab_horizontal_wheel_requires_mounts_tab() {
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![MountConfig {
                src: "/host/source/with/a/very/long/path/that/forces/editor/mount/scrolling"
                    .into(),
                dst: "/container/destination/with/a/very/long/path/that/forces/editor/mount/scrolling"
                    .into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
        };
        let mut editor = EditorState::new_edit("x".into(), ws);
        editor.active_tab = crate::console::manager::state::EditorTab::Mounts;
        state.stage = ManagerStage::Editor(editor);

        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollRight, 10, 6),
            term(100),
            None,
        );
        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!("editor stage expected");
        };
        assert!(editor.workspace_mounts_scroll_focused);
        assert_eq!(
            editor.workspace_mounts_scroll_x,
            MOUSE_HORIZONTAL_SCROLL_STEP
        );

        editor.active_tab = crate::console::manager::state::EditorTab::General;
        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollRight, 10, 6),
            term(100),
            None,
        );
        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(!editor.workspace_mounts_scroll_focused);
        assert_eq!(
            editor.workspace_mounts_scroll_x,
            MOUSE_HORIZONTAL_SCROLL_STEP
        );
    }

    #[test]
    fn editor_mounts_tab_click_full_row_width_selects_mount_without_scroll_focus_when_unscrollable()
    {
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![
                MountConfig {
                    src: "/host/one".into(),
                    dst: "/host/one".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                MountConfig {
                    src: "/host/two".into(),
                    dst: "/host/two".into(),
                    readonly: true,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            ..Default::default()
        };
        let mut editor = EditorState::new_edit("x".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        // Mounts editor body begins at y=5. Interior row y=6 is the
        // header, y=7 is mount 0, y=8 is mount 1. Click far to the
        // right in whitespace on mount 1's row, not on the path text.
        handle_mouse_with_config(&mut state, mouse_at(95, 8), term(100), None);

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(matches!(editor.active_field, FieldFocus::Row(1)));
        assert!(!editor.workspace_mounts_scroll_focused);
    }

    #[test]
    fn editor_mounts_tab_click_host_source_continuation_selects_parent_without_scroll_focus_when_unscrollable()
     {
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![MountConfig {
                src: "/host/source".into(),
                dst: "/container/destination".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
        };
        let mut editor = EditorState::new_edit("x".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(editor.pending.mounts.len());
        state.stage = ManagerStage::Editor(editor);

        // y=8 is the host-source continuation line for the first mount.
        handle_mouse_with_config(&mut state, mouse_at(95, 8), term(100), None);

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(matches!(editor.active_field, FieldFocus::Row(0)));
        assert!(!editor.workspace_mounts_scroll_focused);
    }

    #[test]
    fn scroll_up_decrements_vertical_scroll_offset() {
        let config = config_with_scrollable_workspace_and_global_mounts();
        let mut state = selected_demo_state(&config);
        state.list_scroll_focus = Some(MountScrollFocus::Global);
        state.list_global_mounts_scroll_y = 3;

        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollUp, 31, 12),
            term(100),
            Some(&config),
        );

        assert_eq!(state.list_global_mounts_scroll_y, 2);
    }
}
