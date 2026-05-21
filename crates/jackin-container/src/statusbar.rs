/// Status bar rendered at rows 0–1 of the host terminal.
///
/// Mirrors the jackin console TUI's tab strip (`render_tab_strip` in
/// `src/console/manager/render/editor.rs`):
///
/// - Row 0: ` jackin' ` brand pill, then tab cells, then a
///   right-aligned hint.
/// - Row 1: a thick `━` underline beneath the active tab cell only;
///   blank elsewhere. The underline carries the operator's focus
///   signal — the same pattern the console uses below "General /
///   Mounts / Roles / Environments / Auth."
///
/// Inactive tab cells get a `PHOSPHOR_DARK` background so they stand
/// out against the terminal's default-black background. Active tab
/// has the same dark-green background with a white bold label, plus
/// the row-1 underline.
///
/// Layout columns come from `jackin_tui::lay_out_tabs`, so the
/// console TUI and the multiplexer cannot drift on cell sizing /
/// click-region maths.
use std::io::Write as _;

use jackin_tui::{TAB_GAP, TabCell, lay_out_tabs};

use crate::layout::Tab;
use crate::protocol::AgentState;

const BRAND_BG: &str = "\x1b[48;2;0;255;65m"; // PHOSPHOR_GREEN bg
const BRAND_FG: &str = "\x1b[38;2;0;0;0m"; // black
const BRAND_BOLD: &str = "\x1b[1m";

const TAB_BG_INACTIVE: &str = "\x1b[48;2;30;30;30m"; // subtle dark grey
const TAB_BG_ACTIVE: &str = "\x1b[48;2;0;255;65m"; // PHOSPHOR_GREEN (brand)
const TAB_FG_INACTIVE: &str = "\x1b[38;2;255;255;255m"; // WHITE
const TAB_FG_ACTIVE: &str = "\x1b[38;2;0;0;0m"; // BLACK on bright green
const TAB_UNDERLINE_FG: &str = "\x1b[38;2;255;255;255m"; // WHITE
const BOLD: &str = "\x1b[1m";

const HINT_FG: &str = "\x1b[38;2;0;140;30m"; // PHOSPHOR_DIM
const RESET: &str = "\x1b[0m";

const BRAND_TEXT: &str = " jackin' ";
const BRAND_PAD_COLS: u16 = 1; // single space between brand pill and first tab

/// Rows the status bar occupies. Content rect starts at row 2.
pub const STATUS_BAR_ROWS: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixMode {
    Idle,
    Awaiting,
}

