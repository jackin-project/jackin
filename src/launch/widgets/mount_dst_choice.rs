//! Three-button choice modal for picking a mount destination.
//!
//! Most operator mounts want `dst = src`. This modal offers a fast path
//! (`OK`) for that common case and falls back to the text-input flow via
//! `Edit destination` when the operator wants a different container path.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDstChoice {
    Ok,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDstFocus {
    Ok,
    Edit,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct MountDstChoiceState {
    pub src: String,
    pub focus: MountDstFocus,
}

impl MountDstChoiceState {
    /// Default focus = `Ok`: the common case is "same path inside the
    /// container", so Enter should commit that without extra effort.
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            focus: MountDstFocus::Ok,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<MountDstChoice> {
        match key.code {
            KeyCode::Char('o' | 'O') => ModalOutcome::Commit(MountDstChoice::Ok),
            KeyCode::Char('e' | 'E') => ModalOutcome::Commit(MountDstChoice::Edit),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l' | 'L') => {
                self.focus = match self.focus {
                    MountDstFocus::Ok => MountDstFocus::Edit,
                    MountDstFocus::Edit => MountDstFocus::Cancel,
                    MountDstFocus::Cancel => MountDstFocus::Ok,
                };
                ModalOutcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.focus = match self.focus {
                    MountDstFocus::Ok => MountDstFocus::Cancel,
                    MountDstFocus::Edit => MountDstFocus::Ok,
                    MountDstFocus::Cancel => MountDstFocus::Edit,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => match self.focus {
                MountDstFocus::Ok => ModalOutcome::Commit(MountDstChoice::Ok),
                MountDstFocus::Edit => ModalOutcome::Commit(MountDstChoice::Edit),
                MountDstFocus::Cancel => ModalOutcome::Cancel,
            },
            _ => ModalOutcome::Continue,
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &MountDstChoiceState) {
    let phosphor = Color::Rgb(0, 255, 65);
    let phosphor_dim = Color::Rgb(0, 140, 30);
    let phosphor_dark = Color::Rgb(0, 80, 18);
    let white = Color::Rgb(255, 255, 255);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(phosphor_dark))
        .title(Span::styled(
            " Mount destination ",
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Layout: path | blank | explanation | blank | buttons | blank | hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // src path
            Constraint::Length(1), // spacer
            Constraint::Length(1), // explanation
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Host path line — the operator-picked source.
    let shortened = crate::tui::shorten_home(&state.src);
    frame.render_widget(
        Paragraph::new(Span::styled(
            shortened,
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        chunks[0],
    );

    // Explanation line in PHOSPHOR_DIM.
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Mount into the container at the same path, or pick a different destination?",
            Style::default().fg(phosphor_dim),
        ))
        .alignment(Alignment::Center),
        chunks[2],
    );

    // Buttons — focused choice highlights on white; unfocused stays
    // flush with the modal background so only the focused choice pops.
    let focused_style = Style::default()
        .bg(white)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default().fg(phosphor).add_modifier(Modifier::BOLD);

    let ok_style = if state.focus == MountDstFocus::Ok {
        focused_style
    } else {
        unfocused_style
    };
    let edit_style = if state.focus == MountDstFocus::Edit {
        focused_style
    } else {
        unfocused_style
    };
    let cancel_style = if state.focus == MountDstFocus::Cancel {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  OK  ", ok_style),
        Span::raw("    "),
        Span::styled("  Edit destination  ", edit_style),
        Span::raw("    "),
        Span::styled("  Cancel  ", cancel_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[4],
    );

    // Footer hint — mirrors save_discard styling (key WHITE+BOLD, label
    // PHOSPHOR_GREEN, separator PHOSPHOR_DARK).
    let key_style = Style::default().fg(white).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(phosphor);
    let sep_style = Style::default().fg(phosphor_dark);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", key_style),
            Span::styled(" confirm", text_style),
            Span::raw("   "),
            Span::styled("O", key_style),
            Span::styled(" ok", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("E", key_style),
            Span::styled(" edit", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("C/Esc", key_style),
            Span::styled(" cancel", text_style),
        ]))
        .alignment(Alignment::Center),
        chunks[6],
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
    fn new_defaults_focus_to_ok() {
        let s = MountDstChoiceState::new("/host/path");
        assert_eq!(s.focus, MountDstFocus::Ok);
        assert_eq!(s.src, "/host/path");
    }

    #[test]
    fn tab_cycles_ok_edit_cancel_ok() {
        let mut s = MountDstChoiceState::new("/h");
        assert_eq!(s.focus, MountDstFocus::Ok);
        assert!(matches!(
            s.handle_key(key(KeyCode::Tab)),
            ModalOutcome::Continue
        ));
        assert_eq!(s.focus, MountDstFocus::Edit);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, MountDstFocus::Cancel);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, MountDstFocus::Ok);
    }

    #[test]
    fn backtab_reverse_cycles() {
        let mut s = MountDstChoiceState::new("/h");
        assert_eq!(s.focus, MountDstFocus::Ok);
        s.handle_key(key(KeyCode::BackTab));
        assert_eq!(s.focus, MountDstFocus::Cancel);
        s.handle_key(key(KeyCode::BackTab));
        assert_eq!(s.focus, MountDstFocus::Edit);
        s.handle_key(key(KeyCode::BackTab));
        assert_eq!(s.focus, MountDstFocus::Ok);
    }

    #[test]
    fn enter_with_ok_focus_commits_ok() {
        let mut s = MountDstChoiceState::new("/h");
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(MountDstChoice::Ok)
        ));
    }

    #[test]
    fn enter_with_edit_focus_commits_edit() {
        let mut s = MountDstChoiceState::new("/h");
        s.handle_key(key(KeyCode::Tab)); // Ok -> Edit
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(MountDstChoice::Edit)
        ));
    }

    #[test]
    fn enter_with_cancel_focus_returns_cancel() {
        let mut s = MountDstChoiceState::new("/h");
        s.handle_key(key(KeyCode::Tab)); // Ok -> Edit
        s.handle_key(key(KeyCode::Tab)); // Edit -> Cancel
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn shortcut_o_commits_ok() {
        let mut s = MountDstChoiceState::new("/h");
        // Rotate focus away first to prove `o` is not focus-dependent.
        s.handle_key(key(KeyCode::Tab)); // focus -> Edit
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('o'))),
            ModalOutcome::Commit(MountDstChoice::Ok)
        ));
    }

    #[test]
    fn shortcut_e_commits_edit() {
        let mut s = MountDstChoiceState::new("/h");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('e'))),
            ModalOutcome::Commit(MountDstChoice::Edit)
        ));
    }

    #[test]
    fn shortcut_c_cancels() {
        let mut s = MountDstChoiceState::new("/h");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('c'))),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn esc_cancels() {
        let mut s = MountDstChoiceState::new("/h");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn uppercase_shortcuts_work() {
        // Shift-held shortcut characters should still route to the same
        // commit/cancel outcomes.
        let mut s = MountDstChoiceState::new("/h");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('O'))),
            ModalOutcome::Commit(MountDstChoice::Ok)
        ));
        let mut s = MountDstChoiceState::new("/h");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('E'))),
            ModalOutcome::Commit(MountDstChoice::Edit)
        ));
        let mut s = MountDstChoiceState::new("/h");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('C'))),
            ModalOutcome::Cancel
        ));
    }
}
