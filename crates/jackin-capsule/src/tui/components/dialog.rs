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
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug, Clone, Copy)]
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

use jackin_tui::HintSpan;
use jackin_tui::components::{
    CONFIRM_KEYMAP, ConfirmAction as SharedConfirmAction, raw_bytes_to_chord,
};

use crate::tui::keymap::{FILTER_LIST_KEYMAP, FilterListAction, READ_ONLY_DISMISS_KEYMAP};

const PALETTE_WIDTH: u16 = 50;
const CONTAINER_INFO_WIDTH: u16 = 86;
mod input;
use input::{
    PickerRow, close_target_filtered_indices, dialog_list_row_clickable, first_selectable_idx,
    picker_filtered_rows, printable_filter_char, rename_tab_handle_key,
    split_direction_filtered_indices, step_selectable,
};
mod hint;
pub(crate) use hint::main_view_hint;
use hint::{
    confirm_hint, info_dialog_hint, palette_hint, picker_hint, provider_hint, read_only_hint,
    rename_hint, usage_hint,
};

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
        scroll: jackin_tui::components::DialogBodyScroll,
    },
    /// Read-only modal opened from the bottom branch/PR context.
    /// Branch / PR / loading state come from `GithubContextView` at
    /// render time so a mid-life branch flip reflects without an
    /// explicit refresh step.
    GitHubContext {
        copied: bool,
        /// Persisted scroll offsets (rebuilt each frame like `ContainerInfo`).
        scroll: jackin_tui::components::DialogBodyScroll,
    },
    /// Read-only usage/quota modal for the focused pane.
    Usage {
        view: Box<jackin_protocol::control::FocusedUsageView>,
        selected: UsageDialogTab,
        tab_bar_focused: bool,
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
    /// Request a daemon-side focused usage refresh.
    RefreshUsage,
    /// Request a daemon-side usage snapshot for a specific provider tab.
    SwitchUsageProvider { provider_label: String },
    /// Dialog is still open; redraw.
    Redraw,
    /// Mouse event lands somewhere with no semantic effect (border,
    /// padding row). Swallow it so it does not reach the focused pane.
    Consume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDialogTab {
    Overview,
    Provider,
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
        self.container_info_state_with_debug(crate::logging::debug_enabled())
    }

    fn container_info_state_with_debug(
        &self,
        debug_enabled: bool,
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
            .to_owned();
        let debug = debug_enabled && !diagnostics.run_id.is_empty();
        // Pass the absolute path so the `file://` href the model builds is
        // valid; `run_log_href` already carries it (`file://<abs>`).
        let log_path = debug.then(|| {
            diagnostics
                .run_log_href
                .as_deref()
                .and_then(|href| href.strip_prefix("file://"))
                .map_or_else(|| diagnostics.run_log_display.clone(), str::to_owned)
        });
        let mut state = jackin_tui::components::DebugInfo {
            jackin_version: Some(diagnostics.host_version.clone()),
            capsule_version: Some(env!("JACKIN_CAPSULE_VERSION").to_owned()),
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

    pub(crate) fn github_context_state(
        &self,
        github: Option<&GithubContextView<'_>>,
    ) -> Option<jackin_tui::components::ContainerInfoState> {
        let Self::GitHubContext { copied, scroll } = self else {
            return None;
        };
        let branch = github
            .and_then(|view| view.branch)
            .map_or_else(|| "(unknown)".to_owned(), str::to_owned);
        let loading_placeholder =
            if github.is_some_and(|view| matches!(view.status, PullRequestStatus::Resolving)) {
                "resolving…"
            } else {
                "(none)"
            };
        let pr = github.and_then(|view| view.status.loaded());
        let pr_number = pr.map_or_else(
            || loading_placeholder.to_owned(),
            PullRequestInfo::number_label,
        );
        let pr_title = pr.map_or_else(|| loading_placeholder.to_owned(), |p| p.title.clone());
        let pr_url = pr.map_or_else(|| loading_placeholder.to_owned(), |p| p.url.clone());
        let ci = pr.and_then(|p| p.checks.as_ref()).map_or_else(
            || {
                if github.is_some_and(|view| matches!(view.status, PullRequestStatus::Resolving)) {
                    "resolving…"
                } else {
                    "(unknown)"
                }
                .to_owned()
            },
            crate::pull_request::PullRequestChecks::summary,
        );
        let mut rows = vec![
            jackin_tui::components::ContainerInfoRow::new("Branch", branch),
            jackin_tui::components::ContainerInfoRow::new("Pull Request", pr_number),
            jackin_tui::components::ContainerInfoRow::new("PR Title", pr_title),
        ];
        let mut url_row = jackin_tui::components::ContainerInfoRow::new("GitHub URL", pr_url);
        if let Some(pr) = pr {
            url_row = url_row.copyable().hyperlink(pr.url.clone());
        }
        rows.extend([
            url_row,
            jackin_tui::components::ContainerInfoRow::new("CI Status", ci),
        ]);
        let mut state = jackin_tui::components::ContainerInfoState::new("GitHub context", rows);
        if *copied {
            state.mark_copied(3);
        }
        state.scroll = scroll.clone();
        Some(state)
    }

    pub(crate) fn usage_state(&self) -> Option<jackin_tui::components::ContainerInfoState> {
        let Self::Usage {
            view,
            selected,
            scroll,
            ..
        } = self
        else {
            return None;
        };
        if *selected == UsageDialogTab::Overview {
            return Some(Self::usage_overview_state(view, scroll.clone()));
        }
        let mut rows = Vec::new();
        rows.extend([
            jackin_tui::components::ContainerInfoRow::new(
                "Focused",
                Self::usage_focused_label(view),
            ),
            jackin_tui::components::ContainerInfoRow::new(
                "Header",
                Self::usage_provider_header_label(&view.account.provider_label),
            ),
            jackin_tui::components::ContainerInfoRow::new(
                "Provider",
                view.account.provider_label.clone(),
            ),
            jackin_tui::components::ContainerInfoRow::new(
                "Account",
                view.account.account_label.clone(),
            ),
            jackin_tui::components::ContainerInfoRow::new(
                "Status",
                Self::usage_status_label(view.status),
            ),
            jackin_tui::components::ContainerInfoRow::new("Updated", view.updated_label.clone()),
        ]);
        if let Some(plan) = &view.account.plan_label {
            rows.push(jackin_tui::components::ContainerInfoRow::new(
                "Plan",
                plan.clone(),
            ));
        }
        for bucket in &view.buckets {
            rows.push(jackin_tui::components::ContainerInfoRow::new(
                bucket.label.clone(),
                Self::usage_bucket_value(bucket),
            ));
        }
        if let Some(error) = &view.last_error {
            rows.push(jackin_tui::components::ContainerInfoRow::new(
                "Detail",
                error.clone(),
            ));
        }
        let mut state = jackin_tui::components::ContainerInfoState::new("Usage", rows);
        state.scroll = scroll.clone();
        Some(state)
    }

    fn usage_overview_state(
        view: &jackin_protocol::control::FocusedUsageView,
        scroll: jackin_tui::components::DialogBodyScroll,
    ) -> jackin_tui::components::ContainerInfoState {
        let mut rows = Vec::new();
        if view.tabs.is_empty() {
            rows.push(jackin_tui::components::ContainerInfoRow::new(
                "Providers",
                "usage unavailable",
            ));
        } else {
            for tab in &view.tabs {
                // One quota-focused line per provider, matching the Overview
                // preview: "<provider>  <quota summary / lifecycle>". The
                // account identity lives in the focused header above, not on
                // every row. status_label is the daemon-enriched
                // "Session 37% left · Resets in 1h 21m" (or a lifecycle word).
                let quota = if tab.status_label.trim().is_empty() {
                    "status unavailable"
                } else {
                    tab.status_label.trim()
                };
                let value = quota.to_owned();
                rows.push(jackin_tui::components::ContainerInfoRow::new(
                    Self::usage_provider_header_label(&tab.label),
                    value,
                ));
            }
        }
        let mut state = jackin_tui::components::ContainerInfoState::new("Usage", rows);
        state.scroll = scroll;
        state
    }

    fn usage_focused_label(view: &jackin_protocol::control::FocusedUsageView) -> String {
        let account = view.account.account_label.trim();
        let account = if account.is_empty() {
            "account unavailable"
        } else {
            account
        };
        match (&view.focused_agent, &view.focused_provider) {
            (Some(agent), Some(provider)) => format!("{agent} · {provider} · {account}"),
            (Some(agent), None) => format!("{agent} · {account}"),
            (None, Some(provider)) => format!("{provider} · {account}"),
            (None, None) => format!("no focused agent · {account}"),
        }
    }

    fn usage_provider_header_label(label: &str) -> String {
        match label {
            "Codex" | "OpenAI / Codex" => "OpenAI",
            "Claude" | "Anthropic / Claude" => "Anthropic",
            "Grok Build" | "xAI / Grok" => "xAI",
            "GLM / Z.AI" => "Z.AI",
            other => other,
        }
        .to_owned()
    }

    fn usage_provider_tab_target(&mut self, step: isize) -> Option<String> {
        let Self::Usage { view, selected, .. } = self else {
            return None;
        };
        if view.tabs.is_empty() {
            return None;
        }
        if *selected == UsageDialogTab::Overview {
            if step >= 0 {
                return view.tabs.first().map(|tab| tab.label.clone());
            }
            if let Some(target) = view.tabs.last() {
                return Some(target.label.clone());
            }
            *selected = UsageDialogTab::Provider;
            return None;
        }
        let current = view.tabs.iter().position(|tab| tab.active).unwrap_or(0);
        if step < 0 && current == 0 {
            *selected = UsageDialogTab::Overview;
            return None;
        }
        let next = if step >= 0 && current + 1 >= view.tabs.len() {
            *selected = UsageDialogTab::Overview;
            return None;
        } else if step >= 0 {
            current + 1
        } else {
            current - 1
        };
        Some(view.tabs[next].label.clone())
    }

    pub(crate) fn usage_selected_tab(&self) -> Option<UsageDialogTab> {
        let Self::Usage { selected, .. } = self else {
            return None;
        };
        Some(*selected)
    }

    fn usage_bucket_value(bucket: &jackin_protocol::control::QuotaBucketView) -> String {
        let mut parts = Vec::new();
        if bucket.label == "Extra usage" {
            if let Some(remaining) = bucket.remaining_percent {
                let used = 100u8.saturating_sub(remaining);
                parts.push(format!("{} {used}% used", Self::usage_meter(used)));
            }
            match (&bucket.used_label, &bucket.limit_label) {
                (Some(used), Some(limit)) => parts.push(format!("Monthly cap: {used} / {limit}")),
                (Some(used), None) => parts.push(used.clone()),
                (None, Some(limit)) => parts.push(limit.clone()),
                (None, None) => {}
            }
            if parts.is_empty()
                || bucket.status != jackin_protocol::control::UsageSnapshotStatus::Fresh
            {
                parts.push(Self::usage_status_label(bucket.status));
            }
            return parts.join(" · ");
        }
        if let Some(remaining) = bucket.remaining_percent {
            parts.push(format!(
                "{} {remaining}% left",
                Self::usage_meter(remaining)
            ));
        }
        // Normal buckets show only `N% left · pace · Resets in …` on the
        // stats line (the roadmap previews never put a used/limit token there;
        // only `Extra usage`, handled above, shows a cap).
        if let Some(pace) = &bucket.pace_label {
            parts.push(pace.clone());
        }
        if let Some(reset) = &bucket.reset_label {
            parts.push(reset.clone());
        }
        if parts.is_empty() || bucket.status != jackin_protocol::control::UsageSnapshotStatus::Fresh
        {
            parts.push(Self::usage_status_label(bucket.status));
        }
        parts.join(" · ")
    }

    fn usage_meter(remaining_percent: u8) -> String {
        const WIDTH: usize = 32;
        let remaining = usize::from(remaining_percent.min(100));
        let filled = (remaining * WIDTH + 50) / 100;
        format!(
            "{}{}",
            "█".repeat(filled),
            "·".repeat(WIDTH.saturating_sub(filled))
        )
    }

    fn usage_status_label(status: jackin_protocol::control::UsageSnapshotStatus) -> String {
        match status {
            jackin_protocol::control::UsageSnapshotStatus::Fresh => "fresh",
            jackin_protocol::control::UsageSnapshotStatus::Stale => "stale",
            jackin_protocol::control::UsageSnapshotStatus::NeedsLogin => "needs login",
            jackin_protocol::control::UsageSnapshotStatus::NeedsSecret => "needs secret",
            jackin_protocol::control::UsageSnapshotStatus::Unsupported => "unsupported",
            jackin_protocol::control::UsageSnapshotStatus::Unavailable => "unavailable",
            jackin_protocol::control::UsageSnapshotStatus::Error => "error",
        }
        .to_owned()
    }

    pub fn new_github_context() -> Self {
        Self::GitHubContext {
            copied: false,
            scroll: jackin_tui::components::DialogBodyScroll::new(),
        }
    }

    pub fn new_usage(view: jackin_protocol::control::FocusedUsageView) -> Self {
        Self::new_usage_with_tab(view, UsageDialogTab::Provider)
    }

    pub(crate) fn new_usage_with_tab(
        view: jackin_protocol::control::FocusedUsageView,
        selected: UsageDialogTab,
    ) -> Self {
        Self::Usage {
            view: Box::new(view),
            selected,
            tab_bar_focused: true,
            scroll: jackin_tui::components::DialogBodyScroll::new(),
        }
    }

    /// Mutable body-scroll state for the read-only info dialogs whose content
    /// can overflow (`ContainerInfo`, `GitHubContext`). `None` for dialogs that do
    /// not scroll. Lets the daemon route mouse-wheel events to the dialog body.
    pub(crate) fn body_scroll_mut(
        &mut self,
    ) -> Option<&mut jackin_tui::components::DialogBodyScroll> {
        match self {
            Self::ContainerInfo { scroll, .. }
            | Self::GitHubContext { scroll, .. }
            | Self::Usage { scroll, .. } => Some(scroll),
            _ => None,
        }
    }

    pub(crate) fn clamp_body_scroll(
        &mut self,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let rect = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        if matches!(self, Self::ContainerInfo { .. }) {
            let Some(state) = self.container_info_state() else {
                return;
            };
            if let Self::ContainerInfo { scroll, .. } = self {
                jackin_tui::components::clamp_container_info_scroll(
                    scroll,
                    state.content_width(),
                    state.content_height(),
                    rect,
                );
            }
        } else if matches!(self, Self::GitHubContext { .. } | Self::Usage { .. }) {
            let is_usage = matches!(self, Self::Usage { .. });
            let state = if matches!(self, Self::GitHubContext { .. }) {
                let Some(state) = self.github_context_state(github) else {
                    return;
                };
                state
            } else {
                let Some(state) = self.usage_state() else {
                    return;
                };
                state
            };
            if let Self::GitHubContext { scroll, .. } | Self::Usage { scroll, .. } = self {
                let (content_width, content_height) = if is_usage {
                    crate::tui::components::dialog_widgets::usage_info_content_size(&state)
                } else {
                    (state.content_width(), state.content_height())
                };
                jackin_tui::components::clamp_container_info_scroll(
                    scroll,
                    content_width,
                    content_height,
                    rect,
                );
            }
        }
    }

    pub(crate) fn body_scroll_axes(
        &self,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) -> jackin_tui::components::ScrollAxes {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let rect = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        if matches!(self, Self::ContainerInfo { .. }) {
            let Some(state) = self.container_info_state() else {
                return jackin_tui::components::ScrollAxes::none();
            };
            return jackin_tui::components::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                rect,
            );
        }
        if matches!(self, Self::GitHubContext { .. } | Self::Usage { .. }) {
            let is_usage = matches!(self, Self::Usage { .. });
            let state = if matches!(self, Self::GitHubContext { .. }) {
                let Some(state) = self.github_context_state(github) else {
                    return jackin_tui::components::ScrollAxes::none();
                };
                state
            } else {
                let Some(state) = self.usage_state() else {
                    return jackin_tui::components::ScrollAxes::none();
                };
                state
            };
            if is_usage {
                let (content_width, content_height) =
                    crate::tui::components::dialog_widgets::usage_info_content_size(&state);
                return jackin_tui::components::dialog_scroll_axes(
                    content_width,
                    content_height,
                    rect,
                );
            }
            return jackin_tui::components::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                rect,
            );
        }
        jackin_tui::components::ScrollAxes::none()
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

    /// Construct an `AgentPicker` with `selected` pre-initialised to
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
                    jackin_tui::components::ScrollAxes {
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
                    jackin_tui::components::ScrollAxes {
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
            Some(FilterListAction::Dismiss) => DialogAction::Dismiss,
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
                    | Self::ContainerInfo { .. }
                    | Self::GitHubContext { .. }
                    | Self::Usage { .. }
                    | Self::ConfirmAction { .. } => {}
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
                    | Self::ContainerInfo { .. }
                    | Self::GitHubContext { .. }
                    | Self::Usage { .. }
                    | Self::ConfirmAction { .. } => {}
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
        if jackin_tui::components::classify_click(area, col, row)
            == jackin_tui::components::ModalClickResult::OutsideDismiss
        {
            return DialogAction::Dismiss;
        }
        // Text-input dialog has no clickable rows — clicks inside the
        // box are just swallowed so they don't dismiss or reach the
        // pane underneath.
        if matches!(self, Self::RenameTab { .. }) {
            return DialogAction::Consume;
        }
        // ContainerInfo: any copyable row (Container ID, Run ID, Diagnostics
        // log) copies via the shared hit-test. The clicked row's value goes to
        // the clipboard and that row shows the "Copied!" badge.
        if matches!(self, Self::ContainerInfo { .. }) {
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
            let area = ratatui::layout::Rect {
                x: box_col,
                y: box_row,
                width,
                height,
            };
            let hit = self.github_context_state(github).and_then(|state| {
                jackin_tui::components::container_info_copy_payload_at(area, &state, col, row)
            });
            return match hit {
                Some((_hit_row, payload)) => {
                    if let Self::GitHubContext { copied, .. } = self {
                        *copied = true;
                    }
                    DialogAction::CopyToClipboard(payload)
                }
                _ => DialogAction::Consume,
            };
        }
        if matches!(self, Self::Usage { .. }) {
            return DialogAction::Consume;
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
            | Self::Usage { .. }
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
            | Self::Usage { .. }
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
                let area = ratatui::layout::Rect {
                    x: box_col,
                    y: box_row,
                    width,
                    height,
                };
                self.github_context_state(github).is_some_and(|state| {
                    jackin_tui::components::container_info_copy_payload_at(area, &state, col, row)
                        .is_some()
                })
            }
            Self::Usage { .. } => false,
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
            Self::ContainerInfo { .. } | Self::GitHubContext { .. } | Self::Usage { .. } => {
                CONTAINER_INFO_WIDTH
                    .min(term_cols.saturating_sub(4))
                    .max(PALETTE_WIDTH)
            }
            // Exit data-loss confirm has two warning notes wider than PALETTE_WIDTH.
            // Use the shared Details width percentage (70%) so the notes don't truncate.
            Self::ConfirmAction {
                kind: ConfirmKind::Exit,
                ..
            } => (term_cols.saturating_mul(70) / 100).clamp(
                PALETTE_WIDTH,
                term_cols.saturating_sub(4).max(PALETTE_WIDTH),
            ),
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
            Self::Usage { .. } => self.usage_state().map_or(10, |state| {
                crate::tui::components::dialog_widgets::usage_info_required_height(&state)
            }),
            // 9 = border(2) + leading(1) + question(1) + empty(1) + message(1) + spacer(1) + button(1) + trailing(1)
            // Matches the canonical symmetric dialog layout (Defect 5).
            // Exit shows the shared data-loss variant (extra warning notes), so
            // size it from that state rather than the fixed single-line height.
            Self::ConfirmAction { kind, .. } => match kind {
                ConfirmKind::Exit => jackin_tui::components::confirm_required_height(
                    &jackin_tui::components::exit_confirm_state_with_data_loss(),
                ),
                ConfirmKind::ClosePane | ConfirmKind::CloseTab => 9,
            },
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
            Self::CommandPalette { .. } => palette_hint(),
            Self::SplitDirectionPicker { .. }
            | Self::AgentPicker { .. }
            | Self::CloseTargetPicker { .. } => picker_hint(),
            Self::ProviderPicker { .. } => provider_hint(),
            Self::RenameTab { .. } => rename_hint(),
            Self::ContainerInfo { .. } => info_dialog_hint("copy value", axes),
            Self::GitHubContext { .. } => {
                if github.and_then(|view| view.status.loaded()).is_some() {
                    info_dialog_hint("copy GitHub URL", axes)
                } else {
                    read_only_hint()
                }
            }
            Self::Usage { .. } => usage_hint(axes),
            Self::ConfirmAction { .. } => confirm_hint(),
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
}

#[cfg(test)]
mod tests;
