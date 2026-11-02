// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use jackin_core::{EnvName, ProcessStatus};
use jackin_openshell::{
    EndReason, GatewayError, InputMode, TermSize, TerminalCommand, TerminalDriver, TerminalEvent,
    TerminalKind, TerminalSession,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

use super::state::Cell;
use crate::exec::{HostProgram, SANDBOX_PATH, status_of};

const CAPACITY: usize = 256;
const CHUNK: usize = 4096;

pub(super) fn attach_main(
    cell: &Arc<Cell>,
    kind: TerminalKind,
    input: InputMode,
    size: TermSize,
) -> TerminalSession {
    let (session, driver) = TerminalSession::pair(CAPACITY, kind);
    let (id, queued) = cell.state().main.attach(input, size);
    tokio::spawn(pump_main(Arc::clone(cell), id, driver, queued));
    session
}

async fn pump_main(
    cell: Arc<Cell>,
    id: u64,
    driver: TerminalDriver,
    mut queued: UnboundedReceiver<TerminalEvent>,
) {
    let TerminalDriver {
        mut commands,
        events,
    } = driver;
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => cell.state().main.command(id, command),
                None => break,
            },
            event = queued.recv() => match event {
                Some(event) => {
                    let ended = matches!(event, TerminalEvent::Ended(_));
                    if events.send(event).await.is_err() || ended {
                        break;
                    }
                }
                None => break,
            },
        }
    }
    cell.state().main.forget(id);
}

pub(super) fn spawn_shell(
    cell: &Arc<Cell>,
    kind: TerminalKind,
    workdir: PathBuf,
    env: &BTreeMap<EnvName, String>,
) -> Result<TerminalSession, GatewayError> {
    let mut command = HostProgram::Shell.command();
    command
        .current_dir(workdir)
        .env_clear()
        .env("PATH", SANDBOX_PATH)
        .envs(env.iter().map(|(name, value)| (name.as_str(), value)))
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn().map_err(|error| GatewayError::Protocol {
        message: format!("the fake cannot start /bin/sh: {error}"),
    })?;
    let (session, driver) = TerminalSession::pair(CAPACITY, kind);
    tokio::spawn(pump_shell(cell.closed.clone(), child, driver));
    Ok(session)
}

async fn read_chunk<R: AsyncRead + Unpin>(pipe: &mut Option<R>, buffer: &mut [u8]) -> usize {
    match pipe {
        Some(pipe) => pipe.read(buffer).await.unwrap_or(0),
        None => std::future::pending().await,
    }
}

async fn pump_shell(closed: CancellationToken, mut child: Child, driver: TerminalDriver) {
    let TerminalDriver {
        mut commands,
        events,
    } = driver;
    let mut stdin = child.stdin.take();
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    if events.send(TerminalEvent::Attached).await.is_err() {
        return;
    }
    let mut out = vec![0_u8; CHUNK];
    let mut err = vec![0_u8; CHUNK];
    let reason = loop {
        if stdout.is_none() && stderr.is_none() {
            break None;
        }
        let chunk = tokio::select! {
            () = closed.cancelled() => break Some(EndReason::StreamClosed),
            command = commands.recv() => {
                match command {
                    Some(TerminalCommand::Input(bytes)) => {
                        if let Some(pipe) = stdin.as_mut() {
                            drop(pipe.write_all(&bytes).await);
                        }
                    }
                    Some(TerminalCommand::Resize(_)) => {}
                    Some(TerminalCommand::Detach) | None => break Some(EndReason::Detached),
                }
                continue;
            }
            count = read_chunk(&mut stdout, &mut out), if stdout.is_some() => {
                if count == 0 {
                    stdout = None;
                    continue;
                }
                out[..count].to_vec()
            }
            count = read_chunk(&mut stderr, &mut err), if stderr.is_some() => {
                if count == 0 {
                    stderr = None;
                    continue;
                }
                err[..count].to_vec()
            }
        };
        if events.send(TerminalEvent::Output(chunk)).await.is_err() {
            return;
        }
    };
    match reason {
        None => {
            let code = match child.wait().await.map(status_of) {
                Ok(ProcessStatus::Exited(code)) => Some(code),
                Ok(ProcessStatus::Signaled(_)) | Err(_) => None,
            };
            drop(events.send(TerminalEvent::Exited(code)).await);
            drop(
                events
                    .send(TerminalEvent::Ended(EndReason::StreamClosed))
                    .await,
            );
        }
        Some(reason) => {
            drop(child.kill().await);
            drop(events.send(TerminalEvent::Ended(reason)).await);
        }
    }
}
