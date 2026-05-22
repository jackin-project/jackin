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
///
/// While a dialog is open, panes behind it render with the ANSI dim
/// attribute so the operator sees a clear "focus is inside the
/// dialog" cue (see `render_pane`'s `dim` parameter).
const PALETTE_WIDTH: u16 = 50;
const CONTAINER_INFO_WIDTH: u16 = 86;
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const BG_DARK: &str = "\x1b[48;2;0;0;0m"; // pure black
const FG_GREEN: &str = "\x1b[38;2;0;255;65m"; // PHOSPHOR_GREEN
const FG_DIM: &str = "\x1b[38;2;0;140;30m"; // PHOSPHOR_DIM
const FG_BORDER: &str = "\x1b[38;2;0;80;18m"; // PHOSPHOR_DARK
const FG_WHITE: &str = "\x1b[38;2;255;255;255m"; // WHITE
const SELECT_BG: &str = "\x1b[48;2;0;255;65m"; // PHOSPHOR_GREEN bg
const SELECT_FG: &str = "\x1b[38;2;0;0;0m"; // BLACK fg
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
            Self::Right => "Right →",
            Self::Above => "↑ Above",
            Self::Below => "↓ Below",
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
    /// length live in the same place the console TUI will pull from
    /// when its modal stack switches off ratatui_textarea. Enter
    /// commits; Esc cancels; empty input clears any previous custom
    /// label so the tab returns to auto-naming.
    RenameTab {
        tab_idx: usize,
        input: jackin_tui::TextField,
    },
    /// Read-only modal opened when the operator clicks the status-bar
    /// container-name label. Surfaces the bits that used to clutter
    /// the bar (role key, focused-agent runtime) plus the full
    /// container ID, workspace path, and best-effort git context with
    /// a one-key "copy to clipboard" shortcut.
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
        git_loading: bool,
        git_branch: Option<String>,
        pull_request_loading: bool,
        pull_request_url: Option<String>,
        copied: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCloseLabel {
    ChooseTarget,
    CloseTab,
}

impl PaletteCloseLabel {
    fn label(self) -> &'static str {
        match self {
            Self::ChooseTarget => "Close",
            Self::CloseTab => "Close tab",
        }
    }
}

