// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Focus movement, focus swap synthesis, and pane clearing.

use super::super::{ArrowDir, Direction, Multiplexer, STATUS_BAR_ROWS, content_rect};

impl Multiplexer {
    /// Adjust the split that contains the focused pane along `dir` by
    /// 5% of the parent rectangle. Triggered by `Alt-Shift-Arrow`.
    pub(crate) fn resize_focused(&mut self, dir: ArrowDir) {
        let Some(tab_idx) = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
            .map(|_| self.session_supervisor.active_tab)
        else {
            return;
        };
        let focused = self.session_supervisor.tabs[tab_idx].focused_id;
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        if self.session_supervisor.tabs[tab_idx]
            .tree
            .resize(focused, d, 0.05)
        {
            self.resize_panes();
        }
    }

    pub(crate) fn move_focus(&mut self, dir: ArrowDir) {
        let Some(tab) = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
        else {
            return;
        };
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        let prev = tab.focused_id;
        if let Some(next_id) = tab.tree.adjacent(content_rect, tab.focused_id, d) {
            self.session_supervisor.tabs[self.session_supervisor.active_tab].focused_id = next_id;
            self.synthesise_focus_swap(Some(prev), Some(next_id));
        }
    }

    /// Synthesise `\x1b[O` / `\x1b[I` to track which pane the operator
    /// is actually looking at. Agents that watch focus events use them
    /// to pause polling / animations; without synthesis, a backgrounded
    /// pane thinks it is still focused.
    ///
    /// Also re-emits the newly focused session's mode state
    /// (bracketed paste, etc.) so the outer terminal matches what
    /// the now-visible agent wants. Each agent owns its own mode
    /// state and switching tabs must not leak the previous agent's
    /// setup to the new one.
    pub(crate) fn synthesise_focus_swap(&mut self, old: Option<u64>, new: Option<u64>) {
        if old == new {
            return;
        }
        self.record_pane_focus_change();
        // Synthetic `\x1b[I` / `\x1b[O` to the agent's PTY only
        // when the agent enabled focus-event reporting (DEC ?1004).
        // Shells and pre-mount agents leave it off; writing the
        // bytes into their PTY would surface as literal `[I` /
        // `[O` text at the prompt.
        if let Some(o) = old
            && let Some(s) = self.session_supervisor.sessions.get(o)
            && s.focus_events_enabled()
        {
            let _sent = s.send_input(b"\x1b[O");
        }
        // Cursor and mode state for the newly focused pane are reconciled
        // by the next composed frame (§3.4) — no assertion site here.
        if let Some(n) = new
            && let Some(s) = self.session_supervisor.sessions.get(n)
            && s.focus_events_enabled()
        {
            let _sent = s.send_input(b"\x1b[I");
        }
    }

    /// Switch focus to the pane the operator clicked on, if it differs
    /// from the current focus. Returns `true` when the focus actually
    /// changed so the caller can trigger a redraw.
    ///
    /// Honours the zoomed-pane state: when a pane is zoomed it fills
    /// the entire content rect, so clicks inside that rect must
    /// resolve to the zoomed pane even if a sibling pane's unzoomed
    /// rect happens to cover the click point. Walking `tab.tree.leaves`
    /// without this guard sends focus (and subsequent keystrokes) to
    /// a hidden pane while the zoomed pane stays painted as focused.
    pub(crate) fn focus_pane_at(&mut self, row: u16, col: u16) -> bool {
        if row < STATUS_BAR_ROWS {
            return false;
        }
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        let Some(tab) = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
        else {
            return false;
        };
        let prev = tab.focused_id;
        if let Some(zoom_id) = self.active_zoomed_id() {
            // Click outside the content rect (header chrome, status
            // bar) cannot affect zoom focus; otherwise the only
            // candidate is the zoomed pane itself.
            if row < content_rect.row + content_rect.rows
                && col < content_rect.col + content_rect.cols
                && zoom_id != prev
            {
                self.session_supervisor.tabs[self.session_supervisor.active_tab].focused_id =
                    zoom_id;
                self.synthesise_focus_swap(Some(prev), Some(zoom_id));
                return true;
            }
            return false;
        }
        let leaves = tab.tree.leaves(content_rect);
        for (id, rect) in leaves {
            if row >= rect.row
                && row < rect.row + rect.rows
                && col >= rect.col
                && col < rect.col + rect.cols
                && id != prev
            {
                self.session_supervisor.tabs[self.session_supervisor.active_tab].focused_id = id;
                self.synthesise_focus_swap(Some(prev), Some(id));
                return true;
            }
        }
        false
    }

    pub(crate) fn clear_focused_pane(&mut self) {
        self.cancel_drag();
        if let Some(id) = self.active_focused_id()
            && let Some(session) = self.session_supervisor.sessions.get_mut(id)
        {
            session.clear_scrollback_and_request_screen_clear();
        }
    }

    /// Switch the active tab to whichever tab contains the leaf
    /// carrying `session_id`, and set that tab's `focused_id` to
    /// `session_id`. Returns `true` when the search succeeded;
    /// `false` when no tab references the id, leaving state
    /// untouched.
    pub(crate) fn focus_session_globally(&mut self, session_id: u64) -> bool {
        use crate::tui::layout::Rect;
        let probe_rect = Rect::new(0, 0, self.render.term_rows, self.render.term_cols);
        let prev_focused = self.active_focused_id();
        for (tab_idx, tab) in self.session_supervisor.tabs.iter().enumerate() {
            let leaf_ids: Vec<u64> = tab
                .tree
                .leaves(probe_rect)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            if leaf_ids.contains(&session_id) {
                self.session_supervisor.active_tab = tab_idx;
                self.session_supervisor.tabs[tab_idx].focused_id = session_id;
                self.synthesise_focus_swap(prev_focused, Some(session_id));
                return true;
            }
        }
        false
    }
}
