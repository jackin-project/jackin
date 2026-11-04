// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::fmt;
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use jackin_console::domain::instance::InstanceId;
use jackin_console::panes::{
    EndKind, Lease, LinkEvent, LinkSource, PaneCommand, PaneLink, PaneRequest, PaneSize, Sent,
};
use jackin_core::{SandboxName, SupervisorReading, SupervisorState, plain};
use jackin_openshell::{
    EndReason, InputLease, InputMode, SessionClosed, TermSize, TerminalEvent, TerminalSession,
};
use jackin_session::{
    AttachRequest, AttachedTerminal, LineDelivery, Session, ShellRequest, error_chain,
    launch_events,
};
use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc, watch};
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::observe::Inputs;

const COMMANDS: usize = 64;

#[derive(Debug, Default)]
struct InboxState {
    events: VecDeque<LinkEvent>,
    output_bytes: usize,
}

#[derive(Debug)]
struct Inbox {
    state: Mutex<InboxState>,
    cap: usize,
    room: Notify,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Inbox {
    fn new(cap: usize) -> Inbox {
        Inbox {
            state: Mutex::new(InboxState::default()),
            cap,
            room: Notify::new(),
        }
    }

    fn push(&self, event: LinkEvent) {
        let mut state = locked(&self.state);
        if let LinkEvent::Output(bytes) = &event {
            state.output_bytes += bytes.len();
        }
        state.events.push_back(event);
    }

    fn has_room(&self) -> bool {
        locked(&self.state).output_bytes < self.cap
    }

    fn drain(&self, budget: usize) -> Vec<LinkEvent> {
        let mut state = locked(&self.state);
        let mut left = budget;
        let mut taken = Vec::new();
        while let Some(event) = state.events.pop_front() {
            match event {
                LinkEvent::Output(mut bytes) => {
                    if left == 0 {
                        state.events.push_front(LinkEvent::Output(bytes));
                        break;
                    }
                    if bytes.len() > left {
                        let rest = bytes.split_off(left);
                        state.events.push_front(LinkEvent::Output(rest));
                    }
                    left -= bytes.len();
                    state.output_bytes -= bytes.len();
                    taken.push(LinkEvent::Output(bytes));
                    if left == 0 {
                        break;
                    }
                }
                other => taken.push(other),
            }
        }
        let freed = state.output_bytes < self.cap;
        drop(state);
        if freed {
            self.room.notify_one();
        }
        taken
    }
}

#[derive(Debug)]
struct Registered {
    id: u64,
    instance: InstanceId,
    detach: CancellationToken,
}

struct Shared {
    inputs: Arc<Inputs>,
    handle: Handle,
    inbox_bytes: usize,
    supervisor_poll: Duration,
    next: AtomicU64,
    live: watch::Sender<usize>,
    agents: Mutex<Vec<Registered>>,
}

#[derive(Clone)]
pub(crate) struct PaneHub {
    shared: Arc<Shared>,
}

impl fmt::Debug for PaneHub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PaneHub").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attempt {
    Open,
    Restart,
}

impl PaneHub {
    pub(crate) fn new(
        inputs: Arc<Inputs>,
        handle: Handle,
        inbox_bytes: usize,
        supervisor_poll: Duration,
    ) -> PaneHub {
        PaneHub {
            shared: Arc::new(Shared {
                inputs,
                handle,
                inbox_bytes,
                supervisor_poll,
                next: AtomicU64::new(1),
                live: watch::channel(0).0,
                agents: Mutex::new(Vec::new()),
            }),
        }
    }

    pub(crate) fn links(&self) -> Arc<dyn LinkSource> {
        Arc::new(HubLinks { hub: self.clone() })
    }

    pub(crate) fn live(&self) -> usize {
        *self.shared.live.borrow()
    }

    pub(crate) async fn finished(&self, deadline: Duration) -> bool {
        let mut live = self.shared.live.subscribe();
        tokio::time::timeout(deadline, live.wait_for(|count| *count == 0))
            .await
            .is_ok()
    }

