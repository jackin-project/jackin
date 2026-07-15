// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Modal picker for "open in GitHub" when a workspace has multiple
//! GitHub-hosted mounts.
//!
//! Mirrors `WorkdirPickState`'s shape — one `Vec`-driven list +
//! `tui_widget_list::ListState` — so the rest of the launch TUI can
//! dispatch it with the same Up/Down/Enter pattern.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use crate::github_mounts::GithubChoice;
use jackin_core::shorten_home;
use termrock::ModalOutcome;

#[derive(Debug)]
pub struct GithubPickerState {
    pub choices: Vec<GithubChoice>,
    pub list_state: ListState,
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

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        crate::tui::components::list_helpers::scroll_select(
            &mut self.list_state,
            self.choices.len(),
            delta,
        )
    }
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use termrock::components::render_picker_lines;
use termrock::components::{DialogBorder, render_dialog_shell};
use termrock::style::{PHOSPHOR_DIM, WHITE};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &GithubPickerState) {
    let inner = render_dialog_shell(frame, area, Some("Open in GitHub"), DialogBorder::Default);

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
                termrock::style::DIM,
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

    let lines: Vec<Line<'_>> = state
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let display = &displays[i];
            let pad = path_w.saturating_sub(display.chars().count());
            Line::from(vec![
                Span::styled(display.to_owned(), Style::default().fg(WHITE)),
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

    render_picker_lines(
        rows[1],
        frame.buffer_mut(),
        lines,
        state.list_state.selected,
    );
}

#[cfg(test)]
mod tests;
