// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::Multiplexer;

impl Multiplexer {
    pub(crate) fn exit_all_sessions(&mut self) {
        self.cancel_drag();
        for (_, session) in self.session_supervisor.sessions.drain() {
            session.terminate();
        }
        self.session_supervisor.tabs.clear();
        self.session_supervisor.active_tab = 0;
        self.clipboard.dialog_copy_feedback_deadline = None;
        self.render.hover_target = None;
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "Session-removal fn: per-tab reflow with nested drag/selection \
              cancellation + tab-index clamping. The nesting is the per-tab \
              reflow protocol."
    )]
    pub(crate) fn remove_exited_session(&mut self, session_id: u64) {
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let owning_tab = self
            .session_supervisor
            .tabs
            .iter()
            .position(|t| t.tree.all_ids().contains(&session_id));
        if let Some(tab_idx) = owning_tab {
            let leaves = self.session_supervisor.tabs[tab_idx].tree.all_ids();
            let tab_is_empty = leaves.len() == 1 && leaves[0] == session_id;
            if tab_is_empty {
                let was_active = tab_idx == self.session_supervisor.active_tab;
                let closed_codename = self.session_supervisor.tabs[tab_idx].codename.clone();
                self.session_supervisor.tabs.remove(tab_idx);
                use super::super::ports::{PORTS, StatusPort};
                let now = self.wall_now_utc();
                PORTS.retire_codename(&mut self.session_supervisor, &closed_codename, now);
                if was_active {
                    self.session_supervisor.active_tab = tab_idx.saturating_sub(1);
                    if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
                        self.session_supervisor.active_tab =
                            self.session_supervisor.tabs.len().saturating_sub(1);
                    }
                } else if tab_idx < self.session_supervisor.active_tab {
                    self.session_supervisor.active_tab -= 1;
                }
            } else {
                self.session_supervisor.tabs[tab_idx]
                    .tree
                    .remove(session_id);
                if self.session_supervisor.tabs[tab_idx].focused_id == session_id {
                    let remaining = self.session_supervisor.tabs[tab_idx].tree.all_ids();
                    if let Some(&next_focus) = remaining.first() {
                        self.session_supervisor.tabs[tab_idx].focused_id = next_focus;
                    }
                }
            }
        }
        self.session_supervisor.sessions.remove(session_id);
        self.mark_agent_session_exited(session_id);
        if let Some(tab_idx) = owning_tab
            && let Some(tab) = self.session_supervisor.tabs.get_mut(tab_idx)
        {
            tab.zoomed = tab.zoomed.filter(|&id| id != session_id);
        }
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }
}