    pub(crate) fn leave(&self, instance: &str) {
        let mut agents = locked(&self.shared.agents);
        agents.retain(|agent| {
            let leaving = agent.instance == instance;
            if leaving {
                agent.detach.cancel();
            }
            !leaving
        });
    }

    fn open(
        &self,
        request: &PaneRequest,
        size: PaneSize,
        attempt: Attempt,
    ) -> Result<Box<dyn PaneLink>, String> {
        let (session, sandbox) = self.shared.inputs.pane_target(&request.instance)?;
        let id = self.shared.next.fetch_add(1, Ordering::SeqCst);
        let inbox = Arc::new(Inbox::new(self.shared.inbox_bytes));
        let (commands, receiver) = mpsc::channel(COMMANDS);
        let (size_tx, size_rx) = watch::channel(size);
        let detach = CancellationToken::new();
        let gone = self.shared.inputs.cancel.child_token();
        let agent = request.agent().is_some();
        if agent {
            locked(&self.shared.agents).push(Registered {
                id,
                instance: request.instance.clone(),
                detach: detach.clone(),
            });
        }
        self.shared.live.send_modify(|count| *count += 1);
        let run = Run {
            hub: self.clone(),
            id,
            inbox: Arc::clone(&inbox),
            commands: receiver,
            size: size_rx,
            detach: detach.clone(),
            gone: gone.clone(),
            session,
            sandbox,
            agent,
            attempt,
            initial: size,
            poll: self.shared.supervisor_poll,
        };
        self.shared.handle.spawn(run.run());
        Ok(Box::new(HubLink {
            inbox,
            commands,
            size: size_tx,
            detach,
            _gone: gone.drop_guard(),
        }))
    }

    fn ended(&self, id: u64) {
        locked(&self.shared.agents).retain(|agent| agent.id != id);
        self.shared
            .live
            .send_modify(|count| *count = count.saturating_sub(1));
    }
}

#[derive(Debug)]
struct HubLinks {
    hub: PaneHub,
}

impl LinkSource for HubLinks {
    fn open(&self, request: &PaneRequest, size: PaneSize) -> Result<Box<dyn PaneLink>, String> {
        self.hub.open(request, size, Attempt::Open)
    }

    fn restart(&self, request: &PaneRequest, size: PaneSize) -> Result<Box<dyn PaneLink>, String> {
        self.hub.open(request, size, Attempt::Restart)
    }
}

#[derive(Debug)]
struct HubLink {
    inbox: Arc<Inbox>,
    commands: mpsc::Sender<PaneCommand>,
    size: watch::Sender<PaneSize>,
    detach: CancellationToken,
    _gone: DropGuard,
}

impl PaneLink for HubLink {
    fn send(&self, command: PaneCommand) -> Sent {
        if self.commands.is_closed() {
            return Sent::Closed;
        }
        if command == PaneCommand::Detach {
            self.detach.cancel();
            return Sent::Queued;
        }
        match self.commands.try_send(command) {
            Ok(()) => Sent::Queued,
            Err(mpsc::error::TrySendError::Full(_)) => Sent::Full,
            Err(mpsc::error::TrySendError::Closed(_)) => Sent::Closed,
        }
    }

    fn set_size(&self, size: PaneSize) {
        self.size.send_replace(size);
    }

    fn drain(&self, _now_ms: i64, output_budget: usize) -> Vec<LinkEvent> {
        self.inbox.drain(output_budget)
    }
}

enum Terminal {
    Main(AttachedTerminal),
    Shell(TerminalSession),
}

fn term_size(size: PaneSize) -> TermSize {
    TermSize {
        cols: NonZeroU16::new(size.cols).unwrap_or(NonZeroU16::MIN),
        rows: NonZeroU16::new(size.rows).unwrap_or(NonZeroU16::MIN),
    }
}

impl Terminal {
    async fn next_event(&mut self) -> Option<TerminalEvent> {
        match self {
            Terminal::Main(terminal) => terminal.next_event().await,
            Terminal::Shell(terminal) => terminal.next_event().await,
        }
    }

    async fn input(&mut self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        match self {
            Terminal::Main(terminal) => terminal.input(bytes).await,
            Terminal::Shell(terminal) => terminal.input(bytes).await,
        }
    }

