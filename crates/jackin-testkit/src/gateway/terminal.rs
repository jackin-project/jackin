// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use jackin_core::ProcessStatus;
use jackin_openshell::{
    EndReason, GatewayError, TermSize, TerminalCommand, TerminalDriver, TerminalEvent,
    TerminalKind, TerminalSession,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

use super::ShellWitness;
use super::main_process::Seat;
use super::state::Cell;
use crate::exec::{HostProgram, status_of};
use crate::sync::locked;

const CAPACITY: usize = 256;
const CHUNK: usize = 4096;
const LIMIT_NOTICE: &str = "sandbox SSH connection limit reached";
const ENDED_NOTICE: &str = "openshell: cannot connect to the canonical main process: canonical main process already finished";

pub(super) fn attach_main(
    cell: &Arc<Cell>,
    kind: TerminalKind,
    size: TermSize,
    cap: usize,
) -> Result<TerminalSession, GatewayError> {
    let TerminalKind::MainProcess { input, repaint } = kind.clone() else {
        return Err(GatewayError::Protocol {
            message: "the fake attached a shell as the main process".to_owned(),
        });
    };
    let (session, driver) = TerminalSession::pair(CAPACITY, kind);
    let (seat, queued, pending) = {
        let mut state = cell.state();
        if state.main.attachments() >= cap {
            return Err(GatewayError::ResourceExhausted {
                what: LIMIT_NOTICE.to_owned(),
            });
        }
        let window = state.replay_window;
        state.main.attach(input, size, repaint, window)
    };
    tokio::spawn(pump_main(Arc::clone(cell), seat, driver, queued, pending));
    Ok(session)
}

pub(super) fn refuse_main(kind: TerminalKind) -> TerminalSession {
    let (session, driver) = TerminalSession::pair(CAPACITY, kind);
    for event in [
        TerminalEvent::Attached,
        TerminalEvent::Ended(EndReason::Refused(ENDED_NOTICE.to_owned())),
    ] {
        drop(driver.events.try_send(event));
    }
    session
}

async fn pump_main(
    cell: Arc<Cell>,
    seat: Seat,
    driver: TerminalDriver,
    mut queued: UnboundedReceiver<TerminalEvent>,
    pending: Arc<AtomicUsize>,
) {
    let TerminalDriver {
        mut commands,
        events,
    } = driver;
    loop {
        tokio::select! {
            biased;
            command = commands.recv() => match command {
                Some(command) => cell.state().main.command(seat, command),
                None => break,
            },
            event = queued.recv() => match event {
                Some(event) => {
                    let ended = matches!(event, TerminalEvent::Ended(_));
                    let length = match &event {
                        TerminalEvent::Output(bytes) => bytes.len(),
                        _ => 0,
                    };
                    if events.send(event).await.is_err() || ended {
                        break;
                    }
                    pending.fetch_sub(length, Ordering::SeqCst);
                }
                None => break,
            },
        }
    }
    cell.state().main.forget(seat);
}

pub(super) fn spawn_shell(
    cell: &Arc<Cell>,
    kind: TerminalKind,
    workdir: PathBuf,
    size: TermSize,
) -> Result<TerminalSession, GatewayError> {
    let TerminalKind::Shell {
        workdir: sandbox_workdir,
        env,
    } = kind.clone()
    else {
        return Err(GatewayError::Protocol {
            message: "the fake attached the main process as a shell".to_owned(),
        });
    };
    let mut command = HostProgram::Shell.command();
    command
        .current_dir(workdir)
        .envs(env.iter().map(|(name, value)| (name.as_str(), value)))
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn().map_err(|error| GatewayError::Protocol {
        message: format!("the fake cannot start /bin/sh: {error}"),
    })?;
    let (session, driver) = TerminalSession::pair(CAPACITY, kind);
    let witness = Arc::new(Mutex::new(ShellWitness {
        workdir: sandbox_workdir,
        env,
        input: Vec::new(),
        sizes: vec![size],
        closed: false,
    }));
    cell.state().shells.push(Arc::clone(&witness));
    tokio::spawn(pump_shell(cell.closed.clone(), child, driver, witness));
    Ok(session)
}

struct Closed(Arc<Mutex<ShellWitness>>);

impl Drop for Closed {
    fn drop(&mut self) {
        locked(&self.0).closed = true;
    }
}

async fn read_chunk<R: AsyncRead + Unpin>(pipe: &mut Option<R>, buffer: &mut [u8]) -> usize {
    match pipe {
        Some(pipe) => pipe.read(buffer).await.unwrap_or(0),
        None => std::future::pending().await,
    }
}

async fn pump_shell(
    closed: CancellationToken,
    mut child: Child,
    driver: TerminalDriver,
    witness: Arc<Mutex<ShellWitness>>,
) {
    let _closed = Closed(Arc::clone(&witness));
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
                        locked(&witness).input.extend_from_slice(&bytes);
                        if let Some(pipe) = stdin.as_mut() {
                            drop(pipe.write_all(&bytes).await);
                        }
                    }
                    Some(TerminalCommand::Resize(size)) => locked(&witness).sizes.push(size),
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
