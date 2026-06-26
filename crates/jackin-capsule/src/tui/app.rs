//! Visible multiplexer TUI model vocabulary.
//!
//! The daemon still owns PTY/session/control-plane authority. Small visible
//! state enums live here so hover and pointer rendering share the TUI boundary
//! instead of being defined in daemon internals.

use crate::protocol::AgentState;
use crate::tui::components::branch_context_bar::BranchContextBarHit;
use crate::tui::layout::{Rect, SplitOrient, Tab};
pub(crate) use jackin_tui::PointerShape;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointerShapeState {
    pub(crate) dragging: bool,
    pub(crate) selecting: bool,
    pub(crate) chrome_target: Option<HoverTarget>,
    pub(crate) dialog_open: bool,
    pub(crate) drag_start_orient: Option<SplitOrient>,
    pub(crate) selection_start_available: bool,
    pub(crate) link_target_available: bool,
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
    if state.link_target_available {
        return PointerShape::Pointer;
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
    UsageStatus,
    Container,
    /// The red debug run-id chip at the bottom-right when `--debug` is active.
    DebugChip,
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
        Some(BranchContextBarHit::UsageStatus) => Some(HoverTarget::UsageStatus),
        Some(BranchContextBarHit::Container) => Some(HoverTarget::Container),
        Some(BranchContextBarHit::DebugChip) => Some(HoverTarget::DebugChip),
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
        return "Shell".to_owned();
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
        }];
    }
    let Some(tab) = active_tab else {
        return Vec::new();
    };
    let leaves = tab.tree.leaves(content_rect);
    leaves
        .into_iter()
        .map(|(id, outer)| {
            // Subdivision must never escape the content rect (a pane top can't
            // rise above `content_rect.row` == `STATUS_BAR_ROWS` into the status
            // bar). Production correctness rests on the `leaves()` split
            // arithmetic, which enforces this structurally; this assert is a
            // debug/test tripwire that catches a regression in that arithmetic
            // (release builds drop it, and the `frame-pane` trace is firehose-
            // gated, so neither fires in a normal release run).
            debug_assert!(
                content_rect.contains(outer),
                "pane {id} outer rect {outer:?} escaped content_rect {content_rect:?}",
            );
            let focused = Some(id) == focused_id;
            VisiblePane {
                id,
                outer,
                inner: outer.shrink(1),
                focused,
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
        (0, _) => "Shell".to_owned(),
        (1, false) => agent_labels[0].clone(),
        (_, false) => "Agents".to_owned(),
        (_, true) => "Mix".to_owned(),
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
mod tests;
