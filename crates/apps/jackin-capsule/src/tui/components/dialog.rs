// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "re-export for dialog tests via super::*")
)]
pub(crate) use crate::pull_request::PullRequestInfo;

pub use github_context::{GithubContextView, PullRequestStatus, github_context_view_from_state};

pub use usage::UsageDialogTab;

pub use super::container_info_dialog::ContainerInfoDiagnostics;
pub(super) use super::palette::PALETTE_ITEMS;
#[cfg(test)]
pub(super) use super::palette::palette_filtered_indices;
pub use super::palette::{PaletteCloseLabel, PaletteCommand};

const PALETTE_WIDTH: u16 = 50;
const CONTAINER_INFO_WIDTH: u16 = 86;
const GITHUB_URL_ROW: usize = 3;
const GITHUB_OPEN_PR_ROW: usize = 5;
const GITHUB_OPEN_CI_ROW: usize = 6;
pub(crate) const USAGE_IDENTITY_PROVIDER_ROW: &str = "Identity provider";
pub(crate) const USAGE_IDENTITY_ACCOUNT_ROW: &str = "Identity account";
pub(crate) const USAGE_IDENTITY_ACTIVITY_ROW: &str = "Identity activity";

fn file_url_path(href: &str) -> Option<&str> {
    href.strip_prefix("file://").filter(|path| !path.is_empty())
}
mod input;
#[cfg(test)]
use input::{PickerRow, picker_filtered_rows};

mod hint;
pub(crate) use hint::main_view_hint;
mod constructors;
mod container_info;
mod geometry;
mod github_context;
mod usage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerIntent {
    NewTab,
    Split(SplitDirection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Left,
    Right,
    Above,
    Below,
}

impl SplitDirection {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "← Left",
            Self::Right => "→ Right",
            Self::Above => "↑ Above",
            Self::Below => "↓ Below",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnFailureState {
    pub title: String,
    pub message: String,
}

impl SpawnFailureState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}

pub const MAX_CUSTOM_LABEL_LEN: usize = 16;

#[derive(Debug, Clone)]
pub enum Dialog {
    CommandPalette {
        selected: usize,
        filter: String,
        close_label: PaletteCloseLabel,
    },
    AgentPicker {
        agents: Vec<String>,
        selected: usize,
        intent: PickerIntent,
        filter: String,
    },
    RenameTab {
        tab_idx: usize,
        input: termrock::widgets::TextInputState,
    },
    ExportFile {
        input: termrock::widgets::TextInputState,
        reveal_after_export: bool,
        open_after_export: bool,
    },
    ContainerInfo {
        container_name: String,
        role: String,
        focused_agent: Option<String>,
        workdir: String,
        diagnostics: ContainerInfoDiagnostics,
        copied_row: Option<usize>,
        hovered_row: Option<usize>,
        scroll: termrock::scroll::DialogScroll,
    },
    GitHubContext {
        copied: bool,
        scroll: termrock::scroll::DialogScroll,
    },
    Usage {
        view: Box<jackin_protocol::control::FocusedUsageView>,
        selected: UsageDialogTab,
        tab_bar_focused: bool,
        hovered_tab: Option<usize>,
        scroll: termrock::scroll::DialogScroll,
    },
    SpawnFailure(SpawnFailureState),

    SplitDirectionPicker {
        selected: usize,
        filter: String,
    },

    CloseTargetPicker {
        selected: usize,
        filter: String,
    },

    ConfirmAction {
        kind: ConfirmKind,
        selected_yes: bool,
    },
    ExecPicker(crate::exec::ExecPickerState),
    ExitDirty {
        summary: Vec<String>,
        selected: usize,
        inspect_rows: Arc<[InspectRow]>,
    },
    ExitInspect {
        lines: Arc<[InspectRow]>,
        selected: usize,
    },
}

impl Dialog {
    pub fn set_usage_tab_hover(
        &mut self,
        row: u16,
        col: u16,
        term_rows: u16,
        term_cols: u16,
    ) -> bool {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let area = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        let hit = match self {
            Self::Usage { view, selected, .. } => {
                Self::usage_tab_index_at(view, *selected, area, row, col)
            }
            _ => None,
        };
        if let Self::Usage { hovered_tab, .. } = self
            && *hovered_tab != hit
        {
            *hovered_tab = hit;
            return true;
        }
        false
    }

    pub fn clear_copy_feedback(&mut self) -> bool {
        match self {
            Self::ContainerInfo { copied_row, .. } => {
                let was = copied_row.is_some();
                *copied_row = None;
                was
            }
            Self::GitHubContext { copied, .. } => {
                let was = *copied;
                *copied = false;
                was
            }
            _ => false,
        }
    }

    #[must_use]
    pub fn has_copy_feedback(&self) -> bool {
        matches!(
            self,
            Self::ContainerInfo {
                copied_row: Some(_),
                ..
            } | Self::GitHubContext { copied: true, .. }
        )
    }
}

mod actions;
mod click;
mod filter_keys;
mod hits;
mod key_dispatch;
pub(crate) use actions::{CLOSE_TARGET_ITEMS, SPLIT_DIRECTION_ITEMS};
pub use actions::{ConfirmKind, DialogAction, EXIT_DIRTY_ROWS, ExitDirtyRow, InspectRow};

#[cfg(test)]
mod tests;
