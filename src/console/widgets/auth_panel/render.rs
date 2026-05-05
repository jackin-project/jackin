//! Render path for [`super::AuthPanelState`].
//!
//! Three sections, top-to-bottom:
//!
//!   1. **Global defaults** — read-only, hint links to global-config screen.
//!   2. **This workspace** — `[edit]` / `[reset]` affordances per row.
//!   3. **Per role × agent** — `[edit]` affordance per row.
//!
//! Each row renders as: `<label>: <mode> (<provenance>)  <badge>` where
//! `<badge>` is one of `OK resolves`, `! unset`, or `-` (not applicable).
//! Selection highlight follows the canonical phosphor list-modal style
//! used by [`crate::console::widgets::op_picker::render`].
//!
//! This is a pure render — no input handling, no form mounting (those
//! land in Task 17/18/19).
//!
//! The selection cursor is supplied by the parent panel out-of-band via
//! the `selected` argument; row indexing is the flattened concatenation
//! of `workspace_rows` + `role_agent_rows` (the global section is
//! read-only and not part of the selectable surface).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::form::{AuthForm, CredentialInput};
use super::state::{AuthPanelState, AuthRow, CredentialBadge, ProvenanceTag};
use crate::agent::Agent;
use crate::config::AuthForwardMode;

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);
const DANGER_RED: Color = Color::Rgb(255, 94, 122);

/// Render `state` into `area` with no row selected (read-only display).
pub fn render(frame: &mut Frame, area: Rect, state: &AuthPanelState) {
    render_with_selection(frame, area, state, None);
}

/// Render `state` into `area` with `selected` highlighted.
///
/// `selected` indexes the flattened list of editable rows
/// (`workspace_rows` followed by `role_agent_rows`). The global section
/// is read-only and is never highlighted.
pub fn render_with_selection(
    frame: &mut Frame,
    area: Rect,
    state: &AuthPanelState,
    selected: Option<usize>,
) {
    let title = Span::styled(
        " Auth ",
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = build_lines(state, selected);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Build the panel's full set of lines, including section headers,
/// dividers, rows, and the global-section hint.
fn build_lines(state: &AuthPanelState, selected: Option<usize>) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let editable_offset_workspace = 0usize;
    let editable_offset_role = state.workspace_rows.len();

    // Section 1: Global defaults (read-only).
    lines.push(section_header("Global defaults"));
    for row in &state.global_rows {
        lines.push(format_row_line(row, false, /* show_role = */ false));
    }
    lines.push(Line::from(Span::styled(
        "  (edit on the global-config screen)",
        Style::default().fg(PHOSPHOR_DIM),
    )));

    // Divider between sections 1 and 2.
    lines.push(divider());

    // Section 2: This workspace (editable).
    lines.push(section_header("This workspace"));
    for (i, row) in state.workspace_rows.iter().enumerate() {
        let is_selected = selected == Some(editable_offset_workspace + i);
        lines.push(format_row_line(
            row,
            is_selected,
            /* show_role = */ false,
        ));
    }

    // Divider between sections 2 and 3.
    lines.push(divider());

    // Section 3: Per role × agent (editable).
    lines.push(section_header("Per role x agent"));
    for (i, row) in state.role_agent_rows.iter().enumerate() {
        let is_selected = selected == Some(editable_offset_role + i);
        lines.push(format_row_line(
            row,
            is_selected,
            /* show_role = */ true,
        ));
    }

    lines
}

fn section_header(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    ))
}

fn divider() -> Line<'static> {
    Line::from(Span::styled(
        "  ".to_string() + &"-".repeat(48),
        Style::default().fg(PHOSPHOR_DARK),
    ))
}

fn format_row_line(row: &AuthRow, is_selected: bool, show_role: bool) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let label = if show_role {
        format!(
            "{prefix}{role} / {agent}: {mode}",
            role = row.role,
            agent = agent_display(row.agent),
            mode = mode_str(row.mode),
        )
    } else {
        format!(
            "{prefix}{agent}: {mode}",
            agent = agent_display(row.agent),
            mode = mode_str(row.mode),
        )
    };
    let label_style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };

    let provenance = format!(" ({})", provenance_str(row.provenance));
    let provenance_style = Style::default().fg(PHOSPHOR_DIM);

    let (badge_text, badge_style) = badge_display(row.credential);

    Line::from(vec![
        Span::styled(label, label_style),
        Span::styled(provenance, provenance_style),
        Span::raw("  "),
        Span::styled(badge_text, badge_style),
    ])
}

const fn agent_display(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude",
        Agent::Codex => "Codex",
    }
}

const fn mode_str(m: AuthForwardMode) -> &'static str {
    match m {
        AuthForwardMode::Sync => "sync",
        AuthForwardMode::ApiKey => "api_key",
        AuthForwardMode::OAuthToken => "oauth_token",
        AuthForwardMode::Ignore => "ignore",
    }
}

const fn provenance_str(p: ProvenanceTag) -> &'static str {
    match p {
        ProvenanceTag::Global => "global",
        ProvenanceTag::Workspace => "workspace",
        ProvenanceTag::MostSpecific => "most-specific",
        ProvenanceTag::Inherited => "inherited",
    }
}

