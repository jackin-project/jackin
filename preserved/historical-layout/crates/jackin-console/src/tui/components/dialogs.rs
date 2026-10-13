// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-owned console dialog state composed from canonical `TermRock` widgets.
//!
//! Plan 010 step 1 reviewed the named upstream pairings at rev `29a16b5b` and
//! recorded behavior-preserving non-adoptions (commit-message carve-outs):
//! `alert_dialog` paints a fixed risk-anatomy body (`build_body_text`) with no
//! host free-text override; `error_state` is a centered inline/block anatomy
//! without modal chrome; `question_flow` paints an option list, not the
//! action-row choice body; `loading_overlay`'s `BusyBoundaryState` owns
//! dismissal, which the product status overlay keeps host-side.

#[cfg(test)]
mod tests;

use std::marker::PhantomData;

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
    widgets::{
        Action, ChoiceDialog, ChoiceDialogState, Dialog, HintSpan, PanelChrome, TextInput,
        TextInputOutcome, TextInputState as CanonicalTextInputState, TextInputValidity, Validation,
    },
};

#[must_use]
pub fn text_input_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn confirm_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵"),
        HintSpan::Text("confirm"),
        HintSpan::GroupSep,
        HintSpan::Key("Y/N"),
        HintSpan::Text("choose"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn error_popup_hint_spans() -> Vec<HintSpan<'static>> {
    vec![HintSpan::Key("↵/Esc"), HintSpan::Text("dismiss")]
}

#[must_use]
pub fn save_discard_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("D"),
        HintSpan::Text("discard"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[derive(Clone)]
pub struct TextInputState<'a> {
    pub label: String,
    input: CanonicalTextInputState,
    secret: Option<termrock::widgets::PasswordInputState>,
    pub forbidden_label: String,
    _marker: PhantomData<&'a ()>,
}

impl std::fmt::Debug for TextInputState<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextInputState")
            .field("label", &self.label)
            .field(
                "value",
                &self
                    .secret
                    .as_ref()
                    .map_or(self.input.value(), |_| "[REDACTED]"),
            )
            .field("forbidden_label", &self.forbidden_label)
            .finish()
    }
}

impl TextInputState<'_> {
    #[must_use]
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        Self::new_with_forbidden(label, initial, Vec::new())
    }

    /// Credential editor with redacted Debug and protected clipboard behavior.
    #[must_use]
    pub fn new_secret(label: impl Into<String>, initial: impl Into<String>) -> Self {
        let mut state = Self::new(label, "");
        state.secret = Some(termrock::widgets::PasswordInputState::with_secret(initial));
        state
    }

    #[must_use]
    pub fn new_allow_empty(label: impl Into<String>, initial: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            input: CanonicalTextInputState::new(initial).with_allow_empty(true),
            secret: None,
            forbidden_label: String::new(),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn new_with_forbidden(
        label: impl Into<String>,
        initial: impl Into<String>,
        forbidden: Vec<String>,
    ) -> Self {
        Self {
            label: label.into(),
            input: CanonicalTextInputState::new(initial).with_forbidden(forbidden),
            secret: None,
            forbidden_label: String::new(),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn value(&self) -> String {
        self.secret.as_ref().map_or_else(
            || self.input.value().to_owned(),
            |input| input.secret().to_owned(),
        )
    }

    #[must_use]
    pub fn trimmed_value(&self) -> String {
        self.value().trim().to_owned()
    }

    #[must_use]
    pub fn is_duplicate(&self) -> bool {
        self.input.validity() == TextInputValidity::Forbidden
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.secret
            .as_ref()
            .map_or_else(|| self.input.is_valid(), |input| !input.is_empty())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        if let Some(input) = &mut self.secret {
            return match input.handle_key(key) {
                termrock::widgets::PasswordInputOutcome::Submitted => {
                    ModalOutcome::Commit(input.take_secret())
                }
                termrock::widgets::PasswordInputOutcome::Cancelled => ModalOutcome::Cancel,
                _ => ModalOutcome::Continue,
            };
        }
        match self.input.handle_key(key) {
            TextInputOutcome::Submitted(value) => ModalOutcome::Commit(value),
            TextInputOutcome::Cancelled => ModalOutcome::Cancel,
            TextInputOutcome::Ignored | TextInputOutcome::Changed => ModalOutcome::Continue,
            _ => ModalOutcome::Continue,
        }
    }
}

pub fn render_text_input(frame: &mut Frame<'_>, area: Rect, state: &TextInputState<'_>) {
    let theme = termrock::style::DesignSystem::default();
    let panel = termrock::widgets::Panel::new(&theme)
        .title(&state.label)
        .emphasis(PanelChrome::Focused);
    let inner = panel.inner(area);
    frame.render_widget(&panel, area);
    let input_area = Rect {
        x: inner.x.saturating_add(1),
        y: inner.y.saturating_add(inner.height / 2),
        width: inner.width.saturating_sub(2),
        height: 1,
    };
    if let Some(secret) = &state.secret {
        let mut secret = secret.clone();
        frame.render_stateful_widget(
            &termrock::widgets::PasswordInput::new(&state.label, &theme),
            input_area,
            &mut secret,
        );
        return;
    }
    let duplicate = state.is_duplicate();
    let duplicate_message = if state.forbidden_label.is_empty() {
        "Already exists".to_owned()
    } else {
        format!("Already exists in {}", state.forbidden_label)
    };
    let mut input = state.input.clone();
    frame.render_stateful_widget(
        &TextInput::new(&state.label, &theme)
            .placeholder("")
            .validation(if duplicate {
                Validation::Invalid(&duplicate_message)
            } else {
                Validation::Valid
            }),
        input_area,
        &mut input,
    );
}

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

fn confirm_actions() -> [Action<'static, bool>; 2] {
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

fn confirm_text(state: &ConfirmState) -> Text<'static> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardChoice {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveDiscardFocus {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct SaveDiscardState {
    pub prompt: String,
    focus: SaveDiscardFocus,
}

impl SaveDiscardState {
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            focus: SaveDiscardFocus::Cancel,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveDiscardChoice> {
        match key.code {
            KeyCode::Char('s' | 'S') => ModalOutcome::Commit(SaveDiscardChoice::Save),
            KeyCode::Char('d' | 'D') => ModalOutcome::Commit(SaveDiscardChoice::Discard),
            KeyCode::Esc | KeyCode::Char('c' | 'C') => ModalOutcome::Cancel,
            KeyCode::Left | KeyCode::BackTab => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Save,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Discard,
                };
                ModalOutcome::Continue
            }
            KeyCode::Right | KeyCode::Tab => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Discard,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Save,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => match self.focus {
                SaveDiscardFocus::Save => ModalOutcome::Commit(SaveDiscardChoice::Save),
                SaveDiscardFocus::Discard => ModalOutcome::Commit(SaveDiscardChoice::Discard),
                SaveDiscardFocus::Cancel => ModalOutcome::Cancel,
            },
            _ => ModalOutcome::Continue,
        }
    }
}

