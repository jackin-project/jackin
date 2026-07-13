//! Tests for `roles` — tests.
use super::*;
use jackin_core::WorkspaceName;
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use tempfile::tempdir;

#[test]
fn bootstrap_writes_builtin_agent_entries() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let config = AppConfig::load_or_init(&paths).unwrap();

    assert_eq!(
        config.roles.get("agent-smith").unwrap().git,
        "https://github.com/jackin-project/jackin-agent-smith.git"
    );
    assert_eq!(
        config.roles.get("the-architect").unwrap().git,
        "https://github.com/jackin-project/jackin-the-architect.git"
    );
    assert!(paths.config_file.exists());
}

#[test]
fn sync_updates_stale_builtin_entries_and_preserves_user_agents() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "git@github.com:old/wrong-url.git"

[roles."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();

    // Built-in entries are corrected
    assert_eq!(
        config.roles.get("agent-smith").unwrap().git,
        "https://github.com/jackin-project/jackin-agent-smith.git"
    );
    // Missing built-in entries are added
    assert_eq!(
        config.roles.get("the-architect").unwrap().git,
        "https://github.com/jackin-project/jackin-the-architect.git"
    );
    // User-added entries are preserved
    assert_eq!(
        config.roles.get("chainargos/agent-brown").unwrap().git,
        "git@github.com:chainargos/jackin-agent-brown.git"
    );

    // Config file is updated on disk
    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(persisted.contains("jackin-project/jackin-agent-smith.git"));
    assert!(persisted.contains("jackin-project/jackin-the-architect.git"));
    assert!(persisted.contains("chainargos/jackin-agent-brown.git"));
}

#[test]
fn resolve_agent_source_adds_owner_repo_on_first_use() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    let (source, is_new) = config.resolve_role_source(&selector).unwrap();

    assert_eq!(
        source.git,
        "https://github.com/chainargos/jackin-the-architect.git"
    );
    assert!(is_new);

    // Not yet persisted — write via toml::to_string_pretty (AppConfig::save
    // was removed in Task 14; tests bootstrap the file directly).
    let contents = toml::to_string_pretty(&config).unwrap();
    std::fs::write(&paths.config_file, &contents).unwrap();
    assert!(
        std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("[roles.\"chainargos/the-architect\"]")
    );
}

// --- Trust model tests ---

#[test]
fn builtin_agents_are_trusted_on_bootstrap() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let config = AppConfig::load_or_init(&paths).unwrap();

    assert!(config.roles.get("agent-smith").unwrap().trusted);
    assert!(config.roles.get("the-architect").unwrap().trusted);
}

#[test]
fn new_namespaced_agent_is_not_trusted() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    let (source, _) = config.resolve_role_source(&selector).unwrap();

    assert!(!source.trusted);
}

#[test]
fn trust_agent_marks_source_as_trusted() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    config.resolve_role_source(&selector).unwrap();
    assert!(
        !config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    let changed = config.trust_agent("chainargos/the-architect");
    assert!(changed);
    assert!(
        config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    // Second call is idempotent
    let changed_again = config.trust_agent("chainargos/the-architect");
    assert!(!changed_again);
}

#[test]
fn untrust_agent_revokes_trust() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    config.resolve_role_source(&selector).unwrap();
    config.trust_agent("chainargos/the-architect");
    assert!(
        config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    let changed = config.untrust_agent("chainargos/the-architect");
    assert!(changed);
    assert!(
        !config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    // Second call is idempotent
    let changed_again = config.untrust_agent("chainargos/the-architect");
    assert!(!changed_again);
}

#[test]
fn trusted_flag_round_trips_through_toml() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    config.resolve_role_source(&selector).unwrap();
    config.trust_agent("chainargos/the-architect");
    // AppConfig::save removed in Task 14 — write the bootstrap file directly.
    let contents = toml::to_string_pretty(&config).unwrap();
    std::fs::write(&paths.config_file, &contents).unwrap();

    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert!(
        reloaded
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );
}

#[test]
fn sync_upgrades_untrusted_builtins() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    // Simulate a config from a pre-trust version (no trusted field)
    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.the-architect]
