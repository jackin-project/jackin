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

use super::exec_rpc::{Step, exit_event, host_shell, host_tools, scripted};
use super::fakes::{Recorded, ScriptedRunner, exited};
use super::harness::{Env, WORKSPACE, config, connect, drive_paused, env, guard, resolved};
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
    assert_nothing_left(&env);
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

#[track_caller]
fn assert_no_sandboxes(env: &Env, label: &str) {
    let left: Vec<String> = env
        .server
        .state()
        .sandboxes
        .keys()
        .map(|(_, name)| name.clone())
        .collect();
    assert!(left.is_empty(), "{label}: the run left sandboxes {left:?}");
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
    assert_no_sandboxes(&env, "a refused attach");
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
async fn a_server_that_drops_the_environment_fails_only_the_environment_check() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().store_environment = false;
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    let failed: Vec<&str> = report
        .checks
        .iter()
        .filter(|check| check.outcome.is_err())
        .map(|check| check.name)
        .collect();
    assert_eq!(failed, ["sandbox_environment_is_the_create_request"]);
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
    let report = drive_paused(running, Duration::from_secs(120))
        .await
        .expect("the shell_exit_code check did not end");
    let check = report
        .checks
        .iter()
        .find(|check| check.name == "shell_exit_code")
        .unwrap();
    assert_eq!(check.outcome, Err("the shell never ended".to_owned()));
    assert_no_sandboxes(&env, "a shell that never ends");
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
    {
        let state = env.server.state();
        assert!(state.profiles.is_empty(), "a contract profile is left");
        assert!(state.providers.is_empty(), "a contract provider is left");
    }
    assert_no_sandboxes(env, "the contract run");
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

#[tokio::test]
async fn a_server_that_deletes_a_provider_while_refusing_the_delete_fails_only_the_presence_assertion()
 {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().broken = Some(Broken::DeleteAfterRefusal);
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    let failed = outcome(&report, "delete_provider_while_attached_is_refused");
    assert!(
        failed
            .as_ref()
            .is_err_and(|message| message.contains("the provider is absent after a refused delete")),
        "{failed:?}"
    );
    assert_others_pass(&report, "delete_provider_while_attached_is_refused");
    assert_nothing_left(&env);
}

#[tokio::test]
async fn a_server_that_refuses_the_second_delete_of_a_provider_fails_the_put_and_delete_check() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().broken = Some(Broken::DeleteAbsentRefused);
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    let failed = outcome(&report, "provider_put_and_delete");
    assert!(
        failed
            .as_ref()
            .is_err_and(|message| message.contains("the second delete_provider failed")),
        "{failed:?}"
    );
    for check in report
        .checks
        .iter()
        .take_while(|check| check.name != "provider_put_and_delete")
    {
        assert_eq!(check.outcome, Ok(()), "{}", check.name);
    }
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
    assert_no_sandboxes(&env, "a refused main-ended attach");

    let (env, report) = terminal_report(end_with(1)).await;
    let failed = outcome(&report, "main_ended_is_refused");
    assert!(
        failed.as_ref().is_err_and(
            |message| message.contains("is not refused") && message.contains("Lease(Owner)")
        ),
        "{failed:?}"
    );
    assert_no_sandboxes(&env, "a granted main-ended attach");
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
    assert_no_sandboxes(&env, "a failing CLI on an absent sandbox");

    let passing = ScriptedRunner::always(exited(0, b"", b""));
    let (env, report) = transfer_report(passing).await;
    let failed = outcome(&report, "upload_absent_sandbox_is_a_transfer_error");
    assert!(
        failed
            .as_ref()
            .is_err_and(|message| message.contains("is not a Transfer error")),
        "{failed:?}"
    );
    assert_no_sandboxes(&env, "a succeeding CLI on an absent sandbox");

    let wrong_status = ScriptedRunner::always(exited(2, b"", b"Error: sandbox not found"));
    let (env, report) = transfer_report(wrong_status).await;
    let failed = outcome(&report, "upload_absent_sandbox_is_a_transfer_error");
    assert!(
        failed
            .as_ref()
            .is_err_and(|message| message.contains("an absent sandbox")),
        "the CLI exits 2 saying the sandbox is not found: {failed:?}"
    );
    assert_no_sandboxes(&env, "a CLI that exits 2 on an absent sandbox");
}

#[tokio::test]
async fn an_upload_to_a_stopped_sandbox_passes_the_check_only_when_the_cli_refuses_it_as_not_ready()
{
    const CHECK: &str = "upload_stopped_sandbox_is_a_transfer_error";
    let refusing = ScriptedRunner::always(exited(1, b"", b"Error: sandbox is not ready"));
    let (env, report) = transfer_report(refusing).await;
    assert_eq!(outcome(&report, CHECK), &Ok(()));
    let absent = outcome(&report, "upload_absent_sandbox_is_a_transfer_error");
    assert!(
        absent
            .as_ref()
            .is_err_and(|message| message.contains("is not a Transfer error")),
        "{absent:?}"
    );
    assert_no_sandboxes(&env, "a refusing CLI on a Stopped sandbox");

    for (label, runner) in [
        (
            "the CLI says the sandbox is absent",
            ScriptedRunner::always(exited(1, b"", b"Error: sandbox not found")),
        ),
        (
            "the CLI exits 2 saying the sandbox is not ready",
            ScriptedRunner::always(exited(2, b"", b"Error: sandbox is not ready")),
        ),
        (
            "the CLI succeeds",
            ScriptedRunner::always(exited(0, b"", b"")),
        ),
        (
            "the CLI fails without saying why",
            ScriptedRunner::always(exited(1, b"", b"Error: boom")),
        ),
        (
            "the CLI says both found and ready",
            ScriptedRunner::always(exited(1, b"", b"Error: sandbox not found or not ready")),
        ),
    ] {
        let (env, report) = transfer_report(runner).await;
        let failed = outcome(&report, CHECK);
        assert!(
            failed
                .as_ref()
                .is_err_and(|message| message.contains("a Stopped sandbox")),
            "{label}: {failed:?}"
        );
        assert_no_sandboxes(&env, label);
    }
}

