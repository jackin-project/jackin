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
pub(super) use super::palette::{PALETTE_ITEMS, palette_filtered_indices};
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
    PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE,
    ansi::{BG_DARK, BOLD, RESET, rgb_fg},
};

const PALETTE_WIDTH: u16 = 50;
const CONTAINER_INFO_WIDTH: u16 = 86;
const FG_GREEN: &str = rgb_fg(PHOSPHOR_GREEN);
const FG_DIM: &str = rgb_fg(PHOSPHOR_DIM);
const FG_BORDER: &str = rgb_fg(PHOSPHOR_DARK);
const FG_WHITE: &str = rgb_fg(WHITE);

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
        /// Index of the row whose value was just copied (shows "Copied!"),
        /// or `None`. Indexes into the shared `ContainerInfoState` rows.
        copied_row: Option<usize>,
        /// Index of the copyable row under the pointer (link hover colour).
        hovered_row: Option<usize>,
        /// Persisted scroll offsets. The shared `ContainerInfoState` is rebuilt
        /// every frame, so the scroll must live here on the dialog enum to
        /// survive across redraws.
        scroll: jackin_tui::components::DialogBodyScroll,
    },
    /// Read-only modal opened from the bottom branch/PR context.
    /// Branch / PR / loading state come from `GithubContextView` at
    /// render time so a mid-life branch flip reflects without an
    /// explicit refresh step.
    GitHubContext {
        copied: bool,
        /// Persisted scroll offsets (rebuilt each frame like ContainerInfo).
        scroll: jackin_tui::components::DialogBodyScroll,
    },
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
            copied_row: None,
            hovered_row: None,
            scroll: jackin_tui::components::DialogBodyScroll::new(),
        }
    }

    /// Build the shared [`ContainerInfoState`](jackin_tui::components::ContainerInfoState)
    /// for the `ContainerInfo` ("Debug info") dialog from the accumulating
    /// [`DebugInfo`](jackin_tui::components::DebugInfo) model — the single
    /// source of rows/order/labels/copy-affordances shared with the host
    /// console and launch cockpit. Returns `None` for other dialog variants.
    ///
    /// Run id / diagnostics-log rows are included only under `--debug`, matching
    /// the host. Versions are the exact `jackin --version` / `jackin-capsule
    /// --version` strings.
    pub(crate) fn container_info_state(
        &self,
    ) -> Option<jackin_tui::components::ContainerInfoState> {
        let Self::ContainerInfo {
            container_name,
            role,
            focused_agent,
            workdir,
            diagnostics,
            copied_row,
            hovered_row,
            scroll,
        } = self
        else {
            return None;
        };
        let agent_label = focused_agent
            .as_deref()
            .and_then(jackin_tui::agent_display_name)
            .or(focused_agent.as_deref())
            .unwrap_or("(shell)")
            .to_string();
        let debug = crate::logging::debug_enabled() && !diagnostics.run_id.is_empty();
        // Pass the absolute path so the `file://` href the model builds is
        // valid; `run_log_href` already carries it (`file://<abs>`).
        let log_path = debug.then(|| {
            diagnostics
                .run_log_href
                .as_deref()
                .and_then(|href| href.strip_prefix("file://"))
                .map(str::to_string)
                .unwrap_or_else(|| diagnostics.run_log_display.clone())
        });
        let mut state = jackin_tui::components::DebugInfo {
            jackin_version: Some(diagnostics.host_version.clone()),
            capsule_version: Some(env!("JACKIN_CAPSULE_VERSION").to_string()),
            container_id: Some(container_name.clone()),
            role: (!role.is_empty()).then(|| role.clone()),
            agent: Some(agent_label),
            target: (!workdir.is_empty()).then(|| workdir.clone()),
            run_id: debug.then(|| diagnostics.run_id.clone()),
            diagnostics_log_path: log_path,
        }
        .into_state();
        if let Some(row) = *copied_row {
            state.mark_copied(row);
        }
        state.set_hovered_row(*hovered_row);
        state.scroll = scroll.clone();
        Some(state)
    }

    pub fn new_github_context() -> Self {
        Self::GitHubContext {
            copied: false,
            scroll: jackin_tui::components::DialogBodyScroll::new(),
        }
    }

    /// Mutable body-scroll state for the read-only info dialogs whose content
    /// can overflow (ContainerInfo, GitHubContext). `None` for dialogs that do
    /// not scroll. Lets the daemon route mouse-wheel events to the dialog body.
    pub(crate) fn body_scroll_mut(
        &mut self,
    ) -> Option<&mut jackin_tui::components::DialogBodyScroll> {
        match self {
            Self::ContainerInfo { scroll, .. } | Self::GitHubContext { scroll, .. } => Some(scroll),
            _ => None,
        }
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
            // Scroll the read-only body (offsets clamp at render time): Up/Down +
            // k/j vertical, Left/Right + h/l horizontal. The shared state is
            // rebuilt each frame, so the offset lives on the dialog enum.
            let body_scroll = match self {
                Self::ContainerInfo { scroll, .. } | Self::GitHubContext { scroll, .. } => {
                    Some(scroll)
                }
                _ => None,
            };
            if let Some(scroll) = body_scroll {
                if is_arrow_up(key) || key == b"k" || key == b"K" {
                    scroll.scroll_y = scroll.scroll_y.saturating_sub(1);
                    return DialogAction::Redraw;
                }
                if is_arrow_down(key) || key == b"j" || key == b"J" {
                    scroll.scroll_y = scroll.scroll_y.saturating_add(1);
                    return DialogAction::Redraw;
                }
                if key == b"\x1b[D" || key == b"h" || key == b"H" {
                    scroll.scroll_x = scroll.scroll_x.saturating_sub(1);
                    return DialogAction::Redraw;
                }
                if key == b"\x1b[C" || key == b"l" || key == b"L" {
                    scroll.scroll_x = scroll.scroll_x.saturating_add(1);
                    return DialogAction::Redraw;
                }
            }
            return match key {
                b"\r" | b"\n" => {
                    // ContainerInfo: Enter copies the container id (row 0),
                    // matching the "↵ copy container ID" footer hint. Mouse
                    // clicks copy whichever row was clicked (handle_click).
                    if let Self::ContainerInfo {
                        container_name,
                        copied_row,
                        ..
                    } = self
                    {
                        let payload = container_name.clone();
                        *copied_row = Some(0);
                        return DialogAction::CopyToClipboard(payload);
                    }
                    match self.copy_target(github) {
                        Some(target) => {
                            *target.copied = true;
                            DialogAction::CopyToClipboard(target.payload)
                        }
                        None => DialogAction::Redraw,
                    }
                }
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
        // ContainerInfo: any copyable row (Container ID, Run ID, Diagnostics
        // log) copies via the shared hit-test. The clicked row's value goes to
        // the clipboard and that row shows the "Copied!" badge.
        if matches!(self, Self::ContainerInfo { .. }) {
            let area = ratatui::layout::Rect {
                x: box_col,
                y: box_row,
                width,
                height,
            };
            let hit = self.container_info_state().and_then(|state| {
                jackin_tui::components::container_info_copy_payload_at(area, &state, col, row)
            });
            return match hit {
                Some((hit_row, payload)) => {
                    if let Self::ContainerInfo { copied_row, .. } = self {
                        *copied_row = Some(hit_row);
                    }
                    DialogAction::CopyToClipboard(payload)
                }
                None => DialogAction::Consume,
            };
        }
        if matches!(self, Self::GitHubContext { .. }) {
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
            Self::ContainerInfo { .. } => {
                let area = ratatui::layout::Rect {
                    x: box_col,
                    y: box_row,
                    width,
                    height,
                };
                self.container_info_state().is_some_and(|state| {
                    jackin_tui::components::container_info_copy_payload_at(area, &state, col, row)
                        .is_some()
                })
            }
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
            Self::ContainerInfo { .. } => self.container_info_state().map_or(10, |state| {
                jackin_tui::components::container_info_required_height(&state)
            }),
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

    /// Footer hint spans for this dialog. Rendered by the multiplexer
    /// compositor near the bottom chrome so every dialog follows the same
    /// hint contract without competing with the branch/container status row.
    ///
    /// `axes` reflects the dialog body's *actual* per-axis overflow (computed
    /// by the caller from the rendered snapshot + rect), so the scrollable info
    /// dialogs advertise only the scroll direction(s) the operator can move —
    /// never both axes when the body fits one.
    pub(crate) fn footer_hint_spans(
        &self,
        github: Option<&GithubContextView<'_>>,
        axes: jackin_tui::components::ScrollAxes,
    ) -> Vec<HintSpan<'static>> {
        match self {
            Self::CommandPalette { .. } => PALETTE_HINT.to_vec(),
            Self::SplitDirectionPicker { .. }
            | Self::AgentPicker { .. }
            | Self::CloseTargetPicker { .. }
            | Self::ProviderPicker { .. } => PICKER_HINT.to_vec(),
            Self::RenameTab { .. } => RENAME_HINT.to_vec(),
            Self::ContainerInfo { .. } => info_dialog_hint("copy container ID", axes),
            Self::GitHubContext { .. } => {
                if github.and_then(|view| view.status.loaded()).is_some() {
                    info_dialog_hint("copy GitHub URL", axes)
                } else {
                    READ_ONLY_HINT.to_vec()
                }
            }
            Self::ConfirmAction { .. } => CONFIRM_HINT.to_vec(),
        }
    }

    /// Update the hovered copyable row of the `ContainerInfo` dialog from a
    /// pointer hit at `(row, col)` (1-based). Returns true when the hovered
    /// row changed (the caller redraws so the link hover colour updates).
    /// No-op for other dialog variants.
    pub fn set_container_info_hover(
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
        let hit = self.container_info_state().and_then(|state| {
            jackin_tui::components::container_info_copy_payload_at(area, &state, col, row)
                .map(|(idx, _)| idx)
        });
        if let Self::ContainerInfo { hovered_row, .. } = self
            && *hovered_row != hit
        {
            *hovered_row = hit;
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
            // ContainerInfo copy is handled directly via the shared hit-test
            // (handle_click / handle_key) so it can target any copyable row,
            // not a single fixed offset.
            Self::GitHubContext { copied, .. } => {
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

/// Read-only info-dialog hint: copy key, the *available* scroll axes (per
/// `axes`, omitted when the body fits), then dismiss — built from the shared
/// `scroll_hint_spans` primitive so it never advertises a scroll direction the
/// body cannot move. Used by both ContainerInfo (Debug info) and a loaded
/// GitHubContext, which differ only in their copy label.
fn info_dialog_hint(
    copy_label: &'static str,
    axes: jackin_tui::components::ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let mut spans = vec![HintSpan::Key("↵"), HintSpan::Text(copy_label)];
    let scroll = jackin_tui::components::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.push(HintSpan::GroupSep);
        spans.extend(scroll);
    }
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Key("Esc"));
    spans.push(HintSpan::Text("dismiss"));
    spans
}

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

fn dialog_list_row_clickable(row: u16, box_row: u16, visible_count: usize) -> bool {
    let first_item_row = box_row + 3;
    row >= first_item_row && row < first_item_row + visible_count as u16
}

/// Compute the visual column width of a hint span row. Matches the
/// formatting in `render_hint_row` so centring is exact.
pub(crate) fn render_hint_row(buf: &mut Vec<u8>, row: u16, term_cols: u16, spans: &[HintSpan<'_>]) {
    let total = hint_row_cols(spans);
    let padded_total = total.saturating_add(4);
    if padded_total > term_cols as usize {
        crate::cdebug!(
            "hint-row: SKIP row={} term_cols={} content_cols={} padded={} (too wide)",
            row,
            term_cols,
            total,
            padded_total,
        );
        return;
    }
    let start_col = ((term_cols as usize).saturating_sub(padded_total) / 2) as u16;
    crate::cdebug!(
        "hint-row: row={} term_cols={} content_cols={} padded={} start_col={}",
        row,
        term_cols,
        total,
        padded_total,
        start_col,
    );
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
