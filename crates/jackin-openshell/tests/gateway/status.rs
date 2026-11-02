// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Phase, SandboxPath};
use jackin_core::{ProcessError, ProcessStatus, Program, TerminalText};
use jackin_openshell::{
    Gateway, GatewayError, GatewayOp, Missing, ProgramRun, ProgramStdin, TransferOp,
};
use openshell_sdk::raw::proto::SandboxPhase;
use tokio_util::sync::CancellationToken;
use tonic::Code;

use super::harness::{WORKSPACE, config, connect, env, guard, resolved};
use super::server::{Fault, invalid_field};
use super::support::{SANDBOX, SHORT, add_sandbox, create_spec, kind, sandbox_name};

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
        GatewayError::Timeout { op: GatewayOp::Detail, after } if after == Duration::from_secs(60)
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
    env.server.shared.arrived.notified().await;
    tokio::time::pause();
    let error = call.await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Timeout { op: GatewayOp::Create, after } if after == SHORT
    ));
}

#[test]
fn is_retryable_is_true_only_for_unreachable_timeout_and_resource_exhausted() {
    let errors = vec![
        GatewayError::Unreachable {
            endpoint: String::new(),
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
        },
        GatewayError::Cancelled {
            op: GatewayOp::Health,
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
        GatewayError::HostKeyMismatch,
        GatewayError::TerminalRefused {
            reason: String::new(),
        },
        GatewayError::Protocol {
            message: String::new(),
        },
    ];
    for error in &errors {
        let expected = match error {
            GatewayError::Unreachable { .. }
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
            | GatewayError::HostKeyMismatch
            | GatewayError::TerminalRefused { .. }
            | GatewayError::Protocol { .. } => false,
        };
        assert_eq!(error.is_retryable(), expected, "{}", kind(error));
    }
    assert_eq!(errors.len(), 18);
}
