//! Status bar component: renders the per-pane tab strip and global capsule
//! status line at the top of the host terminal (rows 0–1).
//!
//! Not responsible for: tab lifecycle or focus state mutation — caller passes
//! snapshot state and receives a rendered byte buffer.
//!
//! Key invariant: tab cell sizing uses `jackin_tui::lay_out_tabs` so the
//! capsule and host console TUI cannot drift on cell widths or click regions.

/// Status bar state and click-region planner for rows 0–1 of the host terminal.
///
/// The actual renderer is the Ratatui `StatusBarWidget` in `chrome.rs`. This
/// type stores stable capsule state (container labels, prefix mode) and derives
/// click regions from the same `status_bar_plan` the widget uses to paint.
/// The visible shape mirrors the jackin console TUI's tab strip:
///
/// - Row 0: ` jackin❯ ` brand pill, then tab cells.
/// - Row 1: a thick `━` underline beneath the active tab cell only;
///   blank elsewhere. The underline carries the operator's focus
///   signal — the same pattern the console uses below "General /
///   Mounts / Roles / Environments / Auth."
///
/// Inactive tab cells get a subtle dark-grey background so they stand
/// out against the terminal's default-black background. The active tab
/// uses a slightly lifted graphite background instead of the brand
/// green, so it stays distinct from the ` jackin❯ ` brand pill, plus
/// the row-1 white underline.
///
/// Layout columns come from `jackin_tui::lay_out_tabs`, so the
/// console TUI and the multiplexer cannot drift on cell sizing /
/// click-region maths.
use jackin_tui::components::TabStrip;

use crate::tui::layout::Tab;
use crate::tui::model::{MuxMode, VisibleAgentState};

/// Column width in terminal cells for a label, measured with
/// `unicode-width`. Saturates to `u16::MAX` for absurdly wide labels
/// rather than wrapping. `lay_out_tabs` uses the same crate; routing
/// every per-label width through this helper keeps the renderer and
/// the click-region maths from drifting on CJK / emoji / combining
/// marks.
fn display_cols(s: &str) -> u16 {
    u16::try_from(jackin_tui::display_cols(s)).unwrap_or(u16::MAX)
}

const BRAND_TEXT: &str = " jackin❯ ";
const BRAND_PAD_COLS: u16 = 1; // single space between brand pill and first tab
const TAB_GLYPH_PLACEHOLDER: &str = " X";

/// Rows the status bar occupies. Content rect starts at row 2.
pub const STATUS_BAR_ROWS: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixMode {
    Idle,
    Awaiting,
}

pub(crate) const fn prefix_mode_for_mux_mode(mode: MuxMode) -> PrefixMode {
    if matches!(mode, MuxMode::PrefixAwait) {
        PrefixMode::Awaiting
    } else {
        PrefixMode::Idle
    }
}

#[derive(Debug)]
pub struct StatusBar {
    pub tab_regions: Vec<(u16, u16)>,
    pub hint_region: Option<(u16, u16)>,
    pub prefix_mode: PrefixMode,
    pub prefix_enabled: bool,
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
        Self::new_with_role_labels(String::new(), String::new(), String::new())
    }

    pub fn new_with_role(role: String) -> Self {
        Self::new_with_role_labels(role, String::new(), String::new())
    }

    pub fn new_with_role_labels(
        role: String,
        identity_label: String,
        instance_id_label: String,
    ) -> Self {
        Self {
            tab_regions: Vec::new(),
            hint_region: None,
            prefix_mode: PrefixMode::Idle,
            prefix_enabled: false,
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

    /// Return the tab index clicked at column `c` (1-based), if any.
    pub fn tab_at_col(&self, c: u16) -> Option<usize> {
        self.tab_regions
            .iter()
            .position(|&(start, end)| c >= start && c < end)
    }

    /// Set the tab + menu click regions from an already-computed plan.
    ///
    /// Hit-testing reads `tab_regions` / `hint_region`. The Ratatui compositor
    /// builds one [`StatusBarPlan`] per frame and shares it with both
    /// `StatusBarWidget` (paint) and this method (hit-testing), so the bar is
    /// never laid out more than once per frame and the painted cells and click
    /// regions cannot disagree.
    pub fn set_click_regions_from_plan(&mut self, plan: &StatusBarPlan) {
        self.tab_regions = plan
            .cells
            .iter()
            .map(|c| (c.start_col0 + 1, c.start_col0 + 1 + c.cell_cols))
            .collect();
        self.hint_region = plan.hint_start.map(|start| (start, start + plan.hint_cols));
    }
}

/// One laid-out tab cell, resolved name + state glyph, in 0-based columns.
#[derive(Debug)]
pub(crate) struct StatusTabCell {
    pub(crate) start_col0: u16,
    pub(crate) cell_cols: u16,
    pub(crate) active: bool,
    pub(crate) name: String,
    pub(crate) glyph: TabGlyph,
}

/// Geometry for the whole status bar (row 0): which tab cells fit, where the
/// right-side menu button sits, and whether an overflow `›` is needed. The
/// single source of truth shared by the Ratatui `StatusBarWidget` (which
/// paints it) and `StatusBar::refresh_click_regions` (which turns it into
/// click regions) so the two cannot drift on column maths.
#[derive(Debug)]
pub struct StatusBarPlan {
    pub(crate) cells: Vec<StatusTabCell>,
    pub(crate) hint_text: String,
    pub(crate) hint_cols: u16,
    /// 1-based start column of the menu button, or `None` when there is no
    /// room for it without overlapping the brand pill.
    pub(crate) hint_start: Option<u16>,
    /// 1-based column for the overflow `›` indicator, set only when at least
    /// one tab was clipped past the right edge.
    pub(crate) overflow_col: Option<u16>,
}

pub(crate) fn button_text_for(prefix_mode: PrefixMode) -> &'static str {
    match prefix_mode {
        PrefixMode::Idle => " ☰Menu ",
        PrefixMode::Awaiting => " prefix… ",
    }
}

