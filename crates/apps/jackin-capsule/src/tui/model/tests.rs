// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

mod case_01;
