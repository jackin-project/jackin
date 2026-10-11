// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `TextInputState` and rendering.

use jackin_oppicker::ModalOutcome;
use ratatui::{Frame, layout::Rect};
use std::marker::PhantomData;
use termrock::{
    input::KeyEvent,
    widgets::{
        PanelChrome, TextInput, TextInputOutcome, TextInputState as CanonicalTextInputState,
        TextInputValidity, Validation,
    },
};

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
