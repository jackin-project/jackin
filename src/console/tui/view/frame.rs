use ratatui::{Frame, layout::Rect};

use crate::config::AppConfig;
use crate::console::tui::components::footer::workspace_list_footer_items_for_state;
use crate::console::tui::state::{ManagerStage, ManagerState};
use jackin_console::tui::components::footer_hints::{
    create_prelude_footer_items, destructive_confirm_footer_items,
};
use jackin_console::tui::view::{
    ModalOverlayState, delete_confirm_area, modal_overlay_visible, purge_confirm_area,
    render_footer, render_header, render_modal_backdrop, settings_error_area, status_overlay_area,
    workspace_frame_areas,
};
use jackin_tui::HintSpan;

use super::{editor, list, modal, settings};

#[allow(clippy::too_many_lines)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    if let ManagerStage::Editor(editor) = &state.stage {
        editor::render_editor(frame, area, editor, config, state.op_available);
    } else if let ManagerStage::Settings(settings) = &state.stage {
        settings::render_settings(frame, area, settings, state.op_available);
    } else {
        let areas = workspace_frame_areas(area);

        render_header(frame, areas.header, "workspaces");

        if matches!(&state.stage, ManagerStage::List) {
            list::render_list_body(frame, areas.body, state, config, cwd);
        }

        let footer_items: Vec<HintSpan<'static>> = match &state.stage {
            ManagerStage::List => workspace_list_footer_items_for_state(state, config),
            ManagerStage::CreatePrelude(_) => create_prelude_footer_items(),
            ManagerStage::ConfirmDelete { .. } | ManagerStage::ConfirmInstancePurge { .. } => {
                destructive_confirm_footer_items()
            }
            ManagerStage::Editor(_) => unreachable!("Editor has its own render path"),
            ManagerStage::Settings(_) => unreachable!("Settings has its own render path"),
        };
        render_footer(frame, areas.footer, &footer_items);
    }

    if has_modal_overlay(state) {
        render_modal_backdrop(frame, area);
    }

    // List-anchored modal lives on `ManagerState`, not on a stage
    // variant, so the borrow splits separately from stage-anchored
    // modals.
    let is_list_stage = matches!(state.stage, ManagerStage::List);
    if is_list_stage {
        if let Some(modal) = &state.list_modal {
            modal::render_modal(frame, modal);
        }
    } else {
        match &state.stage {
            ManagerStage::Editor(editor) => {
                if let Some(modal) = &editor.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::CreatePrelude(prelude) => {
                if let Some(modal) = &prelude.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } => {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let modal_area = delete_confirm_area(area);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
            ManagerStage::ConfirmInstancePurge {
                state: confirm_state,
                ..
            } => {
                // The two-line prompt is taller than ConfirmDelete's
                // single line, so allocate more rows for the modal.
                let modal_area = purge_confirm_area(area);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
            ManagerStage::List => {
                // Handled above via the `is_list_stage` early branch.
            }
            ManagerStage::Settings(settings) => {
                if let Some(popup) = &settings.error_popup {
                    let inner_width = (area.width * 60 / 100).saturating_sub(4);
                    let max_rows = area.height.saturating_sub(2);
                    let h = jackin_tui::components::error_dialog::required_height(
                        popup,
                        inner_width,
                        max_rows,
                    );
                    let popup_area = settings_error_area(area, h);
                    jackin_tui::components::render_error_dialog(frame, popup_area, popup);
                } else if let Some(modal) = &settings.mounts.modal {
                    settings::render_global_mount_modal(frame, modal);
                } else if let Some(modal) = &settings.env.modal {
                    settings::render_settings_env_modal(frame, modal);
                } else if let Some(modal) = &settings.auth.modal {
                    settings::render_settings_auth_modal(frame, modal);
                }
            }
        }
    }

    if let Some(overlay) = &state.status_overlay {
        let overlay_area = status_overlay_area(area);
        jackin_tui::components::render_status_popup(frame, overlay_area, overlay);
    }
}

fn has_modal_overlay(state: &ManagerState<'_>) -> bool {
    let mut overlay = ModalOverlayState {
        status_overlay: state.status_overlay.is_some(),
        ..ModalOverlayState::default()
    };
    match &state.stage {
        ManagerStage::List => overlay.list_modal = state.list_modal.is_some(),
        ManagerStage::Editor(editor) => overlay.editor_modal = editor.modal.is_some(),
        ManagerStage::Settings(settings) => {
            overlay.settings_error = settings.error_popup.is_some();
            overlay.settings_mounts_modal = settings.mounts.modal.is_some();
            overlay.settings_env_modal = settings.env.modal.is_some();
            overlay.settings_auth_modal = settings.auth.modal.is_some();
        }
        ManagerStage::CreatePrelude(prelude) => {
            overlay.create_prelude_modal = prelude.modal.is_some();
        }
        ManagerStage::ConfirmDelete { .. } | ManagerStage::ConfirmInstancePurge { .. } => {
            overlay.destructive_confirm = true;
        }
    }
    modal_overlay_visible(overlay)
}
