// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for app dispatch.
use super::*;

#[test]
fn parse_auth_forward_mode_from_cli_accepts_sync() {
    let mode = parse_auth_forward_mode_from_cli("sync").unwrap();
    assert_eq!(mode, jackin_config::AuthForwardMode::Sync);
}

#[test]
fn parse_auth_forward_mode_from_cli_rejects_bogus() {
    assert!(parse_auth_forward_mode_from_cli("bogus").is_err());
}

#[test]
fn auth_show_prints_builtin_agents() {
    // No global override means each agent falls through to its
    // default-mode (Sync). The point of this test is the output shape:
    // all built-in agents are surfaced, so a non-Claude-primary operator
    // running `jackin config auth show` is not silently shown only Claude.
    let config = AppConfig::default();
    let out = render_auth_show(&config);
    assert!(out.contains("claude:"), "missing claude line: {out}");
    assert!(out.contains("codex:"), "missing codex line: {out}");
    assert!(out.contains("amp:"), "missing amp line: {out}");
}

#[test]
fn resolve_instance_reference_matches_manifest_instance_id() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: "jk-k7p9m2xq-workspace-agentsmith",
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
            role_container: "jk-k7p9m2xq-workspace-agentsmith".to_owned(),
            dind_container: Some("jk-k7p9m2xq-workspace-agentsmith-dind".to_owned()),
            network: "jk-k7p9m2xq-workspace-agentsmith-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-workspace-agentsmith-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    let state_dir = paths.data_dir.join(&manifest.container_base);
    manifest.write(&state_dir).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

    let resolved = resolve_instance_reference(&paths, "k7p9m2xq").unwrap();

    assert_eq!(
        resolved.as_deref(),
        Some("jk-k7p9m2xq-workspace-agentsmith")
    );
}

#[test]
fn resolve_instance_reference_ignores_purged_tombstones() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: "jk-k7p9m2xq-workspace-agentsmith",
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
            role_container: "jk-k7p9m2xq-workspace-agentsmith".to_owned(),
            dind_container: Some("jk-k7p9m2xq-workspace-agentsmith-dind".to_owned()),
            network: "jk-k7p9m2xq-workspace-agentsmith-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-workspace-agentsmith-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    });
    manifest.mark_status(instance::InstanceStatus::Purged);
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

    let resolved = resolve_instance_reference(&paths, "k7p9m2xq").unwrap();

    assert!(resolved.is_none());
}

#[test]
fn hardline_action_options_expose_recovery_controls() {
    let options = hardline_action_options();

    assert_eq!(options[0].1, HardlineAction::Reconnect);
    assert_eq!(options[1].1, HardlineAction::NewSession);
    assert_eq!(options[2].1, HardlineAction::Inspect);
    assert_eq!(options[3].1, HardlineAction::Cancel);
    assert!(options[1].0.contains("agent session"));
    assert!(options[2].0.contains("Inspect"));
}

#[test]
fn explicit_hardline_prompts_only_for_multiple_agent_sessions() {
    assert!(!has_multiple_agent_sessions(
        &runtime::AgentSessionInventory::NotRunning
    ));
    assert!(!has_multiple_agent_sessions(
        &runtime::AgentSessionInventory::Sessions(vec![runtime::AgentSession {
            name: "jackin-claude-abc123".to_owned(),
        }])
    ));
    assert!(has_multiple_agent_sessions(
        &runtime::AgentSessionInventory::Sessions(vec![
            runtime::AgentSession {
                name: "jackin-claude-abc123".to_owned(),
            },
            runtime::AgentSession {
                name: "jackin-codex-abc123".to_owned(),
            },
        ])
    ));
}

#[test]
fn ad_hoc_restore_input_accepts_original_project_directory() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let project = project.canonicalize().unwrap();
    let manifest = ad_hoc_manifest_for_workdir(&project);

    let input = ad_hoc_restore_input_for_current_dir(&manifest, &project, false);

    assert!(matches!(input, Some(LoadWorkspaceInput::CurrentDir)));
}

