// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Three-button choice modal for picking a mount destination.
//!
//! Most operator mounts want `dst = src`. This modal offers a fast path
//! (`Mount at same path`) for that common case and falls back to the
//! text-input flow via `Edit destination` when the operator wants a
//! different container path.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::Paragraph,
};

use jackin_core::shorten_home;
use jackin_tui::ModalOutcome;
use jackin_tui::components::{DialogBorder, render_dialog_shell};
use jackin_tui::theme::PHOSPHOR_DIM;

/// Outcome of the mount-destination modal.
///
/// The button label reads "Mount at same path" — the variant name
/// mirrors that intent so grep'ing for `Ok` doesn't conflate this
/// modal's choice with `Result::Ok`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDstChoice {
    /// Use the host source path verbatim as the container destination.
    SamePath,
    /// Open the destination text-input so the user can pick a different
    /// container path.
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
    /// Default focus = `SamePath`: the common case is "same path inside
    /// the container", so Enter should commit that without extra effort.
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
        DialogBorder::Default,
    );

    // Canonical dialog layout: leading spacer + content + spacer + buttons + trailing spacer.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // leading spacer
            Constraint::Length(1), // question
            Constraint::Length(1), // src path
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Length(1), // trailing spacer
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "What would you like to do?",
            jackin_tui::theme::BOLD_WHITE,
        ))
        .alignment(Alignment::Center),
        chunks[1],
    );

    // Host path line — the operator-picked source.
    let shortened = shorten_home(&state.src);
    frame.render_widget(
        Paragraph::new(Span::styled(
            shortened,
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        ))
        .alignment(Alignment::Center),
        chunks[2],
    );

    let items = [
        jackin_tui::components::ButtonStripItem::new("Mount at same path"),
        jackin_tui::components::ButtonStripItem::new("Edit destination"),
        jackin_tui::components::ButtonStripItem::new("Cancel"),
    ];
    let focused = match state.focus {
        MountDstFocus::SamePath => 0,
        MountDstFocus::Edit => 1,
        MountDstFocus::Cancel => 2,
    };
    frame.render_widget(
        jackin_tui::components::ButtonStrip::new(&items).focused(focused),
        chunks[4],
    );
}

#[cfg(test)]
mod tests;
