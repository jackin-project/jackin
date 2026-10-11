// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn workspace_mise_paths_cover_workdir_and_mount_destinations() {
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "sample-workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: "/host/jackin".to_owned(),
                dst: "/workspace/jackin".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: "/host/homebrew-tap".to_owned(),
                dst: "/workspace/homebrew-tap".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };

    let value = workspace_mise_trusted_config_paths(&workspace).unwrap();

    assert_eq!(
        value,
        "/workspace:/workspace/homebrew-tap:/workspace/jackin"
    );
}

#[tokio::test]
async fn workspace_mise_env_does_not_override_operator_value() {
    let workspace = repo_workspace(Path::new("/host/repo"));
    let mut vars = vec![(
        MISE_TRUSTED_CONFIG_PATHS_ENV.to_owned(),
        "/operator/trusted".to_owned(),
    )];

    inject_workspace_mise_env(&mut vars, &workspace);

    assert_eq!(
        vars,
        vec![(
            MISE_TRUSTED_CONFIG_PATHS_ENV.to_owned(),
            "/operator/trusted".to_owned()
        )]
    );
}

#[test]
fn attach_failure_error_preserves_command_context() {
    let error = attach_failure_error(
        "jk-test",
        &anyhow::anyhow!("command failed: docker exec ..."),
    )
    .to_string();

    assert!(
        error.contains("capsule attach failed for jk-test"),
        "{error}"
    );
    assert!(error.contains("command failed: docker exec"), "{error}");
}

#[test]
fn known_socket_close_requires_clean_exit_and_attach_transport_error() {
    use jackin_docker::docker_client::ContainerState;

    let clean = ContainerState::Stopped {
        exit_code: 0,
        oom_killed: false,
    };
    assert!(is_known_socket_close(&anyhow::anyhow!("early eof"), &clean));
    assert!(is_known_socket_close(
        &anyhow::anyhow!("command failed: docker exec jk jackin-capsule"),
        &clean
    ));
    assert!(!is_known_socket_close(
        &anyhow::anyhow!("generation lease admission failed"),
        &clean
    ));
    assert!(!is_known_socket_close(
        &anyhow::anyhow!("early eof"),
        &ContainerState::Running
    ));
}

#[test]
fn seed_codex_project_trust_seeds_every_codex_slot() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    let primary = root.join("home/.codex");
    let secondary = root.join("home/.codex-personal");
    std::fs::create_dir_all(&primary).unwrap();
    std::fs::create_dir_all(&secondary).unwrap();
    std::fs::write(primary.join("config.toml"), "model = \"work-model\"\n").unwrap();
    std::fs::write(
        secondary.join("config.toml"),
        "model = \"personal-model\"\n",
    )
    .unwrap();
    let (mut state, workspace) = codex_trust_fixture(&root);
    state.auth.slots.insert(
        "personal@codex".to_owned(),
        codex_trust_slot("personal", ".codex-personal"),
    );

    seed_codex_project_trust(&state, &workspace).unwrap();

    for (path, model) in [
        (&primary.join("config.toml"), "work-model"),
        (&secondary.join("config.toml"), "personal-model"),
    ] {
        let config = std::fs::read_to_string(path).unwrap();
        assert!(config.contains(format!("model = \"{model}\"").as_str()));
        assert!(config.contains("[projects.\"/workspace\"]"));
        assert!(config.contains("[projects.\"/workspace/repo\"]"));
        assert_eq!(config.matches("trust_level = \"trusted\"").count(), 2);
    }
}

#[test]
fn seed_codex_project_trust_preserves_existing_config() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    std::fs::write(
        root.join("home/.codex/config.toml"),
        "model = \"gpt-5\"\n\n[projects.\"/existing\"]\ntrust_level = \"trusted\"\n",
    )
    .unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();

    let codex_config = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert!(codex_config.contains("model = \"gpt-5\""));
    assert!(codex_config.contains("[projects.\"/existing\"]"));
    assert!(codex_config.contains("[projects.\"/workspace\"]"));
    assert!(codex_config.contains("[projects.\"/workspace/repo\"]"));
    assert_eq!(codex_config.matches("trust_level = \"trusted\"").count(), 3);
}

#[test]
fn seed_codex_project_trust_replaces_non_table_projects_value() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    std::fs::write(root.join("home/.codex/config.toml"), "projects = 5\n").unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();

    let codex_config = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert!(!codex_config.contains("projects = 5"));
    assert!(codex_config.contains("[projects.\"/workspace\"]"));
    assert!(codex_config.contains("trust_level = \"trusted\""));
}

#[test]
fn seed_codex_project_trust_replaces_non_table_project_entry() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    std::fs::write(
        root.join("home/.codex/config.toml"),
        "[projects]\n\"/workspace\" = \"oops\"\n",
    )
    .unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();

    let codex_config = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert!(!codex_config.contains("\"oops\""));
    let doc: toml_edit::DocumentMut = codex_config.parse().unwrap();
    let projects = doc.get("projects").and_then(|i| i.as_table_like()).unwrap();
    let workspace_entry = projects.get("/workspace").and_then(|i| i.as_table_like());
    assert_eq!(
        workspace_entry
            .and_then(|t| t.get("trust_level"))
            .and_then(|i| i.as_str()),
        Some("trusted")
    );
}

