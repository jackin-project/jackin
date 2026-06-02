//! Capsule TUI update-layer vocabulary.
//!
//! The daemon still drives most state transitions while the TUI boundary is
//! being extracted. Redraw reasons live here because they describe visible
//! invalidation causes, not PTY/session authority.

use crate::tui::components::dialog::DialogAction;
use crate::tui::input::PrefixCommand;
use crate::tui::layout::{Rect, SplitOrient};

/// Duration for transient "copied" feedback in TUI dialogs.
pub(crate) const DIALOG_COPY_FEEDBACK_DURATION: std::time::Duration =
    std::time::Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FullRedrawReason {
    FirstAttach,
    Resize,
    TabSwitch,
    LayoutChange,
    SplitClose,
    ZoomChange,
    ScrollbackMovement,
    DialogChange,
    SelectionRepaint,
    PaletteOverlay,
    FocusChange,
    SessionExit,
    PaneClear,
    ExplicitRedraw,
    StatusChange,
    PaneCacheMiss,
    UnsafePartial,
}

impl FullRedrawReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FirstAttach => "first-attach",
            Self::Resize => "resize",
            Self::TabSwitch => "tab-switch",
            Self::LayoutChange => "layout-change",
            Self::SplitClose => "split-close",
            Self::ZoomChange => "zoom-change",
            Self::ScrollbackMovement => "scrollback-movement",
            Self::DialogChange => "dialog-change",
            Self::SelectionRepaint => "selection-repaint",
            Self::PaletteOverlay => "palette-overlay",
            Self::FocusChange => "focus-change",
            Self::SessionExit => "session-exit",
            Self::PaneClear => "pane-clear",
            Self::ExplicitRedraw => "explicit-redraw",
            Self::StatusChange => "status-change",
            Self::PaneCacheMiss => "pane-cache-miss",
            Self::UnsafePartial => "unsafe-partial",
        }
    }
}

