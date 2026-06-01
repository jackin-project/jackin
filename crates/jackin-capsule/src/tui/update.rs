//! Capsule TUI update-layer vocabulary.
//!
//! The daemon still drives most state transitions while the TUI boundary is
//! being extracted. Redraw reasons live here because they describe visible
//! invalidation causes, not PTY/session authority.

use crate::tui::input::PrefixCommand;
use crate::tui::layout::{Rect, SplitOrient};

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

#[cfg(test)]
mod tests {
    use super::{drag_resize_ratio, prefix_full_redraw_reason};
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
}
