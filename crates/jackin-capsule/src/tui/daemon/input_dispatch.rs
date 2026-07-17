//! TUI input dispatch methods for the daemon-owned `Multiplexer`.

use std::sync::Arc;

use crate::tui::components::branch_context_bar::{branch_context_bar_hit, debug_run_id_label};
use crate::tui::input::TAB_DOUBLE_CLICK_WINDOW;
use crate::tui::update::DIALOG_COPY_FEEDBACK_DURATION;
use crate::tui::update::action_frame_plan;
use crate::tui::update::prefix_full_redraw_reason;
use crate::tui::view::encode_osc52_clipboard_write;
use jackin_protocol::attach::ServerFrame;
use jackin_telemetry::ResultTelemetryExt as _;

use super::{
    Action, ConfirmedActionRoute, Dialog, DialogAction, FullRedrawReason, InputDispatchContext,
    InputEvent, Instant, Multiplexer, PaletteCommand, PaletteCommandRoute, PaletteToggleRoute,
    PickerIntent, PrefixCommand, StatusBarClickState, branch_context_bar_click_action,
    confirmed_action_route, dialog_action_frame_plan, drag_resize_redraw_reason,
    encode_wheel_cursor_fallback, focus_change_redraw_reason, github_context_view_from_state,
    input_event_action, mouse_chrome_update_action, mouse_release_action, palette_command_route,
    palette_route_frame_plan, palette_toggle_route, pane_button_motion_action,
    pane_data_redraw_reason, pane_wheel_cursor_fallback_reason, prefix_command_action,
    selection_change_redraw_reason, selection_start_redraw_reason, status_bar_click_action,
    wheel_scrollback_redraw_reason,
};

impl Multiplexer {
    /// Record the invalidation an action implies. Handlers only mutate
    /// state; the render loop composes when the generation moved.
    fn invalidate_for(&mut self, action: &Action) {
        if let Some(plan) = action_frame_plan(action) {
            self.invalidate(plan.reason());
        }
    }

    pub(super) fn open_host_url_from_dialog(&mut self, url: String, opening_allowed: bool) {
        if !opening_allowed {
            self.set_clipboard_image_notice(
                "Host link opening disabled by JACKIN_OPEN_LINKS".to_owned(),
            );
            return;
        }
        if !crate::tui::url_text::is_host_open_url(&url) {
            self.set_clipboard_image_notice(
                "Host link rejected: unsupported URL scheme".to_owned(),
            );
            return;
        }
        self.send_protocol_frame(ServerFrame::HostOpenUrl(url));
    }

