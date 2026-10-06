// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Dialog components: modal overlays for the capsule TUI (tab rename,
//! confirm, error, help, and the Ctrl+J command palette).
//!
//! Not responsible for: input dispatch to focused dialogs (handled in
//! `tui::run`) or dialog stack ordering.
//!
//! Key invariant: dialogs render as centered floating overlays composed on top
//! of the fully-rendered frame; they do not own PTY or tab state.

/// Ctrl+J command palette and agent picker modal.
///
/// The dialog renders as a centred floating overlay on top of the
/// composed frame. Visual contract mirrors the jackin console TUI's
/// left sidebar (`render_role_picker_sidebar` in
/// `src/console/manager/render/list.rs`):
///
/// - **Phosphor palette** — same RGB values as the console:
///   `accent_fg()` rgb(0,255,65) (list text + selection bg),
///   `muted_fg()` rgb(0,140,30) (dim labels), `scroll_track_fg()`
///   rgb(0,80,18) (border + separator), `text_fg()` rgb(255,255,255)
///   (title + hotkey glyphs).
/// - **Selection** uses a green highlight bar with black text and the
///   `▸ ` highlight symbol — identical to the role picker sidebar.
/// - **Hint footer** follows the console TUI's structured format:
///   `Key text_fg()+BOLD`, label `accent_fg()`, dot separator
///   `scroll_track_fg()`, three-space group gap between logical groups.
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
    /// Spawn the chosen agent / shell as a brand-new tab.
    NewTab,
    /// Split the focused pane in the carried direction and spawn the
    /// chosen agent / shell in the new pane.
    Split(SplitDirection),
}

/// Which side of the focused pane the operator wants the new pane on
/// after a Split. Maps deterministically to `(PaneTree::split_h or
/// split_v, SplitPosition)` in `Multiplexer::split_focused_into`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Left,
    Right,
    Above,
    Below,
}

