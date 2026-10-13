// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rendering helper types and functions for the capsule multiplexer.

use crate::pull_request::PullRequestInfo;

use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;

use crate::tui::layout::Tab;
use crate::tui::model::{HoverTarget, VisiblePane};
use jackin_tui::runtime::SurfaceFocus;
use ratatui::{Frame, layout::Rect as RatatuiRect};

pub(crate) const fn hovered_tab(target: Option<HoverTarget>) -> Option<usize> {
    match target {
        Some(HoverTarget::Tab(idx)) => Some(idx),
        _ => None,
    }
}

pub(crate) const fn hovered_menu(target: Option<HoverTarget>) -> bool {
    matches!(target, Some(HoverTarget::Menu))
}

/// Dialog snapshot with its bounding rect — factored out to keep `CapsuleRatatuiFrame` readable.
pub(crate) type DialogFrameSnapshot = (DialogRatatuiSnapshot, (u16, u16, u16, u16));

#[derive(Debug)]
pub(crate) enum PaneScreen<'a> {
    View(termpane::GridView<'a>),
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Six orthogonal render-state flags on the per-frame snapshot \
              (zoomed, dialog_open, menu_hovered, selection_copied, \
              pull_request_loading, scrollback_active) — each tracks an \
              independent UI state consumed individually by the compositor \
              branches. Named-field reads match the per-branch dispatch idiom \
              this snapshot feeds."
)]
#[derive(Clone)]
pub(crate) struct CapsuleRatatuiFrame<'a> {
    pub(crate) tabs: &'a [Tab],
    /// Row-0 layout computed once per frame and shared by the status-bar
    /// widget (paint), the tab tooltip, and the compositor's click-region
    /// refresh, so the bar is laid out once rather than per consumer.
    pub(crate) status_plan: &'a crate::tui::components::status_bar::StatusBarPlan,
    pub(crate) term_cols: u16,
    pub(crate) term_rows: u16,
    pub(crate) panes: &'a [VisiblePane],
    pub(crate) pane_titles: &'a [(u64, String)],
    pub(crate) focus_owner: SurfaceFocus<u64>,
    pub(crate) zoomed: bool,
    pub(crate) dialog_open: bool,
    pub(crate) dialog_snapshot: Option<&'a DialogFrameSnapshot>,
    pub(crate) pane_screens: &'a [(u64, PaneScreen<'a>)],
    pub(crate) prefix_mode: crate::tui::components::status_bar::PrefixMode,
    pub(crate) hovered_tab: Option<usize>,
    pub(crate) menu_hovered: bool,
    pub(crate) selection: Option<crate::tui::selection::SelectionState>,
    pub(crate) selection_copied: bool,
    /// Per-pane scrollbar inputs `(session_id, offset, filled)`. A pane with
    /// `filled > 0` gets a thumb painted on its right border.
    pub(crate) scrollbars: &'a [(u64, usize, usize)],
    pub(crate) branch: Option<&'a str>,
    pub(crate) usage_status_label: Option<&'a str>,
    pub(crate) pull_request: Option<&'a PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) instance_id_label: &'a str,
    pub(crate) hover_target: Option<HoverTarget>,
    pub(crate) scrollback_active: bool,
    pub(crate) main_scroll_axes: termrock::scroll::ScrollAxes,
    pub(crate) debug_run_id: Option<&'a str>,
    pub(crate) dialog_hint_spans: Option<&'a [termrock::widgets::HintSpan<'a>]>,
    /// Resolved palette-key byte (`InputParser::palette_key().unwrap_or(0x1C)`).
    /// Forwarded to the hint builder so the palette-key glyph reflects the
    /// operator's `JACKIN_PALETTE_KEY` setting.
    pub(crate) palette_key: u8,
    /// Transient host clipboard image paste result. Painted in the content
    /// toast area so it cannot overwrite status rows or bottom chrome.
    pub(crate) clipboard_image_notice: Option<&'a str>,
    /// Host-open target under an Alt/Ctrl hover in a mouse-disabled pane.
    /// Painted through the compositor so the PTY never receives hover text.
    pub(crate) link_hover_notice: Option<&'a str>,
}

