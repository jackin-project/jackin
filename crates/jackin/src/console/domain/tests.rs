//! Tests for `domain`.
use super::*;
use crate::config::{AuthForwardMode, GithubAuthConfig, GithubAuthMode, WorkspaceRoleOverride};
use jackin_console::tui::auth::AuthKind;

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
fn validate_auth_source_folder_covers_agents_and_skips_non_agents() {
    let temp = tempfile::tempdir().unwrap();

    // Non-agent kind (Github) and None: nothing to validate → Ok.
    assert!(validate_auth_source_folder(None, temp.path()).is_ok());
    assert!(validate_auth_source_folder(Some(AuthKind::Github), temp.path()).is_ok());

    // Every sync-capable agent rejects a folder lacking its credentials.
    for kind in [
        AuthKind::Claude,
        AuthKind::Codex,
        AuthKind::Amp,
        AuthKind::Kimi,
        AuthKind::Opencode,
        AuthKind::Grok,
    ] {
        let dir = temp.path().join(format!("{kind:?}-empty"));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            validate_auth_source_folder(Some(kind), &dir).is_err(),
            "{kind:?}: empty folder must be rejected"
        );
    }

    // A valid Codex folder is accepted.
    let codex = temp.path().join("codex-good");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("auth.json"), "{\"token\":\"x\"}").unwrap();
    assert!(validate_auth_source_folder(Some(AuthKind::Codex), &codex).is_ok());
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
    existing.env.insert(
        "GH_TOKEN".to_owned(),
        crate::operator_env::EnvValue::Plain("token".into()),
    );

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
        Some(crate::operator_env::EnvValue::Plain("token".into())),
    );

    let github = ws.github.expect("github auth should be stored");
    assert_eq!(github.auth_forward, GithubAuthMode::Token);
    assert_eq!(
        github.env.get("GH_TOKEN"),
        Some(&crate::operator_env::EnvValue::Plain("token".into()))
    );
    assert!(ws.env.is_empty());
}

#[test]
fn apply_role_auth_commit_updates_mode_and_zai_ignore_removes_key() {
    let mut role = WorkspaceRoleOverride::default();
    role.env.insert(
        crate::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        crate::operator_env::EnvValue::Plain("stale".into()),
    );

    apply_role_auth_commit(&mut role, AuthKind::Zai, AuthMode::Ignore, None, None);

    assert!(
        !role
            .env
            .contains_key(crate::env_model::ZAI_API_KEY_ENV_NAME)
    );
}

#[test]
fn clear_workspace_auth_layer_removes_github_block() {
    let mut ws = WorkspaceConfig::default();
    apply_workspace_auth_commit(
        &mut ws,
        AuthKind::Github,
        AuthMode::Token,
        Some("GH_TOKEN"),
        Some(crate::operator_env::EnvValue::Plain("token".into())),
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
        Some(crate::operator_env::EnvValue::Plain("token".into())),
        &mut github_env,
        &mut agent_env,
    );
    apply_settings_auth_env_commit(
        AuthKind::Claude,
        Some("ANTHROPIC_API_KEY"),
        Some(crate::operator_env::EnvValue::Plain("key".into())),
        &mut github_env,
        &mut agent_env,
    );

    assert_eq!(
        github_env.get("GH_TOKEN"),
        Some(&crate::operator_env::EnvValue::Plain("token".into()))
    );
    assert_eq!(
        agent_env.get("ANTHROPIC_API_KEY"),
        Some(&crate::operator_env::EnvValue::Plain("key".into()))
    );
}

