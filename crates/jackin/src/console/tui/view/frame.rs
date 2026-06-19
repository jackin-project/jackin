//! Top-level frame render: compose sidebar, main area, and footer into one `ratatui` frame.
//!
//! Not responsible for: state mutation, event handling, or individual component
//! rendering — delegates to sub-modules in `view/` and `components/`.

use ratatui::{Frame, layout::Rect};

use crate::console::tui::components::footer::editor::editor_footer_items;
use crate::console::tui::components::footer::settings::settings_footer_items;
use crate::console::tui::components::modal::render_modal;
use crate::console::tui::components::workspace_list::render_list_body;
use crate::console::tui::state::{ManagerStage, ManagerState};
use jackin_config::AppConfig;
use jackin_console::tui::screens::workspaces::view::footer::workspace_screen_footer_items_for_state;
use jackin_console::tui::screens::editor::view::editor_frame_areas;
use jackin_console::tui::screens::settings::view::{
    SettingsModalRenderPlan, render_global_mount_modal, render_settings_auth_modal,
    render_settings_env_modal, settings_frame_areas, settings_modal_render_plan,
};
use jackin_console::tui::view::{
    ConsoleMainFramePlan, ConsoleModalRenderPlan, ConsoleReservedFooterHeightPlan,
    ReservedFooterHeightFacts, console_main_frame_plan, console_modal_render_plan,
    console_reserved_footer_height_plan, delete_confirm_area, effective_footer_height,
    measured_footer_height, modal_backdrop_area, modal_overlay_state_for_route,
    modal_overlay_visible, purge_confirm_area, render_footer, render_header, render_modal_backdrop,
    reserved_footer_height_for_facts, settings_error_area, status_overlay_area,
    workspace_frame_areas, workspace_header_title,
};
use super::{editor, settings};

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    match console_main_frame_plan(state.stage.route()) {
        ConsoleMainFramePlan::Editor => {
            if let ManagerStage::Editor(editor) = &state.stage {
                editor::render_editor(frame, area, editor, config, state.op_available);
            }
        }
        ConsoleMainFramePlan::Settings => {
            if let ManagerStage::Settings(settings) = &state.stage {
                settings::render_settings(frame, area, settings, state.op_available);
            }
        }
        ConsoleMainFramePlan::Workspace {
            render_list_body: show_list_body,
        } => {
            let areas = workspace_frame_areas(area);

            render_header(frame, areas.header, workspace_header_title());

            if show_list_body {
                render_list_body(frame, areas.body, state, config, cwd);
            }

            render_footer(
                frame,
                areas.footer,
                &workspace_screen_footer_items_for_state(state, config, cwd, area),
            );
        }
    }

    if has_modal_overlay(state) {
        // The backdrop must not cover the reserved footer — hints stay visible
        // there (the footer is inviolable).
        let footer_h = reserved_footer_height(state, config, area);
        render_modal_backdrop(frame, modal_backdrop_area(area, footer_h));
    }

    match console_modal_render_plan(state.stage.route()) {
        ConsoleModalRenderPlan::List => {
            if let Some(modal) = &state.list_modal {
                render_modal(frame, modal);
            }
        }
        ConsoleModalRenderPlan::Editor => {
            if let ManagerStage::Editor(editor) = &state.stage
                && let Some(modal) = &editor.modal
            {
                render_modal(frame, modal);
            }
        }
        ConsoleModalRenderPlan::CreatePrelude => {
            if let ManagerStage::CreatePrelude(prelude) = &state.stage
                && let Some(modal) = &prelude.modal
            {
                render_modal(frame, modal);
            }
        }
        ConsoleModalRenderPlan::ConfirmDelete => {
            if let ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } = &state.stage
            {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let modal_area = delete_confirm_area(area);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
        }
        ConsoleModalRenderPlan::ConfirmInstancePurge => {
            if let ManagerStage::ConfirmInstancePurge {
                state: confirm_state,
                ..
            } = &state.stage
            {
                // The two-line prompt is taller than ConfirmDelete's
                // single line, so allocate more rows for the modal.
                let modal_area = purge_confirm_area(area);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
        }
        ConsoleModalRenderPlan::Settings => {
            if let ManagerStage::Settings(settings) = &state.stage {
                match settings_modal_render_plan(
                    settings.error_popup.is_some(),
                    settings.mounts.modal.is_some(),
                    settings.env.modal.is_some(),
                    settings.auth.modal_ref().is_some(),
                ) {
                    SettingsModalRenderPlan::ErrorPopup => {
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
                        }
                    }
                    SettingsModalRenderPlan::Mounts => {
                        if let Some(modal) = &settings.mounts.modal {
                            render_global_mount_modal(frame, modal);
                        }
                    }
                    SettingsModalRenderPlan::Environments => {
                        if let Some(modal) = &settings.env.modal {
                            render_settings_env_modal(frame, modal);
                        }
                    }
                    SettingsModalRenderPlan::Auth => {
                        if let Some(modal) = settings.auth.modal_ref() {
                            render_settings_auth_modal(frame, modal);
                        }
                    }
                    SettingsModalRenderPlan::None => {}
                }
            }
        }
    }

    if let Some(overlay) = &state.status_overlay {
        let overlay_area = status_overlay_area(area);
        jackin_tui::components::render_status_popup(frame, overlay_area, overlay);
    }
}

/// Rows the current screen reserves for its footer — excluded from the modal
/// backdrop so the hints stay visible. Editor/settings size theirs to the hint
/// content; the workspace footer is fixed.
fn reserved_footer_height(state: &ManagerState<'_>, config: &AppConfig, area: Rect) -> u16 {
    let mut facts = ReservedFooterHeightFacts {
        editor_footer_height: None,
        settings_footer_height: None,
        workspace_footer_height: workspace_frame_areas(area).footer.height,
    };
    match console_reserved_footer_height_plan(state.stage.route()) {
        ConsoleReservedFooterHeightPlan::Editor => {
            if let ManagerStage::Editor(editor) = &state.stage {
                let body =
                    editor_frame_areas(area, effective_footer_height(editor.cached_footer_h)).body;
                facts.editor_footer_height = Some(measured_footer_height(
                    &editor_footer_items(editor, config, state.op_available, body),
                    area.width,
                ));
            }
        }
        ConsoleReservedFooterHeightPlan::Settings => {
            if let ManagerStage::Settings(settings) = &state.stage {
                let body =
                    settings_frame_areas(area, effective_footer_height(settings.cached_footer_h))
                        .body;
                facts.settings_footer_height = Some(measured_footer_height(
                    &settings_footer_items(settings, state.op_available, body),
                    area.width,
                ));
            }
        }
        ConsoleReservedFooterHeightPlan::Workspace => {}
    }
    reserved_footer_height_for_facts(facts)
}

fn has_modal_overlay(state: &ManagerState<'_>) -> bool {
    modal_overlay_visible(modal_overlay_state_for_route(
        state.stage.route(),
        state.status_overlay.is_some(),
        state.list_modal.is_some(),
        state.stage.modal_facts(),
    ))
}
