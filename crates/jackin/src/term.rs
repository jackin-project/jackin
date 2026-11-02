// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{SandboxName, SupervisorReading, SupervisorState};
use jackin_openshell::{EndReason, SessionClosed, TermSize, TerminalEvent, TerminalSession};
use jackin_session::{AttachRequest, AttachedTerminal, Session, ShellRequest};

use crate::app::App;
use crate::error::CommandError;
use crate::terminal::TerminalInput;

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
        ..
    } = app;
    let terminal = terminal.as_mut().ok_or(CommandError::NoTerminal)?;
    let raw = terminal.enter_raw().map_err(CommandError::Terminal)?;
    let mut exit = None;
    let (reason, interrupted) = loop {
        tokio::select! {
            input = terminal.next_input() => match input {
                TerminalInput::Bytes(bytes) => {
                    if remote.input(bytes).await.is_err() {
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
                Some(TerminalEvent::Output(bytes)) => io.bytes(&bytes)?,
                Some(TerminalEvent::Notice(text)) => io.note(&text),
                Some(TerminalEvent::Ended(reason)) => break (reason, false),
                Some(TerminalEvent::Exited(code)) => exit = code,
                Some(TerminalEvent::Attached | TerminalEvent::Lease(_)) => {}
            },
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
        app.io.note(&format!(
            "Codex exited with code {code}; run jackin attach {sandbox} --restart to start it again"
        ));
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
        EndReason::StreamClosed => Err(CommandError::Session(format!(
            "the terminal stream of sandbox {sandbox} closed"
        ))),
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
