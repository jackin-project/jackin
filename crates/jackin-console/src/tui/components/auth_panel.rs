//! Auth edit-form state and rendering.

use std::marker::PhantomData;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::auth::{AuthKind, AuthMode, auth_mode_requires_credential};
use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;
use crate::tui::components::source_picker::SourcePickerState;
use crate::tui::screens::settings::model::AuthFormFocus;
use jackin_tui::components::{Panel, PanelFocus, TextInputState};
use jackin_tui::theme::{DANGER_RED, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

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
pub fn auth_credential_input_state<'a>(
    literal: impl Into<String>,
) -> TextInputState<'a> {
    TextInputState::new("Credential", literal)
}

#[must_use]
pub const fn auth_form_key_plan(
    focus: AuthFormFocus,
    key: KeyCode,
    shows_credential_block: bool,
    can_save: bool,
) -> AuthFormKeyPlan {
    match focus {
        AuthFormFocus::Mode => match key {
            KeyCode::Char(' ') => AuthFormKeyPlan::CycleMode,
            KeyCode::Down | KeyCode::Char('j') if shows_credential_block => {
                AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
            }
            KeyCode::Tab => {
                if shows_credential_block {
                    AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
                } else {
                    AuthFormKeyPlan::Focus(AuthFormFocus::Save)
                }
            }
            KeyCode::BackTab => AuthFormKeyPlan::Focus(AuthFormFocus::Reset),
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::CredentialSource => match key {
            KeyCode::Enter => AuthFormKeyPlan::OpenCredentialSource,
            KeyCode::Tab => AuthFormKeyPlan::Focus(AuthFormFocus::Save),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                AuthFormKeyPlan::Focus(AuthFormFocus::Mode)
            }
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::Save => match key {
            KeyCode::Right | KeyCode::Tab => AuthFormKeyPlan::Focus(AuthFormFocus::Cancel),
            KeyCode::BackTab => {
                if shows_credential_block {
                    AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
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
    _value: PhantomData<fn() -> V>,
}

/// Output of a successful commit. The parent uses these fields to write the
/// kind block and the env-var entry at the chosen layer.
#[derive(Debug, Clone)]
pub struct AuthFormOutcome<V> {
    pub mode: AuthMode,
    pub env_var_name: Option<&'static str>,
    pub env_value: Option<V>,
}

impl<V: AuthCredential> AuthForm<V> {
    pub const fn new(kind: AuthKind) -> Self {
        Self {
            kind,
            mode: None,
            credential: CredentialInput::None,
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
            _value: PhantomData,
        }
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

    pub fn set_op_ref(&mut self, value: V::Ref) {
        self.credential = CredentialInput::OpRef(value);
    }

    pub fn clear_credential(&mut self) {
        self.credential = CredentialInput::None;
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

    pub const fn next_focus_after_mode(&self) -> AuthFormFocus {
        if self.shows_credential_block() {
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
    frame: &mut Frame,
    area: Rect,
    form: &AuthForm<V>,
    focus: AuthFormFocus,
) {
    frame.render_widget(ratatui::widgets::Clear, area);
    let block = Panel::new()
        .title(" Edit auth ")
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

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
    let inner: u16 = if form.shows_credential_block() { 6 } else { 5 };
    inner + 2
}

fn build_form_lines<V: AuthCredential>(
    form: &AuthForm<V>,
    focus: AuthFormFocus,
) -> Vec<FormLine> {
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
        Span::styled(mode_text.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
    ])));

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

fn credential_env_line<R: AuthCredentialRef>(
    env_var: &str,
    credential: &CredentialInput<R>,
    selected: bool,
) -> Line<'static> {
    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
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
            spans.push(Span::styled(
                "required".to_string(),
                Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
            ));
        }
        CredentialInput::Literal(value) => {
            let masked = if value.is_empty() {
                "required".to_string()
            } else {
                "●".repeat(value.chars().count().clamp(1, 12))
            };
            let style = if value.is_empty() {
                Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(PHOSPHOR_GREEN)
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
            "  Save  ".to_string(),
            selected_button_style(focus == AuthFormFocus::Save, save_style),
        ),
        Span::raw("    "),
        Span::styled(
            "  Cancel  ".to_string(),
            selected_button_style(
                focus == AuthFormFocus::Cancel,
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
        ),
        Span::raw("    "),
        Span::styled(
            "  Reset  ".to_string(),
            selected_button_style(
                focus == AuthFormFocus::Reset,
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
        ),
    ])
}

fn cursor_span(selected: bool) -> Span<'static> {
    if selected {
        Span::styled(
            "▸ ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  ")
    }
}

fn label_style() -> Style {
    Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
}

const fn selected_button_style(selected: bool, style: Style) -> Style {
    if selected {
        style.bg(WHITE).fg(Color::Black)
    } else {
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestOpRef {
        path: String,
    }

    impl AuthCredentialRef for TestOpRef {
        fn path(&self) -> &str {
            &self.path
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestCredential {
        Plain(String),
        OpRef(TestOpRef),
    }

    impl AuthCredential for TestCredential {
        type Ref = TestOpRef;

        fn into_credential_input(self) -> CredentialInput<Self::Ref> {
            match self {
                Self::Plain(value) => CredentialInput::Literal(value),
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

    type TestForm = AuthForm<TestCredential>;

    fn dump_form(form: &TestForm) -> String {
        let backend = TestBackend::new(100, 20);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|frame| {
            let area = frame.area();
            render_form(frame, area, form, AuthFormFocus::Mode);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut output = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                output.push_str(buf[(x, y)].symbol());
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn auth_credential_input_state_names_credential() {
        let state = auth_credential_input_state("secret");

        assert_eq!(state.label, "Credential");
        assert_eq!(state.value(), "secret");
    }

    #[test]
    fn auth_source_picker_state_keeps_env_label() {
        let state = auth_source_picker_state("CLAUDE_API_KEY", true);

        assert_eq!(state.key, "CLAUDE_API_KEY");
    }

    #[test]
    fn save_disabled_when_mode_unset() {
        let form = TestForm::new(AuthKind::Claude);
        assert!(!form.can_save());
    }

    #[test]
    fn save_enabled_for_sync() {
        let mut form = TestForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::Sync);
        assert!(form.can_save());
    }

    #[test]
    fn save_disabled_for_api_key_without_credential() {
        let mut form = TestForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        assert!(!form.can_save());
    }

    #[test]
    fn save_enabled_for_api_key_with_literal() {
        let mut form = TestForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        form.set_literal("sk-ant-test".into());
        assert!(form.can_save());
    }

    #[test]
    fn save_disabled_for_api_key_with_empty_op_ref() {
        let mut form = TestForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        form.set_op_ref(TestOpRef {
            path: String::new(),
        });
        assert!(!form.can_save());
    }

    #[test]
    fn commit_emits_required_env_var() {
        let mut form = TestForm::new(AuthKind::Github);
        form.set_mode(AuthMode::Token);
        form.set_literal("ghp_xxxx".into());
        let outcome = form.commit().expect("can save");
        assert_eq!(outcome.mode, AuthMode::Token);
        assert_eq!(outcome.env_var_name, Some("GH_TOKEN"));
        assert!(matches!(
            outcome.env_value,
            Some(TestCredential::Plain(ref value)) if value == "ghp_xxxx"
        ));
    }

    #[test]
    fn cycle_mode_wraps_supported_modes_and_updates_focus_target() {
        let mut form = TestForm::new(AuthKind::Github);

        assert_eq!(form.next_focus_after_mode(), AuthFormFocus::Save);
        form.cycle_mode();
        assert_eq!(form.mode, Some(AuthMode::Sync));
        assert_eq!(form.next_focus_after_mode(), AuthFormFocus::Save);
        form.cycle_mode();
        assert_eq!(form.mode, Some(AuthMode::Token));
        assert_eq!(
            form.next_focus_after_mode(),
            AuthFormFocus::CredentialSource
        );
        form.cycle_mode();
        assert_eq!(form.mode, Some(AuthMode::Ignore));
        form.cycle_mode();
        assert_eq!(form.mode, Some(AuthMode::Sync));
    }

    #[test]
    fn auth_form_key_plan_routes_shared_focus_model() {
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::Mode, KeyCode::Char(' '), false, false),
            AuthFormKeyPlan::CycleMode
        );
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::Mode, KeyCode::Tab, true, false),
            AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
        );
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::CredentialSource, KeyCode::Enter, true, false),
            AuthFormKeyPlan::OpenCredentialSource
        );
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::Save, KeyCode::BackTab, true, false),
            AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
        );
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::Save, KeyCode::Enter, true, false),
            AuthFormKeyPlan::Stay
        );
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::Save, KeyCode::Enter, true, true),
            AuthFormKeyPlan::Save
        );
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::Cancel, KeyCode::Enter, false, false),
            AuthFormKeyPlan::Cancel
        );
        assert_eq!(
            auth_form_key_plan(AuthFormFocus::Reset, KeyCode::Enter, false, false),
            AuthFormKeyPlan::Reset
        );
    }

    #[test]
    fn form_with_unset_mode_hides_credential_block() {
        let form = TestForm::new(AuthKind::Claude);
        let output = dump_form(&form);
        assert!(output.contains("Edit auth"));
        assert!(output.contains("Mode"));
        assert!(output.contains("(unset)"));
        assert!(!output.contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn form_with_api_key_literal_masks_value() {
        let mut form = TestForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        form.set_literal("sk-ant-test".into());
        let output = dump_form(&form);
        assert!(output.contains("api_key"));
        assert!(output.contains("ANTHROPIC_API_KEY"));
        assert!(output.contains("●●●●●●●●●●●"));
    }

    #[test]
    fn form_with_op_ref_credential_shows_path() {
        let mut form = TestForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        form.set_op_ref(TestOpRef {
            path: "Work/Anthropic/api-key".into(),
        });
        let output = dump_form(&form);
        assert!(output.contains("Work / Anthropic → api-key"));
    }
}
