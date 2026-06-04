//! Tests for `editor` auth flat rows rendering.
use crate::config::AppConfig;
use crate::console::domain::resolve_panel_mode;
use crate::console::tui::state::EditorState;
use crate::console::tui::state::{AuthRow, auth_flat_rows};
use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
use jackin_console::tui::auth::{AuthKind, AuthMode};

#[test]
fn root_view_lists_auth_kinds_in_design_order() {
    let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    let rows = auth_flat_rows(&editor, &AppConfig::default());
    assert_eq!(
        rows,
        vec![
            AuthRow::AuthKindRow {
                kind: AuthKind::Claude,
            },
            AuthRow::AuthKindRow {
                kind: AuthKind::Codex,
            },
            AuthRow::AuthKindRow {
                kind: AuthKind::Amp,
            },
            AuthRow::AuthKindRow {
                kind: AuthKind::Opencode,
            },
            AuthRow::AuthKindRow {
                kind: AuthKind::Github,
            },
            AuthRow::AuthKindRow {
                kind: AuthKind::Zai,
            },
            AuthRow::AuthKindRow {
                kind: AuthKind::Minimax,
            },
        ],
        "root view must list Claude / Codex / Amp / Opencode / Github / Z.AI / MiniMax in this order"
    );
}

#[test]
fn zai_panel_mode_uses_all_operator_env_layers() {
    let mut cfg = AppConfig::default();
    cfg.env.insert(
        "ZAI_API_KEY".into(),
        crate::operator_env::EnvValue::Plain("global-key".into()),
    );
    cfg.workspaces
        .insert("global-demo".into(), WorkspaceConfig::default());
    assert_eq!(
        resolve_panel_mode(&cfg, AuthKind::Zai, "global-demo", "the-architect"),
        AuthMode::ApiKey
    );
    cfg.env.clear();

    let mut workspace = WorkspaceConfig::default();
    workspace.env.insert(
        "ZAI_API_KEY".into(),
        crate::operator_env::EnvValue::Plain("workspace-key".into()),
    );
    cfg.workspaces.insert("workspace-demo".into(), workspace);
    assert_eq!(
        resolve_panel_mode(&cfg, AuthKind::Zai, "workspace-demo", "the-architect"),
        AuthMode::ApiKey
    );

    cfg.workspaces.remove("workspace-demo");
    let mut role = crate::config::RoleSource::default();
    role.env.insert(
        "ZAI_API_KEY".into(),
        crate::operator_env::EnvValue::Plain("role-key".into()),
    );
    cfg.roles.insert("the-architect".into(), role);
    cfg.workspaces
        .insert("role-demo".into(), WorkspaceConfig::default());
    assert_eq!(
        resolve_panel_mode(&cfg, AuthKind::Zai, "role-demo", "the-architect"),
        AuthMode::ApiKey
    );

    cfg.roles.clear();
    let mut workspace_role = WorkspaceConfig::default();
    let mut override_cfg = WorkspaceRoleOverride::default();
    override_cfg.env.insert(
        "ZAI_API_KEY".into(),
        crate::operator_env::EnvValue::Plain("workspace-role-key".into()),
    );
    workspace_role
        .roles
        .insert("the-architect".into(), override_cfg);
    cfg.workspaces
        .insert("workspace-role-demo".into(), workspace_role);
    assert_eq!(
        resolve_panel_mode(&cfg, AuthKind::Zai, "workspace-role-demo", "the-architect"),
        AuthMode::ApiKey
    );

    // No ZAI_API_KEY at any layer → Ignore. This is the branch that
    // suppresses the Source credential row; a regression to ApiKey here
    // would render a phantom row for every Z.AI panel without a key.
    assert_eq!(
        resolve_panel_mode(
            &AppConfig::default(),
            AuthKind::Zai,
            "absent",
            "the-architect"
        ),
        AuthMode::Ignore
    );
}

#[test]
fn role_with_override_renders_collapsed_header_then_sentinel() {
    use crate::config::{AgentAuthConfig, AuthForwardMode};
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    let mut ws = WorkspaceConfig {
        allowed_roles: vec!["the-architect".into(), "agent-smith".into()],
        ..Default::default()
    };
    let over = WorkspaceRoleOverride {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Ignore,
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.auth_selected_kind = Some(AuthKind::Claude);
    let rows = auth_flat_rows(&editor, &AppConfig::default());

    let header_idx = rows
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::RoleHeader {
                    role,
                    expanded: false
                } if role == "the-architect"
            )
        })
        .expect("role override header expected");
    assert!(matches!(
        rows[header_idx],
        AuthRow::RoleHeader { ref role, expanded: false } if role == "the-architect"
    ));
    assert!(matches!(rows[header_idx + 1], AuthRow::Spacer));
    assert!(matches!(
        rows[header_idx + 2],
        AuthRow::AddSentinel { eligible: 1 }
    ));
}

#[test]
fn role_with_override_when_expanded_emits_kind_rows() {
    use crate::config::{AgentAuthConfig, AuthForwardMode, CodexAuthConfig};
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    let mut ws = WorkspaceConfig {
        allowed_roles: vec!["the-architect".into()],
        ..Default::default()
    };
    let over = WorkspaceRoleOverride {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Ignore,
        }),
        codex: Some(CodexAuthConfig(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        })),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.auth_selected_kind = Some(AuthKind::Claude);
    editor.auth_expanded.insert("the-architect".into());
    let rows = auth_flat_rows(&editor, &AppConfig::default());

    let header_pos = rows
        .iter()
        .position(|r| matches!(r, AuthRow::RoleHeader { expanded: true, .. }))
        .expect("expanded role header missing");
    assert!(matches!(
        rows[header_pos + 1],
        AuthRow::RoleMode { ref role, kind: AuthKind::Claude } if role == "the-architect"
    ));
}