    async fn reply(&mut self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        match self {
            Terminal::Main(terminal) => terminal.reply(bytes).await,
            Terminal::Shell(terminal) => terminal.input(bytes).await,
        }
    }

    async fn resize(&self, size: PaneSize) -> Result<(), SessionClosed> {
        match self {
            Terminal::Main(terminal) => terminal.resize(term_size(size)).await,
            Terminal::Shell(terminal) => terminal.resize(term_size(size)).await,
        }
    }

    async fn detach(&self) -> Result<(), SessionClosed> {
        match self {
            Terminal::Main(terminal) => terminal.detach().await,
            Terminal::Shell(terminal) => terminal.detach().await,
        }
    }

    async fn redraw(&self, size: PaneSize) -> Result<(), SessionClosed> {
        if size.rows > 1 {
            self.resize(PaneSize {
                rows: size.rows - 1,
                cols: size.cols,
            })
            .await?;
            self.resize(size).await?;
        }
        Ok(())
    }
}

fn link_event(event: TerminalEvent) -> LinkEvent {
    match event {
        TerminalEvent::Attached => LinkEvent::Attached,
        TerminalEvent::Lease(InputLease::Owner) => LinkEvent::Lease(Lease::Owner),
        TerminalEvent::Lease(InputLease::ReadOnly) => LinkEvent::Lease(Lease::ReadOnly),
        TerminalEvent::Output(bytes) => LinkEvent::Output(bytes),
        TerminalEvent::Notice(text) => LinkEvent::Notice(text),
        TerminalEvent::Exited(code) => LinkEvent::Exited(code),
        TerminalEvent::Ended(reason) => LinkEvent::Ended(match reason {
            EndReason::Detached => EndKind::Detached,
            EndReason::StreamClosed => EndKind::StreamClosed,
            EndReason::Refused(text) => EndKind::Refused(text),
            EndReason::Failed(text) => EndKind::Failed(text),
        }),
    }
}

struct Run {
    hub: PaneHub,
    id: u64,
    inbox: Arc<Inbox>,
    commands: mpsc::Receiver<PaneCommand>,
    size: watch::Receiver<PaneSize>,
    detach: CancellationToken,
    gone: CancellationToken,
    session: Session,
    sandbox: SandboxName,
    agent: bool,
    attempt: Attempt,
    initial: PaneSize,
    poll: Duration,
}

impl Run {
    async fn run(mut self) {
        let id = self.id;
        let hub = self.hub.clone();
        self.drive().await;
        hub.ended(id);
    }

    async fn start(&self) -> Result<Terminal, String> {
        let size = term_size(self.initial);
        if !self.agent {
            return self
                .session
                .open_shell(ShellRequest {
                    sandbox: self.sandbox.clone(),
                    size,
                })
                .await
                .map(Terminal::Shell)
                .map_err(|error| plain(error_chain(&error)));
        }
        if self.attempt == Attempt::Restart {
            let (events, feed) = launch_events(LineDelivery::BestEffort);
            drop(feed);
            self.session
                .restart_agent(&self.sandbox, events, self.gone.clone())
                .await
                .map_err(|error| plain(error_chain(&error)))?;
        }
        self.session
            .attach(AttachRequest {
                sandbox: self.sandbox.clone(),
                size,
                input: InputMode::Owner,
            })
            .await
            .map(Terminal::Main)
            .map_err(|error| plain(error_chain(&error)))
    }

