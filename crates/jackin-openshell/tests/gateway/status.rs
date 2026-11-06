// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{FileMode, Phase, SandboxPath, SecretBytes};
use jackin_core::{OutputStream, ProcessError, ProcessStatus, Program, TerminalText};
use jackin_openshell::{
    Gateway, GatewayError, GatewayOp, Missing, ProgramRun, ProgramStdin, SandboxFile, TransferOp,
};
use openshell_sdk::raw::proto::SandboxPhase;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use tonic::{Code, Status};

use super::exec_rpc::host_shell;
use super::harness::{WORKSPACE, config, connect, env, guard, resolved};
use super::server::{ExecCtx, Fault, invalid_field};
use super::support::{
    SANDBOX, SHORT, add_sandbox, create_spec, kind, main_terminal, sandbox_name, shell_terminal,
};

const CODE_TABLE: [(Code, &str); 14] = [
    (Code::Cancelled, "Protocol"),
    (Code::Unknown, "Protocol"),
    (Code::OutOfRange, "Protocol"),
    (Code::Unimplemented, "Protocol"),
    (Code::Internal, "Protocol"),
    (Code::DataLoss, "Protocol"),
    (Code::InvalidArgument, "InvalidArgument"),
    (Code::NotFound, "NotFound"),
    (Code::PermissionDenied, "PermissionDenied"),
    (Code::Unauthenticated, "Unauthenticated"),
    (Code::ResourceExhausted, "ResourceExhausted"),
    (Code::Aborted, "FailedPrecondition"),
    (Code::Unavailable, "Unreachable"),
    (Code::FailedPrecondition, "FailedPrecondition"),
];

#[tokio::test]
async fn every_grpc_code_maps_to_its_gateway_error() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let name = sandbox_name(SANDBOX);
    for (code, expected) in CODE_TABLE {
        env.server
            .state()
            .faults
            .insert("get_sandbox", Fault::Status(code, "wire".to_owned()));
        let error = guard(gateway.sandbox_detail(&name)).await.unwrap_err();
        assert_eq!(kind(&error), expected, "{code:?}");
        if code == Code::NotFound {
            assert!(matches!(
                error,
                GatewayError::NotFound { what: Missing::Sandbox(ref missing) } if *missing == name
            ));
        }
    }
    env.server.state().faults.insert(
        "create_sandbox",
        Fault::Status(Code::AlreadyExists, "exists".to_owned()),
    );
    let error = guard(gateway.create_sandbox(&create_spec(SANDBOX, "01hzy3k5m7p9r2t4v6w8x0abcd")))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::AlreadyExists { what: Missing::Sandbox(ref missing) } if *missing == name
    ));
}

fn run(name: &str) -> ProgramRun {
    ProgramRun {
        sandbox: sandbox_name(name),
        program: SandboxPath::parse("/usr/bin/true").unwrap(),
        args: Vec::new(),
        workdir: None,
        env: std::collections::BTreeMap::new(),
        stdin: ProgramStdin::Closed,
        touch_on_success: None,
        timeout: Duration::from_secs(30),
    }
}

const GONE: &str = "jackin-ws-gone";

fn not_found_named(error: &GatewayError, name: &str) -> bool {
    matches!(error, GatewayError::NotFound { what: Missing::Sandbox(missing) } if missing.as_str() == name)
}

fn file(path: &str) -> SandboxFile {
    SandboxFile {
        path: SandboxPath::parse(path).unwrap(),
        mode: FileMode(0o600),
        content: SecretBytes::new(b"x".to_vec()),
    }
}

#[tokio::test]
async fn absent_sandbox_is_not_found_on_every_exec_entry_point() {
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
    let cancel = CancellationToken::new();
    let gone = sandbox_name(GONE);
    let control = sandbox_name(SANDBOX);
    let probe = SandboxPath::parse("/sandbox/probe").unwrap();

    let error = guard(gateway.run_program(&run(GONE), &cancel))
        .await
        .unwrap_err();
    assert!(not_found_named(&error, GONE), "run_program {error:?}");
    let error = guard(gateway.read_files(
        &gone,
        std::slice::from_ref(&probe),
        &CancellationToken::new(),
    ))
    .await
    .unwrap_err();
    assert!(not_found_named(&error, GONE), "read_files {error:?}");
    let error =
        guard(gateway.write_file(&gone, &file("/sandbox/probe"), &CancellationToken::new()))
            .await
            .unwrap_err();
    assert!(not_found_named(&error, GONE), "write_file {error:?}");
    let error = guard(gateway.open_terminal(&shell_terminal(GONE)))
        .await
        .unwrap_err();
    assert!(not_found_named(&error, GONE), "open_terminal {error:?}");

    let ok = ProgramRun {
        program: SandboxPath::parse("/bin/sh").unwrap(),
        args: vec!["-c".to_owned(), "exit 0".to_owned()],
        ..run(SANDBOX)
    };
    assert_eq!(
        guard(gateway.run_program(&ok, &cancel))
            .await
            .unwrap()
            .exit_code,
        0
    );
    assert_eq!(
        guard(gateway.read_files(
            &control,
            std::slice::from_ref(&probe),
            &CancellationToken::new()
        ))
        .await
        .unwrap(),
        vec![None]
    );
    guard(gateway.write_file(&control, &file("/sandbox/probe"), &CancellationToken::new()))
        .await
        .unwrap();

    env.server.state().exec = Some(Arc::new(|ctx: ExecCtx| {
        Box::pin(async move {
            drop(
                ctx.events
                    .send(Err(Status::not_found("sandbox not found")))
                    .await,
            );
        })
    }));
    let error = guard(gateway.run_program(&ok, &cancel)).await.unwrap_err();
    assert!(not_found_named(&error, SANDBOX), "run_program {error:?}");
    let error = guard(gateway.read_files(
        &control,
        std::slice::from_ref(&probe),
        &CancellationToken::new(),
    ))
    .await
    .unwrap_err();
    assert!(not_found_named(&error, SANDBOX), "read_files {error:?}");
    let error =
        guard(gateway.write_file(&control, &file("/sandbox/probe"), &CancellationToken::new()))
            .await
            .unwrap_err();
    assert!(not_found_named(&error, SANDBOX), "write_file {error:?}");
}

