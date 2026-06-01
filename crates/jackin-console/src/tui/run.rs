//! Shared helpers for the host console event-loop shell.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Split `area` into a main region and an optional 1-row debug bar at the
/// bottom.
#[must_use]
pub fn split_debug_area(area: Rect, debug_mode: bool) -> (Rect, Option<Rect>) {
    if !debug_mode || area.height < 2 {
        return (area, None);
    }
    let main = Rect {
        height: area.height - 1,
        ..area
    };
    let bar = Rect {
        y: area.y + area.height - 1,
        height: 1,
        ..area
    };
    (main, Some(bar))
}

/// Render the 1-row debug status bar.
///
/// When `instance_id` is provided, shows `run_id:instance_id` as a single
/// danger chip right-aligned on a white bar. The combined chip is clickable
/// in the root event loop.
pub fn render_debug_bar(frame: &mut Frame, area: Rect, run_id: &str, instance_id: Option<&str>) {
    let chip_text =
        instance_id.map_or_else(|| format!(" {run_id} "), |iid| format!(" {run_id}:{iid} "));
    let chip_width = chip_text.chars().count() as u16;

    let [left_area, chip_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(chip_width)]).areas(area);

    let white_bg = Style::default()
        .bg(jackin_tui::theme::WHITE)
        .fg(jackin_tui::theme::PHOSPHOR_DARK);
    let chip_style = Style::default()
        .bg(jackin_tui::theme::DANGER_RED)
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::raw("")])).style(white_bg),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(chip_text, chip_style)])),
        chip_area,
    );
}

#[must_use]
pub const fn should_debug_log_mouse(mouse: crossterm::event::MouseEvent) -> bool {
    !matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::ScrollDown
            | crossterm::event::MouseEventKind::ScrollUp
            | crossterm::event::MouseEventKind::ScrollLeft
            | crossterm::event::MouseEventKind::ScrollRight
    )
}

#[must_use]
pub fn quit_confirm_area(frame: Rect, confirm: &jackin_tui::components::ConfirmState) -> Rect {
    let width: u16 = 44.min(frame.width.saturating_sub(4));
    let height: u16 = jackin_tui::components::confirm_required_height(confirm)
        .min(frame.height.saturating_sub(2));
    let x = frame.x + frame.width.saturating_sub(width) / 2;
    let y = frame.y + frame.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}
