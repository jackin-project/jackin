/// Status bar rendered at the top row of the host terminal.
///
/// Layout: [jackin'] [Tab1] [Tab2] [Tab3 ●]
///         ↑ brand   ↑ tabs — active tab bold+underline, inactive dimmed
///
/// "jackin'" has its own bright-green background to brand the screen
/// so operators sharing their display can immediately see they are
/// inside Jackin.
use crate::layout::Tab;
use crate::protocol::AgentState;

// ANSI color for the "jackin'" brand pill:
// bright green background (#00ff41 → 256-color index 46), black text.
const BRAND_BG: &str = "\x1b[48;5;46m";
const BRAND_FG: &str = "\x1b[38;5;16m"; // near-black
const BRAND_BOLD: &str = "\x1b[1m";

const TAB_ACTIVE_FG: &str = "\x1b[38;5;46m"; // green text
const TAB_ACTIVE_BOLD: &str = "\x1b[1m";
const TAB_INACTIVE_FG: &str = "\x1b[38;5;245m"; // grey
const TAB_SEP: &str = "  ";
const RESET: &str = "\x1b[0m";

pub struct StatusBar {
    /// (start_col, end_col) of each tab, for mouse hit detection.
    pub tab_regions: Vec<(u16, u16)>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            tab_regions: Vec::new(),
        }
    }

    /// Render the status bar into `buf`, returning ANSI escape sequences
    /// that draw row 0 of the host terminal.
    pub fn render(
        &mut self,
        buf: &mut Vec<u8>,
        cols: u16,
        tabs: &[Tab],
        active_tab: usize,
        sessions_state: &[(u64, AgentState)],
    ) {
        self.tab_regions.clear();

        // Move to row 1, col 1 (1-based).
        buf.extend_from_slice(b"\x1b[1;1H");
        buf.extend_from_slice(b"\x1b[2K"); // erase line

        // Brand pill: " jackin' "
        buf.extend_from_slice(BRAND_BG.as_bytes());
        buf.extend_from_slice(BRAND_FG.as_bytes());
        buf.extend_from_slice(BRAND_BOLD.as_bytes());
        buf.extend_from_slice(b" jackin' ");
        buf.extend_from_slice(RESET.as_bytes());
        buf.extend_from_slice(b" "); // separator after brand

        // Track current column position (1-based, after brand).
        // "jackin'" is 9 chars + 1 space = 10 cols. Brand pill " jackin' " is 9 chars,
        // and we add 1 space. Total: 10 chars so far.
        let mut col: u16 = 11; // 1-based: 1..=9 brand, 10 space, col 11 is next

        for (i, tab) in tabs.iter().enumerate() {
            // Compute the display label for this tab.
            let label = tab_label(tab, sessions_state);
            let label_len = label.chars().count() as u16;
            // Tab is: " {label} "
            let tab_width = label_len + 2; // spaces on each side

            let tab_start = col;
            let tab_end = col + tab_width;

            // Register mouse region.
            self.tab_regions.push((tab_start, tab_end));

            if i == active_tab {
                buf.extend_from_slice(TAB_ACTIVE_BOLD.as_bytes());
                buf.extend_from_slice(TAB_ACTIVE_FG.as_bytes());
            } else {
                buf.extend_from_slice(RESET.as_bytes());
                buf.extend_from_slice(TAB_INACTIVE_FG.as_bytes());
            }
            buf.push(b' ');
            buf.extend_from_slice(label.as_bytes());
            buf.push(b' ');
            buf.extend_from_slice(RESET.as_bytes());
            buf.extend_from_slice(TAB_SEP.as_bytes());

            col = tab_end + TAB_SEP.len() as u16;
            if col >= cols {
                break;
            }
        }

        buf.extend_from_slice(RESET.as_bytes());
    }

    /// Return the tab index clicked at column `c` (1-based), if any.
    pub fn tab_at_col(&self, c: u16) -> Option<usize> {
        self.tab_regions
            .iter()
            .position(|&(start, end)| c >= start && c < end)
    }
}

fn tab_label(tab: &Tab, states: &[(u64, AgentState)]) -> String {
    // Check if any session in this tab is blocked (most urgent state).
    let ids = tab.tree.all_ids();
    let has_blocked = ids.iter().any(|id| {
        states
            .iter()
            .any(|(sid, st)| sid == id && *st == AgentState::Blocked)
    });
    let has_done = ids.iter().any(|id| {
        states
            .iter()
            .any(|(sid, st)| sid == id && *st == AgentState::Done)
    });

    if has_blocked {
        format!("{} ●", tab.label)
    } else if has_done {
        format!("{} ○", tab.label)
    } else {
        tab.label.clone()
    }
}

/// Draw a vertical border line at column `col` for rows `from_row..=to_row`.
/// Used to separate panes in an HSplit.
pub fn draw_vertical_border(buf: &mut Vec<u8>, col: u16, from_row: u16, to_row: u16, active: bool) {
    let color = if active {
        "\x1b[38;5;46m"
    } else {
        "\x1b[38;5;238m"
    };
    for row in from_row..=to_row {
        // Move to row, col (1-based).
        buf.extend_from_slice(b"\x1b[");
        write_dec(buf, row + 1);
        buf.push(b';');
        write_dec(buf, col + 1);
        buf.push(b'H');
        buf.extend_from_slice(color.as_bytes());
        buf.extend_from_slice("│".as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
    }
}

/// Draw a horizontal border line at row `row` for cols `from_col..=to_col`.
pub fn draw_horizontal_border(
    buf: &mut Vec<u8>,
    row: u16,
    from_col: u16,
    to_col: u16,
    active: bool,
) {
    let color = if active {
        "\x1b[38;5;46m"
    } else {
        "\x1b[38;5;238m"
    };
    buf.extend_from_slice(b"\x1b[");
    write_dec(buf, row + 1);
    buf.push(b';');
    write_dec(buf, from_col + 1);
    buf.push(b'H');
    buf.extend_from_slice(color.as_bytes());
    for _ in from_col..=to_col {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice(RESET.as_bytes());
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
