//! Plain-or-1Password choice between `EnvKey` input and value entry.

use crossterm::event::{KeyCode, KeyEvent};

use jackin_tui::ModalOutcome;

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
            | KeyCode::BackTab
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
    style::{Modifier, Style},
    text::Span,
    widgets::Paragraph,
};

use super::PHOSPHOR_DARK;
use jackin_tui::components::{Panel, PanelFocus};

pub fn render(frame: &mut Frame, area: Rect, state: &SourcePickerState) {
    let title = format!(" Source for {} ", state.key);
    let block = Panel::new()
        .title(&title)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let items = [
        jackin_tui::components::ButtonStripItem::new("Plain text"),
        if state.op_available {
            jackin_tui::components::ButtonStripItem::new("1Password")
        } else {
            jackin_tui::components::ButtonStripItem::disabled("1Password")
        },
    ];
    let focused = match state.focused {
        SourceChoice::Plain => 0,
        SourceChoice::Op => 1,
    };
    jackin_tui::components::ButtonStrip::new(&items)
        .focused(focused)
        .render(frame, chunks[1]);

    if !state.op_available {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "(install op CLI to enable)",
                Style::default()
                    .fg(PHOSPHOR_DARK)
                    .add_modifier(Modifier::DIM),
            ))
            .alignment(Alignment::Center),
            chunks[2],
        );
    }
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
