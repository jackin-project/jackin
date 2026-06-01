//! Single-line text-input dialog component.

use std::marker::PhantomData;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::ansi::{
    BG_DARK, BOLD, INVERSE, RESET, bg, fg, move_to,
};
use crate::theme::{DANGER_RED, INPUT_BG_DIM, PHOSPHOR_GREEN, WHITE};
use crate::{
    INPUT_BG_DIM as INPUT_BG_DIM_RGB, PHOSPHOR_GREEN as PHOSPHOR_GREEN_RGB, WHITE as WHITE_RGB,
};
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
            BorderStyle::Default => PHOSPHOR_GREEN,
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

/// Centred raw-ANSI text-input dialog matching the Ratatui text input widget.
pub fn render_text_input_dialog(
    buf: &mut Vec<u8>,
    term_rows: u16,
    term_cols: u16,
    label: &str,
    value: &str,
    cursor_byte: usize,
) -> TextInputDialogRect {
    let width = (term_cols * 60 / 100).clamp(40, 100);
    let height: u16 = 5;
    let row = term_rows.saturating_sub(height) / 2;
    let col = term_cols.saturating_sub(width) / 2;

    move_to(buf, row, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    buf.extend_from_slice("┌─ ".as_bytes());
    fg(buf, WHITE_RGB);
    buf.extend_from_slice(BOLD.as_bytes());
    buf.extend_from_slice(label.as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
    buf.extend_from_slice(BG_DARK.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    buf.push(b' ');
    let consumed = 3 + label.chars().count() as u16 + 1;
    for _ in consumed..(width - 1) {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice("┐".as_bytes());

    move_to(buf, row + 1, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    buf.extend_from_slice("│".as_bytes());
    for _ in 1..(width - 1) {
        buf.push(b' ');
    }
    buf.extend_from_slice("│".as_bytes());

    move_to(buf, row + 2, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    buf.extend_from_slice("│".as_bytes());
    buf.push(b' ');
    bg(buf, INPUT_BG_DIM_RGB);
    let band_cols = (width as usize).saturating_sub(4);
    for _ in 0..band_cols {
        buf.push(b' ');
    }

    move_to(buf, row + 2, col + 2);
    bg(buf, INPUT_BG_DIM_RGB);
    fg(buf, WHITE_RGB);
    let cursor_byte = cursor_byte.min(value.len());
    let (before, after) = value.split_at(cursor_byte);
    buf.extend_from_slice(before.as_bytes());
    buf.extend_from_slice(INVERSE.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    if let Some(c) = after.chars().next() {
        let mut b = [0u8; 4];
        let s = c.encode_utf8(&mut b);
        buf.extend_from_slice(s.as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
        buf.extend_from_slice(BG_DARK.as_bytes());
        bg(buf, INPUT_BG_DIM_RGB);
        fg(buf, WHITE_RGB);
        let tail = &after[c.len_utf8()..];
        buf.extend_from_slice(tail.as_bytes());
    } else {
        buf.push(b' ');
        buf.extend_from_slice(RESET.as_bytes());
        buf.extend_from_slice(BG_DARK.as_bytes());
        bg(buf, INPUT_BG_DIM_RGB);
    }
    buf.extend_from_slice(RESET.as_bytes());
    buf.extend_from_slice(BG_DARK.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    move_to(buf, row + 2, col + width - 2);
    buf.push(b' ');
    buf.extend_from_slice("│".as_bytes());

    move_to(buf, row + 3, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    buf.extend_from_slice("│".as_bytes());
    for _ in 1..(width - 1) {
        buf.push(b' ');
    }
    buf.extend_from_slice("│".as_bytes());

    move_to(buf, row + height - 1, col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    fg(buf, PHOSPHOR_GREEN_RGB);
    buf.extend_from_slice("└".as_bytes());
    for _ in 1..(width - 1) {
        buf.extend_from_slice("─".as_bytes());
    }
    buf.extend_from_slice("┘".as_bytes());
    buf.extend_from_slice(RESET.as_bytes());

    TextInputDialogRect {
        row,
        col,
        width,
        height,
    }
}

/// Returned by `render_text_input_dialog` so callers can hit-test clicks.
#[derive(Debug, Clone, Copy)]
pub struct TextInputDialogRect {
    pub row: u16,
    pub col: u16,
    pub width: u16,
    pub height: u16,
}