impl SplitDirection {
    /// Operator-facing label for the `SplitDirectionPicker` rows and
    /// the menu hint footer. Glyphs match the cardinal arrows the
    /// operator presses to reach equivalent panes after the split.
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

/// Cap on operator-typed tab labels. Long names break the tab-strip
/// layout (each tab cell grows with its label width), so the input
/// stops accepting characters past this limit. 16 is enough for the
/// agent names (`OpenCode`) plus a short qualifier the operator picks.
pub const MAX_CUSTOM_LABEL_LEN: usize = 16;

#[derive(Debug, Clone)]
pub enum Dialog {
    /// Type-to-filter list. Typing printable characters narrows the
    /// visible items by case-insensitive substring match on the label;
    /// `selected` indexes into the *filtered* list so arrows + Enter
    /// always act on what the operator sees. Esc / Ctrl+C dismiss
    /// (the `q` / Backspace dismiss shortcuts that the read-only
    /// dialogs use would conflict with typing into the filter).
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
    /// Text-input modal opened when the operator double-clicks a tab.
    /// `tab_idx` records which tab to rename. `input` reuses the
    /// shared `termrock::widgets::TextInputState` so the buffer + cursor + max
    /// length live in the same place as the console TUI text input. Enter
    /// commits; Esc cancels; empty input clears any previous custom
    /// label so the tab returns to auto-naming.
    RenameTab {
        tab_idx: usize,
        input: termrock::widgets::TextInputState,
    },
    /// Text-input modal opened from the command palette. The operator
    /// types a workspace-relative path, workspace absolute path, or a
    /// `/jackin/run/` path; the daemon validates and transfers it over
    /// the host attach protocol.
    ExportFile {
        input: termrock::widgets::TextInputState,
        reveal_after_export: bool,
        open_after_export: bool,
    },
    /// Read-only modal opened when the operator clicks the
    /// container-name segment of the bottom branch/PR context bar.
    /// Surfaces role key, focused-agent runtime, full container ID,
    /// and workspace path with shared copy-to-clipboard affordances.
    /// Enter copies the shared default row (Invocation ID when available) and
    /// clicks copy whichever copyable value was hit. The dialog stays
    /// open so copied-row feedback can render. Esc / q / a click
    /// outside the box dismisses. `focused_agent` is the slug of
    /// whichever pane is active when the modal opens — `Some("claude")`,
    /// `Some("kimi")`, … or `None` for a plain shell pane.
    ContainerInfo {
        container_name: String,
        role: String,
        focused_agent: Option<String>,
        workdir: String,
        diagnostics: ContainerInfoDiagnostics,
        /// Index of the row whose value was just copied (shows a check affordance),
        /// or `None`. Indexes into the shared `ContainerInfoState` rows.
        copied_row: Option<usize>,
        /// Index of the copyable row under the pointer (link hover colour).
        hovered_row: Option<usize>,
        /// Persisted scroll offsets. The shared `ContainerInfoState` is rebuilt
        /// every frame, so the scroll must live here on the dialog enum to
        /// survive across redraws.
        scroll: termrock::scroll::DialogScroll,
    },
    /// Read-only modal opened from the bottom branch/PR context.
    /// Branch / PR / loading state come from `GithubContextView` at
    /// render time so a mid-life branch flip reflects without an
    /// explicit refresh step.
    GitHubContext {
        copied: bool,
        /// Persisted scroll offsets (rebuilt each frame like `ContainerInfo`).
        scroll: termrock::scroll::DialogScroll,
    },
    /// Read-only usage/quota modal for the focused pane.
    Usage {
        view: Box<jackin_protocol::control::FocusedUsageView>,
        selected: UsageDialogTab,
        tab_bar_focused: bool,
        hovered_tab: Option<usize>,
        scroll: termrock::scroll::DialogScroll,
    },
    /// Operator-facing spawn failure surfaced through the shared error popup.
    /// This is intentionally modal: Enter / Esc / O dismiss, while unrelated
    /// printable input is consumed so the reason cannot vanish unread.
    SpawnFailure(SpawnFailureState),
    /// Direction sub-dialog opened when the operator picks "Split pane"
    /// in the main menu. Operator chooses Left / Right / Above / Below;
    /// on confirm, the dialog is replaced with an `AgentPicker` carrying
    /// `PickerIntent::Split(<direction>)` so the standard agent-pick
    /// flow finishes the spawn. Filterable just like the other list
    /// dialogs (`selected` indexes into the filtered visible list).
    SplitDirectionPicker { selected: usize, filter: String },
    /// Sub-dialog opened from `PaletteCommand::Close`. Operator picks
    /// whether they want to close the focused pane or the entire tab;
    /// each confirm path then opens a `ConfirmAction` dialog so a
    /// stray click on "Close" can be walked back via Esc instead of
    /// destroying the operator's work.
    CloseTargetPicker { selected: usize, filter: String },
    /// Yes / No confirmation dialog for irreversible actions (close
    /// pane, close tab, exit). Default selection is `No` so an
    /// operator who hit the action by reflex returns to the previous
    /// step on Enter instead of executing. `Y` / `y` shortcut always
    /// confirms; `N` / `n` / Esc always cancels.
    ConfirmAction {
        kind: ConfirmKind,
        selected_yes: bool,
    },
    /// Operator credential picker for a `jackin-exec` invocation. The daemon
    /// builds it from the workspace's on-demand bindings, stashes the control
    /// reply channel, and drives confirm/cancel through `DialogAction`. Space
    /// toggles the row under the cursor, ↑/↓ move, Enter confirms (resolve the
    /// selected credentials + run the command), Esc cancels (deny, run nothing).
    ExecPicker(crate::exec::ExecPickerState),
    /// Last-session dirty-exit modal (in-capsule). Shows a per-repo summary plus
    /// the four choice rows. `Esc` is ignored — the operator must pick a row.
    ExitDirty {
        /// One summary line per dirty repo (e.g. `jackin   2 changed · 1 unpushed`).
        summary: Vec<String>,
        /// Focused choice row, `0..EXIT_DIRTY_ROWS.len()`.
        selected: usize,
        /// Pre-built Inspect rows (section header + file rows per repo). Shared
        /// with `ExitInspect` via `Arc` so opening Inspect is a ref-count bump.
        inspect_rows: Arc<[InspectRow]>,
    },
    /// Read-only changed-files list opened from the `ExitDirty` modal's Inspect
    /// row. `Esc` walks back to the exit modal (modal stack).
    ExitInspect {
        /// Changed-file rows grouped by repo via section headers.
        lines: Arc<[InspectRow]>,
        /// Focused row for scrolling.
        selected: usize,
    },
}

impl Dialog {
    /// Footer hint spans for this dialog. Rendered by the multiplexer
    /// compositor near the bottom chrome so every dialog follows the same
    /// hint contract without competing with the branch/container status row.
    ///
    /// `axes` reflects the dialog body's *actual* per-axis overflow (computed
    /// by the caller from the rendered snapshot + rect), so the scrollable info
    /// dialogs advertise only the scroll direction(s) the operator can move —
    /// never both axes when the body fits one.
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

    /// Clear transient copy feedback after the daemon-side timer
    /// expires. Returns true only when the visible dialog changed.
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
mod hit_test;
mod key_dispatch;
pub(crate) use actions::{CLOSE_TARGET_ITEMS, SPLIT_DIRECTION_ITEMS};
pub use actions::{ConfirmKind, DialogAction, EXIT_DIRTY_ROWS, ExitDirtyRow, InspectRow};

#[cfg(test)]
mod tests;
