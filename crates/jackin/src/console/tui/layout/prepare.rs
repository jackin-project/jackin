//! Manager-owned state preparation before drawing.

use ratatui::layout::Rect;

use crate::console::tui::components::footer;
use crate::console::tui::layout::list::clamp_list_scroll_for_area;
use crate::console::tui::state::{ManagerStage, ManagerState};
use jackin_config::AppConfig;
use jackin_console::tui::screens::editor::view::editor_frame_areas;
use jackin_console::tui::screens::settings::view::settings_frame_areas;
use jackin_console::tui::view::{
    ConsolePrepareFramePlan, StageModalArea, console_prepare_frame_plan, effective_footer_height,
    measured_footer_height, visible_modal_prepare_areas_for_stage_facts, workspace_frame_areas,
};

pub fn prepare_for_render(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    area: Rect,
) {
    state.cached_term_size = area;
    match console_prepare_frame_plan(state.stage.route()) {
        ConsolePrepareFramePlan::Editor => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                let body =
                    editor_frame_areas(area, effective_footer_height(editor.cached_footer_h)).body;
                let footer =
                    footer::editor::editor_footer_items(editor, config, state.op_available, body);
                editor.cached_footer_h = measured_footer_height(&footer, area.width);
                jackin_console::tui::screens::editor::view::prepare_editor_for_render(
                    area, editor, config,
                );
            }
        }
        ConsolePrepareFramePlan::Settings => {
            if let ManagerStage::Settings(settings) = &mut state.stage {
                let body =
                    settings_frame_areas(area, effective_footer_height(settings.cached_footer_h))
                        .body;
                let footer =
                    footer::settings::settings_footer_items(settings, state.op_available, body);
                settings.cached_footer_h = measured_footer_height(&footer, area.width);
                settings.clamp_mounts_scroll_for_frame(area);
            }
        }
        ConsolePrepareFramePlan::List => {
            let areas = workspace_frame_areas(area);
            clamp_list_scroll_for_area(areas.body, state, config, cwd);
        }
        ConsolePrepareFramePlan::None => {}
    }
    prepare_visible_modal(area, state);
}

fn prepare_visible_modal(area: Rect, state: &mut ManagerState<'_>) {
    // Modals must never overlap the reserved status/hint bar at the bottom.
    // Compute the content area (full terminal minus the footer rows) and
    // center/clamp all modals within it.
    let areas = visible_modal_prepare_areas_for_stage_facts(
        area,
        state
            .stage
            .footer_height_facts(workspace_frame_areas(area).footer.height),
    );

    if let Some(modal) = &mut state.list_modal {
        prepare_modal(areas.list_modal, modal);
    }
    if let Some(area) = areas.stage_modal {
        match (&mut state.stage, area) {
            (ManagerStage::Editor(editor), StageModalArea::Editor(area)) => {
                if let Some(modal) = &mut editor.modal {
                    prepare_modal(area, modal);
                }
            }
            (ManagerStage::CreatePrelude(prelude), StageModalArea::Workspace(area)) => {
                if let Some(modal) = &mut prelude.modal {
                    prepare_modal(area, modal);
                }
            }
            (ManagerStage::Settings(settings), StageModalArea::Settings(area)) => {
                if let Some(modal) = &mut settings.mounts.modal {
                    modal.prepare_for_render(area);
                }
            }
            _ => {}
        }
    }
}

fn prepare_modal(outer: Rect, modal: &mut crate::console::tui::state::Modal<'_>) {
    modal.prepare_for_render(outer);
}
