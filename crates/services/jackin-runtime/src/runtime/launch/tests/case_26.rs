// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn record_instance_attach_outcome_updates_manifest() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest
        .write(&paths.data_dir.join(container_name))
        .unwrap();

    record_instance_attach_outcome(
        &paths,
        container_name,
        crate::isolation::finalize::AttachOutcome::stopped(137),
    )
    .unwrap();

    let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
    assert_eq!(manifest.last_attach_outcome.as_deref(), Some("exit:137"));
}

#[tokio::test]
async fn record_running_attach_outcome_restores_running_status() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::RestoreAvailable);
    write_indexed_manifest(&paths, &manifest);

    record_instance_attach_outcome(
        &paths,
        container_name,
        crate::isolation::finalize::AttachOutcome::still_running(),
    )
    .unwrap();

    let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
    assert_eq!(manifest.status, InstanceStatus::Running);
    assert_eq!(manifest.last_attach_outcome.as_deref(), Some("running"));
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(index.instances[0].status, InstanceStatus::Running);
}

#[tokio::test]
async fn format_attach_outcome_names_running_exit_and_oom() {
    use crate::isolation::finalize::AttachOutcome;

    assert_eq!(
        format_attach_outcome(AttachOutcome::still_running()),
        "running"
    );
    assert_eq!(format_attach_outcome(AttachOutcome::stopped(0)), "exit:0");
    assert_eq!(
        format_attach_outcome(AttachOutcome::oom_killed()),
        "oom_killed"
    );
}

#[test]
fn unassigned_accounts_never_forward_host_auth() {
    let cfg = AppConfig::default();
    let trace = super::capsule_setup::account_auth_selections(&cfg, &[]).unwrap();
    assert!(trace.is_empty());
}

#[test]
fn assigned_account_resolves_mode_and_profile_together() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider, WorkspaceConfig};
    use jackin_core::Agent;
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Work".to_owned(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: "/accounts/work".into(),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    cfg.workspaces.insert(
        "proj".to_owned(),
        WorkspaceConfig {
            accounts: vec!["work".to_owned()],
            ..Default::default()
        },
    );
    let proj = jackin_core::WorkspaceName::parse("proj").unwrap();
    let instances =
        jackin_config::resolve_launch(&cfg, Some(&proj), "builder", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "work@codex");
    let selections = super::capsule_setup::account_auth_selections(&cfg, &instances).unwrap();
    assert_eq!(
        selections["work@codex"],
        (
            jackin_config::AuthForwardMode::Sync,
            Some("/accounts/work".into())
        )
    );
}

#[tokio::test]
async fn inspect_attach_outcome_capture_failure_returns_still_running() {
    // Docker unavailable or container removed mid-inspect must NOT route
    // through finalize_clean_exit's auto-cleanup path — still_running
    // keeps records preserved for `jackin hardline` to recover.
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    for state in [
        ContainerState::NotFound,
        ContainerState::InspectUnavailable("daemon down".into()),
    ] {
        let docker = jackin_test_support::FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([state])),
            ..Default::default()
        };
        let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
        assert_eq!(outcome, AttachOutcome::still_running());
    }
}

#[tokio::test]
async fn inspect_attach_outcome_exited_zero_returns_stopped() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Stopped {
        exit_code: 0,
        oom_killed: false,
    });
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::stopped(0));
}

#[tokio::test]
async fn inspect_attach_outcome_exited_nonzero_returns_stopped_with_code() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Stopped {
        exit_code: 137,
        oom_killed: false,
    });
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::stopped(137));
}

#[tokio::test]
async fn inspect_attach_outcome_exited_oom_returns_oom_killed() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Stopped {
        exit_code: 137,
        oom_killed: true,
    });
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::oom_killed());
}

#[tokio::test]
async fn inspect_attach_outcome_running_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Running);
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::still_running());
}

#[tokio::test]
async fn inspect_attach_outcome_paused_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Paused);
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(
        outcome,
        AttachOutcome::still_running(),
        "paused containers must NOT route through finalize_clean_exit's auto-cleanup path"
    );
}

#[tokio::test]
async fn inspect_attach_outcome_transient_states_return_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    for state in [
        ContainerState::Restarting,
        ContainerState::Removing,
        ContainerState::Created,
    ] {
        let docker = inspect_docker(state.clone());
        let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
        assert_eq!(
            outcome,
            AttachOutcome::still_running(),
            "{state:?} must map to still_running",
        );
    }
}

