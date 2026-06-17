//! Tests for `domain`.
use super::*;
use crate::config::RoleSource;
use crate::isolation::MountIsolation;
use crate::workspace::WorkspaceConfig;
use jackin_console::tui::auth::AuthKind;

#[test]
fn validate_auth_source_folder_covers_agents_and_skips_non_agents() {
    let temp = tempfile::tempdir().unwrap();

    assert!(validate_auth_source_folder(None, temp.path()).is_ok());
    assert!(validate_auth_source_folder(Some(AuthKind::Github), temp.path()).is_ok());

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

    let codex = temp.path().join("codex-good");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("auth.json"), "{\"token\":\"x\"}").unwrap();
    assert!(validate_auth_source_folder(Some(AuthKind::Codex), &codex).is_ok());
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