/// Render the credential badge as plain ASCII so terminal-renderer
/// snapshots stay portable. The check / cross / dash characters in
/// the design doc map to `OK`, `!`, and `-` respectively.
const fn badge_display(b: CredentialBadge) -> (&'static str, Style) {
    match b {
        CredentialBadge::Resolves => (
            "OK resolves",
            Style {
                fg: Some(PHOSPHOR_GREEN),
                bg: None,
                underline_color: None,
                add_modifier: Modifier::empty(),
                sub_modifier: Modifier::empty(),
            },
        ),
        CredentialBadge::Unset => (
            "! unset",
            Style {
                fg: Some(DANGER_RED),
                bg: None,
                underline_color: None,
                add_modifier: Modifier::BOLD,
                sub_modifier: Modifier::empty(),
            },
        ),
        CredentialBadge::NotApplicable => (
            "-",
            Style {
                fg: Some(PHOSPHOR_DIM),
                bg: None,
                underline_color: None,
                add_modifier: Modifier::empty(),
                sub_modifier: Modifier::empty(),
            },
        ),
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
mod render_tests {
    use super::*;
    use crate::config::{AgentAuthConfig, AppConfig};
    use crate::operator_env::EnvValue;
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    use ratatui::{Terminal, backend::TestBackend};

    fn dump(state: &AuthPanelState, selected: Option<usize>) -> String {
        let backend = TestBackend::new(100, 30);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = f.area();
            render_with_selection(f, area, state, selected);
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

    fn cfg_with_role(role: &str) -> AppConfig {
        let mut cfg = AppConfig {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Sync,
            }),
            ..AppConfig::default()
        };
        let ws = WorkspaceConfig {
            workdir: "/tmp/proj".to_string(),
            allowed_roles: vec![role.to_string()],
            ..Default::default()
        };
        cfg.workspaces.insert("proj".into(), ws);
        cfg
    }

    #[test]
    fn renders_three_sections_with_dividers() {
        let cfg = cfg_with_role("smith");
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let s = dump(&state, None);

        assert!(
            s.contains("Global defaults"),
            "missing Global defaults header; dump:\n{s}"
        );
        assert!(
            s.contains("This workspace"),
            "missing This workspace header; dump:\n{s}"
        );
        assert!(
            s.contains("Per role x agent"),
            "missing Per role x agent header; dump:\n{s}"
        );
        // Dividers (long run of '-' characters between sections).
        assert!(
            s.contains(&"-".repeat(20)),
            "missing divider between sections; dump:\n{s}"
        );
        // Both agents present in global + workspace sections.
        assert!(s.contains("Claude"), "missing Claude label; dump:\n{s}");
        assert!(s.contains("Codex"), "missing Codex label; dump:\n{s}");
        // Role appears in role × agent section.
        assert!(
            s.contains("smith"),
            "missing role name in role x agent section; dump:\n{s}"
        );
        // Hint pointing at global-config screen.
        assert!(
            s.contains("global-config screen"),
            "missing global-config hint; dump:\n{s}"
        );
    }

    #[test]
    fn row_displays_mode_provenance_and_badge() {
        let mut cfg = cfg_with_role("smith");
        let ws = cfg.workspaces.get_mut("proj").unwrap();
        ws.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let s = dump(&state, None);

        // Mode label rendered.
        assert!(
            s.contains("api_key"),
            "missing api_key mode label; dump:\n{s}"
        );
        // Provenance tag rendered (workspace overrides global → workspace prov).
        assert!(
            s.contains("workspace"),
            "missing workspace provenance tag; dump:\n{s}"
        );
        // Unset badge rendered (ANTHROPIC_API_KEY not set anywhere).
        assert!(s.contains("unset"), "missing unset badge; dump:\n{s}");
    }

    #[test]
    fn resolves_badge_shown_when_env_var_present() {
        let mut cfg = cfg_with_role("smith");
        let ws = cfg.workspaces.get_mut("proj").unwrap();
        ws.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        ws.env.insert(
            "ANTHROPIC_API_KEY".into(),
            EnvValue::Plain("sk-ant-test".into()),
        );
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let s = dump(&state, None);

        assert!(
            s.contains("resolves"),
            "missing resolves badge when env var is set; dump:\n{s}"
        );
    }

    #[test]
    fn most_specific_provenance_rendered_for_role_override() {
        let mut cfg = cfg_with_role("smith");
        let ws = cfg.workspaces.get_mut("proj").unwrap();
        ws.roles.insert(
            "smith".into(),
            WorkspaceRoleOverride {
                claude: Some(AgentAuthConfig {
                    auth_forward: AuthForwardMode::OAuthToken,
                }),
                ..Default::default()
            },
        );
        let state = AuthPanelState::compute_for(&cfg, "proj");
        let s = dump(&state, None);

        assert!(
            s.contains("oauth_token"),
            "role override mode missing; dump:\n{s}"
        );
        assert!(
            s.contains("most-specific"),
            "missing most-specific provenance for role override; dump:\n{s}"
        );
    }

    #[test]
    fn selection_marker_appears_on_selected_row() {
        let cfg = cfg_with_role("smith");
        let state = AuthPanelState::compute_for(&cfg, "proj");
        // Select the first workspace row (index 0 in the editable surface).
        let s = dump(&state, Some(0));
        assert!(
            s.contains('\u{25b8}'),
            "selected row must render the triangle cursor; dump:\n{s}"
        );
    }
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
