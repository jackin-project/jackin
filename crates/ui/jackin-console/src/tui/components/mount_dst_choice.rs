// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::Paragraph,
};

use jackin_core::shorten_home;
use jackin_oppicker::ModalOutcome;
use termrock::layout::render_dialog_shell;
use termrock::widgets::PanelChrome;
use termrock::widgets::{Action, ActionBar, ActionBarState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDstChoice {
    SamePath,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDstFocus {
    SamePath,
    Edit,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct MountDstChoiceState {
    pub src: String,
    pub focus: MountDstFocus,
}

impl MountDstChoiceState {
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            focus: MountDstFocus::SamePath,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<MountDstChoice> {
        match key.code {
            KeyCode::Char('m' | 'M') => ModalOutcome::Commit(MountDstChoice::SamePath),
            KeyCode::Char('e' | 'E') => ModalOutcome::Commit(MountDstChoice::Edit),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l' | 'L') => {
                self.focus = match self.focus {
                    MountDstFocus::SamePath => MountDstFocus::Edit,
                    MountDstFocus::Edit => MountDstFocus::Cancel,
                    MountDstFocus::Cancel => MountDstFocus::SamePath,
                };
                ModalOutcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.focus = match self.focus {
                    MountDstFocus::SamePath => MountDstFocus::Cancel,
                    MountDstFocus::Edit => MountDstFocus::SamePath,
                    MountDstFocus::Cancel => MountDstFocus::Edit,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => match self.focus {
                MountDstFocus::SamePath => ModalOutcome::Commit(MountDstChoice::SamePath),
                MountDstFocus::Edit => ModalOutcome::Commit(MountDstChoice::Edit),
                MountDstFocus::Cancel => ModalOutcome::Cancel,
            },
            _ => ModalOutcome::Continue,
        }
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &MountDstChoiceState) {
    let inner = render_dialog_shell(
        frame,
        area,
        Some("Mount destination"),
        PanelChrome::Focused,
        &termrock::style::DesignSystem::default(),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "What would you like to do?",
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ))
        .alignment(Alignment::Center),
        chunks[1],
    );

    let shortened = shorten_home(&state.src);
    frame.render_widget(
        Paragraph::new(Span::styled(
            shortened,
            Style::default()
                .fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default())
                .add_modifier(Modifier::ITALIC),
        ))
        .alignment(Alignment::Center),
        chunks[2],
    );

    let actions = [
        Action {
            id: MountDstFocus::SamePath,
            label: "Mount at same path",
            enabled: true,
            style: None,
        },
        Action {
            id: MountDstFocus::Edit,
            label: "Edit destination",
            enabled: true,
            style: None,
        },
        Action {
            id: MountDstFocus::Cancel,
            label: "Cancel",
            enabled: true,
            style: None,
        },
    ];
    let theme = termrock::style::DesignSystem::default();
    frame.render_stateful_widget(
        &ActionBar::new(&actions, &theme).gap(" "),
        chunks[4],
        &mut ActionBarState {
            cursor: Some(state.focus),
            regions: Vec::new(),
        },
    );
}
