// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch container-info dialog helpers.

use crate::tui::components::container_info::{
    ContainerInfoRow, ContainerInfoState, DebugInfo, render_container_info,
    required_height as container_info_required_height,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Clear;
use termrock::widgets::HintSpan;

use crate::LaunchView;
use crate::tui::components::dialog::{
    dialog_scroll, dialog_scroll_axes, percent_dialog_rect, render_dialog_backdrop,
};
use crate::tui::components::footer::{launch_overlay_chrome_areas, render_footer};

fn debug_info_hint_spans(axes: termrock::scroll::ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut spans = Vec::new();
    if axes.vertical {
        spans.extend([HintSpan::Key("↑↓/j/k"), HintSpan::Text("scroll")]);
    }
    if axes.horizontal {
        if !spans.is_empty() {
            spans.push(HintSpan::GroupSep);
        }
        spans.extend([HintSpan::Key("←→/h/l"), HintSpan::Text("scroll")]);
    }
    if axes.any() {
        spans.push(HintSpan::GroupSep);
    }
    spans.extend([
        HintSpan::Key("↵"),
        HintSpan::Text("copy value"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("dismiss"),
        HintSpan::GroupSep,
        HintSpan::Key("click"),
        HintSpan::Text("copy value"),
    ]);
    spans
}

#[must_use]
pub fn launch_container_info_state(
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) -> ContainerInfoState {
    let identity = view.identity.as_ref();
    // The launch surface knows the container/role/agent/target on top of what
    // the console already showed. Build from the shared accumulating model so
    // row order, labels, and copy affordances match every other surface.
    let info = DebugInfo {
        jackin_version: Some(jackin_version.to_owned()),
        container_id: Some(
            identity
                .and_then(|identity| identity.container.as_deref())
                .unwrap_or("loading...")
                .to_owned(),
        ),
        role: identity.map(|identity| identity.role.clone()),
        agent: identity.map(|identity| identity.agent.clone()),
        target: identity.map(|identity| identity.target_label.clone()),
        run_id: debug_mode.then(|| run_id.to_owned()),
        capsule_version: None,
    };
    let mut state = info.into_state();
    if debug_mode {
        let endpoint = jackin_diagnostics::configured_endpoint_summary()
            .unwrap_or_else(|| "OpenTelemetry backend".to_owned());
        state.push_row(ContainerInfoRow::new(
            "Telemetry",
            format!("run {run_id} -> {endpoint}"),
        ));
    }
    if let Some(row) = view.container_info_copied {
        state.mark_copied(row);
    }
    state.set_hovered_row(view.container_info_hover);
    state.scroll = dialog_scroll(&view.container_info_scroll);
    state
}

pub fn render_launch_container_info(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) {
    let chrome = launch_overlay_chrome_areas(area, debug_mode);
    let state = launch_container_info_state(view, run_id, debug_mode, jackin_version);
    let rect = launch_container_info_rect(area, &state, debug_mode);
    render_dialog_backdrop(frame, chrome.body);
    render_container_info(frame, rect, &state);
    let axes = dialog_scroll_axes(state.content_width(), state.content_height(), rect);
    let mut hint_spans = debug_info_hint_spans(axes);
    hint_spans.push(HintSpan::GroupSep);
    hint_spans.extend(crate::tui::keymap::cockpit_global_hint_spans());
    if !debug_mode {
        frame.render_widget(Clear, chrome.hint);
    }
    termrock::widgets::render_hint_bar(
        frame,
        chrome.hint,
        &hint_spans,
        &termrock::Theme::default(),
    );
    if debug_mode {
        frame.render_widget(Clear, chrome.spacer);
        render_footer(frame, chrome.footer, view, run_id, true);
    }
}

#[must_use]
pub fn launch_container_info_rect(
    area: Rect,
    state: &ContainerInfoState,
    debug_mode: bool,
) -> Rect {
    let body = launch_overlay_chrome_areas(area, debug_mode).body;
    let height = container_info_required_height(state);
    percent_dialog_rect(body, 60, 40, 2, 2, height)
}

#[cfg(test)]
mod tests;
