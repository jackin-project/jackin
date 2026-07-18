// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Hover-state update helpers: container-info hover, file-browser
//! pointer position, list-row hover targets.

use super::{
    FileBrowserState, ManagerEffect, ManagerListRow, ManagerStage, ManagerState, Modal, MouseEvent,
    Rect, apply_workspace_list_hover_target, editor_mount_hover_target_at_position,
    editor_scroll_area, settings_trust_hover_target_at_position, split_seam_column,
    workspace_list_hover_row_at_position,
};

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
/// Track the list row under the pointer so the renderer can lift its
/// background, mirroring the tab-hover cue. Cleared when off the list pane,
/// over the seam, or when a list modal is open.
pub fn update_list_row_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    apply_workspace_list_hover_target(
        state,
        list_row_hover_at(state, mouse, term_size)
            .map(crate::tui::screens::workspaces::model::ManagerHoverTarget::ListRow),
    );
}

/// Track the hovered row on the editor Mounts tab and the Settings Trust tab so
/// their renderers can lift it, mirroring the tab/list hover cue. Cleared off
/// the relevant content area.
pub fn update_row_hover(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
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
                settings.mounts.modals.is_open(),
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
