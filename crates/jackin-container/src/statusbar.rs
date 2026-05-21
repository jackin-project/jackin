/// Status bar rendered at row 0 of the host terminal.
///
/// Layout: ` jackin' ` brand pill, then a tab strip with a state glyph
/// appended to each tab label, then a right-aligned prefix-mode hint
/// (`detach: Ctrl+B d` in idle state, `prefix…` immediately after the
/// prefix byte arrives). When tabs overflow the terminal width an
/// overflow indicator (`›`) appears at the right edge.
use crate::layout::Tab;
use crate::protocol::AgentState;

const BRAND_BG: &str = "\x1b[48;5;46m";
const BRAND_FG: &str = "\x1b[38;5;16m";
const BRAND_BOLD: &str = "\x1b[1m";

const TAB_ACTIVE_FG: &str = "\x1b[38;5;46m";
const TAB_ACTIVE_BOLD: &str = "\x1b[1m";
const TAB_INACTIVE_FG: &str = "\x1b[38;5;245m";
const HINT_FG: &str = "\x1b[38;5;244m";
const RESET: &str = "\x1b[0m";

const TAB_SEP: &str = "  ";
const BRAND_TEXT: &str = " jackin' ";
const BRAND_PAD_COLS: u16 = 1; // single space between brand pill and first tab

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixMode {
    Idle,
    Awaiting,
}

pub struct StatusBar {
    pub tab_regions: Vec<(u16, u16)>,
    pub prefix_mode: PrefixMode,
    pub prefix_label: String,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            tab_regions: Vec::new(),
            prefix_mode: PrefixMode::Idle,
            prefix_label: "Ctrl+B".to_string(),
        }
    }

    pub fn set_prefix_mode(&mut self, mode: PrefixMode) {
        self.prefix_mode = mode;
    }

    /// Render the status bar at row 0. Returns the cumulative byte buffer
    /// the caller appends to the wire frame.
    pub fn render(
        &mut self,
        buf: &mut Vec<u8>,
        cols: u16,
        tabs: &[Tab],
        active_tab: usize,
        sessions_state: &[(u64, AgentState)],
    ) {
        self.tab_regions.clear();

        buf.extend_from_slice(b"\x1b[1;1H\x1b[2K");

        // Brand pill.
        buf.extend_from_slice(BRAND_BG.as_bytes());
        buf.extend_from_slice(BRAND_FG.as_bytes());
        buf.extend_from_slice(BRAND_BOLD.as_bytes());
        buf.extend_from_slice(BRAND_TEXT.as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
        for _ in 0..BRAND_PAD_COLS {
            buf.push(b' ');
        }

        let hint = self.right_hint();
        let hint_cols = hint.chars().count() as u16;
        let reserve_right: u16 = hint_cols + 2; // 1 col padding + 1 trailing space

        // Track 1-based column where the next tab cell begins.
        let mut col: u16 = (BRAND_TEXT.chars().count() as u16) + BRAND_PAD_COLS + 1;
        let max_tab_col = cols.saturating_sub(reserve_right);
        let mut clipped = false;

        for (i, tab) in tabs.iter().enumerate() {
            let label = tab_label(tab, sessions_state);
            let label_cols = label.chars().count() as u16;
            let tab_cell_cols = label_cols + 2; // single space pad on each side
            let tab_start = col;
            let tab_end = col + tab_cell_cols;

            if tab_end > max_tab_col {
                clipped = true;
                break;
            }

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
            col = tab_end + TAB_SEP.chars().count() as u16;
        }

        // Overflow indicator at the right edge.
        if clipped {
            // Position immediately before the hint.
            let pos = cols.saturating_sub(reserve_right);
            move_to(buf, 1, pos);
            buf.extend_from_slice(HINT_FG.as_bytes());
            buf.extend_from_slice("›".as_bytes());
            buf.extend_from_slice(RESET.as_bytes());
        }

        // Right-side prefix-mode hint.
        let hint_start = cols.saturating_sub(hint_cols);
        if hint_start > 0 {
            move_to(buf, 1, hint_start);
            buf.extend_from_slice(HINT_FG.as_bytes());
            buf.extend_from_slice(hint.as_bytes());
            buf.extend_from_slice(RESET.as_bytes());
        }
    }

    fn right_hint(&self) -> String {
        match self.prefix_mode {
            PrefixMode::Idle => format!("detach: {} d", self.prefix_label),
            PrefixMode::Awaiting => "prefix…".to_string(),
        }
    }

    /// Return the tab index clicked at column `c` (1-based), if any.
    pub fn tab_at_col(&self, c: u16) -> Option<usize> {
        self.tab_regions
            .iter()
            .position(|&(start, end)| c >= start && c < end)
    }
}

fn tab_label(tab: &Tab, states: &[(u64, AgentState)]) -> String {
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

/// Vertical pane border at column `col` for rows `from_row..=to_row`.
pub fn draw_vertical_border(buf: &mut Vec<u8>, col: u16, from_row: u16, to_row: u16, active: bool) {
    let color = if active {
        "\x1b[38;5;46m"
    } else {
        "\x1b[38;5;238m"
    };
    for row in from_row..=to_row {
        move_to(buf, row + 1, col + 1);
        buf.extend_from_slice(color.as_bytes());
        buf.extend_from_slice("│".as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
    }
}

/// Horizontal pane border at row `row` for cols `from_col..=to_col`.
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
    move_to(buf, row + 1, from_col + 1);
    buf.extend_from_slice(color.as_bytes());
    for _ in from_col..=to_col {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice(RESET.as_bytes());
}

fn move_to(buf: &mut Vec<u8>, row: u16, col: u16) {
    use std::io::Write as _;
    let _ = write!(buf, "\x1b[{};{}H", row, col);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Tab;

    #[test]
    fn tab_click_region_width_includes_state_glyph() {
        // Regression for the click-region drift bug: tab regions must
        // account for the trailing `●`/`○` glyph appended to the label.
        let mut bar = StatusBar::new();
        let tab = Tab::new_single("Claude", 1);
        let tabs = vec![tab];
        let states = vec![(1u64, AgentState::Blocked)]; // appends " ●"
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &tabs, 0, &states);
        let (start, end) = bar.tab_regions[0];
        // Label is "Claude ●" = 8 chars. Tab cell pads ` Claude ● ` = 10 cols.
        assert_eq!(end - start, 10);
    }

    #[test]
    fn idle_hint_renders_detach_label() {
        let mut bar = StatusBar::new();
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("detach: Ctrl+B d"), "missing idle hint: {s:?}");
    }

    #[test]
    fn awaiting_hint_swaps_label() {
        let mut bar = StatusBar::new();
        bar.set_prefix_mode(PrefixMode::Awaiting);
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("prefix…"), "missing awaiting hint: {s:?}");
    }
}
