// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Phase;
use jackin_openshell::{Gateway, GatewayError, PowerAction, SandboxQuery};
use openshell_sdk::raw::proto::SandboxPhase;

use super::harness::{WORKSPACE, connect, env, guard};
use super::support::{add_sandbox, sandbox_name, set_conditions};

#[tokio::test]
async fn stop_and_start_on_an_ended_main_process() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    {
        let mut state = env.server.state();
        add_sandbox(
            &mut state,
            WORKSPACE,
            "jackin-bad",
            SandboxPhase::Error,
            Some(7),
            &[],
        );
        set_conditions(
            &mut state,
            WORKSPACE,
            "jackin-bad",
            &[(
                "Ready",
                "False",
                "MainProcessFailed",
                "Canonical main process exited with status 7",
            )],
        );
        add_sandbox(
            &mut state,
            WORKSPACE,
            "jackin-done",
            SandboxPhase::Completed,
            Some(0),
            &[],
        );
        set_conditions(
            &mut state,
            WORKSPACE,
            "jackin-done",
            &[(
                "Ready",
                "False",
                "MainProcessCompleted",
                "Canonical main process completed successfully",
            )],
        );
    }
    for name in ["jackin-bad", "jackin-done"] {
        let sandbox = sandbox_name(name);
        let error = guard(gateway.set_power(&sandbox, PowerAction::Stop))
            .await
            .unwrap_err();
        assert!(
            matches!(error, GatewayError::MainProcessFailed { sandbox: ref failed } if *failed == sandbox),
            "{name}: {error:?}"
        );
    }
    let bad = sandbox_name("jackin-bad");
    let error = guard(gateway.set_power(&bad, PowerAction::Start))
        .await
        .unwrap_err();
    assert!(
        matches!(error, GatewayError::FailedPrecondition { ref message } if message.contains("already running generation")),
        "{error:?}"
    );
    let found = guard(gateway.find_sandboxes(&SandboxQuery::Name(bad.clone())))
        .await
        .unwrap();
    assert_eq!(found[0].phase, Phase::Starting);
    let error = guard(gateway.set_power(&bad, PowerAction::Stop))
        .await
        .unwrap_err();
    assert!(
        matches!(error, GatewayError::FailedPrecondition { .. }),
        "{error:?}"
    );
    let done = sandbox_name("jackin-done");
    let error = guard(gateway.set_power(&done, PowerAction::Start))
        .await
        .unwrap_err();
    assert!(
        matches!(error, GatewayError::FailedPrecondition { .. }),
        "{error:?}"
    );
}
