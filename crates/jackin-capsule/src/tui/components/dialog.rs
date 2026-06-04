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
use crate::pull_request::PullRequestInfo;

/// Borrowed snapshot of multiplexer PR state, so `GitHubContext`
/// rendering and dispatch stay live without copying the data into
/// the dialog variant.
#[derive(Clone, Copy)]
pub struct GithubContextView<'a> {
    pub branch: Option<&'a str>,
    pub status: PullRequestStatus<'a>,
}

pub fn github_context_view_from_state<'a>(
    branch: Option<&'a str>,
    pull_request: Option<&'a PullRequestInfo>,
    loading: bool,
) -> GithubContextView<'a> {
    let status = match pull_request {
        Some(pr) => PullRequestStatus::Loaded(pr),
        None if loading => PullRequestStatus::Resolving,
        None => PullRequestStatus::Idle,
    };
    GithubContextView { branch, status }
}

/// Resolution state of the multiplexer's PR lookup. Mirrors the
/// daemon's `(in_flight, Option<PullRequestInfo>)` pair but rules
/// out the impossible `Loaded + Resolving` combination at the type
/// level — keeps every dialog branch a single exhaustive match.
#[derive(Clone, Copy)]
pub enum PullRequestStatus<'a> {
    Loaded(&'a PullRequestInfo),
    Resolving,
    Idle,
}

pub use super::container_info_dialog::ContainerInfoDiagnostics;
use super::container_info_dialog::{ContainerInfoRow, non_empty_or_dim};
pub(super) use super::palette::{PALETTE_ITEMS, palette_filtered_indices, palette_item_label};
pub use super::palette::{PaletteCloseLabel, PaletteCommand};

impl<'a> PullRequestStatus<'a> {
    pub fn loaded(&self) -> Option<&'a PullRequestInfo> {
        match self {
            Self::Loaded(pr) => Some(*pr),
            _ => None,
        }
    }
}

use jackin_tui::{
    ACTION_ACCENT, BLACK, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE,
    ansi::{BG_DARK, BOLD, RESET, rgb_bg, rgb_fg},
};

const PALETTE_WIDTH: u16 = 50;
const CONTAINER_INFO_WIDTH: u16 = 86;
const FG_GREEN: &str = rgb_fg(PHOSPHOR_GREEN);
const FG_DIM: &str = rgb_fg(PHOSPHOR_DIM);
const FG_BORDER: &str = rgb_fg(PHOSPHOR_DARK);
const FG_WHITE: &str = rgb_fg(WHITE);
const FG_CLICK_HOVER: &str = rgb_fg(ACTION_ACCENT);
const SELECT_BG: &str = rgb_bg(PHOSPHOR_GREEN);
const SELECT_FG: &str = rgb_fg(BLACK);
const CONFIRM_BG: &str = rgb_bg(WHITE);
const SELECT_MARK: &str = "▸ ";
const UNSELECT_MARK: &str = "  ";

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
    /// Operator-facing label for the SplitDirectionPicker rows and
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
    /// shared `jackin_tui::TextField` so the buffer + cursor + max
    /// length live in the same place as the console TUI text input. Enter
    /// commits; Esc cancels; empty input clears any previous custom
    /// label so the tab returns to auto-naming.
    RenameTab {
        tab_idx: usize,
        input: jackin_tui::TextField,
    },
    /// Read-only modal opened when the operator clicks the
    /// container-name segment of the bottom branch/PR context bar.
    /// Surfaces role key, focused-agent runtime, full container ID,
    /// and workspace path with a one-key "copy to clipboard" shortcut.
    /// Enter or a click on the Container ID row emits OSC 52 with
    /// the container name AND keeps the dialog open — `copied` flips
    /// to `true` so the renderer shows a visible "Copied!" indicator
    /// next to the container ID, confirming the OSC 52 actually
    /// flushed to the outer terminal. Esc / q / a click outside the
    /// box dismisses. `focused_agent` is the slug of whichever pane
    /// is active when the modal opens — `Some("claude")`,
    /// `Some("kimi")`, … or `None` for a plain shell pane.
    ContainerInfo {
        container_name: String,
        role: String,
        focused_agent: Option<String>,
        workdir: String,
        diagnostics: ContainerInfoDiagnostics,
        copied: bool,
    },
    /// Read-only modal opened from the bottom branch/PR context.
    /// Branch / PR / loading state come from `GithubContextView` at
    /// render time so a mid-life branch flip reflects without an
    /// explicit refresh step.
    GitHubContext { copied: bool },
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
            Self::Exit => "Stop all agents; jackin' will clean up.",
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
    /// User picked a split direction in the SplitDirectionPicker —
    /// daemon opens an AgentPicker with `PickerIntent::Split(direction)`.
    SplitDirection(SplitDirection),
    /// User picked a close target in the CloseTargetPicker — daemon
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
    /// User confirmed a provider in the ProviderPicker — the daemon maps
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
    /// Operator clicked or pressed Enter on the `ContainerInfo` copy
    /// target — copy the carried payload to the operator's clipboard
    /// via OSC 52 and keep the dialog open for visible feedback.
    /// Carrying the
    /// payload through the action (rather than the daemon re-deriving
    /// it from the dialog) keeps the dialog the single source of
    /// truth for what gets copied.
    CopyToClipboard(String),
    /// User dismissed with Escape.
    Dismiss,
    /// Dialog is still open; redraw.
    Redraw,
    /// Mouse event lands somewhere with no semantic effect (border,
    /// padding row). Swallow it so it does not reach the focused pane.
    Consume,
}

