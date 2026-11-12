// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::num::NonZeroU16;
use std::time::Duration;

use jackin_core::{AttemptId, FileMode, SandboxName, SecretBytes, SupervisorState, sandbox_layout};
use jackin_openshell::{Gateway, SandboxFile, TermSize, TerminalEvent};
use jackin_session::{AttachRequest, AttachTarget, AttachedTerminal};
use jackin_testkit::{Rig, SandboxHandle};
use tokio_util::sync::CancellationToken;

pub(crate) fn size() -> Result<TermSize, Box<dyn Error>> {
    Ok(TermSize {
        cols: NonZeroU16::new(100).ok_or("zero columns")?,
        rows: NonZeroU16::new(30).ok_or("zero rows")?,
    })
}

pub(crate) async fn target(rig: &Rig, id: u8) -> Result<AttachTarget, Box<dyn Error>> {
    Ok(AttachTarget {
        sandbox: rig.handle(id).await?.name(),
        state: SupervisorState::Running {
            attempt: AttemptId::FIRST,
        },
    })
}

pub(crate) async fn owner(rig: &Rig, id: u8) -> Result<AttachedTerminal, Box<dyn Error>> {
    let request = AttachRequest::owner(target(rig, id).await?, size()?);
    Ok(rig
        .world
        .session
        .attach(request, &CancellationToken::new())
        .await?)
}

pub(crate) async fn viewer(rig: &Rig, id: u8) -> Result<AttachedTerminal, Box<dyn Error>> {
    let request = AttachRequest::viewer(rig.sandbox_name(id)?, size()?);
    Ok(rig
        .world
        .session
        .attach(request, &CancellationToken::new())
        .await?)
}

pub(crate) async fn events(
    terminal: &mut AttachedTerminal,
    count: usize,
) -> Result<Vec<TerminalEvent>, Box<dyn Error>> {
    let mut seen = Vec::new();
    while seen.len() < count {
        seen.push(terminal.next_event().await.ok_or("the terminal ended")?);
    }
    Ok(seen)
}

pub(crate) async fn until(mut done: impl FnMut() -> bool) -> Result<(), Box<dyn Error>> {
    for _ in 0..400 {
        if done() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("the condition never held".into())
}

pub(crate) async fn delivered(
    handle: &SandboxHandle,
    terminal: &mut AttachedTerminal,
    chunk: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let before = handle.main_input().len();
    terminal.input(chunk.to_vec()).await?;
    terminal.input(b"~".to_vec()).await?;
    until(|| handle.main_input().len() > before && handle.main_input().ends_with(b"~")).await?;
    let mut got = handle.main_input().split_off(before);
    got.pop();
    Ok(got)
}

pub(crate) async fn replied(
    handle: &SandboxHandle,
    terminal: &mut AttachedTerminal,
    chunk: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let before = handle.main_input().len();
    terminal.reply(chunk.to_vec()).await?;
    terminal.input(b"~".to_vec()).await?;
    until(|| handle.main_input().len() > before && handle.main_input().ends_with(b"~")).await?;
    let mut got = handle.main_input().split_off(before);
    got.pop();
    Ok(got)
}

pub(crate) async fn set_state(
    rig: &Rig,
    name: &SandboxName,
    text: &str,
) -> Result<(), Box<dyn Error>> {
    rig.world
        .gateway
        .write_file(
            name,
            &SandboxFile {
                path: sandbox_layout::RUN_STATE.clone(),
                mode: FileMode(0o600),
                content: SecretBytes::new(text.as_bytes().to_vec()),
            },
        )
        .await?;
    Ok(())
}

pub(crate) async fn dropped(
    handle: &SandboxHandle,
    terminal: &mut AttachedTerminal,
    chunk: &[u8],
) -> Result<bool, Box<dyn Error>> {
    let before = handle.main_input();
    terminal.input(chunk.to_vec()).await?;
    tokio::time::sleep(Duration::from_millis(40)).await;
    Ok(handle.main_input() == before)
}

pub(crate) async fn delivered_in_two(
    handle: &SandboxHandle,
    terminal: &mut AttachedTerminal,
    first: &[u8],
    second: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let before = handle.main_input().len();
    terminal.input(first.to_vec()).await?;
    terminal.input(second.to_vec()).await?;
    terminal.input(b"~".to_vec()).await?;
    until(|| handle.main_input().len() > before && handle.main_input().ends_with(b"~")).await?;
    let mut got = handle.main_input().split_off(before);
    got.pop();
    Ok(got)
}
