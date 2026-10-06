// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Palette and confirm routing: command, confirmed-action, and toggle routes.

use crate::tui::components::dialog::{ConfirmKind, PaletteCommand, PickerIntent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteCommandRoute {
    OpenSplitDirectionPicker,
    OpenAgentPicker(PickerIntent),
    NextTab,
    PreviousTab,
    ConfirmAction(ConfirmKind),
    OpenCloseTargetPicker,
    ToggleZoom,
    OpenExportFileDialog {
        reveal_after_export: bool,
        open_after_export: bool,
    },
    ExportFileUnderCursor {
        reveal_after_export: bool,
        open_after_export: bool,
    },
    ExportSelectedFile {
        reveal_after_export: bool,
        open_after_export: bool,
    },
    StageImageFromClipboardPath,
    PasteImageFromClipboard,
    StageImageFromClipboard,
    OpenLinkUnderCursor,
    ClearPane,
    OpenUsage,
}

pub(crate) fn palette_command_route(
    cmd: PaletteCommand,
    active_tab_pane_count: usize,
) -> PaletteCommandRoute {
    match cmd {
        PaletteCommand::Split => PaletteCommandRoute::OpenSplitDirectionPicker,
        PaletteCommand::NewTab => PaletteCommandRoute::OpenAgentPicker(PickerIntent::NewTab),
        PaletteCommand::NextTab => PaletteCommandRoute::NextTab,
        PaletteCommand::PrevTab => PaletteCommandRoute::PreviousTab,
        PaletteCommand::Close if active_tab_pane_count == 1 => {
            PaletteCommandRoute::ConfirmAction(ConfirmKind::CloseTab)
        }
        PaletteCommand::Close => PaletteCommandRoute::OpenCloseTargetPicker,
        PaletteCommand::ZoomPane => PaletteCommandRoute::ToggleZoom,
        PaletteCommand::ExportFile => PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: false,
            open_after_export: false,
        },
        PaletteCommand::ExportFileAndReveal => PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: true,
            open_after_export: false,
        },
        PaletteCommand::ExportFileAndOpen => PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: false,
            open_after_export: true,
        },
        PaletteCommand::ExportFileUnderCursor => PaletteCommandRoute::ExportFileUnderCursor {
            reveal_after_export: false,
            open_after_export: false,
        },
        PaletteCommand::ExportFileUnderCursorAndReveal => {
            PaletteCommandRoute::ExportFileUnderCursor {
                reveal_after_export: true,
                open_after_export: false,
            }
        }
        PaletteCommand::ExportFileUnderCursorAndOpen => {
            PaletteCommandRoute::ExportFileUnderCursor {
                reveal_after_export: false,
                open_after_export: true,
            }
        }
        PaletteCommand::ExportSelectedFile => PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: false,
            open_after_export: false,
        },
        PaletteCommand::ExportSelectedFileAndReveal => PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: true,
            open_after_export: false,
        },
        PaletteCommand::ExportSelectedFileAndOpen => PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: false,
            open_after_export: true,
        },
        PaletteCommand::StageImageFromClipboardPath => {
            PaletteCommandRoute::StageImageFromClipboardPath
        }
        PaletteCommand::PasteImageFromClipboard => PaletteCommandRoute::PasteImageFromClipboard,
        PaletteCommand::StageImageFromClipboard => PaletteCommandRoute::StageImageFromClipboard,
        PaletteCommand::OpenLinkUnderCursor => PaletteCommandRoute::OpenLinkUnderCursor,
        PaletteCommand::ClearPane => PaletteCommandRoute::ClearPane,
        PaletteCommand::Usage => PaletteCommandRoute::OpenUsage,
        PaletteCommand::Exit => PaletteCommandRoute::ConfirmAction(ConfirmKind::Exit),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmedActionRoute {
    ClosePane,
    CloseTab,
    ExitAllSessions,
}

pub(crate) fn confirmed_action_route(kind: ConfirmKind) -> ConfirmedActionRoute {
    match kind {
        ConfirmKind::ClosePane => ConfirmedActionRoute::ClosePane,
        ConfirmKind::CloseTab => ConfirmedActionRoute::CloseTab,
        ConfirmKind::Exit => ConfirmedActionRoute::ExitAllSessions,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteToggleRoute {
    CloseDialog,
    OpenPalette,
}

pub(crate) fn palette_toggle_route(dialog_open: bool) -> PaletteToggleRoute {
    if dialog_open {
        PaletteToggleRoute::CloseDialog
    } else {
        PaletteToggleRoute::OpenPalette
    }
}
