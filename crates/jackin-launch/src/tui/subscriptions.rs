//! Launch cockpit input handling.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use jackin_tui::components::{ScrollAxes, ScrollAxis, StatusFooterHover};
use ratatui::layout::Rect;

use crate::tui::components::build_log_dialog::{
    build_log_scroll_filled_for_lines, build_log_scrollbar_top_offset_at,
    build_log_scrollbar_top_offset_for_row,
};
use crate::tui::components::container_info_dialog::{
    launch_container_info_rect, launch_container_info_state,
};
use crate::tui::components::failure_dialog::{failure_copy_payload, failure_copy_target_at};
use crate::tui::components::footer::{footer_instance, format_activity};
use crate::tui::terminal::current_terminal_area;
use crate::{LaunchHostTerminal, LaunchMessage, LaunchView, update_launch_view};

const BUILD_LOG_SCROLL_STEP: usize = 3;
const BUILD_LOG_PAGE_STEP: usize = 10;

pub type SharedView = Arc<Mutex<LaunchView>>;

/// Clamp the Debug-info dialog scroll to its content so over-scrolling cannot
/// accumulate (which would make the opposite key/wheel feel dead while it
/// unwinds). Called after every scroll key/wheel on the dialog.
fn clamp_container_info_scroll(
    view: &mut LaunchView,
    area: Rect,
    run_id: &str,
    terminal: &dyn LaunchHostTerminal,
    jackin_version: &'static str,
) {
    let state =
        launch_container_info_state(view, run_id, "", terminal.is_debug_mode(), jackin_version);
    let rect = launch_container_info_rect(area, &state);
    jackin_tui::components::clamp_container_info_scroll(
        &mut view.container_info_scroll,
        state.content_width(),
        state.content_height(),
        rect,
    );
}

fn update_build_log_scroll(view: &mut LaunchView, area: Rect, delta: isize) {
    let _dirty = update_launch_view(
        view,
        LaunchMessage::BuildLogScrolled {
            filled: build_log_scroll_filled_for_lines(area, &view.build_log_lines),
            delta,
        },
    );
}

fn build_log_scroll_axes(view: &LaunchView, area: Rect) -> ScrollAxes {
    ScrollAxes {
        vertical: build_log_scroll_filled_for_lines(area, &view.build_log_lines) > 0,
        horizontal: false,
    }
}

fn update_build_log_mouse_scroll(
    view: &mut LaunchView,
    area: Rect,
    kind: MouseEventKind,
    modifiers: KeyModifiers,
) -> bool {
    let Some((ScrollAxis::Vertical, top_delta)) = jackin_tui::components::mouse_scroll_delta(
        kind,
        modifiers,
        build_log_scroll_axes(view, area),
    ) else {
        return false;
    };
    update_build_log_scroll(
        view,
        area,
        -(isize::from(top_delta)) * isize::try_from(BUILD_LOG_SCROLL_STEP).unwrap_or(1),
    );
    true
}

