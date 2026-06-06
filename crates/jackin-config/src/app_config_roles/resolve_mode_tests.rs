//! Tests for `roles` — resolve mode tests.
use super::*;
use crate::{AgentAuthConfig, AppConfig, AuthForwardMode, GithubAuthConfig, GithubAuthMode};
use crate::{WorkspaceConfig, WorkspaceRoleOverride};
use jackin_core::Agent;
use jackin_core::EnvValue;
use std::collections::BTreeMap;

/// Build an `AppConfig` with optionally-set Claude modes at each of
/// the 3 layers: global, workspace, workspace × role.
fn cfg_claude(
    global: Option<AuthForwardMode>,
    ws: Option<AuthForwardMode>,
    ws_role: Option<AuthForwardMode>,
) -> AppConfig {
    let mut cfg = AppConfig::default();
    if let Some(m) = global {
        cfg.claude = Some(AgentAuthConfig {
            auth_forward: m,
            ..Default::default()
        });
    }
    let mut ws_cfg = WorkspaceConfig::default();
    if let Some(m) = ws {
        ws_cfg.claude = Some(AgentAuthConfig {
            auth_forward: m,
            ..Default::default()
        });
    }
    if let Some(m) = ws_role {
        let over = WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: m,
                ..Default::default()
            }),
            ..Default::default()
        };
        ws_cfg.roles.insert("smith".to_string(), over);
    }
    cfg.workspaces.insert("proj".to_string(), ws_cfg);
    cfg
}

#[test]
fn default_is_sync_when_nothing_set() {
    let cfg = cfg_claude(None, None, None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
        AuthForwardMode::Sync
    );
}

#[test]
fn global_used_when_others_unset() {
    let cfg = cfg_claude(Some(AuthForwardMode::ApiKey), None, None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
        AuthForwardMode::ApiKey
    );
}

#[test]
fn workspace_overrides_global() {
    let cfg = cfg_claude(
        Some(AuthForwardMode::ApiKey),
        Some(AuthForwardMode::OAuthToken),
        None,
    );
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
        AuthForwardMode::OAuthToken
    );
}

#[test]
fn role_override_wins() {
    let cfg = cfg_claude(
        Some(AuthForwardMode::ApiKey),
        Some(AuthForwardMode::OAuthToken),
        Some(AuthForwardMode::Ignore),
    );
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
        AuthForwardMode::Ignore
    );
}

#[test]
fn workspace_only_when_global_unset() {
    let cfg = cfg_claude(None, Some(AuthForwardMode::ApiKey), None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
        AuthForwardMode::ApiKey
    );
}

#[test]
fn role_only_when_global_and_workspace_unset() {
    let cfg = cfg_claude(None, None, Some(AuthForwardMode::OAuthToken));
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
        AuthForwardMode::OAuthToken
    );
}

#[test]
fn unknown_workspace_falls_back_to_global() {
    let cfg = cfg_claude(Some(AuthForwardMode::ApiKey), None, None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "nonexistent", "smith"),
        AuthForwardMode::ApiKey
    );
}

#[test]
fn unknown_role_falls_back_to_workspace_or_global() {
    let cfg = cfg_claude(
        Some(AuthForwardMode::ApiKey),
        Some(AuthForwardMode::OAuthToken),
        None,
    );
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, "proj", "ghost"),
        AuthForwardMode::OAuthToken
    );
}

#[test]
fn codex_isolated_from_claude_global() {
    let cfg = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        // codex unset
        ..AppConfig::default()
    };
    assert_eq!(
        resolve_mode(&cfg, Agent::Codex, "proj", "smith"),
        AuthForwardMode::Sync
    );
}

#[test]
fn codex_uses_codex_layer() {
    let cfg = AppConfig {
        codex: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..AppConfig::default()
    };
    assert_eq!(
        resolve_mode(&cfg, Agent::Codex, "proj", "smith"),
        AuthForwardMode::ApiKey
    );
}

#[test]
fn amp_uses_amp_layer() {
    let cfg = AppConfig {
        amp: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..AppConfig::default()
    };
    assert_eq!(
        resolve_mode(&cfg, Agent::Amp, "proj", "smith"),
        AuthForwardMode::ApiKey
    );
}

// ── build_github_env_layers — 3-layer merge precedence ──────

fn ws_with_github_env(env: BTreeMap<String, EnvValue>) -> WorkspaceConfig {
    WorkspaceConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Sync,
            env,
        }),
        ..WorkspaceConfig::default()
    }
}

#[test]
fn build_github_env_layers_global_only() {
    let mut global_env = BTreeMap::new();
    global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
    let cfg = AppConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Sync,
            env: global_env,
        }),
        ..AppConfig::default()
    };
    let merged = build_github_env_layers(&cfg, "proj", "smith");
    assert_eq!(
        merged.get("GH_TOKEN"),
        Some(&EnvValue::Plain("ghp_global".into()))
    );
}

