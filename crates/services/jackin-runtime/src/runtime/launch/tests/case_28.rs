// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn programmatic_launch_without_an_agent_fails_before_docker() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["claude-main"]

[accounts.test]
name = "Test"
provider = "anthropic"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.claude-main]
agent = "claude"
account = "test"
"#,
    )
    .unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    config.roles.insert(
        selector.key(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/the-architect".to_owned(),
            trusted: true,
            ..jackin_config::RoleSource::default()
        },
    );
    let workspace = programmatic_role_fixture(&paths, &selector);
    let docker = jackin_test_support::FakeDockerClient::default();
    let mut runner = FakeRunner::for_load_agent([]);

    let mut opts = LoadOptions::programmatic(jackin_core::Agent::Claude);
    opts.agent = None;
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .expect_err("a non-TTY launch cannot open the agent picker");

    assert!(
        error
            .downcast_ref::<LoadOptionsError>()
            .is_some_and(|e| matches!(e, LoadOptionsError::AgentNotResolved { .. })),
        "expected the agent validation failure, got {error:#}"
    );
}

#[tokio::test]
async fn programmatic_launch_refuses_a_role_branch_before_docker() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["claude-main"]

[accounts.test]
name = "Test"
provider = "anthropic"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.claude-main]
agent = "claude"
account = "test"
"#,
    )
    .unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    config.roles.insert(
        selector.key(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/the-architect".to_owned(),
            trusted: true,
            ..jackin_config::RoleSource::default()
        },
    );
    let workspace = programmatic_role_fixture(&paths, &selector);
    let docker = jackin_test_support::FakeDockerClient::default();
    let mut runner = FakeRunner::for_load_agent([]);

    let mut opts = LoadOptions::programmatic(jackin_core::Agent::Claude);
    opts.role_branch = Some("feat/my-pr".to_owned());
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .expect_err("an unreviewed branch needs the interactive branch-trust prompt");

    assert!(
        error
            .downcast_ref::<LoadOptionsError>()
            .is_some_and(|e| matches!(e, LoadOptionsError::RoleBranchNotAllowed { .. })),
        "expected the role-branch validation failure, got {error:#}"
    );
}

#[test]
fn metadata_file_mount_instances_require_recreation_after_layout_change() {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    let temp = tempdir().unwrap();
    let config = AppConfig::default();
    let manifest = crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: "fixture",
        workspace_name: None,
        workspace_label: "fixture",
        workdir: "/workspace",
        host_workdir_fingerprint: "fixture",
        role_key: "role",
        role_display_name: "Role",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "",
        role_source_ref: None,
        image_tag: "fixture",
        docker: crate::instance::DockerResources::from_container_name("fixture"),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.write(temp.path()).unwrap();
    let old_policy = serde_json::to_vec(&serde_json::json!([
        "account-config-v1",
        {},
        {},
        null,
        null
    ]))
    .unwrap();
    let mut old_digest = String::with_capacity(64);
    for byte in Sha256::digest(old_policy) {
        write!(&mut old_digest, "{byte:02x}").unwrap();
    }
    std::fs::write(temp.path().join("account-config.sha256"), old_digest).unwrap();
    assert!(!super::account_configuration_matches(temp.path(), &config, None, "role").unwrap());
    let current = super::account_configuration_fingerprint(&config, None, "role", &[]).unwrap();
    std::fs::write(temp.path().join("account-config.sha256"), current).unwrap();
    assert!(super::account_configuration_matches(temp.path(), &config, None, "role").unwrap());
}

#[tokio::test]
async fn related_restore_load_options_share_the_entry_lease_until_activation() {
    use jackin_test_support::FakeDockerClient;
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let docker = FakeDockerClient::default();
    let manifest = workspace_manifest(
        "jk-related-entry-lease",
        "the-architect",
        "The Architect",
        jackin_core::Agent::Codex,
    );
    let mut current = LoadOptions::for_load(false, false);
    current.entry_claim = Some(std::sync::Arc::new(
        crate::runtime::universe::claim_entry(&paths, &docker).await,
    ));
    let opts = related_restore_load_options(&current, &manifest).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        current.entry_claim.as_ref().unwrap(),
        opts.entry_claim.as_ref().unwrap(),
    ));
    let pending_dir = crate::runtime::coordination::universe_dir(&paths)
        .unwrap()
        .join("universe-pending");
    assert_eq!(std::fs::read_dir(&pending_dir).unwrap().count(), 1);

    drop(current);
    assert_eq!(
        std::fs::read_dir(&pending_dir).unwrap().count(),
        1,
        "nested restore owns the same pending lease after outer options drop"
    );
    opts.entry_claim
        .as_deref()
        .unwrap()
        .activate()
        .await
        .unwrap();
    assert_eq!(std::fs::read_dir(&pending_dir).unwrap().count(), 0);
    assert!(
        crate::runtime::coordination::universe_dir(&paths)
            .unwrap()
            .join("universe-since")
            .exists()
    );
    assert!(
        matches!(
            crate::runtime::universe::take_exit_claim(&paths),
            crate::runtime::universe::ExitClaim::Claimed { .. }
        ),
        "activated nested restore must permit outro while options remain alive"
    );
    drop(opts);
    assert!(
        !crate::runtime::coordination::universe_dir(&paths)
            .unwrap()
            .join("universe-since")
            .exists()
    );
}
