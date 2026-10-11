// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth-form source, credential, and button lines.

use super::{
    AUTH_FORM_CREDENTIAL_LABEL_WIDTH, AuthCredential, AuthCredentialRef, AuthForm, CredentialInput,
};

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tui::components::editor_rows::{AuthSourceFolderKind, cursor_span};
use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;

use crate::tui::screens::settings::model::AuthFormFocus;

pub(crate) fn source_folder_line<V: AuthCredential>(
    form: &AuthForm<V>,
    selected: bool,
) -> Line<'static> {
    let label_style = if selected {
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong)
    } else {
        Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default())
    };
    Line::from(vec![
        cursor_span(selected),
        Span::styled(
            format!("{:<AUTH_FORM_CREDENTIAL_LABEL_WIDTH$}", "Source folder"),
            label_style,
        ),
        Span::raw(" "),
        Span::styled(
            source_folder_text(form),
            termrock::style::DesignSystem::default().style(termrock::style::Role::Accent),
        ),
    ])
}

pub(crate) fn source_folder_text<V: AuthCredential>(form: &AuthForm<V>) -> String {
    if let Some(path) = &form.source_folder {
        return path.display().to_string();
    }
    let Some(display) = &form.source_folder_fallback else {
        return String::new();
    };
    match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    }
}

pub(crate) fn credential_env_line<R: AuthCredentialRef>(
    env_var: &str,
    credential: &CredentialInput<R>,
    selected: bool,
) -> Line<'static> {
    let label_style = if selected {
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong)
    } else {
        Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default())
    };
    let mut spans = vec![
        cursor_span(selected),
        Span::styled(
            format!("{env_var:<AUTH_FORM_CREDENTIAL_LABEL_WIDTH$}"),
            label_style,
        ),
        Span::raw(" "),
    ];
    match credential {
        CredentialInput::None => {
            spans.push(Span::styled(
                "required".to_owned(),
                termrock::style::DesignSystem::default().style(termrock::style::Role::Danger),
            ));
        }
        CredentialInput::Literal(value) => {
            let masked = if value.is_empty() {
                "required".to_owned()
            } else {
                "●".repeat(value.chars().count().clamp(1, 12))
            };
            let style = if value.is_empty() {
                termrock::style::DesignSystem::default().style(termrock::style::Role::Danger)
            } else {
                termrock::style::DesignSystem::default().style(termrock::style::Role::Accent)
            };
            spans.push(Span::styled(masked, style));
        }
        CredentialInput::OpRef(value) => {
            push_op_breadcrumb_spans(&mut spans, value.path());
        }
    }
    Line::from(spans)
}

pub(crate) fn action_buttons_line(
    can_save: bool,
    focus: AuthFormFocus,
    github: bool,
) -> Line<'static> {
    let save_style = if can_save {
        Style::default()
            .fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default())
            .add_modifier(Modifier::DIM)
    };
    Line::from(vec![
        Span::styled(
            "  Save  ".to_owned(),
            selected_button_style(focus == AuthFormFocus::Save, save_style),
        ),
        Span::raw("    "),
        Span::styled(
            "  Cancel  ".to_owned(),
            selected_button_style(
                focus == AuthFormFocus::Cancel,
                termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
            ),
        ),
        Span::raw("    "),
        Span::styled(
            (if github {
                "  Reset  "
            } else {
                "  Delete account  "
            })
            .to_owned(),
            selected_button_style(
                focus == AuthFormFocus::Reset,
                termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
            ),
        ),
    ])
}

pub(crate) fn label_style() -> Style {
    termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong)
}

pub(crate) fn selected_button_style(selected: bool, style: Style) -> Style {
    if selected {
        style
            .bg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default())
            .fg(Color::Black)
    } else {
        style
    }
}
