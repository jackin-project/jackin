//! Two-button modal that asks the operator whether a new env var should
//! land at workspace scope (visible to every agent) or at a single
//! agent's override scope.
//!
//! Inserted at the very start of the Secrets-tab Add flow when the
//! operator presses `Enter` on the workspace-level
//! `+ Add environment variable` sentinel. The "all agents" choice
//! drops into the standard `EnvKey` → `SourcePicker` → value path with
//! `Workspace` scope. The "specific agent" choice opens an agent
//! picker so the operator chooses which agent's overrides to extend;
//! the `EnvKey` modal then opens with `Agent(<name>)` scope.
//!
//! Models on [`super::source_picker`] — same two-button shape, same
//! ←/→ navigation, same `Enter` / `Esc` semantics — so the visual
//! rhythm of the modal stack stays consistent.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

/// Operator's scope choice from the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeChoice {
    /// Workspace-level env — visible to every agent that runs in this
    /// workspace. Drops into the standard `EnvKey` modal with
    /// `SecretsScopeTag::Workspace`.
    AllAgents,
    /// Per-agent override — the next step opens an agent picker so the
    /// operator chooses which agent's override section to extend. The
    /// `EnvKey` modal then opens with `SecretsScopeTag::Agent(<name>)`.
    SpecificAgent,
}

#[derive(Debug, Clone)]
pub struct ScopePickerState {
    /// Currently focused button. Default focus is `AllAgents` — the
    /// most common case (operators land on the workspace sentinel
    /// expecting workspace scope) and the option that doesn't require a
    /// follow-up picker.
    pub focused: ScopeChoice,
}

impl Default for ScopePickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopePickerState {
    /// Construct a picker focused on `AllAgents`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focused: ScopeChoice::AllAgents,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<ScopeChoice> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            // Symmetric two-button modal: any directional key just
            // toggles between the two choices, so left/right/tab/h/l
            // share a body.
            KeyCode::Tab
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

    let title = " New environment variable ".to_string();
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

    // Inner layout matches the SourcePicker rhythm: top pad / buttons /
    // spacer / spacer / hint. Two spacer rows give the buttons room to
    // breathe without the modal feeling cramped.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top pad
            Constraint::Length(1), // buttons
            Constraint::Length(1), // spacer
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint footer
        ])
        .split(inner);

    // Buttons — same focus-pop scheme as `source_picker` and
    // `save_discard`: focused button gets a white background, unfocused
    // stays flush with the modal backdrop.
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
        Span::styled("  All agents  ", all_style),
        Span::raw("    "),
        Span::styled("  Specific agent  ", specific_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[1],
    );

    // Hint footer — same key/text/sep palette as every other modal.
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
        // Default focus is AllAgents — Enter commits it.
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

    /// Left arrow toggles back to `AllAgents` from `SpecificAgent` — the
    /// modal is symmetric: there are exactly two choices, so any
    /// directional cycle just flips between them.
    #[test]
    fn scope_picker_left_arrow_toggles_back_to_all_agents() {
        let mut s = ScopePickerState::new();
        let _ = s.handle_key(key_event(KeyCode::Right));
        assert_eq!(s.focused, ScopeChoice::SpecificAgent);
        let _ = s.handle_key(key_event(KeyCode::Left));
        assert_eq!(s.focused, ScopeChoice::AllAgents);
    }
}
