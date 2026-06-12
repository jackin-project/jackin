//! Launch cockpit input handling.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use jackin_tui::components::{ScrollAxes, StatusFooterHover};
use ratatui::layout::Rect;

use crate::tui::components::build_log_dialog::{
    build_log_scrollbar_top_offset_for_row_cached, refresh_build_log_layout,
};
use crate::tui::components::container_info_dialog::{
    launch_container_info_rect, launch_container_info_state,
};
use crate::tui::components::failure_dialog::{
    failure_copy_payload, failure_copy_target_at, failure_reveal_payload,
};
use crate::tui::components::footer::{footer_instance, format_activity};
use crate::tui::terminal::current_terminal_area;
use crate::{LaunchHostTerminal, LaunchMessage, LaunchView, update_launch_view};

const BUILD_LOG_SCROLL_STEP: usize = 3;
const BUILD_LOG_PAGE_STEP: usize = 10;

pub type SharedView = Arc<Mutex<LaunchView>>;

#[derive(Clone, Copy)]
struct CockpitContext<'a> {
    area: Rect,
    run_id: &'a str,
    run_log_path: &'a str,
    terminal: &'a dyn LaunchHostTerminal,
    jackin_version: &'static str,
}

/// Clamp the Debug-info dialog scroll to its content so over-scrolling cannot
/// accumulate (which would make the opposite key/wheel feel dead while it
/// unwinds). Called after every scroll key/wheel on the dialog.
fn clamp_container_info_scroll(view: &mut LaunchView, ctx: CockpitContext<'_>) {
    let state = launch_container_info_state(
        view,
        ctx.run_id,
        ctx.run_log_path,
        ctx.terminal.is_debug_mode(),
        ctx.jackin_version,
    );
    let rect = launch_container_info_rect(ctx.area, &state);
    jackin_tui::components::clamp_container_info_scroll(
        &mut view.container_info_scroll,
        state.content_width(),
        state.content_height(),
        rect,
    );
}

fn file_url_path(href: &str) -> Option<&str> {
    href.strip_prefix("file://").filter(|path| !path.is_empty())
}

fn reveal_container_info_diagnostics(view: &mut LaunchView, ctx: CockpitContext<'_>) {
    if !ctx.terminal.is_debug_mode() || ctx.run_log_path.is_empty() {
        return;
    }
    if ctx
        .terminal
        .reveal_file(std::path::Path::new(ctx.run_log_path))
    {
        ctx.terminal
            .emit_compact_line("container-info-reveal", "diagnostics log reveal requested");
    } else {
        ctx.terminal.emit_compact_line(
            "container-info-reveal",
            "host file reveal failed — badge suppressed",
        );
    }
    clamp_container_info_scroll(view, ctx);
}

fn update_build_log_scroll(view: &mut LaunchView, area: Rect, delta: isize) {
    refresh_build_log_layout(view, area, false);
    let _dirty = update_launch_view(
        view,
        LaunchMessage::BuildLogScrolled {
            filled: view.build_log_filled,
            delta,
        },
    );
}

fn build_log_scroll_axes(view: &LaunchView, area: Rect) -> ScrollAxes {
    let box_area = crate::tui::components::build_log_dialog::build_log_box_area(area);
    let viewport_w = jackin_tui::components::viewport_width(box_area);
    let viewport_h = jackin_tui::components::viewport_height(box_area);
    ScrollAxes {
        vertical: view.build_log_filled > 0
            && view.build_log_wrapped_width == viewport_w
            && view.build_log_viewport_height == viewport_h,
        horizontal: false,
    }
}

fn update_build_log_mouse_scroll(
    view: &mut LaunchView,
    area: Rect,
    kind: MouseEventKind,
    modifiers: KeyModifiers,
) -> bool {
    refresh_build_log_layout(view, area, false);
    let Some(delta) = jackin_tui::components::mouse_scroll_delta(
        kind,
        modifiers,
        build_log_scroll_axes(view, area),
    ) else {
        return false;
    };
    update_build_log_scroll(
        view,
        area,
        -(isize::from(delta.amount)) * isize::try_from(BUILD_LOG_SCROLL_STEP).unwrap_or(1),
    );
    true
}

