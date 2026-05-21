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
const GLYPH_BLOCKED_FG: &str = "\x1b[38;2;255;60;60m"; // bright red — "waiting for operator"
const BOLD: &str = "\x1b[1m";

const HINT_FG: &str = "\x1b[38;2;0;140;30m"; // PHOSPHOR_DIM
// Right-hand menu button: dark phosphor-green pill with white bold
// text. Matches the brand pill's visual register so the operator
// reads it as "this is a clickable jackin' control", not "this is
// a hint string."
const BUTTON_BG_IDLE: &str = "\x1b[48;2;0;80;18m"; // PHOSPHOR_DARK
const BUTTON_FG_IDLE: &str = "\x1b[38;2;255;255;255m"; // WHITE
const BUTTON_BG_AWAITING: &str = "\x1b[48;2;0;255;65m"; // PHOSPHOR_GREEN (highlight)
const BUTTON_FG_AWAITING: &str = "\x1b[38;2;0;0;0m"; // BLACK
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

        let hint = self.button_text();
        let hint_cols = hint.chars().count() as u16;
        let reserve_right: u16 = hint_cols + 2; // 1 col padding + 1 trailing space

        // Resolve names + glyphs first, then size every cell to the
        // widest name so each tab gets identical interior layout:
        //   ` <name centered>  <glyph> `
        // The name is centred within the shared name-area; the glyph
        // is always pinned to the right column; the left/right pads
        // are always one column each. Result: tab cells are
        // rectangular and visually balanced regardless of which
        // tab the operator focuses or which state glyph it carries.
        let resolved: Vec<(String, TabGlyph, bool)> = tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let (name, glyph) = tab_label(tab, sessions_state);
                (name, glyph, i == active_tab)
            })
            .collect();
        let max_name_cols = resolved
            .iter()
            .map(|(n, _, _)| n.chars().count())
            .max()
            .unwrap_or(0);
        // Padded labels embed the centred name + sep + glyph slot;
        // `lay_out_tabs` then wraps each in its own `1+pad+1` shell.
        let padded: Vec<(String, TabGlyph, bool)> = resolved
            .into_iter()
            .map(|(name, glyph, active)| {
                let name_cols = name.chars().count();
                let pad_total = max_name_cols - name_cols;
                let pad_left = pad_total / 2;
                let pad_right = pad_total - pad_left;
                let label = format!(
                    "{}{}{}  X",
                    " ".repeat(pad_left),
                    name,
                    " ".repeat(pad_right),
                );
                let _ = label.len(); // pad_left + name + pad_right + "  X"
                (label, glyph, active)
            })
            .collect();
        let label_refs: Vec<(&str, bool)> =
            padded.iter().map(|(l, _, a)| (l.as_str(), *a)).collect();

        // First cell starts after brand pill + pad. Layout uses 0-based
        // columns; statusbar render uses 1-based, so we offset by 1
        // when emitting cursor positions.
        let start_col_0based = (BRAND_TEXT.chars().count() as u16) + BRAND_PAD_COLS;
        let cells = lay_out_tabs(&label_refs, start_col_0based);
        let max_tab_col = cols.saturating_sub(reserve_right);

        let mut clipped_at: Option<u16> = None;
        for (cell, (_, glyph, _)) in cells.iter().zip(padded.iter()) {
            let cell_end_0based = cell.start_col + cell.cell_cols;
            if cell_end_0based > max_tab_col {
                clipped_at = Some(cell.start_col);
                break;
            }
            self.emit_tab_row0(buf, cell, *glyph);
            let region_start = cell.start_col + 1;
            let region_end = region_start + cell.cell_cols;
            self.tab_regions.push((region_start, region_end));
        }

        // Right-side menu button.
        let hint_start = cols.saturating_sub(hint_cols);
        if hint_start > 0 {
            move_to(buf, 1, hint_start);
            let (bg, fg) = match self.prefix_mode {
                PrefixMode::Idle => (BUTTON_BG_IDLE, BUTTON_FG_IDLE),
                PrefixMode::Awaiting => (BUTTON_BG_AWAITING, BUTTON_FG_AWAITING),
            };
            buf.extend_from_slice(bg.as_bytes());
            buf.extend_from_slice(fg.as_bytes());
            buf.extend_from_slice(BOLD.as_bytes());
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

    fn emit_tab_row0(&self, buf: &mut Vec<u8>, cell: &TabCell<'_>, glyph: TabGlyph) {
        // Position cursor at the cell's first column (1-based).
        move_to(buf, 1, cell.start_col + 1);
        // Apply tab bg + fg first; the Blocked glyph overrides fg
        // locally and restores it before the trailing pad.
        if cell.active {
            buf.extend_from_slice(TAB_BG_ACTIVE.as_bytes());
            buf.extend_from_slice(TAB_FG_ACTIVE.as_bytes());
            buf.extend_from_slice(BOLD.as_bytes());
        } else {
            buf.extend_from_slice(TAB_BG_INACTIVE.as_bytes());
            buf.extend_from_slice(TAB_FG_INACTIVE.as_bytes());
        }
        // Cell layout: ` <centred name>  <glyph> `.
        //   - 1 col left pad
        //   - centred name (max_name_cols across the tab strip)
        //   - 2 col sep
        //   - 1 col glyph slot (Blocked: bright red ●; Done: ○;
        //     None: space — slot is always allocated so glyph
        //     position never shifts left or right between states)
        //   - 1 col right pad
        // `cell.label` was built upstream as
        //   `{centred_name}  X`
        // where the trailing `  X` reserves the sep + glyph cols. We
        // strip the placeholder `X` (the last char) and the two
        // spaces before it, then paint the actual glyph with its
        // own colour while keeping the slot at the same column.
        buf.push(b' '); // left pad
        let total_cols = cell.label.chars().count();
        let name_cols = total_cols.saturating_sub(3); // 2 sep + 1 placeholder
        let centred_name: String = cell.label.chars().take(name_cols).collect();
        buf.extend_from_slice(centred_name.as_bytes());
        buf.push(b' '); // sep
        buf.push(b' '); // sep
        match glyph {
            TabGlyph::None => buf.push(b' '),
            TabGlyph::Done => buf.extend_from_slice("○".as_bytes()),
            TabGlyph::Blocked => {
                buf.extend_from_slice(GLYPH_BLOCKED_FG.as_bytes());
                buf.extend_from_slice(BOLD.as_bytes());
                buf.extend_from_slice("●".as_bytes());
                // Restore tab fg so any trailing padding inside the
                // cell stays the right colour.
                if cell.active {
                    buf.extend_from_slice(TAB_FG_ACTIVE.as_bytes());
                } else {
                    buf.extend_from_slice(TAB_FG_INACTIVE.as_bytes());
                }
            }
        }
        buf.push(b' '); // right pad — matches the left pad for symmetry
        buf.extend_from_slice(RESET.as_bytes());
        // Inter-tab gap (`TAB_GAP`) renders against the row's
        // default background, naturally separating adjacent cells.
        for _ in 0..TAB_GAP {
            buf.push(b' ');
        }
    }

    /// Render the right-hand menu **button**. The hamburger glyph
    /// (`☰`) + label + key combo, with a dark-green pill background
    /// in idle state and an inverted bright-green highlight when the
    /// optional prefix gesture is mid-way through.
    fn button_text(&self) -> String {
        match self.prefix_mode {
            PrefixMode::Idle => {
                if self.prefix_enabled {
                    format!(
                        " ☰ Menu {} · prefix {} ",
                        self.palette_label, self.prefix_label
                    )
                } else {
                    format!(" ☰ Menu {} ", self.palette_label)
                }
            }
            PrefixMode::Awaiting => " prefix… ".to_string(),
        }
    }

    /// Return the tab index clicked at column `c` (1-based), if any.
    pub fn tab_at_col(&self, c: u16) -> Option<usize> {
        self.tab_regions
            .iter()
            .position(|&(start, end)| c >= start && c < end)
    }
}

