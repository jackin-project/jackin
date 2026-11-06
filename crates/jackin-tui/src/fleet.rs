// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::future::Future;
use std::time::Duration;

use futures::future::join_all;
use jackin_core::{Config, GatewayName, Phase, SandboxSummary, SupervisorReading, SupervisorState};
use jackin_session::{Session, WorkState};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use jackin_console::domain::instance::InstanceId;

use crate::console::ConsoleLimits;
use crate::project::sandbox_id;
use crate::sessions::{Reached, SessionSet};

#[derive(Clone, Debug)]
pub struct ObservedSandbox {
    pub gateway: GatewayName,
    pub summary: SandboxSummary,
    pub supervisor: Option<SupervisorReading>,
    pub work: Option<WorkState>,
}

#[derive(Clone, Debug)]
pub struct GatewayReading {
    pub name: GatewayName,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Fleet {
    pub sandboxes: Vec<ObservedSandbox>,
    pub unreadable: usize,
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
        self.sandboxes.push(observed);
    }

    pub(crate) fn locate(&self, instance: &InstanceId) -> Option<&ObservedSandbox> {
        self.sandboxes
            .iter()
            .find(|observed| &sandbox_id(&observed.summary.name) == instance)
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
) -> Fleet {
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
        fleet.unreadable += unreadable;
    }
    fleet
}

fn unreadable(
    name: &GatewayName,
    failure: String,
) -> (GatewayReading, Vec<ObservedSandbox>, usize) {
    (
        GatewayReading {
            name: name.clone(),
            failure: Some(failure),
        },
        Vec::new(),
        0,
    )
}

async fn read_gateway(
    name: &GatewayName,
    session: &Session,
    config: &Config,
    limits: ConsoleLimits,
    cancel: &CancellationToken,
) -> (GatewayReading, Vec<ObservedSandbox>, usize) {
    let deadline = limits.connect_deadline;
    let listing = match within(deadline, session.list(cancel)).await {
        Some(Ok(listing)) => listing,
        Some(Err(error)) => return unreadable(name, error.to_string()),
        None => return unreadable(name, TimedOut(Waited(deadline)).to_string()),
    };
    let unreadable = listing.unreadable.len();
    let slots = Semaphore::new(limits.work_reads.get());
    let inspected = join_all(
        listing
            .sandboxes
            .into_iter()
            .map(|summary| inspect(session, summary, deadline, &slots, cancel)),
    )
    .await;
    let works = join_all(
        inspected
            .iter()
            .map(|sandbox| read_work(session, config, sandbox, deadline, &slots, cancel)),
    )
    .await;
    let sandboxes = inspected
        .into_iter()
        .zip(works)
        .map(
            |(
                Inspected {
                    summary,
                    supervisor,
                },
                work,
            )| ObservedSandbox {
                gateway: name.clone(),
                summary,
                supervisor,
                work,
            },
        )
        .collect();
    (
        GatewayReading {
            name: name.clone(),
            failure: None,
        },
        sandboxes,
        unreadable,
    )
}

async fn inspect(
    session: &Session,
    summary: SandboxSummary,
    deadline: Duration,
    slots: &Semaphore,
    cancel: &CancellationToken,
) -> Inspected {
    let slot = if summary.phase == Phase::Ready {
        slots.acquire().await.ok()
    } else {
        None
    };
    let supervisor = if slot.is_some() {
        within(deadline, session.inspect(&summary.name, cancel))
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
    deadline: Duration,
    slots: &Semaphore,
    cancel: &CancellationToken,
) -> Option<WorkState> {
    if !sandbox.needs_work() {
        return None;
    }
    let _slot = slots.acquire().await.ok()?;
    within(
        deadline,
        session.work_state(config, &sandbox.summary.name, cancel),
    )
    .await
}