#[test]
fn build_github_env_layers_workspace_overrides_global() {
    let mut global_env = BTreeMap::new();
    global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
    let mut ws_env = BTreeMap::new();
    ws_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_ws".into()));
    let mut workspaces = BTreeMap::new();
    workspaces.insert("proj".into(), ws_with_github_env(ws_env));
    let cfg = AppConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Sync,
            env: global_env,
        }),
        workspaces,
        ..AppConfig::default()
    };
    let merged = build_github_env_layers(&cfg, "proj", "smith");
    assert_eq!(
        merged.get("GH_TOKEN"),
        Some(&EnvValue::Plain("ghp_ws".into()))
    );
}

#[test]
fn build_github_env_layers_role_overrides_workspace_and_global() {
    let mut global_env = BTreeMap::new();
    global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
    let mut ws_env = BTreeMap::new();
    ws_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_ws".into()));
    let mut role_env = BTreeMap::new();
    role_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_role".into()));
    let mut ws = ws_with_github_env(ws_env);
    ws.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                env: role_env,
            }),
            ..WorkspaceRoleOverride::default()
        },
    );
    let mut workspaces = BTreeMap::new();
    workspaces.insert("proj".into(), ws);
    let cfg = AppConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Sync,
            env: global_env,
        }),
        workspaces,
        ..AppConfig::default()
    };
    let merged = build_github_env_layers(&cfg, "proj", "smith");
    assert_eq!(
        merged.get("GH_TOKEN"),
        Some(&EnvValue::Plain("ghp_role".into()))
    );
}

#[test]
fn build_github_env_layers_preserves_distinct_keys_across_layers() {
    let mut global_env = BTreeMap::new();
    global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
    let mut ws_env = BTreeMap::new();
    ws_env.insert("GH_HOST".into(), EnvValue::Plain("ghe.acme.com".into()));
    let mut role_env = BTreeMap::new();
    role_env.insert(
        "GH_ENTERPRISE_TOKEN".into(),
        EnvValue::Plain("ent_token".into()),
    );
    let mut ws = ws_with_github_env(ws_env);
    ws.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                env: role_env,
            }),
            ..WorkspaceRoleOverride::default()
        },
    );
    let mut workspaces = BTreeMap::new();
    workspaces.insert("proj".into(), ws);
    let cfg = AppConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Sync,
            env: global_env,
        }),
        workspaces,
        ..AppConfig::default()
    };
    let merged = build_github_env_layers(&cfg, "proj", "smith");
    // All 3 keys survive — different keys at different layers.
    assert_eq!(merged.len(), 3);
    assert!(merged.contains_key("GH_TOKEN"));
    assert!(merged.contains_key("GH_HOST"));
    assert!(merged.contains_key("GH_ENTERPRISE_TOKEN"));
}

#[test]
fn build_github_env_layers_empty_when_no_layers_set() {
    let cfg = AppConfig::default();
    let merged = build_github_env_layers(&cfg, "proj", "smith");
    assert!(merged.is_empty());
}

// ── resolve_sync_source_dir tests (Defect 46 Phase B) ─────────────────────

#[test]
fn resolve_sync_source_dir_global_wins_when_nothing_else_set() {
    use std::path::PathBuf;
    let mut cfg = AppConfig::default();
    let dir = PathBuf::from("/opt/claude");
    cfg.claude = Some(AgentAuthConfig {
        auth_forward: AuthForwardMode::Sync,
        sync_source_dir: Some(dir.clone()),
    });
    assert_eq!(
        resolve_sync_source_dir(&cfg, Agent::Claude, "ws", "role"),
        Some(dir)
    );
}

#[test]
fn resolve_sync_source_dir_workspace_wins_over_global() {
    use std::path::PathBuf;
    let mut cfg = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/global/claude")),
        }),
        ..AppConfig::default()
    };
    let ws = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/workspace/claude")),
        }),
        ..WorkspaceConfig::default()
    };
    cfg.workspaces.insert("ws".into(), ws);
    assert_eq!(
        resolve_sync_source_dir(&cfg, Agent::Claude, "ws", "role"),
        Some(PathBuf::from("/workspace/claude"))
    );
}

#[test]
fn resolve_sync_source_dir_role_override_wins_over_workspace() {
    use std::path::PathBuf;
    let mut cfg = AppConfig::default();
    let mut ws = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/workspace/claude")),
        }),
        ..WorkspaceConfig::default()
    };
    let role_override = WorkspaceRoleOverride {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/role/claude")),
        }),
        ..WorkspaceRoleOverride::default()
    };
    ws.roles.insert("smith".into(), role_override);
    cfg.workspaces.insert("ws".into(), ws);
    assert_eq!(
        resolve_sync_source_dir(&cfg, Agent::Claude, "ws", "smith"),
        Some(PathBuf::from("/role/claude"))
    );
}

#[test]
fn resolve_sync_source_dir_none_when_not_set() {
    let cfg = AppConfig::default();
    assert_eq!(
        resolve_sync_source_dir(&cfg, Agent::Claude, "ws", "role"),
        None
    );
}
