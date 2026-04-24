//! Three-way confirmation: Save and leave / Discard and leave / Cancel.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardChoice {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardFocus {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct SaveDiscardState {
    pub prompt: String,
    pub focus: SaveDiscardFocus,
}

impl SaveDiscardState {
    /// Default focus = Cancel (safest for accidental Enter).
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            focus: SaveDiscardFocus::Cancel,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveDiscardChoice> {
        match key.code {
            KeyCode::Char('s' | 'S') => ModalOutcome::Commit(SaveDiscardChoice::Save),
            KeyCode::Char('d' | 'D') => ModalOutcome::Commit(SaveDiscardChoice::Discard),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Discard,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Save,
                };
                ModalOutcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Save,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Discard,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => match self.focus {
                SaveDiscardFocus::Save => ModalOutcome::Commit(SaveDiscardChoice::Save),
                SaveDiscardFocus::Discard => ModalOutcome::Commit(SaveDiscardChoice::Discard),
                SaveDiscardFocus::Cancel => ModalOutcome::Cancel,
            },
            _ => ModalOutcome::Continue,
        }
    }
}

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render(frame: &mut Frame, area: Rect, state: &SaveDiscardState) {
    let phosphor = Color::Rgb(0, 255, 65);
    let white = Color::Rgb(255, 255, 255);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(phosphor))
        .title(Span::styled(
            " Unsaved changes ",
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // prompt
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Prompt
    frame.render_widget(
        Paragraph::new(Span::styled(
            state.prompt.clone(),
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        chunks[0],
    );

    // Buttons
    let focused_style = Style::default()
        .bg(white)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default()
        .bg(phosphor)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let save_style = if state.focus == SaveDiscardFocus::Save {
        focused_style
    } else {
        unfocused_style
    };
    let discard_style = if state.focus == SaveDiscardFocus::Discard {
        focused_style
    } else {
        unfocused_style
    };
    let cancel_style = if state.focus == SaveDiscardFocus::Cancel {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  Save  ", save_style),
        Span::raw("    "),
        Span::styled("  Discard  ", discard_style),
        Span::raw("    "),
        Span::styled("  Cancel  ", cancel_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[2],
    );

    // Hint — same key/text/sep scheme as the main TUI footer.
    let key_style = Style::default().fg(white).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(phosphor);
    let sep_style = Style::default().fg(Color::Rgb(0, 80, 18));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Tab", key_style),
            Span::styled(" cycle", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("Enter", key_style),
            Span::styled(" commit", text_style),
            Span::raw("   "),
            Span::styled("S", key_style),
            Span::styled(" save", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("D", key_style),
            Span::styled(" discard", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("C/Esc", key_style),
            Span::styled(" cancel", text_style),
        ]))
        .alignment(Alignment::Center),
        chunks[4],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn default_focus_is_cancel() {
        let s = SaveDiscardState::new("?");
        assert_eq!(s.focus, SaveDiscardFocus::Cancel);
    }

    #[test]
    fn s_commits_save() {
        let mut s = SaveDiscardState::new("?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('s'))),
            ModalOutcome::Commit(SaveDiscardChoice::Save)
        ));
    }

    #[test]
    fn d_commits_discard() {
        let mut s = SaveDiscardState::new("?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('d'))),
            ModalOutcome::Commit(SaveDiscardChoice::Discard)
        ));
    }

    #[test]
    fn esc_cancels() {
        let mut s = SaveDiscardState::new("?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn enter_commits_focused_button() {
        let mut s = SaveDiscardState::new("?");
        s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(SaveDiscardChoice::Save)
        ));

        let mut s = SaveDiscardState::new("?");
        s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
        s.handle_key(key(KeyCode::Tab)); // Save -> Discard
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(SaveDiscardChoice::Discard)
        ));

        let mut s = SaveDiscardState::new("?");
        // Focus is Cancel — Enter cancels.
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Cancel
        ));
    }
}
