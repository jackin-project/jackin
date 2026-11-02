// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::{
    EndReason, GatewayError, InputLease, InputMode, MainProcess, TermSize, TerminalCommand,
    TerminalEvent,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

const SUPERVISOR_PROGRAM: &str = "/usr/local/bin/jackin-start";
const SHELL_PROGRAM: &str = "/bin/sh";
const RETAINED_MAX: usize = 1024 * 1024;
const CTRL_C: u8 = 0x03;
const CTRL_D: u8 = 0x04;
const DETACH_PREFIX: u8 = 0x10;
const DETACH_KEY: u8 = 0x11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlanEnd {
    Sleeps,
    Cat,
    Exits(i32),
    Supervised,
}

#[derive(Clone, Debug)]
pub(super) struct MainPlan {
    pub(super) output: Vec<u8>,
    pub(super) end: PlanEnd,
}

enum Statement<'a> {
    Echo(&'a [&'a str]),
    Ends(PlanEnd),
}

fn statement<'a>(words: &'a [&'a str]) -> Option<Statement<'a>> {
    match words {
        ["echo", rest @ ..] => Some(Statement::Echo(rest)),
        ["sleep", seconds] => seconds
            .parse::<u64>()
            .ok()
            .map(|_| Statement::Ends(PlanEnd::Sleeps)),
        ["exit"] => Some(Statement::Ends(PlanEnd::Exits(0))),
        ["exit", code] => code
            .parse::<i32>()
            .ok()
            .map(|code| Statement::Ends(PlanEnd::Exits(code))),
        ["exec", "cat"] => Some(Statement::Ends(PlanEnd::Cat)),
        _ => None,
    }
}

fn invalid(message: &str) -> GatewayError {
    GatewayError::InvalidArgument {
        field: "command".to_owned(),
        message: message.to_owned(),
    }
}

impl MainPlan {
    pub(super) fn parse(main: &MainProcess) -> Result<MainPlan, GatewayError> {
        if main.program.as_str() == SUPERVISOR_PROGRAM {
            return Ok(MainPlan {
                output: Vec::new(),
                end: PlanEnd::Supervised,
            });
        }
        let [flag, script] = main.args.as_slice() else {
            return Err(invalid("the fake runs only /bin/sh -c <script>"));
        };
        if main.program.as_str() != SHELL_PROGRAM || flag != "-c" {
            return Err(invalid("the fake runs only /bin/sh -c <script>"));
        }
        let mut output = Vec::new();
        for text in script.split(';') {
            let words: Vec<&str> = text.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }
            match statement(&words) {
                Some(Statement::Echo(rest)) => {
                    output.extend_from_slice(rest.join(" ").as_bytes());
                    output.extend_from_slice(b"\r\n");
                }
                Some(Statement::Ends(end)) => return Ok(MainPlan { output, end }),
                None => return Err(invalid("the fake does not know this statement")),
            }
        }
        Ok(MainPlan {
            output,
            end: PlanEnd::Exits(0),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Seat {
    generation: u64,
    id: u64,
}

#[derive(Debug)]
struct Attachment {
    id: u64,
    input: InputMode,
    lease: InputLease,
    prefix_held: bool,
    events: UnboundedSender<TerminalEvent>,
}

impl Attachment {
    fn send(&self, event: TerminalEvent) {
        drop(self.events.send(event));
    }
}

#[derive(Debug)]
pub(super) struct Main {
    generation: u64,
    output: Vec<u8>,
    echo: bool,
    input: Vec<u8>,
    pty: Option<TermSize>,
    attachments: Vec<Attachment>,
    owner: Option<u64>,
    next: u64,
}

impl Main {
    pub(super) fn new(plan: &MainPlan, generation: u64) -> Main {
        Main {
            generation,
            output: plan.output.clone(),
            echo: plan.end == PlanEnd::Cat,
            input: Vec::new(),
            pty: None,
            attachments: Vec::new(),
            owner: None,
            next: 1,
        }
    }

    pub(super) fn input(&self) -> Vec<u8> {
        self.input.clone()
    }

    pub(super) fn pty(&self) -> Option<TermSize> {
        self.pty
    }

    pub(super) fn attachments(&self) -> usize {
        self.attachments.len()
    }

    pub(super) fn attach(
        &mut self,
        input: InputMode,
        size: TermSize,
    ) -> (Seat, UnboundedReceiver<TerminalEvent>) {
        let (events, queued) = mpsc::unbounded_channel();
        let id = self.next;
        self.next += 1;
        let lease = if input == InputMode::Owner && self.owner.is_none() {
            self.owner = Some(id);
            InputLease::Owner
        } else {
            InputLease::ReadOnly
        };
        self.pty = Some(size);
        let attachment = Attachment {
            id,
            input,
            lease,
            prefix_held: false,
            events,
        };
        attachment.send(TerminalEvent::Attached);
        attachment.send(TerminalEvent::Lease(lease));
        if !self.output.is_empty() {
            attachment.send(TerminalEvent::Output(self.output.clone()));
        }
        self.attachments.push(attachment);
        let seat = Seat {
            generation: self.generation,
            id,
        };
        (seat, queued)
    }

    pub(super) fn command(&mut self, seat: Seat, command: TerminalCommand) {
        if seat.generation != self.generation {
            return;
        }
        match command {
            TerminalCommand::Input(bytes) => self.input_from(seat.id, &bytes),
            TerminalCommand::Resize(size) => self.pty = Some(size),
            TerminalCommand::Detach => self.end(seat.id, EndReason::Detached),
        }
    }

    pub(super) fn forget(&mut self, seat: Seat) {
        if seat.generation != self.generation {
            return;
        }
        self.attachments
            .retain(|attachment| attachment.id != seat.id);
        if self.owner == Some(seat.id) {
            self.owner = None;
        }
    }

    pub(super) fn exit(&mut self, code: Option<i32>) {
        for attachment in self.attachments.drain(..) {
            if code.is_some() {
                attachment.send(TerminalEvent::Exited(code));
            }
            attachment.send(TerminalEvent::Ended(EndReason::StreamClosed));
        }
        self.owner = None;
    }

    fn input_from(&mut self, id: u64, bytes: &[u8]) {
        let Some((lease, mode)) = self
            .attachments
            .iter()
            .find(|a| a.id == id)
            .map(|a| (a.lease, a.input))
        else {
            return;
        };
        if bytes.contains(&CTRL_D) {
            self.end(id, EndReason::Detached);
            return;
        }
        if lease == InputLease::ReadOnly && bytes.contains(&CTRL_C) {
            self.end(id, EndReason::StreamClosed);
            return;
        }
        let (keys, detach) = self.keys_from(id, bytes);
        match lease {
            InputLease::Owner => self.forward(&keys),
            InputLease::ReadOnly if mode == InputMode::Owner && self.owner.is_none() => {
                self.owner = Some(id);
                if let Some(attachment) = self.attachments.iter_mut().find(|a| a.id == id) {
                    attachment.lease = InputLease::Owner;
                    attachment.send(TerminalEvent::Lease(InputLease::Owner));
                }
                self.forward(&keys);
            }
            InputLease::ReadOnly => {}
        }
        if detach {
            self.end(id, EndReason::Detached);
        }
    }

    fn keys_from(&mut self, id: u64, bytes: &[u8]) -> (Vec<u8>, bool) {
        let mut keys = Vec::with_capacity(bytes.len());
        let Some(attachment) = self.attachments.iter_mut().find(|a| a.id == id) else {
            return (keys, false);
        };
        for &byte in bytes {
            if std::mem::take(&mut attachment.prefix_held) {
                if byte == DETACH_KEY {
                    return (keys, true);
                }
                keys.push(DETACH_PREFIX);
            }
            if byte == DETACH_PREFIX {
                attachment.prefix_held = true;
            } else {
                keys.push(byte);
            }
        }
        (keys, false)
    }

    pub(super) fn emit(&mut self, bytes: &[u8]) {
        self.output.extend_from_slice(bytes);
        if self.output.len() > RETAINED_MAX {
            let excess = self.output.len() - RETAINED_MAX;
            self.output.drain(..excess);
        }
        for attachment in &self.attachments {
            attachment.send(TerminalEvent::Output(bytes.to_vec()));
        }
    }

    fn forward(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.input.extend_from_slice(bytes);
        if self.echo {
            self.emit(bytes);
        }
    }

    fn end(&mut self, id: u64, reason: EndReason) {
        if let Some(index) = self.attachments.iter().position(|a| a.id == id) {
            let attachment = self.attachments.remove(index);
            attachment.send(TerminalEvent::Ended(reason));
        }
        if self.owner == Some(id) {
            self.owner = None;
        }
    }
}