    async fn drive(&mut self) {
        let opened = tokio::select! {
            biased;
            () = self.gone.cancelled() => return,
            () = self.detach.cancelled() => {
                self.inbox.push(LinkEvent::Ended(EndKind::Detached));
                return;
            }
            opened = self.start() => opened,
        };
        let mut terminal = match opened {
            Ok(terminal) => terminal,
            Err(reason) => {
                self.inbox.push(LinkEvent::Ended(EndKind::Failed(reason)));
                return;
            }
        };
        let mut detaching = false;
        let mut watch =
            tokio::time::interval_at(tokio::time::Instant::now() + self.poll, self.poll);
        watch.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let room = self.inbox.has_room();
            tokio::select! {
                biased;
                () = self.gone.cancelled() => return,
                () = self.detach.cancelled(), if !detaching => {
                    detaching = true;
                    if terminal.detach().await.is_err() {
                        self.inbox.push(LinkEvent::Ended(EndKind::Detached));
                        return;
                    }
                }
                command = self.commands.recv() => {
                    let Some(command) = command else { return };
                    if self.command(&mut terminal, command).await.is_err() {
                        self.inbox.push(LinkEvent::Ended(EndKind::StreamClosed));
                        return;
                    }
                }
                changed = self.size.changed() => {
                    if changed.is_ok() {
                        let size = *self.size.borrow_and_update();
                        if terminal.resize(size).await.is_err() {
                            self.inbox.push(LinkEvent::Ended(EndKind::StreamClosed));
                            return;
                        }
                    }
                }
                _ = watch.tick(), if self.agent && !detaching => {
                    if let Terminal::Main(main) = &terminal
                        && let SupervisorReading::Reported(SupervisorState::Exited { code, .. }) =
                            main.supervisor().await
                    {
                        self.inbox.push(LinkEvent::Exited(Some(code)));
                        return;
                    }
                }
                () = self.inbox.room.notified(), if !room => {}
                event = terminal.next_event(), if room => {
                    let Some(event) = event else {
                        self.inbox.push(LinkEvent::Ended(EndKind::StreamClosed));
                        return;
                    };
                    let ended = matches!(event, TerminalEvent::Ended(_));
                    self.inbox.push(link_event(event));
                    if ended {
                        return;
                    }
                }
            }
        }
    }

    async fn command(
        &self,
        terminal: &mut Terminal,
        command: PaneCommand,
    ) -> Result<(), SessionClosed> {
        match command {
            PaneCommand::Input(bytes) => terminal.input(bytes).await,
            PaneCommand::Reply(bytes) => terminal.reply(bytes).await,
            PaneCommand::Redraw => {
                let size = *self.size.borrow();
                terminal.redraw(size).await
            }
            PaneCommand::Detach => terminal.detach().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(length: usize, byte: u8) -> LinkEvent {
        LinkEvent::Output(vec![byte; length])
    }

    fn trace(events: Vec<LinkEvent>) -> Vec<LinkEvent> {
        let mut out: Vec<LinkEvent> = Vec::new();
        for event in events {
            match (out.last_mut(), event) {
                (Some(LinkEvent::Output(into)), LinkEvent::Output(more)) => into.extend(more),
                (_, event) => out.push(event),
            }
        }
        out
    }

    #[test]
    fn a_drain_splits_output_at_the_budget_and_loses_and_reorders_nothing() {
        let inbox = Inbox::new(1 << 20);
        inbox.push(LinkEvent::Attached);
        inbox.push(output(5, 1));
        inbox.push(output(7, 2));
        inbox.push(LinkEvent::Exited(Some(0)));
        let mut seen = Vec::new();
        loop {
            let taken = inbox.drain(4);
            if taken.is_empty() {
                break;
            }
            let bytes: usize = taken
                .iter()
                .map(|event| match event {
                    LinkEvent::Output(bytes) => bytes.len(),
                    _ => 0,
                })
                .sum();
            assert!(bytes <= 4);
            seen.extend(taken);
        }
        assert_eq!(
            trace(seen),
            vec![
                LinkEvent::Attached,
                LinkEvent::Output([[1; 5].as_slice(), [2; 7].as_slice()].concat()),
                LinkEvent::Exited(Some(0)),
            ]
        );
    }

    #[test]
    fn the_inbox_has_room_until_its_output_reaches_the_cap_and_again_after_a_drain() {
        let inbox = Inbox::new(10);
        assert!(inbox.has_room());
        inbox.push(output(10, 1));
        assert!(!inbox.has_room());
        inbox.push(LinkEvent::Attached);
        assert!(!inbox.has_room());
        assert_eq!(inbox.drain(4), vec![output(4, 1)]);
        assert!(inbox.has_room());
    }
}