#[test]
fn ad_hoc_restore_input_can_use_confirmed_moved_project_directory() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("original");
    let moved = temp.path().join("moved");
    std::fs::create_dir(&original).unwrap();
    std::fs::create_dir(&moved).unwrap();
    let original = original.canonicalize().unwrap();
    let moved = moved.canonicalize().unwrap();
    let manifest = ad_hoc_manifest_for_workdir(&original);

    assert!(ad_hoc_restore_input_for_current_dir(&manifest, &moved, false).is_none());
    let input = ad_hoc_restore_input_for_current_dir(&manifest, &moved, true);

    match input {
        Some(LoadWorkspaceInput::Path { src, dst }) => {
            assert_eq!(src, moved.display().to_string());
            assert_eq!(dst, original.display().to_string());
        }
        other => panic!("expected moved project path input; got {other:?}"),
    }
}

#[test]
fn ad_hoc_restore_input_can_use_entered_moved_project_path() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("original");
    let moved = temp.path().join("moved");
    std::fs::create_dir(&original).unwrap();
    std::fs::create_dir(&moved).unwrap();
    let original = original.canonicalize().unwrap();
    let moved = moved.canonicalize().unwrap();
    let manifest = ad_hoc_manifest_for_workdir(&original);

    let input = ad_hoc_restore_input_for_moved_path(&manifest, &moved);

    match input {
        Some(LoadWorkspaceInput::Path { src, dst }) => {
            assert_eq!(src, moved.display().to_string());
            assert_eq!(dst, original.display().to_string());
        }
        other => panic!("expected moved project path input; got {other:?}"),
    }
}

#[test]
fn ad_hoc_restore_input_rejects_missing_entered_moved_project_path() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("original");
    std::fs::create_dir(&original).unwrap();
    let original = original.canonicalize().unwrap();
    let manifest = ad_hoc_manifest_for_workdir(&original);

    let input = ad_hoc_restore_input_for_moved_path(&manifest, &temp.path().join("missing"));

    assert!(input.is_none());
}

#[test]
fn classify_moved_path_entry_empty_input_cancels() {
    assert!(matches!(
        classify_moved_path_entry(""),
        MovedPathEntryStep::Cancel
    ));
    assert!(matches!(
        classify_moved_path_entry("   \t  "),
        MovedPathEntryStep::Cancel
    ));
}

#[test]
fn classify_moved_path_entry_accepts_existing_directory() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("project");
    std::fs::create_dir_all(&dir).unwrap();
    match classify_moved_path_entry(&dir.display().to_string()) {
        MovedPathEntryStep::Accepted(p) => {
            assert_eq!(p, dir.canonicalize().unwrap());
        }
        other => panic!("expected Accepted, got {other:?}"),
    }
}

#[test]
fn classify_moved_path_entry_rejects_regular_file_with_retry() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("not-a-dir");
    std::fs::write(&file, "").unwrap();
    match classify_moved_path_entry(&file.display().to_string()) {
        MovedPathEntryStep::Retry(msg) => assert!(msg.contains("not a directory"), "{msg}"),
        other => panic!("expected Retry, got {other:?}"),
    }
}

#[test]
fn classify_moved_path_entry_rejects_missing_path_with_retry() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("does-not-exist");
    match classify_moved_path_entry(&missing.display().to_string()) {
        MovedPathEntryStep::Retry(msg) => assert!(msg.contains("cannot use"), "{msg}"),
        other => panic!("expected Retry, got {other:?}"),
    }
}

impl std::fmt::Debug for MovedPathEntryStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancel => write!(f, "Cancel"),
            Self::Accepted(p) => write!(f, "Accepted({})", p.display()),
            Self::Retry(s) => write!(f, "Retry({s})"),
        }
    }
}

#[test]
fn moved_path_browser_choices_include_parent_sorted_children_and_manual_escape() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let cwd = root.join("current");
    let alpha = cwd.join("alpha");
    let beta = cwd.join("Beta");
    std::fs::create_dir_all(&beta).unwrap();
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::write(cwd.join("not-a-dir"), "").unwrap();

    let choices = moved_path_browser_choices(&cwd);

    assert_eq!(
        choices,
        vec![
            MovedPathBrowserChoice::SelectCurrent(cwd.canonicalize().unwrap()),
            MovedPathBrowserChoice::Parent(root.canonicalize().unwrap()),
            MovedPathBrowserChoice::Child(alpha.canonicalize().unwrap()),
            MovedPathBrowserChoice::Child(beta.canonicalize().unwrap()),
            MovedPathBrowserChoice::Manual,
            MovedPathBrowserChoice::Cancel,
        ]
    );
}

