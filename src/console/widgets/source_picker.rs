//! Plain-or-1Password choice between `EnvKey` input and value entry.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceChoice {
    Plain,
    Op,
}

#[derive(Debug, Clone)]
pub struct SourcePickerState {
    pub key: String,
    /// Captured from `ConsoleState::op_available` (probed once at
    /// startup); operator must restart to pick up a mid-session
    /// install. When `false`, the Op button renders dim and `←`/`→`
    /// skip it.
    pub op_available: bool,
    pub focused: SourceChoice,
}

impl SourcePickerState {
    #[must_use]
    pub const fn new(key: String, op_available: bool) -> Self {
        Self {
            key,
            op_available,
            focused: SourceChoice::Plain,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SourceChoice> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char('p' | 'P') => ModalOutcome::Commit(SourceChoice::Plain),
            KeyCode::Char('o' | 'O') if self.op_available => ModalOutcome::Commit(SourceChoice::Op),
            KeyCode::Tab
            | KeyCode::Right
            | KeyCode::Left
            | KeyCode::Char('l' | 'L' | 'h' | 'H') => {
                self.cycle();
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                // Defensive: `cycle` never parks focus on disabled Op,
                // but refuse to commit if a future code path does.
                if self.focused == SourceChoice::Op && !self.op_available {
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Commit(self.focused)
            }
            _ => ModalOutcome::Continue,
        }
    }

    const fn cycle(&mut self) {
        if !self.op_available {
            return;
        }
        self.focused = match self.focused {
            SourceChoice::Plain => SourceChoice::Op,
            SourceChoice::Op => SourceChoice::Plain,
        };
    }
}

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render(frame: &mut Frame, area: Rect, state: &SourcePickerState) {
    let phosphor = Color::Rgb(0, 255, 65);
    let phosphor_dark = Color::Rgb(0, 80, 18);
    let white = Color::Rgb(255, 255, 255);

    let title = format!(" Source for {} ", state.key);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(phosphor_dark))
        .title(Span::styled(
            title,
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let focused_style = Style::default()
        .bg(white)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default().fg(phosphor).add_modifier(Modifier::BOLD);
    let disabled_style = Style::default()
        .fg(phosphor_dark)
        .add_modifier(Modifier::DIM);

    let plain_style = if state.focused == SourceChoice::Plain {
        focused_style
    } else {
        unfocused_style
    };
    let op_style = if !state.op_available {
        disabled_style
    } else if state.focused == SourceChoice::Op {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  Plain text  ", plain_style),
        Span::raw("    "),
        Span::styled("  1Password  ", op_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[1],
    );

    if !state.op_available {
        frame.render_widget(
            Paragraph::new(Span::styled("(install op CLI to enable)", disabled_style))
                .alignment(Alignment::Center),
            chunks[2],
        );
    }

    let key_style = Style::default().fg(white).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(phosphor);
    let sep_style = Style::default().fg(phosphor_dark);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("\u{2190}/\u{2192}", key_style),
            Span::styled(" navigate", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("Enter", key_style),
            Span::styled(" select", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("Esc", key_style),
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

    const fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn source_picker_default_focus_is_plain() {
        let s = SourcePickerState::new("MY_KEY".into(), true);
        assert_eq!(s.focused, SourceChoice::Plain);
    }

    #[test]
    fn source_picker_right_arrow_advances_to_op_when_available() {
        let mut s = SourcePickerState::new("MY_KEY".into(), true);
        let _ = s.handle_key(key_event(KeyCode::Right));
        assert_eq!(s.focused, SourceChoice::Op);
    }

    #[test]
    fn source_picker_right_arrow_skips_op_when_unavailable() {
        let mut s = SourcePickerState::new("MY_KEY".into(), false);
        let _ = s.handle_key(key_event(KeyCode::Right));
        assert_eq!(
            s.focused,
            SourceChoice::Plain,
            "cycling must skip the disabled Op button when op is unavailable"
        );
        let _ = s.handle_key(key_event(KeyCode::Right));
        let _ = s.handle_key(key_event(KeyCode::Tab));
        let _ = s.handle_key(key_event(KeyCode::Char('l')));
        assert_eq!(s.focused, SourceChoice::Plain);
    }

    #[test]
    fn source_picker_enter_on_plain_commits_plain() {
        let mut s = SourcePickerState::new("MY_KEY".into(), true);
        assert!(matches!(
            s.handle_key(key_event(KeyCode::Enter)),
            ModalOutcome::Commit(SourceChoice::Plain)
        ));
    }

    #[test]
    fn source_picker_enter_on_op_when_available_commits_op() {
        let mut s = SourcePickerState::new("MY_KEY".into(), true);
        let _ = s.handle_key(key_event(KeyCode::Right));
        assert_eq!(s.focused, SourceChoice::Op);
        assert!(matches!(
            s.handle_key(key_event(KeyCode::Enter)),
            ModalOutcome::Commit(SourceChoice::Op)
        ));
    }

    #[test]
    fn source_picker_esc_returns_cancel() {
        let mut s = SourcePickerState::new("MY_KEY".into(), true);
        assert!(matches!(
            s.handle_key(key_event(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn source_picker_o_hotkey_inert_when_op_unavailable() {
        let mut s = SourcePickerState::new("MY_KEY".into(), false);
        assert!(matches!(
            s.handle_key(key_event(KeyCode::Char('O'))),
            ModalOutcome::Continue
        ));
        assert_eq!(s.focused, SourceChoice::Plain);
    }
}
