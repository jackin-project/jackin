// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{FileMode, Phase, SecretBytes};
use jackin_openshell::{
    EndReason, Gateway, GatewayError, InputMode, PowerAction, SandboxFile, TerminalEvent,
    TerminalKind, TerminalRequest,
};
use jackin_testkit::{FakeGateway, SandboxHandle};
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, attach, drain, fake, guard, main_request, observe, path, program_run, ready, sh,
    size, write,
};

async fn assert_exec_not_ready(
    fake: &FakeGateway,
    name: &jackin_core::SandboxName,
    expected: Phase,
) -> TestResult {
    let file = SandboxFile {
        path: path("/sandbox/new")?,
        mode: FileMode(0o600),
        content: SecretBytes::new(b"x".to_vec()),
    };
    let shell = TerminalRequest {
        sandbox: name.clone(),
        kind: TerminalKind::Shell {
            workdir: path("/sandbox")?,
            env: BTreeMap::new(),
        },
        size: size(100, 30)?,
    };
    let errors = [
        fake.run_program(
            &program_run(name, "/bin/sh", &["-c", "true"])?,
            &CancellationToken::new(),
        )
        .await
        .err(),
        fake.read_files(name, &[path("/sandbox/kept")?], &CancellationToken::new())
            .await
            .err(),
        fake.write_file(name, &file, &CancellationToken::new())
            .await
            .err(),
        fake.open_terminal(&shell).await.err(),
    ];
    for error in errors {
        assert!(
            matches!(&error, Some(GatewayError::NotReady { phase }) if *phase == expected),
            "{error:?}"
        );
    }
    Ok(())
}

async fn assert_main_not_ready(
    fake: &FakeGateway,
    name: &jackin_core::SandboxName,
    expected: Phase,
) -> TestResult {
    for mode in [InputMode::Owner, InputMode::Viewer] {
        let error = fake.open_terminal(&main_request(name, mode)?).await.err();
        assert!(
            matches!(&error, Some(GatewayError::NotReady { phase }) if *phase == expected),
            "{mode:?}: {error:?}"
        );
    }
    Ok(())
}

async fn assert_main_refused(fake: &FakeGateway, handle: &SandboxHandle) -> TestResult {
    for mode in [InputMode::Owner, InputMode::Viewer] {
        let mut session = attach(fake, &handle.name(), mode).await?;
        let events = drain("the refused attach to close", &mut session).await?;
        let [
            TerminalEvent::Attached,
            TerminalEvent::Ended(EndReason::Refused(text)),
        ] = events.as_slice()
        else {
            return Err(format!("{mode:?}: {events:?}").into());
        };
        assert!(
            text.contains("cannot connect to the canonical main process"),
            "{text}"
        );
        assert_eq!(handle.attachments(), 0);
        assert!(handle.main_input().is_empty());
    }
    Ok(())
}

async fn ended_row(code: i32, ended: Phase) -> TestResult {
    let fake = fake()?;
    let handle = ready(&fake, sh("sleep 3600")?).await?;
    let name = handle.name();
    write(&fake, &name, "/sandbox/kept", "data").await?;
    let mut session = attach(&fake, &name, InputMode::Owner).await?;
    handle.exit_main(code);
    let events = drain("the main process to exit", &mut session).await?;
    assert_eq!(
        events[events.len() - 2..],
        [
            TerminalEvent::Exited(Some(code)),
            TerminalEvent::Ended(EndReason::StreamClosed)
        ]
    );
    assert_eq!(observe(&fake, &name).await?.phase, ended);
    assert_exec_not_ready(&fake, &name, ended).await?;
    assert_main_refused(&fake, &handle).await?;
    let stop = fake
        .set_power(&name, PowerAction::Stop, &CancellationToken::new())
        .await;
    assert!(
        matches!(stop, Err(GatewayError::MainProcessFailed { ref sandbox }) if *sandbox == name)
    );
    assert_eq!(handle.phase(), ended);
    assert_eq!(handle.file(&path("/sandbox/kept")?), Some(b"data".to_vec()));
    let start = fake
        .set_power(&name, PowerAction::Start, &CancellationToken::new())
        .await;
    assert!(matches!(
        start,
        Err(GatewayError::FailedPrecondition { .. })
    ));
    for _ in 0..10 {
        let view = observe(&fake, &name).await?;
        assert_eq!(view.phase, Phase::Starting);
        assert_eq!(view.phase.exit_code(), None);
    }
    for action in [PowerAction::Stop, PowerAction::Start] {
        let again = fake
            .set_power(&name, action, &CancellationToken::new())
            .await;
        assert!(matches!(
            again,
            Err(GatewayError::FailedPrecondition { .. })
        ));
    }
    assert_exec_not_ready(&fake, &name, Phase::Starting).await?;
    assert_main_not_ready(&fake, &name, Phase::Starting).await?;
    fake.delete_sandbox(&name).await?;
    assert!(fake.sandbox(&name).is_none());
    Ok(())
}

#[tokio::test]
async fn an_ended_main_process_refuses_the_main_attach_and_blocks_every_other_call_for_error_and_completed()
-> TestResult {
    guard(async {
        ended_row(7, Phase::Error { exit_code: Some(7) }).await?;
        ended_row(0, Phase::Completed { exit_code: 0 }).await
    })
    .await
}

#[tokio::test]
async fn a_main_attach_is_not_ready_on_a_stopped_sandbox() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        fake.set_power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        assert_main_not_ready(&fake, &name, Phase::Stopped).await?;
        assert_exec_not_ready(&fake, &name, Phase::Stopped).await?;
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}
