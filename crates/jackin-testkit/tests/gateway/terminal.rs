// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvName, Phase};
use jackin_openshell::{
    EndReason, Gateway, InputLease, InputMode, TerminalEvent, TerminalKind, TerminalRequest,
    TerminalSession,
};
use jackin_testkit::{FakeGateway, SandboxHandle};

use super::support::{
    TestResult, attach, collect_until, drain, eventually, fake, guard, output_text, path, ready,
    sh, size, write,
};

const MAIN: &str = "echo banner; sleep 3600";

async fn banner_sandbox(fake: &FakeGateway) -> Result<SandboxHandle, Box<dyn std::error::Error>> {
    ready(fake, sh(MAIN)?).await
}

async fn barrier(session: &TerminalSession, handle: &SandboxHandle, mark: u16) -> TestResult {
    let wanted = size(mark, 10)?;
    session.resize(wanted).await?;
    eventually(|| handle.pty_size() == Some(wanted)).await;
    Ok(())
}

fn is_output(event: &TerminalEvent) -> bool {
    matches!(event, TerminalEvent::Output(_))
}

#[tokio::test]
async fn owner_gets_the_lease_replay_and_input_and_the_size_is_recorded() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner_sandbox(&fake).await?;
        let mut owner = attach(&fake, &handle.name(), InputMode::Owner).await?;
        let events = collect_until(&mut owner, is_output).await?;
        assert_eq!(events[0], TerminalEvent::Attached);
        assert_eq!(events[1], TerminalEvent::Lease(InputLease::Owner));
        assert!(output_text(&events).contains("banner"));
        owner.input(b"ab".to_vec()).await?;
        eventually(|| handle.main_input() == b"ab").await;
        let resized = size(120, 40)?;
        owner.resize(resized).await?;
        eventually(|| handle.pty_size() == Some(resized)).await;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn second_attachment_is_read_only_and_takes_the_lease_after_the_owner_leaves() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner_sandbox(&fake).await?;
        let mut first = attach(&fake, &handle.name(), InputMode::Owner).await?;
        collect_until(&mut first, is_output).await?;
        assert_eq!(handle.attachments(), 1);
        let mut second = attach(&fake, &handle.name(), InputMode::Owner).await?;
        assert_eq!(handle.attachments(), 2);
        let events = collect_until(&mut second, |e| matches!(e, TerminalEvent::Lease(_))).await?;
        assert_eq!(
            events,
            [
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::ReadOnly)
            ]
        );
        second.input(b"x".to_vec()).await?;
        barrier(&second, &handle, 11).await?;
        first.input(b"a".to_vec()).await?;
        eventually(|| handle.main_input() == b"a").await;
        first.detach().await?;
        let ended = drain(&mut first).await;
        assert_eq!(
            ended.last(),
            Some(&TerminalEvent::Ended(EndReason::Detached))
        );
        assert_eq!(handle.attachments(), 1);
        second.input(b"y".to_vec()).await?;
        collect_until(&mut second, |e| {
            *e == TerminalEvent::Lease(InputLease::Owner)
        })
        .await?;
        eventually(|| handle.main_input() == b"ay").await;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn viewer_input_is_always_ignored_and_never_takes_the_lease() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner_sandbox(&fake).await?;
        let mut viewer = attach(&fake, &handle.name(), InputMode::Viewer).await?;
        let events = collect_until(&mut viewer, |e| matches!(e, TerminalEvent::Lease(_))).await?;
        assert_eq!(
            events.last(),
            Some(&TerminalEvent::Lease(InputLease::ReadOnly))
        );
        viewer.input(b"typed".to_vec()).await?;
        barrier(&viewer, &handle, 12).await?;
        assert!(handle.main_input().is_empty());

        let mut owner = attach(&fake, &handle.name(), InputMode::Owner).await?;
        collect_until(&mut owner, is_output).await?;
        owner.detach().await?;
        drain(&mut owner).await;
        viewer.input(b"again".to_vec()).await?;
        barrier(&viewer, &handle, 13).await?;
        assert!(handle.main_input().is_empty());
        viewer.input(vec![0x03]).await?;
        let rest = drain(&mut viewer).await;
        assert!(
            !rest.contains(&TerminalEvent::Lease(InputLease::Owner)),
            "{rest:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn detach_keeps_the_process_ctrl_d_detaches_ctrl_c_ends_only_a_read_only_attachment_and_reattach_replays()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner_sandbox(&fake).await?;
        let name = handle.name();
        let mut first = attach(&fake, &name, InputMode::Owner).await?;
        first.detach().await?;
        let ended = drain(&mut first).await;
        assert_eq!(
            ended.last(),
            Some(&TerminalEvent::Ended(EndReason::Detached))
        );
        assert_eq!(handle.phase(), Phase::Ready);
        assert_eq!(handle.attachments(), 0);

        let mut again = attach(&fake, &name, InputMode::Owner).await?;
        let events = collect_until(&mut again, is_output).await?;
        assert!(output_text(&events).contains("banner"));
        again.input(vec![0x04]).await?;
        let rest = drain(&mut again).await;
        assert_eq!(rest, [TerminalEvent::Ended(EndReason::Detached)]);
        assert!(handle.main_input().is_empty());

        let mut owner = attach(&fake, &name, InputMode::Owner).await?;
        collect_until(&mut owner, is_output).await?;
        let mut reader = attach(&fake, &name, InputMode::Owner).await?;
        reader.input(vec![0x03]).await?;
        let rest = drain(&mut reader).await;
        assert_eq!(
            rest.last(),
            Some(&TerminalEvent::Ended(EndReason::StreamClosed))
        );
        assert!(!rest.iter().any(|e| matches!(e, TerminalEvent::Exited(_))));
        assert_eq!(handle.attachments(), 1);
        assert!(handle.main_input().is_empty());
        owner.input(vec![0x03]).await?;
        eventually(|| handle.main_input() == [0x03]).await;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn shell_terminal_runs_in_the_sandbox_directory_with_env_and_reports_the_exit_code()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = banner_sandbox(&fake).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/marker", "m").await?;
        let request = TerminalRequest {
            sandbox: name.clone(),
            kind: TerminalKind::Shell {
                workdir: path("/sandbox")?,
                env: BTreeMap::from([(EnvName::parse("JACKIN_PANE")?, "shell-1".to_owned())]),
            },
            size: size(100, 30)?,
        };
        let mut shell = fake.open_terminal(&request).await?;
        shell
            .input(b"echo \"$JACKIN_PANE\"; test -f marker && exit 4; exit 5\n".to_vec())
            .await?;
        let events = drain(&mut shell).await;
        assert!(output_text(&events).contains("shell-1"), "{events:?}");
        let tail = &events[events.len() - 2..];
        assert_eq!(
            tail,
            [
                TerminalEvent::Exited(Some(4)),
                TerminalEvent::Ended(EndReason::StreamClosed)
            ]
        );
        assert!(handle.main_input().is_empty());
        let mut main = attach(&fake, &name, InputMode::Owner).await?;
        let events = collect_until(&mut main, |e| matches!(e, TerminalEvent::Lease(_))).await?;
        assert_eq!(
            events.last(),
            Some(&TerminalEvent::Lease(InputLease::Owner))
        );
        Ok(())
    })
    .await
}
