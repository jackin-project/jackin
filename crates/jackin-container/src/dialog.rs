/// Ctrl+J command palette and agent picker modal.
///
/// The dialog is rendered as a centered floating overlay on top of the
/// current compositor frame. The daemon renders it when
/// `Multiplexer::dialog` is `Some(Dialog)`.

const PALETTE_WIDTH: u16 = 40;
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const BG_DARK: &str = "\x1b[48;5;235m";  // very dark grey background
const FG_GREEN: &str = "\x1b[38;5;46m";
const FG_WHITE: &str = "\x1b[38;5;255m";
const FG_GREY: &str = "\x1b[38;5;244m";
const SELECTED_BG: &str = "\x1b[48;5;238m"; // selected row background

#[derive(Debug, Clone)]
pub enum Dialog {
    CommandPalette { selected: usize },
    AgentPicker { agents: Vec<String>, selected: usize },
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
    SplitHorizontal,
    SplitVertical,
    NewTab,
    NewSession,
    ClosePane,
    ZoomPane,
}

const PALETTE_ITEMS: &[(PaletteCommand, &str)] = &[
    (PaletteCommand::NewSession,      "New agent session"),
    (PaletteCommand::SplitHorizontal, "Split pane │ (side by side)"),
    (PaletteCommand::SplitVertical,   "Split pane ─ (top / bottom)"),
    (PaletteCommand::NewTab,          "New tab"),
    (PaletteCommand::ZoomPane,        "Zoom / unzoom pane"),
    (PaletteCommand::ClosePane,       "Close pane"),
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
                        if *selected > 0 { *selected -= 1; }
                        DialogAction::Redraw
                    }
                    b"\x1b[B" | b"j" => {
                        // Down
                        if *selected + 1 < PALETTE_ITEMS.len() { *selected += 1; }
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
                        if *selected > 0 { *selected -= 1; }
                        DialogAction::Redraw
                    }
                    b"\x1b[B" | b"j" => {
                        if *selected + 1 < agents.len() + 1 { *selected += 1; }
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

fn render_palette(buf: &mut Vec<u8>, term_rows: u16, term_cols: u16, selected: usize) {
    let items = PALETTE_ITEMS;
    let height = items.len() as u16 + 4; // title + border + items + padding
    let width = PALETTE_WIDTH;
    let start_row = (term_rows.saturating_sub(height)) / 2;
    let start_col = (term_cols.saturating_sub(width)) / 2;

    render_box(buf, start_row, start_col, height, width, "jackin' commands");

    for (i, (_, label)) in items.iter().enumerate() {
        let row = start_row + 2 + i as u16;
        let col = start_col + 1;
        move_to(buf, row, col);
        if i == selected {
            buf.extend_from_slice(SELECTED_BG.as_bytes());
            buf.extend_from_slice(FG_GREEN.as_bytes());
            buf.extend_from_slice(BOLD.as_bytes());
            buf.push(b'>');
        } else {
            buf.extend_from_slice(BG_DARK.as_bytes());
            buf.extend_from_slice(FG_GREY.as_bytes());
            buf.push(b' ');
        }
        buf.push(b' ');
        let label_bytes = label.as_bytes();
        buf.extend_from_slice(&label_bytes[..label_bytes.len().min((width - 4) as usize)]);
        // Pad to width.
        let used = label_bytes.len() + 2;
        let pad = (width as usize).saturating_sub(used + 2);
        for _ in 0..pad { buf.push(b' '); }
        buf.extend_from_slice(RESET.as_bytes());
    }

    render_hint(buf, start_row + height - 1, start_col, width, "↑↓ navigate  Enter confirm  Esc dismiss");
}

fn render_agent_picker(
    buf: &mut Vec<u8>,
    term_rows: u16,
    term_cols: u16,
    agents: &[String],
    selected: usize,
) {
    let item_count = agents.len() + 1; // +1 for Shell
    let height = item_count as u16 + 4;
    let width = PALETTE_WIDTH;
    let start_row = (term_rows.saturating_sub(height)) / 2;
    let start_col = (term_cols.saturating_sub(width)) / 2;

    render_box(buf, start_row, start_col, height, width, "Launch session");

    let mut all_items: Vec<String> = agents.to_vec();
    all_items.push("Shell".to_string());

    for (i, label) in all_items.iter().enumerate() {
        let row = start_row + 2 + i as u16;
        let col = start_col + 1;
        move_to(buf, row, col);
        if i == selected {
            buf.extend_from_slice(SELECTED_BG.as_bytes());
            buf.extend_from_slice(FG_GREEN.as_bytes());
            buf.extend_from_slice(BOLD.as_bytes());
            buf.push(b'>');
        } else {
            buf.extend_from_slice(BG_DARK.as_bytes());
            buf.extend_from_slice(FG_GREY.as_bytes());
            buf.push(b' ');
        }
        buf.push(b' ');
        buf.extend_from_slice(label.as_bytes());
        let pad = (width as usize).saturating_sub(label.len() + 4);
        for _ in 0..pad { buf.push(b' '); }
        buf.extend_from_slice(RESET.as_bytes());
    }

    render_hint(buf, start_row + height - 1, start_col, width, "↑↓ navigate  Enter launch  Esc dismiss");
}

fn render_box(buf: &mut Vec<u8>, row: u16, col: u16, height: u16, width: u16, title: &str) {
    // Top border with title.
    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.extend_from_slice(BOLD.as_bytes());
    buf.extend_from_slice("┌".as_bytes());
    buf.extend_from_slice(" ".as_bytes());
    buf.extend_from_slice(FG_WHITE.as_bytes());
    buf.extend_from_slice(title.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.extend_from_slice(" ".as_bytes());
    let title_len = title.len() as u16 + 4; // ┌ + spaces + title + space
    for _ in title_len..(width - 1) { buf.extend_from_slice("─".as_bytes()); }
    buf.extend_from_slice("┐".as_bytes());

    // Side borders + interior.
    for r in 1..(height - 1) {
        move_to(buf, row + r, col);
        buf.extend_from_slice(BG_DARK.as_bytes());
        buf.extend_from_slice(FG_GREEN.as_bytes());
        buf.extend_from_slice("│".as_bytes());
        for _ in 1..(width - 1) { buf.push(b' '); }
        buf.extend_from_slice("│".as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
    }

    // Bottom border.
    move_to(buf, row + height - 1, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREEN.as_bytes());
    buf.extend_from_slice("└".as_bytes());
    for _ in 1..(width - 1) { buf.extend_from_slice("─".as_bytes()); }
    buf.extend_from_slice("┘".as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
}

fn render_hint(buf: &mut Vec<u8>, row: u16, col: u16, width: u16, hint: &str) {
    move_to(buf, row, col + 1);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_GREY.as_bytes());
    let hint_bytes = &hint.as_bytes()[..hint.len().min((width - 2) as usize)];
    buf.extend_from_slice(hint_bytes);
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
    if n == 0 { buf.push(b'0'); return; }
    let mut tmp = [0u8; 5];
    let mut i = 5;
    let mut v = n;
    while v > 0 { i -= 1; tmp[i] = b'0' + (v % 10) as u8; v /= 10; }
    buf.extend_from_slice(&tmp[i..]);
}
