//! Pane layout, resize, focus, and split methods for the Multiplexer.

use super::{
    ArrowDir, Direction, Multiplexer, Rect, Result, STATUS_BAR_ROWS, Session, SplitDirection,
    SplitDirectionGeometry, SplitPosition, Tab, VisiblePane, available_content_rows, content_rect,
    normalize_size, split_spawn_inner_size, visible_panes_for_layout,
};

impl Multiplexer {
    /// Split the focused pane and spawn a session of the operator's
    /// choice inside it. `agent_slug = None` opens a shell. Used by
    /// the `AgentPicker` → Split flow so the operator picks the new
    /// pane's identity instead of cloning the source pane's agent.
    pub(super) fn split_focused_into(
        &mut self,
        direction: SplitDirection,
        agent_slug: Option<String>,
        env_overrides: &[(String, String)],
        provider_label: Option<&str>,
    ) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        // Any selection / drag-resize is anchored to a specific pane
        // rect that this reflow is about to invalidate.
        self.cancel_drag();
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Ok(());
        };
        let tab_codename = tab.codename.clone();
        let from_id = tab.focused_id;
        let content_rect = content_rect(self.content_rows, self.term_cols);
        let from_rect = tab
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == from_id)
            .map_or(content_rect, |(_, r)| r);
        let split_geometry = match direction {
            SplitDirection::Left | SplitDirection::Right => SplitDirectionGeometry::LeftRight,
            SplitDirection::Above | SplitDirection::Below => SplitDirectionGeometry::TopBottom,
        };
        let (spawn_rows, spawn_cols) = split_spawn_inner_size(split_geometry, from_rect);
        let env_passthrough = self.env_for_spawn(env_overrides);
        let launch = self.session_launch(
            agent_slug.as_deref(),
            provider_label,
            &env_passthrough,
            &tab_codename,
        );
        let agent_for_log = agent_slug.clone();
        let (session, new_id) = Session::spawn(
            &launch.label,
            agent_slug,
            provider_label.map(|label| crate::session::SessionProvider {
                label: label.to_owned(),
                env_overrides: env_overrides.to_vec(),
            }),
            launch.cmd,
            self.session_terminal(spawn_rows, spawn_cols),
            self.event_tx.clone(),
        )?;
        self.sessions.insert(new_id, session);
        self.record_agent_history(
            new_id,
            tab_codename.clone(),
            agent_for_log.clone(),
            provider_label,
        );
        let tab = &mut self.tabs[self.active_tab];
        let placed = match direction {
            SplitDirection::Left => tab.tree.split_h(from_id, new_id, SplitPosition::Before),
            SplitDirection::Right => tab.tree.split_h(from_id, new_id, SplitPosition::After),
            SplitDirection::Above => tab.tree.split_v(from_id, new_id, SplitPosition::Before),
            SplitDirection::Below => tab.tree.split_v(from_id, new_id, SplitPosition::After),
        };
        if !placed {
            // from_id vanished between split intent and dispatch
            // (e.g. the source pane exited mid-action). Undo the
            // session insert so the spawned PTY + child + tasks do
            // not leak as an orphan that no tab tree references.
            if let Some(orphan) = self.sessions.remove(&new_id) {
                orphan.terminate();
            }
            crate::clog!(
                "action: split aborted — from_id={from_id} no longer in tab tree; reaped orphan id={new_id}",
            );
            return Ok(());
        }
        tab.focused_id = new_id;
        self.resize_panes();
        self.synthesise_focus_swap(Some(from_id), Some(new_id));
        crate::clog!(
            "action: split id={new_id} from={from_id} dir={direction:?} agent={agent_for_log:?} label={label}",
            label = launch.label,
        );
        Ok(())
    }

    /// Split the focused pane and clone the source pane's agent into
    /// the new pane. Used by the `Ctrl+B %` / `Ctrl+B "` prefix
    /// bindings so split-and-spawn skips the agent picker and inherits
    /// the source pane's runtime.
    pub(super) fn split_focused(&mut self, direction: SplitDirection) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        let (agent_slug, provider_env_overrides, provider_label) = self.focused_spawn_metadata();
        self.split_focused_into(
            direction,
            agent_slug,
            &provider_env_overrides,
            provider_label.as_deref(),
        )
    }

    pub(super) fn focused_spawn_metadata(
        &self,
    ) -> (Option<String>, Vec<(String, String)>, Option<String>) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return (None, Vec::new(), None);
        };
        let from_id = tab.focused_id;
        self.sessions
            .get(&from_id)
            .map_or((None, Vec::new(), None), |session| {
                let (env, label) = session.provider.as_ref().map_or_else(
                    || (Vec::new(), None),
                    |provider| (provider.env_overrides.clone(), Some(provider.label.clone())),
                );
                (session.agent.clone(), env, label)
            })
    }

    pub(super) fn close_focused_pane(&mut self) {
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let id = tab.focused_id;
        let all = tab.tree.all_ids();
        let next_focus = all.iter().find(|&&sid| sid != id).copied();
        crate::clog!(
            "action: close_focused_pane id={id} tab_idx={} siblings_remaining={}",
            self.active_tab,
            next_focus.is_some()
        );
        tab.tree.remove(id);
        if let Some(session) = self.sessions.remove(&id) {
            session.terminate();
        }
        // Drop the zoomed reference when the killed pane was the zoom
        // target so the next `compose_frame` does not paint a stale
        // zoom area until the operator manually unzooms.
        self.zoomed = self.zoomed.filter(|&zid| zid != id);
        if let Some(nf) = next_focus {
            tab.focused_id = nf;
        } else {
            self.tabs.remove(self.active_tab);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
        }
        self.mark_agent_session_exited(id);
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    pub(super) fn resize_panes(&mut self) {
        let content_rect = content_rect(self.content_rows, self.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            let inner = content_rect.shrink(1);
            if let Some(session) = self.sessions.get_mut(&zoom_id) {
                session.resize(inner.rows, inner.cols);
            }
            return;
        }
        for tab in &self.tabs {
            let leaves = tab.tree.leaves(content_rect);
            for (id, rect) in leaves {
                let inner = rect.shrink(1);
                if let Some(session) = self.sessions.get_mut(&id) {
                    session.resize(inner.rows, inner.cols);
                }
            }
        }
    }

    pub(super) fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = normalize_size(rows, cols);
        crate::cdebug!(
            "resize: {}x{} → {}x{} content_rows={}",
            self.term_cols,
            self.term_rows,
            cols,
            rows,
            available_content_rows(rows),
        );
        // Outer-terminal resize invalidates the drag's saved rect.
        self.cancel_drag();
        self.term_rows = rows;
        self.term_cols = cols;
        self.content_rows = available_content_rows(self.term_rows);
        self.resize_panes();
        self.ratatui_terminal.backend_mut().resize(cols, rows);
        // A size change invalidates Ratatui's previous-buffer geometry. Reset
        // only the double-buffer state: Terminal::clear() routes through
        // clear_region(All) → `\x1b[2J`, and that stray erase would otherwise
        // sit in the backend buffer and ride whatever frame drains next. The
        // visible wipe belongs to the Resize full redraw, not to this
        // bookkeeping call.
        self.ratatui_terminal
            .backend_mut()
            .suppress_next_clear_escape();
        drop(self.ratatui_terminal.clear());
        self.invalidate(super::FullRedrawReason::Resize);
    }

    pub(super) fn reconcile_content_rows(&mut self) -> bool {
        let next = available_content_rows(self.term_rows);
        if next == self.content_rows {
            return false;
        }
        self.content_rows = next;
        self.resize_panes();
        true
    }

    pub(super) fn active_focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|t| t.focused_id)
    }

    /// `self.zoomed` narrowed to "only when the zoomed session belongs
    /// to the active tab." The zoom field is global (one value across
    /// all tabs), but render / input / scroll / mouse paths must
    /// behave as if zoom is per-tab — switching tabs has to surface
    /// the new tab's panes normally even when a different tab still
    /// has a zoomed session pinned, otherwise opening a new tab paints
    /// the previously-zoomed pane full-screen. Returning `None` from
    /// the active-tab check routes every consumer of zoom state
    /// through the normal multi-pane path for tabs that don't hold
    /// the zoom.
    pub(super) fn active_zoomed_id(&self) -> Option<u64> {
        let zoom_id = self.zoomed?;
        let tab = self.tabs.get(self.active_tab)?;
        if tab.tree.all_ids().contains(&zoom_id) {
            Some(zoom_id)
        } else {
            None
        }
    }

    pub(super) fn active_focused_outer_rect(&self) -> Option<Rect> {
        let focused = self.active_focused_id()?;
        let content_rect = content_rect(self.content_rows, self.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            return (zoom_id == focused).then_some(content_rect);
        }
        self.tabs
            .get(self.active_tab)?
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == focused)
            .map(|(_, rect)| rect)
    }

    pub(super) fn active_focused_inner_rect(&self) -> Option<Rect> {
        self.active_focused_outer_rect().map(|rect| rect.shrink(1))
    }

    /// Derive the label that should appear in the tab strip for `tab`
    /// from session facts, then delegate the visible naming rule to the
    /// TUI model boundary.
    pub(super) fn tab_display_label(&self, tab: &Tab) -> String {
        let ids = tab.tree.all_ids();
        let pane_count = ids.len();
        let panes = ids.into_iter().filter_map(|id| {
            self.sessions.get(&id).map(|session| {
                let provider_label = session
                    .provider
                    .as_ref()
                    .map(|provider| provider.label.as_str());
                crate::tui::app::visible_tab_pane_kind(crate::tui::app::VisibleTabPaneFacts {
                    agent_slug: session.agent.as_deref(),
                    provider_label,
                })
            })
        });
        crate::tui::app::tab_auto_label(pane_count, panes)
    }

    /// Rewrite each tab's auto-label after a spawn / split / remove.
    /// `Tab::label()` reads `custom_label` first, so operator-typed
    /// names survive this refresh automatically. Cheap (clones a few
    /// short strings) and easier to reason about than dispatching
    /// incremental updates from every mutation site.
    pub(super) fn refresh_tab_labels(&mut self) {
        let mut new_labels = Vec::with_capacity(self.tabs.len());
        for tab in &self.tabs {
            new_labels.push(self.tab_display_label(tab));
        }
        for (tab, label) in self.tabs.iter_mut().zip(new_labels) {
            tab.set_auto_label(label);
        }
    }

    pub(super) fn visible_panes(&self) -> Vec<VisiblePane> {
        let content_rect = content_rect(self.content_rows, self.term_cols);
        let focused_id = self.active_focused_id();
        visible_panes_for_layout(
            content_rect,
            focused_id,
            self.active_zoomed_id(),
            self.tabs.get(self.active_tab),
        )
    }

    /// Adjust the split that contains the focused pane along `dir` by
    /// 5% of the parent rectangle. Triggered by `Alt+Shift+Arrow`.
    pub(super) fn resize_focused(&mut self, dir: ArrowDir) {
        let Some(tab_idx) = self.tabs.get(self.active_tab).map(|_| self.active_tab) else {
            return;
        };
        let focused = self.tabs[tab_idx].focused_id;
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        if self.tabs[tab_idx].tree.resize(focused, d, 0.05) {
            self.resize_panes();
        }
    }

    pub(super) fn move_focus(&mut self, dir: ArrowDir) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let content_rect = content_rect(self.content_rows, self.term_cols);
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        let prev = tab.focused_id;
        if let Some(next_id) = tab.tree.adjacent(content_rect, tab.focused_id, d) {
            self.tabs[self.active_tab].focused_id = next_id;
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
    pub(super) fn synthesise_focus_swap(&mut self, old: Option<u64>, new: Option<u64>) {
        if old == new {
            return;
        }
        // Synthetic `\x1b[I` / `\x1b[O` to the agent's PTY only
        // when the agent enabled focus-event reporting (DEC ?1004).
        // Shells and pre-mount agents leave it off; writing the
        // bytes into their PTY would surface as literal `[I` /
        // `[O` text at the prompt.
        if let Some(o) = old
            && let Some(s) = self.sessions.get(&o)
            && s.focus_events_enabled()
        {
            s.send_input(b"\x1b[O");
        }
        // Cursor and mode state for the newly focused pane are reconciled
        // by the next composed frame (§3.4) — no assertion site here.
        if let Some(n) = new
            && let Some(s) = self.sessions.get(&n)
            && s.focus_events_enabled()
        {
            s.send_input(b"\x1b[I");
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
    pub(super) fn focus_pane_at(&mut self, row: u16, col: u16) -> bool {
        if row < STATUS_BAR_ROWS {
            return false;
        }
        let content_rect = content_rect(self.content_rows, self.term_cols);
        let Some(tab) = self.tabs.get(self.active_tab) else {
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
                self.tabs[self.active_tab].focused_id = zoom_id;
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
                self.tabs[self.active_tab].focused_id = id;
                self.synthesise_focus_swap(Some(prev), Some(id));
                return true;
            }
        }
        false
    }

    pub(super) fn clear_focused_pane(&mut self) {
        self.cancel_drag();
        if let Some(id) = self.active_focused_id()
            && let Some(session) = self.sessions.get_mut(&id)
        {
            session.clear_scrollback_and_request_screen_clear();
        }
    }

    /// Switch the active tab to whichever tab contains the leaf
    /// carrying `session_id`, and set that tab's `focused_id` to
    /// `session_id`. Returns `true` when the search succeeded;
    /// `false` when no tab references the id, leaving state
    /// untouched.
    pub(super) fn focus_session_globally(&mut self, session_id: u64) -> bool {
        use crate::tui::layout::Rect;
        let probe_rect = Rect::new(0, 0, self.term_rows, self.term_cols);
        let prev_focused = self.active_focused_id();
        for (tab_idx, tab) in self.tabs.iter().enumerate() {
            let leaf_ids: Vec<u64> = tab
                .tree
                .leaves(probe_rect)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            if leaf_ids.contains(&session_id) {
                self.active_tab = tab_idx;
                self.tabs[tab_idx].focused_id = session_id;
                self.synthesise_focus_swap(prev_focused, Some(session_id));
                return true;
            }
        }
        false
    }
}
