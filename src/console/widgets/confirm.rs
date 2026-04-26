//! Y/N confirmation modal with keyboard focus.
//!
//! Y / N / Esc return distinct outcomes; case-insensitive.
//! Tab / ←→ / h/l cycle focus between Yes and No.
//! Enter commits the focused button.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmFocus {
    Yes,
    No,
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub prompt: String,
    pub focus: ConfirmFocus,
}

impl ConfirmState {
    /// Build a new Confirm modal. Default focus = No (safer for
    /// destructive actions — Enter won't accidentally commit Yes).
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            focus: ConfirmFocus::No,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<bool> {
        match key.code {
            // Direct shortcuts (case-insensitive).
            KeyCode::Char('y' | 'Y') => ModalOutcome::Commit(true),
            KeyCode::Char('n' | 'N') => ModalOutcome::Commit(false),
            // Focus-based interaction — Tab/←→/h/l all toggle focus.
            KeyCode::Tab | KeyCode::Right | KeyCode::Left | KeyCode::Char('l' | 'h') => {
                self.focus = match self.focus {
                    ConfirmFocus::Yes => ConfirmFocus::No,
                    ConfirmFocus::No => ConfirmFocus::Yes,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => ModalOutcome::Commit(matches!(self.focus, ConfirmFocus::Yes)),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn uppercase_y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('Y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn n_commits_false() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('n'))),
            ModalOutcome::Commit(false)
        ));
    }

    #[test]
    fn esc_cancels() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn arrow_is_noop() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Down)),
            ModalOutcome::Continue
        ));
    }

    #[test]
    fn default_focus_is_no() {
        let s = ConfirmState::new("Delete?");
        assert_eq!(s.focus, ConfirmFocus::No);
    }

    #[test]
    fn tab_cycles_focus() {
        let mut s = ConfirmState::new("Delete?");
        assert_eq!(s.focus, ConfirmFocus::No);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, ConfirmFocus::Yes);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, ConfirmFocus::No);
    }

    #[test]
    fn enter_commits_focused_option() {
        let mut s = ConfirmState::new("Delete?");
        // Default focus is No, Enter commits No.
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(false)
        ));

        let mut s = ConfirmState::new("Delete?");
        s.handle_key(key(KeyCode::Tab)); // focus Yes
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn y_still_works_regardless_of_focus() {
        let mut s = ConfirmState::new("Delete?");
        // Focus is No by default; Y should still commit true directly.
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }
}

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

/// Height (rows) this Confirm modal wants, given its current prompt text.
/// Layout is: N prompt lines + 1 spacer + 1 buttons + 1 spacer + 1 hint.
/// Callers use this to size the surrounding modal rect.
#[must_use]
pub fn required_height(state: &ConfirmState) -> u16 {
    let prompt_lines = state.prompt.lines().count().max(1) as u16;
    // Fixed chrome: top/bottom border (2) + spacer + buttons + spacer + hint (4).
    prompt_lines + 6
}

pub fn render(frame: &mut Frame, area: Rect, state: &ConfirmState) {
    // Outer block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Confirm ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Vertical layout inside the inner rect. The prompt area grows with the
    // number of lines in `state.prompt` so multi-line confirmations (e.g.
    // the mount-collapse prompt) render without clipping.
    let prompt_lines = state.prompt.lines().count().max(1) as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(prompt_lines), // prompt (may span multiple lines)
            Constraint::Length(1),            // spacer
            Constraint::Length(1),            // button row
            Constraint::Length(1),            // spacer between buttons and hint
            Constraint::Length(1),            // footer hint
        ])
        .split(inner);

    // Prompt — render each line in turn so centering applies per-line.
    let prompt_lines_vec: Vec<Line> = state
        .prompt
        .lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    let prompt = Paragraph::new(prompt_lines_vec).alignment(Alignment::Center);
    frame.render_widget(prompt, chunks[0]);

    // Button row — focused choice highlights on white; unfocused stays
    // flush with the modal background so only the focused choice pops.
    let yes_focused = matches!(state.focus, ConfirmFocus::Yes);
    let no_focused = matches!(state.focus, ConfirmFocus::No);

    let focused_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);

    let yes_btn_style = if yes_focused {
        focused_style
    } else {
        unfocused_style
    };
    let no_btn_style = if no_focused {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  Yes  ", yes_btn_style),
        Span::raw("    "),
        Span::styled("  No  ", no_btn_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[2],
    );

    // Footer hint — same key/text/sep scheme as the main TUI footer.
    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let sep = Style::default().fg(PHOSPHOR_DARK);
    let hint = Paragraph::new(ratatui::text::Line::from(vec![
        Span::styled("Y", key),
        Span::styled(" yes", text),
        Span::styled(" \u{b7} ", sep),
        Span::styled("N", key),
        Span::styled(" no", text),
        Span::styled(" \u{b7} ", sep),
        Span::styled("Esc", key),
        Span::styled(" cancel", text),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}