const CLOSE_TARGET_ITEMS: &[(ConfirmKind, &str)] = &[
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteCommand {
    NewTab,
    NextTab,
    PrevTab,
    /// Open the SplitDirectionPicker. The operator picks Left /
    /// Right / Above / Below in the sub-dialog, then the agent
    /// picker for the new pane. Replaces the previous two-item
    /// `SplitHorizontal` + `SplitVertical` shape so the menu reads
    /// "Split pane" once and the directional detail lives in the
    /// sub-dialog where it does not clutter the top-level list.
    Split,
    ZoomPane,
    /// Close the active tab or open the CloseTargetPicker when the
    /// active tab has multiple panes. The chosen target then routes
    /// through `ConfirmAction` before the destructive call fires.
    Close,
    ClearPane,
    Exit,
}

/// Next/Previous tab are not exposed in the palette: the operator
/// already clicks tabs directly in the status bar, and the
/// keyboard-driven shortcut for cycle-tab is the tmux-style prefix
/// gesture (`Ctrl+B n` / `Ctrl+B p`). Keeping list entries that only
/// duplicate those existing paths bloats the modal with no new
/// capability. `PaletteCommand::NextTab` / `PrevTab` stay in the enum
/// so prefix-mode bindings continue to work.
const PALETTE_ITEMS: &[(PaletteCommand, &str)] = &[
    (PaletteCommand::NewTab, "New tab"),
    (PaletteCommand::Split, "Split pane"),
    (PaletteCommand::ZoomPane, "Zoom / unzoom pane"),
    (PaletteCommand::ClearPane, "Clear pane"),
    (PaletteCommand::Close, "Close"),
    (PaletteCommand::Exit, "Exit"),
];

/// Items in the SplitDirectionPicker sub-dialog. Order matches the
/// way the operator's hands move on the cardinal keys: Left, Right,
/// Above, Below. The dialog is filter-able like the other list
/// dialogs — typing `a` narrows to "Above," typing `l` narrows to
/// "Left," etc.
const SPLIT_DIRECTION_ITEMS: &[SplitDirection] = &[
    SplitDirection::Left,
    SplitDirection::Right,
    SplitDirection::Above,
    SplitDirection::Below,
];

impl Dialog {
    pub fn new_command_palette(close_label: PaletteCloseLabel) -> Self {
        Self::CommandPalette {
            selected: 0,
            filter: String::new(),
            close_label,
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
    pub fn handle_key(&mut self, key: &[u8]) -> DialogAction {
        // Text-input dialog has its own dismissal / editing rules and
        // must intercept keys before the arrow-key + dismiss-key
        // shortcuts below would steal them (e.g. `q` is a legal
        // character inside a custom tab name).
        if let Self::RenameTab { tab_idx, input } = self {
            return rename_tab_handle_key(*tab_idx, input, key);
        }
        // ContainerInfo is read-only — Enter copies the container
        // name to clipboard, every other key (except dismiss handled
        // below) is a no-op redraw. `copied` flips to `true` inline
        // so the next render's "Copied!" indicator confirms the OSC
        // 52 fired; the dialog stays open until the operator
        // dismisses so the feedback is actually visible.
        if let Self::ContainerInfo {
            container_name,
            copied,
            ..
        } = self
        {
            if is_dismiss_key(key) {
                return DialogAction::Dismiss;
            }
            return match key {
                b"\r" | b"\n" => {
                    let payload = container_name.clone();
                    *copied = true;
                    DialogAction::CopyToClipboard(payload)
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
                Self::RenameTab { .. }
                | Self::ContainerInfo { .. }
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
                Self::RenameTab { .. }
                | Self::ContainerInfo { .. }
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
        if let Self::ContainerInfo {
            container_name,
            copied,
            ..
        } = self
        {
            if !container_info_id_row_clickable(row, col, box_row, box_col, width) {
                return DialogAction::Consume;
            }
            let payload = container_name.clone();
            *copied = true;
            return DialogAction::CopyToClipboard(payload);
        }
        // ConfirmAction: inside-box click is treated as Yes (operator's
        // click landed inside the destructive-action confirmation box,
        // they're acting). Outside-click already dismissed via the
        // early return above.
        if let Self::ConfirmAction { kind, .. } = self {
            return DialogAction::ConfirmedAction(*kind);
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
            Self::RenameTab { .. } | Self::ContainerInfo { .. } | Self::ConfirmAction { .. } => 0,
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
            // RenameTab and ContainerInfo clicks were already handled
            // by early returns above. ConfirmAction has no row list —
            // every inside-box click is dispatched by the early
            // return below the inside_box check.
            Self::RenameTab { .. } | Self::ContainerInfo { .. } | Self::ConfirmAction { .. } => {
                DialogAction::Consume
            }
        }
    }

    /// Return true when `(row, col)` is a dialog hit target that will
    /// perform an action on click. The daemon uses this to drive OSC 22
    /// pointer-shape feedback without duplicating dialog layout maths.
    pub fn clickable_at(&self, row: u16, col: u16, term_rows: u16, term_cols: u16) -> bool {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let inside_box =
            row >= box_row && row < box_row + height && col >= box_col && col < box_col + width;
        if !inside_box {
            return false;
        }
        match self {
            Self::RenameTab { .. } => false,
            Self::ContainerInfo { .. } => {
                container_info_id_row_clickable(row, col, box_row, box_col, width)
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
    fn box_rect(&self, term_rows: u16, term_cols: u16) -> (u16, u16, u16, u16) {
        let width = match self {
            Self::ContainerInfo { .. } => CONTAINER_INFO_WIDTH
                .min(term_cols.saturating_sub(4))
                .max(PALETTE_WIDTH),
            _ => PALETTE_WIDTH,
        };
        // Filterable dialogs grow by 2 rows over the legacy layout to
        // make room for the filter input + a blank separator above
        // the items list. Item count tracks the *filtered* set so the
        // box shrinks as the operator narrows the matches.
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
            // Rename modal: top border + blank pad + input row + blank pad + bottom border.
            Self::RenameTab { .. } => 5,
            // ContainerInfo: top + pad + 6 detail rows + pad + bottom.
            Self::ContainerInfo { .. } => 10,
            // ConfirmAction: top + pad + 2 message rows + pad + button + pad + bottom.
            Self::ConfirmAction { .. } => 9,
        };
        let max_height = term_rows
            .saturating_sub(crate::statusbar::STATUS_BAR_ROWS)
            .max(3);
        let height = natural_height.min(max_height);
        let row = crate::statusbar::STATUS_BAR_ROWS + (max_height.saturating_sub(height)) / 2;
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
    /// instead of recomputing the centring; bottom-hint placement is
    /// still relative to `term_rows` because the hint lives outside
    /// the box.
    pub fn render(&self, buf: &mut Vec<u8>, term_rows: u16, term_cols: u16) {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        // Skip rendering entirely when the terminal is too small to
        // hold the box without overlapping the status bar or the
        // bottom edge. The host terminal would otherwise scroll and
        // destroy operator pane content.
        if term_rows < crate::statusbar::STATUS_BAR_ROWS + 3
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
                render_bottom_hint(buf, term_rows, term_cols, PALETTE_HINT);
            }
            Self::SplitDirectionPicker { selected, filter } => {
                render_split_direction_picker(
                    buf, box_row, box_col, height, width, *selected, filter,
                );
                render_bottom_hint(buf, term_rows, term_cols, PICKER_HINT);
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
                render_bottom_hint(buf, term_rows, term_cols, PICKER_HINT);
            }
            Self::RenameTab { input, .. } => {
                render_rename_tab(buf, term_rows, term_cols, input.value());
            }
            Self::ContainerInfo {
                container_name,
                role,
                focused_agent,
                workdir,
                git_loading,
                git_branch,
                pull_request_loading,
                pull_request_url,
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
                    *git_loading,
                    git_branch.as_deref(),
                    *pull_request_loading,
                    pull_request_url.as_deref(),
                    *copied,
                );
                render_bottom_hint(buf, term_rows, term_cols, CONTAINER_INFO_HINT);
            }
            Self::CloseTargetPicker { selected, filter } => {
                render_close_target_picker(buf, box_row, box_col, height, width, *selected, filter);
                render_bottom_hint(buf, term_rows, term_cols, PICKER_HINT);
            }
            Self::ConfirmAction { kind, selected_yes } => {
                render_confirm_action(buf, box_row, box_col, height, width, *kind, *selected_yes);
                render_bottom_hint(buf, term_rows, term_cols, CONFIRM_HINT);
            }
        }
    }

    /// Clear transient copy feedback after the daemon-side timer
    /// expires. Returns true only when the visible dialog changed.
    pub fn clear_copy_feedback(&mut self) -> bool {
        let Self::ContainerInfo { copied, .. } = self else {
            return false;
        };
        if !*copied {
            return false;
        }
        *copied = false;
        true
    }
}

fn container_info_id_row_clickable(
    row: u16,
    col: u16,
    box_row: u16,
    box_col: u16,
    width: u16,
) -> bool {
    let start = box_col.saturating_add(2);
    let end = box_col.saturating_add(width.saturating_sub(2));
    row == box_row + 2 && col >= start && col < end
}

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
            // emoji / combining-mark labels reach `TextField`. The
            // single-byte ASCII-printable form previously here dropped
            // every non-ASCII keystroke silently, which mismatched the
            // unicode-width measurement `lay_out_tabs` now uses for
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

/// Indices into `PALETTE_ITEMS` whose label contains `filter` as a
/// case-insensitive substring. An empty filter returns every item.
fn palette_item_label(
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

fn palette_filtered_indices(filter: &str, close_label: PaletteCloseLabel) -> Vec<usize> {
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

/// One footer-hint span. Mirrors the console TUI's `FooterItem` model
/// (see `src/console/manager/render/mod.rs`).
#[allow(dead_code)] // `Sep` reserved for future hints; mirrors console FooterItem
enum HintSpan<'a> {
    /// Hotkey glyph(s) — white + bold.
    Key(&'a str),
    /// Action label after a key — phosphor green.
    Text(&'a str),
    /// Dot separator between key+label pairs in the same group.
    Sep,
    /// Three-space group separator.
    GroupSep,
}

// Bottom-hint contract mirrors the host console `Select Role` picker
// (`↑↓ navigate · type filter · Enter select · Esc cancel`) so the
// operator's footer reading carries from the host to the in-container
// dialog without learning a second vocabulary. `type filter` is a
// textual hint (no key glyph) because the action is "any printable
// keystroke," not a specific key.
const PALETTE_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type filter"),
    HintSpan::GroupSep,
    HintSpan::Key("Enter"),
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
    HintSpan::Key("Enter"),
    HintSpan::Text("launch"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
];

const RENAME_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Enter"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
    HintSpan::GroupSep,
    HintSpan::Text("empty = auto name"),
];

const CONTAINER_INFO_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Enter"),
    HintSpan::Text("copy container ID"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("dismiss"),
];

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
/// buffer plus a blinking-style trailing `▌` caret. Width matches the
/// other dialogs so the operator's eye does not have to re-anchor.
fn render_rename_tab(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16, input: &str) {
    // Single source of truth for the dialog visual recipe lives in
    // `jackin_tui::ansi` so this dialog matches the host TUI's
    // `text_input` widget (used by the workspace-environments editor).
    let cursor_byte = input.len();
    jackin_tui::ansi::render_text_input_dialog(
        buf,
        term_rows,
        term_cols,
        "Rename tab",
        input,
        cursor_byte,
    );
    render_bottom_hint(buf, term_rows, term_cols, RENAME_HINT);
}

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
    let button_row = start_row + height.saturating_sub(2);
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
        buf.extend_from_slice(b"\x1b[48;2;255;255;255m"); // WHITE bg
        buf.extend_from_slice(b"\x1b[38;2;0;0;0m"); // BLACK fg
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

/// Filter input row. Visual contract mirrors the host console's
/// `Select Role` picker (`src/console/widgets/role_picker.rs::render`)
/// so the operator sees the same `Filter: …` shape in every dialog
/// jackin renders. Empty filter shows a 20-character `░` placeholder
/// (`U+2591 LIGHT SHADE`) in `PHOSPHOR_DARK` — same glyph + colour as
/// the host picker; populated filter shows the typed text in white
/// followed by a `█` (`U+2588 FULL BLOCK`) caret. Both halves stay
/// inside `Filter: ` (label in `PHOSPHOR_DIM`).
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

/// No-matches state — leave the body blank, same as the host
/// `Select Role` picker. The empty space below the filter row IS the
/// empty state; an inline `(no matches)` placeholder breaks that
/// visual contract. Operator dismisses with Esc or pops filter
/// characters with Backspace until items reappear.
fn render_no_matches_row(_buf: &mut Vec<u8>, _row: u16, _col: u16, _width: u16) {
    // Intentionally blank. See doc-comment.
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
/// target the bottom hint advertises. No selection state — Enter / a
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
    git_loading: bool,
    git_branch: Option<&str>,
    pull_request_loading: bool,
    pull_request_url: Option<&str>,
    copied: bool,
) {
    render_box(buf, box_row, box_col, height, width, "Container info");
    // Label column width — keep the label/value gutter aligned across
    // all rows. "Container ID" and "Pull Request" are the longest labels.
    let label_col_width = "Container ID".chars().count();
    let interior_left = box_col + 2;
    let interior_max_cols = (width as usize).saturating_sub(4);
    let value_col_offset = label_col_width + 2; // 2 = ": "
    let value_max_cols = interior_max_cols.saturating_sub(value_col_offset);

    let rows: [(&str, String, bool); 6] = [
        ("Container ID", container_name.to_string(), true),
        ("Role", non_empty_or_dim(role), false),
        (
            "Agent",
            non_empty_or_dim(focused_agent.unwrap_or("")),
            false,
        ),
        ("Workdir", non_empty_or_dim(workdir), false),
        ("Branch", git_context_value(git_branch, git_loading), false),
        (
            "Pull Request",
            git_context_value(pull_request_url, pull_request_loading),
            false,
        ),
    ];
    for (i, (label, value, emphasise)) in rows.iter().enumerate() {
        let r = box_row + 2 + i as u16;
        move_to(buf, r, interior_left);
        buf.extend_from_slice(BG_DARK.as_bytes());
        buf.extend_from_slice(FG_BORDER.as_bytes());
        buf.extend_from_slice(label.as_bytes());
        for _ in label.chars().count()..label_col_width {
            buf.push(b' ');
        }
        buf.extend_from_slice(b": ");
        if *emphasise {
            buf.extend_from_slice(FG_WHITE.as_bytes());
            buf.extend_from_slice(BOLD.as_bytes());
        } else {
            buf.extend_from_slice(FG_GREEN.as_bytes());
        }
        let badge = if i == 0 && copied {
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
        let value_cols = value.chars().count().min(available_value_cols);
        let value_take: String = value.chars().take(value_cols).collect();
        buf.extend_from_slice(value_take.as_bytes());
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

/// Show `"(none)"` for empty role / agent strings so a missing value
/// is visibly missing rather than a confusingly empty gutter.
fn non_empty_or_dim(s: &str) -> String {
    if s.is_empty() {
        "(none)".to_string()
    } else {
        s.to_string()
    }
}

fn git_context_value(value: Option<&str>, loading: bool) -> String {
    if loading {
        "⠋ loading".to_string()
    } else {
        non_empty_or_dim(value.unwrap_or(""))
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
    buf.extend_from_slice(FG_BORDER.as_bytes());
    buf.extend_from_slice("┌".as_bytes());
    buf.extend_from_slice("─".as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(FG_WHITE.as_bytes());
    buf.extend_from_slice(BOLD.as_bytes());
    buf.extend_from_slice(title.as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
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
        buf.extend_from_slice(FG_BORDER.as_bytes());
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
    buf.extend_from_slice(FG_BORDER.as_bytes());
    buf.extend_from_slice("└".as_bytes());
    for _ in 1..(width - 1) {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice("┘".as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
}

/// Compute the visual column width of a hint span row. Matches the
/// formatting in `render_bottom_hint` so centring is exact.
fn hint_span_cols(spans: &[HintSpan<'_>]) -> usize {
    spans
        .iter()
        .map(|s| match s {
            HintSpan::Key(k) => k.chars().count(),
            HintSpan::Text(t) => 1 /* leading space */ + t.chars().count(),
            HintSpan::Sep => 3,
            HintSpan::GroupSep => 3,
        })
        .sum()
}

/// Paint the hint row centred on the **terminal's last row**, on top of
/// the agent / shell content beneath the dialog box. Lives outside the
/// box so the box border ends cleanly and the hint reads as the
/// global-footer pattern jackin's console TUI uses.
fn render_bottom_hint(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16, spans: &[HintSpan<'_>]) {
    let total = hint_span_cols(spans);
    if total > term_cols as usize || term_rows == 0 {
        return;
    }
    let start_col = ((term_cols as usize).saturating_sub(total) / 2) as u16;
    let row = term_rows - 1;
    move_to(buf, row, start_col);
    buf.extend_from_slice(BG_DARK.as_bytes());
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
    let _ = FG_DIM; // reserved for future Dyn spans (e.g., "N items selected")
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
mod tests {
    use super::*;

    fn picker(agents: Vec<&str>) -> Dialog {
        // Mirror the daemon's construction site: `Dialog::new_agent_picker`
        // computes the initial `selected` past the leading `"agents"`
        // section row. Tests that explicitly want a different starting
        // selection construct `Dialog::AgentPicker { … }` inline.
        Dialog::new_agent_picker(
            agents.into_iter().map(String::from).collect(),
            PickerIntent::NewTab,
        )
    }

    fn palette_with(selected: usize, filter: impl Into<String>) -> Dialog {
        Dialog::CommandPalette {
            selected,
            filter: filter.into(),
            close_label: PaletteCloseLabel::ChooseTarget,
        }
    }

    fn palette() -> Dialog {
        palette_with(0, String::new())
    }

    #[test]
    fn esc_dismisses_palette() {
        let mut d = palette();
        assert_eq!(d.handle_key(b"\x1b"), DialogAction::Dismiss);
    }

    #[test]
    fn ctrl_c_dismisses_palette() {
        let mut d = palette();
        assert_eq!(d.handle_key(b"\x03"), DialogAction::Dismiss);
    }

    #[test]
    fn arrow_down_advances_palette_selection() {
        let mut d = palette();
        assert_eq!(d.handle_key(b"\x1b[B"), DialogAction::Redraw);
        let Dialog::CommandPalette { selected, .. } = d else {
            unreachable!()
        };
        assert_eq!(selected, 1);
    }

    #[test]
    fn arrow_down_clamps_palette_at_last_item() {
        let mut d = palette_with(PALETTE_ITEMS.len() - 1, String::new());
        d.handle_key(b"\x1b[B");
        let Dialog::CommandPalette { selected, .. } = d else {
            unreachable!()
        };
        assert_eq!(selected, PALETTE_ITEMS.len() - 1);
    }

    #[test]
    fn enter_on_palette_emits_command() {
        let mut d = palette();
        match d.handle_key(b"\r") {
            DialogAction::Command(cmd) => assert_eq!(cmd, PALETTE_ITEMS[0].0),
            other => panic!("expected Command, got {other:?}"),
        }
    }

    #[test]
    fn enter_on_agent_picker_emits_spawn() {
        let mut d = picker(vec!["claude", "codex"]);
        match d.handle_key(b"\r") {
            DialogAction::SpawnAgent { agent, intent } => {
                assert_eq!(agent.as_deref(), Some("claude"));
                assert_eq!(intent, PickerIntent::NewTab);
            }
            other => panic!("expected SpawnAgent, got {other:?}"),
        }
    }

    #[test]
    fn agent_picker_shell_slot_emits_none_agent() {
        // Layout for `picker(vec!["claude"])` is:
        //   0: Section("agents")    — non-selectable
        //   1: Agent(claude)        ← initial selected (skipped past Section)
        //   2: Section("shells")    — non-selectable
        //   3: Shell                ← Enter emits agent=None
        // Arrow Down from index 1 must skip the Section at index 2 and
        // land directly on the Shell row at index 3.
        let mut d = picker(vec!["claude"]);
        d.handle_key(b"\x1b[B");
        match d.handle_key(b"\r") {
            DialogAction::SpawnAgent { agent, .. } => assert!(agent.is_none()),
            other => panic!("expected SpawnAgent, got {other:?}"),
        }
    }

    #[test]
    fn picker_arrow_down_skips_section_label() {
        // Direct check: from the last-agent index, Down lands on the
        // first selectable past the "shells" section header, not on
        // the header itself.
        let mut d = picker(vec!["claude", "codex"]);
        // Walk past both agents (selected 1 → 2 → expected 4 = Shell).
        d.handle_key(b"\x1b[B"); // 1 → 2
        d.handle_key(b"\x1b[B"); // 2 → 4 (skips Section at 3)
        let Dialog::AgentPicker { selected, .. } = &d else {
            unreachable!()
        };
        assert_eq!(*selected, 4, "Down must skip the shells section label");
    }

    #[test]
    fn picker_enter_on_section_label_is_noop() {
        // Defensive: an out-of-band selected value pointing at a
        // Section row must not synthesise a SpawnAgent. Real flows
        // can't get there (arrows step past sections, click on a
        // section returns Consume), but a stale `selected` after a
        // filter pass that left only sections behind must degrade
        // to Redraw.
        let mut d = Dialog::AgentPicker {
            agents: vec!["claude".to_string()],
            selected: 0, // points at Section("agents")
            intent: PickerIntent::NewTab,
            filter: String::new(),
        };
        assert_eq!(d.handle_key(b"\r"), DialogAction::Redraw);
    }

    #[test]
    fn click_outside_dialog_dismisses() {
        let mut d = palette();
        // Click in the top-left corner is reliably outside the centred
        // box even on tiny terminals.
        assert_eq!(d.handle_click(0, 0, 40, 100), DialogAction::Dismiss);
    }

    #[test]
    fn clickable_at_reports_container_info_copy_target() {
        let d = container_info_fixture();
        let (row, col, _, _) = d.box_rect(40, 100);
        assert!(d.clickable_at(row + 2, col + 2, 40, 100));
        assert!(!d.clickable_at(row + 3, col + 2, 40, 100));
        assert!(!d.clickable_at(0, 0, 40, 100));
    }

    #[test]
    fn clickable_at_skips_agent_picker_section_labels() {
        let d = picker(vec!["claude"]);
        let (row, col, _, _) = d.box_rect(40, 100);
        let first_item_row = row + 3;
        assert!(
            !d.clickable_at(first_item_row, col + 2, 40, 100),
            "section label must not advertise as clickable"
        );
        assert!(
            d.clickable_at(first_item_row + 1, col + 2, 40, 100),
            "agent row should advertise as clickable"
        );
    }

    #[test]
    fn palette_typing_filters_items_and_resets_selection() {
        let mut d = palette_with(3, String::new());
        // Type "split" — narrows to the single "Split pane" item +
        // resets selection to 0. (The legacy `Split pane │ (side by
        // side)` + `Split pane ─ (top / bottom)` pair collapsed into
        // one menu entry; the directional choice now lives in the
        // SplitDirectionPicker sub-dialog opened on confirm.)
        for &c in b"split" {
            d.handle_key(&[c]);
        }
        let Dialog::CommandPalette {
            selected, filter, ..
        } = &d
        else {
            unreachable!()
        };
        assert_eq!(filter, "split");
        assert_eq!(*selected, 0, "filter input must reset selection to 0");
        assert_eq!(
            palette_filtered_indices(filter, PaletteCloseLabel::ChooseTarget).len(),
            1,
            "exactly one PALETTE_ITEM matches 'split' after the collapse"
        );
    }

    #[test]
    fn palette_split_opens_split_direction_picker_via_dialog_action() {
        // Confirming "Split pane" in the menu produces
        // `DialogAction::Command(PaletteCommand::Split)` — the daemon
        // turns that into a new SplitDirectionPicker dialog. Lock the
        // action shape so a refactor that flips the chain inadvertently
        // (e.g. directly emitting SplitDirection) gets caught.
        let mut d = palette();
        for &c in b"split" {
            d.handle_key(&[c]);
        }
        match d.handle_key(b"\r") {
            DialogAction::Command(cmd) => assert_eq!(cmd, PaletteCommand::Split),
            other => panic!("expected Command(Split), got {other:?}"),
        }
    }

    #[test]
    fn split_direction_picker_enter_emits_split_direction() {
        let mut d = Dialog::SplitDirectionPicker {
            selected: 0,
            filter: String::new(),
        };
        // selected = 0 → first item = Left
        match d.handle_key(b"\r") {
            DialogAction::SplitDirection(dir) => assert_eq!(dir, SplitDirection::Left),
            other => panic!("expected SplitDirection(Left), got {other:?}"),
        }
    }

    #[test]
    fn split_direction_picker_typing_belo_narrows_to_below() {
        let mut d = Dialog::SplitDirectionPicker {
            selected: 0,
            filter: String::new(),
        };
        for &c in b"belo" {
            d.handle_key(&[c]);
        }
        match d.handle_key(b"\r") {
            DialogAction::SplitDirection(dir) => assert_eq!(dir, SplitDirection::Below),
            other => panic!("expected SplitDirection(Below), got {other:?}"),
        }
    }

    #[test]
    fn palette_enter_after_filter_emits_matching_command() {
        let mut d = palette();
        for &c in b"close" {
            d.handle_key(&[c]);
        }
        // "close" matches the top-level Close command; the daemon
        // decides whether to confirm directly or open the target
        // picker based on the active tab's pane count.
        match d.handle_key(b"\r") {
            DialogAction::Command(cmd) => assert_eq!(cmd, PaletteCommand::Close),
            other => panic!("expected Close, got {other:?}"),
        }
    }

    #[test]
    fn palette_single_pane_close_label_renders_as_close_tab() {
        let d = Dialog::new_command_palette(PaletteCloseLabel::CloseTab);
        let mut buf = Vec::new();
        d.render(&mut buf, 40, 100);
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("Close tab"));
        assert_eq!(
            palette_filtered_indices("close tab", PaletteCloseLabel::CloseTab).len(),
            1
        );
    }

    #[test]
    fn palette_clear_filter_emits_clear_pane() {
        let mut d = palette();
        for &c in b"clear" {
            d.handle_key(&[c]);
        }
        match d.handle_key(b"\r") {
            DialogAction::Command(cmd) => assert_eq!(cmd, PaletteCommand::ClearPane),
            other => panic!("expected ClearPane, got {other:?}"),
        }
    }

    #[test]
    fn palette_backspace_pops_filter_char_and_resets_selection() {
        let mut d = palette_with(0, "split");
        d.handle_key(b"\x7f");
        let Dialog::CommandPalette { filter, .. } = &d else {
            unreachable!()
        };
        assert_eq!(filter, "spli");
    }

    #[test]
    fn palette_q_types_into_filter_does_not_dismiss() {
        // Pre-filter dialogs dismissed on `q`; now `q` is a filter
        // character because the dialog is type-to-filter. Esc remains
        // the dismiss key.
        let mut d = palette();
        assert_eq!(d.handle_key(b"q"), DialogAction::Redraw);
        let Dialog::CommandPalette { filter, .. } = &d else {
            unreachable!()
        };
        assert_eq!(filter, "q");
    }

    #[test]
    fn picker_typing_sh_narrows_to_shells_section_plus_shell_row() {
        // Filter "sh" excludes every agent label but keeps the literal
        // "shell" word — so the rendered list collapses to just the
        // shells section header + the Shell row. The shells header
        // stays visible so the operator's eye reads "this is a Shell,
        // not a stray agent."
        let mut d = picker(vec!["claude", "codex", "kimi"]);
        for &c in b"sh" {
            d.handle_key(&[c]);
        }
        let Dialog::AgentPicker { agents, filter, .. } = &d else {
            unreachable!()
        };
        let visible = picker_filtered_rows(agents, filter);
        assert_eq!(
            visible,
            vec![PickerRow::Section("shells"), PickerRow::Shell]
        );
    }

    #[test]
    fn picker_typing_cla_filters_to_claude() {
        let mut d = picker(vec!["claude", "codex", "kimi"]);
        for &c in b"cla" {
            d.handle_key(&[c]);
        }
        // Enter on filtered list[0] = claude
        match d.handle_key(b"\r") {
            DialogAction::SpawnAgent { agent, .. } => {
                assert_eq!(agent.as_deref(), Some("claude"));
            }
            other => panic!("expected SpawnAgent(claude), got {other:?}"),
        }
    }

    #[test]
    fn picker_enter_with_empty_filtered_list_is_redraw_noop() {
        let mut d = picker(vec!["claude", "codex"]);
        for &c in b"zzz" {
            d.handle_key(&[c]);
        }
        assert_eq!(
            d.handle_key(b"\r"),
            DialogAction::Redraw,
            "Enter with no matches must not synthesise a SpawnAgent"
        );
    }

    #[test]
    fn rename_tab_empty_input_clears_label() {
        let mut d = Dialog::RenameTab {
            tab_idx: 3,
            input: jackin_tui::TextField::new("").with_allow_empty(true),
        };
        match d.handle_key(b"\r") {
            DialogAction::RenameTab { tab_idx, label } => {
                assert_eq!(tab_idx, 3);
                assert_eq!(label, "");
            }
            other => panic!("expected RenameTab, got {other:?}"),
        }
    }

    #[test]
    fn rename_tab_backspace_removes_last_char() {
        let mut d = Dialog::RenameTab {
            tab_idx: 0,
            input: jackin_tui::TextField::new("abc"),
        };
        assert_eq!(d.handle_key(b"\x7f"), DialogAction::Redraw);
        let Dialog::RenameTab { input, .. } = d else {
            unreachable!()
        };
        assert_eq!(input.value(), "ab");
    }

    #[test]
    fn rename_tab_esc_dismisses() {
        let mut d = Dialog::RenameTab {
            tab_idx: 0,
            input: jackin_tui::TextField::new("abc"),
        };
        assert_eq!(d.handle_key(b"\x1b"), DialogAction::Dismiss);
    }

    #[test]
    fn rename_tab_consumes_q_as_input_not_dismiss() {
        // `q` is a dismiss key for list-style dialogs but must be
        // accepted as input inside the rename-tab buffer — otherwise
        // operators can't type the letter into their tab name.
        let mut d = Dialog::RenameTab {
            tab_idx: 0,
            input: jackin_tui::TextField::new("a"),
        };
        assert_eq!(d.handle_key(b"q"), DialogAction::Redraw);
        let Dialog::RenameTab { input, .. } = d else {
            unreachable!()
        };
        assert_eq!(input.value(), "aq");
    }

    fn container_info_fixture() -> Dialog {
        Dialog::ContainerInfo {
            container_name: "jk-abc123-thearchitect".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace/jackin".to_string(),
            git_loading: false,
            git_branch: Some("feature/container-info".to_string()),
            pull_request_loading: false,
            pull_request_url: Some("https://github.com/jackin-project/jackin/pull/123".to_string()),
            copied: false,
        }
    }

    #[test]
    fn container_info_enter_flips_copied_flag_for_render_feedback() {
        let mut d = container_info_fixture();
        let _ = d.handle_key(b"\r");
        let Dialog::ContainerInfo { copied, .. } = d else {
            unreachable!()
        };
        assert!(
            copied,
            "Enter must flip `copied` so the next render shows the Copied! indicator"
        );
    }

    #[test]
    fn container_info_enter_does_not_dismiss_dialog() {
        // Operator copies once and expects to read the badge before
        // dismissing themselves — handle_key must NOT return Dismiss
        // for Enter.
        let mut d = container_info_fixture();
        let action = d.handle_key(b"\r");
        assert!(
            matches!(action, DialogAction::CopyToClipboard(_)),
            "Enter must request a copy, not dismiss; got {action:?}"
        );
    }

    #[test]
    fn container_info_enter_copies_container_name() {
        let mut d = container_info_fixture();
        match d.handle_key(b"\r") {
            DialogAction::CopyToClipboard(payload) => {
                assert_eq!(payload, "jk-abc123-thearchitect");
            }
            other => panic!("Enter must request clipboard copy, got {other:?}"),
        }
    }

    #[test]
    fn container_info_click_on_id_row_copies_container_name() {
        let mut d = container_info_fixture();
        let (row, col, _, _) = d.box_rect(40, 100);
        match d.handle_click(row + 2, col + 2, 40, 100) {
            DialogAction::CopyToClipboard(payload) => {
                assert_eq!(payload, "jk-abc123-thearchitect");
            }
            other => panic!("Container ID row click must request clipboard copy, got {other:?}"),
        }
        let Dialog::ContainerInfo { copied, .. } = d else {
            unreachable!()
        };
        assert!(copied, "ID row click must show copy feedback");
    }

    #[test]
    fn container_info_click_on_other_rows_does_not_copy() {
        let mut d = container_info_fixture();
        let (row, col, _, _) = d.box_rect(40, 100);
        assert_eq!(
            d.handle_click(row + 3, col + 2, 40, 100),
            DialogAction::Consume
        );
        let Dialog::ContainerInfo { copied, .. } = d else {
            unreachable!()
        };
        assert!(!copied, "non-ID rows must not show copy feedback");
    }

    #[test]
    fn container_info_clear_copy_feedback_hides_badge() {
        let mut d = Dialog::ContainerInfo {
            container_name: "jk-abc123-thearchitect".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace/jackin".to_string(),
            git_loading: false,
            git_branch: Some("feature/container-info".to_string()),
            pull_request_loading: false,
            pull_request_url: Some("https://github.com/jackin-project/jackin/pull/123".to_string()),
            copied: true,
        };
        assert!(d.clear_copy_feedback());
        let Dialog::ContainerInfo { copied, .. } = d else {
            unreachable!()
        };
        assert!(!copied);
    }

    #[test]
    fn container_info_copied_badge_survives_long_container_name() {
        let d = Dialog::ContainerInfo {
            container_name: "jk-c9g7zpkh-jackin-thearchitect-extra-long".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace/jackin".to_string(),
            git_loading: false,
            git_branch: Some("feature/container-info".to_string()),
            pull_request_loading: false,
            pull_request_url: Some("https://github.com/jackin-project/jackin/pull/123".to_string()),
            copied: true,
        };
        let mut buf = Vec::new();
        d.render(&mut buf, 40, 100);
        let rendered = String::from_utf8_lossy(&buf);
        assert!(
            rendered.contains("Copied!"),
            "long container IDs must not push copy feedback out of the dialog: {rendered:?}"
        );
    }

    #[test]
    fn container_info_renders_workdir_branch_and_pr_url() {
        let d = container_info_fixture();
        let mut buf = Vec::new();
        d.render(&mut buf, 40, 120);
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("Workdir"));
        assert!(rendered.contains("/workspace/jackin"));
        assert!(rendered.contains("Branch"));
        assert!(rendered.contains("feature/container-info"));
        assert!(rendered.contains("Pull Request"));
        assert!(rendered.contains("https://github.com/jackin-project/jackin/pull/123"));
    }

    #[test]
    fn container_info_renders_git_context_loading_state() {
        let d = Dialog::ContainerInfo {
            container_name: "jk-abc123-thearchitect".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace/jackin".to_string(),
            git_loading: true,
            git_branch: None,
            pull_request_loading: true,
            pull_request_url: None,
            copied: false,
        };
        let mut buf = Vec::new();
        d.render(&mut buf, 40, 120);
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("Branch"));
        assert!(rendered.contains("Pull Request"));
        assert_eq!(rendered.matches("⠋ loading").count(), 2);
    }

    #[test]
    fn container_info_esc_dismisses() {
        let mut d = container_info_fixture();
        assert_eq!(d.handle_key(b"\x1b"), DialogAction::Dismiss);
    }

    #[test]
    fn container_info_q_dismisses() {
        // ContainerInfo has no editable input, so `q` is also a valid
        // dismiss key (same as the list-style dialogs).
        let mut d = container_info_fixture();
        assert_eq!(d.handle_key(b"q"), DialogAction::Dismiss);
    }

    #[test]
    fn container_info_arrow_keys_are_redraw_noops() {
        // Read-only modal, no navigation. Arrow keys must neither
        // dismiss the dialog nor produce a Command-like action — a
        // bare Redraw keeps the box on screen and waits for Enter /
        // Esc.
        let mut d = container_info_fixture();
        assert_eq!(d.handle_key(b"\x1b[A"), DialogAction::Redraw);
        assert_eq!(d.handle_key(b"\x1b[B"), DialogAction::Redraw);
        assert_eq!(d.handle_key(b"\x1b[C"), DialogAction::Redraw);
        assert_eq!(d.handle_key(b"\x1b[D"), DialogAction::Redraw);
    }
}
