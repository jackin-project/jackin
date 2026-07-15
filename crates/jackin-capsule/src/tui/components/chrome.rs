// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Ratatui widgets for capsule chrome: status bar, pane borders, branch bar.
//!
//! These widgets replace the raw-ANSI rendering in `compose_full_frame` and
//! `compose_partial_frame`. Together with `PaneBodyWidget` they make the
//! capsule's full rendering path go through the Ratatui `Buffer` → `SocketBackend`
//! pipeline, eliminating the old hand-rolled pane-body ANSI diff.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::tui::components::status_bar::{PrefixMode, StatusBarPlan, StatusTabCell, TabGlyph};

use jackin_tui::components::{
    FooterLeft, Panel, PanelFocus, StatusFooter, StatusRightGroup, tab_cell_style,
};

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
    fn paint_tab(&self, cell: &StatusTabCell, idx: usize, area: Rect, buf: &mut Buffer) {
        let hovered = self.hovered_tab == Some(idx);
        let style = tab_cell_style(cell.active, hovered);
        let bg = style.bg.unwrap_or(Color::Reset);
        let glyph_char = tab_glyph_char(cell.glyph);
        // Cell layout: ` <name> <sep> <glyph> ` — matches emit_tab_row0.
        let content = format!(" {} {} ", cell.name, glyph_char);
        let x = area.x.saturating_add(cell.start_col0);
        buf.set_string(x, area.y, &content, style);
        if let Some(glyph_style) = tab_glyph_style(cell.glyph, bg) {
            let name_cols = u16::try_from(termrock::display_cols(&cell.name)).unwrap_or(u16::MAX);
            let glyph_x = x.saturating_add(name_cols).saturating_add(2);
            buf.set_string(glyph_x, area.y, glyph_char.to_string(), glyph_style);
        }
    }
}

const fn tab_glyph_char(glyph: TabGlyph) -> char {
    match glyph {
        TabGlyph::Blocked => '●',
        TabGlyph::Done => '○',
        TabGlyph::Working => '▶',
        TabGlyph::Idle => '◆',
        TabGlyph::Unknown => ' ',
    }
}

