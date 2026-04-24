//! Single-line text input modal — wraps ratatui-textarea.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{CursorMove, Input, TextArea};

use super::ModalOutcome;

pub struct TextInputState<'a> {
    pub label: String,
    pub textarea: TextArea<'a>,
}

impl std::fmt::Debug for TextInputState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputState")
            .field("label", &self.label)
            .finish()
    }
}

impl TextInputState<'_> {
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        let mut textarea = TextArea::new(vec![initial.into()]);
        // Position cursor at end of initial text so editing feels natural.
        textarea.move_cursor(CursorMove::End);
        Self {
            label: label.into(),
            textarea,
        }
    }

    pub fn value(&self) -> String {
        self.textarea.lines().first().cloned().unwrap_or_default()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Enter => ModalOutcome::Commit(self.value()),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => {
                // Swallow Ctrl+M, which textarea treats as newline.
                if key.code == KeyCode::Char('m') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return ModalOutcome::Continue;
                }
                let input: Input = key.into();
                self.textarea.input(input);
                ModalOutcome::Continue
            }
        }
    }
}

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const WHITE: Color = Color::Rgb(255, 255, 255);

pub fn render(frame: &mut Frame, area: Rect, state: &TextInputState) {
    use ratatui::{
        layout::{Constraint, Direction, Layout},
        text::Span,
        widgets::Paragraph,
    };

    frame.render_widget(ratatui::widgets::Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title(state.label.as_str());

    let mut ta = state.textarea.clone();
    ta.set_block(block);
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(
        Style::default()
            .bg(WHITE)
            .fg(Color::Black)
            .add_modifier(Modifier::SLOW_BLINK),
    );

    frame.render_widget(&ta, chunks[0]);

    // Footer legend — same key/text/sep scheme as the main TUI footer:
    //   Key      = WHITE + BOLD
    //   Text     = PHOSPHOR_GREEN
    //   Sep (·)  = PHOSPHOR_DARK
    let hint = ratatui::text::Line::from(vec![
        Span::styled(
            "Enter",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" confirm", Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled(" \u{b7} ", Style::default().fg(Color::Rgb(0, 80, 18))),
        Span::styled(
            "Esc",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(PHOSPHOR_GREEN)),
    ]);
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }
    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn initial_value_is_returned_on_enter() {
        let mut s = TextInputState::new("name", "my-app");
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "my-app"));
    }

    #[test]
    fn typing_appends_to_value() {
        let mut s = TextInputState::new("name", "");
        s.handle_key(key(KeyCode::Char('h')));
        s.handle_key(key(KeyCode::Char('i')));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "hi"));
    }

    #[test]
    fn backspace_removes_char() {
        let mut s = TextInputState::new("name", "abc");
        s.handle_key(key(KeyCode::Backspace));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "ab"));
    }

    #[test]
    fn esc_cancels() {
        let mut s = TextInputState::new("name", "abc");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn ctrl_m_does_not_insert_newline() {
        let mut s = TextInputState::new("name", "abc");
        s.handle_key(ctrl(KeyCode::Char('m')));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "abc"));
    }
}
