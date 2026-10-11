// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Ratatui widgets for capsule chrome: status bar, pane borders, branch bar.
//!
//! These widgets replace the raw-ANSI rendering in `compose_full_frame` and
//! `compose_partial_frame`. Together with `PaneBodyWidget` they make the
//! capsule's full rendering path go through the Ratatui `Buffer` → `SocketBackend`
//! pipeline, eliminating the old hand-rolled pane-body ANSI diff.

#[cfg(test)]
use crate::tui::components::status_bar::{PrefixMode, StatusTabCell};
#[cfg(test)]
use ratatui::style::Color;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use termrock::style::DesignSystem;
use termrock::widgets::{Panel, PanelChrome};

// ── Pane border ───────────────────────────────────────────────────────────────

/// Renders the border and title for one pane through the Ratatui buffer.
#[derive(Debug)]
pub struct PaneBorderWidget {
    pub title: String,
    pub focused: bool,
}

const fn pane_border_emphasis(focused: bool) -> PanelChrome {
    if focused {
        PanelChrome::Focused
    } else {
        PanelChrome::Normal
    }
}

impl Widget for PaneBorderWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = DesignSystem::default();
        let block = Panel::new(&theme)
            .title(&self.title)
            .emphasis(pane_border_emphasis(self.focused))
            .block();
        block.render(area, buf);
    }
}

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
    pub(crate) scroll_axes: termrock::scroll::ScrollAxes,
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
    pub(crate) hint_spans: Option<&'a [termrock::widgets::HintSpan<'a>]>,
}

impl Widget for DialogBottomChromeWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // The bottom branch/context bar under a dialog renders only in a debug
        // launch, where invocation correlation is visible. Outside debug it is
        // hidden so the modal stays clean; only the dialog hint renders below
        // the dialog in that case.
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
    crate::tui::components::branch_context_bar::render_branch_context_bar(
        buf,
        Rect {
            x: area.x,
            y: area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        },
        crate::tui::components::branch_context_bar::BranchContextBarView {
            branch,
            usage_status_label,
            pull_request,
            pull_request_loading,
            debug_run_id,
            container_name: instance_id_label,
            hover_target,
        },
    );
}

/// The pane and footer chrome need one spacer each, so hints stay visually
/// separate from both the agent border and the branch context bar.
fn render_hint_spans_row(buf: &mut Buffer, area: Rect, spans: &[termrock::widgets::HintSpan<'_>]) {
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
    let lines = termrock::widgets::wrapped_hint_lines(spans, available, &DesignSystem::default());
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
        let start_col = u16::try_from((usize::from(area.width)).saturating_sub(padded_total) / 2)
            .unwrap_or(u16::MAX);
        let mut x = area.x + start_col + 2;
        let row_y = area.y + first_row + u16::try_from(idx).unwrap_or(0);
        for span in &line.spans {
            let content = span.content.as_ref();
            buf.set_string(x, row_y, content, span.style);
            x = x.saturating_add(
                u16::try_from(termrock::text::display_cols(content)).unwrap_or(u16::MAX),
            );
        }
    }
}

fn line_display_cols(line: &ratatui::text::Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| termrock::text::display_cols(span.content.as_ref()))
        .sum()
}

mod status_bar;
pub use status_bar::StatusBarWidget;

#[cfg(test)]
mod tests;
