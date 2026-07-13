// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Listing + footer rendering for the file browser modal.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{HighlightSpacing, ListItem, Paragraph},
};

use super::git_prompt::render_git_prompt;
use super::state::FileBrowserState;
use super::{PHOSPHOR_GREEN, WHITE};
use jackin_tui::components::{
    Panel, PanelFocus, ScrollableList, cursor_follow_offset, viewport_height,
};

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
                jackin_tui::theme::DANGER,
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
        jackin_tui::shorten_home(&state.cwd.display().to_string())
    );
    // File browser is normally the active modal (PHOSPHOR_GREEN border). When a
    // child dialog (Git repo prompt) is stacked on top, the file browser becomes
    // a background modal and must use the inactive border so exactly one bright
    // border is visible (Defect 9 — one-bright-border rule).
    let block = if state.pending_git_prompt.is_some() {
        jackin_tui::components::unfocused_block()
            .title(Span::styled(title.clone(), jackin_tui::theme::BOLD_WHITE))
    } else {
        Panel::new()
            .title(title.as_str())
            .focus(PanelFocus::Focused)
            .block()
    };

    let selected = state.list_state.selected;
    let cursor_symbol = if state.pending_git_prompt.is_some() {
        "  "
    } else {
        "\u{25b8} "
    };
    let base_style = Style::default().fg(WHITE);
    let git_suffix_style = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);

    let items: Vec<ListItem<'_>> = state
        .entries
        .iter()
        .map(|e| {
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
            ListItem::new(line)
        })
        .collect();
    let offset = cursor_follow_offset(
        selected.unwrap_or(0),
        state.entries.len(),
        viewport_height(area),
        0,
    );
    let list = ScrollableList::new(items)
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_symbol(cursor_symbol)
        .offset(offset)
        .selected(selected);
    list.render_with_block(area, frame.buffer_mut(), block);
}

#[cfg(test)]
mod tests;
