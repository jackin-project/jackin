// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvName, Phase};
use jackin_openshell::{
    EndReason, Gateway, InputLease, InputMode, PowerAction, TerminalEvent, TerminalKind,
    TerminalRequest, TerminalSession,
};
use jackin_testkit::{FakeGateway, SandboxHandle};

use super::support::{
    TestResult, attach, collect_until, drain, eventually, fake, guard, observe, output_text, path,
    ready, sh, size, write,
};

const MAIN: &str = "echo banner; sleep 3600";

async fn banner_sandbox(fake: &FakeGateway) -> Result<SandboxHandle, Box<dyn std::error::Error>> {
    ready(fake, sh(MAIN)?).await
}

async fn barrier(session: &TerminalSession, handle: &SandboxHandle, mark: u16) -> TestResult {
    let wanted = size(mark, 10)?;
    session.resize(wanted).await?;
    eventually("the resize barrier", || handle.pty_size(), Some(wanted)).await
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
        let events = collect_until("the owner replay", &mut owner, is_output).await?;
        assert_eq!(events[0], TerminalEvent::Attached);
        assert_eq!(events[1], TerminalEvent::Lease(InputLease::Owner));
        assert!(output_text(&events).contains("banner"));
        owner.input(b"ab".to_vec()).await?;
        let resized = size(120, 40)?;
        owner.resize(resized).await?;
        eventually("the owner resize", || handle.pty_size(), Some(resized)).await?;
        assert_eq!(handle.main_input(), b"ab");
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
        collect_until("the first replay", &mut first, is_output).await?;
        assert_eq!(handle.attachments(), 1);
        let mut second = attach(&fake, &handle.name(), InputMode::Owner).await?;
        assert_eq!(handle.attachments(), 2);
        let events = collect_until("the second lease", &mut second, |e| {
            matches!(e, TerminalEvent::Lease(_))
        })
        .await?;
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
        barrier(&first, &handle, 12).await?;
        assert_eq!(handle.main_input(), b"a");
        first.detach().await?;
        let ended = drain("the first to detach", &mut first).await?;
        assert_eq!(
            ended.last(),
            Some(&TerminalEvent::Ended(EndReason::Detached))
        );
        assert_eq!(handle.attachments(), 1);

        let mut probe = attach(&fake, &handle.name(), InputMode::Owner).await?;
        let events = collect_until("the probe lease", &mut probe, |e| {
            matches!(e, TerminalEvent::Lease(_))
        })
        .await?;
        assert_eq!(
            events,
            [
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::Owner)
            ]
        );
        probe.detach().await?;
        drain("the probe to detach", &mut probe).await?;

        second.input(b"y".to_vec()).await?;
        collect_until("the second to take the lease", &mut second, |e| {
            *e == TerminalEvent::Lease(InputLease::Owner)
        })
        .await?;
        barrier(&second, &handle, 13).await?;
        assert_eq!(handle.main_input(), b"ay");
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
        let events = collect_until("the viewer lease", &mut viewer, |e| {
            matches!(e, TerminalEvent::Lease(_))
        })
        .await?;
        assert_eq!(
            events.last(),
            Some(&TerminalEvent::Lease(InputLease::ReadOnly))
        );
        viewer.input(b"typed".to_vec()).await?;
        barrier(&viewer, &handle, 12).await?;
        assert!(handle.main_input().is_empty());

        let mut owner = attach(&fake, &handle.name(), InputMode::Owner).await?;
        collect_until("the owner replay", &mut owner, is_output).await?;
        owner.detach().await?;
        drain("the owner to detach", &mut owner).await?;
        viewer.input(b"again".to_vec()).await?;
        barrier(&viewer, &handle, 13).await?;
        assert!(handle.main_input().is_empty());
        viewer.input(vec![0x03]).await?;
        let rest = drain("the viewer to close", &mut viewer).await?;
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
        let ended = drain("the first to detach", &mut first).await?;
        assert_eq!(
            ended.last(),
            Some(&TerminalEvent::Ended(EndReason::Detached))
        );
        assert_eq!(handle.phase(), Phase::Ready);
        assert_eq!(handle.attachments(), 0);

        let mut again = attach(&fake, &name, InputMode::Owner).await?;
        let events = collect_until("the replay", &mut again, is_output).await?;
        assert!(output_text(&events).contains("banner"));
        again.input(vec![0x04]).await?;
        let rest = drain("Ctrl-D to detach", &mut again).await?;
        assert_eq!(rest, [TerminalEvent::Ended(EndReason::Detached)]);
        assert!(handle.main_input().is_empty());

        let mut owner = attach(&fake, &name, InputMode::Owner).await?;
        collect_until("the owner replay", &mut owner, is_output).await?;
        let mut reader = attach(&fake, &name, InputMode::Owner).await?;
        reader.input(vec![0x03]).await?;
        let rest = drain("Ctrl-C to end the reader", &mut reader).await?;
        assert_eq!(
            rest.last(),
            Some(&TerminalEvent::Ended(EndReason::StreamClosed))
        );
        assert!(!rest.iter().any(|e| matches!(e, TerminalEvent::Exited(_))));
        assert_eq!(handle.attachments(), 1);
        assert!(handle.main_input().is_empty());
        owner.input(vec![0x03]).await?;
        barrier(&owner, &handle, 14).await?;
        assert_eq!(handle.main_input(), [0x03]);
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
        let events = drain("the shell to exit", &mut shell).await?;
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
        let events = collect_until("the main lease", &mut main, |e| {
            matches!(e, TerminalEvent::Lease(_))
        })
        .await?;
        assert_eq!(
            events.last(),
            Some(&TerminalEvent::Lease(InputLease::Owner))
        );
        Ok(())
    })
    .await
}

