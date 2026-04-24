//! Workdir path picker — choice list of mount dsts plus each ancestor.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use crate::workspace::MountConfig;
use super::ModalOutcome;

#[derive(Debug, Clone)]
pub struct WorkdirChoice {
    pub path: String,
    pub label: String,  // e.g. "(mount dst)", "(parent)", "(root)"
}

#[derive(Debug)]
pub struct WorkdirPickState {
    pub choices: Vec<WorkdirChoice>,
    pub list_state: ListState,
}

impl WorkdirPickState {
    /// Build choices: each mount dst followed by each of its ancestors
    /// up to `/`. Deduplicated across mounts. Labels distinguish dst
    /// vs parent vs root.
    pub fn from_mounts(mounts: &[MountConfig]) -> Self {
        let mut choices: Vec<WorkdirChoice> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = Default::default();

        for m in mounts {
            if seen.insert(m.dst.clone()) {
                choices.push(WorkdirChoice {
                    path: m.dst.clone(),
                    label: "(mount dst)".into(),
                });
            }
            let mut cursor = std::path::PathBuf::from(&m.dst);
            while let Some(parent) = cursor.parent() {
                let p = parent.display().to_string();
                if p.is_empty() { break; }
                if seen.insert(p.clone()) {
                    let label = if p == "/" { "(root)" } else { "(parent)" };
                    choices.push(WorkdirChoice { path: p, label: label.into() });
                }
                cursor = parent.to_path_buf();
            }
        }

        let mut list_state = ListState::default();
        if !choices.is_empty() {
            list_state.select(Some(0));
        }
        Self { choices, list_state }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let n = self.choices.len();
                if n > 0 {
                    let next = self.list_state.selected
                        .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
                    self.list_state.select(Some(next));
                }
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let n = self.choices.len();
                if n > 0 {
                    let next = self.list_state.selected
                        .map_or(0, |i| if i + 1 >= n { 0 } else { i + 1 });
                    self.list_state.select(Some(next));
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(i) = self.list_state.selected {
                    if let Some(c) = self.choices.get(i) {
                        return ModalOutcome::Commit(c.path.clone());
                    }
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

use ratatui::{Frame, layout::Rect, style::{Color, Style}, widgets::{Block, Borders, Paragraph}, text::Line};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);

pub fn render(frame: &mut Frame, area: Rect, state: &WorkdirPickState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title("Workdir — pick from mounts");

    let lines: Vec<Line> = state.choices.iter().enumerate().map(|(i, c)| {
        let prefix = if Some(i) == state.list_state.selected { "▸ " } else { "  " };
        Line::from(format!("{}{}  {}", prefix, c.path, c.label))
    }).collect();

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));

    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig { src: src.into(), dst: dst.into(), readonly: false }
    }

    #[test]
    fn single_mount_generates_dst_plus_ancestors() {
        let mounts = vec![mount("/home/x/p", "/home/x/p")];
        let s = WorkdirPickState::from_mounts(&mounts);
        let paths: Vec<&str> = s.choices.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, vec!["/home/x/p", "/home/x", "/home", "/"]);
    }

    #[test]
    fn first_choice_is_dst_with_mount_dst_label() {
        let mounts = vec![mount("/a", "/a")];
        let s = WorkdirPickState::from_mounts(&mounts);
        assert_eq!(s.choices[0].label, "(mount dst)");
    }

    #[test]
    fn root_choice_is_labelled_root() {
        let mounts = vec![mount("/a", "/a")];
        let s = WorkdirPickState::from_mounts(&mounts);
        assert_eq!(s.choices.last().unwrap().label, "(root)");
    }

    #[test]
    fn enter_commits_selected_path() {
        let mounts = vec![mount("/a", "/a")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/a"));
    }

    #[test]
    fn down_then_enter_picks_second_choice() {
        let mounts = vec![mount("/a/b", "/a/b")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        s.handle_key(key(KeyCode::Down));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/a"));
    }

    #[test]
    fn duplicate_ancestors_across_mounts_are_deduped() {
        let mounts = vec![mount("/a/b", "/a/b"), mount("/a/c", "/a/c")];
        let s = WorkdirPickState::from_mounts(&mounts);
        let a_count = s.choices.iter().filter(|c| c.path == "/a").count();
        assert_eq!(a_count, 1);
    }

    #[test]
    fn esc_cancels() {
        let mounts = vec![mount("/a", "/a")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), ModalOutcome::Cancel));
    }
}
