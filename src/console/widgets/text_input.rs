//! Single-line text input modal — wraps ratatui-textarea.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{CursorMove, Input, TextArea};

use super::ModalOutcome;

/// Single-line text-input modal state.
///
/// `forbidden` is an optional list of values that the input must not
/// commit — used by the `EnvKey` flow to block duplicate keys live (and
/// generic enough to be reused by any future input that needs the same
/// guard). When non-empty, [`TextInputState::is_duplicate`] returns
/// `true` while the trimmed value matches any forbidden entry, the
/// render path shows an inline warning, and Enter is swallowed.
///
/// `forbidden_label` is a human-readable scope hint (e.g. `"workspace
/// env"` or `"agent agent-smith"`) appended to the inline warning so
/// the operator knows where the collision lives. Empty by default.
pub struct TextInputState<'a> {
    pub label: String,
    pub textarea: TextArea<'a>,
    pub forbidden: Vec<String>,
    pub forbidden_label: String,
}

impl std::fmt::Debug for TextInputState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputState")
            .field("label", &self.label)
            .field("forbidden", &self.forbidden)
            .field("forbidden_label", &self.forbidden_label)
            .finish()
    }
}

impl TextInputState<'_> {
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        Self::new_with_forbidden(label, initial, Vec::new())
    }

    /// Construct a text-input state with a populated forbidden list. The
    /// label, initial value, and cursor placement match `new`. Callers
    /// can additionally set `forbidden_label` after construction.
    pub fn new_with_forbidden(
        label: impl Into<String>,
        initial: impl Into<String>,
        forbidden: Vec<String>,
    ) -> Self {
        let mut textarea = TextArea::new(vec![initial.into()]);
        // Position cursor at end of initial text so editing feels natural.
        textarea.move_cursor(CursorMove::End);
        Self {
            label: label.into(),
            textarea,
            forbidden,
            forbidden_label: String::new(),
        }
    }

    pub fn value(&self) -> String {
        self.textarea.lines().first().cloned().unwrap_or_default()
    }

    /// Trimmed form of the current value — used for duplicate detection
    /// and rendered in the inline warning. We trim leading/trailing
    /// whitespace before comparing because the `EnvKey` commit handler
    /// itself trims before storing, so a name like `" DB_URL "` would
    /// otherwise commit and silently overwrite the existing `DB_URL`.
    /// Trimming here keeps the live feedback consistent with what
    /// commit will actually do.
    pub fn trimmed_value(&self) -> String {
        self.value().trim().to_string()
    }

    /// True when the trimmed value collides with an entry in
    /// `forbidden`. An empty value is never reported as a duplicate —
    /// emptiness is a separate validation handled at commit time.
    pub fn is_duplicate(&self) -> bool {
        let v = self.trimmed_value();
        !v.is_empty() && self.forbidden.iter().any(|f| f == &v)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Enter => {
                // Block commit while the trimmed value collides with the
                // forbidden list. The render path shows the inline
                // warning; the operator must backspace and retype.
                if self.is_duplicate() {
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Commit(self.value())
            }
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
    text::Span,
    widgets::{Block, Borders},
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);
const DANGER_RED: Color = Color::Rgb(255, 94, 122);

pub fn render(frame: &mut Frame, area: Rect, state: &TextInputState) {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        widgets::Paragraph,
    };

    frame.render_widget(ratatui::widgets::Clear, area);

    // Block title styled WHITE + BOLD to match the main-screen block titles
    // (General/Mounts/Agents). The default widget text stays PHOSPHOR_GREEN.
    // Wrap the label in leading/trailing spaces so `┌ Label ─┐` renders
    // with breathing room (matches the canonical modal template).
    let title = Span::styled(
        format!(" {} ", state.label),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(&block, area);

    // Inner layout: top pad / input / spacer (or duplicate-warning) /
    // hint — matches the canonical modal template. The hint lives
    // inside the bordered block so the bottom border stays unbroken.
    // When `is_duplicate()` is true we replace the spacer with an
    // inline warning row; the modal's outer height is bumped (see
    // `modal_outer_rect`) so the warning never crowds the hint.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // input field
            Constraint::Length(1), // spacer / duplicate-warning slot
            Constraint::Length(1), // hint
        ])
        .split(inner);

    let mut ta = state.textarea.clone();
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(
        Style::default()
            .bg(WHITE)
            .fg(Color::Black)
            .add_modifier(Modifier::SLOW_BLINK),
    );
    frame.render_widget(&ta, rows[1]);

    // Inline duplicate warning — DANGER_RED, italic + bold. Only drawn
    // when the trimmed value collides with a forbidden entry. The
    // message format matches the spec:
    //   "<KEY>" already exists in <forbidden_label>     (label set)
    //   "<KEY>" already exists                          (label empty)
    if state.is_duplicate() {
        let key = state.trimmed_value();
        let msg = if state.forbidden_label.is_empty() {
            format!("\u{26a0} \"{key}\" already exists")
        } else {
            format!(
                "\u{26a0} \"{key}\" already exists in {}",
                state.forbidden_label
            )
        };
        let warn = Paragraph::new(msg).style(
            Style::default()
                .fg(DANGER_RED)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
        );
        frame.render_widget(warn, rows[2]);
    }

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
        Span::styled(" \u{b7} ", Style::default().fg(PHOSPHOR_DARK)),
        Span::styled(
            "Esc",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(PHOSPHOR_GREEN)),
    ]);
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), rows[3]);
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

    // ── Forbidden-list / duplicate-detection tests ───────────────────

    #[test]
    fn text_input_is_duplicate_returns_false_for_unique_value() {
        let s = TextInputState::new_with_forbidden(
            "Key",
            "API_KEY",
            vec!["DB_URL".into(), "AWS_REGION".into()],
        );
        assert!(!s.is_duplicate());
    }

    #[test]
    fn text_input_is_duplicate_returns_true_for_value_in_forbidden_list() {
        let s = TextInputState::new_with_forbidden(
            "Key",
            "DB_URL",
            vec!["DB_URL".into(), "AWS_REGION".into()],
        );
        assert!(s.is_duplicate());
    }

    #[test]
    fn text_input_is_duplicate_returns_false_for_empty_value() {
        // Empty input is never flagged as a duplicate — emptiness is a
        // separate validation handled at commit time. Without this, a
        // freshly-opened EnvKey modal with `"DB_URL"` already in the
        // forbidden list would still need to render the warning before
        // the operator typed anything, which would be confusing.
        let s = TextInputState::new_with_forbidden("Key", "", vec!["DB_URL".into()]);
        assert!(!s.is_duplicate());
    }

    #[test]
    fn text_input_is_duplicate_trims_whitespace() {
        // Trimming matches what the EnvKey commit handler does before
        // storing the key, so live feedback agrees with the eventual
        // commit decision.
        let s = TextInputState::new_with_forbidden("Key", "  DB_URL  ", vec!["DB_URL".into()]);
        assert!(s.is_duplicate());
    }

    #[test]
    fn text_input_enter_blocked_when_duplicate() {
        let mut s = TextInputState::new_with_forbidden("Key", "DB_URL", vec!["DB_URL".into()]);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(outcome, ModalOutcome::Continue),
            "Enter on a duplicate value must not commit; got {outcome:?}"
        );
    }

    #[test]
    fn text_input_enter_commits_when_unique() {
        let mut s = TextInputState::new_with_forbidden("Key", "API_KEY", vec!["DB_URL".into()]);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "API_KEY"));
    }
}
