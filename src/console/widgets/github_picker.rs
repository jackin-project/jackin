//! Modal picker for "open in GitHub" when a workspace has multiple
//! GitHub-hosted mounts.
//!
//! Mirrors `WorkdirPickState`'s shape — one `Vec`-driven list +
//! `tui_widget_list::ListState` — so the rest of the launch TUI can
//! dispatch it with the same Up/Down/Enter pattern.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use super::ModalOutcome;

/// One picker row. `src` is the operator-facing host path; `branch` is
/// the resolved HEAD/detached label (already formatted); `url` is the
/// web URL to hand to `open::that_detached` on commit.
#[derive(Debug, Clone)]
pub struct GithubChoice {
    pub src: String,
    pub branch: String,
    pub url: String,
}

#[derive(Debug)]
pub struct GithubPickerState {
    pub choices: Vec<GithubChoice>,
    pub list_state: ListState,
}

impl GithubPickerState {
    pub fn new(choices: Vec<GithubChoice>) -> Self {
        let mut list_state = ListState::default();
        if !choices.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            choices,
            list_state,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                super::cycle_select(&mut self.list_state, self.choices.len(), -1);
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                super::cycle_select(&mut self.list_state, self.choices.len(), 1);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(i) = self.list_state.selected
                    && let Some(c) = self.choices.get(i)
                {
                    return ModalOutcome::Commit(c.url.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
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

pub fn render(frame: &mut Frame, area: Rect, state: &GithubPickerState) {
    // Title style matches WorkdirPick (WHITE + BOLD) so the modal feels
    // native next to the rest of the launch TUI.
    let title = Span::styled(
        " Open in GitHub ",
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
    // list-modal layout used by WorkdirPick.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Pre-compute shortened src + width so the `· github · <branch>` suffix
    // lines up across rows.
    let displays: Vec<String> = state
        .choices
        .iter()
        .map(|c| crate::tui::shorten_home(&c.src))
        .collect();
    let path_w = displays
        .iter()
        .map(|d| d.chars().count())
        .max()
        .unwrap_or(0)
        .max(10);

    let lines: Vec<Line> = state
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let prefix = if Some(i) == state.list_state.selected {
                "▸ "
            } else {
                "  "
            };
            let display = &displays[i];
            let pad = path_w.saturating_sub(display.chars().count());
            Line::from(vec![
                Span::styled(format!("{prefix}{display}"), Style::default().fg(WHITE)),
                Span::raw(format!("{}  ", " ".repeat(pad))),
                Span::styled(
                    format!("github \u{b7} {}", c.branch),
                    Style::default()
                        .fg(PHOSPHOR_DIM)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), rows[1]);

    // Hint line — canonical list-modal hint (↑↓ navigate · Enter confirm · Esc cancel).
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("\u{2191}\u{2193}", key_style),
        Span::styled(" navigate", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Enter", key_style),
        Span::styled(" confirm", text_style),
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

    fn choice(src: &str, branch: &str, url: &str) -> GithubChoice {
        GithubChoice {
            src: src.into(),
            branch: branch.into(),
            url: url.into(),
        }
    }

    #[test]
    fn new_selects_first_choice_when_non_empty() {
        let s = GithubPickerState::new(vec![
            choice("/a", "main", "https://github.com/o/a/tree/main"),
            choice("/b", "main", "https://github.com/o/b/tree/main"),
        ]);
        assert_eq!(s.list_state.selected, Some(0));
    }

    #[test]
    fn new_selects_nothing_when_empty() {
        let s = GithubPickerState::new(vec![]);
        assert_eq!(s.list_state.selected, None);
    }

    #[test]
    fn enter_commits_selected_url() {
        // Default selection is index 0 — Enter returns the first URL.
        let mut s = GithubPickerState::new(vec![
            choice("/a", "main", "https://github.com/o/a/tree/main"),
            choice("/b", "dev", "https://github.com/o/b/tree/dev"),
        ]);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome,
            ModalOutcome::Commit(v) if v == "https://github.com/o/a/tree/main"));
    }

    #[test]
    fn down_then_enter_resolves_second_url() {
        // Pin that Enter commits the URL at the *current* selection, not a
        // stale index.
        let mut s = GithubPickerState::new(vec![
            choice("/a", "main", "https://github.com/o/a/tree/main"),
            choice("/b", "dev", "https://github.com/o/b/tree/dev"),
        ]);
        s.handle_key(key(KeyCode::Down));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome,
            ModalOutcome::Commit(v) if v == "https://github.com/o/b/tree/dev"));
    }

    #[test]
    fn down_wraps_at_end() {
        let mut s = GithubPickerState::new(vec![
            choice("/a", "main", "https://github.com/o/a/tree/main"),
            choice("/b", "dev", "https://github.com/o/b/tree/dev"),
        ]);
        s.handle_key(key(KeyCode::Down));
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.list_state.selected, Some(0));
    }

    #[test]
    fn up_wraps_at_start() {
        let mut s = GithubPickerState::new(vec![
            choice("/a", "main", "https://github.com/o/a/tree/main"),
            choice("/b", "dev", "https://github.com/o/b/tree/dev"),
        ]);
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.list_state.selected, Some(1));
    }

    #[test]
    fn esc_cancels() {
        let mut s = GithubPickerState::new(vec![choice(
            "/a",
            "main",
            "https://github.com/o/a/tree/main",
        )]);
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn enter_on_empty_list_is_continue() {
        let mut s = GithubPickerState::new(vec![]);
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Continue
        ));
    }
}
