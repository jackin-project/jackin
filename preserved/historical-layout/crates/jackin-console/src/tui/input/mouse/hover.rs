// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Hover-state update helpers: container-info hover, file-browser
//! pointer position, list-row hover targets.
//!
//! Matrix row 15: stage hover (tab cells, list rows, mount rows, trust
//! rows) rides one consumer `HoverState<ConsoleHoverTarget>` held on
//! `ManagerState`, fed per-Moved-event `HitRegion`s built from the same
//! pure geometry fns the old scans called (so targets are identical).
//! The container-info copy-row hover stays DetailTable-resolved: its
//! per-row rects live inside the widget-owned `DetailTableState` (the
//! upstream-sanctioned widget-owns-input route), and extracting them
//! would mean touching `jackin-tui`, which this plan's scope forbids.

use ratatui::layout::Position;
use termrock::interaction::HitRegion;

use super::{
    FileBrowserState, ManagerEffect, ManagerListRow, ManagerStage, ManagerState, Modal, MouseEvent,
    Rect, apply_workspace_list_hover_target, editor_mount_hover_target_at_position,
    editor_scroll_area, settings_trust_hover_target_at_position, split_seam_column,
    workspace_list_hover_row_at_position,
};
use crate::tui::layout::{
    LIST_FOOTER_HEIGHT, LIST_HEADER_HEIGHT, SCREEN_HEADER_HEIGHT, TAB_STRIP_HEIGHT,
};
use crate::tui::state::{
    EditorHoverTarget, EditorTab, ManagerHoverTarget, SettingsHoverTarget, SettingsTab,
};

/// Stable hover-target identity across every console stage — the `Id`
/// type of the consumer `HoverState` on `ManagerState`.
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

/// Brighten the hovered copyable row in the Debug info dialog (link hover cue),
/// mirroring the launch cockpit. No-op unless that modal is open.
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

/// Resolve the active file-browser modal and its state from whichever stage
/// owns it (editor or create-prelude). Shared by the URL-row hit-test and the
/// click handler so their modal resolution can't drift out of step.
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

/// Whether the pointer is over a file-browser git-prompt URL row (side-effect
/// free; does not open the URL).
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
/// Per-Moved-event hover: rebuild the hover regions from the existing
/// pure geometry, feed the consumer `HoverState`, then apply the cached
/// target through the same setters the old scans used. A miss clears
/// every stage hover (the old explicit clear arms).
pub fn update_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    let regions = hover_regions(state, term_size);
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

/// Build the hover region list for the active stage. Region groups are
/// disjoint (tab strip vs content rows), and each per-line probe calls
/// the same position→target fn the click/cue paths use, so the hit set
/// is byte-identical to the old scans by construction. Modal-open guards
/// match the old scans: they empty the region list.
fn hover_regions(state: &ManagerState<'_>, term_size: Rect) -> Vec<HitRegion<ConsoleHoverTarget>> {
    let mut regions = Vec::new();
    match &state.stage {
        ManagerStage::Editor(editor) => {
            if editor.modal.is_none() {
                push_tab_regions(&mut regions, &EditorTab::ALL.map(EditorTab::label), |idx| {
                    ConsoleHoverTarget::Editor(EditorHoverTarget::Tab(idx))
                });
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
                push_tab_regions(
                    &mut regions,
                    &SettingsTab::ALL.map(SettingsTab::label),
                    |idx| ConsoleHoverTarget::Settings(SettingsHoverTarget::Tab(idx)),
                );
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

/// One region per tab cell, replicating `tab_cell_at_position`'s band
/// (`SCREEN_HEADER_HEIGHT..+TAB_STRIP_HEIGHT`) and the upstream
/// `lay_out_tabs` cell columns it delegates to.
fn push_tab_regions(
    regions: &mut Vec<HitRegion<ConsoleHoverTarget>>,
    labels: &[&str],
    target: impl Fn(usize) -> ConsoleHoverTarget,
) {
    let cells: Vec<(&str, bool)> = labels.iter().map(|label| (*label, false)).collect();
    for (idx, cell) in termrock::widgets::lay_out_tabs(&cells, 0)
        .iter()
        .enumerate()
    {
        regions.push(HitRegion {
            id: target(idx),
            area: Rect::new(
                cell.start_col,
                SCREEN_HEADER_HEIGHT,
                cell.cell_cols,
                TAB_STRIP_HEIGHT,
            ),
        });
    }
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