fn update_build_log_scroll_from_top_offset(view: &mut LaunchView, area: Rect, top_offset: usize) {
    let _dirty = update_launch_view(
        view,
        LaunchMessage::BuildLogScrollSetFromTop {
            filled: build_log_scroll_filled_for_lines(area, &view.build_log_lines),
            top_offset,
        },
    );
}

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
    let footer_row = Rect {
        x: 0,
        y: area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let debug_chip = debug_mode.then_some(run_id);
    let hit_instance =
        jackin_tui::components::status_footer_right_chip_rect(footer_row, &instance, debug_chip)
            .is_some_and(|rect| {
                row >= rect.y
                    && row < rect.y.saturating_add(rect.height)
                    && col >= rect.x
                    && col < rect.x.saturating_add(rect.width)
            });
    // The debug chip (rightmost) opens the same container-info dialog as the
    // instance-ID chip — both show the same content, just from different entry
    // points. Check it too so clicking either chip works.
    let hit_debug = debug_mode
        && jackin_tui::components::status_footer_debug_chip_rect(footer_row, run_id).is_some_and(
            |rect| {
                row >= rect.y
                    && row < rect.y.saturating_add(rect.height)
                    && col >= rect.x
                    && col < rect.x.saturating_add(rect.width)
            },
        );
    hit_instance || hit_debug
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
        if jackin_tui::components::classify_click(rect, col, row)
            == jackin_tui::components::ModalClickResult::OutsideDismiss
        {
            // Click outside the dialog → dismiss (Defect 11).
            let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoClosed);
        } else if let Some((copy_row, payload)) =
            jackin_tui::components::container_info_copy_payload_at(rect, &state, col, row)
        {
            // Click inside on a copyable value → copy.
            if terminal.copy_to_clipboard(&payload) {
                let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoCopied(copy_row));
            }
            // If clipboard write failed: no-op (no close, no dirty).
        }
        // Click inside with no copy target → no-op (Defect 11: inside click swallowed).
    } else if let Some(failure) = v.failure.as_ref() {
        if let Some(target) = failure_copy_target_at(area, failure, run_id, col, row)
            && let Some(payload) = failure_copy_payload(failure, run_id, target)
        {
            if terminal.copy_to_clipboard(&payload) {
                let _dirty = update_launch_view(v, LaunchMessage::FailureCopied(target));
            } else {
                terminal.emit_compact_line(
                    "failure-popup-copy",
                    "OSC 52 clipboard write failed — badge suppressed",
                );
            }
        }
    } else if v.build_log_open {
        if let Some(top_offset) =
            build_log_scrollbar_top_offset_at(area, &v.build_log_lines, col, row)
        {
            let _dirty = update_launch_view(v, LaunchMessage::BuildLogScrollDragChanged(true));
            update_build_log_scroll_from_top_offset(v, area, top_offset);
        } else {
            let _dirty = update_launch_view(v, LaunchMessage::BuildLogClosed);
        }
    } else if hit_footer_container_chip(v, run_id, area, col, row, terminal.is_debug_mode()) {
        let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoOpened);
        terminal.set_pointer_shape(false);
    } else if !v.build_log_lines.is_empty() && hit_activity(v, col, row) {
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
    jackin_version: &'static str,
) {
    if v.container_info_open {
        // Hover over a copyable value brightens it (link hover feedback) and
        // switches the pointer to the clickable shape.
        let state =
            launch_container_info_state(v, run_id, "", terminal.is_debug_mode(), jackin_version);
        let rect = launch_container_info_rect(area, &state);
        let hover = jackin_tui::components::container_info_copy_payload_at(rect, &state, col, row)
            .map(|(idx, _)| idx);
        if hover != v.container_info_hover {
            let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoHovered(hover));
            terminal.set_pointer_shape(hover.is_some());
        }
        return;
    }
    if let Some(failure) = v.failure.as_ref() {
        let hover = failure_copy_target_at(area, failure, run_id, col, row);
        if hover != v.failure_copy_hover {
            let _dirty = update_launch_view(v, LaunchMessage::FailureCopyHovered(hover));
            terminal.set_pointer_shape(hover.is_some());
        }
        return;
    }
    let activity_hovering =
        !v.build_log_open && !v.build_log_lines.is_empty() && hit_activity(v, col, row);
    let container_hovering =
        hit_footer_container_chip(v, run_id, area, col, row, terminal.is_debug_mode());
    // Track debug chip hover separately so its color inverts on hover.
    let debug_chip_hovering = terminal.is_debug_mode()
        && !v.build_log_open
        && v.failure.is_none()
        && !footer_instance(v).is_empty()
        && jackin_tui::components::status_footer_debug_chip_rect(
            Rect {
                x: 0,
                y: area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            },
            run_id,
        )
        .is_some_and(|rect| {
            row >= rect.y
                && row < rect.y.saturating_add(rect.height)
                && col >= rect.x
                && col < rect.x.saturating_add(rect.width)
        });
    let hover = StatusFooterHover {
        left: activity_hovering,
        right: container_hovering,
        right_debug: debug_chip_hovering,
    };
    if hover != v.footer_hover {
        let _dirty = update_launch_view(v, LaunchMessage::FooterHoverChanged(hover));
        terminal.set_pointer_shape(activity_hovering || container_hovering || debug_chip_hovering);
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
    let area = current_terminal_area();
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let Ok(ev) = event::read() else {
            return;
        };
        let Ok(mut v) = view.lock() else {
            return;
        };
        match ev {
            Event::Mouse(m) => {
                // Durable telemetry: capture exactly what the terminal delivers
                // for a dialog mouse event so a `--debug` run reveals whether a
                // horizontal-scroll gesture even reaches the cockpit (and as what
                // kind/modifiers), instead of guessing at the mapping.
                if terminal.is_debug_mode() && (v.container_info_open || v.build_log_open) {
                    terminal.emit_compact_line(
                      "cockpit-dialog-mouse",
                      &format!(
                          "kind={:?} modifiers={:?} col={} row={} container_info_open={} build_log_open={}",
                          m.kind, m.modifiers, m.column, m.row, v.container_info_open, v.build_log_open
                      ),
                  );
                }
                match m.kind {
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
                    MouseEventKind::Drag(MouseButton::Left)
                        if v.build_log_open && v.build_log_scroll_dragging =>
                    {
                        if let Some(top_offset) =
                            build_log_scrollbar_top_offset_for_row(area, &v.build_log_lines, m.row)
                        {
                            update_build_log_scroll_from_top_offset(&mut v, area, top_offset);
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) if v.build_log_scroll_dragging => {
                        let _dirty = update_launch_view(
                            &mut v,
                            LaunchMessage::BuildLogScrollDragChanged(false),
                        );
                    }
                    MouseEventKind::Moved => {
                        handle_cockpit_mouse_move(
                            &mut v,
                            area,
                            run_id,
                            m.column,
                            m.row,
                            terminal,
                            jackin_version,
                        );
                    }
                    kind if v.build_log_open => {
                        let _consumed =
                            update_build_log_mouse_scroll(&mut v, area, kind, m.modifiers);
                    }
                    // The Debug-info dialog scrolls its own body on the wheel
                    // (both axes) via the shared handler; offsets clamp at render.
                    kind if v.container_info_open => {
                        let state = launch_container_info_state(
                            &v,
                            run_id,
                            "",
                            terminal.is_debug_mode(),
                            jackin_version,
                        );
                        let rect = launch_container_info_rect(area, &state);
                        let axes = jackin_tui::components::dialog_scroll_axes(
                            state.content_width(),
                            state.content_height(),
                            rect,
                        );
                        if v.container_info_scroll
                            .on_mouse_scroll_for_axes(kind, m.modifiers, axes)
                        {
                            clamp_container_info_scroll(
                                &mut v,
                                area,
                                run_id,
                                terminal,
                                jackin_version,
                            );
                        }
                    }
                    _ => {}
                }
            }
            // Keyboard scroll for the Debug-info dialog body (both axes).
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.container_info_open
                    && matches!(
                        k.code,
                        KeyCode::Up
                            | KeyCode::Down
                            | KeyCode::Left
                            | KeyCode::Right
                            | KeyCode::Char('h' | 'H' | 'j' | 'J' | 'k' | 'K' | 'l' | 'L')
                    ) =>
            {
                let state = launch_container_info_state(
                    &v,
                    run_id,
                    "",
                    terminal.is_debug_mode(),
                    jackin_version,
                );
                let rect = launch_container_info_rect(area, &state);
                let axes = jackin_tui::components::dialog_scroll_axes(
                    state.content_width(),
                    state.content_height(),
                    rect,
                );
                let _consumed = v.container_info_scroll.handle_key_for_axes(
                    k,
                    state.content_height(),
                    usize::from(rect.height.saturating_sub(2)),
                    state.content_width(),
                    usize::from(rect.width.saturating_sub(2)),
                    axes,
                );
                clamp_container_info_scroll(&mut v, area, run_id, terminal, jackin_version);
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.container_info_open
                    && matches!(k.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) =>
            {
                let _dirty = update_launch_view(&mut v, LaunchMessage::ContainerInfoClosed);
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
                KeyCode::Up if build_log_scroll_axes(&v, area).vertical => {
                    update_build_log_scroll(&mut v, area, 1);
                }
                KeyCode::Down if build_log_scroll_axes(&v, area).vertical => {
                    update_build_log_scroll(&mut v, area, -1);
                }
                KeyCode::PageUp if build_log_scroll_axes(&v, area).vertical => {
                    update_build_log_scroll(&mut v, area, BUILD_LOG_PAGE_STEP as isize);
                }
                KeyCode::PageDown if build_log_scroll_axes(&v, area).vertical => {
                    update_build_log_scroll(&mut v, area, -(BUILD_LOG_PAGE_STEP as isize));
                }
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseEventKind};

    #[test]
    fn build_log_mouse_wheel_scrolls_tail_when_vertical_bar_visible() {
        let mut view = crate::tui::update::initial_view();
        view.build_log_lines = (0..30).map(|idx| format!("line {idx}")).collect();
        let area = Rect::new(0, 0, 40, 8);

        assert!(update_build_log_mouse_scroll(
            &mut view,
            area,
            MouseEventKind::ScrollUp,
            KeyModifiers::NONE,
        ));

        assert_eq!(view.build_log_scroll.offset(), BUILD_LOG_SCROLL_STEP);
    }

    #[test]
    fn build_log_mouse_wheel_ignores_axes_without_visible_scrollbar() {
        let mut view = crate::tui::update::initial_view();
        view.build_log_lines = vec!["short".to_owned()];
        let area = Rect::new(0, 0, 40, 8);

        assert!(!update_build_log_mouse_scroll(
            &mut view,
            area,
            MouseEventKind::ScrollUp,
            KeyModifiers::NONE,
        ));
        assert!(!update_build_log_mouse_scroll(
            &mut view,
            area,
            MouseEventKind::ScrollRight,
            KeyModifiers::NONE,
        ));

        assert_eq!(view.build_log_scroll.offset(), 0);
    }
}
