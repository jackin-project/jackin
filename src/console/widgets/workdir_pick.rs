//! Workdir path picker — choice list of mount dsts plus each ancestor.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use super::ModalOutcome;
use crate::workspace::MountConfig;

#[derive(Debug, Clone)]
pub struct WorkdirChoice {
    pub path: String,
    pub label: String, // e.g. "(mount dst)", "(parent)", "(root)"
}

#[derive(Debug)]
pub struct WorkdirPickState {
    pub choices: Vec<WorkdirChoice>,
    pub list_state: ListState,
}

impl WorkdirPickState {
    /// Build choices: each mount dst followed by each of its ancestors
    /// up to `/`. Deduplicated across mounts. Labels distinguish dst
    /// vs parent vs root vs home.
    ///
    /// Excludes `/` and the literal parent of `$HOME` (typically `/Users`
    /// on macOS or `/home` on Linux) as workdir choices — they're never
    /// useful targets for a workspace workdir.
    pub fn from_mounts(mounts: &[MountConfig]) -> Self {
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        let home_str = home.as_ref().map(|p| p.display().to_string());
        let home_parent_str = home
            .as_ref()
            .and_then(|p| p.parent())
            .map_or_else(|| "/Users".to_string(), |p| p.display().to_string());

        let mut choices: Vec<WorkdirChoice> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::default();

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
                if p.is_empty() {
                    break;
                }
                if seen.insert(p.clone()) {
                    let label = if p == "/" {
                        "(root)"
                    } else if home_str.as_deref() == Some(p.as_str()) {
                        "(home)"
                    } else {
                        "(parent)"
                    };
                    choices.push(WorkdirChoice {
                        path: p,
                        label: label.into(),
                    });
                }
                cursor = parent.to_path_buf();
            }
        }

        // Filter out `/` and the parent-of-home (e.g. `/Users`, `/home`) —
        // they're never useful workdir targets.
        choices.retain(|c| c.path != "/" && c.path != home_parent_str);

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
                    return ModalOutcome::Commit(c.path.clone());
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

pub fn render(frame: &mut Frame, area: Rect, state: &WorkdirPickState) {
    // Block title styled WHITE + BOLD to match the main-screen block titles
    // (General/Mounts/Agents) and the other modal widgets.
    let title = Span::styled(
        " Working directory ",
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
    // list-modal layout used by GithubPicker.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Pre-compute display paths and column width so labels line up.
    let displays: Vec<String> = state
        .choices
        .iter()
        .map(|c| crate::tui::shorten_home(&c.path))
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
                    c.label.clone(),
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

    fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
        }
    }

    #[test]
    fn single_mount_generates_dst_plus_ancestors_minus_filtered() {
        // Intermediate ancestors are kept; `/` and the $HOME-parent are
        // always filtered out regardless of host OS.
        let mounts = vec![mount("/opt/jackin/p", "/opt/jackin/p")];
        let s = WorkdirPickState::from_mounts(&mounts);
        let paths: Vec<&str> = s.choices.iter().map(|c| c.path.as_str()).collect();
        assert!(paths.contains(&"/opt/jackin/p"));
        assert!(paths.contains(&"/opt/jackin"));
        assert!(paths.contains(&"/opt"));
        assert!(!paths.contains(&"/"), "`/` must always be filtered");
    }

    #[test]
    fn first_choice_is_dst_with_mount_dst_label() {
        let mounts = vec![mount("/opt/app", "/opt/app")];
        let s = WorkdirPickState::from_mounts(&mounts);
        assert_eq!(s.choices[0].label, "(mount dst)");
    }

    #[test]
    fn root_path_is_filtered_out() {
        let mounts = vec![mount("/opt/app", "/opt/app")];
        let s = WorkdirPickState::from_mounts(&mounts);
        assert!(
            s.choices.iter().all(|c| c.path != "/"),
            "`/` must be filtered out of the choice list: {:?}",
            s.choices
                .iter()
                .map(|c| c.path.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn home_parent_is_filtered_out() {
        // Build a mount whose dst walks through the user's $HOME so the
        // ancestor chain includes the $HOME-parent directory — which must
        // be filtered.
        let home = directories::BaseDirs::new().map_or_else(
            || "/home/test".to_string(),
            |b| b.home_dir().display().to_string(),
        );
        let dst = format!("{home}/Projects/app");
        let mounts = vec![mount(&dst, &dst)];
        let s = WorkdirPickState::from_mounts(&mounts);

        let home_parent = std::path::Path::new(&home)
            .parent()
            .map_or_else(|| "/Users".to_string(), |p| p.display().to_string());

        assert!(
            s.choices.iter().all(|c| c.path != home_parent),
            "home-parent `{home_parent}` must be filtered out of the choice list: {:?}",
            s.choices
                .iter()
                .map(|c| c.path.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            s.choices.iter().all(|c| c.path != "/"),
            "`/` must also be filtered out"
        );
    }

    #[test]
    fn home_itself_is_labelled_home_not_parent() {
        let home = directories::BaseDirs::new().map_or_else(
            || "/home/test".to_string(),
            |b| b.home_dir().display().to_string(),
        );
        let dst = format!("{home}/Projects/app");
        let mounts = vec![mount(&dst, &dst)];
        let s = WorkdirPickState::from_mounts(&mounts);

        let home_choice = s
            .choices
            .iter()
            .find(|c| c.path == home)
            .expect("home should appear in ancestor chain");
        assert_eq!(home_choice.label, "(home)");
    }

    #[test]
    fn enter_commits_selected_path() {
        let mounts = vec![mount("/opt/app", "/opt/app")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/opt/app"));
    }

    #[test]
    fn down_then_enter_picks_second_choice() {
        let mounts = vec![mount("/opt/app/sub", "/opt/app/sub")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        s.handle_key(key(KeyCode::Down));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/opt/app"));
    }

    #[test]
    fn duplicate_ancestors_across_mounts_are_deduped() {
        let mounts = vec![mount("/opt/a/b", "/opt/a/b"), mount("/opt/a/c", "/opt/a/c")];
        let s = WorkdirPickState::from_mounts(&mounts);
        let a_count = s.choices.iter().filter(|c| c.path == "/opt/a").count();
        assert_eq!(a_count, 1);
    }

    #[test]
    fn esc_cancels() {
        let mounts = vec![mount("/a", "/a")];
        let mut s = WorkdirPickState::from_mounts(&mounts);
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }
}
