//! Visible multiplexer TUI model vocabulary.
//!
//! The daemon still owns PTY/session/control-plane authority. Small visible
//! state enums live here so hover and pointer rendering share the TUI boundary
//! instead of being defined in daemon internals.

use crate::tui::layout::{Rect, SplitOrient};
use crate::tui::render::PaneBodyDim;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuxMode {
    Normal,
    PrefixAwait,
    Dialog,
    Drag,
    Select,
}

impl MuxMode {
    pub const fn forwards_to_pane(self) -> bool {
        matches!(self, Self::Normal | Self::PrefixAwait)
    }

    pub const fn blocks_focus_report(self) -> bool {
        !matches!(self, Self::Normal | Self::PrefixAwait)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointerShape {
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    Grabbing,
}

impl PointerShape {
    pub(crate) fn as_osc22_name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pointer => "pointer",
            Self::Text => "text",
            Self::EwResize => "ew-resize",
            Self::NsResize => "ns-resize",
            Self::Grabbing => "grabbing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoverTarget {
    Tab(usize),
    Menu,
    BranchContext,
    Container,
    DialogCopyTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VisibleAgentState {
    Idle,
    Working,
    Done,
    Blocked,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisiblePane {
    pub(crate) id: u64,
    pub(crate) outer: Rect,
    pub(crate) inner: Rect,
    pub(crate) focused: bool,
    pub(crate) body_dim: PaneBodyDim,
}

#[derive(Debug, Clone)]
pub(crate) struct DragState {
    pub(crate) tab_idx: usize,
    /// Tree path from the tab's root to the split node being resized
    /// (`0` = left/top child, `1` = right/bottom). Empty path = root
    /// split.
    pub(crate) path: Vec<u8>,
    pub(crate) orient: SplitOrient,
    /// Outer rectangle of the split - stable for the duration of the
    /// drag because spawns / closes block on dialog input and the
    /// daemon does not reflow during a drag.
    pub(crate) rect: Rect,
}

#[cfg(test)]
mod tests {
    use super::MuxMode;

    #[test]
    fn pane_forwarding_modes_are_explicit() {
        assert!(MuxMode::Normal.forwards_to_pane());
        assert!(MuxMode::PrefixAwait.forwards_to_pane());
        assert!(!MuxMode::Dialog.forwards_to_pane());
        assert!(!MuxMode::Drag.forwards_to_pane());
        assert!(!MuxMode::Select.forwards_to_pane());
    }

    #[test]
    fn focus_reports_only_pass_through_pane_owned_modes() {
        assert!(!MuxMode::Normal.blocks_focus_report());
        assert!(!MuxMode::PrefixAwait.blocks_focus_report());
        assert!(MuxMode::Dialog.blocks_focus_report());
        assert!(MuxMode::Drag.blocks_focus_report());
        assert!(MuxMode::Select.blocks_focus_report());
    }
}
