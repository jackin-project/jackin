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
///   `PHOSPHOR_GREEN` rgb(0,255,65) (list text + selection bg),
///   `PHOSPHOR_DIM` rgb(0,140,30) (dim labels), `PHOSPHOR_DARK`
///   rgb(0,80,18) (border + separator), `WHITE` rgb(255,255,255)
///   (title + hotkey glyphs).
/// - **Selection** uses a green highlight bar with black text and the
///   `▸ ` highlight symbol — identical to the role picker sidebar.
/// - **Hint footer** follows the console TUI's structured format:
///   `Key WHITE+BOLD`, label `PHOSPHOR_GREEN`, dot separator
///   `PHOSPHOR_DARK`, three-space group gap between logical groups.
use std::sync::Arc;

#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "re-export for dialog tests via super::*")
)]
pub(crate) use crate::pull_request::PullRequestInfo;

pub use github_context::{GithubContextView, PullRequestStatus, github_context_view_from_state};

pub use usage::UsageDialogTab;

pub use super::container_info_dialog::ContainerInfoDiagnostics;
pub(super) use super::palette::{PALETTE_ITEMS, palette_filtered_indices};
pub use super::palette::{PaletteCloseLabel, PaletteCommand};

use crate::tui::components::modal_rects::{ModalRectSpec, modal_rect};
use crate::tui::keymap::raw_bytes_to_chord;
use termrock::components::{CONFIRM_KEYMAP, ConfirmAction as SharedConfirmAction};

use crate::tui::keymap::{FILTER_LIST_KEYMAP, FilterListAction, READ_ONLY_DISMISS_KEYMAP};

const PALETTE_WIDTH: u16 = 50;
const CONTAINER_INFO_WIDTH: u16 = 86;
const GITHUB_URL_ROW: usize = 3;
const GITHUB_OPEN_PR_ROW: usize = 5;
const GITHUB_OPEN_CI_ROW: usize = 6;

fn file_url_path(href: &str) -> Option<&str> {
    href.strip_prefix("file://").filter(|path| !path.is_empty())
}
mod input;
use input::{
    PickerRow, close_target_filtered_indices, dialog_list_row_clickable, exec_picker_handle_key,
    export_file_handle_key, first_selectable_idx, picker_filtered_rows, printable_filter_char,
    rename_tab_handle_key, split_direction_filtered_indices, step_selectable,
};
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
pub struct ProviderChoice {
    pub label: String,
}