fn update_build_log_scroll_from_top_offset(view: &mut LaunchView, area: Rect, top_offset: usize) {
    refresh_build_log_layout(view, area, false);
    let _dirty = update_launch_view(
        view,
        LaunchMessage::BuildLogScrollSetFromTop {
            filled: view.build_log_filled,
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

fn handle_cockpit_mouse_down(v: &mut LaunchView, ctx: CockpitContext<'_>, col: u16, row: u16) {
    if v.container_info_open {
        let state = launch_container_info_state(
            v,
            ctx.run_id,
            ctx.run_log_path,
            ctx.terminal.is_debug_mode(),
            ctx.jackin_version,
        );
        let rect = launch_container_info_rect(ctx.area, &state);
        if jackin_tui::components::classify_click(rect, col, row)
            == jackin_tui::components::ModalClickResult::OutsideDismiss
        {
            // Click outside the dialog → dismiss (Defect 11).
            let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoClosed);
        } else if let Some((copy_row, payload)) =
            jackin_tui::components::container_info_copy_payload_at(rect, &state, col, row)
        {
            // Click inside on a copyable value → copy.
            if ctx.terminal.copy_to_clipboard(&payload) {
                let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoCopied(copy_row));
            }
            // If clipboard write failed: no-op (no close, no dirty).
        } else if let Some((_row, href)) =
            jackin_tui::components::container_info_hyperlink_payload_at(rect, &state, col, row)
            && let Some(path) = file_url_path(&href)
        {
            // Click inside on a reveal-only file URL → ask host to reveal.
            if ctx.terminal.reveal_file(std::path::Path::new(path)) {
                ctx.terminal
                    .emit_compact_line("container-info-reveal", "diagnostics log reveal requested");
            } else {
                ctx.terminal.emit_compact_line(
                    "container-info-reveal",
                    "host file reveal failed — badge suppressed",
                );
            }
        }
        // Click inside with no copy target → no-op (Defect 11: inside click swallowed).
    } else if let Some(failure) = v.failure.as_ref() {
        if let Some(target) = failure_copy_target_at(ctx.area, failure, ctx.run_id, col, row)
            && let Some(payload) = failure_copy_payload(failure, ctx.run_id, target)
        {
            if ctx.terminal.copy_to_clipboard(&payload) {
                let _dirty = update_launch_view(v, LaunchMessage::FailureCopied(target));
            } else {
                ctx.terminal.emit_compact_line(
                    "failure-popup-copy",
                    "OSC 52 clipboard write failed — badge suppressed",
                );
            }
        }
    } else if v.build_log_open {
        refresh_build_log_layout(v, ctx.area, false);
        if let Some(top_offset) =
            build_log_scrollbar_top_offset_for_row_cached(v, ctx.area, col, row)
        {
            let _dirty = update_launch_view(v, LaunchMessage::BuildLogScrollDragChanged(true));
            update_build_log_scroll_from_top_offset(v, ctx.area, top_offset);
        }
        // Plain body clicks are swallowed while the build log owns the overlay.
        // Close stays keyboard-only (`Esc`/`q`); scrollbar hits remain interactive.
    } else if hit_footer_container_chip(
        v,
        ctx.run_id,
        ctx.area,
        col,
        row,
        ctx.terminal.is_debug_mode(),
    ) {
        let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoOpened);
        ctx.terminal.set_pointer_shape(false);
    } else if !v.build_log_lines.is_empty() && hit_activity(v, col, row) {
        let _dirty = update_launch_view(v, LaunchMessage::BuildLogOpened);
        ctx.terminal.set_pointer_shape(false);
    }
}

