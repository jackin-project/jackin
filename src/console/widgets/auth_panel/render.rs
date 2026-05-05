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
