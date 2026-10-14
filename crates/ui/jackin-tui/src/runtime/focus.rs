// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;
use termrock::interaction::{FocusGraph, FocusNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFocusTarget<Content> {
    TabBar,

    Content(Content),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceFocus<Content> {
    graph: FocusGraph<SurfaceFocusTarget<Content>>,
    content: Content,
}

impl<Content: Clone + Copy + Eq> SurfaceFocus<Content> {
    pub fn tab_bar(content: Content) -> Self {
        Self::new(content, SurfaceFocusTarget::TabBar)
    }

    pub fn content(content: Content) -> Self {
        Self::new(content, SurfaceFocusTarget::Content(content))
    }

    fn new(content: Content, focused: SurfaceFocusTarget<Content>) -> Self {
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
            SurfaceFocusTarget::TabBar,
            Rect::new(0, 0, 0, 0),
        ));
        self.graph.register(FocusNode::leaf(
            SurfaceFocusTarget::Content(self.content),
            Rect::new(0, 0, 0, 0),
        ));
    }

    pub fn focused(&self) -> SurfaceFocusTarget<Content> {
        self.graph
            .focused()
            .copied()
            .unwrap_or(SurfaceFocusTarget::TabBar)
    }

    pub fn focused_content(&self) -> Option<Content> {
        match self.focused() {
            SurfaceFocusTarget::Content(content) => Some(content),
            SurfaceFocusTarget::TabBar => None,
        }
    }

    pub fn focus_tab_bar(&mut self) {
        self.register();
        drop(self.graph.request_focus(SurfaceFocusTarget::TabBar));
    }

    pub fn focus_content(&mut self, content: Content) {
        self.content = content;
        self.register();
        drop(
            self.graph
                .request_focus(SurfaceFocusTarget::Content(content)),
        );
    }

    pub fn is_tab_bar(&self) -> bool {
        matches!(self.focused(), SurfaceFocusTarget::TabBar)
    }

    pub fn is_content(&self, content: Content) -> bool {
        self.graph.is_focused(&SurfaceFocusTarget::Content(content))
    }

    pub fn show_cursor_for(&self, content: &Content) -> bool {
        self.is_content(*content)
    }
}
