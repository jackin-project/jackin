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
    /// Split the focused pane side-by-side and spawn the chosen
    /// agent / shell in the new pane.
    SplitHorizontal,
    /// Split the focused pane top/bottom and spawn the chosen
    /// agent / shell in the new pane.
    SplitVertical,
}

/// Cap on operator-typed tab labels. Long names break the tab-strip
/// layout (each tab cell grows with its label width), so the input
/// stops accepting characters past this limit. 16 is enough for the
/// agent names (`OpenCode`) plus a short qualifier the operator picks.
pub const MAX_CUSTOM_LABEL_LEN: usize = 16;

#[derive(Debug, Clone)]
pub enum Dialog {
    CommandPalette {
        selected: usize,
    },
    AgentPicker {
        agents: Vec<String>,
        selected: usize,
        intent: PickerIntent,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAction {
    /// User confirmed a command-palette item.
    Command(PaletteCommand),
    /// User picked an agent slug (or "shell"). `intent` tells the
    /// daemon whether to spawn it as a tab or as a split pane.
    SpawnAgent {
        agent: Option<String>,
        intent: PickerIntent,
    },
    /// Operator typed a new tab label and pressed Enter. Empty
    /// `label` clears the existing custom label and re-enables
    /// auto-naming.
    RenameTab {
        tab_idx: usize,
        label: String,
    },
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
    SplitHorizontal,
    SplitVertical,
    ZoomPane,
    ClosePane,
    CloseTab,
    Detach,
}

/// The "New agent session" entry was removed: it was a duplicate of
/// "New tab" — both opened the agent picker and spawned a new tab
/// for the chosen agent or Shell. The single `New tab` entry now
/// owns that path.
/// Next/Previous tab are not exposed in the palette: the operator
/// already clicks tabs directly in the status bar, and the
/// keyboard-driven shortcut for cycle-tab is the tmux-style prefix
/// gesture (`Ctrl+B n` / `Ctrl+B p`). Keeping list entries that only
/// duplicate those existing paths bloats the modal with no new
/// capability. `PaletteCommand::NextTab` / `PrevTab` stay in the enum
/// so prefix-mode bindings continue to work.
const PALETTE_ITEMS: &[(PaletteCommand, &str)] = &[
    (PaletteCommand::NewTab, "New tab"),
    (
        PaletteCommand::SplitHorizontal,
        "Split pane │ (side by side)",
    ),
    (PaletteCommand::SplitVertical, "Split pane ─ (top / bottom)"),
    (PaletteCommand::ZoomPane, "Zoom / unzoom pane"),
    (PaletteCommand::ClosePane, "Close pane"),
    (PaletteCommand::CloseTab, "Close tab"),
    (PaletteCommand::Detach, "Detach"),
];

impl Dialog {
    /// Handle a raw key byte and return the resulting action.
    pub fn handle_key(&mut self, key: &[u8]) -> DialogAction {
        // Text-input dialog has its own dismissal / editing rules and
        // must intercept keys before the arrow-key + dismiss-key
        // shortcuts below would steal them (e.g. `q` is a legal
        // character inside a custom tab name).
        if let Self::RenameTab { tab_idx, input } = self {
            return rename_tab_handle_key(*tab_idx, input, key);
        }
        // From here on, only the list-style dialogs reach this code
        // path. The arrow / dismiss / character branches do not need
        // to enumerate `RenameTab` — the early return above is the
        // single source of truth for that variant.
        if is_dismiss_key(key) {
            return DialogAction::Dismiss;
        }
        if is_arrow_up(key) {
            return match self {
                Self::CommandPalette { selected } | Self::AgentPicker { selected, .. } => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                    DialogAction::Redraw
                }
                Self::RenameTab { .. } => DialogAction::Redraw,
            };
        }
        if is_arrow_down(key) {
            return match self {
                Self::CommandPalette { selected } => {
                    if *selected + 1 < PALETTE_ITEMS.len() {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                Self::AgentPicker {
                    agents, selected, ..
                } => {
                    if *selected + 1 < agents.len() + 1 {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                Self::RenameTab { .. } => DialogAction::Redraw,
            };
        }
        match self {
            Self::RenameTab { .. } => DialogAction::Redraw,
            Self::CommandPalette { selected } => match key {
                b"k" => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                    DialogAction::Redraw
                }
                b"j" => {
                    if *selected + 1 < PALETTE_ITEMS.len() {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                b"\r" | b"\n" => {
                    let cmd = PALETTE_ITEMS[*selected].0.clone();
                    DialogAction::Command(cmd)
                }
                _ => DialogAction::Redraw,
            },
            Self::AgentPicker {
                agents,
                selected,
                intent,
            } => match key {
                b"k" => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                    DialogAction::Redraw
                }
                b"j" => {
                    if *selected + 1 < agents.len() + 1 {
                        *selected += 1;
                    }
                    DialogAction::Redraw
                }
                b"\r" | b"\n" => {
                    let agent = if *selected < agents.len() {
                        Some(agents[*selected].clone())
                    } else {
                        None // Shell
                    };
                    DialogAction::SpawnAgent {
                        agent,
                        intent: *intent,
                    }
                }
                _ => DialogAction::Redraw,
            },
        }
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
        let inside_box = row >= box_row
            && row < box_row + height
            && col >= box_col
            && col < box_col + width;
        if !inside_box {
            return DialogAction::Dismiss;
        }
        // Text-input dialog has no clickable rows — clicks inside the
        // box are just swallowed so they don't dismiss or reach the
        // pane underneath.
        if matches!(self, Self::RenameTab { .. }) {
            return DialogAction::Consume;
        }
        // First content row sits two rows down from the top border
        // (top border + blank pad). Rows above and below the item
        // list are decorative.
        let first_item_row = box_row + 2;
        let item_count = match self {
            Self::CommandPalette { .. } => PALETTE_ITEMS.len() as u16,
            // Agent picker rows: agents + separator + Shell. The
            // separator row is non-selectable.
            Self::AgentPicker { agents, .. } => agents.len() as u16 + 2,
            // RenameTab is handled by the early consume-on-click
            // return above. Treat the post-check as "no rows" so the
            // outer match still type-checks without a panicky
            // unreachable!.
            Self::RenameTab { .. } => 0,
        };
        if row < first_item_row || row >= first_item_row + item_count {
            return DialogAction::Consume;
        }
        let row_idx = (row - first_item_row) as usize;
        match self {
            Self::CommandPalette { selected } => {
                *selected = row_idx;
                let cmd = PALETTE_ITEMS[row_idx].0.clone();
                DialogAction::Command(cmd)
            }
            Self::AgentPicker {
                agents,
                selected,
                intent,
            } => {
                // The separator sits immediately after the last
                // agent; clicking it is a no-op. Shell sits one
                // row past the separator.
                if row_idx == agents.len() {
                    return DialogAction::Consume;
                }
                if row_idx > agents.len() {
                    *selected = agents.len();
                    return DialogAction::SpawnAgent {
                        agent: None,
                        intent: *intent,
                    };
                }
                *selected = row_idx;
                DialogAction::SpawnAgent {
                    agent: Some(agents[row_idx].clone()),
                    intent: *intent,
                }
            }
            // Same fallthrough as `item_count` above: RenameTab clicks
            // were already handled by the early Consume return so this
            // arm cannot fire in practice. Return Consume rather than
            // panic so a future refactor that drops the early return
            // degrades cleanly.
            Self::RenameTab { .. } => DialogAction::Consume,
        }
    }

    /// Box geometry the dialog will render with for `term_rows` /
    /// `term_cols`. Returned as `(row, col, height, width)`. Kept
    /// next to the render functions so any layout change updates
    /// both surfaces at once.
    fn box_rect(&self, term_rows: u16, term_cols: u16) -> (u16, u16, u16, u16) {
        let width = PALETTE_WIDTH;
        let height = match self {
            Self::CommandPalette { .. } => PALETTE_ITEMS.len() as u16 + 4,
            Self::AgentPicker { agents, .. } => agents.len() as u16 + 2 + 4,
            // Rename modal: top border + blank pad + input row + blank pad + bottom border.
            Self::RenameTab { .. } => 5,
        };
        let row = (term_rows.saturating_sub(height)) / 2;
        let col = (term_cols.saturating_sub(width)) / 2;
        (row, col, height, width)
    }

    /// Render the dialog overlay into `buf`.
    /// `term_rows` and `term_cols` are the host terminal dimensions.
    pub fn render(&self, buf: &mut Vec<u8>, term_rows: u16, term_cols: u16) {
        match self {
            Self::CommandPalette { selected } => {
                render_palette(buf, term_rows, term_cols, *selected);
            }
            Self::AgentPicker {
                agents,
                selected,
                intent,
            } => {
                render_agent_picker(buf, term_rows, term_cols, agents, *selected, *intent);
            }
            Self::RenameTab { input, .. } => {
                render_rename_tab(buf, term_rows, term_cols, input.value());
            }
        }
    }
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
        bytes if bytes.len() == 1 && (0x20..0x7f).contains(&bytes[0]) => {
            input.insert_char(bytes[0] as char);
            DialogAction::Redraw
        }
        _ => DialogAction::Redraw,
    }
}

/// Universal dialog-dismiss keys. Operators reach for `Esc` and `q`
/// most often, but Backspace, Delete, and `Ctrl+C` are common
/// muscle-memory fallbacks. Uppercase `Q` is included so a shift-key
/// slip doesn't trap the operator inside the dialog.
fn is_arrow_up(key: &[u8]) -> bool {
    matches!(key, b"\x1b[A" | b"\x1bOA")
}

fn is_arrow_down(key: &[u8]) -> bool {
    matches!(key, b"\x1b[B" | b"\x1bOB")
}

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

const PALETTE_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Key("Enter"),
    HintSpan::Text("confirm"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("dismiss"),
];

const PICKER_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Key("Enter"),
    HintSpan::Text("launch"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("dismiss"),
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

/// Render the tab-rename modal. One text-input row showing the current
/// buffer plus a blinking-style trailing `▌` caret. Width matches the
/// other dialogs so the operator's eye does not have to re-anchor.
fn render_rename_tab(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16, input: &str) {
    let width = PALETTE_WIDTH;
    let height: u16 = 5;
    let start_row = (term_rows.saturating_sub(height)) / 2;
    let start_col = (term_cols.saturating_sub(width)) / 2;

    render_box(buf, start_row, start_col, height, width, "Rename tab");

    // Input row at the box interior (row index 2 from top: top border +
    // blank pad + input). Render: `▸ <text>▌` then pad to interior end.
    let row = start_row + 2;
    let col = start_col + 1;
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    let prefix = "  ";
    buf.extend_from_slice(prefix.as_bytes());
    buf.extend_from_slice(FG_WHITE.as_bytes());
    buf.extend_from_slice(input.as_bytes());
    // Caret marker so the operator can see the text input is live.
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.extend_from_slice("▌".as_bytes());
    let used = prefix.chars().count() + input.chars().count() + 1; // +1 for caret
    let max_interior = (width as usize).saturating_sub(2);
    for _ in used..max_interior {
        buf.push(b' ');
    }
    buf.extend_from_slice(RESET.as_bytes());

    render_bottom_hint(buf, term_rows, term_cols, RENAME_HINT);
}

fn render_palette(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16, selected: usize) {
    let items = PALETTE_ITEMS;
    // Box rows: top border + blank pad + items + blank pad + bottom border.
    let height = items.len() as u16 + 4;
    let width = PALETTE_WIDTH;
    let start_row = (term_rows.saturating_sub(height)) / 2;
    let start_col = (term_cols.saturating_sub(width)) / 2;

    render_box(buf, start_row, start_col, height, width, "Menu");
    for (i, (_, label)) in items.iter().enumerate() {
        render_row(
            buf,
            start_row + 2 + i as u16,
            start_col + 1,
            width,
            label,
            i == selected,
        );
    }
    render_bottom_hint(buf, term_rows, term_cols, PALETTE_HINT);
}

fn render_agent_picker(
    buf: &mut Vec<u8>,
    term_rows: u16,
    term_cols: u16,
    agents: &[String],
    selected: usize,
    intent: PickerIntent,
) {
    // Render rows: agents + separator + Shell. Selection space is
    // `agents.len() + 1` (the separator is not selectable).
    let render_row_count = agents.len() as u16 + 2; // agents + separator + shell
    let height = render_row_count + 4;
    let width = PALETTE_WIDTH;
    let start_row = (term_rows.saturating_sub(height)) / 2;
    let start_col = (term_cols.saturating_sub(width)) / 2;

    let title = match intent {
        PickerIntent::NewTab => "New tab",
        PickerIntent::SplitHorizontal => "Split pane │  (side by side)",
        PickerIntent::SplitVertical => "Split pane ─  (top / bottom)",
    };
    render_box(buf, start_row, start_col, height, width, title);

    // Agent rows. Each agent slug is mapped through
    // `jackin_tui::agent_display_name` so labels match the console
    // TUI's `agent_picker_label` (Title case + `OpenCode` spelling).
    for (i, slug) in agents.iter().enumerate() {
        let label = jackin_tui::agent_display_name(slug.as_str()).unwrap_or(slug.as_str());
        render_row(
            buf,
            start_row + 2 + i as u16,
            start_col + 1,
            width,
            label,
            i == selected,
        );
    }
    // Separator row between agents and Shell. Non-selectable.
    render_separator(
        buf,
        start_row + 2 + agents.len() as u16,
        start_col + 1,
        width,
        "shell",
    );
    // Shell row at the final selection slot.
    render_row(
        buf,
        start_row + 2 + agents.len() as u16 + 1,
        start_col + 1,
        width,
        "Shell",
        selected == agents.len(),
    );

    render_bottom_hint(buf, term_rows, term_cols, PICKER_HINT);
}

/// Non-selectable visual divider inside the agent picker — `── shell ──`
/// in dim phosphor-green. Sets the operator's expectation that the
/// row below the divider is a different *kind* of session, not just
/// another agent.
fn render_separator(buf: &mut Vec<u8>, row: u16, col: u16, width: u16, label: &str) {
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
    // Interior width: `width - 2` cols.
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
    if total >= term_cols as usize || term_rows == 0 {
        return;
    }
    let start_col = ((term_cols as usize - total) / 2) as u16;
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
