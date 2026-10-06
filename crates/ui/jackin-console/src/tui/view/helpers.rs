// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage surface and footer-height helpers.

use super::{
    ConsoleReservedFooterHeightPlan, ReservedFooterHeightFacts,
    console_reserved_footer_height_plan, effective_footer_height, measured_footer_height,
    modal_overlay_state_for_route, modal_overlay_visible, render_footer, render_header,
    reserved_footer_height_for_facts, workspace_frame_areas,
};
use ratatui::{Frame, layout::Rect};

pub(crate) fn render_usage_surface(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::state::ManagerState<'_>,
) {
    let areas = workspace_frame_areas(area);
    render_header(frame, areas.header, "usage");
    crate::tui::screens::usage::render(frame, area, state);
    let hints = [
        termrock::widgets::HintSpan::Key("↑↓"),
        termrock::widgets::HintSpan::Text(" select  "),
        termrock::widgets::HintSpan::Key("↵"),
        termrock::widgets::HintSpan::Text(" detail  "),
        termrock::widgets::HintSpan::Key("Esc"),
        termrock::widgets::HintSpan::Text(" back"),
    ];
    render_footer(frame, areas.footer, &hints);
}

/// Rows the current screen reserves for its footer — excluded from the modal
/// backdrop so the hints stay visible. Editor/settings size theirs to the hint
/// content; the workspace footer is fixed.
pub(crate) fn reserved_footer_height(
    state: &crate::tui::state::ManagerState<'_>,
    config: &jackin_config::AppConfig,
    area: Rect,
) -> u16 {
    use crate::tui::components::footer_hints::editor_footer_items;
    use crate::tui::screens::editor::view::editor_frame_areas;
    use crate::tui::screens::settings::view::{
        settings_frame_areas, settings_screen_footer_for_state,
    };

    let mut facts = ReservedFooterHeightFacts {
        editor_footer_height: None,
        settings_footer_height: None,
        workspace_footer_height: workspace_frame_areas(area).footer.height,
    };
    match console_reserved_footer_height_plan(state.stage.route()) {
        ConsoleReservedFooterHeightPlan::Editor => {
            if let crate::tui::state::ManagerStage::Editor(editor) = &state.stage {
                let body =
                    editor_frame_areas(area, effective_footer_height(editor.cached_footer_h)).body;
                facts.editor_footer_height = Some(measured_footer_height(
                    &editor_footer_items(editor, config, state.op_available, body),
                    area.width,
                ));
            }
        }
        ConsoleReservedFooterHeightPlan::Settings => {
            if let crate::tui::state::ManagerStage::Settings(settings) = &state.stage {
                let body =
                    settings_frame_areas(area, effective_footer_height(settings.cached_footer_h))
                        .body;
                facts.settings_footer_height = Some(measured_footer_height(
                    &settings_screen_footer_for_state(settings, state.op_available, body),
                    area.width,
                ));
            }
        }
        ConsoleReservedFooterHeightPlan::Workspace => {}
    }
    reserved_footer_height_for_facts(facts)
}

pub(crate) fn has_modal_overlay(state: &crate::tui::state::ManagerState<'_>) -> bool {
    modal_overlay_visible(modal_overlay_state_for_route(
        state.stage.route(),
        state.status_overlay.is_some(),
        state.list_modal.is_some(),
        state.stage.modal_facts(),
    ))
}
