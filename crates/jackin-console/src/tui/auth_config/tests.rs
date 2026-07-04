// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::PathBuf;

use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, EnvValue, GithubAuthConfig, GithubAuthMode,
    RoleSource, WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_core::{Agent, env_model};

use super::*;
use crate::tui::components::editor_rows::AuthSourceFolderKind;

#[test]
fn synthesize_app_config_for_workspace_auth_keeps_globals_and_inserts_pending_workspace() {
    let mut config = AppConfig {
        env: BTreeMap::from([("GLOBAL".into(), EnvValue::Plain("1".into()))]),
        ..AppConfig::default()
    };
    config.roles.insert("alpha".into(), RoleSource::default());

    let pending = WorkspaceConfig {
        workdir: "/work".into(),
        ..WorkspaceConfig::default()
    };

    let synthesized =
        synthesize_app_config_for_workspace_auth(&config, "pending".into(), pending.clone());

    assert_eq!(
        synthesized.env.get("GLOBAL"),
        Some(&EnvValue::Plain("1".into()))
    );
    assert!(synthesized.roles.contains_key("alpha"));
    assert_eq!(
        synthesized.workspaces.get("pending").map(|ws| &ws.workdir),
        Some(&pending.workdir)
    );
}

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
fn auth_form_generate_token_policy_requires_existing_claude_oauth_form() {
    let target = AuthFormTarget::Workspace {
        kind: AuthKind::Claude,
    };
    assert!(editor_auth_form_can_generate_token(
        true,
        &target,
        AuthKind::Claude,
        Some(AuthMode::OAuthToken)
    ));
    assert!(!editor_auth_form_can_generate_token(
        false,
        &target,
        AuthKind::Claude,
        Some(AuthMode::OAuthToken)
    ));
    assert!(!editor_auth_form_can_generate_token(
        true,
        &target,
        AuthKind::Claude,
        Some(AuthMode::ApiKey)
    ));
    assert!(!editor_auth_form_can_generate_token(
        true,
        &AuthFormTarget::Workspace {
            kind: AuthKind::Codex,
        },
        AuthKind::Codex,
        Some(AuthMode::OAuthToken)
    ));

    assert!(settings_auth_form_can_generate_token(
        AuthKind::Claude,
        Some(AuthMode::OAuthToken)
    ));
    assert!(!settings_auth_form_can_generate_token(
        AuthKind::Codex,
        Some(AuthMode::OAuthToken)
    ));
}

#[test]
fn panel_mode_requires_credential_reads_effective_mode() {
    let cfg = AppConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..AppConfig::default()
    };

    assert!(panel_mode_requires_credential(
        &cfg,
        "workspace",
        "",
        AuthKind::Github
    ));
    assert!(!panel_mode_requires_credential(
        &cfg,
        "workspace",
        "",
        AuthKind::Claude
    ));
}

#[test]
fn settings_auth_env_value_uses_github_or_agent_env() {
    let mut github_env = BTreeMap::new();
    github_env.insert("GH_TOKEN".into(), EnvValue::Plain("github-token".into()));
    let mut agent_env = BTreeMap::new();
    agent_env.insert(
        AuthKind::Claude
            .required_env_var(AuthMode::ApiKey)
            .expect("Claude API key env var")
            .into(),
        EnvValue::Plain("anthropic-key".into()),
    );

    assert!(matches!(
        settings_auth_env_value(AuthKind::Github, AuthMode::Token, &github_env, &agent_env),
        Some(EnvValue::Plain(value)) if value == "github-token"
    ));
    assert!(matches!(
        settings_auth_env_value(AuthKind::Claude, AuthMode::ApiKey, &github_env, &agent_env),
        Some(EnvValue::Plain(value)) if value == "anthropic-key"
    ));
    assert!(
        settings_auth_env_value(AuthKind::Claude, AuthMode::Sync, &github_env, &agent_env)
            .is_none()
    );
}

