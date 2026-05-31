//! Launch cockpit top-level frame composition.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::Clear;

use crate::LaunchView;
use crate::tui::build_log::render_build_log_dialog;
use crate::tui::container_info::render_launch_container_info;
use crate::tui::failure::render_failure_popup;
use crate::tui::footer::render_footer;
use crate::tui::header::render_cockpit_header;
use crate::tui::progress::render_progress;
use crate::tui::rain::{RainState, render_rain};

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

    // The build-log overlay owns the whole screen behind an opaque backdrop,
    // matching the capsule modal convention (hide everything, don't dim).
    if view.build_log_open {
        render_build_log_dialog(frame, area, view);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // brand header (pill + spacer) — shared chrome
            Constraint::Min(8),    // launch body
            Constraint::Length(1), // status / diagnostics
        ])
        .split(area);

    // Freeze animated accents while a failure popup owns the screen so no
    // live cue keeps moving behind the modal.
    let frozen = no_motion || view.failure.is_some();

    render_cockpit_header(frame, rows[0], view, frozen);
    render_body(frame, rows[1], view, frozen, rain);
    render_footer(frame, rows[2], view, run_id, debug_mode);

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
