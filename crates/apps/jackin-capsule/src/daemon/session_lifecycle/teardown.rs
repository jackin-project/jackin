// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Session exit, tab teardown, and exit-all.

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

    /// Drop the session whose PTY just exited. Removes the pane from
    /// the owning tab's tree, focuses a sibling if any remain, and
    /// removes the tab itself when its last pane is gone. Same
    /// semantic as `close_focused_pane` but driven by the agent
    /// process exiting instead of an explicit operator action — keeps
    /// `○ Done` tabs from piling up after every agent quits.
    ///
    /// When the closed tab was the active one, focus moves to the
    /// tab on the **left**. Operator's mental model: exiting an
    /// agent should return them to whatever they were looking at
    /// before they opened that tab, not to the next-tab-to-the-right
    /// (which feels like a stack push).
    #[expect(
        clippy::excessive_nesting,
        reason = "Session-removal fn: per-tab reflow with nested drag/selection \
              cancellation + tab-index clamping. The nesting is the per-tab \
              reflow protocol."
    )]
    pub(crate) fn remove_exited_session(&mut self, session_id: u64) {
        // Any in-flight selection / drag-resize was anchored to a
        // pane that may be about to disappear (or whose siblings
        // are about to reflow). Drop both gestures so the next motion
        // event does not paint stale geometry. `cancel_drag` clears
        // selection + drag together; calling it unconditionally is
        // cheaper than per-field re-validation.
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
                // `PaneTree::remove` is a no-op on a top-level
                // `Leaf` (no parent split to collapse), so we drop
                // the tab here instead of calling it. Without this
                // branch the tab persists with a dangling session
                // id and the operator sees a `Done` tab they
                // cannot interact with.
                let was_active = tab_idx == self.session_supervisor.active_tab;
                let closed_codename = self.session_supervisor.tabs[tab_idx].codename.clone();
                self.session_supervisor.tabs.remove(tab_idx);
                // INV-D8: retire codename so tab labels drop the exited name.
                use super::super::ports::{PORTS, StatusPort};
                let now = self.wall_now_utc();
                PORTS.retire_codename(&mut self.session_supervisor, &closed_codename, now);
                if was_active {
                    // Move to the tab on the left when it exists;
                    // otherwise stay at index 0 (the leftmost tab
                    // remaining, which was the next-right neighbour
                    // before the removal). `saturating_sub(1)`
                    // collapses both "go left" and "no-left, stay
                    // at 0" into the same expression. Clamp again
                    // so `active_tab` stays in bounds if the last
                    // tab in the strip just vanished.
                    self.session_supervisor.active_tab = tab_idx.saturating_sub(1);
                    if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
                        self.session_supervisor.active_tab =
                            self.session_supervisor.tabs.len().saturating_sub(1);
                    }
                } else if tab_idx < self.session_supervisor.active_tab {
                    // A non-active tab to the left of the active one
                    // vanished; shift `active_tab` down so it keeps
                    // pointing at the same tab.
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
