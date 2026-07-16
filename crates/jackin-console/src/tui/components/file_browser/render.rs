// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Listing + footer rendering for the file browser modal.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::git_prompt::render_git_prompt;
use super::state::FileBrowserState;
use super::{accent_fg, text_fg};
use termrock::widgets::{List, ListRow, ListState, Panel, PanelEmphasis, RowRole};

/// Vertical-layout constraints used by `render` and by the geometry-only
/// helpers consumed by the mouse-click hit-tester. Keep these in sync.
fn render_constraints(has_rejection: bool) -> Vec<ratatui::layout::Constraint> {
    use ratatui::layout::Constraint;
    if has_rejection {
        vec![Constraint::Length(1), Constraint::Min(3)]
    } else {
        vec![Constraint::Min(3)]
    }
}

/// Rect of the listing area inside the modal.
///
/// This is the same chunk that `render` passes to `render_listing` and
/// anchors `render_git_prompt` on. Exposed so a mouse-handler can
/// recompute the git-prompt overlay geometry without needing `&mut`
/// access at render time.
pub fn listing_rect(modal_area: Rect, has_rejection: bool) -> Rect {
    // Structural exception: File Browser render and mouse paths share this listing sub-rect derived from the modal body.
    use ratatui::layout::{Direction, Layout};
    let constraints = render_constraints(has_rejection);
    let listing_idx = usize::from(has_rejection);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(modal_area);
    chunks[listing_idx]
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &FileBrowserState) {
    use ratatui::layout::{Alignment, Direction, Layout};

    frame.render_widget(ratatui::widgets::Clear, area);

    // Layout: [optional rejection banner][listing]. Hints render in the
    // screen footer (see `FileBrowserState::footer_items`), not inside the box.
    let rejection = state.rejected_reason.as_ref();
    let constraints = render_constraints(rejection.is_some());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let listing_idx = rejection.map_or(0, |reason| {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("\u{2717} {reason}"),
                jackin_core::tui_theme::danger(),
            ))
            .alignment(Alignment::Center),
            chunks[0],
        );
        1
    });

    render_listing(frame, chunks[listing_idx], state);

    // Git-repo prompt overlay — centred inside the listing area so the
    // listing stays visible as context behind the modal.
    if state.pending_git_prompt.is_some() {
        render_git_prompt(frame, chunks[listing_idx], state);
    }
}

/// Render the folder listing inside `area` with a phosphor-framed block
/// and a bold-white cwd title.
fn render_listing(frame: &mut Frame<'_>, area: Rect, state: &FileBrowserState) {
    let title = format!(
        " {} ",
        jackin_core::shorten_home(&state.cwd.display().to_string())
    );
    // File browser is normally the active modal (accent_fg() border). When a
    // child dialog (Git repo prompt) is stacked on top, the file browser becomes
    // a background modal and must use the inactive border so exactly one bright
    // border is visible (Defect 9 — one-bright-border rule).
    let theme = termrock::Theme::default();
    let panel = Panel::new(&theme)
        .title(&title)
        .emphasis(if state.pending_git_prompt.is_some() {
            PanelEmphasis::Normal
        } else {
            PanelEmphasis::Focused
        });
    let inner = panel.inner(area);
    frame.render_widget(&panel, area);

    let selected = state
        .pending_git_prompt
        .is_none()
        .then_some(state.list_state.selected)
        .flatten();
    let base_style = Style::default().fg(text_fg());
    let git_suffix_style = Style::default()
        .fg(accent_fg())
        .add_modifier(Modifier::BOLD);

    let rows: Vec<ListRow<'_, usize>> = state
        .entries
        .iter()
        .enumerate()
        .map(|(id, e)| {
            let name_slash = if e.is_parent {
                "../".to_owned()
            } else {
                format!("{}/", e.name)
            };
            let line = if e.is_git {
                Line::from(vec![
                    Span::styled(name_slash, base_style),
                    Span::styled(" (git)", git_suffix_style),
                ])
            } else {
                Line::from(Span::styled(name_slash, base_style))
            };
            ListRow {
                id,
                label: line,
                trailing: None,
                role: RowRole::Item,
                enabled: true,
            }
        })
        .collect();
    let mut list_state = ListState::new(selected);
    let scrollbar_gutter = u16::from(state.entries.len() > usize::from(inner.height));
    let list_area = Rect {
        width: inner
            .width
            .saturating_add(scrollbar_gutter)
            .min(area.right().saturating_sub(inner.x)),
        ..inner
    };
    frame.render_stateful_widget(&List::new(&rows, &theme), list_area, &mut list_state);
}

#[cfg(test)]
mod tests;
