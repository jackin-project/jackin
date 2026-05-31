//! Launch cockpit input handling.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

use crate::tui::build_log::scroll_build_log;
use crate::tui::container_info::{launch_container_info_rect, launch_container_info_state};
use crate::tui::failure::{failure_copy_payload, failure_copy_target_at};
use crate::tui::footer::{footer_instance, format_activity};
use crate::{LaunchHostTerminal, LaunchMessage, LaunchView, update_launch_view};

const BUILD_LOG_SCROLL_STEP: usize = 3;
const BUILD_LOG_PAGE_STEP: usize = 10;

pub type SharedView = Arc<Mutex<LaunchView>>;

/// Whether `(col, row)` falls on the footer activity text ("Building Docker
/// image…"). The footer is the last terminal row; the activity is left-aligned
/// and the right-side chips never overlap it, so a left-edge span is enough.
fn hit_activity(view: &LaunchView, col: u16, row: u16) -> bool {
    let Ok((_, rows)) = crossterm::terminal::size() else {
        return false;
    };
    if rows == 0 || row != rows - 1 {
        return false;
    }
    let width = u16::try_from(format_activity(&view.status).chars().count()).unwrap_or(u16::MAX);
    // One column of slack for the band's left padding.
    col <= width
}

fn hit_footer_container_chip(
    view: &LaunchView,
    run_id: &str,
    area: Rect,
    col: u16,
    row: u16,
    debug_mode: bool,
) -> bool {
    if view.build_log_open || view.failure.is_some() {
        return false;
    }
    let instance = footer_instance(view);
    if instance.is_empty() {
        return false;
    }
    let debug_chip = debug_mode.then_some(run_id);
    jackin_tui::components::status_footer_right_chip_rect(
        Rect {
            x: 0,
            y: area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        },
        &instance,
        debug_chip,
    )
    .is_some_and(|rect| {
        row >= rect.y
            && row < rect.y.saturating_add(rect.height)
            && col >= rect.x
            && col < rect.x.saturating_add(rect.width)
    })
}

fn handle_cockpit_mouse_down(
    v: &mut LaunchView,
    area: Rect,
    run_id: &str,
    col: u16,
    row: u16,
    terminal: &dyn LaunchHostTerminal,
    jackin_version: &'static str,
) {
    if v.container_info_open {
        let state =
            launch_container_info_state(v, run_id, "", terminal.is_debug_mode(), jackin_version);
        let rect = launch_container_info_rect(area, &state);
        if let Some((row, payload)) =
            jackin_tui::components::container_info_copy_payload_at(rect, &state, col, row)
        {
            if terminal.copy_to_clipboard(&payload) {
                v.container_info_copied = Some(row);
            }
        } else {
            v.container_info_open = false;
            v.container_info_copied = None;
        }
    } else if let Some(failure) = v.failure.as_ref() {
        if let Some(target) = failure_copy_target_at(area, failure, run_id, col, row)
            && let Some(payload) = failure_copy_payload(failure, run_id, target)
        {
            if terminal.copy_to_clipboard(&payload) {
                v.failure_copied = Some(target);
            } else {
                terminal.emit_compact_line(
                    "failure-popup-copy",
                    "OSC 52 clipboard write failed — badge suppressed",
                );
            }
        }
    } else if v.build_log_open {
        let _dirty = update_launch_view(v, LaunchMessage::BuildLogClosed);
    } else if hit_footer_container_chip(v, run_id, area, col, row, terminal.is_debug_mode()) {
        v.container_info_open = true;
        v.container_info_copied = None;
        v.footer_hover.right = false;
        terminal.set_pointer_shape(false);
    } else if crate::build_log::len() > 0 && hit_activity(v, col, row) {
        let _dirty = update_launch_view(v, LaunchMessage::BuildLogOpened);
        terminal.set_pointer_shape(false);
    }
}

fn handle_cockpit_mouse_move(
    v: &mut LaunchView,
    area: Rect,
    run_id: &str,
    col: u16,
    row: u16,
    terminal: &dyn LaunchHostTerminal,
) {
    if v.container_info_open {
        return;
    }
    if let Some(failure) = v.failure.as_ref() {
        let hover = failure_copy_target_at(area, failure, run_id, col, row);
        if hover != v.failure_copy_hover {
            v.failure_copy_hover = hover;
            terminal.set_pointer_shape(hover.is_some());
        }
        return;
    }
    let activity_hovering =
        !v.build_log_open && crate::build_log::len() > 0 && hit_activity(v, col, row);
    let container_hovering =
        hit_footer_container_chip(v, run_id, area, col, row, terminal.is_debug_mode());
    if activity_hovering != v.footer_hover.left || container_hovering != v.footer_hover.right {
        v.footer_hover.left = activity_hovering;
        v.footer_hover.right = container_hovering;
        terminal.set_pointer_shape(activity_hovering || container_hovering);
    }
}

/// Drain queued terminal input and fold it into the build-log overlay / failure
/// state.
///
/// Called only while the render task owns the renderer (no forced-choice picker
/// is reading events), so this poll cannot steal a picker's keystrokes. Polling
/// with a zero timeout keeps the 33 ms render cadence intact.
pub fn handle_cockpit_input(
    view: &SharedView,
    run_id: &str,
    terminal: &dyn LaunchHostTerminal,
    jackin_version: &'static str,
) {
    let area = crossterm::terminal::size()
        .ok()
        .map(|(width, height)| Rect::new(0, 0, width, height))
        .unwrap_or_default();
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let Ok(ev) = event::read() else {
            return;
        };
        let Ok(mut v) = view.lock() else {
            return;
        };
        match ev {
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    handle_cockpit_mouse_down(
                        &mut v,
                        area,
                        run_id,
                        m.column,
                        m.row,
                        terminal,
                        jackin_version,
                    );
                }
                MouseEventKind::Moved => {
                    handle_cockpit_mouse_move(&mut v, area, run_id, m.column, m.row, terminal);
                }
                MouseEventKind::ScrollUp if v.build_log_open => {
                    scroll_build_log(&mut v, area, BUILD_LOG_SCROLL_STEP as isize);
                }
                MouseEventKind::ScrollDown if v.build_log_open => {
                    scroll_build_log(&mut v, area, -(BUILD_LOG_SCROLL_STEP as isize));
                }
                _ => {}
            },
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.container_info_open
                    && matches!(k.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) =>
            {
                v.container_info_open = false;
                v.footer_hover.right = false;
                terminal.set_pointer_shape(false);
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.failure.is_some()
                    && matches!(k.code, KeyCode::Enter | KeyCode::Esc) =>
            {
                // Failure popup is modal over the cockpit; Enter/Esc acknowledges
                // it so the awaiting `stage_failed` returns.
                let _dirty = update_launch_view(&mut v, LaunchMessage::FailureAcknowledged);
                terminal.set_pointer_shape(false);
            }
            Event::Key(k) if k.kind == KeyEventKind::Press && v.build_log_open => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    let _dirty = update_launch_view(&mut v, LaunchMessage::BuildLogClosed);
                }
                KeyCode::Up => scroll_build_log(&mut v, area, 1),
                KeyCode::Down => scroll_build_log(&mut v, area, -1),
                KeyCode::PageUp => {
                    scroll_build_log(&mut v, area, BUILD_LOG_PAGE_STEP as isize);
                }
                KeyCode::PageDown => {
                    scroll_build_log(&mut v, area, -(BUILD_LOG_PAGE_STEP as isize));
                }
                _ => {}
            },
            _ => {}
        }
    }
}
