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

use jackin_tui::components::{Panel, PanelFocus, tab_cell_style};

// ── Status bar (row 0 + row 1) ────────────────────────────────────────────────

const BRAND_TEXT: &str = " jackin' ";

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
        let glyph_char = match cell.glyph {
            TabGlyph::None => ' ',
            TabGlyph::Done => '○',
            TabGlyph::Blocked => '●',
        };
        // Cell layout: ` <name> <sep> <glyph> ` — matches emit_tab_row0.
        let content = format!(" {} {} ", cell.name, glyph_char);
        let x = area.x.saturating_add(cell.start_col0);
        buf.set_string(x, area.y, &content, style);
        // Blocked glyph is bright red; overpaint just that cell, same bg.
        if matches!(cell.glyph, TabGlyph::Blocked) {
            let name_cols = u16::try_from(jackin_tui::display_cols(&cell.name)).unwrap_or(u16::MAX);
            let glyph_x = x.saturating_add(name_cols).saturating_add(2);
            buf.set_string(
                glyph_x,
                area.y,
                "●",
                Style::default()
                    .bg(bg)
                    .fg(jackin_tui::theme::STATUS_BLOCKED_RED)
                    .add_modifier(Modifier::BOLD),
            );
        }
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

        // Row 0: brand pill.
        buf.set_string(
            area.x,
            area.y,
            BRAND_TEXT,
            Style::default()
                .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

        // Row 0: tab cells.
        for (idx, cell) in plan.cells.iter().enumerate() {
            self.paint_tab(cell, idx, area, buf);
        }

        // Row 0: right-side menu button.
        if let Some(start_1based) = plan.hint_start {
            let (bg, fg) = match (self.prefix_mode, self.menu_hovered) {
                (PrefixMode::Idle, false) => (
                    jackin_tui::theme::CAPSULE_MENU_IDLE_BG,
                    jackin_tui::theme::WHITE,
                ),
                (PrefixMode::Idle, true) => (
                    jackin_tui::theme::CAPSULE_MENU_IDLE_HOVER_BG,
                    jackin_tui::theme::WHITE,
                ),
                (PrefixMode::Awaiting, false) => {
                    (jackin_tui::theme::CAPSULE_MENU_AWAITING_BG, Color::Black)
                }
                (PrefixMode::Awaiting, true) => (
                    jackin_tui::theme::CAPSULE_MENU_AWAITING_HOVER_BG,
                    Color::Black,
                ),
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
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            );
        }

        // Row 1: underline beneath the active tab cell only (blank elsewhere),
        // matching the shared capsule/console focus signal.
        if area.height > 1
            && let Some(active) = plan.cells.iter().find(|c| c.active)
        {
            let underline = "━".repeat(active.cell_cols as usize);
            let underline_fg = if self.focused {
                jackin_tui::theme::PHOSPHOR_GREEN
            } else {
                jackin_tui::theme::WHITE
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
use jackin_tui::theme::color;

const BAR_BG: Color = color(jackin_tui::WHITE);
const BAR_FG: Color = color(jackin_tui::BLACK);
const BAR_LINK_FG: Color = color(jackin_tui::LINK_BLUE);
const BAR_HOVER_BG: Color = Color::Rgb(225, 245, 255);
const BAR_HOVER_FG: Color = Color::Rgb(0, 55, 140);

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
    pub(crate) hover_target: Option<crate::tui::app::HoverTarget>,
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

/// Spawn-failure banner: a red one-line notice painted over the top row.
/// Cleared by the next operator keystroke.
pub(crate) struct SpawnFailureBannerWidget<'a> {
    pub(crate) reason: &'a str,
}

impl Widget for SpawnFailureBannerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let style = Style::default()
            .fg(color(jackin_tui::DANGER_RED))
            .add_modifier(Modifier::BOLD);
        for x in area.left()..area.right() {
            buf[(x, area.top())].reset();
        }
        buf.set_string(area.x, area.y, format!("jackin: {}", self.reason), style);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_branch_bar_row(
    buf: &mut Buffer,
    area: Rect,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&crate::pull_request::PullRequestInfo>,
    pull_request_loading: bool,
    debug_run_id: Option<&str>,
    instance_id_label: &str,
    hover_target: Option<crate::tui::app::HoverTarget>,
) {
    use crate::tui::app::HoverTarget;
    use crate::tui::components::branch_context_bar::branch_context_bar_layout;
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
    let base = Style::default().bg(BAR_BG).fg(BAR_FG);
    for x in area.left()..area.right() {
        buf.set_string(x, bar_y, " ", base);
    }
    let left_hovered = hover_target == Some(HoverTarget::BranchContext);
    let left_style = chunk_style(left_hovered, BAR_FG, true);
    buf.set_string(area.x, bar_y, &layout.left, left_style);
    if let Some(region) = layout.container_region {
        let container_hovered = hover_target == Some(HoverTarget::Container);
        let container_style = chunk_style(container_hovered, BAR_LINK_FG, false);
        buf.set_string(
            area.x + region.start.saturating_sub(1),
            bar_y,
            &layout.container,
            container_style,
        );
    }
    if let Some(region) = layout.debug_chip_region {
        let debug_hovered = hover_target == Some(HoverTarget::DebugChip);
        let debug_style = if debug_hovered {
            Style::default()
                .bg(BAR_BG)
                .fg(color(jackin_tui::DANGER_RED))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(color(jackin_tui::DANGER_RED))
                .fg(color(jackin_tui::WHITE))
                .add_modifier(Modifier::BOLD)
        };
        buf.set_string(
            area.x + region.start.saturating_sub(1),
            bar_y,
            &layout.debug_chip,
            debug_style,
        );
    }
    if let Some(region) = layout.usage_region {
        let usage_hovered = hover_target == Some(HoverTarget::UsageStatus);
        let usage_style = chunk_style(usage_hovered, BAR_FG, false);
        buf.set_string(
            area.x + region.start.saturating_sub(1),
            bar_y,
            &layout.usage,
            usage_style,
        );
    }
}

/// Per-chunk colour rule, ported from the raw renderer: the left chunk is
/// always bold; the container chunk is bold only on hover and uses the link
/// foreground when idle.
fn chunk_style(hovered: bool, idle_fg: Color, always_bold: bool) -> Style {
    let mut style = if hovered {
        Style::default().bg(BAR_HOVER_BG).fg(BAR_HOVER_FG)
    } else {
        Style::default().bg(BAR_BG).fg(idle_fg)
    };
    if always_bold || hovered {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

/// Returns the largest prefix of `spans` whose column width fits inside `max_cols`,
/// always truncating at a `GroupSep` boundary to avoid splitting a key+text pair.
/// Returns an empty slice if even the first group overflows.
fn truncate_spans_to_cols<'a>(
    spans: &'a [jackin_tui::HintSpan<'_>],
    max_cols: usize,
) -> &'a [jackin_tui::HintSpan<'a>] {
    // Split into groups at GroupSep boundaries; accumulate greedily.
    let mut last_fit_end = 0usize;
    let mut running_cols = 0usize;

    let mut i = 0;
    while i < spans.len() {
        // Measure from i to next GroupSep (exclusive) — one logical group.
        let group_end = spans[i..]
            .iter()
            .position(|s| matches!(s, jackin_tui::HintSpan::GroupSep))
            .map_or(spans.len(), |rel| i + rel + 1); // include the GroupSep itself

        let group_cols = jackin_tui::hint_row_cols(&spans[i..group_end]);
        let candidate = running_cols.saturating_add(group_cols);
        if candidate > max_cols {
            break;
        }
        running_cols = candidate;
        last_fit_end = group_end;
        i = group_end;
    }

    // Strip trailing GroupSep if present so the last group doesn't end with whitespace.
    let mut end = last_fit_end;
    while end > 0 && matches!(spans[end - 1], jackin_tui::HintSpan::GroupSep) {
        end -= 1;
    }
    &spans[..end]
}

/// Centered hint spans on the row above the separator pad — the widget port
/// of `render_hint_row`, same column math so centring is identical.
/// Gracefully truncates at group boundaries when the full row is too wide.
fn render_hint_spans_row(buf: &mut Buffer, area: Rect, spans: &[jackin_tui::HintSpan<'_>]) {
    use crate::tui::components::branch_context_bar::BRANCH_CONTEXT_BAR_ROWS;
    if area.height < BRANCH_CONTEXT_BAR_ROWS + 2 {
        return;
    }
    let available = usize::from(area.width).saturating_sub(4); // 2 col padding each side
    let visible = truncate_spans_to_cols(spans, available);
    if visible.is_empty() {
        return;
    }
    let total = jackin_tui::hint_row_cols(visible);
    let padded_total = total.saturating_add(4);
    let row_y = area.height - (BRANCH_CONTEXT_BAR_ROWS + 2);
    let start_col = ((usize::from(area.width)).saturating_sub(padded_total) / 2) as u16;
    let key_style = Style::default()
        .fg(color(jackin_tui::WHITE))
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(color(jackin_tui::PHOSPHOR_GREEN));
    let dyn_style = Style::default().fg(color(jackin_tui::PHOSPHOR_DIM));
    let sep_style = Style::default().fg(color(jackin_tui::PHOSPHOR_DARK));
    let mut x = area.x + start_col + 2;
    for span in visible {
        let (text, style): (String, Style) = match span {
            jackin_tui::HintSpan::Key(k) => ((*k).to_owned(), key_style),
            jackin_tui::HintSpan::Text(t) => (format!(" {t}"), text_style),
            jackin_tui::HintSpan::Dyn(t) => (format!(" {t}"), dyn_style),
            jackin_tui::HintSpan::Sep => (" · ".to_owned(), sep_style),
            jackin_tui::HintSpan::GroupSep => ("   ".to_owned(), sep_style),
        };
        buf.set_string(x, row_y, &text, style);
        x += jackin_tui::display_cols(&text) as u16;
    }
}

#[cfg(test)]
mod tests;
