// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    Dialog, GithubContextView, Instant, Multiplexer, MuxMode, MuxModeState,
    github_context_view_from_state, mux_mode_for_state,
};

impl Multiplexer {
    const PANE_WIDGET: &'static str = "capsule.pane";
    const TAB_BAR_WIDGET: &'static str = "capsule.tab";
    const PALETTE_WIDGET: &'static str = "capsule.command_palette";

    pub(super) fn sync_widget_focus(&mut self) {
        let target = if matches!(self.dialog_top(), Some(Dialog::CommandPalette { .. })) {
            Self::PALETTE_WIDGET
        } else if self.dialog_open() {
            Self::PANE_WIDGET
        } else if self.render.tab_bar_focused {
            Self::TAB_BAR_WIDGET
        } else {
            Self::PANE_WIDGET
        };
        if self.widget_focus.current_widget() != Some(target) {
            let _focus_result = self.widget_focus.focus(target);
        }
    }

    pub(super) fn record_pane_focus_change(&mut self) {
        if self.widget_focus.current_widget() == Some(Self::PANE_WIDGET) {
            let _focus_result = self.widget_focus.focus(Self::PANE_WIDGET);
        }
    }

    pub(super) fn dialog_top(&self) -> Option<&Dialog> {
        self.control.dialog_top()
    }

    pub(super) fn dialog_top_mut(&mut self) -> Option<&mut Dialog> {
        self.control.dialog_top_mut()
    }

    pub(super) fn dialog_open(&self) -> bool {
        self.control.dialog_open()
    }

    pub(super) fn mux_mode(&self) -> MuxMode {
        mux_mode_for_state(MuxModeState {
            dialog_open: self.dialog_open(),
            dragging: self.render.drag.is_some(),
            selecting: self.clipboard.is_selecting(),
            awaiting_prefix: self.control.input_parser.is_awaiting_prefix(),
        })
    }

    pub(super) fn dialog_captures_input(&self) -> bool {
        matches!(self.mux_mode(), MuxMode::Dialog)
    }

    pub(super) fn dialog_push(&mut self, d: Dialog) {
        self.clipboard.dialog_copy_feedback_deadline = None;
        self.control.push_dialog(d);
        self.sync_widget_focus();
    }

    pub(super) fn open_container_info_dialog(&mut self) {
        let focused_agent = self
            .active_focused_id()
            .and_then(|id| self.session_supervisor.sessions.get(id))
            .and_then(|s| s.agent.clone());
        let (container_name, role) = self.status.container_identity();
        let container_name = container_name.to_owned();
        let diagnostics = crate::container_context::resolve_container_diagnostics();
        self.dialog_push(Dialog::new_container_info(
            container_name,
            role.to_owned(),
            focused_agent,
            self.launch_env.workdir.to_string_lossy().into_owned(),
            crate::tui::components::dialog::ContainerInfoDiagnostics {
                host_version: diagnostics.host_version,
                invocation_id: diagnostics.invocation_id,
            },
        ));
    }

    pub(super) fn open_github_context_dialog(&mut self, now: Instant) {
        self.dialog_push(Dialog::new_github_context());
        let _spawned = self.force_spawn_pull_request_context_lookup(now);
    }

    pub(super) fn github_context_view(&self) -> GithubContextView<'_> {
        let (branch, pull_request) = self.pr_watch.context();
        github_context_view_from_state(branch, pull_request, self.pull_request_context_loading())
    }

    pub(super) fn dispatch_to_dialog_top<F, R>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Dialog, Option<&GithubContextView<'_>>) -> R,
    {
        let view = github_context_view_from_state(
            self.pr_watch.pull_request_context_branch.as_deref(),
            self.pr_watch.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
        );
        let dialog = self.control.dialog_stack.last_mut()?;
        Some(f(dialog, Some(&view)))
    }

    pub(super) fn clamp_dialog_top_scroll(&mut self) {
        let view = github_context_view_from_state(
            self.pr_watch.pull_request_context_branch.as_deref(),
            self.pr_watch.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
        );
        if let Some(dialog) = self.control.dialog_stack.last_mut() {
            dialog.clamp_body_scroll(self.render.term_rows, self.render.term_cols, Some(&view));
        }
    }

    pub(super) fn dialog_pop_one(&mut self) -> Option<Dialog> {
        let popped = self.control.pop_dialog();
        if !self
            .control
            .dialog_stack
            .last()
            .is_some_and(Dialog::has_copy_feedback)
        {
            self.clipboard.dialog_copy_feedback_deadline = None;
        }
        self.sync_widget_focus();
        popped
    }

    pub(super) fn dialog_clear(&mut self) {
        self.control.clear_dialogs();
        self.clipboard.dialog_copy_feedback_deadline = None;
        self.sync_widget_focus();
    }

    pub(super) fn expire_dialog_copy_feedback(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.clipboard.dialog_copy_feedback_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.clipboard.dialog_copy_feedback_deadline = None;
        self.dialog_top_mut()
            .is_some_and(Dialog::clear_copy_feedback)
    }

    pub(super) fn expire_selection_copy_feedback(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.clipboard.selection_copy_feedback_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.clipboard.selection_copy_feedback_deadline = None;
        if !self.clipboard.selection_copied {
            return false;
        }
        self.clipboard.selection_copied = false;
        true
    }

    pub(super) fn set_clipboard_image_notice(&mut self, message: String) {
        self.clipboard.clipboard_image_notice = Some(message);
        self.clipboard.clipboard_image_notice_deadline =
            Some(Instant::now() + crate::tui::update::DIALOG_COPY_FEEDBACK_DURATION);
        self.invalidate(super::status_change_redraw_reason());
    }

    pub(super) fn clear_clipboard_image_notice(&mut self) -> bool {
        let had_notice = self.clipboard.clipboard_image_notice.take().is_some()
            || self.clipboard.clipboard_image_notice_deadline.is_some();
        self.clipboard.clipboard_image_notice_deadline = None;
        had_notice
    }

    pub(super) fn expire_clipboard_image_notice(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.clipboard.clipboard_image_notice_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.clear_clipboard_image_notice()
    }

    pub(super) fn cancel_drag(&mut self) {
        self.render.drag = None;
        self.clipboard.selection = None;
        self.clipboard.pending_selection = None;
        self.clipboard.selection_copied = false;
        self.clipboard.selection_copy_feedback_deadline = None;
        self.clear_clipboard_image_notice();
    }
}
