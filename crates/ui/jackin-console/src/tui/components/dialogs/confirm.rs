// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ConfirmState` and rendering.

use jackin_oppicker::ModalOutcome;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Text},
};
use termrock::{
    input::{KeyCode, KeyEvent},
    interaction::Outcome,
    widgets::{Action, ChoiceDialog, ChoiceDialogState, Dialog, PanelChrome},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    Default {
        prompt: String,
    },
    Details {
        prompt: String,
        rows: Vec<(String, String)>,
        notes: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    title: String,
    kind: ConfirmKind,
    choice: ChoiceDialogState<bool>,
}

impl ConfirmState {
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            title: "Confirm".to_owned(),
            kind: ConfirmKind::Default {
                prompt: prompt.into(),
            },
            choice: ChoiceDialogState::new(Some(false)),
        }
    }

    #[must_use]
    pub fn details(
        title: impl Into<String>,
        prompt: impl Into<String>,
        rows: Vec<(String, String)>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            title: title.into(),
            kind: ConfirmKind::Details {
                prompt: prompt.into(),
                rows,
                notes,
            },
            choice: ChoiceDialogState::new(Some(false)),
        }
    }

    #[must_use]
    pub fn with_focus_yes(mut self) -> Self {
        self.choice.cursor = Some(true);
        self
    }

    #[must_use]
    pub fn with_focus_no(mut self) -> Self {
        self.choice.cursor = Some(false);
        self
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn kind(&self) -> &ConfirmKind {
        &self.kind
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<bool> {
        let direct = match key.code {
            KeyCode::Char('y' | 'Y') => Some(true),
            KeyCode::Char('n' | 'N') => Some(false),
            _ => None,
        };
        if let Some(value) = direct {
            return ModalOutcome::Commit(value);
        }
        // Head TermRock leaves Tab/BackTab to the host; pre-bump the choice
        // dialog cycled its own actions on them.
        match key.code {
            KeyCode::Tab => {
                self.choice.select_next(&confirm_actions());
                return ModalOutcome::Continue;
            }
            KeyCode::BackTab => {
                self.choice.select_previous(&confirm_actions());
                return ModalOutcome::Continue;
            }
            _ => {}
        }
        match self.choice.handle_key(&confirm_actions(), key) {
            Outcome::Activated(value) => ModalOutcome::Commit(value),
            Outcome::Cancelled => ModalOutcome::Cancel,
            Outcome::Ignored | Outcome::Changed => ModalOutcome::Continue,
            _ => ModalOutcome::Continue,
        }
    }

    #[must_use]
    pub fn required_height(&self) -> u16 {
        let content = match &self.kind {
            ConfirmKind::Default { prompt } => prompt.lines().count().max(1),
            ConfirmKind::Details {
                prompt,
                rows,
                notes,
            } => prompt.lines().count().max(1) + rows.len() + notes.len() + 2,
        };
        u16::try_from(content.saturating_add(4)).unwrap_or(u16::MAX)
    }

    #[must_use]
    pub const fn width_pct(&self) -> u16 {
        if matches!(self.kind, ConfirmKind::Default { .. }) {
            60
        } else {
            70
        }
    }
}

pub(crate) fn confirm_actions() -> [Action<'static, bool>; 2] {
    [
        Action {
            id: true,
            label: "Yes",
            enabled: true,
            style: None,
        },
        Action {
            id: false,
            label: "No",
            enabled: true,
            style: None,
        },
    ]
}

pub(crate) fn confirm_text(state: &ConfirmState) -> Text<'static> {
    match &state.kind {
        ConfirmKind::Default { prompt } => Text::from(prompt.clone()),
        ConfirmKind::Details {
            prompt,
            rows,
            notes,
        } => {
            let mut lines = vec![Line::from(prompt.clone()), Line::default()];
            lines.extend(
                rows.iter()
                    .map(|(label, value)| Line::from(format!("{label}: {value}"))),
            );
            if !notes.is_empty() {
                lines.push(Line::default());
                lines.extend(notes.iter().cloned().map(Line::from));
            }
            Text::from(lines)
        }
    }
}

pub fn render_confirm_dialog(frame: &mut Frame<'_>, area: Rect, state: &ConfirmState) {
    let actions = confirm_actions();
    let mut choice = state.choice.clone();
    let theme = termrock::style::DesignSystem::default();
    let dialog = Dialog::new(&state.title, confirm_text(state), &theme)
        .style(Style::default())
        .emphasis(PanelChrome::Focused);
    frame.render_stateful_widget(
        &ChoiceDialog::new(dialog, &actions).gap(" "),
        area,
        &mut choice,
    );
}
