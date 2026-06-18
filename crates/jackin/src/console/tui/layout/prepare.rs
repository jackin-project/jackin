//! Manager-owned state preparation before drawing.

use ratatui::layout::Rect;

use crate::console::tui::components::footer;
use crate::console::tui::layout::list::clamp_list_scroll_for_area;
use crate::console::tui::state::{ManagerStage, ManagerState};
use jackin_config::AppConfig;
use jackin_console::tui::screens::editor::view::editor_frame_areas;
use jackin_console::tui::screens::settings::view::settings_frame_areas;
use jackin_console::tui::view::{footer_height, modal_content_areas, workspace_frame_areas};

pub fn prepare_for_render(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    area: Rect,
) {
    state.cached_term_size = area;
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let body = editor_frame_areas(area, editor.cached_footer_h.max(1)).body;
            let footer =
                footer::editor::editor_footer_items(editor, config, state.op_available, body);
            editor.cached_footer_h = footer_height(&footer, area.width).max(1);
            jackin_console::tui::screens::editor::view::prepare_editor_for_render(
                area, editor, config,
            );
        }
        ManagerStage::Settings(settings) => {
            let body = settings_frame_areas(area, settings.cached_footer_h.max(1)).body;
            let footer =
                footer::settings::settings_footer_items(settings, state.op_available, body);
            settings.cached_footer_h = footer_height(&footer, area.width).max(1);
            settings.clamp_mounts_scroll_for_frame(area);
        }
        ManagerStage::List => {
            let areas = workspace_frame_areas(area);
            clamp_list_scroll_for_area(areas.body, state, config, cwd);
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    prepare_visible_modal(area, state);
}

fn prepare_visible_modal(area: Rect, state: &mut ManagerState<'_>) {
    // Modals must never overlap the reserved status/hint bar at the bottom.
    // Compute the content area (full terminal minus the footer rows) and
    // center/clamp all modals within it.
    let content_areas = modal_content_areas(
        area,
        workspace_frame_areas(area).footer.height,
        editor_footer_height(state),
        settings_footer_height(state),
    );

    if let Some(modal) = &mut state.list_modal {
        prepare_modal(content_areas.workspace, modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = &mut editor.modal {
                prepare_modal(content_areas.editor, modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = &mut prelude.modal {
                prepare_modal(content_areas.workspace, modal);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = &mut settings.mounts.modal {
                modal.prepare_for_render(content_areas.settings);
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
}

fn prepare_modal(outer: Rect, modal: &mut crate::console::tui::state::Modal<'_>) {
    modal.prepare_for_render(outer);
}

fn editor_footer_height(state: &ManagerState<'_>) -> u16 {
    match &state.stage {
        ManagerStage::Editor(editor) => editor.cached_footer_h,
        _ => 0,
    }
}

fn settings_footer_height(state: &ManagerState<'_>) -> u16 {
    match &state.stage {
        ManagerStage::Settings(settings) => settings.cached_footer_h,
        _ => 0,
    }
}