/// Items in the SplitDirectionPicker sub-dialog. Prefer the common
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
    pub fn new_command_palette(close_label: PaletteCloseLabel) -> Self {
        Self::CommandPalette {
            selected: 0,
            filter: String::new(),
            close_label,
        }
    }

    pub fn new_rename_tab(tab_idx: usize, initial: impl Into<String>) -> Self {
        let input = jackin_tui::TextField::new(initial.into()).with_max_chars(MAX_CUSTOM_LABEL_LEN);
        Self::RenameTab { tab_idx, input }
    }

    pub fn new_split_direction_picker() -> Self {
        Self::SplitDirectionPicker {
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn new_close_target_picker() -> Self {
        Self::CloseTargetPicker {
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn new_confirm_action(kind: ConfirmKind) -> Self {
        Self::ConfirmAction {
            kind,
            selected_yes: false,
        }
    }

    pub fn new_container_info(
        container_name: String,
        role: String,
        focused_agent: Option<String>,
        workdir: String,
        diagnostics: ContainerInfoDiagnostics,
    ) -> Self {
        Self::ContainerInfo {
            container_name,
            role,
            focused_agent,
            workdir,
            diagnostics,
            copied: false,
        }
    }

    pub fn new_github_context() -> Self {
        Self::GitHubContext { copied: false }
    }

    pub fn new_provider_picker(
        agent: Option<String>,
        providers: Vec<ProviderChoice>,
        intent: PickerIntent,
    ) -> Self {
        Self::ProviderPicker {
            agent,
            providers,
            selected: 0,
            intent,
        }
    }

    /// Construct an AgentPicker with `selected` pre-initialised to
    /// the first selectable row of the unfiltered layout. Saves every
    /// caller from having to know about the leading "agents" section
    /// row that pushes the first selectable index off zero — and
    /// keeps the "no agents installed" case working (the layout
    /// degenerates to `[Section("shells"), Shell]`, first selectable
    /// is still `1`).
    pub fn new_agent_picker(agents: Vec<String>, intent: PickerIntent) -> Self {
        let filter = String::new();
        let visible = picker_filtered_rows(&agents, &filter);
        Self::AgentPicker {
            agents,
            selected: first_selectable_idx(&visible),
            intent,
            filter,
        }
    }

    /// Handle a raw key byte and return the resulting action.
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
        // Read-only info dialogs (ContainerInfo, GitHubContext): Esc /
        // dismiss keys close, Enter copies the dialog's value to the
        // operator's clipboard with the `copied` flag flipped to true
        // so the next render's "Copied!" indicator confirms the OSC 52
        // fired. The dialog stays open until dismissed so the feedback
        // is actually visible.
        if matches!(
            self,
            Self::ContainerInfo { .. } | Self::GitHubContext { .. }
        ) {
            if is_dismiss_key(key) {
                return DialogAction::Dismiss;
            }
            return match key {
                b"\r" | b"\n" => match self.copy_target(github) {
                    Some(target) => {
                        *target.copied = true;
                        DialogAction::CopyToClipboard(target.payload)
                    }
                    None => DialogAction::Redraw,
                },
                _ => DialogAction::Redraw,
            };
        }
        // ConfirmAction has its own dispatch — Y/N shortcuts toggle
        // the selection or confirm directly, Enter acts on the
        // current selection, Esc cancels. Routed before the type-to-
        // filter branch so `y` and `n` keys do not flow into a
        // filter buffer.
        if let Self::ConfirmAction { kind, selected_yes } = self {
            if key == b"\x1b" || key == b"\x03" || key == b"n" || key == b"N" {
                return DialogAction::Dismiss;
            }
            if key == b"y" || key == b"Y" {
                return DialogAction::ConfirmedAction(*kind);
            }
            if is_arrow_up(key)
                || is_arrow_down(key)
                || key == b"\x1b[C"
                || key == b"\x1b[D"
                || key == b"\t"
            {
                *selected_yes = !*selected_yes;
                return DialogAction::Redraw;
            }
            if is_enter(key) {
                if *selected_yes {
                    return DialogAction::ConfirmedAction(*kind);
                }
                return DialogAction::Dismiss;
            }
            return DialogAction::Redraw;
        }
        // From here on, only the type-to-filter list dialogs reach
        // this code path. The dismiss surface is narrower than the
        // read-only dialogs above (`q` / Backspace / Delete are
        // typing actions that build the filter, not dismiss keys);
        // only Esc and Ctrl+C close.
        if is_filter_dismiss_key(key) {
            return DialogAction::Dismiss;
        }
        if is_arrow_up(key) {
            return match self {
                Self::CommandPalette { selected, .. }
                | Self::SplitDirectionPicker { selected, .. }
                | Self::CloseTargetPicker { selected, .. } => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                    DialogAction::Redraw
                }
                Self::AgentPicker {
                    agents,
                    selected,
                    filter,
                    ..
                } => {
                    let visible = picker_filtered_rows(agents, filter);
                    *selected = step_selectable(&visible, *selected, false);
                    DialogAction::Redraw
                }
                Self::ProviderPicker { selected, .. } => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                    DialogAction::Redraw
                }
                Self::RenameTab { .. }
                | Self::ContainerInfo { .. }
                | Self::GitHubContext { .. }
                | Self::ConfirmAction { .. } => DialogAction::Redraw,
            };
        }
        if is_arrow_down(key) {
            return match self {
                Self::CommandPalette {
                    selected,
                    filter,
                    close_label,
                } => {
                    let visible = palette_filtered_indices(filter, *close_label);
                    if *selected + 1 < visible.len() {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                Self::SplitDirectionPicker { selected, filter } => {
                    let visible = split_direction_filtered_indices(filter);
                    if *selected + 1 < visible.len() {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                Self::CloseTargetPicker { selected, filter } => {
                    let visible = close_target_filtered_indices(filter);
                    if *selected + 1 < visible.len() {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                Self::AgentPicker {
                    agents,
                    selected,
                    filter,
                    ..
                } => {
                    let visible = picker_filtered_rows(agents, filter);
                    *selected = step_selectable(&visible, *selected, true);
                    DialogAction::Redraw
                }
                Self::ProviderPicker {
                    selected,
                    providers,
                    ..
                } => {
                    if *selected + 1 < providers.len() {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                Self::RenameTab { .. }
                | Self::ContainerInfo { .. }
                | Self::GitHubContext { .. }
                | Self::ConfirmAction { .. } => DialogAction::Redraw,
            };
        }
        if is_backspace(key) {
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
            return DialogAction::Redraw;
        }
        if is_enter(key) {
            return match self {
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
                _ => DialogAction::Redraw,
            };
        }
        // Printable ASCII single-byte chunks become filter input. Multi-
        // byte sequences (CSI fragments that did not match a known key,
        // etc.) are no-op redraws — the parser already classified them,
        // and feeding them into the filter would garble the visible
        // typing state.
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
            return DialogAction::Redraw;
        }
        DialogAction::Redraw
    }

    /// Dispatch a left-click at `(row, col)` against the dialog's
    /// hit regions. Clicks outside the box dismiss the dialog;
    /// clicks on a row select that row and immediately confirm;
    /// clicks on the border or padding rows are consumed so they do
    /// not leak through to the focused pane underneath.
    pub fn handle_click(
        &mut self,
        row: u16,
        col: u16,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) -> DialogAction {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let inside_box =
            row >= box_row && row < box_row + height && col >= box_col && col < box_col + width;
        if !inside_box {
            return DialogAction::Dismiss;
        }
        // Text-input dialog has no clickable rows — clicks inside the
        // box are just swallowed so they don't dismiss or reach the
        // pane underneath.
        if matches!(self, Self::RenameTab { .. }) {
            return DialogAction::Consume;
        }
        // ContainerInfo: only the Container ID row is the copy
        // target. Other inside-box clicks are informational and must
        // not mutate the operator's clipboard.
        if matches!(
            self,
            Self::ContainerInfo { .. } | Self::GitHubContext { .. }
        ) {
            return match self.copy_target(github) {
                Some(target)
                    if info_box_value_row_clickable(
                        row,
                        col,
                        box_row,
                        box_col,
                        width,
                        target.row_offset,
                    ) =>
                {
                    *target.copied = true;
                    DialogAction::CopyToClipboard(target.payload)
                }
                _ => DialogAction::Consume,
            };
        }
        // ConfirmAction: only the visible Yes/No button cells confirm
        // or dismiss; other inside-box clicks (title, explanation,
        // padding) are swallowed. Mirrors the layout in
        // `render_confirm_action`.
        if let Self::ConfirmAction { kind, .. } = self {
            const YES_LABEL: &str = "  Yes  ";
            const GAP: &str = "    ";
            const NO_LABEL: &str = "  No  ";
            let interior_left = box_col + 1;
            let interior_cols = width.saturating_sub(2) as usize;
            let buttons_w =
                YES_LABEL.chars().count() + GAP.chars().count() + NO_LABEL.chars().count();
            let button_col = interior_left
                + u16::try_from(interior_cols.saturating_sub(buttons_w) / 2).unwrap_or(0);
            let button_row = box_row + height.saturating_sub(2);
            if row != button_row {
                return DialogAction::Consume;
            }
            let yes_start = button_col;
            let yes_end = yes_start + YES_LABEL.chars().count() as u16;
            let no_start = yes_end + GAP.chars().count() as u16;
            let no_end = no_start + NO_LABEL.chars().count() as u16;
            if col >= yes_start && col < yes_end {
                return DialogAction::ConfirmedAction(*kind);
            }
            if col >= no_start && col < no_end {
                return DialogAction::Dismiss;
            }
            return DialogAction::Consume;
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
            | Self::ContainerInfo { .. }
            | Self::GitHubContext { .. }
            | Self::ConfirmAction { .. }
            | Self::ProviderPicker { .. } => 0,
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
            // RenameTab, ContainerInfo, ConfirmAction, and ProviderPicker
            // clicks were already handled by early returns above.
            Self::RenameTab { .. }
            | Self::ContainerInfo { .. }
            | Self::GitHubContext { .. }
            | Self::ConfirmAction { .. }
            | Self::ProviderPicker { .. } => DialogAction::Consume,
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
        let inside_box =
            row >= box_row && row < box_row + height && col >= box_col && col < box_col + width;
        if !inside_box {
            return false;
        }
        match self {
            Self::RenameTab { .. } => false,
            Self::ContainerInfo { .. } => info_box_value_row_clickable(
                row,
                col,
                box_row,
                box_col,
                width,
                CONTAINER_INFO_ID_ROW,
            ),
            Self::GitHubContext { .. } => {
                github.and_then(|view| view.status.loaded()).is_some()
                    && info_box_value_row_clickable(
                        row,
                        col,
                        box_row,
                        box_col,
                        width,
                        GITHUB_CONTEXT_URL_ROW,
                    )
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
        let width = match self {
            Self::ContainerInfo { .. } | Self::GitHubContext { .. } => CONTAINER_INFO_WIDTH
                .min(term_cols.saturating_sub(4))
                .max(PALETTE_WIDTH),
            _ => PALETTE_WIDTH,
        };
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
            Self::RenameTab { .. } => 5,
            Self::ContainerInfo { .. } => {
                if crate::logging::debug_enabled() {
                    // 4 base rows + jackin + jackin-capsule + Run ID + Run log + box chrome (4)
                    12
                } else {
                    // 4 base rows + jackin + jackin-capsule + box chrome (4)
                    10
                }
            }
            Self::GitHubContext { .. } => 9,
            // 9 = border(2) + leading(1) + question(1) + empty(1) + message(1) + spacer(1) + button(1) + trailing(1)
            // Matches the canonical symmetric dialog layout (Defect 5).
            Self::ConfirmAction { .. } => 9,
            // No filter row: top border + items + bottom border.
            Self::ProviderPicker { providers, .. } => providers.len() as u16 + 2,
        };
        let max_height = term_rows
            .saturating_sub(crate::tui::components::status_bar::STATUS_BAR_ROWS)
            .saturating_sub(1)
            .max(3);
        let height = natural_height.min(max_height);
        let row = crate::tui::components::status_bar::STATUS_BAR_ROWS
            + (max_height.saturating_sub(height)) / 2;
        let col = (term_cols.saturating_sub(width)) / 2;
        (row, col, height, width)
    }

    /// Render the dialog overlay into `buf`.
    /// `term_rows` and `term_cols` are the host terminal dimensions.
    ///
    /// `box_rect` is the single source of truth for box geometry —
    /// both the renderer AND `handle_click` use it, so paint and
    /// hit-test cannot drift. The free-function `render_*` helpers
    /// take the `(row, col, height, width)` tuple from `box_rect`
    /// instead of recomputing the centring. Footer hints are rendered
    /// by the multiplexer compositor near the bottom chrome so every
    /// dialog follows the same hint contract without competing with
    /// the branch/container status row.
    #[cfg(test)]
    pub fn render(&self, buf: &mut Vec<u8>, term_rows: u16, term_cols: u16) {
        self.render_with_hover(buf, term_rows, term_cols, false, None);
    }

    pub fn render_with_hover(
        &self,
        buf: &mut Vec<u8>,
        term_rows: u16,
        term_cols: u16,
        copy_target_hovered: bool,
        github: Option<&GithubContextView<'_>>,
    ) {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        // Skip rendering entirely when the terminal is too small to
        // hold the box without overlapping the status bar or the
        // bottom edge. The host terminal would otherwise scroll and
        // destroy operator pane content.
        if term_rows < crate::tui::components::status_bar::STATUS_BAR_ROWS + 3
            || box_row + height > term_rows
            || box_col + width > term_cols
        {
            return;
        }
        match self {
            Self::CommandPalette {
                selected,
                filter,
                close_label,
            } => {
                render_palette(
                    buf,
                    box_row,
                    box_col,
                    height,
                    width,
                    *selected,
                    filter,
                    *close_label,
                );
            }
            Self::SplitDirectionPicker { selected, filter } => {
                render_split_direction_picker(
                    buf, box_row, box_col, height, width, *selected, filter,
                );
            }
            Self::AgentPicker {
                agents,
                selected,
                intent,
                filter,
            } => {
                render_agent_picker(
                    buf, box_row, box_col, height, width, agents, *selected, *intent, filter,
                );
            }
            Self::RenameTab { input, .. } => {
                render_rename_tab(buf, term_rows, term_cols, input.value());
            }
            Self::ContainerInfo {
                container_name,
                role,
                focused_agent,
                workdir,
                diagnostics,
                copied,
            } => {
                render_container_info(
                    buf,
                    box_row,
                    box_col,
                    height,
                    width,
                    container_name,
                    role,
                    focused_agent.as_deref(),
                    workdir,
                    diagnostics,
                    *copied,
                    copy_target_hovered,
                );
            }
            Self::GitHubContext { copied } => {
                let branch = github.and_then(|view| view.branch);
                let pull_request = github.and_then(|view| view.status.loaded());
                let loading =
                    github.is_some_and(|view| matches!(view.status, PullRequestStatus::Resolving));
                render_github_context(
                    buf,
                    box_row,
                    box_col,
                    height,
                    width,
                    branch,
                    pull_request,
                    loading,
                    *copied,
                    copy_target_hovered,
                );
            }
            Self::CloseTargetPicker { selected, filter } => {
                render_close_target_picker(buf, box_row, box_col, height, width, *selected, filter);
            }
            Self::ConfirmAction { kind, selected_yes } => {
                render_confirm_action(buf, box_row, box_col, height, width, *kind, *selected_yes);
            }
            Self::ProviderPicker {
                providers,
                selected,
                ..
            } => {
                render_provider_picker(buf, box_row, box_col, height, width, providers, *selected);
            }
        }
    }

    pub fn render_footer_hint(
        &self,
        buf: &mut Vec<u8>,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) {
        if term_rows == 0 {
            return;
        }
        // Bottom row of the (now opaque, full-screen) modal. The hint row
        // fills full-width with the same solid black as the backdrop, so there
        // is no off-colour band; the old blank spacer used the terminal default
        // background and showed as a lighter strip — dropped.
        let spans = self.footer_hint_spans(github);
        render_hint_row(buf, term_rows - 1, term_cols, spans);
    }

    pub(crate) fn footer_hint_spans(
        &self,
        github: Option<&GithubContextView<'_>>,
    ) -> &'static [HintSpan<'static>] {
        match self {
            Self::CommandPalette { .. } => PALETTE_HINT,
            Self::SplitDirectionPicker { .. }
            | Self::AgentPicker { .. }
            | Self::CloseTargetPicker { .. }
            | Self::ProviderPicker { .. } => PICKER_HINT,
            Self::RenameTab { .. } => RENAME_HINT,
            Self::ContainerInfo { .. } => CONTAINER_INFO_HINT,
            Self::GitHubContext { .. } => {
                if github.and_then(|view| view.status.loaded()).is_some() {
                    GITHUB_CONTEXT_HINT
                } else {
                    READ_ONLY_HINT
                }
            }
            Self::ConfirmAction { .. } => CONFIRM_HINT,
        }
    }

    /// Clear transient copy feedback after the daemon-side timer
    /// expires. Returns true only when the visible dialog changed.
    pub fn clear_copy_feedback(&mut self) -> bool {
        match self {
            Self::ContainerInfo { copied, .. } | Self::GitHubContext { copied, .. } => {
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
            Self::ContainerInfo { copied: true, .. } | Self::GitHubContext { copied: true, .. }
        )
    }

    /// Derive the active "copy this value" target for read-only info
    /// dialogs. Returns `None` when the dialog variant is not one of
    /// the info-row shapes, or when a `GitHubContext` lookup has not
    /// yet resolved a PR. Borrowing the `copied` flag lets callers
    /// flip it inline alongside emitting the clipboard action; the
    /// `row_offset` lets `handle_click` / `clickable_at` hit-test the
    /// same row the renderer painted.
    fn copy_target<'a>(
        &'a mut self,
        github: Option<&GithubContextView<'_>>,
    ) -> Option<CopyTarget<'a>> {
        match self {
            Self::ContainerInfo {
                container_name,
                copied,
                ..
            } => Some(CopyTarget {
                payload: container_name.clone(),
                copied,
                row_offset: CONTAINER_INFO_ID_ROW,
            }),
            Self::GitHubContext { copied } => {
                let url = github.and_then(|view| view.status.loaded())?.url.clone();
                Some(CopyTarget {
                    payload: url,
                    copied,
                    row_offset: GITHUB_CONTEXT_URL_ROW,
                })
            }
            _ => None,
        }
    }
}

struct CopyTarget<'a> {
    payload: String,
    copied: &'a mut bool,
    row_offset: u16,
}

/// `box_row + row_offset` is the row of an emphasized / clickable value
/// inside an info-style dialog (Container info row 2, GitHub context
/// URL row 5). Two-column inset on each side so the border / padding
/// isn't treated as a hit.
fn info_box_value_row_clickable(
    row: u16,
    col: u16,
    box_row: u16,
    box_col: u16,
    width: u16,
    row_offset: u16,
) -> bool {
    let start = box_col.saturating_add(2);
    let end = box_col.saturating_add(width.saturating_sub(2));
    row == box_row.saturating_add(row_offset) && col >= start && col < end
}

const CONTAINER_INFO_ID_ROW: u16 = 2;
const GITHUB_CONTEXT_URL_ROW: u16 = 5;

/// Edit a rename-tab input buffer in response to a raw key chunk.
/// Enter commits, Esc cancels, Backspace removes the trailing char,
/// any other printable ASCII char appends. Length cap and printable
/// filter live inside `jackin_tui::TextField` so this handler only
/// needs to dispatch key bytes — the buffer math is shared with the
/// console TUI surface.
fn rename_tab_handle_key(
    tab_idx: usize,
    input: &mut jackin_tui::TextField,
    key: &[u8],
) -> DialogAction {
    match key {
        b"\x1b" | b"\x03" => DialogAction::Dismiss,
        b"\r" | b"\n" => DialogAction::RenameTab {
            tab_idx,
            label: input.trimmed_value(),
        },
        b"\x7f" | b"\x08" => {
            input.backspace();
            DialogAction::Redraw
        }
        _ => {
            // Accept any valid UTF-8 chunk one char at a time so CJK /
            // emoji / combining-mark labels reach `TextField` and match
            // the unicode-width measurement `lay_out_tabs` uses for
            // tab-strip rendering. C0 controls (other than the explicit
            // Esc / Enter / Backspace arms above) and invalid UTF-8
            // chunks fall through as a Redraw no-op.
            let Ok(s) = std::str::from_utf8(key) else {
                return DialogAction::Redraw;
            };
            for ch in s.chars() {
                if (ch.is_control() && ch != '\t') || ch == '\0' {
                    continue;
                }
                input.insert_char(ch);
            }
            DialogAction::Redraw
        }
    }
}

fn is_arrow_up(key: &[u8]) -> bool {
    matches!(key, b"\x1b[A" | b"\x1bOA")
}

fn is_arrow_down(key: &[u8]) -> bool {
    matches!(key, b"\x1b[B" | b"\x1bOB")
}

/// Universal dialog-dismiss keys. Operators reach for `Esc` and `q`
/// most often, but Backspace, Delete, and `Ctrl+C` are common
/// muscle-memory fallbacks. Uppercase `Q` is included so a shift-key
/// slip doesn't trap the operator inside the dialog. Read-only
/// dialogs (`ContainerInfo`) use this set; filterable list dialogs
/// (`CommandPalette`, `AgentPicker`) use the narrower
/// `is_filter_dismiss_key` because Backspace builds the filter and
/// `q` types into it.
fn is_dismiss_key(key: &[u8]) -> bool {
    matches!(
        key,
        b"\x1b"      // Esc
        | b"q"
        | b"Q"
        | b"\x03"   // Ctrl+C
        | b"\x7f"   // Backspace
        | b"\x08" // Ctrl+H / older Backspace
    )
}

/// Narrow dismiss set for type-to-filter dialogs. Only Esc and
/// Ctrl+C close the dialog — every other key either navigates the
/// filtered list, confirms the selection, or builds the filter.
fn is_filter_dismiss_key(key: &[u8]) -> bool {
    matches!(key, b"\x1b" | b"\x03")
}

fn is_backspace(key: &[u8]) -> bool {
    matches!(key, b"\x7f" | b"\x08")
}

fn is_enter(key: &[u8]) -> bool {
    matches!(key, b"\r" | b"\n")
}

/// Filterable dialogs accept printable ASCII (0x20..=0x7e) as filter
/// input. Multi-byte sequences fall through as no-op redraws — they
/// were already classified by the parser (or arrived unrecognised),
/// and feeding them into the filter would garble the visible typing
/// state. Operators who need non-ASCII filtering can fall back to
/// arrow navigation.
fn printable_filter_char(key: &[u8]) -> Option<char> {
    if key.len() != 1 {
        return None;
    }
    let b = key[0];
    if (0x20..=0x7e).contains(&b) {
        Some(b as char)
    } else {
        None
    }
}

/// Indices into `CLOSE_TARGET_ITEMS` whose label contains `filter`
/// as a case-insensitive substring. Empty filter returns every item.
fn close_target_filtered_indices(filter: &str) -> Vec<usize> {
    let needle = filter.to_ascii_lowercase();
    CLOSE_TARGET_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, (_, label))| needle.is_empty() || label.to_ascii_lowercase().contains(&needle))
        .map(|(idx, _)| idx)
        .collect()
}

/// Indices into `SPLIT_DIRECTION_ITEMS` whose label contains `filter`
/// as a case-insensitive substring. Empty filter returns every item.
fn split_direction_filtered_indices(filter: &str) -> Vec<usize> {
    let needle = filter.to_ascii_lowercase();
    SPLIT_DIRECTION_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, dir)| needle.is_empty() || dir.label().to_ascii_lowercase().contains(&needle))
        .map(|(idx, _)| idx)
        .collect()
}

/// One renderable row inside an `AgentPicker` after filtering. The
/// `Section` variant carries a non-selectable label that groups the
/// selectable rows beneath it ("agents" before agent rows, "shells"
/// before the shell row) so the operator visually distinguishes the
/// two kinds of session jackin can spawn. Future shell variants
/// (zsh, bash, fish) will land under the same "shells" section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerRow {
    Section(&'static str),
    Agent(usize),
    Shell,
}

impl PickerRow {
    fn is_selectable(self) -> bool {
        !matches!(self, Self::Section(_))
    }
}

/// Filtered + grouped row list for the current input. Two groups —
/// "agents" first, "shells" last — separated by section labels.
/// A group whose items have all been filtered out is dropped entirely
/// (label included) so the dialog never paints a "shells" header
/// with no items underneath it. Each item passes the filter when its
/// display label (via `jackin_tui::agent_display_name` for agents,
/// the literal `"Shell"` for the shell row) contains the filter as a
/// case-insensitive substring.
fn picker_filtered_rows(agents: &[String], filter: &str) -> Vec<PickerRow> {
    let needle = filter.to_ascii_lowercase();
    let agent_matches: Vec<PickerRow> = agents
        .iter()
        .enumerate()
        .filter(|(_, slug)| {
            let label = jackin_tui::agent_display_name(slug.as_str()).unwrap_or(slug.as_str());
            needle.is_empty() || label.to_ascii_lowercase().contains(&needle)
        })
        .map(|(idx, _)| PickerRow::Agent(idx))
        .collect();
    let shell_match = needle.is_empty() || "shell".contains(&needle);

    let mut out = Vec::with_capacity(agent_matches.len() + 3);
    if !agent_matches.is_empty() {
        out.push(PickerRow::Section("agents"));
        out.extend(agent_matches);
    }
    if shell_match {
        out.push(PickerRow::Section("shells"));
        out.push(PickerRow::Shell);
    }
    out
}

/// First selectable index in `rows`, or `0` when the list is empty
/// (the caller never indexes into an empty list because the render
/// path paints nothing in that state).
fn first_selectable_idx(rows: &[PickerRow]) -> usize {
    rows.iter().position(|r| r.is_selectable()).unwrap_or(0)
}

/// Advance `from` to the next selectable index in `from..rows.len()`
/// when `forward = true`, or to the previous selectable in `0..from`
/// when `false`. Clamps at the bounds (no wrap). Section rows are
/// skipped transparently so an arrow keypress moves from one item
/// to the next without parking on a label.
fn step_selectable(rows: &[PickerRow], from: usize, forward: bool) -> usize {
    if rows.is_empty() {
        return 0;
    }
    let last = rows.len() - 1;
    let mut idx = from.min(last);
    loop {
        let next = if forward {
            if idx >= last {
                break;
            }
            idx + 1
        } else if idx == 0 {
            break;
        } else {
            idx - 1
        };
        idx = next;
        if rows[idx].is_selectable() {
            return idx;
        }
    }
    // Reached an edge while skipping sections. Fall back to whatever
    // the nearest selectable in the opposite direction is so the
    // selection never lands on a label.
    if rows[idx].is_selectable() {
        idx
    } else if forward {
        // Walk back to find a selectable.
        (0..idx)
            .rev()
            .find(|&i| rows[i].is_selectable())
            .unwrap_or(idx)
    } else {
        (idx + 1..rows.len())
            .find(|&i| rows[i].is_selectable())
            .unwrap_or(idx)
    }
}

use jackin_tui::{HintSpan, hint_row_cols};

/// Hint shown in the main pane view when no dialog is open.
const MAIN_VIEW_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Ctrl+\\"),
    HintSpan::Text("menu"),
    HintSpan::GroupSep,
    HintSpan::Key("↑↓"),
    HintSpan::Text("scroll"),
    HintSpan::GroupSep,
    HintSpan::Key("click"),
    HintSpan::Text("focus pane"),
];

/// Hint shown when the operator is in scrollback mode.
const SCROLLBACK_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("scroll"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("exit scrollback"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl+\\"),
    HintSpan::Text("menu"),
];

/// Return the appropriate hint spans for the main view (no dialog open).
pub(crate) fn main_view_hint(scrollback_active: bool) -> &'static [HintSpan<'static>] {
    if scrollback_active {
        SCROLLBACK_HINT
    } else {
        MAIN_VIEW_HINT
    }
}

const PALETTE_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type filter"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
    HintSpan::Text("select"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
];

const PICKER_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type filter"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
    HintSpan::Text("launch"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
];

const RENAME_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
    HintSpan::GroupSep,
    HintSpan::Text("empty = auto name"),
];

