// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Top-level console frame rendering.

use super::{
    ConsoleMainFramePlan, ConsoleModalRenderPlan, console_main_frame_plan,
    console_modal_render_plan, delete_confirm_area, has_modal_overlay, modal_backdrop_area,
    purge_confirm_area, render_footer, render_header, render_modal, render_modal_backdrop,
    render_usage_surface, reserved_footer_height, settings_error_area, status_overlay_area,
    workspace_frame_areas, workspace_header_title,
};
use ratatui::{Frame, layout::Rect};

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::state::ManagerState<'_>,
    config: &jackin_config::AppConfig,
    cwd: &std::path::Path,
) {
    use crate::tui::screens::workspaces::view::footer::workspace_screen_footer_items_for_state;

    match console_main_frame_plan(state.stage.route()) {
        ConsoleMainFramePlan::Editor => {
            if let crate::tui::state::ManagerStage::Editor(editor) = &state.stage {
                crate::tui::screens::editor::view::render_editor_with_footer(
                    frame,
                    area,
                    editor,
                    config,
                    state.op_available,
                );
            }
        }
        ConsoleMainFramePlan::Settings => {
            if let crate::tui::state::ManagerStage::Settings(settings) = &state.stage {
                crate::tui::screens::settings::view::render_settings_with_footer(
                    frame,
                    area,
                    settings,
                    state.op_available,
                );
            }
        }
        ConsoleMainFramePlan::Workspace {
            render_list_body: show_list_body,
        } => {
            let areas = workspace_frame_areas(area);

            if state.usage.visible {
                render_usage_surface(frame, area, state);
            } else {
                render_header(frame, areas.header, workspace_header_title());
            }

            if show_list_body && !state.usage.visible {
                crate::tui::screens::workspaces::view::list::render_list_body(
                    frame, areas.body, state, config, cwd,
                );
            }

            if !state.usage.visible {
                render_footer(
                    frame,
                    areas.footer,
                    &workspace_screen_footer_items_for_state(state, config, cwd, area),
                );
            }
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
            if let crate::tui::state::ManagerStage::Editor(editor) = &state.stage
                && let Some(modal) = &editor.modal
            {
                render_modal(frame, modal);
            }
        }
        ConsoleModalRenderPlan::CreatePrelude => {
            if let crate::tui::state::ManagerStage::CreatePrelude(prelude) = &state.stage
                && let Some(modal) = &prelude.modal
            {
                render_modal(frame, modal);
            }
        }
        ConsoleModalRenderPlan::ConfirmDelete => {
            if let crate::tui::state::ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } = &state.stage
            {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let modal_area = delete_confirm_area(area);
                crate::tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
        }
        ConsoleModalRenderPlan::ConfirmInstancePurge => {
            if let crate::tui::state::ManagerStage::ConfirmInstancePurge {
                state: confirm_state,
                ..
            } = &state.stage
            {
                // The two-line prompt is taller than ConfirmDelete's
                // single line, so allocate more rows for the modal.
                let modal_area = purge_confirm_area(area);
                crate::tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
        }
        ConsoleModalRenderPlan::Settings => {
            if let crate::tui::state::ManagerStage::Settings(settings) = &state.stage {
                use crate::tui::screens::settings::view::{
                    SettingsModalRenderPlan, render_global_mount_modal, render_settings_auth_modal,
                    render_settings_env_modal, settings_modal_render_plan,
                };
                match settings_modal_render_plan(
                    settings.error_popup.is_some(),
                    settings.mounts.modals.is_open(),
                    settings.env.modals.is_open(),
                    settings.auth.modal_ref().is_some(),
                ) {
                    SettingsModalRenderPlan::ErrorPopup => {
                        if let Some(popup) = &settings.error_popup {
                            let inner_width = (area.width * 60 / 100).saturating_sub(4);
                            let max_rows = area.height.saturating_sub(2);
                            let h = popup.required_height(inner_width, max_rows);
                            let popup_area = settings_error_area(area, h);
                            crate::tui::components::render_error_dialog(frame, popup_area, popup);
                        }
                    }
                    SettingsModalRenderPlan::Mounts => {
                        if let Some(modal) = settings.mounts.modals.current() {
                            render_global_mount_modal(frame, modal);
                        }
                    }
                    SettingsModalRenderPlan::Environments => {
                        if let Some(modal) = settings.env.modals.current() {
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
        crate::tui::components::render_status_popup(frame, overlay_area, overlay);
    }

    if let Some(help) = &state.keyboard_help {
        // The help overlay never coexists with another modal (input dispatch
        // precedence), so it draws its own backdrop here rather than widening
        // `has_modal_overlay`.
        let footer_h = reserved_footer_height(state, config, area);
        render_modal_backdrop(frame, modal_backdrop_area(area, footer_h));
        let system = termrock::style::DesignSystem::default();
        let entries = crate::tui::components::keyboard_help::console_help_entries(state, &system);
        let rect = termrock::widgets::place_keyboard_help(
            modal_backdrop_area(area, footer_h),
            termrock::widgets::KeyboardHelpSize::default(),
        );
        let mut paint = help.clone();
        termrock::widgets::KeyboardHelp::new(&entries, &system)
            .title("Keyboard shortcuts")
            .paint(rect, frame.buffer_mut(), &mut paint);
    }
}
