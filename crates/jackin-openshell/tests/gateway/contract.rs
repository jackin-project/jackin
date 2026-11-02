// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use jackin_core::{HostPath, ImageTag, ProjectRoot, SandboxSlug, StdFileSystem};
use jackin_openshell::contract::{
    CORE_CHECKS, ContractFixture, ContractGroup, TERMINAL_CHECKS, run,
};
use openshell_sdk::raw::proto::{ObjectMeta, Provider, ProviderProfile, exec_sandbox_input};
use tempfile::TempDir;
use tonic::Code;

use super::exec_rpc::{exit_event, host_shell};
use super::harness::{Env, WORKSPACE, connect, env};
use super::server::{ExecBackend, ExecCtx, Fault};

const IMAGE: &str = "jackin.invalid/contract:0123456789abcdef";

async fn fixture() -> (TempDir, ContractFixture) {
    let dir = TempDir::new().unwrap();
    let status = tokio::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dir.path())
        .status()
        .await
        .unwrap();
    assert!(status.success());
    let project = ProjectRoot::open(
        &StdFileSystem::default(),
        &HostPath::parse(dir.path()).unwrap(),
    )
    .await
    .unwrap();
    (
        dir,
        ContractFixture {
            image: ImageTag::parse(IMAGE).unwrap(),
            project,
            name_prefix: SandboxSlug::parse("ctr").unwrap(),
        },
    )
}

fn prepare(env: &Env, root: &TempDir) {
    let mut state = env.server.state();
    state.auto_advance = true;
    state.known_images = Some(HashSet::from([IMAGE.to_owned()]));
    state.exec = Some(host_shell(root.path().to_path_buf()));
}

#[tokio::test]
async fn core_group_passes_against_grpc_gateway_on_the_in_process_server() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    let names: Vec<&str> = report.checks.iter().map(|check| check.name).collect();
    assert_eq!(names, CORE_CHECKS.to_vec());
    for check in &report.checks {
        assert_eq!(check.outcome, Ok(()), "{}", check.name);
    }
    let state = env.server.state();
    assert!(state.profiles.is_empty(), "a contract profile is left");
    assert!(state.providers.is_empty(), "a contract provider is left");
    assert!(state.sandboxes.is_empty(), "a contract sandbox is left");
}

#[tokio::test]
async fn a_leftover_profile_and_provider_from_an_aborted_run_do_not_fail_the_core_group() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    {
        let mut state = env.server.state();
        state.profiles.insert(
            (WORKSPACE.to_owned(), "jackin-contract".to_owned()),
            ProviderProfile {
                id: "jackin-contract".to_owned(),
                description: "left by an aborted run".to_owned(),
                resource_version: 1,
                ..ProviderProfile::default()
            },
        );
        state.providers.insert(
            (WORKSPACE.to_owned(), "jackin-ctr-contract".to_owned()),
            Provider {
                metadata: Some(ObjectMeta {
                    name: "jackin-ctr-contract".to_owned(),
                    ..ObjectMeta::default()
                }),
                r#type: "jackin-contract".to_owned(),
                profile_workspace: WORKSPACE.to_owned(),
                ..Provider::default()
            },
        );
    }
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    for check in &report.checks {
        assert_eq!(check.outcome, Ok(()), "{}", check.name);
    }
    let state = env.server.state();
    assert!(state.profiles.is_empty() && state.providers.is_empty());
}

fn exit_when_told() -> ExecBackend {
    Arc::new(|mut ctx: ExecCtx| {
        Box::pin(async move {
            let mut stdin = Vec::new();
            while let Some(payload) = ctx.inputs.recv().await {
                let exec_sandbox_input::Payload::Stdin(bytes) = payload else {
                    continue;
                };
                stdin.extend_from_slice(&bytes);
                if stdin.windows(6).any(|window| window == b"exit 3") {
                    drop(ctx.events.send(Ok(exit_event(3))).await);
                    return;
                }
            }
        })
    })
}

#[tokio::test]
async fn terminal_group_reports_exactly_the_published_checks_and_a_refused_attach_fails_the_attach_checks()
 {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    {
        let mut state = env.server.state();
        state.exec = Some(exit_when_told());
        state.faults.insert(
            "forward_tcp",
            Fault::Status(
                Code::ResourceExhausted,
                "sandbox SSH connection limit reached".to_owned(),
            ),
        );
    }
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Terminal).await;
    let names: Vec<&str> = report.checks.iter().map(|check| check.name).collect();
    assert_eq!(names, TERMINAL_CHECKS.to_vec());
    for check in &report.checks {
        if check.name == "shell_exit_code" {
            assert_eq!(check.outcome, Ok(()), "{}", check.name);
        } else {
            assert!(
                check
                    .outcome
                    .as_ref()
                    .is_err_and(|message| message.contains("connection limit")),
                "{}: {:?}",
                check.name,
                check.outcome
            );
        }
    }
    assert!(env.server.state().sandboxes.is_empty());
}

#[tokio::test]
async fn a_server_that_drops_labels_fails_the_labels_check_and_runs_the_rest() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().store_labels = false;
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    assert_eq!(report.checks.len(), CORE_CHECKS.len());
    let by_name = |name: &str| {
        report
            .checks
            .iter()
            .find(|check| check.name == name)
            .unwrap()
    };
    assert!(
        by_name("create_find_by_label")
            .outcome
            .as_ref()
            .is_err_and(|message| !message.is_empty())
    );
    assert_eq!(by_name("read_missing_file").outcome, Ok(()));
}
