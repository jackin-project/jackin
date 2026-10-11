// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Input-event routing, prefix/palette commands, and tab-bar focus keys.

use crate::tui::components::branch_context_bar::{branch_context_bar_hit, debug_run_id_label};

use crate::tui::update::prefix_full_redraw_reason;

use super::super::{
    Action, Dialog, FullRedrawReason, InputDispatchContext, InputEvent, Multiplexer,
    PaletteCommand, PaletteCommandRoute, PrefixCommand, input_event_action,
    mouse_chrome_update_action, palette_command_route, palette_route_frame_plan,
    prefix_command_action,
};

impl Multiplexer {
    /// Handle a parsed input event from the client terminal. Handlers only
    /// mutate state and record an invalidation; the render loop composes the
    /// next frame when the generation moved.
    /// P5: move focus onto/off the agent-tab bar, redrawing the status bar so
    /// the active-tab underline switches between phosphor-green (focused) and
    /// neutral white (agent content focused).
    pub(crate) fn set_tab_bar_focused(&mut self, focused: bool) {
        if self.render.tab_bar_focused != focused {
            self.render.tab_bar_focused = focused;
            self.sync_widget_focus();
            self.invalidate(FullRedrawReason::StatusChange);
        }
    }

    pub(crate) fn handle_input(&mut self, event: InputEvent) {
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

    pub(crate) fn handle_prefix_command(&mut self, cmd: PrefixCommand) {
        if let Some(action) = prefix_command_action(&cmd) {
            self.apply_action(action);
        }
        // The prefix gesture itself invalidates (the status-bar prefix chip
        // changes) even when the command maps to no action.
        self.invalidate(prefix_full_redraw_reason(&cmd));
    }

    pub(crate) fn handle_palette_command(&mut self, cmd: PaletteCommand) {
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
                let agents = self.launch_env.available_instances.clone();
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
                if !super::super::mouse_input::host_url_opening_allowed() {
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
pub(crate) enum TabBarFocusKey {
    Prev,
    Next,
    Exit,
}

pub(crate) fn tab_bar_focus_key(bytes: &[u8]) -> Option<TabBarFocusKey> {
    match bytes {
        b"\x1b[D" | b"\x1bOD" => Some(TabBarFocusKey::Prev), // Left
        b"\x1b[C" | b"\x1bOC" => Some(TabBarFocusKey::Next), // Right
        b"\x1b[B" | b"\x1bOB" | b"\x1b" => Some(TabBarFocusKey::Exit), // Down / Esc
        _ => None,
    }
}
