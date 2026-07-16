#![expect(
    clippy::too_many_lines,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
//! Launch cockpit input handling.

use std::sync::{Arc, Mutex};

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::Rect;
use termrock::ModalOutcome;
use termrock::keymap::KeyChord;
use termrock::scroll::ScrollAxes;
use tokio_util::sync::CancellationToken;

use crate::tui::components::build_log_dialog::{
    build_log_scrollbar_top_offset_for_row_cached, refresh_build_log_layout, viewport_height,
    viewport_width,
};
use crate::tui::components::container_info_dialog::{
    launch_container_info_rect, launch_container_info_state,
};
use crate::tui::components::dialog::dialog_scroll_axes;
use crate::tui::components::failure_dialog::{
    failure_copy_payload, failure_copy_target_at, failure_popup_block_rect,
    failure_popup_body_metrics,
};
use crate::tui::components::footer::{
    FooterSlot, StatusFooterHover, footer_instance, footer_regions, format_activity,
};
use crate::tui::input::{LaunchInput, is_ctrl_c_event};
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
    is_ctrl_c_event(ev)
}

/// Route a key into the open quit confirmation, mutating `view.quit_confirm`.
/// Pure (no terminal / cancel-token side effects) so the policy is unit-tested
/// without driving a real event loop.
fn apply_quit_confirm_key(view: &mut LaunchView, key: event::KeyEvent) -> QuitConfirmOutcome {
    let Some(confirm) = view.quit_confirm.as_mut() else {
        return QuitConfirmOutcome::Pending;
    };
    match confirm.handle_key(key.into()) {
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

fn cockpit_outcome_for_quit_confirm(outcome: QuitConfirmOutcome) -> CockpitOutcome {
    match outcome {
        QuitConfirmOutcome::Confirmed => CockpitOutcome::HardExit,
        QuitConfirmOutcome::Dismissed | QuitConfirmOutcome::Pending => CockpitOutcome::Continue,
    }
}

#[derive(Clone, Copy)]
struct CockpitContext<'a> {
    area: Rect,
    run_id: &'a str,
    run_log_path: Option<&'a str>,
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
    view.container_info_scroll.clamp(
        state.content_height(),
        usize::from(rect.height.saturating_sub(2)),
        state.content_width(),
        usize::from(rect.width.saturating_sub(2)),
    );
}

/// Clamp the failure-popup body scroll to its content so over-scrolling cannot
/// accumulate (mirrors `clamp_container_info_scroll`). Called after every
/// failure-body scroll key/wheel; the renderer also clamps defensively.
fn clamp_failure_scroll(view: &mut LaunchView, ctx: CockpitContext<'_>) {
    let Some(failure) = view.failure.as_ref() else {
        return;
    };
    let (body, content_height) =
        failure_popup_body_metrics(ctx.area, failure, ctx.run_id, ctx.terminal.is_debug_mode());
    let max = content_height.saturating_sub(usize::from(body.height));
    view.failure_scroll.scroll_y = view
        .failure_scroll
        .scroll_y
        .min(u16::try_from(max).unwrap_or(u16::MAX));
}

/// Vertical-only scroll axes for the failure popup body, gated on whether the
/// wrapped content actually overflows the body viewport.
fn failure_body_scroll_axes(view: &LaunchView, ctx: CockpitContext<'_>) -> ScrollAxes {
    let Some(failure) = view.failure.as_ref() else {
        return ScrollAxes {
            vertical: false,
            horizontal: false,
        };
    };
    let (body, content_height) =
        failure_popup_body_metrics(ctx.area, failure, ctx.run_id, ctx.terminal.is_debug_mode());
    ScrollAxes {
        vertical: content_height > usize::from(body.height),
        horizontal: false,
    }
}

/// Apply a wheel event to the failure popup body. Extracted so the input match
/// arm stays flat enough for `clippy::excessive_nesting`; the caller's guard
/// already proved `view.failure.is_some()`.
fn apply_failure_body_wheel_scroll(
    view: &mut LaunchView,
    ctx: CockpitContext<'_>,
    kind: MouseEventKind,
    modifiers: KeyModifiers,
) {
    let axes = failure_body_scroll_axes(view, ctx);
    if view
        .failure_scroll
        .handle_mouse(kind.into(), modifiers.into(), axes)
    {
        clamp_failure_scroll(view, ctx);
    }
}

/// Apply a vertical scroll key to the failure popup body. Extracted for the
/// same nesting reason as `apply_failure_body_wheel_scroll`.
fn apply_failure_body_key_scroll(
    view: &mut LaunchView,
    ctx: CockpitContext<'_>,
    key: event::KeyEvent,
) {
    let Some(failure) = view.failure.as_ref() else {
        return;
    };
    let (body, content_height) =
        failure_popup_body_metrics(ctx.area, failure, ctx.run_id, ctx.terminal.is_debug_mode());
    let viewport_h = usize::from(body.height);
    let axes = ScrollAxes {
        vertical: content_height > viewport_h,
        horizontal: false,
    };
    let _consumed = view.failure_scroll.handle_key_for_axes(
        key.into(),
        content_height,
        viewport_h,
        usize::MAX,
        usize::MAX,
        axes,
    );
    clamp_failure_scroll(view, ctx);
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
    let viewport_w = viewport_width(box_area);
    let viewport_h = viewport_height(box_area);
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
    let Some(delta) = termrock::scroll::mouse_scroll_delta(
        kind.into(),
        modifiers.into(),
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
    footer_regions(
        footer_row,
        &format_activity(&view.status),
        &instance,
        debug_mode.then_some(run_id),
    )
    .into_iter()
    .any(|region| {
        matches!(region.id, FooterSlot::Container | FooterSlot::RunId)
            && row >= region.area.y
            && row < region.area.bottom()
            && col >= region.area.x
            && col < region.area.right()
    })
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
        if !rect.contains(ratatui::layout::Position { x: col, y: row }) {
            // Click outside the dialog → dismiss (Defect 11).
            let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoClosed);
        } else if let Some((copy_row, payload)) =
            crate::tui::components::container_info::copy_payload_at(rect, &state, col, row)
        {
            // Click inside on a copyable value → copy.
            if ctx.terminal.copy_to_clipboard(&payload) {
                let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoCopied(copy_row));
            }
            // If clipboard write failed: no-op (no close, no dirty).
        }
        // Click inside with no copy target → no-op (Defect 11: inside click swallowed).
    } else if let Some(failure) = v.failure.as_ref() {
        // The failure popup is modal: route the click through the shared modal
        // classifier so outside clicks acknowledge the failure (same path as
        // Enter/Esc), inside copy targets copy, and inside non-target clicks
        // are swallowed instead of falling through to build-log/container-info
        // behavior.
        let failure_scroll = v.failure_scroll.clone();
        let popup_rect =
            failure_popup_block_rect(ctx.area, failure, ctx.run_id, ctx.terminal.is_debug_mode());
        if popup_rect.contains(ratatui::layout::Position { x: col, y: row }) {
            if let Some(target) = failure_copy_target_at(
                ctx.area,
                failure,
                ctx.run_id,
                ctx.terminal.is_debug_mode(),
                col,
                row,
                Some(failure_scroll),
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
            // Inside non-target click → swallowed (no overlay behavior).
        } else {
            let _dirty = update_launch_view(v, LaunchMessage::FailureAcknowledged);
            ctx.terminal.set_pointer_shape(false);
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
        let hover = crate::tui::components::container_info::copy_payload_at(rect, &state, col, row)
            .map(|(idx, _)| idx);
        if hover != v.container_info_hover {
            let _dirty = update_launch_view(v, LaunchMessage::ContainerInfoHovered(hover));
            ctx.terminal.set_pointer_shape(hover.is_some());
        }
        return;
    }
    if let Some(failure) = v.failure.as_ref() {
        let failure_scroll = v.failure_scroll.clone();
        let hover = failure_copy_target_at(
            ctx.area,
            failure,
            ctx.run_id,
            ctx.terminal.is_debug_mode(),
            col,
            row,
            Some(failure_scroll),
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
        && footer_regions(
            Rect {
                x: 0,
                y: ctx.area.height.saturating_sub(1),
                width: ctx.area.width,
                height: 1,
            },
            &format_activity(&v.status),
            &footer_instance(v),
            Some(ctx.run_id),
        )
        .into_iter()
        .any(|region| {
            region.id == FooterSlot::RunId
                && row >= region.area.y
                && row < region.area.bottom()
                && col >= region.area.x
                && col < region.area.right()
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

fn emit_dialog_mouse_debug_telemetry(
    terminal: &dyn LaunchHostTerminal,
    container_info_open: bool,
    build_log_open: bool,
    last_dialog_mouse_cell: &mut Option<(u16, u16)>,
    m: event::MouseEvent,
) {
    if !terminal.is_debug_mode() || !(container_info_open || build_log_open) {
        *last_dialog_mouse_cell = None;
        return;
    }
    let cell = (m.column, m.row);
    if should_emit_dialog_mouse(m.kind, *last_dialog_mouse_cell, cell) {
        if matches!(m.kind, MouseEventKind::Moved) {
            *last_dialog_mouse_cell = Some(cell);
        }
        jackin_diagnostics::incr_mouse_events();
        // Moved rows only at TRACE; clicks/drags stay debug-tier.
        let emit_debug = if matches!(m.kind, MouseEventKind::Moved) {
            jackin_diagnostics::telemetry_level(jackin_diagnostics::is_debug_mode())
                == jackin_diagnostics::TelemetryLevel::Trace
        } else {
            true
        };
        if emit_debug {
            terminal.emit_debug_line(
                "cockpit-dialog-mouse",
                &format!(
                    "kind={:?} modifiers={:?} col={} row={} container_info_open={} build_log_open={}",
                    m.kind, m.modifiers, m.column, m.row, container_info_open, build_log_open
                ),
            );
        }
    }
}

fn should_emit_dialog_mouse(
    kind: MouseEventKind,
    previous_cell: Option<(u16, u16)>,
    cell: (u16, u16),
) -> bool {
    !matches!(kind, MouseEventKind::Moved) || previous_cell != Some(cell)
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
    run_log_path: Option<&str>,
    terminal: &dyn LaunchHostTerminal,
    jackin_version: &'static str,
    _cancel_token: &CancellationToken,
    input: &LaunchInput,
) -> CockpitOutcome {
    let area = current_terminal_area();
    let ctx = CockpitContext {
        area,
        run_id,
        run_log_path,
        terminal,
        jackin_version,
    };
    while let Some(ev) = input.try_recv() {
        let Ok(mut v) = view.lock() else {
            return CockpitOutcome::Continue;
        };
        // Ctrl+C: immediate hard stop. The render task restores the terminal
        // and exits at once — no cleanup, no waiting on in-flight work. Checked
        // before the quit-confirm modal so it wins even while that dialog is
        // open. The input owner also treats a rapid second Ctrl+C as this same
        // hard stop even if the render task is starved.
        if is_ctrl_c(&ev) {
            return CockpitOutcome::HardExit;
        }
        // While the quit confirmation is open it owns all input: route keys to
        // it (Yes → hard exit, No/Esc → dismiss) and swallow the rest.
        if v.quit_confirm.is_some() {
            if let Event::Key(k) = ev
                && k.kind == KeyEventKind::Press
            {
                let outcome = cockpit_outcome_for_quit_confirm(apply_quit_confirm_key(&mut v, k));
                // Yes confirmed: immediate hard exit, matching Ctrl+C. This
                // deliberately skips graceful cleanup so a slow build cannot
                // keep the operator trapped in the launch surface.
                if outcome == CockpitOutcome::HardExit {
                    return outcome;
                }
            }
            continue;
        }
        match ev {
            // Ctrl+Q: ask before quitting. Opens the shared "Exit jackin❯?"
            // confirmation; the dialog (drawn next tick) owns input until
            // answered. No/Esc resumes launch; Yes hard-exits immediately.
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && crate::tui::keymap::COCKPIT_KEYMAP
                        .dispatch(KeyChord::from(termrock::crossterm::key(k)))
                        == Some(crate::tui::keymap::CockpitAction::OpenQuitConfirm) =>
            {
                v.quit_confirm =
                    Some(termrock::components::ConfirmState::new("Exit jackin❯?").with_focus_yes());
                return CockpitOutcome::Continue;
            }
            Event::Mouse(m) => {
                // Durable telemetry: capture exactly what the terminal delivers
                // for a dialog mouse event so a `--debug` run reveals whether a
                // horizontal-scroll gesture even reaches the cockpit (and as what
                // kind/modifiers), instead of guessing at the mapping.
                let container_info_open = v.container_info_open;
                let build_log_open = v.build_log_open;
                emit_dialog_mouse_debug_telemetry(
                    terminal,
                    container_info_open,
                    build_log_open,
                    &mut v.last_dialog_mouse_cell,
                    m,
                );
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
                    // The failure popup owns the wheel while open (it wins over
                    // the build-log overlay because `StageFailed` clears
                    // `build_log_open`). Long diagnostics scroll vertically.
                    kind if v.failure.is_some() => {
                        apply_failure_body_wheel_scroll(&mut v, ctx, kind, m.modifiers);
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
                        let axes =
                            dialog_scroll_axes(state.content_width(), state.content_height(), rect);
                        if v.container_info_scroll.handle_mouse(
                            kind.into(),
                            m.modifiers.into(),
                            axes,
                        ) {
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
                let axes = dialog_scroll_axes(state.content_width(), state.content_height(), rect);
                let _consumed = v.container_info_scroll.handle_key_for_axes(
                    k.into(),
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
                match CONTAINER_INFO_KEYMAP.dispatch(KeyChord::from(termrock::crossterm::key(k))) {
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
            // Vertical scroll for the failure popup body (arrows / j/k / PgUp /
            // PgDn). Reaches long diagnostics or next-step rows that exceed the
            // viewport-safe popup height; Enter/Esc still acknowledge below.
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.failure.is_some()
                    && matches!(
                        k.code,
                        KeyCode::Up
                            | KeyCode::Down
                            | KeyCode::PageUp
                            | KeyCode::PageDown
                            | KeyCode::Char('j' | 'J' | 'k' | 'K')
                    ) =>
            {
                apply_failure_body_key_scroll(&mut v, ctx, k);
            }
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.failure.is_some()
                    && crate::tui::keymap::FAILURE_KEYMAP
                        .dispatch(KeyChord::from(termrock::crossterm::key(k)))
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
                match BUILD_LOG_KEYMAP.dispatch(KeyChord::from(termrock::crossterm::key(k))) {
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
