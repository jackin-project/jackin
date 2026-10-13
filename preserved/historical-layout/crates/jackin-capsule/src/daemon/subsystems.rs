//! Behavioral APIs for daemon-owned state groups.
//!
//! The Multiplexer coordinates these owners; invariant-preserving reads and
//! transitions live here so new daemon state does not leak back into the shell.

use super::{
    ClipboardState, ControlRouting, Dialog, LaunchEnv, PrWatch, PullRequestInfo, RenderState,
    StatusState, UsageState,
};

impl StatusState {
    pub(crate) fn container_identity(&self) -> (&str, &str) {
        (self.status_bar.container_name(), self.status_bar.role())
    }
}

impl ClipboardState {
    pub(crate) const fn is_selecting(&self) -> bool {
        self.selection.is_some()
    }
}

impl PrWatch {
    pub(crate) fn context(&self) -> (Option<&str>, Option<&PullRequestInfo>) {
        (
            self.pull_request_context_branch.as_deref(),
            self.pull_request_context.as_deref(),
        )
    }
}

impl UsageState {
    pub(crate) fn cache(&self) -> &crate::usage::UsageCache {
        &self.usage_cache
    }
}

impl ControlRouting {
    pub(crate) fn dialog_top(&self) -> Option<&Dialog> {
        self.dialog_stack.last()
    }

    pub(crate) fn dialog_top_mut(&mut self) -> Option<&mut Dialog> {
        self.dialog_stack.last_mut()
    }

    pub(crate) fn push_dialog(&mut self, dialog: Dialog) {
        self.dialog_stack.push(dialog);
    }

    pub(crate) fn pop_dialog(&mut self) -> Option<Dialog> {
        self.dialog_stack.pop()
    }

    pub(crate) fn clear_dialogs(&mut self) {
        self.dialog_stack.clear();
    }
}

impl RenderState {
    pub(crate) const fn terminal_size(&self) -> (u16, u16) {
        (self.term_rows, self.term_cols)
    }
}

impl LaunchEnv {
    pub(crate) const fn config(&self) -> &jackin_protocol::CapsuleConfig {
        &self.launch_config
    }
}

#[cfg(test)]
mod tests;
