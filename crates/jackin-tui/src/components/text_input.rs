//! Single-line text-input dialog component.

use std::marker::PhantomData;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::ModalOutcome;
use crate::centered_rect;
use crate::theme::{DANGER_RED, INPUT_BG_DIM, PHOSPHOR_GREEN, WHITE};

/// Cross-surface single-line text-input model. Holds the buffer,
/// cursor position (in bytes), an optional max length, and an
/// optional forbidden set used for duplicate detection.
#[derive(Debug, Clone)]
pub struct TextField {
    value: String,
    cursor: usize,
    max_chars: Option<usize>,
    forbidden: Vec<String>,
    allow_empty: bool,
}

impl Default for TextField {
    fn default() -> Self {
        Self::new("")
    }
}

impl TextField {
    #[must_use]
    pub fn new(initial: impl Into<String>) -> Self {
        let value: String = initial.into();
        let cursor = value.len();
        Self {
            value,
            cursor,
            max_chars: None,
            forbidden: Vec::new(),
            allow_empty: false,
        }
    }

    #[must_use]
    pub fn with_max_chars(mut self, n: usize) -> Self {
        self.max_chars = Some(n);
        self
    }

    #[must_use]
    pub fn with_forbidden(mut self, forbidden: Vec<String>) -> Self {
        self.forbidden = forbidden;
        self
    }

    #[must_use]
    pub fn with_allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn trimmed_value(&self) -> String {
        self.value.trim().to_owned()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.value[..self.cursor]
            .char_indices()
            .last()
            .map_or(0, |(idx, _)| idx);
        self.cursor = prev;
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = self.value[self.cursor..]
            .chars()
            .next()
            .map_or(self.value.len(), |ch| self.cursor + ch.len_utf8());
        self.cursor = next;
    }

    pub fn move_cursor_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.value.len();
    }

    pub fn len_chars(&self) -> usize {
        self.value.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn insert_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        if self.max_chars.is_some_and(|max| self.len_chars() >= max) {
            return;
        }
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.value[..self.cursor]
            .char_indices()
            .last()
            .map_or(0, |(idx, _)| idx);
        self.value.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn delete_char(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = self.value[self.cursor..]
            .chars()
            .next()
            .map_or(self.value.len(), |ch| self.cursor + ch.len_utf8());
        self.value.replace_range(self.cursor..next, "");
    }

    pub fn is_duplicate(&self) -> bool {
        let v = self.trimmed_value();
        !v.is_empty() && self.forbidden.iter().any(|f| f == &v)
    }

    pub fn is_valid(&self) -> bool {
        let v = self.trimmed_value();
        let empty_ok = self.allow_empty || !v.is_empty();
        empty_ok && !self.forbidden.iter().any(|f| f == &v)
    }
}

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
        self.field.value().to_owned()
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

#[derive(Debug)]
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

        let title = Span::styled(format!(" {} ", self.state.label), crate::theme::BOLD_WHITE);
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
    let base_style = crate::theme::GREEN.bg(INPUT_BG_DIM);
    let cursor_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::SLOW_BLINK);
    let mut spans = vec![Span::styled(before.to_owned(), base_style)];
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

/// Canonical outer rectangle for one-label text-input prompts.
///
/// Launch currently owns the only variable-width prompt surface, so this helper
/// preserves that 60%-of-content sizing while moving the geometry into the
/// shared component next to the renderer. Console modal rects intentionally keep
/// their fixed 160-column reference sizing until that modal layer is migrated.
#[must_use]
pub fn text_input_prompt_rect(area: Rect) -> Rect {
    let min_w = 50.min(area.width);
    let width = (area.width.saturating_mul(3) / 5).clamp(min_w, area.width.max(min_w));
    centered_rect(width, 5, area)
}

/// Render a focused modal text-input dialog with a distinct dialog title and
/// field label. Use this when the dialog title ("Rename tab") is not the same
/// text as the editable field label ("Name").
pub fn render_labeled_text_input_dialog(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    dialog_title: &str,
    label: &str,
    value: &str,
    cursor: usize,
) {
    let title = format!(" {dialog_title} ");
    let block = crate::components::Panel::new()
        .title(&title)
        .focus(crate::components::PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    Clear.render(area, frame.buffer_mut());
    block.render(area, frame.buffer_mut());

    if inner.height < 3 {
        return;
    }

    let label_area = Rect { height: 1, ..inner };
    frame.render_widget(
        Paragraph::new(Span::styled(format!("{label}: "), crate::theme::BOLD_WHITE)),
        label_area,
    );

    let input_area = Rect {
        y: inner.y + 2,
        height: 1,
        ..inner
    };
    render_input_value_from_parts(input_area, frame.buffer_mut(), value, cursor);
}

fn render_input_value_from_parts(area: Rect, buf: &mut Buffer, value: &str, cursor: usize) {
    let cursor = cursor.min(value.len());
    let (before, after) = value.split_at(cursor);
    let cursor_style = Style::default()
        .fg(Color::Black)
        .bg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::styled(before.to_owned(), crate::theme::GREEN)];
    if let Some(ch) = after.chars().next() {
        spans.push(Span::styled(ch.to_string(), cursor_style));
        spans.push(Span::styled(
            after[ch.len_utf8()..].to_owned(),
            crate::theme::DIM,
        ));
    } else {
        spans.push(Span::styled(" ", cursor_style));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    #[test]
    fn labeled_text_input_dialog_renders_shared_shell_and_cursor() {
        let backend = TestBackend::new(40, 7);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_labeled_text_input_dialog(
                    frame,
                    Rect::new(2, 1, 30, 5),
                    "Rename tab",
                    "Name",
                    "alpha",
                    2,
                );
            })
            .unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Rename tab"));
        assert!(rendered.contains("Name:"));
        assert!(rendered.contains("alpha"));
        let buf = terminal.backend().buffer();
        let cursor_cell = (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .find(|(x, y)| {
                let cell = &buf[(*x, *y)];
                cell.symbol() == "p" && cell.style().add_modifier.contains(Modifier::BOLD)
            });
        assert!(
            cursor_cell.is_some(),
            "cursor cell should use the shared bold inverse style"
        );
    }

    #[test]
    fn text_input_prompt_rect_matches_launch_prompt_shape() {
        let area = Rect::new(0, 0, 120, 30);
        let rect = text_input_prompt_rect(area);
        assert_eq!(rect.width, 72);
        assert_eq!(rect.height, 5);
        assert_eq!(rect.x, 24);
        assert_eq!(rect.y, 12);
    }
}
