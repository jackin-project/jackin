//! Render helpers for the auth edit form.
//!
//! `render_form` renders the auth-edit modal for a single (workspace, role,
//! agent) combination. The flat-row Auth tab rendering lives in
//! `src/console/manager/render/editor.rs`.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::form::{AuthForm, CredentialInput};
use crate::agent::Agent;
use crate::config::AuthForwardMode;

pub(crate) const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
pub(crate) const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
pub(crate) const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
pub(crate) const WHITE: Color = Color::Rgb(255, 255, 255);
pub(crate) const DANGER_RED: Color = Color::Rgb(255, 94, 122);

pub(crate) const fn agent_display(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude",
        Agent::Codex => "Codex",
    }
}

pub(crate) const fn mode_str(m: AuthForwardMode) -> &'static str {
    match m {
        AuthForwardMode::Sync => "sync",
        AuthForwardMode::ApiKey => "api_key",
        AuthForwardMode::OAuthToken => "oauth_token",
        AuthForwardMode::Ignore => "ignore",
    }
}

/// Identification context passed alongside the form's mutable state so the
/// render can title the modal with the workspace and role being edited.
pub struct FormContext<'a> {
    pub workspace: &'a str,
    pub role: &'a str,
}

/// Render the auth-edit modal for `form` into `area`.
///
/// Lays out, top-to-bottom:
///   - title block: `Edit auth: workspace 'X' / role 'Y' / Agent`
///   - mode picker line (current mode + cycle hint)
///   - credential block (only when [`AuthForm::shows_credential_block`])
///     - `Literal | 1Password` radio
///     - text input or op-ref path display
///     - resolves badge if a literal/op-ref is committed
///   - action buttons: `Save` (DIM if `!form.can_save`) / `Cancel` / `Reset`
///
/// Pure render — no input handling. Task 19 wires the keystroke router.
pub fn render_form(frame: &mut Frame, area: Rect, form: &AuthForm, ctx: &FormContext) {
    frame.render_widget(ratatui::widgets::Clear, area);
    let title = format!(
        " Edit auth: workspace '{ws}' / role '{role}' / {agent} ",
        ws = ctx.workspace,
        role = ctx.role,
        agent = agent_display(form.agent),
    );
    let title_span = Span::styled(
        title,
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title_span);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = build_form_lines(form);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn build_form_lines(form: &AuthForm) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Mode picker line.
    let mode_text = form.mode.map_or("(unset)", mode_str);
    lines.push(Line::from(vec![
        Span::styled(
            "Mode: ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(mode_text.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled(
            "  (Tab/Space to cycle)".to_string(),
            Style::default().fg(PHOSPHOR_DIM),
        ),
    ]));

    if form.shows_credential_block() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Credential",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        )));
        lines.push(credential_radio_line(&form.credential));
        lines.push(credential_value_line(&form.credential));
        lines.push(credential_status_line(&form.credential));
    }

    lines.push(Line::from(""));
    lines.push(action_buttons_line(form.can_save()));
    lines
}

fn credential_radio_line(cred: &CredentialInput) -> Line<'static> {
    // Default selection when no credential committed yet is `Literal`.
    let is_op_ref = matches!(cred, CredentialInput::OpRef(_));
    let is_literal = !is_op_ref;

    let literal_label = if is_literal {
        "(*) Literal"
    } else {
        "( ) Literal"
    };
    let op_label = if is_op_ref {
        "(*) 1Password"
    } else {
        "( ) 1Password"
    };

    let lit_style = if is_literal {
        Style::default().fg(PHOSPHOR_GREEN)
    } else {
        Style::default().fg(PHOSPHOR_DIM)
    };
    let op_style = if is_op_ref {
        Style::default().fg(PHOSPHOR_GREEN)
    } else {
        Style::default().fg(PHOSPHOR_DIM)
    };

    Line::from(vec![
        Span::styled(literal_label.to_string(), lit_style),
        Span::raw("    "),
        Span::styled(op_label.to_string(), op_style),
    ])
}

fn credential_value_line(cred: &CredentialInput) -> Line<'static> {
    match cred {
        CredentialInput::None => Line::from(Span::styled(
            "  (enter a literal value or pick from 1Password)".to_string(),
            Style::default().fg(PHOSPHOR_DIM),
        )),
        CredentialInput::Literal(s) => {
            // Mask the value for display — still need to show *something*
            // so the operator knows there is content.
            let masked = if s.is_empty() {
                String::from("(empty)")
            } else {
                "*".repeat(s.chars().count().min(20))
            };
            Line::from(vec![
                Span::styled(
                    "  Value: ",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(masked, Style::default().fg(WHITE)),
            ])
        }
        CredentialInput::OpRef(r) => Line::from(vec![
            Span::styled(
                "  Path:  ",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(r.path.clone(), Style::default().fg(WHITE)),
            Span::raw("  "),
            Span::styled(
                "[Pick from 1Password]".to_string(),
                Style::default().fg(PHOSPHOR_GREEN),
            ),
        ]),
    }
}

fn credential_status_line(cred: &CredentialInput) -> Line<'static> {
    match cred {
        CredentialInput::None => Line::from(Span::styled(
            "  Status: ! unset".to_string(),
            Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
        )),
        CredentialInput::Literal(s) if s.is_empty() => Line::from(Span::styled(
            "  Status: ! empty".to_string(),
            Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
        )),
        CredentialInput::Literal(_) | CredentialInput::OpRef(_) => Line::from(Span::styled(
            "  Status: OK resolves".to_string(),
            Style::default().fg(PHOSPHOR_GREEN),
        )),
    }
}

