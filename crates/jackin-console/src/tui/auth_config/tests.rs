use std::collections::BTreeMap;
use std::path::PathBuf;

use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, EnvValue, GithubAuthConfig, GithubAuthMode,
    WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_core::{Agent, env_model};

use super::*;

#[test]
fn auth_kind_agent_returns_none_for_github() {
    assert_eq!(auth_kind_agent(AuthKind::Github), None);
    assert_eq!(auth_kind_agent(AuthKind::Claude), Some(Agent::Claude));
    assert_eq!(auth_kind_agent(AuthKind::Codex), Some(Agent::Codex));
    assert_eq!(auth_kind_agent(AuthKind::Amp), Some(Agent::Amp));
    assert_eq!(auth_kind_agent(AuthKind::Kimi), Some(Agent::Kimi));
    assert_eq!(auth_kind_agent(AuthKind::Opencode), Some(Agent::Opencode));
    assert_eq!(auth_kind_agent(AuthKind::Grok), Some(Agent::Grok));
}

#[test]
fn auth_mode_to_auth_forward_round_trip() {
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::ApiKey,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::Ignore,
    ] {
        assert_eq!(
            auth_mode_to_auth_forward(auth_mode_from_auth_forward(mode)),
            Some(mode)
        );
    }
}

#[test]
fn auth_mode_to_github_round_trip() {
    for mode in [
        GithubAuthMode::Sync,
        GithubAuthMode::Token,
        GithubAuthMode::Ignore,
    ] {
        assert_eq!(auth_mode_to_github(auth_mode_from_github(mode)), Some(mode));
    }
}

#[test]
fn github_auth_config_preserves_env_on_mode_change() {
    let mut existing = GithubAuthConfig::default();
    existing
        .env
        .insert("GH_TOKEN".to_owned(), EnvValue::Plain("token".into()));

    let next = github_auth_config_with_preserved_env(Some(AuthMode::Ignore), Some(&existing))
        .expect("github mode should build config");

    assert_eq!(next.auth_forward, GithubAuthMode::Ignore);
    assert_eq!(next.env, existing.env);
    assert!(
        github_auth_config_with_preserved_env(Some(AuthMode::ApiKey), Some(&existing)).is_none()
    );
    assert!(github_auth_config_with_preserved_env(None, Some(&existing)).is_none());
}

#[test]
fn agent_auth_mode_change_preserves_sync_source_dir() {
    let mut ws = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/host/claude")),
        }),
        ..Default::default()
    };

    set_auth_mode(&mut ws, AuthKind::Claude, Some(AuthMode::ApiKey));

    let claude = ws.claude.expect("claude auth block");
    assert_eq!(claude.auth_forward, AuthForwardMode::ApiKey);
    assert_eq!(claude.sync_source_dir, Some(PathBuf::from("/host/claude")));
}

#[test]
fn workspace_sync_source_dir_set_and_reset_keep_mode_rules() {
    let mut ws = WorkspaceConfig::default();

    set_workspace_sync_source_dir(
        &mut ws,
        AuthKind::Claude,
        Some(PathBuf::from("/host/claude")),
    );
    let claude = ws.claude.as_ref().expect("claude source block");
    assert_eq!(claude.auth_forward, AuthForwardMode::Sync);
    assert_eq!(claude.sync_source_dir, Some(PathBuf::from("/host/claude")));

    set_workspace_sync_source_dir(&mut ws, AuthKind::Claude, None);
    assert!(ws.claude.is_none());
}

#[test]
fn role_sync_source_dir_set_reset_and_github_noop() {
    let mut role = WorkspaceRoleOverride::default();

    set_role_sync_source_dir(
        &mut role,
        AuthKind::Claude,
        Some(PathBuf::from("/host/claude")),
    );
    assert_eq!(
        role.claude
            .as_ref()
            .and_then(|cfg| cfg.sync_source_dir.clone()),
        Some(PathBuf::from("/host/claude"))
    );

    set_role_sync_source_dir(&mut role, AuthKind::Github, Some(PathBuf::from("/host/gh")));
    assert!(role.github.is_none());

    set_role_sync_source_dir(&mut role, AuthKind::Claude, None);
    assert!(role.claude.is_none());
}