fn ad_hoc_manifest_for_workdir(workdir: &std::path::Path) -> instance::InstanceManifest {
    let workdir = workdir.display().to_string();
    instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: "jk-k7p9m2xq-agentsmith",
        workspace_name: None,
        workspace_label: &workdir,
        workdir: &workdir,
        host_workdir_fingerprint: &instance::manifest::host_path_fingerprint(&workdir),
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
    })
}

fn write_stop_test_manifest(
    paths: &JackinPaths,
    workdir: &std::path::Path,
    status: instance::InstanceStatus,
) -> String {
    let mut manifest = ad_hoc_manifest_for_workdir(workdir);
    manifest.mark_status(status);
    let container = manifest.container_base.clone();
    manifest.write(&paths.data_dir.join(&container)).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    container
}

#[tokio::test]
async fn stop_failure_leaves_running_manifest_when_container_still_exists() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container =
        write_stop_test_manifest(&paths, temp.path(), instance::InstanceStatus::Running);
    let docker = runtime::test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
            runtime::ContainerState::Running,
        ])),
        ..Default::default()
    };

    mark_instance_restore_available_after_stop(&paths, &container, &docker, false).await;

    let manifest = instance::InstanceManifest::read(&paths.data_dir.join(&container)).unwrap();
    assert_eq!(manifest.status, instance::InstanceStatus::Running);
    let index = instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(index.instances[0].status, instance::InstanceStatus::Running);
}

#[tokio::test]
async fn stop_failure_marks_restore_available_when_container_is_gone() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container =
        write_stop_test_manifest(&paths, temp.path(), instance::InstanceStatus::Running);
    let docker = runtime::test_support::FakeDockerClient::default();

    mark_instance_restore_available_after_stop(&paths, &container, &docker, false).await;

    let manifest = instance::InstanceManifest::read(&paths.data_dir.join(&container)).unwrap();
    assert_eq!(manifest.status, instance::InstanceStatus::RestoreAvailable);
    let index = instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(
        index.instances[0].status,
        instance::InstanceStatus::RestoreAvailable
    );
}

#[tokio::test]
async fn hardline_restore_candidate_marks_missing_manifest_available() {
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
    let state_dir = paths.data_dir.join(container);
    manifest.write(&state_dir).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    // inspect returns NotFound → manifest marked RestoreAvailable
    let docker = runtime::test_support::FakeDockerClient::default();

    let candidate = restore_candidate_for_hardline(&paths, container, &docker)
        .await
        .unwrap()
        .expect("missing crashed manifest should restore");

    assert_eq!(candidate.container_base, container);
    let manifest = instance::InstanceManifest::read(&state_dir).unwrap();
    assert_eq!(manifest.status, instance::InstanceStatus::RestoreAvailable);
    let index = instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(
        index.instances[0].status,
        instance::InstanceStatus::RestoreAvailable
    );
}

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
    let docker = runtime::test_support::FakeDockerClient {
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
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
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
fn validate_setup_role_rejects_disallowed_and_accepts_allowed() {
    let mut config = AppConfig::default();
    let ws = crate::workspace::WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/jackin".into(),
        allowed_roles: vec!["alpha".into(), "beta".into()],
        ..Default::default()
    };
    config.workspaces.insert("proj".into(), ws);

    validate_setup_role_allowed(&config, "proj", "alpha").expect("allowed role passes");
    let err = validate_setup_role_allowed(&config, "proj", "typo").unwrap_err();
    assert!(
        err.to_string().contains("not allowed"),
        "disallowed role must bail: {err}"
    );
}

#[test]
fn validate_setup_role_allows_any_when_allowed_roles_empty() {
    let mut config = AppConfig::default();
    let ws = crate::workspace::WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/jackin".into(),
        allowed_roles: vec![],
        ..Default::default()
    };
    config.workspaces.insert("proj".into(), ws);
    validate_setup_role_allowed(&config, "proj", "anything").expect("empty list = any role");
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