/// State glyph the status-bar paints in the rightmost slot of a tab
/// cell. The `●` Blocked variant is rendered in red so the operator
/// can spot "agent is waiting for you" without reading labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabGlyph {
    /// `Working` / `Idle` — single space placeholder. The slot is
    /// always reserved so cell width stays stable across state
    /// transitions.
    None,
    /// `Done` — `○`, default tab foreground colour.
    Done,
    /// `Blocked` — `●`, rendered in bright red as the high-visibility
    /// "agent waiting" indicator.
    Blocked,
}

/// Resolve the base name + state glyph for a tab. The caller builds
/// the full display label by centring the name and reserving the
/// sep + glyph slots; the glyph is painted separately so its colour
/// can differ from the surrounding tab foreground.
fn tab_label(tab: &Tab, states: &[(u64, AgentState)]) -> (String, TabGlyph) {
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
        TabGlyph::Blocked
    } else if has_done {
        TabGlyph::Done
    } else {
        TabGlyph::None
    };
    (tab.label.clone(), glyph)
}

/// Active pane border uses jackin's brand highlight (phosphor-green) so
/// it matches the row-0 brand pill and the focused list-item bar in
/// the console TUI — an operator scanning the screen sees the same
/// colour cue everywhere "this is what's selected right now" applies.
/// Inactive panes keep a neutral gray so they read as chrome and do
/// not compete with the focused pane. Title text in the top border
/// stays bright white when the pane is focused so the operator can
/// locate the keystroke target at a glance.
const BORDER_ACTIVE: &str = "\x1b[38;2;0;255;65m"; // PHOSPHOR_GREEN
const BORDER_INACTIVE: &str = "\x1b[38;2;80;80;80m"; // dim gray
const TITLE_ACTIVE: &str = "\x1b[1;38;2;255;255;255m"; // bright white, bold
const TITLE_INACTIVE: &str = "\x1b[38;2;160;160;160m"; // mid gray

