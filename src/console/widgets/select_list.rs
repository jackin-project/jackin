//! Generic modal filter-picker over labelled string items.
//!
//! Mirrors the canonical jackin list-modal layout (see the role picker
//! and the in-container Menu dialog): a `Filter:` row directly under the
//! top border, a blank spacer, then a `▸`-cursor selectable list. The
//! widget itself is policy-free — `handle_key` reports `Cancel` on `Esc`,
//! but a caller that must force a decision (the launch stale-instance
//! dialog) simply ignores `Cancel` and keeps the picker open until the
//! operator commits a choice.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use super::ModalOutcome;

#[derive(Debug)]
pub struct SelectListState {
    /// Display labels in their original (caller-defined) order. The index
    /// returned by a committed choice indexes into this vector.
    items: Vec<String>,
    list_state: ListState,
    filter: String,
    /// Indices into `items` that currently pass the filter, in order.
    filtered: Vec<usize>,
}

impl SelectListState {
    #[must_use]
    pub fn new(items: Vec<String>) -> Self {
        let filtered: Vec<usize> = (0..items.len()).collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            items,
            list_state,
            filter: String::new(),
            filtered,
        }
    }

    fn recompute_filtered(&mut self) {
        let needle = self.filter.to_ascii_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, label)| needle.is_empty() || label.to_ascii_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
        self.list_state
            .select((!self.filtered.is_empty()).then_some(0));
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Widest label in display columns, for sizing the dialog box.
    #[must_use]
    pub fn max_label_width(&self) -> u16 {
        self.items
            .iter()
            .map(|label| label.chars().count())
            .max()
            .unwrap_or(0)
            .try_into()
            .unwrap_or(u16::MAX)
    }

    /// Original-items index currently under the cursor, if any.
    #[must_use]
    pub fn selected_index(&self) -> Option<usize> {
        self.list_state
            .selected
            .and_then(|row| self.filtered.get(row).copied())
    }

    /// Commit on `Enter`, narrow on typing, navigate on arrows. `Esc`
    /// yields `Cancel`; force-decision callers ignore it.
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<usize> {
        match key.code {
            KeyCode::Up => {
                super::cycle_select(&mut self.list_state, self.filtered.len(), -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                super::cycle_select(&mut self.list_state, self.filtered.len(), 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.recompute_filtered();
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => self
                .selected_index()
                .map_or(ModalOutcome::Continue, ModalOutcome::Commit),
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char(ch) => {
                self.filter.push(ch);
                self.recompute_filtered();
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::scrollable::render_selected_lines_in_area;
use super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

/// Render the picker into `area` with `title` in the top border. Hint
/// text is the caller's responsibility (it belongs in the screen footer,
/// never inside the dialog).
pub fn render(frame: &mut Frame, area: Rect, state: &SelectListState, title: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // filter row, glued to the top border
            Constraint::Length(1), // spacer
            Constraint::Min(1),    // list
        ])
        .split(inner);

    let filter_line = if state.filter.is_empty() {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled("\u{2591}".repeat(20), Style::default().fg(PHOSPHOR_DARK)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled(state.filter.clone(), Style::default().fg(WHITE)),
            Span::styled(
                "\u{2588}",
                Style::default().fg(WHITE).add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(filter_line), rows[0]);

    let lines: Vec<Line> = state
        .filtered
        .iter()
        .enumerate()
        .map(|(row, &item)| {
            let is_selected = Some(row) == state.list_state.selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(vec![Span::styled(format!("{prefix}{}", state.items[item]), style)])
        })
        .collect();
    render_selected_lines_in_area(frame, rows[2], lines, state.list_state.selected);
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

    fn sample() -> SelectListState {
        SelectListState::new(vec![
            "Start fresh".to_string(),
            "Restore vagsj10n the-architect".to_string(),
            "Restore k7p9m2xq agent-smith".to_string(),
        ])
    }

    #[test]
    fn new_selects_first_when_non_empty() {
        let s = sample();
        assert_eq!(s.selected_index(), Some(0));
    }

    #[test]
    fn new_selects_nothing_when_empty() {
        let s = SelectListState::new(vec![]);
        assert_eq!(s.selected_index(), None);
    }

    #[test]
    fn enter_commits_original_index() {
        let mut s = sample();
        s.handle_key(key(KeyCode::Down));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(1)));
    }

    #[test]
    fn filter_narrows_and_commit_maps_back_to_original_index() {
        let mut s = sample();
        for ch in "smith".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        // Only the agent-smith row survives; it is original index 2.
        assert_eq!(s.selected_index(), Some(2));
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(2)
        ));
    }

    #[test]
    fn backspace_restores_filtered_set() {
        let mut s = sample();
        s.handle_key(key(KeyCode::Char('z')));
        assert_eq!(s.selected_index(), None);
        s.handle_key(key(KeyCode::Backspace));
        assert_eq!(s.selected_index(), Some(0));
    }

    #[test]
    fn enter_on_empty_filtered_list_is_noop() {
        let mut s = sample();
        for ch in "zzzz".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Continue
        ));
    }

    #[test]
    fn esc_reports_cancel_for_callers_that_allow_it() {
        let mut s = sample();
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), ModalOutcome::Cancel));
    }

    #[test]
    fn down_wraps_at_end() {
        let mut s = SelectListState::new(vec!["a".to_string(), "b".to_string()]);
        s.handle_key(key(KeyCode::Down));
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.selected_index(), Some(0));
    }

    fn dump(state: &SelectListState, w: u16, h: u16) -> String {
        use ratatui::{Terminal, backend::TestBackend};
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, Rect::new(0, 0, w, h), state, "Unfinished instances"))
            .unwrap();
        let buf = term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn renders_title_filter_row_and_cursor() {
        let frame = dump(&sample(), 60, 12);
        assert!(frame.contains("Unfinished instances"), "title missing:\n{frame}");
        assert!(frame.contains("Filter:"), "filter row missing:\n{frame}");
        assert!(frame.contains('\u{2591}'), "placeholder dots missing:\n{frame}");
        assert!(frame.contains('\u{25b8}'), "cursor marker missing:\n{frame}");
        assert!(frame.contains("Start fresh"), "first item missing:\n{frame}");
    }
}