/// Lay out row 0 of the status bar. This is the single source of truth for
/// both `StatusBarWidget` painting and `StatusBar::refresh_click_regions`, so
/// a click region computed from this plan lands on the cell the widget drew.
pub fn status_bar_plan(
    cols: u16,
    tabs: &[Tab],
    active_tab: usize,
    sessions_state: &[(u64, VisibleAgentState)],
    prefix_mode: PrefixMode,
) -> StatusBarPlan {
    let hint_text = button_text_for(prefix_mode);
    let hint_cols = display_cols(hint_text);
    let reserve_right: u16 = hint_cols + 2; // 1 col padding + 1 trailing space

    let resolved: Vec<(String, TabGlyph, bool)> = tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let (name, glyph) = tab_label(tab, sessions_state);
            (name, glyph, i == active_tab)
        })
        .collect();
    let padded: Vec<String> = resolved
        .iter()
        .map(|(name, _, _)| tab_display_label(name))
        .collect();
    let label_refs: Vec<(&str, bool)> = padded
        .iter()
        .zip(resolved.iter())
        .map(|(label, (_, _, active))| (label.as_str(), *active))
        .collect();

    let start_col_0based = display_cols(BRAND_TEXT) + BRAND_PAD_COLS;
    let laid = TabStrip::new(&label_refs).cells(start_col_0based);
    let max_tab_col = cols.saturating_sub(reserve_right);

    let mut cells = Vec::with_capacity(laid.len());
    let mut clipped = false;
    for (cell, (name, glyph, active)) in laid.iter().zip(resolved.iter()) {
        if cell.start_col + cell.cell_cols > max_tab_col {
            clipped = true;
            break;
        }
        cells.push(StatusTabCell {
            start_col0: cell.start_col,
            cell_cols: cell.cell_cols,
            active: *active,
            name: name.clone(),
            glyph: *glyph,
        });
    }

    let brand_end_1based = start_col_0based.saturating_add(1);
    let hint_candidate = cols.saturating_sub(hint_cols);
    let hint_start = (hint_candidate > brand_end_1based).then_some(hint_candidate);

    let overflow_col = if clipped {
        let pos = cols.saturating_sub(reserve_right);
        (pos > brand_end_1based).then_some(pos)
    } else {
        None
    };

    StatusBarPlan {
        cells,
        hint_text: hint_text.to_owned(),
        hint_cols,
        hint_start,
        overflow_col,
    }
}

/// State glyph the status-bar paints in the rightmost slot of a tab cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabGlyph {
    Blocked,
    Done,
    Working,
    Idle,
    Unknown,
}

impl From<VisibleAgentState> for TabGlyph {
    fn from(state: VisibleAgentState) -> Self {
        match state {
            VisibleAgentState::Blocked => Self::Blocked,
            VisibleAgentState::Done => Self::Done,
            VisibleAgentState::Working => Self::Working,
            VisibleAgentState::Idle => Self::Idle,
            VisibleAgentState::Unknown => Self::Unknown,
        }
    }
}

/// Resolve the base name + state glyph for a tab. The caller builds
/// the full display label by reserving the sep + glyph slots; the
/// glyph is painted separately so its colour can differ from the
/// surrounding tab foreground.
fn tab_label(tab: &Tab, states: &[(u64, VisibleAgentState)]) -> (String, TabGlyph) {
    let ids = tab.tree.all_ids();
    let state = ids
        .iter()
        .filter_map(|id| states.iter().find(|(sid, _)| sid == id).map(|(_, st)| *st))
        .fold(VisibleAgentState::Unknown, roll_up_tab_state);
    let glyph = TabGlyph::from(state);
    (tab.label_owned(), glyph)
}

fn roll_up_tab_state(
    current: VisibleAgentState,
    candidate: VisibleAgentState,
) -> VisibleAgentState {
    if state_priority(candidate) > state_priority(current) {
        candidate
    } else {
        current
    }
}

const fn state_priority(state: VisibleAgentState) -> u8 {
    match state {
        VisibleAgentState::Unknown => 0,
        VisibleAgentState::Idle => 1,
        VisibleAgentState::Working => 2,
        VisibleAgentState::Done => 3,
        VisibleAgentState::Blocked => 4,
    }
}

fn tab_display_label(name: &str) -> String {
    format!("{name}{TAB_GLYPH_PLACEHOLDER}")
}

#[cfg(test)]
mod tests;
