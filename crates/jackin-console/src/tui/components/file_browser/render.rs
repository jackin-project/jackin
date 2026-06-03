//! Listing + footer rendering for the file browser modal.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::git_prompt::render_git_prompt;
use super::state::FileBrowserState;
use super::{PHOSPHOR_GREEN, WHITE};
use jackin_tui::components::{Panel, PanelFocus};

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
    use ratatui::layout::{Direction, Layout};
    let constraints = render_constraints(has_rejection);
    let listing_idx = usize::from(has_rejection);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(modal_area);
    chunks[listing_idx]
}

pub fn render(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
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
fn render_listing(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let title = format!(
        " {} ",
        jackin_tui::shorten_home(&state.cwd.display().to_string())
    );
    // File browser is a modal picker — always active when visible — so its
    // border must be PHOSPHOR_GREEN (RULE 1: focus-visible border).
    let block = Panel::new()
        .title(title.as_str())
        .focus(PanelFocus::Focused)
        .block();

    let selected = state.list_state.selected;
    let highlight_style = Style::default()
        .bg(PHOSPHOR_GREEN)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let base_style = Style::default().fg(WHITE);
    let git_suffix_style = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);

    let lines: Vec<Line> = state
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let is_sel = Some(i) == selected;
            let name_slash = if e.is_parent {
                "../".to_string()
            } else {
                format!("{}/", e.name)
            };
            if is_sel {
                // Highlight row: single span covering name + optional git suffix.
                let mut text = format!("  {name_slash}");
                if e.is_git {
                    text.push_str(" (git)");
                }
                Line::from(Span::styled(text, highlight_style))
            } else if e.is_git {
                Line::from(vec![
                    Span::styled(format!("  {name_slash}"), base_style),
                    Span::styled(" (git)", git_suffix_style),
                ])
            } else {
                Line::from(Span::styled(format!("  {name_slash}"), base_style))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests;
