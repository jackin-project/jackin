//! Single-line text-input dialog component.

use std::marker::PhantomData;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::theme::{DANGER_RED, INPUT_BG_DIM, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};
use crate::{ModalOutcome, TextField};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    Default,
    Error,
}

#[derive(Clone)]
pub struct TextInputState<'a> {
    pub label: String,
    field: TextField,
    pub forbidden_label: String,
    _marker: PhantomData<&'a ()>,
}

impl std::fmt::Debug for TextInputState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputState")
            .field("label", &self.label)
            .field("value", &self.field.value())
            .field("forbidden_label", &self.forbidden_label)
            .finish()
    }
}

impl TextInputState<'_> {
    #[must_use]
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        Self::new_with_forbidden(label, initial, Vec::new())
    }

    #[must_use]
    pub fn new_allow_empty(label: impl Into<String>, initial: impl Into<String>) -> Self {
        let label = label.into();
        let initial = initial.into();
        Self {
            label,
            field: TextField::new(initial).with_allow_empty(true),
            forbidden_label: String::new(),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn new_with_forbidden(
        label: impl Into<String>,
        initial: impl Into<String>,
        forbidden: Vec<String>,
    ) -> Self {
        Self {
            label: label.into(),
            field: TextField::new(initial).with_forbidden(forbidden),
            forbidden_label: String::new(),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn value(&self) -> String {
        self.field.value().to_string()
    }

    #[must_use]
    pub fn trimmed_value(&self) -> String {
        self.field.trimmed_value()
    }

    #[must_use]
    pub fn is_duplicate(&self) -> bool {
        self.field.is_duplicate()
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.field.is_valid()
    }

    #[must_use]
    pub fn border_style(&self) -> BorderStyle {
        if self.is_duplicate() {
            BorderStyle::Error
        } else {
            BorderStyle::Default
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Enter => {
                if !self.is_valid() {
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Commit(self.value())
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Backspace => {
                self.field.backspace();
                ModalOutcome::Continue
            }
            KeyCode::Delete => {
                self.field.delete_char();
                ModalOutcome::Continue
            }
            KeyCode::Left => {
                self.field.move_cursor_left();
                ModalOutcome::Continue
            }
            KeyCode::Right => {
                self.field.move_cursor_right();
                ModalOutcome::Continue
            }
            KeyCode::Home => {
                self.field.move_cursor_to_start();
                ModalOutcome::Continue
            }
            KeyCode::End => {
                self.field.move_cursor_to_end();
                ModalOutcome::Continue
            }
            KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ModalOutcome::Continue
            }
            KeyCode::Char(ch) => {
                self.field.insert_char(ch);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }
}

pub struct TextInput<'a> {
    state: &'a TextInputState<'a>,
}

impl<'a> TextInput<'a> {
    #[must_use]
    pub const fn new(state: &'a TextInputState<'a>) -> Self {
        Self { state }
    }
}

impl Widget for TextInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let title = Span::styled(
            format!(" {} ", self.state.label),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        );
        let border_color = match self.state.border_style() {
            BorderStyle::Error => DANGER_RED,
            BorderStyle::Default => PHOSPHOR_DARK,
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title);

        let inner = block.inner(area);
        block.render(area, buf);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let input_row = rows[1];
        let input_area = Rect {
            x: input_row.x.saturating_add(1),
            y: input_row.y,
            width: input_row.width.saturating_sub(2),
            height: input_row.height,
        };
        Block::default()
            .style(Style::default().bg(INPUT_BG_DIM))
            .render(input_area, buf);
        render_input_value(input_area, buf, self.state);

        if self.state.is_duplicate() {
            let key = self.state.trimmed_value();
            let message = if self.state.forbidden_label.is_empty() {
                format!("\u{26a0} \"{key}\" already exists")
            } else {
                format!(
                    "\u{26a0} \"{key}\" already exists in {}",
                    self.state.forbidden_label
                )
            };
            Paragraph::new(message)
                .style(
                    Style::default()
                        .fg(DANGER_RED)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                )
                .render(rows[2], buf);
        }
    }
}

fn render_input_value(area: Rect, buf: &mut Buffer, state: &TextInputState<'_>) {
    let value = state.field.value();
    let cursor = state.field.cursor().min(value.len());
    let (before, after) = value.split_at(cursor);
    let base_style = Style::default().fg(PHOSPHOR_GREEN).bg(INPUT_BG_DIM);
    let cursor_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::SLOW_BLINK);
    let mut spans = vec![Span::styled(before.to_string(), base_style)];
    if let Some(ch) = after.chars().next() {
        spans.push(Span::styled(ch.to_string(), cursor_style));
        spans.push(Span::styled(after[ch.len_utf8()..].to_string(), base_style));
    } else {
        spans.push(Span::styled(" ", cursor_style));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

pub fn render_text_input(frame: &mut ratatui::Frame<'_>, area: Rect, state: &TextInputState<'_>) {
    frame.render_widget(TextInput::new(state), area);
}
