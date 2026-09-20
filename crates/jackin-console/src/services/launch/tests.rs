// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use super::*;
use jackin_core::{Agent, WorkspaceName};
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
use jackin_config::{
    AccountCredential, AgentConfiguration, AiProvider, AppConfig, CURRENT_WORKSPACE_VERSION,
    KeepAwakeConfig, MountConfig, MountIsolation, RoleSource, WorkspaceConfig,
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
            accounts: Vec::new(),
            account_bindings: BTreeMap::new(),
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
            dirty_exit_policy: None,
            docker: None,
            default_launch: None,
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
        accounts: Vec::new(),
        account_bindings: BTreeMap::new(),
        github: None,
        git_pull_on_entry: false,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
        default_launch: None,
    }
}

#[test]
fn build_workspace_choice_ignores_missing_global_mount_sources() {
    // Global mounts merge and heal in `resolve_load_workspace`, not here:
    // workspace selection must not fail on a wiped cache.
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().canonicalize().unwrap();
    let mut config = AppConfig::default();
    config
        .roles
        .insert("agent-smith".to_owned(), agent_source_stub());
    config.workspaces.insert(
        "ws".to_owned(),
        launch_workspace(&project_dir, vec!["agent-smith"]),
    );
    config.add_mount(
        "cargo-git",
        MountConfig {
            src: temp.path().join("cache-wiped").display().to_string(),
            dst: "/home/agent/.cargo/git".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
        None,
    );

    let choice = build_workspace_choice(
        &config,
        &project_dir,
        &LoadWorkspaceInput::Saved("ws".into()),
    )
    .unwrap()
    .expect("present saved workspace must resolve");

    assert_eq!(choice.name, "ws");
    assert_eq!(choice.allowed_roles.len(), 1);
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

fn api_key_account(name: &str, provider: AiProvider) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: "test-key".into(),
            base_url: None,
            model: None,
        },
    }
}

fn agent_configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.into(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

/// Saved `demo` workspace (workdir-backed) with two Claude accounts
/// allowlisted, one Claude configuration per account, and no defaults.
fn admission_config(project_dir: &std::path::Path) -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert("smith".to_owned(), agent_source_stub());
    for (id, display) in [("a-claude", "A"), ("z-claude", "Z")] {
        config
            .accounts
            .insert(id.into(), api_key_account(display, AiProvider::Anthropic));
    }
    for (id, account) in [("claude-a", "a-claude"), ("claude-z", "z-claude")] {
        config
            .agent_configurations
            .insert(id.into(), agent_configuration(Agent::Claude, account));
    }
    let mut saved = launch_workspace(project_dir, vec!["smith"]);
    saved.accounts = vec!["a-claude".into(), "z-claude".into()];
    config.workspaces.insert("demo".to_owned(), saved);
    config
}

#[test]
fn admitted_account_choices_defer_to_bindings_without_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let config = admission_config(temp.path());

    let admitted =
        admitted_account_choices(&config, Some(&wn("demo")), "smith", Agent::Claude).unwrap();
    assert!(
        admitted.is_none(),
        "no default anywhere must defer to the legacy bindings path"
    );
}

#[test]
fn admitted_account_choices_resolve_role_default_for_agent() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = admission_config(temp.path());
    config
        .workspaces
        .get_mut("demo")
        .unwrap()
        .roles
        .entry("smith".into())
        .or_default()
        .default_launch = Some(vec!["claude-z".into()]);

    let admitted = admitted_account_choices(&config, Some(&wn("demo")), "smith", Agent::Claude)
        .unwrap()
        .expect("a configured default must admit");
    assert_eq!(
        admitted
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["z-claude"]
    );
    assert_eq!(admitted.first().expect("one admitted row").name, "Z");

    // The same default admits nothing for an agent with no instance in it.
    let admitted = admitted_account_choices(&config, Some(&wn("demo")), "smith", Agent::Codex)
        .unwrap()
        .expect("resolution succeeds; the admitted set is just empty for Codex");
    assert!(admitted.is_empty());
}

#[test]
fn admitted_account_choices_fail_atomically_on_invalid_default() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = admission_config(temp.path());
    config.workspaces.get_mut("demo").unwrap().default_launch = Some(vec!["ghost".into()]);

    let error =
        admitted_account_choices(&config, Some(&wn("demo")), "smith", Agent::Claude).unwrap_err();
    assert!(
        error.to_string().contains("unknown agent configuration"),
        "got {error:?}"
    );
}

