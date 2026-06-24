use std::collections::BTreeMap;

use super::*;
use jackin_config::{
    AppConfig, CURRENT_WORKSPACE_VERSION, KeepAwakeConfig, MountConfig, MountIsolation, RoleSource,
    WorkspaceConfig,
};

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
            version: CURRENT_WORKSPACE_VERSION.to_owned(),
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
            keep_awake: KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
            git_pull_on_entry: false,
            dirty_exit_policy: None,
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

fn agent_source_stub() -> RoleSource {
    RoleSource {
        git: "https://example.invalid/org/repo.git".to_owned(),
        trusted: true,
        env: BTreeMap::new(),
    }
}

fn launch_workspace(workdir: &std::path::Path, allowed_roles: Vec<&str>) -> WorkspaceConfig {
    WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
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
        keep_awake: KeepAwakeConfig::default(),
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
        dirty_exit_policy: None,
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
fn providers_for_launch_include_all_zai_env_layers() {
    use jackin_config::WorkspaceRoleOverride;
    use jackin_core::{Agent, EnvValue};

    let mut config = AppConfig::default();
    config
        .env
        .insert("ZAI_API_KEY".into(), EnvValue::Plain("global-key".into()));
    config
        .workspaces
        .insert("global-demo".into(), WorkspaceConfig::default());
    assert_eq!(
        providers_for_launch(&config, "global-demo", "the-architect", Agent::Claude).len(),
        2
    );
    config.env.clear();

    let mut workspace = WorkspaceConfig::default();
    workspace.env.insert(
        "ZAI_API_KEY".into(),
        EnvValue::Plain("workspace-key".into()),
    );
    config.workspaces.insert("workspace-demo".into(), workspace);
    assert_eq!(
        providers_for_launch(&config, "workspace-demo", "the-architect", Agent::Claude).len(),
        2
    );

    config.workspaces.remove("workspace-demo");
    let mut role = RoleSource::default();
    role.env
        .insert("ZAI_API_KEY".into(), EnvValue::Plain("role-key".into()));
    config.roles.insert("the-architect".into(), role);
    config
        .workspaces
        .insert("role-demo".into(), WorkspaceConfig::default());
    assert_eq!(
        providers_for_launch(&config, "role-demo", "the-architect", Agent::Claude).len(),
        2
    );

    config.roles.clear();
    let mut workspace_role = WorkspaceConfig::default();
    let mut role_override = WorkspaceRoleOverride::default();
    role_override.env.insert(
        "ZAI_API_KEY".into(),
        EnvValue::Plain("workspace-role-key".into()),
    );
    workspace_role
        .roles
        .insert("the-architect".into(), role_override);
    config
        .workspaces
        .insert("workspace-role-demo".into(), workspace_role);
    let providers = providers_for_launch(
        &config,
        "workspace-role-demo",
        "the-architect",
        Agent::Claude,
    );
    assert_eq!(providers.len(), 2);
    assert_eq!(providers[1], jackin_protocol::Provider::Zai);
}

#[test]
fn providers_for_launch_rejects_non_claude_agents() {
    use jackin_core::{Agent, EnvValue};

    let mut config = AppConfig::default();
    config
        .env
        .insert("ZAI_API_KEY".into(), EnvValue::Plain("global-key".into()));
    config
        .workspaces
        .insert("demo".into(), WorkspaceConfig::default());

    let providers = providers_for_launch(&config, "demo", "the-architect", Agent::Codex);
    assert!(providers.is_empty());
}
