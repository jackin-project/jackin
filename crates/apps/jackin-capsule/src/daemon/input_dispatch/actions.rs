// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Top-level `Action` dispatcher for multiplexed input.

use crate::tui::components::branch_context_bar::{branch_context_bar_hit, debug_run_id_label};
use crate::tui::input::TAB_DOUBLE_CLICK_WINDOW;

use crate::tui::update::action_frame_plan;

use jackin_telemetry::ResultTelemetryExt as _;

use super::super::{
    Action, Dialog, Instant, Multiplexer, PaletteToggleRoute, StatusBarClickState,
    branch_context_bar_click_action, drag_resize_redraw_reason, focus_change_redraw_reason,
    mouse_release_action, palette_toggle_route, pane_button_motion_action,
    selection_change_redraw_reason, selection_start_redraw_reason, status_bar_click_action,
};

impl Multiplexer {
    /// Record the invalidation an action implies. Handlers only mutate
    /// state; the render loop composes when the generation moved.
    fn invalidate_for(&mut self, action: &Action) {
        if let Some(plan) = action_frame_plan(action) {
            self.invalidate(plan.reason());
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Action dispatcher with one arm per multiplexed `Action` variant — \
              each arm applies its focused state mutation. Extracting arms into \
              sub-dispatchers would require re-borrowing the multiplexer state \
              across fn boundaries and obscure the per-action readability."
    )]
    pub(crate) fn apply_action(&mut self, action: Action) {
        match action {
            Action::OpenPalette => {
                self.cancel_drag();
                match palette_toggle_route(self.dialog_open()) {
                    PaletteToggleRoute::CloseDialog => self.dialog_clear(),
                    PaletteToggleRoute::OpenPalette => self.open_command_palette(),
                }
                self.invalidate_for(&Action::OpenPalette);
            }
            Action::RequestExit => {
                // Ctrl+Q → confirm before the force-stop. Esc/No dismisses and
                // resumes; Yes routes to ExitAllSessions (immediate teardown).
                self.cancel_drag();
                self.dialog_push(Dialog::new_confirm_action(
                    crate::tui::components::dialog::ConfirmKind::Exit,
                ));
                self.invalidate_for(&Action::RequestExit);
            }
            Action::OpenContainerInfo => {
                self.open_container_info_dialog();
                self.invalidate_for(&Action::OpenContainerInfo);
            }
            Action::OpenGithubContext => {
                self.open_github_context_dialog(Instant::now());
                self.invalidate_for(&Action::OpenGithubContext);
            }
            Action::OpenUsage => {
                let view = self.focused_usage_snapshot();
                self.dialog_push(Dialog::new_usage(view));
                self.request_usage_refresh_for_provider(None);
                self.invalidate_for(&Action::OpenUsage);
            }
            Action::OpenRenameTab(idx) => {
                if idx >= self.session_supervisor.tabs.len() {
                    return;
                }
                self.cancel_drag();
                let initial = self.session_supervisor.tabs[idx]
                    .custom_label()
                    .map(str::to_owned)
                    .unwrap_or_default();
                self.dialog_push(Dialog::new_rename_tab(idx, initial));
                self.render.last_tab_click = None;
                self.invalidate_for(&Action::OpenRenameTab(idx));
            }
            Action::OpenAgentPicker(intent) => {
                let agents = self.launch_env.available_instances.clone();
                self.dialog_push(Dialog::new_agent_picker(agents, intent));
                self.invalidate_for(&Action::OpenAgentPicker(intent));
            }
            Action::SwitchTab(idx) => {
                if idx >= self.session_supervisor.tabs.len()
                    || idx == self.session_supervisor.active_tab
                {
                    return;
                }
                self.cancel_drag();
                let prev = self.active_focused_id();
                self.session_supervisor.active_tab = idx;
                self.synthesise_focus_swap(prev, self.active_focused_id());
                self.invalidate_for(&Action::SwitchTab(idx));
            }
            Action::NextTab => {
                self.next_tab();
                self.invalidate_for(&Action::NextTab);
            }
            Action::PreviousTab => {
                self.prev_tab();
                self.invalidate_for(&Action::PreviousTab);
            }
            Action::JumpTab(idx) => {
                self.jump_tab(idx);
                self.invalidate_for(&Action::JumpTab(idx));
            }
            Action::SplitFocused(direction) => {
                drop(self.split_focused(direction).record_telemetry_error(
                    jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
                ));
                self.invalidate_for(&Action::SplitFocused(direction));
            }
            Action::MoveFocus(dir) => {
                self.move_focus(dir);
                self.invalidate_for(&Action::MoveFocus(dir));
            }
            Action::ToggleZoom => {
                self.toggle_zoom();
                self.invalidate_for(&Action::ToggleZoom);
            }
            Action::CloseFocusedPane => {
                self.close_focused_pane();
                self.invalidate_for(&Action::CloseFocusedPane);
            }
            Action::CloseFocusedTab => {
                self.close_focused_tab();
                self.invalidate_for(&Action::CloseFocusedTab);
            }
            Action::ClearFocusedPane => {
                self.clear_focused_pane();
                self.invalidate_for(&Action::ClearFocusedPane);
            }
            Action::Detach => {
                self.client_registry.detach_requested = true;
                self.invalidate_for(&Action::Detach);
            }
            Action::RefreshUsage => {
                self.request_usage_refresh_for_provider(None);
                self.invalidate_for(&Action::RefreshUsage);
            }
            Action::Palette(cmd) => self.handle_palette_command(cmd),
            Action::Prefix(cmd) => {
                if !self.dialog_captures_input() {
                    self.handle_prefix_command(cmd);
                }
            }
            Action::ResizePane(dir) => {
                if !self.dialog_captures_input() {
                    self.resize_focused(dir);
                    self.invalidate_for(&Action::ResizePane(dir));
                }
            }
            Action::FocusReport(focused) => {
                if self.dialog_captures_input() {
                    return;
                }
                let bytes = if focused {
                    b"\x1b[I".as_ref()
                } else {
                    b"\x1b[O".as_ref()
                };
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.session_supervisor.sessions.get(focused)
                    && session.focus_events_enabled()
                {
                    let _sent = session.send_input(bytes);
                }
            }
            Action::MouseChromeUpdate { row, col, button } => {
                self.update_hover_for_mouse(row, col, button);
                self.update_pointer_shape_for_mouse(row, col, button);
            }
            Action::Wheel { row, col, button } => self.apply_wheel_action(row, col, button),
            Action::FocusPaneAt { row, col } => {
                if let Some(reason) = focus_change_redraw_reason(self.focus_pane_at(row, col)) {
                    self.invalidate(reason);
                }
            }
            Action::OpenVisibleUrlAt { row, col, button } => {
                if !self.open_visible_url_at(row, col) && !self.export_visible_file_at(row, col) {
                    self.apply_action(Action::ForwardMouse {
                        row,
                        col,
                        button,
                        press: true,
                    });
                }
            }
            Action::PanePrimaryPress { row, col } => {
                if self.clipboard.selection.is_some() || self.clipboard.selection_copied {
                    self.clipboard.selection = None;
                    self.clipboard.selection_copied = false;
                    self.clipboard.selection_copy_feedback_deadline = None;
                    // Stamp the press even though it only cleared the old
                    // highlight: a double-click on the next word should be
                    // two presses, not three. The return value is ignored
                    // because a double cannot resolve here — every selection
                    // setter clears `last_pane_press` first, so this press
                    // can only be a fresh first half.
                    if let Some(candidate) = self.detect_selection_start(row, col) {
                        self.register_pane_press(&candidate);
                    }
                    self.invalidate(selection_change_redraw_reason());
                    return;
                }
                // Press on a shared pane border starts a drag — skip focus
                // switch and PTY forward in that case.
                if self.detect_drag_start(row, col).is_some() {
                    self.apply_action(Action::StartDragResize { row, col });
                    return;
                }
                // Press on the focused pane's scrollbar track jumps the
                // scrollback view to the clicked position.
                if self.scrollbar_jump_at(row, col) {
                    return;
                }
                // Click on a pane other than the currently-focused one switches
                // focus first so the operator never has to click twice. Selection
                // or PTY-mouse forwarding then runs against the freshly-focused
                // pane.
                self.apply_action(Action::FocusPaneAt { row, col });
                // Press inside a pane whose program never asked for a mouse
                // protocol arms a text selection. A double-click selects and
                // copies the word under the cursor immediately; a single
                // press only becomes a selection after motion leaves the
                // press cell, so a plain click stays a click/focus gesture
                // and never interacts with copy.
                if let Some(selection) = self.detect_selection_start(row, col) {
                    if self.register_pane_press(&selection) {
                        return;
                    }
                    self.clipboard.pending_selection = Some(selection);
                    return;
                }
                self.apply_action(Action::ForwardMouse {
                    row,
                    col,
                    button: 0,
                    press: true,
                });
            }
            Action::PaneButtonMotion { row, col } => {
                if self.clipboard.pending_selection.is_some() && self.clipboard.selection.is_none()
                {
                    self.pending_selection_motion(row, col);
                    return;
                }
                let action = pane_button_motion_action(
                    self.render.drag.is_some(),
                    self.clipboard.selection.is_some(),
                    row,
                    col,
                );
                self.apply_action(action);
            }
            Action::StatusBarClick { col } => {
                let tab = self.status.status_bar.tab_at_col(col + 1);
                let now = Instant::now();
                let double_click = tab
                    .and_then(|idx| {
                        self.render.last_tab_click.filter(|(prev_idx, prev_t)| {
                            *prev_idx == idx
                                && now.duration_since(*prev_t) <= TAB_DOUBLE_CLICK_WINDOW
                        })
                    })
                    .is_some();
                let Some(action) = status_bar_click_action(StatusBarClickState {
                    tab,
                    tab_count: self.session_supervisor.tabs.len(),
                    double_click,
                    menu_hit: self.status.status_bar.hint_at(1, col + 1),
                }) else {
                    return;
                };
                if matches!(action, Action::SwitchTab(_)) {
                    self.render.last_tab_click = tab.map(|idx| (idx, now));
                    // P5: clicking a tab moves focus onto the tab bar (green
                    // underline + Left/Right nav until the agent is re-focused).
                    self.set_tab_bar_focused(true);
                }
                self.apply_action(action);
            }
            Action::BranchContextBarClick { row, col } => {
                let usage_status_label = self.focused_usage_snapshot().status_bar_label;
                let hit = branch_context_bar_hit(
                    row + 1,
                    col + 1,
                    self.render.term_rows,
                    self.render.term_cols,
                    self.context_bar_branch(),
                    Some(&usage_status_label),
                    self.pr_watch.pull_request_context.as_deref(),
                    self.pull_request_context_loading(),
                    debug_run_id_label().as_deref(),
                    self.status.status_bar.instance_id_label(),
                );
                let Some(action) = branch_context_bar_click_action(hit) else {
                    return;
                };
                self.apply_action(action);
            }
            Action::ForwardMouse {
                row,
                col,
                button,
                press,
            } => {
                self.forward_mouse_to_focused_pane_with_kind(col, row, button, press);
            }
            Action::MouseRelease { row, col, button } => {
                if self.clipboard.pending_selection.is_some() && self.clipboard.selection.is_none()
                {
                    self.clipboard.pending_selection = None;
                    return;
                }
                let action = mouse_release_action(
                    self.render.drag.is_some(),
                    self.clipboard.selection.is_some(),
                    row,
                    col,
                    button,
                );
                self.apply_action(action);
            }
            Action::PaneData(bytes) => {
                self.send_bytes_to_focused_pane(&bytes);
            }
            Action::StartDragResize { row, col } => {
                self.render.drag = self.detect_drag_start(row, col);
            }
            Action::DragMotion { row, col } => self.drag_motion(row, col),
            Action::EndDragResize => {
                self.render.drag = None;
                self.invalidate(drag_resize_redraw_reason());
            }
            Action::StartSelection { row, col } => {
                self.clipboard.pending_selection = None;
                self.clipboard.selection_copied = false;
                self.clipboard.selection_copy_feedback_deadline = None;
                self.clipboard.selection = self.detect_selection_start(row, col);
                if let Some(reason) =
                    selection_start_redraw_reason(self.clipboard.selection.is_some())
                {
                    self.invalidate(reason);
                }
            }
            Action::SelectionMotion { row, col } => self.selection_motion(row, col),
            Action::FinalizeSelection => self.finalize_selection(),
            Action::DialogClick { row, col } => {
                // Mouse handling while a dialog overlay is up:
                //   click on a row  -> select + confirm
                //   click on border / padding -> swallowed
                //   click anywhere outside the box -> dismiss
                //
                // SGR mouse coords are 0-based; `box_rect` returns
                // render-side coords that are 1-based (the values passed to
                // `move_to`, which emits `\x1b[r;cH`). Pass row+1 / col+1 here
                // so the dialog can classify the modal click in render coords.
                let term_rows = self.render.term_rows;
                let term_cols = self.render.term_cols;
                let Some(action) = self.dispatch_to_dialog_top(|dialog, github| {
                    dialog.handle_click(row + 1, col + 1, term_rows, term_cols, github)
                }) else {
                    return;
                };
                self.apply_action(Action::Dialog(action));
            }
            Action::Dialog(action) => self.apply_dialog_action(action),
        }
    }
}
