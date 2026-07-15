// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Auth edit-form state and rendering.

use std::marker::PhantomData;
use std::path::PathBuf;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::auth::{
    AuthKind, AuthMode, auth_mode_requires_credential, auth_mode_supports_source_folder,
};
use crate::tui::components::editor_rows::{
    AuthSourceFolderDisplay, AuthSourceFolderKind, cursor_span,
};
use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;
use crate::tui::components::source_picker::SourcePickerState;
use crate::tui::screens::settings::model::AuthFormFocus;
use jackin_tui::components::TextInputState;
use termrock::style::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

// Structural exception: auth panels are multi-field credential forms with
// breadcrumb, source, input, and action rows, so they cannot use the flat picker
// renderer even though they share its focus-gated cursor contract.

pub trait AuthCredentialRef: Clone + std::fmt::Debug + PartialEq + Eq {
    fn path(&self) -> &str;

    fn is_empty(&self) -> bool {
        self.path().is_empty()
    }
}

pub trait AuthCredential: Clone + std::fmt::Debug + PartialEq + Eq {
    type Ref: AuthCredentialRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref>;
    fn from_plain(value: String) -> Self;
    fn from_op_ref(value: Self::Ref) -> Self;
}

/// What the user has supplied in the credential block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialInput<R> {
    None,
    Literal(String),
    OpRef(R),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFormKeyPlan {
    Stay,
    Focus(AuthFormFocus),
    CycleMode,
    OpenCredentialSource,
    OpenSourceFolderBrowser,
    Save,
    Cancel,
    Reset,
}

#[must_use]
pub fn auth_source_picker_state(
    env_var: impl Into<String>,
    op_available: bool,
) -> SourcePickerState {
    SourcePickerState::new(env_var.into(), op_available)
}

#[must_use]
pub fn generated_token_source_picker_state(op_available: bool) -> SourcePickerState {
    auth_source_picker_state("generated token", op_available)
}

#[must_use]
pub fn generated_token_op_item_name(item_template: &str, scope_label: &str) -> String {
    item_template.replace("{ws}", scope_label)
}

#[must_use]
pub fn auth_credential_input_state<'a>(literal: impl Into<String>) -> TextInputState<'a> {
    TextInputState::new("Credential", literal)
}

#[must_use]
pub fn auth_panel_title(kind_label: &str) -> String {
    format!(" {kind_label} ")
}

#[must_use]
pub const fn auth_form_key_plan(
    focus: AuthFormFocus,
    key: KeyCode,
    shows_credential_block: bool,
    can_save: bool,
) -> AuthFormKeyPlan {
    auth_form_key_plan_with_source_folder(focus, key, false, shows_credential_block, can_save)
}

