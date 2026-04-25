//! Single-line text input modal — wraps ratatui-textarea.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{CursorMove, Input, TextArea};

use super::ModalOutcome;

/// Border-color key for the text-input modal.
///
/// Chosen by [`TextInputState::border_style`]. `Default` means render
/// the canonical `PHOSPHOR_DARK` border; `Error` means render the
/// `DANGER_RED` border to match the inline duplicate warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    Default,
    Error,
}

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

    /// Whether Enter would commit the current input (non-empty AND not
    /// in the forbidden list). Drives both the commit gate and the
    /// footer-hint visibility — when this is false, the modal hides
    /// the `Enter confirm` hint to avoid telling the operator a key
    /// will work that won't.
    pub fn is_valid(&self) -> bool {
        let v = self.trimmed_value();
        !v.is_empty() && !self.forbidden.iter().any(|f| f == &v)
    }

    /// Border style key — `Default` for empty/valid, `Error` for
    /// duplicate. Empty is intentionally `Default`; emptiness is
    /// "not ready" not "wrong", so we don't paint the modal red until
    /// the operator types something that is explicitly an error.
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
                // Block commit unless the input is valid (non-empty
                // AND not a duplicate). The render path shows the
                // inline warning for duplicates and hides the
                // `Enter confirm` hint while empty; the operator must
                // type a unique value before Enter does anything.
                if !self.is_valid() {
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
/// Subtle dim background for the input field, slightly brighter than
/// the modal panel so the input region is visually distinct even when
/// empty — hinting "this is where you type". Stays in the dark
/// green-tinged neutral family the rest of the TUI uses; deliberately
/// "almost invisible" per operator guidance.
const INPUT_BG_DIM: Color = Color::Rgb(20, 24, 22);

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
    // Border color tracks the validation state: DANGER_RED matches the
    // inline duplicate warning; PHOSPHOR_DARK is the canonical default
    // (also used for empty input — emptiness is "not ready" not "wrong").
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

    // Inset the input field by 1 cell on each side so text doesn't sit
    // flush against the modal border, and paint a very subtle dim
    // background across the whole input row so the input region is
    // visible even when empty. Bg covers the full row width (including
    // the 1-cell pads on each side) so the operator sees a clean
    // 1-row band hinting "this is where you type".
    let input_row = rows[1];
    let bg_block = Block::default().style(Style::default().bg(INPUT_BG_DIM));
    frame.render_widget(bg_block, input_row);

    let textarea_area = Rect {
        x: input_row.x.saturating_add(1),
        y: input_row.y,
        width: input_row.width.saturating_sub(2),
        height: input_row.height,
    };
    let mut ta = state.textarea.clone();
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(
        Style::default()
            .bg(WHITE)
            .fg(Color::Black)
            .add_modifier(Modifier::SLOW_BLINK),
    );
    // Match the textarea's own style background to the dim band so the
    // textarea's body (where the cursor lives) blends with the band
    // rather than punching a hole back to the panel color.
    ta.set_style(Style::default().fg(PHOSPHOR_GREEN).bg(INPUT_BG_DIM));
    frame.render_widget(&ta, textarea_area);

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
    //
    // We hide the `Enter confirm` half of the legend whenever Enter
    // wouldn't actually commit (empty input, or duplicate). Telling the
    // operator a key works that doesn't is worse than showing a
    // shorter hint.
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

    // ── is_valid / border_style — the validation predicate that
    // drives both the commit gate and the footer-hint visibility. ───

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
        // Empty is intentionally Default — emptiness is "not ready" not
        // "wrong". The modal stays default-bordered until the operator
        // types something that's explicitly an error (a duplicate).
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

    /// The input field is rendered with a 1-cell left pad — the first
    /// inner column (just inside the left border) is a space, and the
    /// typed value starts in the next column. Mirrors the operator's
    /// "input shouldn't sit flush with the border" feedback.
    #[test]
    fn text_input_renders_with_one_cell_left_padding() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        let area = Rect::new(0, 0, 60, 6);
        let backend = TestBackend::new(area.width, area.height);
        let mut term = Terminal::new(backend).unwrap();
        let state = TextInputState::new("Value for TEST", "abc");
        term.draw(|f| render(f, area, &state)).unwrap();
        let buf = term.backend().buffer();

        // The input field row sits at y=2 (top border y=0, top pad y=1,
        // input y=2). Left border is at x=0.
        // With 1-cell pad, x=1 should be space and x=2 should be 'a'.
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

    /// The input row is painted with the dim INPUT_BG_DIM background
    /// across the full row, so the input region reads as a visible
    /// "this is where you type" band even when empty.
    #[test]
    fn text_input_input_row_has_dim_background() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        let area = Rect::new(0, 0, 60, 6);
        let backend = TestBackend::new(area.width, area.height);
        let mut term = Terminal::new(backend).unwrap();
        // Empty value — verifies the band is visible even with no chars.
        let state = TextInputState::new("Value for TEST", "");
        term.draw(|f| render(f, area, &state)).unwrap();
        let buf = term.backend().buffer();

        // Row y=2 is the input field row. The whole row (excluding the
        // left/right borders at x=0 and x=width-1, and the single
        // cursor cell which carries the WHITE-bg cursor highlight)
        // must carry the dim background. Sample a few representative
        // cells: the left-pad cell, the right-pad cell, and an
        // interior cell well away from the cursor.
        let row_y: u16 = 2;
        let left_pad = &buf[(1, row_y)];
        let right_pad = &buf[(area.width - 2, row_y)];
        let interior = &buf[(area.width / 2, row_y)];
        for (label, cell) in [
            ("left pad", left_pad),
            ("right pad", right_pad),
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
        // Regression — with the new validity gate, Enter on an empty
        // value must not commit. Previously the widget only blocked
        // duplicates; emptiness was caught at the call-site.
        let mut s = TextInputState::new_with_forbidden("Key", "", vec!["DB_URL".into()]);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(outcome, ModalOutcome::Continue),
            "Enter on an empty value must not commit; got {outcome:?}"
        );
    }
}