    /// Single dispatch point for a `DialogAction`. Both the
    /// mouse-click and key-event paths call `Dialog::handle_*`
    /// and route the result here, so adding a new variant means
    /// updating one match arm instead of two.
    pub(super) fn apply_dialog_action(&mut self, action: DialogAction) {
        let frame_plan = dialog_action_frame_plan(&action);
        match action {
            DialogAction::Dismiss => {
                // Back-navigation: pop one dialog so a sub-dialog
                // reveals its parent rather than closing the whole
                // flow. Operator at the top of stack (Menu) pops to
                // an empty stack — same effective "close" the
                // pre-stack code achieved with `self.dialog = None`.
                self.dialog_pop_one();
            }
            DialogAction::Redraw | DialogAction::Consume => {}
            DialogAction::ExecConfirm {
                command,
                args,
                selected,
            } => {
                // Operator approved. Close the picker, then resolve the chosen
                // credentials through the host socket and run the command off
                // the event loop so the daemon keeps rendering; the spawned task
                // owns the deferred control reply and answers when the command
                // finishes (or fails closed).
                self.dialog_pop_one();
                if let Some(reply_tx) = self.control.pending_exec_reply.take() {
                    reply_tx.spawn(async move { run_exec_selected(command, args, selected).await });
                }
            }
            DialogAction::ExecCancel => {
                self.dialog_pop_one();
                if let Some(reply_tx) = self.control.pending_exec_reply.take() {
                    reply_tx.send(jackin_protocol::control::ServerMsg::ExecDenied {
                        reason: "operator cancelled credential selection".to_owned(),
                    });
                }
            }
            DialogAction::ExitDirty(row) => {
                use crate::tui::components::dialog::ExitDirtyRow;
                match row {
                    // Open the verbatim New-tab agent picker over the exit modal.
                    // Picking an agent spawns a session and clears the dialog
                    // stack (SpawnAgent → dialog_clear), dismissing the modal.
                    ExitDirtyRow::StartNewAgent => {
                        self.apply_action(Action::OpenAgentPicker(PickerIntent::NewTab));
                        return;
                    }
                    // Push the read-only changed-files list stored in the
                    // ExitDirty variant; Arc::clone is O(1). Esc walks back.
                    ExitDirtyRow::Inspect => {
                        let rows = match self.dialog_top() {
                            Some(Dialog::ExitDirty { inspect_rows, .. }) => {
                                Arc::clone(inspect_rows)
                            }
                            _ => return,
                        };
                        self.dialog_push(Dialog::new_exit_inspect(rows));
                        self.invalidate(FullRedrawReason::DialogChange);
                        return;
                    }
                    // Record the operator's choice; the event loop writes the
                    // exit-action file and drains on the next iteration.
                    ExitDirtyRow::Keep => {
                        self.control.exit_request = Some(jackin_protocol::ExitAction::Keep);
                    }
                    ExitDirtyRow::Discard => {
                        self.control.exit_request = Some(jackin_protocol::ExitAction::Discard);
                    }
                }
            }
            DialogAction::Command(cmd) => {
                // `handle_palette_command` decides per-arm whether
                // the command opens a sub-dialog (push) or finishes
                // the flow (clear stack). It records its own
                // invalidation, so return before the generic one.
                self.apply_action(Action::Palette(cmd));
                return;
            }
            DialogAction::SpawnAgent { agent, intent } => {
                let providers = self.providers_for_agent(agent.as_deref());
                if providers.len() > 1 {
                    // Multiple providers available — push ProviderPicker
                    // on top so the operator chooses before spawning.
                    let choices = providers
                        .into_iter()
                        .map(|provider| {
                            crate::tui::components::dialog::ProviderChoice::new(provider.label())
                        })
                        .collect();
                    self.dialog_push(Dialog::new_provider_picker(agent, choices, intent));
                    self.invalidate(FullRedrawReason::DialogChange);
                    return;
                }
                // Zero or one provider — spawn immediately without
                // a picker step (operator experience unchanged when
                // Z.AI is not configured).
                self.dialog_clear();
                self.dispatch_spawn_intent(agent, intent);
            }
            DialogAction::SpawnAgentWithProvider {
                agent,
                provider_label,
                intent,
            } => {
                self.dialog_clear();
                // Token resolved here from the env key captured for the picked
                // provider — never a fixed provider's key.
                let env_overrides = jackin_protocol::Provider::from_label(&provider_label)
                    .map_or_else(
                        || {
                            let _warning = jackin_telemetry::record_recovered_degradation();
                            Vec::new()
                        },
                        |provider| {
                            self.provider_spawn_env(agent.as_deref().unwrap_or_default(), provider)
                        },
                    );
                self.dispatch_spawn_intent_with_provider(
                    agent,
                    intent,
                    &env_overrides,
                    Some(provider_label.as_str()),
                );
            }
            DialogAction::RenameTab { tab_idx, label } => {
                self.dialog_clear();
                if let Some(tab) = self.session_supervisor.tabs.get_mut(tab_idx) {
                    tab.set_custom_label(label);
                }
            }
            DialogAction::CopyToClipboard(payload) => {
                // OSC 52 selection write — `\x1b]52;c;<base64>\x07`.
                // `c` is the system clipboard target; modern terminals
                // (Ghostty, iTerm2, Kitty, Alacritty, wezterm, recent
                // gnome-terminal) all honour it. Older / locked-down
                // terminals silently drop the sequence — the copy
                // appears to do nothing but no error fires; the
                // multiplexer can't tell from this side. Emitted to
                // the client via `send_output`; the alt-screen path
                // forwards it byte-for-byte to the operator's outer
                // terminal.
                //
                // Copy-capable dialogs stay on the stack — the
                // operator's "did it actually copy?" question is
                // answered by the copied check affordance the renderer
                // paints now that `copied = true` (flipped by the
                // dialog's handle_key or row-click handler before this
                // action returned).
                // The badge expires from the daemon's tick loop.
                self.send_out_of_band(encode_osc52_clipboard_write(&payload));
                self.clipboard.dialog_copy_feedback_deadline =
                    Some(Instant::now() + DIALOG_COPY_FEEDBACK_DURATION);
            }
            DialogAction::OpenHostUrl(url) => {
                self.open_host_url_from_dialog(url, super::mouse_input::host_url_opening_allowed());
            }
            DialogAction::RevealHostPath(path) => {
                self.send_protocol_frame(ServerFrame::HostRevealPath(path));
            }
            DialogAction::ExportFile {
                path,
                reveal_after_export,
                open_after_export,
            } => {
                self.dialog_clear();
                self.export_file_to_host(path, reveal_after_export, open_after_export);
            }
            DialogAction::RefreshUsage => {
                self.request_usage_refresh_for_provider(None);
            }
            DialogAction::SwitchUsageProvider { provider_label } => {
                let view = self.focused_usage_snapshot_for_provider(Some(&provider_label));
                if let Some(dialog) = self.dialog_top_mut() {
                    *dialog = Dialog::new_usage(view);
                }
                self.request_usage_refresh_for_provider(Some(&provider_label));
            }
            DialogAction::SplitDirection(direction) => {
                // Chain to the agent picker carrying the direction —
                // push it on top of the SplitDirectionPicker so Esc
                // walks the operator one step back instead of
                // closing the whole flow.
                let agents = self.launch_env.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(
                    agents,
                    PickerIntent::Split(direction),
                ));
            }
            DialogAction::PickedCloseTarget(kind) => {
                // Push the ConfirmAction dialog on top of the
                // CloseTargetPicker. Esc walks back to the picker,
                // then back to the Menu — operator can change their
                // mind without destroying anything.
                self.dialog_push(Dialog::new_confirm_action(kind));
            }
            DialogAction::ConfirmedAction(kind) => {
                // Terminal action — clear every dialog under us and
                // fire the matching destructive call.
                self.dialog_clear();
                match confirmed_action_route(kind) {
                    ConfirmedActionRoute::ClosePane => self.close_focused_pane(),
                    ConfirmedActionRoute::CloseTab => self.close_focused_tab(),
                    ConfirmedActionRoute::ExitAllSessions => self.exit_all_sessions(),
                }
            }
        }
        self.invalidate(frame_plan.reason());
    }

    pub(super) fn send_bytes_to_focused_pane(&mut self, bytes: &[u8]) -> bool {
        if self.clear_clipboard_image_notice() {
            self.invalidate(FullRedrawReason::StatusChange);
        }
        let cleared_selection =
            self.clipboard.selection.is_some() || self.clipboard.selection_copied;
        self.clipboard.pending_selection = None;
        if cleared_selection {
            self.clipboard.selection = None;
            self.clipboard.selection_copied = false;
            self.clipboard.selection_copy_feedback_deadline = None;
        }
        let mut snapped = false;
        let mut unblocked = false;
        let mut delivered = false;
        if let Some(focused) = self.active_focused_id()
            && let Some(session) = self.session_supervisor.sessions.get_mut(focused)
        {
            if session.scrollback_offset() != 0 {
                session.scroll_to_live();
                snapped = true;
            }
            unblocked = session.mark_operator_input();
            delivered = session.send_input(bytes);
        }
        if cleared_selection {
            self.invalidate(selection_change_redraw_reason());
        } else if let Some(reason) = pane_data_redraw_reason(snapped, unblocked) {
            self.invalidate(reason);
        }
        delivered
    }

    pub(super) fn paste_text_to_focused_pane(&mut self, text: &[u8]) -> bool {
        let mut paste = Vec::new();
        let bracketed = self
            .active_focused_id()
            .and_then(|focused| self.session_supervisor.sessions.get(focused))
            .is_some_and(crate::session::Session::bracketed_paste);
        if bracketed {
            paste.extend_from_slice(b"\x1b[200~");
        }
        paste.extend_from_slice(text);
        if bracketed {
            paste.extend_from_slice(b"\x1b[201~");
        }
        self.send_bytes_to_focused_pane(&paste)
    }

    /// Open the `jackin-exec` credential picker for a `command`, stashing the
    /// control reply channel so confirm/cancel can answer it later. Built from
    /// the workspace's on-demand bindings (carried on the launch config); the
    /// container only ever sees binding names + sources, never resolved values.
    pub(super) fn begin_exec_picker(
        &mut self,
        command: String,
        args: Vec<String>,
        reply_tx: tokio::sync::oneshot::Sender<crate::attach_protocol::ControlResponse>,
        operation: Option<jackin_telemetry::operation::OperationGuard>,
    ) {
        // Supersede any picker already in flight: deny its deferred reply (so
        // that client gets an answer instead of a closed socket) and drop its
        // now-stale dialog so confirm/cancel can't act on it.
        if let Some(prev) = self.control.pending_exec_reply.take() {
            prev.send(jackin_protocol::control::ServerMsg::ExecDenied {
                reason: "superseded by a newer jackin-exec request".to_owned(),
            });
            if matches!(self.dialog_top(), Some(Dialog::ExecPicker(_))) {
                self.dialog_pop_one();
            }
        }
        let state = crate::exec::ExecPickerState::from_bindings(
            command,
            args,
            &self.launch_env.launch_config.exec_bindings,
        );
        self.control.pending_exec_reply = Some(super::PendingExecReply::new(reply_tx, operation));
        self.dialog_push(Dialog::ExecPicker(state));
        self.invalidate(FullRedrawReason::DialogChange);
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Action dispatcher with one arm per multiplexed `Action` variant — \
              each arm applies its focused state mutation. Extracting arms into \
              sub-dispatchers would require re-borrowing the multiplexer state \
              across fn boundaries and obscure the per-action readability."
    )]
    #[expect(
        clippy::excessive_nesting,
        reason = "Action dispatcher already accepted too_many_lines + too_many_lines \
              allows: per-action arm with nested `match` over sub-actions + \
              optional `dialog_top_mut` + scroll state nested. The nesting is the \
              per-action-arm dispatch protocol."
    )]
    pub(super) fn apply_action(&mut self, action: Action) {
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
                let agents = self.launch_env.available_agents.clone();
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
                    session.send_input(bytes);
                }
            }
            Action::MouseChromeUpdate { row, col, button } => {
                self.update_hover_for_mouse(row, col, button);
                self.update_pointer_shape_for_mouse(row, col, button);
            }
            Action::Wheel { row, col, button } => {
                if self.dialog_open() {
                    // A scrollable read-only dialog (Debug info, GitHub context)
                    // captures the wheel so its body scrolls. Wheel button bits:
                    // bit0 = forward (down / right), bit1 = native horizontal
                    // wheel, bit2 = Shift (terminals that map a horizontal
                    // trackpad swipe onto a shifted vertical wheel).
                    let axes = self
                        .dialog_top()
                        .map(|dialog| {
                            let view = github_context_view_from_state(
                                self.pr_watch.pull_request_context_branch.as_deref(),
                                self.pr_watch.pull_request_context.as_deref(),
                                self.pull_request_context_loading(),
                            );
                            dialog.body_scroll_axes(
                                self.render.term_rows,
                                self.render.term_cols,
                                Some(&view),
                            )
                        })
                        .unwrap_or_default();
                    if let Some(scroll) = self.dialog_top_mut().and_then(|d| d.body_scroll_mut()) {
                        if !crate::tui::scroll_input::apply_sgr_wheel_button(scroll, button, axes) {
                            return;
                        }
                        self.clamp_dialog_top_scroll();
                        self.invalidate(FullRedrawReason::DialogChange);
                    }
                    return;
                }
                if self.forward_mouse_to_focused_pane_with_kind(col, row, button, true) {
                    return;
                }
                let delta = if (button & 1) == 0 { 3 } else { -3 };
                let Some(focused) = self.active_focused_id() else {
                    return;
                };
                let Some(session) = self.session_supervisor.sessions.get_mut(focused) else {
                    return;
                };
                let filled = session.scrollback_filled();
                if pane_wheel_cursor_fallback_reason(
                    session.mouse_enabled(),
                    session.alternate_screen(),
                )
                .is_some()
                    && let Some(buf) = encode_wheel_cursor_fallback(
                        session.mouse_enabled(),
                        session.application_cursor(),
                        button,
                    )
                {
                    session.send_input(&buf);
                    return;
                }
                if filled == 0 {
                    return;
                }
                let moved = session.scroll_by(delta);
                // Every wheel step that moved the offset repaints body and
                // footer together — including the offset→0 return to live
                // (D2).
                if moved {
                    self.invalidate(wheel_scrollback_redraw_reason());
                }
            }
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

    /// Handle a parsed input event from the client terminal. Handlers only
    /// mutate state and record an invalidation; the render loop composes the
    /// next frame when the generation moved.
    /// P5: move focus onto/off the agent-tab bar, redrawing the status bar so
    /// the active-tab underline switches between phosphor-green (focused) and
    /// neutral white (agent content focused).
    pub(super) fn set_tab_bar_focused(&mut self, focused: bool) {
        if self.render.tab_bar_focused != focused {
            self.render.tab_bar_focused = focused;
            self.sync_widget_focus();
            self.invalidate(FullRedrawReason::StatusChange);
        }
    }

    pub(super) fn handle_input(&mut self, event: InputEvent) {
        if matches!(
            &event,
            InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. }
        ) {
            let _counter_result =
                jackin_telemetry::counter(&jackin_telemetry::metric::TERMINAL_INPUT_MOUSE)
                    .add(1, &[]);
        }
        if let Some(action) = mouse_chrome_update_action(&event) {
            self.apply_action(action);
        }
        if let InputEvent::Data(bytes) = event {
            // P5: while the agent-tab bar holds focus, it captures the arrow
            // keys (Left/Right switch tabs; Down/Esc return focus to the agent).
            // Any other key also returns focus to the agent and is forwarded as
            // normal input, so the operator is never trapped in the bar.
            if self.render.tab_bar_focused {
                match tab_bar_focus_key(&bytes) {
                    Some(TabBarFocusKey::Prev) => {
                        self.apply_action(Action::PreviousTab);
                        return;
                    }
                    Some(TabBarFocusKey::Next) => {
                        self.apply_action(Action::NextTab);
                        return;
                    }
                    Some(TabBarFocusKey::Exit) => {
                        self.set_tab_bar_focused(false);
                        return;
                    }
                    None => self.set_tab_bar_focused(false),
                }
            }
            if let Some(action) =
                self.dispatch_to_dialog_top(|dialog, github| dialog.handle_key(&bytes, github))
            {
                self.clamp_dialog_top_scroll();
                self.apply_action(Action::Dialog(action));
            } else {
                // Any keyboard input from the operator returns the
                // focused pane to the live tail. Matches the
                // common multiplexer convention that "I'm typing
                // again" implies "show me what's happening now."
                self.apply_action(Action::PaneData(bytes));
            }
        } else {
            let usage_status_label = self.focused_usage_status_label();
            let branch_context_hit = match &event {
                InputEvent::MousePress {
                    row,
                    col,
                    button: 0,
                } => branch_context_bar_hit(
                    row + 1,
                    col + 1,
                    self.render.term_rows,
                    self.render.term_cols,
                    self.context_bar_branch(),
                    usage_status_label.as_deref(),
                    self.pr_watch.pull_request_context.as_deref(),
                    self.pull_request_context_loading(),
                    debug_run_id_label().as_deref(),
                    self.status.status_bar.instance_id_label(),
                )
                .is_some(),
                _ => false,
            };
            if let Some(action) = input_event_action(
                &event,
                InputDispatchContext {
                    dialog_captures_input: self.dialog_captures_input(),
                    branch_context_hit,
                },
            ) {
                self.apply_action(action);
            }
        }
    }

    pub(super) fn handle_prefix_command(&mut self, cmd: PrefixCommand) {
        if let Some(action) = prefix_command_action(&cmd) {
            self.apply_action(action);
        }
        // The prefix gesture itself invalidates (the status-bar prefix chip
        // changes) even when the command maps to no action.
        self.invalidate(prefix_full_redraw_reason(&cmd));
    }

    pub(super) fn handle_palette_command(&mut self, cmd: PaletteCommand) {
        // Per-arm decision: sub-dialog openings push onto the dialog
        // stack (Menu stays underneath for Esc → back); terminal
        // actions clear the stack and run the action. No blanket
        // clear at the top because that would prevent the sub-dialog
        // back-navigation chain from working.
        let route = palette_command_route(cmd, self.active_tab_pane_count());
        match route {
            PaletteCommandRoute::OpenSplitDirectionPicker => {
                // Open the SplitDirectionPicker sub-dialog. The
                // operator picks the direction; that resolves to a
                // `DialogAction::SplitDirection(...)` which
                // `apply_dialog_action` chains into an `AgentPicker`
                // carrying `PickerIntent::Split(direction)`. Final
                // confirm spawns the new pane.
                self.dialog_push(Dialog::new_split_direction_picker());
            }
            PaletteCommandRoute::OpenAgentPicker(intent) => {
                // Always show the agent picker — even when the role
                // declares a single agent. The operator must
                // explicitly choose between that agent and a Shell;
                // jumping straight into the agent would surprise an
                // operator who picked "New tab" to open a shell.
                let agents = self.launch_env.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(agents, intent));
            }
            PaletteCommandRoute::NextTab => {
                self.dialog_clear();
                self.next_tab();
            }
            PaletteCommandRoute::PreviousTab => {
                self.dialog_clear();
                self.prev_tab();
            }
            PaletteCommandRoute::ConfirmAction(kind) => {
                self.dialog_push(Dialog::new_confirm_action(kind));
            }
            PaletteCommandRoute::OpenCloseTargetPicker => {
                // Drill-down: push the CloseTargetPicker on top
                // of the Menu so split tabs still ask whether
                // the operator wants the focused pane or every
                // pane in the tab. Esc walks back to Menu.
                self.dialog_push(Dialog::new_close_target_picker());
            }
            PaletteCommandRoute::ToggleZoom => {
                self.dialog_clear();
                self.toggle_zoom();
            }
            PaletteCommandRoute::OpenExportFileDialog {
                reveal_after_export,
                open_after_export,
            } => {
                let dialog = if open_after_export {
                    Dialog::new_export_file_and_open()
                } else if reveal_after_export {
                    Dialog::new_export_file_and_reveal()
                } else {
                    Dialog::new_export_file()
                };
                self.dialog_push(dialog);
            }
            PaletteCommandRoute::ExportFileUnderCursor {
                reveal_after_export,
                open_after_export,
            } => {
                self.dialog_clear();
                if !self.export_file_under_cursor_to_host(reveal_after_export, open_after_export) {
                    self.set_clipboard_image_notice(
                        "No exportable file path under focused cursor".to_owned(),
                    );
                }
            }
            PaletteCommandRoute::ExportSelectedFile {
                reveal_after_export,
                open_after_export,
            } => {
                self.dialog_clear();
                if !self.export_selected_file_to_host(reveal_after_export, open_after_export) {
                    self.set_clipboard_image_notice("No selected file path to export".to_owned());
                }
            }
            PaletteCommandRoute::StageImageFromClipboardPath => {
                self.dialog_clear();
                self.set_clipboard_image_notice(
                    "Image stage requested from host clipboard path".to_owned(),
                );
                self.request_clipboard_image_from_text_path();
            }
            PaletteCommandRoute::PasteImageFromClipboard => {
                self.dialog_clear();
                self.set_clipboard_image_notice(
                    "Image paste requested from host clipboard".to_owned(),
                );
                self.request_clipboard_image_paste();
            }
            PaletteCommandRoute::StageImageFromClipboard => {
                self.dialog_clear();
                self.set_clipboard_image_notice(
                    "Image stage requested from host clipboard".to_owned(),
                );
                self.request_clipboard_image_stage_only();
            }
            PaletteCommandRoute::OpenLinkUnderCursor => {
                self.dialog_clear();
                if !super::mouse_input::host_url_opening_allowed() {
                    self.set_clipboard_image_notice(
                        "Host link opening disabled by JACKIN_OPEN_LINKS".to_owned(),
                    );
                } else if !self.open_visible_url_under_cursor() {
                    self.set_clipboard_image_notice(
                        "No host-open link under focused cursor".to_owned(),
                    );
                }
            }
            PaletteCommandRoute::ClearPane => {
                self.dialog_clear();
                self.clear_focused_pane();
            }
            PaletteCommandRoute::OpenUsage => {
                let view = self.focused_usage_snapshot();
                self.dialog_push(Dialog::new_usage(view));
                self.request_usage_refresh_for_provider(None);
            }
        }
        self.invalidate(palette_route_frame_plan(route).reason());
    }
}