fn tab_glyph_style(glyph: TabGlyph, bg: Color) -> Option<Style> {
    match glyph {
        TabGlyph::Blocked => Some(
            Style::default()
                .bg(bg)
                .fg(termrock::style::STATUS_BLOCKED_RED)
                .add_modifier(Modifier::BOLD),
        ),
        TabGlyph::Working => Some(
            Style::default()
                .bg(bg)
                .fg(termrock::style::DEBUG_AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        TabGlyph::Idle => Some(
            Style::default()
                .bg(bg)
                .fg(termrock::style::PHOSPHOR_GREEN)
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

        // Row 0: brand pill — green block, black word, white chevron.
        let pill = Style::default()
            .bg(termrock::style::BRAND_BLOCK)
            .add_modifier(Modifier::BOLD);
        buf.set_string(area.x, area.y, " jackin", pill.fg(Color::Black));
        buf.set_string(
            area.x.saturating_add(7),
            area.y,
            "❯",
            pill.fg(termrock::style::WHITE),
        );
        buf.set_string(area.x.saturating_add(8), area.y, " ", pill);

        // Row 0: tab cells.
        for (idx, cell) in plan.cells.iter().enumerate() {
            self.paint_tab(cell, idx, area, buf);
        }

        // Row 0: right-side menu button.
        if let Some(start_1based) = plan.hint_start {
            let (bg, fg) = match (self.prefix_mode, self.menu_hovered) {
                (PrefixMode::Idle, false) => {
                    (termrock::style::MENU_IDLE_BG, termrock::style::WHITE)
                }
                (PrefixMode::Idle, true) => {
                    (termrock::style::MENU_IDLE_HOVER_BG, termrock::style::WHITE)
                }
                (PrefixMode::Awaiting, false) => (termrock::style::MENU_AWAITING_BG, Color::Black),
                (PrefixMode::Awaiting, true) => {
                    (termrock::style::MENU_AWAITING_HOVER_BG, Color::Black)
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
                Style::default().fg(termrock::style::PHOSPHOR_DIM),
            );
        }

        // Row 1: underline beneath the active tab cell only (blank elsewhere),
        // matching the shared capsule/console focus signal.
        if area.height > 1
            && let Some(active) = plan.cells.iter().find(|c| c.active)
        {
            let underline = "━".repeat(active.cell_cols as usize);
            let underline_fg = if self.focused {
                termrock::style::PHOSPHOR_GREEN
            } else {
                termrock::style::WHITE
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

// ── Pane border ───────────────────────────────────────────────────────────────

/// Renders the border and title for one pane through the Ratatui buffer.
#[derive(Debug)]
pub struct PaneBorderWidget {
    pub title: String,
    pub focused: bool,
}

impl Widget for PaneBorderWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let focus = if self.focused {
            PanelFocus::Focused
        } else {
            PanelFocus::Unfocused
        };
        let block = Panel::new().title(&self.title).focus(focus).block();
        block.render(area, buf);
    }
}

pub use jackin_tui::components::ModalBackdrop as DialogBackdrop;

/// Bottom chrome (branch/PR bar, hint row, debug chip) as a widget. Replaces
/// the raw-ANSI append + byte cache: the rows ride the Ratatui cell buffer
/// like every other cell, so one compositor owns the whole frame (§3.2 of
/// the capsule rendering plan).
pub(crate) struct BottomChromeWidget<'a> {
    pub(crate) branch: Option<&'a str>,
    pub(crate) usage_status_label: Option<&'a str>,
    pub(crate) pull_request: Option<&'a crate::pull_request::PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) instance_id_label: &'a str,
    pub(crate) hover_target: Option<crate::tui::model::HoverTarget>,
    pub(crate) scrollback_active: bool,
    pub(crate) scroll_axes: jackin_tui::scroll::ScrollAxes,
    pub(crate) debug_run_id: Option<&'a str>,
    /// When the operator has pressed the prefix key and the multiplexer is
    /// awaiting a command chord, the hint bar switches to a prefix-command
    /// cheat-sheet instead of the normal navigation hints.
    pub(crate) prefix_awaiting: bool,
    /// Resolved palette-key byte (`InputParser::palette_key().unwrap_or(0x1C)`).
    /// Passed to the hint builder so the palette-key glyph matches the
    /// operator's `JACKIN_PALETTE_KEY` configuration.
    pub(crate) palette_key: u8,
}

impl Widget for BottomChromeWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        render_branch_bar_row(
            buf,
            area,
            self.branch,
            self.usage_status_label,
            self.pull_request,
            self.pull_request_loading,
            self.debug_run_id,
            self.instance_id_label,
            self.hover_target,
        );
        let spans = crate::tui::components::dialog::main_view_hint(
            self.scrollback_active,
            self.palette_key,
            self.scroll_axes,
            self.prefix_awaiting,
        );
        render_hint_spans_row(buf, area, &spans);
    }
}

/// Dialog variant of the bottom chrome: branch/PR bar plus the dialog's own
/// footer hint spans.
pub(crate) struct DialogBottomChromeWidget<'a> {
    pub(crate) branch: Option<&'a str>,
    pub(crate) usage_status_label: Option<&'a str>,
    pub(crate) pull_request: Option<&'a crate::pull_request::PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) debug_run_id: Option<&'a str>,
    pub(crate) instance_id_label: &'a str,
    pub(crate) hint_spans: Option<&'a [jackin_tui::HintSpan<'a>]>,
}

impl Widget for DialogBottomChromeWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // The bottom branch/context bar under a dialog renders only in a debug
        // launch (where the run id + diagnostics matter); outside debug it is
        // hidden so the modal stays clean (commit 5f2076a6). Only the dialog
        // hint renders below the dialog in that case.
        if self.debug_run_id.is_some() {
            render_branch_bar_row(
                buf,
                area,
                self.branch,
                self.usage_status_label,
                self.pull_request,
                self.pull_request_loading,
                self.debug_run_id,
                self.instance_id_label,
                None,
            );
        }
        if let Some(spans) = self.hint_spans {
            render_hint_spans_row(buf, area, spans);
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
fn render_branch_bar_row(
    buf: &mut Buffer,
    area: Rect,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&crate::pull_request::PullRequestInfo>,
    pull_request_loading: bool,
    debug_run_id: Option<&str>,
    instance_id_label: &str,
    hover_target: Option<crate::tui::model::HoverTarget>,
) {
    use crate::tui::components::branch_context_bar::branch_context_bar_layout;
    use crate::tui::model::HoverTarget;
    let Some(layout) = branch_context_bar_layout(
        area.height,
        area.width,
        branch,
        usage_status_label,
        pull_request,
        pull_request_loading,
        debug_run_id,
        instance_id_label,
    ) else {
        return;
    };
    let bar_y = area.height.saturating_sub(1);
    let left_hovered = hover_target == Some(HoverTarget::BranchContext);
    let left = if layout.left_region.is_some() {
        FooterLeft::link(layout.left.trim())
    } else {
        FooterLeft::plain("")
    };
    StatusFooter::new("")
        .left(left)
        .right_group(StatusRightGroup {
            usage: usage_status_label,
            container: instance_id_label,
            run_id: debug_run_id,
        })
        .left_hover(left_hovered)
        .usage_hover(hover_target == Some(HoverTarget::UsageStatus))
        .right_hover(hover_target == Some(HoverTarget::Container))
        .right_debug_hover(hover_target == Some(HoverTarget::DebugChip))
        .render(
            Rect {
                x: area.x,
                y: bar_y,
                width: area.width,
                height: 1,
            },
            buf,
        );
}

/// The pane and footer chrome need one spacer each, so hints stay visually
/// separate from both the agent border and the branch context bar.
fn render_hint_spans_row(buf: &mut Buffer, area: Rect, spans: &[jackin_tui::HintSpan<'_>]) {
    use crate::tui::components::branch_context_bar::BRANCH_CONTEXT_BAR_ROWS;
    use crate::tui::layout::{
        CAPSULE_HINT_BAR_ROWS, CAPSULE_HINT_SEPARATOR_ROWS, CAPSULE_HINT_TOP_SEPARATOR_ROWS,
    };
    if area.height
        < BRANCH_CONTEXT_BAR_ROWS
            + CAPSULE_HINT_SEPARATOR_ROWS
            + CAPSULE_HINT_BAR_ROWS
            + CAPSULE_HINT_TOP_SEPARATOR_ROWS
    {
        return;
    }
    let available = area.width.saturating_sub(4); // 2 col padding each side
    let lines = jackin_tui::components::wrapped_lines(spans, available);
    let hint_rows = usize::from(CAPSULE_HINT_BAR_ROWS);
    if lines.is_empty() {
        return;
    }
    let visible = &lines[..lines.len().min(hint_rows)];
    let first_row = area.height.saturating_sub(
        BRANCH_CONTEXT_BAR_ROWS + CAPSULE_HINT_SEPARATOR_ROWS + CAPSULE_HINT_BAR_ROWS,
    );
    for (idx, line) in visible.iter().enumerate() {
        let total = line_display_cols(line);
        let padded_total = total.saturating_add(4);
        let start_col = ((usize::from(area.width)).saturating_sub(padded_total) / 2) as u16;
        let mut x = area.x + start_col + 2;
        let row_y = area.y + first_row + u16::try_from(idx).unwrap_or(0);
        for span in &line.spans {
            let content = span.content.as_ref();
            buf.set_string(x, row_y, content, span.style);
            x += termrock::display_cols(content) as u16;
        }
    }
}

fn line_display_cols(line: &ratatui::text::Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| termrock::display_cols(span.content.as_ref()))
        .sum()
}

#[cfg(test)]
mod tests;
