// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::{Future, pending};
use std::pin::{Pin, pin};
use std::time::Duration;

use jackin_core::{SandboxName, SupervisorReading, SupervisorState};
use jackin_openshell::{EndReason, SessionClosed, TermSize, TerminalEvent, TerminalSession};
use jackin_session::{
    AttachRequest, AttachedTerminal, ClipboardGuard, Session, ShellRequest, SupervisorWatch,
};

use crate::app::App;
use crate::error::CommandError;
use crate::terminal::TerminalInput;

const WATCH: Duration = Duration::from_secs(1);

pub(crate) enum Remote {
    Main(AttachedTerminal),
    Shell(TerminalSession),
}

impl Remote {
    async fn input(&mut self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        match self {
            Remote::Main(terminal) => terminal.input(bytes).await,
            Remote::Shell(terminal) => terminal.input(bytes).await,
        }
    }

    async fn resize(&self, size: TermSize) -> Result<(), SessionClosed> {
        match self {
            Remote::Main(terminal) => terminal.resize(size).await,
            Remote::Shell(terminal) => terminal.resize(size).await,
        }
    }

    async fn detach(&self) -> bool {
        match self {
            Remote::Main(terminal) => terminal.detach().await.is_ok(),
            Remote::Shell(terminal) => terminal.detach().await.is_ok(),
        }
    }

    async fn next(&mut self) -> Option<TerminalEvent> {
        match self {
            Remote::Main(terminal) => terminal.next_event().await,
            Remote::Shell(terminal) => terminal.next_event().await,
        }
    }

    fn watcher(&self) -> Option<Watcher> {
        match self {
            Remote::Main(terminal) => Some(Watcher {
                watch: terminal.watch(),
                seen: reported(terminal.supervisor_at_attach().clone()),
            }),
            Remote::Shell(_) => None,
        }
    }
}

struct Watcher {
    watch: SupervisorWatch,
    seen: Option<SupervisorState>,
}

type Reading = Pin<Box<dyn Future<Output = SupervisorReading> + Send>>;

fn read_once(watch: SupervisorWatch) -> Reading {
    Box::pin(async move { watch.read().await })
}

fn reported(reading: SupervisorReading) -> Option<SupervisorState> {
    match reading {
        SupervisorReading::Reported(state) => Some(state),
        SupervisorReading::Unreadable | SupervisorReading::Invalid(_) => None,
    }
}

fn exited_note(sandbox: &SandboxName, code: i32) -> String {
    format!(
        "Codex exited with code {code}; Ctrl+D detaches, then jackin attach {sandbox} --restart starts it again"
    )
}

#[derive(Debug)]
pub(crate) struct Ended {
    pub(crate) reason: EndReason,
    pub(crate) exit: Option<i32>,
    pub(crate) interrupted: bool,
}

