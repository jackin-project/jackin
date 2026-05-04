//! Single-button error popup used to surface save-time failures.
//!
//! Opened by the editor's save path when an internal-API call (e.g.
//! `ConfigEditor::create_workspace`) returns an Err that the operator
//! needs to see up-front — duplicate workspace name, I/O error, planner
//! reject. Dismiss with Enter / O / Esc; the editor stays open so the
//! operator can adjust and retry.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::ModalOutcome;

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);
const DANGER_RED: Color = Color::Rgb(255, 94, 122);

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
    /// Memoized `(inner_width, rows)` from the last `estimated_message_rows`
    /// call. The popup is rendered every frame while open and the message
    /// can be a long anyhow chain — re-walking each line every frame is
    /// avoidable. `Cell` because `render` only takes `&self`.
    cached_rows: std::cell::Cell<Option<(u16, u16)>>,
}

impl ErrorPopupState {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            cached_rows: std::cell::Cell::new(None),
        }
    }

    /// Every key dismisses — Enter, O (ok), and Esc are all accepted.
    /// Other keys are no-ops so a stray keystroke doesn't close the
    /// popup prematurely.
    pub const fn handle_key(&self, key: KeyEvent) -> ModalOutcome<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('o' | 'O') | KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

/// Estimate the number of wrapped rows the message needs for a given
/// inner width. Used by the modal sizer so tall messages don't clip.
///
/// The result is memoized on `state` keyed by `inner_width` so the
/// per-line `chars().count()` walk only happens on resize, not on every
/// render frame.
#[must_use]
pub fn estimated_message_rows(state: &ErrorPopupState, inner_width: u16) -> u16 {
    if let Some((cached_width, rows)) = state.cached_rows.get()
        && cached_width == inner_width
    {
        return rows;
    }
    let w = usize::from(inner_width.max(1));
    let mut rows: u32 = 0;
    for line in state.message.lines() {
        // Each logical line takes ceil(len/w) rows; empty lines still
        // count as one.
        let len = line.chars().count().max(1);
        let r = len.div_ceil(w);
        rows = rows.saturating_add(u32::try_from(r).unwrap_or(u32::MAX));
    }
    let result = u16::try_from(rows.max(1)).unwrap_or(u16::MAX);
    state.cached_rows.set(Some((inner_width, result)));
    result
}

/// Total rows the popup wants. Layout: top border + blank + N message
/// rows + blank + button + blank + hint + bottom border.
///
/// `max_rows` is the upper bound the caller is willing to allocate
/// (typically terminal height minus its own chrome). Long anyhow chains
/// — common when role resolution or docker build fails — easily exceed
/// 15 rows, and a fixed cap silently truncates the bottom of the
/// message where the root cause usually lives.
///
/// The chrome floor is 8 (2 borders + 4 spacer/button/hint rows + 1 message
/// row). At smaller `max_rows` the renderer would produce a zero-row body
/// chunk and the message would disappear, which is worse than the popup
/// overflowing the terminal.
#[must_use]
pub fn required_height(state: &ErrorPopupState, inner_width: u16, max_rows: u16) -> u16 {
    let body = estimated_message_rows(state, inner_width);
    body.saturating_add(7).min(max_rows.max(8))
}

pub fn render(frame: &mut Frame, area: Rect, state: &ErrorPopupState) {
    let title = format!(" {} ", state.title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DANGER_RED))
        .title(Span::styled(
            title,
            Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Reserve rows for blank/button/blank/hint (4 rows); the rest
    // belongs to the wrapped message.
    let body_rows = inner.height.saturating_sub(5); // blank + blank + button + blank + hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),         // top blank
            Constraint::Length(body_rows), // message (wrapped)
            Constraint::Length(1),         // blank
            Constraint::Length(1),         // button
            Constraint::Length(1),         // blank
            Constraint::Length(1),         // hint
        ])
        .split(inner);

    // Message body — WHITE to keep readable against the red border.
    let paragraph = Paragraph::new(state.message.as_str())
        .style(Style::default().fg(WHITE))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[1]);

    // Button — always focused (only one).
    let focused_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("  OK  ", focused_style)))
            .alignment(Alignment::Center),
        chunks[3],
    );

    // Hint — PHOSPHOR_DIM would be ideal but we lean on the canonical
    // KEY/TEXT/SEP scheme the cross-widget consistency test enforces:
    // WHITE+BOLD key + PHOSPHOR_GREEN label.
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Enter/O", key_style),
        Span::styled(" ok", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Esc", key_style),
        Span::styled(" close", text_style),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[5]);
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

    fn sample() -> ErrorPopupState {
        ErrorPopupState::new("Save failed", "workspace \"demo\" already exists")
    }

    #[test]
    fn error_popup_enter_dismisses() {
        let s = sample();
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn error_popup_esc_dismisses() {
        let s = sample();
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn error_popup_o_dismisses() {
        let s = sample();
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('o'))),
            ModalOutcome::Cancel
        ));
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('O'))),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn error_popup_other_keys_are_noops() {
        let s = sample();
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('x'))),
            ModalOutcome::Continue
        ));
        assert!(matches!(
            s.handle_key(key(KeyCode::Tab)),
            ModalOutcome::Continue
        ));
    }

    #[test]
    fn required_height_respects_caller_supplied_max() {
        // Long message wants well above the cap — required_height must
        // not exceed `max_rows`, and never drop below the chrome floor.
        let s = ErrorPopupState::new("Save failed", "word ".repeat(500));
        assert!(required_height(&s, 30, 15) <= 15);
        assert!(required_height(&s, 30, 40) <= 40);
        // Floor: caller passing too-small max still yields the 8-row
        // chrome floor so the renderer's body chunk is never zero rows
        // (which would make the message vanish).
        assert!(required_height(&s, 30, 1) >= 8);
    }

    #[test]
    fn estimated_message_rows_wraps_long_lines() {
        let s = ErrorPopupState::new("t", "abcdefghijklmnop"); // 16 chars
        // width 8 → 2 rows
        assert_eq!(estimated_message_rows(&s, 8), 2);
    }

    #[test]
    fn render_single_line_message_is_visible() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        let state = ErrorPopupState::new("Role not found", "repository not found");
        let area = Rect::new(0, 0, 60, required_height(&state, 56, 25));
        let backend = TestBackend::new(area.width, area.height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, area, &state)).unwrap();

        let buf = term.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
            rendered.push('\n');
        }
        assert!(
            rendered.contains("repository not found"),
            "message should be visible in popup:\n{rendered}"
        );
    }
}