fn handle_cockpit_mouse_move(v: &mut LaunchView, ctx: CockpitContext<'_>, col: u16, row: u16) {
    if v.container_info_open {
        // Hover over a copyable value brightens it (link hover feedback) and
        // switches the pointer to the clickable shape.
        let state = launch_container_info_state(
            v,
            ctx.run_id,
            ctx.run_log_path,
            ctx.terminal.is_debug_mode(),
            ctx.jackin_version,
        );
        let rect = launch_container_info_rect(ctx.area, &state);
        let hover = jackin_tui::components::container_info_copy_payload_at(rect, &state, col, row)
            .map(|(idx, _)| idx);
        let reveal_hover =
            jackin_tui::components::container_info_hyperlink_payload_at(rect, &state, col, row)
                .is_some_and(|(_idx, href)| file_url_path(&href).is_some());
        if hover != v.container_info_hover {
            let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoHovered(hover));
        }
        ctx.terminal
            .set_pointer_shape(hover.is_some() || reveal_hover);
        return;
    }
    if let Some(failure) = v.failure.as_ref() {
        let hover = failure_copy_target_at(ctx.area, failure, ctx.run_id, col, row);
        if hover != v.failure_copy_hover {
            let _dirty = update_launch_view(v, LaunchMessage::FailureCopyHovered(hover));
            ctx.terminal.set_pointer_shape(hover.is_some());
        }
        return;
    }
    let activity_hovering =
        !v.build_log_open && !v.build_log_lines.is_empty() && hit_activity(v, col, row);
    let container_hovering = hit_footer_container_chip(
        v,
        ctx.run_id,
        ctx.area,
        col,
        row,
        ctx.terminal.is_debug_mode(),
    );
    // Track debug chip hover separately so its color inverts on hover.
    let debug_chip_hovering = ctx.terminal.is_debug_mode()
        && !v.build_log_open
        && v.failure.is_none()
        && !footer_instance(v).is_empty()
        && jackin_tui::components::status_footer_debug_chip_rect(
            Rect {
                x: 0,
                y: ctx.area.height.saturating_sub(1),
                width: ctx.area.width,
                height: 1,
            },
            ctx.run_id,
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
        ctx.terminal
            .set_pointer_shape(activity_hovering || container_hovering || debug_chip_hovering);
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
    run_log_path: &str,
    terminal: &dyn LaunchHostTerminal,
    jackin_version: &'static str,
) {
    let area = current_terminal_area();
    let ctx = CockpitContext {
        area,
        run_id,
        run_log_path,
        terminal,
        jackin_version,
    };
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
                        handle_cockpit_mouse_down(&mut v, ctx, m.column, m.row);
                    }
                    MouseEventKind::Drag(MouseButton::Left)
                        if v.build_log_open && v.build_log_scroll_dragging =>
                    {
                        refresh_build_log_layout(&mut v, area, false);
                        if let Some(top_offset) =
                            build_log_scrollbar_top_offset_for_row_cached(&v, area, m.column, m.row)
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
                        handle_cockpit_mouse_move(&mut v, ctx, m.column, m.row);
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
                            ctx.run_id,
                            ctx.run_log_path,
                            ctx.terminal.is_debug_mode(),
                            ctx.jackin_version,
                        );
                        let rect = launch_container_info_rect(ctx.area, &state);
                        let axes = jackin_tui::components::dialog_scroll_axes(
                            state.content_width(),
                            state.content_height(),
                            rect,
                        );
                        if v.container_info_scroll
                            .on_mouse_scroll_for_axes(kind, m.modifiers, axes)
                        {
                            clamp_container_info_scroll(&mut v, ctx);
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
                    ctx.run_id,
                    ctx.run_log_path,
                    ctx.terminal.is_debug_mode(),
                    ctx.jackin_version,
                );
                let rect = launch_container_info_rect(ctx.area, &state);
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
                clamp_container_info_scroll(&mut v, ctx);
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.container_info_open
                    && matches!(k.code, KeyCode::Char('r' | 'R' | 'o' | 'O')) =>
            {
                reveal_container_info_diagnostics(&mut v, ctx);
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.container_info_open
                    && matches!(k.code, KeyCode::Enter) =>
            {
                let state = launch_container_info_state(
                    &v,
                    ctx.run_id,
                    ctx.run_log_path,
                    ctx.terminal.is_debug_mode(),
                    ctx.jackin_version,
                );
                if let Some((copy_row, payload)) = state.keyboard_copy_payload()
                    && ctx.terminal.copy_to_clipboard(&payload)
                {
                    let _dirty =
                        update_launch_view(&mut v, LaunchMessage::ContainerInfoCopied(copy_row));
                }
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.container_info_open
                    && matches!(k.code, KeyCode::Esc | KeyCode::Char('q')) =>
            {
                let _dirty = update_launch_view(&mut v, LaunchMessage::ContainerInfoClosed);
                terminal.set_pointer_shape(false);
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.failure.is_some()
                    && matches!(k.code, KeyCode::Char('r' | 'R')) =>
            {
                if let Some(failure) = v.failure.as_ref()
                    && let Some((target, payload)) =
                        failure_reveal_payload(failure, ctx.run_id, v.failure_copy_hover)
                {
                    if ctx.terminal.reveal_file(std::path::Path::new(&payload)) {
                        let _dirty =
                            update_launch_view(&mut v, LaunchMessage::FailureRevealed(target));
                    } else {
                        ctx.terminal.emit_compact_line(
                            "failure-popup-reveal",
                            "host file reveal failed — badge suppressed",
                        );
                    }
                }
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
    use std::sync::Mutex;

    struct RecordingTerminal {
        copied: Mutex<Vec<String>>,
        revealed: Mutex<Vec<String>>,
    }

    impl RecordingTerminal {
        const fn new() -> Self {
            Self {
                copied: Mutex::new(Vec::new()),
                revealed: Mutex::new(Vec::new()),
            }
        }

        fn copied(&self) -> Vec<String> {
            self.copied.lock().expect("test clipboard lock").clone()
        }

        fn revealed(&self) -> Vec<String> {
            self.revealed.lock().expect("test reveal lock").clone()
        }
    }

    impl LaunchHostTerminal for RecordingTerminal {
        fn set_rich_surface_active(&self, _active: bool) {}
        fn host_screen_owned(&self) -> bool {
            false
        }
        fn is_debug_mode(&self) -> bool {
            true
        }
        fn emit_compact_line(&self, _kind: &str, _line: &str) {}
        fn set_pointer_shape(&self, _pointer: bool) {}
        fn copy_to_clipboard(&self, payload: &str) -> bool {
            self.copied
                .lock()
                .expect("test clipboard lock")
                .push(payload.to_owned());
            true
        }
        fn reveal_file(&self, path: &std::path::Path) -> bool {
            self.revealed
                .lock()
                .expect("test reveal lock")
                .push(path.display().to_string());
            true
        }
    }

    fn hit_point_for_payload(
        area: Rect,
        state: &jackin_tui::components::ContainerInfoState,
        payload: &str,
    ) -> (u16, u16) {
        let rect = launch_container_info_rect(area, state);
        for row in rect.y..rect.y.saturating_add(rect.height) {
            for col in rect.x..rect.x.saturating_add(rect.width) {
                if jackin_tui::components::container_info_copy_payload_at(rect, state, col, row)
                    .is_some_and(|(_, candidate)| candidate == payload)
                {
                    return (col, row);
                }
            }
        }
        panic!("copy target for {payload:?} not found");
    }

    fn hit_point_for_reveal_href(
        area: Rect,
        state: &jackin_tui::components::ContainerInfoState,
        href: &str,
    ) -> (u16, u16) {
        let rect = launch_container_info_rect(area, state);
        let reveal_row = state
            .rows()
            .iter()
            .enumerate()
            .find(|(_, row)| row.href() == Some(href) && !row.is_copyable())
            .map(|(idx, _)| idx)
            .expect("reveal-only row present");
        for row in rect.y..rect.y.saturating_add(rect.height) {
            for col in rect.x..rect.x.saturating_add(rect.width) {
                if jackin_tui::components::container_info_hyperlink_payload_at(
                    rect, state, col, row,
                )
                .is_some_and(|(idx, candidate)| idx == reveal_row && candidate == href)
                {
                    return (col, row);
                }
            }
        }
        panic!("reveal target for {href:?} not found");
    }

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

    #[test]
    fn build_log_body_click_is_swallowed() {
        let mut view = crate::tui::update::initial_view();
        view.build_log_open = true;
        view.build_log_lines = vec!["short".to_owned()];
        let area = Rect::new(0, 0, 80, 24);
        let terminal = crate::test_support::test_host_terminal();

        handle_cockpit_mouse_down(
            &mut view,
            CockpitContext {
                area,
                run_id: "jk-run-test",
                run_log_path: "/tmp/jk-run-test.jsonl",
                terminal,
                jackin_version: "jackin 0.0.0-test",
            },
            2,
            2,
        );

        assert!(view.build_log_open);
        assert!(!view.build_log_scroll_dragging);
    }

    #[test]
    fn container_info_click_copies_real_run_id_and_log_path() {
        let mut view = crate::tui::update::initial_view();
        view.container_info_open = true;
        let area = Rect::new(0, 0, 96, 24);
        let run_id = "jk-run-test";
        let run_log_path = "/tmp/jackin/runs/jk-run-test.jsonl";
        let terminal = RecordingTerminal::new();
        let ctx = CockpitContext {
            area,
            run_id,
            run_log_path,
            terminal: &terminal,
            jackin_version: "jackin 0.0.0-test",
        };

        let state =
            launch_container_info_state(&view, run_id, run_log_path, true, "jackin 0.0.0-test");
        let (run_col, run_row) = hit_point_for_payload(area, &state, run_id);
        handle_cockpit_mouse_down(&mut view, ctx, run_col, run_row);

        let state =
            launch_container_info_state(&view, run_id, run_log_path, true, "jackin 0.0.0-test");
        let (log_col, log_row) = hit_point_for_payload(area, &state, run_log_path);
        handle_cockpit_mouse_down(&mut view, ctx, log_col, log_row);

        assert_eq!(
            terminal.copied(),
            vec![run_id.to_owned(), run_log_path.to_owned()]
        );
    }

    #[test]
    fn container_info_reveal_row_opens_diagnostics_path() {
        let mut view = crate::tui::update::initial_view();
        view.container_info_open = true;
        let area = Rect::new(0, 0, 96, 24);
        let run_id = "jk-run-test";
        let run_log_path = "/tmp/jackin/runs/jk-run-test.jsonl";
        let terminal = RecordingTerminal::new();
        let ctx = CockpitContext {
            area,
            run_id,
            run_log_path,
            terminal: &terminal,
            jackin_version: "jackin 0.0.0-test",
        };

        let state =
            launch_container_info_state(&view, run_id, run_log_path, true, "jackin 0.0.0-test");
        let href = format!("file://{run_log_path}");
        let (col, row) = hit_point_for_reveal_href(area, &state, &href);
        handle_cockpit_mouse_down(&mut view, ctx, col, row);

        assert!(terminal.copied().is_empty());
        assert_eq!(terminal.revealed(), vec![run_log_path.to_owned()]);
    }

    #[test]
    fn container_info_reveal_key_opens_diagnostics_path() {
        let mut view = crate::tui::update::initial_view();
        view.container_info_open = true;
        let area = Rect::new(0, 0, 96, 24);
        let run_id = "jk-run-test";
        let run_log_path = "/tmp/jackin/runs/jk-run-test.jsonl";
        let terminal = RecordingTerminal::new();

        reveal_container_info_diagnostics(
            &mut view,
            CockpitContext {
                area,
                run_id,
                run_log_path,
                terminal: &terminal,
                jackin_version: "jackin 0.0.0-test",
            },
        );

        assert_eq!(terminal.revealed(), vec![run_log_path.to_owned()]);
    }

    #[test]
    fn failure_reveal_key_reveals_first_failure_path() {
        let mut view = crate::tui::update::initial_view();
        view.failure = Some(crate::tui::app::LaunchFailure {
            title: "Build failed".to_owned(),
            summary: "docker build failed".to_owned(),
            detail: None,
            next_step: None,
            stage: crate::tui::app::LaunchStage::DerivedImage,
            diagnostics_path: Some("/tmp/jackin/runs/jk-run-test.jsonl".into()),
            command_output_path: Some("/tmp/jackin/runs/jk-run-test.docker.log".into()),
        });
        let terminal = RecordingTerminal::new();
        let view = Arc::new(Mutex::new(view));

        {
            let mut guard = view.lock().expect("view lock");
            let failure = guard.failure.clone().expect("failure");
            let Some((target, payload)) = failure_reveal_payload(&failure, "jk-run-test", None)
            else {
                panic!("failure path should be revealable");
            };
            assert!(terminal.reveal_file(std::path::Path::new(&payload)));
            let _dirty = update_launch_view(&mut guard, LaunchMessage::FailureRevealed(target));
        }

        assert_eq!(
            terminal.revealed(),
            vec!["/tmp/jackin/runs/jk-run-test.jsonl"]
        );
        assert_eq!(
            view.lock().expect("view lock").failure_revealed,
            Some(crate::tui::app::FailureCopyTarget::DiagnosticsPath)
        );
    }
}