git = "https://github.com/jackin-project/jackin-the-architect.git"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();

    // Builtins should be upgraded to trusted
    assert!(config.roles.get("agent-smith").unwrap().trusted);
    assert!(config.roles.get("the-architect").unwrap().trusted);
}

// ── Auth forwarding config tests ────────────────────────────────────

#[test]
fn deserializes_global_claude_auth_forward() {
    let toml_str = r#"
[claude]
auth_forward = "sync"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.claude.as_ref().unwrap().auth_forward,
        AuthForwardMode::Sync
    );
}

// Tests for `roles` — resolve mode tests.
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
        ws_cfg.roles.insert("smith".to_owned(), over);
    }
    cfg.workspaces.insert("proj".to_owned(), ws_cfg);
    cfg
}

#[test]
fn default_is_sync_when_nothing_set() {
    let cfg = cfg_claude(None, None, None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, Some(&wn("proj")), "smith"),
        AuthForwardMode::Sync
    );
}

#[test]
fn global_used_when_others_unset() {
    let cfg = cfg_claude(Some(AuthForwardMode::ApiKey), None, None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, Some(&wn("proj")), "smith"),
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
        resolve_mode(&cfg, Agent::Claude, Some(&wn("proj")), "smith"),
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
        resolve_mode(&cfg, Agent::Claude, Some(&wn("proj")), "smith"),
        AuthForwardMode::Ignore
    );
}

#[test]
fn workspace_only_when_global_unset() {
    let cfg = cfg_claude(None, Some(AuthForwardMode::ApiKey), None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, Some(&wn("proj")), "smith"),
        AuthForwardMode::ApiKey
    );
}

#[test]
fn role_only_when_global_and_workspace_unset() {
    let cfg = cfg_claude(None, None, Some(AuthForwardMode::OAuthToken));
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, Some(&wn("proj")), "smith"),
        AuthForwardMode::OAuthToken
    );
}

#[test]
fn unknown_workspace_falls_back_to_global() {
    let cfg = cfg_claude(Some(AuthForwardMode::ApiKey), None, None);
    assert_eq!(
        resolve_mode(&cfg, Agent::Claude, Some(&wn("nonexistent")), "smith"),
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
        resolve_mode(&cfg, Agent::Claude, Some(&wn("proj")), "ghost"),
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
        resolve_mode(&cfg, Agent::Codex, Some(&wn("proj")), "smith"),
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
        resolve_mode(&cfg, Agent::Codex, Some(&wn("proj")), "smith"),
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
        resolve_mode(&cfg, Agent::Amp, Some(&wn("proj")), "smith"),
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
    let merged = build_github_env_layers(&cfg, Some(&wn("proj")), "smith");
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
    let merged = build_github_env_layers(&cfg, Some(&wn("proj")), "smith");
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
    let merged = build_github_env_layers(&cfg, Some(&wn("proj")), "smith");
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
    let merged = build_github_env_layers(&cfg, Some(&wn("proj")), "smith");
    // All 3 keys survive — different keys at different layers.
    assert_eq!(merged.len(), 3);
    assert!(merged.contains_key("GH_TOKEN"));
    assert!(merged.contains_key("GH_HOST"));
    assert!(merged.contains_key("GH_ENTERPRISE_TOKEN"));
}

#[test]
fn build_github_env_layers_empty_when_no_layers_set() {
    let cfg = AppConfig::default();
    let merged = build_github_env_layers(&cfg, Some(&wn("proj")), "smith");
    assert!(merged.is_empty());
}

// ── resolve_sync_source_dir tests ─────────────────────────────────────────

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
        resolve_sync_source_dir(&cfg, Agent::Claude, Some(&wn("ws")), "role"),
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
        resolve_sync_source_dir(&cfg, Agent::Claude, Some(&wn("ws")), "role"),
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
        resolve_sync_source_dir(&cfg, Agent::Claude, Some(&wn("ws")), "smith"),
        Some(PathBuf::from("/role/claude"))
    );
}

#[test]
fn resolve_sync_source_dir_none_when_not_set() {
    let cfg = AppConfig::default();
    assert_eq!(
        resolve_sync_source_dir(&cfg, Agent::Claude, Some(&wn("ws")), "role"),
        None
    );
}
