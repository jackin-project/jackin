// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Status/tab-bar widget: brand pill, tab cells, menu button, underline.

use crate::tui::components::status_bar::{PrefixMode, StatusBarPlan, StatusTabCell, TabGlyph};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};
use termrock::style::DesignSystem;

// ── Status bar (row 0 + row 1) ────────────────────────────────────────────────

/// Brand pill + tab cells (row 0) and the active-tab underline (row 1),
/// painted into the Ratatui `Buffer` so the `SocketBackend` diff tracks every
/// chrome cell. The `plan` is computed once per frame by the compositor and
/// shared with `StatusBar::set_click_regions_from_plan`, so the painted cells
/// and the click regions derive from the same layout and cannot drift.
#[derive(Debug)]
pub struct StatusBarWidget<'a> {
    pub plan: &'a StatusBarPlan,
    pub prefix_mode: PrefixMode,
    pub hovered_tab: Option<usize>,
    pub menu_hovered: bool,
    /// P5: whether the tab bar itself holds focus. The active-tab underline is
    /// the single focus indicator — bright phosphor-green when the bar is
    /// focused, neutral (white) when focus is in the agent content below.
    pub focused: bool,
}

impl StatusBarWidget<'_> {
    pub(crate) fn paint_tab(&self, cell: &StatusTabCell, idx: usize, area: Rect, buf: &mut Buffer) {
        let hovered = self.hovered_tab == Some(idx);
        let style = tab_cell_style(cell.active, hovered);
        let bg = style.bg.unwrap_or(Color::Reset);
        let glyph_char = tab_glyph_char(cell.glyph);
        // Cell layout: ` <name> <sep> <glyph> ` — matches emit_tab_row0.
        let content = format!(" {} {} ", cell.name, glyph_char);
        let x = area.x.saturating_add(cell.start_col0);
        buf.set_string(x, area.y, &content, style);
        if let Some(glyph_style) = tab_glyph_style(cell.glyph, bg) {
            let name_cols =
                u16::try_from(termrock::text::display_cols(&cell.name)).unwrap_or(u16::MAX);
            let glyph_x = x.saturating_add(name_cols).saturating_add(2);
            buf.set_string(glyph_x, area.y, glyph_char.to_string(), glyph_style);
        }
    }
}

pub(crate) fn tab_cell_style(active: bool, hovered: bool) -> Style {
    let background = match (active, hovered) {
        (true, true) => DesignSystem::default()
            .style(termrock::style::Role::TabActiveHovered)
            .bg
            .unwrap_or_default(),
        (true, false) => DesignSystem::default()
            .style(termrock::style::Role::TabActive)
            .bg
            .unwrap_or_default(),
        (false, true) => DesignSystem::default()
            .style(termrock::style::Role::TabInactiveHovered)
            .bg
            .unwrap_or_default(),
        (false, false) => DesignSystem::default()
            .style(termrock::style::Role::TabInactive)
            .bg
            .unwrap_or_default(),
    };
    let style = Style::default().bg(background).fg(DesignSystem::default()
        .style(termrock::style::Role::Text)
        .fg
        .unwrap_or_default());
    if active {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

pub(crate) const fn tab_glyph_char(glyph: TabGlyph) -> char {
    match glyph {
        TabGlyph::Blocked => '●',
        TabGlyph::Done => '○',
        TabGlyph::Working => '▶',
        TabGlyph::Idle => '◆',
        TabGlyph::Unknown => ' ',
    }
}

pub(crate) fn tab_glyph_style(glyph: TabGlyph, bg: Color) -> Option<Style> {
    match glyph {
        TabGlyph::Blocked => Some(
            Style::default()
                .bg(bg)
                .fg(jackin_tui::tokens::STATUS_BLOCKED_RED)
                .add_modifier(Modifier::BOLD),
        ),
        TabGlyph::Working => Some(
            Style::default()
                .bg(bg)
                .fg(jackin_tui::tokens::DEBUG_AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        TabGlyph::Idle => Some(
            Style::default()
                .bg(bg)
                .fg(DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default())
                .add_modifier(Modifier::BOLD),
        ),
        TabGlyph::Done | TabGlyph::Unknown => None,
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let plan = self.plan;

        let canvas_style = Style::default();
        for row in 0..area.height.min(2) {
            for col in 0..area.width {
                buf[(area.x + col, area.y + row)]
                    .set_char(' ')
                    .set_style(canvas_style);
            }
        }

        // Row 0: brand pill — green block, black word, white chevron. The
        // chevron pins the brand constant: head recolored the Role::Text it
        // used to read, and the brand look is an invariant across the bump.
        let pill = Style::default()
            .bg(jackin_tui::tokens::BRAND_BLOCK)
            .add_modifier(Modifier::BOLD);
        buf.set_string(area.x, area.y, " jackin", pill.fg(Color::Black));
        buf.set_string(
            area.x.saturating_add(7),
            area.y,
            "❯",
            pill.fg(jackin_tui::tokens::BRAND_CHEVRON),
        );
        buf.set_string(area.x.saturating_add(8), area.y, " ", pill);

        // Row 0: tab cells.
        for (idx, cell) in plan.cells.iter().enumerate() {
            self.paint_tab(cell, idx, area, buf);
        }

        // Row 0: right-side menu button.
        if let Some(start_1based) = plan.hint_start {
            let (bg, fg) = match (self.prefix_mode, self.menu_hovered) {
                (PrefixMode::Idle, false) => (
                    jackin_tui::tokens::MENU_IDLE_BG,
                    DesignSystem::default()
                        .style(termrock::style::Role::Text)
                        .fg
                        .unwrap_or_default(),
                ),
                (PrefixMode::Idle, true) => (
                    jackin_tui::tokens::MENU_IDLE_HOVER_BG,
                    DesignSystem::default()
                        .style(termrock::style::Role::Text)
                        .fg
                        .unwrap_or_default(),
                ),
                (PrefixMode::Awaiting, false) => {
                    (jackin_tui::tokens::MENU_AWAITING_BG, Color::Black)
                }
                (PrefixMode::Awaiting, true) => {
                    (jackin_tui::tokens::MENU_AWAITING_HOVER_BG, Color::Black)
                }
            };
            buf.set_string(
                area.x.saturating_add(start_1based.saturating_sub(1)),
                area.y,
                &plan.hint_text,
                Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
            );
        }

        // Row 0: overflow indicator when a tab was clipped.
        if let Some(pos_1based) = plan.overflow_col {
            buf.set_string(
                area.x.saturating_add(pos_1based.saturating_sub(1)),
                area.y,
                "›",
                Style::default().fg(DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            );
        }

        // Row 1: underline beneath the active tab cell only (blank elsewhere),
        // matching the shared capsule/console focus signal.
        if area.height > 1
            && let Some(active) = plan.cells.iter().find(|c| c.active)
        {
            let underline = "━".repeat(active.cell_cols as usize);
            let underline_fg = if self.focused {
                DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default()
            } else {
                DesignSystem::default()
                    .style(termrock::style::Role::Text)
                    .fg
                    .unwrap_or_default()
            };
            buf.set_string(
                area.x.saturating_add(active.start_col0),
                area.y + 1,
                &underline,
                Style::default()
                    .fg(underline_fg)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}