async fn cat_owner()
-> Result<(FakeGateway, SandboxHandle, TerminalSession), Box<dyn std::error::Error>> {
    let fake = fake()?;
    let handle = ready(&fake, sh("exec cat")?).await?;
    let mut owner = attach(&fake, &handle.name(), InputMode::Owner).await?;
    let events = collect_until("the owner lease", &mut owner, |e| {
        matches!(e, TerminalEvent::Lease(_))
    })
    .await?;
    assert_eq!(
        events.last(),
        Some(&TerminalEvent::Lease(InputLease::Owner))
    );
    Ok((fake, handle, owner))
}

async fn flood(session: &TerminalSession) -> TestResult {
    for _ in 0..4000 {
        let sent = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            session.input(b"a".to_vec()),
        )
        .await;
        if sent.is_err() {
            return Ok(());
        }
        tokio::task::yield_now().await;
    }
    Err("the session never filled".into())
}

#[tokio::test]
async fn a_session_dropped_while_its_channels_are_full_leaves_no_attachment() -> TestResult {
    guard(async {
        let (_fake, handle, owner) = cat_owner().await?;
        flood(&owner).await?;
        assert_eq!(handle.attachments(), 1);
        drop(owner);
        eventually("the attachment to be released", || handle.attachments(), 0).await
    })
    .await
}