/// Test fake for [`jackin_env::OpWriteRunner`] used by the
/// rotate-cleanup tests below. Shared with `jackin-env` via
/// `jackin_env::test_support::FakeOpWriter` (Phase 2 dedup).
use jackin_env::test_support::FakeOpWriter;

/// Rotate's prior-item cleanup parses the prior op:// reference,
/// issues a delete with the parsed UUIDs, and returns Ok.
#[test]
fn delete_prior_op_item_with_op_ref_calls_writer_with_parsed_uuids() {
    let prior = Some(jackin_core::EnvValue::OpRef(jackin_core::OpRef {
        op: "op://VAULT_UUID/OLD_ITEM/FIELD".into(),
        path: "Personal/Prior/token".into(),
        account: None,
        on_demand: false,
    }));
    let new_ref = jackin_core::OpRef {
        op: "op://VAULT_UUID/NEW_ITEM/FIELD".into(),
        path: "Personal/New/token".into(),
        account: None,
        on_demand: false,
    };
    let writer = FakeOpWriter::new();
    delete_prior_op_item_with_runner(prior, &new_ref, &writer).unwrap();
    assert_eq!(
        *writer.deletes.borrow(),
        vec![("VAULT_UUID".to_owned(), "OLD_ITEM".to_owned())],
    );
    assert_eq!(*writer.delete_accounts.borrow(), vec![None],);
}

/// The prior item the operator adopted (no jackin tag) must NOT be
/// deleted on rotate — it may hold the operator's other fields.
#[test]
fn delete_prior_op_item_spares_operator_adopted_item() {
    let prior = Some(jackin_core::EnvValue::OpRef(jackin_core::OpRef {
        op: "op://VAULT_UUID/SHARED_ITEM/token".into(),
        path: "Personal/My Vault Item/token".into(),
        account: None,
        on_demand: false,
    }));
    let new_ref = jackin_core::OpRef {
        op: "op://VAULT_UUID/NEW_ITEM/FIELD".into(),
        path: "Personal/New/token".into(),
        account: None,
        on_demand: false,
    };
    let writer = FakeOpWriter::adopted();
    delete_prior_op_item_with_runner(prior, &new_ref, &writer).unwrap();
    assert!(
        writer.deletes.borrow().is_empty(),
        "an adopted (non-jackin-tagged) prior item must never be deleted"
    );
}

/// If the ownership tag-read fails (auth/network), rotate must fail safe:
/// skip the delete (don't risk destroying an item we can't verify) and
/// still return Ok so the freshly-wired token stands.
#[test]
fn delete_prior_op_item_skips_delete_on_tag_read_error() {
    let prior = Some(jackin_core::EnvValue::OpRef(jackin_core::OpRef {
        op: "op://VAULT_UUID/OLD_ITEM/token".into(),
        path: "Personal/Prior/token".into(),
        account: None,
        on_demand: false,
    }));
    let new_ref = jackin_core::OpRef {
        op: "op://VAULT_UUID/NEW_ITEM/FIELD".into(),
        path: "Personal/New/token".into(),
        account: None,
        on_demand: false,
    };
    let writer = FakeOpWriter::tag_read_fails();
    delete_prior_op_item_with_runner(prior, &new_ref, &writer)
        .expect("tag-read failure must not fail the rotate");
    assert!(
        writer.deletes.borrow().is_empty(),
        "an unverifiable prior item must never be deleted"
    );
}

