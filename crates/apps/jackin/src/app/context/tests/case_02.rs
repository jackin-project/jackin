// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn resolve_running_container_from_context_uses_ad_hoc_indexed_instance() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical_project = project_dir.canonicalize().unwrap();
    let project = canonical_project.display().to_string();

    let config = AppConfig::default();
    let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: "jk-k7p9m2xq-agentsmith",
        workspace_name: None,
        workspace_label: &project,
        workdir: &project,
        host_workdir_fingerprint: &instance::manifest::host_path_fingerprint(&project),
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: "jk-k7p9m2xq-agentsmith".to_owned(),
            dind_container: Some("jk-k7p9m2xq-agentsmith-dind".to_owned()),
            network: "jk-k7p9m2xq-agentsmith-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-agentsmith-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    let state_dir = paths.data_dir.join(&manifest.container_base);
    manifest.write(&state_dir).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    // inspect returns Running → ad-hoc indexed candidate is live
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
            jackin_docker::docker_client::ContainerState::Running,
        ])),
        ..Default::default()
    };

    let container = resolve_running_container_from_context(&paths, &config, &project_dir, &docker)
        .await
        .unwrap();

    assert_eq!(container, "jk-k7p9m2xq-agentsmith");
}

#[test]
fn hardline_candidate_prompt_label_includes_manifest_and_docker_state() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-k7p9m2xq-myapp-agentsmith";
    let mut manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: container,
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
    manifest.mark_status(instance::InstanceStatus::RestoreAvailable);
    manifest.write(&paths.data_dir.join(container)).unwrap();
    let candidate = HardlineCandidate {
        name: container.to_owned(),
        state: runtime::ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        },
    };

    let label = hardline_candidate_prompt_label(&paths, &candidate);

    assert!(label.contains(container), "{label}");
    assert!(label.contains("my-app"), "{label}");
    assert!(label.contains("agent-smith"), "{label}");
    assert!(label.contains("agent:claude"), "{label}");
    assert!(label.contains("status:restore_available"), "{label}");
    assert!(label.contains("docker:stopped exit:137"), "{label}");
}

#[test]
fn hardline_candidate_prompt_label_counts_running_agent_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-k7p9m2xq-myapp-agentsmith";
    let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: container,
        workspace_name: Some("my-app"),
        workspace_label: "my-app",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Codex,
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
    manifest.write(&paths.data_dir.join(container)).unwrap();
    let candidate = HardlineCandidate {
        name: container.to_owned(),
        state: runtime::ContainerState::Running,
    };

    let label = hardline_candidate_prompt_label(&paths, &candidate);

    assert!(label.contains("docker:running"), "{label}");
}

#[tokio::test]
async fn resolve_running_container_from_context_errors_when_nothing_running() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config = config_with_workspace(&project_dir, vec!["agent-smith".to_owned()], None);
    let docker = fake_docker_with_running_agents(&[]);

    let paths = JackinPaths::for_tests(temp.path());
    let err = resolve_running_container_from_context(&paths, &config, &project_dir, &docker)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("no running roles"), "got: {err}");
    assert!(err.contains("my-app"), "got: {err}");
}

#[tokio::test]
async fn resolve_running_container_from_context_ignores_disallowed_running_agents() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config = config_with_workspace(&project_dir, vec!["agent-smith".to_owned()], None);
    // the-architect is running but not allowed in this workspace.
    let docker = fake_docker_with_running_agents(&["jk-the-architect"]);

    let paths = JackinPaths::for_tests(temp.path());
    let err = resolve_running_container_from_context(&paths, &config, &project_dir, &docker)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("no running roles"), "got: {err}");
}

#[tokio::test]
async fn resolve_running_container_from_context_errors_when_no_workspace_matches() {
    let temp = tempfile::tempdir().unwrap();
    let unrelated = temp.path().join("unrelated");
    std::fs::create_dir_all(&unrelated).unwrap();

    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let config = config_with_workspace(&project_dir, vec!["agent-smith".to_owned()], None);
    let docker = fake_docker_with_running_agents(&["jk-agent-smith"]);

    let paths = JackinPaths::for_tests(temp.path());
    let err = resolve_running_container_from_context(&paths, &config, &unrelated, &docker)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("no saved workspace matches"), "got: {err}");
}

