// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::{Gateway, GatewayError, Missing};
use openshell_sdk::raw::proto::SandboxPhase;
use tonic::{Code, Status};

use super::harness::{Env, WORKSPACE, connect, env, guard};
use super::support::{SANDBOX, add_sandbox, main_terminal, sandbox_name};

fn ready(env: &Env, name: &str) {
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        name,
        SandboxPhase::Ready,
        None,
        &[],
    );
}

#[tokio::test]
async fn main_attach_on_an_absent_sandbox_is_not_found_at_session_and_relay_steps() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let name = sandbox_name(SANDBOX);

    let error = guard(gateway.open_terminal(&main_terminal(SANDBOX)))
        .await
        .unwrap_err();
    assert!(
        matches!(error, GatewayError::NotFound { what: Missing::Sandbox(ref missing) } if *missing == name),
        "{error:?}"
    );

    ready(&env, SANDBOX);
    let doomed = (WORKSPACE.to_owned(), SANDBOX.to_owned());
    env.server.state().after_ssh_session = Some(Box::new(move |state| {
        state.sandboxes.remove(&doomed);
    }));
    let error = guard(gateway.open_terminal(&main_terminal(SANDBOX)))
        .await
        .unwrap_err();
    assert!(
        matches!(error, GatewayError::NotFound { what: Missing::Sandbox(ref missing) } if *missing == name),
        "{error:?}"
    );
    assert!(env.server.state().sandboxes.is_empty());
}

#[tokio::test]
async fn relay_open_errors_are_reported_with_their_cause_not_as_a_dropped_ssh_stream() {
    let env = env().await;
    ready(&env, SANDBOX);
    let gateway = connect(&env, WORKSPACE).await;
    let endpoint = env.server.endpoint();

    env.server.state().forward_error =
        Some(Status::new(Code::Unavailable, "relay channel dropped"));
    let error = guard(gateway.open_terminal(&main_terminal(SANDBOX)))
        .await
        .unwrap_err();
    match &error {
        GatewayError::Unreachable {
            endpoint: reported,
            message,
        } => {
            assert_eq!(*reported, endpoint);
            assert!(message.contains("relay channel dropped"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert!(error.is_retryable());

    env.server.state().forward_error = Some(Status::new(
        Code::FailedPrecondition,
        "relay target open failed: connection refused",
    ));
    let error = guard(gateway.open_terminal(&main_terminal(SANDBOX)))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, GatewayError::FailedPrecondition { message } if message.contains("connection refused")),
        "{error:?}"
    );
}
