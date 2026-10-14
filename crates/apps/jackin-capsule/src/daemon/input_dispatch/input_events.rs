// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::branch_context_bar::{branch_context_bar_hit, debug_run_id_label};

use crate::tui::update::prefix_full_redraw_reason;

use super::super::{
    Action, Dialog, FullRedrawReason, InputDispatchContext, InputEvent, Multiplexer,
    PaletteCommand, PaletteCommandRoute, PrefixCommand, input_event_action,
    mouse_chrome_update_action, palette_command_route, palette_route_frame_plan,
    prefix_command_action,
};

impl Multiplexer {
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

        self.invalidate(prefix_full_redraw_reason(&cmd));
    }

    pub(crate) fn handle_palette_command(&mut self, cmd: PaletteCommand) {
        let route = palette_command_route(cmd, self.active_tab_pane_count());
        match route {
            PaletteCommandRoute::OpenSplitDirectionPicker => {
                self.dialog_push(Dialog::new_split_direction_picker());
            }
            PaletteCommandRoute::OpenAgentPicker(intent) => {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabBarFocusKey {
    Prev,
    Next,
    Exit,
}

pub(crate) fn tab_bar_focus_key(bytes: &[u8]) -> Option<TabBarFocusKey> {
    match bytes {
        b"\x1b[D" | b"\x1bOD" => Some(TabBarFocusKey::Prev),
        b"\x1b[C" | b"\x1bOC" => Some(TabBarFocusKey::Next),
        b"\x1b[B" | b"\x1bOB" | b"\x1b" => Some(TabBarFocusKey::Exit),
        _ => None,
    }
}
