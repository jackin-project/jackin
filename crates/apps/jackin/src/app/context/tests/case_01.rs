// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn classify_target_tilde_path() {
    let result = classify_target("~/Projects/my-app");
    assert!(matches!(
        result,
        TargetKind::Path { ref src, .. } if src == "~/Projects/my-app"
    ));
}

#[test]
fn classify_target_tilde_path_with_dst() {
    let result = classify_target("~/Projects/my-app:/app");
    assert!(matches!(
        result,
        TargetKind::Path { ref src, ref dst } if src == "~/Projects/my-app" && dst == "/app"
    ));
}

#[test]
fn classify_target_dot_relative_path() {
    let result = classify_target("./my-app");
    assert!(matches!(result, TargetKind::Path { .. }));
}

#[test]
fn classify_target_absolute_path() {
    let result = classify_target("/tmp/my-app");
    assert!(matches!(
        result,
        TargetKind::Path { ref src, ref dst } if src == "/tmp/my-app" && dst == "/tmp/my-app"
    ));
}

#[test]
fn classify_target_absolute_path_with_dst() {
    let result = classify_target("/tmp/my-app:/workspace");
    assert!(matches!(
        result,
        TargetKind::Path { ref src, ref dst } if src == "/tmp/my-app" && dst == "/workspace"
    ));
}

#[test]
fn classify_target_plain_name() {
    let result = classify_target("big-monorepo");
    assert!(matches!(
        result,
        TargetKind::Name(ref name) if name == "big-monorepo"
    ));
}

#[test]
fn classify_target_name_with_no_slash() {
    let result = classify_target("my-workspace");
    assert!(matches!(result, TargetKind::Name(_)));
}

#[test]
fn classify_target_relative_with_slash() {
    // Contains `/` so treated as path
    let result = classify_target("sub/dir");
    assert!(matches!(result, TargetKind::Path { .. }));
}

#[test]
fn resolve_target_name_workspace_only() {
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "my-ws".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            ..Default::default()
        },
    );
    let cwd = std::env::temp_dir();
    let result = resolve_target_name("my-ws", &config, &cwd).unwrap();
    assert!(matches!(result, LoadWorkspaceInput::Saved(ref name) if name == "my-ws"));
}

#[test]
fn resolve_target_name_directory_only() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("my-dir");
    std::fs::create_dir_all(&dir).unwrap();

    let config = AppConfig::default();
    let result = resolve_target_name("my-dir", &config, temp.path()).unwrap();
    assert!(matches!(result, LoadWorkspaceInput::Path { .. }));
}

#[test]
fn resolve_target_name_neither_errors() {
    let config = AppConfig::default();
    let cwd = std::env::temp_dir();
    let result = resolve_target_name("nonexistent-thing", &config, &cwd);
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("neither a saved workspace nor a directory"));
}

#[test]
fn resolve_agent_from_context_matches_workspace_from_nested_mount_path() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let nested_dir = project_dir.join("src/bin");
    std::fs::create_dir_all(&nested_dir).unwrap();

    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: project_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: Some("agent-smith".to_owned()),
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            accounts: Vec::new(),
            account_bindings: std::collections::BTreeMap::new(),
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
            dirty_exit_policy: None,
            docker: None,
            default_launch: None,
        },
    );

    let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

    assert_eq!(resolved.0.key(), "agent-smith");
    assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_owned()));
}

#[test]
fn resolve_agent_from_context_matches_workspace_from_host_workdir_root() {
    let temp = tempfile::tempdir().unwrap();
    let workspace_root = temp.path().join("monorepo");
    let repo_dir = workspace_root.join("jackin");
    std::fs::create_dir_all(&repo_dir).unwrap();
    let workspace_root = workspace_root.canonicalize().unwrap();

    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: workspace_root.display().to_string(),
            mounts: vec![workspace::MountConfig {
                src: repo_dir.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/jackin".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: Some("agent-smith".to_owned()),
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            accounts: Vec::new(),
            account_bindings: std::collections::BTreeMap::new(),
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
            dirty_exit_policy: None,
            docker: None,
            default_launch: None,
        },
    );

    let resolved = resolve_agent_from_context(&config, &workspace_root).unwrap();

    assert_eq!(resolved.0.key(), "agent-smith");
    assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_owned()));
}

#[test]
fn resolve_agent_from_context_ignores_stale_last_agent() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let nested_dir = project_dir.join("src/bin");
    std::fs::create_dir_all(&nested_dir).unwrap();

    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: project_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: None,
            default_agent: None,
            last_role: Some("ghost-role".to_owned()),
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            accounts: Vec::new(),
            account_bindings: std::collections::BTreeMap::new(),
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
            dirty_exit_policy: None,
            docker: None,
            default_launch: None,
        },
    );

    let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

    assert_eq!(resolved.0.key(), "agent-smith");
    assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_owned()));
}

#[tokio::test]
async fn resolve_running_container_from_context_picks_lone_running_agent() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let nested_dir = project_dir.join("src");
    std::fs::create_dir_all(&nested_dir).unwrap();

    let config = config_with_workspace(&project_dir, vec!["agent-smith".to_owned()], None);
    let running = "jk-k7p9m2xq-agentsmith";
    let docker = fake_docker_with_running_agents(&[running]);

    let paths = JackinPaths::for_tests(temp.path());
    let container = resolve_running_container_from_context(&paths, &config, &nested_dir, &docker)
        .await
        .unwrap();

    assert_eq!(container, running);
}

#[tokio::test]
async fn resolve_running_container_from_context_prefers_last_agent() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config = config_with_workspace(
        &project_dir,
        vec!["agent-smith".to_owned(), "the-architect".to_owned()],
        Some("the-architect".to_owned()),
    );
    let smith = "jk-k7p9m2xq-agentsmith";
    let architect = "jk-a1b2c3d4-thearchitect";
    let docker = fake_docker_with_running_agents(&[smith, architect]);

    let paths = JackinPaths::for_tests(temp.path());
    let container = resolve_running_container_from_context(&paths, &config, &project_dir, &docker)
        .await
        .unwrap();

    assert_eq!(container, architect);
}

#[tokio::test]
async fn resolve_running_container_from_context_uses_indexed_unique_instance() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config = config_with_workspace(&project_dir, vec!["agent-smith".to_owned()], None);
    let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: "jk-k7p9m2xq-myapp-agentsmith",
        workspace_name: Some("my-app"),
        workspace_label: "my-app",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: "jk-k7p9m2xq-myapp-agentsmith".to_owned(),
            dind_container: Some("jk-k7p9m2xq-myapp-agentsmith-dind".to_owned()),
            network: "jk-k7p9m2xq-myapp-agentsmith-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-myapp-agentsmith-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    let state_dir = paths.data_dir.join(&manifest.container_base);
    manifest.write(&state_dir).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    // inspect returns Running → indexed candidate is live
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
            jackin_docker::docker_client::ContainerState::Running,
        ])),
        ..Default::default()
    };

    let container = resolve_running_container_from_context(&paths, &config, &project_dir, &docker)
        .await
        .unwrap();

    assert_eq!(container, "jk-k7p9m2xq-myapp-agentsmith");
}
