// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{FileMode, LabelSelector, SandboxPath, SecretBytes};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, PowerAction, SandboxFile, SandboxQuery};
use openshell_sdk::raw::proto::SandboxPhase;
use tempfile::TempDir;
use tokio::sync::{Barrier, Notify};
use tokio_util::sync::CancellationToken;

use super::exec_rpc::{host_shell, host_shell_gated};
use super::harness::{WORKSPACE, connect, env, guard};
use super::server::{ExecBackend, ExecCtx, Fault};
use super::support::{SANDBOX, add_sandbox, sandbox_name};

fn path(text: &str) -> SandboxPath {
    SandboxPath::parse(text).unwrap()
}

fn held_exec(started: Arc<Notify>) -> ExecBackend {
    Arc::new(move |ctx: ExecCtx| {
        let started = Arc::clone(&started);
        Box::pin(async move {
            started.notify_one();
            ctx.events.closed().await;
        })
    })
}

#[tokio::test]
async fn cancelling_a_find_cancels_its_in_flight_unary_rpc() {
    let env = env().await;
    env.server
        .state()
        .faults
        .insert("list_sandboxes", Fault::Hang);
    let gateway = Arc::new(connect(&env, WORKSPACE).await);
    let cancel = CancellationToken::new();
    let arrived = env.server.shared.arrived.notified();
    let request = {
        let gateway = Arc::clone(&gateway);
        let cancel = cancel.clone();
        tokio::spawn(async move {
            gateway
                .find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed()), &cancel)
                .await
        })
    };
    guard(arrived).await;
    cancel.cancel();
    let error = guard(request).await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Cancelled {
            op: GatewayOp::Find,
            ..
        }
    ));
}

#[tokio::test]
async fn cancelling_power_before_or_during_the_transition_leaves_the_sandbox_ready() {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    env.server
        .state()
        .faults
        .insert("stop_sandbox", Fault::Hang);
    let gateway = Arc::new(connect(&env, WORKSPACE).await);
    let name = sandbox_name(SANDBOX);
    let before = CancellationToken::new();
    before.cancel();
    let error = gateway
        .set_power(&name, PowerAction::Stop, &before)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Cancelled {
            op: GatewayOp::Power,
            ..
        }
    ));
    assert_eq!(
        env.server.state().sandboxes[&(WORKSPACE.to_owned(), SANDBOX.to_owned())]
            .status
            .as_ref()
            .unwrap()
            .phase,
        SandboxPhase::Ready as i32
    );

    let cancel = CancellationToken::new();
    let arrived = env.server.shared.arrived.notified();
    let request = {
        let gateway = Arc::clone(&gateway);
        let cancel = cancel.clone();
        tokio::spawn(async move { gateway.set_power(&name, PowerAction::Stop, &cancel).await })
    };
    guard(arrived).await;
    cancel.cancel();
    let error = guard(request).await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Cancelled {
            op: GatewayOp::Power,
            ..
        }
    ));
    assert_eq!(
        env.server.state().sandboxes[&(WORKSPACE.to_owned(), SANDBOX.to_owned())]
            .status
            .as_ref()
            .unwrap()
            .phase,
        SandboxPhase::Ready as i32
    );
}

#[tokio::test]
async fn pre_cancelled_file_operations_do_not_start_a_write_or_read() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    let exec = host_shell(root.path().to_path_buf()).await;
    env.server.state().exec = Some(exec);
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let gateway = connect(&env, WORKSPACE).await;
    let target = root.path().join("sandbox/file.txt");
    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&target, b"old").await.unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let name = sandbox_name(SANDBOX);
    let read = gateway
        .read_files(&name, &[path("/sandbox/file.txt")], &cancel)
        .await;
    assert!(matches!(
        read,
        Err(GatewayError::Cancelled {
            op: GatewayOp::ReadFiles,
            ..
        })
    ));
    let write = gateway
        .write_file(
            &name,
            &SandboxFile {
                path: path("/sandbox/file.txt"),
                mode: FileMode(0o600),
                content: SecretBytes::new(b"new".to_vec()),
            },
            &cancel,
        )
        .await;
    assert!(matches!(
        write,
        Err(GatewayError::Cancelled {
            op: GatewayOp::WriteFile,
            ..
        })
    ));
    assert_eq!(tokio::fs::read(target).await.unwrap(), b"old");
}

#[tokio::test]
async fn cancelling_an_active_read_stream_returns_cancelled() {
    let env = env().await;
    let started = Arc::new(Notify::new());
    env.server.state().exec = Some(held_exec(Arc::clone(&started)));
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let gateway = Arc::new(connect(&env, WORKSPACE).await);
    let cancel = CancellationToken::new();
    let observed = started.notified();
    let name = sandbox_name(SANDBOX);
    let request = {
        let gateway = Arc::clone(&gateway);
        let cancel = cancel.clone();
        tokio::spawn(async move {
            gateway
                .read_files(&name, &[path("/sandbox/file.txt")], &cancel)
                .await
        })
    };
    guard(observed).await;
    cancel.cancel();
    let error = guard(request).await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Cancelled {
            op: GatewayOp::ReadFiles,
            ..
        }
    ));
}

#[tokio::test]
async fn cancelling_an_active_write_stream_returns_cancelled() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    let gate = Arc::new(Barrier::new(2));
    let exec = host_shell_gated(root.path().to_path_buf(), 1, Arc::clone(&gate)).await;
    env.server.state().exec = Some(exec);
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let gateway = Arc::new(connect(&env, WORKSPACE).await);
    let cancel = CancellationToken::new();
    let name = sandbox_name(SANDBOX);
    let file = SandboxFile {
        path: path("/sandbox/file.txt"),
        mode: FileMode(0o600),
        content: SecretBytes::new(vec![b'x'; 64 * 1024]),
    };
    let request = {
        let gateway = Arc::clone(&gateway);
        let cancel = cancel.clone();
        tokio::spawn(async move { gateway.write_file(&name, &file, &cancel).await })
    };
    guard(gate.wait()).await;
    cancel.cancel();
    let error = guard(request).await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Cancelled {
            op: GatewayOp::WriteFile,
            ..
        }
    ));
}
