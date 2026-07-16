// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Modal picker for "open in GitHub" when a workspace has multiple
//! GitHub-hosted mounts.
//!
//! Mirrors `WorkdirPickState`'s shape — one `Vec`-driven list +
//! `termrock::widgets::ListState` — so the rest of the launch TUI can
//! dispatch it with the same Up/Down/Enter pattern.

use crate::github_mounts::GithubChoice;
use crossterm::event::{KeyCode, KeyEvent};
use jackin_core::ModalOutcome;
use jackin_core::shorten_home;
use termrock::widgets::ListState;

#[derive(Debug)]
pub struct GithubPickerState {
    pub choices: Vec<GithubChoice>,
    pub list_state: ListState<usize>,
}

#[derive(Debug)]
pub enum GithubOpenPlan {
    Continue,
    OpenUrl(String),
    Pick(GithubPickerState),
}

#[must_use]
pub fn github_open_plan(choices: Vec<GithubChoice>) -> GithubOpenPlan {
    match choices.len() {
        0 => GithubOpenPlan::Continue,
        1 => GithubOpenPlan::OpenUrl(choices[0].url.clone()),
        _ => GithubOpenPlan::Pick(GithubPickerState::new(choices)),
    }
}

impl GithubPickerState {
    pub fn new(choices: Vec<GithubChoice>) -> Self {
        let list_state = ListState::for_count(choices.len());
        Self {
            choices,
            list_state,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.list_state.cycle_index(self.choices.len(), -1);
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.list_state.cycle_index(self.choices.len(), 1);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(c) = self.list_state.selected_item(&self.choices) {
                    return ModalOutcome::Commit(c.url.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        self.list_state
            .move_index(self.choices.len(), isize::from(delta))
    }
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_core::tui_theme::{muted_fg, text_fg};
use termrock::layout::render_dialog_shell;
use termrock::widgets::PanelEmphasis;
use termrock::widgets::{List, ListRow, RowRole};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &GithubPickerState) {
    let inner = render_dialog_shell(frame, area, Some("Open in GitHub"), PanelEmphasis::Focused, &termrock::Theme::default());

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // list
        ])
        .split(inner);

    if state.choices.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "no GitHub sources",
                jackin_core::tui_theme::text_muted(),
            )))
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

    let items: Vec<ListRow<'_, usize>> = state
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let display = &displays[i];
            let pad = path_w.saturating_sub(display.chars().count());
            ListRow {
                id: i,
                label: Line::from(vec![
                    Span::styled(display.to_owned(), Style::default().fg(text_fg())),
                    Span::raw(format!("{}  ", " ".repeat(pad))),
                    Span::styled(
                        format!("github \u{b7} {}", c.branch),
                        Style::default()
                            .fg(muted_fg())
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]),
                trailing: None,
                role: RowRole::Item,
                enabled: true,
            }
        })
        .collect();
    let theme = termrock::Theme::default();
    frame.render_stateful_widget(
        &List::new(&items, &theme),
        rows[1],
        &mut ListState::new(state.list_state.selected),
    );
}

#[cfg(test)]
mod tests;
