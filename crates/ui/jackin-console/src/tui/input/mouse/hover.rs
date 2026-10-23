// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Position;
use termrock::interaction::HitRegion;

use super::{
    FileBrowserState, ManagerEffect, ManagerListRow, ManagerStage, ManagerState, Modal, MouseEvent,
    Rect, apply_workspace_list_hover_target, editor_mount_hover_target_at_position,
    editor_scroll_area, editor_tab_hit_regions, settings_tab_hit_regions,
    settings_trust_hover_target_at_position, split_seam_column,
    workspace_list_hover_row_at_position,
};
use crate::tui::layout::{LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT};
use crate::tui::state::{
    EditorHoverTarget, EditorTab, ManagerHoverTarget, SettingsHoverTarget, SettingsTab,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleHoverTarget {
    Editor(EditorHoverTarget),
    Settings(SettingsHoverTarget),
    Workspace(ManagerHoverTarget),
}

pub fn try_copy_container_info_value(
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
    let Some((row, payload)) = crate::tui::components::container_info_surface::copy_payload_at(
        area,
        info,
        mouse.column,
        mouse.row,
    ) else {
        return false;
    };
    state.request_effect(ManagerEffect::CopyContainerInfoValue { row, payload });
    true
}

pub fn container_info_copyable_row_at(
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
    crate::tui::components::container_info_surface::copy_payload_at(
        area,
        info,
        mouse.column,
        mouse.row,
    )
    .is_some()
}

pub fn update_container_info_hover(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) {
    let Some(modal @ Modal::ContainerInfo { .. }) = state.list_modal.as_ref() else {
        return;
    };
    let Some(area) = modal.container_info_rect(term_size) else {
        return;
    };
    let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut() else {
        return;
    };
    let hovered = crate::tui::components::container_info_surface::copy_payload_at(
        area,
        info,
        mouse.column,
        mouse.row,
    )
    .map(|(row, _)| row);
    info.set_hovered_row(hovered);
}

pub fn file_browser_modal_and_state<'a, 'b>(
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

pub fn file_browser_url_row_at(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let Some((modal, fb_state)) = file_browser_modal_and_state(state) else {
        return false;
    };
    let modal_area = modal.rect(term_size);
    fb_state.url_row_hit(modal_area, mouse.column, mouse.row)
}
pub fn update_hover(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) {
    let regions = hover_regions(state, term_size, config);
    let hovered = state
        .hover
        .update(Position::new(mouse.column, mouse.row), &regions)
        .copied();
    match &mut state.stage {
        ManagerStage::Editor(editor) => editor.set_hover_target(match hovered {
            Some(ConsoleHoverTarget::Editor(target)) => Some(target),
            _ => None,
        }),
        ManagerStage::Settings(settings) => settings.set_hover_target(match hovered {
            Some(ConsoleHoverTarget::Settings(target)) => Some(target),
            _ => None,
        }),
        _ => {}
    }
    apply_workspace_list_hover_target(
        state,
        match hovered {
            Some(ConsoleHoverTarget::Workspace(target)) => Some(target),
            _ => None,
        },
    );
    update_container_info_hover(state, mouse, term_size);
}

fn hover_regions(
    state: &ManagerState<'_>,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> Vec<HitRegion<ConsoleHoverTarget>> {
    let mut regions = Vec::new();
    match &state.stage {
        ManagerStage::Editor(editor) => {
            if editor.modal.is_none() {
                for region in editor_tab_hit_regions(editor, term_size, config, state.op_available)
                {
                    if EditorTab::ALL.get(region.id).is_some() {
                        regions.push(HitRegion {
                            id: ConsoleHoverTarget::Editor(EditorHoverTarget::Tab(region.id)),
                            area: region.area,
                        });
                    }
                }
                let area = editor_scroll_area(editor, term_size).area;
                let content_x = area.x.saturating_add(1);
                let content_width = area.width.saturating_sub(2);
                let content_height = area.height.saturating_sub(2);
                for offset in 0..content_height {
                    let y = area.y.saturating_add(1).saturating_add(offset);
                    if let Some(target) = editor_mount_hover_target_at_position(
                        editor.active_tab,
                        false,
                        area,
                        content_x,
                        y,
                        editor.tab_scroll.offset_y(),
                        editor.pending.mounts.as_slice(),
                    ) {
                        regions.push(HitRegion {
                            id: ConsoleHoverTarget::Editor(target),
                            area: Rect::new(content_x, y, content_width, 1),
                        });
                    }
                }
            }
        }
        ManagerStage::Settings(settings) => {
            if !settings.mounts.modals.is_open() && !settings.env.modals.is_open() {
                for region in settings_tab_hit_regions(settings, term_size, state.op_available) {
                    if SettingsTab::ALL.get(region.id).is_some() {
                        regions.push(HitRegion {
                            id: ConsoleHoverTarget::Settings(SettingsHoverTarget::Tab(region.id)),
                            area: region.area,
                        });
                    }
                }
            }
            let area = settings.content_area(term_size);
            for offset in 0..area.height {
                let y = area.y.saturating_add(offset);
                if let Some(target) = settings_trust_hover_target_at_position(
                    settings.active_tab,
                    settings.mounts.modals.is_open(),
                    area,
                    area.x,
                    y,
                    settings.trust.scroll.offset_y(),
                    settings.trust.pending.len(),
                ) {
                    regions.push(HitRegion {
                        id: ConsoleHoverTarget::Settings(target),
                        area: Rect::new(area.x, y, area.width, 1),
                    });
                }
            }
        }
        ManagerStage::List if state.list_modal.is_none() => {
            let seam_x = split_seam_column(state.list_split_pct, term_size.width);
            let content_top = LIST_HEADER_HEIGHT.saturating_add(1);
            let content_bottom = term_size
                .height
                .saturating_sub(LIST_FOOTER_HEIGHT)
                .saturating_sub(1);
            let visual_rows = state.visual_rows_vec();
            for y in content_top..content_bottom {
                if let Some(row) = workspace_list_hover_row_at_position(
                    visual_rows.as_slice(),
                    1,
                    y,
                    term_size,
                    seam_x,
                    |row| state.index_of_row(row).is_some(),
                ) {
                    regions.push(HitRegion {
                        id: ConsoleHoverTarget::Workspace(ManagerHoverTarget::ListRow(row)),
                        area: Rect::new(1, y, seam_x.saturating_sub(1), 1),
                    });
                }
            }
        }
        _ => {}
    }
    regions
}

pub fn list_row_hover_at(
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
