//! Modal picker for agent disambiguation when launching a workspace
//! that has more than one eligible agent.
//!
//! Mirrors `github_picker`'s shape — one `Vec`-driven list +
//! `tui_widget_list::ListState` — so the manager can dispatch it with
//! the same Up/Down/Enter pattern. Adds a filter-as-you-type field so a
//! large agent roster can be narrowed in place.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use super::ModalOutcome;
use crate::selector::ClassSelector;

#[derive(Debug)]
pub struct AgentPickerState {
    /// Eligibility-filtered set captured at open time; never mutated
    /// while the picker is up. Filter applies on top of this set.
    pub agents: Vec<ClassSelector>,
    pub list_state: ListState,
    pub filter: String,
    /// Subset of `agents` whose `key()` contains `filter` (case-insensitive).
    /// Recomputed on every filter mutation.
    pub filtered: Vec<ClassSelector>,
}

impl AgentPickerState {
    #[must_use]
    pub fn new(agents: Vec<ClassSelector>) -> Self {
        let filtered = agents.clone();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            agents,
            list_state,
            filter: String::new(),
            filtered,
        }
    }

    /// Recompute `filtered` from `agents` and the current `filter`. Anchors
    /// the selection at index 0 if the previous selection vanished.
    fn recompute_filtered(&mut self) {
        let needle = self.filter.to_ascii_lowercase();
        self.filtered = self
            .agents
            .iter()
            .filter(|agent| needle.is_empty() || agent.key().to_ascii_lowercase().contains(&needle))
            .cloned()
            .collect();
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else {
            // Always reset to the top after a filter change so the
            // operator never lands on a stale row index.
            self.list_state.select(Some(0));
        }
    }

    fn move_up(&mut self) {
        let n = self.filtered.len();
        if n > 0 {
            let next = self
                .list_state
                .selected
                .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
            self.list_state.select(Some(next));
        }
    }

    fn move_down(&mut self) {
        let n = self.filtered.len();
        if n > 0 {
            let next = self
                .list_state
                .selected
                .map_or(0, |i| if i + 1 >= n { 0 } else { i + 1 });
            self.list_state.select(Some(next));
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<ClassSelector> {
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
                    && let Some(agent) = self.filtered.get(i)
                {
                    return ModalOutcome::Commit(agent.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char(ch) => {
                // Every printable char appends to the filter — including
                // `j`/`k`, which would otherwise be ambiguous between
                // "type that letter" and "navigate the list" once the
                // filter is non-empty. Operators use the arrow keys for
                // navigation; the filter is the dominant interaction.
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
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);
const DIM_WHITE: Color = Color::Rgb(180, 180, 180);

pub fn render(frame: &mut Frame, area: Rect, state: &AgentPickerState) {
    // Title style matches the rest of the launch TUI (WHITE + BOLD)
    // so the modal feels native next to GithubPicker / WorkdirPick.
    let title_text = if state.filter.is_empty() {
        " Select Agent ".to_string()
    } else {
        format!(" Select Agent — filter: {} ", state.filter)
    };
    let title = Span::styled(
        title_text,
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Inner layout: blank / list / blank / hint — matches the canonical
    // list-modal layout used by GithubPicker / WorkdirPick.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
        ])
        .split(inner);

    let lines: Vec<Line> = if state.filtered.is_empty() {
        vec![Line::from(Span::styled(
            "  (no agents match filter)",
            Style::default().fg(DIM_WHITE),
        ))]
    } else {
        state
            .filtered
            .iter()
            .enumerate()
            .map(|(i, agent)| {
                let is_selected = Some(i) == state.list_state.selected;
                let prefix = if is_selected { "▸ " } else { "  " };
                let style = if is_selected {
                    Style::default()
                        .fg(PHOSPHOR_GREEN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WHITE)
                };
                Line::from(vec![Span::styled(
                    format!("{prefix}{}", agent.key()),
                    style,
                )])
            })
            .collect()
    };

    frame.render_widget(Paragraph::new(lines), rows[1]);

    // Hint line — canonical list-modal hint.
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Enter", key_style),
        Span::styled(" launch", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("\u{2191}\u{2193}", key_style),
        Span::styled(" navigate", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Type", key_style),
        Span::styled(" filter", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Esc", key_style),
        Span::styled(" cancel", text_style),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[3]);
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

    fn agents(keys: &[&str]) -> Vec<ClassSelector> {
        keys.iter()
            .map(|k| ClassSelector::parse(k).expect("valid selector"))
            .collect()
    }

    #[test]
    fn new_selects_first_when_non_empty() {
        let s = AgentPickerState::new(agents(&["chainargos/agent-smith", "agent-brown"]));
        assert_eq!(s.list_state.selected, Some(0));
        assert_eq!(s.filtered.len(), 2);
    }

    #[test]
    fn new_selects_nothing_when_empty() {
        let s = AgentPickerState::new(vec![]);
        assert_eq!(s.list_state.selected, None);
    }

    #[test]
    fn enter_commits_selected_agent() {
        let mut s = AgentPickerState::new(agents(&[
            "chainargos/agent-smith",
            "chainargos/agent-brown",
        ]));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome,
            ModalOutcome::Commit(a) if a.key() == "chainargos/agent-smith"));
    }

    #[test]
    fn esc_cancels() {
        let mut s = AgentPickerState::new(agents(&["agent-smith"]));
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    /// Typing into the filter narrows the visible set; agents whose key
    /// does not contain the substring are dropped.
    #[test]
    fn filter_narrows_agent_list() {
        let mut s = AgentPickerState::new(agents(&[
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
        // Selection re-anchors at index 0 of the filtered set.
        assert_eq!(s.list_state.selected, Some(0));
    }

    /// An empty filter shows every agent — equivalent to the initial
    /// state. Round-trip via Backspace must re-populate the list.
    #[test]
    fn filter_empty_shows_all() {
        let mut s = AgentPickerState::new(agents(&["agent-smith", "agent-brown"]));
        s.handle_key(key(KeyCode::Char('s')));
        // Only "agent-smith" contains 's'.
        assert_eq!(s.filtered.len(), 1);
        s.handle_key(key(KeyCode::Backspace));
        assert!(s.filter.is_empty());
        assert_eq!(s.filtered.len(), 2);
        assert_eq!(s.list_state.selected, Some(0));
    }

    /// Pressing Enter when the filter has narrowed the list to nothing
    /// is a no-op (no Commit, no Cancel) — the operator can keep typing
    /// or backspace out.
    #[test]
    fn enter_on_empty_filtered_list_is_noop() {
        let mut s = AgentPickerState::new(agents(&["agent-smith"]));
        for ch in "zzzz".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert!(s.filtered.is_empty());
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
    }

    /// Down/Up wrap around the filtered list.
    #[test]
    fn down_wraps_at_end() {
        let mut s = AgentPickerState::new(agents(&["agent-a", "agent-b"]));
        s.handle_key(key(KeyCode::Down));
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.list_state.selected, Some(0));
    }

    #[test]
    fn up_wraps_at_start() {
        let mut s = AgentPickerState::new(agents(&["agent-a", "agent-b"]));
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.list_state.selected, Some(1));
    }

    /// Printable chars always append to the filter — including `j`/`k`,
    /// which are ambiguous between navigation and filter input. Pin
    /// that the filter wins so agents with those letters in their key
    /// can be typed naturally.
    #[test]
    fn j_and_k_append_to_filter_not_navigate() {
        let mut s = AgentPickerState::new(agents(&["agent-jenkins", "agent-kafka"]));
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.filter, "j");
        assert_eq!(s.filtered.len(), 1);
        assert_eq!(s.filtered[0].key(), "agent-jenkins");
    }
}
