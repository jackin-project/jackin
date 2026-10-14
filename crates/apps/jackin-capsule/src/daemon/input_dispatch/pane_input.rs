// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    Dialog, FullRedrawReason, Multiplexer, encode_wheel_cursor_fallback,
    github_context_view_from_state, pane_data_redraw_reason, pane_wheel_cursor_fallback_reason,
    selection_change_redraw_reason, wheel_scrollback_redraw_reason,
};

impl Multiplexer {
    pub(crate) fn send_bytes_to_focused_pane(&mut self, bytes: &[u8]) -> bool {
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

    pub(crate) fn paste_text_to_focused_pane(&mut self, text: &[u8]) -> bool {
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

    pub(crate) fn begin_exec_picker(
        &mut self,
        command: String,
        args: Vec<String>,
        reply_tx: tokio::sync::oneshot::Sender<crate::attach_protocol::ControlResponse>,
        operation: Option<jackin_telemetry::operation::OperationGuard>,
    ) {
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
        self.control.pending_exec_reply =
            Some(super::super::PendingExecReply::new(reply_tx, operation));
        self.dialog_push(Dialog::ExecPicker(state));
        self.invalidate(FullRedrawReason::DialogChange);
    }

    pub(crate) fn apply_wheel_action(&mut self, row: u16, col: u16, button: u8) {
        if self.dialog_open() {
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
        if pane_wheel_cursor_fallback_reason(session.mouse_enabled(), session.alternate_screen())
            .is_some()
            && let Some(buf) = encode_wheel_cursor_fallback(
                session.mouse_enabled(),
                session.application_cursor(),
                button,
            )
        {
            let _sent = session.send_input(&buf);
            return;
        }
        if filled == 0 {
            return;
        }
        let moved = session.scroll_by(delta);
        if moved {
            self.invalidate(wheel_scrollback_redraw_reason());
        }
    }
}
