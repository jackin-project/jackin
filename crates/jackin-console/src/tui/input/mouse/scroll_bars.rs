// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Scroll-bar drag handlers: drag the horizontal scrollbar of the
//! long-content body and the vertical scrollbar of the focused panel.

use super::{
    EditorTab, LIST_FOOTER_HEIGHT, ManagerStage, ManagerState, MountScrollFocus, MouseEvent, Rect,
    SCREEN_HEADER_HEIGHT, ScrollbarAxis, SettingsTab, TAB_STRIP_HEIGHT, apply_scrollbar_drag,
    editor_scroll_area, editor_scroll_focus_plan, list_scroll_areas, settings_modal_open,
    workspace_list_scroll_focus_plan,
};

pub fn try_drag_horizontal_scrollbar(
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

pub fn try_drag_vertical_scrollbar(
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