#[tokio::test]
async fn inspect_attach_outcome_dead_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Dead);
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::still_running());
}

#[tokio::test]
async fn inspect_attach_outcome_unknown_status_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::InspectUnavailable("unexpected".into()));
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::still_running());
}

#[tokio::test]
async fn verify_github_token_present_ok_when_token_resolves() {
    let r = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        Some("ghp_real"),
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
}

#[tokio::test]
async fn verify_github_token_present_ok_for_sync_and_ignore_regardless_of_token() {
    // Sync / Ignore have no pre-flight invariant on GH_TOKEN —
    // Sync sources its token from the host, Ignore exports nothing.
    let r = verify_github_token_present(
        jackin_config::GithubAuthMode::Sync,
        None,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
    let r = verify_github_token_present(
        jackin_config::GithubAuthMode::Ignore,
        None,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
}

#[tokio::test]
async fn verify_github_token_present_errors_when_token_missing() {
    let err = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        None,
        &WorkspaceName::parse("customer-acme").unwrap(),
        "release-bot",
    )
    .unwrap_err();
    let s = err.to_string();
    assert!(s.contains("auth_forward = \"token\""), "got: {s}");
    assert!(s.contains("workspace 'customer-acme'"), "got: {s}");
    assert!(s.contains("role 'release-bot'"), "got: {s}");
    assert!(s.contains("GH_TOKEN"), "got: {s}");
    // Operator-actionable remediation suggestions.
    assert!(s.contains("[github.env]"), "got: {s}");
    assert!(
        s.contains("[workspaces.customer-acme.github.env]"),
        "got: {s}"
    );
    assert!(
        s.contains("[workspaces.customer-acme.roles.release-bot.github.env]"),
        "got: {s}"
    );
    assert!(s.contains("auth_forward = \"sync\""), "got: {s}");
    assert!(s.contains("\"ignore\""), "got: {s}");
}

#[tokio::test]
async fn verify_github_token_present_errors_when_token_empty_string() {
    // Empty string must be rejected the same as missing — `gh`
    // reads `GH_TOKEN=""` as no token, and we don't want to
    // launch DinD just for the agent to fail at first push.
    let err = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        Some(""),
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    )
    .unwrap_err();
    assert!(err.to_string().contains("GH_TOKEN"));
}

#[tokio::test]
async fn resolve_github_env_map_returns_empty_for_no_declarations() {
    use std::collections::BTreeMap;
    let decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    let resolved = resolve_github_env_map(&decls, &LoadOptions::default()).unwrap();
    assert!(resolved.is_empty());
}

#[tokio::test]
async fn resolve_github_env_map_resolves_plain_values() {
    use std::collections::BTreeMap;
    let mut decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    decls.insert(
        "GH_TOKEN".into(),
        jackin_core::EnvValue::Plain("ghp_test".into()),
    );
    decls.insert(
        "GH_HOST".into(),
        jackin_core::EnvValue::Plain("ghe.acme.com".into()),
    );
    let resolved = resolve_github_env_map(&decls, &LoadOptions::default()).unwrap();
    assert_eq!(
        resolved.get("GH_TOKEN").map(String::as_str),
        Some("ghp_test")
    );
    assert_eq!(
        resolved.get("GH_HOST").map(String::as_str),
        Some("ghe.acme.com"),
    );
}

#[tokio::test]
async fn resolve_github_env_map_aggregates_failures() {
    use std::collections::BTreeMap;
    // Two host-env references, both unset → both reported in
    // one structured error rather than aborting on the first.
    let mut decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    decls.insert(
        "GH_TOKEN".into(),
        jackin_core::EnvValue::Plain("$JACKIN_TEST_MISSING_TOKEN".into()),
    );
    decls.insert(
        "GH_HOST".into(),
        jackin_core::EnvValue::Plain("$JACKIN_TEST_MISSING_HOST".into()),
    );
    let opts = LoadOptions {
        // Empty host-env map so `$NAME` references fail to resolve.
        host_env: Some(BTreeMap::new()),
        ..LoadOptions::default()
    };
    let err = resolve_github_env_map(&decls, &opts).unwrap_err();
    let s = err.to_string();
    assert!(
        s.contains("github env resolution failed for 2 var(s)"),
        "expected aggregated count, got: {s}"
    );
    assert!(s.contains("GH_TOKEN"), "got: {s}");
    assert!(s.contains("GH_HOST"), "got: {s}");
}
