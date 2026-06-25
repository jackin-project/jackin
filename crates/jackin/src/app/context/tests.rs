//! Tests for `context`.
use super::*;
use crate::config;
use crate::workspace;
use jackin_config::find_saved_workspace_for_cwd;

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
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
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
    assert!(result.is_err());
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
        config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: project_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: Some("agent-smith".to_owned()),
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
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
        config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: workspace_root.display().to_string(),
            mounts: vec![workspace::MountConfig {
                src: repo_dir.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/jackin".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: Some("agent-smith".to_owned()),
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
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
        config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: project_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: None,
            default_agent: None,
            last_role: Some("ghost-role".to_owned()),
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        },
    );

    let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

    assert_eq!(resolved.0.key(), "agent-smith");
    assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_owned()));
}

/// Build an `AppConfig` pre-populated with an `agent-smith` role and a
/// single workspace rooted at `project_dir`.
fn config_with_workspace(
    project_dir: &Path,
    allowed_roles: Vec<String>,
    last_role: Option<String>,
) -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.roles.insert(
        "the-architect".to_owned(),
        config::RoleSource {
            git: "https://github.com/jackin-project/jackin-the-architect.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: project_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles,
            default_role: None,
            default_agent: None,
            last_role,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        },
    );
    config
}

fn fake_docker_with_running_agents(names: &[&str]) -> crate::docker_client::FakeDockerClient {
    use crate::docker_client::{ContainerRow, FakeDockerClient};
    let rows: Vec<ContainerRow> = names
        .iter()
        .map(|name| ContainerRow {
            name: name.to_string(),
            labels: std::collections::HashMap::default(),
        })
        .collect();
    FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([rows])),
        ..Default::default()
    }
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
        agent_runtime: crate::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: "jk-k7p9m2xq-myapp-agentsmith".to_owned(),
            dind_container: "jk-k7p9m2xq-myapp-agentsmith-dind".to_owned(),
            network: "jk-k7p9m2xq-myapp-agentsmith-net".to_owned(),
            certs_volume: "jk-k7p9m2xq-myapp-agentsmith-dind-certs".to_owned(),
        },
    });
    let state_dir = paths.data_dir.join(&manifest.container_base);
    manifest.write(&state_dir).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    // inspect returns Running → indexed candidate is live
    let docker = crate::docker_client::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
            crate::docker_client::ContainerState::Running,
        ])),
        ..Default::default()
    };

    let container = resolve_running_container_from_context(&paths, &config, &project_dir, &docker)
        .await
        .unwrap();

    assert_eq!(container, "jk-k7p9m2xq-myapp-agentsmith");
}

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
        agent_runtime: crate::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: "jk-k7p9m2xq-agentsmith".to_owned(),
            dind_container: "jk-k7p9m2xq-agentsmith-dind".to_owned(),
            network: "jk-k7p9m2xq-agentsmith-net".to_owned(),
            certs_volume: "jk-k7p9m2xq-agentsmith-dind-certs".to_owned(),
        },
    });
    let state_dir = paths.data_dir.join(&manifest.container_base);
    manifest.write(&state_dir).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    // inspect returns Running → ad-hoc indexed candidate is live
    let docker = crate::docker_client::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
            crate::docker_client::ContainerState::Running,
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
        agent_runtime: crate::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: container.to_owned(),
            dind_container: format!("{container}-dind"),
            network: format!("{container}-net"),
            certs_volume: format!("{container}-dind-certs"),
        },
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
        agent_runtime: crate::agent::Agent::Codex,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: container.to_owned(),
            dind_container: format!("{container}-dind"),
            network: format!("{container}-net"),
            certs_volume: format!("{container}-dind-certs"),
        },
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

/// Test helper: construct a minimal workspace-containing `AppConfig`,
/// persist it to disk at the expected config path, and return the
/// live in-memory copy. Matches the production invariant that
/// `remember_last_agent` observes: the config is already on disk.
fn persisted_config_with_workspace(paths: &JackinPaths, temp_path: &Path) -> AppConfig {
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: temp_path.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );
    let serialized = toml::to_string_pretty(&config).unwrap();
    std::fs::write(&paths.config_file, serialized).unwrap();
    config
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

/// Regression: a workspace whose workdir is a broad parent directory must not
/// match when cwd is an unrelated subdirectory not covered by any mount source.
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
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
            mounts: vec![workspace::MountConfig {
                src: agent_repo.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/role-repo".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
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

/// Complement: workspace still matches when cwd IS under a mount source.
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
            version: config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
            mounts: vec![workspace::MountConfig {
                src: agent_repo.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/role-repo".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
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

// ── supported_agents_requiring_prompt gating ─────────────────────

fn write_role_manifest(role_dir: &Path, body: &str) {
    std::fs::create_dir_all(role_dir).unwrap();
    std::fs::write(role_dir.join("jackin.role.toml"), body).unwrap();
}

#[test]
fn requires_prompt_when_role_supports_two_agents_and_no_workspace_default() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("the-architect").unwrap();
    write_role_manifest(
        &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
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
        vec![crate::agent::Agent::Claude, crate::agent::Agent::Codex]
    );
}

#[test]
fn requires_prompt_includes_amp_when_role_supports_three_agents() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("the-architect").unwrap();
    write_role_manifest(
        &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
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
            crate::agent::Agent::Claude,
            crate::agent::Agent::Codex,
            crate::agent::Agent::Amp,
        ]
    );
}

#[test]
fn skips_prompt_when_workspace_default_agent_is_set() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("the-architect").unwrap();
    write_role_manifest(
        &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
    );

    let result =
        supported_agents_requiring_prompt(&paths, &selector, Some(crate::agent::Agent::Codex));
    assert!(
        result.is_none(),
        "explicit workspace default_agent must short-circuit the prompt"
    );
}

#[test]
fn skips_prompt_when_role_supports_a_single_agent() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("solo").unwrap();
    write_role_manifest(
        &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
    );

    assert!(
        supported_agents_requiring_prompt(&paths, &selector, None).is_none(),
        "single-agent roles have nothing to disambiguate"
    );
}

#[test]
fn skips_prompt_when_manifest_is_missing_or_unreadable() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("ghost").unwrap();
    // No manifest written — load_role will fetch and validate later.
    assert!(supported_agents_requiring_prompt(&paths, &selector, None).is_none());
}
