//! Render helpers for the auth edit form.
//!
//! `render_form` renders the auth-edit modal. The flat-row Auth tab
//! rendering lives in `src/console/manager/render/editor.rs`.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::form::{AuthForm, CredentialInput};
use crate::console::manager::auth_kind::AuthMode;
use crate::console::manager::render::editor::push_op_breadcrumb_spans;
use crate::console::manager::state::AuthFormFocus;

pub(crate) const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
pub(crate) const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
pub(crate) const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
pub(crate) const WHITE: Color = Color::Rgb(255, 255, 255);
pub(crate) const DANGER_RED: Color = Color::Rgb(255, 94, 122);
// Width chosen so the longest credential env-var name
// (`CLAUDE_CODE_OAUTH_TOKEN`, 23 chars) fits without overflow and the
// Mode value column lines up with the credential value column when
// rendered with a single-space separator.
const AUTH_FORM_MODE_LABEL_WIDTH: usize = 23;
const AUTH_FORM_CREDENTIAL_LABEL_WIDTH: usize = 23;

/// Operator-facing slug for an [`AuthMode`]. Wraps
/// [`AuthMode::as_str`] so the panel keeps a single re-export point
/// for the auth-tab renderers and the form modal.
pub(crate) const fn mode_str(m: AuthMode) -> &'static str {
    m.as_str()
}

/// Render the auth-edit modal for `form` into `area`.
///
/// Lays out, top-to-bottom:
///   - title block: `Edit auth`
///   - mode picker row
///   - credential block (only when [`AuthForm::shows_credential_block`])
///     - one required env-var row that opens the shared source picker
///   - action buttons and a compact key hint row
///
/// Pure render — no input handling. Keystrokes are routed by
/// `super::super::manager::input::auth::handle_auth_form_key`.
pub fn render_form(frame: &mut Frame, area: Rect, form: &AuthForm, focus: AuthFormFocus) {
    frame.render_widget(ratatui::widgets::Clear, area);
    let title_span = Span::styled(
        " Edit auth ",
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title_span);
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
///
/// Inner content + 2 borders. Used by `render::modal` to size the
/// modal so its vertical layout hugs the content rather than leaving
/// dead space below the hint line.
#[must_use]
pub const fn required_height(form: &AuthForm) -> u16 {
    // Layout (without credential block):
    //   blank, Mode, blank, buttons, blank, hint = 6 inner rows
    // With credential block, +1 (cred row) = 7 inner rows.
    // With the OAuth-token setup tip, +1 more.
    let mut inner: u16 = if form.shows_credential_block() { 7 } else { 6 };
    if shows_oauth_token_setup_tip(form) {
        inner += 1;
    }
    inner + 2
}

/// Whether the OAuth-token setup tip line is rendered for this
/// form. Visible only when the operator has picked
/// `oauth_token` mode but has not yet supplied a credential — that's
/// the moment a pointer at `jackin workspace claude-token setup` is
/// most actionable. Once a credential is set, the tip would be
/// noise.
const fn shows_oauth_token_setup_tip(form: &AuthForm) -> bool {
    matches!(
        (form.kind, form.mode, &form.credential),
        (
            crate::console::manager::auth_kind::AuthKind::Claude,
            Some(crate::console::manager::auth_kind::AuthMode::OAuthToken),
            crate::console::widgets::auth_panel::form::CredentialInput::None
        )
    )
}

fn build_form_lines(form: &AuthForm, focus: AuthFormFocus) -> Vec<FormLine> {
    let mut lines: Vec<FormLine> = Vec::new();

    lines.push(FormLine::left(Line::from("")));

    // Mode picker line.
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
    if shows_oauth_token_setup_tip(form) {
        lines.push(FormLine::centered(oauth_token_setup_tip_line()));
    }
    lines.push(FormLine::left(Line::from("")));
    lines.push(FormLine::centered(form_hint_line(form, focus)));
    lines
}

/// Render the inline "wire this slot from the shell" tip shown when
/// the operator has picked `oauth_token` mode but has not yet
/// supplied a credential. The CLI command is rendered verbatim so
/// the operator can copy it directly.
fn oauth_token_setup_tip_line() -> Line<'static> {
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let cmd = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("tip: run ", dim),
        Span::styled(
            "jackin workspace claude-token setup <workspace> --vault <vault>",
            cmd,
        ),
        Span::styled(" from your shell", dim),
    ])
}