/// Cross-account rotate: the prior item lives in account A, the new
/// item in account B. The delete must target account A (the prior
/// ref's own account) via the per-call override, NOT the new ref's
/// account — otherwise the prior item is orphaned.
#[test]
fn delete_prior_op_item_targets_prior_refs_account() {
    let prior = Some(jackin_core::EnvValue::OpRef(jackin_core::OpRef {
        op: "op://VAULT_UUID/OLD_ITEM/FIELD".into(),
        path: "Personal/Prior/token".into(),
        account: Some("account-A".into()),
        on_demand: false,
    }));
    let new_ref = jackin_core::OpRef {
        op: "op://VAULT_UUID/NEW_ITEM/FIELD".into(),
        path: "Personal/New/token".into(),
        account: Some("account-B".into()),
        on_demand: false,
    };
    // The OpCli is pinned to the NEW account; the per-call override
    // (the prior ref's account) must still win.
    let writer = FakeOpWriter::new();
    delete_prior_op_item_with_runner(prior, &new_ref, &writer).unwrap();
    assert_eq!(
        *writer.deletes.borrow(),
        vec![("VAULT_UUID".to_owned(), "OLD_ITEM".to_owned())],
    );
    assert_eq!(
        *writer.delete_accounts.borrow(),
        vec![Some("account-A".to_owned())],
        "delete must target the account the prior item actually lives in"
    );
}

/// Rotate's prior-item cleanup is a no-op when the prior slot is
/// `None` or holds a literal token — jackin does not know where
/// the literal came from.
#[test]
fn delete_prior_op_item_skips_when_prior_is_none_or_literal() {
    let new_ref = jackin_core::OpRef {
        op: "op://V/I/F".into(),
        path: "Personal/New/token".into(),
        account: None,
        on_demand: false,
    };
    let writer = FakeOpWriter::new();
    delete_prior_op_item_with_runner(None, &new_ref, &writer).unwrap();
    assert!(writer.deletes.borrow().is_empty());

    let writer = FakeOpWriter::new();
    delete_prior_op_item_with_runner(
        Some(jackin_core::EnvValue::Plain("literal".into())),
        &new_ref,
        &writer,
    )
    .unwrap();
    assert!(writer.deletes.borrow().is_empty());
}

/// Rotate must NOT delete the new item it just created if the
/// new and prior `op://` references are equal — a same-ref result
/// indicates a deeper bug, but the safety guard prevents data
/// loss until the operator runs `doctor`.
#[test]
fn delete_prior_op_item_skips_when_new_ref_equals_prior() {
    let same = jackin_core::OpRef {
        op: "op://V/I/F".into(),
        path: "Personal/Item/token".into(),
        account: None,
        on_demand: false,
    };
    let writer = FakeOpWriter::new();
    delete_prior_op_item_with_runner(
        Some(jackin_core::EnvValue::OpRef(same.clone())),
        &same,
        &writer,
    )
    .unwrap();
    assert!(writer.deletes.borrow().is_empty());
}

/// `op item delete` failure promotes to whole-rotate `Err` with
/// a copy-pasteable manual-delete command, so exit-code-driven
/// automation surfaces the orphan.
#[test]
fn delete_prior_op_item_propagates_err_with_actionable_hint() {
    let prior = Some(jackin_core::EnvValue::OpRef(jackin_core::OpRef {
        op: "op://V_UUID/I_UUID/F".into(),
        path: "Personal/Prior/token".into(),
        account: None,
        on_demand: false,
    }));
    let new_ref = jackin_core::OpRef {
        op: "op://V_UUID/I_NEW/F".into(),
        path: "Personal/New/token".into(),
        account: None,
        on_demand: false,
    };
    let writer = FakeOpWriter::failing();
    let err = delete_prior_op_item_with_runner(prior, &new_ref, &writer).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("simulated item_delete failure"), "got: {msg}");
    assert!(
        msg.contains("op item delete I_UUID --vault V_UUID"),
        "must include copy-pasteable recovery command, got: {msg}"
    );
}

use std::collections::HashMap;

#[tokio::test]
async fn resolve_role_no_match_errors() {
    let selector = RoleSelector::new(None, "agent-smith");
    // list_containers returns empty → no match
    let docker = runtime::test_support::FakeDockerClient::default();
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
    let docker = runtime::test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            jackin_docker::docker_client::ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                labels: HashMap::default(),
            },
            jackin_docker::docker_client::ContainerRow {
                name: "jk-a1b2c3d4-agentsmith".to_owned(),
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
    let docker = runtime::test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            jackin_docker::docker_client::ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let name = resolve_role_to_container(&selector, &docker).await.unwrap();
    assert_eq!(name, "jk-k7p9m2xq-agentsmith");
}
