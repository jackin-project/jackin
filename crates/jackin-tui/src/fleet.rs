// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::future::Future;
use std::time::Duration;

use futures::future::join_all;
use jackin_core::{Config, GatewayName, Phase, SandboxSummary, SupervisorReading, SupervisorState};
use jackin_session::{Session, UnreadableSandbox, WorkState};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use jackin_console::domain::instance::InstanceId;

use crate::console::ConsoleLimits;
use crate::sessions::{Reached, SessionSet};

#[derive(Clone, Debug)]
pub struct ObservedTask {
    pub text: String,
    pub statuses: Vec<(jackin_core::Agent, String)>,
}

#[derive(Clone, Debug)]
pub struct ObservedSandbox {
    pub gateway: GatewayName,
    pub summary: SandboxSummary,
    pub supervisor: Option<SupervisorReading>,
    pub work: Option<WorkState>,
    pub task: Option<ObservedTask>,
}

#[derive(Clone, Debug)]
pub struct ObservedUnreadable {
    pub gateway: GatewayName,
    pub sandbox: UnreadableSandbox,
}

#[derive(Clone, Debug)]
pub struct GatewayReading {
    pub name: GatewayName,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Fleet {
    pub sandboxes: Vec<ObservedSandbox>,
    pub unreadable: Vec<ObservedUnreadable>,
    pub gateways: Vec<GatewayReading>,
}

impl Fleet {
    pub fn failed(&self) -> impl Iterator<Item = &GatewayReading> {
        self.gateways.iter().filter(|g| g.failure.is_some())
    }

    pub fn all_failed(&self) -> bool {
        !self.gateways.is_empty() && self.gateways.iter().all(|g| g.failure.is_some())
    }

    pub(crate) fn admit(&mut self, observed: ObservedSandbox) {
        self.sandboxes.retain(|seen| {
            seen.gateway != observed.gateway || seen.summary.name != observed.summary.name
        });
        self.unreadable.retain(|seen| {
            seen.gateway != observed.gateway || seen.sandbox.name != observed.summary.name
        });
        self.sandboxes.push(observed);
    }

    pub(crate) fn locate(&self, instance: &InstanceId) -> Option<&ObservedSandbox> {
        self.sandboxes.iter().find(|observed| {
            observed.gateway.as_str() == instance.gateway.as_str()
                && observed.summary.name.as_str() == instance.sandbox_name.as_str()
        })
    }

    pub(crate) fn locate_unreadable(&self, instance: &InstanceId) -> Option<&ObservedUnreadable> {
        self.unreadable.iter().find(|observed| {
            observed.gateway.as_str() == instance.gateway.as_str()
                && observed.sandbox.name.as_str() == instance.sandbox_name.as_str()
        })
    }
}

#[derive(Debug)]
pub(crate) struct Waited(pub(crate) Duration);

impl fmt::Display for Waited {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[derive(Debug, Error)]
#[error("no answer within {0}")]
pub(crate) struct TimedOut(pub(crate) Waited);

pub(crate) async fn within<T>(deadline: Duration, work: impl Future<Output = T>) -> Option<T> {
    tokio::time::timeout(deadline, work).await.ok()
}

struct Inspected {
    summary: SandboxSummary,
    supervisor: Option<SupervisorReading>,
}

impl Inspected {
    fn needs_work(&self) -> bool {
        self.summary.phase == Phase::Ready
            && !matches!(
                self.supervisor,
                Some(SupervisorReading::Reported(SupervisorState::Running { .. }))
            )
    }
}

pub async fn read_fleet(
    sessions: &SessionSet,
    config: &Config,
    limits: ConsoleLimits,
    cancel: &CancellationToken,
) -> Option<Fleet> {
    let readings = join_all(
        sessions
            .reached()
            .into_iter()
            .map(|(name, reached)| async move {
                match reached {
                    Reached::Down(error) => unreadable(&name, error.to_string()),
                    Reached::Up(session) => {
                        read_gateway(&name, &session, config, limits, cancel).await
                    }
                }
            }),
    )
    .await;
    let mut fleet = Fleet::default();
    for (reading, sandboxes, unreadable) in readings {
        fleet.gateways.push(reading);
        fleet.sandboxes.extend(sandboxes);
        fleet.unreadable.extend(unreadable);
    }
    (!cancel.is_cancelled()).then_some(fleet)
}

fn unreadable(
    name: &GatewayName,
    failure: String,
) -> (
    GatewayReading,
    Vec<ObservedSandbox>,
    Vec<ObservedUnreadable>,
) {
    (
        GatewayReading {
            name: name.clone(),
            failure: Some(failure),
        },
        Vec::new(),
        Vec::new(),
    )
}

struct Reads<'a> {
    slots: Semaphore,
    deadline: Duration,
    until: Instant,
    cancel: &'a CancellationToken,
}

impl Reads<'_> {
    async fn run<T>(&self, call: impl Future<Output = T>) -> Option<T> {
        if self.cancel.is_cancelled() || Instant::now() >= self.until {
            return None;
        }
        let queued = async {
            let _slot = self.slots.acquire().await.ok()?;
            within(self.deadline, call).await
        };
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => None,
            read = tokio::time::timeout_at(self.until, queued) => read.ok().flatten(),
        }
    }
}

