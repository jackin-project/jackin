// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroU16;
use std::time::Duration;

use jackin_core::{EnvName, SandboxName, SandboxPath};
use tokio::sync::mpsc;

use crate::error::SessionClosed;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TermSize {
    pub cols: NonZeroU16,
    pub rows: NonZeroU16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputMode {
    Owner,
    Viewer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RepaintPolicy {
    Never,
    NudgeAfter(Duration),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalKind {
    MainProcess {
        input: InputMode,
        repaint: RepaintPolicy,
    },
    Shell {
        workdir: SandboxPath,
        env: BTreeMap<EnvName, String>,
    },
    Exec {
        argv: Vec<String>,
        workdir: SandboxPath,
        env: BTreeMap<EnvName, String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalRequest {
    pub sandbox: SandboxName,
    pub kind: TerminalKind,
    pub size: TermSize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalCommand {
    Input(Vec<u8>),
    Resize(TermSize),
    Detach,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    Attached,
    Lease(InputLease),
    Output(Vec<u8>),
    Notice(String),
    Exited(Option<i32>),
    Ended(EndReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputLease {
    Owner,
    ReadOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndReason {
    Detached,
    StreamClosed,
    Refused(String),
    Failed(String),
}

#[derive(Debug)]
pub struct TerminalSession {
    kind: TerminalKind,
    commands: mpsc::Sender<TerminalCommand>,
    events: mpsc::Receiver<TerminalEvent>,
}

#[derive(Debug)]
pub struct TerminalDriver {
    pub commands: mpsc::Receiver<TerminalCommand>,
    pub events: mpsc::Sender<TerminalEvent>,
}

impl TerminalSession {
    pub fn pair(capacity: usize, kind: TerminalKind) -> (TerminalSession, TerminalDriver) {
        let capacity = capacity.max(1);
        let (command_tx, command_rx) = mpsc::channel(capacity);
        let (event_tx, event_rx) = mpsc::channel(capacity);
        (
            TerminalSession {
                kind,
                commands: command_tx,
                events: event_rx,
            },
            TerminalDriver {
                commands: command_rx,
                events: event_tx,
            },
        )
    }

    pub fn kind(&self) -> &TerminalKind {
        &self.kind
    }

    pub async fn input(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.send(TerminalCommand::Input(bytes)).await
    }

    pub async fn resize(&self, size: TermSize) -> Result<(), SessionClosed> {
        self.send(TerminalCommand::Resize(size)).await
    }

    pub async fn detach(&self) -> Result<(), SessionClosed> {
        self.send(TerminalCommand::Detach).await
    }

    pub async fn next_event(&mut self) -> Option<TerminalEvent> {
        self.events.recv().await
    }

    async fn send(&self, command: TerminalCommand) -> Result<(), SessionClosed> {
        self.commands.send(command).await.map_err(|_| SessionClosed)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if matches!(self.kind, TerminalKind::MainProcess { .. }) {
            drop(self.commands.try_send(TerminalCommand::Detach));
        }
    }
}