/// P5: keys the agent-tab-bar focus mode captures, as raw terminal byte
/// sequences. `Prev`/`Next` are Left/Right (switch tabs); `Exit` is Down or Esc
/// (return focus to the agent). Any other key returns `None` — it ends tab-bar
/// focus and is forwarded to the agent as normal input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TabBarFocusKey {
    Prev,
    Next,
    Exit,
}

pub(super) fn tab_bar_focus_key(bytes: &[u8]) -> Option<TabBarFocusKey> {
    match bytes {
        b"\x1b[D" | b"\x1bOD" => Some(TabBarFocusKey::Prev), // Left
        b"\x1b[C" | b"\x1bOC" => Some(TabBarFocusKey::Next), // Right
        b"\x1b[B" | b"\x1bOB" | b"\x1b" => Some(TabBarFocusKey::Exit), // Down / Esc
        _ => None,
    }
}

/// Resolve the operator-selected credentials through the host socket and run the
/// command with them injected as env vars, returning the framed control reply.
///
/// Fails closed (`ExecDenied`) on any resolver or spawn error — the command is
/// never run with a partially-resolved credential set. The container reaches the
/// host resolver at `/jackin/run/host.sock` (bind-mounted by the launch path).
async fn run_exec_selected(
    command: String,
    args: Vec<String>,
    selected: Vec<jackin_protocol::ExecBinding>,
) -> jackin_protocol::control::ServerMsg {
    use jackin_protocol::control::ServerMsg;

    let resolved =
        match crate::exec::resolve_credentials(jackin_protocol::HOST_SOCK_CONTAINER_PATH, selected)
            .await
        {
            Ok(map) => map,
            Err(error) => {
                return ServerMsg::ExecDenied {
                    reason: format!("credential resolution failed: {error}"),
                };
            }
        };
    // Redaction set borrows the resolved values — no second copy of secret
    // material; the strings already live in `resolved` for the env injection.
    let secrets: Vec<&str> = resolved.values().map(String::as_str).collect();
    match crate::exec::execute_command(&command, &args, &resolved, &secrets).await {
        Ok((exit_code, stdout, stderr, redacted_count)) => ServerMsg::ExecResult {
            exit_code,
            stdout,
            stderr,
            redacted_count,
        },
        Err(error) => ServerMsg::ExecDenied {
            reason: format!("command execution failed: {error}"),
        },
    }
}