#[must_use]
pub const fn auth_form_key_plan_with_source_folder(
    focus: AuthFormFocus,
    key: KeyCode,
    shows_source_folder: bool,
    shows_credential_block: bool,
    can_save: bool,
) -> AuthFormKeyPlan {
    match focus {
        AuthFormFocus::Mode => match key {
            KeyCode::Char(' ') => AuthFormKeyPlan::CycleMode,
            KeyCode::Down | KeyCode::Char('j') if shows_source_folder => {
                AuthFormKeyPlan::Focus(AuthFormFocus::SourceFolder)
            }
            KeyCode::Down | KeyCode::Char('j') if shows_credential_block => {
                AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
            }
            KeyCode::Tab => {
                if shows_source_folder {
                    AuthFormKeyPlan::Focus(AuthFormFocus::SourceFolder)
                } else if shows_credential_block {
                    AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
                } else {
                    AuthFormKeyPlan::Focus(AuthFormFocus::Save)
                }
            }
            KeyCode::BackTab => AuthFormKeyPlan::Focus(AuthFormFocus::Reset),
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::SourceFolder => match key {
            KeyCode::Enter => AuthFormKeyPlan::OpenSourceFolderBrowser,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                if shows_credential_block {
                    AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
                } else {
                    AuthFormKeyPlan::Focus(AuthFormFocus::Save)
                }
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                AuthFormKeyPlan::Focus(AuthFormFocus::Mode)
            }
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::CredentialSource => match key {
            KeyCode::Enter => AuthFormKeyPlan::OpenCredentialSource,
            KeyCode::Tab => AuthFormKeyPlan::Focus(AuthFormFocus::Save),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                if shows_source_folder {
                    AuthFormKeyPlan::Focus(AuthFormFocus::SourceFolder)
                } else {
                    AuthFormKeyPlan::Focus(AuthFormFocus::Mode)
                }
            }
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::Save => match key {
            KeyCode::Right | KeyCode::Tab => AuthFormKeyPlan::Focus(AuthFormFocus::Cancel),
            KeyCode::BackTab => {
                if shows_credential_block {
                    AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
                } else if shows_source_folder {
                    AuthFormKeyPlan::Focus(AuthFormFocus::SourceFolder)
                } else {
                    AuthFormKeyPlan::Focus(AuthFormFocus::Mode)
                }
            }
            KeyCode::Enter if can_save => AuthFormKeyPlan::Save,
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::Cancel => match key {
            KeyCode::Left | KeyCode::BackTab => AuthFormKeyPlan::Focus(AuthFormFocus::Save),
            KeyCode::Right | KeyCode::Tab => AuthFormKeyPlan::Focus(AuthFormFocus::Reset),
            KeyCode::Enter => AuthFormKeyPlan::Cancel,
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::Reset => match key {
            KeyCode::Left | KeyCode::BackTab => AuthFormKeyPlan::Focus(AuthFormFocus::Cancel),
            KeyCode::Tab => AuthFormKeyPlan::Focus(AuthFormFocus::Mode),
            KeyCode::Enter => AuthFormKeyPlan::Reset,
            _ => AuthFormKeyPlan::Stay,
        },
    }
}

/// The form's mutable state. Mode and credential are independently editable;
/// only the [`AuthForm::can_save`] invariant decides whether the parent should
/// allow the Save action.
#[derive(Debug)]
pub struct AuthForm<V: AuthCredential> {
    pub kind: AuthKind,
    pub mode: Option<AuthMode>,
    pub credential: CredentialInput<V::Ref>,
    pub source_folder: Option<PathBuf>,
    pub source_folder_fallback: Option<AuthSourceFolderDisplay>,
    _value: PhantomData<fn() -> V>,
}

/// Output of a successful commit. The parent uses these fields to write the
/// kind block and the env-var entry at the chosen layer.
#[derive(Debug, Clone)]
pub struct AuthFormOutcome<V> {
    pub mode: AuthMode,
    pub env_var_name: Option<&'static str>,
    pub env_value: Option<V>,
    pub source_folder: Option<PathBuf>,
}

impl<V: AuthCredential> AuthForm<V> {
    pub const fn new(kind: AuthKind) -> Self {
        Self {
            kind,
            mode: None,
            credential: CredentialInput::None,
            source_folder: None,
            source_folder_fallback: None,
            _value: PhantomData,
        }
    }

    /// Pre-populate the form from an existing row's mode and credential.
    pub fn from_existing(kind: AuthKind, mode: AuthMode, credential: Option<V>) -> Self {
        let credential = credential.map_or(CredentialInput::None, V::into_credential_input);
        Self {
            kind,
            mode: Some(mode),
            credential,
            source_folder: None,
            source_folder_fallback: None,
            _value: PhantomData,
        }
    }

    #[must_use]
    pub fn with_source_folder(
        mut self,
        source_folder: Option<PathBuf>,
        fallback: Option<AuthSourceFolderDisplay>,
    ) -> Self {
        self.source_folder = source_folder;
        self.source_folder_fallback = fallback;
        self
    }

    /// Set the mode. If switching to a mode that doesn't need a credential,
    /// clears the credential field automatically.
    pub fn set_mode(&mut self, mode: AuthMode) {
        debug_assert!(
            self.kind.supported_modes().contains(&mode),
            "AuthMode::{mode:?} not supported by AuthKind::{:?}",
            self.kind,
        );
        self.mode = Some(mode);
        if !mode_requires_credential(self.kind, mode) {
            self.credential = CredentialInput::None;
        }
    }

    pub fn set_literal(&mut self, value: String) {
        self.credential = CredentialInput::Literal(value);
    }

    pub fn literal_buffer(&self) -> String {
        match &self.credential {
            CredentialInput::Literal(value) => value.clone(),
            CredentialInput::None | CredentialInput::OpRef(_) => String::new(),
        }
    }

    pub fn set_op_ref(&mut self, value: V::Ref) {
        self.credential = CredentialInput::OpRef(value);
    }

    pub fn set_source_folder(&mut self, value: PathBuf) {
        self.source_folder = Some(value);
    }

    /// Whether the source-folder row should be shown.
    pub fn shows_source_folder(&self) -> bool {
        (self.source_folder.is_some() || self.source_folder_fallback.is_some())
            && matches!(self.mode, Some(mode) if auth_mode_supports_source_folder(self.kind, mode))
    }

    /// Whether the credential input block should be shown.
    pub const fn shows_credential_block(&self) -> bool {
        matches!(self.mode, Some(mode) if mode_requires_credential(self.kind, mode))
    }

    pub fn cycle_mode(&mut self) {
        let modes = self.available_modes();
        if modes.is_empty() {
            return;
        }
        let next = self.mode.map_or(modes[0], |current| {
            let idx = modes.iter().position(|mode| *mode == current).unwrap_or(0);
            modes[(idx + 1) % modes.len()]
        });
        self.set_mode(next);
    }

    pub fn next_focus_after_mode(&self) -> AuthFormFocus {
        if self.shows_source_folder() {
            AuthFormFocus::SourceFolder
        } else if self.shows_credential_block() {
            AuthFormFocus::CredentialSource
        } else {
            AuthFormFocus::Save
        }
    }

    /// Modes the user can pick.
    pub const fn available_modes(&self) -> &'static [AuthMode] {
        self.kind.supported_modes()
    }

    /// Save invariant: mode is committed and, if needed, a non-empty credential.
    pub fn can_save(&self) -> bool {
        let Some(mode) = self.mode else { return false };
        if !mode_requires_credential(self.kind, mode) {
            return true;
        }
        match &self.credential {
            CredentialInput::None => false,
            CredentialInput::Literal(value) => !value.is_empty(),
            CredentialInput::OpRef(value) => !value.is_empty(),
        }
    }

    /// Build the outcome for the parent to persist. Returns None if `!can_save`.
    pub fn commit(&self) -> Option<AuthFormOutcome<V>> {
        if !self.can_save() {
            return None;
        }
        let mode = self.mode?;
        let env_var_name = self.kind.required_env_var(mode);
        let env_value = match &self.credential {
            CredentialInput::None => None,
            CredentialInput::Literal(value) => Some(V::from_plain(value.clone())),
            CredentialInput::OpRef(value) => Some(V::from_op_ref(value.clone())),
        };
        Some(AuthFormOutcome {
            mode,
            env_var_name,
            env_value,
            source_folder: self.source_folder.clone(),
        })
    }
}

const fn mode_requires_credential(kind: AuthKind, mode: AuthMode) -> bool {
    auth_mode_requires_credential(kind, mode)
}

/// Operator-facing slug for an [`AuthMode`].
pub const fn mode_str(mode: AuthMode) -> &'static str {
    mode.as_str()
}

const AUTH_FORM_MODE_LABEL_WIDTH: usize = 23;
const AUTH_FORM_CREDENTIAL_LABEL_WIDTH: usize = 23;

/// Render the auth-edit modal for `form` into `area`.
pub fn render_form<V: AuthCredential>(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &AuthForm<V>,
    focus: AuthFormFocus,
) {
    let inner = jackin_tui::components::render_dialog_shell(
        frame,
        area,
        Some("Edit auth"),
        jackin_tui::components::DialogBorder::Default,
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

struct FormLine {
    line: Line<'static>,
    centered: bool,
}

impl FormLine {
    const fn left(line: Line<'static>) -> Self {
        Self {
            line,
            centered: false,
        }
    }

    const fn centered(line: Line<'static>) -> Self {
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

fn build_form_lines<V: AuthCredential>(form: &AuthForm<V>, focus: AuthFormFocus) -> Vec<FormLine> {
    let mut lines: Vec<FormLine> = Vec::new();

    lines.push(FormLine::left(Line::from("")));

    let mode_text = form.mode.map_or("(unset)", mode_str);
    lines.push(FormLine::left(Line::from(vec![
        cursor_span(focus == AuthFormFocus::Mode),
        Span::styled(
            format!("{:<AUTH_FORM_MODE_LABEL_WIDTH$}", "Mode"),
            label_style(),
        ),
        Span::raw(" "),
        Span::styled(mode_text.to_owned(), termrock::style::GREEN),
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
    )));
    lines.push(FormLine::left(Line::from("")));
    lines
}

fn source_folder_line<V: AuthCredential>(form: &AuthForm<V>, selected: bool) -> Line<'static> {
    let label_style = if selected {
        termrock::style::BOLD_WHITE
    } else {
        Style::default().fg(WHITE)
    };
    Line::from(vec![
        cursor_span(selected),
        Span::styled(
            format!("{:<AUTH_FORM_CREDENTIAL_LABEL_WIDTH$}", "Source folder"),
            label_style,
        ),
        Span::raw(" "),
        Span::styled(source_folder_text(form), termrock::style::GREEN),
    ])
}

fn source_folder_text<V: AuthCredential>(form: &AuthForm<V>) -> String {
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

fn credential_env_line<R: AuthCredentialRef>(
    env_var: &str,
    credential: &CredentialInput<R>,
    selected: bool,
) -> Line<'static> {
    let label_style = if selected {
        termrock::style::BOLD_WHITE
    } else {
        Style::default().fg(WHITE)
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
            spans.push(Span::styled("required".to_owned(), termrock::style::DANGER));
        }
        CredentialInput::Literal(value) => {
            let masked = if value.is_empty() {
                "required".to_owned()
            } else {
                "●".repeat(value.chars().count().clamp(1, 12))
            };
            let style = if value.is_empty() {
                termrock::style::DANGER
            } else {
                termrock::style::GREEN
            };
            spans.push(Span::styled(masked, style));
        }
        CredentialInput::OpRef(value) => {
            push_op_breadcrumb_spans(&mut spans, value.path());
        }
    }
    Line::from(spans)
}

fn action_buttons_line(can_save: bool, focus: AuthFormFocus) -> Line<'static> {
    let save_style = if can_save {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(PHOSPHOR_DIM)
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
            selected_button_style(focus == AuthFormFocus::Cancel, termrock::style::BOLD_WHITE),
        ),
        Span::raw("    "),
        Span::styled(
            "  Reset  ".to_owned(),
            selected_button_style(focus == AuthFormFocus::Reset, termrock::style::BOLD_WHITE),
        ),
    ])
}

fn label_style() -> Style {
    termrock::style::BOLD_WHITE
}

const fn selected_button_style(selected: bool, style: Style) -> Style {
    if selected {
        style.bg(WHITE).fg(Color::Black)
    } else {
        style
    }
}

/// `AuthCredentialRef` impl for `jackin_core::OpRef`.
///
/// Lives here (where the trait is defined) rather than in the binary crate
/// to satisfy the orphan rule — both the trait and the type are external to
/// the binary but this crate defines the trait.
impl AuthCredentialRef for jackin_core::OpRef {
    fn path(&self) -> &str {
        &self.path
    }

    fn is_empty(&self) -> bool {
        self.op.is_empty() || self.path.is_empty()
    }
}

/// `AuthCredential` impl for `jackin_core::EnvValue`.
impl AuthCredential for jackin_core::EnvValue {
    type Ref = jackin_core::OpRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref> {
        match self {
            Self::Plain(value) => CredentialInput::Literal(value),
            Self::Extended(e) => CredentialInput::Literal(e.value),
            Self::OpRef(value) => CredentialInput::OpRef(value),
        }
    }

    fn from_plain(value: String) -> Self {
        Self::Plain(value)
    }

    fn from_op_ref(value: Self::Ref) -> Self {
        Self::OpRef(value)
    }
}

#[cfg(test)]
mod tests;
