use std::time::Instant;

use chrono::{DateTime, Utc};
use jackin_protocol::control::ServerMsg;

use super::{ClientRegistry, ControlRouting, SessionRegistry, SessionSupervisor};

#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub(crate) struct RuntimeEvent<'a> {
    pub(crate) session_id: u64,
    pub(crate) source_id: &'a str,
    pub(crate) runtime: &'a str,
    pub(crate) event: &'a str,
    pub(crate) payload: Option<&'a str>,
    pub(crate) observed_at: Instant,
}

pub(crate) trait ControlPort {
    fn report_runtime_event(
        &self,
        sessions: &mut SessionRegistry,
        event: RuntimeEvent<'_>,
    ) -> ServerMsg;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachTransition {
    Attach,
    Displace,
}

pub(crate) trait AttachPort {
    fn begin_attach(&self, clients: &ClientRegistry) -> AttachTransition;
    fn record_attached(&self) {}
    fn record_detached(&self) {}
    fn prepare_session_spawn(&self, _sessions: &SessionSupervisor) -> anyhow::Result<()> {
        Ok(())
    }
}

pub(crate) trait StatusPort {
    fn retire_codename(
        &self,
        sessions: &mut SessionSupervisor,
        codename: &str,
        observed_at: DateTime<Utc>,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitDisposition {
    Evaluate,
    Defer,
}

pub(crate) trait PersistencePort {
    fn last_session_exit(&self, control: &ControlRouting) -> ExitDisposition;
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DefaultDaemonPorts;

impl ControlPort for DefaultDaemonPorts {
    fn report_runtime_event(
        &self,
        sessions: &mut SessionRegistry,
        event: RuntimeEvent<'_>,
    ) -> ServerMsg {
        if let Some(session) = sessions.get_mut(event.session_id) {
            session.apply_runtime_event(
                event.source_id,
                event.runtime,
                event.event,
                event.payload,
                event.observed_at,
            );
        }

        ServerMsg::Ack
    }
}

impl AttachPort for DefaultDaemonPorts {
    fn begin_attach(&self, clients: &ClientRegistry) -> AttachTransition {
        if clients.has_attached_client() {
            AttachTransition::Displace
        } else {
            AttachTransition::Attach
        }
    }
}

impl StatusPort for DefaultDaemonPorts {
    fn retire_codename(
        &self,
        sessions: &mut SessionSupervisor,
        codename: &str,
        observed_at: DateTime<Utc>,
    ) {
        sessions.retire_codename(codename, observed_at);
    }
}

impl PersistencePort for DefaultDaemonPorts {
    fn last_session_exit(&self, control: &ControlRouting) -> ExitDisposition {
        if control.dialog_open() {
            ExitDisposition::Defer
        } else {
            ExitDisposition::Evaluate
        }
    }
}

pub(crate) const PORTS: DefaultDaemonPorts = DefaultDaemonPorts;

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct FakeDaemonPorts {
    pub fail_spawn: AtomicBool,
    pub attach_count: AtomicUsize,
    pub detach_count: AtomicUsize,
    pub transitions: Mutex<Vec<AttachTransition>>,
    pub runtime_events: Mutex<Vec<u64>>,
}

#[cfg(test)]
impl FakeDaemonPorts {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
impl ControlPort for FakeDaemonPorts {
    fn report_runtime_event(
        &self,
        sessions: &mut SessionRegistry,
        event: RuntimeEvent<'_>,
    ) -> ServerMsg {
        self.runtime_events.lock().unwrap().push(event.session_id);
        DefaultDaemonPorts.report_runtime_event(sessions, event)
    }
}

#[cfg(test)]
impl AttachPort for FakeDaemonPorts {
    fn begin_attach(&self, clients: &ClientRegistry) -> AttachTransition {
        let transition = DefaultDaemonPorts.begin_attach(clients);
        self.transitions.lock().unwrap().push(transition);
        transition
    }

    fn record_attached(&self) {
        self.attach_count.fetch_add(1, Ordering::SeqCst);
    }

    fn record_detached(&self) {
        self.detach_count.fetch_add(1, Ordering::SeqCst);
    }

    fn prepare_session_spawn(&self, _sessions: &SessionSupervisor) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.fail_spawn.load(Ordering::SeqCst),
            "injected PTY spawn failure"
        );
        Ok(())
    }
}

#[cfg(test)]
impl StatusPort for FakeDaemonPorts {
    fn retire_codename(
        &self,
        sessions: &mut SessionSupervisor,
        codename: &str,
        observed_at: DateTime<Utc>,
    ) {
        sessions.retire_codename(codename, observed_at);
    }
}

#[cfg(test)]
impl PersistencePort for FakeDaemonPorts {
    fn last_session_exit(&self, control: &ControlRouting) -> ExitDisposition {
        DefaultDaemonPorts.last_session_exit(control)
    }
}

#[cfg(test)]
mod tests;
