// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{HostPath, ImageTag, ProjectRoot, SandboxSlug, StdFileSystem};
use jackin_openshell::GrpcGateway;
use jackin_openshell::contract::{
    CORE_CHECKS, ContractFixture, ContractGroup, ContractReport, TERMINAL_CHECKS, TRANSFER_CHECKS,
    run,
};
use openshell_sdk::raw::proto::{ObjectMeta, Provider, ProviderProfile, exec_sandbox_input};
use tempfile::TempDir;
use tonic::Code;

use super::exec_rpc::{exit_event, host_shell};
use super::fakes::{ScriptedRunner, exited};
use super::harness::{Env, WORKSPACE, config, connect, env, guard, resolved};
use super::server::{Broken, ExecBackend, ExecCtx, Fault};
use super::ssh_server::{
    Action, REFUSED_NOTICE, RIGHT_FINGERPRINT, RIGHT_SEED, Script, SshModel, host_key,
};

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

#[tokio::test]
async fn a_sandbox_that_never_reaches_ready_is_deleted_by_the_check() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().faults.insert(
        "get_sandbox",
        Fault::Status(Code::Unavailable, "gateway lost".to_owned()),
    );
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    for name in ["power_transitions", "touch_on_success", "delete_sandbox"] {
        let check = report
            .checks
            .iter()
            .find(|check| check.name == name)
            .unwrap();
        assert!(check.outcome.is_err(), "{name}: {:?}", check.outcome);
    }
    let state = env.server.state();
    let leaked: Vec<&String> = state.sandboxes.keys().map(|(_, name)| name).collect();
    assert!(leaked.is_empty(), "the contract run left {leaked:?}");
}

fn hang_after_exit() -> ExecBackend {
    Arc::new(|ctx: ExecCtx| {
        Box::pin(async move {
            let ExecCtx {
                mut inputs,
                events: _held_open,
                shared,
                ..
            } = ctx;
            let mut stdin = Vec::new();
            while let Some(payload) = inputs.recv().await {
                let exec_sandbox_input::Payload::Stdin(bytes) = payload else {
                    continue;
                };
                stdin.extend_from_slice(&bytes);
                if stdin.windows(6).any(|window| window == b"exit 3") {
                    shared.lock().observed.child_started = true;
                    shared.child_started.notify_one();
                    std::future::pending::<()>().await;
                }
            }
        })
    })
}

#[tokio::test]
async fn shell_exit_code_cannot_hang_the_run() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().exec = Some(hang_after_exit());
    let (_project, fixture) = fixture().await;
    let mut settings = config(&env, resolved(&env).await, WORKSPACE, env.runner.clone());
    settings.call_timeout = Duration::from_secs(300);
    let gateway = guard(GrpcGateway::connect(settings)).await.unwrap();
    let running =
        tokio::spawn(async move { run(&gateway, &fixture, ContractGroup::Terminal).await });
    guard(env.server.shared.child_started.notified()).await;
    tokio::time::pause();
    let finished = tokio::time::timeout(Duration::from_secs(120), running).await;
    tokio::time::resume();
    let report = finished
        .expect("the shell_exit_code check did not end")
        .unwrap();
    let check = report
        .checks
        .iter()
        .find(|check| check.name == "shell_exit_code")
        .unwrap();
    assert_eq!(check.outcome, Err("the shell never ended".to_owned()));
    assert!(env.server.state().sandboxes.is_empty());
}

fn outcome<'a>(report: &'a ContractReport, name: &str) -> &'a Result<(), String> {
    &report
        .checks
        .iter()
        .find(|check| check.name == name)
        .unwrap()
        .outcome
}

fn assert_others_pass(report: &ContractReport, except: &str) {
    for check in &report.checks {
        if check.name != except {
            assert_eq!(check.outcome, Ok(()), "{}", check.name);
        }
    }
}

fn assert_nothing_left(env: &Env) {
    let state = env.server.state();
    assert!(state.profiles.is_empty(), "a contract profile is left");
    assert!(state.providers.is_empty(), "a contract provider is left");
    assert!(state.sandboxes.is_empty(), "a contract sandbox is left");
}

