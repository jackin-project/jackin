//! Launch cockpit input handling.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use jackin_tui::ModalOutcome;
use jackin_tui::components::KeyChord;
use jackin_tui::components::{ScrollAxes, StatusFooterHover};
use ratatui::layout::Rect;
use tokio_util::sync::CancellationToken;

use crate::tui::components::build_log_dialog::{
    build_log_scrollbar_top_offset_for_row_cached, refresh_build_log_layout,
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

/// What the render task should do after draining cockpit input this tick.
#[derive(Debug, PartialEq, Eq)]
pub enum CockpitOutcome {
    /// Keep rendering; cancellation (if any) flows through the cancel token.
    Continue,
    /// Ctrl+C: stop the process immediately. The render task restores the
    /// terminal and exits without running cleanup — no graceful teardown, no
    /// waiting on in-flight blocking work. Stale docker resources are left for
    /// the next launch's `gc_orphaned_resources` to sweep.
    HardExit,
}

/// Result of feeding a key to the open quit confirmation.
#[derive(Debug, PartialEq, Eq)]
enum QuitConfirmOutcome {
    /// Operator chose Yes — caller must cancel the launch.
    Confirmed,
    /// Operator chose No / Esc — dialog dismissed, launch resumes.
    Dismissed,
    /// Focus toggled or key ignored — dialog stays open.
    Pending,
}

/// `true` for a Ctrl+C key press. Hard cancel — wins over any open dialog.
fn is_ctrl_c(ev: &Event) -> bool {
    matches!(
        ev,
        Event::Key(k)
            if k.kind == KeyEventKind::Press
                && k.code == KeyCode::Char('c')
                && k.modifiers.contains(KeyModifiers::CONTROL)
    )
}

/// Route a key into the open quit confirmation, mutating `view.quit_confirm`.
/// Pure (no terminal / cancel-token side effects) so the policy is unit-tested
/// without driving a real event loop.
fn apply_quit_confirm_key(view: &mut LaunchView, key: event::KeyEvent) -> QuitConfirmOutcome {
    let Some(confirm) = view.quit_confirm.as_mut() else {
        return QuitConfirmOutcome::Pending;
    };
    match confirm.handle_key(key) {
        ModalOutcome::Commit(true) => {
            view.quit_confirm = None;
            QuitConfirmOutcome::Confirmed
        }
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
            view.quit_confirm = None;
            QuitConfirmOutcome::Dismissed
        }
        ModalOutcome::Continue => QuitConfirmOutcome::Pending,
    }
}

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
    let rect = launch_container_info_rect(ctx.area, &state, ctx.terminal.is_debug_mode());
    jackin_tui::components::clamp_container_info_scroll(
        &mut view.container_info_scroll,
        state.content_width(),
        state.content_height(),
        rect,
    );
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
        let rect = launch_container_info_rect(ctx.area, &state, ctx.terminal.is_debug_mode());
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
        }
        // Click inside with no copy target → no-op (Defect 11: inside click swallowed).
    } else if let Some(failure) = v.failure.as_ref() {
        if let Some(target) = failure_copy_target_at(
            ctx.area,
            failure,
            ctx.run_id,
            ctx.terminal.is_debug_mode(),
            col,
            row,
        ) && let Some(payload) = failure_copy_payload(failure, ctx.run_id, target)
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
        // Close stays keyboard-only (`Esc`); scrollbar hits remain interactive.
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
        let rect = launch_container_info_rect(ctx.area, &state, ctx.terminal.is_debug_mode());
        let hover = jackin_tui::components::container_info_copy_payload_at(rect, &state, col, row)
            .map(|(idx, _)| idx);
        if hover != v.container_info_hover {
            let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoHovered(hover));
            ctx.terminal.set_pointer_shape(hover.is_some());
        }
        return;
    }
    if let Some(failure) = v.failure.as_ref() {
        let hover = failure_copy_target_at(
            ctx.area,
            failure,
            ctx.run_id,
            ctx.terminal.is_debug_mode(),
            col,
            row,
        );
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
        usage: false,
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
    cancel_token: &CancellationToken,
) -> CockpitOutcome {
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
            return CockpitOutcome::Continue;
        };
        let Ok(mut v) = view.lock() else {
            return CockpitOutcome::Continue;
        };
        // Ctrl+C: immediate hard stop. The render task restores the terminal
        // and exits at once — no cleanup, no waiting on in-flight work. Checked
        // before the quit-confirm modal so it wins even while that dialog is
        // open. (Ctrl+Q, below, is the graceful, cleanup-running alternative.)
        if is_ctrl_c(&ev) {
            return CockpitOutcome::HardExit;
        }
        // While the quit confirmation is open it owns all input: route keys to
        // it (Yes → graceful cancel, No/Esc → dismiss) and swallow the rest.
        if v.quit_confirm.is_some() {
            if let Event::Key(k) = ev
                && k.kind == KeyEventKind::Press
                && apply_quit_confirm_key(&mut v, k) == QuitConfirmOutcome::Confirmed
            {
                // Yes confirmed: graceful cancel. The pipeline unwinds via
                // `Err` and runs `LoadCleanup` (removes the half-built
                // container, network, volume) before exiting.
                cancel_token.cancel();
                return CockpitOutcome::Continue;
            }
            continue;
        }
        match ev {
            // Ctrl+Q: ask before quitting. Opens the shared "Exit jackin'?"
            // confirmation; the dialog (drawn next tick) owns input until
            // answered. Unlike Ctrl+C this is reversible — No resumes launch.
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && crate::tui::keymap::COCKPIT_KEYMAP.dispatch(KeyChord::from(k))
                        == Some(crate::tui::keymap::CockpitAction::OpenQuitConfirm) =>
            {
                v.quit_confirm = Some(jackin_tui::components::exit_confirm_state());
                return CockpitOutcome::Continue;
            }
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
                        let rect = launch_container_info_rect(
                            ctx.area,
                            &state,
                            ctx.terminal.is_debug_mode(),
                        );
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
                let rect =
                    launch_container_info_rect(ctx.area, &state, ctx.terminal.is_debug_mode());
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
            Event::Key(k) if k.kind == KeyEventKind::Press && v.container_info_open => {
                use crate::tui::keymap::{CONTAINER_INFO_KEYMAP, ContainerInfoAction};
                match CONTAINER_INFO_KEYMAP.dispatch(KeyChord::from(k)) {
                    Some(ContainerInfoAction::CopyValue) => {
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
                            let _dirty = update_launch_view(
                                &mut v,
                                LaunchMessage::ContainerInfoCopied(copy_row),
                            );
                        }
                    }
                    Some(ContainerInfoAction::Close) => {
                        let _dirty = update_launch_view(&mut v, LaunchMessage::ContainerInfoClosed);
                        terminal.set_pointer_shape(false);
                    }
                    None => {}
                }
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.failure.is_some()
                    && crate::tui::keymap::FAILURE_KEYMAP
                        .dispatch(KeyChord::from(k))
                        .is_some() =>
            {
                // Failure popup is modal over the cockpit; Enter/Esc acknowledges
                // it so the awaiting `stage_failed` returns.
                let _dirty = update_launch_view(&mut v, LaunchMessage::FailureAcknowledged);
                terminal.set_pointer_shape(false);
            }
            Event::Key(k) if k.kind == KeyEventKind::Press && v.build_log_open => {
                use crate::tui::keymap::{BUILD_LOG_KEYMAP, BuildLogAction};
                let vertical = build_log_scroll_axes(&v, area).vertical;
                match BUILD_LOG_KEYMAP.dispatch(KeyChord::from(k)) {
                    Some(BuildLogAction::Close) => {
                        let _dirty = update_launch_view(&mut v, LaunchMessage::BuildLogClosed);
                    }
                    Some(BuildLogAction::ScrollUp) if vertical => {
                        update_build_log_scroll(&mut v, area, 1);
                    }
                    Some(BuildLogAction::ScrollDown) if vertical => {
                        update_build_log_scroll(&mut v, area, -1);
                    }
                    Some(BuildLogAction::PageUp) if vertical => {
                        update_build_log_scroll(&mut v, area, BUILD_LOG_PAGE_STEP as isize);
                    }
                    Some(BuildLogAction::PageDown) if vertical => {
                        update_build_log_scroll(&mut v, area, -(BUILD_LOG_PAGE_STEP as isize));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    CockpitOutcome::Continue
}

#[cfg(test)]
mod tests;