fn render_clipboard_image_notice(frame: &mut Frame<'_>, view: &CapsuleRatatuiFrame<'_>) {
    if let Some(notice) = view.clipboard_image_notice {
        render_notice_toast(frame, selection_toast_area(view), notice);
    }
}

fn render_link_hover_notice(frame: &mut Frame<'_>, view: &CapsuleRatatuiFrame<'_>) {
    if view.clipboard_image_notice.is_some() {
        return;
    }
    if let Some(notice) = view.link_hover_notice {
        render_notice_toast(frame, selection_toast_area(view), notice);
    }
}

fn render_notice_toast(frame: &mut Frame<'_>, area: RatatuiRect, message: &str) {
    let theme = termrock::style::DesignSystem::default();
    // Full TermRock toast contract: severity border role, bottom-left anchor
    // under the status strip, and theme-derived text — no product local chrome.
    frame.render_widget(
        termrock::widgets::Toast::new(&theme, message, termrock::widgets::Severity::Success)
            .anchor(termrock::widgets::Anchor::BottomLeft)
            .margins(1, 0),
        area,
    );
}

/// Paint the hovered tab's codename as a dark-bg + phosphor-green pill on the
/// row directly below the tab strip, left-aligned with the tab cell. Ratatui
/// `Buffer::set_string` clips to the buffer area, so an out-of-range column or
/// a too-long codename cannot overflow the frame.
fn apply_tab_codename_tooltip(
    buf: &mut ratatui::buffer::Buffer,
    plan: &crate::tui::components::status_bar::StatusBarPlan,
    hovered_idx: usize,
    codename: &str,
) {
    use ratatui::style::{Modifier, Style};
    let Some(cell) = plan.cells.get(hovered_idx) else {
        return;
    };
    // Row index 2 (0-based): one row below the tab strip (row 0) and the
    // active-tab underline (row 1).
    let tooltip_row = crate::tui::components::status_bar::STATUS_BAR_ROWS;
    let pill = format!(" {codename} ");
    buf.set_string(
        cell.start_col0,
        tooltip_row,
        &pill,
        Style::default()
            .bg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TabInactive)
                .bg
                .unwrap_or_default())
            .fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
            .add_modifier(Modifier::BOLD),
    );
}

/// Format a `label: error` string.
pub(crate) fn spawn_failure_message(agent_label: &str, error: impl std::fmt::Display) -> String {
    format!("{agent_label}: {error:#}")
}

pub(crate) fn spawn_failure_agent_label(agent_slug: Option<&str>) -> &str {
    agent_slug.unwrap_or("shell")
}

pub(crate) fn spawn_request_failure_message(
    request_label: &str,
    error: impl std::fmt::Display,
) -> String {
    format!("spawn {request_label} failed: {error:#}")
}

pub(crate) fn tab_limit_failure_message(max_tabs: usize) -> String {
    format!("tab limit reached ({max_tabs}); close one before spawning another")
}

pub(crate) fn pane_limit_failure_message(max_sessions: usize) -> String {
    format!("pane limit reached ({max_sessions}); close some panes before opening more")
}

/// Forwarded to the operator's outer terminal via `send_output` from the
/// `CopyToClipboard` dialog action. The OSC 52 byte encoding and terminal
/// compatibility notes live with the canonical typed `TermRock` encoder.
pub(crate) fn encode_osc52_clipboard_write(payload: &str) -> Vec<u8> {
    termrock::osc::encode_clipboard(termrock::osc::ClipboardWrite {
        selection: termrock::osc::ClipboardSelection::Clipboard,
        text: payload,
    })
}

mod frame;
pub(crate) use frame::{render_capsule_ratatui_frame, selection_toast_area};

#[cfg(test)]
mod tests;