pub(crate) fn prefix_full_redraw_reason(cmd: &PrefixCommand) -> FullRedrawReason {
    match cmd {
        PrefixCommand::NewTab | PrefixCommand::Palette => FullRedrawReason::PaletteOverlay,
        PrefixCommand::NextTab | PrefixCommand::PrevTab | PrefixCommand::JumpTab(_) => {
            FullRedrawReason::TabSwitch
        }
        PrefixCommand::SplitTopBottom | PrefixCommand::SplitSideBySide => {
            FullRedrawReason::LayoutChange
        }
        PrefixCommand::MoveFocus(_) => FullRedrawReason::FocusChange,
        PrefixCommand::ZoomToggle => FullRedrawReason::ZoomChange,
        PrefixCommand::KillPane | PrefixCommand::KillTab => FullRedrawReason::SplitClose,
        PrefixCommand::ClearPane => FullRedrawReason::PaneClear,
        PrefixCommand::Detach | PrefixCommand::Redraw => FullRedrawReason::ExplicitRedraw,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoverFramePlan {
    DialogOverlay(FullRedrawReason),
    ChromeHover,
}

pub(crate) fn hover_frame_plan(dialog_open: bool) -> HoverFramePlan {
    if dialog_open {
        HoverFramePlan::DialogOverlay(FullRedrawReason::DialogChange)
    } else {
        HoverFramePlan::ChromeHover
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogActionFramePlan {
    Full(FullRedrawReason),
    Overlay(FullRedrawReason),
}

pub(crate) fn dialog_action_frame_plan(action: &DialogAction) -> DialogActionFramePlan {
    if matches!(action, DialogAction::CopyToClipboard(_)) {
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    } else {
        DialogActionFramePlan::Full(FullRedrawReason::DialogChange)
    }
}

pub(crate) fn drag_resize_ratio(orient: SplitOrient, rect: Rect, row: u16, col: u16) -> f32 {
    match orient {
        SplitOrient::Horizontal => {
            let off = col.saturating_sub(rect.col);
            (off as f32 / rect.cols as f32).clamp(0.05, 0.95)
        }
        SplitOrient::Vertical => {
            let off = row.saturating_sub(rect.row);
            (off as f32 / rect.rows as f32).clamp(0.05, 0.95)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartialFramePlan {
    Empty,
    OverlayDiff,
    Full(FullRedrawReason),
    Partial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PartialFrameState {
    pub(crate) dirty_empty: bool,
    pub(crate) overlay_active: bool,
    pub(crate) any_dirty_visible_pane: bool,
    pub(crate) dirty_pane_scrollback_active: bool,
    pub(crate) dirty_pane_cache_invalid: bool,
}

pub(crate) fn partial_frame_plan(state: PartialFrameState) -> PartialFramePlan {
    if state.dirty_empty {
        PartialFramePlan::Empty
    } else if state.overlay_active {
        PartialFramePlan::OverlayDiff
    } else if !state.any_dirty_visible_pane {
        PartialFramePlan::Empty
    } else if state.dirty_pane_scrollback_active {
        PartialFramePlan::Full(FullRedrawReason::ScrollbackMovement)
    } else if state.dirty_pane_cache_invalid {
        PartialFramePlan::Full(FullRedrawReason::PaneCacheMiss)
    } else {
        PartialFramePlan::Partial
    }
}

pub(crate) fn pane_data_redraw_reason(
    snapped_scrollback: bool,
    unblocked_operator_input: bool,
) -> Option<FullRedrawReason> {
    if snapped_scrollback {
        Some(FullRedrawReason::ScrollbackMovement)
    } else if unblocked_operator_input {
        Some(FullRedrawReason::ExplicitRedraw)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DialogActionFramePlan, HoverFramePlan, PartialFramePlan, PartialFrameState,
        dialog_action_frame_plan, drag_resize_ratio, hover_frame_plan, pane_data_redraw_reason,
        partial_frame_plan, prefix_full_redraw_reason,
    };
    use crate::tui::components::dialog::{DialogAction, PickerIntent};
    use crate::tui::input::{ArrowDir, PrefixCommand};
    use crate::tui::layout::{Rect, SplitOrient};
    use crate::tui::update::FullRedrawReason;

    #[test]
    fn prefix_commands_map_to_visible_redraw_reasons() {
        assert_eq!(
            prefix_full_redraw_reason(&PrefixCommand::NewTab),
            FullRedrawReason::PaletteOverlay
        );
        assert_eq!(
            prefix_full_redraw_reason(&PrefixCommand::MoveFocus(ArrowDir::Right)),
            FullRedrawReason::FocusChange
        );
        assert_eq!(
            prefix_full_redraw_reason(&PrefixCommand::Detach),
            FullRedrawReason::ExplicitRedraw
        );
    }

    #[test]
    fn hover_frame_plan_uses_overlay_when_dialog_owns_screen() {
        assert_eq!(
            hover_frame_plan(true),
            HoverFramePlan::DialogOverlay(FullRedrawReason::DialogChange)
        );
        assert_eq!(hover_frame_plan(false), HoverFramePlan::ChromeHover);
    }

    #[test]
    fn dialog_action_frame_plan_keeps_copy_feedback_overlay_scoped() {
        assert_eq!(
            dialog_action_frame_plan(&DialogAction::CopyToClipboard("id".into())),
            DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
        );
        assert_eq!(
            dialog_action_frame_plan(&DialogAction::SpawnAgent {
                agent: None,
                intent: PickerIntent::NewTab,
            }),
            DialogActionFramePlan::Full(FullRedrawReason::DialogChange)
        );
    }

    #[test]
    fn drag_resize_ratio_clamps_to_visible_resize_bounds() {
        let rect = Rect::new(2, 4, 20, 100);
        assert_eq!(
            drag_resize_ratio(SplitOrient::Horizontal, rect, 2, 0),
            0.05
        );
        assert_eq!(
            drag_resize_ratio(SplitOrient::Horizontal, rect, 2, 200),
            0.95
        );
        assert_eq!(
            drag_resize_ratio(SplitOrient::Horizontal, rect, 2, 54),
            0.5
        );

        let rect = Rect::new(2, 4, 20, 100);
        assert_eq!(drag_resize_ratio(SplitOrient::Vertical, rect, 0, 4), 0.05);
        assert_eq!(drag_resize_ratio(SplitOrient::Vertical, rect, 40, 4), 0.95);
        assert_eq!(drag_resize_ratio(SplitOrient::Vertical, rect, 12, 4), 0.5);
    }

    #[test]
    fn partial_frame_plan_promotes_unsafe_cases_to_full_redraw() {
        let base = PartialFrameState {
            dirty_empty: false,
            overlay_active: false,
            any_dirty_visible_pane: true,
            dirty_pane_scrollback_active: false,
            dirty_pane_cache_invalid: false,
        };
        assert_eq!(partial_frame_plan(base), PartialFramePlan::Partial);
        assert_eq!(
            partial_frame_plan(PartialFrameState {
                dirty_empty: true,
                ..base
            }),
            PartialFramePlan::Empty
        );
        assert_eq!(
            partial_frame_plan(PartialFrameState {
                overlay_active: true,
                ..base
            }),
            PartialFramePlan::OverlayDiff
        );
        assert_eq!(
            partial_frame_plan(PartialFrameState {
                any_dirty_visible_pane: false,
                ..base
            }),
            PartialFramePlan::Empty
        );
        assert_eq!(
            partial_frame_plan(PartialFrameState {
                dirty_pane_scrollback_active: true,
                ..base
            }),
            PartialFramePlan::Full(FullRedrawReason::ScrollbackMovement)
        );
        assert_eq!(
            partial_frame_plan(PartialFrameState {
                dirty_pane_cache_invalid: true,
                ..base
            }),
            PartialFramePlan::Full(FullRedrawReason::PaneCacheMiss)
        );
    }

    #[test]
    fn pane_data_redraw_reason_prioritizes_scrollback_snap() {
        assert_eq!(
            pane_data_redraw_reason(true, true),
            Some(FullRedrawReason::ScrollbackMovement)
        );
        assert_eq!(
            pane_data_redraw_reason(false, true),
            Some(FullRedrawReason::ExplicitRedraw)
        );
        assert_eq!(pane_data_redraw_reason(false, false), None);
    }
}
