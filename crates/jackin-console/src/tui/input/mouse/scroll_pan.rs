// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-panel wheel scrolling: wheel events on the focused active panel
//! and the helper that re-derives the focused scroll-focus plan.

use super::{
    EditorTab, LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT, ManagerMessage, ManagerStage, ManagerState,
    MountScrollFocus, MouseEvent, Rect, SettingsTab, apply_horizontal_scroll,
    apply_vertical_scroll, dispatch_manager, editor_scroll_area, editor_scroll_focus_plan,
    editor_tab_bar_focus_plan, horizontal_split_pane_dims, is_horizontally_scrollable,
    list_scroll_areas, point_in_rect, scroll_viewport_width, settings_modal_open_fact,
    settings_scroll_focus_plan, settings_tab_bar_focus_plan, split_seam_column,
    workspace_list_scroll_focus_plan,
};
use crate::tui::layout::list::list_names_content_width;

pub fn update_scroll_focus(
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

pub fn settings_modal_open(settings: &crate::tui::state::SettingsState<'_>) -> bool {
    settings_modal_open_fact(
        settings.error_popup.is_some(),
        settings.mounts.modal.is_some(),
        settings.env.modal.is_some(),
        settings.auth.has_modal(),
    )
}

pub fn scroll_active_panel(
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
#[allow(
    clippy::missing_const_for_fn,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn scroll_active_panel_vertical(
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
