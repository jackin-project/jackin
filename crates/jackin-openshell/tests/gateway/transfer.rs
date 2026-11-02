// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_core::{
    HostPath, ProcessError, ProcessStatus, Program, ProjectRoot, SandboxPath, Sha256, StdFileSystem,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, ProjectUpload, TransferOp};
use openshell_sdk::raw::proto::SandboxPhase;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::exec_rpc::{Step, scripted};
use super::fakes::{Recorded, ScriptedRunner, exited};
use super::harness::{Env, config, env, guard, resolved};
use super::support::{add_sandbox, sandbox_name};

const TRANSFER_WORKSPACE: &str = "jackin-ws";
const TRANSFER_SANDBOX: &str = "jackin-ws-ab12";

async fn gateway_with(env: &Env, runner: ScriptedRunner) -> jackin_openshell::GrpcGateway {
    let resolved = resolved(env).await;
    guard(jackin_openshell::GrpcGateway::connect(config(
        env,
        resolved,
        TRANSFER_WORKSPACE,
        runner,
    )))
    .await
    .unwrap()
}

async fn project() -> (TempDir, ProjectRoot) {
    let dir = TempDir::new().unwrap();
    let status = tokio::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dir.path())
        .status()
        .await
        .unwrap();
    assert!(status.success());
    let path = HostPath::parse(dir.path()).unwrap();
    let root = ProjectRoot::open(&StdFileSystem::default(), &path)
        .await
        .unwrap();
    (dir, root)
}

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn upload_tail(tail: &[&'static str]) -> Vec<String> {
    let mut args = vec!["sandbox", "upload", TRANSFER_SANDBOX];
    args.extend_from_slice(tail);
    words(&args)
}

fn assert_scoped(call: &Recorded, endpoint: &str, tail: &[String]) {
    let position = call
        .args
        .iter()
        .position(|arg| arg == "sandbox")
        .expect("a sandbox subcommand");
    let scope: BTreeSet<(&str, &str)> = call.args[..position]
        .chunks(2)
        .map(|pair| (pair[0].as_str(), pair[1].as_str()))
        .collect();
    assert_eq!(call.args[..position].len(), 6, "{:?}", call.args);
    assert_eq!(
        scope,
        BTreeSet::from([
            ("-g", "jackin-local"),
            ("--gateway-endpoint", endpoint),
            ("--workspace", TRANSFER_WORKSPACE),
        ])
    );
    assert_eq!(&call.args[position..], tail);
    call.assert_openshell_env_isolated();
}

#[tokio::test]
async fn upload_runs_two_openshell_calls_naming_gateway_workspace_and_sandbox() {
    let env = env().await;
    let (dir, root) = project().await;
    let runner = ScriptedRunner::always(exited(0, b"", b""));
    let gateway = gateway_with(&env, runner.clone()).await;
    let upload = ProjectUpload {
        source: root.clone(),
        dest: SandboxPath::parse("/sandbox/project").unwrap(),
    };
    guard(gateway.upload_project(
        &sandbox_name(TRANSFER_SANDBOX),
        &upload,
        &CancellationToken::new(),
    ))
    .await
    .unwrap();
    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    for call in &calls {
        assert_eq!(call.program, Program::Openshell);
        assert_eq!(call.cwd.as_ref(), Some(root.path()));
        assert_eq!(call.cwd.as_ref().unwrap().as_path(), dir.path());
        assert!(call.stdin_null);
        assert_eq!(call.timeout, Duration::from_secs(123));
    }
    let endpoint = env.server.endpoint();
    assert_scoped(
        &calls[0],
        &endpoint,
        &upload_tail(&[".", "/sandbox/project"]),
    );
    assert_scoped(
        &calls[1],
        &endpoint,
        &upload_tail(&[".git", "/sandbox/project", "--no-git-ignore"]),
    );
}

#[tokio::test]
async fn upload_failures_name_the_failed_step_and_stop() {
    let env = env().await;
    let (_dir, root) = project().await;
    let upload = ProjectUpload {
        source: root,
        dest: SandboxPath::parse("/sandbox/project").unwrap(),
    };
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();

    let runner = ScriptedRunner::always(exited(1, b"", b"\x1b[31mfatal\x1b[0m"));
    let gateway = gateway_with(&env, runner.clone()).await;
    let error = guard(gateway.upload_project(&name, &upload, &cancel))
        .await
        .unwrap_err();
    match error {
        GatewayError::Transfer { op, status, tail } => {
            assert_eq!(
                (op, status),
                (TransferOp::UploadFiltered, ProcessStatus::Exited(1))
            );
            assert!(!tail.as_str().contains('\u{1b}') && tail.as_str().contains("fatal"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(runner.calls().len(), 1);

    let counter = Arc::new(AtomicUsize::new(0));
    let runner = ScriptedRunner::new(move |_| {
        Ok(exited(
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                0
            } else {
                2
            },
            b"",
            b"boom",
        ))
    });
    let gateway = gateway_with(&env, runner).await;
    let error = guard(gateway.upload_project(&name, &upload, &cancel))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Transfer {
            op: TransferOp::UploadGit,
            status: ProcessStatus::Exited(2),
            ..
        }
    ));

    let after = Duration::from_secs(9);
    for (reply, check) in [
        (
            ProcessError::Timeout {
                program: Program::Openshell,
                after,
            },
            0_u8,
        ),
        (
            ProcessError::Cancelled {
                program: Program::Openshell,
            },
            1,
        ),
        (
            ProcessError::Spawn {
                program: Program::Openshell,
                message: "missing".to_owned(),
            },
            2,
        ),
    ] {
        let reply = Arc::new(Mutex::new(Some(reply)));
        let runner = ScriptedRunner::new(move |_| Err(reply.lock().unwrap().take().unwrap()));
        let gateway = gateway_with(&env, runner).await;
        let error = guard(gateway.upload_project(&name, &upload, &cancel))
            .await
            .unwrap_err();
        match (check, error) {
            (
                0,
                GatewayError::Timeout {
                    op: GatewayOp::Upload,
                    after: got,
                },
            ) => assert_eq!(got, after),
            (
                1,
                GatewayError::Cancelled {
                    op: GatewayOp::Upload,
                },
            )
            | (2, GatewayError::Process(ProcessError::Spawn { .. })) => {}
            (_, other) => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn download_names_the_targets_and_verifies_the_digest() {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        TRANSFER_WORKSPACE,
        TRANSFER_SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let bytes = b"bundle bytes".to_vec();
    let digest = Sha256::of(&bytes);
    let remote_hex = Arc::new(Mutex::new(digest.as_str().to_owned()));
    let remote_exit = Arc::new(AtomicUsize::new(0));
    let (hex, exit) = (Arc::clone(&remote_hex), Arc::clone(&remote_exit));
    env.server.state().exec = Some(scripted(move |_, _| {
        if exit.load(Ordering::SeqCst) == 0 {
            vec![
                Step::Out(
                    format!("{}  /sandbox/.jackin/export.bundle\n", hex.lock().unwrap())
                        .into_bytes(),
                ),
                Step::Exit(0),
            ]
        } else {
            vec![Step::Err(b"no such file".to_vec()), Step::Exit(1)]
        }
    }));
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("export.bundle");
    let local = HostPath::parse(&destination).unwrap();
    let write_bytes = bytes.clone();
    let cli_exit = Arc::new(AtomicUsize::new(0));
    let cli = Arc::clone(&cli_exit);
    let runner = ScriptedRunner::new(move |call: &Recorded| {
        if cli.load(Ordering::SeqCst) != 0 {
            return Ok(exited(1, b"", b"download failed"));
        }
        std::fs::write(call.args.last().unwrap(), &write_bytes).unwrap();
        Ok(exited(0, b"", b""))
    });
    let gateway = gateway_with(&env, runner.clone()).await;
    let remote = SandboxPath::parse("/sandbox/.jackin/export.bundle").unwrap();
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();

    guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap();
    let call = runner.calls().remove(0);
    let mut expected = words(&[
        "sandbox",
        "download",
        TRANSFER_SANDBOX,
        "/sandbox/.jackin/export.bundle",
    ]);
    expected.push(destination.display().to_string());
    assert_scoped(&call, &env.server.endpoint(), &expected);
    assert!(call.stdin_null && call.cwd.is_none());
    assert_eq!(call.timeout, Duration::from_secs(123));

    "0".repeat(64).clone_into(&mut remote_hex.lock().unwrap());
    let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Digest,
                ..
            }
        ),
        "{error:?}"
    );

    digest.as_str().clone_into(&mut remote_hex.lock().unwrap());
    remote_exit.store(1, Ordering::SeqCst);
    let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Digest,
                ..
            }
        ),
        "{error:?}"
    );

    remote_exit.store(0, Ordering::SeqCst);
    cli_exit.store(1, Ordering::SeqCst);
    let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::Download,
                ..
            }
        ),
        "{error:?}"
    );
}
