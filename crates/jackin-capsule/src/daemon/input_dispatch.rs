//! Input dispatch methods for the Multiplexer.

use crate::tui::components::branch_context_bar::{
    BranchContextBarHit, branch_context_bar_hit,
};
use crate::tui::input::TAB_DOUBLE_CLICK_WINDOW;
use crate::tui::update::DIALOG_COPY_FEEDBACK_DURATION;
use crate::tui::update::prefix_full_redraw_reason;
use crate::tui::view::encode_osc52_clipboard_write;

use super::*;

impl Multiplexer {
    /// Single dispatch point for a `DialogAction`. Both the
    /// mouse-click and key-event paths call `Dialog::handle_*`
    /// and route the result here, so adding a new variant means
    /// updating one match arm instead of two.
    pub(super) fn apply_dialog_action(&mut self, action: DialogAction) -> Vec<u8> {
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
                // the flow (clear stack).
                if let Some(frame) = self.apply_action(Action::Palette(cmd)) {
                    return frame;
                }
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
                } else {
                    // Zero or one provider — spawn immediately without
                    // a picker step (operator experience unchanged when
                    // Z.AI is not configured).
                    self.dialog_clear();
                    self.dispatch_spawn_intent(agent, intent);
                }
            }
            DialogAction::SpawnAgentWithProvider {
                agent,
                provider_label,
                intent,
            } => {
                self.dialog_clear();
                // Token resolved here from the container's ZAI_API_KEY.
                let env_overrides =
                    jackin_protocol::Provider::from_label(&provider_label).map_or_else(
                        || {
                            crate::clog!(
                                "spawn: unknown provider label {provider_label:?}; no env redirect applied"
                            );
                            Vec::new()
                        },
                        |provider| provider.env_overrides(self.zai_key.as_deref()),
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
                self.send_output(encode_osc52_clipboard_write(&payload));
                self.dialog_copy_feedback_deadline =
                    Some(Instant::now() + DIALOG_COPY_FEEDBACK_DURATION);
                return self.compose_dialog_overlay_frame(FullRedrawReason::DialogChange);
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
                match kind {
                    ConfirmKind::ClosePane => self.close_focused_pane(),
                    ConfirmKind::CloseTab => self.close_focused_tab(),
                    ConfirmKind::Exit => self.exit_all_sessions(),
                }
            }
        }
        self.compose_full_frame(FullRedrawReason::DialogChange)
    }

    pub(super) fn apply_action(&mut self, action: Action) -> Option<Vec<u8>> {
        match action {
            Action::OpenPalette => {
                self.cancel_drag();
                if self.dialog_open() {
                    self.dialog_clear();
                } else {
                    self.open_command_palette();
                }
                Some(self.compose_full_frame(FullRedrawReason::PaletteOverlay))
            }
            Action::OpenContainerInfo => {
                self.open_container_info_dialog();
                Some(self.compose_dialog_overlay_frame(FullRedrawReason::DialogChange))
            }
            Action::OpenGithubContext => {
                self.open_github_context_dialog(Instant::now());
                Some(self.compose_dialog_overlay_frame(FullRedrawReason::DialogChange))
            }
            Action::OpenRenameTab(idx) => {
                if idx >= self.tabs.len() {
                    return None;
                }
                self.cancel_drag();
                let initial = self.tabs[idx]
                    .custom_label()
                    .map(str::to_owned)
                    .unwrap_or_default();
                self.dialog_push(Dialog::new_rename_tab(idx, initial));
                self.last_tab_click = None;
                Some(self.compose_full_frame(FullRedrawReason::DialogChange))
            }
            Action::OpenAgentPicker(intent) => {
                let agents = self.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(agents, intent));
                Some(self.compose_full_frame(FullRedrawReason::PaletteOverlay))
            }
            Action::SwitchTab(idx) => {
                if idx >= self.tabs.len() || idx == self.active_tab {
                    return None;
                }
                self.cancel_drag();
                let prev = self.active_focused_id();
                self.active_tab = idx;
                self.synthesise_focus_swap(prev, self.active_focused_id());
                Some(self.compose_full_frame(FullRedrawReason::TabSwitch))
            }
            Action::NextTab => {
                self.next_tab();
                Some(self.compose_full_frame(FullRedrawReason::TabSwitch))
            }
            Action::PreviousTab => {
                self.prev_tab();
                Some(self.compose_full_frame(FullRedrawReason::TabSwitch))
            }
            Action::JumpTab(idx) => {
                self.jump_tab(idx);
                Some(self.compose_full_frame(FullRedrawReason::TabSwitch))
            }
            Action::SplitFocused(direction) => {
                if let Err(err) = self.split_focused(direction) {
                    crate::clog!("split ({direction:?}) failed: {err:?}");
                }
                Some(self.compose_full_frame(FullRedrawReason::LayoutChange))
            }
            Action::MoveFocus(dir) => {
                self.move_focus(dir);
                Some(self.compose_full_frame(FullRedrawReason::FocusChange))
            }
            Action::ToggleZoom => {
                self.toggle_zoom();
                Some(self.compose_full_frame(FullRedrawReason::ZoomChange))
            }
            Action::CloseFocusedPane => {
                self.close_focused_pane();
                Some(self.compose_full_frame(FullRedrawReason::SplitClose))
            }
            Action::CloseFocusedTab => {
                self.close_focused_tab();
                Some(self.compose_full_frame(FullRedrawReason::SplitClose))
            }
            Action::ClearFocusedPane => {
                self.clear_focused_pane();
                Some(self.compose_full_frame(FullRedrawReason::PaneClear))
            }
            Action::Detach => {
                self.detach_requested = true;
                Some(self.compose_full_frame(FullRedrawReason::ExplicitRedraw))
            }
            Action::Palette(cmd) => self.handle_palette_command(cmd),
            Action::Prefix(cmd) => {
                if self.dialog_captures_input() {
                    None
                } else {
                    self.handle_prefix_command(cmd)
                }
            }
            Action::ResizePane(dir) => {
                if self.dialog_captures_input() {
                    None
                } else {
                    self.resize_focused(dir);
                    Some(self.compose_full_frame(FullRedrawReason::LayoutChange))
                }
            }
            Action::FocusReport(focused) => {
                if self.dialog_captures_input() {
                    return None;
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
                None
            }
            Action::MouseChromeUpdate { row, col, button } => {
                if let Some(frame) = self.update_hover_for_mouse(row, col) {
                    self.send_output(frame);
                }
                self.update_pointer_shape_for_mouse(row, col, button);
                None
            }
            Action::Wheel { row, col, button } => {
                if self.dialog_open() {
                    return None;
                }
                if self.forward_mouse_to_focused_pane_with_kind(col, row, button, true) {
                    crate::cdebug!(
                        "wheel dispatch: forwarded-to-pty row={} col={} button={}",
                        row,
                        col,
                        button
                    );
                    return None;
                }
                let delta = if (button & 1) == 0 { 3 } else { -3 };
                let focused = self.active_focused_id()?;
                let session = self.sessions.get_mut(&focused)?;
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
                    session.screen().alternate_screen(),
                )
                    && let Some(buf) = encode_wheel_cursor_fallback(
                        session.mouse_enabled(),
                        session.screen().application_cursor(),
                        button,
                    )
                {
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
                    return None;
                }
                if filled == 0 {
                    crate::cdebug!(
                        "wheel dispatch: no-scrollback session={} agent={:?} row={} col={} button={} alt_screen={} mouse_enabled={} vt_scrollback={} inline_scrollback={}",
                        focused,
                        session.agent,
                        row,
                        col,
                        button,
                        session.screen().alternate_screen(),
                        session.mouse_enabled(),
                        vt_filled,
                        inline_filled
                    );
                    return None;
                }
                crate::cdebug!(
                    "wheel dispatch: jackin-scrollback session={} row={} col={} button={} delta={} before={} filled={}",
                    focused,
                    row,
                    col,
                    button,
                    delta,
                    session.scrollback_offset,
                    filled
                );
                session.scroll_by(delta);
                crate::cdebug!(
                    "wheel dispatch: jackin-scrollback session={} after={}",
                    focused,
                    session.scrollback_offset
                );
                Some(self.compose_full_frame(FullRedrawReason::ScrollbackMovement))
            }
            Action::FocusPaneAt { row, col } => self
                .focus_pane_at(row, col)
                .then(|| self.compose_full_frame(FullRedrawReason::FocusChange)),
            Action::PanePrimaryPress { row, col } => {
                // Press on a shared pane border starts a drag — skip focus
                // switch and PTY forward in that case.
                if self.detect_drag_start(row, col).is_some() {
                    self.apply_action(Action::StartDragResize { row, col });
                    return None;
                }
                // Click on a pane other than the currently-focused one switches
                // focus first so the operator never has to click twice. Selection
                // or PTY-mouse forwarding then runs against the freshly-focused
                // pane.
                let focus_frame = self.apply_action(Action::FocusPaneAt { row, col });
                // Press inside a pane whose program never asked for a mouse
                // protocol starts a text selection.
                if self.detect_selection_start(row, col).is_some() {
                    return self.apply_action(Action::StartSelection { row, col });
                }
                self.apply_action(Action::ForwardMouse {
                    row,
                    col,
                    button: 0,
                    press: true,
                });
                focus_frame
            }
            Action::PaneButtonMotion { row, col } => {
                if self.drag.is_some() {
                    return self.apply_action(Action::DragMotion { row, col });
                }
                if self.selection.is_some() {
                    return self.apply_action(Action::SelectionMotion { row, col });
                }
                // No drag / selection in flight: motion events belong to the
                // focused pane only if it asked for any-event tracking
                // (`?1003h`) or button-motion tracking (`?1002h`). Forwarding
                // them blindly would dump SGR bytes into shells that ignored
                // mouse mode.
                self.apply_action(Action::ForwardMouse {
                    row,
                    col,
                    button: 32,
                    press: true,
                })
            }
            Action::StatusBarClick { col } => {
                // 1) Click on a tab cell switches active tab. A
                //    second click on the same cell within the
                //    double-click window opens the rename modal.
                if let Some(idx) = self.status_bar.tab_at_col(col + 1)
                    && idx < self.tabs.len()
                {
                    let now = std::time::Instant::now();
                    let is_double = self
                        .last_tab_click
                        .filter(|(prev_idx, prev_t)| {
                            *prev_idx == idx
                                && now.duration_since(*prev_t) <= TAB_DOUBLE_CLICK_WINDOW
                        })
                        .is_some();
                    if is_double {
                        return self.apply_action(Action::OpenRenameTab(idx));
                    }
                    self.last_tab_click = Some((idx, now));
                    return self.apply_action(Action::SwitchTab(idx));
                }
                if self.status_bar.hint_at(1, col + 1) {
                    return self.apply_action(Action::OpenPalette);
                }
                None
            }
            Action::BranchContextBarClick { row, col } => {
                let action = match branch_context_bar_hit(
                    row + 1,
                    col + 1,
                    self.term_rows,
                    self.term_cols,
                    self.context_bar_branch(),
                    self.pull_request_context.as_deref(),
                    self.pull_request_context_loading(),
                    self.status_bar.instance_id_label(),
                ) {
                    Some(BranchContextBarHit::Context) => Action::OpenGithubContext,
                    Some(BranchContextBarHit::Container) => Action::OpenContainerInfo,
                    None => return None,
                };
                self.apply_action(action)
            }
            Action::ForwardMouse {
                row,
                col,
                button,
                press,
            } => {
                self.forward_mouse_to_focused_pane_with_kind(col, row, button, press);
                None
            }
            Action::MouseRelease { row, col, button } => {
                // End an in-flight pane resize on left-button release. Drop the
                // PTY forward so the source agent does not see a half-paired
                // release in the middle of a drag.
                if self.drag.is_some() && (button & 0b11) == 0 {
                    return self.apply_action(Action::EndDragResize);
                }
                // Commit any active text selection: copy to clipboard and clear
                // the highlight.
                if self.selection.is_some() && (button & 0b11) == 0 {
                    return self.apply_action(Action::FinalizeSelection);
                }
                self.apply_action(Action::ForwardMouse {
                    row,
                    col,
                    button,
                    press: false,
                })
            }
            Action::PaneData(bytes) => {
                let mut snapped = false;
                let mut unblocked = false;
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.sessions.get_mut(&focused)
                {
                    if session.scrollback_offset != 0 {
                        session.scroll_to_live();
                        snapped = true;
                    }
                    unblocked = session.mark_operator_input();
                    session.send_input(&bytes);
                }
                if snapped || unblocked {
                    let reason = if snapped {
                        FullRedrawReason::ScrollbackMovement
                    } else {
                        FullRedrawReason::ExplicitRedraw
                    };
                    Some(self.compose_full_frame(reason))
                } else {
                    None
                }
            }
            Action::StartDragResize { row, col } => {
                self.drag = self.detect_drag_start(row, col);
                None
            }
            Action::DragMotion { row, col } => self.drag_motion(row, col),
            Action::EndDragResize => {
                self.drag = None;
                Some(self.compose_full_frame(FullRedrawReason::LayoutChange))
            }
            Action::StartSelection { row, col } => {
                self.selection = self.detect_selection_start(row, col);
                self.selection
                    .is_some()
                    .then(|| self.compose_full_frame(FullRedrawReason::SelectionRepaint))
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
                // so `handle_click` compares apples to apples.
                let term_rows = self.term_rows;
                let term_cols = self.term_cols;
                let action = self
                    .dispatch_to_dialog_top(|dialog, github| {
                        dialog.handle_click(row + 1, col + 1, term_rows, term_cols, github)
                    })
                    .expect("dialog presence checked");
                self.apply_action(Action::Dialog(action))
            }
            Action::Dialog(action) => Some(self.apply_dialog_action(action)),
        }
    }

    /// Handle a parsed input event from the client terminal.
    /// Returns bytes to send to the client (e.g. redraws), if any.
    pub(super) fn handle_input(&mut self, event: InputEvent) -> Option<Vec<u8>> {
        if let Some(action) = mouse_chrome_update_action(&event) {
            self.apply_action(action);
        }
        if let InputEvent::Data(bytes) = event {
                if let Some(action) =
                    self.dispatch_to_dialog_top(|dialog, github| dialog.handle_key(&bytes, github))
                {
                    self.apply_action(Action::Dialog(action))
                } else {
                    // Any keyboard input from the operator returns the
                    // focused pane to the live tail. Matches the
                    // common multiplexer convention that "I'm typing
                    // again" implies "show me what's happening now."
                    self.apply_action(Action::PaneData(bytes))
                }
        } else {
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
                    self.pull_request_context.as_deref(),
                    self.pull_request_context_loading(),
                    self.status_bar.instance_id_label(),
                )
                .is_some(),
                _ => false,
            };
            input_event_action(
                &event,
                InputDispatchContext {
                    dialog_captures_input: self.dialog_captures_input(),
                    branch_context_hit,
                },
            )
            .and_then(|action| self.apply_action(action))
        }
    }

    pub(super) fn handle_prefix_command(&mut self, cmd: PrefixCommand) -> Option<Vec<u8>> {
        // Action breadcrumb: every prefix-key chord lands here, so one
        // line per dispatch is enough to reconstruct what the operator
        // pressed when triaging a bug report. The Debug formatter
        // includes any payload (`JumpTab(i)`, `MoveFocus(dir)`).
        crate::clog!("action: prefix={cmd:?}");
        let full_redraw_reason = prefix_full_redraw_reason(&cmd);
        if let Some(action) = prefix_command_action(&cmd)
            && let Some(frame) = self.apply_action(action)
        {
            return Some(frame);
        }
        Some(self.compose_full_frame(full_redraw_reason))
    }

    pub(super) fn handle_palette_command(&mut self, cmd: PaletteCommand) -> Option<Vec<u8>> {
        // Per-arm decision: sub-dialog openings push onto the dialog
        // stack (Menu stays underneath for Esc → back); terminal
        // actions clear the stack and run the action. No blanket
        // clear at the top because that would prevent the sub-dialog
        // back-navigation chain from working.
        match cmd {
            PaletteCommand::Split => {
                // Open the SplitDirectionPicker sub-dialog. The
                // operator picks the direction; that resolves to a
                // `DialogAction::SplitDirection(...)` which
                // `apply_dialog_action` chains into an `AgentPicker`
                // carrying `PickerIntent::Split(direction)`. Final
                // confirm spawns the new pane.
                self.dialog_push(Dialog::new_split_direction_picker());
            }
            PaletteCommand::NewTab => {
                // Always show the agent picker — even when the role
                // declares a single agent. The operator must
                // explicitly choose between that agent and a Shell;
                // jumping straight into the agent would surprise an
                // operator who picked "New tab" to open a shell.
                let agents = self.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(agents, PickerIntent::NewTab));
            }
            PaletteCommand::NextTab => {
                self.dialog_clear();
                self.next_tab();
            }
            PaletteCommand::PrevTab => {
                self.dialog_clear();
                self.prev_tab();
            }
            PaletteCommand::Close => {
                if self.active_tab_pane_count() == 1 {
                    self.dialog_push(Dialog::new_confirm_action(ConfirmKind::CloseTab));
                } else {
                    // Drill-down: push the CloseTargetPicker on top
                    // of the Menu so split tabs still ask whether
                    // the operator wants the focused pane or every
                    // pane in the tab. Esc walks back to Menu.
                    self.dialog_push(Dialog::new_close_target_picker());
                }
            }
            PaletteCommand::ZoomPane => {
                self.dialog_clear();
                self.toggle_zoom();
            }
            PaletteCommand::ClearPane => {
                self.dialog_clear();
                self.clear_focused_pane();
                return Some(self.compose_full_frame(FullRedrawReason::PaneClear));
            }
            PaletteCommand::Exit => {
                // Push ConfirmAction for Exit — the operator
                // confirms before every agent session is stopped. Esc
                // walks back to Menu.
                self.dialog_push(Dialog::new_confirm_action(ConfirmKind::Exit));
            }
        }
        None
    }
}
