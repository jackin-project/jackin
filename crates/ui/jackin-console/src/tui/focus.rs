// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;
use termrock::interaction::{FocusGraph, FocusNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleFocusTarget<Content> {
    TabBar,
    Content(Content),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabFocus<Content> {
    graph: FocusGraph<ConsoleFocusTarget<Content>>,
    content: Content,
}

impl<Content: Clone + Copy + Eq> TabFocus<Content> {
    pub fn tab_bar(content: Content) -> Self {
        Self::new(content, ConsoleFocusTarget::TabBar)
    }

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
        self.graph.register(FocusNode::leaf(
            ConsoleFocusTarget::TabBar,
            Rect::new(0, 0, 0, 0),
        ));
        self.graph.register(FocusNode::leaf(
            ConsoleFocusTarget::Content(self.content),
            Rect::new(0, 0, 0, 0),
        ));
    }

    pub fn focused(&self) -> ConsoleFocusTarget<Content> {
        self.graph
            .focused()
            .copied()
            .unwrap_or(ConsoleFocusTarget::TabBar)
    }

    pub fn focused_content(&self) -> Option<Content> {
        match self.focused() {
            ConsoleFocusTarget::Content(content) => Some(content),
            ConsoleFocusTarget::TabBar => None,
        }
    }

    pub fn focus_tab_bar(&mut self) {
        self.register();
        drop(self.graph.request_focus(ConsoleFocusTarget::TabBar));
    }

    pub fn focus_content(&mut self, content: Content) {
        self.content = content;
        self.register();
        drop(
            self.graph
                .request_focus(ConsoleFocusTarget::Content(content)),
        );
    }

    pub fn is_tab_bar(&self) -> bool {
        matches!(self.focused(), ConsoleFocusTarget::TabBar)
    }

    pub fn is_content(&self, content: Content) -> bool {
        self.graph.is_focused(&ConsoleFocusTarget::Content(content))
    }

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

#[must_use]
pub fn collection_move_index(selected: usize, row_count: usize, delta: isize) -> usize {
    let items: Vec<termrock::interaction::CollectionItem<usize>> = (0..row_count)
        .map(|index| termrock::interaction::CollectionItem::new(index, String::new()))
        .collect();
    let mut collection = termrock::interaction::CollectionState::new().wrap(false);
    let _ = collection.reconcile(&items);
    if selected > 0 {
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
    let chrome = 7u16.saturating_add(footer_h);
    let viewport_h = (term_height.saturating_sub(chrome) as usize).max(1);
    let content_height = usize::from(u16::MAX).saturating_add(viewport_h);
    follow_cursor_y(cursor, content_height, viewport_h, scroll_y)
}
