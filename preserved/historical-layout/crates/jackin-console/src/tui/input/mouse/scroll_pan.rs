// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-panel wheel scrolling: wheel events on the focused active panel
//! and the helper that re-derives the focused scroll-focus plan.

use super::{
    ConsoleScrollBlock, EditorTab, LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT, ManagerMessage,
    ManagerStage, ManagerState, MouseEvent, Rect, dispatch_manager, editor_scroll_area,
    editor_scroll_focus_plan, editor_tab_bar_focus_plan, hit, is_horizontally_scrollable,
    list_scroll_areas, point_in_rect, scroll_block_registry, settings_modal_open_fact,
    settings_scroll_focus_plan, settings_tab_bar_focus_plan, split_seam_column,
    workspace_list_scroll_focus_plan,
};
use crate::tui::scroll_block::scroll_block_by;
use termrock::widgets::ScrollOutcome;

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
        settings.mounts.modals.is_open(),
        settings.env.modals.is_open(),
        settings.auth.has_modal(),
    )
}

/// Wheel dispatch through the per-event scroll-block registry (matrix rows
/// 3, 4, 12): hit-test the pointer against the paint-z-ordered blocks, run
/// the stage's focus side effect, then apply the delta to the hit block's
/// `ScrollAreaState` via the dims-plumbing idiom. Returns the upstream
/// outcome so the caller can fire the Shift-fallback vertical retry on
/// `ScrollOutcome::Ignored` (row 3 — upstream never retries vertical).
pub fn dispatch_wheel(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
    dy: i16,
    dx: i16,
) -> ScrollOutcome {
    // Modal guards (row 13) hold their place before any block resolves.
    match &state.stage {
        ManagerStage::List if state.list_modal.is_some() => return ScrollOutcome::Ignored,
        ManagerStage::Editor(editor) if editor.modal.is_some() => return ScrollOutcome::Ignored,
        ManagerStage::Settings(settings) if settings_modal_open(settings) => {
            return ScrollOutcome::Ignored;
        }
        _ => {}
    }

    let blocks = scroll_block_registry(state, term_size, config);
    let hit_id = hit(&blocks, mouse.column, mouse.row);

    match &mut state.stage {
        ManagerStage::List => {
            // Row 4: scroll focus re-derives from the pointer on every wheel
            // event, before the offset moves.
            update_scroll_focus(state, mouse, term_size, config);
            let Some(id) = hit_id else {
                return ScrollOutcome::Ignored;
            };
            let Some(region) = blocks.iter().find(|block| block.id == id) else {
                return ScrollOutcome::Ignored;
            };
            let region = *region;
            let scroll = match id {
                ConsoleScrollBlock::ListNames => {
                    // Horizontal-only block (row 12): vertical wheel never
                    // moves the names pane.
                    if dy != 0 {
                        return ScrollOutcome::Ignored;
                    }
                    &mut state.list_names_scroll
                }
                ConsoleScrollBlock::ListWorkspaceMounts => &mut state.list_mounts_scroll,
                ConsoleScrollBlock::ListGlobalMounts => &mut state.list_global_mounts_scroll,
                ConsoleScrollBlock::ListRoleGlobalMounts => {
                    &mut state.list_role_global_mounts_scroll
                }
                ConsoleScrollBlock::ListRoles => &mut state.list_roles_scroll,
                _ => return ScrollOutcome::Ignored,
            };
            scroll_block_by(
                scroll,
                region.rect,
                region.content_w,
                region.content_h,
                isize::from(dy),
                isize::from(dx),
            )
        }
        ManagerStage::Editor(editor) => {
            if dx != 0 {
                // Row 4: horizontal wheel applies the same focus plan the
                // pre-registry lane did, and scrolls only when the plan
                // grants the block focus.
                let in_scrollable = if editor.active_tab == EditorTab::Mounts {
                    let area = editor_scroll_area(editor, term_size);
                    point_in_rect(mouse.column, mouse.row, area.area)
                        && is_horizontally_scrollable(area.area, area.content_width)
                } else {
                    let area = editor.content_area(term_size);
                    point_in_rect(mouse.column, mouse.row, area)
                        && is_horizontally_scrollable(area, editor.tab_content_width)
                };
                let plan = if editor.active_tab == EditorTab::Mounts {
                    editor_scroll_focus_plan(editor.active_tab, false, in_scrollable, false)
                } else {
                    editor_scroll_focus_plan(editor.active_tab, false, false, in_scrollable)
                };
                editor.apply_scroll_focus_plan(plan);
                let focused = if editor.active_tab == EditorTab::Mounts {
                    plan.workspace_mounts_scroll_focused
                } else {
                    plan.tab_content_scroll_focused
                };
                if !focused {
                    return ScrollOutcome::Ignored;
                }
            }
            let Some(id) = hit_id else {
                return ScrollOutcome::Ignored;
            };
            let Some(region) = blocks.iter().find(|block| block.id == id) else {
                return ScrollOutcome::Ignored;
            };
            let region = *region;
            match id {
                ConsoleScrollBlock::EditorTabContent => scroll_block_by(
                    &mut editor.tab_scroll,
                    region.rect,
                    region.content_w,
                    region.content_h,
                    isize::from(dy),
                    isize::from(dx),
                ),
                ConsoleScrollBlock::EditorWorkspaceMounts => {
                    if dy != 0 {
                        // Mounts-tab vertical wheel drives the tab content
                        // offset, exactly as the pre-registry lane did.
                        scroll_block_by(
                            &mut editor.tab_scroll,
                            region.rect,
                            region.content_w,
                            region.content_h,
                            isize::from(dy),
                            0,
                        )
                    } else {
                        scroll_block_by(
                            &mut editor.workspace_mounts_scroll,
                            region.rect,
                            region.content_w,
                            region.content_h,
                            0,
                            isize::from(dx),
                        )
                    }
                }
                _ => ScrollOutcome::Ignored,
            }
        }
        ManagerStage::Settings(settings) => {
            // Settings wheel sets no focus (row 4: side-effect-free stage).
            let Some(id) = hit_id else {
                return ScrollOutcome::Ignored;
            };
            let Some(region) = blocks.iter().find(|block| block.id == id) else {
                return ScrollOutcome::Ignored;
            };
            let region = *region;
            let scroll = match id {
                ConsoleScrollBlock::SettingsMounts => &mut settings.mounts.scroll,
                ConsoleScrollBlock::SettingsEnv if dy != 0 => &mut settings.env.scroll,
                ConsoleScrollBlock::SettingsTrust => &mut settings.trust.scroll,
                ConsoleScrollBlock::SettingsAuth if dy != 0 => settings.auth.scroll_state_mut(),
                _ => return ScrollOutcome::Ignored,
            };
            scroll_block_by(
                scroll,
                region.rect,
                region.content_w,
                region.content_h,
                isize::from(dy),
                isize::from(dx),
            )
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => ScrollOutcome::Ignored,
    }
}