#[test]
fn clear_ignored_env_only_settings_auth_keys_removes_zai_and_minimax_only() {
    let rows = vec![
        SettingsAuthRow {
            kind: AuthKind::Zai,
            mode: AuthMode::Ignore,
            sync_source_dir: None,
        },
        SettingsAuthRow {
            kind: AuthKind::Minimax,
            mode: AuthMode::Ignore,
            sync_source_dir: None,
        },
        SettingsAuthRow {
            kind: AuthKind::Claude,
            mode: AuthMode::Ignore,
            sync_source_dir: None,
        },
    ];
    let mut env = BTreeMap::from([
        (
            env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
            EnvValue::Plain("zai".into()),
        ),
        (
            env_model::MINIMAX_API_KEY_ENV_NAME.to_owned(),
            EnvValue::Plain("minimax".into()),
        ),
        (
            AuthKind::Claude
                .required_env_var(AuthMode::ApiKey)
                .expect("Claude API key env var")
                .to_owned(),
            EnvValue::Plain("claude".into()),
        ),
    ]);

    clear_ignored_env_only_settings_auth_keys(&rows, &mut env);

    assert!(!env.contains_key(env_model::ZAI_API_KEY_ENV_NAME));
    assert!(!env.contains_key(env_model::MINIMAX_API_KEY_ENV_NAME));
    assert!(
        env.contains_key(
            AuthKind::Claude
                .required_env_var(AuthMode::ApiKey)
                .expect("Claude API key env var")
        )
    );
}

#[test]
fn workspace_auth_mode_and_credential_reads_workspace_layers() {
    let mut workspace = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..Default::default()
    };
    workspace.env.insert(
        AuthKind::Claude
            .required_env_var(AuthMode::ApiKey)
            .expect("Claude API key env var")
            .into(),
        EnvValue::Plain("anthropic-key".into()),
    );
    workspace
        .github
        .as_mut()
        .expect("github")
        .env
        .insert("GH_TOKEN".into(), EnvValue::Plain("github-token".into()));

    assert!(matches!(
        workspace_auth_mode_and_credential(&workspace, AuthKind::Claude),
        (Some(AuthMode::ApiKey), Some(EnvValue::Plain(value))) if value == "anthropic-key"
    ));
    assert!(matches!(
        workspace_auth_mode_and_credential(&workspace, AuthKind::Github),
        (Some(AuthMode::Token), Some(EnvValue::Plain(value))) if value == "github-token"
    ));
}

#[test]
fn role_auth_mode_and_credential_reads_role_layers() {
    let mut role = WorkspaceRoleOverride {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..Default::default()
    };
    role.github
        .as_mut()
        .expect("github")
        .env
        .insert("GH_TOKEN".into(), EnvValue::Plain("github-token".into()));

    assert!(matches!(
        role_auth_mode_and_credential(Some(&role), AuthKind::Github),
        (Some(AuthMode::Token), Some(EnvValue::Plain(value))) if value == "github-token"
    ));
    assert_eq!(
        role_auth_mode_and_credential(None, AuthKind::Github),
        (None, None)
    );
}

#[test]
fn explicit_workspace_auth_mode_reads_workspace_block() {
    let workspace = WorkspaceConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..Default::default()
    };

    assert_eq!(
        explicit_workspace_auth_mode(&workspace, AuthKind::Github),
        Some(AuthMode::Token)
    );
    assert_eq!(
        explicit_workspace_auth_mode(&workspace, AuthKind::Claude),
        None
    );
}

#[test]
fn panel_auth_source_value_prefers_workspace_role_then_workspace_then_global() {
    let env_name = AuthKind::Claude
        .required_env_var(AuthMode::ApiKey)
        .expect("Claude API key env var");
    let mut cfg = AppConfig::default();
    cfg.env
        .insert(env_name.into(), EnvValue::Plain("global".into()));
    let mut workspace = WorkspaceConfig::default();
    workspace
        .env
        .insert(env_name.into(), EnvValue::Plain("workspace".into()));
    let mut role = WorkspaceRoleOverride::default();
    role.env
        .insert(env_name.into(), EnvValue::Plain("workspace-role".into()));
    workspace.roles.insert("smith".into(), role);
    cfg.workspaces.insert("ws".into(), workspace);

    assert!(matches!(
        panel_auth_source_value(&cfg, "ws", "smith", env_name, AuthKind::Claude),
        Some(EnvValue::Plain(value)) if value == "workspace-role"
    ));
    assert!(matches!(
        panel_auth_source_value(&cfg, "ws", "", env_name, AuthKind::Claude),
        Some(EnvValue::Plain(value)) if value == "workspace"
    ));
    assert!(matches!(
        panel_auth_source_value(&cfg, "missing", "", env_name, AuthKind::Claude),
        Some(EnvValue::Plain(value)) if value == "global"
    ));
}

