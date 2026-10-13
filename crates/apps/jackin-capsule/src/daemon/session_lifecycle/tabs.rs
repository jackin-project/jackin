// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Tab navigation, tab close, and per-tab zoom.

use super::super::Multiplexer;

impl Multiplexer {
    pub(crate) fn active_tab_pane_count(&self) -> usize {
        self.session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
            .map(|tab| tab.tree.all_ids().len())
            .unwrap_or_default()
    }

    pub(crate) fn next_tab(&mut self) {
        if self.session_supervisor.tabs.is_empty() {
            return;
        }
        self.cancel_drag();
        let prev = self.active_focused_id();
        self.session_supervisor.active_tab =
            (self.session_supervisor.active_tab + 1) % self.session_supervisor.tabs.len();
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    pub(crate) fn prev_tab(&mut self) {
        if self.session_supervisor.tabs.is_empty() {
            return;
        }
        self.cancel_drag();
        let prev = self.active_focused_id();
        self.session_supervisor.active_tab = if self.session_supervisor.active_tab == 0 {
            self.session_supervisor.tabs.len() - 1
        } else {
            self.session_supervisor.active_tab - 1
        };
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    pub(crate) fn jump_tab(&mut self, idx: usize) {
        if idx < self.session_supervisor.tabs.len() && idx != self.session_supervisor.active_tab {
            self.cancel_drag();
            let prev = self.active_focused_id();
            self.session_supervisor.active_tab = idx;
            self.synthesise_focus_swap(prev, self.active_focused_id());
        }
    }

    pub(crate) fn close_focused_tab(&mut self) {
        if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
            return;
        }
        // Drop any in-flight selection / drag-resize anchored to a
        // pane in this tab — resize_panes below invalidates every
        // remaining pane's rect and removing the active tab swaps the
        // visible content entirely. Mirrors close_focused_pane and
        // remove_exited_session, which both call cancel_drag for the
        // same reason.
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let tab_ids = self.session_supervisor.tabs[self.session_supervisor.active_tab]
            .tree
            .all_ids();
        let closed_codename = self.session_supervisor.tabs[self.session_supervisor.active_tab]
            .codename
            .clone();
        for id in tab_ids {
            if let Some(session) = self.session_supervisor.sessions.remove(id) {
                self.mark_agent_session_exited(id);
                session.terminate();
            }
        }
        self.session_supervisor
            .tabs
            .remove(self.session_supervisor.active_tab);
        self.retire_codename(&closed_codename);
        if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
            self.session_supervisor.active_tab =
                self.session_supervisor.tabs.len().saturating_sub(1);
        }
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    pub(crate) fn toggle_zoom(&mut self) {
        let Some(tab) = self
            .session_supervisor
            .tabs
            .get_mut(self.session_supervisor.active_tab)
        else {
            return;
        };
        let focused = tab.focused_id;
        let was_zoomed = tab
            .zoomed
            .is_some_and(|zoom_id| tab.tree.all_ids().contains(&zoom_id));
        tab.zoomed = if was_zoomed { None } else { Some(focused) };
        self.resize_panes();
    }
}
