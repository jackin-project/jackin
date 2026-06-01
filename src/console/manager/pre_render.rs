//! Manager-owned state preparation before drawing.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::AppConfig;
use crate::console::manager::editor_geometry::prepare_editor_for_render;
use crate::console::manager::list_geometry::clamp_list_scroll_for_area;
use crate::console::manager::modal_layout::modal_outer_rect;
use crate::console::manager::settings_geometry::clamp_global_mounts_scroll_for_frame;
use crate::console::manager::state::{GlobalMountModal, ManagerStage, ManagerState, Modal};

pub fn prepare_for_render(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    area: Rect,
) {
    state.cached_term_size = area;
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let footer = crate::console::manager::editor_footer::editor_footer_items(
                editor,
                config,
                state.op_available,
            );
            editor.cached_footer_h = footer_height(&footer, area.width).max(1);
            prepare_editor_for_render(area, editor, config);
        }
        ManagerStage::Settings(settings) => {
            let footer = crate::console::manager::settings_footer::settings_footer_items(
                settings,
                state.op_available,
            );
            settings.cached_footer_h = footer_height(&footer, area.width).max(1);
            clamp_global_mounts_scroll_for_frame(area, &mut settings.mounts);
        }
        ManagerStage::List => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Min(10),
                    Constraint::Length(2),
                ])
                .split(area);
            clamp_list_scroll_for_area(chunks[1], state, config, cwd);
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    prepare_visible_modal(area, state);
}

fn footer_height(items: &[jackin_tui::HintSpan<'_>], width: u16) -> u16 {
    jackin_tui::components::wrapped_height(items, width)
}

fn prepare_visible_modal(area: Rect, state: &mut ManagerState<'_>) {
    if let Some(modal) = &mut state.list_modal {
        prepare_modal(area, modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = &mut editor.modal {
                prepare_modal(area, modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = &mut prelude.modal {
                prepare_modal(area, modal);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(GlobalMountModal::PreviewSave { state }) = &mut settings.mounts.modal {
                use jackin_console::widgets::confirm_save;
                let height = confirm_save::required_height(state).min(area.height);
                let modal_area = jackin_console::tui::layout::centered_rect_fixed(area, 80, height);
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
    match modal {
        Modal::ConfirmSave { state } => {
            jackin_console::widgets::confirm_save::prepare_for_render(modal_area, state);
        }
        _ => {}
    }
}