fn credential_env_line(env_var: &str, cred: &CredentialInput, selected: bool) -> Line<'static> {
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
    match cred {
        CredentialInput::None => {
            spans.push(Span::styled(
                "required".to_string(),
                Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
            ));
        }
        CredentialInput::Literal(s) => {
            let masked = if s.is_empty() {
                "required".to_string()
            } else {
                "●".repeat(s.chars().count().clamp(1, 12))
            };
            let style = if s.is_empty() {
                Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(PHOSPHOR_GREEN)
            };
            spans.push(Span::styled(masked, style));
        }
        CredentialInput::OpRef(r) => {
            push_op_breadcrumb_spans(&mut spans, &r.path);
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

fn form_hint_line(form: &AuthForm, focus: AuthFormFocus) -> Line<'static> {
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let mut spans = match focus {
        AuthFormFocus::Mode => vec![
            Span::styled("Space", key_style),
            Span::styled(" cycle mode", text_style),
        ],
        AuthFormFocus::CredentialSource => vec![
            Span::styled("Enter", key_style),
            Span::styled(" set credential", text_style),
        ],
        AuthFormFocus::Save | AuthFormFocus::Cancel | AuthFormFocus::Reset => vec![
            Span::styled("Enter", key_style),
            Span::styled(" select", text_style),
        ],
    };
    if form.shows_credential_block() {
        spans.push(Span::styled(" · ", sep_style));
        spans.push(Span::styled("↑/↓", key_style));
        spans.push(Span::styled(" navigate", text_style));
    }
    spans.push(Span::styled(" · ", sep_style));
    spans.push(Span::styled("Esc", key_style));
    spans.push(Span::styled(" cancel", text_style));
    Line::from(spans)
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
mod form_render_tests {
    use super::*;
    use crate::console::manager::auth_kind::{AuthKind, AuthMode};
    use crate::operator_env::OpRef;
    use ratatui::{Terminal, backend::TestBackend};

    fn dump_form(form: &AuthForm) -> String {
        let backend = TestBackend::new(100, 20);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = f.area();
            render_form(f, area, form, AuthFormFocus::Mode);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn form_header_is_short() {
        let form = AuthForm::new(AuthKind::Claude);
        let s = dump_form(&form);
        assert!(s.contains("Edit auth"), "missing header; dump:\n{s}");
        assert!(
            !s.contains("workspace"),
            "header should not include verbose scope text; dump:\n{s}"
        );
    }

    #[test]
    fn form_with_unset_mode_hides_credential_block_and_dims_save() {
        let form = AuthForm::new(AuthKind::Claude);
        let s = dump_form(&form);
        assert!(s.contains("Mode"), "missing mode line; dump:\n{s}");
        assert!(
            !s.contains("Mode:"),
            "mode label should not use punctuation; dump:\n{s}"
        );
        assert!(
            s.contains("(unset)"),
            "expected (unset) mode label; dump:\n{s}"
        );
        // No credential row when mode is unset.
        assert!(
            !s.contains("ANTHROPIC_API_KEY"),
            "credential row must be hidden when mode unset; dump:\n{s}"
        );
        // Save still appears as a button label even when disabled.
        assert!(s.contains("Save"), "missing Save button; dump:\n{s}");
        assert!(s.contains("Cancel"), "missing Cancel button; dump:\n{s}");
        assert!(s.contains("Reset"), "missing Reset button; dump:\n{s}");
    }

    #[test]
    fn form_with_sync_mode_hides_credential_block_and_enables_save() {
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::Sync);
        let s = dump_form(&form);
        assert!(s.contains("sync"), "missing sync mode label; dump:\n{s}");
        // Sync requires no credential.
        assert!(
            !s.contains("ANTHROPIC_API_KEY"),
            "sync should hide credential row; dump:\n{s}"
        );
        assert!(form.can_save());
    }

    #[test]
    fn form_with_api_key_literal_shows_credential_block_and_resolves() {
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        form.set_literal("sk-ant-test".into());
        let s = dump_form(&form);
        assert!(s.contains("api_key"), "missing api_key mode; dump:\n{s}");
        assert!(
            s.contains("ANTHROPIC_API_KEY"),
            "missing env var row; dump:\n{s}"
        );
        assert!(
            s.contains("●●●●●●●●●●●"),
            "literal credential should be masked; dump:\n{s}"
        );
        assert!(
            !s.contains("plain text"),
            "plain text source label should be omitted; dump:\n{s}"
        );
    }

    #[test]
    fn form_with_op_ref_credential_shows_path_and_picker_button() {
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        form.set_op_ref(OpRef {
            op: "op://uuid/anthropic".into(),
            path: "Work/Anthropic/api-key".into(),
        });
        let s = dump_form(&form);
        assert!(
            !s.contains("1Password"),
            "1Password source label should be omitted; dump:\n{s}"
        );
        assert!(
            s.contains("Work / Anthropic → api-key"),
            "missing op-ref breadcrumb display; dump:\n{s}"
        );
    }

    #[test]
    fn long_credential_env_name_has_gap_before_source_label() {
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::OAuthToken);
        form.set_op_ref(OpRef {
            op: "op://uuid/oauth".into(),
            path: "Boris/Roblox/token".into(),
        });
        let s = dump_form(&form);
        assert!(
            s.contains("CLAUDE_CODE_OAUTH_TOKEN Boris / Roblox → token"),
            "env var and breadcrumb should have a visible gap; dump:\n{s}"
        );
    }

    /// The OAuth-token setup tip line is the operator's "what do I
    /// type next?" hint when they pick `oauth_token` mode but have
    /// no credential set yet. It is Claude-only — Codex has no
    /// `oauth_token` mode at the deserializer (parse-rejected) and
    /// the TUI does not offer it for Codex either; the predicate
    /// matrix below pins those invariants.
    #[test]
    fn oauth_token_setup_tip_shows_for_claude_no_credential_only() {
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::OAuthToken);
        assert!(
            shows_oauth_token_setup_tip(&form),
            "Claude + OAuthToken + no credential ⇒ show tip"
        );
        let s = dump_form(&form);
        assert!(
            s.contains("jackin workspace claude-token setup"),
            "tip with the canonical CLI command must render; dump:\n{s}"
        );
    }

    #[test]
    fn oauth_token_setup_tip_does_not_show_for_codex() {
        // Codex's `AuthForm::set_mode(OAuthToken)` panics by design
        // (the form refuses unsupported kind/mode combinations).
        // The Claude-specific tip must therefore stay hidden across
        // every Codex-supported mode — pin every supported variant
        // here so a future tip-predicate refactor can't accidentally
        // light up for Codex.
        let mut form = AuthForm::new(AuthKind::Codex);
        for mode in AuthKind::Codex.supported_modes() {
            form.set_mode(*mode);
            assert!(
                !shows_oauth_token_setup_tip(&form),
                "Codex must never surface the Claude tip (mode {mode:?})"
            );
            let s = dump_form(&form);
            assert!(
                !s.contains("jackin workspace claude-token setup"),
                "Codex modal must not render the Claude tip; mode {mode:?}; dump:\n{s}"
            );
        }
    }

    #[test]
    fn oauth_token_setup_tip_does_not_show_when_credential_set() {
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::OAuthToken);
        form.set_literal("sk-ant-oat01-already-set".into());
        assert!(
            !shows_oauth_token_setup_tip(&form),
            "tip must hide once a credential is supplied"
        );
    }

    #[test]
    fn oauth_token_setup_tip_does_not_show_for_other_modes() {
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::ApiKey);
        assert!(!shows_oauth_token_setup_tip(&form));
        form.set_mode(AuthMode::Sync);
        assert!(!shows_oauth_token_setup_tip(&form));
        form.set_mode(AuthMode::Ignore);
        assert!(!shows_oauth_token_setup_tip(&form));
    }
}
