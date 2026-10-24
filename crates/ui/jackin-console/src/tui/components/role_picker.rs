// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crossterm::event::{KeyCode, KeyEvent};
use jackin_oppicker::ModalOutcome;
use termrock::widgets::ListState;

pub trait RoleChoice: Clone {
    fn key(&self) -> String;
}

#[derive(Debug)]
pub struct RolePickerState<R: RoleChoice> {
    pub roles: Vec<R>,
    pub list_state: ListState<usize>,
    pub filter: String,
    pub filtered: Vec<R>,
    pub confirm_label: String,
}

impl<R: RoleChoice> RolePickerState<R> {
    #[must_use]
    pub fn new(roles: Vec<R>) -> Self {
        Self::with_confirm_label(roles, "select")
    }

    #[must_use]
    pub fn launch(roles: Vec<R>) -> Self {
        Self::with_confirm_label(roles, "launch")
    }

    #[must_use]
    pub fn with_confirm_label(roles: Vec<R>, confirm_label: &str) -> Self {
        let filtered = roles.clone();
        let list_state = ListState::for_count(filtered.len());
        Self {
            roles,
            list_state,
            filter: String::new(),
            filtered,
            confirm_label: confirm_label.to_owned(),
        }
    }

    fn recompute_filtered(&mut self) {
        self.filtered = self
            .roles
            .iter()
            .filter(|role| {
                self.filter.is_empty()
                    || role
                        .key()
                        .to_lowercase()
                        .contains(&self.filter.to_lowercase())
            })
            .cloned()
            .collect();
        self.list_state = ListState::for_count(self.filtered.len());
    }

    fn move_up(&mut self) {
        self.list_state.cycle_index(self.filtered.len(), -1);
    }

    fn move_down(&mut self) {
        self.list_state.cycle_index(self.filtered.len(), 1);
    }

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        self.list_state
            .move_index(self.filtered.len(), isize::from(delta))
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<R> {
        match key.code {
            KeyCode::Up => {
                self.move_up();
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                self.move_down();
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.recompute_filtered();
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(role) = self.list_state.selected_item(&self.filtered) {
                    return ModalOutcome::Commit(role.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char(ch) => {
                self.filter.push(ch);
                self.recompute_filtered();
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }
}

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::StatefulWidget,
};

use termrock::layout::render_dialog_shell;
use termrock::widgets::PanelChrome;
use termrock::widgets::{List, ListRow, Panel, TextInput, TextInputState, Validation};

const ROLE_PICKER_TITLE: &str = "Select Role";

fn role_picker_inner_area(area: Rect) -> Rect {
    let theme = termrock::style::DesignSystem::default();
    Panel::new(&theme)
        .emphasis(PanelChrome::Focused)
        .title(ROLE_PICKER_TITLE)
        .block()
        .inner(area)
}

fn role_picker_layout(inner: Rect) -> (Rect, Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    (rows[0], rows[1], rows[2])
}

fn role_picker_rows_area(inner: Rect) -> Rect {
    role_picker_layout(inner).2
}

fn role_picker_rows<R: RoleChoice>(
    state: &RolePickerState<R>,
    theme: &termrock::style::DesignSystem,
) -> Vec<ListRow<'static, usize>> {
    state
        .filtered
        .iter()
        .enumerate()
        .map(|(id, role)| {
            ListRow::item(
                id,
                Line::from(vec![Span::styled(
                    role.key(),
                    Style::default().fg(theme
                        .style(termrock::style::Role::Text)
                        .fg
                        .unwrap_or_default()),
                )]),
            )
        })
        .collect()
}

pub(in crate::tui) fn selection_hit_at<R: RoleChoice>(
    area: Rect,
    state: &RolePickerState<R>,
    position: Position,
) -> bool {
    if area.width == 0 || area.height == 0 || state.filtered.is_empty() {
        return false;
    }

    let list_area = role_picker_rows_area(role_picker_inner_area(area));
    if list_area.width == 0 || list_area.height == 0 {
        return false;
    }

    let theme = termrock::style::DesignSystem::default();
    let rows = role_picker_rows(state, &theme);
    let mut list_state = ListState::new(state.list_state.selected().copied());
    let mut buffer = Buffer::empty(list_area);
    (&List::new(&rows, &theme)).render(list_area, &mut buffer, &mut list_state);

    list_state
        .regions()
        .iter()
        .any(|region| region.area.contains(position))
}

pub fn render<R: RoleChoice>(frame: &mut Frame<'_>, area: Rect, state: &RolePickerState<R>) {
    let theme = termrock::style::DesignSystem::default();
    let inner = render_dialog_shell(
        frame,
        area,
        Some(ROLE_PICKER_TITLE),
        PanelChrome::Focused,
        &theme,
    );

    let (filter_area, _spacer_area, list_area) = role_picker_layout(inner);

    let mut filter = TextInputState::new(&state.filter).with_allow_empty(true);
    let filter_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(7), Constraint::Min(1)])
        .split(filter_area);
    frame.render_widget(
        ratatui::widgets::Paragraph::new("Filter: "),
        filter_columns[0],
    );
    frame.render_stateful_widget(
        &TextInput::new("Filter", &theme)
            .placeholder("░░░")
            .validation(Validation::Valid),
        filter_columns[1],
        &mut filter,
    );

    if state.filtered.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "no matches",
                termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            )))
            .alignment(ratatui::layout::Alignment::Center),
            list_area,
        );
        return;
    }
    let items = role_picker_rows(state, &theme);
    frame.render_stateful_widget(
        &List::new(&items, &theme),
        list_area,
        &mut ListState::new(state.list_state.selected().copied()),
    );
}

#[cfg(test)]
mod tests;