/// Draw a full bordered box around a pane: `┌─ title ─┐` top, `│` sides,
/// `└──┘` bottom. The pane's PTY content renders into the box
/// **interior** (`rect` shrunk by one cell on every side). Title shows
/// what's running inside the pane — the operator's `label` for the
/// session (`Claude` / `Codex` / `Amp` / `OpenCode` / `Kimi` / `Shell`).
/// Mirrors zellij's pane chrome so an operator coming from there has
/// the same visual cue stack here.
pub fn draw_pane_box(
    buf: &mut Vec<u8>,
    row: u16,
    col: u16,
    rows: u16,
    cols: u16,
    title: &str,
    active: bool,
) {
    if rows < 2 || cols < 2 {
        return;
    }
    let border = if active {
        BORDER_ACTIVE
    } else {
        BORDER_INACTIVE
    };
    let title_color = if active {
        TITLE_ACTIVE
    } else {
        TITLE_INACTIVE
    };
    let interior_cols = cols.saturating_sub(2);
    let title_cols = title.chars().count() as u16;
    // Top border: `┌─ title ─` then dashes filling to `┐`. Title is
    // omitted entirely when the pane is too narrow to fit the
    // `┌─ X ─┐` minimum (8 cols of chrome).
    move_to(buf, row + 1, col + 1);
    buf.extend_from_slice(border.as_bytes());
    buf.extend_from_slice("┌".as_bytes());
    let title_fits = title_cols + 4 <= interior_cols;
    let mut consumed: u16 = 0;
    if title_fits {
        buf.extend_from_slice("─".as_bytes());
        buf.push(b' ');
        buf.extend_from_slice(title_color.as_bytes());
        buf.extend_from_slice(title.as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
        buf.extend_from_slice(border.as_bytes());
        buf.push(b' ');
        consumed = 2 + title_cols + 1;
    }
    for _ in consumed..interior_cols {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice("┐".as_bytes());
    buf.extend_from_slice(RESET.as_bytes());

    // Side borders.
    for r in 1..(rows - 1) {
        move_to(buf, row + r + 1, col + 1);
        buf.extend_from_slice(border.as_bytes());
        buf.extend_from_slice("│".as_bytes());
        move_to(buf, row + r + 1, col + cols);
        buf.extend_from_slice(border.as_bytes());
        buf.extend_from_slice("│".as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
    }

    // Bottom border.
    move_to(buf, row + rows, col + 1);
    buf.extend_from_slice(border.as_bytes());
    buf.extend_from_slice("└".as_bytes());
    for _ in 0..interior_cols {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice("┘".as_bytes());
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
    fn tab_click_region_width_matches_layout() {
        // Tab cell layout: ` <centred-name>  <glyph> ` = 1 pad + name +
        // 2 sep + 1 glyph + 1 pad = name + 5. With name="Claude" the
        // cell is 11 cols wide; the region is stable regardless of
        // the agent state.
        let mut bar = StatusBar::new();
        let tab = Tab::new_single("Claude", 1);
        let tabs = vec![tab];
        let states = vec![(1u64, AgentState::Blocked)];
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &tabs, 0, &states);
        let (start, end) = bar.tab_regions[0];
        assert_eq!(end - start, 11);
        // Re-rendering with no state must keep the same width.
        let mut buf2 = Vec::new();
        bar.render(&mut buf2, 80, &tabs, 0, &[]);
        let (s2, e2) = bar.tab_regions[0];
        assert_eq!(e2 - s2, 11);
        assert_eq!((s2, e2), (start, end));
    }

    #[test]
    fn idle_hint_renders_palette_label() {
        let mut bar = StatusBar::new();
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("Menu Ctrl+\\"), "missing idle hint: {s:?}");
    }

    #[test]
    fn idle_hint_includes_prefix_when_enabled() {
        let mut bar = StatusBar::new();
        bar.set_prefix_enabled(true);
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[]);
        let s = String::from_utf8_lossy(&buf);
        assert!(
            s.contains("Menu Ctrl+\\") && s.contains("prefix Ctrl+B"),
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