#[test]
fn clear_settings_auth_env_values_removes_kind_credentials() {
    let mut github_env = BTreeMap::new();
    let mut agent_env = BTreeMap::new();
    github_env.insert(
        "GH_TOKEN".to_owned(),
        crate::operator_env::EnvValue::Plain("token".into()),
    );
    agent_env.insert(
        "ANTHROPIC_API_KEY".to_owned(),
        crate::operator_env::EnvValue::Plain("key".into()),
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
    github.env.insert(
        "GH_TOKEN".to_owned(),
        crate::operator_env::EnvValue::Plain("token".into()),
    );
    cfg.github = Some(github.clone());

    assert_eq!(app_github_env(&cfg), github.env);
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
fn eligible_role_keys_for_override_uses_allowed_or_all_roles() {
    let mut cfg = AppConfig::default();
    cfg.roles.insert("alpha".into(), RoleSource::default());
    cfg.roles.insert("beta".into(), RoleSource::default());

    let mut workspace = WorkspaceConfig::default();
    let mut eligible = eligible_role_keys_for_override(&cfg, &workspace);
    eligible.sort();
    assert_eq!(eligible, vec!["alpha".to_owned(), "beta".to_owned()]);

    workspace.allowed_roles = vec!["ghost".into()];
    assert_eq!(
        eligible_role_keys_for_override(&cfg, &workspace),
        vec!["ghost".to_owned()]
    );
}

#[test]
fn settings_auth_env_value_uses_github_or_agent_env() {
    let mut github_env = BTreeMap::new();
    github_env.insert(
        "GH_TOKEN".into(),
        crate::operator_env::EnvValue::Plain("github-token".into()),
    );
    let mut agent_env = BTreeMap::new();
    agent_env.insert(
        AuthKind::Claude
            .required_env_var(AuthMode::ApiKey)
            .expect("Claude API key env var")
            .into(),
        crate::operator_env::EnvValue::Plain("anthropic-key".into()),
    );

    assert!(matches!(
        settings_auth_env_value(AuthKind::Github, AuthMode::Token, &github_env, &agent_env),
        Some(crate::operator_env::EnvValue::Plain(value)) if value == "github-token"
    ));
    assert!(matches!(
        settings_auth_env_value(AuthKind::Claude, AuthMode::ApiKey, &github_env, &agent_env),
        Some(crate::operator_env::EnvValue::Plain(value)) if value == "anthropic-key"
    ));
    assert!(
        settings_auth_env_value(AuthKind::Claude, AuthMode::Sync, &github_env, &agent_env)
            .is_none()
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
        crate::operator_env::EnvValue::Plain("anthropic-key".into()),
    );
    workspace.github.as_mut().expect("github").env.insert(
        "GH_TOKEN".into(),
        crate::operator_env::EnvValue::Plain("github-token".into()),
    );

    assert!(matches!(
        workspace_auth_mode_and_credential(&workspace, AuthKind::Claude),
        (Some(AuthMode::ApiKey), Some(crate::operator_env::EnvValue::Plain(value)))
            if value == "anthropic-key"
    ));
    assert!(matches!(
        workspace_auth_mode_and_credential(&workspace, AuthKind::Github),
        (Some(AuthMode::Token), Some(crate::operator_env::EnvValue::Plain(value)))
            if value == "github-token"
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
    role.github.as_mut().expect("github").env.insert(
        "GH_TOKEN".into(),
        crate::operator_env::EnvValue::Plain("github-token".into()),
    );

    assert!(matches!(
        role_auth_mode_and_credential(Some(&role), AuthKind::Github),
        (Some(AuthMode::Token), Some(crate::operator_env::EnvValue::Plain(value)))
            if value == "github-token"
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
    cfg.env.insert(
        env_name.into(),
        crate::operator_env::EnvValue::Plain("global".into()),
    );
    let mut workspace = WorkspaceConfig::default();
    workspace.env.insert(
        env_name.into(),
        crate::operator_env::EnvValue::Plain("workspace".into()),
    );
    let mut role = WorkspaceRoleOverride::default();
    role.env.insert(
        env_name.into(),
        crate::operator_env::EnvValue::Plain("workspace-role".into()),
    );
    workspace.roles.insert("smith".into(), role);
    cfg.workspaces.insert("ws".into(), workspace);

    assert!(matches!(
        panel_auth_source_value(&cfg, "ws", "smith", env_name, AuthKind::Claude),
        Some(crate::operator_env::EnvValue::Plain(value)) if value == "workspace-role"
    ));
    assert!(matches!(
        panel_auth_source_value(&cfg, "ws", "", env_name, AuthKind::Claude),
        Some(crate::operator_env::EnvValue::Plain(value)) if value == "workspace"
    ));
    assert!(matches!(
        panel_auth_source_value(&cfg, "missing", "", env_name, AuthKind::Claude),
        Some(crate::operator_env::EnvValue::Plain(value)) if value == "global"
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
    cfg.github.as_mut().expect("github").env.insert(
        "GH_TOKEN".into(),
        crate::operator_env::EnvValue::Plain("global-gh".into()),
    );
    let mut workspace = WorkspaceConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            ..Default::default()
        }),
        ..Default::default()
    };
    workspace.github.as_mut().expect("github").env.insert(
        "GH_TOKEN".into(),
        crate::operator_env::EnvValue::Plain("workspace-gh".into()),
    );
    cfg.workspaces.insert("ws".into(), workspace);

    assert!(matches!(
        panel_auth_source_value(&cfg, "ws", "", "GH_TOKEN", AuthKind::Github),
        Some(crate::operator_env::EnvValue::Plain(value)) if value == "workspace-gh"
    ));
    assert!(matches!(
        panel_auth_source_value(&cfg, "missing", "", "GH_TOKEN", AuthKind::Github),
        Some(crate::operator_env::EnvValue::Plain(value)) if value == "global-gh"
    ));
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
        crate::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        crate::operator_env::EnvValue::Plain("k".into()),
    );
    assert!(role_override_present(AuthKind::Zai, &ro));
    assert!(!role_override_present(AuthKind::Claude, &ro));
    assert!(!role_override_present(AuthKind::Github, &ro));
}

#[test]
fn build_workspace_choice_returns_none_for_unknown_saved_name() {
    let config = AppConfig::default();
    let cwd = std::env::temp_dir();
    let result =
        build_workspace_choice(&config, &cwd, &LoadWorkspaceInput::Saved("ghost".into())).unwrap();
    assert!(
        result.is_none(),
        "Saved(name) for an absent workspace must return None, not fabricate a choice"
    );
}

#[test]
fn build_workspace_choice_picks_up_default_agent_from_config() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().canonicalize().unwrap();
    let workdir = project_dir.display().to_string();
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "ws".to_owned(),
        WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: workdir.clone(),
            mounts: vec![MountConfig {
                src: workdir.clone(),
                dst: workdir,
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: Some("agent-smith".to_owned()),
            default_agent: None,
            last_role: None,
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
            git_pull_on_entry: false,
            docker: None,
        },
    );

    let choice = build_workspace_choice(
        &config,
        &project_dir,
        &LoadWorkspaceInput::Saved("ws".into()),
    )
    .unwrap()
    .expect("present saved workspace must resolve");
    assert_eq!(choice.default_role.as_deref(), Some("agent-smith"));
    assert_eq!(choice.allowed_roles.len(), 1);
}

// ── role-eligibility composition ───────────────────────────────

fn agent_source_stub() -> RoleSource {
    RoleSource {
        git: "https://example.invalid/org/repo.git".to_owned(),
        trusted: true,
        env: BTreeMap::new(),
    }
}

fn workspace_with_allowed(allowed: &[&str]) -> WorkspaceConfig {
    WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/work".to_owned(),
        mounts: vec![],
        allowed_roles: allowed.iter().map(|s| (*s).to_owned()).collect(),
        default_role: None,
        default_agent: None,
        last_role: None,
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
        keep_awake: crate::workspace::KeepAwakeConfig::default(),
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
        docker: None,
    }
}

fn launch_workspace(workdir: &std::path::Path, allowed_roles: Vec<&str>) -> WorkspaceConfig {
    WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: workdir.display().to_string(),
        mounts: vec![MountConfig {
            src: workdir.display().to_string(),
            dst: workdir.display().to_string(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        allowed_roles: allowed_roles.into_iter().map(str::to_owned).collect(),
        default_role: None,
        default_agent: None,
        last_role: None,
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
        keep_awake: crate::workspace::KeepAwakeConfig::default(),
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
        docker: None,
    }
}

#[test]
fn resolve_launch_dispatch_returns_none_for_deleted_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();

    let resolution = resolve_launch_dispatch(
        &config,
        temp.path(),
        LoadWorkspaceInput::Saved("missing".to_owned()),
    )
    .unwrap();

    assert!(resolution.is_none());
}

#[test]
fn resolve_launch_dispatch_reports_no_eligible_roles() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "empty".to_owned(),
        launch_workspace(temp.path(), Vec::new()),
    );

    let resolution = resolve_launch_dispatch(
        &config,
        temp.path(),
        LoadWorkspaceInput::Saved("empty".to_owned()),
    )
    .unwrap()
    .expect("workspace exists");

    assert!(matches!(
        resolution,
        LaunchDispatchResolution::NoEligibleRoles { name } if name == "empty"
    ));
}

#[test]
fn resolve_launch_dispatch_resolves_single_role_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.roles.insert("smith".to_owned(), agent_source_stub());
    config.workspaces.insert(
        "solo".to_owned(),
        launch_workspace(temp.path(), vec!["smith"]),
    );

    let resolution = resolve_launch_dispatch(
        &config,
        temp.path(),
        LoadWorkspaceInput::Saved("solo".to_owned()),
    )
    .unwrap()
    .expect("workspace exists");

    let LaunchDispatchResolution::SingleRole { role, workspace } = resolution else {
        panic!("expected single-role launch dispatch");
    };
    assert_eq!(role.key(), "smith");
    assert_eq!(workspace.label, "solo");
}

