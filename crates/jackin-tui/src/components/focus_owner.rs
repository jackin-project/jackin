// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `FocusOwner` — per-screen single focus owner model.
//!
//! Every screen that uses tab bars + content blocks instantiates one
//! `FocusOwner<Tab>` where `Tab` is an enum naming the tabs. The current
//! owner is always one of: the tab bar, or a specific content block.
//!
//! All rendering decisions (green border, ▸ cursor, hint scope) derive from
//! this single value rather than from scattered bools.

use crate::components::panel::PanelFocus;

/// Focus behavior shared by every button-row dialog: a closed ring of
/// semantic focus states with a stable [`ButtonStrip`](super::button_strip::ButtonStrip)
/// index.
pub trait ButtonFocus: Copy + Eq + 'static {
    const RING: &'static [Self];

    /// Index into the dialog's button strip items.
    #[must_use]
    fn index(self) -> usize {
        Self::RING
            .iter()
            .position(|focus| focus == &self)
            .unwrap_or(0)
    }

    #[must_use]
    fn next(self) -> Self {
        let ring = Self::RING;
        if ring.is_empty() {
            return self;
        }
        let idx = self.index();
        ring[(idx + 1) % ring.len()]
    }

    #[must_use]
    fn prev(self) -> Self {
        let ring = Self::RING;
        if ring.is_empty() {
            return self;
        }
        let idx = self.index();
        ring[(idx + ring.len() - 1) % ring.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusOwner<Tab> {
    #[default]
    /// The tab bar owns focus (green tab underline, dark content borders, no ▸).
    TabBar,
    /// A specific tab's content block owns focus (green border, ▸ cursor, white tab underline).
    Content(Tab),
}

impl<Tab: Copy> FocusOwner<Tab> {
    #[must_use]
    pub const fn is_tab_bar(self) -> bool {
        matches!(self, Self::TabBar)
    }

    #[must_use]
    pub const fn is_content(self) -> bool {
        matches!(self, Self::Content(_))
    }

    #[must_use]
    pub fn content_tab(self) -> Option<Tab> {
        match self {
            Self::Content(tab) => Some(tab),
            Self::TabBar => None,
        }
    }

    /// Return the `PanelFocus` for the content block identified by `tab`.
    ///
    /// Returns `Focused` when this owner is `Content(t)` where `tab == t`,
    /// and `Unfocused` otherwise (including when the tab bar owns focus).
    #[must_use]
    pub fn panel_focus_for<F: PartialEq<Tab>>(self, tab: &F) -> PanelFocus {
        match self {
            Self::Content(owned) if tab == &owned => PanelFocus::Focused,
            _ => PanelFocus::Unfocused,
        }
    }

    /// Whether the ▸ cursor should appear on the content block for `tab`.
    #[must_use]
    pub fn show_cursor_for<F: PartialEq<Tab>>(self, tab: &F) -> bool {
        matches!(self, Self::Content(ref owned) if tab == owned)
    }
}
