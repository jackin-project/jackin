// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Resize, content-row reconciliation, focus queries, and tab labels.

use super::super::{
    Multiplexer, Rect, Tab, VisiblePane, available_content_rows, content_rect, normalize_size,
    visible_panes_for_layout,
};

impl Multiplexer {
    pub(crate) fn resize_panes(&mut self) {
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            let inner = content_rect.shrink(1);
            if let Some(session) = self.session_supervisor.sessions.get_mut(zoom_id) {
                session.resize(inner.rows, inner.cols);
            }
            return;
        }
        for tab in &self.session_supervisor.tabs {
            let leaves = tab.tree.leaves(content_rect);
            for (id, rect) in leaves {
                let inner = rect.shrink(1);
                if let Some(session) = self.session_supervisor.sessions.get_mut(id) {
                    session.resize(inner.rows, inner.cols);
                }
            }
        }
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = normalize_size(rows, cols);
        // Outer-terminal resize invalidates the drag's saved rect.
        self.cancel_drag();
        self.render.term_rows = rows;
        self.render.term_cols = cols;
        self.render.content_rows = available_content_rows(self.render.term_rows);
        self.resize_panes();
        self.render
            .ratatui_terminal
            .backend_mut()
            .resize(cols, rows);
        // Drive Ratatui's own resize so the buffers AND `last_known_area` move
        // to the new geometry together. `Terminal::clear()` resets the diff
        // baseline but leaves `last_known_area` stale, so the `autoresize()` at
        // the top of the next `draw()` re-fires `Terminal::resize` mid-frame —
        // emitting an extra screen erase (and a second one on a width shrink;
        // this backend writes `\x1b[2J\x1b[H` for every `clear_region(All)`)
        // that, with the render sentinel gone, leaves a transient pane border
        // one row high over the status bar. Syncing the area here makes that
        // autoresize a no-op. Suppression keeps this bookkeeping byte-silent
        // (the clears would otherwise ride a later frame); the single visible
        // wipe belongs to the Resize full redraw in `compose_pending_frame`.
        self.render
            .ratatui_terminal
            .backend_mut()
            .begin_clear_suppression();
        drop(
            self.render
                .ratatui_terminal
                .resize(ratatui::layout::Rect::new(0, 0, cols, rows)),
        );
        self.render
            .ratatui_terminal
            .backend_mut()
            .end_clear_suppression();
        self.invalidate(super::super::FullRedrawReason::Resize);
    }

    pub(crate) fn reconcile_content_rows(&mut self) -> bool {
        let next = available_content_rows(self.render.term_rows);
        if next == self.render.content_rows {
            return false;
        }
        self.render.content_rows = next;
        self.resize_panes();
        true
    }

    pub(crate) fn active_focused_id(&self) -> Option<u64> {
        self.session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
            .map(|t| t.focused_id)
    }

    /// Active tab's zoomed pane, if the stored id still belongs to
    /// that tab. Render / input / scroll / mouse paths keep consuming
    /// this helper so inactive tabs can retain their own zoom state
    /// without leaking it into the visible tab.
    pub(crate) fn active_zoomed_id(&self) -> Option<u64> {
        let tab = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)?;
        let zoom_id = tab.zoomed?;
        if tab.tree.all_ids().contains(&zoom_id) {
            Some(zoom_id)
        } else {
            None
        }
    }

    pub(crate) fn active_focused_outer_rect(&self) -> Option<Rect> {
        let focused = self.active_focused_id()?;
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            return (zoom_id == focused).then_some(content_rect);
        }
        self.session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)?
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == focused)
            .map(|(_, rect)| rect)
    }

    pub(crate) fn active_focused_inner_rect(&self) -> Option<Rect> {
        self.active_focused_outer_rect().map(|rect| rect.shrink(1))
    }

    /// Derive the label that should appear in the tab strip for `tab`
    /// from session facts, then delegate the visible naming rule to the
    /// TUI model boundary.
    pub(crate) fn tab_display_label(&self, tab: &Tab) -> String {
        let ids = tab.tree.all_ids();
        let pane_count = ids.len();
        let panes = ids.into_iter().filter_map(|id| {
            self.session_supervisor.sessions.get(id).map(|session| {
                let provider_label = session
                    .provider
                    .as_ref()
                    .map(|provider| provider.label.as_str());
                // Sessions store instance config IDs; tabs show the
                // per-instance label. Unknown IDs (hand-built test
                // sessions) render verbatim.
                let slug = session.agent.as_deref().map(|stored| {
                    self.launch_env
                        .launch_config
                        .agent_for_instance(stored)
                        .unwrap_or(stored)
                });
                let instance_label = session
                    .agent
                    .as_deref()
                    .and_then(|stored| self.launch_env.launch_config.label_for_instance(stored));
                crate::tui::model::visible_tab_pane_kind(crate::tui::model::VisibleTabPaneFacts {
                    instance_label,
                    agent_slug: slug,
                    provider_label,
                })
            })
        });
        crate::tui::model::tab_auto_label(pane_count, panes)
    }

    /// Rewrite each tab's auto-label after a spawn / split / remove.
    /// `Tab::label()` reads `custom_label` first, so operator-typed
    /// names survive this refresh automatically. Cheap (clones a few
    /// short strings) and easier to reason about than dispatching
    /// incremental updates from every mutation site.
    pub(crate) fn refresh_tab_labels(&mut self) {
        let mut new_labels = Vec::with_capacity(self.session_supervisor.tabs.len());
        for tab in &self.session_supervisor.tabs {
            new_labels.push(self.tab_display_label(tab));
        }
        for (tab, label) in self.session_supervisor.tabs.iter_mut().zip(new_labels) {
            tab.set_auto_label(label);
        }
    }

    pub(crate) fn visible_panes(&self) -> Vec<VisiblePane> {
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        let focused_id = self.active_focused_id();
        visible_panes_for_layout(
            content_rect,
            focused_id,
            self.active_zoomed_id(),
            self.session_supervisor
                .tabs
                .get(self.session_supervisor.active_tab),
        )
    }
}