#[test]
fn panel_auth_source_value_uses_github_env_layers() {
    let mut cfg = AppConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..Default::default()
    };
    cfg.github
        .as_mut()
        .expect("github")
        .env
        .insert("GH_TOKEN".into(), EnvValue::Plain("global-gh".into()));
    let mut workspace = WorkspaceConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..Default::default()
    };
    workspace
        .github
        .as_mut()
        .expect("github")
        .env
        .insert("GH_TOKEN".into(), EnvValue::Plain("workspace-gh".into()));
    cfg.workspaces.insert("ws".into(), workspace);

    assert!(matches!(
        panel_auth_source_value(&cfg, "ws", "", "GH_TOKEN", AuthKind::Github),
        Some(EnvValue::Plain(value)) if value == "workspace-gh"
    ));
    assert!(matches!(
        panel_auth_source_value(&cfg, "missing", "", "GH_TOKEN", AuthKind::Github),
        Some(EnvValue::Plain(value)) if value == "global-gh"
    ));
}

#[test]
fn resolve_panel_mode_reads_env_only_layers() {
    let mut cfg = AppConfig::default();
    cfg.env.insert(
        env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("global".into()),
    );
    assert_eq!(
        resolve_panel_mode(&cfg, AuthKind::Zai, "missing", "missing"),
        AuthMode::ApiKey
    );

    let mut workspace = WorkspaceConfig::default();
    workspace.env.insert(
        env_model::MINIMAX_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("workspace".into()),
    );
    cfg.workspaces.insert("ws".into(), workspace);
    assert_eq!(
        resolve_panel_mode(&cfg, AuthKind::Minimax, "ws", ""),
        AuthMode::ApiKey
    );
    assert_eq!(
        resolve_panel_mode(&cfg, AuthKind::Minimax, "missing", ""),
        AuthMode::Ignore
    );
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
fn clear_workspace_auth_layer_removes_env_only_keys() {
    let mut ws = WorkspaceConfig::default();
    ws.env.insert(
        env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("zai".into()),
    );
    ws.env.insert(
        env_model::MINIMAX_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("minimax".into()),
    );

    clear_workspace_auth_layer(&mut ws, AuthKind::Zai);
    clear_workspace_auth_layer(&mut ws, AuthKind::Minimax);

    assert!(!ws.env.contains_key(env_model::ZAI_API_KEY_ENV_NAME));
    assert!(!ws.env.contains_key(env_model::MINIMAX_API_KEY_ENV_NAME));
}

#[test]
fn clear_role_auth_layer_removes_typed_and_env_only_keys() {
    let mut role = WorkspaceRoleOverride {
        kimi: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            sync_source_dir: None,
        }),
        ..WorkspaceRoleOverride::default()
    };
    role.env.insert(
        env_model::KIMI_CODE_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("kimi".into()),
    );
    role.env.insert(
        env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("zai".into()),
    );

    clear_role_auth_layer(&mut role, AuthKind::Kimi);
    clear_role_auth_layer(&mut role, AuthKind::Zai);

    assert!(role.kimi.is_none());
    assert!(!role.env.contains_key(env_model::KIMI_CODE_API_KEY_ENV_NAME));
    assert!(!role.env.contains_key(env_model::ZAI_API_KEY_ENV_NAME));
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
        Some(env_model::ANTHROPIC_API_KEY_ENV_NAME),
        Some(EnvValue::Plain("key".into())),
        &mut github_env,
        &mut agent_env,
    );

    assert_eq!(
        github_env.get("GH_TOKEN"),
        Some(&EnvValue::Plain("token".into()))
    );
    assert_eq!(
        agent_env.get(env_model::ANTHROPIC_API_KEY_ENV_NAME),
        Some(&EnvValue::Plain("key".into()))
    );
}

#[test]
fn clear_settings_auth_env_values_removes_kind_credentials() {
    let mut github_env = BTreeMap::new();
    let mut agent_env = BTreeMap::new();
    github_env.insert("GH_TOKEN".to_owned(), EnvValue::Plain("token".into()));
    agent_env.insert(
        env_model::ANTHROPIC_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("key".into()),
    );

    clear_settings_auth_env_values(AuthKind::Github, &mut github_env, &mut agent_env);

    assert!(!github_env.contains_key("GH_TOKEN"));
    assert!(agent_env.contains_key(env_model::ANTHROPIC_API_KEY_ENV_NAME));
}

