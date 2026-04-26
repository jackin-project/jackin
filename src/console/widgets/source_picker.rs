//! Two-button modal that asks the operator whether a new env var's value
//! should come from a Plain text input or from the 1Password picker.
//!
//! Inserted between the `EnvKey` text modal and the value-entry path on
//! the Secrets-tab Enter-on-sentinel flow. The 1Password choice is
//! disabled when the `op` CLI isn't on PATH (probed once at startup —
//! see `ConsoleState::op_available`); the modal still opens, the
//! disabled choice renders dim with an explanatory line, and `←`/`→`
//! navigation skips it.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

/// Operator's source choice from the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceChoice {
    /// Open the `EnvValue` text modal and let the operator type the value.
    Plain,
    /// Open the `OpPicker` modal and drill Account → Vault → Item → Field
    /// to produce an `op://...` reference.
    Op,
}

#[derive(Debug, Clone)]
pub struct SourcePickerState {
    /// `EnvKey` the operator typed in the previous modal — surfaced in
    /// the modal title (`Source for MY_KEY`) so the choice's context is
    /// visible without context-switching back to the previous step.
    pub key: String,
    /// Whether the 1Password CLI was reachable at console startup. When
    /// `false`, the Op button renders dim with an `(install op CLI to
    /// enable)` hint and `←`/`→` skip it. Captured once in
    /// `ConsoleState::op_available`; the operator must restart `jackin
    /// console` to pick up a mid-session install.
    pub op_available: bool,
    /// Currently focused button. Default focus is `Plain` (the safer /
    /// always-available option). `←`/`→`/`h`/`l`/`Tab` cycle focus,
    /// skipping the Op button when it's unavailable.
    pub focused: SourceChoice,
}

impl SourcePickerState {
    /// Construct a picker for the given key with the supplied
    /// `op_available` flag. Default focus is `Plain`.
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
            // Direct hotkeys — `O` only commits when 1Password is
            // actually available; otherwise it's a silent no-op (same as
            // pressing it on a disabled button).
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
                // `Enter` on the disabled Op button is a defensive no-op
                // — `cycle_*` should never park focus there, but if some
                // future code path leaves focus on an unavailable
                // button, silently refuse to commit it.
                if self.focused == SourceChoice::Op && !self.op_available {
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Commit(self.focused)
            }
            _ => ModalOutcome::Continue,
        }
    }

    const fn cycle(&mut self) {
        // Symmetric two-button modal: forward and backward produce the
        // same result, so navigation collapses to one method (matching
        // ScopePickerState's pattern). With Op disabled, cycling is a
        // no-op — only Plain is selectable, so focus stays put.
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
            Constraint::Length(1), // top pad
            Constraint::Length(1), // buttons
            Constraint::Length(1), // disabled-explainer (always rendered, may be empty)
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint footer
        ])
        .split(inner);

    // Buttons — same focus-pop scheme as `save_discard`: focused button
    // gets a white background; unfocused stays flush with the modal
    // backdrop. Disabled (Op when op_available=false) renders dim
    // regardless of focus, since cycling skips it.
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

    // Disabled-explainer: only rendered when 1Password is unavailable.
    // Same dim style as the disabled button so the relationship reads
    // visually.
    if !state.op_available {
        frame.render_widget(
            Paragraph::new(Span::styled("(install op CLI to enable)", disabled_style))
                .alignment(Alignment::Center),
            chunks[2],
        );
    }

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
        // Multiple presses don't park focus on Op either.
        let _ = s.handle_key(key_event(KeyCode::Right));
        let _ = s.handle_key(key_event(KeyCode::Tab));
        let _ = s.handle_key(key_event(KeyCode::Char('l')));
        assert_eq!(s.focused, SourceChoice::Plain);
    }

    #[test]
    fn source_picker_enter_on_plain_commits_plain() {
        let mut s = SourcePickerState::new("MY_KEY".into(), true);
        // Default focus is Plain — Enter commits it.
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

    /// The `O` hotkey is a no-op when the picker is unavailable —
    /// rejecting it here matches the `← / →` skip behavior so a
    /// disabled choice is never committable.
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
