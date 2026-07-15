// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Modal picker for role disambiguation.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use jackin_tui::ModalOutcome;

pub trait RoleChoice: Clone {
    fn key(&self) -> String;
}

#[derive(Debug)]
pub struct RolePickerState<R: RoleChoice> {
    pub roles: Vec<R>,
    pub list_state: ListState,
    pub filter: String,
    pub filtered: Vec<R>,
    /// Verb after `Enter` in the footer (`launch` for launch
    /// disambiguation, `select` for editor override-scope picking).
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
        let list_state = crate::tui::components::list_helpers::list_state_for_count(filtered.len());
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
                crate::tui::components::list_helpers::matches_filter(
                    &self.filter,
                    [role.key().as_str()],
                )
            })
            .cloned()
            .collect();
        self.list_state
            .select(crate::tui::components::list_helpers::first_selection(
                self.filtered.len(),
            ));
    }

    fn move_up(&mut self) {
        crate::tui::components::list_helpers::cycle_select(
            &mut self.list_state,
            self.filtered.len(),
            -1,
        );
    }

    fn move_down(&mut self) {
        crate::tui::components::list_helpers::cycle_select(
            &mut self.list_state,
            self.filtered.len(),
            1,
        );
    }

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        crate::tui::components::list_helpers::scroll_select(
            &mut self.list_state,
            self.filtered.len(),
            delta,
        )
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
                if let Some(role) = crate::tui::components::list_helpers::selected_choice(
                    &self.filtered,
                    self.list_state.selected,
                ) {
                    return ModalOutcome::Commit(role.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char(ch) => {
                // Every printable char goes to the filter — `j`/`k`
                // included; navigation is via arrow keys.
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
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
};

use jackin_tui::components::render_filter_input;
use jackin_tui::components::render_picker_lines;
use jackin_tui::components::{DialogBorder, render_dialog_shell};
use termrock::style::WHITE;

pub fn render<R: RoleChoice>(frame: &mut Frame<'_>, area: Rect, state: &RolePickerState<R>) {
    let inner = render_dialog_shell(frame, area, Some("Select Role"), DialogBorder::Default);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // filter row
            Constraint::Length(1), // spacer
            Constraint::Min(1),    // list
        ])
        .split(inner);

    render_filter_input(frame, rows[0], &state.filter);

    // List body. When the filter narrows the visible set to nothing, show
    // a dim centered placeholder so the operator knows the list is empty,
    // not broken.
    if state.filtered.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "no matches",
                termrock::style::DIM,
            )))
            .alignment(ratatui::layout::Alignment::Center),
            rows[2],
        );
        return;
    }
    let lines: Vec<Line<'_>> = state
        .filtered
        .iter()
        .map(|role| Line::from(vec![Span::styled(role.key(), Style::default().fg(WHITE))]))
        .collect();
    render_picker_lines(
        rows[2],
        frame.buffer_mut(),
        lines,
        state.list_state.selected,
    );
}

#[cfg(test)]
mod tests;
