// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroU16;
use std::time::Duration;

use jackin_core::{EnvName, SandboxName, SandboxPath};

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
pub struct TerminalSession {}

#[derive(Debug)]
pub struct TerminalDriver {
    pub commands: tokio::sync::mpsc::Receiver<TerminalCommand>,
    pub events: tokio::sync::mpsc::Sender<TerminalEvent>,
}
