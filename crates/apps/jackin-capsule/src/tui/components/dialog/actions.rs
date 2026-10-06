// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dialog action vocabulary: confirm rows, choices, and `DialogAction`.

use super::{PickerIntent, SplitDirection};

use crate::tui::components::palette::PaletteCommand;

/// The four selectable rows of the dirty-exit modal, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDirtyRow {
    /// Open the verbatim New-tab agent picker and return to work.
    StartNewAgent,
    /// Open the read-only changed-files Inspect view.
    Inspect,
    /// Exit; the host preserves the instance as resumable dirty state.
    Keep,
    /// Exit; the host discards the instance and its dirty work.
    Discard,
}

/// The exit modal's choice rows in display order, with their labels.
pub const EXIT_DIRTY_ROWS: [(ExitDirtyRow, &str); 4] = [
    (ExitDirtyRow::StartNewAgent, "Start a new agent"),
    (ExitDirtyRow::Inspect, "Inspect changes"),
    (ExitDirtyRow::Keep, "Exit & keep changes"),
    (ExitDirtyRow::Discard, "Exit & discard changes"),
];

/// One row of the read-only dirty-exit Inspect list — a repo header or a
/// changed-file line. A public type so the `Dialog` API does not leak the
/// crate-private `PickerItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectRow {
    /// Repo section header.
    Repo(String),
    /// A `<status> <path>` changed-file line.
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
    /// User confirmed a command-palette item.
    Command(PaletteCommand),
    /// User picked a split direction in the `SplitDirectionPicker` —
    /// daemon opens an `AgentPicker` with `PickerIntent::Split(direction)`.
    SplitDirection(SplitDirection),
    /// User picked a close target in the `CloseTargetPicker` — daemon
    /// opens a `ConfirmAction` dialog for the chosen `kind`.
    PickedCloseTarget(ConfirmKind),
    /// User said "Yes" in a `ConfirmAction` dialog — daemon fires
    /// the matching action (close focused pane, close focused tab,
    /// exit every session).
    ConfirmedAction(ConfirmKind),
    /// User picked an agent slug (or "shell"). `intent` tells the
    /// daemon whether to spawn it as a tab or as a split pane.
    SpawnAgent {
        agent: Option<String>,
        intent: PickerIntent,
    },
    /// Operator typed a new tab label and pressed Enter. Empty
    /// `label` clears the existing custom label and re-enables
    /// auto-naming.
    RenameTab { tab_idx: usize, label: String },
    /// Operator typed a path for explicit host file export.
    ExportFile {
        path: String,
        reveal_after_export: bool,
        open_after_export: bool,
    },
    /// Operator clicked or pressed Enter on the `ContainerInfo` copy
    /// target — copy the carried payload to the operator's clipboard
    /// via OSC 52 and keep the dialog open for visible feedback.
    /// Carrying the
    /// payload through the action (rather than the daemon re-deriving
    /// it from the dialog) keeps the dialog the single source of
    /// truth for what gets copied.
    CopyToClipboard(String),
    /// Operator picked a row in the dirty-exit modal. The daemon opens the
    /// agent picker, opens Inspect, or records keep/discard and drains.
    ExitDirty(ExitDirtyRow),
    /// Ask the host attach client to open an allowlisted host URL.
    OpenHostUrl(String),
    /// Ask the host attach client to reveal an allowlisted jackin-owned host
    /// path. Host side validates the path before touching the OS.
    RevealHostPath(String),
    /// User dismissed with Escape.
    Dismiss,
    /// Request a daemon-side focused usage refresh.
    RefreshUsage,
    /// Request a daemon-side usage snapshot for a specific provider tab.
    /// `account_id` is the stable canonical account id and the resolution
    /// key; `provider_label` stays for display and old-payload back-compat
    /// (empty id falls back to label resolution).
    SwitchUsageProvider {
        provider_label: String,
        account_id: String,
    },
    /// Dialog is still open; redraw.
    Redraw,
    /// Operator confirmed a `jackin-exec` credential picker (Enter). Carries
    /// the command + the selected credentials; the daemon resolves them via the
    /// host socket, runs the command, and replies `ExecResult`.
    ExecConfirm {
        command: String,
        args: Vec<String>,
        selected: Vec<jackin_protocol::ExecBinding>,
    },
    /// Operator cancelled the `jackin-exec` picker (Esc) — daemon replies
    /// `ExecDenied` and runs nothing.
    ExecCancel,
    /// Mouse event lands somewhere with no semantic effect (border,
    /// padding row). Swallow it so it does not reach the focused pane.
    Consume,
}

/// Items in the `SplitDirectionPicker` sub-dialog. Prefer the common
/// forward/default placement first, then its opposite, then the
/// vertical pair. The dialog is filter-able like the other list
/// dialogs — typing `a` narrows to "Above," typing `l` narrows to
/// "Left," etc.
pub(crate) const SPLIT_DIRECTION_ITEMS: &[SplitDirection] = &[
    SplitDirection::Right,
    SplitDirection::Left,
    SplitDirection::Below,
    SplitDirection::Above,
];
