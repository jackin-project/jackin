// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `AuthForm` rendering and line builder.

use super::{
    AUTH_FORM_MODE_LABEL_WIDTH, AuthCredential, AuthForm, action_buttons_line, credential_env_line,
    label_style, mode_str, source_folder_line,
};

use crate::tui::auth::{AuthKind, AuthMode};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::components::editor_rows::cursor_span;

use crate::tui::screens::settings::model::AuthFormFocus;

/// Render the auth-edit modal for `form` into `area`.
pub fn render_form<V: AuthCredential>(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &AuthForm<V>,
    focus: AuthFormFocus,
) {
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some("Edit auth"),
        termrock::widgets::PanelChrome::Focused,
        &termrock::style::DesignSystem::default(),
    );

    for (idx, row) in build_form_lines(form, focus).into_iter().enumerate() {
        let y = inner.y.saturating_add(idx as u16);
        if y >= inner.y.saturating_add(inner.height) {
            break;
        }
        let row_area = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        };
        let alignment = if row.centered {
            Alignment::Center
        } else {
            Alignment::Left
        };
        frame.render_widget(Paragraph::new(row.line).alignment(alignment), row_area);
    }
}

pub(crate) struct FormLine {
    line: Line<'static>,
    centered: bool,
}

impl FormLine {
    pub(crate) const fn left(line: Line<'static>) -> Self {
        Self {
            line,
            centered: false,
        }
    }

    pub(crate) const fn centered(line: Line<'static>) -> Self {
        Self {
            line,
            centered: true,
        }
    }
}

/// Total rendered rows the auth-edit modal needs.
#[must_use]
pub fn required_height<V: AuthCredential>(form: &AuthForm<V>) -> u16 {
    let mut inner: u16 = 5;
    if form.shows_source_folder() {
        inner += 1;
    }
    if form.shows_credential_block() {
        inner += 1;
    }
    inner + 2
}

pub(crate) fn build_form_lines<V: AuthCredential>(
    form: &AuthForm<V>,
    focus: AuthFormFocus,
) -> Vec<FormLine> {
    let mut lines: Vec<FormLine> = Vec::new();

    lines.push(FormLine::left(Line::from("")));

    let mode_text = if form.kind == AuthKind::Github && form.mode == Some(AuthMode::Sync) {
        "sync"
    } else {
        form.mode.map_or("(unset)", mode_str)
    };
    lines.push(FormLine::left(Line::from(vec![
        cursor_span(focus == AuthFormFocus::Mode),
        Span::styled(
            format!("{:<AUTH_FORM_MODE_LABEL_WIDTH$}", "Mode"),
            label_style(),
        ),
        Span::raw(" "),
        Span::styled(
            mode_text.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::Accent),
        ),
    ])));

    if form.shows_source_folder() {
        lines.push(FormLine::left(source_folder_line(
            form,
            focus == AuthFormFocus::SourceFolder,
        )));
    }

    if form.shows_credential_block()
        && let Some(env_var) = form.mode.and_then(|mode| form.kind.required_env_var(mode))
    {
        lines.push(FormLine::left(credential_env_line(
            env_var,
            &form.credential,
            matches!(focus, AuthFormFocus::CredentialSource),
        )));
    }

    lines.push(FormLine::left(Line::from("")));
    lines.push(FormLine::centered(action_buttons_line(
        form.can_save(),
        focus,
        form.kind == AuthKind::Github,
    )));
    lines.push(FormLine::left(Line::from("")));
    lines
}
