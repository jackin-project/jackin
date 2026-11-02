// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]

use std::collections::BTreeMap;
use std::num::NonZeroU16;

use futures::FutureExt;
use jackin_core::SandboxPath;
use jackin_openshell::{
    EndReason, InputLease, InputMode, RepaintPolicy, SessionClosed, TermSize, TerminalCommand,
    TerminalEvent, TerminalKind, TerminalSession,
};

fn main_kind() -> TerminalKind {
    TerminalKind::MainProcess {
        input: InputMode::Owner,
        repaint: RepaintPolicy::Never,
    }
}

fn size(cols: u16, rows: u16) -> TermSize {
    TermSize {
        cols: NonZeroU16::new(cols).unwrap(),
        rows: NonZeroU16::new(rows).unwrap(),
    }
}

#[tokio::test]
async fn session_forwards_commands_to_the_driver_and_events_to_the_owner() {
    let (mut session, driver) = TerminalSession::pair(8, main_kind());
    assert_eq!(session.kind(), &main_kind());
    let events = [
        TerminalEvent::Attached,
        TerminalEvent::Lease(InputLease::Owner),
        TerminalEvent::Output(b"x".to_vec()),
        TerminalEvent::Notice("note".to_owned()),
        TerminalEvent::Exited(Some(0)),
        TerminalEvent::Ended(EndReason::StreamClosed),
    ];
    let mut driver = driver;
    for event in events.clone() {
        driver.events.send(event).await.unwrap();
    }
    for event in events {
        assert_eq!(session.next_event().await, Some(event));
    }
    session.input(b"k".to_vec()).await.unwrap();
    session.resize(size(80, 24)).await.unwrap();
    session.detach().await.unwrap();
    assert_eq!(
        driver.commands.recv().await,
        Some(TerminalCommand::Input(b"k".to_vec()))
    );
    assert_eq!(
        driver.commands.recv().await,
        Some(TerminalCommand::Resize(size(80, 24)))
    );
    assert_eq!(driver.commands.recv().await, Some(TerminalCommand::Detach));
    drop(driver);
    assert_eq!(session.next_event().await, None);

    let (session, mut driver) = TerminalSession::pair(1, main_kind());
    session.input(b"1".to_vec()).await.unwrap();
    assert!(session.input(b"2".to_vec()).now_or_never().is_none());
    assert_eq!(
        driver.commands.recv().await,
        Some(TerminalCommand::Input(b"1".to_vec()))
    );
    assert!(matches!(
        session.input(b"2".to_vec()).now_or_never(),
        Some(Ok(()))
    ));
}

#[tokio::test]
async fn closed_driver_makes_commands_fail_with_session_closed() {
    let (mut session, driver) = TerminalSession::pair(2, main_kind());
    drop(driver);
    assert_eq!(session.input(b"a".to_vec()).await, Err(SessionClosed));
    assert_eq!(session.resize(size(80, 24)).await, Err(SessionClosed));
    assert_eq!(session.detach().await, Err(SessionClosed));
    assert_eq!(session.next_event().await, None);
}

#[tokio::test]
async fn dropping_a_main_process_session_detaches_and_a_shell_session_closes_the_stream() {
    let (session, mut driver) = TerminalSession::pair(2, main_kind());
    drop(session);
    assert_eq!(driver.commands.recv().await, Some(TerminalCommand::Detach));
    assert_eq!(driver.commands.recv().await, None);

    let shell = TerminalKind::Shell {
        workdir: SandboxPath::parse("/sandbox").unwrap(),
        env: BTreeMap::new(),
    };
    let (session, mut driver) = TerminalSession::pair(2, shell);
    drop(session);
    assert_eq!(driver.commands.recv().await, None);
}
