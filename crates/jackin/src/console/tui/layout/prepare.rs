//! Manager-owned state preparation before drawing.

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::tui::components::footer;
use crate::console::tui::components::modal_layout::modal_outer_rect;
use crate::console::tui::layout::editor::prepare_editor_for_render;
use crate::console::tui::layout::list::clamp_list_scroll_for_area;
use crate::console::tui::layout::settings::clamp_global_mounts_scroll_for_frame;
use crate::console::tui::state::{GlobalMountModal, ManagerStage, ManagerState, Modal};
use jackin_console::tui::screens::settings::view::settings_frame_areas;
use jackin_console::tui::view::{footer_height, workspace_frame_areas};

pub fn prepare_for_render(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    area: Rect,
) {
    state.cached_term_size = area;
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let footer = footer::editor::editor_footer_items(editor, config, state.op_available);
            editor.cached_footer_h = footer_height(&footer, area.width).max(1);
            prepare_editor_for_render(area, editor, config);
        }
        ManagerStage::Settings(settings) => {
            let body = settings_frame_areas(area, settings.cached_footer_h.max(1)).body;
            let footer =
                footer::settings::settings_footer_items(settings, state.op_available, body);
            settings.cached_footer_h = footer_height(&footer, area.width).max(1);
            clamp_global_mounts_scroll_for_frame(area, &mut settings.mounts);
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
    let list_footer_h = workspace_frame_areas(area).footer.height;
    let content_for_modal = |footer_h: u16| Rect {
        height: area.height.saturating_sub(footer_h),
        ..area
    };

    if let Some(modal) = &mut state.list_modal {
        prepare_modal(content_for_modal(list_footer_h), modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let content = content_for_modal(editor.cached_footer_h);
            if let Some(modal) = &mut editor.modal {
                prepare_modal(content, modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = &mut prelude.modal {
                prepare_modal(content_for_modal(list_footer_h), modal);
            }
        }
        ManagerStage::Settings(settings) => {
            let content = content_for_modal(settings.cached_footer_h);
            if let Some(GlobalMountModal::PreviewSave { state }) = &mut settings.mounts.modal {
                use jackin_console::tui::components::confirm_save;
                let height = confirm_save::required_height(state).min(content.height);
                let modal_area =
                    jackin_console::tui::layout::centered_rect_fixed(content, 80, height);
                confirm_save::prepare_for_render(modal_area, state);
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
}

fn prepare_modal(outer: Rect, modal: &mut Modal<'_>) {
    let modal_area = modal_outer_rect(modal, outer);
    if let Modal::ConfirmSave { state } = modal {
        jackin_console::tui::components::confirm_save::prepare_for_render(modal_area, state);
    }
}