const CONTAINER_INFO_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("copy container ID"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("dismiss"),
];

const GITHUB_CONTEXT_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("copy GitHub URL"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("dismiss"),
];

const READ_ONLY_HINT: &[HintSpan<'static>] = &[HintSpan::Key("Esc"), HintSpan::Text("dismiss")];

const CONFIRM_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Y"),
    HintSpan::Text("confirm"),
    HintSpan::GroupSep,
    HintSpan::Key("N"),
    HintSpan::Text("cancel"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("back"),
];

/// Render the tab-rename modal. One text-input row showing the current
/// buffer plus a blinking-style trailing `▌` caret.
fn render_rename_tab(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16, input: &str) {
    let cursor_byte = input.len();
    jackin_tui::ansi::render_text_input_dialog(
        buf,
        term_rows,
        term_cols,
        "Rename tab",
        input,
        cursor_byte,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_palette(
    buf: &mut Vec<u8>,
    start_row: u16,
    start_col: u16,
    height: u16,
    width: u16,
    selected: usize,
    filter: &str,
    close_label: PaletteCloseLabel,
) {
    render_box(buf, start_row, start_col, height, width, "Menu");
    render_filter_input(buf, start_row + 1, start_col + 1, width, filter);
    // Items occupy the rows below the filter + separator pad
    // (`start_row + 3` onward). Clamp by the available interior so
    // a tight-fit terminal never paints past the bottom border.
    let interior_items = height.saturating_sub(4) as usize;
    let visible = palette_filtered_indices(filter, close_label);
    let drawn = visible.len().min(interior_items);
    if drawn == 0 {
        render_no_matches_row(buf, start_row + 3, start_col + 1, width);
        return;
    }
    for (i, &source_idx) in visible.iter().enumerate().take(drawn) {
        let (command, label) = &PALETTE_ITEMS[source_idx];
        let label = palette_item_label(command, label, close_label);
        render_row(
            buf,
            start_row + 3 + i as u16,
            start_col + 1,
            width,
            label,
            i == selected,
        );
    }
}

fn render_split_direction_picker(
    buf: &mut Vec<u8>,
    start_row: u16,
    start_col: u16,
    height: u16,
    width: u16,
    selected: usize,
    filter: &str,
) {
    render_box(buf, start_row, start_col, height, width, "Split pane");
    render_filter_input(buf, start_row + 1, start_col + 1, width, filter);
    let interior_items = height.saturating_sub(4) as usize;
    let visible = split_direction_filtered_indices(filter);
    let drawn = visible.len().min(interior_items);
    for (i, &source_idx) in visible.iter().enumerate().take(drawn) {
        let label = SPLIT_DIRECTION_ITEMS[source_idx].label();
        render_row(
            buf,
            start_row + 3 + i as u16,
            start_col + 1,
            width,
            label,
            i == selected,
        );
    }
}

fn render_close_target_picker(
    buf: &mut Vec<u8>,
    start_row: u16,
    start_col: u16,
    height: u16,
    width: u16,
    selected: usize,
    filter: &str,
) {
    render_box(buf, start_row, start_col, height, width, "Close");
    render_filter_input(buf, start_row + 1, start_col + 1, width, filter);
    let interior_items = height.saturating_sub(4) as usize;
    let visible = close_target_filtered_indices(filter);
    let drawn = visible.len().min(interior_items);
    if drawn == 0 {
        render_no_matches_row(buf, start_row + 3, start_col + 1, width);
        return;
    }
    for (i, &source_idx) in visible.iter().enumerate().take(drawn) {
        let (_, label) = CLOSE_TARGET_ITEMS[source_idx];
        render_row(
            buf,
            start_row + 3 + i as u16,
            start_col + 1,
            width,
            label,
            i == selected,
        );
    }
}

/// Canonical jackin' confirm dialog — must visually match the host
/// console's `widgets::confirm::render` so the operator sees the
/// same shape on both surfaces. Layout (per the TUI design rule
/// "Confirmation dialogs use the canonical Yes/No layout"):
///   ┌─ Confirm ─────────────┐
///   │                       │   ← pad
///   │     <question?>       │   ← question, centered, bold white
///   │                       │   ← pad
///   │     <explanation>     │   ← optional, centered, dim
///   │                       │   ← pad
///   │      Yes      No      │   ← buttons, centered, focused = WHITE bg
///   │                       │   ← pad
///   └───────────────────────┘
/// Default focus = `No` (safer for destructive arms — Enter on a
/// freshly-opened confirm won't fire the action). The dispatch in
/// `apply_dialog_action` reads `selected_yes` so changing the
/// rendered button labels alone never affects semantics.
fn render_confirm_action(
    buf: &mut Vec<u8>,
    start_row: u16,
    start_col: u16,
    height: u16,
    width: u16,
    kind: ConfirmKind,
    selected_yes: bool,
) {
    render_box(buf, start_row, start_col, height, width, "Confirm");
    let interior_left = start_col + 1;
    let interior_cols = width.saturating_sub(2) as usize;

    // Question — bold white, centered. Falls back gracefully when
    // the box is narrower than the question by clipping; the
    // dialog-rect calculation in `Dialog::natural_height` keeps the
    // box wide enough for the longest configured `ConfirmKind::title`.
    render_centered_line(
        buf,
        start_row + 2,
        interior_left,
        interior_cols,
        kind.title(),
        FG_WHITE,
        true,
    );

    // Explanation — dim, wrapped to one line so the button row
    // stays at `height - 2` regardless of message length. Operators
    // who need the full message in `--debug` get it on stdout via
    // the dispatch breadcrumbs.
    let wrapped = wrap_two_lines(kind.message(), interior_cols.saturating_sub(4));
    if let Some(line) = wrapped.first() {
        render_centered_line(
            buf,
            start_row + 4,
            interior_left,
            interior_cols,
            line,
            FG_DIM,
            false,
        );
    }

    // Button row: "  Yes      No  " centred. Focused button gets
    // WHITE bg + BLACK fg + bold; unfocused stays green-on-dark so
    // only the focus pops. Matches host `widgets::confirm::render`.
    let yes_label = "  Yes  ";
    let gap = "    ";
    let no_label = "  No  ";
    let buttons_w = yes_label.chars().count() + gap.chars().count() + no_label.chars().count();
    let button_col = interior_left + (interior_cols.saturating_sub(buttons_w) / 2) as u16;
    // Place buttons at height-3 to leave one trailing spacer row before the bottom border.
    let button_row = start_row + height.saturating_sub(3);
    move_to(buf, button_row, button_col);
    write_confirm_button(buf, yes_label, selected_yes);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.extend_from_slice(gap.as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
    write_confirm_button(buf, no_label, !selected_yes);
}

/// Centered text on a single dialog interior row. `width` is the
/// inner column count (between the box borders); the helper pads on
/// both sides with `BG_DARK` so the row stays uniform background.
fn render_centered_line(
    buf: &mut Vec<u8>,
    row: u16,
    col: u16,
    width: usize,
    text: &str,
    fg_color: &str,
    bold: bool,
) {
    let len = text.chars().count().min(width);
    let lpad = width.saturating_sub(len) / 2;
    let rpad = width.saturating_sub(lpad + len);
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(fg_color.as_bytes());
    if bold {
        buf.extend_from_slice(BOLD.as_bytes());
    }
    for _ in 0..lpad {
        buf.push(b' ');
    }
    let truncated: String = text.chars().take(len).collect();
    buf.extend_from_slice(truncated.as_bytes());
    for _ in 0..rpad {
        buf.push(b' ');
    }
    buf.extend_from_slice(RESET.as_bytes());
}

/// Confirm Yes/No button cell. Focused = WHITE bg + BLACK fg +
/// BOLD; unfocused = green-on-dark + BOLD. Caller positions cursor
/// with `move_to` before calling.
fn write_confirm_button(buf: &mut Vec<u8>, label: &str, focused: bool) {
    if focused {
        buf.extend_from_slice(CONFIRM_BG.as_bytes());
        buf.extend_from_slice(SELECT_FG.as_bytes());
    } else {
        buf.extend_from_slice(BG_DARK.as_bytes());
        buf.extend_from_slice(FG_GREEN.as_bytes());
    }
    buf.extend_from_slice(BOLD.as_bytes());
    buf.extend_from_slice(label.as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
}

#[allow(clippy::too_many_arguments)]
fn render_agent_picker(
    buf: &mut Vec<u8>,
    start_row: u16,
    start_col: u16,
    height: u16,
    width: u16,
    agents: &[String],
    selected: usize,
    intent: PickerIntent,
    filter: &str,
) {
    let title: String = match intent {
        PickerIntent::NewTab => "New tab".to_string(),
        PickerIntent::Split(direction) => format!("Split: {}", direction.label()),
    };
    render_box(buf, start_row, start_col, height, width, &title);
    render_filter_input(buf, start_row + 1, start_col + 1, width, filter);

    // Items occupy the rows below the filter + separator pad
    // (`start_row + 3` onward). Each row maps back to PickerRow so
    // an Agent / Shell distinction stays explicit even after
    // filtering rearranges the list. Section rows render as
    // non-selectable group labels ("── agents ──", "── shells ──").
    let interior_items = height.saturating_sub(4) as usize;
    let visible = picker_filtered_rows(agents, filter);
    let drawn = visible.len().min(interior_items);
    if drawn == 0 {
        return;
    }
    for (i, row) in visible.iter().enumerate().take(drawn) {
        let target_row = start_row + 3 + i as u16;
        match row {
            PickerRow::Section(label) => {
                render_separator(buf, target_row, start_col + 1, width, label);
            }
            PickerRow::Agent(idx) => {
                let label = jackin_tui::agent_display_name(agents[*idx].as_str())
                    .unwrap_or(agents[*idx].as_str());
                render_row(buf, target_row, start_col + 1, width, label, i == selected);
            }
            PickerRow::Shell => {
                render_row(
                    buf,
                    target_row,
                    start_col + 1,
                    width,
                    "Shell",
                    i == selected,
                );
            }
        }
    }
}

fn render_provider_picker(
    buf: &mut Vec<u8>,
    start_row: u16,
    start_col: u16,
    height: u16,
    width: u16,
    providers: &[ProviderChoice],
    selected: usize,
) {
    render_box(buf, start_row, start_col, height, width, "Choose provider");
    let interior_items = height.saturating_sub(2) as usize;
    let drawn = providers.len().min(interior_items);
    for (i, provider) in providers.iter().enumerate().take(drawn) {
        render_row(
            buf,
            start_row + 1 + i as u16,
            start_col + 1,
            width,
            provider.label.as_str(),
            i == selected,
        );
    }
}

/// Non-selectable group divider — `── agents ──` / `── shells ──` in
/// dim phosphor-green with PHOSPHOR_DARK dashes. Sets the operator's
/// expectation that rows above and below the divider are different
/// *kinds* of session jackin can spawn, not just neighbouring items
/// in a flat list. Future shell variants (zsh, bash, fish) will sit
/// under the "shells" divider without restructuring the renderer.
fn render_separator(buf: &mut Vec<u8>, row: u16, col: u16, width: u16, label: &str) {
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
    let interior = (width as usize).saturating_sub(2);
    let label_with_pad = format!(" {label} ");
    let label_cols = label_with_pad.chars().count();
    let total_dashes = interior.saturating_sub(label_cols);
    let left_dashes = total_dashes / 2;
    let right_dashes = total_dashes - left_dashes;
    for _ in 0..left_dashes {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice(FG_DIM.as_bytes());
    buf.extend_from_slice(label_with_pad.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
    for _ in 0..right_dashes {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice(RESET.as_bytes());
}

/// Filter input row. Empty filter shows a 20-character `░` placeholder
/// (`U+2591 LIGHT SHADE`) in `PHOSPHOR_DARK`; populated filter shows
/// the typed text in white followed by a `█` (`U+2588 FULL BLOCK`)
/// caret. Both halves stay inside `Filter: ` (label in `PHOSPHOR_DIM`).
fn render_filter_input(buf: &mut Vec<u8>, row: u16, col: u16, width: u16, filter: &str) {
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_DIM.as_bytes());
    let label = "Filter: ";
    buf.extend_from_slice(label.as_bytes());
    let label_cols = label.chars().count();
    let mut filled = label_cols;
    if filter.is_empty() {
        buf.extend_from_slice(FG_BORDER.as_bytes());
        for _ in 0..20 {
            buf.extend_from_slice("░".as_bytes());
        }
        filled += 20;
    } else {
        buf.extend_from_slice(FG_WHITE.as_bytes());
        buf.extend_from_slice(filter.as_bytes());
        buf.extend_from_slice(FG_WHITE.as_bytes());
        buf.extend_from_slice(BOLD.as_bytes());
        buf.extend_from_slice("█".as_bytes());
        filled += filter.chars().count() + 1;
    }
    // Pad to right border so leftover chars from a longer filter
    // round-trip cleanly when the operator hits Backspace.
    buf.extend_from_slice(RESET.as_bytes());
    buf.extend_from_slice(BG_DARK.as_bytes());
    let interior = (width as usize).saturating_sub(2);
    for _ in filled..interior {
        buf.push(b' ');
    }
    buf.extend_from_slice(RESET.as_bytes());
}

/// Render a dim centered "no matches" placeholder when the picker filter
/// returns no items — consistent with the host console pickers (Defect 32).
fn render_no_matches_row(buf: &mut Vec<u8>, row: u16, col: u16, width: u16) {
    use std::io::Write as _;
    let text = "no matches";
    let text_len = text.len() as u16;
    let x = col.saturating_add(width.saturating_sub(text_len) / 2);
    jackin_tui::ansi::move_to(buf, row, x);
    jackin_tui::ansi::fg(buf, jackin_tui::PHOSPHOR_DIM);
    let _ = write!(buf, "{text}");
    buf.extend_from_slice(jackin_tui::ansi::RESET.as_bytes());
}

/// Render one row of a palette/picker list at `(row, col)` spanning
/// `width-2` columns. Mirrors the console TUI sidebar style: selected
/// rows get the phosphor-green highlight bar with black text and a
/// `▸ ` marker; unselected rows get phosphor-green text on black.
fn render_row(buf: &mut Vec<u8>, row: u16, col: u16, width: u16, label: &str, selected: bool) {
    move_to(buf, row, col);
    if selected {
        buf.extend_from_slice(SELECT_BG.as_bytes());
        buf.extend_from_slice(SELECT_FG.as_bytes());
        buf.extend_from_slice(BOLD.as_bytes());
        buf.extend_from_slice(SELECT_MARK.as_bytes());
    } else {
        buf.extend_from_slice(BG_DARK.as_bytes());
        buf.extend_from_slice(FG_GREEN.as_bytes());
        buf.extend_from_slice(UNSELECT_MARK.as_bytes());
    }
    // Row interior is `width - 2` cols (excluding both side borders).
    // The marker takes the first 2; the label and trailing pad fill
    // the remaining `width - 4`. Drawing one cell more here would
    // overwrite the right border `│` painted by `render_box`,
    // making the dialog look like its right edge dropped out.
    let max_label_cols = (width as usize).saturating_sub(4);
    let label_cols = label.chars().count();
    let truncated_cols = label_cols.min(max_label_cols);
    let label_take: String = label.chars().take(truncated_cols).collect();
    buf.extend_from_slice(label_take.as_bytes());
    let pad_cols = max_label_cols.saturating_sub(truncated_cols);
    for _ in 0..pad_cols {
        buf.push(b' ');
    }
    buf.extend_from_slice(RESET.as_bytes());
}

/// Render the read-only ContainerInfo modal. Label/value rows live
/// inside the standard `render_box` chrome.
/// The container ID is rendered in white-bold to flag it as the copy
/// target the footer hint advertises. No selection state — Enter / a
/// click on the Container ID row copies the ID via OSC 52; Esc / q
/// dismisses.
#[allow(clippy::too_many_arguments)]
fn render_container_info(
    buf: &mut Vec<u8>,
    box_row: u16,
    box_col: u16,
    height: u16,
    width: u16,
    container_name: &str,
    role: &str,
    focused_agent: Option<&str>,
    workdir: &str,
    diagnostics: &ContainerInfoDiagnostics,
    copied: bool,
    copy_target_hovered: bool,
) {
    render_box(buf, box_row, box_col, height, width, "Debug info");

    let capsule_ver = env!("JACKIN_CAPSULE_VERSION");

    if crate::logging::debug_enabled() {
        let run_id_display = if diagnostics.run_id.is_empty() {
            "(not set)".to_string()
        } else {
            diagnostics.run_id.clone()
        };
        let run_log_href_ref = diagnostics.run_log_href.as_deref();
        let rows: [ContainerInfoRow<'_>; 8] = [
            ContainerInfoRow::new("Container ID", container_name.to_string()).emphasised(),
            ContainerInfoRow::new("Role", non_empty_or_dim(role)),
            ContainerInfoRow::new("Agent", non_empty_or_dim(focused_agent.unwrap_or(""))),
            ContainerInfoRow::new("Workdir", non_empty_or_dim(workdir)),
            ContainerInfoRow::new("jackin", diagnostics.host_version.clone()),
            ContainerInfoRow::new("jackin-capsule", capsule_ver.to_string()),
            ContainerInfoRow::new("Run ID", run_id_display),
            ContainerInfoRow::new("Run log", diagnostics.run_log_display.clone())
                .hyperlink(run_log_href_ref),
        ];
        render_info_rows(
            buf,
            box_row,
            box_col,
            width,
            &rows,
            copied.then_some(0),
            copy_target_hovered.then_some(0),
        );
    } else {
        let rows: [ContainerInfoRow<'_>; 6] = [
            ContainerInfoRow::new("Container ID", container_name.to_string()).emphasised(),
            ContainerInfoRow::new("Role", non_empty_or_dim(role)),
            ContainerInfoRow::new("Agent", non_empty_or_dim(focused_agent.unwrap_or(""))),
            ContainerInfoRow::new("Workdir", non_empty_or_dim(workdir)),
            ContainerInfoRow::new("jackin", diagnostics.host_version.clone()),
            ContainerInfoRow::new("jackin-capsule", capsule_ver.to_string()),
        ];
        render_info_rows(
            buf,
            box_row,
            box_col,
            width,
            &rows,
            copied.then_some(0),
            copy_target_hovered.then_some(0),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_github_context(
    buf: &mut Vec<u8>,
    box_row: u16,
    box_col: u16,
    height: u16,
    width: u16,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    copied: bool,
    copy_target_hovered: bool,
) {
    render_box(buf, box_row, box_col, height, width, "GitHub context");
    let none_placeholder = if pull_request_loading {
        "resolving…"
    } else {
        "(none)"
    };
    let unknown_placeholder = if pull_request_loading {
        "resolving…"
    } else {
        "(unknown)"
    };
    let pull_request_number = pull_request
        .map(PullRequestInfo::number_label)
        .unwrap_or_else(|| none_placeholder.to_string());
    let pull_request_title = pull_request
        .map(|pr| non_empty_or_dim(&pr.title))
        .unwrap_or_else(|| none_placeholder.to_string());
    let (pull_request_link, pull_request_href) = pull_request
        .map(|pr| (non_empty_or_dim(&pr.url), Some(pr.url.as_str())))
        .unwrap_or_else(|| (none_placeholder.to_string(), None));
    let ci_status = pull_request
        .and_then(|pr| pr.checks.as_ref())
        .map(|checks| checks.summary())
        .unwrap_or_else(|| unknown_placeholder.to_string());

    let rows: [ContainerInfoRow; 5] = [
        ContainerInfoRow::new("Branch", non_empty_or_dim(branch.unwrap_or(""))),
        ContainerInfoRow::new("Pull Request", pull_request_number),
        ContainerInfoRow::new("PR Title", pull_request_title),
        ContainerInfoRow::new("GitHub URL", pull_request_link)
            .hyperlink(pull_request_href)
            .emphasised(),
        ContainerInfoRow::new("CI Status", ci_status),
    ];
    render_info_rows(
        buf,
        box_row,
        box_col,
        width,
        &rows,
        copied.then_some(3),
        copy_target_hovered.then_some(3),
    );
}

fn render_info_rows(
    buf: &mut Vec<u8>,
    box_row: u16,
    box_col: u16,
    width: u16,
    rows: &[ContainerInfoRow<'_>],
    copied_row: Option<usize>,
    hovered_row: Option<usize>,
) {
    let label_col_width = rows
        .iter()
        .map(|row| row.label.chars().count())
        .max()
        .unwrap_or(0);
    let interior_left = box_col + 2;
    let interior_max_cols = (width as usize).saturating_sub(4);
    let value_col_offset = label_col_width + 2; // 2 = ": "
    let value_max_cols = interior_max_cols.saturating_sub(value_col_offset);

    for (i, row) in rows.iter().enumerate() {
        let r = box_row + 2 + i as u16;
        move_to(buf, r, interior_left);
        buf.extend_from_slice(BG_DARK.as_bytes());
        buf.extend_from_slice(FG_BORDER.as_bytes());
        buf.extend_from_slice(row.label.as_bytes());
        for _ in row.label.chars().count()..label_col_width {
            buf.push(b' ');
        }
        buf.extend_from_slice(b": ");
        if row.emphasise {
            if hovered_row == Some(i) {
                buf.extend_from_slice(FG_CLICK_HOVER.as_bytes());
            } else {
                buf.extend_from_slice(FG_WHITE.as_bytes());
            }
            buf.extend_from_slice(BOLD.as_bytes());
        } else {
            buf.extend_from_slice(FG_GREEN.as_bytes());
        }
        let badge = if copied_row == Some(i) {
            "  ✓ Copied!"
        } else {
            ""
        };
        let badge_cols = badge.chars().count();
        let available_value_cols = if badge.is_empty() {
            value_max_cols
        } else {
            value_max_cols.saturating_sub(badge_cols)
        };
        let value_cols = row.value.chars().count().min(available_value_cols);
        let value_take: String = row.value.chars().take(value_cols).collect();
        if let Some(href) = row.href {
            jackin_tui::ansi::emit_osc8_open(buf, href);
            buf.extend_from_slice(value_take.as_bytes());
            jackin_tui::ansi::emit_osc8_close(buf);
        } else {
            buf.extend_from_slice(value_take.as_bytes());
        }
        // Trailing "Copied!" badge on the Container ID row reserves
        // space before truncating the container name so long IDs still
        // show copy feedback.
        if !badge.is_empty() {
            let consumed = label_col_width + 2 /* ": " */ + value_cols;
            if consumed + badge_cols <= interior_max_cols {
                buf.extend_from_slice(RESET.as_bytes());
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_GREEN.as_bytes());
                buf.extend_from_slice(BOLD.as_bytes());
                buf.extend_from_slice(badge.as_bytes());
            }
        }
        buf.extend_from_slice(RESET.as_bytes());
    }
}

fn dialog_list_row_clickable(row: u16, box_row: u16, visible_count: usize) -> bool {
    let first_item_row = box_row + 3;
    row >= first_item_row && row < first_item_row + visible_count as u16
}

fn wrap_two_lines(text: &str, max_cols: usize) -> [String; 2] {
    if max_cols == 0 {
        return [String::new(), String::new()];
    }
    let mut lines = [String::new(), String::new()];
    let mut line_idx = 0usize;
    for word in text.split_whitespace() {
        let word_cols = word.chars().count();
        let line_cols = lines[line_idx].chars().count();
        let sep_cols = usize::from(line_cols > 0);
        if line_cols + sep_cols + word_cols > max_cols && line_idx == 0 {
            line_idx = 1;
        }
        if !lines[line_idx].is_empty() {
            lines[line_idx].push(' ');
        }
        let remaining = max_cols.saturating_sub(lines[line_idx].chars().count());
        lines[line_idx].extend(word.chars().take(remaining));
    }
    lines
}

fn render_box(buf: &mut Vec<u8>, row: u16, col: u16, height: u16, width: u16, title: &str) {
    // Top border with white-bold title.
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.extend_from_slice("┌".as_bytes());
    buf.extend_from_slice("─".as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(FG_WHITE.as_bytes());
    buf.extend_from_slice(BOLD.as_bytes());
    buf.extend_from_slice(title.as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.push(b' ');
    let title_cols = title.chars().count() as u16;
    let consumed = 1 /* ┌ */ + 1 /* ─ */ + 1 /* space */ + title_cols + 1 /* space */;
    for _ in consumed..(width - 1) {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice("┐".as_bytes());

    // Side borders + interior.
    for r in 1..(height - 1) {
        move_to(buf, row + r, col);
        buf.extend_from_slice(BG_DARK.as_bytes());
        buf.extend_from_slice(FG_GREEN.as_bytes());
        buf.extend_from_slice("│".as_bytes());
        for _ in 1..(width - 1) {
            buf.push(b' ');
        }
        buf.extend_from_slice("│".as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
    }

    // Bottom border.
    move_to(buf, row + height - 1, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.extend_from_slice("└".as_bytes());
    for _ in 1..(width - 1) {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice("┘".as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
}

/// Compute the visual column width of a hint span row. Matches the
/// formatting in `render_hint_row` so centring is exact.
pub(crate) fn render_hint_row(buf: &mut Vec<u8>, row: u16, term_cols: u16, spans: &[HintSpan<'_>]) {
    let total = hint_row_cols(spans);
    let padded_total = total.saturating_add(4);
    if padded_total > term_cols as usize {
        return;
    }
    let start_col = ((term_cols as usize).saturating_sub(padded_total) / 2) as u16;
    move_to(buf, row, 0);
    buf.extend_from_slice(BG_DARK.as_bytes());
    for _ in 0..term_cols {
        buf.push(b' ');
    }
    move_to(buf, row, start_col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
    buf.extend_from_slice("  ".as_bytes());
    for span in spans {
        match span {
            HintSpan::Key(k) => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_WHITE.as_bytes());
                buf.extend_from_slice(BOLD.as_bytes());
                buf.extend_from_slice(k.as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::Text(t) => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_GREEN.as_bytes());
                buf.push(b' ');
                buf.extend_from_slice(t.as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::Dyn(t) => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_DIM.as_bytes());
                buf.push(b' ');
                buf.extend_from_slice(t.as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::Sep => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_BORDER.as_bytes());
                buf.extend_from_slice(" · ".as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::GroupSep => {
                buf.extend_from_slice("   ".as_bytes());
            }
        }
    }
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
    buf.extend_from_slice("  ".as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
}

fn move_to(buf: &mut Vec<u8>, row: u16, col: u16) {
    buf.extend_from_slice(b"\x1b[");
    write_dec(buf, row + 1);
    buf.push(b';');
    write_dec(buf, col + 1);
    buf.push(b'H');
}

fn write_dec(buf: &mut Vec<u8>, n: u16) {
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let mut tmp = [0u8; 5];
    let mut i = 5;
    let mut v = n;
    while v > 0 {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    buf.extend_from_slice(&tmp[i..]);
}

#[cfg(test)]
mod tests;
