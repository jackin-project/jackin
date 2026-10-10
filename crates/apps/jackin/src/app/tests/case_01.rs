// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_commands_suppress_the_automatic_teardown_notice() {
    for argv in [
        ["jackin", "usage", "--format", "json"].as_slice(),
        [
            "jackin",
            "usage",
            "doctor",
            "--provider",
            "claude",
            "--unattended",
        ]
        .as_slice(),
    ] {
        let cli = Cli::try_parse_from(argv).expect("usage argv should parse");
        let command = cli.command.expect("usage command should be present");

        assert!(
            !should_announce_run_teardown(&command),
            "usage commands must keep stderr available for structured output"
        );
    }

    let cli = Cli::try_parse_from(["jackin", "doctor"]).expect("doctor argv should parse");
    let command = cli.command.expect("doctor command should be present");
    assert!(should_announce_run_teardown(&command));
}

#[test]
fn usage_auth_and_passive_startup_skip_fresh_account_config_bootstrap() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let passive = Cli::try_parse_from(["jackin", "usage"])
        .expect("bare usage should parse")
        .command
        .expect("explicit usage command should be present");
    assert!(matches!(&passive, Command::Usage(args) if args.scope.is_none()));

    let auth_prepare =
        Cli::try_parse_from(["jackin", "usage", "auth", "prepare", "--provider", "claude"])
            .expect("the current foreground auth command should parse")
            .command
            .expect("explicit usage command should be present");
    assert_usage_auth_prepare(&auth_prepare);

    for command in [&passive, &auth_prepare] {
        let (config, bootstrap) = load_startup_config(command, &paths)
            .expect("usage startup should skip config loading on a fresh home");

        assert!(config.accounts.is_empty());
        assert!(config.bootstrap.is_none());
        assert!(!bootstrap.fresh_install);
        assert!(bootstrap.added_accounts.is_empty());
        assert!(bootstrap.issues.is_empty());
    }

    assert!(!paths.config_dir.exists());
    assert!(!paths.jackin_home.exists());
    assert!(!paths.data_dir.exists());
}

#[test]
fn usage_startup_ignores_inline_account_credentials_without_reading_or_rewriting_config() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    let config_contents = format!(
        "version = \"{}\"\n\n[accounts.claude]\nname = \"Claude\"\nprovider = \"anthropic\"\n[accounts.claude.credential]\ntype = \"api_key\"\nvalue = \"inline-account-credential-must-not-be-read\"\n\n[telemetry]\nlevel = \"debug\"\ncategories = [\"usage\"]\n",
        jackin_config::CURRENT_CONFIG_VERSION,
    );
    std::fs::write(&paths.config_file, &config_contents).unwrap();
    let cli = Cli::try_parse_from(["jackin", "usage", "auth", "prepare", "--provider", "claude"])
        .expect("auth command should parse");
    let command = cli
        .command
        .expect("explicit usage command should be present");
    assert_usage_auth_prepare(&command);

    let (config, bootstrap) = load_startup_config(&command, &paths)
        .expect("usage startup must not inspect selected account config");

    assert!(config.accounts.is_empty());
    assert_eq!(config.telemetry, jackin_config::TelemetryConfig::default());
    assert!(!bootstrap.fresh_install);
    assert!(bootstrap.added_accounts.is_empty());
    assert_eq!(
        std::fs::read_to_string(&paths.config_file).unwrap(),
        config_contents
    );
    assert!(!paths.jackin_home.exists());
    assert!(!paths.data_dir.exists());
}

fn assert_usage_auth_prepare(command: &Command) {
    assert!(matches!(
        command,
        Command::Usage(args)
            if matches!(
                args.scope.as_ref(),
                Some(crate::cli::usage::UsageScope::Auth(auth))
                    if matches!(
                        &auth.command,
                        crate::cli::usage::UsageAuthCommand::Prepare(prepare)
                            if prepare.provider == crate::cli::usage::UsageProviderArg::Claude
                    )
            )
    ));
}

#[test]
fn retired_launch_command_is_rejected() {
    let error = Cli::try_parse_from(["jackin", "launch", "agent-smith", "workspace"])
        .expect_err("retired launch syntax must not parse");

    assert_eq!(error.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

#[test]
fn load_command_remains_the_launch_entry_point() {
    let cli =
        Cli::try_parse_from(["jackin", "load", "agent-smith"]).expect("load syntax should parse");

    assert!(matches!(cli.command, Some(Command::Load(_))));
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

#[tokio::test]
async fn stop_failure_leaves_running_manifest_when_container_still_exists() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container =
        write_stop_test_manifest(&paths, temp.path(), instance::InstanceStatus::Running);
    let docker = jackin_test_support::FakeDockerClient {
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
    let docker = jackin_test_support::FakeDockerClient::default();

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
    let docker = jackin_test_support::FakeDockerClient::default();

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