#[tokio::test]
async fn supervisor_relay_failure_names_the_sandbox_and_stays_retryable() {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let gateway = connect(&env, WORKSPACE).await;
    let endpoint = env.server.endpoint();
    let cancel = CancellationToken::new();
    for (method, message, named) in [
        (
            "exec_sandbox_interactive",
            "supervisor relay failed: no supervisor session",
            true,
        ),
        (
            "forward_tcp",
            "supervisor relay failed: no supervisor session",
            true,
        ),
        (
            "exec_sandbox_interactive",
            "transport error: connection reset",
            false,
        ),
        ("forward_tcp", "transport error: connection reset", false),
    ] {
        env.server
            .state()
            .faults
            .insert(method, Fault::Status(Code::Unavailable, message.to_owned()));
        let mut errors = Vec::new();
        if method == "exec_sandbox_interactive" {
            errors.push(
                guard(gateway.run_program(&run(SANDBOX), &cancel))
                    .await
                    .unwrap_err(),
            );
            errors.push(
                guard(gateway.open_terminal(&shell_terminal(SANDBOX)))
                    .await
                    .unwrap_err(),
            );
        } else {
            errors.push(
                guard(gateway.open_terminal(&main_terminal(SANDBOX)))
                    .await
                    .unwrap_err(),
            );
        }
        env.server.state().faults.clear();
        for error in errors {
            assert!(error.is_retryable(), "{method} {error:?}");
            if named {
                assert!(
                    matches!(&error, GatewayError::SandboxUnreachable { sandbox, message: got }
                        if sandbox.as_str() == SANDBOX && got.contains("no supervisor session")),
                    "{method} {error:?}"
                );
            } else {
                assert!(
                    matches!(&error, GatewayError::Unreachable { endpoint: got, message: text }
                        if *got == endpoint && text.contains("connection reset")),
                    "{method} {error:?}"
                );
            }
        }
    }
}

#[tokio::test]
async fn failed_precondition_is_not_ready_only_when_the_phase_cannot_exec() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let cancel = CancellationToken::new();
    let cases = [
        (
            "jackin-done",
            SandboxPhase::Completed,
            Some(0),
            Phase::Completed { exit_code: 0 },
        ),
        (
            "jackin-bad",
            SandboxPhase::Error,
            Some(7),
            Phase::Error { exit_code: Some(7) },
        ),
        ("jackin-off", SandboxPhase::Stopped, None, Phase::Stopped),
    ];
    for (name, phase, exit, expected) in cases {
        add_sandbox(&mut env.server.state(), WORKSPACE, name, phase, exit, &[]);
        let error = guard(gateway.run_program(&run(name), &cancel))
            .await
            .unwrap_err();
        assert!(
            matches!(error, GatewayError::NotReady { phase } if phase == expected),
            "{name}"
        );
    }
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        "jackin-live",
        SandboxPhase::Ready,
        None,
        &[],
    );
    env.server.state().faults.insert(
        "exec_sandbox_interactive",
        Fault::Status(
            Code::FailedPrecondition,
            "supervisor unavailable".to_owned(),
        ),
    );
    let error = guard(gateway.run_program(&run("jackin-live"), &cancel))
        .await
        .unwrap_err();
    assert!(matches!(error, GatewayError::FailedPrecondition { .. }));
}

#[tokio::test]
async fn invalid_argument_field_comes_from_the_error_details() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let name = sandbox_name(SANDBOX);
    env.server.state().faults.insert(
        "get_sandbox",
        Fault::Detailed(invalid_field("name", "bad name")),
    );
    let error = guard(gateway.sandbox_detail(&name)).await.unwrap_err();
    assert!(matches!(
        error,
        GatewayError::InvalidArgument { ref field, ref message } if field == "name" && message.contains("bad")
    ));
    env.server.state().faults.insert(
        "get_sandbox",
        Fault::Status(
            Code::InvalidArgument,
            "label value exceeds 63 characters".to_owned(),
        ),
    );
    let error = guard(gateway.sandbox_detail(&name)).await.unwrap_err();
    assert!(matches!(
        error,
        GatewayError::InvalidArgument { ref field, ref message } if field.is_empty() && message.contains("63")
    ));
}