#[test]
fn seed_codex_project_trust_is_idempotent_across_relaunches() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();
    let first = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    seed_codex_project_trust(&state, &workspace).unwrap();
    let second = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();

    assert_eq!(first, second);
    assert_eq!(second.matches("trust_level = \"trusted\"").count(), 2);
}

#[test]
fn seed_codex_project_trust_errors_on_invalid_toml_without_clobbering() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    let original = "[unterminated\n";
    std::fs::write(root.join("home/.codex/config.toml"), original).unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    let err = seed_codex_project_trust(&state, &workspace).unwrap_err();
    assert!(err.to_string().contains("parsing Codex config"));
    let after = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert_eq!(after, original);
}

#[tokio::test]
async fn git_pull_on_entry_starts_all_repo_pulls_before_waiting() {
    let temp = tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    let marker_dir = temp.path().join("markers");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&marker_dir).unwrap();

    let git_script = bin_dir.join("git");
    std::fs::write(
        &git_script,
        r#"#!/bin/sh
set -eu
marker_dir="$(dirname "$0")/../markers"
touch "$marker_dir/$(basename "$2").started"
i=0
while [ "$(find "$marker_dir" -name '*.started' | wc -l | tr -d ' ')" -lt 2 ]; do
  i=$((i + 1))
  if [ "$i" -gt 80 ]; then
    echo "timed out waiting for peer pull" >&2
    exit 42
  fi
  sleep 0.025
done
echo "pulled $2"
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&git_script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&git_script, perms).unwrap();

    let repo_a = temp.path().join("repo-a");
    let repo_b = temp.path().join("repo-b");
    std::fs::create_dir_all(repo_a.join(".git")).unwrap();
    std::fs::create_dir_all(repo_b.join(".git")).unwrap();

    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "parallel".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: repo_a.display().to_string(),
                dst: "/workspace/a".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: repo_b.display().to_string(),
                dst: "/workspace/b".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: true,
        mount_heal: jackin_config::MountHealReport::default(),
    };

    pull_workspace_repos_with_git(&workspace, false, &git_script);

    assert!(marker_dir.join("repo-a.started").is_file());
    assert!(marker_dir.join("repo-b.started").is_file());
}

#[test]
fn git_pull_exports_spawn_failure_without_repo_or_program_paths() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let results = pull_git_sources_with_git(
        vec!["/operator-secret/repository".to_owned()],
        false,
        Path::new("/operator-secret/missing-git"),
        false,
    );
    assert!(matches!(
        results.as_slice(),
        [super::git_pull::GitPullResult::SpawnError { .. }]
    ));
    export.force_flush();

    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(!export.contains_span_text("operator-secret"));
    assert!(!export.contains_span_text("repository"));
    assert!(!export.contains_span_text("missing-git"));
}

#[test]
fn host_runtime_passthrough_env_keeps_only_explicit_runtime_knobs() {
    let passthrough = host_runtime_passthrough_env([
        ("JACKIN_DISABLE_TIRITH".to_owned(), "1".to_owned()),
        ("JACKIN_DHAT_ALLOC_LOG".to_owned(), "1".to_owned()),
        ("JACKIN_CAPSULE_FORCE_PANIC".to_owned(), "true".to_owned()),
        ("TZ".to_owned(), "Asia/Ho_Chi_Minh".to_owned()),
        ("PATH".to_owned(), "/bin".to_owned()),
    ]);

    assert_eq!(
        passthrough,
        vec![
            "JACKIN_DISABLE_TIRITH=1",
            "JACKIN_DHAT_ALLOC_LOG=1",
            "JACKIN_CAPSULE_FORCE_PANIC=true",
            "TZ=Asia/Ho_Chi_Minh",
        ]
    );
}

#[test]
fn debug_runtime_envs_do_not_propagate_file_configuration() {
    let debug_envs = debug_runtime_envs(true);
    assert!(debug_envs.is_empty());
}

#[test]
fn telemetry_runtime_envs_forward_effective_level_to_capsule() {
    assert_eq!(
        telemetry_runtime_envs_for(jackin_diagnostics::TelemetryLevel::Info),
        vec!["JACKIN_TELEMETRY_LEVEL=info".to_owned()]
    );
    assert_eq!(
        telemetry_runtime_envs_for(jackin_diagnostics::TelemetryLevel::Debug),
        vec!["JACKIN_TELEMETRY_LEVEL=debug".to_owned()]
    );
    assert_eq!(
        telemetry_runtime_envs_for(jackin_diagnostics::TelemetryLevel::Trace),
        vec!["JACKIN_TELEMETRY_LEVEL=trace".to_owned()]
    );
}

#[tokio::test]
async fn validate_agent_supported_rejects_unsupported_choice() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");

    let err =
        validate_agent_supported(&selector, &manifest, jackin_core::Agent::Codex).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("role \"agent-smith\""));
    assert!(message.contains("agent \"codex\""));
    assert!(message.contains("supported: [claude]"));
}
