//! Capsule TUI update-layer vocabulary.
//!
//! The daemon still drives most state transitions while the TUI boundary is
//! being extracted. Redraw reasons live here because they describe visible
//! invalidation causes, not PTY/session authority.

use crate::tui::components::dialog::{ConfirmKind, DialogAction, PickerIntent};
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
    /// PTY bytes arrived for a pane. Telemetry-only label: under derived
    /// rendering every invalidation reason produces the same composed frame.
    PtyOutput,
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
            Self::PtyOutput => "pty-output",
        }
    }
}

pub(crate) fn prefix_full_redraw_reason(cmd: &PrefixCommand) -> FullRedrawReason {
    match cmd {
        PrefixCommand::NewTab | PrefixCommand::Palette | PrefixCommand::Usage => {
            FullRedrawReason::PaletteOverlay
        }
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

impl DialogActionFramePlan {
    /// Invalidation label under derived rendering: the Full/Overlay split no
    /// longer selects a compose path, only the telemetry reason survives.
    pub(crate) fn reason(self) -> FullRedrawReason {
        match self {
            Self::Full(reason) | Self::Overlay(reason) => reason,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActionFramePlan {
    Full(FullRedrawReason),
    Overlay(FullRedrawReason),
    Diff(FullRedrawReason),
}

impl ActionFramePlan {
    /// Invalidation label under derived rendering; see
    /// [`DialogActionFramePlan::reason`].
    pub(crate) fn reason(self) -> FullRedrawReason {
        match self {
            Self::Full(reason) | Self::Overlay(reason) | Self::Diff(reason) => reason,
        }
    }
}

pub(crate) fn dialog_action_frame_plan(action: &DialogAction) -> DialogActionFramePlan {
    match action {
        DialogAction::Command(_) => DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange),
        DialogAction::ConfirmedAction(kind) => match kind {
            ConfirmKind::ClosePane | ConfirmKind::CloseTab => {
                DialogActionFramePlan::Full(FullRedrawReason::SplitClose)
            }
            ConfirmKind::Exit => DialogActionFramePlan::Full(FullRedrawReason::SessionExit),
        },
        DialogAction::SpawnAgent { intent, .. }
        | DialogAction::SpawnAgentWithProvider { intent, .. } => match intent {
            PickerIntent::NewTab => DialogActionFramePlan::Full(FullRedrawReason::TabSwitch),
            PickerIntent::Split(_) => DialogActionFramePlan::Full(FullRedrawReason::LayoutChange),
        },
        DialogAction::RefreshUsage
        | DialogAction::SwitchUsageProvider { .. }
        | DialogAction::SplitDirection(_)
        | DialogAction::PickedCloseTarget(_)
        | DialogAction::RenameTab { .. }
        | DialogAction::ExportFile { .. }
        | DialogAction::CopyToClipboard(_)
        | DialogAction::ExitDirty(_)
        | DialogAction::OpenHostUrl(_)
        | DialogAction::RevealHostPath(_)
        | DialogAction::Dismiss
        | DialogAction::Redraw
        | DialogAction::Consume => DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange),
    }
}

pub(crate) fn action_frame_plan(action: &Action) -> Option<ActionFramePlan> {
    match action {
        Action::OpenPalette => Some(ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)),
        Action::OpenContainerInfo | Action::OpenGithubContext | Action::OpenUsage => {
            Some(ActionFramePlan::Overlay(FullRedrawReason::DialogChange))
        }
        Action::OpenRenameTab(_) => Some(ActionFramePlan::Overlay(FullRedrawReason::DialogChange)),
        Action::OpenAgentPicker(_) => {
            Some(ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay))
        }
        Action::SwitchTab(_) | Action::NextTab | Action::PreviousTab | Action::JumpTab(_) => {
            Some(ActionFramePlan::Full(FullRedrawReason::TabSwitch))
        }
        Action::SplitFocused(_) | Action::ResizePane(_) => {
            Some(ActionFramePlan::Full(FullRedrawReason::LayoutChange))
        }
        Action::MoveFocus(_) => Some(ActionFramePlan::Diff(FullRedrawReason::FocusChange)),
        Action::ToggleZoom => Some(ActionFramePlan::Full(FullRedrawReason::ZoomChange)),
        Action::CloseFocusedPane | Action::CloseFocusedTab => {
            Some(ActionFramePlan::Full(FullRedrawReason::SplitClose))
        }
        Action::ClearFocusedPane => Some(ActionFramePlan::Diff(FullRedrawReason::PaneClear)),
        Action::Detach => Some(ActionFramePlan::Full(FullRedrawReason::ExplicitRedraw)),
        Action::RefreshUsage => Some(ActionFramePlan::Overlay(FullRedrawReason::DialogChange)),
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

pub(crate) fn wheel_scrollback_redraw_reason() -> FullRedrawReason {
    FullRedrawReason::ScrollbackMovement
}

pub(crate) fn selection_start_redraw_reason(selection_started: bool) -> Option<FullRedrawReason> {
    selection_started.then_some(FullRedrawReason::SelectionRepaint)
}

pub(crate) fn palette_route_frame_plan(route: PaletteCommandRoute) -> ActionFramePlan {
    match route {
        PaletteCommandRoute::OpenSplitDirectionPicker
        | PaletteCommandRoute::OpenAgentPicker(_)
        | PaletteCommandRoute::ConfirmAction(_)
        | PaletteCommandRoute::OpenCloseTargetPicker
        | PaletteCommandRoute::OpenExportFileDialog { .. }
        | PaletteCommandRoute::OpenUsage => {
            ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
        }
        PaletteCommandRoute::NextTab | PaletteCommandRoute::PreviousTab => {
            ActionFramePlan::Full(FullRedrawReason::TabSwitch)
        }
        PaletteCommandRoute::ToggleZoom => ActionFramePlan::Full(FullRedrawReason::ZoomChange),
        PaletteCommandRoute::StageImageFromClipboardPath
        | PaletteCommandRoute::PasteImageFromClipboard
        | PaletteCommandRoute::StageImageFromClipboard
        | PaletteCommandRoute::ExportFileUnderCursor { .. }
        | PaletteCommandRoute::ExportSelectedFile { .. }
        | PaletteCommandRoute::OpenLinkUnderCursor => {
            ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
        }
        PaletteCommandRoute::ClearPane => ActionFramePlan::Diff(FullRedrawReason::PaneClear),
    }
}

#[cfg(test)]
mod tests;
