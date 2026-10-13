// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn hardline_restore_candidate_errors_when_docker_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: container,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: container.to_owned(),
            dind_container: Some(format!("{container}-dind")),
            network: format!("{container}-net"),
            certs_volume: Some(format!("{container}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    manifest.mark_status(instance::InstanceStatus::Crashed);
    manifest.write(&paths.data_dir.join(container)).unwrap();
    // inspect returns InspectUnavailable → Docker is unavailable error
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
            runtime::ContainerState::InspectUnavailable(
                "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
            ),
        ])),
        ..Default::default()
    };

    let error = restore_candidate_for_hardline(&paths, container, &docker)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("Docker is unavailable"));
}

#[test]
fn workspace_show_includes_isolation_column() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_src = temp.path().join("x");
    let cache_src = temp.path().join("cache");
    std::fs::create_dir_all(&worktree_src).unwrap();
    std::fs::create_dir_all(&cache_src).unwrap();
    let ws = crate::workspace::WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/jackin".into(),
        mounts: vec![
            crate::workspace::MountConfig {
                src: worktree_src.display().to_string(),
                dst: "/workspace/jackin".into(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Worktree,
            },
            crate::workspace::MountConfig {
                src: cache_src.display().to_string(),
                dst: "/workspace/cache".into(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            },
        ],
        allowed_roles: vec![],
        default_role: None,
        default_agent: None,
        last_role: None,
        env: std::collections::BTreeMap::new(),
        roles: std::collections::BTreeMap::new(),
        keep_awake: crate::workspace::KeepAwakeConfig::default(),
        accounts: Vec::new(),
        account_bindings: std::collections::BTreeMap::new(),
        github: None,
        git_pull_on_entry: false,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
        default_launch: None,
    };
    let out = render_workspace_show(&AppConfig::default(), "jackin", &ws);
    assert!(out.contains("Isolation"));
    assert!(out.contains("Type"));
    assert!(out.contains("folder"));
    assert!(out.contains("worktree"));
    assert!(out.contains("shared"));
}

#[test]
fn workspace_show_splits_workspace_and_global_mount_groups() {
    let temp = tempfile::tempdir().unwrap();
    let global_src = temp.path().join("gradle");
    std::fs::create_dir_all(&global_src).unwrap();
    let work_src = temp.path().join("work");
    std::fs::create_dir_all(&work_src).unwrap();
    let mut config = AppConfig::default();
    config
        .roles
        .insert("agent-smith".into(), jackin_config::RoleSource::default());
    config.add_mount(
        "gradle-cache",
        crate::workspace::MountConfig {
            src: global_src.display().to_string(),
            dst: "/home/agent/.gradle/caches".into(),
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        },
        None,
    );
    let ws = crate::workspace::WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/jackin".into(),
        mounts: vec![crate::workspace::MountConfig {
            src: work_src.display().to_string(),
            dst: "/workspace/jackin".into(),
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        allowed_roles: vec!["agent-smith".into()],
        ..Default::default()
    };

    let out = render_workspace_show(&config, "jackin", &ws);

    assert!(out.contains("Workspace mounts:"), "{out}");
    assert!(out.contains("Global mounts:"), "{out}");
    assert!(!out.contains("Global mounts (agent-smith):"), "{out}");
    assert!(out.contains("gradle-cache"), "{out}");
    assert!(!out.contains("│ Scope"), "{out}");
}

#[test]
fn workspace_show_explains_ambiguous_role_scoped_global_mounts() {
    let temp = tempfile::tempdir().unwrap();
    let global_src = temp.path().join("secrets");
    std::fs::create_dir_all(&global_src).unwrap();
    let mut config = AppConfig::default();
    config
        .roles
        .insert("alpha".into(), jackin_config::RoleSource::default());
    config
        .roles
        .insert("beta".into(), jackin_config::RoleSource::default());
    config.add_mount(
        "team-secrets",
        crate::workspace::MountConfig {
            src: global_src.display().to_string(),
            dst: "/secrets".into(),
            readonly: true,
            isolation: jackin_core::MountIsolation::Shared,
        },
        Some("alpha"),
    );
    let ws = crate::workspace::WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/jackin".into(),
        mounts: vec![],
        allowed_roles: vec!["alpha".into(), "beta".into()],
        ..Default::default()
    };

    let out = render_workspace_show(&config, "jackin", &ws);

    assert!(out.contains("selected role"), "{out}");
    assert!(!out.contains("team-secrets"), "{out}");
}

#[test]
fn workspace_show_keeps_scope_column_for_scoped_global_mounts() {
    let temp = tempfile::tempdir().unwrap();
    let global_src = temp.path().join("secrets");
    std::fs::create_dir_all(&global_src).unwrap();
    let mut config = AppConfig::default();
    config.roles.insert(
        "chainargos/agent-brown".into(),
        jackin_config::RoleSource::default(),
    );
    config.add_mount(
        "team-secrets",
        crate::workspace::MountConfig {
            src: global_src.display().to_string(),
            dst: "/secrets".into(),
            readonly: true,
            isolation: jackin_core::MountIsolation::Shared,
        },
        Some("chainargos/*"),
    );
    let ws = crate::workspace::WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/jackin".into(),
        mounts: vec![],
        allowed_roles: vec!["chainargos/agent-brown".into()],
        ..Default::default()
    };

    let out = render_workspace_show(&config, "jackin", &ws);

    assert!(
        out.contains("Global mounts (chainargos/agent-brown):"),
        "{out}"
    );
    assert!(out.contains("│ Scope"), "{out}");
    assert!(out.contains("chainargos/*"), "{out}");
}

#[tokio::test]
async fn resolve_role_no_match_errors() {
    let selector = RoleSelector::new(None, "agent-smith");
    // list_containers returns empty → no match
    let docker = jackin_test_support::FakeDockerClient::default();
    let err = resolve_role_to_container(&selector, &docker)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("no managed container found"),
        "{err}"
    );
}

#[tokio::test]
async fn resolve_role_multiple_matches_errors_with_names() {
    let selector = RoleSelector::new(None, "agent-smith");
    // list_containers returns two containers → multiple match error
    let docker = jackin_test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            jackin_docker::docker_client::ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            },
            jackin_docker::docker_client::ContainerRow {
                name: "jk-a1b2c3d4-agentsmith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let err = resolve_role_to_container(&selector, &docker)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("multiple containers found"), "{msg}");
    assert!(msg.contains("jk-k7p9m2xq-agentsmith"), "{msg}");
    assert!(msg.contains("jk-a1b2c3d4-agentsmith"), "{msg}");
}

#[tokio::test]
async fn resolve_role_single_match_returns_name() {
    let selector = RoleSelector::new(None, "agent-smith");
    // list_containers returns one container → single match
    let docker = jackin_test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            jackin_docker::docker_client::ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let name = resolve_role_to_container(&selector, &docker).await.unwrap();
    assert_eq!(name, "jk-k7p9m2xq-agentsmith");
}
