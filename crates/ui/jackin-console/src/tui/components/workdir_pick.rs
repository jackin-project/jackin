// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use jackin_core::shorten_home;
use jackin_oppicker::ModalOutcome;
use termrock::widgets::ListState;

#[derive(Debug, Clone)]
pub struct WorkdirChoice {
    pub path: String,
    pub label: String,
}

pub trait WorkdirMount {
    fn dst(&self) -> &str;
}

#[derive(Debug)]
pub struct WorkdirPickState {
    pub choices: Vec<WorkdirChoice>,
    pub list_state: ListState<usize>,
}

impl WorkdirPickState {
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

        choices.retain(|c| c.path != "/" && c.path != home_parent_str);

        let list_state = ListState::for_count(choices.len());
        Self {
            choices,
            list_state,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.list_state.cycle_index(self.choices.len(), -1);
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.list_state.cycle_index(self.choices.len(), 1);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(c) = self.list_state.selected_item(&self.choices) {
                    return ModalOutcome::Commit(c.path.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        self.list_state
            .move_index(self.choices.len(), isize::from(delta))
    }
}

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::StatefulWidget,
};

use termrock::layout::render_dialog_shell;
use termrock::widgets::{List, ListRow, Panel, PanelChrome};

const WORKDIR_PICK_TITLE: &str = "Working directory";

fn workdir_inner_area(area: Rect) -> Rect {
    let theme = termrock::style::DesignSystem::default();
    Panel::new(&theme)
        .emphasis(PanelChrome::Focused)
        .title(WORKDIR_PICK_TITLE)
        .block()
        .inner(area)
}

fn workdir_list_area(inner: Rect) -> Rect {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    rows[1]
}

fn workdir_rows(state: &WorkdirPickState) -> Vec<ListRow<'_, usize>> {
    let displays: Vec<String> = state
        .choices
        .iter()
        .map(|choice| shorten_home(&choice.path))
        .collect();
    let path_width = displays
        .iter()
        .map(|display| display.chars().count())
        .max()
        .unwrap_or(0)
        .max(10);

    state
        .choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let display = &displays[index];
            let pad = path_width.saturating_sub(display.chars().count());
            ListRow::item(
                index,
                Line::from(vec![
                    Span::styled(
                        display.to_owned(),
                        Style::default().fg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Text)
                            .fg
                            .unwrap_or_default()),
                    ),
                    Span::raw(format!("{}  ", " ".repeat(pad))),
                    Span::styled(
                        choice.label.clone(),
                        Style::default()
                            .fg(termrock::style::DesignSystem::default()
                                .style(termrock::style::Role::TextMuted)
                                .fg
                                .unwrap_or_default())
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]),
            )
        })
        .collect()
}

pub(in crate::tui) fn selection_hit_at(
    area: Rect,
    state: &WorkdirPickState,
    position: ratatui::layout::Position,
) -> bool {
    if area.width == 0 || area.height == 0 || state.choices.is_empty() {
        return false;
    }

    let list_area = workdir_list_area(workdir_inner_area(area));
    if list_area.width == 0 || list_area.height == 0 {
        return false;
    }

    let theme = termrock::style::DesignSystem::default();
    let rows = workdir_rows(state);
    let mut list_state = ListState::new(state.list_state.selected().copied());
    let mut buffer = Buffer::empty(list_area);
    (&List::new(&rows, &theme)).render(list_area, &mut buffer, &mut list_state);

    list_state
        .regions()
        .iter()
        .any(|region| region.area.contains(position))
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &WorkdirPickState) {
    let theme = termrock::style::DesignSystem::default();
    let inner = render_dialog_shell(
        frame,
        area,
        Some(WORKDIR_PICK_TITLE),
        PanelChrome::Focused,
        &theme,
    );
    let list_area = workdir_list_area(inner);

    if state.choices.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "no directories",
                theme.style(termrock::style::Role::TextMuted),
            )))
            .alignment(ratatui::layout::Alignment::Center),
            list_area,
        );
        return;
    }
    let items = workdir_rows(state);
    frame.render_stateful_widget(
        &List::new(&items, &theme),
        list_area,
        &mut ListState::new(state.list_state.selected().copied()),
    );
}

impl WorkdirMount for jackin_config::MountConfig {
    fn dst(&self) -> &str {
        &self.dst
    }
}
