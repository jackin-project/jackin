//! Modal picker for role disambiguation.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use super::ModalOutcome;
use crate::selector::RoleSelector;

#[derive(Debug)]
pub struct RolePickerState {
    pub roles: Vec<RoleSelector>,
    pub list_state: ListState,
    pub filter: String,
    pub filtered: Vec<RoleSelector>,
    /// Verb after `Enter` in the footer (`launch` for launch
    /// disambiguation, `select` for editor override-scope picking).
    pub confirm_label: String,
}

impl RolePickerState {
    #[must_use]
    pub fn new(roles: Vec<RoleSelector>) -> Self {
        Self::with_confirm_label(roles, "select")
    }

    #[must_use]
    pub fn with_confirm_label(roles: Vec<RoleSelector>, confirm_label: &str) -> Self {
        let filtered = roles.clone();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            roles,
            list_state,
            filter: String::new(),
            filtered,
            confirm_label: confirm_label.to_string(),
        }
    }

    fn recompute_filtered(&mut self) {
        let needle = self.filter.to_ascii_lowercase();
        self.filtered = self
            .roles
            .iter()
            .filter(|role| needle.is_empty() || role.key().to_ascii_lowercase().contains(&needle))
            .cloned()
            .collect();
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
    }

    fn move_up(&mut self) {
        super::cycle_select(&mut self.list_state, self.filtered.len(), -1);
    }

    fn move_down(&mut self) {
        super::cycle_select(&mut self.list_state, self.filtered.len(), 1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<RoleSelector> {
        match key.code {
            KeyCode::Up => {
                self.move_up();
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                self.move_down();
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.recompute_filtered();
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(i) = self.list_state.selected
                    && let Some(role) = self.filtered.get(i)
                {
                    return ModalOutcome::Commit(role.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char(ch) => {
                // Every printable char goes to the filter — `j`/`k`
                // included; navigation is via arrow keys.
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
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

pub fn render(frame: &mut Frame, area: Rect, state: &RolePickerState) {
    // Filter row stays out of the title — see RULES.md "TUI List
    // Modals" for the canonical layout.
    let title = Span::styled(
        " Select Role ",
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // filter row
            Constraint::Length(1), // spacer
            Constraint::Min(1),    // list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // footer
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
                Style::default()
                    .fg(WHITE)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(filter_line), rows[0]);

    // List body. When the filter narrows the visible set to nothing,
    // render no rows — the blank space below the filter row IS the
    // empty state. No `(no roles match)` placeholder per the canonical
    // list-modal layout.
    let lines: Vec<Line> = state
        .filtered
        .iter()
        .enumerate()
        .map(|(i, role)| {
            let is_selected = Some(i) == state.list_state.selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(vec![Span::styled(format!("{prefix}{}", role.key()), style)])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[2]);

    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let confirm_label = format!(" {}", state.confirm_label);
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("\u{2191}\u{2193}", key_style),
        Span::styled(" navigate", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("type", key_style),
        Span::styled(" filter", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Enter", key_style),
        Span::styled(confirm_label, text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Esc", key_style),
        Span::styled(" cancel", text_style),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn roles(keys: &[&str]) -> Vec<RoleSelector> {
        keys.iter()
            .map(|k| RoleSelector::parse(k).expect("valid selector"))
            .collect()
    }

    #[test]
    fn new_selects_first_when_non_empty() {
        let s = RolePickerState::new(roles(&["chainargos/agent-smith", "agent-brown"]));
        assert_eq!(s.list_state.selected, Some(0));
        assert_eq!(s.filtered.len(), 2);
    }

    #[test]
    fn new_selects_nothing_when_empty() {
        let s = RolePickerState::new(vec![]);
        assert_eq!(s.list_state.selected, None);
    }

    #[test]
    fn enter_commits_selected_agent() {
        let mut s =
            RolePickerState::new(roles(&["chainargos/agent-smith", "chainargos/agent-brown"]));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome,
            ModalOutcome::Commit(a) if a.key() == "chainargos/agent-smith"));
    }

    #[test]
    fn esc_cancels() {
        let mut s = RolePickerState::new(roles(&["agent-smith"]));
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn filter_narrows_agent_list() {
        let mut s = RolePickerState::new(roles(&[
            "chainargos/agent-smith",
            "chainargos/agent-brown",
            "agent-architect",
        ]));
        for ch in "smith".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(s.filter, "smith");
        assert_eq!(s.filtered.len(), 1);
        assert_eq!(s.filtered[0].key(), "chainargos/agent-smith");
        assert_eq!(s.list_state.selected, Some(0));
    }

    #[test]
    fn filter_empty_shows_all() {
        let mut s = RolePickerState::new(roles(&["agent-smith", "agent-brown"]));
        s.handle_key(key(KeyCode::Char('s')));
        assert_eq!(s.filtered.len(), 1);
        s.handle_key(key(KeyCode::Backspace));
        assert!(s.filter.is_empty());
        assert_eq!(s.filtered.len(), 2);
        assert_eq!(s.list_state.selected, Some(0));
    }

    #[test]
    fn enter_on_empty_filtered_list_is_noop() {
        let mut s = RolePickerState::new(roles(&["agent-smith"]));
        for ch in "zzzz".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert!(s.filtered.is_empty());
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
    }

    #[test]
    fn down_wraps_at_end() {
        let mut s = RolePickerState::new(roles(&["agent-a", "agent-b"]));
        s.handle_key(key(KeyCode::Down));
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.list_state.selected, Some(0));
    }

    #[test]
    fn up_wraps_at_start() {
        let mut s = RolePickerState::new(roles(&["agent-a", "agent-b"]));
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.list_state.selected, Some(1));
    }

    /// `j`/`k` append to the filter (no vim-style nav) so roles with
    /// those letters in their key can be typed naturally.
    #[test]
    fn j_and_k_append_to_filter_not_navigate() {
        let mut s = RolePickerState::new(roles(&["agent-jenkins", "agent-kafka"]));
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.filter, "j");
        assert_eq!(s.filtered.len(), 1);
        assert_eq!(s.filtered[0].key(), "agent-jenkins");
    }

    // ── Render-buffer smoke tests ─────────────────────────────────────

    fn dump(state: &RolePickerState, w: u16, h: u16) -> String {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = Rect::new(0, 0, w, h);
            super::render(f, area, state);
        })
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
    fn agent_picker_renders_filter_row_with_placeholder_dots_when_empty() {
        let s = RolePickerState::new(roles(&["chainargos/agent-smith"]));
        let frame = dump(&s, 60, 12);
        assert!(
            frame.contains("Filter:"),
            "filter row label missing; frame:\n{frame}"
        );
        assert!(
            frame.contains('\u{2591}'),
            "filter row missing placeholder dots `░`; frame:\n{frame}"
        );
        let top: String = frame.lines().next().unwrap().to_string();
        assert!(
            top.contains("Select Role"),
            "title bar must read `Select Role`; top row:\n{top}"
        );
        assert!(
            !top.contains("filter:"),
            "filter must NOT be inlined into the title; top row:\n{top}"
        );
    }

    #[test]
    fn agent_picker_renders_filter_row_with_live_chars_when_typing() {
        let mut s =
            RolePickerState::new(roles(&["chainargos/agent-smith", "chainargos/agent-brown"]));
        for ch in "smi".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        let frame = dump(&s, 60, 12);
        assert!(
            frame.contains("Filter: smi"),
            "filter row must show live characters; frame:\n{frame}"
        );
        let top: String = frame.lines().next().unwrap().to_string();
        assert!(
            !top.contains("smi"),
            "live filter must NOT bleed into the title; top row:\n{top}"
        );
    }

    #[test]
    fn agent_picker_footer_uses_configured_confirm_label() {
        let s_launch =
            RolePickerState::with_confirm_label(roles(&["chainargos/agent-smith"]), "launch");
        let frame = dump(&s_launch, 60, 12);
        assert!(
            frame.contains("Enter") && frame.contains("launch"),
            "launch-context footer must read `Enter launch`; frame:\n{frame}"
        );
        assert!(
            !frame.contains(" select"),
            "launch-context footer must not say `select`; frame:\n{frame}"
        );

        let s_select =
            RolePickerState::with_confirm_label(roles(&["chainargos/agent-smith"]), "select");
        let frame = dump(&s_select, 60, 12);
        assert!(
            frame.contains("Enter") && frame.contains("select"),
            "select-context footer must read `Enter select`; frame:\n{frame}"
        );
        assert!(
            !frame.contains(" launch"),
            "select-context footer must not say `launch`; frame:\n{frame}"
        );
    }

    #[test]
    fn agent_picker_renders_no_empty_state_placeholder_when_filter_excludes_all() {
        let mut s = RolePickerState::new(roles(&["agent-smith", "agent-brown"]));
        for ch in "zzzz".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert!(s.filtered.is_empty());
        let frame = dump(&s, 60, 12);
        assert!(
            !frame.contains("(no roles match"),
            "must not render an empty-state placeholder; frame:\n{frame}"
        );
        assert!(
            !frame.contains("(no items match"),
            "must not render an empty-state placeholder; frame:\n{frame}"
        );
        assert!(frame.contains("Filter: zzzz"));
    }
}
