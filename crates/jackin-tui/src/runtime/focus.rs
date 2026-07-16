// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product projection over `TermRock`'s scoped focus ring.

use termrock::interaction::FocusRing;

/// Stable focus identities shared by jackin❯ tabbed surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFocusTarget<Content> {
    /// The tab strip owns keyboard focus.
    TabBar,
    /// A surface-owned content region owns keyboard focus.
    Content(Content),
}

/// Two-level tab/content focus backed by `TermRock`'s canonical focus mechanics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceFocus<Content> {
    ring: FocusRing<SurfaceFocusTarget<Content>, ()>,
    content: Content,
}

impl<Content: Clone + Copy + Eq> SurfaceFocus<Content> {
    /// Create a surface with its tab strip focused.
    pub fn tab_bar(content: Content) -> Self {
        Self::new(content, SurfaceFocusTarget::TabBar)
    }

    /// Create a surface with one content region focused.
    pub fn content(content: Content) -> Self {
        Self::new(content, SurfaceFocusTarget::Content(content))
    }

    fn new(content: Content, focused: SurfaceFocusTarget<Content>) -> Self {
        let mut state = Self {
            ring: FocusRing::new((), Some(focused)),
            content,
        };
        state.register();
        drop(state.ring.reconcile());
        state
    }

    fn register(&mut self) {
        self.ring.begin_frame();
        self.ring.register_order(
            (),
            [
                (SurfaceFocusTarget::TabBar, None, true),
                (SurfaceFocusTarget::Content(self.content), None, true),
            ],
        );
    }

    /// Return the currently focused product identity.
    pub fn focused(&self) -> SurfaceFocusTarget<Content> {
        self.ring
            .focused()
            .copied()
            .unwrap_or(SurfaceFocusTarget::TabBar)
    }

    /// Return the focused content identity, if content owns focus.
    pub fn focused_content(&self) -> Option<Content> {
        match self.focused() {
            SurfaceFocusTarget::Content(content) => Some(content),
            SurfaceFocusTarget::TabBar => None,
        }
    }

    /// Move focus to the tab strip.
    pub fn focus_tab_bar(&mut self) {
        self.register();
        drop(self.ring.request_focus(SurfaceFocusTarget::TabBar));
    }

    /// Move focus to a content identity.
    pub fn focus_content(&mut self, content: Content) {
        self.content = content;
        self.register();
        drop(
            self.ring
                .request_focus(SurfaceFocusTarget::Content(content)),
        );
    }

    /// Whether the tab strip owns focus.
    pub fn is_tab_bar(&self) -> bool {
        matches!(self.focused(), SurfaceFocusTarget::TabBar)
    }

    /// Whether the given content identity owns focus.
    pub fn is_content(&self, content: Content) -> bool {
        self.ring.is_focused(&SurfaceFocusTarget::Content(content))
    }

    /// Whether a content identity should expose its focused cursor.
    pub fn show_cursor_for(&self, content: &Content) -> bool {
        self.is_content(*content)
    }
}
