// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::github_mounts::GithubChoice;
use crossterm::event::{KeyCode, KeyEvent};
use jackin_core::shorten_home;
use jackin_oppicker::ModalOutcome;
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
    Pick(Box<GithubPickerState>),
}

#[must_use]
pub fn github_open_plan(choices: Vec<GithubChoice>) -> GithubOpenPlan {
    match choices.len() {
        0 => GithubOpenPlan::Continue,
        1 => GithubOpenPlan::OpenUrl(choices[0].url.clone()),
        _ => GithubOpenPlan::Pick(Box::new(GithubPickerState::new(choices))),
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
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::StatefulWidget,
};

use termrock::layout::render_dialog_shell;
use termrock::widgets::{List, ListRow, Panel, PanelChrome};

const GITHUB_PICKER_TITLE: &str = "Open in GitHub";

fn picker_inner_area(area: Rect) -> Rect {
    let theme = termrock::style::DesignSystem::default();
    Panel::new(&theme)
        .emphasis(PanelChrome::Focused)
        .title(GITHUB_PICKER_TITLE)
        .block()
        .inner(area)
}

fn picker_list_area(inner: Rect) -> Rect {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    rows[1]
}

fn picker_rows(state: &GithubPickerState) -> Vec<ListRow<'_, usize>> {
    let displays: Vec<String> = state
        .choices
        .iter()
        .map(|choice| shorten_home(&choice.src))
        .collect();
    let path_width = displays
        .iter()
        .map(|display| display.chars().count())
        .max()
        .unwrap_or(0)
        .max(10);

    state
        .choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let display = &displays[index];
            let pad = path_width.saturating_sub(display.chars().count());
            ListRow::item(
                index,
                Line::from(vec![
                    Span::styled(
                        display.to_owned(),
                        Style::default().fg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Text)
                            .fg
                            .unwrap_or_default()),
                    ),
                    Span::raw(format!("{}  ", " ".repeat(pad))),
                    Span::styled(
                        format!("github \u{b7} {}", choice.branch),
                        Style::default()
                            .fg(termrock::style::DesignSystem::default()
                                .style(termrock::style::Role::TextMuted)
                                .fg
                                .unwrap_or_default())
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]),
            )
        })
        .collect()
}

pub(in crate::tui) fn selection_hit_at(
    area: Rect,
    state: &GithubPickerState,
    position: Position,
) -> bool {
    if area.width == 0 || area.height == 0 || state.choices.is_empty() {
        return false;
    }

    let list_area = picker_list_area(picker_inner_area(area));
    if list_area.width == 0 || list_area.height == 0 {
        return false;
    }

    let theme = termrock::style::DesignSystem::default();
    let rows = picker_rows(state);
    let mut list_state = ListState::new(state.list_state.selected().copied());
    let mut buffer = Buffer::empty(list_area);
    (&List::new(&rows, &theme)).render(list_area, &mut buffer, &mut list_state);

    list_state
        .regions()
        .iter()
        .any(|region| region.area.contains(position))
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &GithubPickerState) {
    let theme = termrock::style::DesignSystem::default();
    let inner = render_dialog_shell(
        frame,
        area,
        Some(GITHUB_PICKER_TITLE),
        PanelChrome::Focused,
        &theme,
    );
    let list_area = picker_list_area(inner);

    if state.choices.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "no GitHub sources",
                theme.style(termrock::style::Role::TextMuted),
            )))
            .alignment(ratatui::layout::Alignment::Center),
            list_area,
        );
        return;
    }
    let items = picker_rows(state);
    frame.render_stateful_widget(
        &List::new(&items, &theme),
        list_area,
        &mut ListState::new(state.list_state.selected().copied()),
    );
}

#[cfg(test)]
mod tests;