#[test]
fn resolve_auth_row_target_picks_workspace_default_for_workspacedefault_row() {
    use crate::console::tui::state::AuthFormTarget;
    use crate::workspace::WorkspaceConfig;

    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.auth_selected_kind = Some(AuthKind::Claude);
    let cfg = AppConfig::default();
    let rows = auth_flat_rows(&editor, &cfg);
    let workspace_claude_idx = rows
        .iter()
        .position(|r| {
            matches!(
                r,
                AuthRow::WorkspaceMode {
                    kind: AuthKind::Claude
                }
            )
        })
        .unwrap();
    assert_eq!(
        super::resolve_auth_row_target(&editor, &cfg, workspace_claude_idx),
        Some(AuthFormTarget::Workspace {
            kind: AuthKind::Claude
        }),
    );
}

#[test]
fn resolve_auth_row_target_returns_none_for_navigation_and_header_rows() {
    use crate::workspace::WorkspaceConfig;
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.auth_selected_kind = Some(AuthKind::Claude);
    let cfg = AppConfig::default();
    let rows = auth_flat_rows(&editor, &cfg);
    for (idx, row) in rows.iter().enumerate() {
        match row {
            AuthRow::AuthKindRow { .. }
            | AuthRow::AddSentinel { .. }
            | AuthRow::Spacer
            | AuthRow::RoleHeader { .. } => assert!(
                super::resolve_auth_row_target(&editor, &cfg, idx).is_none(),
                "row {idx} ({row:?}) must not resolve to an editable target"
            ),
            _ => {}
        }
    }
}

/// Globally configured `api_key` mode (in `[claude].auth_forward`)
/// must surface a `WorkspaceSource` row so the operator can set
/// the credential — even when the workspace has no explicit
/// `claude` block of its own.
#[test]
fn workspace_source_surfaces_when_global_requires_credential() {
    use crate::config::{AgentAuthConfig, AuthForwardMode};
    use crate::workspace::WorkspaceConfig;
    let config = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        }),
        ..AppConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.auth_selected_kind = Some(AuthKind::Claude);

    let rows = auth_flat_rows(&editor, &config);
    assert!(
        rows.iter().any(|r| matches!(
            r,
            AuthRow::WorkspaceSource {
                kind: AuthKind::Claude
            }
        )),
        "global claude.auth_forward = api_key must surface WorkspaceSource row; got {rows:?}"
    );
}

/// Selecting the GitHub kind opens a detail view that mirrors the
/// Claude / Codex shape: workspace mode → spacer → add-sentinel.
/// The agent dimension is intentionally absent (Github has no per-
/// agent split).
#[test]
fn github_detail_view_emits_workspace_mode_then_sentinel() {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.auth_selected_kind = Some(AuthKind::Github);
    let rows = auth_flat_rows(&editor, &AppConfig::default());
    // Sync mode (the global default) requires no credential — no
    // WorkspaceSource row.
    assert!(
        matches!(
            rows.first(),
            Some(AuthRow::WorkspaceMode {
                kind: AuthKind::Github
            })
        ),
        "first row must be the GitHub workspace mode; got {rows:?}"
    );
    assert!(
        rows.iter()
            .any(|r| matches!(r, AuthRow::AddSentinel { .. })),
        "+ Override sentinel must be present; got {rows:?}"
    );
}

/// Globally configured `token` mode must surface a `WorkspaceSource`
/// row for `GH_TOKEN` so the operator can set the credential without
/// chasing an explicit workspace-level `[github]` block.
#[test]
fn github_workspace_source_surfaces_for_global_token_mode() {
    use crate::config::{GithubAuthConfig, GithubAuthMode};
    let config = AppConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..AppConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.auth_selected_kind = Some(AuthKind::Github);

    let rows = auth_flat_rows(&editor, &config);
    assert!(
        rows.iter().any(|r| matches!(
            r,
            AuthRow::WorkspaceSource {
                kind: AuthKind::Github
            }
        )),
        "global github.auth_forward = token must surface WorkspaceSource row; got {rows:?}"
    );
}

/// A workspace × role override on the Github kind shows up as a
/// collapsed `RoleHeader` in the detail view, exactly like Claude /
/// Codex overrides do.
#[test]
fn github_role_override_emits_role_header_when_override_present() {
    use crate::config::{GithubAuthConfig, GithubAuthMode};
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    let mut ws = WorkspaceConfig {
        allowed_roles: vec!["the-architect".into()],
        ..Default::default()
    };
    let over = WorkspaceRoleOverride {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Ignore,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.auth_selected_kind = Some(AuthKind::Github);
    let rows = auth_flat_rows(&editor, &AppConfig::default());

    assert!(
        rows.iter().any(|r| {
            matches!(
                r,
                AuthRow::RoleHeader { role, .. } if role == "the-architect"
            )
        }),
        "github role override must surface a RoleHeader; got {rows:?}"
    );
}
