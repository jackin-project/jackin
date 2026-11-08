// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::future::Future;
use std::time::Duration;

use futures::future::join_all;
use jackin_core::{
    Config, GatewayName, Phase, SandboxName, SandboxSummary, SupervisorReading, SupervisorState,
};
use jackin_session::{
    AgentMetadataError, SandboxInspection, Session, UnreadableSandbox, VerifiedAgentMetadata,
    WorkState,
};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use jackin_console::domain::instance::InstanceId;

use crate::console::ConsoleLimits;
use crate::project::sandbox_id;
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
    pub agent_metadata: Result<VerifiedAgentMetadata, AgentMetadataError>,
}

#[derive(Clone, Debug)]
pub struct ObservedUnreadableSandbox {
    pub gateway: GatewayName,
    pub sandbox: UnreadableSandbox,
}

#[derive(Clone, Debug)]
pub enum ObservedInstance {
    Available(Box<ObservedSandbox>),
    Unavailable {
        gateway: GatewayName,
        sandbox: SandboxName,
        reason: String,
    },
}

#[derive(Clone, Debug)]
pub struct GatewayReading {
    pub name: GatewayName,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Fleet {
    pub sandboxes: Vec<ObservedSandbox>,
    pub unreadable_sandboxes: Vec<ObservedUnreadableSandbox>,
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
        self.unreadable_sandboxes.retain(|seen| {
            seen.gateway != observed.gateway || seen.sandbox.name != observed.summary.name
        });
        self.sandboxes.push(observed);
        self.unreadable = self.unreadable_sandboxes.len();
    }

    pub(crate) fn admit_unreadable(&mut self, observed: ObservedUnreadableSandbox) {
        self.sandboxes.retain(|seen| {
            seen.gateway != observed.gateway || seen.summary.name != observed.sandbox.name
        });
        self.unreadable_sandboxes.retain(|seen| {
            seen.gateway != observed.gateway || seen.sandbox.name != observed.sandbox.name
        });
        self.unreadable_sandboxes.push(observed);
        self.unreadable = self.unreadable_sandboxes.len();
    }

