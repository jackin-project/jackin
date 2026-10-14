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

    Split,
    ZoomPane,

    ExportFile,

    ExportFileAndReveal,

    ExportFileAndOpen,

    ExportFileUnderCursor,

    ExportFileUnderCursorAndReveal,

    ExportFileUnderCursorAndOpen,

    ExportSelectedFile,

    ExportSelectedFileAndReveal,

    ExportSelectedFileAndOpen,

    StageImageFromClipboardPath,

    PasteImageFromClipboard,

    StageImageFromClipboard,

    OpenLinkUnderCursor,

    Close,
    ClearPane,
    Usage,
    Exit,
}

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
