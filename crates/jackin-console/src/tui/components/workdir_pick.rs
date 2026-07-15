// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workdir path picker — choice list of mount dsts plus each ancestor.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use jackin_core::shorten_home;
use jackin_tui::ModalOutcome;

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
    #[expect(
        clippy::excessive_nesting,
        reason = "Workdir-picker choice builder: per-mount + per-ancestor nested \
              loop with `if seen.insert` dedup + `match` on label type. The \
              nesting is the per-ancestor dedup protocol."
    )]
    pub fn from_mounts<M: WorkdirMount>(mounts: &[M]) -> Self {
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        let home_str = home.as_ref().map(|p| p.display().to_string());
        let home_parent_str = home
            .as_ref()
            .and_then(|p| p.parent())
            .map_or_else(|| "/Users".to_owned(), |p| p.display().to_string());

        let mut choices: Vec<WorkdirChoice> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::default();

        for mount in mounts {
            let dst = mount.dst();
            if seen.insert(dst.to_owned()) {
                choices.push(WorkdirChoice {
                    path: dst.to_owned(),
                    label: "(mount dst)".into(),
                });
            }
            let mut cursor = PathBuf::from(dst);
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

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        crate::tui::components::list_helpers::scroll_select(
            &mut self.list_state,
            self.choices.len(),
            delta,
        )
    }
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::components::render_picker_lines;
use jackin_tui::components::{DialogBorder, render_dialog_shell};
use termrock::style::{PHOSPHOR_DIM, WHITE};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &WorkdirPickState) {
    let inner = render_dialog_shell(
        frame,
        area,
        Some("Working directory"),
        DialogBorder::Default,
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Min(1),    // list
        ])
        .split(inner);

    if state.choices.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "no directories",
                termrock::style::DIM,
            )))
            .alignment(ratatui::layout::Alignment::Center),
            rows[1],
        );
        return;
    }
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

    let lines: Vec<Line<'_>> = state
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let display = &displays[i];
            let pad = path_w.saturating_sub(display.chars().count());
            Line::from(vec![
                Span::styled(display.to_owned(), Style::default().fg(WHITE)),
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
    render_picker_lines(
        rows[1],
        frame.buffer_mut(),
        lines,
        state.list_state.selected,
    );
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
