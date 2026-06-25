//! Tests for `app`.
use crate::protocol::AgentState;
use crate::tui::components::branch_context_bar::BranchContextBarHit;
use crate::tui::layout::{PaneTree, Rect, SplitOrient, Tab};

use super::{
    ChromeHitState, CursorVisibilityState, HoverState, HoverTarget, MuxMode, MuxModeState,
    PointerShape, PointerShapeState, VisibleAgentState, VisibleTabPaneFacts, VisibleTabPaneKind,
    chrome_hover_target_for_state, cursor_visible_for_state, hover_target_for_state,
    mux_mode_for_state, pointer_shape_for_state, tab_auto_label, visible_agent_label,
    visible_agent_state_from_protocol, visible_panes_for_layout, visible_tab_pane_kind,
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
            branch_hit: Some(BranchContextBarHit::UsageStatus),
            ..base
        }),
        Some(HoverTarget::UsageStatus)
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
    let mut tab = Tab::new_single("tab", 1, "test");
    tab.tree = PaneTree::HSplit {
        left: Box::new(PaneTree::Leaf(1)),
        right: Box::new(PaneTree::Leaf(2)),
        ratio: 0.5,
    };

    let panes = visible_panes_for_layout(Rect::new(1, 0, 10, 20), Some(2), None, Some(&tab));

    assert_eq!(panes.len(), 2);
    assert_eq!(panes[0].id, 1);
    assert!(!panes[0].focused);
    assert_eq!(panes[1].id, 2);
    assert!(panes[1].focused);
}

#[test]
fn zoomed_visible_pane_uses_whole_content_rect() {
    let tab = Tab::new_single("tab", 1, "test");
    let panes = visible_panes_for_layout(Rect::new(1, 0, 10, 20), Some(1), Some(1), Some(&tab));

    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].id, 1);
    assert_eq!(panes[0].outer, Rect::new(1, 0, 10, 20));
    assert_eq!(panes[0].inner, Rect::new(2, 1, 8, 18));
    assert!(panes[0].focused);
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
