//! Single-line text input modal — wraps ratatui-textarea.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{CursorMove, Input, TextArea};

use super::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    Default,
    Error,
}

/// Single-line text input with optional duplicate guard.
///
/// `forbidden` blocks live commit on a match (e.g. duplicate
/// `EnvKey`); `forbidden_label` is a scope hint appended to the
/// warning. `allow_empty=true` lets `EnvValue` distinguish POSIX
/// `VAR=""` from `unset VAR`.
pub struct TextInputState<'a> {
    pub label: String,
    pub textarea: TextArea<'a>,
    pub forbidden: Vec<String>,
    pub forbidden_label: String,
    pub allow_empty: bool,
}

impl std::fmt::Debug for TextInputState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputState")
            .field("label", &self.label)
            .field("forbidden", &self.forbidden)
            .field("forbidden_label", &self.forbidden_label)
            .field("allow_empty", &self.allow_empty)
            .finish()
    }
}

impl TextInputState<'_> {
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        Self::new_with_forbidden(label, initial, Vec::new())
    }

    /// `EnvValue` opts in here: POSIX `VAR=""` differs from
    /// `unset VAR`, so the operator must be able to commit an empty
    /// string.
    pub fn new_allow_empty(label: impl Into<String>, initial: impl Into<String>) -> Self {
        let mut s = Self::new(label, initial);
        s.allow_empty = true;
        s
    }

    pub fn new_with_forbidden(
        label: impl Into<String>,
        initial: impl Into<String>,
        forbidden: Vec<String>,
    ) -> Self {
        let mut textarea = TextArea::new(vec![initial.into()]);
        textarea.move_cursor(CursorMove::End);
        Self {
            label: label.into(),
            textarea,
            forbidden,
            forbidden_label: String::new(),
            allow_empty: false,
        }
    }

    pub fn value(&self) -> String {
        self.textarea.lines().first().cloned().unwrap_or_default()
    }

    /// Trimmed because `EnvKey` commit also trims — without matching
    /// here, `" DB_URL "` would silently overwrite an existing
    /// `DB_URL`.
    pub fn trimmed_value(&self) -> String {
        self.value().trim().to_string()
    }

    /// Empty values are not flagged here — emptiness is a separate
    /// validation at commit time.
    pub fn is_duplicate(&self) -> bool {
        let v = self.trimmed_value();
        !v.is_empty() && self.forbidden.iter().any(|f| f == &v)
    }

    pub fn is_valid(&self) -> bool {
        let v = self.trimmed_value();
        let empty_ok = self.allow_empty || !v.is_empty();
        empty_ok && !self.forbidden.iter().any(|f| f == &v)
    }

    /// Empty stays Default — emptiness is "not ready" not "wrong"; we
    /// don't paint red until the operator types something explicitly
    /// invalid.
    pub fn border_style(&self) -> BorderStyle {
        if self.is_duplicate() {
            BorderStyle::Error
        } else {
            BorderStyle::Default
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Enter => {
                if !self.is_valid() {
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Commit(self.value())
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => {
                // textarea treats Ctrl+M as newline — swallow.
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
/// Almost-invisible dim background so the input region is visible
/// even when empty.
const INPUT_BG_DIM: Color = Color::Rgb(20, 24, 22);

pub fn render(frame: &mut Frame, area: Rect, state: &TextInputState) {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        widgets::Paragraph,
    };

    frame.render_widget(ratatui::widgets::Clear, area);

    let title = Span::styled(
        format!(" {} ", state.label),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let border_color = match state.border_style() {
        BorderStyle::Error => DANGER_RED,
        BorderStyle::Default => PHOSPHOR_DARK,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // 1-cell pad on each side keeps text off the border; only the
    // textarea_area gets the dim band so the pads stay panel-colored.
    let input_row = rows[1];
    let textarea_area = Rect {
        x: input_row.x.saturating_add(1),
        y: input_row.y,
        width: input_row.width.saturating_sub(2),
        height: input_row.height,
    };
    let bg_block = Block::default().style(Style::default().bg(INPUT_BG_DIM));
    frame.render_widget(bg_block, textarea_area);
    let mut ta = state.textarea.clone();
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(
        Style::default()
            .bg(WHITE)
            .fg(Color::Black)
            .add_modifier(Modifier::SLOW_BLINK),
    );
    // Match textarea bg to the dim band so the cursor row doesn't
    // punch back to the panel color.
    ta.set_style(Style::default().fg(PHOSPHOR_GREEN).bg(INPUT_BG_DIM));
    frame.render_widget(&ta, textarea_area);

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

    // Hide `Enter confirm` whenever Enter wouldn't commit — telling
    // the operator a key works that doesn't is worse than a shorter
    // hint.
    let hint_spans: Vec<Span> = if state.is_valid() {
        vec![
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
        ]
    } else {
        vec![
            Span::styled(
                "Esc",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(PHOSPHOR_GREEN)),
        ]
    };
    let hint = ratatui::text::Line::from(hint_spans);
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
        let s = TextInputState::new_with_forbidden("Key", "", vec!["DB_URL".into()]);
        assert!(!s.is_duplicate());
    }

    #[test]
    fn text_input_is_duplicate_trims_whitespace() {
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

    // ── is_valid / border_style ──────────────────────────────────────

    #[test]
    fn text_input_is_valid_false_for_empty() {
        let s = TextInputState::new_with_forbidden("Key", "", vec!["DB_URL".into()]);
        assert!(!s.is_valid());
    }

    #[test]
    fn text_input_is_valid_false_for_duplicate() {
        let s = TextInputState::new_with_forbidden("Key", "DB_URL", vec!["DB_URL".into()]);
        assert!(!s.is_valid());
    }

    #[test]
    fn text_input_is_valid_true_for_unique_non_empty() {
        let s = TextInputState::new_with_forbidden("Key", "API_KEY", vec!["DB_URL".into()]);
        assert!(s.is_valid());
    }

    #[test]
    fn text_input_border_style_default_for_empty() {
        let s = TextInputState::new_with_forbidden("Key", "", vec!["DB_URL".into()]);
        assert_eq!(s.border_style(), BorderStyle::Default);
    }

    #[test]
    fn text_input_border_style_error_for_duplicate() {
        let s = TextInputState::new_with_forbidden("Key", "DB_URL", vec!["DB_URL".into()]);
        assert_eq!(s.border_style(), BorderStyle::Error);
    }

    #[test]
    fn text_input_border_style_default_for_valid() {
        let s = TextInputState::new_with_forbidden("Key", "API_KEY", vec!["DB_URL".into()]);
        assert_eq!(s.border_style(), BorderStyle::Default);
    }

    #[test]
    fn text_input_renders_with_one_cell_left_padding() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        let area = Rect::new(0, 0, 60, 6);
        let backend = TestBackend::new(area.width, area.height);
        let mut term = Terminal::new(backend).unwrap();
        let state = TextInputState::new("Value for TEST", "abc");
        term.draw(|f| render(f, area, &state)).unwrap();
        let buf = term.backend().buffer();

        // y=0 top border, y=1 top pad, y=2 input row. x=1 = pad, x=2 = first char.
        let row_y: u16 = 2;
        let cell_pad = buf[(1, row_y)].symbol();
        let cell_first_char = buf[(2, row_y)].symbol();
        assert_eq!(
            cell_pad, " ",
            "x=1 (just inside left border) must be the 1-cell left pad; got {cell_pad:?}",
        );
        assert_eq!(
            cell_first_char, "a",
            "x=2 (after the left pad) must hold the first input char 'a'; \
             got {cell_first_char:?}",
        );
    }

    /// Dim band only on `textarea_area` (1-cell pads stay panel-color).
    #[test]
    fn text_input_input_row_has_dim_background() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        let area = Rect::new(0, 0, 60, 6);
        let backend = TestBackend::new(area.width, area.height);
        let mut term = Terminal::new(backend).unwrap();
        let state = TextInputState::new("Value for TEST", "");
        term.draw(|f| render(f, area, &state)).unwrap();
        let buf = term.backend().buffer();

        let row_y: u16 = 2;
        let left_pad = &buf[(1, row_y)];
        let right_pad = &buf[(area.width - 2, row_y)];
        assert_ne!(
            left_pad.bg, INPUT_BG_DIM,
            "left pad cell at x=1 must NOT carry INPUT_BG_DIM (it's the 1-cell padding outside the textarea); got bg={:?}",
            left_pad.bg,
        );
        assert_ne!(
            right_pad.bg,
            INPUT_BG_DIM,
            "right pad cell at x={} must NOT carry INPUT_BG_DIM; got bg={:?}",
            area.width - 2,
            right_pad.bg,
        );

        // Sample left edge, right edge, mid-row (cursor lives at x=2
        // with a WHITE highlight, so don't sample there).
        let band_left = &buf[(3, row_y)];
        let band_right = &buf[(area.width - 3, row_y)];
        let interior = &buf[(area.width / 2, row_y)];
        for (label, cell) in [
            ("band left edge", band_left),
            ("band right edge", band_right),
            ("interior (mid-row)", interior),
        ] {
            assert_eq!(
                cell.bg, INPUT_BG_DIM,
                "input row {label} bg={:?}, expected INPUT_BG_DIM (subtle dim band)",
                cell.bg,
            );
        }
    }

    #[test]
    fn text_input_enter_blocked_when_empty() {
        let mut s = TextInputState::new_with_forbidden("Key", "", vec!["DB_URL".into()]);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(outcome, ModalOutcome::Continue),
            "Enter on an empty value must not commit; got {outcome:?}"
        );
    }

    // ── allow_empty: target-specific validity (EnvValue) ─────────────

    #[test]
    fn text_input_allow_empty_is_valid_for_empty_value() {
        let s = TextInputState::new_allow_empty("Value for FOO", "");
        assert!(
            s.is_valid(),
            "allow_empty=true must report empty value as valid"
        );
    }

    #[test]
    fn text_input_allow_empty_still_rejects_duplicates() {
        let mut s = TextInputState::new_allow_empty("Value", "foo");
        s.forbidden = vec!["foo".into()];
        assert!(
            !s.is_valid(),
            "allow_empty must still reject values present in `forbidden`"
        );
    }

    #[test]
    fn text_input_default_constructor_still_rejects_empty() {
        let s = TextInputState::new("Key", "");
        assert!(
            !s.is_valid(),
            "default constructor must keep the non-empty validity rule"
        );
    }

    #[test]
    fn text_input_allow_empty_enter_commits_empty() {
        let mut s = TextInputState::new_allow_empty("Value", "");
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(&outcome, ModalOutcome::Commit(v) if v.is_empty()),
            "Enter on empty value must commit empty string when allow_empty; got {outcome:?}"
        );
    }
}