#[test]
fn env_display_map_without_auth_credentials_hides_known_secret_keys() {
    let mut values = BTreeMap::new();
    values.insert("GH_TOKEN".to_owned(), EnvValue::Plain("token".into()));
    values.insert(
        env_model::ANTHROPIC_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("secret".into()),
    );
    values.insert("PROJECT_ENV".to_owned(), EnvValue::Plain("visible".into()));

    let display = env_display_map_without_auth_credentials(&values);

    assert_eq!(display.len(), 1);
    assert_eq!(display.get("PROJECT_ENV"), Some(&"visible".to_owned()));
    assert!(!display.contains_key("GH_TOKEN"));
    assert!(!display.contains_key(env_model::ANTHROPIC_API_KEY_ENV_NAME));
}

#[test]
fn auth_credential_env_keys_includes_settings_mode_credentials() {
    let keys = auth_credential_env_keys();

    assert!(keys.contains("GH_TOKEN"));
    assert!(keys.contains(env_model::ANTHROPIC_API_KEY_ENV_NAME));
    assert!(keys.contains(env_model::ZAI_API_KEY_ENV_NAME));
    assert!(keys.contains(env_model::MINIMAX_API_KEY_ENV_NAME));
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

#[test]
fn settings_auth_rows_from_app_config_reads_global_modes_and_sources() {
    let cfg = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            sync_source_dir: Some(PathBuf::from("/global/claude")),
        }),
        env: BTreeMap::from([(
            env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
            EnvValue::Plain("zai".into()),
        )]),
        ..Default::default()
    };

    let rows = settings_auth_rows_from_app_config(&cfg);
    let claude = rows
        .iter()
        .find(|row| row.kind == AuthKind::Claude)
        .expect("Claude settings row");
    let zai = rows
        .iter()
        .find(|row| row.kind == AuthKind::Zai)
        .expect("Z.AI settings row");

    assert_eq!(claude.mode, AuthMode::ApiKey);
    assert_eq!(
        claude.sync_source_dir,
        Some(PathBuf::from("/global/claude"))
    );
    assert_eq!(zai.mode, AuthMode::ApiKey);
    assert_eq!(zai.sync_source_dir, None);
}

#[test]
fn editor_source_folder_display_marks_inherited_and_default_paths() {
    let mut cfg = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/global/claude")),
        }),
        ..Default::default()
    };
    cfg.workspaces.insert(
        "proj".into(),
        WorkspaceConfig {
            roles: [("smith".into(), WorkspaceRoleOverride::default())].into(),
            ..Default::default()
        },
    );

    let workspace = editor_source_folder_display(&cfg, "proj", "", AuthKind::Claude);
    assert_eq!(workspace.kind, AuthSourceFolderKind::Inherited);
    assert_eq!(workspace.path, "/global/claude");

    let role = editor_source_folder_display(&cfg, "proj", "smith", AuthKind::Claude);
    assert_eq!(role.kind, AuthSourceFolderKind::Inherited);
    assert_eq!(role.path, "/global/claude");

    cfg.claude = None;
    let default = editor_source_folder_display(&cfg, "proj", "", AuthKind::Claude);
    assert_eq!(default.kind, AuthSourceFolderKind::Default);
    assert_eq!(
        default.path,
        format!("~/{}", Agent::Claude.runtime().state_paths().credential_dir)
    );
}

#[test]
fn editor_source_folder_display_prefers_explicit_role_path() {
    let mut cfg = AppConfig::default();
    let mut workspace = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/workspace/claude")),
        }),
        ..Default::default()
    };
    workspace.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Sync,
                sync_source_dir: Some(PathBuf::from("/role/claude")),
            }),
            ..Default::default()
        },
    );
    cfg.workspaces.insert("proj".into(), workspace);

    let display = editor_source_folder_display(&cfg, "proj", "smith", AuthKind::Claude);

    assert_eq!(display.kind, AuthSourceFolderKind::Explicit);
    assert_eq!(display.path, "/role/claude");
}