#[test]
fn apply_workspace_auth_commit_updates_mode_and_env_layer() {
    let mut ws = WorkspaceConfig::default();

    apply_workspace_auth_commit(
        &mut ws,
        AuthKind::Github,
        AuthMode::Token,
        Some("GH_TOKEN"),
        Some(EnvValue::Plain("token".into())),
    );

    let github = ws.github.expect("github auth should be stored");
    assert_eq!(github.auth_forward, GithubAuthMode::Token);
    assert_eq!(
        github.env.get("GH_TOKEN"),
        Some(&EnvValue::Plain("token".into()))
    );
    assert!(ws.env.is_empty());
}

#[test]
fn apply_role_auth_commit_updates_mode_and_zai_ignore_removes_key() {
    let mut role = WorkspaceRoleOverride::default();
    role.env.insert(
        env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("stale".into()),
    );

    apply_role_auth_commit(&mut role, AuthKind::Zai, AuthMode::Ignore, None, None);

    assert!(!role.env.contains_key(env_model::ZAI_API_KEY_ENV_NAME));
}

#[test]
fn clear_workspace_auth_layer_removes_github_block() {
    let mut ws = WorkspaceConfig::default();
    apply_workspace_auth_commit(
        &mut ws,
        AuthKind::Github,
        AuthMode::Token,
        Some("GH_TOKEN"),
        Some(EnvValue::Plain("token".into())),
    );

    clear_workspace_auth_layer(&mut ws, AuthKind::Github);

    assert!(ws.github.is_none());
}

#[test]
fn apply_settings_auth_env_commit_routes_by_kind() {
    let mut github_env = BTreeMap::new();
    let mut agent_env = BTreeMap::new();

    apply_settings_auth_env_commit(
        AuthKind::Github,
        Some("GH_TOKEN"),
        Some(EnvValue::Plain("token".into())),
        &mut github_env,
        &mut agent_env,
    );
    apply_settings_auth_env_commit(
        AuthKind::Claude,
        Some("ANTHROPIC_API_KEY"),
        Some(EnvValue::Plain("key".into())),
        &mut github_env,
        &mut agent_env,
    );

    assert_eq!(
        github_env.get("GH_TOKEN"),
        Some(&EnvValue::Plain("token".into()))
    );
    assert_eq!(
        agent_env.get("ANTHROPIC_API_KEY"),
        Some(&EnvValue::Plain("key".into()))
    );
}

#[test]
fn clear_settings_auth_env_values_removes_kind_credentials() {
    let mut github_env = BTreeMap::new();
    let mut agent_env = BTreeMap::new();
    github_env.insert("GH_TOKEN".to_owned(), EnvValue::Plain("token".into()));
    agent_env.insert(
        "ANTHROPIC_API_KEY".to_owned(),
        EnvValue::Plain("key".into()),
    );

    clear_settings_auth_env_values(AuthKind::Github, &mut github_env, &mut agent_env);

    assert!(!github_env.contains_key("GH_TOKEN"));
    assert!(agent_env.contains_key("ANTHROPIC_API_KEY"));
}

#[test]
fn app_github_env_reads_global_github_env() {
    let mut cfg = AppConfig::default();
    assert!(app_github_env(&cfg).is_empty());

    let mut github = GithubAuthConfig::default();
    github
        .env
        .insert("GH_TOKEN".to_owned(), EnvValue::Plain("token".into()));
    cfg.github = Some(github.clone());

    assert_eq!(app_github_env(&cfg), github.env);
}

#[test]
fn role_override_present_false_when_no_blocks_set() {
    let ro = WorkspaceRoleOverride::default();
    assert!(!role_override_present(AuthKind::Claude, &ro));
    assert!(!role_override_present(AuthKind::Codex, &ro));
    assert!(!role_override_present(AuthKind::Amp, &ro));
    assert!(!role_override_present(AuthKind::Kimi, &ro));
    assert!(!role_override_present(AuthKind::Opencode, &ro));
    assert!(!role_override_present(AuthKind::Grok, &ro));
    assert!(!role_override_present(AuthKind::Github, &ro));
    assert!(!role_override_present(AuthKind::Zai, &ro));
}

#[test]
fn role_override_present_zai_keys_off_env_var() {
    let mut ro = WorkspaceRoleOverride::default();
    assert!(!role_override_present(AuthKind::Zai, &ro));
    ro.env.insert(
        env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("k".into()),
    );
    assert!(role_override_present(AuthKind::Zai, &ro));
    assert!(!role_override_present(AuthKind::Claude, &ro));
    assert!(!role_override_present(AuthKind::Github, &ro));
}