pub struct StatusBar {
    pub tab_regions: Vec<(u16, u16)>,
    /// Click region (1-based, inclusive-exclusive) covering the
    /// right-side `menu: …` hint. A mouse press in this region acts
    /// as a clickable shortcut for the palette key — useful when the
    /// keyboard shortcut isn't reaching the parser for any reason.
    pub hint_region: Option<(u16, u16)>,
    pub prefix_mode: PrefixMode,
    pub prefix_label: String,
    pub palette_label: String,
    pub prefix_enabled: bool,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            tab_regions: Vec::new(),
            hint_region: None,
            prefix_mode: PrefixMode::Idle,
            prefix_label: "Ctrl+B".to_string(),
            palette_label: "Ctrl+\\".to_string(),
            prefix_enabled: false,
        }
    }

    /// Return `true` when the (1-based) click at `(row, col)` falls
    /// inside the menu hint region. The daemon treats that as an
    /// alternate-path "open palette" gesture so the operator never
    /// loses access to the menu when the keyboard shortcut isn't
    /// reaching the parser.
    pub fn hint_at(&self, row: u16, col: u16) -> bool {
        if row != 1 {
            return false;
        }
        match self.hint_region {
            Some((start, end)) => col >= start && col < end,
            None => false,
        }
    }

    pub fn set_prefix_mode(&mut self, mode: PrefixMode) {
        self.prefix_mode = mode;
    }

    pub fn set_prefix_enabled(&mut self, enabled: bool) {
        self.prefix_enabled = enabled;
    }

    /// Render the status bar at rows 0–1 of the host terminal.
    pub fn render(
        &mut self,
        buf: &mut Vec<u8>,
        cols: u16,
        tabs: &[Tab],
        active_tab: usize,
        sessions_state: &[(u64, AgentState)],
    ) {
        self.tab_regions.clear();
        self.hint_region = None;

        // ── Row 0: brand pill + tabs + hint ─────────────────────────
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

        // Build labels including the state glyph, then lay them out.
        let labels: Vec<(String, bool)> = tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| (tab_label(tab, sessions_state), i == active_tab))
            .collect();
        let label_refs: Vec<(&str, bool)> = labels.iter().map(|(l, a)| (l.as_str(), *a)).collect();

        // First cell starts after brand pill + pad. Layout uses 0-based
        // columns; statusbar render uses 1-based, so we offset by 1
        // when emitting cursor positions.
        let start_col_0based = (BRAND_TEXT.chars().count() as u16) + BRAND_PAD_COLS;
        let cells = lay_out_tabs(&label_refs, start_col_0based);
        let max_tab_col = cols.saturating_sub(reserve_right);

        let mut clipped_at: Option<u16> = None;
        for cell in &cells {
            let cell_end_0based = cell.start_col + cell.cell_cols;
            if cell_end_0based > max_tab_col {
                clipped_at = Some(cell.start_col);
                break;
            }
            self.emit_tab_row0(buf, cell);
            // Click region: 1-based, inclusive-exclusive.
            let region_start = cell.start_col + 1;
            let region_end = region_start + cell.cell_cols;
            self.tab_regions.push((region_start, region_end));
        }

        // Right-side hint.
        let hint_start = cols.saturating_sub(hint_cols);
        if hint_start > 0 {
            move_to(buf, 1, hint_start);
            buf.extend_from_slice(HINT_FG.as_bytes());
            buf.extend_from_slice(hint.as_bytes());
            buf.extend_from_slice(RESET.as_bytes());
            // 1-based, inclusive-exclusive — matches `tab_regions`.
            self.hint_region = Some((hint_start, hint_start + hint_cols));
        }

        // Overflow indicator before the hint.
        if let Some(start) = clipped_at {
            let _ = start;
            let pos = cols.saturating_sub(reserve_right);
            move_to(buf, 1, pos);
            buf.extend_from_slice(HINT_FG.as_bytes());
            buf.extend_from_slice("›".as_bytes());
            buf.extend_from_slice(RESET.as_bytes());
        }

        // ── Row 1: active-tab underline ─────────────────────────────
        buf.extend_from_slice(b"\x1b[2;1H\x1b[2K");
        for cell in &cells {
            let cell_end_0based = cell.start_col + cell.cell_cols;
            if cell_end_0based > max_tab_col {
                break;
            }
            if cell.active {
                move_to(buf, 2, cell.start_col + 1);
                buf.extend_from_slice(TAB_UNDERLINE_FG.as_bytes());
                buf.extend_from_slice(BOLD.as_bytes());
                for _ in 0..cell.cell_cols {
                    buf.extend_from_slice("━".as_bytes());
                }
                buf.extend_from_slice(RESET.as_bytes());
                break;
            }
        }
    }

    fn emit_tab_row0(&self, buf: &mut Vec<u8>, cell: &TabCell<'_>) {
        // Position cursor at the cell's first column (1-based).
        move_to(buf, 1, cell.start_col + 1);
        if cell.active {
            buf.extend_from_slice(TAB_BG_ACTIVE.as_bytes());
            buf.extend_from_slice(TAB_FG_ACTIVE.as_bytes());
            buf.extend_from_slice(BOLD.as_bytes());
        } else {
            buf.extend_from_slice(TAB_BG_INACTIVE.as_bytes());
            buf.extend_from_slice(TAB_FG_INACTIVE.as_bytes());
        }
        buf.push(b' ');
        buf.extend_from_slice(cell.label.as_bytes());
        buf.push(b' ');
        buf.extend_from_slice(RESET.as_bytes());
        // TAB_GAP cells between tabs render with no background, so the
        // separation is naturally visible against the surrounding row.
        for _ in 0..TAB_GAP {
            buf.push(b' ');
        }
    }

    fn right_hint(&self) -> String {
        match self.prefix_mode {
            PrefixMode::Idle => {
                if self.prefix_enabled {
                    format!(
                        "menu: {}  prefix: {}",
                        self.palette_label, self.prefix_label
                    )
                } else {
                    format!("menu: {}", self.palette_label)
                }
            }
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

/// Always render the state-glyph slot — `●`, `○`, or a space when
/// the tab is in `Working`/`Idle`. Reserving the slot keeps every tab
/// cell at a stable width across state transitions, so the tab strip
/// doesn't reflow every time an agent finishes responding.
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

    let glyph = if has_blocked {
        '●'
    } else if has_done {
        '○'
    } else {
        ' '
    };
    format!("{} {}", tab.label, glyph)
}

/// Vertical pane border at column `col` for rows `from_row..=to_row`.
pub fn draw_vertical_border(buf: &mut Vec<u8>, col: u16, from_row: u16, to_row: u16, active: bool) {
    let color = if active {
        "\x1b[38;2;0;255;65m"
    } else {
        "\x1b[38;2;0;80;18m"
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
        "\x1b[38;2;0;255;65m"
    } else {
        "\x1b[38;2;0;80;18m"
    };
    move_to(buf, row + 1, from_col + 1);
    buf.extend_from_slice(color.as_bytes());
    for _ in from_col..=to_col {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice(RESET.as_bytes());
}

fn move_to(buf: &mut Vec<u8>, row: u16, col: u16) {
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
    fn idle_hint_renders_palette_label() {
        let mut bar = StatusBar::new();
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("menu: Ctrl+\\"), "missing idle hint: {s:?}");
    }

    #[test]
    fn idle_hint_includes_prefix_when_enabled() {
        let mut bar = StatusBar::new();
        bar.set_prefix_enabled(true);
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[]);
        let s = String::from_utf8_lossy(&buf);
        assert!(
            s.contains("menu: Ctrl+\\") && s.contains("prefix: Ctrl+B"),
            "missing combined hint: {s:?}"
        );
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

    #[test]
    fn active_tab_emits_row1_underline() {
        let mut bar = StatusBar::new();
        let tabs = vec![Tab::new_single("Claude", 1)];
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &tabs, 0, &[]);
        let s = String::from_utf8_lossy(&buf);
        // Row 1 = ANSI row 2 (1-based). Underline uses `━`.
        assert!(s.contains("\x1b[2;"), "row 2 cursor move missing: {s:?}");
        assert!(s.contains("━"), "underline glyph missing: {s:?}");
    }
}
