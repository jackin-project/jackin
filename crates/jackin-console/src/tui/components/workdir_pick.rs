//! Workdir path picker — choice list of mount dsts plus each ancestor.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use jackin_tui::{ModalOutcome, shorten_home};

#[derive(Debug, Clone)]
pub struct WorkdirChoice {
    pub path: String,
    pub label: String, // e.g. "(mount dst)", "(parent)", "(root)"
}

pub trait WorkdirMount {
    fn dst(&self) -> &str;
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
    pub fn from_mounts<M: WorkdirMount>(mounts: &[M]) -> Self {
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        let home_str = home.as_ref().map(|p| p.display().to_string());
        let home_parent_str = home
            .as_ref()
            .and_then(|p| p.parent())
            .map_or_else(|| "/Users".to_string(), |p| p.display().to_string());

        let mut choices: Vec<WorkdirChoice> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::default();

        for mount in mounts {
            let dst = mount.dst();
            if seen.insert(dst.to_string()) {
                choices.push(WorkdirChoice {
                    path: dst.to_string(),
                    label: "(mount dst)".into(),
                });
            }
            let mut cursor = std::path::PathBuf::from(dst);
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

        let list_state = crate::tui::components::list_helpers::list_state_for_count(choices.len());
        Self {
            choices,
            list_state,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                crate::tui::components::list_helpers::cycle_select(
                    &mut self.list_state,
                    self.choices.len(),
                    -1,
                );
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                crate::tui::components::list_helpers::cycle_select(
                    &mut self.list_state,
                    self.choices.len(),
                    1,
                );
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(c) = crate::tui::components::list_helpers::selected_choice(
                    &self.choices,
                    self.list_state.selected,
                ) {
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
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::components::scrollable_panel::render_selected_lines_in_area;
use jackin_tui::components::{Panel, PanelFocus};
use jackin_tui::theme::{PHOSPHOR_DIM, WHITE};

pub fn render(frame: &mut Frame, area: Rect, state: &WorkdirPickState) {
    // Block title styled WHITE + BOLD to match the main-screen block titles
    // (General/Mounts/Roles) and the other modal widgets — see Panel::block().
    let block = Panel::new()
        .title(" Working directory ")
        .focus(PanelFocus::Focused)
        .block();

    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // list
        ])
        .split(inner);

    // Pre-compute display paths and column width so labels line up.
    let displays: Vec<String> = state
        .choices
        .iter()
        .map(|c| shorten_home(&c.path))
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
    render_selected_lines_in_area(frame, rows[1], lines, state.list_state.selected);
}

/// `WorkdirMount` impl for `jackin_config::MountConfig`.
/// Lives here (trait definition site) to satisfy the orphan rule.
impl WorkdirMount for jackin_config::MountConfig {
    fn dst(&self) -> &str {
        &self.dst
    }
}

#[cfg(test)]
mod tests;
