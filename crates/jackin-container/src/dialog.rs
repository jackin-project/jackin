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

#[derive(Debug, Clone)]
pub enum Dialog {
    CommandPalette {
        selected: usize,
    },
    AgentPicker {
        agents: Vec<String>,
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAction {
    /// User confirmed a command-palette item.
    Command(PaletteCommand),
    /// User picked an agent slug (or "shell").
    SpawnAgent { agent: Option<String> },
    /// User dismissed with Escape.
    Dismiss,
    /// Dialog is still open; redraw.
    Redraw,
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
const PALETTE_ITEMS: &[(PaletteCommand, &str)] = &[
    (PaletteCommand::NewTab, "New tab"),
    (PaletteCommand::NextTab, "Next tab"),
    (PaletteCommand::PrevTab, "Previous tab"),
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
        match self {
            Self::CommandPalette { selected } => {
                match key {
                    b"\x1b" | b"q" => DialogAction::Dismiss,
                    b"\x1b[A" | b"k" => {
                        // Up
                        if *selected > 0 {
                            *selected -= 1;
                        }
                        DialogAction::Redraw
                    }
                    b"\x1b[B" | b"j" => {
                        // Down
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
                }
            }
            Self::AgentPicker { agents, selected } => {
                match key {
                    b"\x1b" | b"q" => DialogAction::Dismiss,
                    b"\x1b[A" | b"k" => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                        DialogAction::Redraw
                    }
                    b"\x1b[B" | b"j" => {
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
                        DialogAction::SpawnAgent { agent }
                    }
                    _ => DialogAction::Redraw,
                }
            }
        }
    }

    /// Render the dialog overlay into `buf`.
    /// `term_rows` and `term_cols` are the host terminal dimensions.
    pub fn render(&self, buf: &mut Vec<u8>, term_rows: u16, term_cols: u16) {
        match self {
            Self::CommandPalette { selected } => {
                render_palette(buf, term_rows, term_cols, *selected);
            }
            Self::AgentPicker { agents, selected } => {
                render_agent_picker(buf, term_rows, term_cols, agents, *selected);
            }
        }
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

fn render_palette(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16, selected: usize) {
    let items = PALETTE_ITEMS;
    let height = items.len() as u16 + 5;
    let width = PALETTE_WIDTH;
    let start_row = (term_rows.saturating_sub(height)) / 2;
    let start_col = (term_cols.saturating_sub(width)) / 2;

    render_box(buf, start_row, start_col, height, width, "jackin' commands");
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
    render_hint(
        buf,
        start_row + height - 2,
        start_col + 1,
        width,
        PALETTE_HINT,
    );
}

fn render_agent_picker(
    buf: &mut Vec<u8>,
    term_rows: u16,
    term_cols: u16,
    agents: &[String],
    selected: usize,
) {
    let item_count = agents.len() + 1; // +1 for Shell
    let height = item_count as u16 + 5;
    let width = PALETTE_WIDTH;
    let start_row = (term_rows.saturating_sub(height)) / 2;
    let start_col = (term_cols.saturating_sub(width)) / 2;

    render_box(buf, start_row, start_col, height, width, "Launch session");

    let mut all_items: Vec<String> = agents.to_vec();
    all_items.push("Shell".to_string());

    for (i, label) in all_items.iter().enumerate() {
        render_row(
            buf,
            start_row + 2 + i as u16,
            start_col + 1,
            width,
            label,
            i == selected,
        );
    }

    render_hint(
        buf,
        start_row + height - 2,
        start_col + 1,
        width,
        PICKER_HINT,
    );
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
    // Both markers occupy 2 display columns; the row interior is
    // `width - 2` columns wide, so labels and trailing pad share
    // `width - 4` columns.
    let max_label_cols = (width as usize).saturating_sub(4);
    let label_cols = label.chars().count();
    let truncated_cols = label_cols.min(max_label_cols);
    let label_take: String = label.chars().take(truncated_cols).collect();
    buf.extend_from_slice(label_take.as_bytes());
    let pad_cols = max_label_cols.saturating_sub(truncated_cols) + 1; // +1 trailing pad column
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

/// Render the dedicated hint row inside the box (above the bottom
/// border). Each `HintSpan` carries its own colour so hotkeys read
/// distinct from labels — same convention as the console TUI footer.
/// Clipping is done by **display column count**, not byte count, so
/// multibyte glyphs (`↑↓`, `·`) survive a tight width without
/// truncating mid-string.
fn render_hint(buf: &mut Vec<u8>, row: u16, col: u16, width: u16, spans: &[HintSpan<'_>]) {
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    let interior = (width as usize).saturating_sub(2);
    let mut cols_used = 0usize;
    for span in spans {
        let (text, style) = match span {
            HintSpan::Key(k) => (
                std::borrow::Cow::Borrowed(*k),
                Some([BG_DARK, FG_WHITE, BOLD]),
            ),
            HintSpan::Text(t) => (
                std::borrow::Cow::Owned(format!(" {t}")),
                Some([BG_DARK, FG_GREEN, ""]),
            ),
            HintSpan::Sep => (
                std::borrow::Cow::Borrowed(" · "),
                Some([BG_DARK, FG_BORDER, ""]),
            ),
            HintSpan::GroupSep => (std::borrow::Cow::Borrowed("   "), None),
        };
        let cols = text.chars().count();
        if cols_used + cols > interior {
            break;
        }
        if let Some(styles) = style {
            for s in styles {
                if !s.is_empty() {
                    buf.extend_from_slice(s.as_bytes());
                }
            }
        } else {
            buf.extend_from_slice(BG_DARK.as_bytes());
            buf.extend_from_slice(FG_GREEN.as_bytes());
        }
        buf.extend_from_slice(text.as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
        cols_used += cols;
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
