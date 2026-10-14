// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{ArrowDir, Direction, Multiplexer, STATUS_BAR_ROWS, content_rect};

impl Multiplexer {
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

    pub(crate) fn synthesise_focus_swap(&mut self, old: Option<u64>, new: Option<u64>) {
        if old == new {
            return;
        }
        self.record_pane_focus_change();
        if let Some(o) = old
            && let Some(s) = self.session_supervisor.sessions.get(o)
            && s.focus_events_enabled()
        {
            let _sent = s.send_input(b"\x1b[O");
        }
        if let Some(n) = new
            && let Some(s) = self.session_supervisor.sessions.get(n)
            && s.focus_events_enabled()
        {
            let _sent = s.send_input(b"\x1b[I");
        }
    }

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
