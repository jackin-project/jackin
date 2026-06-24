//! Input dispatch methods for the Multiplexer.

use crate::tui::components::branch_context_bar::{branch_context_bar_hit, debug_run_id_label};
use crate::tui::input::TAB_DOUBLE_CLICK_WINDOW;
use crate::tui::update::DIALOG_COPY_FEEDBACK_DURATION;
use crate::tui::update::action_frame_plan;
use crate::tui::update::prefix_full_redraw_reason;
use crate::tui::view::encode_osc52_clipboard_write;

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

    /// Single dispatch point for a `DialogAction`. Both the
    /// mouse-click and key-event paths call `Dialog::handle_*`
    /// and route the result here, so adding a new variant means
    /// updating one match arm instead of two.
    pub(super) fn apply_dialog_action(&mut self, action: DialogAction) {
        // Compact breadcrumb (always logged) for the load-bearing
        // dispatch arms — Dismiss, Command, SpawnAgent, RenameTab. The
        // Redraw / Consume arms fire on every arrow key inside a dialog
        // and would swamp the production log; they go through the
        // debug-only `cdebug!` surface so a `--debug` trace shows
        // dialog dispatch landing for arrow keys while quiet runs stay
        // tidy.
        match &action {
            DialogAction::Redraw | DialogAction::Consume => {
                crate::cdebug!("action: dialog={action:?}");
            }
            _ => crate::clog!("action: dialog={action:?}"),
        }
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
                let env_overrides =
                    jackin_protocol::Provider::from_label(&provider_label).map_or_else(
                        || {
                            crate::clog!(
                                "spawn: unknown provider label {provider_label:?}; no env redirect applied"
                            );
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
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
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
                // answered by the green "✓ Copied!" badge the renderer
                // paints now that `copied = true` (flipped by the
                // dialog's handle_key or row-click handler before this
                // action returned).
                // The badge expires from the daemon's tick loop.
                self.send_out_of_band(encode_osc52_clipboard_write(&payload));
                self.dialog_copy_feedback_deadline =
                    Some(Instant::now() + DIALOG_COPY_FEEDBACK_DURATION);
            }
            DialogAction::RefreshUsage => {
                self.request_usage_refresh_for_provider(None);
            }
            DialogAction::SwitchUsageProvider { provider_label } => {
                let view = self.focused_usage_snapshot_for_provider(Some(&provider_label), false);
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
                let agents = self.available_agents.clone();
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
                let view = self.focused_usage_snapshot(false);
                self.dialog_push(Dialog::new_usage(view));
                self.request_usage_refresh_for_provider(None);
                self.invalidate_for(&Action::OpenUsage);
            }
            Action::OpenRenameTab(idx) => {
                if idx >= self.tabs.len() {
                    return;
                }
                self.cancel_drag();
                let initial = self.tabs[idx]
                    .custom_label()
                    .map(str::to_owned)
                    .unwrap_or_default();
                self.dialog_push(Dialog::new_rename_tab(idx, initial));
                self.last_tab_click = None;
                self.invalidate_for(&Action::OpenRenameTab(idx));
            }
            Action::OpenAgentPicker(intent) => {
                let agents = self.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(agents, intent));
                self.invalidate_for(&Action::OpenAgentPicker(intent));
            }
            Action::SwitchTab(idx) => {
                if idx >= self.tabs.len() || idx == self.active_tab {
                    return;
                }
                self.cancel_drag();
                let prev = self.active_focused_id();
                self.active_tab = idx;
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
                if let Err(err) = self.split_focused(direction) {
                    crate::clog!("split ({direction:?}) failed: {err:?}");
                }
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
                self.detach_requested = true;
                self.invalidate_for(&Action::Detach);
            }
            Action::RefreshUsage => {
                let view = self.focused_usage_snapshot(true);
                if let Some(dialog) = self.dialog_top_mut() {
                    *dialog = Dialog::new_usage(view);
                }
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
                    && let Some(session) = self.sessions.get(&focused)
                    && session.focus_events_enabled()
                {
                    session.send_input(bytes);
                }
            }
            Action::MouseChromeUpdate { row, col, button } => {
                self.update_hover_for_mouse(row, col);
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
                                self.pull_request_context_branch.as_deref(),
                                self.pull_request_context.as_deref(),
                                self.pull_request_context_loading(),
                            );
                            dialog.body_scroll_axes(self.term_rows, self.term_cols, Some(&view))
                        })
                        .unwrap_or_default();
                    if let Some(scroll) = self.dialog_top_mut().and_then(|d| d.body_scroll_mut()) {
                        if !scroll.on_sgr_wheel_button_for_axes(button, axes) {
                            return;
                        }
                        self.clamp_dialog_top_scroll();
                        self.invalidate(FullRedrawReason::DialogChange);
                    }
                    return;
                }
                if self.forward_mouse_to_focused_pane_with_kind(col, row, button, true) {
                    crate::cdebug!(
                        "wheel dispatch: forwarded-to-pty row={} col={} button={}",
                        row,
                        col,
                        button
                    );
                    return;
                }
                let delta = if (button & 1) == 0 { 3 } else { -3 };
                let Some(focused) = self.active_focused_id() else {
                    return;
                };
                let Some(session) = self.sessions.get_mut(&focused) else {
                    return;
                };
                let debug_enabled = crate::logging::debug_enabled();
                let (filled, vt_filled, inline_filled) = if debug_enabled {
                    let (vt_filled, inline_filled) = session.scrollback_counts();
                    (
                        vt_filled.saturating_add(inline_filled),
                        vt_filled,
                        inline_filled,
                    )
                } else {
                    (session.scrollback_filled(), 0, 0)
                };
                if let Some(fallback_reason) = pane_wheel_cursor_fallback_reason(
                    session.mouse_enabled(),
                    session.alternate_screen(),
                ) && let Some(buf) = encode_wheel_cursor_fallback(
                    session.mouse_enabled(),
                    session.application_cursor(),
                    button,
                ) {
                    crate::cdebug!(
                        "wheel dispatch: cursor-fallback session={} agent={:?} row={} col={} button={} scrollback_filled={} reason={} bytes={:02x?}",
                        focused,
                        session.agent,
                        row,
                        col,
                        button,
                        filled,
                        fallback_reason,
                        buf
                    );
                    session.send_input(&buf);
                    return;
                }
                if filled == 0 {
                    crate::cdebug!(
                        "wheel dispatch: no-scrollback session={} agent={:?} row={} col={} button={} alt_screen={} mouse_enabled={} vt_scrollback={} inline_scrollback={}",
                        focused,
                        session.agent,
                        row,
                        col,
                        button,
                        session.alternate_screen(),
                        session.mouse_enabled(),
                        vt_filled,
                        inline_filled
                    );
                    return;
                }
                crate::cdebug!(
                    "wheel dispatch: jackin-scrollback session={} row={} col={} button={} delta={} before={} filled={}",
                    focused,
                    row,
                    col,
                    button,
                    delta,
                    session.scrollback_offset(),
                    filled
                );
                let moved = session.scroll_by(delta);
                crate::cdebug!(
                    "wheel dispatch: jackin-scrollback session={} after={} moved={}",
                    focused,
                    session.scrollback_offset(),
                    moved
                );
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
            Action::PanePrimaryPress { row, col } => {
                if self.selection.is_some() || self.selection_copied {
                    self.selection = None;
                    self.selection_copied = false;
                    self.selection_copy_feedback_deadline = None;
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
                    self.pending_selection = Some(selection);
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
                if self.pending_selection.is_some() && self.selection.is_none() {
                    self.pending_selection_motion(row, col);
                    return;
                }
                let action = pane_button_motion_action(
                    self.drag.is_some(),
                    self.selection.is_some(),
                    row,
                    col,
                );
                self.apply_action(action);
            }
            Action::StatusBarClick { col } => {
                let tab = self.status_bar.tab_at_col(col + 1);
                let now = Instant::now();
                let double_click = tab
                    .and_then(|idx| {
                        self.last_tab_click.filter(|(prev_idx, prev_t)| {
                            *prev_idx == idx
                                && now.duration_since(*prev_t) <= TAB_DOUBLE_CLICK_WINDOW
                        })
                    })
                    .is_some();
                let Some(action) = status_bar_click_action(StatusBarClickState {
                    tab,
                    tab_count: self.tabs.len(),
                    double_click,
                    menu_hit: self.status_bar.hint_at(1, col + 1),
                }) else {
                    return;
                };
                if matches!(action, Action::SwitchTab(_)) {
                    self.last_tab_click = tab.map(|idx| (idx, now));
                }
                self.apply_action(action);
            }
            Action::BranchContextBarClick { row, col } => {
                let usage_status_label = self.focused_usage_snapshot(false).status_bar_label;
                let hit = branch_context_bar_hit(
                    row + 1,
                    col + 1,
                    self.term_rows,
                    self.term_cols,
                    self.context_bar_branch(),
                    Some(&usage_status_label),
                    self.pull_request_context.as_deref(),
                    self.pull_request_context_loading(),
                    debug_run_id_label().as_deref(),
                    self.status_bar.instance_id_label(),
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
                if self.pending_selection.is_some() && self.selection.is_none() {
                    self.pending_selection = None;
                    return;
                }
                let action = mouse_release_action(
                    self.drag.is_some(),
                    self.selection.is_some(),
                    row,
                    col,
                    button,
                );
                self.apply_action(action);
            }
            Action::PaneData(bytes) => {
                // Any operator keystroke dismisses the spawn-failure banner.
                if self.spawn_failure.take().is_some() {
                    self.invalidate(FullRedrawReason::StatusChange);
                }
                let cleared_selection = self.selection.is_some() || self.selection_copied;
                self.pending_selection = None;
                if cleared_selection {
                    self.selection = None;
                    self.selection_copied = false;
                    self.selection_copy_feedback_deadline = None;
                }
                let mut snapped = false;
                let mut unblocked = false;
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.sessions.get_mut(&focused)
                {
                    if session.scrollback_offset() != 0 {
                        session.scroll_to_live();
                        snapped = true;
                    }
                    unblocked = session.mark_operator_input();
                    session.send_input(&bytes);
                }
                if cleared_selection {
                    self.invalidate(selection_change_redraw_reason());
                } else if let Some(reason) = pane_data_redraw_reason(snapped, unblocked) {
                    self.invalidate(reason);
                }
            }
            Action::StartDragResize { row, col } => {
                self.drag = self.detect_drag_start(row, col);
            }
            Action::DragMotion { row, col } => self.drag_motion(row, col),
            Action::EndDragResize => {
                self.drag = None;
                self.invalidate(drag_resize_redraw_reason());
            }
            Action::StartSelection { row, col } => {
                self.pending_selection = None;
                self.selection_copied = false;
                self.selection_copy_feedback_deadline = None;
                self.selection = self.detect_selection_start(row, col);
                if let Some(reason) = selection_start_redraw_reason(self.selection.is_some()) {
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
                let term_rows = self.term_rows;
                let term_cols = self.term_cols;
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
    pub(super) fn handle_input(&mut self, event: InputEvent) {
        if let Some(action) = mouse_chrome_update_action(&event) {
            self.apply_action(action);
        }
        if let InputEvent::Data(bytes) = event {
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
                    self.term_rows,
                    self.term_cols,
                    self.context_bar_branch(),
                    usage_status_label.as_deref(),
                    self.pull_request_context.as_deref(),
                    self.pull_request_context_loading(),
                    debug_run_id_label().as_deref(),
                    self.status_bar.instance_id_label(),
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
        // Action breadcrumb: every prefix-key chord lands here, so one
        // line per dispatch is enough to reconstruct what the operator
        // pressed when triaging a bug report. The Debug formatter
        // includes any payload (`JumpTab(i)`, `MoveFocus(dir)`).
        crate::clog!("action: prefix={cmd:?}");
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
                let agents = self.available_agents.clone();
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
            PaletteCommandRoute::ClearPane => {
                self.dialog_clear();
                self.clear_focused_pane();
            }
            PaletteCommandRoute::OpenUsage => {
                let view = self.focused_usage_snapshot(false);
                self.dialog_push(Dialog::new_usage(view));
                self.request_usage_refresh_for_provider(None);
            }
        }
        self.invalidate(palette_route_frame_plan(route).reason());
    }
}
