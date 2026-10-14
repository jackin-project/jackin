// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::Clear;

use crate::LaunchView;
use crate::tui::components::build_log_dialog::render_build_log_dialog;
use crate::tui::components::chrome::{BOTTOM_CHROME_ROWS, bottom_chrome_areas};
use crate::tui::components::container_info_dialog::{
    launch_container_info_rect, launch_container_info_state, render_launch_container_info,
};
use crate::tui::components::failure_dialog::{
    failure_popup_hyperlink_overlay, render_failure_popup,
};
use crate::tui::components::footer::render_footer;
use crate::tui::components::header::render_cockpit_header;
use crate::tui::components::progress_rail::render_progress;
use crate::tui::components::prompts::draw_confirm;
use crate::tui::components::rain::{RainState, render_rain};

pub fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    no_motion: bool,
    rain: Option<&RainState>,
    debug_mode: bool,
    jackin_version: &'static str,
) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    if let Some(confirm) = &view.quit_confirm {
        draw_confirm(frame, &mut confirm.clone());
        if debug_mode {
            render_footer(frame, bottom_chrome_areas(area).footer, view, run_id, true);
        }
        return;
    }

    if view.build_log_open && view.failure.is_none() {
        render_build_log_dialog(frame, area, view, run_id, debug_mode);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(BOTTOM_CHROME_ROWS),
        ])
        .split(area);

    let frozen = no_motion || view.failure.is_some();
    let chrome = bottom_chrome_areas(rows[2]);

    render_cockpit_header(frame, rows[0], view, frozen);
    render_body(frame, rows[1], view, frozen, rain);
    termrock::widgets::render_hint_bar(
        frame,
        chrome.hint,
        &crate::tui::keymap::cockpit_global_hint_spans(),
        &termrock::style::DesignSystem::default(),
    );
    render_footer(frame, chrome.footer, view, run_id, debug_mode);

    if let Some(failure) = &view.failure {
        render_failure_popup(frame, area, view, failure, run_id, debug_mode);
    } else if view.container_info_open {
        render_launch_container_info(frame, area, view, run_id, debug_mode, jackin_version);
    }
}

fn render_body(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    frozen: bool,
    rain: Option<&RainState>,
) {
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    });
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);
    render_rain(frame, parts[0], rain);
    render_progress(frame, parts[1], view, frozen);
}

pub fn launch_hyperlink_overlays(
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) -> Vec<u8> {
    let mut overlays = failure_popup_hyperlink_overlay_bytes(area, view, run_id, debug_mode);
    overlays.extend(launch_container_info_hyperlink_overlay_bytes(
        area,
        view,
        run_id,
        debug_mode,
        jackin_version,
    ));
    overlays
}

fn launch_container_info_hyperlink_overlay_bytes(
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) -> Vec<u8> {
    if !view.container_info_open || view.failure.is_some() || view.build_log_open {
        return Vec::new();
    }
    let state = launch_container_info_state(view, run_id, debug_mode, jackin_version);
    let rect = launch_container_info_rect(area, &state, debug_mode);
    crate::tui::components::container_info::hyperlink_overlay(rect, &state)
}

fn failure_popup_hyperlink_overlay_bytes(
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
) -> Vec<u8> {
    if view.build_log_open {
        return Vec::new();
    }
    let Some(failure) = view.failure.as_ref() else {
        return Vec::new();
    };
    failure_popup_hyperlink_overlay(
        area,
        failure,
        run_id,
        debug_mode,
        Some(view.failure_scroll.clone()),
        view.failure_copy_hover,
        view.failure_copied,
        view.failure_revealed,
        view.failure_opened,
    )
}
