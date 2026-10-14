// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{PickerIntent, SplitDirection};

use crate::tui::components::palette::PaletteCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDirtyRow {
    StartNewAgent,

    Inspect,

    Keep,

    Discard,
}

pub const EXIT_DIRTY_ROWS: [(ExitDirtyRow, &str); 4] = [
    (ExitDirtyRow::StartNewAgent, "Start a new agent"),
    (ExitDirtyRow::Inspect, "Inspect changes"),
    (ExitDirtyRow::Keep, "Exit & keep changes"),
    (ExitDirtyRow::Discard, "Exit & discard changes"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectRow {
    Repo(String),

    File(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    ClosePane,
    CloseTab,
    Exit,
}

impl ConfirmKind {
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::ClosePane => "Close pane?",
            Self::CloseTab => "Close tab?",
            Self::Exit => "Exit?",
        }
    }

    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::ClosePane => "Reap the focused pane's agent. Unsaved state in that pane is lost.",
            Self::CloseTab => {
                "Reap every pane in this tab. Unsaved state across all panes is lost."
            }
            Self::Exit => "Stop all agents; jackin❯ will clean up.",
        }
    }
}

pub(crate) const CLOSE_TARGET_ITEMS: &[(ConfirmKind, &str)] = &[
    (ConfirmKind::ClosePane, "Close pane"),
    (ConfirmKind::CloseTab, "Close tab"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAction {
    Command(PaletteCommand),

    SplitDirection(SplitDirection),

    PickedCloseTarget(ConfirmKind),

    ConfirmedAction(ConfirmKind),

    SpawnAgent {
        agent: Option<String>,
        intent: PickerIntent,
    },

    RenameTab {
        tab_idx: usize,
        label: String,
    },

    ExportFile {
        path: String,
        reveal_after_export: bool,
        open_after_export: bool,
    },

    CopyToClipboard(String),

    ExitDirty(ExitDirtyRow),

    OpenHostUrl(String),

    RevealHostPath(String),

    Dismiss,

    RefreshUsage,

    SwitchUsageProvider {
        provider_label: String,
        account_id: String,
    },

    Redraw,

    ExecConfirm {
        command: String,
        args: Vec<String>,
        selected: Vec<jackin_protocol::ExecBinding>,
    },

    ExecCancel,

    Consume,
}

pub(crate) const SPLIT_DIRECTION_ITEMS: &[SplitDirection] = &[
    SplitDirection::Right,
    SplitDirection::Left,
    SplitDirection::Below,
    SplitDirection::Above,
];
