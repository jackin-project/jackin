// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::HostPath;
use jackin_core::{
    LineSink, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, Program, SandboxPath,
    SecretOutput,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use openshell_sdk::raw::proto::SandboxPhase;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::exec_rpc::{Step, scripted};
use super::fakes::exited;
use super::harness::{env, guard};
use super::support::{add_sandbox, sandbox_name};
use super::transfer::{
    FOUR_GIB, STAT, TRANSFER_SANDBOX, TRANSFER_WORKSPACE, entries, gateway_with,
};

const STEP: u64 = 256 * 1024 * 1024;
const STEPS: u64 = 128;

#[derive(Clone, Debug)]
struct GrowingCli {
    written: Arc<AtomicU64>,
}

#[async_trait]
impl ProcessRunner for GrowingCli {
    async fn run(
        &self,
        spec: &ProcessSpec,
        _lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let target = Path::new(spec.args.last().unwrap());
        let staging = target.parent().unwrap().join(".tmpFAKE");
        std::fs::create_dir(&staging).unwrap();
        let staged = staging.join(target.file_name().unwrap());
        let file = std::fs::File::create(&staged).unwrap();
        for step in 1..=STEPS {
            if cancel.is_cancelled() {
                return Err(ProcessError::Cancelled {
                    program: Program::Openshell,
                    group: None,
                });
            }
            file.set_len(step * STEP).unwrap();
            self.written.store(step * STEP, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        std::fs::rename(&staged, target).unwrap();
        std::fs::remove_dir(&staging).unwrap();
        Ok(exited(0, b"", b""))
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        _cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        Err(ProcessError::Spawn {
            program: spec.program,
            message: "secret runs are not scripted".to_owned(),
        })
    }
}

#[tokio::test]
async fn a_file_that_grows_after_the_size_check_is_stopped_while_it_streams_and_leaves_nothing() {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        TRANSFER_WORKSPACE,
        TRANSFER_SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    env.server.state().exec = Some(scripted(|start, _| {
        assert_eq!(start.command[0], STAT);
        vec![Step::Out(b"regular file 12\n".to_vec()), Step::Exit(0)]
    }));
    let written = Arc::new(AtomicU64::new(0));
    let runner = GrowingCli {
        written: Arc::clone(&written),
    };
    let gateway = gateway_with(&env, runner).await;
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("out.bundle");
    std::fs::write(&destination, b"stale").unwrap();
    let local = HostPath::parse(&destination).unwrap();
    let remote = SandboxPath::parse("/sandbox/.jackin/export.bundle").unwrap();
    let name = sandbox_name(TRANSFER_SANDBOX);

    let error = guard(gateway.download_file(&name, &remote, &local, &CancellationToken::new()))
        .await
        .unwrap_err();

    let stopped_at = written.load(Ordering::SeqCst);
    assert!(
        matches!(&error, GatewayError::DownloadTooLarge { size, limit } if *size > FOUR_GIB && *limit == FOUR_GIB),
        "{error:?}"
    );
    assert!(
        stopped_at < STEP * STEPS / 2,
        "the CLI ran on to {stopped_at} bytes"
    );
    assert_eq!(std::fs::read(&destination).unwrap(), b"stale");
    assert_eq!(entries(dir.path()), ["out.bundle"]);
}

#[tokio::test]
async fn an_operator_cancel_while_the_file_streams_stops_the_cli_and_removes_the_staging_directory()
{
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        TRANSFER_WORKSPACE,
        TRANSFER_SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    env.server.state().exec = Some(scripted(|start, _| {
        assert_eq!(start.command[0], STAT);
        vec![Step::Out(b"regular file 12\n".to_vec()), Step::Exit(0)]
    }));
    let written = Arc::new(AtomicU64::new(0));
    let runner = GrowingCli {
        written: Arc::clone(&written),
    };
    let gateway = gateway_with(&env, runner).await;
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("out.bundle");
    std::fs::write(&destination, b"stale").unwrap();
    let local = HostPath::parse(&destination).unwrap();
    let remote = SandboxPath::parse("/sandbox/.jackin/export.bundle").unwrap();
    let name = sandbox_name(TRANSFER_SANDBOX);
    let cancel = CancellationToken::new();
    let operator = {
        let written = Arc::clone(&written);
        let cancel = cancel.clone();
        tokio::spawn(async move {
            while written.load(Ordering::SeqCst) < STEP {
                tokio::task::yield_now().await;
            }
            cancel.cancel();
        })
    };

    let error = guard(gateway.download_file(&name, &remote, &local, &cancel))
        .await
        .unwrap_err();

    operator.await.unwrap();
    assert!(
        matches!(
            &error,
            GatewayError::Cancelled {
                op: GatewayOp::Download,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(written.load(Ordering::SeqCst) < FOUR_GIB, "{error:?}");
    assert_eq!(std::fs::read(&destination).unwrap(), b"stale");
    assert_eq!(entries(dir.path()), ["out.bundle"]);
}
