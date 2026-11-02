// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::Phase;
use jackin_openshell::{
    EndReason, Gateway, GatewayError, PowerAction, SandboxQuery, TerminalEvent,
};
use openshell_sdk::raw::proto::SandboxPhase;

use super::harness::{WORKSPACE, connect, env, guard};
use super::ssh_server::{
    Action, REFUSED_NOTICE, RIGHT_FINGERPRINT, RIGHT_SEED, Script, SshModel, host_key,
};
use super::support::{add_sandbox, main_terminal, sandbox_name, set_conditions};

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

#[tokio::test]
async fn a_main_attach_on_an_ended_sandbox_is_refused_and_on_a_stopped_one_not_ready() {
    let env = env().await;
    let model = SshModel::new(
        &host_key(&RIGHT_SEED),
        RIGHT_FINGERPRINT,
        Script {
            after_subsystem: vec![
                Action::Notice(REFUSED_NOTICE),
                Action::ExitStatus(1),
                Action::Eof,
                Action::Close,
            ],
            ..Script::default()
        },
    );
    {
        let mut state = env.server.state();
        state.ssh = Some(Arc::clone(&model));
        for (name, phase, code) in [
            ("jackin-bad", SandboxPhase::Error, Some(7)),
            ("jackin-done", SandboxPhase::Completed, Some(0)),
            ("jackin-stopped", SandboxPhase::Stopped, None),
        ] {
            add_sandbox(&mut state, WORKSPACE, name, phase, code, &[]);
        }
    }
    let gateway = connect(&env, WORKSPACE).await;
    for name in ["jackin-bad", "jackin-done"] {
        let mut session = guard(gateway.open_terminal(&main_terminal(name)))
            .await
            .unwrap();
        let mut events = Vec::new();
        while let Some(event) = guard(session.next_event()).await {
            events.push(event);
        }
        assert_eq!(
            events,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Ended(EndReason::Refused(REFUSED_NOTICE.trim().to_owned())),
            ],
            "{name}"
        );
    }
    let stopped = guard(gateway.open_terminal(&main_terminal("jackin-stopped"))).await;
    assert!(
        matches!(
            stopped,
            Err(GatewayError::NotReady {
                phase: Phase::Stopped
            })
        ),
        "{:?}",
        stopped.err()
    );
}