#[tokio::test]
async fn a_late_ending_attachment_of_the_old_generation_does_not_release_or_feed_the_new_attachment()
-> TestResult {
    guard(async {
        let (fake, handle, mut old) = cat_owner().await?;
        let name = handle.name();
        flood(&old).await?;
        fake.set_power(&name, PowerAction::Stop).await?;
        fake.set_power(&name, PowerAction::Start).await?;
        assert_eq!(observe(&fake, &name).await?.phase, Phase::Ready);
        assert!(handle.main_input().is_empty());
        let mut fresh = attach(&fake, &name, InputMode::Owner).await?;
        let events = collect_until("the new lease", &mut fresh, |e| {
            matches!(e, TerminalEvent::Lease(_))
        })
        .await?;
        assert_eq!(
            events.last(),
            Some(&TerminalEvent::Lease(InputLease::Owner))
        );

        let ended = drain("the old attachment to end", &mut old).await?;
        assert_eq!(
            ended.last(),
            Some(&TerminalEvent::Ended(EndReason::StreamClosed))
        );
        assert_eq!(handle.attachments(), 1);
        fresh.input(b"x".to_vec()).await?;
        eventually(
            "the new attachment input",
            || handle.main_input(),
            b"x".to_vec(),
        )
        .await?;

        fresh.detach().await?;
        drain("the new attachment to detach", &mut fresh).await?;
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}

async fn expect_detached(
    session: &mut TerminalSession,
    handle: &SandboxHandle,
    label: &str,
) -> TestResult {
    let events = drain(label, session).await?;
    assert_eq!(
        events.last(),
        Some(&TerminalEvent::Ended(EndReason::Detached)),
        "{label}: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, TerminalEvent::Exited(_))),
        "{label}: {events:?}"
    );
    assert_eq!(handle.phase(), Phase::Ready, "{label}");
    assert_eq!(handle.attachments(), 0, "{label}");
    assert!(handle.main_input().is_empty(), "{label}");
    Ok(())
}

#[tokio::test]
async fn ctrl_p_ctrl_q_detaches_in_one_chunk_or_two_and_the_process_survives() -> TestResult {
    guard(async {
        let (_fake, handle, mut owner) = cat_owner().await?;
        owner.input(vec![0x10, 0x11]).await?;
        expect_detached(&mut owner, &handle, "one chunk").await?;

        let (_fake, handle, mut owner) = cat_owner().await?;
        owner.input(vec![0x10]).await?;
        owner.input(vec![0x11]).await?;
        expect_detached(&mut owner, &handle, "two chunks").await
    })
    .await
}

#[tokio::test]
async fn a_lone_ctrl_p_is_held_and_released_in_front_of_the_next_byte_and_a_lone_ctrl_q_is_forwarded()
-> TestResult {
    guard(async {
        let (_fake, handle, mut owner) = cat_owner().await?;
        owner.input(vec![0x10]).await?;
        barrier(&owner, &handle, 101).await?;
        assert!(handle.main_input().is_empty());
        assert_eq!(handle.attachments(), 1);

        owner.input(b"x".to_vec()).await?;
        eventually(
            "the released bytes",
            || handle.main_input(),
            vec![0x10, 0x78],
        )
        .await?;
        let events = collect_until("the echo", &mut owner, |e| {
            matches!(e, TerminalEvent::Output(_))
        })
        .await?;
        assert_eq!(
            events.last(),
            Some(&TerminalEvent::Output(vec![0x10, 0x78]))
        );
        assert_eq!(handle.attachments(), 1);

        let (_fake, handle, owner) = cat_owner().await?;
        owner.input(vec![0x11]).await?;
        eventually("a lone Ctrl-Q", || handle.main_input(), vec![0x11]).await?;
        assert_eq!(handle.attachments(), 1);
        Ok(())
    })
    .await
}

async fn shell_in(
    fake: &FakeGateway,
    name: &jackin_core::SandboxName,
    workdir: &str,
    script: &str,
) -> Result<Vec<TerminalEvent>, Box<dyn std::error::Error>> {
    let request = TerminalRequest {
        sandbox: name.clone(),
        kind: TerminalKind::Shell {
            workdir: path(workdir)?,
            env: BTreeMap::new(),
        },
        size: size(100, 30)?,
    };
    let mut shell = fake.open_terminal(&request).await?;
    shell.input(script.as_bytes().to_vec()).await?;
    drain("the shell to exit", &mut shell).await
}

fn exit_of(events: &[TerminalEvent]) -> Option<Option<i32>> {
    events.iter().find_map(|event| match event {
        TerminalEvent::Exited(code) => Some(*code),
        _ => None,
    })
}

#[tokio::test]
async fn a_shell_workdir_outside_sandbox_is_a_per_sandbox_directory_and_never_the_host()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let first = ready(&fake, sh("sleep 3600")?).await?;
        let second = ready(&fake, sh("sleep 3600")?).await?;
        let marker = format!("m-{}", jackin_core::LaunchId::generate()?);

        let wrote = shell_in(
            &fake,
            &first.name(),
            "/tmp",
            &format!("printf x > {marker}; exit 3\n"),
        )
        .await?;
        assert_eq!(exit_of(&wrote), Some(Some(3)), "{wrote:?}");
        let sees = shell_in(
            &fake,
            &first.name(),
            "/tmp",
            &format!("test -f {marker} && exit 0; exit 9\n"),
        )
        .await?;
        assert_eq!(exit_of(&sees), Some(Some(0)), "{sees:?}");
        let other = shell_in(
            &fake,
            &second.name(),
            "/tmp",
            &format!("test -f {marker} && exit 0; exit 9\n"),
        )
        .await?;
        assert_eq!(exit_of(&other), Some(Some(9)), "{other:?}");
        assert!(!tokio::fs::try_exists(std::path::Path::new("/tmp").join(&marker)).await?);
        assert_eq!(first.file(&path(&format!("/sandbox/{marker}"))?), None);
        Ok(())
    })
    .await
}