#[tokio::test]
async fn an_upload_through_a_symlinked_parent_passes_the_check_only_when_the_cli_exits_1_with_the_tar_text()
 {
    const CHECK: &str = "upload_through_a_symlinked_parent_is_a_transfer_error";
    const TAR: &[u8] = b"Error: sandbox upload failed: extraction exited with status 2: tar: d/f.txt: Cannot open: Not a directory\ntar: Exiting with failure status due to previous errors";
    let failing = ScriptedRunner::always(exited(1, b"", TAR));
    let (env, report) = transfer_report(failing).await;
    let names: Vec<&str> = report.checks.iter().map(|check| check.name).collect();
    assert_eq!(names, TRANSFER_CHECKS.to_vec());
    assert_eq!(outcome(&report, CHECK), &Ok(()));
    assert_no_sandboxes(&env, "a failing CLI on a symlinked parent");

    for (label, runner) in [
        (
            "the CLI copies the file behind the link and exits 0",
            ScriptedRunner::always(exited(0, b"", b"")),
        ),
        (
            "the CLI exits 2 with the tar text",
            ScriptedRunner::always(exited(2, b"", TAR)),
        ),
        (
            "the CLI exits 1 without the tar text",
            ScriptedRunner::always(exited(1, b"", b"Error: boom")),
        ),
        (
            "the CLI refuses as not ready",
            ScriptedRunner::always(exited(1, b"", b"Error: sandbox is not ready")),
        ),
    ] {
        let (env, report) = transfer_report(runner).await;
        let failed = outcome(&report, CHECK);
        assert!(
            failed
                .as_ref()
                .is_err_and(|message| message.contains("a symlinked parent")),
            "{label}: {failed:?}"
        );
        assert_no_sandboxes(&env, label);
    }
}

fn sandbox_download(root: std::path::PathBuf) -> ScriptedRunner {
    ScriptedRunner::new(move |call: &Recorded| {
        let Some(at) = call.args.iter().position(|arg| arg == "download") else {
            return Ok(exited(1, b"", b"Error: only downloads are emulated"));
        };
        let remote = call.args[at + 2].trim_start_matches("/sandbox/");
        let target = call.args[at + 3].clone();
        match std::fs::copy(root.join("sandbox").join(remote), target) {
            Ok(_) => Ok(exited(0, b"", b"")),
            Err(error) => Ok(exited(1, b"", error.to_string().as_bytes())),
        }
    })
}

#[tokio::test]
async fn the_download_checks_pass_on_the_adapter_when_the_sandbox_answers_like_the_real_tools() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().exec = Some(host_tools(root.path().to_path_buf()));
    let (_project, fixture) = fixture().await;
    let settings = config(
        &env,
        resolved(&env).await,
        WORKSPACE,
        sandbox_download(root.path().to_path_buf()),
    );
    let gateway = guard(GrpcGateway::connect(settings)).await.unwrap();
    let report = run(&gateway, &fixture, ContractGroup::Transfer).await;
    let names: Vec<&str> = report.checks.iter().map(|check| check.name).collect();
    assert_eq!(names, TRANSFER_CHECKS.to_vec());
    for name in [
        "download_with_digest",
        "download_missing_remote_is_a_size_error",
        "download_pre_cancelled_is_cancelled",
    ] {
        assert_eq!(outcome(&report, name), &Ok(()), "{name}");
    }
    assert_no_sandboxes(&env, "the download checks");
}

#[tokio::test]
async fn a_gateway_that_finds_a_missing_file_and_fails_later_fails_the_size_check() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().exec = Some(scripted(|start, _| {
        if start.command[0] == "/usr/bin/stat" {
            vec![Step::Out(b"1\n".to_vec()), Step::Exit(0)]
        } else {
            vec![Step::Err(b"no such file".to_vec()), Step::Exit(1)]
        }
    }));
    let (_project, fixture) = fixture().await;
    let succeeding = ScriptedRunner::always(exited(0, b"", b""));
    let settings = config(&env, resolved(&env).await, WORKSPACE, succeeding);
    let gateway = guard(GrpcGateway::connect(settings)).await.unwrap();
    let report = run(&gateway, &fixture, ContractGroup::Transfer).await;
    let missing = outcome(&report, "download_missing_remote_is_a_size_error");
    assert!(
        missing.as_ref().is_err_and(
            |message| message.contains("not a Size error") && message.contains("Digest")
        ),
        "{missing:?}"
    );
}