#[tokio::test]
async fn a_server_that_refuses_stop_on_a_stopped_sandbox_fails_only_that_check() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().broken = Some(Broken::StopOnStopped);
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    assert_eq!(report.checks.len(), CORE_CHECKS.len());
    let failed = outcome(&report, "stop_on_stopped_is_ok");
    assert!(
        failed
            .as_ref()
            .is_err_and(|message| message.contains("must be Ready")),
        "{failed:?}"
    );
    assert_others_pass(&report, "stop_on_stopped_is_ok");
    assert_nothing_left(&env);
}

#[tokio::test]
async fn a_server_that_deletes_an_attached_provider_fails_only_that_check_and_leaves_nothing() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().broken = Some(Broken::DeleteAttachedProviders);
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    let failed = outcome(&report, "delete_provider_while_attached_is_refused");
    assert!(
        failed
            .as_ref()
            .is_err_and(|message| message.contains("not FailedPrecondition")),
        "{failed:?}"
    );
    assert_others_pass(&report, "delete_provider_while_attached_is_refused");
    assert_nothing_left(&env);
}

fn end_with(status: u32) -> Vec<Action> {
    vec![Action::ExitStatus(status), Action::Eof, Action::Close]
}

async fn terminal_report(after_subsystem: Vec<Action>) -> (Env, ContractReport) {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    let model = SshModel::new(
        &host_key(&RIGHT_SEED),
        RIGHT_FINGERPRINT,
        Script {
            after_subsystem,
            ..Script::default()
        },
    );
    {
        let mut state = env.server.state();
        state.exec = Some(exit_when_told());
        state.ssh = Some(model);
    }
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Terminal).await;
    (env, report)
}

#[tokio::test]
async fn an_ended_main_process_attach_that_is_refused_passes_the_check_and_one_that_grants_the_lease_fails_it()
 {
    let refusing = [vec![Action::Notice(REFUSED_NOTICE)], end_with(1)].concat();
    let (env, report) = terminal_report(refusing).await;
    let names: Vec<&str> = report.checks.iter().map(|check| check.name).collect();
    assert_eq!(names, TERMINAL_CHECKS.to_vec());
    assert_eq!(outcome(&report, "main_ended_is_refused"), &Ok(()));
    assert!(env.server.state().sandboxes.is_empty());

    let (env, report) = terminal_report(end_with(1)).await;
    let failed = outcome(&report, "main_ended_is_refused");
    assert!(
        failed.as_ref().is_err_and(
            |message| message.contains("is not refused") && message.contains("Lease(Owner)")
        ),
        "{failed:?}"
    );
    assert!(env.server.state().sandboxes.is_empty());
}

async fn transfer_report(runner: ScriptedRunner) -> (Env, ContractReport) {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    let (_project, fixture) = fixture().await;
    let settings = config(&env, resolved(&env).await, WORKSPACE, runner);
    let gateway = guard(GrpcGateway::connect(settings)).await.unwrap();
    let report = run(&gateway, &fixture, ContractGroup::Transfer).await;
    (env, report)
}

#[tokio::test]
async fn an_upload_to_an_absent_sandbox_passes_the_check_when_the_cli_fails_and_fails_it_when_the_cli_succeeds()
 {
    let failing = ScriptedRunner::always(exited(1, b"", b"Error: sandbox not found"));
    let (env, report) = transfer_report(failing).await;
    let names: Vec<&str> = report.checks.iter().map(|check| check.name).collect();
    assert_eq!(names, TRANSFER_CHECKS.to_vec());
    assert_eq!(
        outcome(&report, "upload_absent_sandbox_is_a_transfer_error"),
        &Ok(())
    );
    assert!(env.server.state().sandboxes.is_empty());

    let passing = ScriptedRunner::always(exited(0, b"", b""));
    let (env, report) = transfer_report(passing).await;
    let failed = outcome(&report, "upload_absent_sandbox_is_a_transfer_error");
    assert!(
        failed
            .as_ref()
            .is_err_and(|message| message.contains("is not a Transfer error")),
        "{failed:?}"
    );
    assert!(env.server.state().sandboxes.is_empty());
}