pub fn render_save_discard_dialog(frame: &mut Frame<'_>, area: Rect, state: &SaveDiscardState) {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Decision {
        Save,
        Discard,
        Cancel,
    }
    let actions = [
        Action {
            id: Decision::Save,
            label: "Save",
            enabled: true,
            style: None,
        },
        Action {
            id: Decision::Discard,
            label: "Discard",
            enabled: true,
            style: None,
        },
        Action {
            id: Decision::Cancel,
            label: "Cancel",
            enabled: true,
            style: None,
        },
    ];
    let focused = match state.focus {
        SaveDiscardFocus::Save => Decision::Save,
        SaveDiscardFocus::Discard => Decision::Discard,
        SaveDiscardFocus::Cancel => Decision::Cancel,
    };
    let theme = termrock::style::DesignSystem::default();
    let dialog = Dialog::new("Unsaved changes", Text::from(state.prompt.clone()), &theme)
        .style(Style::default())
        .emphasis(PanelChrome::Focused);
    frame.render_stateful_widget(
        &ChoiceDialog::new(dialog, &actions).gap(" "),
        area,
        &mut ChoiceDialogState::new(Some(focused)),
    );
}

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
}

impl ErrorPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<()> {
        if matches!(
            key.code,
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('o' | 'O')
        ) {
            ModalOutcome::Cancel
        } else {
            ModalOutcome::Continue
        }
    }

    #[must_use]
    pub fn required_height(&self, inner_width: u16, max_rows: u16) -> u16 {
        let width = usize::from(inner_width.max(1));
        let rows = self
            .message
            .lines()
            .map(|line| termrock::text::display_cols(line).max(1).div_ceil(width))
            .sum::<usize>();
        u16::try_from(rows.saturating_add(4))
            .unwrap_or(u16::MAX)
            .min(max_rows.max(3))
    }
}

pub fn render_error_dialog(frame: &mut Frame<'_>, area: Rect, state: &ErrorPopupState) {
    let theme = termrock::style::DesignSystem::default();
    frame.render_widget(
        Dialog::new(&state.title, Text::from(state.message.clone()), &theme)
            .style(
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Danger)
                    .fg
                    .unwrap_or_default()),
            )
            .emphasis(PanelChrome::Focused),
        area,
    );
}

#[derive(Debug, Clone)]
pub struct StatusPopupState {
    pub title: String,
    pub message: String,
}

impl StatusPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}

pub fn render_status_popup(frame: &mut Frame<'_>, area: Rect, state: &StatusPopupState) {
    let theme = termrock::style::DesignSystem::default();
    frame.render_widget(
        Dialog::new(
            &state.title,
            Text::from(vec![
                Line::from(state.message.clone()),
                Line::default(),
                Line::from("Please wait"),
            ]),
            &theme,
        )
        .style(Style::default())
        .emphasis(PanelChrome::Focused),
        area,
    );
}