#[test]
fn resolve_launch_dispatch_preselects_role_picker() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.roles.insert("alpha".to_owned(), agent_source_stub());
    config.roles.insert("beta".to_owned(), agent_source_stub());
    let mut saved = launch_workspace(temp.path(), vec!["alpha", "beta"]);
    saved.last_role = Some("beta".to_owned());
    config.workspaces.insert("multi".to_owned(), saved);

    let resolution = resolve_launch_dispatch(
        &config,
        temp.path(),
        LoadWorkspaceInput::Saved("multi".to_owned()),
    )
    .unwrap()
    .expect("workspace exists");

    let LaunchDispatchResolution::RolePicker {
        roles, selected, ..
    } = resolution
    else {
        panic!("expected role picker dispatch");
    };
    assert_eq!(
        roles.iter().map(RoleSelector::key).collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(selected, Some(1));
}

#[test]
fn eligible_agents_returns_all_configured_when_allowed_list_empty() {
    let mut config = AppConfig::default();
    config.roles.insert("alice".to_owned(), agent_source_stub());
    config.roles.insert("bob".to_owned(), agent_source_stub());

    let ws = workspace_with_allowed(&[]);
    let eligible = eligible_roles_for_workspace(&config, &ws);
    let keys: Vec<String> = eligible.iter().map(RoleSelector::key).collect();

    assert_eq!(eligible.len(), 2, "empty allowed_roles must mean 'any'");
    assert!(keys.contains(&"alice".to_owned()));
    assert!(keys.contains(&"bob".to_owned()));
}

#[test]
fn eligible_agents_narrows_to_allowed_list_when_non_empty() {
    let mut config = AppConfig::default();
    config.roles.insert("alice".to_owned(), agent_source_stub());
    config.roles.insert("bob".to_owned(), agent_source_stub());
    config.roles.insert("carol".to_owned(), agent_source_stub());

    let ws = workspace_with_allowed(&["alice", "carol"]);
    let eligible = eligible_roles_for_workspace(&config, &ws);
    let keys: Vec<String> = eligible.iter().map(RoleSelector::key).collect();

    assert_eq!(eligible.len(), 2);
    assert!(keys.contains(&"alice".to_owned()));
    assert!(keys.contains(&"carol".to_owned()));
    assert!(!keys.contains(&"bob".to_owned()));
}

#[test]
fn eligible_agents_drops_ghost_name_not_in_config() {
    let mut config = AppConfig::default();
    config.roles.insert("alice".to_owned(), agent_source_stub());

    let ws = workspace_with_allowed(&["ghost"]);
    let eligible = eligible_roles_for_workspace(&config, &ws);

    assert!(
        eligible.is_empty(),
        "eligibility must not resurrect a name absent from config.roles"
    );
}
