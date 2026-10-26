// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::layout::Tab;
use crate::tui::model::{MuxMode, VisibleAgentState};

fn display_cols(s: &str) -> u16 {
    u16::try_from(termrock::text::display_cols(s)).unwrap_or(u16::MAX)
}

const BRAND_TEXT: &str = " jackin❯ ";
const BRAND_PAD_COLS: u16 = 1;
const TAB_GLYPH_PLACEHOLDER: &str = " X";

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
    pub identity_label: String,
    pub instance_id_label: String,
    pub role: String,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_role_labels(String::new(), String::new(), String::new())
    }

    #[must_use]
    pub fn new_with_role(role: String) -> Self {
        Self::new_with_role_labels(role, String::new(), String::new())
    }

    #[must_use]
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

    #[must_use]
    pub fn container_name(&self) -> &str {
        &self.identity_label
    }

    #[must_use]
    pub fn instance_id_label(&self) -> &str {
        &self.instance_id_label
    }

    #[must_use]
    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn set_prefix_mode(&mut self, mode: PrefixMode) {
        self.prefix_mode = mode;
    }

    pub fn set_prefix_enabled(&mut self, enabled: bool) {
        self.prefix_enabled = enabled;
    }

    #[must_use]
    pub fn hint_at(&self, row: u16, col: u16) -> bool {
        if row != 1 {
            return false;
        }
        match self.hint_region {
            Some((start, end)) => col >= start && col < end,
            None => false,
        }
    }

    #[must_use]
    pub fn tab_at_col(&self, c: u16) -> Option<usize> {
        self.tab_regions
            .iter()
            .position(|&(start, end)| c >= start && c < end)
    }

    pub fn set_click_regions_from_plan(&mut self, plan: &StatusBarPlan) {
        self.tab_regions = plan
            .cells
            .iter()
            .map(|c| (c.start_col0 + 1, c.start_col0 + 1 + c.cell_cols))
            .collect();
        self.hint_region = plan.hint_start.map(|start| (start, start + plan.hint_cols));
    }
}

#[derive(Debug)]
pub(crate) struct StatusTabCell {
    pub(crate) start_col0: u16,
    pub(crate) cell_cols: u16,
    pub(crate) active: bool,
    pub(crate) name: String,
    pub(crate) glyph: TabGlyph,
}

#[derive(Debug)]
pub struct StatusBarPlan {
    pub(crate) cells: Vec<StatusTabCell>,
    pub(crate) hint_text: String,
    pub(crate) hint_cols: u16,
    pub(crate) hint_start: Option<u16>,
    pub(crate) overflow_col: Option<u16>,
}

pub(crate) fn button_text_for(prefix_mode: PrefixMode) -> &'static str {
    match prefix_mode {
        PrefixMode::Idle => " ☰Menu ",
        PrefixMode::Awaiting => " prefix… ",
    }
}

#[must_use]
pub fn status_bar_plan(
    cols: u16,
    tabs: &[Tab],
    active_tab: usize,
    sessions_state: &[(u64, VisibleAgentState)],
    prefix_mode: PrefixMode,
) -> StatusBarPlan {
    let hint_text = button_text_for(prefix_mode);
    let hint_cols = display_cols(hint_text);
    let reserve_right: u16 = hint_cols + 2;

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
    let laid = termrock::widgets::lay_out_tabs(&label_refs, start_col_0based);
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
