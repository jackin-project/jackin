//! Capsule TUI update-layer vocabulary.
//!
//! The daemon still drives most state transitions while the TUI boundary is
//! being extracted. Redraw reasons live here because they describe visible
//! invalidation causes, not PTY/session authority.

use crate::tui::components::dialog::DialogAction;
use crate::tui::input::PrefixCommand;
use crate::tui::layout::{Rect, SplitOrient};
use crate::tui::message::{Action, PaletteCommandRoute};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActionFramePlan {
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

pub(crate) fn action_frame_plan(action: &Action) -> Option<ActionFramePlan> {
    match action {
        Action::OpenPalette => Some(ActionFramePlan::Full(FullRedrawReason::PaletteOverlay)),
        Action::OpenContainerInfo | Action::OpenGithubContext => {
            Some(ActionFramePlan::Overlay(FullRedrawReason::DialogChange))
        }
        Action::OpenRenameTab(_) => Some(ActionFramePlan::Full(FullRedrawReason::DialogChange)),
        Action::OpenAgentPicker(_) => Some(ActionFramePlan::Full(FullRedrawReason::PaletteOverlay)),
        Action::SwitchTab(_) | Action::NextTab | Action::PreviousTab | Action::JumpTab(_) => {
            Some(ActionFramePlan::Full(FullRedrawReason::TabSwitch))
        }
        Action::SplitFocused(_) | Action::ResizePane(_) => {
            Some(ActionFramePlan::Full(FullRedrawReason::LayoutChange))
        }
        Action::MoveFocus(_) => Some(ActionFramePlan::Full(FullRedrawReason::FocusChange)),
        Action::ToggleZoom => Some(ActionFramePlan::Full(FullRedrawReason::ZoomChange)),
        Action::CloseFocusedPane | Action::CloseFocusedTab => {
            Some(ActionFramePlan::Full(FullRedrawReason::SplitClose))
        }
        Action::ClearFocusedPane => Some(ActionFramePlan::Full(FullRedrawReason::PaneClear)),
        Action::Detach => Some(ActionFramePlan::Full(FullRedrawReason::ExplicitRedraw)),
        _ => None,
    }
}

pub(crate) fn drag_resize_ratio(orient: SplitOrient, rect: Rect, row: u16, col: u16) -> f32 {
    match orient {
        SplitOrient::Horizontal => {
            let off = col.saturating_sub(rect.col);
            (f32::from(off) / f32::from(rect.cols)).clamp(0.05, 0.95)
        }
        SplitOrient::Vertical => {
            let off = row.saturating_sub(rect.row);
            (f32::from(off) / f32::from(rect.rows)).clamp(0.05, 0.95)
        }
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

pub(crate) fn focus_change_redraw_reason(focus_changed: bool) -> Option<FullRedrawReason> {
    focus_changed.then_some(FullRedrawReason::FocusChange)
}

pub(crate) fn drag_resize_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::LayoutChange
}

pub(crate) fn first_attach_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::FirstAttach
}

pub(crate) fn resize_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::Resize
}

pub(crate) fn session_exit_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::SessionExit
}

pub(crate) fn status_change_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::StatusChange
}

pub(crate) fn dialog_change_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::DialogChange
}

pub(crate) fn explicit_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::ExplicitRedraw
}

pub(crate) fn selection_change_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::SelectionRepaint
}

#[cfg(test)]
pub(crate) fn wheel_scrollback_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::ScrollbackMovement
}

pub(crate) fn selection_start_redraw_reason(selection_started: bool) -> Option<FullRedrawReason> {
    selection_started.then_some(FullRedrawReason::SelectionRepaint)
}

pub(crate) fn palette_route_redraw_reason(route: PaletteCommandRoute) -> Option<FullRedrawReason> {
    match route {
        PaletteCommandRoute::ClearPane => Some(FullRedrawReason::PaneClear),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