#[test]
fn account_choices_for_instances_preserve_configuration_identity_and_sort() {
    let temp = tempfile::tempdir().unwrap();
    let config = admission_config(temp.path());
    let instances = resolve_launch(
        &config,
        Some(&wn("demo")),
        "smith",
        Some(&["claude-z".to_owned(), "claude-a".to_owned()]),
        None,
    )
    .unwrap();

    let rows = account_choices_for_instances(&config, &instances);
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["a-claude", "z-claude"]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.configuration_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("claude-a"), Some("claude-z")]
    );
    assert!(
        rows.iter().all(|row| row.agents.contains(&Agent::Claude)),
        "admitted rows keep full compatibility info"
    );

    // Instances naming an unregistered account are skipped, never fabricated.
    let mut foreign = instances;
    foreign.push(ResolvedInstance {
        config_id: "foreign".into(),
        agent: Agent::Claude,
        account_id: "ghost".into(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: "Ghost".into(),
        synthesized: true,
    });
    let rows = account_choices_for_instances(&config, &foreign);
    assert_eq!(rows.len(), 2);
}

#[test]
fn live_account_choices_preserve_duplicate_agent_instances_and_ids() {
    let temp = tempfile::tempdir().unwrap();
    let config = admission_config(temp.path());
    let rows = account_choices_for_live_instances(
        &config,
        &[
            LiveInstanceAdmission {
                instance_id: "claude-z".into(),
                agent: Agent::Claude,
                account_id: "z-claude".into(),
            },
            LiveInstanceAdmission {
                instance_id: "claude-a".into(),
                agent: Agent::Claude,
                account_id: "a-claude".into(),
            },
        ],
    );

    assert_eq!(
        rows.iter()
            .map(|row| row.instance_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("claude-a"), Some("claude-z")]
    );
    assert!(rows.iter().all(|row| row.agents == vec![Agent::Claude]));
    assert!(rows[0].label().contains("instance claude-a"));
}

#[test]
fn resolve_committed_agent_launch_carries_admitted_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().canonicalize().unwrap();
    let mut config = admission_config(&project_dir);
    config.workspaces.get_mut("demo").unwrap().default_launch =
        Some(vec!["claude-z".into(), "claude-a".into()]);

    let resolved = resolve_committed_agent_launch(
        &config,
        &project_dir,
        LoadWorkspaceInput::Saved("demo".into()),
        RoleSelector::parse("smith").unwrap(),
        Agent::Claude,
    )
    .unwrap()
    .expect("present saved workspace must resolve");
    assert_eq!(
        resolved
            .accounts
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a-claude", "z-claude"]
    );

    // An invalid default fails the commit instead of returning the
    // eligible list.
    config.workspaces.get_mut("demo").unwrap().default_launch = Some(vec!["ghost".into()]);
    resolve_committed_agent_launch(
        &config,
        &project_dir,
        LoadWorkspaceInput::Saved("demo".into()),
        RoleSelector::parse("smith").unwrap(),
        Agent::Claude,
    )
    .unwrap_err();
}

#[test]
fn launch_accounts_require_workspace_assignment_and_agent_support() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};
    use jackin_core::{Agent, EnvValue};
    let mut config = AppConfig::default();
    for id in ["personal", "work"] {
        config.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: id.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::Profile {
                    agent: Agent::Claude,
                    directory: format!("/profiles/{id}").into(),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
    }
    let mut workspace = WorkspaceConfig::default();
    workspace.accounts.push("work".into());
    config.workspaces.insert("demo".into(), workspace);
    config
        .env
        .insert("ZAI_API_KEY".into(), EnvValue::Plain("unregistered".into()));
    let choices = accounts_for_launch(&config, Some(&wn("demo")), Agent::Claude);
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].id, "work");
    assert!(accounts_for_launch(&config, Some(&wn("demo")), Agent::Codex).is_empty());
    assert!(accounts_for_launch(&config, Some(&wn("missing")), Agent::Claude).is_empty());
    assert_eq!(accounts_for_launch(&config, None, Agent::Claude).len(), 2);
    config.accounts.get_mut("work").unwrap().enabled = false;
    assert!(accounts_for_launch(&config, Some(&wn("demo")), Agent::Claude).is_empty());
    assert!(account_choices(&config, Some(&wn("demo"))).is_empty());
}
