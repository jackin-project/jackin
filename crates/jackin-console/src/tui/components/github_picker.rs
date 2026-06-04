//! Modal picker for "open in GitHub" when a workspace has multiple
//! GitHub-hosted mounts.
//!
//! Mirrors `WorkdirPickState`'s shape — one `Vec`-driven list +
//! `tui_widget_list::ListState` — so the rest of the launch TUI can
//! dispatch it with the same Up/Down/Enter pattern.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use crate::github_mounts::GithubChoice;
use jackin_tui::{ModalOutcome, shorten_home};

#[derive(Debug)]
pub struct GithubPickerState {
    pub choices: Vec<GithubChoice>,
    pub list_state: ListState,
}

impl GithubPickerState {
    pub fn new(choices: Vec<GithubChoice>) -> Self {
        let list_state = crate::tui::components::list_helpers::list_state_for_count(choices.len());
        Self {
            choices,
            list_state,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                crate::tui::components::list_helpers::cycle_select(
                    &mut self.list_state,
                    self.choices.len(),
                    -1,
                );
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                crate::tui::components::list_helpers::cycle_select(
                    &mut self.list_state,
                    self.choices.len(),
                    1,
                );
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(c) = crate::tui::components::list_helpers::selected_choice(
                    &self.choices,
                    self.list_state.selected,
                ) {
                    return ModalOutcome::Commit(c.url.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::components::scrollable_panel::render_selected_lines_in_area;
use jackin_tui::components::{Panel, PanelFocus};
use jackin_tui::theme::{PHOSPHOR_DIM, WHITE};

pub fn render(frame: &mut Frame, area: Rect, state: &GithubPickerState) {
    // Title style matches WorkdirPick — Panel::block() applies the correct
    // modal focus styling (PHOSPHOR_GREEN border, WHITE + BOLD title).
    let block = Panel::new()
        .title(" Open in GitHub ")
        .focus(PanelFocus::Focused)
        .block();

    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // list
        ])
        .split(inner);

    if state.choices.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(ratatui::text::Line::from(
                ratatui::text::Span::styled("no GitHub sources", jackin_tui::theme::DIM),
            ))
            .alignment(ratatui::layout::Alignment::Center),
            rows[1],
        );
        return;
    }
    // Pre-compute shortened src + width so the `· github · <branch>` suffix
    // lines up across rows.
    let displays: Vec<String> = state.choices.iter().map(|c| shorten_home(&c.src)).collect();
    let path_w = displays
        .iter()
        .map(|d| d.chars().count())
        .max()
        .unwrap_or(0)
        .max(10);

    let lines: Vec<Line> = state
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let prefix = if Some(i) == state.list_state.selected {
                "▸ "
            } else {
                "  "
            };
            let display = &displays[i];
            let pad = path_w.saturating_sub(display.chars().count());
            Line::from(vec![
                Span::styled(format!("{prefix}{display}"), Style::default().fg(WHITE)),
                Span::raw(format!("{}  ", " ".repeat(pad))),
                Span::styled(
                    format!("github \u{b7} {}", c.branch),
                    Style::default()
                        .fg(PHOSPHOR_DIM)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])
        })
        .collect();

    render_selected_lines_in_area(frame, rows[1], lines, state.list_state.selected);
}

#[cfg(test)]
mod tests;