async fn read_gateway(
    name: &GatewayName,
    session: &Session,
    config: &Config,
    limits: ConsoleLimits,
    cancel: &CancellationToken,
) -> (
    GatewayReading,
    Vec<ObservedSandbox>,
    Vec<ObservedUnreadable>,
) {
    let deadline = limits.connect_deadline;
    let listed = tokio::select! {
        biased;
        () = cancel.cancelled() => None,
        listed = within(deadline, session.list()) => listed,
    };
    let listing = match listed {
        Some(Ok(listing)) => listing,
        Some(Err(error)) => return unreadable(name, error.to_string()),
        None => return unreadable(name, TimedOut(Waited(deadline)).to_string()),
    };
    let unreadable = listing
        .unreadable
        .into_iter()
        .map(|sandbox| ObservedUnreadable {
            gateway: name.clone(),
            sandbox,
        })
        .collect();
    let reads = Reads {
        slots: Semaphore::new(limits.work_reads.get()),
        deadline,
        until: Instant::now()
            .checked_add(deadline.saturating_mul(2))
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600)),
        cancel,
    };
    let sandboxes = join_all(
        listing
            .sandboxes
            .into_iter()
            .map(|summary| observe(name, session, config, summary, &reads)),
    )
    .await;
    (
        GatewayReading {
            name: name.clone(),
            failure: None,
        },
        sandboxes,
        unreadable,
    )
}

async fn observe(
    gateway: &GatewayName,
    session: &Session,
    config: &Config,
    summary: SandboxSummary,
    reads: &Reads<'_>,
) -> ObservedSandbox {
    let inspected = inspect(session, summary, reads).await;
    let (task, work) = tokio::join!(
        read_task(session, &inspected.summary, reads),
        read_work(session, config, &inspected, reads),
    );
    ObservedSandbox {
        gateway: gateway.clone(),
        summary: inspected.summary,
        supervisor: inspected.supervisor,
        work,
        task,
    }
}

async fn inspect(session: &Session, summary: SandboxSummary, reads: &Reads<'_>) -> Inspected {
    let supervisor = if summary.phase == Phase::Ready {
        reads
            .run(session.inspect(&summary.name))
            .await
            .and_then(Result::ok)
            .map(|view| view.supervisor)
    } else {
        None
    };
    Inspected {
        summary,
        supervisor,
    }
}

async fn read_work(
    session: &Session,
    config: &Config,
    sandbox: &Inspected,
    reads: &Reads<'_>,
) -> Option<WorkState> {
    if !sandbox.needs_work() {
        return None;
    }
    reads
        .run(session.work_state(config, &sandbox.summary.name))
        .await
}

async fn read_task(
    session: &Session,
    summary: &SandboxSummary,
    reads: &Reads<'_>,
) -> Option<ObservedTask> {
    let agents: Vec<jackin_core::Agent> = summary
        .labels
        .planned
        .agents
        .iter()
        .map(|item| item.agent)
        .collect();
    let mut paths = vec![jackin_core::sandbox_layout::TASK_FILE.clone()];
    for core_agent in &agents {
        paths.push(jackin_core::sandbox_layout::task_status(*core_agent));
    }
    let found = reads
        .run(session.gateway().read_files(&summary.name, &paths))
        .await
        .and_then(Result::ok)?;
    let mut found = found.into_iter();
    let text = String::from_utf8(found.next().flatten()?).ok()?;
    let mut statuses = Vec::new();
    for (core_agent, bytes) in agents.into_iter().zip(found) {
        if let Some(bytes) = bytes
            && let Ok(text) = String::from_utf8(bytes)
        {
            statuses.push((core_agent, text));
        }
    }
    Some(ObservedTask { text, statuses })
}