#[test]
fn remember_last_agent_persists_successful_loads() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = persisted_config_with_workspace(&paths, temp.path());

    remember_last_agent(
        &paths,
        &mut config,
        Some("my-app"),
        &RoleSelector::new(None, "agent-smith"),
        &Ok(()),
    );

    assert_eq!(
        config
            .workspaces
            .get("my-app")
            .and_then(|workspace| workspace.last_role.as_deref()),
        Some("agent-smith")
    );
}

#[test]
fn remember_last_agent_skips_failed_loads() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = persisted_config_with_workspace(&paths, temp.path());

    remember_last_agent(
        &paths,
        &mut config,
        Some("my-app"),
        &RoleSelector::new(None, "agent-smith"),
        &Err(anyhow::anyhow!("load failed")),
    );

    assert_eq!(
        config
            .workspaces
            .get("my-app")
            .and_then(|workspace| workspace.last_role.as_deref()),
        None
    );
}

#[test]
fn broad_workdir_does_not_match_unrelated_subdirectory() {
    let temp = tempfile::tempdir().unwrap();
    let broad_workdir = temp.path().join("Projects");
    let agent_repo = broad_workdir.join("role-repo");
    let unrelated = broad_workdir.join("jackin4");
    std::fs::create_dir_all(&agent_repo).unwrap();
    std::fs::create_dir_all(&unrelated).unwrap();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "jackin-roles".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
            mounts: vec![workspace::MountConfig {
                src: agent_repo.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/role-repo".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );

    let result = find_saved_workspace_for_cwd(&config, &unrelated);
    assert!(
        result.is_none(),
        "broad workdir must not preselect for an unrelated subdirectory"
    );
}

#[test]
fn workspace_matches_when_cwd_is_under_mount_src() {
    let temp = tempfile::tempdir().unwrap();
    let broad_workdir = temp.path().join("Projects");
    let agent_repo = broad_workdir.join("role-repo");
    let inside_repo = agent_repo.join("src");
    std::fs::create_dir_all(&inside_repo).unwrap();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "jackin-roles".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
            mounts: vec![workspace::MountConfig {
                src: agent_repo.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/role-repo".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );

    let result = find_saved_workspace_for_cwd(&config, &inside_repo);
    assert!(
        result.is_some(),
        "cwd inside a mount source must still preselect the workspace"
    );
    assert_eq!(result.unwrap().0, "jackin-roles");
}

#[test]
fn requires_prompt_when_role_supports_two_agents_and_no_workspace_default() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("the-architect").unwrap();
    write_role_manifest(
        &jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir,
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
    );

    let agents = supported_agents_requiring_prompt(&paths, &selector, None)
        .expect("multi-agent role with no workspace default must trigger a prompt");
    assert_eq!(
        agents,
        vec![jackin_core::Agent::Claude, jackin_core::Agent::Codex]
    );
}

#[test]
fn requires_prompt_includes_amp_when_role_supports_three_agents() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("the-architect").unwrap();
    write_role_manifest(
        &jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir,
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex", "amp"]

[claude]
plugins = []

[codex]

[amp]
"#,
    );

    let agents = supported_agents_requiring_prompt(&paths, &selector, None)
        .expect("three-agent role with no workspace default must trigger a prompt");
    assert_eq!(
        agents,
        vec![
            jackin_core::Agent::Claude,
            jackin_core::Agent::Codex,
            jackin_core::Agent::Amp,
        ]
    );
}

#[test]
fn skips_prompt_when_workspace_default_agent_is_set() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("the-architect").unwrap();
    write_role_manifest(
        &jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir,
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
    );

    let result =
        supported_agents_requiring_prompt(&paths, &selector, Some(jackin_core::Agent::Codex));
    assert!(
        result.is_none(),
        "explicit workspace default_agent must short-circuit the prompt"
    );
}
