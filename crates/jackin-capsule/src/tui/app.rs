//! Visible multiplexer TUI model vocabulary.
//!
//! The daemon still owns PTY/session/control-plane authority. Small visible
//! state enums live here so hover and pointer rendering share the TUI boundary
//! instead of being defined in daemon internals.

use crate::tui::layout::{Rect, SplitOrient};
use crate::tui::render::PaneBodyDim;
use crate::tui::components::branch_context_bar::BranchContextBarHit;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MuxModeState {
    pub(crate) dialog_open: bool,
    pub(crate) dragging: bool,
    pub(crate) selecting: bool,
    pub(crate) awaiting_prefix: bool,
}

pub(crate) fn mux_mode_for_state(state: MuxModeState) -> MuxMode {
    if state.dialog_open {
        MuxMode::Dialog
    } else if state.dragging {
        MuxMode::Drag
    } else if state.selecting {
        MuxMode::Select
    } else if state.awaiting_prefix {
        MuxMode::PrefixAwait
    } else {
        MuxMode::Normal
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointerShapeState {
    pub(crate) dragging: bool,
    pub(crate) selecting: bool,
    pub(crate) chrome_target: Option<HoverTarget>,
    pub(crate) dialog_open: bool,
    pub(crate) drag_start_orient: Option<SplitOrient>,
    pub(crate) selection_start_available: bool,
    pub(crate) no_button_motion: bool,
}

pub(crate) fn pointer_shape_for_state(state: PointerShapeState) -> PointerShape {
    if state.dragging {
        return PointerShape::Grabbing;
    }
    if state.selecting {
        return PointerShape::Text;
    }
    match state.chrome_target {
        Some(HoverTarget::DialogCopyTarget) => return PointerShape::Pointer,
        None if state.dialog_open => return PointerShape::Default,
        Some(_) => return PointerShape::Pointer,
        None => {}
    }
    if let Some(orient) = state.drag_start_orient {
        return match orient {
            SplitOrient::Horizontal => PointerShape::EwResize,
            SplitOrient::Vertical => PointerShape::NsResize,
        };
    }
    if state.no_button_motion && state.selection_start_available {
        return PointerShape::Text;
    }
    PointerShape::Default
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoverTarget {
    Tab(usize),
    Menu,
    BranchContext,
    Container,
    DialogCopyTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChromeHitState {
    pub(crate) dialog_copy_target: bool,
    pub(crate) dialog_open: bool,
    pub(crate) tab: Option<usize>,
    pub(crate) menu_hit: bool,
    pub(crate) branch_hit: Option<BranchContextBarHit>,
}

pub(crate) fn chrome_hover_target_for_state(state: ChromeHitState) -> Option<HoverTarget> {
    if state.dialog_open {
        return state
            .dialog_copy_target
            .then_some(HoverTarget::DialogCopyTarget);
    }
    if let Some(tab_idx) = state.tab {
        return Some(HoverTarget::Tab(tab_idx));
    }
    if state.menu_hit {
        return Some(HoverTarget::Menu);
    }
    match state.branch_hit {
        Some(BranchContextBarHit::Context) => Some(HoverTarget::BranchContext),
        Some(BranchContextBarHit::Container) => Some(HoverTarget::Container),
        None => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HoverState {
    pub(crate) dragging: bool,
    pub(crate) selecting: bool,
    pub(crate) chrome_target: Option<HoverTarget>,
}

pub(crate) fn hover_target_for_state(state: HoverState) -> Option<HoverTarget> {
    if state.dragging || state.selecting {
        None
    } else {
        state.chrome_target
    }
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
    use crate::tui::components::branch_context_bar::BranchContextBarHit;
    use crate::tui::layout::SplitOrient;

    use super::{
        ChromeHitState, HoverState, HoverTarget, MuxMode, MuxModeState, PointerShape,
        PointerShapeState, chrome_hover_target_for_state, hover_target_for_state,
        mux_mode_for_state, pointer_shape_for_state,
    };

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

    #[test]
    fn mux_mode_priority_matches_visible_gestures() {
        assert_eq!(
            mux_mode_for_state(MuxModeState {
                dialog_open: true,
                dragging: true,
                selecting: true,
                awaiting_prefix: true,
            }),
            MuxMode::Dialog
        );
        assert_eq!(
            mux_mode_for_state(MuxModeState {
                dialog_open: false,
                dragging: true,
                selecting: true,
                awaiting_prefix: true,
            }),
            MuxMode::Drag
        );
        assert_eq!(
            mux_mode_for_state(MuxModeState {
                dialog_open: false,
                dragging: false,
                selecting: true,
                awaiting_prefix: true,
            }),
            MuxMode::Select
        );
        assert_eq!(
            mux_mode_for_state(MuxModeState {
                dialog_open: false,
                dragging: false,
                selecting: false,
                awaiting_prefix: true,
            }),
            MuxMode::PrefixAwait
        );
    }

    #[test]
    fn pointer_shape_priority_keeps_dialog_and_gestures_visible() {
        let base = PointerShapeState {
            dragging: false,
            selecting: false,
            chrome_target: None,
            dialog_open: false,
            drag_start_orient: None,
            selection_start_available: false,
            no_button_motion: true,
        };
        assert_eq!(
            pointer_shape_for_state(PointerShapeState {
                dragging: true,
                ..base
            }),
            PointerShape::Grabbing
        );
        assert_eq!(
            pointer_shape_for_state(PointerShapeState {
                chrome_target: Some(HoverTarget::Tab(0)),
                ..base
            }),
            PointerShape::Pointer
        );
        assert_eq!(
            pointer_shape_for_state(PointerShapeState {
                dialog_open: true,
                ..base
            }),
            PointerShape::Default
        );
        assert_eq!(
            pointer_shape_for_state(PointerShapeState {
                drag_start_orient: Some(SplitOrient::Horizontal),
                ..base
            }),
            PointerShape::EwResize
        );
        assert_eq!(
            pointer_shape_for_state(PointerShapeState {
                selection_start_available: true,
                ..base
            }),
            PointerShape::Text
        );
    }

    #[test]
    fn chrome_hover_priority_matches_visible_layers() {
        let base = ChromeHitState {
            dialog_copy_target: false,
            dialog_open: false,
            tab: None,
            menu_hit: false,
            branch_hit: None,
        };
        assert_eq!(
            chrome_hover_target_for_state(ChromeHitState {
                dialog_open: true,
                dialog_copy_target: true,
                tab: Some(1),
                menu_hit: true,
                ..base
            }),
            Some(HoverTarget::DialogCopyTarget)
        );
        assert_eq!(
            chrome_hover_target_for_state(ChromeHitState {
                tab: Some(1),
                menu_hit: true,
                branch_hit: Some(BranchContextBarHit::Container),
                ..base
            }),
            Some(HoverTarget::Tab(1))
        );
        assert_eq!(
            chrome_hover_target_for_state(ChromeHitState {
                menu_hit: true,
                branch_hit: Some(BranchContextBarHit::Container),
                ..base
            }),
            Some(HoverTarget::Menu)
        );
        assert_eq!(
            chrome_hover_target_for_state(ChromeHitState {
                branch_hit: Some(BranchContextBarHit::Container),
                ..base
            }),
            Some(HoverTarget::Container)
        );
    }

    #[test]
    fn gesture_state_suppresses_hover_targets() {
        assert_eq!(
            hover_target_for_state(HoverState {
                dragging: true,
                selecting: false,
                chrome_target: Some(HoverTarget::Menu),
            }),
            None
        );
        assert_eq!(
            hover_target_for_state(HoverState {
                dragging: false,
                selecting: false,
                chrome_target: Some(HoverTarget::Menu),
            }),
            Some(HoverTarget::Menu)
        );
    }
}
