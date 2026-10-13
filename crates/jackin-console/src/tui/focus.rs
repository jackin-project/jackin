// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Focus management helpers: track which TUI component owns input focus and
//! compute cursor movement within a scrollable list.
//!
//! Not responsible for: rendering focus indicators or routing key events.

use ratatui::layout::Rect;
use termrock::interaction::{FocusGraph, FocusNode};

/// Console-owned two-level focus identity: the tab strip vs one surface-owned
/// content region (the role the retired facade `ConsoleFocusTarget` played).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleFocusTarget<Content> {
    /// The tab strip owns keyboard focus.
    TabBar,
    /// A surface-owned content region owns keyboard focus.
    Content(Content),
}

/// Two-level tab/content focus driven directly by `TermRock`'s [`FocusGraph`].
///
/// No painted geometry exists at this seam: registrations use a zero rect so
/// the graph stays pure keyboard identity and never hit-tests. `focused()`
/// falls back to the tab strip when the graph is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabFocus<Content> {
    graph: FocusGraph<ConsoleFocusTarget<Content>>,
    content: Content,
}

impl<Content: Clone + Copy + Eq> TabFocus<Content> {
    /// Create a surface with its tab strip focused.
    pub fn tab_bar(content: Content) -> Self {
        Self::new(content, ConsoleFocusTarget::TabBar)
    }

    /// Create a surface with one content region focused.
    pub fn content(content: Content) -> Self {
        Self::new(content, ConsoleFocusTarget::Content(content))
    }

    fn new(content: Content, focused: ConsoleFocusTarget<Content>) -> Self {
        let mut state = Self {
            graph: FocusGraph::new(),
            content,
        };
        state.register();
        drop(state.graph.request_focus(focused));
        state
    }

    fn register(&mut self) {
        self.graph.begin_frame();
        // Zero areas keep the graph to pure keyboard identity, never hit
        // testing.
        self.graph.register(FocusNode::leaf(
            ConsoleFocusTarget::TabBar,
            Rect::new(0, 0, 0, 0),
        ));
        self.graph.register(FocusNode::leaf(
            ConsoleFocusTarget::Content(self.content),
            Rect::new(0, 0, 0, 0),
        ));
    }

    /// Return the currently focused identity.
    pub fn focused(&self) -> ConsoleFocusTarget<Content> {
        self.graph
            .focused()
            .copied()
            .unwrap_or(ConsoleFocusTarget::TabBar)
    }

    /// Return the focused content identity, if content owns focus.
    pub fn focused_content(&self) -> Option<Content> {
        match self.focused() {
            ConsoleFocusTarget::Content(content) => Some(content),
            ConsoleFocusTarget::TabBar => None,
        }
    }

    /// Move focus to the tab strip.
    pub fn focus_tab_bar(&mut self) {
        self.register();
        drop(self.graph.request_focus(ConsoleFocusTarget::TabBar));
    }

    /// Move focus to a content identity.
    pub fn focus_content(&mut self, content: Content) {
        self.content = content;
        self.register();
        drop(
            self.graph
                .request_focus(ConsoleFocusTarget::Content(content)),
        );
    }

    /// Whether the tab strip owns focus.
    pub fn is_tab_bar(&self) -> bool {
        matches!(self.focused(), ConsoleFocusTarget::TabBar)
    }

    /// Whether the given content identity owns focus.
    pub fn is_content(&self, content: Content) -> bool {
        self.graph.is_focused(&ConsoleFocusTarget::Content(content))
    }

    /// Whether a content identity should expose its focused cursor.
    pub fn show_cursor_for(&self, content: &Content) -> bool {
        self.is_content(*content)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountScrollFocus {
    Workspace,
    Global,
    RoleGlobal,
    Roles,
}

/// Flat selection movement driven by upstream `CollectionState`
/// (saturating both ends — the retired hand-rolled helper never wrapped,
/// so every console list rides `.wrap(false)`). One-shot construction:
/// these lists keep no persistent collection state (no viewport of their
/// own; the workspaces list's persistent state is the step-2 wrapper).
#[must_use]
pub fn collection_move_index(selected: usize, row_count: usize, delta: isize) -> usize {
    let items: Vec<termrock::interaction::CollectionItem<usize>> = (0..row_count)
        .map(|index| termrock::interaction::CollectionItem::new(index, String::new()))
        .collect();
    let mut collection = termrock::interaction::CollectionState::new().wrap(false);
    let _ = collection.reconcile(&items);
    if selected > 0 {
        // Seed the cursor at `selected` (reconcile lands on the first item).
        let _ = collection.move_by(&items, isize::try_from(selected).unwrap_or(isize::MAX));
    }
    let _ = collection.move_by(&items, delta);
    collection.active_index(&items).unwrap_or(0)
}

#[must_use]
pub fn selected_index(selected: usize, row_count: usize) -> usize {
    selected.min(row_count.saturating_sub(1))
}

#[must_use]
pub fn follow_cursor_y(
    cursor: usize,
    content_height: usize,
    viewport_h: usize,
    stored_scroll_y: u16,
) -> u16 {
    termrock::scroll::cursor_follow_offset(
        cursor,
        content_height,
        viewport_h,
        usize::from(stored_scroll_y),
    )
    .min(usize::from(u16::MAX)) as u16
}

#[must_use]
pub fn cursor_scroll_for_panel(
    cursor: usize,
    scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> u16 {
    // header(3) + tab-strip(2) + block-borders(2) + the renderer's dynamic footer.
    let chrome = 7u16.saturating_add(footer_h);
    let viewport_h = (term_height.saturating_sub(chrome) as usize).max(1);
    // content_height - viewport_h = u16::MAX exactly: max_offset returns u16::MAX without
    // tripping its debug_assert, while the upper clamp on cursor rows stays unreachable.
    let content_height = usize::from(u16::MAX).saturating_add(viewport_h);
    follow_cursor_y(cursor, content_height, viewport_h, scroll_y)
}

#[cfg(test)]
mod tests;