fn action_buttons_line(can_save: bool) -> Line<'static> {
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
        Span::styled("[ Save ]".to_string(), save_style),
        Span::raw("  "),
        Span::styled(
            "[ Cancel ]".to_string(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "[ Reset ]".to_string(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ])
}

#[cfg(test)]
mod form_render_tests {
    use super::*;
    use crate::operator_env::OpRef;
    use ratatui::{Terminal, backend::TestBackend};

    fn dump_form(form: &AuthForm, ctx: &FormContext) -> String {
        let backend = TestBackend::new(100, 20);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = f.area();
            render_form(f, area, form, ctx);
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

    fn ctx() -> FormContext<'static> {
        FormContext {
            workspace: "proj",
            role: "smith",
        }
    }

    #[test]
    fn form_header_includes_workspace_role_and_agent() {
        let form = AuthForm::new(Agent::Claude);
        let s = dump_form(&form, &ctx());
        assert!(s.contains("Edit auth:"), "missing header; dump:\n{s}");
        assert!(s.contains("proj"), "missing workspace name; dump:\n{s}");
        assert!(s.contains("smith"), "missing role name; dump:\n{s}");
        assert!(s.contains("Claude"), "missing agent name; dump:\n{s}");
    }

    #[test]
    fn form_with_unset_mode_hides_credential_block_and_dims_save() {
        let form = AuthForm::new(Agent::Claude);
        let s = dump_form(&form, &ctx());
        assert!(s.contains("Mode:"), "missing mode line; dump:\n{s}");
        assert!(
            s.contains("(unset)"),
            "expected (unset) mode label; dump:\n{s}"
        );
        // No credential block when mode is unset.
        assert!(
            !s.contains("Credential"),
            "credential block must be hidden when mode unset; dump:\n{s}"
        );
        // Save still appears as a button label even when disabled.
        assert!(s.contains("Save"), "missing Save button; dump:\n{s}");
        assert!(s.contains("Cancel"), "missing Cancel button; dump:\n{s}");
        assert!(s.contains("Reset"), "missing Reset button; dump:\n{s}");
    }

    #[test]
    fn form_with_sync_mode_hides_credential_block_and_enables_save() {
        let mut form = AuthForm::new(Agent::Claude);
        form.set_mode(AuthForwardMode::Sync);
        let s = dump_form(&form, &ctx());
        assert!(s.contains("sync"), "missing sync mode label; dump:\n{s}");
        // Sync requires no credential.
        assert!(
            !s.contains("Credential"),
            "sync should hide credential block; dump:\n{s}"
        );
        assert!(form.can_save());
    }

    #[test]
    fn form_with_api_key_literal_shows_credential_block_and_resolves() {
        let mut form = AuthForm::new(Agent::Claude);
        form.set_mode(AuthForwardMode::ApiKey);
        form.set_literal("sk-ant-test".into());
        let s = dump_form(&form, &ctx());
        assert!(s.contains("api_key"), "missing api_key mode; dump:\n{s}");
        assert!(
            s.contains("Credential"),
            "credential block must show for api_key; dump:\n{s}"
        );
        assert!(s.contains("Literal"), "missing Literal radio; dump:\n{s}");
        assert!(
            s.contains("1Password"),
            "missing 1Password radio; dump:\n{s}"
        );
        // Active radio marker on the Literal side.
        assert!(
            s.contains("(*) Literal"),
            "Literal radio not selected; dump:\n{s}"
        );
        assert!(
            s.contains("resolves"),
            "missing resolves status badge; dump:\n{s}"
        );
    }

    #[test]
    fn form_with_op_ref_credential_shows_path_and_picker_button() {
        let mut form = AuthForm::new(Agent::Claude);
        form.set_mode(AuthForwardMode::ApiKey);
        form.set_op_ref(OpRef {
            op: "op://uuid/anthropic".into(),
            path: "Work/Anthropic/api-key".into(),
        });
        let s = dump_form(&form, &ctx());
        assert!(
            s.contains("(*) 1Password"),
            "1Password radio not selected; dump:\n{s}"
        );
        assert!(
            s.contains("Work/Anthropic/api-key"),
            "missing op-ref path display; dump:\n{s}"
        );
        assert!(
            s.contains("Pick from 1Password"),
            "missing op-picker button; dump:\n{s}"
        );
        assert!(
            s.contains("resolves"),
            "missing resolves status badge for op-ref; dump:\n{s}"
        );
    }
}
