/// Status bar rendered at rows 0–1 of the host terminal.
///
/// Mirrors the jackin console TUI's tab strip (`render_tab_strip` in
/// `src/console/manager/render/editor.rs`):
///
/// - Row 0: ` jackin' ` brand pill, then tab cells.
/// - Row 1: a thick `━` underline beneath the active tab cell only;
///   blank elsewhere. The underline carries the operator's focus
///   signal — the same pattern the console uses below "General /
///   Mounts / Roles / Environments / Auth."
///
/// Inactive tab cells get a subtle dark-grey background so they stand
/// out against the terminal's default-black background. The active tab
/// uses a slightly lifted graphite background instead of the brand
/// green, so it stays distinct from the ` jackin' ` brand pill, plus
/// the row-1 white underline.
///
/// Layout columns come from `jackin_tui::lay_out_tabs`, so the
/// console TUI and the multiplexer cannot drift on cell sizing /
/// click-region maths.
use std::io::Write as _;

use jackin_tui::{TAB_GAP, TabCell, lay_out_tabs};
use unicode_width::UnicodeWidthStr;

use crate::layout::Tab;

/// Column width in terminal cells for a label, measured with
/// `unicode-width`. Saturates to `u16::MAX` for absurdly wide labels
/// rather than wrapping. `lay_out_tabs` uses the same crate; routing
/// every per-label width through this helper keeps the renderer and
/// the click-region maths from drifting on CJK / emoji / combining
/// marks.
fn display_cols(s: &str) -> u16 {
    u16::try_from(UnicodeWidthStr::width(s)).unwrap_or(u16::MAX)
}
use crate::protocol::AgentState;

const JACKIN_CONTAINER_NAME_ENV: &str = "JACKIN_CONTAINER_NAME";
const JACKIN_INSTANCE_ID_ENV: &str = "JACKIN_INSTANCE_ID";

const BRAND_BG: &str = "\x1b[48;2;0;255;65m"; // PHOSPHOR_GREEN bg
const BRAND_FG: &str = "\x1b[38;2;0;0;0m"; // black
const BRAND_BOLD: &str = "\x1b[1m";
const BRAND_BG_DIM: &str = "\x1b[48;2;0;51;13m";
const BRAND_FG_DIM: &str = "\x1b[38;2;0;0;0m";

const TAB_BG_INACTIVE: &str = "\x1b[48;2;30;30;30m"; // subtle dark grey
const TAB_BG_INACTIVE_HOVER: &str = "\x1b[48;2;48;48;48m"; // hover lift for clickable tabs
const TAB_BG_ACTIVE: &str = "\x1b[48;2;42;42;42m"; // graphite, distinct from brand
const TAB_BG_ACTIVE_HOVER: &str = "\x1b[48;2;58;58;58m"; // active tab hover lift
const TAB_FG_INACTIVE: &str = "\x1b[38;2;255;255;255m"; // WHITE
const TAB_FG_ACTIVE: &str = "\x1b[38;2;255;255;255m"; // WHITE on graphite
const TAB_UNDERLINE_FG: &str = "\x1b[38;2;255;255;255m"; // WHITE
const GLYPH_BLOCKED_FG: &str = "\x1b[38;2;255;60;60m"; // bright red — "waiting for operator"
const TAB_BG_DIM: &str = "\x1b[48;2;8;8;8m";
const TAB_FG_DIM: &str = "\x1b[38;2;51;51;51m";
const TAB_UNDERLINE_FG_DIM: &str = "\x1b[38;2;51;51;51m";
const GLYPH_BLOCKED_FG_DIM: &str = "\x1b[38;2;51;12;12m";
const BOLD: &str = "\x1b[1m";

const HINT_FG: &str = "\x1b[38;2;0;140;30m"; // PHOSPHOR_DIM
const BUTTON_BG_IDLE: &str = "\x1b[48;2;18;70;130m"; // restrained blue
const BUTTON_BG_IDLE_HOVER: &str = "\x1b[48;2;32;92;158m"; // hover lift
const BUTTON_FG_IDLE: &str = "\x1b[38;2;255;255;255m"; // WHITE
const BUTTON_BG_AWAITING: &str = "\x1b[48;2;96;180;255m"; // active blue
const BUTTON_BG_AWAITING_HOVER: &str = "\x1b[48;2;132;202;255m"; // active hover lift
const BUTTON_FG_AWAITING: &str = "\x1b[38;2;0;0;0m"; // BLACK
const HINT_FG_DIM: &str = "\x1b[38;2;0;28;6m";
const BUTTON_BG_DIM: &str = "\x1b[48;2;4;14;26m";
const BUTTON_FG_DIM: &str = "\x1b[38;2;51;51;51m";
const RESET: &str = "\x1b[0m";

