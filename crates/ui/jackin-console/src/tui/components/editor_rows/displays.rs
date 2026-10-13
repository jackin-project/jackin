// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth and secret display types.

use jackin_tui::tokens::{ACTION_ACCENT, DISCLOSURE_ACCENT};
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretValueDisplay<'a> {
    Plain(&'a str),
    OpRefPath(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceDisplay {
    NotRequired,
    OpRefPath(String),
    MaskedPlain {
        chars: usize,
    },
    Unset {
        env_name: String,
        mode_label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceFolderKind {
    Default,
    Explicit,
    Inherited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSourceFolderDisplay {
    pub kind: AuthSourceFolderKind,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceValue {
    Plain(String),
    OpRefPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthLineRow {
    AuthKind { label: String },
    WorkspaceMode { mode_label: String, inherited: bool },
    WorkspaceSource { display: AuthSourceDisplay },
    WorkspaceSourceFolder { display: AuthSourceFolderDisplay },
    RoleHeader { role: String, expanded: bool },
    RoleMode { mode_label: String },
    RoleSource { display: AuthSourceDisplay },
    RoleSourceFolder { display: AuthSourceFolderDisplay },
    AddSentinel { eligible: usize },
    Spacer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretLineRow<S> {
    Key { scope: S, key: String },
    WorkspaceAddSentinel,
    RoleHeader { role: String, expanded: bool },
    RoleAddSentinel(String),
    SectionSpacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretEnvLineFrame {
    pub cursor: usize,
    pub show_cursor: bool,
    pub area_width: u16,
}

#[must_use]
pub fn auth_source_display(
    value: Option<AuthSourceValue>,
    env_name: impl Into<String>,
    mode_label: impl Into<String>,
) -> AuthSourceDisplay {
    match value {
        Some(AuthSourceValue::Plain(value)) if !value.is_empty() => {
            AuthSourceDisplay::MaskedPlain {
                chars: value.chars().count(),
            }
        }
        Some(AuthSourceValue::OpRefPath(path)) => AuthSourceDisplay::OpRefPath(path),
        _ => AuthSourceDisplay::Unset {
            env_name: env_name.into(),
            mode_label: mode_label.into(),
        },
    }
}

#[must_use]
pub fn auth_source_display_for_required_env(
    required_env_name: Option<&str>,
    value: Option<AuthSourceValue>,
    mode_label: impl Into<String>,
) -> AuthSourceDisplay {
    let Some(env_name) = required_env_name else {
        return AuthSourceDisplay::NotRequired;
    };
    auth_source_display(value, env_name, mode_label)
}

#[must_use]
pub fn action_row_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .bg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACTION_ACCENT)
    }
}

#[must_use]
pub fn disclosure_style() -> Style {
    Style::default()
        .fg(DISCLOSURE_ACCENT)
        .add_modifier(Modifier::BOLD)
}
