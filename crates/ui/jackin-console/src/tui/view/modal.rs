// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Modal rendering and preparation.

use super::{
    ConsolePrepareFramePlan, StageModalArea, console_prepare_frame_plan, effective_footer_height,
    measured_footer_height, visible_modal_prepare_areas_for_stage_facts, workspace_frame_areas,
};
use ratatui::{Frame, layout::Rect};

/// Render the active modal overlay for the current console state.
///
/// Dispatches to the appropriate component renderer based on the `Modal` variant.
/// The modal area has already been computed by `prepare_for_render` and stored
/// on the modal via `modal.prepare_for_render`.
pub fn render_modal(frame: &mut Frame<'_>, modal: &crate::tui::state::Modal<'_>) {
    use crate::tui::state::Modal;

    let area = frame.area();
    let modal_area = modal.rect(area);
    match modal {
        Modal::TextInput { state, .. } => {
            crate::tui::components::render_text_input(frame, modal_area, state);
        }
        Modal::FileBrowser { state, .. } => {
            crate::tui::components::file_browser::render(frame, modal_area, state);
        }
        Modal::WorkdirPick { state } => {
            crate::tui::components::workdir_pick::render(frame, modal_area, state);
        }
        Modal::Confirm { state, .. } => {
            crate::tui::components::render_confirm_dialog(frame, modal_area, state);
        }
        Modal::SaveDiscardCancel { state } => {
            crate::tui::components::render_save_discard_dialog(frame, modal_area, state);
        }
        Modal::MountDstChoice { state, .. } => {
            crate::tui::components::mount_dst_choice::render(frame, modal_area, state);
        }
        Modal::GithubPicker { state } => {
            crate::tui::components::github_picker::render(frame, modal_area, state);
        }
        Modal::ConfirmSave { state } => {
            crate::tui::components::confirm_save::render(frame, modal_area, state);
        }
        Modal::ErrorPopup { state } => {
            crate::tui::components::render_error_dialog(frame, modal_area, state);
        }
        Modal::ContainerInfo { state } => {
            crate::tui::components::container_info_surface::render_container_info(
                frame, modal_area, state,
            );
        }
        Modal::StatusPopup { state } => {
            crate::tui::components::render_status_popup(frame, modal_area, state);
        }
        Modal::OpPicker { state, .. } => {
            crate::tui::components::op_picker::render_picker(frame, modal_area, state.as_ref());
        }
        Modal::RolePicker { state } | Modal::RoleOverridePicker { state } => {
            crate::tui::components::role_picker::render(frame, modal_area, state);
        }
        Modal::SourcePicker { state, .. } | Modal::AuthSourcePicker { state } => {
            crate::tui::components::source_picker::render(frame, modal_area, state);
        }
        Modal::ScopePicker { state } => {
            crate::tui::components::scope_picker::render(frame, modal_area, state);
        }
        Modal::AuthForm { state, focus, .. } => {
            crate::tui::components::auth_panel::render_form(
                frame,
                modal_area,
                state.as_ref(),
                *focus,
            );
        }
    }
}

/// Prepare `state` for the next render pass.
///
/// Must be called once before `render` each frame. Computes and caches footer
/// heights, clamps all scroll offsets to the current terminal area, and
/// positions modals within the drawable content area.
pub fn prepare_for_render(
    state: &mut crate::tui::state::ManagerState<'_>,
    config: &jackin_config::AppConfig,
    cwd: &std::path::Path,
    area: Rect,
) {
    use crate::tui::components::footer_hints::editor_footer_items;
    use crate::tui::layout::list::clamp_list_scroll_for_area;
    use crate::tui::model::ConsoleManagerStage;
    use crate::tui::screens::editor::view::{editor_frame_areas, prepare_editor_for_render};
    use crate::tui::screens::settings::view::{
        settings_frame_areas, settings_screen_footer_for_state,
    };

    state.cached_term_size = area;
    match console_prepare_frame_plan(state.stage.route()) {
        ConsolePrepareFramePlan::Editor => {
            if let ConsoleManagerStage::Editor(editor) = &mut state.stage {
                let body =
                    editor_frame_areas(area, effective_footer_height(editor.cached_footer_h)).body;
                let footer = editor_footer_items(editor, config, state.op_available, body);
                editor.cached_footer_h = measured_footer_height(&footer, area.width);
                prepare_editor_for_render(area, editor, config);
            }
        }
        ConsolePrepareFramePlan::Settings => {
            if let ConsoleManagerStage::Settings(settings) = &mut state.stage {
                let body =
                    settings_frame_areas(area, effective_footer_height(settings.cached_footer_h))
                        .body;
                let footer = settings_screen_footer_for_state(settings, state.op_available, body);
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

pub(crate) fn prepare_visible_modal(area: Rect, state: &mut crate::tui::state::ManagerState<'_>) {
    use crate::tui::model::ConsoleManagerStage;

    let areas = visible_modal_prepare_areas_for_stage_facts(
        area,
        state
            .stage
            .footer_height_facts(workspace_frame_areas(area).footer.height),
    );

    if let Some(modal) = &mut state.list_modal {
        modal.prepare_for_render(areas.list_modal);
    }
    if let Some(area) = areas.stage_modal {
        match (&mut state.stage, area) {
            (ConsoleManagerStage::Editor(editor), StageModalArea::Editor(area)) => {
                if let Some(modal) = &mut editor.modal {
                    modal.prepare_for_render(area);
                }
            }
            (ConsoleManagerStage::CreatePrelude(prelude), StageModalArea::Workspace(area)) => {
                if let Some(modal) = &mut prelude.modal {
                    modal.prepare_for_render(area);
                }
            }
            (ConsoleManagerStage::Settings(settings), StageModalArea::Settings(area)) => {
                if let Some(modal) = settings.mounts.modals.current_mut() {
                    modal.prepare_for_render(area);
                }
            }
            _ => {}
        }
    }
}