    pub(crate) fn locate(&self, instance: &InstanceId) -> Option<ObservedInstance> {
        if let Some(observed) = self
            .sandboxes
            .iter()
            .find(|observed| &sandbox_id(&observed.gateway, &observed.summary.name) == instance)
        {
            return Some(match &observed.agent_metadata {
                Ok(_) => ObservedInstance::Available(Box::new(observed.clone())),
                Err(error) => ObservedInstance::Unavailable {
                    gateway: observed.gateway.clone(),
                    sandbox: observed.summary.name.clone(),
                    reason: error.to_string(),
                },
            });
        }
        self.unreadable_sandboxes
            .iter()
            .find(|observed| &sandbox_id(&observed.gateway, &observed.sandbox.name) == instance)
            .map(|observed| ObservedInstance::Unavailable {
                gateway: observed.gateway.clone(),
                sandbox: observed.sandbox.name.clone(),
                reason: observed.sandbox.reason(),
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
    agent_metadata: Result<VerifiedAgentMetadata, AgentMetadataError>,
}

enum InspectionOutcome {
    Readable(Inspected),
    Unreadable(UnreadableSandbox),
}

impl Inspected {
    fn needs_work(&self) -> bool {
        self.agent_metadata.is_ok()
            && self.summary.phase == Phase::Ready
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
    for (reading, sandboxes, unreadable_sandboxes) in readings {
        fleet.gateways.push(reading);
        fleet.sandboxes.extend(sandboxes);
        fleet.unreadable += unreadable_sandboxes.len();
        fleet.unreadable_sandboxes.extend(unreadable_sandboxes);
    }
    fleet
}

fn unreadable(
    name: &GatewayName,
    failure: String,
) -> (
    GatewayReading,
    Vec<ObservedSandbox>,
    Vec<ObservedUnreadableSandbox>,
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

async fn read_gateway(
    name: &GatewayName,
    session: &Session,
    config: &Config,
    limits: ConsoleLimits,
    cancel: &CancellationToken,
) -> (
    GatewayReading,
    Vec<ObservedSandbox>,
    Vec<ObservedUnreadableSandbox>,
) {
    let deadline = limits.connect_deadline;
    let listing = match within(deadline, session.list(cancel)).await {
        Some(Ok(listing)) => listing,
        Some(Err(error)) => return unreadable(name, error.to_string()),
        None => return unreadable(name, TimedOut(Waited(deadline)).to_string()),
    };
    let mut unreadable_sandboxes: Vec<_> = listing
        .unreadable
        .into_iter()
        .map(|sandbox| ObservedUnreadableSandbox {
            gateway: name.clone(),
            sandbox,
        })
        .collect();
    let slots = Semaphore::new(limits.work_reads.get());
    let inspected = join_all(
        listing
            .sandboxes
            .into_iter()
            .map(|summary| inspect(session, summary, deadline, &slots, cancel)),
    )
    .await;
    let inspected: Vec<Inspected> = inspected
        .into_iter()
        .filter_map(|outcome| match outcome {
            InspectionOutcome::Readable(sandbox) => Some(sandbox),
            InspectionOutcome::Unreadable(sandbox) => {
                unreadable_sandboxes.push(ObservedUnreadableSandbox {
                    gateway: name.clone(),
                    sandbox,
                });
                None
            }
        })
        .collect();
    let works = join_all(
        inspected
            .iter()
            .map(|sandbox| read_work(session, config, sandbox, deadline, &slots, cancel)),
    )
    .await;
    let tasks = join_all(inspected.iter().map(|sandbox| {
        read_task(
            session,
            &sandbox.summary,
            &sandbox.agent_metadata,
            deadline,
            cancel,
        )
    }))
    .await;
    let sandboxes = inspected
        .into_iter()
        .zip(works)
        .zip(tasks)
        .map(
            |(
                (
                    Inspected {
                        summary,
                        supervisor,
                        agent_metadata,
                    },
                    work,
                ),
                task,
            )| ObservedSandbox {
                gateway: name.clone(),
                summary,
                supervisor,
                work,
                task,
                agent_metadata,
            },
        )
        .collect();
    (
        GatewayReading {
            name: name.clone(),
            failure: None,
        },
        sandboxes,
        unreadable_sandboxes,
    )
}

async fn inspect(
    session: &Session,
    summary: SandboxSummary,
    deadline: Duration,
    slots: &Semaphore,
    cancel: &CancellationToken,
) -> InspectionOutcome {
    let slot = if summary.phase == Phase::Ready {
        tokio::select! {
            biased;
            () = cancel.cancelled() => None,
            permit = slots.acquire() => permit.ok(),
        }
    } else {
        None
    };
    let (supervisor, agent_metadata) = if slot.is_some() {
        let inspection = within(deadline, session.inspect(&summary.name, cancel)).await;
        let supervisor = match inspection {
            Some(Ok(SandboxInspection::Readable(view))) => Some(view.supervisor),
            Some(Ok(SandboxInspection::Unreadable(sandbox))) => {
                return InspectionOutcome::Unreadable(sandbox);
            }
            Some(Err(_)) | None => None,
        };
        let metadata = within(deadline, session.agent_metadata(&summary, cancel)).await;
        let agent_metadata = metadata.unwrap_or(Err(AgentMetadataError::TimedOut));
        (supervisor, agent_metadata)
    } else {
        let error = if cancel.is_cancelled() {
            AgentMetadataError::Cancelled
        } else {
            AgentMetadataError::NotReady(summary.phase)
        };
        (None, Err(error))
    };
    InspectionOutcome::Readable(Inspected {
        summary,
        supervisor,
        agent_metadata,
    })
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
    let _slot = tokio::select! {
        biased;
        () = cancel.cancelled() => return None,
        permit = slots.acquire() => permit.ok()?,
    };
    within(
        deadline,
        session.work_state(config, &sandbox.summary.name, cancel),
    )
    .await
}

async fn read_task(
    session: &Session,
    summary: &SandboxSummary,
    metadata: &Result<VerifiedAgentMetadata, AgentMetadataError>,
    deadline: Duration,
    cancel: &CancellationToken,
) -> Option<ObservedTask> {
    let metadata = metadata.as_ref().ok()?;
    let agents: Vec<jackin_core::Agent> =
        metadata.instances.iter().map(|item| item.agent).collect();
    let mut paths = vec![jackin_core::sandbox_layout::TASK_FILE.clone()];
    for core_agent in &agents {
        paths.push(jackin_core::sandbox_layout::task_status(*core_agent));
    }
    let found = within(
        deadline,
        session.gateway().read_files(&summary.name, &paths, cancel),
    )
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
