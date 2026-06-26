//! Named color palette for the capsule TUI: phosphor-green brand colors and
//! semantic aliases used across capsule components.
//!
//! Not responsible for: terminal capability detection or color downgrading —
//! all values are unconditional 24-bit RGB ANSI sequences.
//!
//! Key invariant: capsule components must source colors from `jackin_tui`
//! palette constants (shared with the host console TUI) so the two surfaces
//! cannot drift; no ad-hoc inline RGB literals in component render code.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCloseLabel {
    ChooseTarget,
    CloseTab,
}

impl PaletteCloseLabel {
    pub(crate) fn for_pane_count(count: usize) -> Self {
        if count == 1 {
            Self::CloseTab
        } else {
            Self::ChooseTarget
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ChooseTarget => "Close",
            Self::CloseTab => "Close tab",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteCommand {
    NewTab,
    NextTab,
    PrevTab,
    /// Open the `SplitDirectionPicker`. The operator picks Right /
    /// Left / Below / Above in the sub-dialog, then the agent
    /// picker for the new pane. Top-level entry is one item; the
    /// directional detail lives in the sub-dialog so the palette
    /// stays scannable.
    Split,
    ZoomPane,
    /// Export one workspace or `/jackin/run/` file to the host attach
    /// client's `~/Downloads/jackin/` directory.
    ExportFile,
    /// Export one file, then ask the host OS file manager to reveal the
    /// verified exported copy.
    ExportFileAndReveal,
    /// Export one file, then ask the host OS to open the verified
    /// exported copy.
    ExportFileAndOpen,
    /// Export the visible file token under the focused pane cursor.
    ExportFileUnderCursor,
    /// Export the visible file token under the focused pane cursor, then
    /// ask the host OS file manager to reveal the verified exported copy.
    ExportFileUnderCursorAndReveal,
    /// Export the visible file token under the focused pane cursor, then
    /// ask the host OS to open the verified exported copy.
    ExportFileUnderCursorAndOpen,
    /// Export the currently selected text as a file path.
    ExportSelectedFile,
    /// Export the currently selected text as a file path, then ask the host
    /// OS file manager to reveal the verified exported copy.
    ExportSelectedFileAndReveal,
    /// Export the currently selected text as a file path, then ask the host
    /// OS to open the verified exported copy.
    ExportSelectedFileAndOpen,
    /// Ask the host attach client to read the host clipboard as an
    /// absolute image path, stage that image into the container, and
    /// paste the staged container path into the focused pane.
    StageImageFromClipboardPath,
    /// Ask the host attach client to read an image directly from the
    /// host clipboard and paste the staged container path into the
    /// focused pane.
    PasteImageFromClipboard,
    /// Ask the host attach client to read an image directly from the
    /// host clipboard and stage it without pasting the staged path into
    /// the focused pane.
    StageImageFromClipboard,
    /// Open the host-open URL token currently under the focused pane's
    /// terminal cursor through the host attach client.
    OpenLinkUnderCursor,
    /// Close the active tab or open the `CloseTargetPicker` when the
    /// active tab has multiple panes. The chosen target then routes
    /// through `ConfirmAction` before the destructive call fires.
    Close,
    ClearPane,
    Usage,
    Exit,
}

/// Next/Previous tab are not exposed in the palette: the operator
/// already clicks tabs directly in the status bar, and the
/// keyboard-driven shortcut for cycle-tab is the tmux-style prefix
/// gesture (`Ctrl+B n` / `Ctrl+B p`). Keeping list entries that only
/// duplicate those existing paths bloats the modal with no new
/// capability. `PaletteCommand::NextTab` / `PrevTab` stay in the enum
/// so prefix-mode bindings continue to work.
pub(crate) const PALETTE_ITEMS: &[(PaletteCommand, &str)] = &[
    (PaletteCommand::NewTab, "New tab"),
    (PaletteCommand::Split, "Split pane"),
    (PaletteCommand::ZoomPane, "Zoom / unzoom pane"),
    (PaletteCommand::ExportFile, "Export file"),
    (
        PaletteCommand::ExportFileAndReveal,
        "Export file and reveal",
    ),
    (PaletteCommand::ExportFileAndOpen, "Export file and open"),
    (
        PaletteCommand::ExportFileUnderCursor,
        "Export file under cursor",
    ),
    (
        PaletteCommand::ExportFileUnderCursorAndReveal,
        "Export file under cursor and reveal",
    ),
    (
        PaletteCommand::ExportFileUnderCursorAndOpen,
        "Export file under cursor and open",
    ),
    (PaletteCommand::ExportSelectedFile, "Export selected file"),
    (
        PaletteCommand::ExportSelectedFileAndReveal,
        "Export selected file and reveal",
    ),
    (
        PaletteCommand::ExportSelectedFileAndOpen,
        "Export selected file and open",
    ),
    (
        PaletteCommand::StageImageFromClipboardPath,
        "Stage image from clipboard path",
    ),
    (
        PaletteCommand::PasteImageFromClipboard,
        "Paste image from host clipboard",
    ),
    (
        PaletteCommand::StageImageFromClipboard,
        "Stage image without pasting",
    ),
    (
        PaletteCommand::OpenLinkUnderCursor,
        "Open link under cursor",
    ),
    (PaletteCommand::ClearPane, "Clear pane"),
    (PaletteCommand::Usage, "Usage"),
    (PaletteCommand::Close, "Close"),
    (PaletteCommand::Exit, "Exit"),
];

pub(crate) fn palette_item_label(
    command: &PaletteCommand,
    label: &'static str,
    close_label: PaletteCloseLabel,
) -> &'static str {
    if matches!(command, PaletteCommand::Close) {
        close_label.label()
    } else {
        label
    }
}

pub(crate) fn palette_filtered_indices(filter: &str, close_label: PaletteCloseLabel) -> Vec<usize> {
    let needle = filter.to_ascii_lowercase();
    PALETTE_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, (command, label))| {
            let label = palette_item_label(command, label, close_label);
            needle.is_empty() || label.to_ascii_lowercase().contains(&needle)
        })
        .map(|(idx, _)| idx)
        .collect()
}