#[tokio::test]
async fn call_timeouts_report_the_operation_and_the_configured_duration() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    env.server.state().faults.insert(
        "get_sandbox",
        Fault::Status(Code::DeadlineExceeded, "slow".to_owned()),
    );
    let error = guard(gateway.sandbox_detail(&sandbox_name(SANDBOX)))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Timeout { op: GatewayOp::Detail, after, .. } if after == Duration::from_secs(60)
    ));

    let short = {
        let resolved = resolved(&env).await;
        let mut config = config(&env, resolved, WORKSPACE, env.runner.clone());
        config.call_timeout = SHORT;
        guard(jackin_openshell::GrpcGateway::connect(config))
            .await
            .unwrap()
    };
    env.server
        .state()
        .faults
        .insert("create_sandbox", Fault::Hang);
    let spec = create_spec(SANDBOX, "01hzy3k5m7p9r2t4v6w8x0abcd");
    let call = tokio::spawn(async move { short.create_sandbox(&spec).await });
    guard(env.server.shared.arrived.notified()).await;
    tokio::time::pause();
    let error = call.await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Timeout { op: GatewayOp::Create, after, .. } if after == SHORT
    ));
}

#[test]
fn is_retryable_is_true_only_for_unreachable_timeout_and_resource_exhausted() {
    let errors = vec![
        GatewayError::Unreachable {
            endpoint: String::new(),
            message: String::new(),
        },
        GatewayError::SandboxUnreachable {
            sandbox: sandbox_name(SANDBOX),
            message: String::new(),
        },
        GatewayError::Tls {
            message: String::new(),
        },
        GatewayError::Unauthenticated,
        GatewayError::PermissionDenied {
            what: String::new(),
        },
        GatewayError::NotFound {
            what: Missing::Sandbox(sandbox_name(SANDBOX)),
        },
        GatewayError::AlreadyExists {
            what: Missing::Sandbox(sandbox_name(SANDBOX)),
        },
        GatewayError::NotReady {
            phase: Phase::Stopped,
        },
        GatewayError::MainProcessFailed {
            sandbox: sandbox_name(SANDBOX),
        },
        GatewayError::FailedPrecondition {
            message: String::new(),
        },
        GatewayError::ResourceExhausted {
            what: String::new(),
        },
        GatewayError::InvalidArgument {
            field: String::new(),
            message: String::new(),
        },
        GatewayError::Timeout {
            op: GatewayOp::Health,
            after: Duration::ZERO,
            group: None,
        },
        GatewayError::Cancelled {
            op: GatewayOp::Health,
            group: None,
        },
        GatewayError::Process(ProcessError::Spawn {
            program: Program::Openshell,
            message: String::new(),
        }),
        GatewayError::Transfer {
            op: TransferOp::Digest,
            status: ProcessStatus::Exited(1),
            tail: TerminalText::sanitize(b""),
        },
        GatewayError::OutputTooLarge {
            op: GatewayOp::Program,
            stream: OutputStream::Stdout,
            limit: 0,
        },
        GatewayError::DownloadTooLarge { size: 1, limit: 0 },
        GatewayError::NotRegularFile {
            kind: TerminalText::sanitize(b"directory"),
        },
        GatewayError::HostKeyMismatch,
        GatewayError::TerminalRefused {
            reason: String::new(),
        },
        GatewayError::UnsafePath {
            path: SandboxPath::parse("/sandbox/a").unwrap(),
        },
        GatewayError::Protocol {
            message: String::new(),
        },
    ];
    for error in &errors {
        let expected = match error {
            GatewayError::Unreachable { .. }
            | GatewayError::SandboxUnreachable { .. }
            | GatewayError::Timeout { .. }
            | GatewayError::ResourceExhausted { .. } => true,
            GatewayError::Tls { .. }
            | GatewayError::Unauthenticated
            | GatewayError::PermissionDenied { .. }
            | GatewayError::NotFound { .. }
            | GatewayError::AlreadyExists { .. }
            | GatewayError::NotReady { .. }
            | GatewayError::MainProcessFailed { .. }
            | GatewayError::FailedPrecondition { .. }
            | GatewayError::InvalidArgument { .. }
            | GatewayError::Cancelled { .. }
            | GatewayError::Process(_)
            | GatewayError::Transfer { .. }
            | GatewayError::OutputTooLarge { .. }
            | GatewayError::DownloadTooLarge { .. }
            | GatewayError::NotRegularFile { .. }
            | GatewayError::HostKeyMismatch
            | GatewayError::TerminalRefused { .. }
            | GatewayError::UnsafePath { .. }
            | GatewayError::Protocol { .. } => false,
        };
        assert_eq!(error.is_retryable(), expected, "{}", kind(error));
    }
    assert_eq!(errors.len(), 23);
}
