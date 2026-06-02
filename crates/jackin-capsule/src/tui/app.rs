//! Visible multiplexer TUI model vocabulary.
//!
//! The daemon still owns PTY/session/control-plane authority. Small visible
//! state enums live here so hover and pointer rendering share the TUI boundary
//! instead of being defined in daemon internals.

use crate::protocol::AgentState;
use crate::tui::components::branch_context_bar::BranchContextBarHit;
use crate::tui::layout::{Rect, SplitOrient, Tab};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorVisibilityState {
    pub(crate) dialog_open: bool,
    pub(crate) focused_pane_available: bool,
    pub(crate) focused_session_received_output: bool,
    pub(crate) scrollback_active: bool,
    pub(crate) agent_cursor_hidden: bool,
}

pub(crate) fn cursor_visible_for_state(state: CursorVisibilityState) -> bool {
    !state.dialog_open
        && state.focused_pane_available
        && state.focused_session_received_output
        && !state.scrollback_active
        && !state.agent_cursor_hidden
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibleAgentState {
    Idle,
    Working,
    Done,
    Blocked,
}

pub fn visible_agent_state_from_protocol(state: AgentState) -> VisibleAgentState {
    match state {
        AgentState::Idle => VisibleAgentState::Idle,
        AgentState::Working => VisibleAgentState::Working,
        AgentState::Done => VisibleAgentState::Done,
        AgentState::Blocked => VisibleAgentState::Blocked,
    }
}

/// Human-readable label for an agent/shell visible in tab and pane chrome.
pub(crate) fn visible_agent_label(
    agent_slug: Option<&str>,
    provider_label: Option<&str>,
) -> String {
    let Some(slug) = agent_slug else {
        return "Shell".to_string();
    };
    let mut chars = slug.chars();
    let base = match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    };
    match provider_label {
        Some(provider) => format!("{base} ({provider})"),
        None => base,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisiblePane {
    pub(crate) id: u64,
    pub(crate) outer: Rect,
    pub(crate) inner: Rect,
    pub(crate) focused: bool,
    pub(crate) body_dim: PaneBodyDim,
}

pub(crate) fn visible_panes_for_layout(
    content_rect: Rect,
    focused_id: Option<u64>,
    zoom_id: Option<u64>,
    active_tab: Option<&Tab>,
) -> Vec<VisiblePane> {
    if let Some(zoom_id) = zoom_id {
        let outer = content_rect;
        return vec![VisiblePane {
            id: zoom_id,
            outer,
            inner: outer.shrink(1),
            focused: Some(zoom_id) == focused_id,
            body_dim: PaneBodyDim::Normal,
        }];
    }
    let Some(tab) = active_tab else {
        return Vec::new();
    };
    let leaves = tab.tree.leaves(content_rect);
    let multi_pane = leaves.len() > 1;
    leaves
        .into_iter()
        .map(|(id, outer)| {
            let focused = Some(id) == focused_id;
            VisiblePane {
                id,
                outer,
                inner: outer.shrink(1),
                focused,
                body_dim: if multi_pane && !focused {
                    PaneBodyDim::Inactive
                } else {
                    PaneBodyDim::Normal
                },
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VisibleTabPaneKind {
    Agent(String),
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleTabPaneFacts<'a> {
    pub(crate) agent_slug: Option<&'a str>,
    pub(crate) provider_label: Option<&'a str>,
}

pub(crate) fn visible_tab_pane_kind(facts: VisibleTabPaneFacts<'_>) -> VisibleTabPaneKind {
    match facts.agent_slug {
        Some(agent) => {
            VisibleTabPaneKind::Agent(visible_agent_label(Some(agent), facts.provider_label))
        }
        None => VisibleTabPaneKind::Shell,
    }
}

/// Derive the auto-label shown in the tab strip from visible pane makeup.
///
/// Operator-owned custom labels still shadow this in [`Tab::label`]; this helper
/// only owns the visible default when the daemon refreshes tab chrome.
pub(crate) fn tab_auto_label(
    pane_count: usize,
    panes: impl IntoIterator<Item = VisibleTabPaneKind>,
) -> String {
    let mut agent_labels: Vec<String> = Vec::new();
    let mut has_shell = false;
    for pane in panes {
        match pane {
            VisibleTabPaneKind::Agent(label) => {
                if !agent_labels.iter().any(|existing| existing == &label) {
                    agent_labels.push(label);
                }
            }
            VisibleTabPaneKind::Shell => has_shell = true,
        }
    }
    let base = match (agent_labels.len(), has_shell) {
        (0, _) => "Shell".to_string(),
        (1, false) => agent_labels[0].clone(),
        (_, false) => "Agents".to_string(),
        (_, true) => "Mix".to_string(),
    };
    if pane_count > 1 {
        format!("{base} ({pane_count})")
    } else {
        base
    }
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
    use crate::protocol::AgentState;
    use crate::tui::components::branch_context_bar::BranchContextBarHit;
    use crate::tui::layout::{PaneTree, Rect, SplitOrient, Tab};
    use crate::tui::render::PaneBodyDim;

    use super::{
        ChromeHitState, CursorVisibilityState, HoverState, HoverTarget, MuxMode, MuxModeState,
        PointerShape, PointerShapeState, VisibleAgentState, VisibleTabPaneFacts,
        VisibleTabPaneKind, chrome_hover_target_for_state, cursor_visible_for_state,
        hover_target_for_state, mux_mode_for_state, pointer_shape_for_state, tab_auto_label,
        visible_agent_label, visible_agent_state_from_protocol, visible_panes_for_layout,
        visible_tab_pane_kind,
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

    #[test]
    fn cursor_visibility_requires_live_focused_pane() {
        let visible = CursorVisibilityState {
            dialog_open: false,
            focused_pane_available: true,
            focused_session_received_output: true,
            scrollback_active: false,
            agent_cursor_hidden: false,
        };
        assert!(cursor_visible_for_state(visible));
        assert!(!cursor_visible_for_state(CursorVisibilityState {
            dialog_open: true,
            ..visible
        }));
        assert!(!cursor_visible_for_state(CursorVisibilityState {
            focused_pane_available: false,
            ..visible
        }));
        assert!(!cursor_visible_for_state(CursorVisibilityState {
            focused_session_received_output: false,
            ..visible
        }));
        assert!(!cursor_visible_for_state(CursorVisibilityState {
            scrollback_active: true,
            ..visible
        }));
        assert!(!cursor_visible_for_state(CursorVisibilityState {
            agent_cursor_hidden: true,
            ..visible
        }));
    }

    #[test]
    fn visible_panes_mark_unfocused_split_bodies_inactive() {
        let mut tab = Tab::new_single("tab", 1);
        tab.tree = PaneTree::HSplit {
            left: Box::new(PaneTree::Leaf(1)),
            right: Box::new(PaneTree::Leaf(2)),
            ratio: 0.5,
        };

        let panes = visible_panes_for_layout(Rect::new(1, 0, 10, 20), Some(2), None, Some(&tab));

        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].id, 1);
        assert!(!panes[0].focused);
        assert_eq!(panes[0].body_dim, PaneBodyDim::Inactive);
        assert_eq!(panes[1].id, 2);
        assert!(panes[1].focused);
        assert_eq!(panes[1].body_dim, PaneBodyDim::Normal);
    }

    #[test]
    fn zoomed_visible_pane_uses_whole_content_rect() {
        let tab = Tab::new_single("tab", 1);
        let panes = visible_panes_for_layout(Rect::new(1, 0, 10, 20), Some(1), Some(1), Some(&tab));

        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].id, 1);
        assert_eq!(panes[0].outer, Rect::new(1, 0, 10, 20));
        assert_eq!(panes[0].inner, Rect::new(2, 1, 8, 18));
        assert!(panes[0].focused);
        assert_eq!(panes[0].body_dim, PaneBodyDim::Normal);
    }

    #[test]
    fn tab_auto_label_tracks_visible_pane_makeup() {
        assert_eq!(tab_auto_label(1, [VisibleTabPaneKind::Shell]), "Shell");
        assert_eq!(
            tab_auto_label(1, [VisibleTabPaneKind::Agent("Claude (Z.AI)".into())]),
            "Claude (Z.AI)"
        );
        assert_eq!(
            tab_auto_label(
                2,
                [
                    VisibleTabPaneKind::Agent("Claude".into()),
                    VisibleTabPaneKind::Agent("Codex".into()),
                ],
            ),
            "Agents (2)"
        );
        assert_eq!(
            tab_auto_label(
                2,
                [
                    VisibleTabPaneKind::Agent("Claude".into()),
                    VisibleTabPaneKind::Shell
                ],
            ),
            "Mix (2)"
        );
    }

    #[test]
    fn visible_agent_label_formats_shell_agent_and_provider() {
        assert_eq!(visible_agent_label(None, None), "Shell");
        assert_eq!(visible_agent_label(Some("claude"), None), "Claude");
        assert_eq!(
            visible_agent_label(Some("claude"), Some("Z.AI")),
            "Claude (Z.AI)"
        );
    }

    #[test]
    fn visible_agent_state_mapping_uses_protocol_state() {
        assert_eq!(
            visible_agent_state_from_protocol(AgentState::Idle),
            VisibleAgentState::Idle
        );
        assert_eq!(
            visible_agent_state_from_protocol(AgentState::Working),
            VisibleAgentState::Working
        );
        assert_eq!(
            visible_agent_state_from_protocol(AgentState::Done),
            VisibleAgentState::Done
        );
        assert_eq!(
            visible_agent_state_from_protocol(AgentState::Blocked),
            VisibleAgentState::Blocked
        );
    }

    #[test]
    fn visible_tab_pane_kind_uses_tui_agent_labeling() {
        assert_eq!(
            visible_tab_pane_kind(VisibleTabPaneFacts {
                agent_slug: Some("claude"),
                provider_label: Some("Z.AI"),
            }),
            VisibleTabPaneKind::Agent("Claude (Z.AI)".into())
        );
        assert_eq!(
            visible_tab_pane_kind(VisibleTabPaneFacts {
                agent_slug: None,
                provider_label: Some("ignored"),
            }),
            VisibleTabPaneKind::Shell
        );
    }
}