impl ProviderChoice {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
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
    /// shared `termrock::components::TextField` so the buffer + cursor + max
    /// length live in the same place as the console TUI text input. Enter
    /// commits; Esc cancels; empty input clears any previous custom
    /// label so the tab returns to auto-naming.
    RenameTab {
        tab_idx: usize,
        input: termrock::components::TextField,
    },
    /// Text-input modal opened from the command palette. The operator
    /// types a workspace-relative path, workspace absolute path, or a
    /// `/jackin/run/` path; the daemon validates and transfers it over
    /// the host attach protocol.
    ExportFile {
        input: termrock::components::TextField,
        reveal_after_export: bool,
        open_after_export: bool,
    },
    /// Read-only modal opened when the operator clicks the
    /// container-name segment of the bottom branch/PR context bar.
    /// Surfaces role key, focused-agent runtime, full container ID,
    /// and workspace path with shared copy-to-clipboard affordances.
    /// Enter copies the shared default row (Run ID when available) and
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
        /// Index of the row whose value was just copied (shows "Copied!"),
        /// or `None`. Indexes into the shared `ContainerInfoState` rows.
        copied_row: Option<usize>,
        /// Index of the copyable row under the pointer (link hover colour).
        hovered_row: Option<usize>,
        /// Persisted scroll offsets. The shared `ContainerInfoState` is rebuilt
        /// every frame, so the scroll must live here on the dialog enum to
        /// survive across redraws.
        scroll: termrock::components::DialogBodyScroll,
    },
    /// Read-only modal opened from the bottom branch/PR context.
    /// Branch / PR / loading state come from `GithubContextView` at
    /// render time so a mid-life branch flip reflects without an
    /// explicit refresh step.
    GitHubContext {
        copied: bool,
        /// Persisted scroll offsets (rebuilt each frame like `ContainerInfo`).
        scroll: termrock::components::DialogBodyScroll,
    },
    /// Read-only usage/quota modal for the focused pane.
    Usage {
        view: Box<jackin_protocol::control::FocusedUsageView>,
        selected: UsageDialogTab,
        tab_bar_focused: bool,
        hovered_tab: Option<usize>,
        scroll: termrock::components::DialogBodyScroll,
    },
    /// Operator-facing spawn failure surfaced through the shared error popup.
    /// This is intentionally modal: Enter / Esc / O dismiss, while unrelated
    /// printable input is consumed so the reason cannot vanish unread.
    SpawnFailure(termrock::components::ErrorPopupState),
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
    /// Two-step provider picker shown after agent selection when multiple
    /// providers (e.g. Anthropic and Z.AI) are available. The daemon keeps
    /// the provider enum/env mapping; the dialog owns only visible labels.
    ProviderPicker {
        agent: Option<String>,
        providers: Vec<ProviderChoice>,
        selected: usize,
        intent: PickerIntent,
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
    pub fn title(self) -> &'static str {
        match self {
            Self::ClosePane => "Close pane?",
            Self::CloseTab => "Close tab?",
            Self::Exit => "Exit?",
        }
    }

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
    /// User confirmed a provider in the `ProviderPicker` — the daemon maps
    /// the chosen visible label back to provider/env facts.
    SpawnAgentWithProvider {
        agent: Option<String>,
        provider_label: String,
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
    SwitchUsageProvider { provider_label: String },
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

impl Dialog {
    /// Mutable body-scroll state for the read-only info dialogs whose content
    /// can overflow (`ContainerInfo`, `GitHubContext`). `None` for dialogs that do
    /// not scroll. Lets the daemon route mouse-wheel events to the dialog body.
    /// Handle a raw key byte and return the resulting action.
    #[expect(
        clippy::too_many_lines,
        reason = "Dialog key-event dispatcher with one arm per key binding. \
                  Each arm carries its focused state transition; extracting \
                  arms into sub-dispatchers would obscure per-binding readability."
    )]
    #[expect(
        clippy::excessive_nesting,
        reason = "Dialog key-event dispatcher: per-key + per-Dialog-variant nested \
                  with state-update branches. Modal nesting is the dispatch protocol."
    )]
    pub fn handle_key(
        &mut self,
        key: &[u8],
        github: Option<&GithubContextView<'_>>,
    ) -> DialogAction {
        // Text-input dialog has its own dismissal / editing rules and
        // must intercept keys before the arrow-key + dismiss-key
        // shortcuts below would steal them (e.g. `q` is a legal
        // character inside a custom tab name).
        if let Self::RenameTab { tab_idx, input } = self {
            return rename_tab_handle_key(*tab_idx, input, key);
        }
        // The exec credential picker is multi-select (Space toggles), so it
        // intercepts keys before the shared single-select arrow/dismiss logic.
        if let Self::ExecPicker(state) = self {
            return exec_picker_handle_key(state, key);
        }
        if let Self::ExportFile {
            input,
            reveal_after_export,
            open_after_export,
        } = self
        {
            return export_file_handle_key(input, *reveal_after_export, *open_after_export, key);
        }
        if let Self::SpawnFailure(state) = self {
            return match raw_bytes_to_chord(key)
                .and_then(|chord| termrock::components::ERROR_POPUP_KEYMAP.dispatch(chord))
            {
                Some(termrock::components::ErrorPopupAction::Dismiss) => DialogAction::Dismiss,
                None => {
                    // Touch the state so this branch remains explicitly tied to
                    // `ErrorPopupState`; printable input is consumed and does
                    // not reach the PTY behind the modal.
                    let _ = state;
                    DialogAction::Redraw
                }
            };
        }
        // Read-only info dialogs (ContainerInfo, GitHubContext): Esc /
        // dismiss keys close, Enter copies the dialog's value to the
        // operator's clipboard with the `copied` flag flipped to true
        // so the next render's "Copied!" indicator confirms the OSC 52
        // fired. The dialog stays open until dismissed so the feedback
        // is actually visible.
        if matches!(self, Self::Usage { .. }) {
            if matches!(key, b"r" | b"R") {
                return DialogAction::RefreshUsage;
            }
            if matches!(key, b"\t" | b"\x1b[Z") {
                if let Self::Usage {
                    tab_bar_focused, ..
                } = self
                {
                    *tab_bar_focused = !*tab_bar_focused;
                }
                return DialogAction::Redraw;
            }
            let tab_bar_focused = matches!(
                self,
                Self::Usage {
                    tab_bar_focused: true,
                    ..
                }
            );
            if tab_bar_focused {
                if raw_bytes_to_chord(key)
                    .and_then(|chord| READ_ONLY_DISMISS_KEYMAP.dispatch(chord))
                    .is_some()
                {
                    return DialogAction::Dismiss;
                }
                if let Some(provider_label) = match key {
                    b"\x1b[C" => self.usage_provider_tab_target(1),
                    b"\x1b[D" => self.usage_provider_tab_target(-1),
                    _ => None,
                } {
                    return DialogAction::SwitchUsageProvider { provider_label };
                }
                return DialogAction::Redraw;
            }
            if raw_bytes_to_chord(key)
                .and_then(|chord| READ_ONLY_DISMISS_KEYMAP.dispatch(chord))
                .is_some()
            {
                if let Self::Usage {
                    tab_bar_focused, ..
                } = self
                {
                    *tab_bar_focused = true;
                }
                return DialogAction::Redraw;
            }
            if let Self::Usage { scroll, .. } = self
                && scroll.handle_raw_key_for_axes(
                    key,
                    termrock::components::ScrollAxes {
                        vertical: true,
                        horizontal: true,
                    },
                )
            {
                return DialogAction::Redraw;
            }
            return DialogAction::Redraw;
        }
        if matches!(
            self,
            Self::ContainerInfo { .. } | Self::GitHubContext { .. }
        ) {
            if raw_bytes_to_chord(key)
                .and_then(|chord| READ_ONLY_DISMISS_KEYMAP.dispatch(chord))
                .is_some()
            {
                return DialogAction::Dismiss;
            }
            // Scroll the read-only body (offsets clamp at render time): Up/Down +
            // k/j vertical, Left/Right + h/l horizontal. The shared state is
            // rebuilt each frame, so the offset lives on the dialog enum.
            let body_scroll = match self {
                Self::ContainerInfo { scroll, .. } | Self::GitHubContext { scroll, .. } => {
                    Some(scroll)
                }
                _ => None,
            };
            if let Some(scroll) = body_scroll
                && scroll.handle_raw_key_for_axes(
                    key,
                    termrock::components::ScrollAxes {
                        vertical: true,
                        horizontal: true,
                    },
                )
            {
                return DialogAction::Redraw;
            }
            return match key {
                b"\r" | b"\n" => {
                    // ContainerInfo: Enter copies the shared default copy
                    // target. Mouse clicks copy whichever row was clicked.
                    if let Some((row, payload)) = self
                        .container_info_state()
                        .and_then(|state| state.keyboard_copy_payload())
                    {
                        if let Self::ContainerInfo { copied_row, .. } = self {
                            *copied_row = Some(row);
                        }
                        return DialogAction::CopyToClipboard(payload);
                    }
                    if let Some((_, payload)) = self
                        .github_context_state(github)
                        .and_then(|state| state.keyboard_copy_payload())
                    {
                        if let Self::GitHubContext { copied, .. } = self {
                            *copied = true;
                        }
                        DialogAction::CopyToClipboard(payload)
                    } else {
                        DialogAction::Redraw
                    }
                }
                b"o" | b"O" => match self {
                    Self::GitHubContext { .. } => github
                        .and_then(|view| view.status.loaded())
                        .map_or(DialogAction::Redraw, |pr| {
                            DialogAction::OpenHostUrl(pr.url.clone())
                        }),
                    Self::ContainerInfo { diagnostics, .. } => diagnostics
                        .run_log_href
                        .as_deref()
                        .and_then(file_url_path)
                        .map_or(DialogAction::Redraw, |path| {
                            DialogAction::RevealHostPath(path.to_owned())
                        }),
                    _ => DialogAction::Redraw,
                },
                b"c" | b"C" => {
                    if !matches!(self, Self::GitHubContext { .. }) {
                        return DialogAction::Redraw;
                    }
                    github
                        .and_then(|view| view.status.loaded())
                        .and_then(|pr| pr.checks.as_ref())
                        .and_then(crate::pull_request::PullRequestChecks::ci_url)
                        .map_or(DialogAction::Redraw, |url| {
                            DialogAction::OpenHostUrl(url.to_owned())
                        })
                }
                b"r" | b"R" => {
                    if let Self::ContainerInfo { diagnostics, .. } = self
                        && let Some(path) =
                            diagnostics.run_log_href.as_deref().and_then(file_url_path)
                    {
                        return DialogAction::RevealHostPath(path.to_owned());
                    }
                    DialogAction::Redraw
                }
                _ => DialogAction::Redraw,
            };
        }
        // ConfirmAction: dispatch through shared CONFIRM_KEYMAP so key
        // behaviour and hint advertisement stay coupled.
        if let Self::ConfirmAction { kind, selected_yes } = self {
            let action = raw_bytes_to_chord(key).and_then(|chord| CONFIRM_KEYMAP.dispatch(chord));
            return match action {
                Some(SharedConfirmAction::Yes) => DialogAction::ConfirmedAction(*kind),
                Some(SharedConfirmAction::No | SharedConfirmAction::Cancel) => {
                    DialogAction::Dismiss
                }
                Some(SharedConfirmAction::ToggleFocus) => {
                    *selected_yes = !*selected_yes;
                    DialogAction::Redraw
                }
                Some(SharedConfirmAction::CommitFocused) => {
                    if *selected_yes {
                        DialogAction::ConfirmedAction(*kind)
                    } else {
                        DialogAction::Dismiss
                    }
                }
                None => DialogAction::Redraw,
            };
        }
        // From here on, only the type-to-filter list dialogs reach this
        // code path. Dispatch through `FILTER_LIST_KEYMAP`: navigation,
        // confirm, filter-backspace, and dismiss are advertised keys;
        // printable `Char` input is not in the table and falls through
        // (the `None` arm) to `printable_filter_char` filter building.
        // The dismiss surface is narrower than the read-only dialogs
        // above (`q` / Delete are typing actions that build the filter,
        // not dismiss keys); only Esc / Ctrl+C / Ctrl+Q close.
        match raw_bytes_to_chord(key).and_then(|chord| FILTER_LIST_KEYMAP.dispatch(chord)) {
            Some(FilterListAction::Dismiss) => match self {
                // Esc / Ctrl+C on the dirty-exit modal = keep changes and exit
                // (never lose work). The read-only Inspect list and every other
                // dialog dismiss normally; Inspect pops back to the modal
                // underneath via the dialog stack.
                Self::ExitDirty { .. } => DialogAction::ExitDirty(ExitDirtyRow::Keep),
                _ => DialogAction::Dismiss,
            },
            Some(FilterListAction::NavigateUp) => {
                match self {
                    Self::CommandPalette { selected, .. }
                    | Self::SplitDirectionPicker { selected, .. }
                    | Self::CloseTargetPicker { selected, .. } => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    Self::AgentPicker {
                        agents,
                        selected,
                        filter,
                        ..
                    } => {
                        let visible = picker_filtered_rows(agents, filter);
                        *selected = step_selectable(&visible, *selected, false);
                    }
                    Self::ProviderPicker { selected, .. } => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    Self::RenameTab { .. }
                    | Self::ExportFile { .. }
                    | Self::ContainerInfo { .. }
                    | Self::GitHubContext { .. }
                    | Self::Usage { .. }
                    | Self::SpawnFailure(_)
                    | Self::ConfirmAction { .. }
                    | Self::ExecPicker(_) => {}
                    Self::ExitDirty { selected, .. } => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    Self::ExitInspect { selected, .. } => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                }
                DialogAction::Redraw
            }
            Some(FilterListAction::NavigateDown) => {
                match self {
                    Self::CommandPalette {
                        selected,
                        filter,
                        close_label,
                    } => {
                        let visible = palette_filtered_indices(filter, *close_label);
                        if *selected + 1 < visible.len() {
                            *selected += 1;
                        }
                    }
                    Self::SplitDirectionPicker { selected, filter } => {
                        let visible = split_direction_filtered_indices(filter);
                        if *selected + 1 < visible.len() {
                            *selected += 1;
                        }
                    }
                    Self::CloseTargetPicker { selected, filter } => {
                        let visible = close_target_filtered_indices(filter);
                        if *selected + 1 < visible.len() {
                            *selected += 1;
                        }
                    }
                    Self::AgentPicker {
                        agents,
                        selected,
                        filter,
                        ..
                    } => {
                        let visible = picker_filtered_rows(agents, filter);
                        *selected = step_selectable(&visible, *selected, true);
                    }
                    Self::ProviderPicker {
                        selected,
                        providers,
                        ..
                    } => {
                        if *selected + 1 < providers.len() {
                            *selected += 1;
                        }
                    }
                    Self::RenameTab { .. }
                    | Self::ExportFile { .. }
                    | Self::ContainerInfo { .. }
                    | Self::GitHubContext { .. }
                    | Self::Usage { .. }
                    | Self::SpawnFailure(_)
                    | Self::ConfirmAction { .. }
                    | Self::ExecPicker(_) => {}
                    Self::ExitDirty { selected, .. } => {
                        if *selected + 1 < EXIT_DIRTY_ROWS.len() {
                            *selected += 1;
                        }
                    }
                    Self::ExitInspect { selected, lines } => {
                        if *selected + 1 < lines.len() {
                            *selected += 1;
                        }
                    }
                }
                DialogAction::Redraw
            }
            Some(FilterListAction::FilterBackspace) => {
                match self {
                    Self::CommandPalette {
                        filter, selected, ..
                    }
                    | Self::SplitDirectionPicker { filter, selected }
                    | Self::CloseTargetPicker { filter, selected } => {
                        filter.pop();
                        *selected = 0;
                    }
                    Self::AgentPicker {
                        agents,
                        filter,
                        selected,
                        ..
                    } => {
                        filter.pop();
                        let visible = picker_filtered_rows(agents, filter);
                        *selected = first_selectable_idx(&visible);
                    }
                    _ => {}
                }
                DialogAction::Redraw
            }
            Some(FilterListAction::Confirm) => match self {
                Self::CommandPalette {
                    selected,
                    filter,
                    close_label,
                } => {
                    let visible = palette_filtered_indices(filter, *close_label);
                    match visible.get(*selected) {
                        Some(idx) => DialogAction::Command(PALETTE_ITEMS[*idx].0.clone()),
                        None => DialogAction::Redraw,
                    }
                }
                Self::SplitDirectionPicker { selected, filter } => {
                    let visible = split_direction_filtered_indices(filter);
                    match visible.get(*selected) {
                        Some(idx) => DialogAction::SplitDirection(SPLIT_DIRECTION_ITEMS[*idx]),
                        None => DialogAction::Redraw,
                    }
                }
                Self::CloseTargetPicker { selected, filter } => {
                    let visible = close_target_filtered_indices(filter);
                    match visible.get(*selected) {
                        Some(idx) => DialogAction::PickedCloseTarget(CLOSE_TARGET_ITEMS[*idx].0),
                        None => DialogAction::Redraw,
                    }
                }
                Self::AgentPicker {
                    agents,
                    selected,
                    intent,
                    filter,
                } => {
                    let visible = picker_filtered_rows(agents, filter);
                    match visible.get(*selected) {
                        Some(PickerRow::Agent(idx)) => DialogAction::SpawnAgent {
                            agent: Some(agents[*idx].clone()),
                            intent: *intent,
                        },
                        Some(PickerRow::Shell) => DialogAction::SpawnAgent {
                            agent: None,
                            intent: *intent,
                        },
                        // Section row or out-of-bounds index — no
                        // action. The render path keeps `selected`
                        // on a selectable row, but a stale value
                        // (e.g. from a filter pass that emptied the
                        // list) falls through to Redraw rather than
                        // panic.
                        Some(PickerRow::Section(_)) | None => DialogAction::Redraw,
                    }
                }
                Self::ProviderPicker {
                    agent,
                    providers,
                    selected,
                    intent,
                } => match providers.get(*selected) {
                    Some(provider) => DialogAction::SpawnAgentWithProvider {
                        agent: agent.clone(),
                        provider_label: provider.label.clone(),
                        intent: *intent,
                    },
                    None => DialogAction::Redraw,
                },
                // Enter on the dirty-exit modal emits the focused row's action.
                Self::ExitDirty { selected, .. } => match EXIT_DIRTY_ROWS.get(*selected) {
                    Some((row, _)) => DialogAction::ExitDirty(*row),
                    None => DialogAction::Redraw,
                },
                _ => DialogAction::Redraw,
            },
            // Printable ASCII single-byte chunks become filter input. Multi-
            // byte sequences (CSI fragments that did not match a known key,
            // etc.) are no-op redraws — the parser already classified them,
            // and feeding them into the filter would garble the visible
            // typing state.
            None => {
                if let Some(c) = printable_filter_char(key) {
                    match self {
                        Self::CommandPalette {
                            filter, selected, ..
                        }
                        | Self::SplitDirectionPicker { filter, selected }
                        | Self::CloseTargetPicker { filter, selected } => {
                            filter.push(c);
                            *selected = 0;
                        }
                        Self::AgentPicker {
                            agents,
                            filter,
                            selected,
                            ..
                        } => {
                            filter.push(c);
                            let visible = picker_filtered_rows(agents, filter);
                            *selected = first_selectable_idx(&visible);
                        }
                        _ => {}
                    }
                }
                DialogAction::Redraw
            }
        }
    }

    /// Dispatch a left-click at `(row, col)` against the dialog's
    /// hit regions. Shared modal lifecycle classification handles
    /// outside-dismiss; inside clicks on a row select that row and
    /// immediately confirm; clicks on the border or padding rows are
    /// consumed so they do not leak through to the focused pane underneath.
    #[expect(
        clippy::too_many_lines,
        reason = "Dialog click dispatcher: per-Dialog-variant handle-click arm with \
                  nested per-hit-test + border-click + confirm-cancel + dialog-pop. \
                  Modal nesting is the per-variant dispatch protocol."
    )]
    pub fn handle_click(
        &mut self,
        row: u16,
        col: u16,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) -> DialogAction {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let area = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        // Outside the box dismisses; an inside hit falls through to the
        // per-dialog click handling below.
        if termrock::components::classify_click(area, col, row)
            == termrock::components::ModalClickResult::OutsideDismiss
        {
            return DialogAction::Dismiss;
        }
        // Text-input dialog has no clickable rows — clicks inside the
        // box are just swallowed so they don't dismiss or reach the
        // pane underneath.
        if matches!(self, Self::RenameTab { .. } | Self::ExportFile { .. }) {
            return DialogAction::Consume;
        }
        if matches!(self, Self::SpawnFailure(_)) {
            return DialogAction::Consume;
        }
        // ContainerInfo: any copyable row (Container ID, Run ID, Diagnostics
        // log) copies via the shared hit-test. The clicked row's value goes to
        // the clipboard and that row shows the "Copied!" badge.
        if matches!(self, Self::ContainerInfo { .. }) {
            let hit = self.container_info_state().and_then(|state| {
                crate::tui::components::container_info_surface::container_info_copy_payload_at(
                    area, &state, col, row,
                )
            });
            if let Some((hit_row, payload)) = hit {
                if let Self::ContainerInfo { copied_row, .. } = self {
                    *copied_row = Some(hit_row);
                }
                return DialogAction::CopyToClipboard(payload);
            }
            let reveal_hit = self.container_info_state().and_then(|state| {
                crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                    area, &state, col, row,
                )
            });
            return match reveal_hit.and_then(|(_, href)| file_url_path(&href).map(str::to_owned)) {
                Some(path) => DialogAction::RevealHostPath(path),
                None => DialogAction::Consume,
            };
        }
        if matches!(self, Self::GitHubContext { .. }) {
            let area = ratatui::layout::Rect {
                x: box_col,
                y: box_row,
                width,
                height,
            };
            let hit = self.github_context_state(github).and_then(|state| {
                crate::tui::components::container_info_surface::container_info_copy_payload_at(
                    area, &state, col, row,
                )
            });
            if let Some((_hit_row, payload)) = hit {
                if let Self::GitHubContext { copied, .. } = self {
                    *copied = true;
                }
                return DialogAction::CopyToClipboard(payload);
            }
            let open_hit = self.github_context_state(github).and_then(|state| {
                crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                    area, &state, col, row,
                )
            });
            return match open_hit {
                Some((GITHUB_OPEN_PR_ROW | GITHUB_OPEN_CI_ROW, payload)) => {
                    DialogAction::OpenHostUrl(payload)
                }
                Some(_) | None => DialogAction::Consume,
            };
        }
        if let Self::Usage { view, selected, .. } = self {
            let tab = Self::usage_tab_index_at(view, *selected, area, row, col);
            return match tab {
                Some(0) => {
                    *selected = UsageDialogTab::Overview;
                    DialogAction::Redraw
                }
                Some(idx) => view.tabs.get(idx.saturating_sub(1)).map_or_else(
                    || DialogAction::Consume,
                    |tab| DialogAction::SwitchUsageProvider {
                        provider_label: tab.label.clone(),
                    },
                ),
                None => DialogAction::Consume,
            };
        }
        // ConfirmAction: only the visible Yes/No button cells confirm or
        // dismiss. The shared confirm widget owns button geometry, including
        // the taller data-loss exit variant.
        if let Self::ConfirmAction { kind, selected_yes } = self {
            let mut state = if matches!(kind, ConfirmKind::Exit) {
                crate::tui::components::exit_confirm_state_with_data_loss()
            } else {
                termrock::components::ConfirmState::new(format!(
                    "{}\n\n{}",
                    kind.title(),
                    kind.message()
                ))
            };
            if *selected_yes {
                state = state.with_focus_yes();
            }
            let area = ratatui::layout::Rect {
                x: box_col,
                y: box_row,
                width,
                height,
            };
            return match termrock::components::confirm_button_hit(area, &state, col, row) {
                Some(true) => DialogAction::ConfirmedAction(*kind),
                Some(false) => DialogAction::Dismiss,
                None => DialogAction::Consume,
            };
        }
        // ProviderPicker: flat list, no filter row. Items start at box_row + 1.
        if let Self::ProviderPicker {
            agent,
            providers,
            selected,
            intent,
        } = self
        {
            let first_item_row = box_row + 1;
            let count = providers.len() as u16;
            if row < first_item_row || row >= first_item_row + count {
                return DialogAction::Consume;
            }
            let idx = (row - first_item_row) as usize;
            let Some(provider) = providers.get(idx) else {
                return DialogAction::Consume;
            };
            *selected = idx;
            return DialogAction::SpawnAgentWithProvider {
                agent: agent.clone(),
                provider_label: provider.label.clone(),
                intent: *intent,
            };
        }
        // Row layout inside the box for filterable dialogs:
        //   box_row + 0:  top border (decorative)
        //   box_row + 1:  blank pad row
        //   box_row + 2:  filter input ("/ <text>▏")
        //   box_row + 3:  blank pad row separating filter from items
        //   box_row + 3:  first item row
        //
        // Clicks on the filter row are no-op consumes (no in-place
        // edit yet); clicks on item rows select + confirm against
        // the current filtered list so a future refactor that
        // shortens / lengthens the visible item count via filter
        // input still routes the click to the right action.
        let first_item_row = box_row + 3;
        let visible_count: u16 = match self {
            Self::CommandPalette {
                filter,
                close_label,
                ..
            } => palette_filtered_indices(filter, *close_label).len() as u16,
            Self::SplitDirectionPicker { filter, .. } => {
                split_direction_filtered_indices(filter).len() as u16
            }
            Self::CloseTargetPicker { filter, .. } => {
                close_target_filtered_indices(filter).len() as u16
            }
            Self::AgentPicker { agents, filter, .. } => {
                picker_filtered_rows(agents, filter).len() as u16
            }
            Self::RenameTab { .. }
            | Self::ExportFile { .. }
            | Self::ContainerInfo { .. }
            | Self::GitHubContext { .. }
            | Self::Usage { .. }
            | Self::SpawnFailure(_)
            | Self::ConfirmAction { .. }
            | Self::ProviderPicker { .. }
            | Self::ExecPicker(_)
            | Self::ExitDirty { .. }
            | Self::ExitInspect { .. } => 0,
        };
        if row < first_item_row || row >= first_item_row + visible_count {
            return DialogAction::Consume;
        }
        let visible_idx = (row - first_item_row) as usize;
        match self {
            Self::CommandPalette {
                selected,
                filter,
                close_label,
            } => {
                let visible = palette_filtered_indices(filter, *close_label);
                let Some(&source_idx) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                *selected = visible_idx;
                DialogAction::Command(PALETTE_ITEMS[source_idx].0.clone())
            }
            Self::SplitDirectionPicker { selected, filter } => {
                let visible = split_direction_filtered_indices(filter);
                let Some(&source_idx) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                *selected = visible_idx;
                DialogAction::SplitDirection(SPLIT_DIRECTION_ITEMS[source_idx])
            }
            Self::CloseTargetPicker { selected, filter } => {
                let visible = close_target_filtered_indices(filter);
                let Some(&source_idx) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                *selected = visible_idx;
                DialogAction::PickedCloseTarget(CLOSE_TARGET_ITEMS[source_idx].0)
            }
            Self::AgentPicker {
                agents,
                selected,
                intent,
                filter,
            } => {
                let visible = picker_filtered_rows(agents, filter);
                let Some(&picker_row) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                match picker_row {
                    PickerRow::Section(_) => DialogAction::Consume,
                    PickerRow::Agent(idx) => {
                        *selected = visible_idx;
                        DialogAction::SpawnAgent {
                            agent: Some(agents[idx].clone()),
                            intent: *intent,
                        }
                    }
                    PickerRow::Shell => {
                        *selected = visible_idx;
                        DialogAction::SpawnAgent {
                            agent: None,
                            intent: *intent,
                        }
                    }
                }
            }
            // Text-input, ContainerInfo, ConfirmAction, and ProviderPicker
            // clicks were already handled by early returns above.
            Self::RenameTab { .. }
            | Self::ExportFile { .. }
            | Self::ContainerInfo { .. }
            | Self::GitHubContext { .. }
            | Self::Usage { .. }
            | Self::ConfirmAction { .. }
            | Self::ProviderPicker { .. }
            | Self::ExecPicker(_)
            | Self::ExitDirty { .. }
            | Self::ExitInspect { .. }
            | Self::SpawnFailure(_) => DialogAction::Consume,
        }
    }

    /// Return true when `(row, col)` is a dialog hit target that will
    /// perform an action on click. The daemon uses this to drive OSC 22
    /// pointer-shape feedback without duplicating dialog layout maths.
    pub fn clickable_at(
        &self,
        row: u16,
        col: u16,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) -> bool {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let area = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        let inside_box =
            row >= box_row && row < box_row + height && col >= box_col && col < box_col + width;
        if !inside_box {
            return false;
        }
        match self {
            Self::RenameTab { .. }
            | Self::ExportFile { .. }
            | Self::ExecPicker(_)
            | Self::SpawnFailure(_) => false,
            Self::ContainerInfo { .. } => {
                let area = ratatui::layout::Rect {
                    x: box_col,
                    y: box_row,
                    width,
                    height,
                };
                self.container_info_state().is_some_and(|state| {
                    crate::tui::components::container_info_surface::container_info_copy_payload_at(area, &state, col, row)
                        .is_some()
                        || crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                            area, &state, col, row,
                        )
                        .is_some_and(|(_, href)| file_url_path(&href).is_some())
                })
            }
            Self::GitHubContext { .. } => {
                let area = ratatui::layout::Rect {
                    x: box_col,
                    y: box_row,
                    width,
                    height,
                };
                self.github_context_state(github).is_some_and(|state| {
                    crate::tui::components::container_info_surface::container_info_copy_payload_at(area, &state, col, row)
                        .is_some()
                        || crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                            area, &state, col, row,
                        )
                        .is_some_and(|(idx, _)| {
                            matches!(idx, GITHUB_OPEN_PR_ROW | GITHUB_OPEN_CI_ROW)
                        })
                })
            }
            Self::Usage { view, selected, .. } => {
                Self::usage_tab_index_at(view, *selected, area, row, col).is_some()
            }
            Self::ConfirmAction { .. } => true,
            Self::CommandPalette {
                filter,
                close_label,
                ..
            } => dialog_list_row_clickable(
                row,
                box_row,
                palette_filtered_indices(filter, *close_label).len(),
            ),
            Self::SplitDirectionPicker { filter, .. } => dialog_list_row_clickable(
                row,
                box_row,
                split_direction_filtered_indices(filter).len(),
            ),
            Self::CloseTargetPicker { filter, .. } => {
                dialog_list_row_clickable(row, box_row, close_target_filtered_indices(filter).len())
            }
            Self::AgentPicker { agents, filter, .. } => {
                let first_item_row = box_row + 3;
                let visible = picker_filtered_rows(agents, filter);
                if row < first_item_row || row >= first_item_row + visible.len() as u16 {
                    return false;
                }
                matches!(
                    visible[(row - first_item_row) as usize],
                    PickerRow::Agent(_) | PickerRow::Shell
                )
            }
            Self::ProviderPicker { providers, .. } => {
                let first_item_row = box_row + 1;
                row >= first_item_row && row < first_item_row + providers.len() as u16
            }
            // Keyboard-only modals — no click targets.
            Self::ExitDirty { .. } | Self::ExitInspect { .. } => false,
        }
    }

    /// Box geometry the dialog will render with for `term_rows` /
    /// `term_cols`. Returned as `(row, col, height, width)`. Kept
    /// next to the render functions so any layout change updates
    /// both surfaces at once.
    ///
    /// Height clamps to the area below the status bar so a very small
    /// terminal does not paint past the bottom edge (which would
    /// scroll the host terminal and destroy the operator's pane
    /// content) and does not overlap row 0 (the brand pill / tab
    /// strip). The dialog can render unusable when the terminal is
    /// pathologically small; the trade-off is that the host terminal
    /// stays in a recoverable state regardless.
    pub(crate) fn box_rect(&self, term_rows: u16, term_cols: u16) -> (u16, u16, u16, u16) {
        // Filterable dialogs reserve 2 extra rows: one for the filter
        // input and one for the separator above the items list. Item
        // count tracks the *filtered* set so the box shrinks as the
        // operator narrows the matches.
        let natural_height = match self {
            Self::CommandPalette {
                filter,
                close_label,
                ..
            } => {
                let items = palette_filtered_indices(filter, *close_label).len() as u16;
                items + 4 // top + filter + pad + items + bottom
            }
            Self::SplitDirectionPicker { filter, .. } => {
                let items = split_direction_filtered_indices(filter).len() as u16;
                items + 4
            }
            Self::CloseTargetPicker { filter, .. } => {
                let items = close_target_filtered_indices(filter).len() as u16;
                items + 4
            }
            Self::AgentPicker { agents, filter, .. } => {
                let items = picker_filtered_rows(agents, filter).len() as u16;
                items + 4
            }
            Self::RenameTab { .. } | Self::ExportFile { .. } => 5,
            Self::ContainerInfo { .. } => self.container_info_state().map_or(10, |state| {
                crate::tui::components::container_info_surface::container_info_required_height(
                    &state,
                )
            }),
            Self::GitHubContext { .. } => 11,
            Self::Usage { .. } => self.usage_state().map_or(10, |state| {
                crate::tui::components::dialog_widgets::usage_info_required_height(&state)
            }),
            Self::SpawnFailure(state) => {
                let inner_width = PALETTE_WIDTH.saturating_sub(2);
                termrock::components::required_height(state, inner_width, term_rows)
            }
            // 9 = border(2) + leading(1) + question(1) + empty(1) + message(1) + spacer(1) + button(1) + trailing(1)
            // Matches the canonical symmetric dialog layout (Defect 5).
            // Exit shows the shared data-loss variant (extra warning notes), so
            // size it from that state rather than the fixed single-line height.
            Self::ConfirmAction { kind, .. } => match kind {
                ConfirmKind::Exit => termrock::components::confirm_required_height(
                    &crate::tui::components::exit_confirm_state_with_data_loss(),
                ),
                ConfirmKind::ClosePane | ConfirmKind::CloseTab => 9,
            },
            // No filter row: top border + items + bottom border.
            Self::ProviderPicker { providers, .. } => providers.len() as u16 + 2,
            // Top border + command line + separator + one row per credential +
            // hint + bottom border.
            Self::ExecPicker(state) => state.items.len() as u16 + 5,
            Self::ExitDirty { summary, .. } => (summary.len() + EXIT_DIRTY_ROWS.len()) as u16 + 4,
            Self::ExitInspect { lines, .. } => lines.len() as u16 + 4,
        };
        let content_height = crate::tui::layout::available_content_rows(term_rows).max(3);
        let max_height = if matches!(self, Self::Usage { .. }) {
            content_height.saturating_sub(1).max(3)
        } else {
            content_height
        };
        let height = natural_height.min(max_height);
        let top_row = crate::tui::components::status_bar::STATUS_BAR_ROWS;
        let area_y = if matches!(self, Self::Usage { .. }) {
            top_row.saturating_add(1)
        } else {
            top_row
        };
        let area_height = if matches!(self, Self::Usage { .. }) {
            content_height.saturating_sub(1)
        } else {
            content_height
        };
        let area = ratatui::layout::Rect::new(0, area_y, term_cols, area_height);
        let spec = match self {
            Self::ContainerInfo { .. } | Self::GitHubContext { .. } => ModalRectSpec::MaxWidthMin {
                max_width: CONTAINER_INFO_WIDTH,
                min_width: PALETTE_WIDTH,
                side_margin: 4,
                height,
            },
            Self::Usage { .. } => ModalRectSpec::TopAlignedMaxWidthMin {
                max_width: CONTAINER_INFO_WIDTH,
                min_width: PALETTE_WIDTH,
                side_margin: 4,
                height,
            },
            // Exit data-loss confirm has two warning notes wider than PALETTE_WIDTH.
            // Use the shared Details width percentage (70%) so the notes don't truncate.
            Self::ConfirmAction {
                kind: ConfirmKind::Exit,
                ..
            } => ModalRectSpec::PercentClamp {
                width_pct: 70,
                min_width: PALETTE_WIDTH,
                side_margin: 4,
                height,
            },
            _ => ModalRectSpec::Exact {
                width: PALETTE_WIDTH,
                height,
            },
        };
        let rect = modal_rect(area, spec);
        (rect.y, rect.x, rect.height, rect.width)
    }

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

#[cfg(test)]
mod tests;