const BRAND_TEXT: &str = " jackin' ";
const BRAND_PAD_COLS: u16 = 1; // single space between brand pill and first tab
const TAB_GLYPH_PLACEHOLDER: &str = " X";

/// Rows the status bar occupies. Content rect starts at row 2.
pub const STATUS_BAR_ROWS: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixMode {
    Idle,
    Awaiting,
}

pub struct StatusBar {
    pub tab_regions: Vec<(u16, u16)>,
    pub hint_region: Option<(u16, u16)>,
    pub prefix_mode: PrefixMode,
    pub prefix_enabled: bool,
    pub prefix_label: String,
    pub palette_label: String,
    /// Full role-container name (`jk-<short>-<workspace>-<role>`).
    /// Consumed by the `ContainerInfo` modal and copy action.
    pub identity_label: String,
    /// Short instance id rendered in the bottom context row.
    pub instance_id_label: String,
    /// The role key from Capsule launch config. Stored separately so
    /// the `ContainerInfo` modal can name it explicitly without
    /// re-deriving it from the container-name suffix (which is the
    /// lossy short form `thearchitect`, not the canonical
    /// `the-architect` selector the operator typed).
    pub role: String,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self::new_with_role(String::new())
    }

    pub fn new_with_role(role: String) -> Self {
        let identity_label = resolve_container_name();
        let instance_id_label = resolve_instance_id(&identity_label);
        Self::new_with_role_container_and_instance(role, identity_label, instance_id_label)
    }

    pub fn new_with_role_and_container(role: String, identity_label: String) -> Self {
        let instance_id_label = instance_id_from_container_name(&identity_label)
            .unwrap_or_else(|| identity_label.clone());
        Self::new_with_role_container_and_instance(role, identity_label, instance_id_label)
    }

    fn new_with_role_container_and_instance(
        role: String,
        identity_label: String,
        instance_id_label: String,
    ) -> Self {
        Self {
            tab_regions: Vec::new(),
            hint_region: None,
            prefix_mode: PrefixMode::Idle,
            prefix_enabled: false,
            prefix_label: "Ctrl+B".to_string(),
            palette_label: "Ctrl+\\".to_string(),
            identity_label,
            instance_id_label,
            role,
        }
    }

    pub fn container_name(&self) -> &str {
        &self.identity_label
    }

    pub fn instance_id_label(&self) -> &str {
        &self.instance_id_label
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn set_prefix_mode(&mut self, mode: PrefixMode) {
        self.prefix_mode = mode;
    }

    pub fn set_prefix_enabled(&mut self, enabled: bool) {
        self.prefix_enabled = enabled;
    }

    /// Return `true` when the (1-based) click at `(row, col)` falls
    /// inside the right-side menu button.
    pub fn hint_at(&self, row: u16, col: u16) -> bool {
        if row != 1 {
            return false;
        }
        match self.hint_region {
            Some((start, end)) => col >= start && col < end,
            None => false,
        }
    }

    /// Render the status bar at rows 0–1 of the host terminal.
    pub fn render(
        &mut self,
        buf: &mut Vec<u8>,
        cols: u16,
        tabs: &[Tab],
        active_tab: usize,
        sessions_state: &[(u64, AgentState)],
        hovered_tab: Option<usize>,
        menu_hovered: bool,
        dim: bool,
    ) {
        self.tab_regions.clear();
        self.hint_region = None;

        // ── Row 0: brand pill + tabs ────────────────────────────────
        buf.extend_from_slice(b"\x1b[1;1H\x1b[2K");

        // Brand pill.
        buf.extend_from_slice(if dim { BRAND_BG_DIM } else { BRAND_BG }.as_bytes());
        buf.extend_from_slice(if dim { BRAND_FG_DIM } else { BRAND_FG }.as_bytes());
        if !dim {
            buf.extend_from_slice(BRAND_BOLD.as_bytes());
        }
        buf.extend_from_slice(BRAND_TEXT.as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
        for _ in 0..BRAND_PAD_COLS {
            buf.push(b' ');
        }

        let hint = self.button_text();
        let hint_cols = display_cols(&hint);
        let reserve_right: u16 = hint_cols + 2; // 1 col padding + 1 trailing space

        // Resolve names + glyphs first, then reserve a stable glyph
        // slot per tab. The text starts after the same short one-cell
        // pad in every tab; the cell width follows the label length
        // instead of centring shorter labels inside the widest name.
        let resolved: Vec<(String, TabGlyph, bool)> = tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let (name, glyph) = tab_label(tab, sessions_state);
                (name, glyph, i == active_tab)
            })
            .collect();
        let padded: Vec<(String, TabGlyph, bool)> = resolved
            .into_iter()
            .map(|(name, glyph, active)| (tab_display_label(&name), glyph, active))
            .collect();
        let label_refs: Vec<(&str, bool)> =
            padded.iter().map(|(l, _, a)| (l.as_str(), *a)).collect();

        // First cell starts after brand pill + pad. Layout uses 0-based
        // columns; statusbar render uses 1-based, so we offset by 1
        // when emitting cursor positions.
        let start_col_0based = display_cols(BRAND_TEXT) + BRAND_PAD_COLS;
        let cells = lay_out_tabs(&label_refs, start_col_0based);
        let max_tab_col = cols.saturating_sub(reserve_right);

        let mut clipped_at: Option<u16> = None;
        for (idx, (cell, (_, glyph, _))) in cells.iter().zip(padded.iter()).enumerate() {
            let cell_end_0based = cell.start_col + cell.cell_cols;
            if cell_end_0based > max_tab_col {
                clipped_at = Some(cell.start_col);
                break;
            }
            self.emit_tab_row0(buf, cell, *glyph, hovered_tab == Some(idx), dim);
            let region_start = cell.start_col + 1;
            let region_end = region_start + cell.cell_cols;
            self.tab_regions.push((region_start, region_end));
        }

        let brand_end_1based = start_col_0based.saturating_add(1);

        // Right-side menu button. Keep it on row 0 so the operator
        // always has a visible pointer/click target for the palette.
        let hint_start = cols.saturating_sub(hint_cols);
        if hint_start > brand_end_1based {
            move_to(buf, 1, hint_start);
            let (bg, fg) = match (dim, self.prefix_mode, menu_hovered) {
                (true, _, _) => (BUTTON_BG_DIM, BUTTON_FG_DIM),
                (false, PrefixMode::Idle, false) => (BUTTON_BG_IDLE, BUTTON_FG_IDLE),
                (false, PrefixMode::Idle, true) => (BUTTON_BG_IDLE_HOVER, BUTTON_FG_IDLE),
                (false, PrefixMode::Awaiting, false) => (BUTTON_BG_AWAITING, BUTTON_FG_AWAITING),
                (false, PrefixMode::Awaiting, true) => {
                    (BUTTON_BG_AWAITING_HOVER, BUTTON_FG_AWAITING)
                }
            };
            buf.extend_from_slice(bg.as_bytes());
            buf.extend_from_slice(fg.as_bytes());
            if !dim {
                buf.extend_from_slice(BOLD.as_bytes());
            }
            buf.extend_from_slice(hint.as_bytes());
            buf.extend_from_slice(RESET.as_bytes());
            self.hint_region = Some((hint_start, hint_start + hint_cols));
        }

        // Overflow indicator before the hint when at least one tab got
        // clipped past the right edge. Same brand-overlap guard as the
        // hint — a `›` painted on top of " jackin' " is worse than no
        // overflow signal.
        if clipped_at.is_some() {
            let pos = cols.saturating_sub(reserve_right);
            if pos > brand_end_1based {
                move_to(buf, 1, pos);
                buf.extend_from_slice(if dim { HINT_FG_DIM } else { HINT_FG }.as_bytes());
                buf.extend_from_slice("›".as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
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
                buf.extend_from_slice(
                    if dim {
                        TAB_UNDERLINE_FG_DIM
                    } else {
                        TAB_UNDERLINE_FG
                    }
                    .as_bytes(),
                );
                if !dim {
                    buf.extend_from_slice(BOLD.as_bytes());
                }
                for _ in 0..cell.cell_cols {
                    buf.extend_from_slice("━".as_bytes());
                }
                buf.extend_from_slice(RESET.as_bytes());
                break;
            }
        }
    }

    fn button_text(&self) -> String {
        match self.prefix_mode {
            PrefixMode::Idle => " ☰Menu ".to_string(),
            PrefixMode::Awaiting => " prefix… ".to_string(),
        }
    }

    fn emit_tab_row0(
        &self,
        buf: &mut Vec<u8>,
        cell: &TabCell<'_>,
        glyph: TabGlyph,
        hovered: bool,
        dim: bool,
    ) {
        // Position cursor at the cell's first column (1-based).
        move_to(buf, 1, cell.start_col + 1);
        // Apply tab bg + fg first; the Blocked glyph overrides fg
        // locally and restores it before the trailing pad.
        if dim {
            buf.extend_from_slice(TAB_BG_DIM.as_bytes());
            buf.extend_from_slice(TAB_FG_DIM.as_bytes());
        } else if cell.active {
            let bg = if hovered {
                TAB_BG_ACTIVE_HOVER
            } else {
                TAB_BG_ACTIVE
            };
            buf.extend_from_slice(bg.as_bytes());
            buf.extend_from_slice(TAB_FG_ACTIVE.as_bytes());
            buf.extend_from_slice(BOLD.as_bytes());
        } else {
            let bg = if hovered {
                TAB_BG_INACTIVE_HOVER
            } else {
                TAB_BG_INACTIVE
            };
            buf.extend_from_slice(bg.as_bytes());
            buf.extend_from_slice(TAB_FG_INACTIVE.as_bytes());
        }
        // Cell layout: ` <name> <glyph> `.
        //   - 1 col left pad
        //   - tab name
        //   - 1 col sep
        //   - 1 col glyph slot (Blocked: bright red ●; Done: ○;
        //     None: space — slot is always allocated so glyph
        //     position never shifts left or right between states)
        //   - 1 col right pad
        // `cell.label` was built upstream as `{name} X`, where the
        // trailing `X` reserves the glyph column. We strip that
        // placeholder, then paint the actual glyph with its own
        // colour while keeping the slot at the same column.
        buf.push(b' '); // left pad
        let name = cell
            .label
            .strip_suffix(TAB_GLYPH_PLACEHOLDER)
            .unwrap_or(cell.label);
        buf.extend_from_slice(name.as_bytes());
        buf.push(b' '); // sep
        match glyph {
            TabGlyph::None => buf.push(b' '),
            TabGlyph::Done => buf.extend_from_slice("○".as_bytes()),
            TabGlyph::Blocked => {
                buf.extend_from_slice(
                    if dim {
                        GLYPH_BLOCKED_FG_DIM
                    } else {
                        GLYPH_BLOCKED_FG
                    }
                    .as_bytes(),
                );
                if !dim {
                    buf.extend_from_slice(BOLD.as_bytes());
                }
                buf.extend_from_slice("●".as_bytes());
                // Restore tab fg so any trailing padding inside the
                // cell stays the right colour.
                if dim {
                    buf.extend_from_slice(TAB_FG_DIM.as_bytes());
                } else if cell.active {
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
/// the full display label by reserving the sep + glyph slots; the
/// glyph is painted separately so its colour can differ from the
/// surrounding tab foreground.
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
    (tab.label_owned(), glyph)
}

fn tab_display_label(name: &str) -> String {
    format!("{name}{TAB_GLYPH_PLACEHOLDER}")
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
const BORDER_DIM: &str = "\x1b[38;2;48;48;48m";
const TITLE_DIM: &str = "\x1b[38;2;58;58;58m";

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
    dim: bool,
) {
    if rows < 2 || cols < 2 {
        return;
    }
    let border = if dim {
        BORDER_DIM
    } else if active {
        BORDER_ACTIVE
    } else {
        BORDER_INACTIVE
    };
    let title_color = if dim {
        TITLE_DIM
    } else if active {
        TITLE_ACTIVE
    } else {
        TITLE_INACTIVE
    };
    let interior_cols = cols.saturating_sub(2);
    let title_cols = display_cols(title);
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

/// Container name used by the bottom context row. The role is shown
/// in the `ContainerInfo` dialog opened from that row, not in the top
/// chrome.
fn resolve_container_name() -> String {
    std::env::var(JACKIN_CONTAINER_NAME_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|value| value.trim().to_string())
        })
        .unwrap_or_default()
}

fn resolve_instance_id(container_name: &str) -> String {
    std::env::var(JACKIN_INSTANCE_ID_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            instance_id_from_container_name(container_name)
                .unwrap_or_else(|| container_name.to_string())
        })
}

fn instance_id_from_container_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix("jk-")?;
    let id = rest.split('-').next()?;
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Tab;

    #[test]
    fn tab_click_region_width_matches_layout() {
        // Tab cell layout: ` <name> <glyph> ` = 1 pad + name +
        // 1 sep + 1 glyph + 1 pad = name + 4. With name="Claude" the
        // cell is 10 cols wide; the region is stable regardless of
        // the agent state.
        let mut bar = StatusBar::new();
        let tab = Tab::new_single("Claude", 1);
        let tabs = vec![tab];
        let states = vec![(1u64, AgentState::Blocked)];
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &tabs, 0, &states, None, false, false);
        let (start, end) = bar.tab_regions[0];
        assert_eq!(end - start, 10);
        // Re-rendering with no state must keep the same width.
        let mut buf2 = Vec::new();
        bar.render(&mut buf2, 80, &tabs, 0, &[], None, false, false);
        let (s2, e2) = bar.tab_regions[0];
        assert_eq!(e2 - s2, 10);
        assert_eq!((s2, e2), (start, end));
    }

    #[test]
    fn tab_display_label_has_no_name_centering_padding() {
        assert_eq!(tab_display_label("Kimi"), "Kimi X");
        assert_eq!(tab_display_label("OpenCode"), "OpenCode X");
        assert!(!tab_display_label("Kimi").starts_with(' '));
    }

    #[test]
    fn status_bar_keeps_full_container_name_and_short_instance_id() {
        let bar = StatusBar::new_with_role_and_container(
            "the-architect".to_string(),
            "jk-spamcw91-jackin-thearchitect".to_string(),
        );

        assert_eq!(bar.container_name(), "jk-spamcw91-jackin-thearchitect");
        assert_eq!(bar.instance_id_label(), "spamcw91");
    }

    #[test]
    fn idle_hint_is_rendered() {
        let mut bar = StatusBar::new();
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[], None, false, false);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("☰Menu"), "menu hint missing: {s:?}");
        assert!(
            !s.contains("☰ Menu"),
            "menu hint should not pad between icon and label: {s:?}"
        );
        assert!(
            !s.contains("Ctrl+\\"),
            "menu hint should omit shortcut: {s:?}"
        );
        assert!(
            s.contains(BUTTON_BG_IDLE),
            "menu hint should use blue button chrome: {s:?}"
        );
        assert!(bar.hint_at(1, 75), "menu hint should be clickable");
    }

    #[test]
    fn idle_hint_hover_uses_lifted_button_chrome() {
        let mut bar = StatusBar::new();
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[], None, true, false);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains(" ☰Menu "), "menu hint should be padded: {s:?}");
        assert!(
            s.contains(BUTTON_BG_IDLE_HOVER),
            "hovered menu hint should use lifted blue chrome: {s:?}"
        );
    }

    #[test]
    fn awaiting_prefix_hint_is_rendered() {
        let mut bar = StatusBar::new();
        bar.set_prefix_mode(PrefixMode::Awaiting);
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &[], 0, &[], None, false, false);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("prefix…"), "prefix hint missing: {s:?}");
        assert!(
            s.contains(BUTTON_BG_AWAITING),
            "awaiting prefix hint should use active blue chrome: {s:?}"
        );
    }

    #[test]
    fn active_tab_emits_row1_underline() {
        let mut bar = StatusBar::new();
        let tabs = vec![Tab::new_single("Claude", 1)];
        let mut buf = Vec::new();
        bar.render(&mut buf, 80, &tabs, 0, &[], None, false, false);
        let s = String::from_utf8_lossy(&buf);
        // Row 1 = ANSI row 2 (1-based). Underline uses `━`.
        assert!(s.contains("\x1b[2;"), "row 2 cursor move missing: {s:?}");
        assert!(s.contains("━"), "underline glyph missing: {s:?}");
    }
}