pub(crate) async fn pump(app: &mut App, remote: &mut Remote) -> Result<Ended, CommandError> {
    let App {
        io,
        terminal,
        cancel,
        deps,
        ..
    } = app;
    let clock = &deps.host.clock;
    let terminal = terminal.as_mut().ok_or(CommandError::NoTerminal)?;
    let raw = terminal.enter_raw().map_err(CommandError::Terminal)?;
    let mut exit = None;
    let mut clipboard = ClipboardGuard::default();
    let mut watcher = remote.watcher();
    let mut reading: Option<Reading> = None;
    let mut tick = pin!(clock.sleep(WATCH));
    let (reason, interrupted) = loop {
        tokio::select! {
            input = terminal.next_input() => match input {
                TerminalInput::Bytes(bytes) => {
                    let bytes = clipboard.from_operator(&bytes);
                    if !bytes.is_empty() && remote.input(bytes).await.is_err() {
                        break (EndReason::StreamClosed, false);
                    }
                }
                TerminalInput::Resized(size) => {
                    if remote.resize(size).await.is_err() {
                        break (EndReason::StreamClosed, false);
                    }
                }
                TerminalInput::Closed => {
                    remote.detach().await;
                    break (EndReason::Detached, false);
                }
            },
            event = remote.next() => match event {
                None => break (EndReason::StreamClosed, false),
                Some(TerminalEvent::Output(bytes)) => {
                    let bytes = clipboard.from_sandbox(&bytes);
                    if !bytes.is_empty() {
                        io.bytes(&bytes)?;
                    }
                }
                Some(TerminalEvent::Notice(text)) => io.raw_note(&text),
                Some(TerminalEvent::Ended(reason)) => break (reason, false),
                Some(TerminalEvent::Exited(code)) => exit = code,
                Some(TerminalEvent::Attached | TerminalEvent::Lease(_)) => {}
            },
            () = &mut tick, if watcher.is_some() && reading.is_none() => {
                reading = watcher.as_ref().map(|watcher| read_once(watcher.watch.clone()));
            }
            seen = async {
                match reading.as_mut() {
                    Some(read) => read.await,
                    None => pending().await,
                }
            }, if reading.is_some() => {
                reading = None;
                tick.set(clock.sleep(WATCH));
                if let (Some(state), Some(watcher)) = (reported(seen), watcher.as_mut()) {
                    if let SupervisorState::Exited { code, .. } = state
                        && watcher.seen != Some(state)
                    {
                        io.raw_note(&exited_note(watcher.watch.sandbox(), code));
                    }
                    watcher.seen = Some(state);
                }
            }
            () = cancel.cancelled() => {
                remote.detach().await;
                break (EndReason::Detached, true);
            }
        }
    };
    drop(raw);
    Ok(Ended {
        reason,
        exit,
        interrupted,
    })
}

pub(crate) async fn attach(
    app: &mut App,
    session: &Session,
    request: AttachRequest,
) -> Result<(), CommandError> {
    let sandbox = request.sandbox.clone();
    let attached = session.attach(request).await?;
    if let SupervisorReading::Reported(SupervisorState::Exited { code, .. }) =
        attached.supervisor_at_attach()
    {
        app.io.note(&exited_note(&sandbox, *code));
    }
    let mut remote = Remote::Main(attached);
    let ended = pump(app, &mut remote).await?;
    finish(app, &sandbox, ended)
}

fn finish(app: &mut App, sandbox: &SandboxName, ended: Ended) -> Result<(), CommandError> {
    if ended.interrupted {
        return Err(CommandError::Cancelled(format!(
            "cancelled; detached from sandbox {sandbox}"
        )));
    }
    match ended.reason {
        EndReason::Detached => {
            app.io.note(&format!(
                "detached from sandbox {sandbox}; reattach with: jackin attach {sandbox}"
            ));
            Ok(())
        }
        EndReason::StreamClosed => match ended.exit {
            Some(0) => {
                app.io.note(&format!(
                    "the main process of sandbox {sandbox} exited with code 0"
                ));
                Ok(())
            }
            Some(code) => Err(CommandError::Session(format!(
                "the main process of sandbox {sandbox} exited with code {code}"
            ))),
            None => Err(CommandError::Session(format!(
                "the terminal stream of sandbox {sandbox} closed"
            ))),
        },
        EndReason::Refused(text) | EndReason::Failed(text) => {
            Err(CommandError::Session(format!("sandbox {sandbox}: {text}")))
        }
    }
}

pub(crate) async fn shell(
    app: &mut App,
    session: &Session,
    request: ShellRequest,
) -> Result<(), CommandError> {
    let sandbox = request.sandbox.clone();
    let mut remote = Remote::Shell(session.open_shell(request).await?);
    let ended = pump(app, &mut remote).await?;
    if ended.interrupted {
        return Err(CommandError::Cancelled(format!(
            "cancelled; closed the shell in sandbox {sandbox}"
        )));
    }
    match ended.reason {
        EndReason::Detached | EndReason::StreamClosed => {
            if let Some(code) = ended.exit.filter(|code| *code != 0) {
                app.io.note(&format!(
                    "the shell in sandbox {sandbox} exited with code {code}"
                ));
            }
            Ok(())
        }
        EndReason::Refused(text) | EndReason::Failed(text) => {
            Err(CommandError::Session(format!("sandbox {sandbox}: {text}")))
        }
    }
}
