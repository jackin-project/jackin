//! Capsule TUI update-layer vocabulary.
//!
//! The daemon still drives most state transitions while the TUI boundary is
//! being extracted. Redraw reasons live here because they describe visible
//! invalidation causes, not PTY/session authority.

use crate::tui::input::PrefixCommand;

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
