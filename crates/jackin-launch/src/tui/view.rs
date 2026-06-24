//! Launch cockpit top-level frame composition.

use jackin_tui::components::{BOTTOM_CHROME_ROWS, bottom_chrome_areas, render_hint_bar};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::Clear;

use crate::LaunchView;
use crate::tui::components::build_log_dialog::render_build_log_dialog;
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

#[allow(clippy::too_many_arguments)]
pub fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    no_motion: bool,
    rain: Option<&RainState>,
    debug_mode: bool,
    jackin_version: &'static str,
) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    // Quit confirmation supersedes every other surface (matching the console),
    // owning the screen behind its own backdrop until the operator answers.
    // `draw_confirm` lays out the dimmed backdrop, the centered dialog, and the
    // hint row; render the status footer underneath so the bottom chrome stays
    // intact — hint row, blank spacer, then the status bar at the very bottom.
    if let Some(confirm) = &view.quit_confirm {
        draw_confirm(frame, confirm);
        render_footer(
            frame,
            bottom_chrome_areas(area).footer,
            view,
            run_id,
            debug_mode,
        );
        return;
    }

    // The build-log overlay owns the whole screen behind an opaque backdrop,
    // matching the capsule modal convention (hide everything, don't dim).
    if view.build_log_open {
        render_build_log_dialog(frame, area, view, run_id, debug_mode);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // brand header (pill + spacer) — shared chrome
            Constraint::Min(8),    // launch body
            Constraint::Length(BOTTOM_CHROME_ROWS), // hint bar + spacer + status footer
        ])
        .split(area);

    // Freeze animated accents while a failure popup owns the screen so no
    // live cue keeps moving behind the modal.
    let frozen = no_motion || view.failure.is_some();
    let chrome = bottom_chrome_areas(rows[2]);

    render_cockpit_header(frame, rows[0], view, frozen);
    render_body(frame, rows[1], view, frozen, rain);
    render_hint_bar(
        frame,
        chrome.hint,
        &crate::tui::keymap::cockpit_global_hint_spans(),
    );
    render_footer(frame, chrome.footer, view, run_id, debug_mode);

    if let Some(failure) = &view.failure {
        render_failure_popup(frame, area, view, failure, run_id);
    } else if view.container_info_open {
        render_launch_container_info(
            frame,
            area,
            view,
            run_id,
            run_log_path,
            debug_mode,
            jackin_version,
        );
    }
}

fn render_body(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    frozen: bool,
    rain: Option<&RainState>,
) {
    // No border — the rain fills the whole body; a one-cell side margin keeps
    // glyphs off the screen edge.
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    });
    // Digital rain fills the space; the block progress + stage words sit above
    // a blank gap so the bar does not stick to the status bar.
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // rain
            Constraint::Length(2), // progress blocks + stage words
            Constraint::Length(2), // gap above the status bar
        ])
        .split(inner);
    render_rain(frame, parts[0], rain);
    render_progress(frame, parts[1], view, frozen);
}

pub fn launch_hyperlink_overlays(
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) -> Vec<u8> {
    let mut overlays = failure_popup_hyperlink_overlay_bytes(area, view, run_id);
    overlays.extend(launch_container_info_hyperlink_overlay_bytes(
        area,
        view,
        run_id,
        run_log_path,
        debug_mode,
        jackin_version,
    ));
    overlays
}

fn launch_container_info_hyperlink_overlay_bytes(
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    debug_mode: bool,
    jackin_version: &'static str,
) -> Vec<u8> {
    if !view.container_info_open || view.failure.is_some() || view.build_log_open {
        return Vec::new();
    }
    let state = launch_container_info_state(view, run_id, run_log_path, debug_mode, jackin_version);
    let rect = launch_container_info_rect(area, &state);
    jackin_tui::components::container_info_hyperlink_overlay(rect, &state)
}

fn failure_popup_hyperlink_overlay_bytes(area: Rect, view: &LaunchView, run_id: &str) -> Vec<u8> {
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
        view.failure_copy_hover,
        view.failure_copied,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::layout::Rect;

    use super::launch_hyperlink_overlays;
    use crate::tui::update::initial_view;
    use crate::{LaunchStage, tui::app::LaunchFailure};

    #[test]
    fn build_log_open_suppresses_failure_hyperlink_overlay() {
        let mut view = initial_view();
        view.build_log_open = true;
        view.failure = Some(LaunchFailure {
            title: "Docker build failed".to_owned(),
            summary: "build failed".to_owned(),
            detail: None,
            next_step: None,
            stage: LaunchStage::DerivedImage,
            diagnostics_path: Some(PathBuf::from(
                "/Users/donbeave/Projects/jackin-project/test/pr-641/state/home/data/diagnostics/runs/18bc0fd1093b23b0.jsonl",
            )),
            command_output_path: None,
        });

        let overlay = launch_hyperlink_overlays(
            Rect::new(0, 0, 120, 40),
            &view,
            "18bc0fd1093b23b0",
            "/Users/donbeave/Projects/jackin-project/test/pr-641/state/home/data/diagnostics/runs/18bc0fd1093b23b0.jsonl",
            true,
            "0.6.0-test",
        );

        assert!(
            overlay.is_empty(),
            "build-log overlay owns the screen; failure hyperlinks must not render over it: {:?}",
            String::from_utf8_lossy(&overlay)
        );
    }
}
