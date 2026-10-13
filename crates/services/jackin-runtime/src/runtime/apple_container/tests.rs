// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn test_binding() -> jackin_protocol::ExecBinding {
    jackin_protocol::ExecBinding {
        name: "TOKEN".to_owned(),
        kind: jackin_protocol::ExecKind::Op,
        source: "op://vault/item/field".to_owned(),
    }
}

#[test]
fn generated_provider_config_fails_before_any_runtime_or_authenticated_session_work() {
    use std::future::Future as _;
    use std::task::{Context, Poll, Waker};
    let fixture = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(fixture.path());
    let state = crate::instance::RoleState {
        root: fixture.path().join("role"),
        gh_config_dir: fixture.path().join("role/gh"),
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Codex,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth::default(),
        auth_outcomes: std::collections::BTreeMap::default(),
        auth_mount_paths: std::collections::BTreeSet::default(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: vec![(
            fixture.path().join("role/provider-config/config.toml"),
            "/home/agent/.codex/config.toml".into(),
        )],
    };
    let config = jackin_protocol::CapsuleConfig::default();
    let resolved_env = jackin_env::ResolvedEnv { vars: Vec::new() };
    let scope = jackin_protocol::usage_broker::UsageCredentialScope::default();
    let mut future = std::pin::pin!(launch(AppleContainerLaunch {
        paths: &paths,
        container_name: "fixture-container",
        image: "fixture-image",
        workspace_name: None,
        workspace_label: "Fixture",
        workdir: "/workspace",
        role_key: "fixture",
        role_display_name: "Fixture",
        agent: jackin_core::Agent::Codex,
        role_source_git: "fixture-only",
        role_source_ref: None,
        image_tag: "fixture-tag",
        env_pairs: &[],
        mounts: &[],
        host_workdir_fingerprint: "fixture",
        capsule_config: &config,
        state: &state,
        resolved_env: &resolved_env,
        credential_scope: &scope,
        debug: false,
        entry_claim: None,
    }));
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(Err(error)) => {
            assert!(format!("{error:#}").contains("requires read-only file overlays"));
        }
        result => panic!(
            "unsupported generated provider config must reject before the first runtime await: {result:?}"
        ),
    }
    assert!(
        !state.root.exists(),
        "runtime state or credentials must not be published"
    );
}

#[test]
fn empty_exec_bindings_are_supported() {
    validate_exec_bindings(&[]).expect("no credential relay is required");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_exec_bindings_are_supported() {
    validate_exec_bindings(&[test_binding()]).expect("Linux peer auth is available");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn non_linux_exec_bindings_are_rejected_explicitly() {
    let error =
        validate_exec_bindings(&[test_binding()]).expect_err("non-Linux peer auth is unavailable");
    let message = format!("{error:#}");
    assert!(message.contains("apple-container does not support on-demand credential bindings"));
    assert!(message.contains("peer authentication is unavailable"));
}

mod coordination_tests;
