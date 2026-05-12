//! Workspace-vs-specific-role choice for the Secrets-tab Add flow.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeChoice {
    AllAgents,
    SpecificAgent,
}

#[derive(Debug, Clone)]
pub struct ScopePickerState {
    pub focused: ScopeChoice,
    pub title: &'static str,
}

impl Default for ScopePickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopePickerState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focused: ScopeChoice::AllAgents,
            title: " New environment variable ",
        }
    }

    #[must_use]
    pub const fn with_title(title: &'static str) -> Self {
        Self {
            focused: ScopeChoice::AllAgents,
            title,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<ScopeChoice> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Right
            | KeyCode::Left
            | KeyCode::Char('l' | 'L' | 'h' | 'H') => {
                self.cycle();
                ModalOutcome::Continue
            }
            KeyCode::Enter => ModalOutcome::Commit(self.focused),
            _ => ModalOutcome::Continue,
        }
    }

    const fn cycle(&mut self) {
        self.focused = match self.focused {
            ScopeChoice::AllAgents => ScopeChoice::SpecificAgent,
            ScopeChoice::SpecificAgent => ScopeChoice::AllAgents,
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

pub fn render(frame: &mut Frame, area: Rect, state: &ScopePickerState) {
    let phosphor = Color::Rgb(0, 255, 65);
    let phosphor_dark = Color::Rgb(0, 80, 18);
    let white = Color::Rgb(255, 255, 255);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(phosphor_dark))
        .title(Span::styled(
            state.title,
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let focused_style = Style::default()
        .bg(white)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default().fg(phosphor).add_modifier(Modifier::BOLD);

    let all_style = if state.focused == ScopeChoice::AllAgents {
        focused_style
    } else {
        unfocused_style
    };
    let specific_style = if state.focused == ScopeChoice::SpecificAgent {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  All roles  ", all_style),
        Span::raw("    "),
        Span::styled("  Specific role  ", specific_style),
    ]);
    // inner area is 3 rows (5 outer − 2 border): blank, button, blank.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top blank
            Constraint::Length(1), // button
            Constraint::Length(1), // bottom blank
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[1],
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
    fn scope_picker_default_focus_is_all_agents() {
        let s = ScopePickerState::new();
        assert_eq!(s.focused, ScopeChoice::AllAgents);
    }

    #[test]
    fn scope_picker_right_arrow_advances_to_specific() {
        let mut s = ScopePickerState::new();
        let _ = s.handle_key(key_event(KeyCode::Right));
        assert_eq!(s.focused, ScopeChoice::SpecificAgent);
    }

    #[test]
    fn scope_picker_enter_on_all_commits_all() {
        let mut s = ScopePickerState::new();
        assert!(matches!(
            s.handle_key(key_event(KeyCode::Enter)),
            ModalOutcome::Commit(ScopeChoice::AllAgents)
        ));
    }

    #[test]
    fn scope_picker_enter_on_specific_commits_specific() {
        let mut s = ScopePickerState::new();
        let _ = s.handle_key(key_event(KeyCode::Right));
        assert_eq!(s.focused, ScopeChoice::SpecificAgent);
        assert!(matches!(
            s.handle_key(key_event(KeyCode::Enter)),
            ModalOutcome::Commit(ScopeChoice::SpecificAgent)
        ));
    }

    #[test]
    fn scope_picker_esc_cancels() {
        let mut s = ScopePickerState::new();
        assert!(matches!(
            s.handle_key(key_event(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn scope_picker_left_arrow_toggles_back_to_all_agents() {
        let mut s = ScopePickerState::new();
        let _ = s.handle_key(key_event(KeyCode::Right));
        assert_eq!(s.focused, ScopeChoice::SpecificAgent);
        let _ = s.handle_key(key_event(KeyCode::Left));
        assert_eq!(s.focused, ScopeChoice::AllAgents);
    }
}
